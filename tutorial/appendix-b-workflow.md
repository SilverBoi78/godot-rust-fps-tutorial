# Appendix B — Editor and cargo workflow

The day-to-day mechanics: what to press, what to run, and how to keep the two
halves of a Rust + Godot project in step.

---

## The build loop

Rust edits do not take effect until you rebuild. The loop is:

```bash
cargo build --manifest-path rust/Cargo.toml
```

then, in the editor, **F5** to run.

Roughly ten seconds on this project once dependencies are compiled. You will run
that command hundreds of times, so make it cheap to run:

**A shell alias** (`~/.bashrc`, `~/.zshrc`):

```bash
alias b='cargo build --manifest-path ~/code/shooter/rust/Cargo.toml'
```

**Fish** (`~/.config/fish/config.fish`):

```fish
alias b 'cargo build --manifest-path ~/code/shooter/rust/Cargo.toml'
```

**PowerShell** (`$PROFILE`):

```powershell
function b { cargo build --manifest-path $HOME\code\shooter\rust\Cargo.toml }
```

Or just run the terminal with `rust/` as its working directory and type
`cargo build`.

### Watch mode

```bash
cargo install cargo-watch
cargo watch -x build --workdir rust
```

Rebuilds on every save. Combined with `reloadable = true` in the `.gdextension`,
the editor picks the new library up on its own most of the time.

Worth knowing: this makes it easy to leave a *broken* build sitting there while
you tab to the editor and wonder why nothing changed. Keep the watch window
visible.

### Check without building

```bash
cargo check --manifest-path rust/Cargo.toml
```

Type-checks without producing a library, and is roughly twice as fast. Use it
while you are still fixing compile errors; use `build` when you actually want to
run.

---

## Hot reload

`reloadable = true` lets the editor unload and reload the library when it changes.
When it works, you rebuild and the editor picks it up with no restart.

It does not always work, and the failure is quiet — you run and get the *old*
behaviour, which is a genuinely confusing ten minutes.

**Restart the editor when:**

- you added, removed or renamed a class
- you changed a `#[signal]` or `#[export]` declaration
- the editor has instances of your class open in a scene and is holding them
- behaviour does not match code you are certain you rebuilt

**A reliable 20-second reset when something is inexplicable:**

```bash
# close the editor first
rm -rf godot/.godot
cargo build --manifest-path rust/Cargo.toml
godot4 --headless --path godot --import
godot4 --path godot
```

Deleting `.godot` throws away the import cache and forces a clean rescan. It is
safe — everything in there is regenerated — and it resolves the majority of
"Godot is not seeing my change" problems.

---

## Useful commands

```bash
# Run the game without opening the editor
godot4 --path godot

# Run headless (no window) -- for tests and CI
godot4 --headless --path godot

# Re-scan the filesystem, registering .gdextension changes
godot4 --headless --path godot --import

# Run a specific scene
godot4 --path godot res://scenes/player.tscn

# The reference build's test suite
godot4 --headless --path reference/godot res://tests/tests.tscn

# Release build -- much faster at runtime, slower to compile
cargo build --release --manifest-path rust/Cargo.toml
```

> **Debug builds are slow, and it is not subtle.** An unoptimised gdext build can
> be several times slower than a release one. If 48 enemies stutter in debug,
> measure with `--release` before you start optimising — and remember the
> `.gdextension` file has separate `debug` and `release` entries, so Godot picks
> whichever matches how it was launched.

---

## Editor shortcuts worth learning

### Running

| Key | Action |
|---|---|
| **F5** | Run the main scene |
| **F6** | Run the *current* scene |
| **F8** | Stop |
| **Ctrl+S** | Save scene |
| **Ctrl+Shift+S** | Save all |

**F6** is the one people miss. Testing the player alone, without spawning
enemies, is much faster than loading the whole game — as long as your code
tolerates `get_current_scene()` being unusual, which is why `Weapon::impact_parent`
has a fallback.

### Scene dock

| Key | Action |
|---|---|
| **Ctrl+A** | Add child node |
| **Ctrl+Shift+A** | Instance a child scene |
| **F2** | Rename |
| **Ctrl+D** | Duplicate |
| **Ctrl+Shift+Down** | Save branch as scene |

### 3D viewport

