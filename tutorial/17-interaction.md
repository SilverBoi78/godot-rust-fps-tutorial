# Lesson 17 — Interaction, and what to do without inheritance

## What we're building

A door you can buy open, and a wall-mounted weapon you can buy ammunition from.
Look at either and a prompt appears; press E and, if you can afford it, something
happens.

Two very different behaviours sharing one set of rules about prompts,
affordability and payment. In GDScript that is a base class and two subclasses.
**gdext will not let you do that**, and this lesson is about what you do instead —
and why the result is arguably better.

---

## The concept

### The obvious design, and why it is unavailable

A door and a wall buy differ in what interacting *does*. They are identical in
how they are found, prompted and paid for. That shared part wants to live in one
place.

GDScript writes:

```gdscript
class_name Interactable extends StaticBody3D
    func get_prompt() -> String
    func can_interact(player) -> bool
    func interact(player)          # enforces payment, then calls _on_interact
    func _on_interact(player)      # subclasses override THIS
```

and then `extends Interactable` for the door and the wall buy.

In Rust:

```rust
#[derive(GodotClass)]
#[class(base = Interactable)]   // Interactable is your own class
pub struct Door { /* ... */ }
```

```
error[E0425]: cannot find type `Interactable` in module `::godot::classes`
error[E0433]: cannot find `inherit_from_Interactable__ensure_class_exists`
```

**`#[class(base = ...)]` accepts engine classes only.** A gdext class cannot
extend another gdext class. This is a documented limitation of the bindings, not
something you have configured wrong, and it is the single largest structural
difference between writing Godot in GDScript and in Rust.

So the design has to change. There are three honest options.

### Option 1: a Rust trait

<!-- illustrative -->
```rust
trait Interactable {
    fn prompt(&self) -> GString;
    fn cost(&self) -> i32;
    fn on_interact(&mut self, player: Gd<Node3D>);

    // A default method -- Rust's version of a base-class template method.
    fn interact(&mut self, player: Gd<Node3D>) { /* payment protocol */ }
}
```

Traits with default methods really are Rust's answer to a base class with a
template method, and for pure-Rust code this would be the right call.

It breaks at the boundary. The interactor finds things by raycasting, and a
raycast returns a `Gd<Node>`. Going from that to `&mut dyn Interactable` means
either try-casting against every known implementor — which does not scale — or
gdext's `DynGd` machinery, which is real but is a lot of concept to introduce
here.

### Option 2: duck typing through Godot

Declare `#[func] fn get_prompt`, `#[func] fn interact` on each type and have the
interactor call them by name with `Object::call`.

This works, and it is what a GDScript-shaped mind reaches for. It also throws away
the reason you chose Rust: no compile-time checking, method names as strings,
failures at runtime.

### Option 3: a component — what this project does

`Interactable` becomes a **child node**, exactly like `Health`:

```
Door                          WallBuy
├── Interactable   <--        ├── Interactable   <--
├── Mesh                      ├── Plate
└── Shape                     ├── Shape
                              └── Display
```

The component holds everything shared — display name, cost, single-use flag, the
used latch, and the payment protocol. The parent supplies the behaviour.

Two hooks connect them, and the choice of hook per direction is the design:

| Direction | Mechanism | Replaces |
|---|---|---|
| Component asks parent "may I?" | a `Callable` the parent installs | an overridden `can_interact` |
| Component tells parent "go" | a signal the parent connects to | an overridden `_on_interact` |

```rust
    used: bool,
    /// Installed by the parent in its `ready`. Called with the player node,
    /// returns a bool. `None` means "always available".
    availability_check: Option<Callable>,
```

### Why this is better, and where it is worse

Better:

- **The payment protocol lives in exactly one place and cannot be overridden.**
  In the inheritance version a subclass can override `interact` and skip payment.
  Here that is not expressible.
- **An interactable can be any node type.** The inheritance version forces every
  interactable to be a `StaticBody3D`. A future interactable that wants to be an
  `Area3D`, or an `AnimatableBody3D`, just works.
- **It matches `Health`.** One composition pattern, used twice, found by the same
  kind of lookup. A reader who has understood Lesson 10 has already understood
  this.

Worse, and worth saying plainly:

- **`availability_check` is a string-named `Callable`.** A typo compiles and fails
  at runtime. The inheritance version's `can_interact` override is checked by the
  compiler.
