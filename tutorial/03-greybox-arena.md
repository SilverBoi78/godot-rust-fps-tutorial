# Lesson 3 — The greybox arena

## What we're building

The map: a walled room with a raised platform and scattered cover, plus a second
area behind a doorway that stays closed until Lesson 17. All of it boxes.

Along the way, a `Wall` class that rebuilds its own mesh and collision shape when
you resize it in the Inspector — which introduces `#[class(tool)]`, the attribute
that makes Rust code run *inside the editor*.

---

## The concept

### Greyboxing

A greybox is a level built entirely from untextured primitives. No art, no
detail, just the shapes that determine how the space plays.

You greybox because level layout and level art are separate problems with
separate feedback loops, and mixing them is expensive. Deciding a corridor is two
metres too narrow is a thirty-second fix in a greybox and a day's work once it
has been modelled and textured. Every studio does this, and the ones that do not
regret it.

This project stays greybox permanently, which is a deliberate scope decision: it
keeps the tutorial about mechanics and keeps other people's art out of the
repository.

### Making a greybox readable

Grey boxes all look the same, which is exactly the problem. Three rules do most
of the work:

1. **Colour by function, not by fantasy.** Floors one colour, walls another,
   cover another, anything interactive a colour nothing else uses. The player
   learns "orange means I can use it" in about four seconds.
2. **Keep scale honest.** A human is about 1.8m. Doorways ~2.2m, cover ~1.2m
   (chest height for crouching behind), walls 4m so they read as enclosing rather
   than as a fence. Get scale wrong in the greybox and every asset made later is
   wrong too.
3. **Give the eye something to measure against.** A completely flat room has no
   depth cues. A raised platform, a few crates, and variation in wall length make
   distances readable.

### Collision layers and masks

This is the concept most likely to cost you an evening later, so it gets taught
properly now.

Every physics object has two 32-bit numbers:

- **Layer** — "what I am."
- **Mask** — "what I collide with / detect."

They are independent. A collides with B if **A's mask includes B's layer**. This
is not symmetric, and that asymmetry is the whole point: a player can collide
with a wall while the wall does not have to care about players.

Godot lets you name the bits, and naming them is not optional if you want to stay
sane. Ours:

| Bit | Name | What lives there |
|---|---|---|
| 1 | World | Floors, walls, cover — static geometry |
| 2 | Player | The player body |
| 3 | Enemy | Enemy bodies |
| 4 | EnemyHitbox | Headshot areas |
| 5 | Interactable | Doors, wall buys |

And the assignments this project uses:

| Object | Layer | Mask |
|---|---|---|
| Floor / wall / cover | World | *nothing* |
| Player | Player | World |
| Enemy | Enemy | World |
| Head hitbox | EnemyHitbox | *nothing* |
| Door / wall buy | World + Interactable | *nothing* |

Read the empty masks carefully — they are the interesting part. A wall's mask is
empty because a wall never initiates a collision; it is *collided with*. Giving
static geometry a mask is a very common beginner mistake and it costs performance
for no benefit.

The head hitbox is an `Area3D` on its own layer with an empty mask because it
never detects anything. It exists purely to be *hit by a ray* in Lesson 7.

> **The single biggest source of "my gun doesn't shoot anything" is a mask.** A
> ray with the wrong mask silently passes through the world and returns nothing.
> There is no error and no warning. Lesson 7 comes back to this with the specific
> numbers.

### `#[class(tool)]` — Rust that runs in the editor

By default your code only runs when the game runs. `#[class(tool)]` makes the
class run inside the editor too, so a change in the Inspector can take effect
immediately.

We use it for `Wall`, which builds its own box mesh and collision shape from a
`size` field. Resize a wall and it rebuilds while you watch.

Two warnings that come with the territory:

- **Tool code runs in the editor process.** A panic takes the editor down with
  it, unsaved work included. Keep tool code boring and defensive.
- **`ready` runs in the editor too**, so anything that assumes a running game —
  `get_tree().get_current_scene()`, input, autoloads — needs guarding.

### The shared-resource trap

This one catches everybody, at least once.

If you make a `BoxMesh` inside `wall.tscn` and save it, **every instance of that
scene shares the same mesh object.** Resize one wall and all of them resize,
because there is one mesh and they all point at it.

That is often what you want — one material shared by fifty walls is a real memory
saving. It is emphatically not what you want when the whole point of the node is
that each instance has its own size.

The fix is to create the resource *in code, per instance*:

```rust
let mut box_mesh = BoxMesh::new_gd();
box_mesh.set_size(self.size);
mesh_node.set_mesh(&box_mesh);
```

`BoxMesh::new_gd()` makes a fresh one every time, so nothing is shared.

