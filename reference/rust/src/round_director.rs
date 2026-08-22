//! The only thing in the game that knows the round number.
//!
//! Owns the spawn budget, difficulty scaling, and the gap between rounds.
//! Everything else finds out what is happening by listening to the EventBus,
//! which is what stops "what round is it?" leaking into twenty other scripts.
//!
//! Scaling comes from `Curve` RESOURCES rather than formulas in code, so tuning
//! the difficulty ramp is a matter of dragging a line in the editor. That is
//! "content is data, code is engine" applied to the one system whose feel is
//! hardest to get right.

use godot::classes::{Curve, Marker3D};
use godot::prelude::*;

use crate::enemy_pool::EnemyPool;
use crate::event_bus::EventBus;
use crate::game_state::GameState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Spawning,
    WaitingForClear,
    Intermission,
}

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct RoundDirector {
    /// Enemies in a round, indexed by round number / max_round.
    #[export]
    count_curve: Option<Gd<Curve>>,
    /// Enemy health multiplier.
    #[export]
    health_curve: Option<Gd<Curve>>,
    /// Enemy movement speed multiplier.
    #[export]
    speed_curve: Option<Gd<Curve>>,
    /// Seconds between individual spawns.
    #[export]
    spawn_interval_curve: Option<Gd<Curve>>,

    /// The round at which the curves reach their right-hand edge.
    #[export]
    #[init(val = 30)]
    max_round: i32,
    /// Simultaneous enemies allowed.
    #[export]
    #[init(val = 32)]
    max_active: i32,
    #[export]
    #[init(val = 6.0)]
    intermission_seconds: f32,
    #[export]
    #[init(val = 3.0)]
    first_round_delay: f32,

    /// Assigned by `Main` in its `ready`. Keeping the wiring in one place beats
    /// having each system hunt for its own dependencies through the tree.
    pool: Option<Gd<EnemyPool>>,
    spawn_root: Option<Gd<Node3D>>,

    phase: Phase,
    target: Option<Gd<Node3D>>,
    spawn_points: Vec<Gd<Marker3D>>,
    to_spawn: i32,
    alive: i32,
    spawn_timer: f32,
    phase_timer: f32,

    base: Base<Node>,
}

#[godot_api]
impl INode for RoundDirector {
    fn ready(&mut self) {
        let this = self.to_gd();
        EventBus::singleton()
            .signals()
            .enemy_died()
            .connect_other(&this, RoundDirector::on_enemy_died);
    }

    fn physics_process(&mut self, delta: f64) {
        match self.phase {
            Phase::Idle => {}
            Phase::Intermission => {
                self.phase_timer -= delta as f32;
                if self.phase_timer <= 0.0 {
                    self.start_round();
                }
            }
            Phase::Spawning => self.tick_spawning(delta),
            Phase::WaitingForClear => {
                if self.alive <= 0 {
                    self.clear_round();
                }
            }
        }
    }
}

#[godot_api]
impl RoundDirector {
    #[func]
    pub fn get_alive(&self) -> i32 {
        self.alive
    }

    #[func]
    pub fn spawn_point_count(&self) -> i32 {
        self.spawn_points.len() as i32
    }
}

impl RoundDirector {
    pub fn begin(&mut self, target: Gd<Node3D>, pool: Gd<EnemyPool>, spawns: Gd<Node3D>) {
        self.target = Some(target);
        self.pool = Some(pool);
        self.spawn_root = Some(spawns);
        self.refresh_spawn_points();

        let mut state = GameState::singleton();
        state.bind_mut().start_run();
        state.bind_mut().round_number = 0;

        self.phase = Phase::Intermission;
        self.phase_timer = self.first_round_delay;
    }

    pub fn get_phase(&self) -> Phase {
        self.phase
    }

    // --------------------------------------------------------------- rounds

