# Appendix A — gdext reference card

Everything you need often, in one place. Most Godot documentation and nearly all
Godot tutorials are written in GDScript, so the translation table is the part you
will reach for most.

---

## Declaring a class

<!-- illustrative -->
```rust
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct Thing {
    #[export]
    #[init(val = 1.0)]
    speed: f32,

    internal: f32,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Thing {
    fn ready(&mut self) {}
    fn process(&mut self, delta: f64) {}
}

#[godot_api]
impl Thing {
    #[signal]
    fn something_happened(amount: f32);

    #[func]
    fn callable_from_godot(&self) -> i32 { 0 }
}

impl Thing {
    fn rust_only(&self) {}      // no attribute: invisible to Godot
}
```

### `#[class(...)]` options

| Option | Effect |
|---|---|
| `base = Node3D` | what it extends. **Engine classes only** |
| `init` | generate a constructor from `#[init(val = ...)]` defaults |
| `tool` | also run inside the editor |
| `rename = "Other"` | expose under a different name to Godot |

Without `init` you write it yourself:

```rust
fn init(base: Base<Node3D>) -> Self { Self { /* ... */ base } }
```

### Field visibility

| Attribute | Inspector | Godot / scenes | For |
|---|---|---|---|
| `#[export]` | yes | yes | anything tuned by feel |
| `#[var]` | no | yes | state other code reads |
| *(none)* | no | no | internal bookkeeping |

### `#[export]` hints

```rust
#[export(range = (0.0, 10.0, 0.1))]     // slider: min, max, step
#[export(flags_3d_physics)]             // named collision-layer checkboxes
#[export(file = "*.tres")]              // file picker
```

### `#[init(...)]` forms

```rust
#[init(val = 5.0)]                       // a default value
#[init(node = "Head/Camera3D")]          // resolve a node in `ready`
#[init(load = "res://scenes/x.tscn")]    // load a resource at construction
```

---

## GDScript → Rust

### Declarations

| GDScript | Rust |
|---|---|
| `extends Node3D` | `#[class(base=Node3D)]` |
| `class_name Thing` | the struct's name |
| `@tool` | `#[class(tool)]` |
| `@export var x := 1.0` | `#[export] #[init(val = 1.0)] x: f32` |
| `@export_range(0,10) var x` | `#[export(range = (0.0, 10.0))] x: f32` |
| `@onready var h = $Head` | `#[init(node = "Head")] h: OnReady<Gd<Node3D>>` |
| `const S = preload("...")` | `#[init(load = "...")] s: OnReady<Gd<PackedScene>>` |
| `signal hit(a: float)` | `#[signal] fn hit(a: f32);` |
| `func f() -> int` | `#[func] fn f(&self) -> i32` |
| `static func f()` | a free `pub fn` in the module |
| `enum State { A, B }` | `enum State { A, B }` + derives |

### Lifecycle

| GDScript | Rust |
|---|---|
| `_ready()` | `fn ready(&mut self)` |
| `_process(delta)` | `fn process(&mut self, delta: f64)` |
| `_physics_process(delta)` | `fn physics_process(&mut self, delta: f64)` |
| `_input(event)` | `fn input(&mut self, event: Gd<InputEvent>)` |
| `_unhandled_input(event)` | `fn unhandled_input(&mut self, event: Gd<InputEvent>)` |
| `_exit_tree()` | `fn exit_tree(&mut self)` |

### Reaching the engine

| GDScript | Rust |
|---|---|
| `position` | `self.base().get_position()` / `set_position(v)` |
| `velocity` | `self.base().get_velocity()` / `set_velocity(v)` |
| `move_and_slide()` | `self.base_mut().move_and_slide()` |
| `is_on_floor()` | `self.base().is_on_floor()` |
| `rotate_y(a)` | `self.base_mut().rotate_y(a)` |
| `queue_free()` | `self.base_mut().queue_free()` |
| `get_tree()` | `self.base().get_tree()` |
| `self` (as a reference) | `self.to_gd()` |
| `$Path` | `self.base().get_node_as::<T>("Path")` |
| `%UniqueName` | `self.base().get_node_as::<T>("%UniqueName")` |

### Signals

| GDScript | Rust |
|---|---|
| `hit.emit(5.0)` | `self.signals().hit().emit(5.0)` |
| `x.hit.connect(_on_hit)` | `x.signals().hit().connect_other(&this, Self::on_hit)` |
| `hit.connect(_on_hit)` | `self.signals().hit().connect_self(Self::on_hit)` |
| `await get_tree().process_frame` | `Signal::from_object_signal(&tree, "process_frame").to_future::<()>().await` |

### Types and maths

