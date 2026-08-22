# Lesson 15 — The RoundDirector

## What we're building

The thing that makes this a game: rounds that start, spawn a budget of enemies at
a controlled rate, wait for the field to clear, and then do it again harder.

The difficulty ramp comes from `Curve` resources you drag in the editor rather
than from formulas in code, which is the most valuable idea in this lesson.

---

## The concept

### One owner for the round number

The round number is exactly the kind of value that leaks. It is interesting to
the HUD, the spawner, the enemy scaling, the score, the music — and if each of
them reads it from wherever, changing how rounds work means touching all of them.

So: **one class knows the round number, and everything else finds out by
listening.** The `RoundDirector` owns it; `EventBus` carries the announcements.

### The phase machine

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Spawning,
    WaitingForClear,
    Intermission,
}
```

| Phase | What happens | Leaves when |
|---|---|---|
| `Idle` | nothing; the run has not started | `begin()` is called |
| `Intermission` | counting down to the next round | the timer runs out |
| `Spawning` | releasing enemies on a drip | the budget is spent |
| `WaitingForClear` | the field still has enemies | the last one dies |

Third enum-and-`match` state machine in this project. That repetition is the
point — once the shape is familiar you stop inventing structure for each new
system and start recognising which one it is.

```rust
    fn physics_process(&mut self, delta: f64) {
        match self.phase {
            Phase::Idle => {}
            Phase::Intermission => {
                self.phase_timer -= delta as f32;
                if self.phase_timer <= 0.0 {
                    self.start_round();
                }
            }
            Phase::Spawning => self.tick_spawning(delta),
            Phase::WaitingForClear => {
                if self.alive <= 0 {
                    self.clear_round();
                }
            }
        }
    }
```

Note `Phase::Idle => {}` — an explicit empty arm rather than a `_ =>` catch-all.
A wildcard would silently swallow any phase added later; an explicit arm means
adding one breaks the build until you have thought about it. **Prefer exhaustive
arms over `_` in state machines.**

### Tuning curves as data

Here is the idea worth taking to every project you build after this one.

The naive way to scale difficulty:

```rust
let enemy_count = 6 + round_number * 2;           // linear, and a guess
let health_scale = 1.0 + round_number as f32 * 0.15;
```

The problems are not subtle. You cannot see the shape. Tuning means editing code
and recompiling. Two curves that should relate — count going up while the gap
between spawns comes down — are two unrelated expressions. And a designer who is
not a programmer cannot touch any of it.

A `Curve` is a Godot resource: a hand-drawn line from x = 0 to x = 1, edited by
dragging control points. So:

```rust
    /// Enemies in a round, indexed by round number / max_round.
    #[export]
    count_curve: Option<Gd<Curve>>,
    /// Enemy health multiplier.
    #[export]
    health_curve: Option<Gd<Curve>>,
    /// Enemy movement speed multiplier.
    #[export]
    speed_curve: Option<Gd<Curve>>,
    /// Seconds between individual spawns.
    #[export]
    spawn_interval_curve: Option<Gd<Curve>>,
```

and one sampler:

```rust
    /// Curves are authored over x = 0..1, so the round number has to be
    /// normalised. Past `max_round` the value clamps, which means late rounds
    /// plateau rather than scaling to absurdity -- deliberate, and easy to
    /// change by editing the curve.
    fn sample(&self, curve: &Option<Gd<Curve>>, round_number: i32, fallback: f32) -> f32 {
        let Some(curve) = curve else {
            return fallback;
        };
        let t = (round_number as f32 / self.max_round as f32).clamp(0.0, 1.0);
        curve.sample(t)
    }
