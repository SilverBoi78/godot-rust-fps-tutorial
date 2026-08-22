//! A round-based survival shooter, written in Rust against Godot 4 via gdext.
//!
//! Every module here corresponds to a lesson in `tutorial/`. The code is
//! deliberately plain: no clever abstractions that a reader has to decode
//! before they can see what the game does.

use godot::prelude::*;

pub mod arena;
pub mod audio;
pub mod barrier;
pub mod enemy;
pub mod enemy_pool;
pub mod event_bus;
pub mod game_state;
pub mod health;
pub mod hud;
pub mod impact;
pub mod interactable;
pub mod interactor;
pub mod main_scene;
pub mod player;
pub mod player_input;
pub mod player_intent;
pub mod round_director;
pub mod spinner;
pub mod stages;
pub mod target_dummy;
pub mod tests;
pub mod wall;
pub mod wall_buy;
pub mod weapon;
pub mod zone;

/// The unit struct that identifies this extension to Godot. It holds no data;
/// its only job is to be the type the `#[gdextension]` macro hangs off.
struct ShooterExtension;

/// `unsafe` because Godot calls into this across an FFI boundary and trusts us
/// to be a well-formed extension. There is nothing for you to get wrong here --
/// write it once and forget it.
#[gdextension]
unsafe impl ExtensionLibrary for ShooterExtension {}
