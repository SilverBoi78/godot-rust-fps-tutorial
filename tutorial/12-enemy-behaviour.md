# Lesson 12 — Enemy behaviour

## What we're building

The enemy becomes a threat: it chases you, stops at swinging range, winds up,
swings, and hurts you if you are still there. It flashes when hit, dies when its
health runs out, and registers headshots.

Everything is a state machine again — the same enum-and-`match` shape as the
weapon, applied to something with four states instead of three.

---

## The concept

### The state machine

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Dormant,
    Chasing,
    Attacking,
    Dying,
}
```

| State | What it does | Leaves when |
|---|---|---|
| `Dormant` | nothing at all | the pool activates it |
| `Chasing` | paths toward the target | inside attack range |
| `Attacking` | faces the target, swings on a timer | target moves out of range |
| `Dying` | falls over, then returns to the pool | never — the pool resets it |

`Dormant` exists because of Lesson 13: pooled enemies are never destroyed, only
parked. Designing for that now costs nothing; retrofitting it means auditing
every field for whether it survives reuse.

Gravity applies in every state, which is why it sits outside the `match`:

```rust
    fn physics_process(&mut self, delta: f64) {
        if !self.base().is_on_floor() {
            let g = self.gravity * delta as f32;
            let mut v = self.base().get_velocity();
            v.y -= g;
            self.base_mut().set_velocity(v);
        } else {
            let mut v = self.base().get_velocity();
            v.y = 0.0;
            self.base_mut().set_velocity(v);
        }

        match self.state {
            State::Chasing => self.tick_chase(delta),
            State::Attacking => self.tick_attack(delta),
            State::Dying | State::Dormant => {
                let mut v = self.base().get_velocity();
                v.x = 0.0;
                v.z = 0.0;
                self.base_mut().set_velocity(v);
            }
        }

        self.base_mut().move_and_slide();
    }
```

A dying enemy keeps falling but stops moving horizontally, which is why the
velocity is zeroed in that arm rather than the whole body being frozen.

### Wind-up, and why it matters

The attack is not instant. There is a delay between the swing starting and the
damage landing:

```rust
        self.attack_cooldown -= delta as f32;
        if self.attack_cooldown <= 0.0 {
            self.attack_cooldown = self.attack_interval;
            self.windup_remaining = self.attack_windup;
            self.flash(Color::from_rgb(0.9, 0.75, 0.3));
        }
```

0.35 seconds, with a colour flash to telegraph it. That window is what turns
"getting hit" into "getting hit because you did not move", which is the entire
difference between a fair melee enemy and an unfair one.

And it only works if backing off actually avoids the hit:

```rust
    fn land_hit(&mut self) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !target.is_instance_valid() {
            return;
        }

        // Re-check range at the moment the blow lands, so backing off during
        // the windup actually avoids it. Without this the swing is undodgeable
        // and the windup is decoration.
        let mut to_target = target.get_global_position() - self.base().get_global_position();
        to_target.y = 0.0;
        if to_target.length() > self.attack_range * 1.3 {
            return;
        }
```

**Without that re-check the wind-up is decoration.** The telegraph promises the
player they can react; the range check is what keeps the promise.

The `× 1.3` is deliberate slack. Exactly `attack_range` would make a hit land or
miss based on a millimetre, which reads as random rather than as skill.

### Hysteresis, so the enemy does not stutter

Enter attack range at `attack_range`; leave it at `attack_range * 1.25`:

```rust
        if to_target.length() > self.attack_range * 1.25 {
            self.state = State::Chasing;
            return;
        }
```

Using the same threshold both ways makes an enemy standing right at the boundary
flip between chasing and attacking every tick, twitching in place. Different
thresholds for entering and leaving a state is called **hysteresis**, and it is
the fix for stuttering state machines everywhere.

### Turning smoothly

```rust
    fn face(&mut self, direction: Vector3, delta: f64) {
        let wanted = (-direction.x).atan2(-direction.z);
        let weight = 1.0 - (-self.turn_speed * delta as f32).exp();
        let mut rotation = self.base().get_rotation();
        rotation.y = lerp_angle(rotation.y, wanted, weight);
        self.base_mut().set_rotation(rotation);
    }
