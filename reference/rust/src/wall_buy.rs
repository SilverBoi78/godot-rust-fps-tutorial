//! A weapon mounted on a wall. Buying it refills your reserve ammo.
//!
//! The interface it presents to the player is identical to the door's, because
//! both delegate all of that to the same `Interactable` component. Only the
//! `performed` handler differs.

use godot::classes::{IStaticBody3D, Node3D, StaticBody3D};
use godot::prelude::*;

use crate::interactable::Interactable;
use crate::player::Player;
use crate::weapon::Weapon;

#[derive(GodotClass)]
#[class(base=StaticBody3D, init)]
pub struct WallBuy {
    #[export]
    #[init(val = 120)]
    ammo_granted: i32,
    #[export]
    #[init(val = 500)]
    refill_cost: i32,
    #[export]
    #[init(val = "Rifle".into())]
    weapon_display_name: GString,

    #[init(node = "Interactable")]
    interactable: OnReady<Gd<Interactable>>,
    #[init(node = "Display")]
    display: OnReady<Gd<Node3D>>,

    base: Base<StaticBody3D>,
}

#[godot_api]
impl IStaticBody3D for WallBuy {
    fn ready(&mut self) {
        let this = self.to_gd();
        let name = self.weapon_display_name.clone();
        let cost = self.refill_cost;

        {
            let mut interactable = self.interactable.bind_mut();
            interactable.display_name = format!("Buy {name} ammo").as_str().into();
            interactable.cost = cost;
            interactable.set_availability_check(Callable::from_object_method(
                &this,
                "player_needs_ammo",
            ));
        }
        self.interactable
            .signals()
            .performed()
            .connect_other(&this, WallBuy::on_performed);
    }

    fn process(&mut self, delta: f64) {
        // Slowly rotate the mounted weapon so it reads as interactive. The exact
        // trick from the first script lesson, finally doing a real job.
        self.display.rotate_y(delta as f32 * 0.6);
    }
}

#[godot_api]
impl WallBuy {
    /// No point offering a purchase that would do nothing.
    #[func]
    fn player_needs_ammo(&self, player: Gd<Node3D>) -> bool {
        match find_weapon(&player) {
            Some(weapon) => {
                let weapon = weapon.bind();
                weapon.get_reserve() < weapon.max_reserve
            }
            None => false,
        }
    }
}

impl WallBuy {
    fn on_performed(&mut self, player: Gd<Node3D>) {
        if let Some(mut weapon) = find_weapon(&player) {
            weapon.bind_mut().add_reserve(self.ammo_granted);
        }
        // Reusable: clear the single-use latch that `interact` just set.
        self.interactable.bind_mut().clear_used();
    }
}

fn find_weapon(player: &Gd<Node3D>) -> Option<Gd<Weapon>> {
    let player = player.clone().try_cast::<Player>().ok()?;
    let weapon = player.bind().weapon.clone();
    Some(weapon)
}
