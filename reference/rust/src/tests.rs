//! Headless verification for the whole reference build.
//!
//! Run:  `godot4 --headless --path reference/godot res://tests/tests.tscn`
//!
//! This is a SCENE, not a `--script` tool. Booting a scene normally gives the
//! autoloads a chance to register before anything touches them.
//!
//! Waiting for frames is the one place this file is unusual. GDScript writes
//! `await get_tree().process_frame`; gdext gives the same thing through
//! `godot::task::spawn` plus `Signal::to_future()`, so the test body is a
//! single `async` block rather than a state machine.

use godot::classes::{Camera3D, INode, NavigationServer3D, Node, PackedScene, SceneTree};
use godot::prelude::*;
use godot::task;

use crate::enemy_pool::EnemyPool;
use crate::game_state::GameState;
use crate::interactable::Interactable;
use crate::player::Player;
use crate::player_intent::PlayerIntent;
use crate::round_director::RoundDirector;
use crate::zone::Zone;

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct TestRunner {
    base: Base<Node>,
}

#[godot_api]
impl INode for TestRunner {
    fn ready(&mut self) {
        let this = self.to_gd().upcast::<Node>();
        task::spawn(async move {
            run_all(this).await;
        });
    }
}

/// Mutable across `await` points, so it lives in the async body rather than in
/// the node's fields.
struct Results {
    checks: u32,
    failures: u32,
}

