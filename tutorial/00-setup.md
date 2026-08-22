# Lesson 0 — Setup

## What we're building

Nothing yet. By the end of this lesson you will have a Godot project, a Rust
crate, and a `.gdextension` file gluing them together — and you will have seen a
Rust struct show up in Godot's node list, which is the moment the whole approach
starts to feel real.

Setup lessons are boring. This one is worth doing carefully anyway, because
almost every "my Rust class doesn't exist" problem people hit later traces back
to one of the four steps here.

---

## The concept

### Three things have to agree

A Rust + Godot project is three pieces that each have to point at the other two:

1. **A Godot project** — `project.godot` and a folder of scenes and resources.
2. **A Rust crate** — compiled as a `cdylib`, a dynamic library with a C-compatible
   interface, which is the only kind of library Godot knows how to load.
3. **A `.gdextension` file** — a small text file inside the Godot project that
   says "there is a library over there, load it, and here is its entry point."

When you press play, Godot reads `.gdextension`, loads the library, calls its
entry function, and the library registers every `#[derive(GodotClass)]` type it
contains with Godot's class database. From that moment on, `Player` is a node
type as far as the engine is concerned.

### Why the folder layout matters

We are going to use this layout:

```
shooter/
├── godot/          <- the Godot project lives here
│   ├── project.godot
│   ├── shooter.gdextension
│   └── scenes/
└── rust/           <- the Rust crate lives here
    ├── Cargo.toml
    ├── src/
    └── target/     <- cargo's build output, gitignored
```

Two sibling folders, not one nested in the other. The reason is `target/`.

Godot's asset importer scans everything under the project folder and builds an
index of it. `target/` for this project contains tens of thousands of files and
several hundred megabytes. If it lived inside the Godot project, every import
scan would crawl the whole thing, and your editor would be unusable.

You *can* keep them nested and add a `.gdignore` file to `target/` to make Godot
skip it. Plenty of projects do. Two siblings is simpler and there is nothing to
forget.

---

## Do it

### Step 1 — Install Godot 4.7.2

Get the **standard** build, not the .NET/Mono one. The .NET build is for C#, it
is larger, and it brings nothing to a Rust project.

We are going to install it as a command named `godot4`, so that every command in
this tutorial is identical on every platform. Getting this right now saves you
translating commands for the next several months.

**Linux**

```bash
mkdir -p ~/.local/bin
cd /tmp
curl -LO https://github.com/godotengine/godot/releases/download/4.7.2-stable/Godot_v4.7.2-stable_linux.x86_64.zip
unzip Godot_v4.7.2-stable_linux.x86_64.zip
chmod +x Godot_v4.7.2-stable_linux.x86_64
mv Godot_v4.7.2-stable_linux.x86_64 ~/.local/bin/godot4
```

Make sure `~/.local/bin` is on your `PATH`. If `godot4 --version` says "command
not found", add this to your `~/.bashrc` (or `~/.zshrc`, or
`~/.config/fish/config.fish`):

```bash
export PATH="$HOME/.local/bin:$PATH"
```

**macOS**

Download the macOS build from <https://godotengine.org/download/macos/>, drag
`Godot.app` to `/Applications`, then:

```bash
mkdir -p ~/.local/bin
ln -s /Applications/Godot.app/Contents/MacOS/Godot ~/.local/bin/godot4
```

**Windows (PowerShell)**

Download the Windows build from <https://godotengine.org/download/windows/> and
unzip it somewhere permanent, such as `C:\Tools\Godot`. Then:

```powershell
mkdir $HOME\bin -Force
Copy-Item C:\Tools\Godot\Godot_v4.7.2-stable_win64.exe $HOME\bin\godot4.exe
[Environment]::SetEnvironmentVariable(
    "Path", $env:Path + ";$HOME\bin", "User")
```

Open a new terminal for the `PATH` change to take effect.

**Verify, on all three:**

```
godot4 --version
```

