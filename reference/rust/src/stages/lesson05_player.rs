//! The player as Lesson 5 leaves it: everything from Lesson 4, plus
//! acceleration, friction, air control, head bob and a sprint FOV kick.
//!
//! Still reads `Input` directly. Lesson 6 is where that goes.

use godot::classes::input::MouseMode;
use godot::classes::{
    Camera3D, CharacterBody3D, ICharacterBody3D, Input, InputEvent, InputEventMouseMotion, Node3D,
};
use godot::prelude::*;

use crate::player::move_toward;
use crate::weapon::smooth;

#[derive(GodotClass)]
#[class(base=CharacterBody3D, init)]
pub struct Lesson05Player {
    #[export]
    #[init(val = 5.2)]
    walk_speed: f32,
    #[export]
    #[init(val = 8.0)]
    sprint_speed: f32,
    #[export]
    #[init(val = 26.0)]
    fall_acceleration: f32,
    #[export]
    #[init(val = 7.2)]
    jump_velocity: f32,
    #[export]
    #[init(val = 65.0)]
    acceleration: f32,
    #[export]
    #[init(val = 75.0)]
    friction: f32,
    #[export(range = (0.0, 1.0))]
    #[init(val = 0.3)]
    air_control: f32,

    #[export(range = (0.0005, 0.01, 0.0001))]
    #[init(val = 0.0022)]
    mouse_sensitivity: f32,
    #[export(range = (60.0, 89.9))]
    #[init(val = 89.0)]
    pitch_limit_degrees: f32,

    #[export]
    #[init(val = 1.7)]
    bob_frequency: f32,
    #[export]
    #[init(val = 0.045)]
    bob_amplitude: f32,
    #[export]
    #[init(val = 78.0)]
    base_fov: f32,
    #[export]
    #[init(val = 9.0)]
    sprint_fov_bonus: f32,
    #[export]
    #[init(val = 8.0)]
    fov_response: f32,

    #[init(node = "Head")]
    head: OnReady<Gd<Node3D>>,
    #[init(node = "Head/CameraRig")]
    camera_rig: OnReady<Gd<Node3D>>,
    #[init(node = "Head/CameraRig/Camera3D")]
    camera: OnReady<Gd<Camera3D>>,

    pitch: f32,
    bob_time: f32,
    bob_amount: f32,

    base: Base<CharacterBody3D>,
}

#[godot_api]
impl ICharacterBody3D for Lesson05Player {
    fn ready(&mut self) {
        Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
        let fov = self.base_fov;
        self.camera.set_fov(fov);
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

    fn process(&mut self, delta: f64) {
        let velocity = self.base().get_velocity();
        let planar_speed = Vector2::new(velocity.x, velocity.z).length();
        let moving = self.base().is_on_floor() && planar_speed > 0.6;

        let target = if moving { self.bob_amplitude } else { 0.0 };
        self.bob_amount = smooth(self.bob_amount, target, 9.0, delta);
        self.bob_time += delta as f32 * planar_speed * self.bob_frequency;

        let mut position = self.camera_rig.get_position();
        position.y = self.bob_time.sin() * self.bob_amount;
        position.x = (self.bob_time * 0.5).cos() * self.bob_amount * 0.6;
        self.camera_rig.set_position(position);

        let sprinting = Input::singleton().is_action_pressed("sprint") && planar_speed > 1.5;
        let fov_target = self.base_fov
            + if sprinting {
                self.sprint_fov_bonus
            } else {
                0.0
            };
        let fov = smooth(self.camera.get_fov(), fov_target, self.fov_response, delta);
        self.camera.set_fov(fov);
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
        let mut wish = basis * Vector3::new(move_dir.x, 0.0, -move_dir.y);
        wish.y = 0.0;

        let speed = if input.is_action_pressed("sprint") {
            self.sprint_speed
        } else {
            self.walk_speed
        };
        let control = if self.base().is_on_floor() {
            1.0
        } else {
            self.air_control
        };

        if wish.length_squared() > 0.001 {
            let target = wish.normalized() * speed;
            let step = self.acceleration * control * delta as f32;
            velocity.x = move_toward(velocity.x, target.x, step);
            velocity.z = move_toward(velocity.z, target.z, step);
        } else {
            let step = self.friction * control * delta as f32;
            velocity.x = move_toward(velocity.x, 0.0, step);
            velocity.z = move_toward(velocity.z, 0.0, step);
        }

        self.base_mut().set_velocity(velocity);
        self.base_mut().move_and_slide();
    }
}
