# Lesson 8 — Weapon feel

## What we're building

The gun stops being a raycast and starts being a weapon: a fire rate, camera
recoil that eases back, a viewmodel that punches backwards, a muzzle flash, and a
gunshot sound synthesised in Rust at startup.

Nothing here changes what the gun *does*. All of it changes whether firing it is
satisfying, which — for this genre — is close to the whole game.

---

## The concept

### Feel is made of layers

A satisfying shot is four or five cheap effects firing together. Individually
each is nearly unnoticeable; together they read as impact:

| Layer | What it does |
|---|---|
| Fire rate | Stops the gun being a continuous beam |
| Camera recoil | Your aim is disturbed — the shot had force |
| Viewmodel kick | The gun visibly moves in your hands |
| Muzzle flash | A single bright frame — the eye reads it as a bang |
| Sound | Removes the "is this broken?" reaction entirely |

Ship without sound and everyone describes the gun as broken, regardless of how
good the rest is. Silence is not neutral; it reads as failure.

### Fire rate as a cooldown

Rounds per minute converts to seconds per round:

```rust
self.shot_cooldown = 60.0 / self.rounds_per_minute;
```

and counts down every tick:

```rust
self.shot_cooldown = (self.shot_cooldown - delta as f32).max(0.0);
```

540 rpm is a fairly typical assault rifle: fast enough to feel automatic, slow
enough that individual shots are audible.

> **This drifts by up to one tick, and that is fine here.** A perfectly rigid
> cadence would subtract the cooldown rather than clamping it at zero, carrying
> the remainder into the next shot. It matters for a rhythm game; for a shooter
> it is inaudible, and clamping is what keeps the gun from firing a burst after
> a lag spike.

### Recoil that recovers, not recoil that teleports

Naive recoil rotates the camera up and leaves it there. Real weapons in games
kick the view and let it settle, and the *settling* is what makes it feel
mechanical rather than glitchy.

The design here separates recoil from aim entirely:

- The weapon **emits** a `recoil_kick` signal with an amount in degrees.
- The player **accumulates** it into a `recoil: Vector2`, in radians.
- The player applies that as `CameraRig`'s rotation, on top of `Head`'s pitch.
- Every frame, `recoil` eases back to zero with `smooth` from Lesson 5.

```rust
    fn recover_recoil(&mut self, delta: f64) {
        self.recoil.x = smooth(self.recoil.x, 0.0, self.recoil_recovery, delta);
        self.recoil.y = smooth(self.recoil.y, 0.0, self.recoil_recovery, delta);
    }
```

Because recoil lives on `CameraRig` and pitch lives on `Head`, recoil recovery
cannot undo your own aim adjustment. If they shared a node, correcting for recoil
by pulling down would then get *un*-corrected as the recoil recovered, and the
gun would feel like it was fighting you. Real shooters get this wrong sometimes;
it is very noticeable when they do.

Vertical kick is fixed, horizontal is random:

```rust
        let yaw = randf_range(-self.recoil_yaw as f64, self.recoil_yaw as f64) as f32;
```

Consistent vertical means a skilled player can learn to counter it. Random
horizontal means they cannot fully eliminate spray. That combination is the
standard design, and it is a design decision rather than a technical one.

### Viewmodel kick

Separate from camera recoil, and separately tuned: the gun model slides backwards
along its own Z and eases forward again.

```rust
    fn process(&mut self, delta: f64) {
        self.viewmodel_offset = smooth(self.viewmodel_offset, 0.0, self.viewmodel_recovery, delta);
        let rest = self.viewmodel_rest;
        let offset = self.viewmodel_offset;
        self.viewmodel
            .set_position(rest + Vector3::new(0.0, 0.0, offset));
    }
```

`viewmodel_rest` is captured in `ready` from wherever you positioned the node in
the editor, so the kick is relative to your layout rather than to the origin.
Same habit as `start_y` in Lesson 2, and same reason.

