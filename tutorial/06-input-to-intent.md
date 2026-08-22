# Lesson 6 — Input → intent

## What we're building

A seam. The player stops reading `Input` and starts reading a struct that says
what it wants to do. A separate node fills that struct in from real hardware.

Nothing visible changes. Everything about how the rest of the project is written
changes, which is why this lesson exists now rather than after there is a weapon,
an interactor and an enemy all reading `Input` independently.

---

## The concept

### The problem

Right now the controller reads `Input` directly. That is fine for a controller,
and it stops being fine the moment anything else needs to know what the player is
doing.

Consider what is coming: a weapon that fires on `fire`, an interactor that acts on
`interact`, a reload state machine on `reload`. If each of them reads `Input`
itself, you get:

- **Four things polling the same global.** Nothing can tell you who consumed what.
- **No way to test any of it.** You cannot write "fire the weapon" in a test
  without synthesising OS-level input events.
- **No way to drive a player with anything else.** No AI, no replay, no network.
- **A real bug at high frame rates**, which is the part that is not
  architecture-astronaut theorising — see below.

### The frame-rate bug, concretely

`process` runs per rendered frame. `physics_process` runs 60 times a second. At
200 fps that is roughly three renders per physics tick.

`Input::is_action_just_pressed("jump")` is true for one *tick* of whichever loop
asks. Ask from `physics_process` and it works. Ask from `process` — as a weapon
firing a semi-automatic shot might — and you can read it on a frame that the
simulation never acts on, and the press vanishes.

The fix is **latching**: record the press when it happens, keep it until the
simulation has acted, then clear it. That is a piece of state, and state needs
somewhere to live. That somewhere is the intent struct.

### The design

```
     hardware              PlayerInputSource            Player / Weapon /
  (keys, mouse,     ->    (the only thing that    ->    Interactor
   gamepad)                touches `Input`)              (read the struct)
                                  |
                            PlayerIntent
```

`PlayerIntent` holds what the player *wants*: a movement direction, an
accumulated look delta, some held flags, some latched one-shots. It says nothing
about how any of it was expressed.

Swap `PlayerInputSource` for something else and the simulation cannot tell:

- A **network** source filling it from packets → co-op.
- An **AI** source → bots, and a demo mode for a menu background.
- A **replay** source reading a recorded file → deterministic bug reproduction.

None of those are in scope here. The point is that all of them stay *possible*
for about twenty lines, and become a rewrite of every gameplay file if you skip
this.

### Not everything should be a Godot class

Here is a decision worth making deliberately, because gdext makes it easy to
reach for `#[derive(GodotClass)]` reflexively.

`PlayerIntent` is a **plain Rust struct**:

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerIntent { /* ... */ }
```

No derive, no `Base<T>`, no registration. Because:

- **Nothing outside Rust touches it.** The input source writes it, the player and
  weapon read it. Registering it would be pure cost.
- **`Copy` is exactly what we want.** The player reads a *copy* each tick and
  passes it to the weapon and the interactor. No refcounting, no `bind()`, no
  chance of a double-borrow panic.
- **It is trivially testable.** `PlayerIntent { fire_held: true, ..Default::default() }`
  and call `weapon.tick(&intent, 1.0/60.0)`. Lesson 10's test suite does exactly
  that, and it is only possible because this type has no engine entanglement.

The rule: **make it a `GodotClass` when Godot needs to see it.** A node in a
scene, a resource on disk, something GDScript calls, something that receives
signals. Otherwise write ordinary Rust.

This is the first place where the Rust version comes out *better* than the
GDScript one rather than merely different. GDScript's version of this type has to
be a `RefCounted` object, allocated and refcounted, because GDScript has no plain
structs.

### Consuming, and who is allowed to

Two kinds of field, cleared in two different places:

- **`look_delta` accumulates** across however many mouse events arrive, and is
  zeroed by the player once applied — in `process`, because looking is per-frame.
- **One-shots latch** with `|=` and are cleared by `clear_one_shots()` at the end
  of `physics_process`, once the simulation has acted.

The `|=` matters:

<!-- illustrative -->
```rust
// Wrong at high frame rates: two frames later, this erases the press.
self.intent.jump_pressed = input.is_action_just_pressed("jump");

// Right: latch it, and let the simulation decide when it is spent.
self.intent.jump_pressed |= input.is_action_just_pressed("jump");
```

**Exactly one place is allowed to clear each field.** Two consumers clearing the
same latch means one of them silently eats the other's input, and that bug is
extremely annoying to find. In this project the player owns clearing, because the
player is what drives the weapon and interactor.

---

## Do it

### Step 1 — The intent struct

Create `rust/src/player_intent.rs`:

```rust
use godot::prelude::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerIntent {
    /// Movement on the ground plane, in the player's own space.
    /// x = strafe (+right), y = forward (+forward). Length <= 1.
    pub move_dir: Vector2,

    /// Accumulated look change in RADIANS since the last time it was consumed.
    /// x = yaw (+left), y = pitch (+up).
    pub look_delta: Vector2,

    // Continuous states -- true for as long as the player holds them.
    pub sprint_held: bool,
    pub fire_held: bool,
    pub aim_held: bool,

    // One-shot events. These LATCH: they stay true until the simulation
    // consumes them, which is what stops a fast render rate from dropping
    // inputs that happened between two physics ticks.
    pub jump_pressed: bool,
    pub fire_pressed: bool,
    pub reload_pressed: bool,
    pub interact_pressed: bool,
}

