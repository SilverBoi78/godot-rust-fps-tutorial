# Lesson 11 — Navigation

## What we're building

A navigation mesh baked over the arena, and an enemy that finds its way to you
around cover instead of walking into it.

This lesson also contains a gdext-specific obstacle that will stop you dead if
nobody warns you: **Godot's navigation classes are marked experimental, and gdext
hides experimental APIs behind a feature flag.**

---

## The concept

### Why not just walk toward the player

```rust
let direction = (player_position - my_position).normalized();
```

This works beautifully in an empty room and is useless anywhere else. Put a wall
between the enemy and the player and it walks into the wall and stays there,
pressing forward forever.

You need **pathfinding**: a route through the walkable space that goes around
obstacles.

### The navigation mesh

A navmesh is a simplified description of where a character can stand — a set of
flat polygons covering the floor, shrunk back from walls by the character's
radius, excluding anything too steep to climb or too low to fit under.

Godot builds one by scanning your geometry:

| Setting | Ours | What it means |
|---|---|---|
| `agent_radius` | 0.5 | Shrink the mesh half a metre from every wall |
| `agent_height` | 1.75 | Ignore gaps shorter than this |
| `agent_max_climb` | 0.5 | Steps up to this tall are walkable |
| `cell_size` | 0.25 | Resolution of the scan |
| `geometry_collision_mask` | 1 (World) | Only World-layer geometry counts |

`agent_radius` is the one to get right. Too small and enemies clip corners and
snag on walls. Too large and they refuse to fit through your doorway. Our enemies
have a 0.36m capsule radius, so 0.5 leaves a margin.

`geometry_collision_mask = 1` means the navmesh is built from World-layer
collision only — floors, walls and cover. Without it, the *player's* capsule and
every enemy would be baked into the mesh as obstacles, which would be both wrong
and different every time you baked.

### Baking at load, in code

You normally bake in the editor with a button, and for a static map that is
enough. This map is not static: a door physically blocks the north doorway and
carves a hole in the navmesh. Once it sinks out of the way in Lesson 17, the mesh
has to be rebuilt or enemies keep treating the doorway as a wall.

So we bake at load and re-bake when a zone opens:

```rust
#[godot_api]
impl INavigationRegion3D for Arena {
    fn ready(&mut self) {
        self.base_mut()
            .bake_navigation_mesh_ex()
            .on_thread(false)
            .done();
```

Baking at load rather than shipping a pre-baked `.tres` also means the mesh can
never go stale relative to the geometry — a real class of bug where a level is
edited and someone forgets to re-bake.

**`bake_navigation_mesh_ex()`** is gdext's builder form for a method with optional
arguments. Godot methods with defaults get an `_ex()` variant returning a builder;
set what you want and call `.done()`. The plain `bake_navigation_mesh()` uses all
defaults.

`on_thread(false)` bakes synchronously. This map is small enough that a
synchronous bake is a blip, and it avoids agents querying a half-built mesh. Bake
on a thread for a large map, and accept a moment where the old mesh is still in
use.

### `NavigationAgent3D`

Give the enemy an agent, tell the agent where you want to go, and ask it for the
next point along the route:

```rust
        // Only repath when the target has actually moved. Setting
        // `target_position` every frame forces a full path recalculation every
        // frame, which is the usual reason navigation shows up as a CPU spike.
        if target_position.distance_to(self.last_target_position) > self.repath_threshold {
            self.last_target_position = target_position;
            self.agent.set_target_position(target_position);
        }

        if self.agent.is_navigation_finished() {
            return;
        }

        // The NEXT point along the path, not the destination. Using the
        // destination directly makes enemies walk into walls.
        let next_point = self.agent.get_next_path_position();
```

Three gotchas live in those nine lines, and every one of them is a real
performance or behaviour bug people hit:

**Setting `target_position` every frame** re-runs the pathfinder every frame. With
32 agents that is 32 full path computations per tick, and it is the usual reason
navigation shows up as a spike in the profiler. Only repath when the target has
actually moved — 1.2 metres here.

