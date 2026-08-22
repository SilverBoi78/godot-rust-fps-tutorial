# Lesson 9 — Ammo and reload

## What we're building

A magazine, a reserve, and a reload that takes time. Fire thirty rounds and the
gun stops. Press R and it reloads. Fire on empty and it reloads by itself.

The real subject is **state machines**, and this is the first lesson where Rust
is not merely different from GDScript but meaningfully better at the job.

---

## The concept

### The problem with booleans

The obvious way to track a reloading gun:

```rust
is_reloading: bool,
is_firing: bool,
```

Two booleans, four combinations, and **only three of them mean anything**.
`is_reloading && is_firing` is nonsense, and yet nothing stops you writing it.

Add a third state — bolt cycling, jammed, being swapped out — and you have eight
combinations, five of which are nonsense, and you are now writing defensive
checks like `if is_reloading && !is_swapping` scattered across the file. Every one
of those is a chance to forget a case.

### An enum makes the illegal states unrepresentable

```rust
/// The reload cycle as an explicit state machine.
///
/// A pile of booleans (`is_reloading`, `is_firing`) can represent states that
/// cannot actually happen -- reloading AND firing at once. An enum makes those
/// unrepresentable, and the compiler then forces every `match` to handle every
/// case. This is the single most useful thing Rust's type system does for
/// gameplay code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Ready,
    Firing,
    Reloading,
}
```

Three variants, three states, no nonsense representable. And then:

```rust
        match self.state {
            State::Ready | State::Firing => {
                if intent.reload_pressed {
                    self.begin_reload();
                } else if self.wants_to_fire(intent) {
                    self.try_fire();
                }
            }
            State::Reloading => {
                self.reload_remaining -= delta as f32;
                if self.reload_remaining <= 0.0 {
                    self.finish_reload();
                }
            }
        }
```

**The compiler requires that every variant is handled.** Add a `Jammed` variant
later and this stops compiling until you decide what a jammed gun does when you
pull the trigger. GDScript's `match` has no such requirement — a missing case is
a silent fallthrough that you find during a playtest, if you are lucky.

That guarantee is worth the whole cost of writing Godot in Rust, in this
author's opinion, and it applies to enemy AI, round phases, menus and every other
state machine in a game.

### The derives, and why each one

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
```

- **`Debug`** — printing it during debugging. You will want this at 2am.
- **`Clone, Copy`** — it is one byte. Copying is free, and making it `Copy` means
  reading `self.state` does not move it out of `self`.
- **`PartialEq, Eq`** — so you can write `self.state == State::Reloading`.
- **`Default`** with `#[default]` on `Ready` — so `#[class(init)]` can construct
  the whole struct without you writing a constructor.

That last one is a small but real ergonomic win: without `Default`, adding one
enum field forces you to hand-write `init` for the entire class.

### Magazine and reserve

Two numbers, and the distinction matters:

- **Magazine** — loaded, ready to fire. Decrements per shot.
- **Reserve** — carried, not loaded. Only moves during a reload.

```rust
    fn finish_reload(&mut self) {
        let wanted = self.magazine_size - self.in_magazine;
        let taken = wanted.min(self.reserve_ammo);
        self.in_magazine += taken;
        self.reserve_ammo -= taken;
```

`wanted.min(reserve)` is what makes a partial reload work. Reload with 22 in the
magazine and 5 in reserve and you get 27 loaded and 0 reserve — not 30 loaded and
−3 reserve. Integer underflow in an ammo counter is a classic, and this is how it
does not happen.

Note the design choice: rounds left in the magazine are **kept**, not discarded.
Realistic games throw away the partial magazine. That punishes a habit — topping
up between fights — that is otherwise good play, so most arcade shooters keep it.
This one keeps it, which is a design decision you are free to reverse in one line.

### Guarding the reload

```rust
    fn begin_reload(&mut self) {
        if self.state == State::Reloading {
            return;
        }
        if self.in_magazine >= self.magazine_size || self.reserve_ammo <= 0 {
            return;
        }
```

Three refusals, each preventing a specific bad experience:

- **Already reloading** — mashing R would restart the timer forever, and the gun
  would never reload. Extremely annoying, extremely common.
- **Magazine full** — a wasted two seconds for nothing.
- **No reserve** — a reload animation that produces no ammunition.

### Auto-reload on empty

