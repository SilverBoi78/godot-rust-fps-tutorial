//! A practice target. Exists so the gun has something to shoot at before there
//! are real enemies, and so the signals lesson has somewhere concrete to
//! demonstrate them.
//!
//! Note what this class does NOT do: it never checks its own health in
//! `process`, and the weapon never calls into it directly. `Health` announces
//! what happened and this node reacts. Adding a HUD hitmarker, a sound, or a
//! score award later means connecting to the same signals -- not editing this
//! file.

use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{Area3D, IStaticBody3D, MeshInstance3D, StandardMaterial3D, StaticBody3D};
use godot::prelude::*;

use crate::health::Health;

#[derive(GodotClass)]
#[class(base=StaticBody3D, init)]
pub struct TargetDummy {
    #[export]
    #[init(val = 2.5)]
    respawn_seconds: f64,

    #[init(node = "Health")]
    health: OnReady<Gd<Health>>,
    #[init(node = "Body")]
    mesh: OnReady<Gd<MeshInstance3D>>,
    #[init(node = "HeadHitbox")]
    head_hitbox: OnReady<Gd<Area3D>>,
    #[init(node = "HeadHitbox/Head")]
    head_mesh: OnReady<Gd<MeshInstance3D>>,

    material: Option<Gd<StandardMaterial3D>>,
    base_color: Color,

    base: Base<StaticBody3D>,
}

#[godot_api]
impl IStaticBody3D for TargetDummy {
    fn ready(&mut self) {
        // Duplicate the material so flashing this dummy doesn't flash every
        // other dummy sharing the same resource. Shared-resource surprises like
        // this are a classic Godot gotcha.
        if let Some(active) = self.mesh.get_active_material(0) {
            if let Ok(std_mat) = active.try_cast::<StandardMaterial3D>() {
                let copy = std_mat.duplicate_resource();
                self.mesh.set_surface_override_material(0, &copy);
                self.head_mesh.set_surface_override_material(0, &copy);
                self.base_color = copy.get_albedo();
                self.material = Some(copy);
            }
        }

        let this = self.to_gd();
        self.health.signals().damaged().connect_other(&this, TargetDummy::on_damaged);
        self.health.signals().died().connect_other(&this, TargetDummy::on_died);
        self.health.signals().changed().connect_other(&this, TargetDummy::on_changed);
    }
}

#[godot_api]
impl TargetDummy {
    #[func]
    fn respawn(&mut self) {
        let mut rotation = self.base().get_rotation_degrees();
        rotation.x = 0.0;
        self.base_mut().set_rotation_degrees(rotation);
        self.base_mut().set_collision_layer_value(3, true);
        self.head_hitbox.set_monitorable(true);
        self.health.bind_mut().reset();
    }

    #[func]
    fn restore_damage_colour(&mut self) {
        let (current, max) = {
            let health = self.health.bind();
            (health.get_current(), health.max_health)
        };
        self.on_changed(current, max);
    }
}

impl TargetDummy {
    fn on_damaged(&mut self, amount: f32, current: f32, _source: Option<Gd<Node>>) {
        godot_print!("Target hit for {amount:.0}, {current:.0} left");
        self.flash(Color::from_rgb(1.0, 0.45, 0.3));
    }

    fn on_changed(&mut self, current: f32, maximum: f32) {
        // Darken as it gets hurt, so damage is readable without a health bar.
        let t = current / maximum;
        let hurt = Color::from_rgb(0.25, 0.1, 0.1);
        let colour = self.base_color.lerp(hurt, (1.0 - t) as f64);
        if let Some(material) = &mut self.material {
            material.set_albedo(colour);
        }
    }

    fn on_died(&mut self, _source: Option<Gd<Node>>) {
        godot_print!("Target down.");
        self.base_mut().set_collision_layer_value(3, false);
        self.head_hitbox.set_monitorable(false);

        // Tip it over. `rotation_degrees` is fine for a one-off flourish.
        let target = self.to_gd();
        let seconds = self.respawn_seconds;
        let callback = Callable::from_object_method(&target, "respawn");

        let mut tween = self.base_mut().create_tween();
        tween
            .tween_property(&target, "rotation_degrees:x", &(-82.0).to_variant(), 0.45)
            .set_trans(TransitionType::CUBIC)
            .set_ease(EaseType::IN);
        tween.tween_interval(seconds);
        tween.tween_callback(&callback);
    }

    fn flash(&mut self, colour: Color) {
        if let Some(material) = &mut self.material {
            material.set_albedo(colour);
        }
        let callback = Callable::from_object_method(&self.to_gd(), "restore_damage_colour");
        let mut tween = self.base_mut().create_tween();
        tween.tween_interval(0.06);
        tween.tween_callback(&callback);
    }
}
