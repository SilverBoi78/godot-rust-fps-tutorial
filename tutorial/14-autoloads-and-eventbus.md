# Lesson 14 — Autoloads and the EventBus

## What we're building

Two globals: an `EventBus` carrying signals anything can listen to, and (next
lesson) a `GameState` holding the numbers a run is made of.

The interesting part is that Rust has no globals worth the name, and Godot's
autoload mechanism expects a script file that gdext does not produce. The answer
is neither difficult nor obvious.

---

## The concept

### The problem

Look at what already needs to talk to things it should not know about:

- The **round director** needs to know when an enemy dies. It should not have to
  find and connect to all 48 pooled enemies.
- The **HUD** needs the points total. It should not reach into the economy.
- The **arena** needs to re-bake when a zone opens. It should not know what a
  door is.

Every one of those is a pair of systems that are conceptually unrelated and would
become permanently coupled by a direct reference.

### The EventBus

One global object carrying signals. Emitters emit; listeners listen; neither
knows the other exists.

```rust
#[godot_api]
impl EventBus {
    // --- combat ---------------------------------------------------------
    #[signal]
    pub fn enemy_damaged(
        enemy: Gd<Node3D>,
        amount: f32,
        is_headshot: bool,
        source: Option<Gd<Node>>,
    );
    #[signal]
    pub fn enemy_died(enemy: Gd<Node3D>, killer: Option<Gd<Node>>);
    #[signal]
    pub fn player_damaged(amount: f32, current: f32, maximum: f32);
    #[signal]
    pub fn player_died();
```

Three rules keep this from becoming spaghetti, and they are not optional:

**1. Signals here describe facts, never commands.** `enemy_died`, not
`kill_enemy`. A command-shaped global signal is a method call you can no longer
trace, and a bus full of them is worse than the coupling it replaced.

**2. Anything with a clear owner uses a direct call or a local signal instead.**
A weapon's recoil goes straight to its own player — putting it on the bus would
mean every player kicks when anyone fires. The bus is for *broadcast*, not for
convenience.

**3. Every signal is declared here, with typed arguments.** This one file is the
readable index of everything that can happen in the game. That is worth as much
as the decoupling.

The full set, grouped by area:

```rust
    // --- economy --------------------------------------------------------
    #[signal]
    pub fn points_changed(total: i32);
    #[signal]
    pub fn points_awarded(amount: i32, reason: GString);
    #[signal]
    pub fn purchase_failed(cost: i32);

    // --- round flow -----------------------------------------------------
    #[signal]
    pub fn round_started(round_number: i32, enemy_count: i32);
    #[signal]
    pub fn round_cleared(round_number: i32);
    #[signal]
    pub fn enemies_remaining_changed(remaining: i32);

    // --- world ----------------------------------------------------------
    #[signal]
    pub fn zone_opened(zone_name: GString);
    #[signal]
    pub fn interact_target_changed(prompt: GString, affordable: bool);
}
```

Fourteen signals for the whole game. If this list grows past thirty, that is a
signal (so to speak) that rule 2 is being ignored.

### Autoloads, and the gdext wrinkle

An **autoload** is a node Godot creates at startup, parents to the scene tree
root, and keeps alive for the whole session. It persists across scene changes,
which is what makes it a global.

You register one in **Project Settings → Autoload** by giving it a path and a
name. And here is the wrinkle: **the path must be a script or a scene**, and
gdext produces neither. Your Rust classes are node *types*, not script files
sitting in `res://`.

The answer is a one-line scene whose root node is your class:

```ini
[gd_scene format=3]

[node name="EventBus" type="EventBus"]
```

That is the entire file. Autoload it and Godot instantiates the root node —
your Rust class — as the autoload.

It feels like a workaround and it is essentially free. The scene has no children,
no resources, and costs one file read at startup.

### Reaching it from Rust

GDScript autoloads register a *global identifier*: type `EventBus` anywhere and it
resolves. Rust has no such mechanism, so we walk to it:

```rust
/// Shared by both autoloads. Panics if the autoload is missing, which is the
/// right behaviour: a missing autoload is a project misconfiguration, not a
/// runtime condition worth handling.
pub(crate) fn autoload<T: Inherits<Node>>(name: &str) -> Gd<T> {
    Engine::singleton()
        .get_main_loop()
        .expect("no main loop")
        .cast::<SceneTree>()
        .get_root()
        .get_node_as::<T>(name)
}
```

and give each autoload a named accessor:

```rust
impl EventBus {
    /// Reach the autoload from anywhere.
    ///
    /// GDScript gets `EventBus` as a free-floating global name; Rust has no
    /// equivalent, so we walk to it explicitly. `Engine::singleton()` is
    /// reachable without a node, which is what makes this callable from code
    /// that is not itself in the scene tree.
    pub fn singleton() -> Gd<EventBus> {
        autoload("EventBus")
    }
}
```

so that call sites read almost like GDScript:

```rust
EventBus::singleton().signals().enemy_died().emit(&this, killer.as_ref());
```

**Why `Engine::singleton()` rather than `self.base().get_tree()`?** Because it
needs no node. `GameState::award_points` is called from `Enemy::on_damaged`, from
the weapon, from an interactable — and a free function that works from anywhere,
including from code not in the tree, is worth the two extra hops.

`Inherits<Node>` is the generic bound that says "T is some Godot class descended
from `Node`", which is what lets one function serve both autoloads.

**Why panic instead of returning `Option`?** Because a missing autoload is a
project misconfiguration, not a runtime condition. Returning `Option` would push
an `unwrap` or a silent early return to every one of the several dozen call
sites, all of which can do nothing useful about it. Panicking at the first use,
with a message naming the missing node, is the more honest failure. Choosing
where to put the panic is a real design decision, and this is a case where "as
early as possible, as loudly as possible" wins.

### The cost, honestly

Every `EventBus::singleton()` walks Engine → MainLoop → SceneTree → root →
`get_node`. That is a handful of pointer hops and a string-keyed child lookup, and
it happens on every emit.

For this project that is comfortably invisible — it is a few hundred lookups a
second against a 60 Hz budget. If profiling ever said otherwise, the fix is to
cache the `Gd<EventBus>` in a field during `ready`. That is deliberately *not*
done here, because a cached handle is one more thing to invalidate on a scene
change, and paying for a problem you do not have is how codebases get hard to
read.

GDScript's version is a direct global lookup and is genuinely faster. Worth
knowing; not worth acting on.

### Autoloads are not an architecture

An autoload is a global variable, and everything you know about global variables
still applies. This project has exactly two, and both earn it:

- `EventBus` holds **no state at all** — it is a set of signals.
- `GameState` holds only **run-scoped numbers**, and announces every change.

The moment an autoload holds a reference to a node, it becomes a way for any
system to reach any other, and the decoupling it was supposed to provide is gone.
If you find yourself adding `var player: Player` to a singleton, stop.

---

## Do it

### Step 1 — The `EventBus` class

Create `rust/src/event_bus.rs` and add `pub mod event_bus;` to `lib.rs`.

```rust
#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct EventBus {
    base: Base<Node>,
}
```

That is the whole struct. No fields, no state, no methods beyond `singleton()`.
Everything it does is in the `#[signal]` declarations quoted above.

`base=Node`, not `Node3D`, because it has no position and never will.

### Step 2 — The autoload scene

Create `godot/autoload/event_bus.tscn` by hand — it is faster than doing it in
the editor:

```ini
[gd_scene format=3]

[node name="EventBus" type="EventBus"]
```

Do the same for `game_state.tscn` (Lesson 15 writes the class; making the file
now saves a trip back):

```ini
[gd_scene format=3]

[node name="GameState" type="GameState"]
```

### Step 3 — Register them

**Project → Project Settings → Autoload.** Add each scene with a node name
matching exactly:

| Path | Node Name | Enabled |
|---|---|---|
| `res://autoload/event_bus.tscn` | `EventBus` | yes |
| `res://autoload/game_state.tscn` | `GameState` | yes |

The **Node Name** is what `get_node_as::<T>(name)` looks up. Get it wrong and
`singleton()` panics with a message naming the path it could not find.

Order matters when one autoload uses another during `ready`. `EventBus` first,
since `GameState` emits on it.

Your `project.godot` now contains:

```ini
[autoload]

EventBus="*res://autoload/event_bus.tscn"
GameState="*res://autoload/game_state.tscn"
```

The leading `*` means "expose this as a global name" — relevant for GDScript,
harmless for us.

### Step 4 — Emit from the enemy

Replace the commented-out lines in `Enemy::on_damaged` and `on_died`:

```rust
        let this = self.to_gd().upcast::<Node3D>();
        let headshot = self.last_hit_was_headshot;
        EventBus::singleton().signals().enemy_damaged().emit(
            &this,
            amount,
            headshot,
            source.as_ref(),
        );
```

and in `on_died`:

```rust
        let this = self.to_gd().upcast::<Node3D>();
        EventBus::singleton()
            .signals()
            .enemy_died()
            .emit(&this, killer.as_ref());
```

