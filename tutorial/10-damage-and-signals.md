# Lesson 10 — Damage and signals

## What we're building

A `Health` component, a practice target that reacts to being shot, and the wiring
between them — which is Godot's signal system, and gdext's typed version of it.

This is the lesson that makes the rest of the project possible. Every system from
here on communicates through signals.

---

## The concept

### Components, because Rust gives you no choice

Lots of things can be hurt: the player, enemies, destructible props. In an
inheritance language you might reach for a `Damageable` base class.

You cannot here. **gdext classes may extend engine classes only** —
`#[class(base = ...)]` will not accept another `#[derive(GodotClass)]` type. Try
it and the error is `cannot find type 'Health' in module '::godot::classes'`.

So `Health` is a **component**: a child node that anything can have.

```
Player                    Enemy                   TargetDummy
├── CollisionShape3D      ├── CollisionShape3D    ├── CollisionShape3D
├── Health         <--    ├── Health        <--   ├── Health      <--
└── Head                  └── NavigationAgent3D   └── HeadHitbox
```

This restriction is worth being honest about: it is a real limitation, and
Lesson 17 hits a case where it genuinely costs something. Here it does not,
because composition was already the better answer. A player **has** health while
**being** a `CharacterBody3D`. "Has a" wants composition; "is a" wants
inheritance. Godot's own design agrees — that is why `CollisionShape3D` is a
child node rather than a property.

### Finding a component

```rust
/// Walks the collider and its scene owner looking for a `Health` component.
/// A hitbox `Area3D` is usually a grandchild of the thing that owns the health.
pub fn find_health(collider: &Gd<Node>) -> Option<Gd<Health>> {
    let candidates = [Some(collider.clone()), collider.get_owner()];
    for candidate in candidates.into_iter().flatten() {
        for child in candidate.get_children().iter_shared() {
            if let Ok(health) = child.try_cast::<Health>() {
                return Some(health);
            }
        }
    }
    None
}
```

Two candidates, because a ray might hit either the body itself or a hitbox
`Area3D` nested inside it. `get_owner()` returns the root of the scene the node
belongs to — so a head hitbox two levels down still finds its enemy's `Health`.

`try_cast::<Health>()` is how you ask "is this node of my Rust type?" It returns
`Result<Gd<Health>, Gd<Node>>` — success or the original handle back.

`iter_shared()` iterates a Godot `Array` without copying it. Godot arrays are not
Rust `Vec`s; they live on the engine side and are refcounted, so iteration is
explicit about whether you are borrowing or taking.

### Signals

A signal is Godot's built-in observer pattern. An object announces something
happened; anyone interested has registered to be told.

Without signals, `Health` would have to know about the HUD, the score, the sound
system and the particle system — and adding a fifth reaction would mean editing
`Health`. With signals, `Health` announces `damaged` and knows nothing about who
listens.

**The rule that keeps this from becoming spaghetti: signals describe facts that
already happened, never commands.** `died`, not `kill`. A signal named as a
command is a method call wearing a disguise, and it makes the flow of control
impossible to follow.

### gdext's typed signals

GDScript signals are untyped at the call site: emit the wrong argument count and
you find out at runtime. gdext generates a typed API from your declarations.

```rust
#[godot_api]
impl Health {
    #[signal]
    pub fn damaged(amount: f32, current: f32, source: Option<Gd<Node>>);
    #[signal]
    pub fn healed(amount: f32, current: f32);
    #[signal]
    pub fn died(source: Option<Gd<Node>>);
    /// Fires on every change, including the initial one. Ideal for HUD bars.
    #[signal]
    pub fn changed(current: f32, maximum: f32);
```

Note the signature: a semicolon instead of a body, and no `&self`. You are
declaring a signal, not writing a function.

Emitting is type-checked:

```rust
        self.signals().damaged().emit(amount, current, source.as_ref());
```

Connecting is type-checked too, including that the handler's parameters match:

```rust
        self.health
            .signals()
            .damaged()
            .connect_other(&this, Enemy::on_damaged);
```

