# Appendix C — Debugging playbook

Symptom → cause, for the mistakes that account for most of the time beginners
lose. Roughly half the problems you hit while working through this tutorial are
in here.

Two sections: gdext-specific problems, which have no GDScript equivalent, and
Godot 3D problems, which everyone hits regardless of language.

---

## Part 1 — gdext problems

### "Cannot get class 'X'" / nodes load as placeholders

```
ERROR: Cannot get class 'Player'.
WARNING: Node Player of type Player cannot be created. A placeholder will be
         created instead.
```

Godot does not know your class exists. In order of likelihood:

1. **The project has not been scanned since you added `.gdextension`.** Run
   `godot4 --headless --path godot --import`, or just open the editor.
2. **The library was not built.** Check `rust/target/debug/` for `libshooter.so`
   / `shooter.dll` / `libshooter.dylib`.
3. **`crate-type` is not `["cdylib"]`.** `cargo build` succeeds and produces an
   `.rlib` Godot cannot load.
4. **The path in `[libraries]` is wrong.** It is relative to the Godot project:
   `res://../rust/target/debug/...`.
5. **`entry_symbol` is misspelled.** It must be exactly `gdext_rust_init`.
6. **You built release but are running debug**, or the reverse. The
   `.gdextension` has separate entries.

### `error[E0503]: cannot use 'self.x' because it was mutably borrowed`

```rust
self.base_mut().rotate_y(self.speed * delta);        // no
```

`base_mut()` borrows all of `self` for the whole statement. Hoist the read:

```rust
let step = self.speed * delta;
self.base_mut().rotate_y(step);                      // yes
```

The general rule: **compute everything, then touch the base.** This is also true
of `self.signals()`.

### "signals() allows only one signal configuration at a time"

```rust
let s = bus.signals();
s.a().connect_other(..);
s.b().connect_other(..);        // panics here
```

Call `signals()` fresh for each signal:

```rust
bus.signals().a().connect_other(..);
bus.signals().b().connect_other(..);
```

Compiles either way; only the second runs.

### "Attempt to call bind_mut() on a Gd<X> that is already borrowed"

A double borrow, at runtime. Two causes:

**A borrow held across a call that reaches back:**

```rust
self.weapon.bind_mut().tick(&self.input_source.bind().intent, delta);   // risky
```

```rust
let intent = self.input_source.bind().intent;    // copy out, borrow ends
self.weapon.bind_mut().tick(&intent, delta);     // safe
```

**A borrow that outlives its usefulness:**

```rust
let mut h = self.health.bind_mut();
h.reset();
self.restore_colour();      // panics: `h` is still alive
```

<!-- illustrative -->
```rust
{
    let mut h = self.health.bind_mut();
    h.reset();
}                           // released here
self.restore_colour();
```

### "no `X` in `classes`" for a class that plainly exists

Godot marks it experimental and gdext hides it. Enable the feature:

```toml
godot = { version = "0.5.5", features = ["api-4-7", "experimental-godot-api"] }
```

Affects all of navigation (`NavigationAgent3D`, `NavigationRegion3D`,
`NavigationServer3D`, `NavigationMesh`), `GraphEdit`, `Parallax2D`, several XR
classes.

### "no `MouseMode` in `global`" / cannot find an engine enum

Enums belonging to a class live in a module named after that class:

```rust
use godot::classes::input::MouseMode;          // not godot::global
use godot::classes::node::ProcessMode;
use godot::classes::tween::{EaseType, TransitionType};
```

### "Method `X::get_y` shadows a method of a base class"

`#[export] gravity` generates `get_gravity`, and `CharacterBody3D` already has
one. A warning today, an error in gdext 0.6. Name the accessors explicitly:

```rust
#[export]
#[var(get = get_gravity_strength, set = set_gravity_strength)]
gravity: f32,
```

plus matching `#[func]` methods. The Inspector property keeps the name `gravity`.

### "no associated function named `get_x`"

You wrote `#[var(get, set = set_x)]`. A bare `get` means "use my hand-written
`get_x`". Omit it and gdext generates one:

```rust
#[var(set = set_x)]
```

### "use of deprecated method `X::get_y`"

gdext is phasing out auto-generated accessors. Inside Rust, read the field
directly (`weapon.bind().magazine_size`). Write explicit `#[func]` accessors only
where Godot or another class needs to call them.

### A tween callback never fires

`Callable::from_object_method(&self.to_gd(), "hide_flash")` looks the method up
by **string**. Check:

- the spelling matches exactly
- the method is `#[func]`
- the method is in a `#[godot_api] impl` block

All three are silent failures.

### `GString: From<String> is not satisfied`

`GString` implements `From<&str>`, not `From<String>`:

```rust
format!("{x}").as_str().into()      // not format!("{x}").into()
```

### `expected 'ByValue', found 'ByOption'` when emitting a signal

A nullable object argument is passed as `Option<&Gd<T>>`:

```rust
self.signals().died().emit(source.as_ref());
self.signals().died().emit(None::<Gd<MyClass>>.as_ref());   // explicit none
```

### `find_children` returns nothing

Default `owned = true` only finds nodes owned by the searching scene. Nodes
inside an instanced sub-scene are owned by *that* scene:

```rust
root.find_children_ex("*").type_("Marker3D").owned(false).done()
```

### Behaviour does not match the code

The editor is running a stale library.

```bash
# close the editor
rm -rf godot/.godot
cargo build --manifest-path rust/Cargo.toml
godot4 --headless --path godot --import
```

Also: did the build actually succeed? Look at the terminal, not the editor.

