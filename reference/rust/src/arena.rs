//! Bakes the navigation mesh, and re-bakes it whenever a zone opens.
//!
//! Baking normally happens in the editor with the Bake NavigationMesh button,
//! and for a static map that is all you need. This map is not static: a door
//! physically blocks the doorway and carves a hole in the navmesh, so once it
//! sinks out of the way the mesh has to be rebuilt or enemies will keep
//! treating the doorway as impassable.
//!
//! Baking at load rather than shipping a pre-baked `.tres` also means the mesh
//! can never go stale relative to the geometry -- a real class of bug where a
//! level is edited and someone forgets to re-bake.

use godot::classes::{INavigationRegion3D, NavigationRegion3D, SceneTreeTimer};
use godot::prelude::*;

use crate::event_bus::EventBus;

#[derive(GodotClass)]
#[class(base=NavigationRegion3D, init)]
pub struct Arena {
    /// The door animates out of the way over ~0.6s; wait for it to clear before
    /// re-reading the geometry.
    #[export]
    #[init(val = 0.8)]
    rebake_delay: f64,

    base: Base<NavigationRegion3D>,
}

#[godot_api]
impl INavigationRegion3D for Arena {
    fn ready(&mut self) {
        self.base_mut().bake_navigation_mesh_ex().on_thread(false).done();

        let this = self.to_gd();
        EventBus::singleton()
            .signals()
            .zone_opened()
            .connect_other(&this, Arena::on_zone_opened);
    }
}

#[godot_api]
impl Arena {
    /// GDScript would `await` a timer here. Rust has no `await` for Godot
    /// signals in a plain method, so we create the timer and connect its
    /// `timeout` to a `#[func]` -- which is exactly what `await` compiles down
    /// to anyway.
    #[func]
    fn rebake(&mut self) {
        // `on_thread = false`: this map is small enough that a synchronous bake
        // is a blip, and it avoids agents querying a half-built mesh. Bake on a
        // thread for a large map, and accept a moment where the old mesh is
        // still in use.
        self.base_mut().bake_navigation_mesh_ex().on_thread(false).done();
    }
}

impl Arena {
    fn on_zone_opened(&mut self, _zone_name: GString) {
        let delay = self.rebake_delay;
        let callback = Callable::from_object_method(&self.to_gd(), "rebake");
        let mut timer: Gd<SceneTreeTimer> = self
            .base()
            .get_tree()
            .create_timer(delay);
        timer.connect("timeout", &callback);
    }
}
