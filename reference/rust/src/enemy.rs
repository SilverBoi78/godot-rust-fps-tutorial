//! The basic enemy: paths to the player and swings when close.
//!
//! Written to be POOLED from the start. That means it must be able to go fully
//! dormant and come back clean, so all of its mutable state is reset in
//! `activate` rather than assumed fresh from `ready`.

use godot::classes::node::ProcessMode;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{
    Area3D, CharacterBody3D, CollisionShape3D, ICharacterBody3D, MeshInstance3D, NavigationAgent3D,
    StandardMaterial3D, Tween,
};
use godot::prelude::*;

use crate::event_bus::EventBus;
use crate::game_state::GameState;
use crate::health::Health;
use crate::weapon::find_health;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Dormant,
    Chasing,
    Attacking,
    Dying,
}

#[derive(GodotClass)]
#[class(base=CharacterBody3D, init)]
pub struct Enemy {
    #[export]
    #[init(val = 2.6)]
    move_speed: f32,
    #[export]
    #[init(val = 9.0)]
    turn_speed: f32,
    /// Custom accessor names avoid shadowing `CharacterBody3D::get_gravity()`.
    #[export]
    #[var(get = get_gravity_strength, set = set_gravity_strength)]
    #[init(val = 26.0)]
    gravity: f32,
    /// Stop closing once inside this range and start swinging.
    #[export]
    #[init(val = 1.9)]
    attack_range: f32,
    /// Give up the current path and repath if the target has moved this far.
    #[export]
    #[init(val = 1.2)]
    repath_threshold: f32,

    #[export]
    #[init(val = 18.0)]
    attack_damage: f32,
    #[export]
    #[init(val = 1.1)]
    attack_interval: f32,
    /// Delay between starting a swing and the damage landing, so it can be dodged.
    #[export]
    #[init(val = 0.35)]
    attack_windup: f32,

    #[export]
    #[init(val = 10)]
    points_on_hit: i32,
    #[export]
    #[init(val = 60)]
    points_on_kill: i32,
    #[export]
    #[init(val = 100)]
    points_on_headshot_kill: i32,

    #[init(node = "Health")]
    pub health: OnReady<Gd<Health>>,
    #[init(node = "NavigationAgent3D")]
    pub agent: OnReady<Gd<NavigationAgent3D>>,
    #[init(node = "HeadHitbox")]
    head_hitbox: OnReady<Gd<Area3D>>,
    #[init(node = "CollisionShape3D")]
    body_shape: OnReady<Gd<CollisionShape3D>>,
    #[init(node = "Body")]
    mesh: OnReady<Gd<MeshInstance3D>>,
    #[init(node = "HeadHitbox/Head")]
    head_mesh: OnReady<Gd<MeshInstance3D>>,

    state: State,
    target: Option<Gd<Node3D>>,
    attack_cooldown: f32,
    windup_remaining: f32,
    last_target_position: Vector3,
    material: Option<Gd<StandardMaterial3D>>,
    base_color: Color,
    last_hit_was_headshot: bool,

    base: Base<CharacterBody3D>,
}

#[godot_api]
impl ICharacterBody3D for Enemy {
    fn ready(&mut self) {
        // Per-instance material, so flashing one enemy doesn't flash all 48.
        if let Some(active) = self.mesh.get_active_material(0) {
            if let Ok(std_mat) = active.try_cast::<StandardMaterial3D>() {
                let copy = std_mat.duplicate_resource();
                self.mesh.set_surface_override_material(0, &copy);
                self.head_mesh.set_surface_override_material(0, &copy);
                self.base_color = copy.get_albedo();
                self.material = Some(copy);
            }
        }

        let this = self.to_gd();
        self.health
            .signals()
            .damaged()
            .connect_other(&this, Enemy::on_damaged);
        self.health
            .signals()
            .died()
            .connect_other(&this, Enemy::on_died);

        self.deactivate();
    }

    fn physics_process(&mut self, delta: f64) {
        if !self.base().is_on_floor() {
            let g = self.gravity * delta as f32;
            let mut v = self.base().get_velocity();
            v.y -= g;
            self.base_mut().set_velocity(v);
        } else {
            let mut v = self.base().get_velocity();
            v.y = 0.0;
            self.base_mut().set_velocity(v);
        }

        match self.state {
            State::Chasing => self.tick_chase(delta),
            State::Attacking => self.tick_attack(delta),
            State::Dying | State::Dormant => {
                let mut v = self.base().get_velocity();
                v.x = 0.0;
                v.z = 0.0;
                self.base_mut().set_velocity(v);
            }
        }

        self.base_mut().move_and_slide();
    }
}