impl PlayerIntent {
    /// Called by the simulation once it has acted on this intent.
    pub fn clear_one_shots(&mut self) {
        self.jump_pressed = false;
        self.fire_pressed = false;
        self.reload_pressed = false;
        self.interact_pressed = false;
    }
}
```

Note `aim_held` and `fire_pressed` are here despite nothing using them yet.
Adding the whole vocabulary now is cheaper than growing it one field at a time,
and it documents what a player is capable of in one readable place.

### Step 2 — The input source

Create `rust/src/player_input.rs`:

```rust
use godot::classes::input::MouseMode;
use godot::classes::{Input, InputEvent, InputEventMouseMotion};
use godot::prelude::*;

use crate::player_intent::PlayerIntent;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct PlayerInputSource {
    #[export(range = (0.0005, 0.01, 0.0001))]
    #[init(val = 0.0022)]
    mouse_sensitivity: f32,
    #[export(range = (0.5, 8.0, 0.1))]
    #[init(val = 3.0)]
    gamepad_sensitivity: f32,
    #[export]
    invert_y: bool,

    /// Read by `Player` every frame. Never replaced wholesale, only mutated.
    pub intent: PlayerIntent,

    base: Base<Node>,
}

#[godot_api]
impl INode for PlayerInputSource {
    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        // Mouse movement arrives as EVENTS, not as a pollable state. Reading it
        // in `process` would miss motion and feel laggy, which is why look
        // accumulates here and gets consumed later.
        let Ok(motion) = event.try_cast::<InputEventMouseMotion>() else {
            return;
        };
        if Input::singleton().get_mouse_mode() != MouseMode::CAPTURED {
            return;
        }

        let relative = motion.get_relative();
        let invert = if self.invert_y { -1.0 } else { 1.0 };
        self.intent.look_delta.x -= relative.x * self.mouse_sensitivity;
        self.intent.look_delta.y -= relative.y * self.mouse_sensitivity * invert;
    }

    fn process(&mut self, delta: f64) {
        let input = Input::singleton();

        // `get_vector` handles deadzones and diagonal normalisation for us, and
        // works identically for the keyboard and the gamepad stick bound to the
        // same actions. Argument order is (neg_x, pos_x, neg_y, pos_y).
        self.intent.move_dir =
            input.get_vector("move_left", "move_right", "move_back", "move_forward");

        // Gamepad look is a HELD axis rather than a burst of motion, so unlike
        // the mouse it has to be scaled by delta.
        let stick = input.get_vector("look_left", "look_right", "look_up", "look_down");
        if stick.length_squared() > 0.0 {
            let invert = if self.invert_y { -1.0 } else { 1.0 };
            let scale = self.gamepad_sensitivity * delta as f32;
            self.intent.look_delta.x -= stick.x * scale;
            self.intent.look_delta.y -= stick.y * scale * invert;
        }

        self.intent.sprint_held = input.is_action_pressed("sprint");
        self.intent.fire_held = input.is_action_pressed("fire");
        self.intent.aim_held = input.is_action_pressed("aim");

        // Latch one-shots with `|=` rather than `=`. At 200 fps roughly three
        // frames run per physics tick; a plain assignment would let the two
        // frames after the press erase it before the simulation ever saw it.
        self.intent.jump_pressed |= input.is_action_just_pressed("jump");
        self.intent.fire_pressed |= input.is_action_just_pressed("fire");
        self.intent.reload_pressed |= input.is_action_just_pressed("reload");
        self.intent.interact_pressed |= input.is_action_just_pressed("interact");
    }
}
```

Notice the asymmetry between mouse and stick. The mouse delivers *displacement*
already — "you moved 14 pixels" — so multiplying by `delta` would be wrong twice
over. The stick delivers a *rate* — "you are pushing 0.8 to the left" — so it
must be multiplied by elapsed time. Getting this backwards gives you a mouse that
is frame-rate dependent, or a stick that turns at wildly different speeds
depending on your monitor.

### Step 3 — Rewrite the player to read it

Rename the class back to `Player` and change how it gets input. The parts that
change:

```rust
    #[init(node = "InputSource")]
    pub input_source: OnReady<Gd<PlayerInputSource>>,