Three ways to connect:

| Method | Receiver |
|---|---|
| `connect_self(Self::handler)` | the emitting object itself |
| `connect_other(&target, Type::handler)` | another object |
| `connect(Type::handler)` | a free or associated function |

Getting a handler's signature wrong is a compile error naming both signatures.
In GDScript it is a runtime error in a log file, sometimes.

### Two rules that will bite you

**One `signals()` call per signal.** The handle it returns configures a single
signal at a time. This compiles and then panics at runtime:

```rust
let signals = bus.signals();
signals.points_changed().connect_other(&this, Hud::on_points_changed);
signals.round_started().connect_other(&this, Hud::on_round_started);   // panic
```

The message is clear when you see it —
*"signals() allows only one signal configuration at a time"* — but the fix is not
obvious if you have not met it. Call `signals()` fresh each time:

```rust
// One `signals()` call per signal. The handle it returns configures a
// single signal at a time, so holding one in a variable and reusing it
// for seven connections panics at runtime -- it compiles perfectly well.
bus.signals()
    .points_changed()
    .connect_other(&this, Hud::on_points_changed);
bus.signals()
    .round_started()
    .connect_other(&this, Hud::on_round_started);
```

**`self.signals()` borrows `self`.** So arguments must be computed first — the
same hoist as Lesson 4:

```rust
        let (current, max) = (self.current, self.max_health);
        self.signals().damaged().emit(amount, current, source.as_ref());
        self.signals().changed().emit(current, max);
```

### Nullable object arguments

`source: Option<Gd<Node>>` — damage might come from nobody (falling, poison). The
emit side needs a reference:

```rust
self.signals().died().emit(source.as_ref());
```

`Option<Gd<T>>` is passed as `Option<&Gd<T>>`, so `.as_ref()`. For an explicit
none, write `None::<Gd<MyClass>>.as_ref()` — bare `None` cannot be inferred. It is
a small wart; you meet it once and then remember it.

### Writing `apply_damage` as though it were already networked

```rust
    #[func]
    pub fn apply_damage(&mut self, amount: f32, source: Option<Gd<Node>>) {
        if self.dead || self.invulnerable || amount <= 0.0 {
            return;
        }

        self.current = (self.current - amount).max(0.0);

        let (current, max) = (self.current, self.max_health);
        self.signals()
            .damaged()
            .emit(amount, current, source.as_ref());
        self.signals().changed().emit(current, max);

        if self.current <= 0.0 {
            self.dead = true;
            self.signals().died().emit(source.as_ref());
```

Three properties, all deliberate:

1. **Every input is an argument.** Nothing is read from global state.
2. **It returns nothing.** Callers do not get to branch on the result, so they
   cannot come to depend on knowing it immediately.
3. **All effects are announced, not performed.** It changes a number and emits.

Together these make it a function that could be an authoritative call arriving
over a network without its signature changing. Whether or not co-op ever happens,
this is a good shape for damage: one place decides, everyone else reacts.

The `dead` flag exists so `died` fires exactly once. Without it, two shots landing
in the same frame both push health below zero and both emit — you award points
twice, play two death sounds, and return the same enemy to the pool twice.

### The shared-resource trap, again

Flashing a target red means changing its material's colour. But the material is a
`.tres` file shared by every node that uses it, so this flashes all of them:

```rust
        if let Some(active) = self.mesh.get_active_material(0)
            && let Ok(std_mat) = active.try_cast::<StandardMaterial3D>()
        {
            let copy = std_mat.duplicate_resource();
            self.mesh.set_surface_override_material(0, &copy);
            self.head_mesh.set_surface_override_material(0, &copy);
            self.base_color = copy.get_albedo();
            self.material = Some(copy);
        }
```

`duplicate_resource()` gives this node its own copy. Same trap as `BoxMesh` in
Lesson 3, and it will catch you a third time in Lesson 12 with 48 enemies.

Note the `&& let` in the condition. That is a **let-chain**: two fallible
patterns in one `if`, without nesting. The equivalent nested form works
identically —

