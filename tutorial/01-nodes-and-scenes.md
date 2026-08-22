# Lesson 1 — Nodes and scenes

## What we're building

Understanding, and one small scene. This is the shortest lesson with the highest
payoff: nearly every "how do I do X in Godot" question has an answer that only
makes sense once the node/scene model has clicked.

If you skim one lesson in this tutorial, do not let it be this one.

---

## The concept

### Everything is a node

A **node** is one object that does one thing. Godot ships with a few hundred of
them, and their names describe them honestly:

| Node | What it does |
|---|---|
| `Node3D` | Has a position, rotation and scale in 3D. Nothing else. |
| `MeshInstance3D` | Draws a mesh. |
| `CollisionShape3D` | Describes a shape for the physics engine. |
| `Camera3D` | Renders the world from where it is. |
| `AudioStreamPlayer3D` | Plays a sound from a point in space. |
| `Timer` | Counts down and emits a signal. |
| `Label` | Draws text on the screen. |

Notice what is *not* there: there is no `Player` node, no `Enemy` node, no
`Gun` node. Those are things you build, and building one means combining several
small nodes into a tree.

### Nodes form a tree, and children inherit transforms

Nodes have exactly one parent and any number of children. A player might be:

```
Player              (CharacterBody3D -- can move and collide)
├── CollisionShape3D    (the capsule the physics engine sees)
├── Head                (Node3D -- an empty pivot at eye height)
│   └── Camera3D        (what you look through)
└── Health              (a component -- Lesson 10)
```

The single most useful property of this tree: **a child's transform is relative
to its parent's.** Move `Player` and everything inside it moves. Rotate `Head`
and only the camera turns. That is why the head is an empty `Node3D` rather than
the camera itself — it gives you a pivot to rotate for looking up and down,
separate from the body's own left-and-right rotation.

You will use this constantly. When something in Godot seems to need
"attach this to that", the answer is usually "make it a child."

### A scene is a tree you saved

Save a node tree to a `.tscn` file and it becomes a **scene**. You can then
place copies of it — **instances** — inside other scenes.

That is the whole idea. `enemy.tscn` is a scene. The arena is a scene that
contains 48 instances of it. Edit `enemy.tscn` once and all 48 change, because
each instance is a reference to the saved scene, not a copy of its contents.

An instance can override individual properties of its scene without breaking the
link — one enemy placed with a different position, one wall with a different
size. The scene file stores what is common; the instance stores the differences.

> **This is the closest thing Godot has to a class hierarchy for content.** A
> scene is roughly "a prefab", and instancing it is roughly "constructing one."
> If you want ten kinds of enemy that share 90% of their setup, you make a base
> enemy scene and inherit scenes from it, rather than reaching for code.

### Where Rust comes in

In a GDScript project, you add behaviour by *attaching a script to a node*. A
`CharacterBody3D` plus `player.gd` becomes a player.

**Rust does not work that way, and this trips up everyone who has seen a GDScript
tutorial first.** There is no "attach Rust script" button. Instead:

```rust
#[derive(GodotClass)]
#[class(base = CharacterBody3D)]
pub struct Player {
    base: Base<CharacterBody3D>,
}
```

This registers a **new node type** called `Player`. It appears in the editor's
Create Node dialog next to `CharacterBody3D`, and when you add one you get a
node that does everything a `CharacterBody3D` does plus whatever you wrote.

So the mapping is:

| GDScript | Rust + gdext |
|---|---|
| `extends CharacterBody3D` | `#[class(base = CharacterBody3D)]` |
| Attach `player.gd` to a `CharacterBody3D` | Add a `Player` node |
| `class_name Player` | The struct's name is the class name |

The consequence worth internalising: **your scene files reference your Rust
types by name.** `player.tscn` contains `type="Player"`. If you rename the
struct, the scene breaks — and unlike a Rust rename, the compiler cannot help
you, because the scene file is data Godot reads at runtime. Renaming a class
means grepping your `.tscn` files.

### `Base<T>` is composition, not inheritance

Look at that struct again:

```rust
pub struct Player {
    base: Base<CharacterBody3D>,
}
```

Rust has no inheritance, so gdext does the next best thing: your struct *holds*
the engine object it extends. `Base<T>` is that handle.

