//! The player as Lesson 4 leaves it: walk, jump, mouse look. Nothing else.
//!
//! It reads `Input` directly, which Lesson 6 will take away again. That is the
//! intended order: feel the coupling first, then remove it.

use godot::classes::input::MouseMode;
use godot::classes::{
    CharacterBody3D, ICharacterBody3D, Input, InputEvent, InputEventMouseMotion, Node3D,
};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=CharacterBody3D, init)]
pub struct Lesson04Player {
    #[export]
    #[init(val = 5.2)]
    speed: f32,
    #[export]
    #[init(val = 26.0)]
    fall_acceleration: f32,
    #[export]
    #[init(val = 7.2)]
    jump_velocity: f32,
    #[export(range = (0.0005, 0.01, 0.0001))]
    #[init(val = 0.0022)]
    mouse_sensitivity: f32,
    #[export(range = (60.0, 89.9))]
    #[init(val = 89.0)]
    pitch_limit_degrees: f32,

    #[init(node = "Head")]
    head: OnReady<Gd<Node3D>>,

    pitch: f32,

    base: Base<CharacterBody3D>,
}

#[godot_api]
impl ICharacterBody3D for Lesson04Player {
    fn ready(&mut self) {
        Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        let Ok(motion) = event.try_cast::<InputEventMouseMotion>() else {
            return;
        };
        if Input::singleton().get_mouse_mode() != MouseMode::CAPTURED {
            return;
        }

        let relative = motion.get_relative();
        // Compute the yaw BEFORE calling `base_mut()`. `base_mut()` borrows all
        // of `self`, so reading `self.mouse_sensitivity` inside the same
        // expression is a borrow-checker error. This is the single most common
        // thing to trip over when moving gameplay code from GDScript to Rust.
        let yaw = -relative.x * self.mouse_sensitivity;
        self.base_mut().rotate_y(yaw);

        let limit = self.pitch_limit_degrees.to_radians();
        self.pitch = (self.pitch - relative.y * self.mouse_sensitivity).clamp(-limit, limit);

        let mut rotation = self.head.get_rotation();
        rotation.x = self.pitch;
        self.head.set_rotation(rotation);
    }

    fn physics_process(&mut self, delta: f64) {
        let input = Input::singleton();
        let mut velocity = self.base().get_velocity();

        if self.base().is_on_floor() {
            if input.is_action_just_pressed("jump") {
                velocity.y = self.jump_velocity;
            }
        } else {
            velocity.y -= self.fall_acceleration * delta as f32;
        }

        let move_dir = input.get_vector("move_left", "move_right", "move_back", "move_forward");
        let basis = self.base().get_transform().basis;
        let wish = basis * Vector3::new(move_dir.x, 0.0, -move_dir.y);

        // No acceleration yet -- instant start and stop. Lesson 5 fixes that.
        velocity.x = wish.x * self.speed;
        velocity.z = wish.z * self.speed;

        self.base_mut().set_velocity(velocity);
        self.base_mut().move_and_slide();
    }
}
