# Lesson 16 — Economy and GameState

## What we're building

Points. Killing things pays; buying things costs. And a clean line between "state
that belongs to this run" and "state that outlives it", drawn now while it is
free.

This is the shortest lesson in Part 4 and the one with the most durable idea in
it.

---

## The concept

### Two kinds of state

Every game with progression has two:

| | Run state | Persistent state |
|---|---|---|
| Lifetime | one attempt | forever |
| Examples | points, round, kills | unlocks, high scores, settings |
| On death | wiped | kept |
| Written by | gameplay code, constantly | one summary, at the end |

The mistake is letting them be the same object. It is a very easy mistake,
because at the start they look identical — both are "numbers about the player".

The consequences arrive later and all at once:

- **Save files become fragile**, because they contain half a live run.
- **Resetting a run means remembering which fields to clear** rather than
  constructing a fresh object.
- **There is nowhere to validate.** In a game with any online component, the
  boundary between "the run said so" and "the profile records it" is exactly
  where anti-cheat goes. No boundary, nowhere to put it.

So: `GameState` holds run state and nothing else. Persistent state does not exist
in this project yet, and the seam is drawn anyway:

```rust
/// The plain-Rust summary that would cross into persistent storage.
///
/// A normal struct, not a Godot class: nothing outside Rust needs to touch it,
/// so there is no reason to pay for registration. Reach for `#[derive(GodotClass)]`
/// when Godot has to see the type -- not by reflex.
#[derive(Debug, Clone, Copy)]
pub struct RunSummary {
    pub rounds_survived: i32,
    pub kills: i32,
    pub headshots: i32,
    pub shots_fired: i32,
    pub accuracy: f32,
    pub seconds: f64,
}
```

One value crosses the boundary, once, at the end of a run. When a profile system
arrives it consumes a `RunSummary` and never sees `GameState` at all.

`RunSummary` is a plain struct for the same reason `PlayerIntent` was. That is
the third time this decision has come up, and it is deliberate — the pull toward
making everything a `GodotClass` is strong and mostly wrong.

### The state itself

```rust
#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct GameState {
    #[var]
    pub points: i32,
    #[var]
    pub round_number: i32,
    #[var]
    pub kills: i32,
    #[var]
    pub headshots: i32,
    #[var]
    pub shots_fired: i32,
    #[var]
    pub run_seconds: f64,

    run_active: bool,

    base: Base<Node>,
}
```

`#[var]`, not `#[export]`. The distinction from Lesson 2, now with a concrete
reason: these are visible to Godot (so a debug overlay or a GDScript tool can
read them) but **not** shown in the Inspector — because there is nothing to
hand-tune. An Inspector field for `kills` would be an invitation to set a
starting value, which is meaningless.

`run_active` is fully private. Nothing outside needs it.

### Two functions, two different signatures, on purpose

```rust
    /// Takes everything as arguments and returns nothing, the same way
    /// `Health::apply_damage` does. If this ever becomes a host-authoritative
    /// call in co-op, the signature does not have to change.
    #[func]
    pub fn award_points(&mut self, amount: i32, reason: GString) {
        if amount <= 0 {
            return;
        }
        self.points += amount;
        EventBus::singleton()
            .signals()
            .points_awarded()
            .emit(amount, &reason);
        EventBus::singleton()
            .signals()
            .points_changed()
            .emit(self.points);
    }
```

```rust
    /// Returns whether the purchase went through. This one DOES return a value,
    /// because the caller genuinely needs to know -- a door must not open if you
    /// could not afford it. Compare with `award_points`, which nobody needs an
    /// answer from.
    #[func]
    pub fn try_spend(&mut self, cost: i32) -> bool {
        if cost > self.points {
            EventBus::singleton().signals().purchase_failed().emit(cost);
            return false;
        }
        self.points -= cost;
        EventBus::singleton()
            .signals()
            .points_changed()
            .emit(self.points);
        true
    }
```

The asymmetry is the lesson.

`award_points` returns nothing, so no caller can branch on the result and come to
depend on knowing it immediately. If this ever becomes a call that goes to a
host and comes back, nothing breaks. Same discipline as `Health::apply_damage`.

`try_spend` **must** return a value, because a door that opens when you could not
afford it is a real bug and the caller has no other way to find out. Making a
purchase authoritative later means the caller has to wait for an answer, and
that is a change worth making consciously rather than discovering.

`try_` is the standard prefix for "this can fail and returns whether it did".
Naming it `spend` would let a caller ignore the result and never notice.

