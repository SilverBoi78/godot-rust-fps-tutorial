//! Pre-instantiates every enemy at load and reuses them forever.
//!
//! Never instantiate an enemy mid-round. Runtime instantiation is the number
//! one cause of frame hitching in this genre, and the cost is not the
//! allocation -- it is loading the scene, building its nodes, resolving its
//! resources, and entering the tree, all inside one frame, at the exact moment
//! the game is busiest.
//!
//! Paying that cost once at load turns spawning into flipping a few booleans.

use godot::classes::PackedScene;
use godot::prelude::*;

use crate::enemy::Enemy;

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct EnemyPool {
    #[export]
    enemy_scene: Option<Gd<PackedScene>>,
    /// Budget: 32 active target, 48 hard cap.
    #[export]
    #[init(val = 48)]
    pool_size: i32,

    all: Vec<Gd<Enemy>>,
    available: Vec<Gd<Enemy>>,
    active: Vec<Gd<Enemy>>,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for EnemyPool {
    fn ready(&mut self) {
        self.prewarm();
    }
}

#[godot_api]
impl EnemyPool {
    #[func]
    pub fn active_count(&self) -> i32 {
        self.active.len() as i32
    }

    #[func]
    pub fn available_count(&self) -> i32 {
        self.available.len() as i32
    }

    /// Used when a round ends or a run resets -- clears the field without freeing.
    #[func]
    pub fn despawn_all(&mut self) {
        for mut enemy in std::mem::take(&mut self.active) {
            enemy.bind_mut().deactivate();
            if !self.available.contains(&enemy) {
                self.available.push(enemy);
            }
        }
    }

    /// The pool's own handler for every enemy's `despawned` signal.
    #[func]
    fn on_despawned(&mut self, enemy: Gd<Enemy>) {
        self.recycle(enemy);
    }
}

impl EnemyPool {
    fn prewarm(&mut self) {
        let Some(scene) = self.enemy_scene.clone() else {
            godot_error!("EnemyPool has no enemy_scene assigned.");
            return;
        };

        let this = self.to_gd();

        for i in 0..self.pool_size {
            let mut enemy = scene
                .instantiate_as::<Enemy>();
            enemy.set_name(&format!("Enemy{i:02}"));

            self.base_mut().add_child(&enemy);

            enemy
                .signals()
                .despawned()
                .connect_other(&this, EnemyPool::on_despawned);
            enemy.bind_mut().deactivate();

            self.all.push(enemy.clone());
            self.available.push(enemy);
        }

        godot_print!(
            "EnemyPool ready: {} enemies pre-instantiated.",
            self.all.len()
        );
    }

    /// Returns `None` when the pool is exhausted. The caller is expected to
    /// cope -- silently failing to spawn is much better than a hitch or a
    /// crash, and the RoundDirector simply tries again next tick.
    pub fn spawn(
        &mut self,
        spawn_position: Vector3,
        target: Gd<Node3D>,
        health_scale: f32,
        speed_scale: f32,
    ) -> Option<Gd<Enemy>> {
        let mut enemy = self.available.pop()?;
        self.active.push(enemy.clone());
        enemy
            .bind_mut()
            .activate(spawn_position, target, health_scale, speed_scale);
        Some(enemy)
    }

    fn recycle(&mut self, enemy: Gd<Enemy>) {
        self.active.retain(|e| e != &enemy);
        if !self.available.contains(&enemy) {
            self.available.push(enemy);
        }
    }
}
