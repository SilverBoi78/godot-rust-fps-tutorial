# Appendix D — Glossary

Terms this tutorial uses, in the sense it uses them. Split into Godot, gdext, and
game-design vocabulary, because the three overlap confusingly.

---

## Godot

**Node** — one object that does one thing. `MeshInstance3D` draws a mesh;
`Camera3D` renders a view. Combining several makes a character.

**Scene** — a saved tree of nodes, in a `.tscn` file. Roughly a prefab.

**Instance** — a copy of a scene placed inside another. Editing the saved scene
changes every instance; an instance may override individual properties without
breaking the link.

**Scene tree** — the live tree of every node currently in the game.

**Root** — the top node of a scene. `get_tree().get_root()` is the window; the
*current scene* is its child.

**`res://`** — the project folder. Use it for everything shipped with the game.

**`user://`** — a per-user writable folder (AppData, `~/.local/share`). Saves and
settings go here; `res://` is read-only in an exported build.

**Autoload** — a node Godot creates at startup and keeps for the session. A
global. This project has two.

**Signal** — Godot's observer pattern. An object announces something happened;
listeners have registered to be told.

**Group** — a string tag on a node. `is_in_group("headshot")`. Lets code ask what
something *is for* without knowing its type.

**Resource** — data that lives in a file and can be shared: materials, meshes,
curves, scenes. Refcounted. **Shared by default**, which is the source of several
classic bugs.

**`Variant`** — Godot's dynamically-typed value. Anything crossing the engine
boundary untyped is one.

**`Dictionary` / `Array`** — Godot's collections, holding `Variant`s. Not Rust's
`HashMap` and `Vec`; they live engine-side and are refcounted.

**Collision layer** — "what I am." A 32-bit mask.

**Collision mask** — "what I collide with." A collides with B when A's mask
contains B's layer. Deliberately not symmetric.

**`Area3D`** — detects overlaps without blocking. Triggers, and hitboxes.

**`CharacterBody3D`** — a solid body you move yourself. Players and enemies.

**`StaticBody3D`** — solid and never moves. Level geometry.

**`move_and_slide()`** — moves a body by its velocity, resolving collisions and
sliding along surfaces rather than stopping dead.

**Navmesh** — the walkable surface, shrunk back from walls by the agent radius,
that pathfinding runs on.

**`NavigationAgent3D`** — asks the navigation server for a route and hands you the
next point along it.

**Tween** — a small animation built in code and owned by the scene tree. Runs
itself, cleans itself up.

**`Callable`** — Godot's function pointer. Looked up by method **name**, so the
target must be registered and a typo fails silently.

**`Control`** — the base of all UI. Has a rectangle, anchors and offsets.

**`CanvasLayer`** — draws its children in screen space, above the 3D world.

**Unique name** — a node marked "Access as Unique Name", reachable as `%Name` from
anywhere in its scene.

**`delta`** — seconds since the previous frame. Multiply every per-second rate by
it.

**`_process` vs `_physics_process`** — per rendered frame versus per fixed 60 Hz
tick. Visuals in the first, movement in the second.

**RID** — an opaque handle to a resource inside a server. The physics engine deals
in these, not in nodes.

**`.tres` / `.tscn`** — a saved resource and a saved scene. Both plain text, both
diffable.

**`.import`** — generated metadata for an imported asset. Commit it.

---

## gdext

**gdext** — the `godot` crate: Rust bindings for Godot 4. Not to be confused with
gdnative, which was the Godot 3 equivalent.

**GDExtension** — Godot's C-ABI plugin interface. gdext is a Rust implementation
of it.

**`.gdextension`** — the file telling Godot where the library is and what its
entry point is called.

**`cdylib`** — a dynamic library with a C-compatible interface. The only kind
Godot can load.

**`Gd<T>`** — a handle to a Godot object. Refcounted for resources, a raw pointer
for manually-freed nodes. Does **not** keep a node alive.

**`Base<T>`** — the engine object your struct extends, held by composition because
Rust has no inheritance. Reached with `base()` and `base_mut()`.

**`OnReady<T>`** — a field empty until `ready`, then permanently populated.
`Deref`s to the inner type. Panics loudly if a node path is wrong.

**`bind()` / `bind_mut()`** — borrow the *Rust* struct inside someone else's
`Gd<T>`. Checked at runtime, not compile time.