| GDScript | Rust |
|---|---|
| `Vector3(1,2,3)` | `Vector3::new(1.0, 2.0, 3.0)` |
| `Vector3.UP` | `Vector3::UP` |
| `Color(1,0,0)` | `Color::from_rgb(1.0, 0.0, 0.0)` |
| `"text"` | `GString::from("text")` or `"text".into()` |
| `sin(x)` | `x.sin()` |
| `deg_to_rad(x)` | `x.to_radians()` |
| `clampf(x, a, b)` | `x.clamp(a, b)` |
| `maxf(a, b)` | `a.max(b)` |
| `lerpf(a, b, t)` | `a + (b - a) * t` |
| `randf_range(a, b)` | `godot::global::randf_range(a, b)` |
| `randi()` | `godot::global::randi()` |
| `print("x", y)` | `godot_print!("x{y}")` |
| `push_warning(s)` | `godot_warn!("{s}")` |
| `push_error(s)` | `godot_error!("{s}")` |
| `basis.z` | `basis.col_c()` |
| `Array[T]` | `Array<T>`, or `Vec<T>` if Godot never sees it |
| `Dictionary` | `Dictionary` (untyped `Variant`s) |

Not provided by gdext — write them yourself:

```rust
pub fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}
```

```rust
fn lerp_angle(from: f32, to: f32, weight: f32) -> f32 {
    let difference = (to - from) % std::f32::consts::TAU;
    let distance = (2.0 * difference) % std::f32::consts::TAU - difference;
    from + distance * weight
}
```

### Casting and nullability

| Need | Rust |
|---|---|
| downcast, panic on failure | `node.cast::<T>()` |
| downcast, recoverable | `node.try_cast::<T>()` → `Result<Gd<T>, Gd<U>>` |
| upcast | `node.upcast::<Node>()` |
| is it in a group | `node.is_in_group("name")` |
| is the object still alive | `node.is_instance_valid()` |
| nullable child | `get_node_or_null(p)` → `Option<Gd<Node>>` |

---

## The four things that trip everyone up

### 1. `base_mut()` borrows all of `self`

```rust
self.base_mut().rotate_y(self.speed * d);      // ERROR
let step = self.speed * d;                     // read first
self.base_mut().rotate_y(step);                // then write
```

### 2. One `signals()` call per signal

```rust
let s = x.signals();
s.a().connect_other(..);
s.b().connect_other(..);      // PANICS at runtime
```

```rust
x.signals().a().connect_other(..);
x.signals().b().connect_other(..);      // correct
```

### 3. Scope your borrows

<!-- illustrative -->
```rust
{
    let mut h = self.health.bind_mut();
    h.max_health = 150.0;
    h.reset();
}   // released here, before anything that might call back
```

### 4. Where engine enums live

`godot::classes::<snake_case_class>::<EnumName>`:

<!-- illustrative -->
```rust
use godot::classes::input::MouseMode;
use godot::classes::node::ProcessMode;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::audio_stream_wav::Format;
```

Not `godot::global`, not the prelude, not `godot::classes::MouseMode`.

---

## Builder methods

Godot methods with optional arguments get an `_ex()` variant:

<!-- illustrative -->
```rust
self.base_mut()
    .bake_navigation_mesh_ex()
    .on_thread(false)
    .done();

let markers = root
    .find_children_ex("*")
    .type_("Marker3D")
    .owned(false)       // include nodes owned by instanced sub-scenes
    .done();
```

The plain form uses all defaults. Note `type_` with a trailing underscore, because
`type` is a Rust keyword.

---

## Constructing engine objects

| Kind | Constructor | Freed by |
|---|---|---|
| Resource (refcounted) | `BoxMesh::new_gd()` | refcount |
| Node (manually managed) | `Timer::new_alloc()` | `queue_free()` |
| From a scene | `scene.instantiate_as::<T>()` | `queue_free()` |
| Duplicate a resource | `res.duplicate_resource()` | refcount |

---

## Tweens

<!-- illustrative -->
```rust
let mut tween = self.base_mut().create_tween();
tween.tween_interval(0.5);
tween.tween_property(&node, "position:y", &2.0f32.to_variant(), 0.3)
    .set_trans(TransitionType::CUBIC)
    .set_ease(EaseType::IN);
tween.tween_callback(&Callable::from_object_method(&self.to_gd(), "done"));
tween.set_parallel();      // subsequent steps run together, not in sequence
```

Property paths use `:` for a component: `"modulate:a"`, `"rotation_degrees:x"`.

Callback targets must be `#[func]`, and the method name is a **string** — a typo
compiles and silently never fires.

---

## Autoloads

A gdext class cannot be an autoload directly; autoload a one-line scene:

```ini
[gd_scene format=3]

[node name="EventBus" type="EventBus"]
```

and reach it from anywhere:

```rust
pub(crate) fn autoload<T: Inherits<Node>>(name: &str) -> Gd<T> {
    Engine::singleton()
        .get_main_loop()
        .expect("no main loop")
        .cast::<SceneTree>()
        .get_root()
        .get_node_as::<T>(name)
}
```

---

## What gdext will not do

| Want | Reality |
|---|---|
| Extend your own class | Not supported. Use a component (Lesson 17) |
| Attach a Rust "script" to a node | There are no scripts. Your class *is* a node type |
| `await` a signal in a normal method | Use a `Callable`, or `godot::task::spawn` for an async block |
| A class rename that updates scenes | `.tscn` files reference classes by name. Grep them |
| Experimental APIs by default | Enable `experimental-godot-api` |