From inside your methods you reach the engine's own functionality through it:

```rust
self.base().is_on_floor()             // read
self.base_mut().move_and_slide()      // write
```

That is more typing than GDScript's bare `is_on_floor()`, and it is the single
most visible tax of writing Godot in Rust. It also has a consequence the
borrow checker will explain to you in Lesson 4, loudly.

### The node types that matter in 3D

You will meet these constantly. Learning them now saves confusion later:

- **`Node3D`** — anything with a 3D transform. The base of everything spatial.
- **`MeshInstance3D`** — draws a mesh. Purely visual; the physics engine cannot
  see it.
- **`StaticBody3D`** — solid and never moves. Walls, floors, props.
- **`CharacterBody3D`** — solid, moves under your control, does *not* get pushed
  around by physics. Players and enemies. This is the one you want for anything
  a human or an AI drives.
- **`RigidBody3D`** — solid, moves under the physics engine's control. Barrels
  you can knock over. You will not use one in this tutorial.
- **`Area3D`** — detects overlaps but does not block anything. Trigger volumes,
  and — importantly for Lesson 12 — hitboxes.
- **`CollisionShape3D`** — the actual shape. **Every body and area needs one as
  a child**, or it is invisible to physics. Forgetting this is the most common
  first-week Godot mistake there is.

> **`MeshInstance3D` is not collision, and `CollisionShape3D` is not visible.**
> They are separate nodes because they are separate concerns: a complex mesh is
> usually paired with a much simpler collision shape for performance. A wall you
> can see and walk through, or one you can walk into and not see, means you
> added one and not the other.

---

## Do it

### Step 1 — Look at the scene you have

Open the project:

```
godot4 --path godot
```

You should have `scenes/main.tscn` from Lesson 0, containing your `Hello` node.

The **Scene** dock, top left, shows the node tree. The **Inspector**, on the
right, shows the selected node's properties. The **FileSystem** dock, bottom
left, shows `res://` — your project folder.

Click on `Hello` in the Scene dock and look at the Inspector. Because `Hello`
extends `Node3D`, it has `Transform`, `Visibility`, and so on — all inherited
from `Node3D` without you writing a line.

### Step 2 — Delete it and build a real root

Right-click `Hello` → **Delete Node(s)**.

Click **+** (Add Child Node) or press **Ctrl+A**. Search for `Node3D`, add it,
and rename it to `Main` — double-click the name, or press F2.

> **Why is the root a plain `Node3D` and not something more specific?** Because
> the root of a scene is a container for everything else, and giving it
> capabilities it does not need only creates opportunities for confusion. A
> generic root also makes the scene easy to instance somewhere else later.

Save with **Ctrl+S**. Keep the path `res://scenes/main.tscn`.

### Step 3 — Give it a floor

With `Main` selected, add a **`StaticBody3D`** child. Rename it to `Floor`.

Add a **`MeshInstance3D`** as a child of `Floor`. In the Inspector, find **Mesh**,
click the dropdown that says `<empty>`, and choose **New BoxMesh**.

Click the BoxMesh that now appears in that slot to expand its own properties, and
set **Size** to `(20, 0.5, 20)`.

Add a **`CollisionShape3D`** as a child of `Floor` too. In its **Shape** slot,
choose **New BoxShape3D**, click it, and set **Size** to `(20, 0.5, 20)`.

Your tree:

```
Main            (Node3D)
└── Floor       (StaticBody3D)
    ├── MeshInstance3D   (BoxMesh, 20 x 0.5 x 20)
    └── CollisionShape3D (BoxShape3D, 20 x 0.5 x 20)
```

The mesh and the shape have the same size because you want the thing you see and
the thing you collide with to agree. Godot will not check that for you, and
"invisible ledge" bugs are always this.

### Step 4 — Light it

Add a **`DirectionalLight3D`** as a child of `Main`. It represents the sun: only
its rotation matters, not its position.

Set its **Rotation** to `(-45, 30, 0)` and tick **Shadow → Enabled**.

Add a **`WorldEnvironment`** as a child of `Main`. In its **Environment** slot,
choose **New Environment**. Click it, set **Background → Mode** to `Sky`, and in
the **Sky** slot choose **New Sky**, then inside that choose **New
ProceduralSkyMaterial**.

