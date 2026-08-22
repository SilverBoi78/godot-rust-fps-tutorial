# Lesson 4 — FPS controller I

## What we're building

A first-person controller you can walk and jump around the arena with, and look
around using the mouse. No acceleration, no polish — instant start, instant stop.
Lesson 5 adds the feel.

This lesson also contains your first serious argument with the borrow checker,
which is not incidental. It is the thing that makes writing Godot in Rust feel
different, and it is much better to meet it here, in twenty lines of movement
code, than at 1am in Lesson 12.

---

## The concept

### `CharacterBody3D`

Godot has three kinds of solid body, and picking the right one matters:

| Node | Who moves it | Use for |
|---|---|---|
| `StaticBody3D` | nobody | walls, floors |
| `RigidBody3D` | the physics engine | barrels, debris |
| `CharacterBody3D` | **you** | players, enemies |

A `CharacterBody3D` is solid and collides with the world, but the physics engine
never pushes it. You set `velocity`, call `move_and_slide()`, and it moves —
sliding along walls instead of stopping dead against them.

That last part is the reason this node exists. Naive collision handling makes a
character catch on every doorframe. `move_and_slide` projects the leftover motion
along the surface, which is what makes movement feel smooth rather than sticky.

### `process` vs `physics_process`

| | `process` | `physics_process` |
|---|---|---|
| Runs | once per rendered frame | 60 times per second, fixed |
| Rate | whatever the GPU manages | constant regardless of frame rate |
| `delta` | varies constantly | effectively fixed |
| Use for | visuals, camera, UI | movement, collision, anything physical |

**Movement goes in `physics_process`.** Always. Physics is stepped on a fixed
clock, so moving a body outside that clock means moving it between steps, which
produces jitter, tunnelling through thin walls, and collision results that differ
from machine to machine.

**Mouse look goes in `process`** (or in the input callback). Looking is not
physical — it changes what you see, not where you are. Limiting it to 60 Hz on a
165 Hz monitor is immediately visible as choppiness.

So this controller splits: rotation per frame, movement per tick. Almost every
FPS controller does.

### Mouse motion is an event, not a state

You can *ask* whether a key is down — `Input::is_action_pressed`. You cannot ask
"how far did the mouse move"; the OS delivers it as a stream of events, and if
you do not read them as they arrive, you lose them.

So mouse look is handled in `unhandled_input`, which Godot calls once per motion
event with an `InputEventMouseMotion` carrying a `relative` vector.

> **Why `unhandled_input` rather than `input`?** Godot offers input to the UI
> first. Anything the UI consumes never reaches `unhandled_input`. Use it for
> gameplay and a menu will not fire your gun through it.

### Capturing the mouse

An FPS needs the cursor hidden and locked — otherwise you hit the edge of the
screen and stop turning.

```rust
Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
```

`MouseMode` lives in `godot::classes::input`, not the prelude and not
`godot::global`. Engine enums that belong to a class live in a module named after
that class in snake_case. This is worth writing on a sticky note.

> **You will lock yourself out.** Once captured, the cursor is gone, and if your
> game has no way to release it you cannot get back to the editor without
> alt-tabbing. Godot's editor releases it when the game window loses focus, and
> **F8** stops a running game. Know both before you press play.

### The Input Map: never read keys directly

You could ask "is W held". Do not. Instead you define **actions** — `move_forward`,
`jump`, `fire` — bind keys and gamepad buttons to them, and ask about actions.

Three reasons, all of which bite eventually:

1. **Rebinding.** Changing a key becomes a settings change, not a code change.
2. **Gamepads.** The same action carries a key and a stick axis, and your code
   never learns which one was used.
3. **Keyboard layouts.** Bound by *physical key*, W-A-S-D stays a diamond under
   your left hand on AZERTY and Dvorak. Bound by character, French players get
   Z-Q-S-D and a bad first impression.

`Input::get_vector` then turns four actions into a `Vector2`, handling deadzones
and diagonal normalisation for you. Without it, holding W and D gives you 1.41×
your intended speed, and "diagonal is faster" is a bug shipped by a surprising
number of real games.

### `self.base_mut()` borrows everything

Here is the thing that makes gameplay Rust feel different.

```rust
// Does not compile.
self.base_mut().rotate_y(-relative.x * self.mouse_sensitivity);
```

The compiler says:

```
error[E0503]: cannot use `self.mouse_sensitivity` because it was mutably borrowed
  |
  |         self.base_mut()
  |         ---------------
  |         `*self` is borrowed here
  |         a temporary with access to the borrow is created here ...
  |             .rotate_y(-relative.x * self.mouse_sensitivity);
  |                                     ^^^^^^^^^^^^^^^^^^^^^^ use of borrowed `*self`
```

