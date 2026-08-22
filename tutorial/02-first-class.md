# Lesson 2 — Your first Rust class

## What we're building

A cube in the middle of the arena that spins and bobs gently up and down, with
its speed adjustable from the Inspector *while the game is running*.

It's a toy. But it's the vehicle for the fundamentals every later lesson
assumes: how a Rust struct becomes a live node, when your code runs, what
`delta` is and why nearly every beginner movement bug traces back to it, and how
`#[export]` turns code into something you can tune by feel instead of by editing
numbers and recompiling.

---

## The concept

### The anatomy of a gdext class

Here is the smallest useful one, with every part labelled:

```rust
use godot::prelude::*;
use godot::classes::{INode3D, Node3D};

#[derive(GodotClass)]                    // 1. register this type with Godot
#[class(base=Node3D, init)]              // 2. what it extends, how it's built
pub struct Spinner {
    #[export]                            // 3. show this in the Inspector
    #[init(val = 90.0)]                  // 4. its default value
    degrees_per_second: f32,

    base: Base<Node3D>,                  // 5. the engine object we extend
}

#[godot_api]                             // 6. expose the impl block to Godot
impl INode3D for Spinner {               // 7. the base class's virtual methods
    fn process(&mut self, delta: f64) {
        let step = self.degrees_per_second.to_radians() * delta as f32;
        self.base_mut().rotate_y(step);  // 8. reach the engine through base
    }
}
```

1. **`#[derive(GodotClass)]`** — generates the registration machinery. Without
   it this is an ordinary Rust struct that Godot has never heard of.
2. **`#[class(base=Node3D, init)]`** — `base` is what GDScript spells `extends`.
   `init` asks the macro to write a constructor that fills every field with its
   `#[init(val = ...)]` default, or `Default::default()` where there isn't one.
   Without `init` you must write `fn init(base: Base<Node3D>) -> Self` yourself.
3. **`#[export]`** — publishes the field to the Inspector so you can tune it.
4. **`#[init(val = ...)]`** — the default. Note it is *not* Rust's `Default`
   trait; it's a gdext attribute, and it accepts any expression.
5. **`base: Base<Node3D>`** — required, exactly one per class, and the type must
   match the `base =` in the attribute.
6. **`#[godot_api]`** — every impl block containing anything Godot needs to see
   (virtual methods, `#[func]`, `#[signal]`) must carry this.
7. **`impl INode3D`** — gdext names the virtual-method trait after the class with
   an `I` prefix: `INode`, `INode3D`, `ICharacterBody3D`, `IControl`. You
   implement only the methods you care about; the rest have defaults.
8. **`self.base_mut()`** — the engine's own methods live here.

### Godot calls your functions — you don't call them

This is the biggest mental shift coming from ordinary programming. There is no
`main()`. You do not write a loop that runs the game.

Instead you implement methods from the `I…` trait and **the engine calls them for
you** at the right moments:

| Method | When Godot calls it |
|---|---|
| `ready(&mut self)` | Once, when the node and all its children have entered the scene tree |
| `process(&mut self, delta: f64)` | Once per **rendered frame** — as fast as the machine draws |
| `physics_process(&mut self, delta: f64)` | Once per **physics tick** — a fixed 60 times a second |
| `input(&mut self, event: Gd<InputEvent>)` | When an input event happens |
| `exit_tree(&mut self)` | When the node is removed from the tree |

You will use `ready` and `physics_process` most. Lesson 4 covers the
`process` vs `physics_process` distinction properly — for now, know that
`process` runs at your frame rate (variable, could be 45 fps or 300 fps) and
`physics_process` runs on a fixed clock regardless.

> **Why does `ready` exist instead of doing the work in `init`?** Because when a
> node is first constructed, its children may not exist yet, and it isn't in the
> scene tree, so it has no parent, no world, and no access to anything around it.
> `ready` fires at the first moment all of that is guaranteed. Trying to reach
> other nodes from `init` is a classic source of crashes — and gdext makes it
> hard on purpose: `init` only receives the `Base<T>`, nothing else.

### `delta` — the single most important parameter you will meet

`delta` is **the number of seconds since the previous frame**. At a steady 60 fps
it's about `0.0167`. At 240 fps it's about `0.0042`. It is never exactly the same
twice.

Consider rotating something. The naive version:

```rust
fn process(&mut self, _delta: f64) {
    self.base_mut().rotate_y(0.05);      // WRONG
}
```

