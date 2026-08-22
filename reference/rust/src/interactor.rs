//! Looks for an `Interactable` in front of the camera and reports it.
//!
//! Lives on the player as a sibling of the input source. It reads the intent
//! struct, never `Input` -- the same rule as everything else.

use godot::classes::{Camera3D, CollisionObject3D, PhysicsRayQueryParameters3D};
use godot::prelude::*;

use crate::event_bus::EventBus;
use crate::game_state::GameState;
use crate::interactable::{Interactable, find_interactable};
use crate::player_intent::PlayerIntent;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct Interactor {
    #[export]
    #[init(val = 2.6)]
    reach: f32,
    /// Interactable is layer 5.
    #[export(flags_3d_physics)]
    #[init(val = 0b10000)]
    interact_mask: u32,

    camera: Option<Gd<Camera3D>>,
    owner_body: Option<Gd<Node3D>>,
    current: Option<Gd<Interactable>>,

    base: Base<Node>,
}

impl Interactor {
    pub fn setup(&mut self, camera: Gd<Camera3D>, owner_body: Gd<Node3D>) {
        self.camera = Some(camera);
        self.owner_body = Some(owner_body);
    }

    pub fn tick(&mut self, intent: &PlayerIntent) {
        self.refresh_target();

        if intent.interact_pressed {
            if let (Some(mut current), Some(player)) =
                (self.current.clone(), self.owner_body.clone())
            {
                current.bind_mut().interact(player);
                // Re-check straight away: a door that just opened should stop
                // prompting this frame rather than next.
                self.refresh_target();
            }
        }
    }

    pub fn get_current(&self) -> Option<Gd<Interactable>> {
        self.current.clone()
    }

    fn refresh_target(&mut self) {
        let found = self.probe();

        // Only announce on CHANGE of target. Emitting every frame would make
        // the HUD rebuild its label sixty times a second for no reason.
        let changed = found != self.current;
        let affordable = match &found {
            Some(interactable) => GameState::singleton()
                .bind()
                .can_afford(interactable.bind().cost),
            None => false,
        };

        if changed {
            self.current = found.clone();
            let prompt = match &found {
                Some(interactable) => interactable.bind().get_prompt(),
                None => GString::new(),
            };
            EventBus::singleton()
                .signals()
                .interact_target_changed()
                .emit(&prompt, affordable);
        } else if let Some(interactable) = &found {
            // Same target, but affordability may have changed as points came in.
            let prompt = interactable.bind().get_prompt();
            EventBus::singleton()
                .signals()
                .interact_target_changed()
                .emit(&prompt, affordable);
        }
    }

    fn probe(&self) -> Option<Gd<Interactable>> {
        let camera = self.camera.clone()?;

        let from = camera.get_global_position();
        let to = from - camera.get_global_transform().basis.col_c() * self.reach;

        let mut query = PhysicsRayQueryParameters3D::create(from, to)?;
        query.set_collision_mask(self.interact_mask);
        query.set_collide_with_areas(false);
        if let Some(body) = self.owner_body.clone() {
            if let Ok(collider) = body.try_cast::<CollisionObject3D>() {
                query.set_exclude(&array![collider.get_rid()]);
            }
        }

        let hit = camera
            .get_world_3d()?
            .get_direct_space_state()?
            .intersect_ray(&query);

        let collider = hit.get("collider")?.try_to::<Gd<Node>>().ok()?;
        let interactable = find_interactable(&collider)?;

        let player = self.owner_body.clone()?;
        if !interactable.bind().can_interact(player) {
            return None;
        }
        Some(interactable)
    }
}