<!-- illustrative -->
```rust
if let Some(active) = self.mesh.get_active_material(0) {
    if let Ok(std_mat) = active.try_cast::<StandardMaterial3D>() {
        // ...
    }
}
```

— and clippy will tell you to collapse it, which is why the reference build is
written the flat way. You will use this constantly against Godot's API, where
almost every lookup returns an `Option` or a `Result` and two in a row is the
normal case.

Note both meshes get the *same* copy, so the head and body flash together.

---

## Do it

### Step 1 — The `Health` component

Create `rust/src/health.rs` and add `pub mod health;` to `lib.rs`.

```rust
#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct Health {
    #[export]
    #[init(val = 100.0)]
    pub max_health: f32,
    #[export]
    pub invulnerable: bool,

    current: f32,
    dead: bool,

    base: Base<Node>,
}

#[godot_api]
impl INode for Health {
    fn ready(&mut self) {
        self.current = self.max_health;
        let (current, max) = (self.current, self.max_health);
        self.signals().changed().emit(current, max);
    }
}
```

`current` is private with a `get_current()` accessor, so nothing outside can set
health without going through `apply_damage` or `heal` and skipping the signals.

`ready` emits `changed` immediately, which means a HUD bar connecting to it gets
its initial value for free rather than needing a separate "read the current value
once" path. Small, and it removes a whole category of "the bar is empty until I
take damage" bug.

The rest of the file — `apply_damage`, `heal`, the accessors — is quoted above and
complete in `reference/rust/src/health.rs`. One more method, for Lesson 13:

```rust
    /// Used by the object pool to bring a corpse back into service.
    #[func]
    pub fn reset(&mut self) {
        self.current = self.max_health;
        self.dead = false;
        let (current, max) = (self.current, self.max_health);
        self.signals().changed().emit(current, max);
    }
```

### Step 2 — The target dummy

Create `rust/src/target_dummy.rs`.

```rust
#[godot_api]
impl IStaticBody3D for TargetDummy {
    fn ready(&mut self) {
        // Duplicate the material so flashing this dummy doesn't flash every
        // other dummy sharing the same resource. Shared-resource surprises like
        // this are a classic Godot gotcha.
        if let Some(active) = self.mesh.get_active_material(0)
            && let Ok(std_mat) = active.try_cast::<StandardMaterial3D>()
        {
            let copy = std_mat.duplicate_resource();
            self.mesh.set_surface_override_material(0, &copy);
            self.head_mesh.set_surface_override_material(0, &copy);
            self.base_color = copy.get_albedo();
            self.material = Some(copy);
        }

        let this = self.to_gd();
        self.health
            .signals()
            .damaged()
            .connect_other(&this, TargetDummy::on_damaged);
        self.health
            .signals()
            .died()
            .connect_other(&this, TargetDummy::on_died);
        self.health
            .signals()
            .changed()
            .connect_other(&this, TargetDummy::on_changed);
    }
}
```

and the reactions:

```rust
    fn on_damaged(&mut self, amount: f32, current: f32, _source: Option<Gd<Node>>) {
        godot_print!("Target hit for {amount:.0}, {current:.0} left");
        self.flash(Color::from_rgb(1.0, 0.45, 0.3));
    }

    fn on_changed(&mut self, current: f32, maximum: f32) {
        // Darken as it gets hurt, so damage is readable without a health bar.
        let t = current / maximum;
        let hurt = Color::from_rgb(0.25, 0.1, 0.1);
        let colour = self.base_color.lerp(hurt, (1.0 - t) as f64);
        if let Some(material) = &mut self.material {
            material.set_albedo(colour);
        }
    }
```

**Notice what this class does not do.** It never checks its own health in
`process`, and the weapon never calls into it. `Health` announces; this node
reacts. Adding a hitmarker, a sound or a score award later means connecting to the
same signals — not editing this file.

Death tips it over and schedules a respawn:

```rust
    fn on_died(&mut self, _source: Option<Gd<Node>>) {
        godot_print!("Target down.");
        self.base_mut().set_collision_layer_value(3, false);
        self.head_hitbox.set_monitorable(false);

        // Tip it over. `rotation_degrees` is fine for a one-off flourish.
        let target = self.to_gd();
        let seconds = self.respawn_seconds;
        let callback = Callable::from_object_method(&target, "respawn");

        let mut tween = self.base_mut().create_tween();
        tween
            .tween_property(&target, "rotation_degrees:x", &(-82.0).to_variant(), 0.45)
            .set_trans(TransitionType::CUBIC)
            .set_ease(EaseType::IN);
        tween.tween_interval(seconds);
        tween.tween_callback(&callback);
    }
```

`tween_property` animates a property **by name**, and `"rotation_degrees:x"`
targets a single component of it. That colon syntax is Godot-specific and very
useful — `"modulate:a"` for just the alpha, `"position:y"` for just the height.
The name is a string, so as with `Callable`, a typo fails silently.

### Step 3 — The dummy scene

New scene, root of type **`TargetDummy`**:

```
TargetDummy               (StaticBody3D -> TargetDummy), layer Enemy, mask empty
├── Body                  (MeshInstance3D)  CapsuleMesh r=0.36 h=1.7, pos (0, 0.85, 0)
│                                           material greybox_target.tres
├── CollisionShape3D      CapsuleShape3D r=0.36 h=1.7, pos (0, 0.85, 0)
├── Health                (Health)  max_health 150
└── HeadHitbox            (Area3D)  layer EnemyHitbox, mask empty, monitoring OFF
    │                               pos (0, 1.85, 0), group "headshot"
    ├── Head              (MeshInstance3D)  SphereMesh r=0.23, greybox_target.tres
    └── CollisionShape3D  SphereShape3D r=0.23
```

Save as `res://scenes/target_dummy.tscn` and place two or three in the arena.

Three things to get right on `HeadHitbox`:

- **Layer `EnemyHitbox`, mask empty.** It is hit *by* rays; it detects nothing.
- **Monitoring off.** It never needs to know about overlaps, and monitoring costs
  work every tick. `monitorable` — whether *others* can detect it — stays on.
- **Group `headshot`.** Set in the Inspector's **Node → Groups** tab. The weapon
  asks `collider.is_in_group("headshot")` rather than checking a node name, so
  any node can be a headshot zone without the weapon knowing its type.

### Step 4 — Connect the weapon

In `shoot_ray`, after extracting the hit:

```rust
        let is_headshot = collider.is_in_group("headshot");

        if let Some(mut health) = find_health(&collider) {
            // Tell the target the hit is coming BEFORE applying damage, so its
            // death handler knows whether the killing blow was a headshot.
            if let Some(parent) = health.get_parent()
                && let Ok(mut enemy) = parent.try_cast::<Enemy>()
            {
                enemy.bind_mut().note_incoming_hit(is_headshot);
            }

            let multiplier = if is_headshot {
                self.headshot_multiplier
            } else {
                1.0
            };
            let source = self.owner_body.clone().map(|b| b.upcast::<Node>());
            health
                .bind_mut()
                .apply_damage(self.damage * multiplier, source);

            self.signals().hit_confirmed().emit(is_headshot);
        }
```

The `Enemy` part belongs to Lesson 12 — skip it for now.

`note_incoming_hit` happens **before** `apply_damage`, and the ordering is
load-bearing. `apply_damage` may emit `died` synchronously, and the death handler
needs to know whether the killing blow was a headshot in order to award the right
points. Tell it first, then hit it.

> **This is a smell, and it is worth naming.** Passing context through a
> side-channel because the signal does not carry it is a workaround. The clean
> fix is a `DamageInfo` struct as the argument to `apply_damage` — amount, source,
> and whether it was a critical hit, all in one value. That is a good extension,
> and the reason it is not the tutorial's version is that it is easier to see why
> you want it after you have felt the alternative.

### Step 5 — Test it