**Using the destination as your movement direction** is the single most common
navigation mistake. `get_next_path_position()` returns the next corner of the
route; the destination is where you eventually want to be. Steer toward the
destination and you walk into the wall between you and it — that is exactly the
problem the navmesh was supposed to solve.

**Not checking `is_navigation_finished()`** means asking for a next point when
there is no path — because the target is unreachable, or you have arrived.

### The timing trap

The navigation map syncs on its own schedule, once per physics frame, on the
server. Query it the frame after a bake and you get nothing.

This makes for a genuinely confusing bug: your code is correct, and it does not
work for the first few frames. The test suite deals with it explicitly:

```rust
    // The navigation map syncs on its own schedule; querying it the frame after
    // a bake returns nothing. Give it a few ticks.
    for _ in 0..5 {
        next_physics_frame(&tree).await;
    }
```

Same reason the door's re-bake waits 0.8 seconds — the door needs ~0.6s to animate
out of the way, and then the mesh needs a moment to settle.

### The gdext obstacle

Try to use it and you get:

```
error[E0432]: unresolved import `godot::classes::NavigationAgent3D`
  |     use godot::classes::{..., NavigationAgent3D};
  |                               ^^^^^^^^^^^^^^^^^ no `NavigationAgent3D` in `classes`
```

The class exists in Godot 4.7.2 — you can add one in the editor right now. It is
missing from the Rust bindings because **Godot marks its entire navigation module
as experimental**, and gdext refuses to generate bindings for experimental APIs
unless you opt in:

```toml
godot = { version = "0.5.5", features = ["api-4-7", "experimental-godot-api"] }
```

Lesson 0 turned this on already, precisely so you would not meet the error here.
It is worth knowing why the flag is there:

- gdext is telling you these APIs may change or vanish between Godot versions.
- For navigation specifically, the flag has been on for a long time and the API
  has been stable in practice.
- The affected list includes `NavigationAgent3D`, `NavigationRegion3D`,
  `NavigationServer3D`, `NavigationMesh`, `GraphEdit`, `Parallax2D` and a handful
  of XR classes.

If you ever see "no `X` in `classes`" for a class you can plainly see in the
editor, this feature flag is the first thing to check.

### Avoidance

```
avoidance_enabled = true
radius = 0.42
max_speed = 4.0
```

RVO avoidance makes agents steer around *each other*, not just around walls.
Without it, 32 enemies converging on you become one enemy-shaped column. With it
they spread into a rough crowd.

`radius` here is the *avoidance* radius, unrelated to the navmesh's `agent_radius`
— slightly larger than the physical capsule so they keep a little personal space.

---

## Do it

### Step 1 — Confirm the feature flag

```toml
godot = { version = "0.5.5", features = ["api-4-7", "experimental-godot-api"] }
```

If you have to add it now, the next `cargo build` regenerates the bindings and
takes a minute.

### Step 2 — Make `NavRegion` a navigation region

Open `arena.tscn`. Select the `NavRegion` node you added in Lesson 3 and change
its type (**right-click → Change Type**) to **`NavigationRegion3D`**.

In the Inspector, **NavigationMesh → New NavigationMesh**, then click it and set:

| Property | Value |
|---|---|
| Geometry → Parsed Geometry Type | `Static Colliders` |
| Geometry → Collision Mask | `World` only |
| Cell → Size | `0.25` |
| Cell → Height | `0.25` |
| Agents → Height | `1.75` |
| Agents → Radius | `0.5` |
| Agents → Max Climb | `0.5` |

Click **Bake NavigationMesh** in the toolbar. A blue translucent surface appears
over the floor, shrunk back from every wall and hollowed around each cover block.

**Look at it carefully.** If it does not cover somewhere the enemies need to
reach, they will never go there and you will spend an hour debugging the enemy
instead of the mesh. Common causes: geometry not on the World layer, or a gap
narrower than `2 × agent_radius`.

### Step 3 — The `Arena` class

Create `rust/src/arena.rs`:

```rust
#[derive(GodotClass)]
#[class(base=NavigationRegion3D, init)]
pub struct Arena {
    /// The door animates out of the way over ~0.6s; wait for it to clear before
    /// re-reading the geometry.
    #[export]
    #[init(val = 0.8)]
    rebake_delay: f64,

    base: Base<NavigationRegion3D>,
}
```

