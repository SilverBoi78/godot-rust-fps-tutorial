# Lesson 7 — Hitscan

## What we're building

A gun that shoots. Left-click casts a ray from the camera, and wherever it lands
you get a small mark that shrinks and disappears.

This lesson also contains the single most common reason a beginner's gun does not
work, which is not a bug in their code: **collision masks**.

---

## The concept

### Hitscan vs projectiles

Two ways to shoot in a game:

- **Hitscan** — cast a ray the instant the trigger is pulled. Whatever it hits is
  hit, immediately.
- **Projectile** — spawn an object that travels, and detect collisions as it
  goes.

Hitscan is what nearly every shooter uses for rifles and pistols. It is cheap
(one query, no ongoing objects), it is exact (no leading targets), and it matches
what a bullet does at human distances anyway. Projectiles are for rockets,
grenades, and anything where travel time is the point.

We are hitscan. A projectile weapon would be a good extension once the rest works.

### The ray comes from the camera, not the muzzle

This one is counter-intuitive and it is not negotiable.

The shot must go where the **crosshair** points. The crosshair is the centre of
the camera's view, so the ray starts at the camera and travels along the camera's
forward axis.

The muzzle is somewhere off to the side and below — it is a *cosmetic* position,
where the flash and smoke appear. Firing from the muzzle means the bullet goes
somewhere slightly different from where you aimed, and at close range against a
doorframe the difference is enormous and infuriating.

Every first-person shooter does it this way. The visual and the simulation are
deliberately not the same thing.

### How a ray query works

```rust
        let from = camera.get_global_position();
        let to = from - camera.get_global_transform().basis.col_c() * self.range_metres;

        let mut query = PhysicsRayQueryParameters3D::create(from, to).unwrap();
        query.set_collision_mask(self.hit_mask);
        // Off by default, and hitboxes are Area3Ds -- forget this and headshots
        // silently never register.
        query.set_collide_with_areas(true);
        query.set_collide_with_bodies(true);
```

**`basis.col_c()`** is the third column of the orientation matrix — the node's
local **+Z** in world space. Forward is −Z, hence `from - ... * range`. In
GDScript this is spelled `basis.z`; gdext names the columns `col_a`, `col_b`,
`col_c` because `Basis` is a matrix type and `x`/`y`/`z` would be ambiguous about
whether you meant a row or a column.

**`create(from, to)` returns an `Option`.** Almost every Godot constructor that
can fail does, in gdext. `.unwrap()` is acceptable here because there is no
failure mode that is not a programming error.

Then you run it:

```rust
let hit = self
    .base()
    .get_world_3d()
    .expect("weapon is not in a 3D world")
    .get_direct_space_state()
    .expect("no space state")
    .intersect_ray(&query);
```

`intersect_ray` returns a `Dictionary` — Godot's untyped map — which is empty on
a miss and otherwise has `collider`, `position`, `normal`, `rid`, `shape` and
`face_index`.

### Untyped data at the boundary

This is the first place Godot's dynamic typing collides with Rust's static
typing, and it is worth handling honestly rather than papering over.

```rust
let Some(collider) = hit
    .get("collider")
    .and_then(|v| v.try_to::<Gd<Node>>().ok())
else {
    return;
};
```

Reading it outward: `get` returns `Option<Variant>` (the key might be absent);
`try_to::<T>()` returns `Result<T, ConvertError>` (the value might be the wrong
type); `.ok()` turns that into an `Option` so `and_then` can chain it; `let
... else` handles the whole thing failing.

That is four lines to express what GDScript writes as `hit.get("collider")`.
GDScript's version is shorter and will hand you `null` at some later point with
no explanation of where it came from. This version cannot.

For the two vectors we take a different route, because a missing normal is not
worth aborting the shot over:

```rust
let point = hit
    .get("position")
    .and_then(|v| v.try_to::<Vector3>().ok())
    .unwrap_or_default();
```

`unwrap_or_default()` gives `Vector3::ZERO`. The judgement call — abort on a
missing collider, shrug at a missing position — is the sort of thing static
typing forces you to make explicit, and it is usually a good thing.

### Collision masks: the lesson within the lesson

A ray only hits objects whose **layer** is in the ray's **mask**. Get it wrong
and the ray passes through everything, returns an empty dictionary, and your gun
does nothing. **No error. No warning. Nothing in the Output dock.**

Our mask:

```rust
/// World | Enemy | EnemyHitbox -- layers 1, 3 and 4.
#[export(flags_3d_physics)]
#[init(val = 0b1101)]
pub hit_mask: u32,
```

