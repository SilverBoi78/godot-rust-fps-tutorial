//! A region of the map that starts closed and is opened by buying a door.
//!
//! Opening a zone makes its geometry visible and collidable, and makes its
//! spawn points eligible -- the RoundDirector only collects markers that are
//! visible in the tree, so hiding a zone removes its spawns with no extra
//! bookkeeping.
//!
//! The goal is that map authors compose zones in the editor with ZERO code.
//! That works because everything a zone needs to know is structural: its
//! contents are its children.

use godot::classes::CollisionObject3D;
use godot::classes::node::ProcessMode;
use godot::prelude::*;

use crate::event_bus::EventBus;

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Zone {
    #[export]
    #[init(val = "Zone".into())]
    zone_name: GString,
    /// The starting area must be open from the beginning.
    #[export]
    open_at_start: bool,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Zone {
    fn ready(&mut self) {
        if self.open_at_start {
            self.open();
        } else {
            self.close();
        }
    }
}

#[godot_api]
impl Zone {
    #[func]
    pub fn open(&mut self) {
        self.base_mut().set_visible(true);
        self.base_mut().set_process_mode(ProcessMode::INHERIT);
        self.set_collision_enabled(true);

        let name = self.zone_name.clone();
        EventBus::singleton().signals().zone_opened().emit(&name);
    }

    #[func]
    pub fn close(&mut self) {
        self.base_mut().set_visible(false);
        self.set_collision_enabled(false);
    }

    #[func]
    pub fn is_open(&self) -> bool {
        self.base().is_visible()
    }
}

impl Zone {
    /// `visible` alone hides the geometry but leaves it solid, so a closed zone
    /// would still be an invisible wall you could stand on. Collision has to be
    /// switched separately.
    fn set_collision_enabled(&mut self, enabled: bool) {
        let children = self
            .base()
            .find_children_ex("*")
            .type_("CollisionObject3D")
            .owned(false)
            .done();

        for child in children.iter_shared() {
            if let Ok(mut body) = child.try_cast::<CollisionObject3D>() {
                // `process_mode` on the zone handles scripts; this handles physics.
                body.set_collision_layer_value(1, enabled);
            }
        }
    }
}
