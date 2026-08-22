//! Translates real hardware into a `PlayerIntent`. The ONLY place in the whole
//! game that is allowed to call `Input` for gameplay purposes.
//!
//! Swap this node for a network input source or an AI one later and the player
//! works unchanged -- that is the entire point.

use godot::classes::input::MouseMode;
use godot::classes::{Input, InputEvent, InputEventMouseMotion};
use godot::prelude::*;

use crate::player_intent::PlayerIntent;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct PlayerInputSource {
    #[export(range = (0.0005, 0.01, 0.0001))]
    #[init(val = 0.0022)]
    mouse_sensitivity: f32,
    #[export(range = (0.5, 8.0, 0.1))]
    #[init(val = 3.0)]
    gamepad_sensitivity: f32,
    #[export]
    invert_y: bool,

    /// Read by `Player` every frame. Never replaced wholesale, only mutated.
    pub intent: PlayerIntent,

    base: Base<Node>,
}

#[godot_api]
impl INode for PlayerInputSource {
    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        // Mouse movement arrives as EVENTS, not as a pollable state. Reading it
        // in `process` would miss motion and feel laggy, which is why look
        // accumulates here and gets consumed later.
        let Ok(motion) = event.try_cast::<InputEventMouseMotion>() else {
            return;
        };
        if Input::singleton().get_mouse_mode() != MouseMode::CAPTURED {
            return;
        }

        let relative = motion.get_relative();
        let invert = if self.invert_y { -1.0 } else { 1.0 };
        self.intent.look_delta.x -= relative.x * self.mouse_sensitivity;
        self.intent.look_delta.y -= relative.y * self.mouse_sensitivity * invert;
    }

    fn process(&mut self, delta: f64) {
        let input = Input::singleton();

        // `get_vector` handles deadzones and diagonal normalisation for us, and
        // works identically for the keyboard and the gamepad stick bound to the
        // same actions. Argument order is (neg_x, pos_x, neg_y, pos_y).
        self.intent.move_dir =
            input.get_vector("move_left", "move_right", "move_back", "move_forward");

        // Gamepad look is a HELD axis rather than a burst of motion, so unlike
        // the mouse it has to be scaled by delta.
        let stick = input.get_vector("look_left", "look_right", "look_up", "look_down");
        if stick.length_squared() > 0.0 {
            let invert = if self.invert_y { -1.0 } else { 1.0 };
            let scale = self.gamepad_sensitivity * delta as f32;
            self.intent.look_delta.x -= stick.x * scale;
            self.intent.look_delta.y -= stick.y * scale * invert;
        }

        self.intent.sprint_held = input.is_action_pressed("sprint");
        self.intent.fire_held = input.is_action_pressed("fire");
        self.intent.aim_held = input.is_action_pressed("aim");

        // Latch one-shots with `|=` rather than `=`. At 200 fps roughly three
        // frames run per physics tick; a plain assignment would let the two
        // frames after the press erase it before the simulation ever saw it.
        self.intent.jump_pressed |= input.is_action_just_pressed("jump");
        self.intent.fire_pressed |= input.is_action_just_pressed("fire");
        self.intent.reload_pressed |= input.is_action_just_pressed("reload");
        self.intent.interact_pressed |= input.is_action_just_pressed("interact");
    }
}
