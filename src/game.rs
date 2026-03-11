use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use rand::{rngs::StdRng, SeedableRng, Rng};
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseMotion;      // <-- fixes MouseMotion path
// use bevy::window::PrimaryWindow;


use crate::{
    maze::{Maze, ExitMarker},
    stats::{PlayerStats},
    inventory::{BagGrid, Equipment, Chest},
    items::{Item, roll_item, Rarity},
    ui::{UISelection, start_menu_ui, inventory_ui, pause_menu_ui, spell_menu_ui},
};

#[derive(States, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub enum AppState { #[default] Menu, InGame }

#[derive(Component)] pub struct Player;
#[derive(Component)] pub struct PlayerBody;
#[derive(Component)] pub struct ChestMarker;
#[derive(Component)] pub struct Interactable;

#[derive(Resource, Clone)]
pub struct WorldCfg {
    pub maze_w: u32,
    pub maze_h: u32,
    pub tile: f32,
    pub seed: u64,
}

#[derive(Resource, Default)]
pub struct UIFlags {
    pub inventory_open: bool,
    pub spell_menu_open: bool,
    pub pause_menu_open: bool,
}

#[derive(Resource, Default)]
pub struct ChestSettings { pub per_cells: usize } // chest density control

#[derive(Resource, Default)]
pub struct OpenChest(pub Option<Entity>);

#[derive(Component)]
struct MenuCamera;

#[derive(Resource, Clone, Copy)]
pub struct PlayerSpawn(pub Vec3);

#[derive(Resource, Default)]
pub struct PendingRespawn(pub Option<Timer>);

// === Health & Damage ===
#[derive(Component, Clone, Copy, Debug)]
pub struct Health { pub hp: f32, pub max: f32 }
impl Health {
    pub fn new(max: f32) -> Self { Self { hp: max, max } }
    pub fn apply(&mut self, delta: f32) { self.hp = (self.hp + delta).clamp(0.0, self.max); }
    pub fn is_dead(&self) -> bool { self.hp <= 0.0 }
}

// Burn DoT (applied by Fireball)
#[derive(Component)]
pub struct Burn { pub dps: f32, pub timer: Timer }

// === Spells ===
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spell { Fireball, WaterGun, Zap, WindSlash, LightHeal }

#[derive(Resource, Default)]
pub struct ActiveSpell { pub selected: Option<Spell> }

#[derive(Resource)]
pub struct Spellbook {
    pub charges: std::collections::HashMap<Spell, i32>,
    pub cooldowns: std::collections::HashMap<Spell, Timer>,
}

fn feet(ft: f32) -> f32 { ft * 0.3048 } // convert feet to meters

impl Default for Spellbook {
    fn default() -> Self {
        use Spell::*;
        let mut charges = std::collections::HashMap::new();
        charges.insert(Fireball, 5);
        charges.insert(Zap, 10);
        charges.insert(WindSlash, 10);
        charges.insert(WaterGun, 8);
        charges.insert(LightHeal, 5);

        let mut cooldowns = std::collections::HashMap::new();
        cooldowns.insert(Fireball, Timer::from_seconds(15.0, TimerMode::Repeating));
        cooldowns.insert(Zap,      Timer::from_seconds(5.0,  TimerMode::Repeating));
        cooldowns.insert(WindSlash,Timer::from_seconds(5.0,  TimerMode::Repeating));
        cooldowns.insert(WaterGun, Timer::from_seconds(10.0, TimerMode::Repeating));
        cooldowns.insert(LightHeal,Timer::from_seconds(30.0, TimerMode::Repeating));
        Self { charges, cooldowns }
    }
}

// WaterGun active-channel state on the player
#[derive(Component)]
pub struct WaterChannel { pub remaining: f32 }

// === Stash: 100 slots ===
#[derive(Resource, Clone)]
pub struct Stash { pub slots: Vec<Option<crate::items::Item>> }
impl Default for Stash {
    fn default() -> Self { Self { slots: vec![None; 100] } }
}

// Which tab is currently active on the Start screen
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StartMenuTab { Main, Stash }

#[derive(Resource)]
pub struct StartMenuState { pub active: StartMenuTab }

impl Default for StartMenuState {
    fn default() -> Self { Self { active: StartMenuTab::Main } }
}