Note that a *failed* purchase emits `purchase_failed`. That is what lets the HUD
say "NEED 750" without the door knowing a HUD exists.

### The `reason` argument

```rust
    #[signal]
    pub fn points_awarded(amount: i32, reason: GString);
```

`award_points(60, "kill".into())`. The reason is carried but never branched on —
it exists so a listener can do something with it. A floating "+60" in a different
colour for a headshot, a statistics breakdown at the end of a run, an
achievement.

`GString` is Godot's string type. `"kill".into()` converts from `&str`, and you
will write that conversion constantly at the Rust/Godot boundary. Note that
`GString` does *not* implement `From<String>` — only `From<&str>` — so a
`format!` result needs `.as_str().into()`. It is a small wart you meet once.

### Timing the run

```rust
#[godot_api]
impl INode for GameState {
    fn process(&mut self, delta: f64) {
        if self.run_active {
            self.run_seconds += delta;
        }
    }
}
```

An autoload with a `process` method, which is exactly why an autoload is a node
rather than a bare object.

`run_seconds` is `f64`, not `f32`. Accumulating a small delta into a float
thousands of times loses precision, and at `f32` a run of a few hours would visibly
drift. This is the one place in the project where the wider type is worth it.

### The pricing

| Event | Points |
|---|---|
| Hit an enemy | +10 |
| Kill | +60 |
| Headshot kill | +100 |
| Door | −750 |
| Ammo refill | −500 |

Two things worth noticing.

**Hits pay, not just kills.** Ten points per hit means contribution is rewarded
rather than the last bullet. That matters enormously the moment there is a second
player, and it also makes the economy less swingy against high-health enemies.

**A door costs about twelve kills.** That ratio is the actual pacing control: too
cheap and the map opens before the player has learned it; too expensive and they
are stuck in one room being bored. Adjust it by playing, not by reasoning.

---

## Do it

### Step 1 — The class

Create `rust/src/game_state.rs` and add `pub mod game_state;` to `lib.rs`. The
struct and the two spending functions are quoted above.

Run lifecycle:

```rust
    #[func]
    pub fn start_run(&mut self) {
        self.points = 0;
        self.round_number = 0;
        self.kills = 0;
        self.headshots = 0;
        self.shots_fired = 0;
        self.run_seconds = 0.0;
        self.run_active = true;
        EventBus::singleton().signals().points_changed().emit(0);
    }
```

Explicitly resetting every field beats constructing a new object, because the
autoload's identity has to survive — every listener connected to `EventBus` at
startup stays connected. It is also the same "reset everything mutable" contract
as the pooled enemy in Lesson 13, and the same failure mode if you miss a field.

The rest:

```rust
    #[func]
    pub fn can_afford(&self, cost: i32) -> bool {
        self.points >= cost
    }

    #[func]
    pub fn record_kill(&mut self, is_headshot: bool) {
        self.kills += 1;
        if is_headshot {
            self.headshots += 1;
        }
    }
```

and, in a **plain** `impl` block rather than the `#[godot_api]` one, the
singleton accessor and the summary:

```rust
impl GameState {
    pub fn singleton() -> Gd<GameState> {
        autoload("GameState")
    }

    pub fn build_run_summary(&self) -> RunSummary {
        RunSummary {
            rounds_survived: self.round_number,
            kills: self.kills,
            headshots: self.headshots,
            shots_fired: self.shots_fired,
            accuracy: self.kills as f32 / (self.shots_fired.max(1) as f32),
            seconds: self.run_seconds,
        }
    }
}
```

`shots_fired.max(1)` avoids dividing by zero on a run where nothing was fired.
The alternative — a `NaN` accuracy — propagates silently through every later
calculation and shows up as `NaN%` on a results screen.

`build_run_summary` has to live outside `#[godot_api]` because `RunSummary` is
not a Godot type and cannot cross that boundary. `singleton()` sits with it and
uses the same `autoload()` helper as `EventBus`.

### Step 2 — Award points from the enemy

Uncomment the lines you stubbed in Lesson 12:

```rust
        GameState::singleton()
            .bind_mut()
            .award_points(self.points_on_hit, "hit".into());
```

and on death:

```rust
        let headshot = self.last_hit_was_headshot;
        let award = if headshot {
            self.points_on_headshot_kill
        } else {
            self.points_on_kill
        };
        {
            let mut state = GameState::singleton();
            let mut state = state.bind_mut();
            state.record_kill(headshot);
            state.award_points(award, "kill".into());
        }
```

The `{ }` block again. Two operations on one borrow, released before the
`EventBus` emit that follows. Third time this pattern has appeared; it should be
starting to look normal.