```rust
    fn try_fire(&mut self) {
        if self.shot_cooldown > 0.0 {
            return;
        }

        if self.in_magazine <= 0 {
            // Out of ammo: reload automatically rather than punishing the
            // player for not noticing. Small decision, large effect on feel.
            self.begin_reload();
            return;
        }

        self.fire();
    }
```

Firing on empty starts a reload rather than doing nothing. This is a small
decision with a large effect: without it, running dry mid-fight means a moment of
clicking on nothing while you work out what is wrong.

### Time in a tick, not a timer

The reload counts down inside `tick`:

```rust
            State::Reloading => {
                self.reload_remaining -= delta as f32;
                if self.reload_remaining <= 0.0 {
                    self.finish_reload();
                }
            }
```

rather than using a `Timer` node or a tween. Three reasons:

1. **It is testable.** `weapon.tick(&PlayerIntent::default(), 2.1)` advances the
   reload by 2.1 seconds instantly. The test suite does exactly that. A `Timer`
   would require actually waiting.
2. **It pauses correctly.** When you pause the game the tick stops, and so does
   the reload — automatically, with no extra code.
3. **The state is inspectable.** `reload_remaining` is a number you can print,
   show on a HUD as a progress bar, or serialise into a save.

**Prefer counting down a float in your own tick over scheduling a callback**, for
anything gameplay-relevant. Save timers and tweens for cosmetics — like Lesson 8's
muzzle flash, which nothing needs to test or pause precisely.

### Automatic vs semi-automatic, in one line

```rust
    fn wants_to_fire(&self, intent: &PlayerIntent) -> bool {
        if self.automatic {
            intent.fire_held
        } else {
            intent.fire_pressed
        }
    }
```

Held versus pressed. This is exactly why Lesson 6 put *both* in `PlayerIntent`
even though nothing used `fire_pressed` at the time — the vocabulary was there
when the feature arrived.

---

## Do it

### Step 1 — The enum

At the top of `rust/src/weapon.rs`, above the struct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Ready,
    Firing,
    Reloading,
}
```

### Step 2 — The fields

```rust
    #[export]
    #[init(val = 30)]
    pub magazine_size: i32,
    #[export]
    #[init(val = 270)]
    pub reserve_ammo: i32,
    #[export]
    #[init(val = 400)]
    pub max_reserve: i32,
    #[export(range = (0.2, 5.0, 0.05))]
    #[init(val = 2.0)]
    pub reload_seconds: f32,
```

and the internal state:

```rust
    state: State,
    in_magazine: i32,
    shot_cooldown: f32,
    reload_remaining: f32,
```

`magazine_size`, `reserve_ammo`, `max_reserve` and `reload_seconds` are `pub`
because the test suite reads them and the wall buy in Lesson 17 writes
`reserve_ammo`. `state` and `in_magazine` are private, reached through accessors —
so nothing outside can put the weapon into an inconsistent state.

### Step 3 — The tick

```rust
    /// Driven by `Player` rather than reading `Input` itself -- same rule as
    /// everything else. The weapon has no idea a keyboard exists.
    pub fn tick(&mut self, intent: &PlayerIntent, delta: f64) {
        self.shot_cooldown = (self.shot_cooldown - delta as f32).max(0.0);

        match self.state {
            State::Ready | State::Firing => {
                if intent.reload_pressed {
                    self.begin_reload();
                } else if self.wants_to_fire(intent) {
                    self.try_fire();
                }
            }
            State::Reloading => {
                self.reload_remaining -= delta as f32;
                if self.reload_remaining <= 0.0 {
                    self.finish_reload();
                }
            }
        }
    }
```

Reload is checked **before** firing, so pressing R while holding the trigger
reloads instead of being swallowed.

### Step 4 — Begin and finish

```rust
    fn begin_reload(&mut self) {
        if self.state == State::Reloading {
            return;
        }
        if self.in_magazine >= self.magazine_size || self.reserve_ammo <= 0 {
            return;
        }

        self.state = State::Reloading;
        self.reload_remaining = self.reload_seconds;
        let seconds = self.reload_seconds;
        self.signals().reload_started().emit(seconds);
    }

    fn finish_reload(&mut self) {
        let wanted = self.magazine_size - self.in_magazine;
        let taken = wanted.min(self.reserve_ammo);
        self.in_magazine += taken;
        self.reserve_ammo -= taken;

        self.state = State::Ready;
        self.signals().reload_finished().emit();
        let (mag, reserve) = (self.in_magazine, self.reserve_ammo);
        self.signals().ammo_changed().emit(mag, reserve);
    }
