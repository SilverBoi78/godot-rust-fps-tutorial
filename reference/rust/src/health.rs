//! A component. Anything that can be hurt gets one of these as a child --
//! the player, an enemy, a destructible prop. No shared base class required,
//! which is convenient, because Rust does not have one to offer.
//!
//! `apply_damage` is deliberately written as though the call were already
//! coming in over the network: every input arrives as an argument, nothing is
//! read from global state, and it returns nothing. Making it authoritative
//! later is then a matter of adding an attribute rather than rewriting the
//! call sites.

use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct Health {
    #[export]
    #[init(val = 100.0)]
    pub max_health: f32,
    #[export]
    pub invulnerable: bool,

    current: f32,
    dead: bool,

    base: Base<Node>,
}

#[godot_api]
impl INode for Health {
    fn ready(&mut self) {
        self.current = self.max_health;
        let (current, max) = (self.current, self.max_health);
        self.signals().changed().emit(current, max);
    }
}

#[godot_api]
impl Health {
    #[signal]
    pub fn damaged(amount: f32, current: f32, source: Option<Gd<Node>>);
    #[signal]
    pub fn healed(amount: f32, current: f32);
    #[signal]
    pub fn died(source: Option<Gd<Node>>);
    /// Fires on every change, including the initial one. Ideal for HUD bars.
    #[signal]
    pub fn changed(current: f32, maximum: f32);

    #[func]
    pub fn get_current(&self) -> f32 {
        self.current
    }

    #[func]
    pub fn get_fraction(&self) -> f32 {
        if self.max_health > 0.0 {
            self.current / self.max_health
        } else {
            0.0
        }
    }

    #[func]
    pub fn is_dead(&self) -> bool {
        self.dead
    }

    #[func]
    pub fn apply_damage(&mut self, amount: f32, source: Option<Gd<Node>>) {
        if self.dead || self.invulnerable || amount <= 0.0 {
            return;
        }

        self.current = (self.current - amount).max(0.0);

        let (current, max) = (self.current, self.max_health);
        self.signals()
            .damaged()
            .emit(amount, current, source.as_ref());
        self.signals().changed().emit(current, max);

        if self.current <= 0.0 {
            self.dead = true;
            self.signals().died().emit(source.as_ref());
        }
    }

    #[func]
    pub fn heal(&mut self, amount: f32) {
        if self.dead || amount <= 0.0 {
            return;
        }

        self.current = (self.current + amount).min(self.max_health);

        let (current, max) = (self.current, self.max_health);
        self.signals().healed().emit(amount, current);
        self.signals().changed().emit(current, max);
    }

    /// Used by the object pool to bring a corpse back into service.
    #[func]
    pub fn reset(&mut self) {
        self.current = self.max_health;
        self.dead = false;
        let (current, max) = (self.current, self.max_health);
        self.signals().changed().emit(current, max);
    }
}