impl Results {
    fn check(&mut self, condition: bool, message: &str) {
        self.checks += 1;
        if condition {
            godot_print!("  ok   {message}");
        } else {
            self.failures += 1;
            godot_print!("  FAIL {message}");
        }
    }
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

async fn next_process_frame(tree: &Gd<SceneTree>) {
    Signal::from_object_signal(tree, "process_frame")
        .to_future::<()>()
        .await;
}

async fn next_physics_frame(tree: &Gd<SceneTree>) {
    Signal::from_object_signal(tree, "physics_frame")
        .to_future::<()>()
        .await;
}

async fn load_main(runner: &Gd<Node>) -> Gd<Node> {
    let scene = load::<PackedScene>("res://scenes/main.tscn")
        .instantiate()
        .expect("main.tscn failed to instantiate");
    let mut runner = runner.clone();
    runner.add_child(&scene);

    let tree = runner.get_tree();
    next_process_frame(&tree).await;
    next_process_frame(&tree).await;
    scene
}

async fn run_all(runner: Gd<Node>) {
    godot_print!("=== reference build checks ===");
    let mut results = Results {
        checks: 0,
        failures: 0,
    };

    player_and_weapon(&runner, &mut results).await;
    loop_systems(&runner, &mut results).await;

    godot_print!("");
    let mut tree = runner.get_tree();
    if results.failures == 0 {
        godot_print!("ALL {} CHECKS PASSED", results.checks);
        tree.quit();
    } else {
        godot_print!("{} of {} CHECKS FAILED", results.failures, results.checks);
        tree.quit_ex().exit_code(1).done();
    }
}

// =========================================================== player and weapon

async fn player_and_weapon(runner: &Gd<Node>, r: &mut Results) {
    godot_print!("\n-- player and weapon --");
    let mut scene = load_main(runner).await;
    let tree = runner.get_tree();

    let Some(player) = scene
        .get_node_or_null("Player")
        .and_then(|n| n.try_cast::<Player>().ok())
    else {
        r.check(false, "Player node present and typed as Player");
        scene.queue_free();
        return;
    };
    r.check(true, "Player node present and typed as Player");

    let (mut camera, mut weapon, health) = {
        let p = player.bind();
        (p.camera.clone(), p.weapon.clone(), p.health.clone())
    };

    r.check(camera.is_instance_valid(), "camera resolved");
    r.check(weapon.is_instance_valid(), "weapon resolved");
    r.check(health.is_instance_valid(), "player has health");
    r.check(player.get_collision_layer() == 2, "player on Player layer");
    r.check(
        player.get_collision_mask() == 1,
        "player collides with World only",
    );

    {
        let w = weapon.bind();
        r.check(
            w.get_in_magazine() == w.magazine_size,
            "magazine starts full",
        );
        r.check(w.hit_mask == 13, "hit mask is World|Enemy|EnemyHitbox");
    }

    // Park an enemy in front of the player and shoot it, through the real path.
    let mut pool = scene.get_node_as::<EnemyPool>("EnemyPool");
    let enemy = pool.bind_mut().spawn(
        Vector3::new(-3.0, 0.1, -6.0),
        player.clone().upcast::<Node3D>(),
        1.0,
        1.0,
    );
    r.check(enemy.is_some(), "spawned a target to shoot");
    let Some(enemy) = enemy else {
        scene.queue_free();
        return;
    };

    let mut player_node = player.clone();
    player_node.set_global_position(Vector3::new(-3.0, 0.1, 0.0));
    player_node.set_rotation(Vector3::ZERO);
    next_physics_frame(&tree).await;
    aim_at(&mut camera, Vector3::new(-3.0, 0.85, -6.0));
    next_physics_frame(&tree).await;

    let enemy_health = enemy.bind().health.clone();
    let before = enemy_health.bind().get_current();

    let intent = PlayerIntent {
        fire_held: true,
        fire_pressed: true,
        ..Default::default()
    };
    weapon.bind_mut().tick(&intent, 1.0 / 60.0);
    next_physics_frame(&tree).await;

    let body_damage = before - enemy_health.bind().get_current();
    {
        let w = weapon.bind();
        r.check(
            w.get_in_magazine() == w.magazine_size - 1,
            "firing consumed a round",
        );
        r.check(
            approx(body_damage, w.damage),
            &format!("body shot dealt base damage ({body_damage:.1})"),
        );
    }

    // The head hitbox is an Area3D -- only registers with collide_with_areas on.
    aim_at(&mut camera, Vector3::new(-3.0, 1.95, -6.0));
    next_physics_frame(&tree).await;
    let before_head = enemy_health.bind().get_current();
    weapon.bind_mut().tick(&intent, 10.0);
    next_physics_frame(&tree).await;
    let head_damage = before_head - enemy_health.bind().get_current();
    {
        let w = weapon.bind();
        r.check(
            approx(head_damage, w.damage * w.headshot_multiplier),
            &format!(
                "headshot dealt {:.1}x damage ({head_damage:.1})",
                w.headshot_multiplier
            ),
        );
    }

    // Reload cycle.
    let reserve_before = weapon.bind().get_reserve();
    let reload_intent = PlayerIntent {
        reload_pressed: true,
        ..Default::default()
    };
    weapon.bind_mut().tick(&reload_intent, 1.0 / 60.0);
    r.check(weapon.bind().is_reloading(), "reload started");

    let reload_seconds = weapon.bind().reload_seconds as f64;
    weapon
        .bind_mut()
        .tick(&PlayerIntent::default(), reload_seconds + 0.1);
    {
        let w = weapon.bind();
        r.check(!w.is_reloading(), "reload finished");
        r.check(w.get_in_magazine() == w.magazine_size, "magazine refilled");
        r.check(
            w.get_reserve() == reserve_before - 2,
            "two rounds taken from reserve",
        );
    }

    // The lesson stages are not used by the game, so nothing else would catch
    // them going stale. Checking they are registered is enough.
    for class_name in ["Lesson04Player", "Lesson05Player"] {
        let registered = godot::classes::ClassDb::singleton().class_exists(class_name);
        r.check(registered, &format!("{class_name} is registered"));
    }

    scene.queue_free();
    next_process_frame(&tree).await;
}

fn aim_at(camera: &mut Gd<Camera3D>, point: Vector3) {
    camera.look_at(point);
}

// ============================================================== loop systems

async fn loop_systems(runner: &Gd<Node>, r: &mut Results) {
    godot_print!("\n-- navigation, pooling, rounds, economy, interactables --");
    let mut scene = load_main(runner).await;
    let tree = runner.get_tree();

    let player = scene.get_node_as::<Player>("Player");
    let mut pool = scene.get_node_as::<EnemyPool>("EnemyPool");
    let mut director = scene.get_node_as::<RoundDirector>("RoundDirector");
    let nav_region = scene.get_node_as::<crate::arena::Arena>("Arena/NavRegion");
    let yard = scene.get_node_as::<Zone>("Arena/NavRegion/ZoneYard");
    let mut door = scene.get_node_as::<Interactable>("Arena/NavRegion/Door/Interactable");
    let mut wall_buy =
        scene.get_node_as::<Interactable>("Arena/NavRegion/ZoneStart/WallBuy/Interactable");

    r.check(
        pool.bind().available_count() == 48,
        "48 enemies pre-instantiated",
    );
    r.check(pool.bind().active_count() == 0, "pool starts empty");

    // --- navigation ---
    let nav_map = nav_region.get_navigation_map();
    // The navigation map syncs on its own schedule; querying it the frame after
    // a bake returns nothing. Give it a few ticks.
    for _ in 0..5 {
        next_physics_frame(&tree).await;
    }
    let start_point = Vector3::new(0.0, 0.0, 8.0);
    let yard_point = Vector3::new(0.0, 0.0, -22.0);

    let inside = NavigationServer3D::singleton().map_get_path(
        nav_map,
        start_point,
        Vector3::new(-10.0, 0.0, -2.0),
        true,
    );
    r.check(
        inside.len() >= 2,
        "navmesh baked: path exists inside the start zone",
    );

    let blocked =
        NavigationServer3D::singleton().map_get_path(nav_map, start_point, yard_point, true);
    let reaches = reaches_target(&blocked, yard_point);
    r.check(
        !reaches,
        "closed zone is unreachable before the door is bought",
    );

    // --- economy ---
    let mut state = GameState::singleton();
    state.bind_mut().start_run();
    r.check(state.bind().points == 0, "run starts at 0 points");
    state.bind_mut().award_points(500, "test".into());
    r.check(state.bind().points == 500, "points awarded");
    r.check(!state.bind_mut().try_spend(9999), "cannot overspend");
    r.check(state.bind().points == 500, "failed purchase costs nothing");
    r.check(
        state.bind_mut().try_spend(200),
        "affordable purchase succeeds",
    );
    r.check(state.bind().points == 300, "points deducted");

    // --- pooled enemy pathing ---
    let enemy = pool.bind_mut().spawn(
        Vector3::new(-10.0, 0.2, 10.0),
        player.clone().upcast::<Node3D>(),
        1.0,
        1.0,
    );
    r.check(enemy.is_some(), "pool spawned an enemy");
    let Some(enemy) = enemy else {
        scene.queue_free();
        return;
    };
    r.check(
        pool.bind().active_count() == 1,
        "active count tracks spawns",
    );
    r.check(
        enemy.is_visible() && enemy.bind().is_active(),
        "spawned enemy is active and visible",
    );

    for _ in 0..4 {
        next_physics_frame(&tree).await;
    }
    let path_len = enemy.bind().agent.get_current_navigation_path().len();
    r.check(path_len > 0, "enemy has a navigation path");

    let start_pos = enemy.get_global_position();
    for _ in 0..40 {
        next_physics_frame(&tree).await;
    }
    let travelled = enemy.get_global_position().distance_to(start_pos);
    r.check(
        travelled > 0.5,
        &format!("enemy moved along its path ({travelled:.2} m)"),
    );

    // --- kill, recycle, reuse ---
    let points_before = state.bind().points;
    let kills_before = state.bind().kills;
    {
        let mut e = enemy.clone();
        e.bind_mut().note_incoming_hit(true);
    }
    let mut enemy_health = enemy.bind().health.clone();
    enemy_health
        .bind_mut()
        .apply_damage(99999.0, Some(player.clone().upcast::<Node>()));
    next_physics_frame(&tree).await;
    r.check(state.bind().kills == kills_before + 1, "kill recorded");
    r.check(state.bind().points > points_before, "kill awarded points");

    let mut waited = 0.0;
    while pool.bind().active_count() > 0 && waited < 3.0 {
        next_physics_frame(&tree).await;
        waited += 1.0 / 60.0;
    }
    r.check(
        pool.bind().active_count() == 0,
        "dead enemy returned to the pool",
    );
    r.check(
        pool.bind().available_count() == 48,
        "pool is whole again -- nothing was freed",
    );

    let reused = pool.bind_mut().spawn(
        Vector3::new(10.0, 0.2, 10.0),
        player.clone().upcast::<Node3D>(),
        2.0,
        1.0,
    );
    r.check(reused.is_some(), "recycled enemy can be spawned again");
    if let Some(reused) = reused {
        let health = reused.bind().health.clone();
        let current = health.bind().get_current();
        r.check(
            approx(current, 300.0),
            &format!("health scale applied on reuse ({current:.0})"),
        );
        r.check(!health.bind().is_dead(), "reused enemy is not still dead");
    }

    // --- interactables ---
    let player_node = player.clone().upcast::<Node3D>();
    state.bind_mut().points = 0;
    door.bind_mut().interact(player_node.clone());
    r.check(
        !yard.bind().is_open(),
        "door does nothing when you cannot afford it",
    );

    state.bind_mut().points = 1000;
    door.bind_mut().interact(player_node.clone());
    r.check(
        yard.bind().is_open(),
        "door opened the zone when affordable",
    );
    r.check(state.bind().points == 250, "door charged 750");

    director.bind_mut().refresh_spawn_points();
    let spawn_points = director.bind().spawn_point_count();
    r.check(
        spawn_points == 5,
        &format!("opened zone contributes spawn points ({spawn_points})"),
    );

    let mut t = 0.0;
    while t < 1.6 {
        next_physics_frame(&tree).await;
        t += 1.0 / 60.0;
    }
    let now = NavigationServer3D::singleton().map_get_path(nav_map, start_point, yard_point, true);
    r.check(
        reaches_target(&now, yard_point),
        "navmesh rebaked: yard reachable after buying the door",
    );

    // --- wall buy ---
    let mut weapon = player.bind().weapon.clone();
    weapon.bind_mut().reserve_ammo = 10;
    state.bind_mut().points = 1000;
    wall_buy.bind_mut().interact(player_node);
    let reserve = weapon.bind().get_reserve();
    r.check(
        reserve == 130,
        &format!("wall buy granted ammo ({reserve})"),
    );
    r.check(state.bind().points == 500, "wall buy charged 500");
    r.check(
        wall_buy
            .bind()
            .can_interact(player.clone().upcast::<Node3D>()),
        "wall buy is reusable",
    );

    scene.queue_free();
    next_process_frame(&tree).await;
}

fn reaches_target(path: &PackedVector3Array, target: Vector3) -> bool {
    match path.as_slice().last() {
        Some(last) => last.distance_to(target) < 3.0,
        None => false,
    }
}
