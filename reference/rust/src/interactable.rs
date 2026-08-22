//! The shared half of everything the player can look at and press E on.
//!
//! GDScript would make this a base class and have doors and wall buys extend
//! it. Rust has no inheritance, and gdext will not let one `#[derive(GodotClass)]`
//! type sit under another -- `#[class(base = ...)]` only accepts engine classes.
//!
//! So this is a COMPONENT, exactly like `Health`: a child node holding the part
//! every interactable shares -- a name, a price, whether it has been used, and
//! the payment protocol. The specific behaviour lives in the parent, which
//! reacts to the `performed` signal.
//!
//! Two hooks let the parent customise the shared logic without overriding a
//! method:
//!
//! * `availability_check` -- a `Callable` the parent installs, asked "can this
//!   be used right now?". This is what a virtual method would have been.
//! * `performed` -- the signal that says "paid for, go do your thing."
//!
//! The result is arguably better than the inheritance version: the payment
//! protocol lives in exactly one place and cannot be overridden by accident,
//! and an interactable can be *any* node type rather than being forced to
//! extend one particular body class.

use godot::prelude::*;

use crate::game_state::GameState;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct Interactable {
    #[export]
    #[init(val = "Interactable".into())]
    pub display_name: GString,
    #[export]
    pub cost: i32,
    /// Once used, stop offering the prompt (wall buys stay usable; doors do not).
    #[export]
    pub single_use: bool,

    used: bool,
    /// Installed by the parent in its `ready`. Called with the player node,
    /// returns a bool. `None` means "always available".
    availability_check: Option<Callable>,

    base: Base<Node>,
}

#[godot_api]
impl Interactable {
    /// Emitted after a successful interaction -- that is, after `can_interact`
    /// passed AND the cost was paid. The parent does its actual work here.
    #[signal]
    pub fn performed(player: Gd<Node3D>);

    /// The text shown on screen.
    #[func]
    pub fn get_prompt(&self) -> GString {
        if self.cost > 0 {
            format!("{}  [{}]", self.display_name, self.cost).as_str().into()
        } else {
            self.display_name.clone()
        }
    }

    #[func]
    pub fn was_used(&self) -> bool {
        self.used
    }

    /// Lets a reusable interactable clear the latch that `interact` sets.
    #[func]
    pub fn clear_used(&mut self) {
        self.used = false;
    }

    #[func]
    pub fn set_availability_check(&mut self, check: Callable) {
        self.availability_check = Some(check);
    }

    /// Whether the prompt should appear at all.
    #[func]
    pub fn can_interact(&self, player: Gd<Node3D>) -> bool {
        if self.used && self.single_use {
            return false;
        }
        match &self.availability_check {
            Some(check) => check.callv(&varray![&player]).booleanize(),
            None => true,
        }
    }

    /// Called by the player's `Interactor`. Handles the payment protocol once,
    /// so no parent has to remember to check affordability.
    ///
    /// The parent cannot override this, only react to the signal it emits.
    /// A method that enforces an invariant and then hands off is much harder to
    /// get wrong than one every subclass is trusted to reimplement correctly.
    #[func]
    pub fn interact(&mut self, player: Gd<Node3D>) {
        if !self.can_interact(player.clone()) {
            return;
        }

        let price = self.cost;
        if price > 0 && !GameState::singleton().bind_mut().try_spend(price) {
            return;
        }

        self.used = true;
        self.signals().performed().emit(&player);
    }
}

/// Finds the `Interactable` component on a node, the same way
/// `weapon::find_health` finds a `Health`. Two components, one lookup pattern.
pub fn find_interactable(node: &Gd<Node>) -> Option<Gd<Interactable>> {
    for child in node.get_children().iter_shared() {
        if let Ok(interactable) = child.try_cast::<Interactable>() {
            return Some(interactable);
        }
    }
    None
}