This rotates 0.05 radians *per frame*. On a 60 fps machine that's 3 rad/s. On
your 165 Hz laptop screen it's 8.25 rad/s. The game literally runs at different
speeds on different computers — and worse, it speeds up and slows down on the
*same* computer whenever the frame rate dips.

The correct version:

```rust
fn process(&mut self, delta: f64) {
    self.base_mut().rotate_y(1.5 * delta as f32);   // 1.5 radians per SECOND
}
```

Now `1.5` is a rate *per second*, and multiplying by elapsed time converts it
into "amount for this particular frame." Sixty small steps or 240 tiny ones —
same total per second.

**The rule: any quantity expressed "per second" gets multiplied by `delta`.**
Movement speed, rotation speed, reload progress, damage-over-time. If you ever
see something in your game running faster on a better machine, this is why,
essentially every time.

*(Two exceptions you'll meet later: `physics_process` has an effectively fixed
delta, so mistakes there are less visible — but still multiply, because the tick
rate is configurable. And instantaneous events like "fire a bullet" aren't
rates, so they don't get `delta`.)*

### `f64` in, `f32` everywhere else

You will have noticed `delta: f64` and `degrees_per_second: f32`, and the
`as f32` cast between them. That is not an accident and it is not avoidable.

Godot's *scripting* API passes time as a 64-bit float, but its *math* types —
`Vector3`, `Transform3D`, `Color` — are built from 32-bit floats in a standard
build. So `delta` arrives as `f64`, and the moment it touches anything spatial it
has to become `f32`.

You have two options, and this tutorial takes the second:

- Store your tunables as `f64` and cast at the end.
- Store them as `f32` and cast `delta` at the point of use.

The second means one visible `as f32` per method and no surprises inside your
arithmetic. It is also the shape most gdext code in the wild takes.

### `#[export]` vs `#[var]` vs neither

Three levels of visibility, and picking the right one is a real decision:

| Attribute | Visible in Inspector | Visible to GDScript / scenes | Use for |
|---|---|---|---|
| `#[export]` | yes | yes | anything you want to tune by feel |
| `#[var]` | no | yes | state other code sets, but nobody hand-tunes |
| *(nothing)* | no | no | internal bookkeeping |

Default to *nothing*. A field is only worth exposing if someone outside Rust
genuinely needs it, and every exposed field is one more thing a scene file can
override behind your back.

> **A rough edge worth knowing now.** Once a node has an Inspector-set value, the
> scene file stores that override and it *wins* over your `#[init(val = ...)]`.
> Change the default in code and that node does not move. If a value refuses to
> change no matter what you type, look for the **revert arrow** next to it in the
> Inspector and click it to drop the override.

### Naming, and what Godot sees

gdext converts between Rust and Godot conventions automatically:

- A struct `RoundDirector` becomes a Godot class `RoundDirector`.
- A field `degrees_per_second` becomes a Godot property `degrees_per_second`.
- A method `#[func] fn apply_damage` becomes `apply_damage`.

So write ordinary Rust — `snake_case` fields and functions, `PascalCase` types —
and the Godot side comes out idiomatic on its own.

One thing gdext will *not* fix for you: **a generated accessor that collides with
one on the base class.** `#[export] gravity` generates `get_gravity`, and
`CharacterBody3D` already has a `get_gravity()`. gdext warns about this today and
will reject it in a future version. Lesson 4 hits it for real and shows the fix.

---

## Do it

### Step 1 — Write the class

Create `rust/src/spinner.rs`. Type this out — all of it, by hand.

```rust
use godot::classes::{INode3D, Node3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Spinner {
    // `#[export]` makes a field appear in the Inspector dock. You can change it
    // without editing code -- and, crucially, WHILE the game is running.
    #[export]
    #[init(val = 90.0)]
    degrees_per_second: f32,
    #[export]
    #[init(val = 0.35)]
    bob_height: f32,
    #[export]
    #[init(val = 2.0)]
    bob_speed: f32,

    // No `#[export]`: this is internal bookkeeping, so it stays out of the
    // Inspector. We accumulate elapsed time here because `sin` needs an
    // ever-growing input.
    elapsed: f32,

    // Set once in `ready` and never changed, so the bob is measured from
    // wherever you placed this node in the editor rather than from the origin.
    start_y: f32,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Spinner {
    /// Runs once, after this node and all its children have entered the scene
    /// tree. Think of it as "the node is now fully alive and safe to touch."
    fn ready(&mut self) {
        self.start_y = self.base().get_position().y;
        godot_print!(
            "Spinner ready at y={:.2}, spinning at {:.0} deg/s.",
            self.start_y,
            self.degrees_per_second
        );
    }

    /// Runs once per rendered frame. `delta` is the number of SECONDS since the
    /// previous frame -- about 0.0167 at 60 fps, 0.0042 at 240 fps.
    fn process(&mut self, delta: f64) {
        self.elapsed += delta as f32;

        // Multiplying by delta is what makes this frame-rate independent: at
        // 60 fps we rotate a little 60 times a second, at 240 fps we rotate a
        // quarter as much 240 times a second. Same real-world speed either way.
        let step = self.degrees_per_second.to_radians() * delta as f32;
        self.base_mut().rotate_y(step);

        let y = self.bob_offset();
        let mut position = self.base().get_position();
        position.y = y;
        self.base_mut().set_position(position);
    }
}

impl Spinner {
    /// Splitting this out isn't necessary here -- it's a habit worth building,
    /// because `process` gets crowded fast once a node does more than one thing.
    fn bob_offset(&self) -> f32 {
        // `sin` cycles between -1 and 1. Scaling by bob_height turns that into
        // a gentle rise and fall around the node's starting height.
        self.start_y + (self.elapsed * self.bob_speed).sin() * self.bob_height
    }
}
```

Now register the module. In `rust/src/lib.rs`, above the `ShooterExtension`
struct, add:

```rust
pub mod spinner;
```

And delete the `Hello` struct and its `impl` from Lesson 0 — it has done its job.

### Step 2 — Read it back

**`use godot::prelude::*;`** — brings in `Gd`, `Base`, `Vector3`, `godot_print!`,
the derive macros, and the rest of the everyday vocabulary. Almost every file you
write will start with it.

**`use godot::classes::{INode3D, Node3D};`** — the prelude re-exports about a
dozen of the most common classes (`Node`, `Node3D`, `Object`, `Resource`,
`PackedScene`, `SceneTree` and their `I…` traits), so this particular line is
redundant. It is written out anyway because the *other* thousand classes are not
in the prelude, and being in the habit of importing what you use means you never
have to work out which group a class falls into.

Two rules that save a lot of searching:

- Engine class → `godot::classes::ClassName`.
- An enum that belongs to a class → `godot::classes::snake_case_class::EnumName`.
  So `Input`'s mouse mode is `godot::classes::input::MouseMode`, not
  `godot::classes::MouseMode` and not `godot::global::MouseMode`. This one is not
  guessable and costs everyone twenty minutes exactly once.

The prelude also brings in a set of *traits* whose methods you use constantly
without ever naming them: `base()` and `base_mut()` come from `WithBaseField`,
`signals()` from `WithSignals`, `Input::singleton()` from `Singleton`. If a
method you expect to exist is missing, a trait import is the usual reason.

**Two `impl` blocks, and why.** `impl INode3D for Spinner` is for methods Godot
calls, so it needs `#[godot_api]`. `impl Spinner` holds `bob_offset`, which only
Rust calls, so it stays a plain block. Keeping them separate makes the boundary
between "engine-facing" and "internal" visible at a glance.

**`self.base().get_position()` and `self.base_mut().set_position(...)`** — the
`Node3D` API. GDScript writes `position.y = 3` and Godot handles it; Rust makes
you fetch the whole `Vector3`, modify it, and set it back, because `position` is
a *method pair*, not a field.

Note the shape of it in `process`:

```rust
let y = self.bob_offset();                     // read first
let mut position = self.base().get_position();
position.y = y;
self.base_mut().set_position(position);        // write last
```

`self.base_mut()` borrows *all* of `self` mutably. Calling `self.bob_offset()`
inside the `set_position(...)` argument list would be a borrow-checker error. So
you hoist reads above writes. **Get used to this shape now** — it is most of what
porting gameplay code to Rust actually consists of, and Lesson 4 has a worked
example of the error message.

**`godot_print!`** — Rust's `println!` goes to the terminal; `godot_print!` goes
to Godot's **Output** dock as well, which is where you will actually be looking.
It takes the same format syntax. There is also `godot_warn!` and `godot_error!`,
which show up in the Debugger dock with a stack trace attached.

**`(self.elapsed * self.bob_speed).sin()`** — Rust puts maths on the number
(`x.sin()`) where GDScript puts it in front (`sin(x)`). The full set is on `f32`
and `f64` in the standard library: `sin`, `cos`, `atan2`, `exp`, `sqrt`, `abs`,
`clamp`, `powf`, `to_radians`, `to_degrees`.

**Assigning `position.y`, not adding to it.** `bob_offset` computes an absolute
height from `start_y` every frame. Accumulating (`position.y += ...`) with a sine
wave would drift, because floating-point error compounds. **Where you can compute
an absolute value instead of accumulating, do.**

### Step 3 — Build and place it

```bash
cargo build --manifest-path rust/Cargo.toml
```

Open the editor and `main.tscn`. Add a **`Spinner`** as a child of `Main` — it is
in the Create Node dialog now, under `Node3D`. Set its **Position** to
`(0, 1.6, 0)`.

Add a **`MeshInstance3D`** as a child of `Spinner`, give it a **New BoxMesh** with
**Size** `(1.2, 1.2, 1.2)`, and set the *mesh instance's* **Rotation** `y` to
`22.5` — a slight turn so you can actually see it spinning, which a perfectly
axis-aligned cube deliberately hides.

> **Why is the cube a child of the `Spinner` rather than the `Spinner` being the
> mesh?** Because it separates "where the thing is" from "what the thing looks
> like." The Rust code drives `Spinner`; the mesh is along for the ride. Swap the
> mesh for a different one and the behaviour is untouched. Your player, enemies
> and weapons all use this same separation.

Press **F5**.

The cube spins and bobs, and the Output dock says:

```
Spinner ready at y=1.60, spinning at 90 deg/s.
```

### Step 4 — Tune it live

With the game still running, switch to the editor and click **Remote** at the
top of the Scene dock. This shows the *running* game's tree rather than the saved
one.

Select `Spinner` and drag `Degrees Per Second` in the Inspector.

The cube's speed changes as you drag. **This is the entire reason `#[export]`
exists.** Finding numbers that feel right by dragging a slider takes seconds;
finding them by editing code, recompiling and relaunching takes minutes, and you
will not do it enough times to find a good value.

Switch back to **Local** when you are done — changes made in Remote are not
saved, and forgetting that has cost everyone an afternoon's tuning at least once.

---

## Check yourself

1. Where does `#[class(base = Node3D)]` show up in the GDScript version of this
   code?
2. What is `delta`, and what happens if you forget to multiply by it?
3. Why does `start_y` get set in `ready` rather than `init`?
4. Why is `elapsed` not `#[export]`ed?
5. Why is `bob_offset` in a plain `impl Spinner` and not in the `#[godot_api]`
   block?
6. Why does `process` compute `let y = self.bob_offset();` on its own line
   instead of passing it straight into `set_position`?
7. You change `#[init(val = 90.0)]` to `45.0`, rebuild, and the cube spins at the
   old speed. What happened?

<details>
<summary>Answers</summary>

1. `extends Node3D` at the top of the script.
2. Seconds since the previous frame. Forgetting it makes every rate a
   per-*frame* rate, so the game runs at different speeds on different machines
   and changes speed whenever the frame rate does.
3. `init` runs before the node is in the scene tree, so its position is not
   meaningful yet. `ready` runs once everything is live.
4. It is internal bookkeeping. Nothing outside the struct needs it, and exposing
   it would let a scene file overwrite it.
5. Because only Rust calls it. `#[godot_api]` is for things Godot needs to see —
   virtual methods, `#[func]`, `#[signal]`.
6. `self.base_mut()` borrows all of `self`, so calling another `&self` method
   inside its argument list is a borrow-checker error. Reads get hoisted above
   writes.
7. The node has an Inspector override stored in the scene file, and the override
   wins. Click the revert arrow next to the property.

</details>

---

## Extend it

- Make the bob follow a different shape. Try `(self.elapsed * self.bob_speed).sin().abs()`
  for a bouncing motion, and think about why it looks wrong at the bottom.
- Add an `#[export] wobble: bool` and make the spinner also rock on X when it is
  on. Notice how much better tuning it feels than editing constants.
- Move `rotate_y` into `physics_process` instead of `process` and watch carefully
  at a high frame rate. What changed, and why? (Lesson 4 explains it — try to
  work it out first.)
- Deliberately write `self.base_mut().set_position(Vector3::new(0.0, self.bob_offset(), 0.0))`
  and read the compiler error in full. That error is going to be a companion for
  the next few lessons; it is worth meeting it on purpose while the stakes are
  low.

---

## Commit

```bash
git add -A
git commit -m "Lesson 2: first Rust class -- Spinner"
```

---

**Next:** [Lesson 3 — The greybox arena](03-greybox-arena.md)