```

and in the two per-frame methods:

```rust
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
```

```rust
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
```

The weapon and interactor lines will not compile yet — those classes arrive in
Lessons 7 and 17. Comment them out for now and uncomment as you go. The full
`player.rs` is in `reference/rust/src/player.rs`.

`apply_look` becomes:

```rust
    fn apply_look(&mut self, intent: &PlayerIntent) {
        self.base_mut().rotate_y(intent.look_delta.x);

        let limit = self.pitch_limit_degrees.to_radians();
        self.pitch = (self.pitch + intent.look_delta.y).clamp(-limit, limit);
    }
```

and `apply_horizontal_movement` takes the intent instead of asking `Input`:

```rust
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
```

### Step 4 — `bind()` and `bind_mut()`, and how to not panic

This is the other gdext concept you need, alongside `base_mut()`.

`Gd<T>` is a *handle*. To reach the Rust struct inside it you borrow:

```rust
let intent = self.input_source.bind().intent;        // shared borrow
self.input_source.bind_mut().intent.clear_one_shots(); // exclusive borrow
```

These are checked **at runtime**, not compile time — gdext cannot know statically
whether two handles point at the same object. Break the rules and you get a panic
that names the class and the method:

```
godot-rust function call failed: Player::physics_process()
  Attempt to call bind_mut() on a Gd<PlayerInputSource> that is already borrowed
```

Two habits keep this from happening:

**Keep the guard's lifetime as short as possible.** Write
`self.input_source.bind().intent` as its own statement, not inside a larger
expression where the guard survives longer than you expect.

**Copy small data out instead of holding a borrow across a call.** This is why
`PlayerIntent` is `Copy`:

<!-- illustrative -->
```rust
// Good: the borrow ends at the semicolon, `intent` is an independent copy.
let intent = self.input_source.bind().intent;
self.weapon.bind_mut().tick(&intent, delta);

// Risky: the borrow on input_source is alive while weapon code runs, and if
// that code ever reaches back to the input source, it panics.
self.weapon.bind_mut().tick(&self.input_source.bind().intent, delta);
```

Where the value is too big to copy, scope the borrow explicitly with a block:

<!-- illustrative -->
```rust
{
    let mut health = self.health.bind_mut();
    health.max_health = 150.0 * health_scale;
    health.reset();
}   // borrow released here, before anything else runs
```

You will see that pattern throughout the reference build, and every one of them
is deliberate.

### Step 5 — Wire the scene

Open `player.tscn`. Add a **`PlayerInputSource`** child named exactly
`InputSource` — the `#[init(node = "InputSource")]` looks it up by that string.

Build and run. Everything behaves exactly as it did at the end of Lesson 5.

**That is the intended outcome.** A refactor that changes behaviour has a bug in
it. What changed is that the player no longer knows a keyboard exists, and there
is now one place — and only one — where hardware enters the game.

---

## Check yourself

1. Why does `PlayerIntent` latch one-shots with `|=` instead of `=`?
2. Why is the mouse look delta *not* multiplied by `delta` while the gamepad
   stick is?
3. Why is `PlayerIntent` a plain Rust struct rather than a `GodotClass`?
4. Why does it derive `Copy`, and what does that buy at the call site?
5. Where is `look_delta` cleared, and where are the one-shots cleared? Why are
   those different places?
6. What is the difference between `bind()` and `base()`?
7. What runtime error does holding a `bind_mut()` across a call to another
   object risk, and what is the habit that avoids it?

<details>
<summary>Answers</summary>

1. At high frame rates several `process` calls happen per physics tick. A plain
   assignment lets a later frame overwrite the press with `false` before the
   simulation ever sees it.
2. The mouse reports displacement that already happened; the stick reports a
   held rate. Only the rate needs multiplying by elapsed time.
3. Nothing outside Rust touches it, so registering it would cost an allocation
   and refcounting for no benefit.
4. So consumers can take an independent copy and the borrow on the input source
   ends immediately — which is what keeps `bind_mut()` from panicking later.
5. `look_delta` in `process`, because looking is per-frame; the one-shots at the
   end of `physics_process`, because that is when the simulation has acted.
6. `base()` reaches the *engine* object your struct extends. `bind()` reaches the
   *Rust* struct inside someone else's `Gd<T>` handle.
7. A double-borrow panic, if the called code reaches back to the borrowed object.
   Copy small data out first, or scope the borrow in a block.

</details>

---

## Extend it

- Write a second input source — a `DemoInputSource` that walks in a slow circle
  and occasionally jumps. Swap it into `player.tscn` in place of the real one and
  watch the player drive itself. This is the payoff, and it takes about fifteen
  minutes.
- Add `#[export] mouse_smoothing: f32` that eases `look_delta` toward the raw
  value. Try it. Most players hate it; understanding *why* it was ever added is
  worth the experiment.
- Add a field to `PlayerIntent` that records how many frames a one-shot has been
  latched, and print it. On a machine that can run this project at 300 fps you
  will see it hit 4 or 5 — which is the bug this lesson prevents, made visible.

---

## Commit

```bash
git add -A
git commit -m "Lesson 6: separate input from simulation via PlayerIntent"
```

---

**Next:** [Lesson 7 — Hitscan](07-hitscan.md)