### Step 3 — Count shots

In `Weapon::fire`:

```rust
        GameState::singleton().bind_mut().shots_fired += 1;
```

Direct field access rather than a setter, because the field is `pub` and gdext's
auto-generated setters are deprecated. Inside Rust, touch the field.

### Step 4 — Start and end a run

`RoundDirector::begin` starts it:

```rust
        let mut state = GameState::singleton();
        state.bind_mut().start_run();
        state.bind_mut().round_number = 0;
```

and `Main` ends it:

```rust
    fn on_player_died(&mut self) {
        let mut state = GameState::singleton();
        let summary = state.bind().build_run_summary();
        state.bind_mut().end_run();

        godot_print!(
            "Run over. Rounds: {}  Kills: {}  Accuracy: {:.0}%",
            summary.rounds_survived,
            summary.kills,
            summary.accuracy * 100.0
        );
    }
```

Note the two separate borrows: `bind()` to read the summary, then `bind_mut()` to
end the run. Combining them into one `bind_mut()` would work here, but keeping
the read immutable documents that `build_run_summary` does not change anything.

Lesson 18 puts this on screen. For now it prints.

### Step 5 — Play it

Run, and kill things.

- Each hit is +10, each kill +60, each headshot kill +100.
- Watch the numbers in the **Remote** scene tree: select `GameState` under `root`.
- Die and check the summary line in Output.

Add a temporary `godot_print!` in `award_points` if you want to see it live —
there is still no HUD.

### Step 6 — Check the accounting

Worth doing deliberately, because economy bugs are silent:

- Kill one enemy with body shots only: you should have exactly
  `10 × hits + 60`. With 150 health and 26 damage, that is 6 hits: 60 + 60 = 120.
- Headshot kill: 3 hits at 65 damage, so 30 + 100 = 130.
- Set `points` to 100 in the Remote inspector and try to buy a 750 door (once
  Lesson 17 exists). Nothing happens, and `points` is still 100.

That last check is the one that matters. A `try_spend` that deducts before
checking is a classic bug and it is invisible until someone goes negative.

---

## Check yourself

1. What is the difference between run state and persistent state, and what breaks
   if you merge them?
2. Why is `RunSummary` a plain Rust struct?
3. Why does `award_points` return nothing while `try_spend` returns a `bool`?
4. Why `#[var]` rather than `#[export]` on `points`?
5. What is the `reason` argument for, given nothing branches on it?
6. Why is `run_seconds` an `f64`?
7. Why does `start_run` reset fields instead of constructing a fresh object?
8. Why `shots_fired.max(1)` in the accuracy calculation?
9. Why do hits award points rather than only kills?

<details>
<summary>Answers</summary>

1. Run state lasts one attempt; persistent state outlives it. Merged, save files
   contain half a live run, resetting means remembering which fields to clear,
   and there is nowhere to put validation.
2. Nothing outside Rust touches it, so registering it would be pure cost.
3. Nobody needs an answer from `award_points`, so keeping it void means it could
   become a remote call unchanged. A door genuinely must know whether the
   purchase succeeded.
4. `#[var]` exposes it to Godot without putting it in the Inspector — there is
   nothing to hand-tune, and an editable `kills` field would be meaningless.
5. So listeners can react differently — a coloured pop-up for a headshot, a
   breakdown at the end of a run — without `GameState` knowing about them.
6. Accumulating a small delta thousands of times loses `f32` precision, and a
   long run would visibly drift.
7. The autoload's identity must survive, because every listener connected at
   startup stays connected to that object.
8. To avoid dividing by zero and producing a `NaN` that propagates silently.
9. It rewards contribution rather than the last bullet, which matters as soon as
   there are two players, and it makes the economy less swingy.

</details>

---

## Extend it

- Add a combo multiplier: consecutive kills within two seconds scale the award.
  Where does the timer live — `GameState`, or something that listens to it?
  Argue for one.
- Add a `Profile` autoload that saves a high score to `user://profile.cfg`, fed
  only by `RunSummary`. Resist letting anything else write to it; that discipline
  is the whole point of the seam.
- Emit `points_awarded` with a world position so a HUD can float "+60" at the
  place the kill happened. Note this changes the signal signature, and the
  compiler lists every emitter and listener for you.
- Write a test that fires `try_spend` at a boundary: exactly enough, one short,
  one over. The reference suite has these; write yours before looking.

---

## Commit

```bash
git add -A
git commit -m "Lesson 16: points economy and run-scoped GameState"
```

---

**Next:** [Lesson 17 — Interaction and inheritance](17-interaction.md)