// === Weapon handling (primary/secondary) ===
#[derive(Resource, Default)]
pub struct ActiveWeapon {
    pub slot: WeaponSlot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponSlot { Primary, Secondary }
impl Default for WeaponSlot { fn default() -> Self { WeaponSlot::Primary } }

#[derive(Component)]
pub struct Parry { pub timer: Timer }

pub struct GamePlugin;
impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app
            .init_state::<AppState>()

            // Existing resources...
            .insert_resource(ClearColor(Color::srgb_u8(0x0f,0x0f,0x13)))
            .insert_resource(Msaa::Sample4)
            .insert_resource(WorldCfg { maze_w: 28, maze_h: 24, tile: 2.0, seed: 1337 })
            .insert_resource(StartMenuState::default())
            .insert_resource(PlayerStats::default())
            .insert_resource(BagGrid::new(3*1, 14))
            .insert_resource(Equipment::default())
            .insert_resource(UISelection::default())
            .insert_resource(ChestSettings { per_cells: 14 })
            .insert_resource(OpenChest::default())
            .init_resource::<UIFlags>()
            // ↓↓↓ ADD THIS (must come before start_menu_ui can run)
            .insert_resource(Stash::default())
            .insert_resource(ActiveSpell::default())
            .insert_resource(Spellbook::default())
            .insert_resource(ActiveWeapon::default())

            // NEW: respawn-related resources (PendingRespawn starts empty; PlayerSpawn is set in setup_world)
            .insert_resource(PendingRespawn::default())

            .add_systems(Startup, spawn_menu_camera)
            .add_systems(Update, close_on_esc_system)
            .add_systems(Update, weapon_input_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, start_menu_ui.run_if(in_state(AppState::Menu)))

            // When entering the game, set up world AND record spawn
            .add_systems(OnEnter(AppState::InGame), (despawn_menu_camera, setup_world))

            // Toggle inventory with Tab
            .add_systems(Update, ui_toggle_system.run_if(in_state(AppState::InGame)))

            // NEW: fall detection + respawn tick while in-game
            .add_systems(Update, (
                spell_recharge_system,
                spell_cast_system,         // LMB casting & channeling
                spell_channel_tick_system, // water gun channel tick
                burn_tick_system,          // Damage over time
                fall_off_map_detector,
                respawn_tick_system,
                weapon_input_system,       // 1/2 select, LMB swing, RMB parry
                player_look_and_move,
                interact_system,
                inventory_ui,
                crate::ui::pause_menu_ui,             // NEW Pause Menu (Esc)
                crate::ui::spell_menu_ui,

            ).run_if(in_state(AppState::InGame)));
    }
}

fn spawn_menu_camera(mut commands: Commands) {
    // 2D camera is fine for showing egui and a clear color
    commands.spawn((Camera2dBundle::default(), MenuCamera));
}