**`base()` / `base_mut()`** — reach the *engine* object your struct extends.

**`to_gd()`** — get a `Gd<Self>` from inside your own method. Needed to connect a
signal to yourself.

**`#[derive(GodotClass)]`** — register this struct as a Godot class.

**`#[godot_api]`** — expose this `impl` block to Godot. Required for virtual
methods, `#[func]` and `#[signal]`.

**`#[func]`** — make a method callable from Godot. Required for `Callable`
targets.

**`#[signal]`** — declare a signal. A semicolon instead of a body.

**`#[export]` / `#[var]`** — visible in the Inspector / visible to Godot but not
the Inspector.

**`I…` trait** — the virtual-method trait for a base class: `INode`, `INode3D`,
`ICharacterBody3D`, `IControl`.

**`_ex()` builder** — the variant of a method with optional arguments. Set what
you want, call `.done()`.

**`new_gd()` / `new_alloc()`** — construct a refcounted object / a manually-freed
one.

**`cast` / `try_cast` / `upcast`** — downcast panicking, downcast recoverable,
upcast infallibly.

**`GString`** — Godot's string. `From<&str>` but not `From<String>`.

**`experimental-godot-api`** — the feature flag exposing APIs Godot marks
experimental, including all of navigation.

**Double borrow** — calling `bind_mut()` on an object already borrowed. A runtime
panic; avoided by scoping borrows and copying small data out.

---

## Game design

**Greybox** — a level built from untextured primitives, to settle layout before
art exists.

**Hitscan** — a weapon that resolves instantly with a raycast, as opposed to one
that spawns a travelling projectile.

**Viewmodel** — the first-person model of your own weapon. Cosmetic; the shot
comes from the camera.

**Recoil** — the camera disturbance from firing. Fixed vertical can be learned;
random horizontal cannot be fully eliminated.

**Head bob** — small camera motion while walking, tied to distance travelled.

**Feel** — how a control scheme responds. Made of acceleration curves, response
times and feedback layers, and found by iterating on numbers rather than by
reasoning about them.

**Juice** — the feedback layer: hitmarkers, screen shake, flashes. Cheap
individually, and the difference between "functional" and "satisfying".

**Object pool** — a fixed set of objects created at load and reused, so nothing is
instantiated mid-combat.

**Prewarm** — building the pool up front, paying the cost where a pause is
invisible.

**Wind-up / telegraph** — the delay and visual cue before an attack lands, so it
can be reacted to. Only meaningful if range is re-checked at the moment of impact.

**Hysteresis** — using different thresholds to enter and leave a state, so
something sitting on the boundary does not flip every tick.

**Spawn budget** — how many enemies a round owes, separate from how many may exist
at once.

**Concurrency cap** — the maximum simultaneously active. A performance guarantee.

**Intermission** — the gap between rounds. The main pacing lever.

**Run state vs persistent state** — what is wiped on death versus what outlives
it. Keeping the seam clean is where saving and validation later go.

**Wall buy** — a purchase point fixed to the level geometry.

**Zone** — a region of the map that starts closed and is opened by a purchase,
widening both the space and where enemies come from.

---

## Naming in this project

Consistent vocabulary, so grep works and readers are not guessing:

| Term | Means |
|---|---|
| `Player` | the player character |
| `Enemy` | the basic hostile |
| `Health` | the damage component |
| `Interactable` | the interaction component |
| `Door` | a purchasable barrier that opens a `Zone` |
| `WallBuy` | a purchasable ammunition refill |
| `Zone` | a region opened by a `Door` |
| `RoundDirector` | the only thing that knows the round number |
| `GameState` | run-scoped numbers |
| `EventBus` | global signals |
| `EnemyPool` | the pre-instantiated enemy pool |
| `PlayerIntent` | what the player wants this tick |
| `PlayerInputSource` | the only thing that touches `Input` |
| `Interactor` | the player's component that finds interactables |

Conventions:

- `snake_case` fields and functions, `PascalCase` types — ordinary Rust. gdext
  converts to Godot's conventions on its own.
- Leading `_` on a parameter means deliberately unused.
- Signals are **past tense**: `died`, `points_changed`, `zone_opened`. Never
  imperative.
- `try_` prefixes a function that can fail and returns whether it did.
- `find_` prefixes a lookup returning `Option`.
