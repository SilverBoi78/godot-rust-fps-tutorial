//! Heads-up display. Reads the EventBus and never reaches into gameplay code.
//!
//! Everything here is a reaction to something announced elsewhere. The HUD does
//! not know what an `Enemy` is, does not hold a reference to the RoundDirector,
//! and could be deleted entirely without breaking the game. That is the
//! property to protect: UI is the layer most likely to be rewritten, so it
//! should be the layer nothing else depends on.

use godot::classes::{Control, IControl, Label, ProgressBar};
use godot::prelude::*;

use crate::event_bus::EventBus;
use crate::game_state::GameState;
use crate::health::Health;
use crate::weapon::Weapon;

const AFFORDABLE: Color = Color::from_rgb(0.95, 0.95, 0.9);
const TOO_EXPENSIVE: Color = Color::from_rgb(0.9, 0.35, 0.3);

#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct Hud {
    #[init(node = "%HealthBar")]
    health_bar: OnReady<Gd<ProgressBar>>,
    #[init(node = "%HealthLabel")]
    health_label: OnReady<Gd<Label>>,
    #[init(node = "%AmmoLabel")]
    ammo_label: OnReady<Gd<Label>>,
    #[init(node = "%PointsLabel")]
    points_label: OnReady<Gd<Label>>,
    #[init(node = "%RoundLabel")]
    round_label: OnReady<Gd<Label>>,
    #[init(node = "%RemainingLabel")]
    remaining_label: OnReady<Gd<Label>>,
    #[init(node = "%PromptLabel")]
    prompt_label: OnReady<Gd<Label>>,
    #[init(node = "%Banner")]
    banner: OnReady<Gd<Label>>,
    #[init(node = "%Crosshair")]
    crosshair: OnReady<Gd<Control>>,

    base: Base<Control>,
}

#[godot_api]
impl IControl for Hud {
    fn ready(&mut self) {
        let this = self.to_gd();
        let bus = EventBus::singleton();

        // One `signals()` call per signal. The handle it returns configures a
        // single signal at a time, so holding one in a variable and reusing it
        // for seven connections panics at runtime -- it compiles perfectly well.
        bus.signals().points_changed().connect_other(&this, Hud::on_points_changed);
        bus.signals().round_started().connect_other(&this, Hud::on_round_started);
        bus.signals().round_cleared().connect_other(&this, Hud::on_round_cleared);
        bus.signals().enemies_remaining_changed().connect_other(&this, Hud::on_remaining_changed);
        bus.signals().player_damaged().connect_other(&this, Hud::on_player_damaged);
        bus.signals().interact_target_changed().connect_other(&this, Hud::on_interact_target_changed);
        bus.signals().purchase_failed().connect_other(&this, Hud::on_purchase_failed);

        self.prompt_label.set_text("");
        let mut modulate = self.banner.get_modulate();
        modulate.a = 0.0;
        self.banner.set_modulate(modulate);

        let points = GameState::singleton().bind().points;
        self.on_points_changed(points);
    }
}

impl Hud {
    /// Called by `Player`, which owns the weapon. Signals are for things with
    /// unknown listeners; this is a known, fixed relationship, so a direct
    /// wire-up is clearer than routing weapon ammo through the global bus.
    pub fn bind_weapon(&mut self, weapon: Gd<Weapon>) {
        let this = self.to_gd();
        weapon.signals().ammo_changed().connect_other(&this, Hud::on_ammo_changed);
        weapon.signals().reload_started().connect_other(&this, Hud::on_reload_started);
        weapon.signals().hit_confirmed().connect_other(&this, Hud::on_hit_confirmed);

        let (mag, reserve) = {
            let weapon = weapon.bind();
            (weapon.get_in_magazine(), weapon.get_reserve())
        };
        self.on_ammo_changed(mag, reserve);
    }

    pub fn bind_health(&mut self, health: Gd<Health>) {
        let this = self.to_gd();
        health.signals().changed().connect_other(&this, Hud::on_health_changed);

        let (current, max) = {
            let health = health.bind();
            (health.get_current(), health.max_health)
        };
        self.on_health_changed(current, max);
    }

    // ---------------------------------------------------------------- handlers

    fn on_health_changed(&mut self, current: f32, maximum: f32) {
        self.health_bar.set_max(maximum as f64);
        self.health_bar.set_value(current as f64);
        self.health_label.set_text(&format!("{}", current.round() as i32));
    }

    fn on_player_damaged(&mut self, _amount: f32, _current: f32, _maximum: f32) {
        let crosshair = self.crosshair.clone();
        let mut crosshair_mut = crosshair.clone();
        crosshair_mut.set_modulate(Color::from_rgb(1.0, 0.4, 0.4));

        let mut tween = self.base_mut().create_tween();
        tween.tween_property(&crosshair, "modulate", &Color::WHITE.to_variant(), 0.25);
    }

    fn on_ammo_changed(&mut self, in_magazine: i32, reserve: i32) {
        self.ammo_label.set_text(&format!("{in_magazine} / {reserve}"));
    }

    fn on_reload_started(&mut self, _seconds: f32) {
        self.ammo_label.set_text("RELOADING");
    }

    fn on_hit_confirmed(&mut self, is_headshot: bool) {
        let colour = if is_headshot {
            Color::from_rgb(1.0, 0.55, 0.2)
        } else {
            Color::WHITE
        };
        let scale = if is_headshot {
            Vector2::new(1.35, 1.35)
        } else {
            Vector2::new(1.2, 1.2)
        };

        let crosshair = self.crosshair.clone();
        let mut crosshair_mut = crosshair.clone();
        crosshair_mut.set_modulate(colour);
        crosshair_mut.set_scale(scale);

        let mut tween = self.base_mut().create_tween();
        tween.set_parallel();
        tween.tween_property(&crosshair, "scale", &Vector2::ONE.to_variant(), 0.14);
        tween.tween_property(&crosshair, "modulate", &Color::WHITE.to_variant(), 0.2);
    }

    fn on_points_changed(&mut self, total: i32) {
        self.points_label.set_text(&format!("{total}"));
    }

    fn on_round_started(&mut self, round_number: i32, enemy_count: i32) {
        self.round_label.set_text(&format!("ROUND {round_number}"));
        self.show_banner(&format!("ROUND {round_number}  —  {enemy_count} INCOMING"), 1.6);
    }

    fn on_round_cleared(&mut self, round_number: i32) {
        self.show_banner(&format!("ROUND {round_number} CLEAR"), 1.6);
    }

    fn on_remaining_changed(&mut self, remaining: i32) {
        self.remaining_label.set_text(&format!("{remaining} left"));
    }

    fn on_interact_target_changed(&mut self, prompt: GString, affordable: bool) {
        self.prompt_label.set_text(&prompt);
        let colour = if affordable { AFFORDABLE } else { TOO_EXPENSIVE };
        self.prompt_label.set_modulate(colour);
    }

    fn on_purchase_failed(&mut self, cost: i32) {
        self.show_banner(&format!("NEED {cost}"), 0.9);
    }

    fn show_banner(&mut self, text: &str, hold: f64) {
        self.banner.set_text(text);
        let banner = self.banner.clone();

        let mut tween = self.base_mut().create_tween();
        tween.tween_property(&banner, "modulate:a", &1.0f32.to_variant(), 0.15);
        tween.tween_interval(hold);
        tween.tween_property(&banner, "modulate:a", &0.0f32.to_variant(), 0.4);
    }
}