#[godot_api]
impl Enemy {
    #[func]
    fn get_gravity_strength(&self) -> f32 {
        self.gravity
    }

    #[func]
    fn set_gravity_strength(&mut self, value: f32) {
        self.gravity = value;
    }

    #[signal]
    pub fn despawned(enemy: Gd<Enemy>);

    #[func]
    pub fn is_active(&self) -> bool {
        self.state != State::Dormant
    }

    /// Called by the weapon just before applying damage, so the death handler
    /// knows whether the killing blow was a headshot.
    #[func]
    pub fn note_incoming_hit(&mut self, is_headshot: bool) {
        self.last_hit_was_headshot = is_headshot;
    }

    /// Take this enemy out of play without freeing it. `PROCESS_MODE_DISABLED`
    /// stops `physics_process` entirely, so a dormant enemy costs nothing but
    /// memory.
    #[func]
    pub fn deactivate(&mut self) {
        self.state = State::Dormant;
        self.target = None;
        self.body_shape.set_disabled(true);
        self.head_hitbox.set_monitorable(false);

        let mut base = self.base_mut();
        base.set_visible(false);
        base.set_collision_layer_value(3, false);
        base.set_velocity(Vector3::ZERO);
        // Park it far below the arena so a stray query cannot find it.
        base.set_global_position(Vector3::new(0.0, -100.0, 0.0));
        base.set_process_mode(ProcessMode::DISABLED);
    }

    #[func]
    fn return_to_pool(&mut self) {
        self.deactivate();
        let this = self.to_gd();
        self.signals().despawned().emit(&this);
    }

    #[func]
    fn restore_colour(&mut self) {
        let base_color = self.base_color;
        if let Some(material) = &mut self.material {
            material.set_albedo(base_color);
        }
    }
}

impl Enemy {
    // -------------------------------------------------------- pool interface

    /// Bring this enemy into play. Everything mutable is reset here -- a pooled
    /// object that relies on `ready` for initialisation works exactly once.
    pub fn activate(
        &mut self,
        spawn_position: Vector3,
        target: Gd<Node3D>,
        health_scale: f32,
        speed_scale: f32,
    ) {
        self.target = Some(target);
        self.state = State::Chasing;
        self.attack_cooldown = 0.0;
        self.windup_remaining = 0.0;
        self.last_hit_was_headshot = false;

        {
            let mut health = self.health.bind_mut();
            health.max_health = 150.0 * health_scale;
            health.reset();
        }
        self.move_speed = 2.6 * speed_scale;

        self.restore_colour();

        self.body_shape.set_disabled(false);
        self.head_hitbox.set_monitorable(true);

        let mut base = self.base_mut();
        base.set_global_position(spawn_position);
        base.set_velocity(Vector3::ZERO);
        base.set_rotation(Vector3::ZERO);
        base.set_visible(true);
        base.set_collision_layer_value(3, true);
        base.set_process_mode(ProcessMode::INHERIT);
    }

    // ------------------------------------------------------------ simulation

    fn tick_chase(&mut self, delta: f64) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !target.is_instance_valid() {
            return;
        }

        let position = self.base().get_global_position();
        let target_position = target.get_global_position();
        let mut to_target = target_position - position;
        to_target.y = 0.0;

        if to_target.length() <= self.attack_range {
            self.state = State::Attacking;
            let mut v = self.base().get_velocity();
            v.x = 0.0;
            v.z = 0.0;
            self.base_mut().set_velocity(v);
            return;
        }

        // Only repath when the target has actually moved. Setting
        // `target_position` every frame forces a full path recalculation every
        // frame, which is the usual reason navigation shows up as a CPU spike.
        if target_position.distance_to(self.last_target_position) > self.repath_threshold {
            self.last_target_position = target_position;
            self.agent.set_target_position(target_position);
        }

        if self.agent.is_navigation_finished() {
            return;
        }

        // The NEXT point along the path, not the destination. Using the
        // destination directly makes enemies walk into walls.
        let next_point = self.agent.get_next_path_position();
        let mut direction = next_point - position;
        direction.y = 0.0;

        if direction.length_squared() < 0.0001 {
            return;
        }

