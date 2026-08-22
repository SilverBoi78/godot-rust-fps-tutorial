# Design

The design document. What the game is, how it is put together, and why each
structural decision went the way it did.

---

## The game

A round-based survival shooter for one player, in a small enclosed arena.

- Enemies spawn in waves. Each round sends more of them, and they arrive faster,
  hit harder and take more killing than the round before.
- Killing an enemy pays points. Points are the only currency.
- Points buy doors, which open new areas of the map — more space to work with,
  and more directions for enemies to come from.
- Points also buy ammunition, from weapons mounted on walls.
- The round ends when the field is clear. Then there is a short breather, and
  the next round starts.
- You die when your health reaches zero. There is no continue.

The whole loop is: *shoot things → get points → spend points on more room and
more ammunition → survive a harder round.*

## Scope

This repository covers the prototype: an arena, a controller that feels good, one
gun that feels good, pooled enemies that path around cover, an escalating round
system, an economy, purchasable doors, a wall-mounted ammo vendor, and a HUD.

It stops before menus, saving, meta-progression, multiple weapons, or a second
map. Those are all reachable from where it ends, which is the point.

## Engine and language

**Godot 4.7.2, Forward+ renderer, all logic in Rust via gdext 0.5.5.**

Rust does not replace the editor. Scenes, materials, meshes, collision shapes,
the navigation mesh and the UI layout are all authored in Godot the normal way.
What Rust replaces is GDScript: every node type that has behaviour is a
`#[derive(GodotClass)]` struct compiled into a dynamic library the engine loads.

### What Rust buys

- **A compiler that reads the whole project.** Renaming a signal argument breaks
  the build instead of failing silently at 2am in round 14.
- **Enums that make illegal states unrepresentable.** The weapon cannot be
  reloading and firing at once, because `State` has no such variant, and every
  `match` on it must handle every case.
- **No per-call script overhead** in code that runs 48 times a physics tick.
- **`Result` and `Option` at the boundaries.** Godot returns nullable objects
  constantly; Rust makes you say what happens when they are null.

### What Rust costs

- **No inheritance.** gdext classes may extend engine classes only, never each
  other. The interaction system in this project is designed around that
  restriction rather than fighting it — see below.
- **A compile step.** GDScript edits are live; Rust edits are a `cargo build`
  and a re-open. Roughly ten seconds each time on this project.
- **Borrow-checker friction in gameplay code.** `self.base_mut()` borrows all of
  `self`, so half the porting work is hoisting reads above writes.
- **Verbosity.** `self.base().get_velocity()` where GDScript writes `velocity`.

The tutorial names each of these where it hits, rather than pretending the
trade is free.

## Architecture

Six ideas do most of the structural work.

### 1. Components over inheritance

Anything that can be hurt has a `Health` child node. Anything that can be
interacted with has an `Interactable` child node. Neither requires a shared base
class, which is convenient, because Rust has none to offer.

This is not a workaround. Composition was already the better answer for `Health`
— a player *has* health while *being* a `CharacterBody3D` — and being forced
into it for interactables produced a design that is arguably cleaner than the
inheritance version: the payment protocol lives in exactly one place and cannot
be overridden by accident, and an interactable can be any node type at all.

Where a subclass would have overridden a virtual method, the component holds a
`Callable` hook the parent installs, and emits a signal the parent reacts to.

### 2. Input is separated from simulation

Hardware fills in a `PlayerIntent` struct. The simulation reads that struct and
never touches `Input`. A keyboard, a gamepad, a replay file, an AI, or a network
packet can all produce one, and nothing downstream can tell the difference.

This costs about twenty lines now. Retrofitting it later means touching every
line of movement, weapon and interaction code.

### 3. Facts on a bus, commands down a wire

`EventBus` is an autoload carrying global signals. Everything on it describes
something that **already happened** — `enemy_died`, never `kill_enemy`.

Anything with a clear owner uses a direct call or a local signal instead. A
weapon's recoil goes straight to its own player; putting it on the bus would
mean every player kicks when anyone fires.

### 4. State that a run owns, separated from state that outlives it

`GameState` is the autoload holding run state — points, round number, kills. It
is wiped when a run ends. Persistent progression does not exist yet, and the
seam is drawn now so that adding it later is additive rather than surgical.

`GameState::award_points` takes everything as arguments and returns nothing, so
that making it host-authoritative in co-op would not change its signature.
`try_spend` does return a value, because a door genuinely must not open if you
could not afford it.

### 5. Enemies are pooled, never instantiated mid-round

48 enemies are built at load and reused forever. Runtime instantiation is the
main cause of frame hitching in this genre, and the cost is not the allocation —
it is loading the scene, building its nodes, resolving its resources and
entering the tree, all inside one frame, at the moment the game is busiest.

A pooled object must be able to go dormant and come back clean, so every mutable
field is reset in `activate` rather than assumed fresh from `ready`.

### 6. Tuning is data, not code

Round scaling — enemy count, health, speed, spawn interval — comes from `Curve`
resources sampled over a normalised round number. Changing the difficulty ramp
means dragging a line in the editor, not editing a formula and recompiling.

## Performance budget

| Thing | Target |
|---|---|
| Active enemies | 32 typical, 48 hard cap |
| Physics tick | 60 Hz |
| Mid-round allocations | none for enemies |

## Cross-platform

Windows, macOS and Linux. The rules that keep it that way:

1. `res://` paths everywhere, never OS paths.
2. Paths are case-sensitive on Linux and not on Windows. Match the case exactly.
3. LF line endings in the repository, enforced by `.gitattributes`.
4. The `.gdextension` file lists a library entry for every target platform, and
   the naming differs per platform (`libshooter.so`, `shooter.dll`,
   `libshooter.dylib`).
5. Physical key scancodes, not unicode, so WASD works on AZERTY.
6. No hard-coded absolute paths anywhere, including in the tutorial's commands.

## Verification

`reference/` has a headless test suite — 48 checks covering the player, the
weapon's hitscan and reload cycle, headshot registration through an `Area3D`,
navigation baking and rebaking, pooling and recycling, the economy, and both
interactables.

Every code snippet in the tutorial is copied out of `reference/` **after** that
suite passes. Nothing in the lessons is written from memory.

## Assets

None. Boxes, capsules and spheres; flat-colour materials; audio synthesised at
startup by `reference/rust/src/audio.rs`.

Two reasons. It keeps someone else's work out of the repository, including the
"just a placeholder for now" kind that gets committed and forgotten. And it
removes the excuse that art is needed before the mechanics can be evaluated.
