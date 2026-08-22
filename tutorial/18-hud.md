# Lesson 18 — The HUD

## What we're building

Everything the player needs to see: health, ammunition, points, the round number,
enemies remaining, a crosshair that reacts to hits, an interaction prompt, and a
banner announcing each round.

Godot's UI system is a genuinely separate skill from Godot 3D — different nodes,
different layout model, different instincts.

---

## The concept

### `Control` is a different world

3D uses `Node3D` and transforms. UI uses `Control` and a layout system: anchors,
offsets, and containers that position their children for you.

| Node | What it does |
|---|---|
| `Control` | base of everything UI; has a rectangle |
| `Label` | draws text |
| `Panel` | draws a styled box |
| `ProgressBar` | a bar with a value |
| `VBoxContainer` | stacks children vertically, sizing them |
| `HBoxContainer` | the same, horizontally |
| `CanvasLayer` | draws its children on top of the 3D world |

`CanvasLayer` is the one that makes a HUD a HUD. Children of a `CanvasLayer` are
drawn in screen space, unaffected by the camera. Without it, your UI would be a
flat object floating in the world.

### Anchors and offsets

The layout model in two ideas:

- **Anchors** are fractions of the parent: 0 is left/top, 1 is right/bottom.
- **Offsets** are pixels from the anchor.

Anchor a label to `(1, 0)` with an offset of `-320, 24` and it sits 320 pixels
from the right edge, 24 from the top — **at any resolution**. That is the whole
point: you describe the relationship, not the position.

The editor's **Layout** menu (in the toolbar when a `Control` is selected) sets
common anchor presets — Top Left, Center, Full Rect — and you will use it far more
than you will type anchor numbers.

### The stretch settings, which Lesson 0 set

Under **Display → Window**:

- **Viewport** 1920×1080 — design everything for this.
- **Stretch Mode `canvas_items`** — scale the UI to the actual window.
- **Stretch Aspect `expand`** — wider monitors get more width, not black bars.

Without `canvas_items` your HUD is laid out in physical pixels, so it is tiny on
a 4K monitor and enormous on a small one. This is the single most common reason
a HUD that looks right for its author looks wrong for everyone else.

### Unique names

```rust
    #[init(node = "%HealthBar")]
    health_bar: OnReady<Gd<ProgressBar>>,
    #[init(node = "%HealthLabel")]
    health_label: OnReady<Gd<Label>>,
```

The `%` prefix means **unique name in owner**. Tick that box on a node
(right-click → **Access as Unique Name**) and it can be reached with `%Name` from
anywhere in the scene, regardless of where it sits in the tree.

For UI this is close to essential. A HUD's tree is deep and gets rearranged
constantly — move a label into a new container and every hard-coded path to it
breaks. A unique name survives.

The trade-off: uniqueness is per-scene, and a typo is a runtime failure rather
than a compile error. For a scene you own entirely, that is a good deal.

### The HUD knows nothing

This is the property to protect:

```rust
        bus.signals()
            .points_changed()
            .connect_other(&this, Hud::on_points_changed);
        bus.signals()
            .round_started()
            .connect_other(&this, Hud::on_round_started);
```

The HUD does not know what an `Enemy` is. It holds no reference to the
`RoundDirector`. **It could be deleted entirely without breaking the game.**

That matters because UI is the layer most likely to be rewritten — twice before
release, usually. Everything that depends on it has to be rewritten with it, so
the correct number of things depending on it is zero.

### Two kinds of wiring, and when to use which

Not everything goes through the bus:

```rust
    /// Called by `Player`, which owns the weapon. Signals are for things with
    /// unknown listeners; this is a known, fixed relationship, so a direct
    /// wire-up is clearer than routing weapon ammo through the global bus.
    pub fn bind_weapon(&mut self, weapon: Gd<Weapon>) {
        let this = self.to_gd();
        weapon
            .signals()
            .ammo_changed()
            .connect_other(&this, Hud::on_ammo_changed);
```

Ammunition belongs to *a* weapon held by *a* player. On the global bus, a second
player's reload would repaint your ammo counter. Rule 2 from Lesson 14, with a
concrete consequence.

The rule: **broadcast facts go on the bus; a known one-to-one relationship gets a
direct connection.** "Which player's ammo is this?" having an answer is the test.

### Getting the initial value right

```rust
        let (mag, reserve) = {
            let weapon = weapon.bind();
            (weapon.get_in_magazine(), weapon.get_reserve())
        };
        self.on_ammo_changed(mag, reserve);
```