- **Two nodes instead of one.** A little more scene structure to keep right.
- **The relationship is implicit.** Nothing in the type system says a `Door` needs
  an `Interactable` child; you find out at `ready` when `OnReady` panics.

That third point is the real cost. Godot's own design has the same property —
nothing says a `StaticBody3D` needs a `CollisionShape3D` — so it is at least
consistent with the engine, but it is a genuine loss of static checking and it
would be dishonest to present the workaround as a free win.

### The payment protocol

```rust
    /// Called by the player's `Interactor`. Handles the payment protocol once,
    /// so no parent has to remember to check affordability.
    ///
    /// The parent cannot override this, only react to the signal it emits.
    /// A method that enforces an invariant and then hands off is much harder to
    /// get wrong than one every subclass is trusted to reimplement correctly.
    #[func]
    pub fn interact(&mut self, player: Gd<Node3D>) {
        if !self.can_interact(player.clone()) {
            return;
        }

        let price = self.cost;
        if price > 0 && !GameState::singleton().bind_mut().try_spend(price) {
            return;
        }

        self.used = true;
        self.signals().performed().emit(&player);
    }
```

Four steps, in this order, every time: check availability, take payment, latch,
announce. A parent that forgot one of those is not possible.

And availability:

```rust
    /// Whether the prompt should appear at all.
    #[func]
    pub fn can_interact(&self, player: Gd<Node3D>) -> bool {
        if self.used && self.single_use {
            return false;
        }
        match &self.availability_check {
            Some(check) => check.callv(&varray![&player]).booleanize(),
            None => true,
        }
    }
```

`callv` calls a `Callable` with an array of arguments and returns a `Variant`;
`booleanize()` applies Godot's truthiness rules. `varray![&player]` builds a
`VariantArray` — note the `&`, because object arguments are passed by reference.

`None` meaning "always available" keeps the simple case simple: an interactable
with no extra conditions installs no hook.

### Finding it, exactly like `Health`

```rust
/// Finds the `Interactable` component on a node, the same way
/// `weapon::find_health` finds a `Health`. Two components, one lookup pattern.
pub fn find_interactable(node: &Gd<Node>) -> Option<Gd<Interactable>> {
    for child in node.get_children().iter_shared() {
        if let Ok(interactable) = child.try_cast::<Interactable>() {
            return Some(interactable);
        }
    }
    None
}
```

Simpler than `find_health` because an interactable is always a direct child —
there are no nested hitboxes to account for.

### Announcing only on change

The interactor raycasts every physics tick. Telling the HUD every tick would make
it rebuild a label sixty times a second for nothing:

```rust
        // Only announce on CHANGE of target. Emitting every frame would make
        // the HUD rebuild its label sixty times a second for no reason.
        let changed = found != self.current;
```

with one deliberate exception: if the target is unchanged, affordability may still
have changed as points came in, so the prompt is re-emitted to update its colour.

That is a small, real piece of UI thinking. The prompt turning from red to white
the instant you can afford it is the feedback that makes the economy legible.

### Zones

A `Zone` is a region of the map that starts hidden and non-solid:

```rust
    /// `visible` alone hides the geometry but leaves it solid, so a closed zone
    /// would still be an invisible wall you could stand on. Collision has to be
    /// switched separately.
    fn set_collision_enabled(&mut self, enabled: bool) {
        let children = self
            .base()
            .find_children_ex("*")
            .type_("CollisionObject3D")
            .owned(false)
            .done();

        for child in children.iter_shared() {
            if let Ok(mut body) = child.try_cast::<CollisionObject3D>() {
                // `process_mode` on the zone handles scripts; this handles physics.
                body.set_collision_layer_value(1, enabled);
            }
        }
    }
```

Visibility and collision are independent in Godot, and forgetting that gives you
an invisible wall — one of the more disorienting bugs to debug, because the thing
causing it cannot be seen.

Everything a zone needs to know is structural: its contents are its children. A
map author composes zones in the editor with no code at all, which is the point.

---

## Do it

### Step 1 — The `Interactable` component

Create `rust/src/interactable.rs`:

```rust
#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct Interactable {
    #[export]
    #[init(val = "Interactable".into())]
    pub display_name: GString,
    #[export]
    pub cost: i32,
    /// Once used, stop offering the prompt (wall buys stay usable; doors do not).
    #[export]
    pub single_use: bool,
```

with the signal and the prompt:

```rust
    /// Emitted after a successful interaction -- that is, after `can_interact`
    /// passed AND the cost was paid. The parent does its actual work here.
    #[signal]
    pub fn performed(player: Gd<Node3D>);

    /// The text shown on screen.
    #[func]
    pub fn get_prompt(&self) -> GString {
        if self.cost > 0 {
            format!("{}  [{}]", self.display_name, self.cost)
                .as_str()
                .into()
        } else {
            self.display_name.clone()
        }
    }
```

plus the hook installer and the reuse latch:

```rust
    /// Lets a reusable interactable clear the latch that `interact` sets.
    #[func]
    pub fn clear_used(&mut self) {
        self.used = false;
    }

    #[func]
    pub fn set_availability_check(&mut self, check: Callable) {
        self.availability_check = Some(check);
    }
```

> **`.as_str().into()`** rather than `.into()`. `GString` implements
> `From<&str>` but not `From<String>`, so a `format!` result needs the extra hop.
> You will hit this the first time you build a string and it is not obvious from
> the error.

### Step 2 — The interactor

Create `rust/src/interactor.rs`. It raycasts on layer 5 and reports what it finds:

```rust
    #[export]
    #[init(val = 2.6)]
    reach: f32,
    /// Interactable is layer 5.
    #[export(flags_3d_physics)]
    #[init(val = 0b10000)]
    interact_mask: u32,
```

```rust
    fn probe(&self) -> Option<Gd<Interactable>> {
        let camera = self.camera.clone()?;

        let from = camera.get_global_position();
        let to = from - camera.get_global_transform().basis.col_c() * self.reach;

        let mut query = PhysicsRayQueryParameters3D::create(from, to)?;
        query.set_collision_mask(self.interact_mask);
        query.set_collide_with_areas(false);
        if let Some(body) = self.owner_body.clone() {
            if let Ok(collider) = body.try_cast::<CollisionObject3D>() {
                query.set_exclude(&array![collider.get_rid()]);
            }
        }

        let hit = camera
            .get_world_3d()?
            .get_direct_space_state()?
            .intersect_ray(&query);

        let collider = hit.get("collider")?.try_to::<Gd<Node>>().ok()?;
        let interactable = find_interactable(&collider)?;

        let player = self.owner_body.clone()?;
        if !interactable.bind().can_interact(player) {
            return None;
        }
        Some(interactable)
    }
```

Note how much shorter this is than the weapon's ray, and why: the whole function
returns `Option`, so `?` handles every failure and there is not one `else` block.
When a function's failure mode is "no result", making it return `Option` and
leaning on `?` is much cleaner than early returns.

`set_collide_with_areas(false)` here — the opposite of the weapon. Interactables
are bodies; there is no reason to pay for area checks.

And the tick:

```rust
    pub fn tick(&mut self, intent: &PlayerIntent) {
        self.refresh_target();

        if intent.interact_pressed {
            if let (Some(mut current), Some(player)) =
                (self.current.clone(), self.owner_body.clone())
            {
                current.bind_mut().interact(player);
                // Re-check straight away: a door that just opened should stop
                // prompting this frame rather than next.
                self.refresh_target();
            }
        }
    }
```

### Step 3 — `Zone`

Create `rust/src/zone.rs`:

```rust
#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Zone {
    #[export]
    #[init(val = "Zone".into())]
    zone_name: GString,
    /// The starting area must be open from the beginning.
    #[export]
    open_at_start: bool,

    base: Base<Node3D>,
}
```

```rust
    #[func]
    pub fn open(&mut self) {
        self.base_mut().set_visible(true);
        self.base_mut().set_process_mode(ProcessMode::INHERIT);
        self.set_collision_enabled(true);

        let name = self.zone_name.clone();
        EventBus::singleton().signals().zone_opened().emit(&name);
    }
```

The `zone_opened` emit is what triggers the arena's navmesh re-bake from
Lesson 11 — two systems that never mention each other.

In `arena.tscn`, change `ZoneStart` and `ZoneYard` to type **`Zone`**. Set
`ZoneStart`'s **Open At Start** to on, and name them `Start` and `Yard`.

### Step 4 — The door

Create `rust/src/barrier.rs`:

```rust
#[godot_api]
impl IStaticBody3D for Door {
    fn ready(&mut self) {
        let path = self.zone_path.clone();
        self.zone_to_open = self
            .base()
            .get_node_or_null(&path)
            .and_then(|node| node.try_cast::<Zone>().ok());

        if self.zone_to_open.is_none() {
            godot_warn!("Door '{}' has no zone to open.", self.base().get_name());
        }

        let this = self.to_gd();
        {
            let mut interactable = self.interactable.bind_mut();
            interactable.single_use = true;
            interactable
                .set_availability_check(Callable::from_object_method(&this, "zone_is_closed"));
        }
        self.interactable
            .signals()
            .performed()
            .connect_other(&this, Door::on_performed);
    }
}
```

Both hooks installed in `ready`: the availability `Callable`, and the `performed`
connection. That pair is what a subclass's two overrides would have been.

The `{ }` block releases the `Interactable` borrow before `signals()` is called
on it again. Fourth appearance of this pattern.

The condition:

```rust
    /// The extra condition an overridden `can_interact` would have expressed.
    /// Takes the player because that is the contract `Interactable` calls with;
    /// this particular check does not need it.
    #[func]
    fn zone_is_closed(&self, _player: Gd<Node3D>) -> bool {
        match &self.zone_to_open {
            Some(zone) => !zone.bind().is_open(),
            None => false,
        }
    }
```

and the behaviour:

```rust
    fn on_performed(&mut self, _player: Gd<Node3D>) {
        if let Some(zone) = &mut self.zone_to_open {
            zone.bind_mut().open();
        }

        // Stop blocking immediately; sink out of sight for effect.
        self.base_mut().set_collision_layer_value(1, false);
        self.base_mut().set_collision_layer_value(5, false);
```

Collision is cleared **immediately**, before the animation. Waiting for the tween
would leave an invisible wall for 0.6 seconds, and the player will walk into it,
because they started moving the moment they pressed the button.

Note how little there is in this file. Everything about prompts, affordability
and payment lives in the component; this class only knows what buying it *means*.
**That is the test of whether the shared part was drawn in the right place.**

### Step 5 — The wall buy

Create `rust/src/wall_buy.rs`. Its availability condition genuinely needs the
player:

```rust
    /// No point offering a purchase that would do nothing.
    #[func]
    fn player_needs_ammo(&self, player: Gd<Node3D>) -> bool {
        match find_weapon(&player) {
            Some(weapon) => {
                let weapon = weapon.bind();
                weapon.get_reserve() < weapon.max_reserve
            }
            None => false,
        }
    }
```

and its behaviour clears the latch, because it is reusable:

```rust
    fn on_performed(&mut self, player: Gd<Node3D>) {
        if let Some(mut weapon) = find_weapon(&player) {
            weapon.bind_mut().add_reserve(self.ammo_granted);
        }
        // Reusable: clear the single-use latch that `interact` just set.
        self.interactable.bind_mut().clear_used();
    }
```

It also configures its component's display text at runtime, rather than in the
scene:

```rust
        {
            let mut interactable = self.interactable.bind_mut();
            interactable.display_name = format!("Buy {name} ammo").as_str().into();
            interactable.cost = cost;
            interactable
                .set_availability_check(Callable::from_object_method(&this, "player_needs_ammo"));
        }
```

And the spinning display, which is Lesson 2's cube doing an actual job:

```rust
    fn process(&mut self, delta: f64) {
        // Slowly rotate the mounted weapon so it reads as interactive. The exact
        // trick from the first script lesson, finally doing a real job.
        self.display.rotate_y(delta as f32 * 0.6);
    }
```

### Step 6 — The scenes

**`door.tscn`** — root type `Door`, layer `World + Interactable` (17), mask empty:

```
Door
├── Interactable   (Interactable)  display_name "Door", cost 750
├── Mesh           BoxMesh (4, 3.6, 0.35), greybox_barrier.tres, pos (0, 1.8, 0)
└── Shape          BoxShape3D (4, 3.6, 0.35), pos (0, 1.8, 0)
```

**`wall_buy.tscn`** — root type `WallBuy`, same layers:

```
WallBuy
├── Interactable   (Interactable)   -- configured in ready
├── Plate          BoxMesh (1.6, 1.1, 0.12), pos (0, 1.5, 0)
├── Shape          BoxShape3D (1.6, 1.1, 0.4), pos (0, 1.5, 0.14)
└── Display        (Node3D) pos (0, 1.5, 0.3)
    └── Gun        BoxMesh (0.1, 0.13, 0.75), greybox_accent.tres, rot y 15
```