        direction = direction.normalized();
        let speed = self.move_speed;
        let mut v = self.base().get_velocity();
        v.x = direction.x * speed;
        v.z = direction.z * speed;
        self.base_mut().set_velocity(v);
        self.face(direction, delta);
    }

    fn tick_attack(&mut self, delta: f64) {
        let Some(target) = self.target.clone() else {
            self.state = State::Chasing;
            return;
        };
        if !target.is_instance_valid() {
            self.state = State::Chasing;
            return;
        }

        let mut to_target = target.get_global_position() - self.base().get_global_position();
        to_target.y = 0.0;

        if to_target.length() > self.attack_range * 1.25 {
            self.state = State::Chasing;
            return;
        }

        self.face(to_target.normalized(), delta);

        if self.windup_remaining > 0.0 {
            self.windup_remaining -= delta as f32;
            if self.windup_remaining <= 0.0 {
                self.land_hit();
            }
            return;
        }

        self.attack_cooldown -= delta as f32;
        if self.attack_cooldown <= 0.0 {
            self.attack_cooldown = self.attack_interval;
            self.windup_remaining = self.attack_windup;
            self.flash(Color::from_rgb(0.9, 0.75, 0.3));
        }
    }

    fn land_hit(&mut self) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !target.is_instance_valid() {
            return;
        }

        // Re-check range at the moment the blow lands, so backing off during
        // the windup actually avoids it. Without this the swing is undodgeable
        // and the windup is decoration.
        let mut to_target = target.get_global_position() - self.base().get_global_position();
        to_target.y = 0.0;
        if to_target.length() > self.attack_range * 1.3 {
            return;
        }

        if let Some(mut health) = find_health(&target.clone().upcast::<Node>()) {
            let source = self.to_gd().upcast::<Node>();
            health
                .bind_mut()
                .apply_damage(self.attack_damage, Some(source));
        }
    }

    fn face(&mut self, direction: Vector3, delta: f64) {
        let wanted = (-direction.x).atan2(-direction.z);
        let weight = 1.0 - (-self.turn_speed * delta as f32).exp();
        let mut rotation = self.base().get_rotation();
        rotation.y = lerp_angle(rotation.y, wanted, weight);
        self.base_mut().set_rotation(rotation);
    }

    // ---------------------------------------------------------------- damage

    fn on_damaged(&mut self, amount: f32, _current: f32, source: Option<Gd<Node>>) {
        self.flash(Color::from_rgb(1.0, 0.4, 0.35));

        let this = self.to_gd().upcast::<Node3D>();
        let headshot = self.last_hit_was_headshot;
        EventBus::singleton().signals().enemy_damaged().emit(
            &this,
            amount,
            headshot,
            source.as_ref(),
        );

        GameState::singleton()
            .bind_mut()
            .award_points(self.points_on_hit, "hit".into());
    }

    fn on_died(&mut self, killer: Option<Gd<Node>>) {
        if self.state == State::Dying {
            return;
        }
        self.state = State::Dying;

        self.base_mut().set_collision_layer_value(3, false);
        self.head_hitbox.set_monitorable(false);

        let headshot = self.last_hit_was_headshot;
        let award = if headshot {
            self.points_on_headshot_kill
        } else {
            self.points_on_kill
        };
        {
            let mut state = GameState::singleton();
            let mut state = state.bind_mut();
            state.record_kill(headshot);
            state.award_points(award, "kill".into());
        }

        let this = self.to_gd().upcast::<Node3D>();
        EventBus::singleton()
            .signals()
            .enemy_died()
            .emit(&this, killer.as_ref());

        let callback = Callable::from_object_method(&self.to_gd(), "return_to_pool");
        let target = self.to_gd();
        let mut tween: Gd<Tween> = self.base_mut().create_tween();
        tween
            .tween_property(&target, "rotation_degrees:x", &(-85.0).to_variant(), 0.35)
            .set_trans(TransitionType::CUBIC)
            .set_ease(EaseType::IN);
        tween.tween_interval(0.6);
        tween.tween_callback(&callback);
    }

    fn flash(&mut self, colour: Color) {
        let Some(material) = self.material.clone() else {
            return;
        };
        let mut material = material;
        material.set_albedo(colour);

        let base_color = self.base_color;
        let mut tween = self.base_mut().create_tween();
        tween.tween_property(&material, "albedo_color", &base_color.to_variant(), 0.18);
    }
}

/// Godot's `lerp_angle`, which takes the shorter way around the circle.
fn lerp_angle(from: f32, to: f32, weight: f32) -> f32 {
    let difference = (to - from) % std::f32::consts::TAU;
    let distance = (2.0 * difference) % std::f32::consts::TAU - difference;
    from + distance * weight
}
