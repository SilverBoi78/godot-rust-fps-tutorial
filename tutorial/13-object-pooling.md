# Lesson 13 — Object pooling

## What we're building

Forty-eight enemies, built once at load and reused forever. Spawning stops being
"construct an object" and becomes "flip some booleans on one that already exists."

---

## The concept

### Why instantiation hitches

Creating a node from a `PackedScene` at runtime is not one allocation. It is:

1. Reading the scene resource (cached after the first time, at least).
2. Constructing every node in it — six, for our enemy.
3. Resolving and loading every resource it references.
4. Adding it all to the scene tree, which notifies the physics and render servers.
5. Running `ready` on every node.

All inside one frame. Do it once and nobody notices. Do it eight times in the
same frame — which is exactly what a round start looks like — and you drop a
frame. The player experiences that as a stutter at the precise moment the action
begins, which is the worst possible time.

**The cost is not the memory. It is doing all of that work at the busiest moment.**

Pooling moves the work to load time, where a pause is expected and invisible.

### The pool

Three lists:

```rust
    all: Vec<Gd<Enemy>>,
    available: Vec<Gd<Enemy>>,
    active: Vec<Gd<Enemy>>,
```

- `all` — everything, never modified after prewarm. The owner.
- `available` — dormant, ready to spawn.
- `active` — currently in play.

A `Gd<Enemy>` is a cheap handle, so an enemy appearing in two lists is not two
enemies. `all` exists so the pool has an authoritative record independent of what
is in play — useful for diagnostics, and for `despawn_all`.

`Vec<Gd<T>>` rather than a Godot `Array` because nothing outside Rust touches
these. Same judgement as `PlayerIntent` in Lesson 6: use the engine's collection
types when the engine needs to see the data, and Rust's when it does not.

### Prewarm

```rust
    fn prewarm(&mut self) {
        let Some(scene) = self.enemy_scene.clone() else {
            godot_error!("EnemyPool has no enemy_scene assigned.");
            return;
        };

        let this = self.to_gd();

        for i in 0..self.pool_size {
            let mut enemy = scene.instantiate_as::<Enemy>();
            enemy.set_name(&format!("Enemy{i:02}"));

            self.base_mut().add_child(&enemy);

            enemy
                .signals()
                .despawned()
                .connect_other(&this, EnemyPool::on_despawned);
            enemy.bind_mut().deactivate();

            self.all.push(enemy.clone());
            self.available.push(enemy);
        }

        godot_print!(
            "EnemyPool ready: {} enemies pre-instantiated.",
            self.all.len()
        );
    }
```

`instantiate_as::<Enemy>()` instantiates and casts in one step, panicking if the
scene's root is not an `Enemy`. That is the behaviour you want: a mismatch means
the wrong scene is assigned in the Inspector, and finding out at load beats
finding out mid-round.

The numbered names are worth the two characters. When something goes wrong you
will be reading a scene tree with 48 children in it, and `Enemy07` is far easier
to reason about than 48 nodes called `Enemy`.

The `godot_print!` at the end is a load-time confirmation that the pool exists
and is the size you think. Cheap, and it has caught a mis-set `pool_size` more
than once.

### Activate and deactivate

The contract a pooled object must honour: **everything mutable is reset on
activation.** An object that relies on `ready` for initialisation works exactly
once.

```rust
    pub fn activate(
        &mut self,
        spawn_position: Vector3,
        target: Gd<Node3D>,
        health_scale: f32,
        speed_scale: f32,
    ) {
        self.target = Some(target);
        self.state = State::Chasing;
        self.attack_cooldown = 0.0;
        self.windup_remaining = 0.0;
        self.last_hit_was_headshot = false;

        {
            let mut health = self.health.bind_mut();
            health.max_health = 150.0 * health_scale;
            health.reset();
        }
        self.move_speed = 2.6 * speed_scale;

        self.restore_colour();

        self.body_shape.set_disabled(false);
        self.head_hitbox.set_monitorable(true);

        let mut base = self.base_mut();
        base.set_global_position(spawn_position);
        base.set_velocity(Vector3::ZERO);
        base.set_rotation(Vector3::ZERO);
        base.set_visible(true);
        base.set_collision_layer_value(3, true);
        base.set_process_mode(ProcessMode::INHERIT);
    }
```

Look at how much has to be undone: the rotation from falling over, the health,
the colour from the death flash, the disabled collision shape, the process mode.
Miss one and you get a bug that only appears on an enemy's *second* life, which
is a genuinely nasty thing to track down.

**The `{ }` block around the health borrow is load-bearing.** `bind_mut()` holds
the borrow until the guard drops; without the braces it would live until the end
of the function, and `restore_colour()` — which touches `self` — would be
attempting a second borrow. The compiler catches this one, but the *fix* is worth
recognising on sight.

