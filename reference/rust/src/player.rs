//! First-person controller. Reads a `PlayerIntent` -- never `Input` directly.

use godot::classes::input::MouseMode;
use godot::classes::{Camera3D, CharacterBody3D, ICharacterBody3D, Input, Node3D};
use godot::prelude::*;

use crate::event_bus::EventBus;
use crate::health::Health;
use crate::interactor::Interactor;
use crate::player_input::PlayerInputSource;
use crate::player_intent::PlayerIntent;
use crate::weapon::{Weapon, smooth};

#[derive(GodotClass)]
#[class(base=CharacterBody3D, init)]
pub struct Player {
    #[export]
    #[init(val = 5.2)]
    walk_speed: f32,
    #[export]
    #[init(val = 8.0)]
    sprint_speed: f32,
    /// Not 9.8. Realistic gravity makes a shooter feel like it is underwater.
    ///
    /// The custom accessor names are not decoration: `#[export]` would
    /// otherwise generate `get_gravity`, which shadows `CharacterBody3D`'s own
    /// `get_gravity()`. gdext warns about that today and will reject it in
    /// v0.6, so the property keeps the name `gravity` while its accessors do not.
    #[export]
    #[var(get = get_gravity_strength, set = set_gravity_strength)]
    #[init(val = 26.0)]
    gravity: f32,
    #[export]
    #[init(val = 7.2)]
    jump_velocity: f32,
    #[export]
    #[init(val = 65.0)]
    acceleration: f32,
    #[export]
    #[init(val = 75.0)]
    friction: f32,
    /// How much steering you keep in mid-air. 0 = none, 1 = full ground control.
    #[export(range = (0.0, 1.0))]
    #[init(val = 0.3)]
    air_control: f32,

    #[export(range = (60.0, 89.9))]
    #[init(val = 89.0)]
    pitch_limit_degrees: f32,

    #[export]
    #[init(val = 1.7)]
    bob_frequency: f32,
    #[export]
    #[init(val = 0.045)]
    bob_amplitude: f32,
    #[export]
    #[init(val = 78.0)]
    base_fov: f32,
    #[export]
    #[init(val = 9.0)]
    sprint_fov_bonus: f32,
    #[export]
    #[init(val = 8.0)]
    fov_response: f32,
    #[export]
    #[init(val = 11.0)]
    recoil_recovery: f32,

    #[init(node = "Head")]
    head: OnReady<Gd<Node3D>>,
    #[init(node = "Head/CameraRig")]
    camera_rig: OnReady<Gd<Node3D>>,
    #[init(node = "Head/CameraRig/Camera3D")]
    pub camera: OnReady<Gd<Camera3D>>,
    #[init(node = "Head/CameraRig/Camera3D/Weapon")]
    pub weapon: OnReady<Gd<Weapon>>,
    #[init(node = "InputSource")]
    pub input_source: OnReady<Gd<PlayerInputSource>>,
    #[init(node = "Interactor")]
    pub interactor: OnReady<Gd<Interactor>>,
    #[init(node = "Health")]
    pub health: OnReady<Gd<Health>>,

    pitch: f32,
    bob_time: f32,
    bob_amount: f32,
    /// Current recoil offset in radians, applied on top of aim and eased to 0.
    recoil: Vector2,

    base: Base<CharacterBody3D>,
}

#[godot_api]
impl ICharacterBody3D for Player {
    fn ready(&mut self) {
        Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
        let fov = self.base_fov;
        self.camera.set_fov(fov);

        // Explicit dependency injection. The weapon needs a camera to aim from,
        // and handing it over is far more robust than having the weapon guess
        // at a node path -- rearranging the scene later cannot silently break it.
        let camera = self.camera.clone();
        let body = self.to_gd().upcast::<Node3D>();
        self.weapon.bind_mut().setup(camera.clone(), body.clone());
        self.interactor.bind_mut().setup(camera, body);

        let this = self.to_gd();
        self.weapon
            .signals()
            .recoil_kick()
            .connect_other(&this, Player::on_recoil_kick);
        self.health
            .signals()
            .damaged()
            .connect_other(&this, Player::on_health_damaged);
        self.health
            .signals()
            .died()
            .connect_other(&this, Player::on_health_died);
    }

    // ------------------------------------------------------------- per frame

    fn process(&mut self, delta: f64) {
        let intent = self.input_source.bind().intent;

        self.apply_look(&intent);
        self.apply_bob(delta);
        self.apply_fov(&intent, delta);
        self.recover_recoil(delta);
        self.apply_camera_rotation();

        // Look has been consumed -- zero it so the same motion is not applied
        // twice. We read a COPY above, so the clear has to be explicit.
        self.input_source.bind_mut().intent.look_delta = Vector2::ZERO;
    }

    // ----------------------------------------------------------- per physics

    fn physics_process(&mut self, delta: f64) {
        let intent = self.input_source.bind().intent;

        let on_floor = self.base().is_on_floor();
        let mut velocity = self.base().get_velocity();
        if on_floor {
            if intent.jump_pressed {
                velocity.y = self.jump_velocity;
            }
        } else {
            velocity.y -= self.gravity * delta as f32;
        }
        self.base_mut().set_velocity(velocity);

        self.apply_horizontal_movement(&intent, delta);

        self.base_mut().move_and_slide();

        // Weapon and interactor are both driven from here rather than polling
        // Input themselves, so the same intent that moved the player also fires
        // the gun.
        self.weapon.bind_mut().tick(&intent, delta);
        self.interactor.bind_mut().tick(&intent);

        // The simulation has now acted on this intent, so release the latches.
        self.input_source.bind_mut().intent.clear_one_shots();
    }
}