Connecting to a signal tells you about *future* changes. It says nothing about
the current value, so a HUD that only connects shows zeros until something
happens.

Two fixes, and this project uses both:

- **Call the handler once by hand** after connecting, as above.
- **Emit on initialisation.** `Health::ready` emits `changed` immediately, so
  anything connected before then gets its starting value for free.

The `{ }` block scopes the `bind()` so the borrow is released before
`on_ammo_changed` runs — which touches `self` and would otherwise be a second
borrow while the first is live. Fifth appearance of that pattern; by now it
should read as ordinary.

### Feedback the player actually notices

**The crosshair, on a hit:**

```rust
    fn on_hit_confirmed(&mut self, is_headshot: bool) {
        let colour = if is_headshot {
            Color::from_rgb(1.0, 0.55, 0.2)
        } else {
            Color::WHITE
        };
        let scale = if is_headshot {
            Vector2::new(1.35, 1.35)
        } else {
            Vector2::new(1.2, 1.2)
        };
```

Punch it out and animate it back, in parallel:

```rust
        let mut tween = self.base_mut().create_tween();
        tween.set_parallel();
        tween.tween_property(&crosshair, "scale", &Vector2::ONE.to_variant(), 0.14);
        tween.tween_property(&crosshair, "modulate", &Color::WHITE.to_variant(), 0.2);
    }
```

`set_parallel()` makes subsequent steps run together rather than in sequence —
the default. Both properties animate at once, over slightly different durations.

A hitmarker is the highest-value 15 lines in any shooter's UI. Without it, players
genuinely cannot tell whether they are hitting anything.

**The prompt, coloured by affordability:**

```rust
    fn on_interact_target_changed(&mut self, prompt: GString, affordable: bool) {
        self.prompt_label.set_text(&prompt);
        let colour = if affordable {
            AFFORDABLE
        } else {
            TOO_EXPENSIVE
        };
        self.prompt_label.set_modulate(colour);
    }
```

Red when you cannot afford it, white when you can, updating live as points come
in. Two colours doing the work of a paragraph of explanation.

**The banner, as a tween sequence:**

```rust
    fn show_banner(&mut self, text: &str, hold: f64) {
        self.banner.set_text(text);
        let banner = self.banner.clone();

        let mut tween = self.base_mut().create_tween();
        tween.tween_property(&banner, "modulate:a", &1.0f32.to_variant(), 0.15);
        tween.tween_interval(hold);
        tween.tween_property(&banner, "modulate:a", &0.0f32.to_variant(), 0.4);
    }
```

Sequential this time — fade in, hold, fade out — because `set_parallel` was not
called. `"modulate:a"` animates only the alpha channel, leaving the colour alone.

Fast in (0.15s), slow out (0.4s). Announcements should arrive promptly and leave
gently; the reverse feels wrong in a way people notice without being able to say
why.

### `mouse_filter`

Every `Control` in this HUD has **Mouse Filter** set to **Ignore**.

By default a `Control` swallows mouse events in its rectangle. A full-screen HUD
therefore eats every click — including the ones meant to fire your gun. Set it to
Ignore on anything that is not a button, or spend an evening wondering why the
trigger stopped working through the top-left corner of the screen.

---

## Do it

### Step 1 — Build the HUD scene

New scene, root `Control` named `HUD`, **Layout → Full Rect**, **Mouse Filter →
Ignore**. Save as `res://scenes/hud.tscn`.

```
HUD                       Control, full rect, mouse filter Ignore
├── Crosshair             Control, Layout → Center           [unique name]
│   └── Dot               Panel, centered, 4x4px, white StyleBoxFlat
├── TopLeft               VBoxContainer, offsets (32, 24, 320, 110)
│   ├── RoundLabel        Label, font size 30, "ROUND 0"      [unique name]
│   └── RemainingLabel    Label, font size 18, grey, "0 left" [unique name]
├── TopRight              VBoxContainer, anchor right, offsets (-320, 24, -32, 80)
│   └── PointsLabel       Label, size 34, gold, right-aligned [unique name]
├── BottomLeft            VBoxContainer, anchor bottom, offsets (32, -104, 320, -32)
│   ├── HealthLabel       Label, font size 26, "100"          [unique name]
│   └── HealthBar         ProgressBar, min size (240, 14)     [unique name]
├── BottomRight           VBoxContainer, anchor bottom-right, offsets (-320, -96, -32, -32)
│   └── AmmoLabel         Label, size 32, right-aligned       [unique name]
├── PromptLabel           Label, centered, 48px below middle  [unique name]
└── Banner                Label, centered, 160px above middle [unique name]
```