and the re-bake, which needs a delay:

```rust
#[godot_api]
impl Arena {
    /// GDScript would `await` a timer here. Rust has no `await` for Godot
    /// signals in a plain method, so we create the timer and connect its
    /// `timeout` to a `#[func]` -- which is exactly what `await` compiles down
    /// to anyway.
    #[func]
    fn rebake(&mut self) {
        // `on_thread = false`: this map is small enough that a synchronous bake
        // is a blip, and it avoids agents querying a half-built mesh. Bake on a
        // thread for a large map, and accept a moment where the old mesh is
        // still in use.
        self.base_mut()
            .bake_navigation_mesh_ex()
            .on_thread(false)
            .done();
    }
}

impl Arena {
    fn on_zone_opened(&mut self, _zone_name: GString) {
        let delay = self.rebake_delay;
        let callback = Callable::from_object_method(&self.to_gd(), "rebake");
        let mut timer: Gd<SceneTreeTimer> = self.base().get_tree().create_timer(delay);
        timer.connect("timeout", &callback);
    }
}
```

The `zone_opened` connection is Lesson 14 — for now just bake in `ready`.

> **Where GDScript is nicer.** GDScript writes
> `await get_tree().create_timer(0.8).timeout` — one line, and execution resumes
> where it left off. Rust has no equivalent inside an ordinary method, so a
> deferred action becomes a timer plus a named `#[func]` callback. gdext *does*
> have `godot::task::spawn` for async blocks (the test suite uses it), but it is
> heavier machinery than this needs. This is a genuine ergonomic loss and it is
> only fair to say so.

Change `NavRegion`'s type again, to **`Arena`**.

### Step 4 — Give the enemy an agent

The enemy class arrives properly in Lesson 12; for now, build the scene.

New scene, root **`CharacterBody3D`** named `Enemy`, layer **Enemy**, mask
**World**. Children:

```
Enemy
├── Body                  (MeshInstance3D)  CapsuleMesh r=0.36 h=1.7, pos (0, 0.85, 0)
│                                           material greybox_enemy.tres
├── CollisionShape3D      CapsuleShape3D r=0.36 h=1.7, pos (0, 0.85, 0)
├── Health                (Health)  max_health 150
├── HeadHitbox            (Area3D)  layer EnemyHitbox, mask empty, monitoring OFF
│   │                               pos (0, 1.85, 0), group "headshot"
│   ├── Head              (MeshInstance3D)  SphereMesh r=0.23
│   └── CollisionShape3D  SphereShape3D r=0.23
└── NavigationAgent3D     (NavigationAgent3D)
```

On the agent:

| Property | Value |
|---|---|
| Path Desired Distance | `0.6` |
| Target Desired Distance | `1.0` |
| Path Max Distance | `3.0` |
| Avoidance → Enabled | on |
| Avoidance → Radius | `0.42` |
| Avoidance → Max Speed | `4.0` |

Save as `res://scenes/enemy.tscn`.

**Path Desired Distance** is how close the agent must get to a path corner before
advancing to the next one. Too small and the agent oscillates around corners; too
large and it cuts them and clips walls.

**Target Desired Distance** is how close counts as arrived — 1.0 pairs with an
attack range of 1.9 so the enemy stops just outside swinging distance.

### Step 5 — Move along the path

The chase logic, from `reference/rust/src/enemy.rs`:

```rust
    fn tick_chase(&mut self, delta: f64) {
        let Some(target) = self.target.clone() else {
            return;
        };
        if !target.is_instance_valid() {
            return;
        }

        let position = self.base().get_global_position();
        let target_position = target.get_global_position();
        let mut to_target = target_position - position;
        to_target.y = 0.0;

        if to_target.length() <= self.attack_range {
            self.state = State::Attacking;
            let mut v = self.base().get_velocity();
            v.x = 0.0;
            v.z = 0.0;
            self.base_mut().set_velocity(v);
            return;
        }
```

and, after the repath block quoted earlier:

```rust
        let next_point = self.agent.get_next_path_position();
        let mut direction = next_point - position;
        direction.y = 0.0;

        if direction.length_squared() < 0.0001 {
            return;
        }

        direction = direction.normalized();
        let speed = self.move_speed;
        let mut v = self.base().get_velocity();
        v.x = direction.x * speed;
        v.z = direction.z * speed;
        self.base_mut().set_velocity(v);
        self.face(direction, delta);
    }
```

`direction.y = 0.0` keeps the enemy from trying to walk up into the air when the
next path point is on a raised platform. Gravity handles vertical movement;
navigation is a ground-plane problem.

`is_instance_valid()` guards against the target having been freed since we stored
the handle. `Gd<T>` does not keep a `Node` alive — nodes are manually freed, so a
handle can outlive its object. Using a dead one is a crash, and this check is the
defence.

### Step 6 — Watch it path

Add an enemy to `main.tscn`, and temporarily set its target in `ready` to the
player. Run, and stand behind a cover block.

**Turn on the visual debugger**: with the game running, **Debug → Visible
Navigation** in the editor's top menu. You will see the navmesh and the agent's
current path drawn live. Do this whenever an enemy behaves oddly — it turns a
guessing game into a look.

Things to check:

- The enemy goes *around* cover, not into it.
- It stops about two metres away rather than pressing into you.
- Moving makes it recompute — but only after you have moved about a metre.

If the enemy does not move at all, in order of likelihood: the navmesh is not
baked; the enemy is standing off the mesh; `target` was never set; the first
query happened before the map synced.

---

## Check yourself

1. Why is walking directly toward the player not good enough?
2. What does `agent_radius` do, and what are the symptoms of it being too small
   or too large?
3. Why is `geometry_collision_mask` set to World only?
4. Why bake at load instead of shipping a baked mesh in the scene?
5. Why is `target_position` only set when the target has moved a metre?
6. What is the difference between `get_next_path_position()` and the target
   position, and what happens if you confuse them?
7. Why does the test suite wait five physics frames before querying the map?
8. Why does `NavigationAgent3D` not exist in `godot::classes` by default?
9. Why is `is_instance_valid()` checked on a stored `Gd<Node3D>`?

<details>
<summary>Answers</summary>

1. Any obstacle between the two makes the character walk into it and stay there.
2. It shrinks the walkable mesh back from walls. Too small: enemies clip corners
   and snag. Too large: they will not fit through doorways.
3. So the mesh is built from static geometry only. Otherwise the player and every
   enemy would be baked in as obstacles.
4. Because the geometry changes at runtime when a door opens, and because a baked
   asset can go stale relative to a level someone edited.
5. Setting it every frame re-runs the pathfinder every frame, which with 32
   agents is the usual cause of a navigation CPU spike.
6. `get_next_path_position()` is the next corner of the route; the target is the
   final destination. Steering at the destination walks you into the wall the
   path was meant to avoid.
7. The navigation map syncs on the server's own schedule, so a query immediately
   after a bake returns nothing.
8. Godot marks its navigation classes experimental, and gdext gates experimental
   APIs behind the `experimental-godot-api` feature.
9. Nodes are manually freed, so a `Gd<Node>` handle can outlive its object. Using
   a dead handle crashes.

</details>

---

## Extend it

- Add a `NavigationLink3D` connecting the platform top to the floor, so enemies
  can drop off it as a shortcut. This is how you express traversal a navmesh
  cannot represent.
- Set `avoidance_enabled` to false, spawn ten enemies, and watch them merge into
  one. Turn it back on. Now you know what that setting is worth.
- Profile it: with 32 enemies active, open the profiler (**Debug → Profiler**) and
  compare `set_target_position` every frame against the threshold version. Measure
  the difference rather than trusting this lesson.
- Make `repath_threshold` scale with distance to the target, so far-away enemies
  repath less often than close ones. This is a standard optimisation and it is
  about three lines.

---

## Commit

```bash
git add -A
git commit -m "Lesson 11: navigation mesh, agents, and runtime re-baking"
```

---

**Next:** [Lesson 12 — Enemy behaviour](12-enemy-behaviour.md)