You want `4.7.2.stable.official.<hash>`. If you get a different version, the
lessons will still mostly work, but scene file formats and API details drift
between minor versions and you will hit small differences the tutorial does not
warn you about.

### Step 2 — Install Rust

If you already have Rust, check the version:

```
rustc --version
```

You need **1.94 or newer** — that is gdext 0.5.5's minimum supported Rust
version. If yours is older, or if Rust is not installed at all, use
[rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

On Windows, download and run `rustup-init.exe` from the same site, and choose
the MSVC toolchain when it offers.

Then:

```
rustup update
rustc --version
cargo --version
```

> **Windows only:** gdext links against the Microsoft C runtime, so you also
> need the **Visual Studio Build Tools** with the "Desktop development with C++"
> workload. `rustup-init` will tell you if they are missing and offer to install
> them. Say yes; nothing will link without them.

### Step 3 — Create the folders

```bash
mkdir -p shooter/godot
cd shooter
cargo new --lib rust
```

`cargo new --lib` creates `rust/Cargo.toml` and `rust/src/lib.rs`. It also runs
`git init` inside `rust/`, which we do not want — we want one repository at the
top, not one per subfolder:

```bash
rm -rf rust/.git
git init
```

### Step 4 — Configure the crate

Open `rust/Cargo.toml` and replace it with this:

```toml
[package]
name = "shooter"
version = "0.1.0"
edition = "2024"
rust-version = "1.94"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
godot = { version = "0.5.5", features = ["api-4-7", "experimental-godot-api"] }
```

Line by line, because three of these are load-bearing:

**`crate-type = ["cdylib"]`** — the important one. By default `cargo new --lib`
produces an `rlib`, a Rust library that only other Rust crates can use. Godot is
written in C++ and needs a plain dynamic library with a C ABI. Get this wrong
and `cargo build` succeeds, produces a `.rlib`, and Godot finds nothing.

**`features = ["api-4-7"]`** — tells gdext which version of Godot's API to
generate bindings against. `api-4-7` covers all of 4.7.x. Pick a *higher* API
level than the Godot you actually run and you will get missing-symbol crashes;
pick a lower one and you simply cannot call the newer functions.

**`features = [..., "experimental-godot-api"]`** — Godot marks some of its own
classes as experimental, and gdext hides those behind this flag rather than
letting you depend on an unstable API by accident. The entire navigation system
is on that list, which we need from Lesson 11 onward. Turning it on now saves a
confusing "no `NavigationAgent3D` in `classes`" error later.

**`publish = false`** — this crate is a game, not a library for crates.io. This
stops you publishing it by accident.

### Step 5 — Write the entry point

Replace `rust/src/lib.rs` entirely:

```rust
use godot::prelude::*;

/// The unit struct that identifies this extension to Godot. It holds no data;
/// its only job is to be the type the `#[gdextension]` macro hangs off.
struct ShooterExtension;

/// `unsafe` because Godot calls into this across an FFI boundary and trusts us
/// to be a well-formed extension. There is nothing for you to get wrong here --
/// write it once and forget it.
#[gdextension]
unsafe impl ExtensionLibrary for ShooterExtension {}
```

Then build it:

```bash
cargo build --manifest-path rust/Cargo.toml
```

The first build downloads gdext, generates Rust bindings for all ~1000 Godot
classes, and compiles them. It takes a minute or two. Later builds take seconds,
because only your own crate is recompiled.

Check that a library appeared:

```bash
ls rust/target/debug/
```

You are looking for `libshooter.so` (Linux), `libshooter.dylib` (macOS), or
`shooter.dll` (Windows).

### Step 6 — Create the Godot project

Launch the editor:

```
godot4
```

The Project Manager opens. Click **Import**... no — click **Create**, then:

- **Project Name:** `Shooter`
- **Project Path:** the `shooter/godot` folder you made in step 3
- **Renderer:** **Forward+**
- **Version Control Metadata:** **None** (we already have git)

> **Which renderer?** Forward+ is the desktop-quality one, with proper shadows
> and screen-space effects. Mobile is a cut-down version, and Compatibility
> targets old hardware and the web. This project targets desktop, so Forward+.
> Changing it later is a project setting, not a rewrite, but the lighting will
> look different.

Click **Create & Edit**. The editor opens on an empty 3D scene.

### Step 6a — Two project settings, now rather than later

Open **Project → Project Settings**. Turn on **Advanced Settings** with the
toggle in the top right, or half of these will not be visible.

Under **Display → Window**:

| Setting | Value |
|---|---|
| Size → Viewport Width | `1920` |
| Size → Viewport Height | `1080` |
| Size → Window Width Override | `1280` |
| Size → Window Height Override | `720` |
| Stretch → Mode | `canvas_items` |
| Stretch → Aspect | `expand` |

The first two say "design everything for 1080p." The overrides say "but run in a
1280×720 window while developing, so the editor and the game both fit on
screen." The stretch settings say "scale the UI to whatever resolution the
player actually has, and give ultrawide monitors more width rather than black
bars."

Do this now because Lesson 18 builds a HUD that assumes it, and a HUD laid out
without it looks fine on your monitor and wrong on everyone else's.

### Step 7 — Write the `.gdextension` file

Close the editor for a moment. Create `shooter/godot/shooter.gdextension` with
this content:

```ini
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.7
reloadable = true

[libraries]
linux.debug.x86_64 =   "res://../rust/target/debug/libshooter.so"
linux.release.x86_64 = "res://../rust/target/release/libshooter.so"
windows.debug.x86_64 =   "res://../rust/target/debug/shooter.dll"
windows.release.x86_64 = "res://../rust/target/release/shooter.dll"
macos.debug =   "res://../rust/target/debug/libshooter.dylib"
macos.release = "res://../rust/target/release/libshooter.dylib"
macos.debug.arm64 =   "res://../rust/target/debug/libshooter.dylib"
macos.release.arm64 = "res://../rust/target/release/libshooter.dylib"
```

What each part does:

**`entry_symbol = "gdext_rust_init"`** — the name of the C function Godot calls
to start your extension. The `#[gdextension]` macro generates a function with
exactly this name. If you mistype it, Godot loads the library and then reports
that it cannot find the entry point.

**`compatibility_minimum = 4.7`** — the oldest Godot this extension claims to
work with. Godot refuses to load an extension that says it needs something newer
than the running engine.

**`reloadable = true`** — lets the editor unload and reload the library when it
changes on disk, so you can rebuild without restarting the editor. It works well
in 4.7 but not perfectly; Appendix B covers when to just restart.

**The `[libraries]` table** — one line per platform and build profile. Note that
the library file is named differently on each platform: Linux and macOS prefix
`lib`, Windows does not; the extensions all differ. Listing all of them now
means a collaborator on another OS can clone and run without editing anything.

**`res://../rust/...`** — `res://` means "the root of the Godot project", so
`res://..` climbs out of it into `shooter/`, and then down into `rust/`. This is
the line that ties the two-folder layout together. If you chose a different
layout, this is the line to change.

### Step 8 — Prove it works

We need something to look for. Add this to the bottom of `rust/src/lib.rs`:

```rust
/// A throwaway class, purely to prove the extension is loading. Lesson 2
/// replaces it with something that actually does a job.
#[derive(GodotClass)]
#[class(base=Node3D, init)]
struct Hello {
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Hello {
    fn ready(&mut self) {
        godot_print!("Hello from Rust.");
    }
}
```

Build it, then import the project once:

```bash
cargo build --manifest-path rust/Cargo.toml
godot4 --headless --path godot --import
```

> **This `--import` step is the trap this lesson exists to prevent.**
>
> Godot only notices a `.gdextension` file during a filesystem scan. If you skip
> this and go straight to running the game, you get:
>
> ```
> ERROR: Cannot get class 'Hello'.
> WARNING: Node Hello of type Hello cannot be created.
>          A placeholder will be created instead.
> ```
>
> which reads like your Rust code is broken. It is not; Godot simply has not
> looked yet. Opening the editor normally does the same scan, so in day-to-day
> work you will rarely need `--import` explicitly. After a fresh `git clone`, or
> right after adding the `.gdextension` file, you do.

Now open the editor:

```
godot4 --path godot
```

In the **Scene** dock click **Other Node**, and type `Hello` in the search box.

Your Rust struct is in Godot's node list. Add it, save the scene as
`res://scenes/main.tscn`, set it as the main scene when prompted, and press
**F5**.

The **Output** dock at the bottom says:

```
Hello from Rust.
```

That is the whole pipeline working: Rust source → `cargo build` → dynamic
library → `.gdextension` → Godot's class database → a node in a scene → a
running game.

### Step 9 — Set up git

At `shooter/`, create `.gitignore`:

```gitignore
# Rust
/rust/target/
*.pdb

# Godot
.godot/
export_presets.cfg

# Editors and OS
.vscode/
.idea/
*.swp
.DS_Store
Thumbs.db
```

`.godot/` is Godot's import cache — machine-generated, large, and it will produce
merge conflicts forever if you commit it. `target/` likewise.

Create `.gitattributes` next to it:

```gitattributes
* text=auto eol=lf

*.png binary
*.jpg binary
*.ogg binary
*.wav binary
*.ttf binary
*.so  binary
*.dll binary
*.dylib binary
```

Godot writes LF line endings on every platform. Without this, a Windows
collaborator's git turns every file into CRLF on checkout and their first commit
is a 4,000-line whitespace diff.

Then:

```bash
git add -A
git commit -m "Lesson 0: Godot 4.7.2 + Rust project skeleton"
```

---

## Check yourself

1. Why does the Rust crate need `crate-type = ["cdylib"]`, and what happens if
   you forget?
2. What is `entry_symbol` in the `.gdextension` file, and who generates the
   function it names?
3. Why is `rust/` a sibling of `godot/` rather than a folder inside it?
4. You clone this project on a new machine, run `cargo build`, then run the
   game, and get "Cannot get class". What did you skip?
5. What does `res://` resolve to, and why does the `.gdextension` file use
   `res://../`?

<details>
<summary>Answers</summary>

1. Godot can only load a dynamic library with a C-compatible interface. The
   default `rlib` is Rust-internal. If you forget, `cargo build` succeeds and
   produces a file Godot cannot load, so you get "Cannot get class" errors with
   no hint about why.
2. The name of the C entry function Godot calls to initialise the extension. The
   `#[gdextension]` macro generates it. Godot cannot find it if the name differs.
3. So that `target/` — hundreds of megabytes and tens of thousands of files —
   is outside the folder Godot's asset importer scans.
4. The import scan. Run `godot4 --headless --path godot --import`, or just open
   the editor once.
5. The root of the Godot project folder. The `.gdextension` file uses `res://../`
   to climb out of the Godot project and into the sibling `rust/` folder where
   cargo puts its output.

</details>

---

## Extend it

- Run `cargo build --release --manifest-path rust/Cargo.toml`, then look at
  `rust/target/release/`. Compare the file size to the debug build. Which entry
  in `[libraries]` would Godot use for it, and when?
- Deliberately break it: change `entry_symbol` to `wrong_name`, re-import, and
  read the error. Change it back. Do the same with `compatibility_minimum = 4.9`.
  Knowing what each failure *looks like* is worth two minutes now and an hour
  later.
- Add a second `#[export]`-free field to `Hello` and print it in `ready`. Does it
  need a default? What does `#[class(init)]` do for you here?

---

**Next:** [Lesson 1 — Nodes and scenes](01-nodes-and-scenes.md)