You will meet the same trap again in Lesson 12, where flashing one enemy red
flashes all forty-eight of them, and the fix is the same shape:
`duplicate_resource()`.

---

## Do it

### Step 1 — Name the collision layers

**Project → Project Settings → Layer Names → 3D Physics.** Fill in the first
five:

```
Layer 1: World
Layer 2: Player
Layer 3: Enemy
Layer 4: EnemyHitbox
Layer 5: Interactable
```

These names are cosmetic — the engine only sees bits — but they turn the
Inspector's collision checkboxes from an unlabelled grid into something you can
read. Do it now; retrofitting names onto a project where you have already set
twenty masks by number is miserable.

### Step 2 — Make the materials

In the **FileSystem** dock, right-click `res://` → **Create New → Folder**, named
`materials`.

Right-click `materials` → **Create New → Resource**, choose
**StandardMaterial3D**, and save it as `greybox_floor.tres`. Double-click it and
set **Albedo → Color** to `(0.34, 0.35, 0.38)`.

Repeat for the rest:

| File | Albedo | Roughness | Used for |
|---|---|---|---|
| `greybox_floor.tres` | `(0.34, 0.35, 0.38)` | 0.95 | floors |
| `greybox_wall.tres` | `(0.46, 0.45, 0.44)` | 0.9 | walls |
| `greybox_cover.tres` | `(0.30, 0.32, 0.36)` | 0.85 | cover blocks |
| `greybox_accent.tres` | `(0.62, 0.36, 0.22)` | 0.7 | anything interactive |
| `greybox_enemy.tres` | `(0.36, 0.44, 0.34)` | 0.9 | enemies |
| `greybox_target.tres` | `(0.55, 0.52, 0.45)` | 0.8 | practice targets |
| `greybox_barrier.tres` | `(0.72, 0.24, 0.18)` | 0.65 | the door |

The orange accent is doing real work: it is the only warm colour in the palette,
so anything the player can interact with jumps out of a grey room without a
single label.

### Step 3 — Write the `Wall` class

Create `rust/src/wall.rs`:

```rust
use godot::classes::{BoxMesh, BoxShape3D, CollisionShape3D, IStaticBody3D, Material, MeshInstance3D, StaticBody3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(tool, base=StaticBody3D, init)]
pub struct Wall {
    /// A setter runs the rebuild. `#[var(set = ...)]` is how gdext spells
    /// GDScript's inline `set:` block.
    #[export]
    #[var(set = set_size)]
    #[init(val = Vector3::new(24.0, 4.0, 0.5))]
    size: Vector3,

    #[export]
    #[var(set = set_material)]
    material: Option<Gd<Material>>,

    base: Base<StaticBody3D>,
}

#[godot_api]
impl IStaticBody3D for Wall {
    fn ready(&mut self) {
        self.rebuild();
    }
}

#[godot_api]
impl Wall {
    #[func]
    fn set_size(&mut self, value: Vector3) {
        self.size = value;
        self.rebuild();
    }

    #[func]
    fn set_material(&mut self, value: Option<Gd<Material>>) {
        self.material = value;
        self.rebuild();
    }
}

impl Wall {
    fn rebuild(&mut self) {
        if !self.base().is_inside_tree() {
            return;
        }

        let Some(mesh_node) = self.base().get_node_or_null("Mesh") else {
            return;
        };
        let Some(shape_node) = self.base().get_node_or_null("Shape") else {
            return;
        };
        let (Ok(mut mesh_node), Ok(mut shape_node)) = (
            mesh_node.try_cast::<MeshInstance3D>(),
            shape_node.try_cast::<CollisionShape3D>(),
        ) else {
            return;
        };

        let mut box_mesh = BoxMesh::new_gd();
        box_mesh.set_size(self.size);
        if let Some(material) = &self.material {
            box_mesh.set_material(material);
        }
        mesh_node.set_mesh(&box_mesh);

        let mut shape = BoxShape3D::new_gd();
        shape.set_size(self.size);
        shape_node.set_shape(&shape);

        // Sit the wall's BASE at y = 0 so it can be placed on the floor without
        // doing half-height arithmetic every time.
        let lift = Vector3::new(0.0, self.size.y * 0.5, 0.0);
        mesh_node.set_position(lift);
        shape_node.set_position(lift);
    }
}
```

Add `pub mod wall;` to `lib.rs` and build.

### Step 4 — Read the new pieces

**`#[class(tool, ...)]`** — this class runs in the editor as well as the game.

**`#[var(set = set_size)]`** — by default `#[export]` generates a getter and
setter for you. Naming a setter here replaces the generated one, so anything that
writes `size` — Inspector, scene file, other code — goes through your function
and triggers a rebuild.