fn despawn_menu_camera(mut commands: Commands, q: Query<Entity, With<MenuCamera>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

pub fn ui_toggle_system(
    kb: Res<ButtonInput<KeyCode>>,
    mut flags: ResMut<UIFlags>,
) {
    // Inventory (Tab)
    if kb.just_pressed(KeyCode::Tab) {
        flags.inventory_open = !flags.inventory_open;
    }
    // Spells (E)
    if kb.just_pressed(KeyCode::KeyE) {
        flags.spell_menu_open = !flags.spell_menu_open;
    }
    // Pause (Esc)
    if kb.just_pressed(KeyCode::Escape) {
        flags.pause_menu_open = !flags.pause_menu_open;
    }
}

pub fn setup_world(
    mut commands: Commands,
    cfg: Res<WorldCfg>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Maze
    let maze = Maze::generate_with_three_exits(cfg.maze_w, cfg.maze_h, cfg.seed);
    let tile = cfg.tile;
    let wall_height = 2.3;
    let wall_thickness = 0.3; // (optional) slightly thicker helps collision robustness
    let floor_mat = materials.add(Color::srgb_u8(0x3a,0x3a,0x42));
    let wall_mat  = materials.add(Color::srgb_u8(0x7b,0x7b,0x89));

    // Floor collider (one big plate) + FIXED body
    let total_x = cfg.maze_w as f32 * tile;
    let total_z = cfg.maze_h as f32 * tile;
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(total_x, 0.2, total_z))),
            material: floor_mat.clone(),
            transform: Transform::from_xyz((total_x - tile)*0.5, -1.1, (total_z - tile)*0.5),
            ..default()
        },
        RigidBody::Fixed,                                // <-- NEW
        Collider::cuboid(total_x*0.5, 0.1, total_z*0.5),
        Name::new("Floor"),
    ));

    // Lights (unchanged)
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight{ illuminance: 35_000.0, shadows_enabled: true, ..default() },
        transform: Transform::from_xyz(20.0, 40.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Walls & floor visuals
    let wall_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile, wall_height, wall_thickness)));
    let half = tile*0.5;

    // Spawn *fixed* colliders for walls
    for y in 0..maze.h as i32 {
        for x in 0..maze.w as i32 {
            let idx = (y as u32 * maze.w + x as u32) as usize;
            let c = maze.cells[idx];
            let center = Vec3::new(x as f32 * tile, 0.0, y as f32 * tile);

            let mut spawn_wall = |pos: Vec3, rotate: bool| {
                let mut t = Transform::from_translation(pos);
                if rotate { t.rotate_y(std::f32::consts::FRAC_PI_2); }

                // Compute collider half-extents depending on rotation
                let hx = if rotate { wall_thickness*0.5 } else { tile*0.5 };
                let hz = if rotate { tile*0.5 } else { wall_thickness*0.5 };

                commands.spawn((
                    PbrBundle {
                        mesh: wall_mesh.clone(),
                        material: wall_mat.clone(),
                        transform: t,
                        ..default()
                    },
                    RigidBody::Fixed,                         // <-- NEW (make it solid/static)
                    Collider::cuboid(hx, wall_height*0.5, hz),
                    Name::new("Wall"),
                ));
            };

            // N,E,S,W
            if c.walls[0] { spawn_wall(center + Vec3::new(0.0,0.0,-half), false); }
            if c.walls[2] { spawn_wall(center + Vec3::new(0.0,0.0, half), false); }
            if c.walls[3] { spawn_wall(center + Vec3::new(-half,0.0,0.0), true); }
            if c.walls[1] { spawn_wall(center + Vec3::new( half,0.0,0.0), true); }
        }
    }

    // Exits (unchanged except optional Name)
    let exits = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new((maze.w as f32 -1.0)*tile, 0.0, 0.0),
        Vec3::new((maze.w as f32 -1.0)*tile, 0.0, (maze.h as f32 -1.0)*tile),
    ];
    let exit_mat = materials.add(Color::srgb_u8(0x3c,0xff,0x9d));
    let exit_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile, 0.5, tile)));
    for epos in exits {
        let eid = commands.spawn(PbrBundle {
            mesh: exit_mesh.clone(),
            material: exit_mat.clone(),
            transform: Transform::from_translation(epos - Vec3::Y*0.85),
            ..default()
        }).id();

        commands.entity(eid)
            .insert(ExitMarker)
            .insert(Collider::cuboid(tile*0.5, 0.25, tile*0.5))
            .insert(Sensor)
            .insert(Name::new("Exit"));
    }

    // Player spawn
    let start = Vec3::new(0.0, 1.7, 0.0); // roughly on floor
    let pid = commands.spawn((
        TransformBundle::from_transform(Transform::from_translation(start)),
        VisibilityBundle::default(),
        Player,
        Health::new(100.0),
        Name::new("Player"),
    )).id();
    commands.insert_resource(PlayerSpawn(start));

    commands.entity(pid)
        .insert(Collider::capsule_y(0.5, 0.3))
        .insert(KinematicCharacterController {
            offset: CharacterLength::Absolute(0.01),
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.35),
                min_width: CharacterLength::Absolute(0.2),
                include_dynamic_bodies: false,
            }),
            snap_to_ground: Some(CharacterLength::Absolute(0.4)),
            ..default()
        });

    commands.entity(pid).with_children(|c| {
        c.spawn(Camera3dBundle {
            transform: Transform::from_xyz(0.0, 0.3, -0.6)
                .looking_at(Vec3::new(0.0, 0.0, 1.0), Vec3::Y),
            ..default()
        });
    });

    // Save the spawn position for respawn:
    commands.insert_resource(PlayerSpawn(start));

    // Spawn chests (density controlled by per_cells)
    let mut rng = StdRng::seed_from_u64(cfg.seed ^ 0xC0FFEE);
    let total_cells = (maze.w * maze.h) as usize;
    let to_spawn = (total_cells / 12).max(8).min(40);

    for _ in 0..to_spawn {
        let x = rng.gen_range(0..maze.w);
        let y = rng.gen_range(0..maze.h);
        let center = Vec3::new(x as f32 * tile, 0.0, y as f32 * tile);

        // random pile of 1..=3 items
        let mut items = vec![];
        let count = rng.gen_range(1..=3);
        for _ in 0..count {
            items.push(roll_item(&mut rng));
        }

        // simple visual cube chest
        let chest_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile*0.35,0.35,tile*0.35)));
        let chest_mat = materials.add(Color::srgb_u8(0xd1,0x9e,0x4a));

        let cid = commands.spawn(PbrBundle {
            mesh: chest_mesh.clone(),
            material: chest_mat.clone(),
            transform: Transform::from_translation(center - Vec3::Y*0.8),
            ..default()
        }).id();

        commands.entity(cid)
            .insert(ChestMarker)
            .insert(Chest { items })
            .insert(Interactable)
            .insert(Collider::cuboid(tile*0.175, 0.175, tile*0.175))
            .insert(Sensor);
    }
}