Note also `let mut base = self.base_mut();` near the end: one guard, five calls.
That is fine because nothing between them touches `self` any other way, and it is
cheaper than five separate `base_mut()` calls.

And going dormant:

```rust
    /// Take this enemy out of play without freeing it. `PROCESS_MODE_DISABLED`
    /// stops `physics_process` entirely, so a dormant enemy costs nothing but
    /// memory.
    #[func]
    pub fn deactivate(&mut self) {
        self.state = State::Dormant;
        self.target = None;
        self.body_shape.set_disabled(true);
        self.head_hitbox.set_monitorable(false);

        let mut base = self.base_mut();
        base.set_visible(false);
        base.set_collision_layer_value(3, false);
        base.set_velocity(Vector3::ZERO);
        // Park it far below the arena so a stray query cannot find it.
        base.set_global_position(Vector3::new(0.0, -100.0, 0.0));
        base.set_process_mode(ProcessMode::DISABLED);
    }
```

Four independent switches, because each closes a different door:

- **`ProcessMode::DISABLED`** — no `physics_process`. This is the big one: a
  dormant enemy costs nothing but memory.
- **Invisible** — nothing to draw.
- **Collision layer cleared, shape disabled** — invisible to rays and bodies.
- **Parked at y = −100** — belt and braces. If any of the above is missed, a
  stray query still cannot reach it.

That last one looks like superstition. It is not: it means a bug in the other
three produces "an enemy is missing" rather than "an invisible enemy is blocking
the doorway", and the first is much easier to diagnose.

### Spawning

```rust
    /// Returns `None` when the pool is exhausted. The caller is expected to
    /// cope -- silently failing to spawn is much better than a hitch or a
    /// crash, and the RoundDirector simply tries again next tick.
    pub fn spawn(
        &mut self,
        spawn_position: Vector3,
        target: Gd<Node3D>,
        health_scale: f32,
        speed_scale: f32,
    ) -> Option<Gd<Enemy>> {
        let mut enemy = self.available.pop()?;
        self.active.push(enemy.clone());
        enemy
            .bind_mut()
            .activate(spawn_position, target, health_scale, speed_scale);
        Some(enemy)
    }
```

The whole function is six lines because `Option` and `?` do the work. `pop()`
returns `Option<Gd<Enemy>>`; `?` returns `None` from `spawn` if the pool is empty.

**Returning `None` rather than growing the pool is a deliberate design choice.** A
pool that grows on demand is a pool that hitches on demand, which defeats the
point. A fixed cap is also a hard performance guarantee: you know the worst case
because you chose it.

### Recycling

An enemy announces its own return:

```rust
    #[func]
    fn return_to_pool(&mut self) {
        self.deactivate();
        let this = self.to_gd();
        self.signals().despawned().emit(&this);
    }
```

and the pool listens:

```rust
    /// The pool's own handler for every enemy's `despawned` signal.
    #[func]
    fn on_despawned(&mut self, enemy: Gd<Enemy>) {
        self.recycle(enemy);
    }
```

```rust
    fn recycle(&mut self, enemy: Gd<Enemy>) {
        self.active.retain(|e| e != &enemy);
        if !self.available.contains(&enemy) {
            self.available.push(enemy);
        }
    }
```

The enemy signals rather than calling the pool directly, so it needs no reference
to the pool and could live anywhere.

The `contains` guard prevents the same enemy being added twice — a double-add
means two spawns hand out the same enemy, and you get an enemy that appears to be
in two places while actually being in one, moving strangely. It is a *very*
confusing bug, and one line prevents it.

`retain` with `!=` compares `Gd` handles by object identity, which is what you
want.

### Clearing the field

```rust
    /// Used when a round ends or a run resets -- clears the field without freeing.
    #[func]
    pub fn despawn_all(&mut self) {
        for mut enemy in std::mem::take(&mut self.active) {
            enemy.bind_mut().deactivate();
            if !self.available.contains(&enemy) {
                self.available.push(enemy);
            }
        }
    }
```

`std::mem::take` swaps `active` for an empty `Vec` and gives you the old one to
iterate. That sidesteps the obvious problem: you cannot iterate `self.active`
while `recycle` mutates it. GDScript's version of this bug is worked around with
`.duplicate()`; Rust's borrow checker refuses to compile it, which is a small
demonstration of the language earning its keep.

---

## Do it

### Step 1 — The pool class

Create `rust/src/enemy_pool.rs`:

```rust
#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct EnemyPool {
    #[export]
    enemy_scene: Option<Gd<PackedScene>>,
    /// Budget: 32 active target, 48 hard cap.
    #[export]
    #[init(val = 48)]
    pool_size: i32,

    all: Vec<Gd<Enemy>>,
    available: Vec<Gd<Enemy>>,
    active: Vec<Gd<Enemy>>,

    base: Base<Node3D>,
}
```

with prewarm on ready:

```rust
#[godot_api]
impl INode3D for EnemyPool {
    fn ready(&mut self) {
        self.prewarm();
    }
}
```

and two accessors the round director will need:

```rust
    #[func]
    pub fn active_count(&self) -> i32 {
        self.active.len() as i32
    }

    #[func]
    pub fn available_count(&self) -> i32 {
        self.available.len() as i32
    }
```

Add `pub mod enemy_pool;` to `lib.rs`.

### Step 2 — Make the enemy poolable

Add `activate`, `deactivate`, `return_to_pool` and the `despawned` signal to
`Enemy` (all quoted above), and change `on_died`'s tween to call
`return_to_pool` instead of `queue_free`.

Declare the signal:

```rust
    #[signal]
    pub fn despawned(enemy: Gd<Enemy>);
```

A signal carrying the emitter itself is a common shape — the pool needs to know
*which* enemy came back, and there is no other way for it to find out.

### Step 3 — Wire the scene

In `main.tscn`, add an **`EnemyPool`** node as a child of `Main`. Drag
`enemy.tscn` into its **Enemy Scene** slot. Leave **Pool Size** at 48.

Delete any hand-placed enemies from the arena — the pool owns them all now.

### Step 4 — Prove it

Run. The Output dock says:

```
EnemyPool ready: 48 enemies pre-instantiated.
```

Switch the Scene dock to **Remote** and expand `EnemyPool`. Forty-eight children,
all invisible, all parked at `y = -100`.

Nothing spawns yet — that is Lesson 15. To test now, add a temporary call in
`Main::ready`:

```rust
pool.bind_mut().spawn(Vector3::new(0.0, 0.5, -5.0), player, 1.0, 1.0);
```

Kill it, wait for the fall and the delay, and watch `available_count` return to
48 in the Remote inspector. Spawn again and you get the *same* node reused — check
the name in the Remote tree.

### Step 5 — Measure it

Worth doing once, because "pooling is faster" should be something you have
observed rather than something you were told.

Run with the profiler open (**Debug → Profiler**, then **Start**). Compare:

- Spawning eight enemies from the pool in one frame.
- Instantiating eight from the `PackedScene` in one frame.

The second shows a visible spike in the frame time graph. The first does not.

---

## Check yourself

1. What exactly is expensive about runtime instantiation? Name at least three of
   the steps.
2. Why does the pool keep three lists rather than one?
3. Why is every mutable field reset in `activate` rather than `ready`?
4. Name the four things `deactivate` switches off, and why parking at y = −100 is
   worth doing on top of the other three.
5. Why does `spawn` return `Option` instead of growing the pool?
6. Why does the enemy emit a signal rather than calling the pool directly?
7. What does the `contains` guard in `recycle` prevent, and what would the bug
   look like?
8. What does `std::mem::take` solve in `despawn_all`, and what would GDScript do
   instead?

<details>
<summary>Answers</summary>

1. Loading the scene resource, constructing every node, resolving its resources,
   entering the tree (notifying the physics and render servers), and running
   `ready` on each node — all in one frame.
2. `all` is the authoritative record, `available` and `active` are the working
   sets. Handles are cheap, so an enemy in two lists is still one enemy.
3. A pooled object is never re-constructed, so `ready` runs exactly once. Anything
   initialised there works for the first life only.
4. Process mode, visibility, collision (layer and shape), and position. Parking
   below the map means a bug in the other three produces "missing enemy" rather
   than "invisible enemy blocking a doorway".
5. A pool that grows on demand hitches on demand. A fixed cap is a hard
   performance guarantee.
6. So the enemy needs no reference to the pool and could be owned by anything.
7. A double-add, which would let two spawns hand out the same enemy — it appears
   to be in two places while actually being in one.
8. It swaps the list out so you are not iterating `active` while `recycle`
   mutates it. GDScript works around the same hazard with `.duplicate()`; Rust
   refuses to compile the broken version.

</details>

---

## Extend it

- Pool the impact effects from Lesson 7 — the code deliberately left them
  unpooled. Note that impacts have no natural "despawned" moment the way a death
  does, so you will need a different return mechanism.
- Add `#[func] fn debug_report(&self)` printing the three list sizes, and call it
  from a key binding. You will use it more than you expect.
- Deliberately remove one line from `activate` — say `health.reset()` — and spawn,
  kill, and respawn. That "works once" bug is the one this lesson exists to
  prevent, and meeting it deliberately is worth five minutes.
- Make `pool_size` too small (say 4) and run a round. Confirm the game degrades
  gracefully rather than crashing. Graceful degradation under a resource cap is a
  property worth testing on purpose.

---

## Commit

```bash
git add -A
git commit -m "Lesson 13: enemy object pool with activate/deactivate lifecycle"
```

---

**Next:** [Lesson 14 — Autoloads and the EventBus](14-autoloads-and-eventbus.md)
