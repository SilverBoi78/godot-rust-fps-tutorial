//! The first script lesson's toy: a node that spins and bobs.
//!
//! Kept because a later lesson reuses exactly this behaviour to make a wall-buy
//! display rotate, and because it is the smallest complete example of a Godot
//! class in Rust.

use godot::classes::{INode3D, Node3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Spinner {
    // `#[export]` makes a field appear in the Inspector dock. You can change it
    // without editing code -- and, crucially, WHILE the game is running.
    #[export]
    #[init(val = 90.0)]
    degrees_per_second: f32,
    #[export]
    #[init(val = 0.35)]
    bob_height: f32,
    #[export]
    #[init(val = 2.0)]
    bob_speed: f32,

    // No `#[export]`: this is internal bookkeeping, so it stays out of the
    // Inspector. We accumulate elapsed time here because `sin` needs an
    // ever-growing input.
    elapsed: f32,

    // Set once in `ready` and never changed, so the bob is measured from
    // wherever you placed this node in the editor rather than from the origin.
    start_y: f32,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Spinner {
    /// Runs once, after this node and all its children have entered the scene
    /// tree. Think of it as "the node is now fully alive and safe to touch."
    fn ready(&mut self) {
        self.start_y = self.base().get_position().y;
        godot_print!(
            "Spinner ready at y={:.2}, spinning at {:.0} deg/s.",
            self.start_y,
            self.degrees_per_second
        );
    }

    /// Runs once per rendered frame. `delta` is the number of SECONDS since the
    /// previous frame -- about 0.0167 at 60 fps, 0.0042 at 240 fps.
    fn process(&mut self, delta: f64) {
        self.elapsed += delta as f32;

        // Multiplying by delta is what makes this frame-rate independent: at
        // 60 fps we rotate a little 60 times a second, at 240 fps we rotate a
        // quarter as much 240 times a second. Same real-world speed either way.
        let step = self.degrees_per_second.to_radians() * delta as f32;
        self.base_mut().rotate_y(step);

        let y = self.bob_offset();
        let mut position = self.base().get_position();
        position.y = y;
        self.base_mut().set_position(position);
    }
}

impl Spinner {
    /// Splitting this out isn't necessary here -- it's a habit worth building,
    /// because `process` gets crowded fast once a node does more than one thing.
    fn bob_offset(&self) -> f32 {
        // `sin` cycles between -1 and 1. Scaling by bob_height turns that into
        // a gentle rise and fall around the node's starting height.
        self.start_y + (self.elapsed * self.bob_speed).sin() * self.bob_height
    }
}