pub fn player_look_and_move(
    time: Res<Time>,
    kb: Res<ButtonInput<KeyCode>>,
    mut mouse: EventReader<MouseMotion>,
    mut q: Query<&mut Transform, With<Player>>,
    mut ctrl: Query<&mut KinematicCharacterController, With<Player>>,
) {
    // store yaw/pitch directly on transform (simple)
    let mut t = q.single_mut();

    // mouse look
    let sens = 0.0025;
    for ev in mouse.read() {
        let yaw = -ev.delta.x as f32 * sens;
        let pitch = ev.delta.y as f32 * sens;
        let rot_y = Quat::from_axis_angle(Vec3::Y, yaw);
        let rot_x = Quat::from_axis_angle(Vec3::X, pitch);
        t.rotation = rot_y * t.rotation * rot_x;
    }

    // kinematic move
    let mut dir = Vec3::ZERO;
    if kb.pressed(KeyCode::KeyW) { dir -= Vec3::from(t.forward()); }
    if kb.pressed(KeyCode::KeyS) { dir += Vec3::from(t.forward()); }
    if kb.pressed(KeyCode::KeyA) { dir += Vec3::from(t.right());   }
    if kb.pressed(KeyCode::KeyD) { dir -= Vec3::from(t.right());   }
    dir.y = 0.0;

    let mut c = ctrl.single_mut();
    let mut vel = Vec3::ZERO;
    if dir.length_squared() > 0.0 {
        vel = dir.normalize() * 6.0 * time.delta_seconds();
    }
    vel.y -= 2.0 * time.delta_seconds();
    c.translation = Some(vel);
}

pub fn interact_system(
    kb: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<OpenChest>,
    player: Query<&Transform, With<Player>>,
    chests: Query<(Entity, &Transform), (With<ChestMarker>, With<Interactable>)>,
) {
    if !kb.just_pressed(KeyCode::KeyF) { return; }

    let p = player.single();
    let range2 = 2.25f32 * 2.25;

    // Find nearest chest in range
    let mut nearest: Option<(Entity, f32)> = None;
    for (e, t) in chests.iter() {
        let d2 = t.translation.distance_squared(p.translation);
        if d2 < range2 {
            if nearest.map(|(_, m)| d2 < m).unwrap_or(true) {
                nearest = Some((e, d2));
            }
        }
    }

    // Toggle logic:
    // - If pressing F on the same chest that is already open -> close it (None).
    // - If pressing F on a new nearby chest -> open that chest.
    // - If pressing F with no chest in range -> close (None).
    open.0 = match (open.0, nearest.map(|(e, _)| e)) {
        (Some(current), Some(hit)) if current == hit => None,         // close current
        (_, Some(hit))                                 => Some(hit),  // open new chest
        _                                              => None,       // nothing in range -> close
    };
}