For each node marked `[unique name]`: right-click → **Access as Unique Name**. A
small `%` appears next to it.

Set **Mouse Filter → Ignore** on every node.

The four corners are conventional and worth following. Health bottom-left,
ammunition bottom-right, score top-right, objective top-left — players already
know where to look, and being clever here costs you nothing but confusion.

The crosshair is a `Control` wrapping a `Panel` rather than a bare `Panel` for a
specific reason: scaling a `Control` scales around its **pivot**, and the wrapper
gives the animation something to scale that is already centred.

> **Hand-editing is faster.** UI scenes are fiddly to build by clicking. The
> reference's is at `reference/godot/scenes/hud.tscn` — 176 lines of readable
> text. Copying its structure and adjusting is a legitimate way to work, and
> reading it will teach you the layout properties faster than the Inspector will.

### Step 2 — The class

Create `rust/src/hud.rs` and add `pub mod hud;` to `lib.rs`.

```rust
const AFFORDABLE: Color = Color::from_rgb(0.95, 0.95, 0.9);
const TOO_EXPENSIVE: Color = Color::from_rgb(0.9, 0.35, 0.3);
```

`Color::from_rgb` is `const`, so these are compile-time constants rather than
values built on every call.

Then the node handles, all by unique name, and the connections in `ready`:

```rust
    fn ready(&mut self) {
        let this = self.to_gd();
        let bus = EventBus::singleton();

        // One `signals()` call per signal. The handle it returns configures a
        // single signal at a time, so holding one in a variable and reusing it
        // for seven connections panics at runtime -- it compiles perfectly well.
        bus.signals()
            .points_changed()
            .connect_other(&this, Hud::on_points_changed);
```

Change `hud.tscn`'s root type to **`Hud`**.

### Step 3 — The handlers

```rust
    fn on_health_changed(&mut self, current: f32, maximum: f32) {
        self.health_bar.set_max(maximum as f64);
        self.health_bar.set_value(current as f64);
        self.health_label
            .set_text(&format!("{}", current.round() as i32));
    }

    fn on_player_damaged(&mut self, _amount: f32, _current: f32, _maximum: f32) {
        let crosshair = self.crosshair.clone();
        let mut crosshair_mut = crosshair.clone();
        crosshair_mut.set_modulate(Color::from_rgb(1.0, 0.4, 0.4));

        let mut tween = self.base_mut().create_tween();
        tween.tween_property(&crosshair, "modulate", &Color::WHITE.to_variant(), 0.25);
    }

    fn on_ammo_changed(&mut self, in_magazine: i32, reserve: i32) {
        self.ammo_label
            .set_text(&format!("{in_magazine} / {reserve}"));
    }

    fn on_reload_started(&mut self, _seconds: f32) {
        self.ammo_label.set_text("RELOADING");
    }
```

There is no `on_reload_finished` handler, deliberately: the `ammo_changed` that
`finish_reload` emits repaints the label anyway. Two signals doing one job is a
chance for them to disagree.

`current.round() as i32` rather than a plain cast — `as i32` truncates, so 99.7
health would display as 99. A player at 99.7 who reads "99" thinks they took
damage they did not take.

```rust
    fn on_round_started(&mut self, round_number: i32, enemy_count: i32) {
        self.round_label.set_text(&format!("ROUND {round_number}"));
        self.show_banner(
            &format!("ROUND {round_number}  —  {enemy_count} INCOMING"),
            1.6,
        );
    }
```

### Step 4 — Bind it

In `Player`:

```rust
    /// Called by `Main` once the HUD exists. Keeping the wiring in one place
    /// beats having the HUD hunt for the player.
    #[func]
    pub fn bind_hud(&mut self, hud: Gd<crate::hud::Hud>) {
        let mut hud = hud;
        let weapon = self.weapon.clone();
        let health = self.health.clone();
        let mut hud_ref = hud.bind_mut();
        hud_ref.bind_weapon(weapon);
        hud_ref.bind_health(health);
    }
```

and in `Main::ready`:

```rust
        let hud = self.hud.clone();
        self.player.bind_mut().bind_hud(hud);
```

The direction matters. `Main` knows the scene layout and hands the HUD to the
player; the player, which owns the weapon and the health, does the binding. The
HUD hunts for nothing.

In `main.tscn`: add a **`CanvasLayer`** child of `Main` named `HUDLayer`, and
instance `hud.tscn` inside it.

### Step 5 — Play it

Run. All of it should be live:

- Health bar drains when an enemy hits you; the crosshair flashes red.
- Ammo counts down, says RELOADING, comes back.
- Points climb on every hit.
- A banner announces each round; another says CLEAR.
- "N left" counts down as you kill.
- The crosshair pops white on a hit, orange and bigger on a headshot.
- Looking at the door shows a prompt — red until you can afford it.
- Trying to buy something you cannot afford shows "NEED 750".

**Then resize the window.** Drag it narrow, drag it wide, maximise it. Everything
should stay in its corner at a sensible size. If it does not, check the stretch
settings from Lesson 0.

---

## Check yourself

1. What does `CanvasLayer` do, and what happens without it?
2. Explain anchors versus offsets.
3. Why do the stretch settings matter, and what is the symptom of getting them
   wrong?
4. What does the `%` prefix mean, and what does it buy in a UI scene?
5. Why does ammunition use a direct connection while points use the EventBus?
6. Why does `bind_weapon` call `on_ammo_changed` by hand after connecting?
7. What does `set_parallel()` change, and why does the banner not use it?
8. Why is Mouse Filter set to Ignore everywhere?
9. Why `current.round() as i32` rather than `current as i32`?
10. Why is there no `on_reload_finished` handler?

<details>
<summary>Answers</summary>

1. Draws its children in screen space, on top of the 3D world. Without it the UI
   is a flat object in the world, affected by the camera.
2. Anchors are fractions of the parent rectangle; offsets are pixels from the
   anchor. Together they describe a relationship that survives any resolution.
3. Without `canvas_items` stretch the HUD is laid out in physical pixels, so it
   is tiny on a 4K display and enormous on a small one.
4. Unique name in owner — the node is reachable as `%Name` from anywhere in the
   scene. UI trees get rearranged constantly, and unique names survive it.
5. Ammunition belongs to one specific weapon on one specific player. On the bus, a
   second player's reload would repaint your counter.
6. Connecting only tells you about future changes. Without the manual call the
   HUD shows zeros until the first reload.
7. It makes subsequent tween steps run together instead of in sequence. The
   banner wants fade-in, hold, fade-out in order.
8. A `Control` swallows mouse events in its rectangle by default, so a
   full-screen HUD would eat every click including firing.
9. `as i32` truncates: 99.7 health would show as 99, which reads as damage the
   player did not take.
10. The `ammo_changed` signal that follows repaints the label anyway. Two signals
    doing one job is a chance for them to disagree.

</details>

---

## Extend it

- Add a reload progress bar under the ammo counter, driven by `reload_started`'s
  `seconds` argument and a tween. Note that `Weapon` already exposes everything
  needed and requires no changes at all — that is the decoupling paying off.
- Add floating damage numbers at the hit position. This needs
  `points_awarded` to carry a world position, which is the extension suggested in
  Lesson 16.
- Add a low-health vignette: a red `ColorRect` whose alpha follows
  `1.0 - health_fraction`. Then decide whether it should pulse, and playtest
  whether the pulse is helpful or annoying.
- Delete the HUD node from `main.tscn` and run. The game should work perfectly,
  blind. If anything breaks, something depends on the UI and should not.

---

## Where this leaves you

That is the prototype. You have:

- an arena with buyable areas
- a controller that feels good
- a gun that feels good
- pooled enemies that path around cover and telegraph their attacks
- rounds that escalate on curves you can draw
- an economy, two interactables, and a HUD
- 48 headless checks proving it all still works

And, more to the point, you have the vocabulary to add the next thing without a
tutorial: a second weapon, a new enemy, a boss round, a menu.

The obvious next steps, roughly in order of value:

1. **Pause, game over, and restart.** The run currently ends with a printed line.
2. **Juice** — screen shake, hit-stop, better death effects.
3. **A second weapon**, which is where you will feel the pull toward making
   weapons *data* (a `Resource` holding the stats) rather than a class per gun.
4. **Export a build** and give it to someone who has never seen it. Watch them
   play without helping. Nothing else will teach you as much in twenty minutes.

Number 3 is the one that changes the architecture, and it is worth doing the
moment you have two weapons rather than the moment you have five.

---

## Commit

```bash
git add -A
git commit -m "Lesson 18: HUD -- health, ammo, points, rounds, prompts, hitmarkers"
```

---

**Appendices:** [A — gdext reference card](appendix-a-gdext.md) ·
[B — workflow](appendix-b-workflow.md) ·
[C — debugging](appendix-c-debugging.md) ·
[D — glossary](appendix-d-glossary.md)