    fn start_round(&mut self) {
        let round_number = {
            let mut state = GameState::singleton();
            let mut state = state.bind_mut();
            state.round_number += 1;
            state.round_number
        };

        self.refresh_spawn_points();

        self.to_spawn = self.count_for_round(round_number);
        self.alive = self.to_spawn;
        self.spawn_timer = 0.0;
        self.phase = Phase::Spawning;

        EventBus::singleton().signals().round_started().emit(round_number, self.to_spawn);
        EventBus::singleton().signals().enemies_remaining_changed().emit(self.alive);
    }

    fn tick_spawning(&mut self, delta: f64) {
        if self.to_spawn <= 0 {
            self.phase = Phase::WaitingForClear;
            return;
        }

        self.spawn_timer -= delta as f32;
        if self.spawn_timer > 0.0 {
            return;
        }

        // Respect the concurrency cap. If the field is full we simply wait --
        // the enemies still owed will arrive as the player thins them out.
        let full = match &self.pool {
            Some(pool) => pool.bind().active_count() >= self.max_active,
            None => true,
        };
        if full {
            return;
        }

        if self.spawn_one() {
            self.to_spawn -= 1;
            let round_number = GameState::singleton().bind().round_number;
            self.spawn_timer = self.sample(&self.spawn_interval_curve, round_number, 1.4);
        }
    }

    fn spawn_one(&mut self) -> bool {
        if self.spawn_points.is_empty() {
            return false;
        }
        let (Some(target), Some(mut pool)) = (self.target.clone(), self.pool.clone()) else {
            return false;
        };

        let index = (godot::global::randi() as usize) % self.spawn_points.len();
        let position = self.spawn_points[index].get_global_position();

        let round_number = GameState::singleton().bind().round_number;
        let health_scale = self.sample(&self.health_curve, round_number, 1.0);
        let speed_scale = self.sample(&self.speed_curve, round_number, 1.0);

        pool.bind_mut()
            .spawn(position, target, health_scale, speed_scale)
            .is_some()
    }

    fn clear_round(&mut self) {
        let round_number = GameState::singleton().bind().round_number;
        EventBus::singleton().signals().round_cleared().emit(round_number);
        self.phase = Phase::Intermission;
        self.phase_timer = self.intermission_seconds;
    }

    fn on_enemy_died(&mut self, _enemy: Gd<Node3D>, _killer: Option<Gd<Node>>) {
        self.alive = (self.alive - 1).max(0);
        EventBus::singleton()
            .signals()
            .enemies_remaining_changed()
            .emit(self.alive);
    }

    // -------------------------------------------------------------- scaling

    /// Curves are authored over x = 0..1, so the round number has to be
    /// normalised. Past `max_round` the value clamps, which means late rounds
    /// plateau rather than scaling to absurdity -- deliberate, and easy to
    /// change by editing the curve.
    fn sample(&self, curve: &Option<Gd<Curve>>, round_number: i32, fallback: f32) -> f32 {
        let Some(curve) = curve else {
            return fallback;
        };
        let t = (round_number as f32 / self.max_round as f32).clamp(0.0, 1.0);
        curve.sample(t)
    }

    fn count_for_round(&self, round_number: i32) -> i32 {
        let sampled = self.sample(&self.count_curve, round_number, 6.0);
        (sampled.round() as i32).max(1)
    }

    pub fn refresh_spawn_points(&mut self) {
        self.spawn_points.clear();
        let Some(spawn_root) = self.spawn_root.clone() else {
            return;
        };

        // Only spawn points in zones that are currently open. Buying a door
        // therefore widens where enemies come from, with no extra wiring.
        let markers = spawn_root
            .find_children_ex("*")
            .type_("Marker3D")
            .owned(false)
            .done();

        for node in markers.iter_shared() {
            let Ok(marker) = node.try_cast::<Marker3D>() else {
                continue;
            };
            if marker.is_visible_in_tree() && marker.is_in_group("spawn_point") {
                self.spawn_points.push(marker);
            }
        }
    }
}
