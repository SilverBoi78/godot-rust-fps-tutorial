# Lesson 5 — FPS controller II: feel

## What we're building

The same controller, but it feels like a game instead of a tech demo:
acceleration and friction, weaker control in the air, a subtle head bob while
walking, and a field-of-view widening when you sprint.

None of this changes what the controller *can do*. All of it changes whether
anyone wants to use it.

---

## The concept

### "Feel" is a real engineering problem

It is tempting to treat feel as decoration you add at the end. It is not. The
difference between a controller people describe as "tight" and one they describe
as "floaty" is four or five numbers, and finding them takes iteration — which
means every one of them needs to be `#[export]`ed and tunable while the game
runs.

The pieces this lesson adds, and what each one is actually for:

| Piece | What it fixes |
|---|---|
| Acceleration / friction | Instant velocity changes read as weightless |
| Air control | Full mid-air steering makes jumping feel like flying |
| Head bob | A perfectly steady camera reads as a floating eyeball |
| Sprint FOV | Speed is nearly invisible in first person without it |

### `move_toward`, not `lerp`

To ease a value toward a target you reach for one of two functions, and they are
not interchangeable.

`lerp(current, target, t)` moves a *fraction* of the remaining distance. It gets
closer forever and never arrives. For velocity that means you never quite reach
top speed and never quite stop — you drift at 0.001 m/s indefinitely, and
`is_on_floor()`-adjacent checks start behaving strangely.

`move_toward(current, target, delta)` moves a *fixed amount* and clamps at the
target. It arrives, exactly, and stays.

```rust
pub fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}
```

For **rates** — acceleration, friction — you want `move_toward`, because
"65 m/s² of acceleration" is a fixed amount per second.

### Frame-rate independent smoothing

For things that genuinely should ease — FOV, bob amplitude — `lerp` is right, but
the naive form is a bug:

```rust
// Wrong: moves 10% per FRAME, so it converges 4x faster at 240fps than at 60.
current = current + (target - current) * 0.1;
```

The fix is an exponential:

```rust
pub fn smooth(current: f32, target: f32, response: f32, delta: f64) -> f32 {
    let t = 1.0 - (-response * delta as f32).exp();
    current + (target - current) * t
}
```

`response` is roughly "how many e-foldings per second" — bigger is snappier. 8 to
12 feels responsive; 2 feels sluggish; 30 is nearly instant.

Why it works: after time `t`, the fraction of the gap remaining is `e^(-response·t)`,
which depends only on elapsed time. Sixty steps or two hundred and forty, same
result. This is worth understanding rather than copying, because you will need it
for camera follow, recoil recovery, UI animation, and about six other things.

**Any `lerp` with a constant weight, called every frame, is this bug.** It is
everywhere in tutorials, including good ones.

### Head bob, done in a way that does not feel bad

Bob is a sine wave applied to the camera's height. Three details separate
tolerable bob from nauseating bob:

1. **Drive the phase by distance travelled, not by time.** `bob_time += delta *
   speed * frequency` means the bob is tied to your footfalls. Driving it by time
   alone makes it keep bobbing while you stand still.
2. **Ease the amplitude, not the position.** If you snap the amplitude to zero
   when you stop, the camera jumps to wherever the sine happened to be. Easing
   the amplitude toward zero lets the wave die out smoothly.
3. **Keep it small.** 0.045m — four and a half centimetres. Anything you can
   clearly notice is too much. Bob works when players feel it and cannot point at
   it.

A horizontal component at half the vertical frequency gives a figure-eight, which
reads as a gait rather than a bounce.

### Sprint FOV

Widening the field of view while sprinting is the standard trick for conveying
speed in first person, because the periphery moves faster and your brain reads
that as acceleration.

Two rules: ease it (a snap is jarring), and keep it modest. 78° base, +9° while
sprinting. Larger values distort the view and, for some people, cause motion
sickness.

Gate it on *actual* speed, not on the sprint key being held. Otherwise holding
shift while standing still zooms out for no reason.

---

## Do it

### Step 1 — Add the camera rig, if you have not

Confirm `player.tscn` has `Head → CameraRig → Camera3D`. Bob moves `CameraRig`,
so the pitch on `Head` and the bob on `CameraRig` stay independent.

### Step 2 — Rewrite the controller

Replace the contents of `rust/src/player.rs`. This is the Lesson 5 stage, mirrored
exactly at `reference/rust/src/stages/lesson05_player.rs`.