`base_mut()` takes `&mut self` and returns a guard that holds that borrow for as
long as it lives — which, in a single expression, is until the end of the
statement. So the argument list cannot read `self`.

The fix is mechanical:

```rust
let yaw = -relative.x * self.mouse_sensitivity;   // read
self.base_mut().rotate_y(yaw);                    // then write
```

**Compute everything you need, then touch the base.** Once you have internalised
that, this error stops happening. Until then it will happen a lot, and it is not
a sign you are doing something wrong — it is a sign you are writing Godot code in
the shape GDScript taught you.

The same rule applies to the whole `self`, not just the base:

```rust
let mut velocity = self.base().get_velocity();   // read out
velocity.y -= self.gravity * delta as f32;       // work on a local
self.base_mut().set_velocity(velocity);          // write back
```

That read-modify-write triple is the single most common shape in this codebase.
It is more verbose than GDScript's `velocity.y -= gravity * delta`, and there is
no way around it. Decide now whether that is a price you are happy to pay; the
rest of the tutorial assumes yes.

### Godot's coordinate system

- **+X right, +Y up, +Z toward the camera.** So **forward is −Z**.
- Rotations in the Inspector are **degrees**; in code they are **radians**.
  `to_radians()` and `to_degrees()` convert.
- `transform.basis` is the node's orientation. Multiplying a local direction by
  it gives a world direction — which is how "forward" becomes "forward *for this
  player*".

The −Z thing catches everyone. If your character walks backwards, that is why.

---

## Do it

### Step 1 — Define the input actions

**Project → Project Settings → Input Map.**

Add each action name in the box and press **Add**, then click the **+** next to
it to bind. When the key dialog opens, press the **key icon** and choose
**Physical Key**, then press the key.

| Action | Key | Gamepad |
|---|---|---|
| `move_forward` | W | Left stick up |
| `move_back` | S | Left stick down |
| `move_left` | A | Left stick left |
| `move_right` | D | Left stick right |
| `jump` | Space | A / Cross |
| `sprint` | Shift | Left stick click |
| `fire` | Left mouse | Right trigger |
| `aim` | Right mouse | Left trigger |
| `reload` | R | X / Square |
| `interact` | E | Y / Triangle |
| `look_up` / `look_down` / `look_left` / `look_right` | — | Right stick |
| `pause` | Escape | Start |

Set **Deadzone** to `0.2` on every action with a stick binding. The default 0.5
is far too aggressive and makes analogue movement feel like a d-pad.

Define all of them now, including the ones for later lessons. Coming back to this
screen eight times is tedious.

> **Physical Key, not Key.** The dropdown in the binding dialog defaults to the
> character. Choose **Physical Key** and the binding follows the key's *position*
> — W-A-S-D stays a diamond on every keyboard layout in the world.

### Step 2 — Build the player scene

New scene, root **`CharacterBody3D`**, renamed `Player`. Save as
`res://scenes/player.tscn`.

Children:

| Node | Type | Settings |
|---|---|---|
| `CollisionShape3D` | `CollisionShape3D` | New **CapsuleShape3D**, radius `0.4`, height `1.8`, position `(0, 0.9, 0)` |
| `Head` | `Node3D` | position `(0, 1.62, 0)` |
| `Head/CameraRig` | `Node3D` | position `(0, 0, 0)` |
| `Head/CameraRig/Camera3D` | `Camera3D` | **FOV** `78`, **Near** `0.05` |

On the `Player` root: **Layer** = `Player` only, **Mask** = `World` only.

The capsule sits at `y = 0.9` so its bottom is at the origin — put the origin at
the feet and placing the player is just "put it on the floor".

Three nested nodes for the camera looks like overkill. It is not:

- **`Head`** rotates for looking up and down.
- **`CameraRig`** carries head bob (Lesson 5) and recoil (Lesson 8).
- **`Camera3D`** is just the camera.

Keeping them separate means recoil cannot fight your aim, and head bob cannot
accumulate into your pitch. Every one of those is a real bug that comes from
stacking effects onto one node.

Eye height 1.62m for a 1.8m character is roughly correct, and getting it right
matters: too low reads as a child, too high reads as floating.

### Step 3 — Write the controller

Create `rust/src/player.rs`. This is the Lesson 4 version — the finished one in
`reference/rust/src/player.rs` is bigger, and there is an exact copy of *this*
stage at `reference/rust/src/stages/lesson04_player.rs` to diff against.