```

`atan2(-x, -z)` converts a direction into a Y rotation, with the negations
accounting for Godot's forward being −Z.

The weight is Lesson 5's frame-rate independent smoothing again.

`lerp_angle` is the interesting one, and gdext does not give it to you:

```rust
/// Godot's `lerp_angle`, which takes the shorter way around the circle.
fn lerp_angle(from: f32, to: f32, weight: f32) -> f32 {
    let difference = (to - from) % std::f32::consts::TAU;
    let distance = (2.0 * difference) % std::f32::consts::TAU - difference;
    from + distance * weight
}
```

A plain `lerp` between 350° and 10° goes the long way round — 340 degrees of
spin — instead of the 20° that is actually between them. `lerp_angle` normalises
the difference so it always takes the short route. Any time something rotates
dramatically the wrong way to reach a nearby angle, this is why.

Writing small maths helpers yourself is normal in gdext. `godot::global` has many
of GDScript's built-ins, but not all, and a five-line function is cheaper than
hunting for one.

### Per-instance materials, for the third time

```rust
    fn ready(&mut self) {
        // Per-instance material, so flashing one enemy doesn't flash all 48.
        if let Some(active) = self.mesh.get_active_material(0) {
            if let Ok(std_mat) = active.try_cast::<StandardMaterial3D>() {
                let copy = std_mat.duplicate_resource();
                self.mesh.set_surface_override_material(0, &copy);
                self.head_mesh.set_surface_override_material(0, &copy);
                self.base_color = copy.get_albedo();
                self.material = Some(copy);
            }
        }
```

Same trap as Lesson 3's `BoxMesh` and Lesson 10's dummy, now with 48 instances.
Miss it and hitting one enemy flashes the entire horde red.

There is a real cost here: 48 materials instead of 1 means 48 draw calls that
cannot be batched. For a greybox prototype that is fine. The alternative — a
shader with a per-instance uniform — is the right answer at scale and is well
beyond this lesson.

### Headshots and the ordering problem

The head is an `Area3D` in the `headshot` group. The weapon checks
`collider.is_in_group("headshot")`, so any node can be a critical zone without the
weapon knowing anything about enemy types.

Scoring the kill correctly needs the headshot flag *at the moment of death*, and
death happens inside `apply_damage`. So the weapon tells the enemy first:

```rust
    /// Called by the weapon just before applying damage, so the death handler
    /// knows whether the killing blow was a headshot.
    #[func]
    pub fn note_incoming_hit(&mut self, is_headshot: bool) {
        self.last_hit_was_headshot = is_headshot;
    }
```

As Lesson 10 said: this is a workaround for the signal not carrying the
information. A `DamageInfo` struct passed to `apply_damage` would be cleaner, and
it is the best refactor available in this project.

### Dying without freeing

```rust
    fn on_died(&mut self, killer: Option<Gd<Node>>) {
        if self.state == State::Dying {
            return;
        }
        self.state = State::Dying;

        self.base_mut().set_collision_layer_value(3, false);
        self.head_hitbox.set_monitorable(false);
```

The guard makes death idempotent — two shots in the same frame cannot award
points twice. `Health`'s own `dead` flag already prevents the second `died`
signal; this is a second line of defence, and in this kind of code that is
proportionate.

Clearing the collision layer immediately means bullets stop hitting the corpse
during its fall, so ammunition is not wasted on a dead enemy.

Then a tween tips it over and hands it back to the pool:

```rust
        let callback = Callable::from_object_method(&self.to_gd(), "return_to_pool");
        let target = self.to_gd();
        let mut tween: Gd<Tween> = self.base_mut().create_tween();
        tween
            .tween_property(&target, "rotation_degrees:x", &(-85.0).to_variant(), 0.35)
            .set_trans(TransitionType::CUBIC)
            .set_ease(EaseType::IN);
        tween.tween_interval(0.6);
        tween.tween_callback(&callback);
```

`queue_free()` is conspicuously absent. Lesson 13 explains why.

### The double-borrow hazard, for real this time

This lesson is where `bind_mut()` discipline starts to matter. Consider the chain
when you shoot an enemy in the head:

1. `Weapon::shoot_ray` calls `enemy.bind_mut().note_incoming_hit(true)` —
   borrow taken and **released** at the end of the statement.
2. It calls `health.bind_mut().apply_damage(...)` — borrows `Health`.
3. Inside that, `died` is emitted, which synchronously calls
   `Enemy::on_died(&mut self)` — borrows `Enemy`.

That works because `Health` and `Enemy` are different objects, and because the
`Enemy` borrow from step 1 was dropped before step 2. Had it been held — say by
writing both calls as one expression — step 3 would panic.

The habit that keeps this safe: **release borrows before calling into anything
that might call back.** In practice that means one `bind_mut()` per statement,
and an explicit block when you need several operations on one object:

```rust
        {
            let mut health = self.health.bind_mut();
            health.max_health = 150.0 * health_scale;
            health.reset();
        }
```

The braces are not style. They are where the borrow ends.

---

## Do it

### Step 1 — The tunables

Create `rust/src/enemy.rs`:

```rust
    #[export]
    #[init(val = 2.6)]
    move_speed: f32,
    #[export]
    #[init(val = 9.0)]
    turn_speed: f32,
    /// Custom accessor names avoid shadowing `CharacterBody3D::get_gravity()`.
    #[export]
    #[var(get = get_gravity_strength, set = set_gravity_strength)]
    #[init(val = 26.0)]
    gravity: f32,
    /// Stop closing once inside this range and start swinging.
    #[export]
    #[init(val = 1.9)]
    attack_range: f32,
    /// Give up the current path and repath if the target has moved this far.
    #[export]
    #[init(val = 1.2)]
    repath_threshold: f32,
```

> **The `gravity` shadowing problem, and the fix.** `#[export] gravity` normally
> generates `get_gravity` / `set_gravity`, and `CharacterBody3D` already has a
> `get_gravity()`. gdext warns today —
> *"Method `Enemy::get_gravity` shadows a method of a base class"* — and will
> reject it in v0.6. Naming the accessors explicitly keeps the Inspector property
> called `gravity` while the generated methods do not collide. `Player` has the
> same fix for the same reason.

Combat and scoring:

```rust
    #[export]
    #[init(val = 18.0)]
    attack_damage: f32,
    #[export]
    #[init(val = 1.1)]
    attack_interval: f32,
    /// Delay between starting a swing and the damage landing, so it can be dodged.
    #[export]
    #[init(val = 0.35)]
    attack_windup: f32,

    #[export]
    #[init(val = 10)]
    points_on_hit: i32,
    #[export]
    #[init(val = 60)]
    points_on_kill: i32,
    #[export]
    #[init(val = 100)]
    points_on_headshot_kill: i32,
```

Points for *hits*, not just kills, is a deliberate design choice: it rewards
contribution rather than the last bullet, which matters enormously the moment
there is a second player.

### Step 2 — Node handles and state

```rust
    #[init(node = "Health")]
    pub health: OnReady<Gd<Health>>,
    #[init(node = "NavigationAgent3D")]
    pub agent: OnReady<Gd<NavigationAgent3D>>,
    #[init(node = "HeadHitbox")]
    head_hitbox: OnReady<Gd<Area3D>>,
    #[init(node = "CollisionShape3D")]
    body_shape: OnReady<Gd<CollisionShape3D>>,
    #[init(node = "Body")]
    mesh: OnReady<Gd<MeshInstance3D>>,
    #[init(node = "HeadHitbox/Head")]
    head_mesh: OnReady<Gd<MeshInstance3D>>,

    state: State,
    target: Option<Gd<Node3D>>,
    attack_cooldown: f32,
    windup_remaining: f32,
    last_target_position: Vector3,
    material: Option<Gd<StandardMaterial3D>>,
    base_color: Color,
    last_hit_was_headshot: bool,
```

### Step 3 — Connect health in `ready`

```rust
        let this = self.to_gd();
        self.health
            .signals()
            .damaged()
            .connect_other(&this, Enemy::on_damaged);
        self.health
            .signals()
            .died()
            .connect_other(&this, Enemy::on_died);

        self.deactivate();
```

`deactivate()` at the end of `ready` means an enemy starts dormant. Lesson 13's
pool relies on that; it also means dropping one into the arena by hand does
nothing until something activates it, which is worth knowing before you spend ten
minutes wondering why.

### Step 4 — The attack

```rust
    fn tick_attack(&mut self, delta: f64) {
        let Some(target) = self.target.clone() else {
            self.state = State::Chasing;
            return;
        };
        if !target.is_instance_valid() {
            self.state = State::Chasing;
            return;
        }

        let mut to_target = target.get_global_position() - self.base().get_global_position();
        to_target.y = 0.0;

        if to_target.length() > self.attack_range * 1.25 {
            self.state = State::Chasing;
            return;
        }

        self.face(to_target.normalized(), delta);

        if self.windup_remaining > 0.0 {
            self.windup_remaining -= delta as f32;
            if self.windup_remaining <= 0.0 {
                self.land_hit();
            }
            return;
        }
```

and landing it:

```rust
        if let Some(mut health) = find_health(&target.clone().upcast::<Node>()) {
            let source = self.to_gd().upcast::<Node>();
            health
                .bind_mut()
                .apply_damage(self.attack_damage, Some(source));
        }
```

`find_health` is the same free function the weapon uses — one lookup pattern,
used by both the thing shooting and the thing punching.

### Step 5 — Damage reactions

```rust
    fn on_damaged(&mut self, amount: f32, _current: f32, source: Option<Gd<Node>>) {
        self.flash(Color::from_rgb(1.0, 0.4, 0.35));

        let this = self.to_gd().upcast::<Node3D>();
        let headshot = self.last_hit_was_headshot;
        EventBus::singleton().signals().enemy_damaged().emit(
            &this,
            amount,
            headshot,
            source.as_ref(),
        );

        GameState::singleton()
            .bind_mut()
            .award_points(self.points_on_hit, "hit".into());
    }
```

`EventBus` and `GameState` are Lessons 14 and 16 — comment those out for now.

The flash itself:

```rust
    fn flash(&mut self, colour: Color) {
        let Some(material) = self.material.clone() else {
            return;
        };
        let mut material = material;
        material.set_albedo(colour);

        let base_color = self.base_color;
        let mut tween = self.base_mut().create_tween();
        tween.tween_property(&material, "albedo_color", &base_color.to_variant(), 0.18);
    }
```

The tween animates the *material's* property, not the node's. Tweens work on any
Godot object, which is more useful than it first appears.

### Step 6 — Test it

Set the enemy's target to the player and run. Check each behaviour on purpose:

- It paths around cover to reach you.
- It stops just outside arm's length rather than pushing into you.
- It flashes amber, then hits you 0.35s later.
- **Back away during the amber flash and the hit misses.** If it lands anyway,
  your `land_hit` range re-check is missing.
- Shot in the body: red flash, 26 damage. In the head: 65.
- It falls over on death and stops taking hits during the fall.
- Standing exactly at attack range does not make it twitch.

---

## Check yourself

1. Why is gravity applied outside the `match`?
2. What does the attack wind-up achieve, and what makes it meaningful rather than
   decorative?
3. What is hysteresis here, and what happens without it?
4. Why is `lerp_angle` needed instead of a plain lerp on rotation?
5. Why duplicate the material, and what does it cost?
6. Why is the head an `Area3D` in a group rather than a specially-named node?
7. Trace the borrows when a headshot kills an enemy. Why does none of it panic?
8. Why does `on_died` return early if the state is already `Dying`?
9. Why does `#[export] gravity` need explicit accessor names?

<details>
<summary>Answers</summary>

1. It applies in every state — a dying or dormant enemy still falls.
2. It gives the player time to react. It is only meaningful because `land_hit`
   re-checks range at the moment of impact; without that the hit is undodgeable.
3. Entering attack range at one distance and leaving at a larger one. Without it,
   an enemy at the boundary flips state every tick and twitches.
4. A plain lerp from 350° to 10° travels 340° the wrong way. `lerp_angle`
   normalises the difference to take the short route.
5. So flashing one enemy does not flash all 48. It costs batching — 48 materials
   means 48 draw calls.
6. So the weapon can ask "is this a critical zone" without knowing anything about
   enemy types, and any node can be one.
7. The `bind_mut()` on the enemy is released at the end of its statement, before
   `Health` is borrowed. `Health` and `Enemy` are different objects, so the
   re-entrant `on_died` borrow is a fresh one.
8. To make death idempotent — two hits in the same frame must not award points
   twice or return the enemy to the pool twice.
9. The generated `get_gravity` would shadow `CharacterBody3D::get_gravity()`,
   which gdext warns about now and rejects in v0.6.

</details>

---

## Extend it

- Add a `Stunned` state entered on a headshot that does not kill: frozen for
  0.4s, no attacks. The compiler will list every place that needs updating.
- Make `attack_windup` scale down as rounds progress, so late enemies telegraph
  less. Where should that number come from — the enemy, or the round director?
  (Lesson 15 has an opinion.)
- Add a lunge: during the wind-up, move slightly toward the target. Now the
  dodge window has to account for closing distance, which is a real design
  problem.
- Comment out `duplicate_resource()`, spawn ten enemies, and shoot one. Restore
  it. That is the shared-resource trap, seen rather than read about.

---

## Commit

```bash
git add -A
git commit -m "Lesson 12: enemy state machine, wind-up attacks, headshots, death"
```

---

**Next:** [Lesson 13 — Object pooling](13-object-pooling.md)