**Layer 17 is `World + Interactable`** — bit 1 plus bit 5, `0b10001`. World so it
blocks movement and bullets; Interactable so the interactor's ray finds it.

The collision shape is deeper than the plate (`0.4` vs `0.12`) and pushed back, so
you can interact from slightly in front rather than having to press your face
against it.

Place the door in `arena.tscn` at `(0, 0, -12)` — the gap between the two north
walls — with `zone_path` set to `../ZoneYard`. Place the wall buy against the
east wall at `(11.6, 0, 3)`, rotated to face inward.

### Step 7 — Wire the player

Add the `Interactor` child to `player.tscn`, and in `Player::ready`:

```rust
        self.interactor.bind_mut().setup(camera, body);
```

with `physics_process` already calling `self.interactor.bind_mut().tick(&intent);`
from Lesson 6.

### Step 8 — Play it

Run. Walk up to the door.

Without a HUD you cannot see the prompt yet — Lesson 18 fixes that. Test it by
printing in `refresh_target`, or just award yourself points and press E:

- With fewer than 750 points, E does nothing and `points` is unchanged.
- With 750 or more, the door sinks, the zone appears, and points drop by 750.
- A few seconds later, enemies path into the new area — the navmesh re-baked.
- The next round spawns from the new markers too.

**That last pair is worth pausing on.** Nothing connects the door to the navmesh
or to the spawner. The door opens a zone; the zone announces itself; the arena
re-bakes; the director collects visible markers. Four systems, no direct
references, correct behaviour.

For the wall buy: set `reserve_ammo` low in the Remote inspector, then buy. You
get +120, capped at 400, for 500 points — and it stays usable, unlike the door.

---

## Check yourself

1. Why can't `Door` extend an `Interactable` class in gdext?
2. Name the three options for the shared behaviour, and why this project chose
   composition.
3. What replaces an overridden `can_interact`? What replaces an overridden
   `_on_interact`?
4. Why is the payment protocol in a method the parent cannot override?
5. Give one way this design is better than inheritance and one way it is worse.
6. Why does the door clear its collision layers before the tween rather than
   after?
7. Why does the wall buy call `clear_used()` but the door does not?
8. Why does hiding a zone require touching collision separately?
9. Trace what happens between pressing E on the door and enemies pathing into the
   new area. How many direct references are involved?

<details>
<summary>Answers</summary>

1. `#[class(base = ...)]` accepts engine classes only. A gdext class cannot extend
   another gdext class.
2. A Rust trait (breaks at the raycast boundary, where you only have a `Gd<Node>`);
   duck typing through `Object::call` (throws away compile-time checking);
   composition (works, and matches `Health`).
3. A `Callable` the parent installs on the component; a signal the parent connects
   to.
4. So a subclass cannot skip payment. A method enforcing an invariant and then
   handing off is much harder to get wrong than one everyone reimplements.
5. Better: payment cannot be bypassed, and an interactable can be any node type.
   Worse: `availability_check` is a string-named `Callable` that fails at runtime
   rather than compile time.
6. The player starts walking the instant they press the button. Waiting would
   leave an invisible wall for 0.6 seconds.
7. The wall buy is reusable, so it clears the latch `interact` set. The door is
   single-use and should stop prompting.
8. Visibility and collision are independent in Godot. Hiding alone leaves an
   invisible solid wall.
9. Door → zone (`open()`); zone → EventBus (`zone_opened`); arena listens and
   re-bakes; director collects visible markers next round. Exactly one direct
   reference — the door's `zone_path`.

</details>

---

## Extend it

- Add a third interactable — a health station that costs points and heals. It
  should take you about ten minutes, and if it takes longer the component
  boundary is in the wrong place.
- Give `Interactable` an optional `Callable` for the prompt text too, so a parent
  can build it dynamically ("Buy Rifle ammo [500]" vs "Full"). Then ask whether
  three hooks is still better than a base class.
- Implement the trait version from Option 1 for these two types and see how far
  you get before the `Gd<Node>` boundary stops you. Understanding *why* it fails
  is worth more than being told.
- Make the door play `audio::click()` on a failed purchase. Which object should
  own that — the door, the component, or the HUD listening to `purchase_failed`?

---

## Commit

```bash
git add -A
git commit -m "Lesson 17: Interactable component, doors, wall buys, zones"
```

---

**Next:** [Lesson 18 — HUD](18-hud.md)