fn close_on_esc_system( 
    kb: Res<ButtonInput<KeyCode>>,
    mut flags: ResMut<UIFlags>,
    windows: Query<&Window>,
) {
    // Only react if any window is currently focused
    let focused = windows.iter().any(|w| w.focused);
    if focused && kb.just_pressed(KeyCode::Escape) {
        flags.pause_menu_open = !flags.pause_menu_open; // open/close pause menu
    }

}

pub fn fall_off_map_detector(
    mut pending: ResMut<PendingRespawn>,
    q_player: Query<&Transform, With<Player>>,
) {
    // Don’t stack timers.
    if pending.0.is_some() { return; }

    let Ok(t) = q_player.get_single() else { return; };

    // Fell below the map?
    if t.translation.y < -5.0 {
        // Start a 5-second respawn timer once.
        pending.0 = Some(Timer::from_seconds(5.0, TimerMode::Once));
    }
}

pub fn respawn_tick_system(
    time: Res<Time>,
    spawn: Res<PlayerSpawn>,
    mut pending: ResMut<PendingRespawn>,
    mut q_t: Query<&mut Transform, With<Player>>,
    mut q_ctrl: Query<&mut KinematicCharacterController, With<Player>>,
) {
    let Some(timer) = pending.0.as_mut() else { return; };

    timer.tick(time.delta());
    if !timer.finished() { return; }

    // Teleport player back to the recorded spawn
    if let Ok(mut t) = q_t.get_single_mut() {
        t.translation = spawn.0;
        t.rotation = Quat::IDENTITY;
    }
    if let Ok(mut ctrl) = q_ctrl.get_single_mut() {
        // Clear any residual displacement this frame.
        ctrl.translation = Some(Vec3::ZERO);
    }

    pending.0 = None; // Clear timer
}

pub fn spell_recharge_system(
    time: Res<Time>,
    mut book: ResMut<Spellbook>,
) {
    // First tick all timers and collect the spells that just finished
    let mut ready: Vec<Spell> = Vec::new();
    for (spell, timer) in book.cooldowns.iter_mut() {
        timer.tick(time.delta());
        if timer.just_finished() {
            ready.push(*spell);
        }
    }
    // Then update charges in a separate pass
    for spell in ready {
        *book.charges.entry(spell).or_insert(0) += 1;
    }
}

