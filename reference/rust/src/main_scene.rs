//! Wires the run together and starts it.
//!
//! Deliberately thin. Its whole job is introducing systems that were built not
//! to know about each other -- the RoundDirector gets a target, the HUD gets a
//! weapon and a health component. Everything else talks over the EventBus.
//!
//! Keeping the wiring in ONE place is what makes the decoupling survive. If
//! every system hunted for its own dependencies with `get_node("../../Player")`,
//! the architecture would be just as tangled as direct calls, only harder to
//! read.

use godot::classes::{INode3D, Node3D};
use godot::prelude::*;

use crate::enemy_pool::EnemyPool;
use crate::event_bus::EventBus;
use crate::game_state::GameState;
use crate::hud::Hud;
use crate::player::Player;
use crate::round_director::RoundDirector;

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Main {
    #[init(node = "Player")]
    player: OnReady<Gd<Player>>,
    #[init(node = "HUDLayer/HUD")]
    hud: OnReady<Gd<Hud>>,
    #[init(node = "RoundDirector")]
    round_director: OnReady<Gd<RoundDirector>>,
    #[init(node = "EnemyPool")]
    enemy_pool: OnReady<Gd<EnemyPool>>,
    #[init(node = "Arena")]
    arena: OnReady<Gd<Node3D>>,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Main {
    fn ready(&mut self) {
        let hud = self.hud.clone();
        self.player.bind_mut().bind_hud(hud);

        let target = self.player.clone().upcast::<Node3D>();
        let pool = self.enemy_pool.clone();
        let arena = self.arena.clone();
        self.round_director.bind_mut().begin(target, pool, arena);

        let this = self.to_gd();
        EventBus::singleton()
            .signals()
            .player_died()
            .connect_other(&this, Main::on_player_died);
    }
}

impl Main {
    fn on_player_died(&mut self) {
        let mut state = GameState::singleton();
        let summary = state.bind().build_run_summary();
        state.bind_mut().end_run();

        godot_print!(
            "Run over. Rounds: {}  Kills: {}  Accuracy: {:.0}%",
            summary.rounds_survived,
            summary.kills,
            summary.accuracy * 100.0
        );
    }
}
