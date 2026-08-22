//! What a player WANTS to do this tick, with no reference to how they asked.
//!
//! This is the seam that co-op depends on: gameplay code reads this struct and
//! never touches `Input` directly. A keyboard, a gamepad, a replay file, an AI,
//! or a network packet can all fill one of these in, and the simulation cannot
//! tell the difference.
//!
//! Building it now costs about twenty lines. Retrofitting it later means
//! touching every line of movement, weapon, and interaction code.
//!
//! It is a plain Rust struct. Only Rust code ever reads or writes it, so
//! registering it as a Godot class would buy nothing and cost an allocation
//! and a refcount on every access.

use godot::prelude::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerIntent {
    /// Movement on the ground plane, in the player's own space.
    /// x = strafe (+right), y = forward (+forward). Length <= 1.
    pub move_dir: Vector2,

    /// Accumulated look change in RADIANS since the last time it was consumed.
    /// x = yaw (+left), y = pitch (+up).
    pub look_delta: Vector2,

    // Continuous states -- true for as long as the player holds them.
    pub sprint_held: bool,
    pub fire_held: bool,
    pub aim_held: bool,

    // One-shot events. These LATCH: they stay true until the simulation
    // consumes them, which is what stops a fast render rate from dropping
    // inputs that happened between two physics ticks.
    pub jump_pressed: bool,
    pub fire_pressed: bool,
    pub reload_pressed: bool,
    pub interact_pressed: bool,
}

impl PlayerIntent {
    /// Called by the simulation once it has acted on this intent.
    pub fn clear_one_shots(&mut self) {
        self.jump_pressed = false;
        self.fire_pressed = false;
        self.reload_pressed = false;
        self.interact_pressed = false;
    }
}