`0b1101` is 13: bit 1 (World) + bit 3 (Enemy) + bit 4 (EnemyHitbox). Bit 2 is the
player, deliberately excluded — though we also exclude the player by RID, because
belt and braces is correct here.

`#[export(flags_3d_physics)]` renders it in the Inspector as the same named
checkbox grid you use on nodes, using the layer names from Lesson 3. Without it
you get a bare integer field and no chance of noticing a mistake.

**And the one that catches everyone:**

```rust
query.set_collide_with_areas(true);
```

`collide_with_areas` defaults to **false**. Headshot hitboxes are `Area3D`s. Miss
this line and headshots never register — body shots work perfectly, heads do
nothing, and there is no diagnostic of any kind. Lesson 12 depends on this being
right, and the test suite checks it explicitly for exactly this reason.

### Excluding the shooter

Without this, a ray starting inside the player's own capsule hits the player and
travels no distance at all:

```rust
if let Some(body) = self.owner_body.clone() {
    // `get_rid` lives on CollisionObject3D, not Node3D -- the exclude
    // list is a list of physics objects, not of nodes.
    if let Ok(collider) = body.try_cast::<CollisionObject3D>() {
        query.set_exclude(&array![collider.get_rid()]);
    }
}
```

An **RID** is Godot's opaque handle to a resource inside a server — here, the
physics body. The exclude list is RIDs rather than nodes because the physics
server does not know what a node is.

### Dependency injection, not node-path guessing

The weapon needs the camera. Two ways to get it:

<!-- illustrative -->
```rust
// Fragile: breaks silently if the scene is rearranged.
let camera = self.base().get_node_as::<Camera3D>("../../Camera3D");

// Robust: the player hands it over.
pub fn setup(&mut self, camera: Gd<Camera3D>, owner_body: Gd<Node3D>) {
    self.camera = Some(camera);
    self.owner_body = Some(owner_body);
}
```

The second is better for a reason that has nothing to do with Rust: `"../../"`
encodes an assumption about scene structure inside a file that has no way to
check it. Move the weapon one level and it breaks at runtime with a null.

We use `Option<Gd<Camera3D>>` rather than `OnReady` precisely *because* it is not
available at `ready` time — it arrives when the player calls `setup`. That is
what `Option` is for, and the error path is explicit:

```rust
let Some(camera) = self.camera.clone() else {
    godot_error!("Weapon has no camera. Did Player call setup()?");
    return;
};
```

An error message that names the fix beats one that names the symptom.

---

## Do it

### Step 1 — The impact effect

Create `rust/src/impact.rs`:

```rust
use godot::classes::MeshInstance3D;
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Impact {
    #[export]
    #[init(val = 0.9)]
    lifetime: f32,

    #[init(node = "MeshInstance3D")]
    mesh: OnReady<Gd<MeshInstance3D>>,

    elapsed: f32,
    start_scale: Vector3,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Impact {
    fn ready(&mut self) {
        self.start_scale = self.mesh.get_scale();
    }

    fn process(&mut self, delta: f64) {
        self.elapsed += delta as f32;
        let t = self.elapsed / self.lifetime;

        if t >= 1.0 {
            // `queue_free`, never `free`. Deleting a node in the middle of a
            // frame that other code is still walking is how you get very
            // confusing crashes.
            self.base_mut().queue_free();
            return;
        }

        let scale = self.start_scale * (1.0 - t);
        self.mesh.set_scale(scale);
    }
}
```

Build, then make the scene: new scene, root of type **`Impact`**, with a
`MeshInstance3D` child (named exactly that) holding a **New SphereMesh** —
radius `0.045`, height `0.09`, **Radial Segments** `8`, **Rings** `4`, with a new
**StandardMaterial3D** in dark grey `(0.08, 0.07, 0.07)`. Set the mesh instance's
**Scale** to `(1, 0.3, 1)` so it reads as a flattened splat.

Save as `res://scenes/impact.tscn`.

> **`queue_free` vs `free`.** `free` deletes immediately, which is a disaster if
> anything is mid-iteration over the tree. `queue_free` defers until the end of
> the frame. Use `queue_free`. Always.

### Step 2 — The weapon scene

New scene, root of type **`Weapon`** — which does not exist yet, so create the
scene with a plain `Node3D` root for now and change its type after step 3.

Children:

```
Weapon                (Node3D -> Weapon)
├── Viewmodel         (Node3D)  pos (0.28, -0.22, -0.42)
│   ├── Body          (MeshInstance3D)  BoxMesh (0.09, 0.11, 0.52), greybox_accent
│   ├── Barrel        (MeshInstance3D)  BoxMesh (0.035, 0.035, 0.34), pos (0, 0.015, -0.4)
│   └── Grip          (MeshInstance3D)  BoxMesh (0.07, 0.16, 0.09), pos (0, -0.12, 0.13), rot x -15
├── Muzzle            (Marker3D)  pos (0.28, -0.205, -0.98)
│   └── Flash         (OmniLight3D)  colour (1, 0.83, 0.55), energy 6, range 5
└── Audio             (AudioStreamPlayer3D)  unit size 14
```