`self.to_gd().upcast::<Node3D>()` because the signal is declared as carrying a
`Gd<Node3D>` rather than a `Gd<Enemy>`. That is deliberate: a listener that only
wants to know something died should not have to depend on the `Enemy` type. A
listener that *does* care can `try_cast` it back.

### Step 5 — Listen from the player

```rust
    fn on_health_damaged(&mut self, amount: f32, current: f32, _source: Option<Gd<Node>>) {
        let max = self.health.bind().max_health;
        EventBus::singleton()
            .signals()
            .player_damaged()
            .emit(amount, current, max);
    }
```

Note the shape: the player's `Health` signals **locally** to the player, and the
player re-broadcasts **globally**. That is rule 2 doing its job — `Health` is a
component with a clear owner, so it stays local, and the player decides what the
rest of the game is told.

### Step 6 — Wire the arena

The zone-opening connection from Lesson 11, now that there is a bus to connect to:

```rust
        let this = self.to_gd();
        EventBus::singleton()
            .signals()
            .zone_opened()
            .connect_other(&this, Arena::on_zone_opened);
```

`Zone` itself arrives in Lesson 17. For now the connection compiles and never
fires.

### Step 7 — Listen from `Main`

`Main` is the scene root that wires the run together. Give it a bus connection —
this exact one is used from Lesson 16 onward, so it is not throwaway:

```rust
        let this = self.to_gd();
        EventBus::singleton()
            .signals()
            .player_died()
            .connect_other(&this, Main::on_player_died);
```

with a handler that, for now, just prints:

```rust
    fn on_player_died(&mut self) {
        godot_print!("player died");
    }
```

(Lesson 16 replaces the body with a real run summary.)

To watch the bus work end to end, temporarily point the same `connect_other` at
`enemy_died` instead, with a handler matching *its* signature —
`fn on_enemy_died(&mut self, _enemy: Gd<Node3D>, _killer: Option<Gd<Node>>)`. The
handler's parameters must match the signal's declaration or it will not compile,
which is exactly the check GDScript does not give you.

Run, spawn an enemy, kill it, and watch the message appear — printed by a
listener that holds no reference to the enemy, the pool, or anything else. Then
put the `player_died` connection back.

---

## Check yourself

1. What is an autoload, and why can't a gdext class be one directly?
2. What goes in the `.tscn` file that makes it work?
3. Why does `singleton()` go through `Engine::singleton()` rather than
   `self.base().get_tree()`?
4. Why does `autoload()` panic instead of returning `Option`?
5. State the three rules that keep the EventBus from becoming spaghetti.
6. Why does the weapon's recoil *not* go on the bus?
7. Why does `enemy_died` carry a `Gd<Node3D>` rather than a `Gd<Enemy>`?
8. What is the runtime cost of `EventBus::singleton()`, and why is it not cached?

<details>
<summary>Answers</summary>

1. A node Godot creates at startup and keeps alive for the session. Autoload
   paths must point at a script or a scene, and gdext produces node *types*, not
   script files.
2. A one-line scene whose root node's `type` is your Rust class.
3. `Engine::singleton()` needs no node, so the accessor works from anywhere —
   including code that is not itself in the scene tree.
4. A missing autoload is a project misconfiguration. `Option` would push a
   useless `unwrap` to dozens of call sites that can do nothing about it.
5. Facts not commands; anything with a clear owner uses a direct call or local
   signal; every signal declared in one place with typed arguments.
6. It has a clear owner. On the bus, every player would kick when anyone fired.
7. So a listener that only cares that something died does not have to depend on
   the `Enemy` type.
8. Four pointer hops and a string-keyed child lookup per call — invisible at this
   scale. It is not cached because a cached handle is one more thing to
   invalidate on a scene change.

</details>

---

## Extend it

- Add a `#[func] fn log_all(&mut self)` to `EventBus` that connects every signal
  to a printer, and enable it behind a debug flag. A live event log is the single
  most useful debugging tool in an event-driven codebase.
- Deliberately misspell the autoload's Node Name in Project Settings and run. Read
  the panic. That is the failure mode, and it is a good one.
- Add a `Profile` autoload holding a persistent high score, saved to
  `user://profile.cfg`. Note where the seam between it and `GameState` should be,
  and resist the temptation to let run code write to it directly.
- Count the connections: how many listeners does `points_changed` have by the end
  of Lesson 18? Would a direct call have been simpler for any of them?

---

## Commit

```bash
git add -A
git commit -m "Lesson 14: EventBus and GameState autoloads"
```

---

**Next:** [Lesson 15 — The RoundDirector](15-round-director.md)
