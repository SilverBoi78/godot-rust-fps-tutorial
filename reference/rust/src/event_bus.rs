//! Global signal hub, registered as an autoload.
//!
//! The point is to let systems that should not know about each other still
//! communicate. The RoundDirector needs to know when an enemy dies; it should
//! not have to find and connect to every pooled Enemy. The HUD needs the points
//! total; it should not have to reach into the economy. Both listen here.
//!
//! The discipline that keeps this from becoming spaghetti:
//!
//!   1. Signals here describe FACTS that already happened, never commands.
//!      `enemy_died`, not `kill_enemy`.
//!   2. Anything with a clear owner uses a direct call or a local signal
//!      instead. A weapon's recoil goes straight to its own player -- putting
//!      it on the bus would mean every player kicks when anyone fires.
//!   3. Every signal is declared here with typed arguments, so this file is the
//!      readable index of everything that can happen in the game.

use godot::classes::{Engine, SceneTree};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct EventBus {
    base: Base<Node>,
}

#[godot_api]
impl EventBus {
    // --- combat ---------------------------------------------------------
    #[signal]
    pub fn enemy_damaged(enemy: Gd<Node3D>, amount: f32, is_headshot: bool, source: Option<Gd<Node>>);
    #[signal]
    pub fn enemy_died(enemy: Gd<Node3D>, killer: Option<Gd<Node>>);
    #[signal]
    pub fn player_damaged(amount: f32, current: f32, maximum: f32);
    #[signal]
    pub fn player_died();

    // --- economy --------------------------------------------------------
    #[signal]
    pub fn points_changed(total: i32);
    #[signal]
    pub fn points_awarded(amount: i32, reason: GString);
    #[signal]
    pub fn purchase_failed(cost: i32);

    // --- round flow -----------------------------------------------------
    #[signal]
    pub fn round_started(round_number: i32, enemy_count: i32);
    #[signal]
    pub fn round_cleared(round_number: i32);
    #[signal]
    pub fn enemies_remaining_changed(remaining: i32);

    // --- world ----------------------------------------------------------
    #[signal]
    pub fn zone_opened(zone_name: GString);
    #[signal]
    pub fn interact_target_changed(prompt: GString, affordable: bool);
}

impl EventBus {
    /// Reach the autoload from anywhere.
    ///
    /// GDScript gets `EventBus` as a free-floating global name; Rust has no
    /// equivalent, so we walk to it explicitly. `Engine::singleton()` is
    /// reachable without a node, which is what makes this callable from code
    /// that is not itself in the scene tree.
    pub fn singleton() -> Gd<EventBus> {
        autoload("EventBus")
    }
}

/// Shared by both autoloads. Panics if the autoload is missing, which is the
/// right behaviour: a missing autoload is a project misconfiguration, not a
/// runtime condition worth handling.
pub(crate) fn autoload<T: Inherits<Node>>(name: &str) -> Gd<T> {
    Engine::singleton()
        .get_main_loop()
        .expect("no main loop")
        .cast::<SceneTree>()
        .get_root()
        .get_node_as::<T>(name)
}