```rust
use godot::classes::input::MouseMode;
use godot::classes::{
    Camera3D, CharacterBody3D, ICharacterBody3D, Input, InputEvent, InputEventMouseMotion, Node3D,
};
use godot::prelude::*;

use crate::player::move_toward;
use crate::weapon::smooth;

#[derive(GodotClass)]
#[class(base=CharacterBody3D, init)]
pub struct Lesson05Player {
    #[export]
    #[init(val = 5.2)]
    walk_speed: f32,
    #[export]
    #[init(val = 8.0)]
    sprint_speed: f32,
    #[export]
    #[init(val = 26.0)]
    fall_acceleration: f32,
    #[export]
    #[init(val = 7.2)]
    jump_velocity: f32,
    #[export]
    #[init(val = 65.0)]
    acceleration: f32,
    #[export]
    #[init(val = 75.0)]
    friction: f32,
    #[export(range = (0.0, 1.0))]
    #[init(val = 0.3)]
    air_control: f32,

    #[export(range = (0.0005, 0.01, 0.0001))]
    #[init(val = 0.0022)]
    mouse_sensitivity: f32,
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

    #[init(node = "Head")]
    head: OnReady<Gd<Node3D>>,
    #[init(node = "Head/CameraRig")]
    camera_rig: OnReady<Gd<Node3D>>,
    #[init(node = "Head/CameraRig/Camera3D")]
    camera: OnReady<Gd<Camera3D>>,

    pitch: f32,
    bob_time: f32,
    bob_amount: f32,

    base: Base<CharacterBody3D>,
}
```

That is the data. Now the behaviour, in the same file:

```rust
#[godot_api]
impl ICharacterBody3D for Lesson05Player {
    fn ready(&mut self) {
        Input::singleton().set_mouse_mode(MouseMode::CAPTURED);
        let fov = self.base_fov;
        self.camera.set_fov(fov);
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

    fn process(&mut self, delta: f64) {
        let velocity = self.base().get_velocity();
        let planar_speed = Vector2::new(velocity.x, velocity.z).length();
        let moving = self.base().is_on_floor() && planar_speed > 0.6;

        let target = if moving { self.bob_amplitude } else { 0.0 };
        self.bob_amount = smooth(self.bob_amount, target, 9.0, delta);
        self.bob_time += delta as f32 * planar_speed * self.bob_frequency;

        let mut position = self.camera_rig.get_position();
        position.y = self.bob_time.sin() * self.bob_amount;
        position.x = (self.bob_time * 0.5).cos() * self.bob_amount * 0.6;
        self.camera_rig.set_position(position);

        let sprinting = Input::singleton().is_action_pressed("sprint") && planar_speed > 1.5;
        let fov_target = self.base_fov
            + if sprinting {
                self.sprint_fov_bonus
            } else {
                0.0
            };
        let fov = smooth(self.camera.get_fov(), fov_target, self.fov_response, delta);
        self.camera.set_fov(fov);
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
        let mut wish = basis * Vector3::new(move_dir.x, 0.0, -move_dir.y);
        wish.y = 0.0;

        let speed = if input.is_action_pressed("sprint") {
            self.sprint_speed
        } else {
            self.walk_speed
        };
        let control = if self.base().is_on_floor() {
            1.0
        } else {
            self.air_control
        };

        if wish.length_squared() > 0.001 {
            let target = wish.normalized() * speed;
            let step = self.acceleration * control * delta as f32;
            velocity.x = move_toward(velocity.x, target.x, step);
            velocity.z = move_toward(velocity.z, target.z, step);
        } else {
            let step = self.friction * control * delta as f32;
            velocity.x = move_toward(velocity.x, 0.0, step);
            velocity.z = move_toward(velocity.z, 0.0, step);
        }

        self.base_mut().set_velocity(velocity);
        self.base_mut().move_and_slide();
```

`move_toward` and `smooth` are two small free functions. Put `move_toward` at the
bottom of `player.rs`:

```rust
/// Godot's `move_toward`: step from `from` to `to` by at most `delta`.
pub fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}
```

and `smooth` wherever you like — in the reference it lives at the bottom of
`weapon.rs`, because that is the file that needed it second and moving it would
have meant a third module for two functions:

```rust
/// Frame-rate independent exponential smoothing.
///
/// The naive `lerp(current, target, 0.1)` is a bug: it moves 10% per FRAME, so
/// it converges more than four times faster at 240 fps than at 60. This form
/// converges at the same real-world rate regardless.
pub fn smooth(current: f32, target: f32, response: f32, delta: f64) -> f32 {
    let t = 1.0 - (-response * delta as f32).exp();
    current + (target - current) * t
}
```

