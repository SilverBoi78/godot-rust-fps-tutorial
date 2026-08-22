//! Hitscan weapon: fire rate, recoil, ammo, and a reload state machine.
//!
//! Everything tunable is `#[export]`ed, because the only real question at this
//! stage is whether it feels good, and that is found by dragging sliders while
//! the game runs -- not by editing numbers and restarting.

use godot::classes::{
    AudioStreamPlayer3D, Camera3D, CollisionObject3D, Marker3D, Node3D, OmniLight3D, PackedScene,
    PhysicsRayQueryParameters3D,
};
use godot::global::randf_range;
use godot::prelude::*;

use crate::audio;
use crate::enemy::Enemy;
use crate::game_state::GameState;
use crate::health::Health;
use crate::player_intent::PlayerIntent;

/// The reload cycle as an explicit state machine.
///
/// A pile of booleans (`is_reloading`, `is_firing`) can represent states that
/// cannot actually happen -- reloading AND firing at once. An enum makes those
/// unrepresentable, and the compiler then forces every `match` to handle every
/// case. This is the single most useful thing Rust's type system does for
/// gameplay code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Ready,
    Firing,
    Reloading,
}

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Weapon {
    #[export]
    #[init(val = 26.0)]
    pub damage: f32,
    #[export]
    #[init(val = 2.5)]
    pub headshot_multiplier: f32,
    #[export]
    #[init(val = 150.0)]
    range_metres: f32,
    #[export(range = (60.0, 1200.0, 10.0))]
    #[init(val = 540.0)]
    rounds_per_minute: f32,
    #[export]
    #[init(val = true)]
    automatic: bool,

    #[export]
    #[init(val = 30)]
    pub magazine_size: i32,
    #[export]
    #[init(val = 270)]
    pub reserve_ammo: i32,
    #[export]
    #[init(val = 400)]
    pub max_reserve: i32,
    #[export(range = (0.2, 5.0, 0.05))]
    #[init(val = 2.0)]
    pub reload_seconds: f32,

    /// Degrees of upward camera kick per shot.
    #[export(range = (0.0, 5.0, 0.05))]
    #[init(val = 0.85)]
    recoil_pitch: f32,
    /// Maximum degrees of random horizontal kick per shot.
    #[export(range = (0.0, 3.0, 0.05))]
    #[init(val = 0.32)]
    recoil_yaw: f32,
    /// Metres the viewmodel punches backward per shot.
    #[export(range = (0.0, 0.5, 0.005))]
    #[init(val = 0.055)]
    viewmodel_kick: f32,
    #[export]
    #[init(val = 14.0)]
    viewmodel_recovery: f32,

    /// World | Enemy | EnemyHitbox -- layers 1, 3 and 4.
    #[export(flags_3d_physics)]
    #[init(val = 0b1101)]
    pub hit_mask: u32,

    #[init(node = "Muzzle")]
    muzzle: OnReady<Gd<Marker3D>>,
    #[init(node = "Muzzle/Flash")]
    muzzle_flash: OnReady<Gd<OmniLight3D>>,
    #[init(node = "Audio")]
    audio_player: OnReady<Gd<AudioStreamPlayer3D>>,
    #[init(node = "Viewmodel")]
    viewmodel: OnReady<Gd<Node3D>>,

    #[init(load = "res://scenes/impact.tscn")]
    impact_scene: OnReady<Gd<PackedScene>>,

    state: State,
    in_magazine: i32,
    shot_cooldown: f32,
    reload_remaining: f32,
    viewmodel_rest: Vector3,
    viewmodel_offset: f32,

    camera: Option<Gd<Camera3D>>,
    owner_body: Option<Gd<Node3D>>,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Weapon {
    fn ready(&mut self) {
        self.in_magazine = self.magazine_size;
        self.viewmodel_rest = self.viewmodel.get_position();
        self.muzzle_flash.set_visible(false);
        self.audio_player.set_stream(&audio::gunshot(1337));

        let (mag, reserve) = (self.in_magazine, self.reserve_ammo);
        self.signals().ammo_changed().emit(mag, reserve);
    }

    fn process(&mut self, delta: f64) {
        self.viewmodel_offset = smooth(self.viewmodel_offset, 0.0, self.viewmodel_recovery, delta);
        let rest = self.viewmodel_rest;
        let offset = self.viewmodel_offset;
        self.viewmodel
            .set_position(rest + Vector3::new(0.0, 0.0, offset));
    }
}

#[godot_api]
impl Weapon {
    #[signal]
    pub fn fired(in_magazine: i32);
    #[signal]
    pub fn ammo_changed(in_magazine: i32, reserve: i32);
    #[signal]
    pub fn reload_started(seconds: f32);
    #[signal]
    pub fn reload_finished();
    #[signal]
    pub fn recoil_kick(pitch_degrees: f32, yaw_degrees: f32);
    #[signal]
    pub fn hit_confirmed(is_headshot: bool);

    #[func]
    pub fn get_in_magazine(&self) -> i32 {
        self.in_magazine
    }

    #[func]
    pub fn get_reserve(&self) -> i32 {
        self.reserve_ammo
    }

    #[func]
    pub fn is_reloading(&self) -> bool {
        self.state == State::Reloading
    }

    /// Used by wall buys. Clamped so repeat purchases cannot stack infinitely.
    #[func]
    pub fn add_reserve(&mut self, amount: i32) {
        self.reserve_ammo = (self.reserve_ammo + amount).min(self.max_reserve);
        let (mag, reserve) = (self.in_magazine, self.reserve_ammo);
        self.signals().ammo_changed().emit(mag, reserve);
    }

    /// Turns the muzzle flash off again. A `#[func]` because the tween calls it
    /// back through Godot, which can only see registered methods.
    #[func]
    fn hide_flash(&mut self) {
        self.muzzle_flash.set_visible(false);
    }
}

impl Weapon {
    /// Called by `Player`. Handing the camera over explicitly beats guessing at
    /// a node path from in here -- rearranging the player scene cannot silently
    /// break it.
    pub fn setup(&mut self, camera: Gd<Camera3D>, owner_body: Gd<Node3D>) {
        self.camera = Some(camera);
        self.owner_body = Some(owner_body);
    }

    /// Driven by `Player` rather than reading `Input` itself -- same rule as
    /// everything else. The weapon has no idea a keyboard exists.
    pub fn tick(&mut self, intent: &PlayerIntent, delta: f64) {
        self.shot_cooldown = (self.shot_cooldown - delta as f32).max(0.0);

        match self.state {
            State::Ready | State::Firing => {
                if intent.reload_pressed {
                    self.begin_reload();
                } else if self.wants_to_fire(intent) {
                    self.try_fire();
                }
            }
            State::Reloading => {
                self.reload_remaining -= delta as f32;
                if self.reload_remaining <= 0.0 {
                    self.finish_reload();
                }
            }
        }
    }

    // ------------------------------------------------------------- firing

    fn wants_to_fire(&self, intent: &PlayerIntent) -> bool {
        if self.automatic {
            intent.fire_held
        } else {
            intent.fire_pressed
        }
    }

    fn try_fire(&mut self) {
        if self.shot_cooldown > 0.0 {
            return;
        }

        if self.in_magazine <= 0 {
            // Out of ammo: reload automatically rather than punishing the
            // player for not noticing. Small decision, large effect on feel.
            self.begin_reload();
            return;
        }

        self.fire();
    }

    fn fire(&mut self) {
        self.in_magazine -= 1;
        GameState::singleton().bind_mut().shots_fired += 1;
        self.shot_cooldown = 60.0 / self.rounds_per_minute;
        self.state = State::Firing;

        self.shoot_ray();

        let yaw = randf_range(-self.recoil_yaw as f64, self.recoil_yaw as f64) as f32;
        let pitch = self.recoil_pitch;
        self.signals().recoil_kick().emit(pitch, yaw);

        self.viewmodel_offset = self.viewmodel_kick;
        self.flash();
        self.audio_player.play();

        let (mag, reserve) = (self.in_magazine, self.reserve_ammo);
        self.signals().fired().emit(mag);
        self.signals().ammo_changed().emit(mag, reserve);
    }

    fn shoot_ray(&mut self) {
        let Some(camera) = self.camera.clone() else {
            godot_error!("Weapon has no camera. Did Player call setup()?");
            return;
        };

        // Rays are cast from the CAMERA, not the muzzle: the shot has to go
        // where the crosshair points. The muzzle is only where the flash is.
        let from = camera.get_global_position();
        let to = from - camera.get_global_transform().basis.col_c() * self.range_metres;

        let mut query = PhysicsRayQueryParameters3D::create(from, to).unwrap();
        query.set_collision_mask(self.hit_mask);
        // Off by default, and hitboxes are Area3Ds -- forget this and headshots
        // silently never register.
        query.set_collide_with_areas(true);
        query.set_collide_with_bodies(true);
        if let Some(body) = self.owner_body.clone() {
            // `get_rid` lives on CollisionObject3D, not Node3D -- the exclude
            // list is a list of physics objects, not of nodes.
            if let Ok(collider) = body.try_cast::<CollisionObject3D>() {
                query.set_exclude(&array![collider.get_rid()]);
            }
        }

        let hit = self
            .base()
            .get_world_3d()
            .expect("weapon is not in a 3D world")
            .get_direct_space_state()
            .expect("no space state")
            .intersect_ray(&query);

        if hit.is_empty() {
            return;
        }

        let Some(collider) = hit
            .get("collider")
            .and_then(|v| v.try_to::<Gd<Node>>().ok())
        else {
            return;
        };
        let point = hit
            .get("position")
            .and_then(|v| v.try_to::<Vector3>().ok())
            .unwrap_or_default();
        let normal = hit
            .get("normal")
            .and_then(|v| v.try_to::<Vector3>().ok())
            .unwrap_or_default();

        let is_headshot = collider.is_in_group("headshot");

        if let Some(mut health) = find_health(&collider) {
            // Tell the target the hit is coming BEFORE applying damage, so its
            // death handler knows whether the killing blow was a headshot.
            if let Some(parent) = health.get_parent() {
                if let Ok(mut enemy) = parent.try_cast::<Enemy>() {
                    enemy.bind_mut().note_incoming_hit(is_headshot);
                }
            }

            let multiplier = if is_headshot {
                self.headshot_multiplier
            } else {
                1.0
            };
            let source = self.owner_body.clone().map(|b| b.upcast::<Node>());
            health
                .bind_mut()
                .apply_damage(self.damage * multiplier, source);

            self.signals().hit_confirmed().emit(is_headshot);
        }

        self.spawn_impact(point, normal);
    }

    fn spawn_impact(&mut self, point: Vector3, normal: Vector3) {
        // Instantiating mid-combat, which is exactly what the pooling lesson
        // forbids for enemies. It is fine for one small effect and will be
        // pooled later -- left visible here on purpose.
        let Some(mut impact) = self.impact_scene.instantiate() else {
            return;
        };

        self.impact_parent().add_child(&impact);

        let mut impact3d = impact.clone().cast::<Node3D>();
        impact3d.set_global_position(point + normal * 0.02);
        if normal.length_squared() > 0.0 && normal.dot(Vector3::UP).abs() < 0.99 {
            let target = impact3d.get_global_position() + normal;
            impact3d.look_at(target);
        }
        impact.set_name("Impact");
    }

    /// Impacts are parented to the running scene, not to the weapon -- otherwise
    /// they would follow the gun around as you move.
    ///
    /// `current_scene` is null whenever this scene was not loaded as THE main
    /// scene: running `player.tscn` directly, or from a headless test. Reaching
    /// for it without a fallback is a small landmine.
    fn impact_parent(&self) -> Gd<Node> {
        let tree = self.base().get_tree();
        match tree.get_current_scene() {
            Some(scene) => scene,
            None => tree.get_root().upcast(),
        }
    }

    fn flash(&mut self) {
        self.muzzle_flash.set_visible(true);
        // A tween is a small animation you build in code. This one just turns
        // the light off again a moment later without needing a Timer node.
        let callback = Callable::from_object_method(&self.to_gd(), "hide_flash");
        let mut tween = self.base_mut().create_tween();
        tween.tween_interval(0.035);
        tween.tween_callback(&callback);
    }

    // ----------------------------------------------------------- reloading

    fn begin_reload(&mut self) {
        if self.state == State::Reloading {
            return;
        }
        if self.in_magazine >= self.magazine_size || self.reserve_ammo <= 0 {
            return;
        }

        self.state = State::Reloading;
        self.reload_remaining = self.reload_seconds;
        let seconds = self.reload_seconds;
        self.signals().reload_started().emit(seconds);
    }

    fn finish_reload(&mut self) {
        let wanted = self.magazine_size - self.in_magazine;
        let taken = wanted.min(self.reserve_ammo);
        self.in_magazine += taken;
        self.reserve_ammo -= taken;

        self.state = State::Ready;
        self.signals().reload_finished().emit();
        let (mag, reserve) = (self.in_magazine, self.reserve_ammo);
        self.signals().ammo_changed().emit(mag, reserve);
    }
}

/// Walks the collider and its scene owner looking for a `Health` component.
/// A hitbox `Area3D` is usually a grandchild of the thing that owns the health.
pub fn find_health(collider: &Gd<Node>) -> Option<Gd<Health>> {
    let candidates = [Some(collider.clone()), collider.get_owner()];
    for candidate in candidates.into_iter().flatten() {
        for child in candidate.get_children().iter_shared() {
            if let Ok(health) = child.try_cast::<Health>() {
                return Some(health);
            }
        }
    }
    None
}

/// Frame-rate independent exponential smoothing.
///
/// The naive `lerp(current, target, 0.1)` is a bug: it moves 10% per FRAME, so
/// it converges more than four times faster at 240 fps than at 60. This form
/// converges at the same real-world rate regardless.
pub fn smooth(current: f32, target: f32, response: f32, delta: f64) -> f32 {
    let t = 1.0 - (-response * delta as f32).exp();
    current + (target - current) * t
}