```

The signal lines are Lesson 10 — comment them out if you are following in order.

### Step 5 — The accessors

```rust
    #[func]
    pub fn get_in_magazine(&self) -> i32 {
        self.in_magazine
    }

    #[func]
    pub fn get_reserve(&self) -> i32 {
        self.reserve_ammo
    }

    #[func]
    pub fn is_reloading(&self) -> bool {
        self.state == State::Reloading
    }
```

and the one the wall buy will use in Lesson 17:

```rust
    /// Used by wall buys. Clamped so repeat purchases cannot stack infinitely.
    #[func]
    pub fn add_reserve(&mut self, amount: i32) {
        self.reserve_ammo = (self.reserve_ammo + amount).min(self.max_reserve);
        let (mag, reserve) = (self.in_magazine, self.reserve_ammo);
        self.signals().ammo_changed().emit(mag, reserve);
    }
```

> **A gdext detail worth knowing now.** gdext used to auto-generate `get_x` /
> `set_x` for every `#[export]` field, and those are being phased out — using one
> produces a deprecation warning today. Inside Rust, read the field directly
> (`weapon.bind().magazine_size`). Write explicit `#[func]` accessors only where
> Godot itself, or another class, needs to call them.

### Step 6 — Test it

There is no HUD yet, so print:

```rust
godot_print!("{} / {}", self.in_magazine, self.reserve_ammo);
```

at the end of `fire`, then run and hold the trigger. You should see the count fall
to zero, the gun stop, a two-second pause, and 30/240 come back.

Check each of these deliberately:

- Fire once, press R: reloads, reserve drops by exactly 1.
- Fire once, press R twice fast: reloads once, in the normal time.
- Reload with a full magazine: nothing happens.
- Hold the trigger to empty: reloads automatically.
- Set `reserve_ammo` to `5` in the Inspector and empty the magazine: you get 5
  loaded and 0 reserve, not a negative number.

Remove the print when you are satisfied.

---

## Check yourself

1. Why an `enum` instead of two booleans?
2. What does the compiler do for you here that GDScript's `match` does not?
3. Why does `State` derive `Default`, and what breaks without it?
4. Why is it `wanted.min(self.reserve_ammo)` rather than just `wanted`?
5. Name the three cases `begin_reload` refuses, and the bad experience each
   prevents.
6. Why does the reload count down in `tick` instead of using a `Timer`?
7. Why is reload checked before firing in the `match` arm?
8. What is the difference between `intent.fire_held` and `intent.fire_pressed`,
   and which does a semi-automatic use?

<details>
<summary>Answers</summary>

1. Two booleans can represent states that cannot exist, like reloading and firing
   at once. An enum makes those unrepresentable.
2. It requires every variant to be handled. Adding a state later breaks the build
   until you decide what it does, instead of silently falling through.
3. So `#[class(init)]` can construct the struct without a hand-written
   constructor. Without it you must write `init` yourself.
4. So a partial reserve does not push `reserve_ammo` negative.
5. Already reloading (mashing R would restart the timer forever); magazine
   already full (a wasted two seconds); no reserve (an animation that produces
   nothing).
6. It is testable by passing a large delta, it pauses with the game for free, and
   the remaining time is an inspectable number.
7. So pressing R while holding the trigger reloads rather than being swallowed by
   the fire branch.
8. Held is true for as long as the button is down; pressed is latched for one
   simulation tick after the press. Semi-automatic uses `fire_pressed`.

</details>

---

## Extend it

- Add a `Jammed` state with a small random chance per shot, cleared by pressing R.
  Note that the compiler tells you every place that needs to change. That is the
  lesson, made concrete.
- Play a `audio::click()` at the start and end of the reload. Two lines, and the
  reload stops feeling like a pause.
- Make the reload time depend on whether the magazine was empty — a "tactical"
  reload being faster than one where the bolt has to be cycled. This is a real
  mechanic in several shooters and it is about four lines.
- Make the auto-reload optional behind an `#[export] auto_reload: bool` and play
  with it off. Decide which you prefer, and notice that you now have an opinion.

---

## Commit

```bash
git add -A
git commit -m "Lesson 9: magazine, reserve, and a reload state machine"
```

---

**Next:** [Lesson 10 — Damage and signals](10-damage-and-signals.md)