```rust
use godot::classes::input::MouseMode;
use godot::classes::{CharacterBody3D, ICharacterBody3D, Input, InputEvent, InputEventMouseMotion, Node3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=CharacterBody3D, init)]
pub struct Lesson04Player {
    #[export]
    #[init(val = 5.2)]
    speed: f32,
    #[export]
    #[init(val = 26.0)]
    fall_acceleration: f32,
    #[export]
    #[init(val = 7.2)]
    jump_velocity: f32,
    #[export(range = (0.0005, 0.01, 0.0001))]
    #[init(val = 0.0022)]
    mouse_sensitivity: f32,
    #[export(range = (60.0, 89.9))]
    #[init(val = 89.0)]
    pitch_limit_degrees: f32,

    #[init(node = "Head")]
    head: OnReady<Gd<Node3D>>,

    pitch: f32,

    base: Base<CharacterBody3D>,
}

#[godot_api]
impl ICharacterBody3D for Lesson04Player {
    fn ready(&mut self) {
        Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        let Ok(motion) = event.try_cast::<InputEventMouseMotion>() else {
            return;
        };
        if Input::singleton().get_mouse_mode() != MouseMode::CAPTURED {
            return;
        }

        let relative = motion.get_relative();
        // Compute the yaw BEFORE calling `base_mut()`. `base_mut()` borrows all
        // of `self`, so reading `self.mouse_sensitivity` inside the same
        // expression is a borrow-checker error. This is the single most common
        // thing to trip over when moving gameplay code from GDScript to Rust.
        let yaw = -relative.x * self.mouse_sensitivity;
        self.base_mut().rotate_y(yaw);

        let limit = self.pitch_limit_degrees.to_radians();
        self.pitch = (self.pitch - relative.y * self.mouse_sensitivity).clamp(-limit, limit);

        let mut rotation = self.head.get_rotation();
        rotation.x = self.pitch;
        self.head.set_rotation(rotation);
    }

    fn physics_process(&mut self, delta: f64) {
        let input = Input::singleton();
        let mut velocity = self.base().get_velocity();

        if self.base().is_on_floor() {
            if input.is_action_just_pressed("jump") {
                velocity.y = self.jump_velocity;
            }
        } else {
            velocity.y -= self.fall_acceleration * delta as f32;
        }

        let move_dir = input.get_vector("move_left", "move_right", "move_back", "move_forward");
        let basis = self.base().get_transform().basis;
        let wish = basis * Vector3::new(move_dir.x, 0.0, -move_dir.y);

        // No acceleration yet -- instant start and stop. Lesson 5 fixes that.
        velocity.x = wish.x * self.speed;
        velocity.z = wish.z * self.speed;

        self.base_mut().set_velocity(velocity);
        self.base_mut().move_and_slide();
    }
}
```

Add `pub mod player;` to `lib.rs` and build.

> **Name it `Lesson04Player` for now**, exactly as written. Lesson 6 rewrites
> this class as `Player`, and having both lets you diff. If you would rather call
> it `Player` from the start, that is fine — just rename it back before Lesson 6
> or you will have two classes fighting over the same name.

### Step 4 — Read the new pieces

**`#[init(node = "Head")]` and `OnReady<Gd<Node3D>>`** — gdext's equivalent of
GDScript's `@onready var head = $Head`.

`OnReady<T>` is a field that is empty until `ready` runs and then holds a value
forever. It `Deref`s to the inner type, so `self.head.get_rotation()` works with
no unwrapping. The `node = "..."` form fills it by looking up that path.

This is better than it looks. Without it, every node reference would be an
`Option<Gd<Node3D>>` that you unwrap on every single use, forever, despite it
never actually being `None` after `ready`. `OnReady` panics loudly if the node is
missing — at startup, with the path in the message — instead of letting the
problem leak into gameplay code.

**`#[export(range = (0.0005, 0.01, 0.0001))]`** — turns the Inspector field into
a slider with a min, max and step. Worth doing for anything you tune by feel:
dragging beats typing, and it stops you setting a sensitivity of 40 by accident.

**`event.try_cast::<InputEventMouseMotion>()`** — `unhandled_input` receives a
generic `Gd<InputEvent>`. `try_cast` returns `Result<Gd<Target>, Gd<Original>>`,
giving the original back on failure so nothing is lost. The `let ... else` idiom
turns "not the event I wanted" into an early return.

**`self.pitch.clamp(-limit, limit)`** — accumulate pitch in a field and clamp it,
rather than adding to `head.rotation.x` directly. Adding to the node lets it wrap
past vertical and flip the world upside down. Note the limit is 89°, not 90 —
looking exactly straight up makes the camera's basis degenerate and can produce a
sudden snap in yaw.