> **Why free functions and not methods on a trait?** Because they are pure
> arithmetic with no relationship to any type. A `MathExt` trait would be
> ceremony for two functions. Rust makes it easy to reach for abstraction earlier
> than you should; a `pub fn` in a module is a perfectly good unit of code.

### Step 3 — Read the new pieces

**`wish.y = 0.0`** — the basis multiply can introduce a small Y component when
you are looking up or down. Left in, walking forward while looking at the sky
would push you into the ground.

**`wish.length_squared() > 0.001`** — "is there input". `length_squared` avoids a
square root; comparing against a small epsilon rather than `> 0.0` avoids
floating-point noise re-triggering acceleration when you have released the keys.

**Separate `acceleration` and `friction`, and `friction > acceleration`** — 75 vs
65. Stopping faster than you start is what makes a controller feel responsive
rather than slippery. This asymmetry is present in almost every shooter that
feels good and almost none that feel bad.

**`control`** multiplies *both* acceleration and friction, so at `air_control =
0.3` you steer at 30% authority and also decelerate at 30% — momentum carries
through a jump, which is what makes jumping feel committed.

**`planar_speed > 0.6` and `> 1.5`** — bob starts almost as soon as you move; the
FOV kick waits until you are genuinely moving fast. Different thresholds because
they answer different questions.

**Reading `self.camera.get_fov()` as the smoothing input** — the camera is the
source of truth for its own FOV, so there is no separate field to drift out of
sync with it. Worth preferring wherever the engine already stores the state.

### Step 4 — Tune it

Build, run, and switch the Scene dock to **Remote**. Select the player and drag
things while you walk:

- Set `acceleration` to `10`. Feel the sludge.
- Set it to `400`. Feel it become Lesson 4 again.
- Set `bob_amplitude` to `0.2`. Notice how quickly bob goes from "alive" to
  "unpleasant".
- Set `air_control` to `1.0`, jump, and steer. Notice it feels like flying.
- Set `fov_response` to `1.0` and sprint. Notice the lag.

**Do this.** Reading the numbers teaches you nothing; feeling the boundaries
teaches you what each one controls. Then put them back, or keep whatever you
prefer — these are your numbers now.

---

## Check yourself

1. Why `move_toward` for velocity instead of `lerp`?
2. Why is `current + (target - current) * 0.1` every frame a bug, and what
   replaces it?
3. Why is bob's phase driven by `delta * speed` rather than by `delta`?
4. Why ease the bob *amplitude* rather than the bob *position*?
5. Why does `control` multiply friction as well as acceleration?
6. Why is `friction` larger than `acceleration`?
7. Why gate the FOV kick on actual speed rather than on the sprint key?

<details>
<summary>Answers</summary>

1. `lerp` moves a fraction of the remaining distance and never arrives, so you
   never quite stop. `move_toward` moves a fixed amount and clamps at the target.
2. It moves 10% per *frame*, so it converges much faster at high frame rates.
   Replace with `1.0 - (-response * delta).exp()` as the weight.
3. So the bob is tied to distance travelled — it stops when you stop, and speeds
   up when you sprint.
4. Snapping the amplitude to zero jumps the camera to wherever the sine happened
   to be. Easing the amplitude lets the wave die out in place.
5. So momentum carries through a jump. Full air friction would let you stop dead
   in mid-air.
6. Stopping faster than you start is what makes a controller feel responsive
   instead of slippery.
7. Otherwise holding sprint while stationary zooms out for no reason.

</details>

---

## Extend it

- Add landing impact: when `is_on_floor()` becomes true after being false, punch
  `camera_rig.position.y` down and let `smooth` bring it back. Scale it by the
  fall speed. This is about eight lines and adds more perceived weight than
  anything else in the lesson.
- Add crouch: an action that lowers `Head`, shrinks the capsule, and reduces
  `walk_speed`. The interesting part is refusing to stand up when something is
  above you — which needs Lesson 7's raycasting, so note it and come back.
- Make bob amplitude scale with speed so sprinting bobs harder. Decide whether
  you like it; plenty of shooters deliberately do not.
- Set the physics tick rate to 30 in Project Settings (**Physics → Common →
  Physics Ticks per Second**) and play. What breaks? What does not? This tells
  you which of your code is genuinely frame-rate independent.

---

## Commit

```bash
git add -A
git commit -m "Lesson 5: acceleration, air control, head bob, sprint FOV"
```

---

**Next:** [Lesson 6 — Input to intent](06-input-to-intent.md)