> **Do not add a bare `get` alongside it.** `#[var(get, set = set_size)]` means
> "use a getter I wrote called `get_size`", and if you have not written one, the
> error is a confusing "no associated function named `get_size`". Omit `get`
> entirely and gdext generates one.

**`Option<Gd<Material>>`** — every object reference from Godot can be null, and
gdext makes you say so. `Gd<T>` is a handle to a Godot object — the Rust
equivalent of a GDScript object reference, refcounted where the object is
refcounted.

**`let ... else`** — the shape you will use constantly against Godot's nullable
API. `get_node_or_null` returns `Option<Gd<Node>>`; `try_cast` returns a `Result`.
Both get unwrapped with an early return, because a `Wall` whose children have
been renamed should quietly do nothing rather than crash the editor.

**`if !self.base().is_inside_tree() { return; }`** — the guard that makes tool
code survivable. `rebuild` gets called by the setter, which fires while the scene
is still being loaded and the children do not exist yet.

**`BoxMesh::new_gd()`** — a fresh mesh per instance, dodging the shared-resource
trap. `new_gd()` is for refcounted types (resources); `new_alloc()` is for
manually-freed ones (nodes). Reach for the wrong one and the compiler tells you.

### Step 5 — Build the wall scene

In the editor, create a new scene (**Scene → New Scene**), choose **Other Node**,
and pick **Wall**.

Add two children, named **exactly** `Mesh` (a `MeshInstance3D`) and `Shape` (a
`CollisionShape3D`). The names matter — `rebuild` looks them up by string, and
`get_node_or_null` returning `None` is precisely why it fails silently rather
than crashing.

Select the `Wall` root and set:

- **Collision → Layer:** `World` only (bit 1)
- **Collision → Mask:** *nothing checked*
- **Size:** `(24, 4, 0.5)`
- **Material:** drag `greybox_wall.tres` in

The wall appears immediately, because the class is a `tool`. Change **Size** and
watch it rebuild live.

Save as `res://scenes/wall.tscn`.

### Step 6 — Cover

New scene, root **`StaticBody3D`**, renamed `Cover`. No Rust needed — it never
changes size.

- `MeshInstance3D` child with a **New BoxMesh**, **Size** `(2, 1.2, 2)`,
  material `greybox_cover.tres`, **Position** `(0, 0.6, 0)`
- `CollisionShape3D` child with a **New BoxShape3D**, **Size** `(2, 1.2, 2)`,
  **Position** `(0, 0.6, 0)`
- Root: layer `World`, mask empty

The `(0, 0.6, 0)` offsets sit the box on the floor rather than half-buried, the
same trick the `Wall` class does in code.

Save as `res://scenes/cover.tscn`.

### Step 7 — Assemble the arena

New scene, root `Node3D` named `Arena`. Save as `res://scenes/arena.tscn`.

Add a `Node3D` child named `NavRegion` — in Lesson 11 this becomes a
`NavigationRegion3D`, and putting it in now saves re-parenting the whole map
later. Under it, add two `Node3D` children: `ZoneStart` and `ZoneYard`. Lesson 17
turns those into `Zone` nodes.

Under **`ZoneStart`**:

| Node | Type | Transform | Notes |
|---|---|---|---|
| `Floor` | `StaticBody3D` | pos `(0, -0.25, 0)` | BoxMesh + BoxShape3D, size `(24, 0.5, 24)`, `greybox_floor.tres`, layer World |
| `WallSouth` | instance of `wall.tscn` | pos `(0, 0, 12)` | |
| `WallEast` | instance of `wall.tscn` | pos `(12, 0, 0)`, rot y `-90` | |
| `WallWest` | instance of `wall.tscn` | pos `(-12, 0, 0)`, rot y `-90` | |
| `WallNorthLeft` | instance of `wall.tscn` | pos `(-7, 0, -12)` | size `(10, 4, 0.5)` |
| `WallNorthRight` | instance of `wall.tscn` | pos `(7, 0, -12)` | size `(10, 4, 0.5)` |
| `Platform` | `StaticBody3D` | pos `(-7.5, 0.5, -7.5)` | BoxMesh + BoxShape3D, size `(7, 1, 7)`, `greybox_accent.tres` |
| `CoverA` | instance of `cover.tscn` | pos `(5, 0, -3)` | |
| `CoverB` | instance of `cover.tscn` | pos `(-4, 0, 4)`, rot y `45` | |
| `CoverC` | instance of `cover.tscn` | pos `(7, 0, 7)` | |
| `CoverD` | instance of `cover.tscn` | pos `(0.5, 0, -8)`, rot y `30` | |