Save as `res://scenes/weapon.tscn`.

The viewmodel sits down and to the right because that is where a gun is when you
are holding it. `Viewmodel` is a separate node from `Weapon` so that Lesson 8 can
punch it backwards on each shot without moving the muzzle or the ray origin.

Instance `weapon.tscn` into `player.tscn` as a child of
`Head/CameraRig/Camera3D` — so the gun follows the camera exactly, including bob
and recoil.

### Step 3 — The weapon class, first pass

Create `rust/src/weapon.rs`. This lesson writes the firing half; Lessons 8 and 9
fill in feel, ammo and reloading. The finished file is
`reference/rust/src/weapon.rs`, and the pieces below are copied from it.

The ray:

```rust
    fn shoot_ray(&mut self) {
        let Some(camera) = self.camera.clone() else {
            godot_error!("Weapon has no camera. Did Player call setup()?");
            return;
        };

        // Rays are cast from the CAMERA, not the muzzle: the shot has to go
        // where the crosshair points. The muzzle is only where the flash is.
        let from = camera.get_global_position();
        let to = from - camera.get_global_transform().basis.col_c() * self.range_metres;

        let mut query = PhysicsRayQueryParameters3D::create(from, to).unwrap();
        query.set_collision_mask(self.hit_mask);
        // Off by default, and hitboxes are Area3Ds -- forget this and headshots
        // silently never register.
        query.set_collide_with_areas(true);
        query.set_collide_with_bodies(true);
        if let Some(body) = self.owner_body.clone() {
            // `get_rid` lives on CollisionObject3D, not Node3D -- the exclude
            // list is a list of physics objects, not of nodes.
            if let Ok(collider) = body.try_cast::<CollisionObject3D>() {
                query.set_exclude(&array![collider.get_rid()]);
            }
        }

        let hit = self
            .base()
            .get_world_3d()
            .expect("weapon is not in a 3D world")
            .get_direct_space_state()
            .expect("no space state")
            .intersect_ray(&query);

        if hit.is_empty() {
            return;
        }
```

Reading the result:

```rust
        let Some(collider) = hit
            .get("collider")
            .and_then(|v| v.try_to::<Gd<Node>>().ok())
        else {
            return;
        };
        let point = hit
            .get("position")
            .and_then(|v| v.try_to::<Vector3>().ok())
            .unwrap_or_default();
```

(The damage handling that comes next in the reference file belongs to Lesson 10 —
skip it for now and finish with `self.spawn_impact(point, normal);`.)

Spawning the mark:

```rust
    fn spawn_impact(&mut self, point: Vector3, normal: Vector3) {
        // Instantiating mid-combat, which is exactly what the pooling lesson
        // forbids for enemies. It is fine for one small effect and will be
        // pooled later -- left visible here on purpose.
        let Some(mut impact) = self.impact_scene.instantiate() else {
            return;
        };

        self.impact_parent().add_child(&impact);

        let mut impact3d = impact.clone().cast::<Node3D>();
        impact3d.set_global_position(point + normal * 0.02);
        if normal.length_squared() > 0.0 && normal.dot(Vector3::UP).abs() < 0.99 {
            let target = impact3d.get_global_position() + normal;
            impact3d.look_at(target);
        }
        impact.set_name("Impact");
    }
```

and choosing where to put it:

```rust
    fn impact_parent(&self) -> Gd<Node> {
        let tree = self.base().get_tree();
        match tree.get_current_scene() {
            Some(scene) => scene,
            None => tree.get_root().upcast(),
        }
    }
```

The scene is loaded once, at construction:

```rust
    #[init(load = "res://scenes/impact.tscn")]
    impact_scene: OnReady<Gd<PackedScene>>,
```

### Step 4 — Read the new pieces

**`#[init(load = "res://...")]`** — gdext's answer to GDScript's `preload`. It
loads the resource when the object is constructed rather than on first use, so
the disk hit happens at load time instead of mid-combat.

**`point + normal * 0.02`** — push the mark two centimetres out along the surface
normal. Placed exactly on the surface, it fights with the wall for the same
pixels and flickers. That artifact is called **z-fighting**, you will see it
often, and a small offset is the standard fix.