#[godot_api]
impl Player {
    #[func]
    fn get_gravity_strength(&self) -> f32 {
        self.gravity
    }

    #[func]
    fn set_gravity_strength(&mut self, value: f32) {
        self.gravity = value;
    }

    /// Called by `Main` once the HUD exists. Keeping the wiring in one place
    /// beats having the HUD hunt for the player.
    #[func]
    pub fn bind_hud(&mut self, hud: Gd<crate::hud::Hud>) {
        let mut hud = hud;
        let weapon = self.weapon.clone();
        let health = self.health.clone();
        let mut hud_ref = hud.bind_mut();
        hud_ref.bind_weapon(weapon);
        hud_ref.bind_health(health);
    }
}

impl Player {
    /// Look runs per FRAME, not per physics tick, so it stays smooth at any
    /// refresh rate. Movement stays in `physics_process` where it belongs.
    fn apply_look(&mut self, intent: &PlayerIntent) {
        self.base_mut().rotate_y(intent.look_delta.x);

        let limit = self.pitch_limit_degrees.to_radians();
        self.pitch = (self.pitch + intent.look_delta.y).clamp(-limit, limit);
    }

    fn apply_camera_rotation(&mut self) {
        let mut head_rotation = self.head.get_rotation();
        head_rotation.x = self.pitch;
        self.head.set_rotation(head_rotation);

        let mut rig_rotation = self.camera_rig.get_rotation();
        rig_rotation.x = self.recoil.y;
        rig_rotation.y = self.recoil.x;
        self.camera_rig.set_rotation(rig_rotation);
    }

    fn apply_bob(&mut self, delta: f64) {
        let velocity = self.base().get_velocity();
        let planar_speed = Vector2::new(velocity.x, velocity.z).length();
        let moving = self.base().is_on_floor() && planar_speed > 0.6;

        // Ease the AMPLITUDE toward its target rather than the bob position
        // itself, so stopping mid-stride settles smoothly instead of snapping.
        let target = if moving { self.bob_amplitude } else { 0.0 };
        self.bob_amount = smooth(self.bob_amount, target, 9.0, delta);
        self.bob_time += delta as f32 * planar_speed * self.bob_frequency;

        let mut position = self.camera_rig.get_position();
        position.y = self.bob_time.sin() * self.bob_amount;
        position.x = (self.bob_time * 0.5).cos() * self.bob_amount * 0.6;
        self.camera_rig.set_position(position);
    }

    fn apply_fov(&mut self, intent: &PlayerIntent, delta: f64) {
        let velocity = self.base().get_velocity();
        let planar_speed = Vector2::new(velocity.x, velocity.z).length();
        let sprinting = intent.sprint_held && planar_speed > 1.5;
        let target = self.base_fov
            + if sprinting {
                self.sprint_fov_bonus
            } else {
                0.0
            };
        let fov = smooth(self.camera.get_fov(), target, self.fov_response, delta);
        self.camera.set_fov(fov);
    }

    fn recover_recoil(&mut self, delta: f64) {
        self.recoil.x = smooth(self.recoil.x, 0.0, self.recoil_recovery, delta);
        self.recoil.y = smooth(self.recoil.y, 0.0, self.recoil_recovery, delta);
    }

    fn on_recoil_kick(&mut self, pitch_degrees: f32, yaw_degrees: f32) {
        self.recoil.y += pitch_degrees.to_radians();
        self.recoil.x += yaw_degrees.to_radians();
    }

    fn apply_horizontal_movement(&mut self, intent: &PlayerIntent, delta: f64) {
        // `move_dir` is in the player's own space; the basis multiply turns it
        // into world space. -Z is forward in Godot, hence the minus sign.
        let basis = self.base().get_transform().basis;
        let mut wish = basis * Vector3::new(intent.move_dir.x, 0.0, -intent.move_dir.y);
        wish.y = 0.0;

        let speed = if intent.sprint_held {
            self.sprint_speed
        } else {
            self.walk_speed
        };
        let control = if self.base().is_on_floor() {
            1.0
        } else {
            self.air_control
        };

        let mut velocity = self.base().get_velocity();

        if wish.length_squared() > 0.001 {
            let target = wish.normalized() * speed;
            // `move_toward`, not `lerp`: it closes at a constant rate and
            // actually ARRIVES, where lerp approaches forever and never quite
            // gets there.
            let step = self.acceleration * control * delta as f32;
            velocity.x = move_toward(velocity.x, target.x, step);
            velocity.z = move_toward(velocity.z, target.z, step);
        } else {
            let step = self.friction * control * delta as f32;
            velocity.x = move_toward(velocity.x, 0.0, step);
            velocity.z = move_toward(velocity.z, 0.0, step);
        }

        self.base_mut().set_velocity(velocity);
    }

    // --------------------------------------------------------------- damage

    fn on_health_damaged(&mut self, amount: f32, current: f32, _source: Option<Gd<Node>>) {
        let max = self.health.bind().max_health;
        EventBus::singleton()
            .signals()
            .player_damaged()
            .emit(amount, current, max);
    }

    fn on_health_died(&mut self, _source: Option<Gd<Node>>) {
        EventBus::singleton().signals().player_died().emit();
        // A later lesson turns this into a real game-over flow. For now, stop
        // moving and release the cursor so the run visibly ends rather than
        // silently continuing.
        self.base_mut().set_physics_process(false);
        Input::singleton().set_mouse_mode(MouseMode::VISIBLE);
    }
}

/// Godot's `move_toward`: step from `from` to `to` by at most `delta`.
pub fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}