### Tweens, and `Callable`

The muzzle flash needs to be visible for about 35 milliseconds. You could add a
`Timer` node, or count down a field. A **tween** is lighter than either:

```rust
    fn flash(&mut self) {
        self.muzzle_flash.set_visible(true);
        // A tween is a small animation you build in code. This one just turns
        // the light off again a moment later without needing a Timer node.
        let callback = Callable::from_object_method(&self.to_gd(), "hide_flash");
        let mut tween = self.base_mut().create_tween();
        tween.tween_interval(0.035);
        tween.tween_callback(&callback);
    }
```

A tween is a small scheduled animation owned by the scene tree. It runs itself,
cleans itself up, and is free when idle.

The callback is a **`Callable`** — Godot's function pointer. Godot cannot hold a
Rust closure, so the target must be a method it can find by name, which means it
must be registered:

```rust
    /// Turns the muzzle flash off again. A `#[func]` because the tween calls it
    /// back through Godot, which can only see registered methods.
    #[func]
    fn hide_flash(&mut self) {
        self.muzzle_flash.set_visible(false);
    }
```

**This is the pattern for every deferred callback in gdext**: a `#[func]` method,
plus `Callable::from_object_method(&self.to_gd(), "name")`. The name is a string,
so a typo compiles fine and fails silently at runtime — one of the few places in
this project where Rust cannot protect you. When a tween callback never fires,
check the spelling first.

### Audio, synthesised

Rather than shipping a `.wav`, we generate one:

```rust
/// A short, dry crack. Noise burst for the transient, low sine for the body.
pub fn gunshot(seed: u64) -> Gd<AudioStreamWav> {
    let length = 0.20_f32;
    let count = (SAMPLE_RATE as f32 * length) as usize;
    let mut data = PackedByteArray::new();
    data.resize(count * 2);

    let mut rng = Noise::new(seed);

    for i in 0..count {
        let t = i as f32 / SAMPLE_RATE as f32;
        // Exponential decay: loud at t=0, effectively silent by the end.
        let crack = rng.next_bipolar() * (-t * 34.0).exp();
        let body = (std::f32::consts::TAU * 85.0 * t).sin() * (-t * 26.0).exp();
        let tail = rng.next_bipolar() * (-t * 9.0).exp() * 0.18;
        let value = (crack * 0.75 + body * 0.55 + tail).clamp(-1.0, 1.0);
        write_sample(&mut data, i, value, 32000.0);
    }

    wav(data)
}
```

Three components, which is roughly how a real gunshot is structured:

- **Crack** — white noise decaying very fast (`e^-34t`). The transient. This is
  the part your ear reads as "sharp".
- **Body** — an 85 Hz sine decaying fast. The thump you feel.
- **Tail** — quiet noise decaying slowly. A hint of room.

Two reasons this exists instead of a folder of audio files. First, legal hygiene:
"I'll just grab a placeholder gunshot for now" is exactly how someone else's asset
ends up committed to a public repo and forgotten. Second, it removes the "I need
to find audio before I can test feel" excuse, which is a real and common way for
a prototype to stall.

It sounds like a placeholder. That is fine. What matters is that pulling the
trigger makes a noise.

### Writing samples by hand

```rust
/// Little-endian signed 16-bit, which is what `Format::FORMAT_16_BITS` means.
fn write_sample(data: &mut PackedByteArray, index: usize, value: f32, scale: f32) {
    let sample = (value * scale) as i16;
    let bytes = sample.to_le_bytes();
    data[index * 2] = bytes[0];
    data[index * 2 + 1] = bytes[1];
}
```

GDScript has a helper for this (`encode_s16`); in Rust you write the two bytes
yourself, which is arguably clearer about what the format actually is. `32000` of
a possible `32767` leaves a little headroom so mixing several shots does not clip.

And a deterministic noise source, because we want the same gunshot on every
machine:

```rust
/// A tiny deterministic noise source.
///
/// Godot's `RandomNumberGenerator` would work, but a seeded generator we own
/// means the exact same gunshot every run on every machine -- which matters
/// once anything is compared against a recorded expectation in a test.
struct Noise(u64);
```

### A module with no class in it

`audio.rs` registers nothing with Godot. It is plain Rust functions that happen
to return an engine type:

```rust
fn wav(data: PackedByteArray) -> Gd<AudioStreamWav> {
    let mut stream = AudioStreamWav::new_gd();
    stream.set_format(Format::FORMAT_16_BITS);
    stream.set_mix_rate(SAMPLE_RATE as i32);
    stream.set_stereo(false);
    stream.set_data(&data);
    stream
}
```

Worth noticing, because the pull toward making everything a node is strong and
usually wrong. GDScript would need a class here (a `static func` on a
`class_name`) purely because it has no free functions. Rust does not, so it does
not.

---

## Do it

### Step 1 — The audio module

Create `rust/src/audio.rs` and add `pub mod audio;` to `lib.rs`. The whole file
is `reference/rust/src/audio.rs`; the essential parts are quoted above. It also
has `click()` and `impact()` for later use.

Build. Nothing changes yet.

### Step 2 — The tunables

Add to `Weapon`:

```rust
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
```

and the node handles it needs:

```rust
    #[init(node = "Muzzle")]
    muzzle: OnReady<Gd<Marker3D>>,
    #[init(node = "Muzzle/Flash")]
    muzzle_flash: OnReady<Gd<OmniLight3D>>,
    #[init(node = "Audio")]
    audio_player: OnReady<Gd<AudioStreamPlayer3D>>,
    #[init(node = "Viewmodel")]
    viewmodel: OnReady<Gd<Node3D>>,
```

### Step 3 — Set up in `ready`

```rust
    fn ready(&mut self) {
        self.in_magazine = self.magazine_size;
        self.viewmodel_rest = self.viewmodel.get_position();
        self.muzzle_flash.set_visible(false);
        self.audio_player.set_stream(&audio::gunshot(1337));

        let (mag, reserve) = (self.in_magazine, self.reserve_ammo);
        self.signals().ammo_changed().emit(mag, reserve);
    }
```

The last two lines belong to Lessons 9 and 10 — leave them out for now if you are
following strictly in order.

Notice `let (mag, reserve) = (self.in_magazine, self.reserve_ammo);` on its own
line. `self.signals()` borrows `self`, so reading fields inside the emit call
would be the Lesson 4 borrow error again. Copying them to locals first is the
same hoist, and you will write it dozens of times.

### Step 4 — The firing sequence

```rust
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
```

The `GameState` line arrives in Lesson 16 and the signals in Lesson 10 — comment
them out and uncomment as you reach them.

Order matters in one place: **`shoot_ray` before the recoil emit.** The shot goes
where you were aiming when you pulled the trigger, not where the recoil is about
to move you. Swap them and rapid fire climbs noticeably faster than the recoil
numbers suggest, which is a genuinely confusing bug to diagnose.

### Step 5 — Player-side recoil

In `Player::ready`, connect the signal:

```rust
        self.weapon
            .signals()
            .recoil_kick()
            .connect_other(&this, Player::on_recoil_kick);
```

and handle it:

```rust
    fn on_recoil_kick(&mut self, pitch_degrees: f32, yaw_degrees: f32) {
        self.recoil.y += pitch_degrees.to_radians();
        self.recoil.x += yaw_degrees.to_radians();
    }
```

Apply it to the rig, alongside the head's pitch:

```rust
    fn apply_camera_rotation(&mut self) {
        let mut head_rotation = self.head.get_rotation();
        head_rotation.x = self.pitch;
        self.head.set_rotation(head_rotation);

        let mut rig_rotation = self.camera_rig.get_rotation();
        rig_rotation.x = self.recoil.y;
        rig_rotation.y = self.recoil.x;
        self.camera_rig.set_rotation(rig_rotation);
    }
```