The two north walls leave a four-metre gap in the middle. That gap is the doorway
the door will block in Lesson 17.

Add a `Node3D` under `ZoneStart` named `SpawnPoints`, with three `Marker3D`
children at `(-10, 0.2, 10)`, `(10, 0.2, 10)` and `(-10.5, 0.2, -2)`. A `Marker3D`
draws a cross in the editor and does nothing at runtime — it is a labelled
position, and that is all we need.

Select all three, and in the Inspector's **Node → Groups** tab add them to a group
named `spawn_point`. Lesson 15 finds them by that name.

Under **`ZoneYard`**, a smaller area behind the doorway:

| Node | Type | Transform | Notes |
|---|---|---|---|
| `Floor` | `StaticBody3D` | pos `(0, -0.25, -20)` | size `(16, 0.5, 16)`, `greybox_floor.tres` |
| `WallNorth` | instance of `wall.tscn` | pos `(0, 0, -28)` | size `(16, 4, 0.5)` |
| `WallEast` | instance of `wall.tscn` | pos `(8, 0, -20)`, rot y `-90` | size `(16, 4, 0.5)` |
| `WallWest` | instance of `wall.tscn` | pos `(-8, 0, -20)`, rot y `-90` | size `(16, 4, 0.5)` |
| `CoverE` | instance of `cover.tscn` | pos `(-4, 0, -22)` | |
| `CoverF` | instance of `cover.tscn` | pos `(4.5, 0, -17)`, rot y `45` | |
| `SpawnPoints/Spawn1` | `Marker3D` | pos `(-6, 0.2, -26)` | group `spawn_point` |
| `SpawnPoints/Spawn2` | `Marker3D` | pos `(6, 0.2, -26)` | group `spawn_point` |

> **Overriding a property on an instance.** `WallNorthLeft` uses `size = (10, 4, 0.5)`
> while `wall.tscn` says `(24, 4, 0.5)`. Change it in the Inspector and a revert
> arrow appears — that arrow means "this instance overrides the scene." The scene
> file records only the difference, which is why instancing scales.

Save the arena.

### Step 8 — Put it in the main scene

Open `main.tscn`. Delete the old `Floor`. Instance `arena.tscn` as a child of
`Main` and name it `Arena`.

Keep the `Spinner` — Lesson 17 reuses its behaviour for real. Keep the
`DirectionalLight3D` and `WorldEnvironment`.

Press **F5**. You get an arena with no way to move through it. That is Lesson 4.

---

## Check yourself

1. What is the difference between a collision layer and a collision mask?
2. Why is the walls' mask empty?
3. What does `#[class(tool)]` do, and what is the risk?
4. Why does `Wall` build its `BoxMesh` in code instead of storing one in
   `wall.tscn`?
5. `rebuild` starts with `if !self.base().is_inside_tree() { return; }`. What
   goes wrong without it?
6. Why is `#[var(set = set_size)]` correct where `#[var(get, set = set_size)]`
   fails to compile?

<details>
<summary>Answers</summary>

1. Layer is "what I am", mask is "what I collide with". A collides with B when
   A's mask contains B's layer — which is deliberately not symmetric.
2. A wall never initiates a collision; things collide *with* it. Giving static
   geometry a mask costs performance and buys nothing.
3. Makes the class run inside the editor as well as the game, so Inspector edits
   take effect immediately. The risk is that a panic takes the editor down with
   your unsaved work.
4. A mesh saved in the scene is shared by every instance, so resizing one wall
   would resize all of them. `BoxMesh::new_gd()` makes a fresh one per instance.
5. The setter fires while the scene is still loading, before `Mesh` and `Shape`
   exist. Without the guard you are looking up children that are not there yet.
6. A bare `get` means "use my hand-written `get_size`". Omitting it lets gdext
   generate the getter, which is what you want.

</details>

---

## Extend it

- Give `Wall` an `#[export] doorway_width: f32` that leaves a gap in the middle,
  built from two boxes. Then replace `WallNorthLeft`/`WallNorthRight` with one
  wall. Notice how much easier the arena is to edit afterwards.
- Walk the arena in the editor's 3D view (right-drag to look, WASD to fly) and
  ask whether it reads as a *place*. If the answer is "it's a grey box", that is
  information: move cover, vary wall lengths, raise something.
- Set a wall's mask to `World` and think about what you just told the physics
  engine to compute every tick, forever, for no benefit.

---

## Commit

```bash
git add -A
git commit -m "Lesson 3: greybox arena, Wall tool class, collision layers"
```

---

**Next:** [Lesson 4 — FPS controller I](04-fps-controller-1.md)
