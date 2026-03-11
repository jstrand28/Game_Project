use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use bevy_egui::EguiPlugin;
use rand::{rngs::StdRng, SeedableRng, Rng};
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseMotion;      // <-- fixes MouseMotion path
// use bevy::window::PrimaryWindow;


use crate::{
    maze::{Maze, ExitMarker},
    stats::{PlayerStats},
    inventory::{BagGrid, Equipment, Chest},
    items::{Item, roll_item, Rarity},
    ui::{UISelection, start_menu_ui, inventory_ui},
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
pub struct ChestSettings { pub per_cells: usize } // chest density control

#[derive(Resource, Default)]
pub struct OpenChest(pub Option<Entity>);

#[derive(Component)]
struct MenuCamera;

pub struct GamePlugin;
impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app
        .init_state::<AppState>()

        .insert_resource(ClearColor(Color::srgb_u8(0x0f,0x0f,0x13)))
        .insert_resource(Msaa::Sample4)
        .insert_resource(WorldCfg { maze_w: 28, maze_h: 24, tile: 2.0, seed: 1337 })

        .insert_resource(PlayerStats::default())
        .insert_resource(BagGrid::new(3*1, 14)) // base 3x14 (you asked 3*14)
        .insert_resource(Equipment::default())
        .insert_resource(UISelection::default())
        .insert_resource(ChestSettings { per_cells: 14 })
        .insert_resource(OpenChest::default()) 
        .add_systems(Startup, spawn_menu_camera)
        .add_systems(Update, close_on_esc_system)
        .add_systems(Update, start_menu_ui.run_if(in_state(AppState::Menu)))
        .add_systems(OnEnter(AppState::InGame), (despawn_menu_camera, setup_world))
        .add_systems(Update, (
            player_look_and_move,
            interact_system,
            inventory_ui,
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
    let wall_thickness = 0.2;
    let floor_mat = materials.add(Color::srgb_u8(0x3a,0x3a,0x42));
    let wall_mat  = materials.add(Color::srgb_u8(0x7b,0x7b,0x89));

    // Floor collider (one big plate)
    let total_x = cfg.maze_w as f32 * tile;
    let total_z = cfg.maze_h as f32 * tile;
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(total_x, 0.2, total_z))),
            material: floor_mat.clone(),
            transform: Transform::from_xyz((total_x - tile)*0.5, -1.1, (total_z - tile)*0.5),
            ..default()
        },
        Collider::cuboid(total_x*0.5, 0.1, total_z*0.5),
    ));

    // Lights
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight{ illuminance: 35_000.0, shadows_enabled: true, ..default() },
        transform: Transform::from_xyz(20.0, 40.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Walls & floor visuals
    let wall_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile, wall_height, wall_thickness)));
    let floor_tile = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile, 0.2, tile)));
    let half = tile*0.5;

    for y in 0..maze.h as i32 {
        for x in 0..maze.w as i32 {
            let idx = (y as u32 * maze.w + x as u32) as usize;
            let c = maze.cells[idx];
            let center = Vec3::new(x as f32 * tile, 0.0, y as f32 * tile);

            // walls with colliders
            let mut spawn_wall = |pos: Vec3, rotate: bool| {
                let mut t = Transform::from_translation(pos);
                if rotate { t.rotate_y(std::f32::consts::FRAC_PI_2); }
                let eid = commands.spawn(PbrBundle {
                    mesh: wall_mesh.clone(),
                    material: wall_mat.clone(),
                    transform: t,
                    ..default()
                }).id();

                commands.entity(eid).insert(Collider::cuboid(
                    if rotate { wall_thickness*0.5 } else { tile*0.5 },
                    wall_height*0.5,
                    if rotate { tile*0.5 } else { wall_thickness*0.5 },
                ));
            };
            // N,E,S,W
            if c.walls[0] { spawn_wall(center + Vec3::new(0.0,0.0,-half), false); }
            if c.walls[2] { spawn_wall(center + Vec3::new(0.0,0.0, half), false); }
            if c.walls[3] { spawn_wall(center + Vec3::new(-half,0.0,0.0), true); }
            if c.walls[1] { spawn_wall(center + Vec3::new( half,0.0,0.0), true); }
        }
    }

    // Place 3 exits (visual markers) on approximate corners
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
            .insert(Sensor);
    }

    // Player spawn
    let start = Vec3::new(0.0, 1.7, 0.0); // roughly on floor
    let pid = commands.spawn((
        TransformBundle::from_transform(Transform::from_translation(start)),
        VisibilityBundle::default(),
        Player,
    )).id();

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
        let pitch = -ev.delta.y as f32 * sens;
        let rot_y = Quat::from_axis_angle(Vec3::Y, yaw);
        let rot_x = Quat::from_axis_angle(Vec3::X, pitch);
        t.rotation = rot_y * t.rotation * rot_x;
    }

    // kinematic move
    let mut dir = Vec3::ZERO;
    if kb.pressed(KeyCode::KeyW) { dir += Vec3::from(t.forward()); }
    if kb.pressed(KeyCode::KeyS) { dir -= Vec3::from(t.forward()); }
    if kb.pressed(KeyCode::KeyA) { dir -= Vec3::from(t.right());   }
    if kb.pressed(KeyCode::KeyD) { dir += Vec3::from(t.right());   }
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
    // toggle open nearest chest within 1.5m
    let mut nearest: Option<(Entity, f32)> = None;
    for (e, t) in chests.iter() {
        let d2 = t.translation.distance_squared(p.translation);
        if d2 < range2 {
            if nearest.map(|(_,m)| d2<m).unwrap_or(true) {
                nearest = Some((e, d2));
            }
        }
    }
    open.0 = nearest.map(|(e,_)| Some(e)).unwrap_or(None);
}

fn close_on_esc_system( 
    kb: Res<ButtonInput<KeyCode>>,
    mut exit: EventWriter<AppExit>,
    windows: Query<&Window>,
) {
    // Only react if any window is currently focused
    let focused = windows.iter().any(|w| w.focused);
    if focused && kb.just_pressed(KeyCode::Escape) {
        exit.send(AppExit::Success);
    }

}
