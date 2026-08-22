//! A purchasable door. Buying it opens a `Zone` and removes itself.
//!
//! Note how little there is here. Everything about prompts, affordability, and
//! payment lives in the `Interactable` component; this class only knows what
//! buying it MEANS. That is the test of whether the shared part was drawn in
//! the right place.

use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{IStaticBody3D, StaticBody3D};
use godot::prelude::*;

use crate::interactable::Interactable;
use crate::zone::Zone;

#[derive(GodotClass)]
#[class(base=StaticBody3D, init)]
pub struct Door {
    /// A `NodePath` rather than a typed reference: an explicit path is
    /// unambiguous and survives hand-edited scene files.
    #[export]
    zone_path: NodePath,
    #[export]
    #[init(val = 0.6)]
    open_animation_seconds: f64,

    #[init(node = "Interactable")]
    interactable: OnReady<Gd<Interactable>>,

    zone_to_open: Option<Gd<Zone>>,

    base: Base<StaticBody3D>,
}

#[godot_api]
impl IStaticBody3D for Door {
    fn ready(&mut self) {
        let path = self.zone_path.clone();
        self.zone_to_open = self
            .base()
            .get_node_or_null(&path)
            .and_then(|node| node.try_cast::<Zone>().ok());

        if self.zone_to_open.is_none() {
            godot_warn!("Door '{}' has no zone to open.", self.base().get_name());
        }

        let this = self.to_gd();
        {
            let mut interactable = self.interactable.bind_mut();
            interactable.single_use = true;
            interactable
                .set_availability_check(Callable::from_object_method(&this, "zone_is_closed"));
        }
        self.interactable
            .signals()
            .performed()
            .connect_other(&this, Door::on_performed);
    }
}

#[godot_api]
impl Door {
    /// The extra condition an overridden `can_interact` would have expressed.
    /// Takes the player because that is the contract `Interactable` calls with;
    /// this particular check does not need it.
    #[func]
    fn zone_is_closed(&self, _player: Gd<Node3D>) -> bool {
        match &self.zone_to_open {
            Some(zone) => !zone.bind().is_open(),
            None => false,
        }
    }

    #[func]
    fn finish_opening(&mut self) {
        self.base_mut().set_visible(false);
    }
}

impl Door {
    fn on_performed(&mut self, _player: Gd<Node3D>) {
        if let Some(zone) = &mut self.zone_to_open {
            zone.bind_mut().open();
        }

        // Stop blocking immediately; sink out of sight for effect.
        self.base_mut().set_collision_layer_value(1, false);
        self.base_mut().set_collision_layer_value(5, false);

        let target = self.to_gd();
        let end_y = self.base().get_position().y - 4.2;
        let seconds = self.open_animation_seconds;
        let callback = Callable::from_object_method(&target, "finish_opening");

        let mut tween = self.base_mut().create_tween();
        tween
            .tween_property(&target, "position:y", &end_y.to_variant(), seconds)
            .set_trans(TransitionType::CUBIC)
            .set_ease(EaseType::IN_OUT);
        tween.tween_callback(&callback);
    }
}
