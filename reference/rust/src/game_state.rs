//! RUN state. Everything here is wiped when a run ends.
//!
//! There is a hard line between this and persistent progression (a profile,
//! unlocks, statistics that survive death). Run code never writes to a profile;
//! a run summary crosses that boundary once, at the end of a run. Keeping the
//! seam clean now is cheap; carving one out later is not.
//!
//! Nothing in here knows about nodes, scenes, or the UI. It holds numbers and
//! announces changes on the EventBus.

use godot::prelude::*;

use crate::event_bus::{autoload, EventBus};

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct GameState {
    #[var]
    pub points: i32,
    #[var]
    pub round_number: i32,
    #[var]
    pub kills: i32,
    #[var]
    pub headshots: i32,
    #[var]
    pub shots_fired: i32,
    #[var]
    pub run_seconds: f64,

    run_active: bool,

    base: Base<Node>,
}

#[godot_api]
impl INode for GameState {
    fn process(&mut self, delta: f64) {
        if self.run_active {
            self.run_seconds += delta;
        }
    }
}

#[godot_api]
impl GameState {
    #[func]
    pub fn start_run(&mut self) {
        self.points = 0;
        self.round_number = 0;
        self.kills = 0;
        self.headshots = 0;
        self.shots_fired = 0;
        self.run_seconds = 0.0;
        self.run_active = true;
        EventBus::singleton().signals().points_changed().emit(0);
    }

    #[func]
    pub fn end_run(&mut self) {
        self.run_active = false;
    }

    #[func]
    pub fn is_run_active(&self) -> bool {
        self.run_active
    }

    /// Takes everything as arguments and returns nothing, the same way
    /// `Health::apply_damage` does. If this ever becomes a host-authoritative
    /// call in co-op, the signature does not have to change.
    #[func]
    pub fn award_points(&mut self, amount: i32, reason: GString) {
        if amount <= 0 {
            return;
        }
        self.points += amount;
        EventBus::singleton().signals().points_awarded().emit(amount, &reason);
        EventBus::singleton().signals().points_changed().emit(self.points);
    }

    /// Returns whether the purchase went through. This one DOES return a value,
    /// because the caller genuinely needs to know -- a door must not open if you
    /// could not afford it. Compare with `award_points`, which nobody needs an
    /// answer from.
    #[func]
    pub fn try_spend(&mut self, cost: i32) -> bool {
        if cost > self.points {
            EventBus::singleton().signals().purchase_failed().emit(cost);
            return false;
        }
        self.points -= cost;
        EventBus::singleton()
            .signals()
            .points_changed()
            .emit(self.points);
        true
    }

    #[func]
    pub fn can_afford(&self, cost: i32) -> bool {
        self.points >= cost
    }

    #[func]
    pub fn record_kill(&mut self, is_headshot: bool) {
        self.kills += 1;
        if is_headshot {
            self.headshots += 1;
        }
    }
}

/// The plain-Rust summary that would cross into persistent storage.
///
/// A normal struct, not a Godot class: nothing outside Rust needs to touch it,
/// so there is no reason to pay for registration. Reach for `#[derive(GodotClass)]`
/// when Godot has to see the type -- not by reflex.
#[derive(Debug, Clone, Copy)]
pub struct RunSummary {
    pub rounds_survived: i32,
    pub kills: i32,
    pub headshots: i32,
    pub shots_fired: i32,
    pub accuracy: f32,
    pub seconds: f64,
}

impl GameState {
    pub fn singleton() -> Gd<GameState> {
        autoload("GameState")
    }

    pub fn build_run_summary(&self) -> RunSummary {
        RunSummary {
            rounds_survived: self.round_number,
            kills: self.kills,
            headshots: self.headshots,
            shots_fired: self.shots_fired,
            accuracy: self.kills as f32 / (self.shots_fired.max(1) as f32),
            seconds: self.run_seconds,
        }
    }
}