| Key | Action |
|---|---|
| **Right-drag** | Look around |
| **Right-drag + WASD** | Fly |
| **Middle-drag** | Orbit |
| **F** | Frame the selected node |
| **Q / W / E / R** | Select / Move / Rotate / Scale |
| **Ctrl+Alt+F** | Move the selected node to the current view |

**Ctrl+Alt+F** is the fastest way to place something: fly to where you want it,
select the node, press it.

---

## Local vs Remote

At the top of the Scene dock while the game runs:

- **Local** — the scene as saved on disk.
- **Remote** — the scene as it exists *right now* in the running game.

Remote is where you tune. Select a node, drag values in the Inspector, and the
running game responds immediately. This is the entire reason `#[export]` exists,
and it is the difference between finding good numbers and settling for the first
ones you typed.

**Remote changes are not saved.** Write down anything you liked before you press
F8, or you will do the work twice. (Everyone does this once.)

Remote is also the best way to inspect state: expand `EnemyPool` and count active
children, check `GameState`'s points, confirm a zone is actually hidden.

---

## Reading .tscn files

Scenes are text. Get comfortable with them.

```bash
cat godot/scenes/player.tscn
git diff godot/scenes/
```

Some things are simply faster typed than clicked — setting eight transforms,
renaming a node used in twenty places, or building a UI scene. The reference's
`hud.tscn` was written by hand.

**The `type=` field is your Rust struct's name.** Rename `Player` to `PlayerBody`
and every scene saying `type="Player"` breaks, with no compiler help at all.
After any class rename:

```bash
grep -rn 'type="OldName"' godot/
```

---

## Git

Commit after every lesson. When you break something badly — and you will — a
working state to return to turns an evening into ten minutes.

```bash
git add -A
git commit -m "Lesson 12: enemy state machine"
```

### What is ignored, and why

- **`rust/target/`** — hundreds of megabytes of build output.
- **`godot/.godot/`** — Godot's import cache. Machine-generated, and it produces
  merge conflicts forever if committed.
- **`export_presets.cfg`** — often contains local paths and signing details.

### What is committed

Everything else, including `.tscn`, `.tres` and `.import` files. Godot generates
`.import` files for imported assets, and they must be committed or a fresh clone
re-imports everything with different UIDs.

### Line endings

`.gitattributes` with `* text=auto eol=lf`. Godot writes LF on every platform.
Without this a Windows collaborator's first commit is a 4,000-line whitespace
diff.

---

## Working across Windows and Linux

If you move between machines:

1. **Install Godot as `godot4` on both.** Every command in this tutorial is then
   identical.
2. **Never commit `target/` or `.godot/`.** They contain absolute paths and are
   not portable.
3. **Watch case sensitivity.** `res://Scenes/Player.tscn` works on Windows and
   fails on Linux. Keep everything lowercase and match it exactly.
4. **Rebuild after switching.** The `.gdextension` file points at
   platform-specific filenames — `libshooter.so`, `shooter.dll`,
   `libshooter.dylib` — and a fresh clone has none of them until you build.
5. **Re-import after a fresh clone.** `.godot/` is gitignored, so the extension is
   not registered until a scan has run.

---

## When the editor lies to you

Symptoms that mean "the editor's picture of your code is stale":

- A class you just added is not in the Create Node dialog.
- An `#[export]` you removed is still in the Inspector.
- A scene shows nodes as "placeholder".
- Behaviour matches code you deleted.

In escalating order:

1. Rebuild. (Did the build actually succeed? Look at the terminal.)
2. Restart the editor.
3. `rm -rf godot/.godot`, rebuild, `--import`, reopen.

Step 3 fixes essentially all of them.

---

## A pre-commit check

Worth running before you push:

```bash
cargo fmt --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml
cargo build --manifest-path rust/Cargo.toml
godot4 --headless --path godot res://tests/tests.tscn
```

Format, lint, build, verify. Clippy is genuinely good at spotting the small
inefficiencies that accumulate in gameplay code — an unnecessary `clone()` on a
`Gd<T>` in a hot loop, a `String` allocated every frame for a label.

Note that `cargo fmt` rewraps long lines, which will reflow code you have quoted
elsewhere. Run it early and often rather than once at the end.
