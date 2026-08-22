//! A resizable greybox wall.
//!
//! `#[class(tool)]` makes this class run INSIDE THE EDITOR as well as at
//! runtime, so changing `size` in the Inspector rebuilds the wall immediately
//! instead of only when you press play.
//!
//! The mesh and collision shape are built in code rather than being
//! sub-resources saved in the scene. That matters: a `BoxMesh` saved in
//! `wall.tscn` would be SHARED by every wall instance, so resizing one would
//! resize all of them -- the same shared-resource trap as the enemy material.
//! Creating fresh resources per instance sidesteps it entirely.

use godot::classes::{BoxMesh, BoxShape3D, CollisionShape3D, IStaticBody3D, Material, MeshInstance3D, StaticBody3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(tool, base=StaticBody3D, init)]
pub struct Wall {
    /// A setter runs the rebuild. `#[var(get, set = ...)]` is how gdext spells
    /// GDScript's inline `set:` block.
    #[export]
    #[var(set = set_size)]
    #[init(val = Vector3::new(24.0, 4.0, 0.5))]
    size: Vector3,

    #[export]
    #[var(set = set_material)]
    material: Option<Gd<Material>>,

    base: Base<StaticBody3D>,
}

#[godot_api]
impl IStaticBody3D for Wall {
    fn ready(&mut self) {
        self.rebuild();
    }
}

#[godot_api]
impl Wall {
    #[func]
    fn set_size(&mut self, value: Vector3) {
        self.size = value;
        self.rebuild();
    }

    #[func]
    fn set_material(&mut self, value: Option<Gd<Material>>) {
        self.material = value;
        self.rebuild();
    }
}

impl Wall {
    fn rebuild(&mut self) {
        if !self.base().is_inside_tree() {
            return;
        }

        let Some(mesh_node) = self.base().get_node_or_null("Mesh") else {
            return;
        };
        let Some(shape_node) = self.base().get_node_or_null("Shape") else {
            return;
        };
        let (Ok(mut mesh_node), Ok(mut shape_node)) = (
            mesh_node.try_cast::<MeshInstance3D>(),
            shape_node.try_cast::<CollisionShape3D>(),
        ) else {
            return;
        };

        let mut box_mesh = BoxMesh::new_gd();
        box_mesh.set_size(self.size);
        if let Some(material) = &self.material {
            box_mesh.set_material(material);
        }
        mesh_node.set_mesh(&box_mesh);

        let mut shape = BoxShape3D::new_gd();
        shape.set_size(self.size);
        shape_node.set_shape(&shape);

        // Sit the wall's BASE at y = 0 so it can be placed on the floor without
        // doing half-height arithmetic every time.
        let lift = Vector3::new(0.0, self.size.y * 0.5, 0.0);
        mesh_node.set_position(lift);
        shape_node.set_position(lift);
    }
}