// Helper: reset all recharge timers to the beginning (called when damage is taken)
fn reset_spell_timers(book: &mut Spellbook) {
    for timer in book.cooldowns.values_mut() {
        timer.reset(); // back to start
    }
}
pub fn burn_tick_system(
    time: Res<Time>,
    mut q: Query<(&mut Health, &mut Burn)>,
    mut book: ResMut<Spellbook>, // to reset on damage for player use-case
    player_q: Query<Entity, With<Player>>,
) {
    let player_e = player_q.get_single().ok();

    for (mut hp, mut burn) in q.iter_mut() {
        burn.timer.tick(time.delta());
        if burn.timer.finished() || burn.timer.paused() {
            // consume final tick, then remove (system can despawn component elsewhere)
            continue;
        }
        // Apply DoT this frame:
        let dps = burn.dps;
        hp.apply(-dps * time.delta_seconds());

        // If target is the player, reset spell timers on damage:
        if let Ok(pe) = player_q.get_single() {
            // A bit rough: if this Health belongs to player, reset
            // (In a richer ECS you'd compare entities; omitted here for brevity.)
            // We'll assume players get their own damage via separate pathway below.
        }
    }
}
pub fn spell_cast_system(
    time: Res<Time>,
    kb: Res<ButtonInput<MouseButton>>,
    flags: Res<UIFlags>,
    mut active: ResMut<ActiveSpell>,
    mut book: ResMut<Spellbook>,
    rapier: Res<RapierContext>,
    cam_q: Query<&GlobalTransform, With<Camera3d>>,
    mut health_sets: ParamSet<( 
        Query<&mut Health>,
        Query<(Entity, &GlobalTransform, &mut Health), With<Player>>,
    )>,
    mut commands: Commands,
) {
    // Don't cast if UI wants pointer (if you want that behavior, we can read EguiContexts here).
    if flags.spell_menu_open || flags.inventory_open || flags.pause_menu_open { return; }
    if !kb.just_pressed(MouseButton::Left) { return; }

    let Some(spell) = active.selected else { return; };

    // Make sure we have the camera forward
    let cam_tf = if let Ok(t) = cam_q.get_single() { t } else { return; };
    let forward: Vec3 = cam_tf.forward().into();
    let origin = cam_tf.translation();

    // --- Borrow the player from Set 1 in its own scope
    let (player_e, player_tf, mut player_hp) = {
        if let Ok((e, tf, hp)) = health_sets.p1().get_single_mut() {
            (e, *tf, hp) // copy tf, keep hp mutable borrow only within this block
        } else {
            return;
        }
    };

    // Utility closures
    let mut try_consume = |s: Spell| -> bool {
        let entry = book.charges.entry(s).or_insert(0);
        if *entry > 0 { *entry -= 1; true } else { false }
    };
    let hitscan = |ctx: &RapierContext, from: Vec3, dir: Vec3, max_dist: f32| -> Option<(Entity, Vec3)> {
        ctx.cast_ray(from, dir.normalize(), max_dist, true, QueryFilter::default())
            .map(|(e, toi)| (e, from + dir.normalize() * toi))
    };

    match spell {
        Spell::Fireball => {
            if !try_consume(Spell::Fireball) { return; }

            // Base impact along forward ray (20 ft)
            let impact = hitscan(&rapier, origin, forward, feet(20.0))
                .map(|(_e, p)| p)
                .unwrap_or(origin + forward * 2.0);

            // Base damage 40 to direct hit (if any)
            if let Some((hit_e, _)) = hitscan(&rapier, origin, forward, feet(20.0)) {
                // Borrow Set 0 to mutate any entity other than player (or including player—see below)
                if let Ok(mut hp) = health_sets.p0().get_mut(hit_e) {
                    hp.apply(-40.0);
                    reset_spell_timers(&mut book); // if you later gate to only player-damage, move this
                } else if hit_e == player_e {
                    // Re-borrow Set 1 briefly for player self-damage
                    if let Ok((_, _, mut php)) = health_sets.p1().get_single_mut() {
                        php.apply(-40.0);
                        reset_spell_timers(&mut book);
                    }
                }
            }

            // Splash: 2 ft radius around impact; add Burn 2 hp/s for 5s
            let radius = feet(2.0);
            rapier.intersections_with_shape(
                impact,
                Quat::IDENTITY,
                &Collider::ball(radius),
                QueryFilter::default(),
                |other_e| {
                    // Damage others via Set 0
                    if let Ok(mut hp) = health_sets.p0().get_mut(other_e) {
                        hp.apply(-20.0);
                        // Apply/refresh burn
                        commands.entity(other_e).insert(Burn {
                            dps: 2.0,
                            timer: Timer::from_seconds(5.0, TimerMode::Once),
                        });
                        reset_spell_timers(&mut book);
                    } else if other_e == player_e {
                        // Short, scoped borrow of Set 1 for player splash/self damage
                        if let Ok((_, _, mut php)) = health_sets.p1().get_single_mut() {
                            php.apply(-20.0);
                            commands.entity(player_e).insert(Burn {
                                dps: 2.0,
                                timer: Timer::from_seconds(5.0, TimerMode::Once),
                            });
                            reset_spell_timers(&mut book);
                        }
                    }
                    true
                }
            );
        }
        Spell::Zap => {
            if !try_consume(Spell::Zap) { return; }
            if let Some((hit_e, _pt)) = hitscan(&rapier, origin, forward, feet(60.0)) {
                if let Ok(mut hp) = health_sets.p0().get_mut(hit_e) {
                    hp.apply(-30.0);
                    reset_spell_timers(&mut book);
                } else if hit_e == player_e {
                    if let Ok((_, _, mut php)) = health_sets.p1().get_single_mut() {
                        php.apply(-30.0);
                        reset_spell_timers(&mut book);
                    }
                }
            }
        }
        Spell::WindSlash => {
            if !try_consume(Spell::WindSlash) { return; }
            if let Some((hit_e, _pt)) = hitscan(&rapier, origin, forward, feet(60.0)) {
                if let Ok(mut hp) = health_sets.p0().get_mut(hit_e) {
                    hp.apply(-35.0);
                    reset_spell_timers(&mut book);
                } else if hit_e == player_e {
                    if let Ok((_, _, mut php)) = health_sets.p1().get_single_mut() {
                        php.apply(-35.0);
                        reset_spell_timers(&mut book);
                    }
                }
            }
        }
        Spell::LightHeal => {
            if !try_consume(Spell::LightHeal) { return; }
            let center = player_tf.translation();
            let radius = feet(3.0);
            rapier.intersections_with_shape(
                center,
                Quat::IDENTITY,
                &Collider::ball(radius),
                QueryFilter::default(),
                |other_e| {
                    if let Ok(mut hp) = health_sets.p0().get_mut(other_e) {
                        hp.apply(25.0);
                    } else if other_e == player_e {
                        if let Ok((_, _, mut php)) = health_sets.p1().get_single_mut() {
                            php.apply(25.0);
                        }
                    }
                    true
                }
            );
        }
        Spell::WaterGun => {
            if !try_consume(Spell::WaterGun) { return; }
            // Start 10s channel on player
            commands.entity(player_e).insert(WaterChannel { remaining: 10.0 });
        }

    }
}