Build, run, and shoot a dummy. Output shows:

```
Target hit for 26, 124 left
Target hit for 26, 98 left
```

Shoot the head. `65` per hit — 26 × 2.5.

**If body shots register but headshots do not**, you have found the
`collide_with_areas` trap from Lesson 7. Go and check that line.

### Step 6 — A test suite

This project has one, and now is when it starts earning its keep. It is a scene
with a Rust node that loads the game, drives it, and checks the results:

```rust
    let intent = PlayerIntent {
        fire_held: true,
        fire_pressed: true,
        ..Default::default()
    };
    weapon.bind_mut().tick(&intent, 1.0 / 60.0);
    next_physics_frame(&tree).await;

    let body_damage = before - enemy_health.bind().get_current();
```

This is possible only because of choices made three lessons ago. `PlayerIntent` is
a plain struct you can construct in a line, and `tick` takes it as an argument
instead of reading `Input`. Had the weapon polled `Input` directly, testing it
would require synthesising OS input events.

**Decoupling is not an aesthetic preference. It is what makes verification
possible.**

Run it:

```bash
godot4 --headless --path reference/godot res://tests/tests.tscn
```

The full runner is `reference/rust/src/tests.rs`. It uses `godot::task::spawn`
and `Signal::to_future()` to `await` frames, which is gdext's equivalent of
GDScript's `await get_tree().process_frame`.

---

## Check yourself

1. Why is `Health` a child node rather than a base class? Give both the Rust
   reason and the design reason.
2. What is the rule for naming signals, and why does it matter?
3. What does `connect_other` check that GDScript's `connect` does not?
4. What happens if you store `bus.signals()` in a variable and use it for three
   connections?
5. Why does `apply_damage` return nothing?
6. Why does `Health` have a `dead` flag when `current <= 0.0` already tells you?
7. Why does `find_health` check `get_owner()` as well as the collider itself?
8. Why call `duplicate_resource()` on the material?
9. Why is `note_incoming_hit` called before `apply_damage`, and what would be
   cleaner?

<details>
<summary>Answers</summary>

1. Rust reason: gdext classes can only extend engine classes, so there is no
   shared base available. Design reason: a player *has* health while *being* a
   `CharacterBody3D` — "has a" wants composition.
2. They describe facts that already happened, never commands. A command-shaped
   signal is a disguised method call and makes control flow untraceable.
3. That the handler's parameter types match the signal's declaration, at compile
   time.
4. It panics at runtime on the second one: the handle configures one signal at a
   time. Call `signals()` fresh each time.
5. So callers cannot branch on the result and come to depend on knowing it
   immediately — which is what would break if the call became remote.
6. So `died` fires exactly once. Two hits in one frame would otherwise both push
   health below zero and both emit.
7. A ray may hit a hitbox `Area3D` nested inside the thing that owns the health.
   `get_owner()` finds the scene root from there.
8. Otherwise every node using that `.tres` shares one material, and flashing one
   target flashes all of them.
9. Because `apply_damage` can emit `died` synchronously, and the death handler
   needs the headshot flag to award points. Cleaner: pass a `DamageInfo` struct
   carrying amount, source and criticality as one argument.

</details>

---

## Extend it

- Replace the `note_incoming_hit` side-channel with a `DamageInfo` struct. This is
  the best exercise in the lesson: it touches the weapon, `Health`, and every
  handler, and the compiler walks you through all of it.
- Add a `damage_over_time` component that calls `apply_damage` on a tick. Notice
  it needs no changes to `Health` at all — that is the payoff of components.
- Give `Health` an `armour` field that reduces incoming damage, and decide
  whether armour should be part of `Health` or its own component. Argue both
  sides before choosing.
- Make the target's death sound use `audio::impact()`. Then make the pitch depend
  on whether it was a headshot.

---

## Commit

```bash
git add -A
git commit -m "Lesson 10: Health component, typed signals, target dummy"
```

---

**Next:** [Lesson 11 — Navigation](11-navigation.md)