```

What this buys:

- **You can see the difficulty ramp.** It is a drawn line.
- **Tuning is dragging, not recompiling.** You can do it while the game runs.
- **The shape is not constrained to what you can express in one line.** Flat for
  three rounds, then a jump, then a slow climb — trivial as a curve, awkward as a
  formula.
- **The `fallback` argument** means a missing curve degrades to a sensible
  constant rather than crashing. Forgetting to assign one in the Inspector is a
  very easy mistake.

Clamping at `max_round` is a design decision, not a technical one: round 40 plays
the same as round 30 rather than scaling into nonsense. Where a game should
plateau is a real question, and having it be one number and one curve edge makes
it a question you can answer by playing.

### The spawn budget

```rust
    fn tick_spawning(&mut self, delta: f64) {
        if self.to_spawn <= 0 {
            self.phase = Phase::WaitingForClear;
            return;
        }

        self.spawn_timer -= delta as f32;
        if self.spawn_timer > 0.0 {
            return;
        }

        // Respect the concurrency cap. If the field is full we simply wait --
        // the enemies still owed will arrive as the player thins them out.
        let full = match &self.pool {
            Some(pool) => pool.bind().active_count() >= self.max_active,
            None => true,
        };
        if full {
            return;
        }
```

Two independent limits, doing different jobs:

- **`to_spawn`** — how many are owed this round. A budget.
- **`max_active`** — how many may exist at once. A performance cap.

Round 30 owes 34 enemies but only 32 may be alive. The remaining two arrive as
you kill others, which is exactly the pressure the design wants: the field stays
full while you are working through it.

Note that hitting the cap is *not* an error and produces no message. It returns
and tries again next tick.

### Spawn points that come and go

```rust
    pub fn refresh_spawn_points(&mut self) {
        self.spawn_points.clear();
        let Some(spawn_root) = self.spawn_root.clone() else {
            return;
        };

        // Only spawn points in zones that are currently open. Buying a door
        // therefore widens where enemies come from, with no extra wiring.
        let markers = spawn_root
            .find_children_ex("*")
            .type_("Marker3D")
            .owned(false)
            .done();

        for node in markers.iter_shared() {
            let Ok(marker) = node.try_cast::<Marker3D>() else {
                continue;
            };
            if marker.is_visible_in_tree() && marker.is_in_group("spawn_point") {
                self.spawn_points.push(marker);
            }
        }
    }
```

This is the quiet best part of the design. `is_visible_in_tree()` is true only if
this node *and every ancestor* is visible — and a closed `Zone` is hidden. So
closed zones contribute no spawn points, and **buying a door widens the attack
without a single line of code connecting doors to spawning.**

Two systems that never mention each other, composing correctly through a property
they both already had. When a design does this, you have drawn the boundaries in
the right place.

`find_children_ex("*")` is the builder form again: `type_("Marker3D")` filters by
class, `owned(false)` includes nodes that belong to instanced sub-scenes. That
last one matters — with the default `true` you would find nothing, because every
marker belongs to `arena.tscn` rather than to the scene doing the search. It is
an easy hour to lose.

### Dependency injection, once more

```rust
    /// Assigned by `Main` in its `ready`. Keeping the wiring in one place beats
    /// having each system hunt for its own dependencies through the tree.
    pool: Option<Gd<EnemyPool>>,
    spawn_root: Option<Gd<Node3D>>,
```

```rust
    pub fn begin(&mut self, target: Gd<Node3D>, pool: Gd<EnemyPool>, spawns: Gd<Node3D>) {
        self.target = Some(target);
        self.pool = Some(pool);
        self.spawn_root = Some(spawns);
        self.refresh_spawn_points();
```

Same rule as the weapon's camera in Lesson 7. `Main` knows the scene layout;
nothing else has to.

### Counting the dead

```rust
    fn on_enemy_died(&mut self, _enemy: Gd<Node3D>, _killer: Option<Gd<Node>>) {
        self.alive = (self.alive - 1).max(0);
        EventBus::singleton()
            .signals()
            .enemies_remaining_changed()
            .emit(self.alive);
    }
```

The director never touches an enemy. It listens to the bus, decrements, and
re-broadcasts a number the HUD will use. `.max(0)` guards against a double-count
pushing it negative and stranding the round in `WaitingForClear` forever — a
bug that would look like "the game froze" and take a while to find.

---

## Do it

### Step 1 — Make the curves

In the FileSystem dock, create `res://content/curves/`.

Right-click → **Create New → Resource → Curve**, save as `round_count.tres`, and
double-click it. The curve editor opens at the bottom.

Set its **Max Value** to `40` (in the Inspector, under `_limits`, or by dragging
the range in the editor), then place three points:

| x | y | meaning |
|---|---|---|
| 0.0 | 6 | round 1: six enemies |
| 0.35 | 18 | round ~10: eighteen |
| 1.0 | 34 | round 30+: thirty-four |

Repeat for the other three:

**`round_health.tres`** — max value `8`:

| x | y |
|---|---|
| 0.0 | 1.0 |
| 0.5 | 2.6 |
| 1.0 | 6.0 |

**`round_speed.tres`** — max value `2`:

| x | y |
|---|---|
| 0.0 | 0.75 |
| 0.45 | 1.0 |
| 1.0 | 1.45 |

**`round_spawn_interval.tres`** — max value `2`:

| x | y |
|---|---|
| 0.0 | 1.5 |
| 0.4 | 0.85 |
| 1.0 | 0.35 |

Note that enemies start *slower* than their base speed (0.75×) and only exceed it
around round 14. Early rounds should be a warm-up.

> **Curves are `.tres` files and `.tres` files are text.** If dragging points is
> fiddly, open one in an editor and type the numbers. The reference build's are
> at `reference/godot/content/curves/`, and they are four lines each.

### Step 2 — The director class

Create `rust/src/round_director.rs` and add `pub mod round_director;` to `lib.rs`.

The tuning:

```rust
    /// The round at which the curves reach their right-hand edge.
    #[export]
    #[init(val = 30)]
    max_round: i32,
    /// Simultaneous enemies allowed.
    #[export]
    #[init(val = 32)]
    max_active: i32,
    #[export]
    #[init(val = 6.0)]
    intermission_seconds: f32,
    #[export]
    #[init(val = 3.0)]
    first_round_delay: f32,
```

The internal state:

```rust
    phase: Phase,
    target: Option<Gd<Node3D>>,
    spawn_points: Vec<Gd<Marker3D>>,
    to_spawn: i32,
    alive: i32,
    spawn_timer: f32,
    phase_timer: f32,
```

`first_round_delay` gives the player three seconds to get their bearings before
anything happens. Games that start the action instantly feel hostile in a way
players notice but rarely articulate.

### Step 3 — Starting a round

```rust
    fn start_round(&mut self) {
        let round_number = {
            let mut state = GameState::singleton();
            let mut state = state.bind_mut();
            state.round_number += 1;
            state.round_number
        };

        self.refresh_spawn_points();

        self.to_spawn = self.count_for_round(round_number);
        self.alive = self.to_spawn;
        self.spawn_timer = 0.0;
        self.phase = Phase::Spawning;

        EventBus::singleton()
            .signals()
            .round_started()
            .emit(round_number, self.to_spawn);
        EventBus::singleton()
            .signals()
            .enemies_remaining_changed()
            .emit(self.alive);
    }
```

The `{ ... }` block around the `GameState` borrow returns the round number and
releases the borrow at the closing brace. Without it, the borrow would still be
alive when `EventBus` emits — and a listener that reads `GameState` would panic.
This is the Lesson 12 discipline applied to a global rather than a child.

`refresh_spawn_points()` runs at the start of every round, so a door bought
mid-round takes effect from the next one.

### Step 4 — Spawning one

```rust
    fn spawn_one(&mut self) -> bool {
        if self.spawn_points.is_empty() {
            return false;
        }
        let (Some(target), Some(mut pool)) = (self.target.clone(), self.pool.clone()) else {
            return false;
        };

        let index = (godot::global::randi() as usize) % self.spawn_points.len();
        let position = self.spawn_points[index].get_global_position();

        let round_number = GameState::singleton().bind().round_number;
        let health_scale = self.sample(&self.health_curve, round_number, 1.0);
        let speed_scale = self.sample(&self.speed_curve, round_number, 1.0);

        pool.bind_mut()
            .spawn(position, target, health_scale, speed_scale)
            .is_some()
    }
```

Returning `bool` rather than the enemy makes the caller's job obvious:

```rust
        if self.spawn_one() {
            self.to_spawn -= 1;
            let round_number = GameState::singleton().bind().round_number;
            self.spawn_timer = self.sample(&self.spawn_interval_curve, round_number, 1.4);
        }
```

**The budget only decrements on success.** If the pool is exhausted, `spawn_one`
returns false, `to_spawn` is unchanged, and the round still owes that enemy. It
arrives when one is freed. Getting this wrong — decrementing regardless — means
rounds silently spawn fewer enemies than the curve says, which is very hard to
notice and impossible to diagnose from the symptoms.

`godot::global::randi()` is Godot's global RNG rather than Rust's `rand` crate,
deliberately. Using the engine's generator means the whole game shares one
seedable source, which is what a replay or a deterministic co-op simulation would
need later. Adding `rand` would work today and be a problem then.

### Step 5 — Clearing

```rust
    fn clear_round(&mut self) {
        let round_number = GameState::singleton().bind().round_number;
        EventBus::singleton()
            .signals()
            .round_cleared()
            .emit(round_number);
        self.phase = Phase::Intermission;
        self.phase_timer = self.intermission_seconds;
    }
```

Six seconds between rounds. That is the window the player uses to reload, spend
points and reposition — and it is the pacing lever most worth playtesting. Too
short and the game is exhausting; too long and it drags.

### Step 6 — Wire it in `Main`

```rust
#[godot_api]
impl INode3D for Main {
    fn ready(&mut self) {
        let hud = self.hud.clone();
        self.player.bind_mut().bind_hud(hud);

        let target = self.player.clone().upcast::<Node3D>();
        let pool = self.enemy_pool.clone();
        let arena = self.arena.clone();
        self.round_director.bind_mut().begin(target, pool, arena);
```

(The HUD line is Lesson 18.)

In `main.tscn`, add a **`RoundDirector`** node and drag the four curves into its
Inspector slots.

### Step 7 — Play it

Run and survive.

- Three seconds, then round 1: six enemies, arriving 1.5s apart.
- Kill them all; six seconds later, round 2.
- By round 5 or so it should be genuinely busy.

**Play at least five rounds before changing anything.** Then open the curves —
you can edit them while the game runs — and adjust. That loop, playing and
dragging, is the whole reason the curves exist.

Watch for: enemies arriving faster than you can kill them from round 1 (count
curve too steep), or nothing threatening until round 8 (too flat).

---

## Check yourself

1. Why does only one class know the round number?
2. Why `Phase::Idle => {}` instead of a `_ =>` arm?
3. What do `Curve` resources buy over an arithmetic formula? Name three things.
4. Why are curves sampled over 0..1 and clamped at `max_round`?
5. What is the difference between `to_spawn` and `max_active`?
6. How does buying a door widen where enemies spawn, with no code linking the
   two?
7. Why does `find_children_ex` need `owned(false)`?
8. Why does `to_spawn` decrement only when a spawn succeeds?
9. Why is there a block around the `GameState` borrow in `start_round`?

<details>
<summary>Answers</summary>

1. Otherwise it leaks into every system that finds it mildly interesting, and
   changing how rounds work means touching all of them.
2. A wildcard silently swallows phases added later. An explicit arm makes adding
   one a compile error until you have decided what it does.
3. You can see the shape; tuning is dragging rather than recompiling; the shape
   is not limited to what fits in one expression; a non-programmer can edit it.
4. So the curve is independent of how many rounds there are. Clamping makes late
   rounds plateau rather than scale into nonsense.
5. `to_spawn` is the round's budget; `max_active` is a concurrency cap. Owing 34
   with a cap of 32 means the last two arrive as others die.
6. Spawn points are collected only if `is_visible_in_tree()`, and a closed zone is
   hidden. Opening the zone makes its markers visible and therefore eligible.
7. The markers belong to `arena.tscn`, not to the searching node, so the default
   `owned(true)` finds nothing.
8. Otherwise a failed spawn silently reduces the round's enemy count, which is
   invisible from the symptoms.
9. To release the borrow before `EventBus` emits, so a listener that reads
   `GameState` does not panic on a double borrow.

</details>

---

## Extend it

- Add a `Boss` phase every fifth round: one enemy with a large health scale. What
  changes in the enum, and what does the compiler make you handle?
- Make `intermission_seconds` come from a curve too, so late rounds give less
  breathing room.
- Add `EventBus::round_countdown(seconds_left)` emitted each second during the
  intermission, and have the HUD show it in Lesson 18. Players want to know when
  the next wave is coming.
- Weight spawn points by distance from the player, so enemies prefer to arrive
  behind you. Then playtest whether that is fun or merely annoying — the answer
  is not obvious.

---

## Commit

```bash
git add -A
git commit -m "Lesson 15: RoundDirector with curve-driven difficulty scaling"
```

---

**Next:** [Lesson 16 — Economy and GameState](16-economy-and-gamestate.md)
