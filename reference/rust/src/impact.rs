//! A bullet impact mark that shrinks away and deletes itself.
//!
//! Deliberately naive: it is created with `instantiate()` at the moment of
//! impact and freed a second later. That is exactly the pattern the pooling
//! lesson bans for enemies, and pooling this is one of that lesson's exercises.
//! At one small node per shot it is genuinely fine; at 32 enemies per round it
//! is not.

use godot::classes::MeshInstance3D;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Impact {
    #[export]
    #[init(val = 0.9)]
    lifetime: f32,

    #[init(node = "MeshInstance3D")]
    mesh: OnReady<Gd<MeshInstance3D>>,

    elapsed: f32,
    start_scale: Vector3,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Impact {
    fn ready(&mut self) {
        self.start_scale = self.mesh.get_scale();
    }

    fn process(&mut self, delta: f64) {
        self.elapsed += delta as f32;
        let t = self.elapsed / self.lifetime;

        if t >= 1.0 {
            // `queue_free`, never `free`. Deleting a node in the middle of a
            // frame that other code is still walking is how you get very
            // confusing crashes.
            self.base_mut().queue_free();
            return;
        }

        let scale = self.start_scale * (1.0 - t);
        self.mesh.set_scale(scale);
    }
}