**`normal.dot(Vector3::UP).abs() < 0.99`** — `look_at` needs an "up" reference
and fails when the direction you are aiming at *is* up. On a floor or ceiling the
normal is vertical, so we skip the orientation instead of crashing. This is the
kind of edge case you find by shooting the floor once.

**`impact_parent`** — impacts are parented to the running scene, not to the
weapon. Parent them to the weapon and they follow the gun as you walk, which
looks extremely strange.

`get_current_scene()` returns `None` whenever this scene was not loaded as *the*
main scene — running `player.tscn` directly with F6, or from a headless test. The
fallback to the tree root costs one line and prevents a crash that only shows up
in the test suite. That is not a hypothetical; it is why the fallback is there.

**`impact.clone().cast::<Node3D>()`** — `cast` panics on failure where `try_cast`
returns a `Result`. Use `cast` when a failure means your scene file is wrong and
you want to know immediately; `try_cast` when failure is an expected outcome.

### Step 5 — Wire it up

The player hands the weapon its camera, in `ready`:

```rust
        // Explicit dependency injection. The weapon needs a camera to aim from,
        // and handing it over is far more robust than having the weapon guess
        // at a node path -- rearranging the scene later cannot silently break it.
        let camera = self.camera.clone();
        let body = self.to_gd().upcast::<Node3D>();
        self.weapon.bind_mut().setup(camera.clone(), body.clone());
```

and drives it from `physics_process`, which you added in Lesson 6:

```rust
        self.weapon.bind_mut().tick(&intent, delta);
```

`self.to_gd()` gives you a `Gd<Self>` from inside a method — the handle to your
own object. You will use it constantly from Lesson 10 onward, because connecting
a signal to yourself needs it.

### Step 6 — Test it

Add a temporary target to shoot at: in `main.tscn`, a `StaticBody3D` with a
BoxMesh and BoxShape3D, on layer **World**, a few metres in front of the spawn.

Run. Left-click. Marks appear where you point, and shrink away.

If nothing happens, in order of likelihood:

1. **The mask.** Is `hit_mask` set to include World (bit 1)? Print it.
2. **The target's layer.** Is it on World?
3. **`setup` never ran.** The error message says so — check the Output dock.
4. **The action is not bound.** Test `fire` in the Input Map.

---

## Check yourself

1. Why does the ray start at the camera rather than the muzzle?
2. What is a collision mask, and what is the symptom of getting it wrong?
3. What does `set_collide_with_areas(true)` change, and which feature breaks
   without it?
4. Why is the shooter excluded by RID rather than by node?
5. Why is `camera` an `Option<Gd<Camera3D>>` rather than an `OnReady`?
6. Why is the impact offset along the normal?
7. Why does `impact_parent` have a fallback for a null current scene?
8. What is `basis.col_c()`, and why is the ray `from - col_c * range`?

<details>
<summary>Answers</summary>

1. The shot must go where the crosshair points, and the crosshair is the centre
   of the camera's view. The muzzle is a cosmetic position.
2. The set of layers a query is allowed to hit. Get it wrong and the ray passes
   through everything silently — no error, no warning, the gun just does nothing.
3. Whether the ray can hit `Area3D`s. It defaults to false, and headshot hitboxes
   are areas, so headshots never register without it.
4. The exclude list belongs to the physics server, which deals in RIDs and knows
   nothing about nodes.
5. Because it is not available at `ready` time — it arrives when the player calls
   `setup`. `OnReady` is for things resolvable during `ready`.
6. To avoid z-fighting with the surface it is sitting on.
7. `get_current_scene()` is `None` when the scene was not loaded as the main
   scene — running a sub-scene directly, or in a headless test.
8. The third column of the orientation basis, which is the node's local +Z in
   world space. Forward is −Z, so travelling forward means subtracting it.

</details>

---

## Extend it

- Add `#[export] spread_degrees: f32` and randomise the ray direction slightly
  within that cone. Then make it grow while firing continuously and shrink while
  still — which is most of what "recoil pattern" means in a modern shooter.
- Add a tracer: a thin, long box from muzzle to hit point that fades over 0.05s.
  Note that it should start at the *muzzle* even though the ray started at the
  camera. Why does that look right rather than wrong?
- Make the impact colour depend on what was hit, using a group on the collider.
  This is the first step toward surface-specific effects and sounds.
- Set `set_collide_with_areas(false)` and add an `Area3D` on layer World to shoot
  at. Confirm it is ignored. Knowing what that failure looks like will save you
  an hour in Lesson 12.

---

## Commit

```bash
git add -A
git commit -m "Lesson 7: hitscan weapon with raycasting and impact marks"
```

---

**Next:** [Lesson 8 — Weapon feel](08-weapon-feel.md)