---

## Part 2 — Godot 3D problems

### The gun does not hit anything

**Almost always a collision mask.** In order:

1. Does the ray's mask include the target's layer? Print `hit_mask` — it should be
   13 (`World | Enemy | EnemyHitbox`).
2. Is the target's layer set at all?
3. Is `collide_with_areas` true? Defaults to **false**, and hitboxes are `Area3D`s.
4. Is the shooter excluded? A ray starting inside your own capsule hits you first.
5. Is the ray long enough? `range_metres`.

There is no error for any of these. The dictionary comes back empty and nothing
happens.

### Body shots register but headshots do not

`query.set_collide_with_areas(true)`. This is its own entry because it is so
common and so silent.

### I walk through a wall

The wall has a `MeshInstance3D` and no `CollisionShape3D` — or the shape is
present but has no shape *resource* assigned, which looks identical in the tree.

### There is an invisible wall

Either a `CollisionShape3D` with no mesh, or a hidden `Zone` whose collision was
not disabled along with its visibility. `visible = false` does not affect physics.

### The character sinks through the floor

`is_on_floor()` is only meaningful after `move_and_slide()`. Applying gravity but
never calling it means nothing ever registers as grounded.

Also check the floor has collision, and that the character's mask includes the
floor's layer.

### The character catches on corners

Not using `move_and_slide()`, or a box collision shape instead of a capsule. A
capsule slides past corners; a box catches.

### Movement speed differs between machines

A missing `delta`. Any per-second rate must be multiplied by it.

### Something eases four times faster at 240 fps

`lerp(current, target, 0.1)` every frame moves 10% *per frame*. Use:

```rust
pub fn smooth(current: f32, target: f32, response: f32, delta: f64) -> f32 {
    let t = 1.0 - (-response * delta as f32).exp();
    current + (target - current) * t
}
```

### The camera flips upside down when I look up

Pitch is not clamped, or it is being applied to the body instead of the head.
Clamp to ±89°, not ±90°.

### Enemies do not move

1. Is the navmesh baked? Turn on **Debug → Visible Navigation**.
2. Is the enemy standing *on* the mesh? It shrinks back `agent_radius` from walls.
3. Was `target_position` ever set?
4. Are you querying before the map has synced? Give it a few physics frames.
5. Is the enemy `Dormant`? Pooled enemies start deactivated.

### Enemies walk into walls

You are steering toward the destination instead of `get_next_path_position()`.

### Enemies cannot reach a new area

The navmesh was not re-baked after the geometry changed. Also check the door
actually cleared its collision layer.

### Navigation shows up as a CPU spike

`set_target_position` is being called every frame. Only repath when the target has
actually moved.

### Everything flashes when I hit one thing

A shared material resource. `duplicate_resource()` it per instance.

### Resizing one wall resizes all of them

A shared mesh resource saved in the scene. Build it in code with
`BoxMesh::new_gd()`.

### A pooled object works once

Something is initialised in `ready` instead of `activate`. Every mutable field
must be reset on activation — health, rotation, colour, collision, process mode,
velocity.

### The frame hitches when a round starts

Something is being instantiated mid-round. Pool it.

### The HUD is tiny on a 4K monitor

Stretch settings. **Display → Window → Stretch**: mode `canvas_items`, aspect
`expand`, viewport 1920×1080.

### Clicking does not fire the gun

A `Control` is swallowing the event. Set **Mouse Filter → Ignore** on every
non-interactive UI node.

### The HUD shows zeros until something changes

Connecting to a signal tells you about future changes only. Call the handler once
after connecting, or emit an initial value in `ready`.

### Changing a value in the Inspector does nothing

The scene file stores an override, and it wins over the code default. Click the
**revert arrow** next to the property.

### Tuning I did while the game was running disappeared

Those were Remote changes, which are never saved. Write them down before pressing
F8.

---

## Tools

### Print, deliberately

```rust
godot_print!("state={:?} alive={}", self.state, self.alive);
```

Deriving `Debug` on your enums makes this useful rather than a guessing game.

`godot_warn!` and `godot_error!` go to the Debugger dock with a stack trace, which
is worth the extra characters for anything that should not happen.

### Visible collision and navigation

**Debug → Visible Collision Shapes** and **Debug → Visible Navigation**, then run.
Both turn "why is this not working" into a look. Use them before adding prints.

### The Remote scene tree

While running, switch the Scene dock to **Remote**. Inspect live state: pool
counts, `GameState` points, whether a zone is hidden, what a node's transform
actually is.

### The profiler

**Debug → Profiler**, then **Start**, then play. Frame-time spikes line up with
whatever caused them. Measure before optimising — and measure a `--release`
build, because debug gdext is several times slower.

### The test suite

```bash
godot4 --headless --path reference/godot res://tests/tests.tscn
```

When your build misbehaves, run this against the *reference* to confirm the
reference still works, then diff your file against
`reference/rust/src/<the same file>`. That is what the reference build is for.

---

## How to ask a good question

1. **Which lesson and step.**
2. **The exact error, in full.** For Rust that means the whole `cargo build`
   output including the `note:` and `help:` lines — the answer is usually in
   those.
3. **What you expected versus what happened.**
4. **The code you actually wrote**, not what the tutorial says it should be.
   Those differ, and the difference is the bug.

Good: *"Lesson 11, step 6. Enemy spawns but does not move. No errors. Visible
Navigation shows the mesh but no path. Here is my `tick_chase`: [code]"*

Hard to help: *"the enemy doesn't work"*