**`let basis = self.base().get_transform().basis;`** — the player's orientation.
Multiplying a local direction by it produces a world direction, so `move_dir`
("left-ish and forward-ish, relative to me") becomes a world-space vector.

**`Vector3::new(move_dir.x, 0.0, -move_dir.y)`** — `get_vector` returns 2D:
x = strafe, y = forward. Forward maps to **−Z**, hence the minus. Drop it and you
walk backwards.

**`is_action_just_pressed` vs `is_action_pressed`** — "was pressed this frame"
vs "is held". Jump wants the first, or holding space would re-trigger. Note that
in `physics_process`, `just_pressed` refers to this *physics tick*, which is
subtly not the same as this *frame* — a real source of dropped inputs at high
frame rates, and exactly what Lesson 6 exists to fix.

**Two separate `self.base_mut()` calls** at the end rather than one chained
expression: each takes the borrow, uses it, and drops it. Chaining
`.set_velocity(velocity).move_and_slide()` would not compile even if the return
types allowed it.

### Step 5 — Test it

Open `main.tscn`, add a `Lesson04Player` node — it is in the Create Node dialog —
and give it the same children as Step 2. (Or: open `player.tscn`, right-click the
root → **Change Type**, and pick `Lesson04Player`, then instance the scene into
`main.tscn` at position `(0, 0.1, 8)`.)

Press **F5**.

You can look around, walk, and jump. Movement is abrupt — full speed instantly,
dead stop instantly — and that is expected. It is also *correct*, in the sense
that it does the right thing on any frame rate and any machine.

Things to actually check:

- Walking into a wall slides along it rather than stopping dead. That is
  `move_and_slide`.
- Looking straight up stops at 89° and does not flip.
- Jumping while walking keeps your horizontal speed.
- The Output dock has no errors.

Press **F8** to stop.

---

## Check yourself

1. Why is movement in `physics_process` and looking in `process`/input?
2. Why is mouse look driven by events instead of by polling?
3. What does `move_and_slide` do that setting `position` directly would not?
4. Why is the camera under `Head/CameraRig/` instead of directly under `Player`?
5. What is `OnReady<Gd<Node3D>>` for, and what would you write without it?
6. Explain the error you get from
   `self.base_mut().rotate_y(-relative.x * self.mouse_sensitivity);`
   and the fix.
7. Why bind actions to *physical* keys?
8. Why is the pitch limit 89° and not 90°?

<details>
<summary>Answers</summary>

1. Physics runs on a fixed clock; moving a body outside it causes jitter and
   inconsistent collisions. Looking is visual, so limiting it to 60 Hz is
   visible as choppiness on a high-refresh monitor.
2. Mouse motion is delivered as a stream of events. There is no "how far did it
   move" state to poll, and events not read are lost.
3. It resolves collisions and slides the remaining motion along surfaces, so you
   do not catch on every doorframe. Setting `position` teleports and ignores
   collision entirely.
4. So that pitch, bob and recoil each get their own node and cannot contaminate
   each other.
5. A field that is empty until `ready` and holds a node afterwards, `Deref`ing to
   it. Without it you would write `Option<Gd<Node3D>>` and unwrap at every use.
6. `base_mut()` mutably borrows all of `self`, and the borrow lives until the end
   of the statement — so the argument cannot read `self.mouse_sensitivity`.
   Compute the value into a local first, then call `base_mut()`.
7. So W-A-S-D stays in the same physical position on AZERTY, Dvorak and every
   other layout.
8. At exactly 90° the camera's orientation becomes degenerate and yaw can snap.

</details>

---

## Extend it

- Add an `#[export] air_control: f32` and make mid-air steering weaker than
  ground steering. (Lesson 5 does this — try it yourself first.)
- Print `self.base().get_velocity().length()` every physics tick and watch it
  while you walk diagonally. Now remove `get_vector` and build the direction from
  four `is_action_pressed` calls instead. What is the diagonal speed, and why?
- Add a `toggle_mouse` action bound to Escape that switches between `CAPTURED`
  and `VISIBLE`. You will want this constantly from here on.
- Deliberately delete the `Head` node from the scene and run. Read the panic
  message. That is `OnReady` doing its job.

---

## Commit

```bash
git add -A
git commit -m "Lesson 4: first-person controller -- walk, jump, mouse look"
```

---

**Next:** [Lesson 5 — FPS controller II, feel](05-fps-controller-2.md)
