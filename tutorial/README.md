# Building a first-person shooter in Rust and Godot 4

You are going to build a round-based survival shooter using Godot 4.7.2 for the
engine and Rust for every line of game logic, starting from having never opened
a game engine.

This is not a recipe. Every lesson teaches a concept *before* it uses it, and
asks you questions afterwards to check that you actually learned it rather than
transcribed it. The goal is not "finish the tutorial." The goal is that when you
finish, **you can build the next feature without one.**

---

## What you need before starting

**You have read [The Rust Book](https://doc.rust-lang.org/book/) and understood
it.** Ownership, borrowing, traits, enums, `Option`, `Result`, pattern matching
and modules are assumed knowledge — this tutorial will not re-teach them.

**You have not necessarily written much Rust of your own**, and that is fine.
Reading the book and being able to sit down in front of an empty file are
different skills, and the second one is built by doing exactly this.

**You have never used a game engine.** Nothing about Godot is assumed.

If you have used Godot before but not Rust, you will find the engine parts slow
and the Rust parts fast. That is fine too; the lessons are self-contained enough
to skim the half you know.

---

## What you're building

An arena you can move through, a gun that feels good to fire, enemies that path
around cover and swing at you, escalating rounds, a points economy, doors you
can buy open, a wall-mounted ammo vendor, and a HUD.

[`../DESIGN.md`](../DESIGN.md) is the design document. Read it — it explains the
*why* behind a lot of decisions these lessons will make, including an honest
accounting of what Rust costs here as well as what it buys.

At the end you will have a playable prototype and the answer to the only
question that matters at this stage: **is it fun?**

---

## The shape of a Rust + Godot project

Worth understanding before Lesson 0, because it is the thing most unlike what
you would expect.

In a GDScript project you write scripts and *attach* them to nodes. There is no
build step; you press play.

With Rust it works differently. You write structs, annotate them with
`#[derive(GodotClass)]`, and `cargo build` compiles the whole crate into one
dynamic library. Godot loads that library at startup and your structs appear in
the editor's **Create Node** dialog as node types, sitting alongside `Node3D`
and `CharacterBody3D` as though they shipped with the engine.

So:

- **Rust replaces GDScript. It does not replace the editor.** You will still
  build scenes by clicking, still place meshes by dragging, still bake the
  navigation mesh with a button.
- **Adding behaviour means adding a node of your own type**, not attaching a
  script to a generic one.
- **There is a build step.** About ten seconds on this project. You will get
  used to it.

---

## How to use this

**Type the code. Don't paste it.**

I mean this seriously, and it's the single biggest factor in whether this works.
Typing is slow and that is the point — it forces you to read every token, and
you will make mistakes, and fixing them is how you learn what the compiler's
messages mean. Pasting produces a working game and no knowledge.

The Rust compiler makes this much less painful than it sounds. Most of your
mistakes will be caught before you ever run the game, with an error that points
at the exact character and usually tells you the fix.

**Go in order.** Lessons build on each other, and later ones assume you
understand earlier concepts without re-explaining them.

**Do the "Check yourself" questions.** If you can't answer them without
scrolling back, redo the lesson. That's not a punishment — it means the lesson
didn't land yet, and lessons that don't land compound into confusion four
lessons later.

**Do the "Extend it" exercises when you have energy for them.** They have no
provided solution on purpose. Struggling with an open-ended change is where the
actual learning happens; the guided part just gives you the vocabulary.

**Commit after every lesson.** Each lesson ends with a git command. This gives
you a working state to roll back to whenever you break something badly, which
you will, repeatedly, and that's normal.

### When you're stuck

Check [Appendix C — the debugging playbook](appendix-c-debugging.md) first. It's
a symptom → cause table covering both the Godot mistakes every beginner makes in
3D and the gdext-specific ones that have no GDScript equivalent. Roughly half
the problems you hit are already in there.

If you're asking someone for help, the useful things to give them are:

1. **Which lesson and step** you're on.
2. **The exact error text** — the whole thing, not a summary. For Rust errors
   that means the full `cargo build` output including the `note:` and `help:`
   lines, which are usually where the answer is.
3. **What you expected vs. what happened.**
4. **The code you actually wrote** (not what the tutorial says it should be —
   those differ, and the difference is usually the bug).

---

## Lessons

### Part 0 — Setup and mental model
| # | Lesson | You'll learn |
|---|---|---|
| 0 | [Setup](00-setup.md) | Godot 4.7.2, the Rust toolchain, the two-folder layout, `.gdextension`, git |
| 1 | [Nodes and scenes](01-nodes-and-scenes.md) | Godot's core mental model, and how a Rust struct becomes a node type |
| 2 | [Your first Rust class](02-first-class.md) | `#[derive(GodotClass)]`, lifecycle methods, `delta`, `#[export]`, `Base<T>` |

### Part 1 — The player
| # | Lesson | You'll learn |
|---|---|---|
| 3 | [Greybox arena](03-greybox-arena.md) | Materials, collision layers, `#[class(tool)]`, readable level geometry |
| 4 | [FPS controller I](04-fps-controller-1.md) | `CharacterBody3D`, physics frames, the Input Map, and the borrow rule |
| 5 | [FPS controller II — feel](05-fps-controller-2.md) | Acceleration, head bob, frame-rate independent smoothing |
| 6 | [Input → intent](06-input-to-intent.md) | Decoupling input from simulation, and when *not* to make something a class |

### Part 2 — The gun
| # | Lesson | You'll learn |
|---|---|---|
| 7 | [Hitscan](07-hitscan.md) | Raycasting, untyped `Dictionary` results, and **collision masks** |
| 8 | [Weapon feel](08-weapon-feel.md) | Fire rate, recoil, tweens, `Callable`, audio synthesised in Rust |
| 9 | [Ammo and reload](09-ammo-and-reload.md) | State machines — the thing Rust's type system is best at |
| 10 | [Damage and signals](10-damage-and-signals.md) | Component nodes, and gdext's typed signals |

### Part 3 — The enemy
| # | Lesson | You'll learn |
|---|---|---|
| 11 | [Navigation](11-navigation.md) | Navmeshes, `NavigationAgent3D`, and the experimental-API feature flag |
| 12 | [Enemy behaviour](12-enemy-behaviour.md) | State machines applied, hitboxes, headshots, death |
| 13 | [Object pooling](13-object-pooling.md) | Why mid-round instantiation hitches, and `Vec<Gd<T>>` as a pool |
| 14 | [Autoloads and the EventBus](14-autoloads-and-eventbus.md) | Singletons in a language with no globals |

### Part 4 — The loop
| # | Lesson | You'll learn |
|---|---|---|
| 15 | [RoundDirector](15-round-director.md) | Spawn budgets, and tuning curves as **data** instead of code |
| 16 | [Economy and GameState](16-economy-and-gamestate.md) | Run state vs. persistent state, and why that seam matters |
| 17 | [Interaction and inheritance](17-interaction.md) | What to do when your language has no inheritance and the engine assumes it |
| 18 | [HUD](18-hud.md) | Godot's UI system — a genuinely separate skill from Godot 3D |

### Appendices
- [A — gdext reference card](appendix-a-gdext.md) — the GDScript ↔ Rust translation table
- [B — Editor and cargo workflow](appendix-b-workflow.md)
- [C — Debugging playbook](appendix-c-debugging.md) ← *bookmark this one*
- [D — Glossary](appendix-d-glossary.md)

---

## Verifying your build

```bash
# The reference build's own test suite -- 48 checks, no window needed
godot4 --headless --path reference/godot res://tests/tests.tscn
```

Useful when something in your project misbehaves and you want to confirm the
reference still works, or to see exactly what a system is expected to do.

## `reference/`

[`../reference/`](../reference) is a complete, working version of the project,
compiled against Godot 4.7.2 and gdext 0.5.5. Every code snippet in these
lessons was copied out of it after the test suite passed — so if a snippet
fails for you, the difference is on your side, and diffing against `reference/`
will find it fast.

**Use it as a diff target, not a shortcut.** Copying from it gets you a finished
prototype and no ability to build the next thing.

It also contains `reference/rust/src/stages/`, which holds the player exactly as
Lessons 4 and 5 leave it. If your controller misbehaves partway through Part 1,
that is what to diff against rather than the finished `player.rs`.

---

## Pace

Expect **two to three months** of part-time work, and expect Part 1 and Part 3
to be the slow ones. Going slowly there is not a sign anything is wrong —
learning an engine *is* the work right now.

If you have never written Rust outside of the book's exercises, add time for
Lessons 2, 4 and 6, where the borrow checker and gameplay code meet for the
first time. That collision is uncomfortable and then it stops being
uncomfortable, fairly suddenly, usually around Lesson 7.