That is three levels of nested resource, which feels absurd the first time. The
reason is that each layer is independently reusable: many environments can share
one sky, many skies can share one material.

Press **F5**. You get a lit floor, and no way to move.

### Step 5 — Save the arena as its own scene

Right-click `Floor` → **Save Branch as Scene**, and save it as
`res://scenes/floor.tscn`.

Look at the Scene dock: `Floor` now has a small clapperboard icon and its
children are hidden. It is an instance of a saved scene.

Now select `Main` and add a second instance: click the **chain-link icon**
(Instance Child Scene) in the Scene dock toolbar, choose `floor.tscn`, and set
the new node's **Position** to `(25, 0, 0)`.

Open `floor.tscn` (double-click it in FileSystem), change the BoxMesh **Size**
to `(20, 0.5, 30)`, and save. Go back to `main.tscn`.

**Both floors changed.** That is the entire value of scenes in one observation.

Now select just the second floor in `main.tscn` and change its **Position**. Only
that one moves — the instance stores its own overrides, and the shared scene
stores everything else.

Delete the second floor when you have seen it. Undo the size change to
`floor.tscn` too, or leave it; we rebuild the arena properly in Lesson 3.

Save everything: **Ctrl+Shift+S**.

### Step 6 — Read the scene file

In a terminal, print the scene you just made:

```bash
cat godot/scenes/main.tscn
```

It is plain text, and readable:

```ini
[gd_scene load_steps=4 format=3]

[ext_resource type="PackedScene" path="res://scenes/floor.tscn" id="1_floor"]

[node name="Main" type="Node3D"]

[node name="Floor" parent="." instance=ExtResource("1_floor")]
```

Worth noticing:

- **Scenes are text, so git can diff and merge them.** Unusual and very welcome.
- `[node ... type="Node3D"]` is how a node's class is recorded. When you build a
  `Player` node in Lesson 4, this will say `type="Player"` — your Rust struct's
  name, written into a data file.
- `instance=ExtResource(...)` is what makes something an instance rather than a
  copy.

You will hand-edit these occasionally, and being unafraid of them is a genuine
advantage.

---

## Check yourself

1. What is the difference between a node and a scene?
2. You have a `MeshInstance3D` and you can walk straight through it. Why?
3. Why is the camera a child of an empty `Head` node rather than a direct child
   of the player body?
4. In GDScript you write `extends CharacterBody3D` and attach the script. What
   is the Rust equivalent, and how does the resulting node get into a scene?
5. What does `Base<CharacterBody3D>` do, and why does gdext need it?
6. You rename your `Player` struct to `PlayerBody`. The project compiles. What
   breaks, and why can't the compiler tell you?

<details>
<summary>Answers</summary>

1. A node is one object with one job. A scene is a saved tree of nodes that can
   be instanced into other scenes.
2. It has no collision. A `MeshInstance3D` is purely visual; solidity needs a
   physics body with a `CollisionShape3D` child.
3. So that looking up and down can rotate the head alone while the body's
   left-right rotation stays separate. Rotating the body on two axes would tip
   the whole character over.
4. `#[derive(GodotClass)]` with `#[class(base = CharacterBody3D)]`. That
   registers a new node *type*, which you add from the Create Node dialog like
   any built-in node.
5. It holds the engine object your struct extends, since Rust has no
   inheritance. You reach the engine's own methods through `self.base()` and
   `self.base_mut()`.
6. Every `.tscn` file containing `type="Player"` breaks. The compiler cannot
   help because scene files are data read at runtime, not code. Renaming a class
   means grepping your scenes.

</details>

---

## Extend it

- Build a small scene of your own — a table as a `Node3D` with four leg
  `MeshInstance3D`s and a top. Save it, instance it three times, then change the
  original and watch all three follow.
- Take one of those instances and override a property, then use the **revert
  arrow** next to it in the Inspector to drop the override. Notice the scene
  file grows and shrinks as you do.
- Open the Godot documentation for `Node3D` (select the node and click the book
  icon, or press F1 and search). Skim the property list. You do not need to
  learn it; you need to know it exists and where it lives.

---

**Next:** [Lesson 2 — Your first Rust class](02-first-class.md)