pub fn spell_channel_tick_system(
    time: Res<Time>,
    rapier: Res<RapierContext>,
    cam_q: Query<&GlobalTransform, With<Camera3d>>,
    mut q_player: Query<(Entity, &mut WaterChannel), With<Player>>,
    mut health_sets: ParamSet<(
        Query<&mut Health>,                                    // any entity
        Query<(Entity, &GlobalTransform, &mut Health), With<Player>>,            // player
    )>,
    mut book: ResMut<Spellbook>,
) {
    let Ok((_pe, mut chan)) = q_player.get_single_mut() else { return; };
    let Ok(cam_tf) = cam_q.get_single() else { return; };

    let dt = time.delta_seconds();
    chan.remaining -= dt;
    if chan.remaining <= 0.0 {
        // Remove component by returning early (we’ll remove in a cleanup pass if desired)
        // For brevity: just let remaining go negative and system will no-op next frame.
        return;
    }

    // Hitscan each frame up to 6 ft; deal 10 hp/sec (scaled by dt)
    if let Some((hit_e, _)) = {
        let origin = cam_tf.translation();
        let forward: Vec3 = cam_tf.forward().into();
        rapier.cast_ray(origin, forward.normalize(), feet(6.0), true, QueryFilter::default())
            .and_then(|(e, toi)| Some((e, origin + forward.normalize() * toi)))
    } {
        if let Ok(mut hp) = health_sets.p0().get_mut(hit_e) {
            hp.apply(-10.0 * dt);
            reset_spell_timers(&mut book); // reset timers if damaging player; refine if needed
        }
    }
}
pub fn weapon_input_system(
    kb: Res<ButtonInput<KeyCode>>,
    mb: Res<ButtonInput<MouseButton>>,
    mut active: ResMut<ActiveWeapon>,
    mut player_q: Query<(Entity, &GlobalTransform), With<Player>>,
    rapier: Res<RapierContext>,
    mut q_health: Query<&mut Health>,
    // If you want swing cooldowns/parry times, store them in a resource or component
    mut commands: Commands,
) {
    // 1 = Primary, 2 = Secondary
    if kb.just_pressed(KeyCode::Digit1) { active.slot = WeaponSlot::Primary; }
    if kb.just_pressed(KeyCode::Digit2) { active.slot = WeaponSlot::Secondary; }

    let Ok((_pe, tf)) = player_q.get_single_mut() else { return; };
    // Very simple melee: short ray up to ~1.5 m
    if mb.just_pressed(MouseButton::Left) {
        let origin = tf.translation();
        let dir: Vec3 = tf.forward().into();
        if let Some((hit_e, _pt)) = rapier.cast_ray(origin, dir, 1.5, true, QueryFilter::default()) {
            if let Ok(mut hp) = q_health.get_mut(hit_e) {
                // Example damage; you could scale by weapon rarity/type
                hp.apply(-15.0);
            }
        }
    }
    // Right click: parry for 0.5s
    if mb.just_pressed(MouseButton::Right) {
        if let Ok((pe, _)) = player_q.get_single_mut() {
            commands.entity(pe).insert(Parry { timer: Timer::from_seconds(0.5, TimerMode::Once) });
        }
    }
}