Note that recoil's `y` (the pitch kick) goes to the rig's `x` rotation. Rotation
about X *is* pitch. This mismatch between "the axis you rotate about" and "the
direction you look" catches people constantly; when a camera moves in the wrong
direction, this is usually why.

`Player` also gets an `#[export] recoil_recovery: f32` defaulting to `11.0`, and
`recover_recoil` called from `process`. Signals themselves are Lesson 10 — if you
are strictly in order, call `on_recoil_kick` directly from `tick` for now and
switch to the signal in two lessons.

### Step 6 — Tune it

Run, switch the Scene dock to **Remote**, and select the weapon.

- `rounds_per_minute` at `120` — a slow, deliberate rifle. At `900`, a buzzsaw.
- `recoil_pitch` at `3.0` — uncontrollable. At `0.1` — the gun feels like a
  pointer.
- `viewmodel_kick` at `0.3` — the gun visibly leaves the screen.
- `viewmodel_recovery` at `3.0` — it never gets back before the next shot.

Find numbers you like. The defaults here are a starting point, not an answer.

**Hold the trigger and watch where the crosshair ends up.** That drift is the
recoil pattern, and it is the thing players will actually learn to fight. If it
climbs straight up it is too predictable; if it is pure noise it is unlearnable.

---

## Check yourself

1. Why does the shot's raycast happen before the recoil is applied?
2. Why does recoil live on `CameraRig` while pitch lives on `Head`?
3. Why is vertical recoil fixed and horizontal recoil random?
4. What is a `Callable`, and why must a tween's target method be `#[func]`?
5. What compiles fine but fails silently in `Callable::from_object_method`?
6. Why does `ready` copy `self.in_magazine` into a local before emitting?
7. Why is `audio.rs` not a `GodotClass`?
8. Why is `viewmodel_rest` captured in `ready` instead of hardcoded?

<details>
<summary>Answers</summary>

1. So the bullet goes where you were aiming when you pulled the trigger, rather
   than where the recoil has just moved you.
2. So that recovering from recoil cannot undo the player's own aim correction.
   Sharing a node makes the gun feel like it is fighting you.
3. Fixed vertical can be learned and countered by a skilled player; random
   horizontal means spray cannot be fully eliminated.
4. Godot's function pointer. Godot cannot hold a Rust closure, so it looks the
   method up by name — which requires registration via `#[func]`.
5. The method-name string. A typo compiles and then the callback simply never
   fires.
6. `self.signals()` borrows `self`, so reading a field inside the emit call is a
   borrow error. Hoist the reads.
7. Nothing outside Rust calls it. It is plain functions that return an engine
   type, and Rust — unlike GDScript — allows free functions.
8. So the kick is relative to wherever you positioned the viewmodel in the
   editor, rather than to the origin.

</details>

---

## Extend it

- Add an aim-down-sights mode on `intent.aim_held`: ease the camera FOV down,
  move the viewmodel toward the centre, and halve the recoil. This is the single
  biggest feel upgrade available and it is about twenty lines.
- Make the muzzle flash light's energy random per shot within a range, and its
  duration scale with fire rate. Small, and it stops rapid fire strobing.
- Give the gunshot slight per-shot pitch variation by generating three streams
  with different seeds and picking one at random. Repetition is what makes
  synthesised audio sound cheap.
- Deliberately misspell the string in `Callable::from_object_method`. Confirm it
  compiles, run it, and see the muzzle flash stay on forever. Now you know that
  failure mode from the inside.

---

## Commit

```bash
git add -A
git commit -m "Lesson 8: fire rate, recoil, viewmodel kick, muzzle flash, audio"
```

---

**Next:** [Lesson 9 — Ammo and reload](09-ammo-and-reload.md)
