use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_egui::{EguiPrimaryContextPass, PrimaryEguiContext};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy_rapier3d::prelude::*;
use rand::{rngs::StdRng, SeedableRng, RngExt};
use std::{collections::HashSet, sync::Arc};
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseMotion;      // <-- fixes MouseMotion path
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};


use crate::{
    maze::{Maze, ExitMarker},
    persistence::{load_inventory_persistence, save_inventory_on_app_exit},
    stats::PlayerStats,
    inventory::{BagGrid, Equipment, Chest, ChestTier},
    items::{roll_item, Item, WeaponKind},
    ui::{UISelection, start_menu_ui, inventory_ui, stamina_hud_ui, player_health_hud_ui, enemy_health_bars_ui, damage_popup_ui},
};

#[derive(States, Clone, Eq, PartialEq, Debug, Hash, Default)]
pub enum AppState { #[default] Menu, InGame }

#[derive(Component)] pub struct Player;
#[derive(Component)] pub struct PlayerBody;
#[derive(Component)] pub struct PlayerCamera;
#[derive(Component)] pub struct PlayerAvatar;
#[derive(Component)] pub struct FirstPersonViewModel;
#[derive(Component)] pub struct ChestMarker;
#[derive(Component)] pub struct Interactable;
#[derive(Component)] struct CollisionDebugVisual;
#[derive(Component)] struct SnagMarker;
#[derive(Component)] struct SkeletonEnemy {
    variant: SkeletonVariant,
    home: Vec3,
    attack_timer: Timer,
}
#[derive(Component)] struct EnemyProjectile {
    velocity: Vec3,
    damage: f32,
    lifetime: Timer,
    owner: Entity,
    kind: EnemyProjectileKind,
}
#[derive(Component)] struct SpellVisualEffect {
    lifetime: Timer,
    drift: Vec3,
}
#[derive(Component)] pub struct DamagePopup {
    pub amount: f32,
    pub lifetime: Timer,
    pub positive: bool,
}
#[derive(Component)] struct FireballSpellProjectile {
    velocity: Vec3,
    lifetime: Timer,
}
#[derive(Component)] struct BurnVisual {
    owner: Entity,
    phase: f32,
}
#[derive(Component)] struct WaterStreamVisual;
#[derive(Component)] struct EnemyHitRecoil {
    velocity: Vec3,
    remaining: f32,
    tilt_axis: Vec3,
    tilt: f32,
}
#[derive(Component)]
struct GoldPickup {
    amount: i32,
    bob_phase: f32,
    base_y: f32,
    spin_speed: f32,
    magnet_radius: f32,
}
#[derive(Component)] pub struct EnemyHealthBarAnchor;

#[derive(Resource, Default)]
struct HitStopState {
    remaining: f32,
    move_scale: f32,
}

impl HitStopState {
    fn trigger(&mut self, duration: f32, move_scale: f32) {
        if duration > self.remaining {
            self.remaining = duration;
            self.move_scale = move_scale.clamp(0.0, 1.0);
        }
    }

    fn time_scale(&self) -> f32 {
        if self.remaining > 0.0 { self.move_scale } else { 1.0 }
    }
}

#[derive(Resource, Default)]
struct CameraImpulseState {
    translation: Vec3,
    pitch: f32,
    roll: f32,
}

impl CameraImpulseState {
    fn trigger(&mut self, translation: Vec3, pitch: f32, roll: f32) {
        self.translation += translation;
        self.pitch += pitch;
        self.roll += roll;
    }

    fn decay(&mut self, dt: f32) {
        let decay = (-10.0 * dt).exp();
        self.translation *= decay;
        self.pitch *= decay;
        self.roll *= decay;
        if self.translation.length_squared() < 0.00001 {
            self.translation = Vec3::ZERO;
        }
        if self.pitch.abs() < 0.0001 {
            self.pitch = 0.0;
        }
        if self.roll.abs() < 0.0001 {
            self.roll = 0.0;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SkeletonVariant {
    Archer,
    Knight,
    SwordShield,
    Guard,
    Mage,
    King,
}

#[derive(Clone, Copy, Debug)]
enum EnemyProjectileKind {
    Arrow,
    Fireball,
    Zap,
    WindSlash,
}

#[derive(Resource, Clone)]
struct EnemyProjectileAssets {
    arrow_mesh: Handle<Mesh>,
    bolt_mesh: Handle<Mesh>,
    fire_material: Handle<StandardMaterial>,
    zap_material: Handle<StandardMaterial>,
    wind_material: Handle<StandardMaterial>,
    arrow_material: Handle<StandardMaterial>,
}

#[derive(Resource, Clone)]
struct SpellVisualAssets {
    orb_mesh: Handle<Mesh>,
    beam_mesh: Handle<Mesh>,
    slash_mesh: Handle<Mesh>,
    cross_mesh: Handle<Mesh>,
    ring_mesh: Handle<Mesh>,
    fireball_material: Handle<StandardMaterial>,
    burn_material: Handle<StandardMaterial>,
    heal_material: Handle<StandardMaterial>,
    zap_material: Handle<StandardMaterial>,
    wind_material: Handle<StandardMaterial>,
    water_material: Handle<StandardMaterial>,
}

#[derive(Resource, Clone)]
struct GoldPickupAssets {
    coin_mesh: Handle<Mesh>,
    common_material: Handle<StandardMaterial>,
    martial_material: Handle<StandardMaterial>,
    arcane_material: Handle<StandardMaterial>,
    royal_material: Handle<StandardMaterial>,
}

#[derive(Resource, Clone)]
struct ProceduralTextureAssets {
    skin: Handle<Image>,
    cloth: Handle<Image>,
    leather: Handle<Image>,
    hair: Handle<Image>,
    steel: Handle<Image>,
    dark_steel: Handle<Image>,
    brass: Handle<Image>,
    wood: Handle<Image>,
    glass: Handle<Image>,
    paper: Handle<Image>,
    ember: Handle<Image>,
}

fn texture_hash(x: u32, y: u32, seed: u32) -> u32 {
    let mut value = x.wrapping_mul(374_761_393)
        .wrapping_add(y.wrapping_mul(668_265_263))
        .wrapping_add(seed.wrapping_mul(362_437));
    value = (value ^ (value >> 13)).wrapping_mul(1_274_126_177);
    value ^ (value >> 16)
}

fn texture_noise(x: u32, y: u32, seed: u32) -> f32 {
    (texture_hash(x, y, seed) & 0xff) as f32 / 255.0
}

fn texture_rgba(r: f32, g: f32, b: f32, a: f32) -> [u8; 4] {
    [
        (r.clamp(0.0, 1.0) * 255.0) as u8,
        (g.clamp(0.0, 1.0) * 255.0) as u8,
        (b.clamp(0.0, 1.0) * 255.0) as u8,
        (a.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

fn procedural_texture<F>(
    images: &mut Assets<Image>,
    width: u32,
    height: u32,
    mut pixel: F,
) -> Handle<Image>
where
    F: FnMut(u32, u32) -> [u8; 4],
{
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    for y in 0..height {
        for x in 0..width {
            if let Some(pixel_bytes) = image.pixel_bytes_mut(UVec3::new(x, y, 0)) {
                pixel_bytes.copy_from_slice(&pixel(x, y));
            }
        }
    }

    images.add(image)
}

fn create_procedural_texture_assets(images: &mut Assets<Image>) -> ProceduralTextureAssets {
    let skin = procedural_texture(images, 64, 64, |x, y| {
        let pore = texture_noise(x, y, 11);
        let blush = texture_noise(x / 3 + 17, y / 3 + 9, 12);
        let shadow = ((y as f32 / 63.0) - 0.5).abs() * 0.04;
        texture_rgba(
            0.71 + pore * 0.09 + blush * 0.03 - shadow,
            0.59 + pore * 0.06 + blush * 0.02 - shadow * 0.6,
            0.50 + pore * 0.05,
            1.0,
        )
    });
    let cloth = procedural_texture(images, 64, 64, |x, y| {
        let weave = if x % 8 == 0 || y % 8 == 0 { -0.06 } else { 0.0 };
        let fleck = texture_noise(x, y, 21) * 0.09;
        texture_rgba(0.17 + weave + fleck, 0.2 + weave * 0.5 + fleck, 0.26 + fleck * 1.1, 1.0)
    });
    let leather = procedural_texture(images, 64, 64, |x, y| {
        let grain = texture_noise(x / 2 + 5, y / 2 + 7, 31);
        let crease = if (x + y) % 19 == 0 { -0.08 } else { 0.0 };
        texture_rgba(0.24 + grain * 0.12 + crease, 0.14 + grain * 0.07 + crease * 0.4, 0.08 + grain * 0.05, 1.0)
    });
    let hair = procedural_texture(images, 64, 64, |x, y| {
        let sheen = (((x as f32 * 0.26) + (y as f32 * 0.08)).sin() * 0.5 + 0.5) * 0.08;
        let strand = texture_noise(x, y, 41) * 0.05;
        texture_rgba(0.08 + sheen + strand, 0.055 + sheen * 0.7 + strand * 0.7, 0.04 + strand * 0.5, 1.0)
    });
    let steel = procedural_texture(images, 64, 64, |x, y| {
        let brush = texture_noise(x * 3 + 9, y / 3 + 2, 51) * 0.16;
        let edge = if y % 16 == 0 { 0.08 } else { 0.0 };
        texture_rgba(0.48 + brush + edge, 0.5 + brush * 0.95 + edge, 0.54 + brush * 0.9 + edge, 1.0)
    });
    let dark_steel = procedural_texture(images, 64, 64, |x, y| {
        let brush = texture_noise(x * 2 + 3, y / 3 + 11, 61) * 0.12;
        let temper = if (x + y) % 23 == 0 { 0.05 } else { 0.0 };
        texture_rgba(0.22 + brush + temper, 0.24 + brush * 0.9 + temper, 0.28 + brush + temper * 0.6, 1.0)
    });
    let brass = procedural_texture(images, 64, 64, |x, y| {
        let polish = texture_noise(x * 2 + 13, y * 2 + 7, 71) * 0.12;
        let tarnish = if (x / 7 + y / 5) % 5 == 0 { -0.05 } else { 0.0 };
        texture_rgba(0.63 + polish + tarnish, 0.52 + polish * 0.85 + tarnish, 0.22 + polish * 0.4, 1.0)
    });
    let wood = procedural_texture(images, 64, 64, |x, y| {
        let grain = (((x as f32 * 0.32) + texture_noise(x, y, 81) * 6.0).sin() * 0.5 + 0.5) * 0.18;
        let pore = texture_noise(x / 2 + 9, y / 2 + 15, 82) * 0.08;
        texture_rgba(0.24 + grain + pore, 0.15 + grain * 0.55 + pore * 0.8, 0.08 + pore * 0.7, 1.0)
    });
    let glass = procedural_texture(images, 64, 64, |x, y| {
        let diag = (((x as f32 + y as f32) * 0.18).sin() * 0.5 + 0.5) * 0.12;
        let cloud = texture_noise(x, y, 91) * 0.08;
        texture_rgba(0.44 + diag, 0.63 + diag * 0.7 + cloud, 0.82 + cloud, 0.86)
    });
    let paper = procedural_texture(images, 64, 64, |x, y| {
        let fiber = texture_noise(x, y, 101) * 0.08;
        let edge = if x < 2 || y < 2 || x > 61 || y > 61 { -0.08 } else { 0.0 };
        texture_rgba(0.74 + fiber + edge, 0.7 + fiber * 0.9 + edge, 0.58 + fiber * 0.7 + edge * 0.5, 1.0)
    });
    let ember = procedural_texture(images, 64, 64, |x, y| {
        let center = Vec2::new(31.5, 31.5);
        let distance = Vec2::new(x as f32, y as f32).distance(center) / 31.5;
        let flicker = texture_noise(x, y, 111) * 0.2;
        let heat = (1.0 - distance).clamp(0.0, 1.0);
        texture_rgba(0.78 + heat * 0.22 + flicker, 0.22 + heat * 0.5 + flicker * 0.5, 0.04 + heat * 0.16, 1.0)
    });

    ProceduralTextureAssets {
        skin,
        cloth,
        leather,
        hair,
        steel,
        dark_steel,
        brass,
        wood,
        glass,
        paper,
        ember,
    }
}

fn avatar_part_texture(textures: &ProceduralTextureAssets, part: AvatarPart) -> Handle<Image> {
    match part {
        AvatarPart::Head | AvatarPart::LeftArm | AvatarPart::RightArm => textures.skin.clone(),
        AvatarPart::Torso | AvatarPart::LeftLeg | AvatarPart::RightLeg | AvatarPart::Shirt | AvatarPart::Pants | AvatarPart::Hat | AvatarPart::Cape => textures.cloth.clone(),
        AvatarPart::Shoes | AvatarPart::Gloves | AvatarPart::Bag => textures.leather.clone(),
        AvatarPart::Necklace | AvatarPart::Watch => textures.steel.clone(),
        AvatarPart::MainHand | AvatarPart::OffHand => textures.cloth.clone(),
    }
}

fn weapon_surface_texture(textures: &ProceduralTextureAssets, surface: WeaponSurface) -> Handle<Image> {
    match surface {
        WeaponSurface::Steel => textures.steel.clone(),
        WeaponSurface::DarkSteel => textures.dark_steel.clone(),
        WeaponSurface::Wood => textures.wood.clone(),
        WeaponSurface::Leather => textures.leather.clone(),
        WeaponSurface::Brass => textures.brass.clone(),
        WeaponSurface::Glass => textures.glass.clone(),
        WeaponSurface::Paper => textures.paper.clone(),
        WeaponSurface::Ember => textures.ember.clone(),
    }
}

#[derive(Resource)]
pub struct EnemyRuntimeRng(StdRng);

#[derive(Clone)]
struct SkeletonVisualAssets {
    skull_mesh: Handle<Mesh>,
    rib_mesh: Handle<Mesh>,
    arm_mesh: Handle<Mesh>,
    leg_mesh: Handle<Mesh>,
    pelvis_mesh: Handle<Mesh>,
    bow_mesh: Handle<Mesh>,
    sword_mesh: Handle<Mesh>,
    greatsword_mesh: Handle<Mesh>,
    shield_mesh: Handle<Mesh>,
    axe_mesh: Handle<Mesh>,
    staff_mesh: Handle<Mesh>,
    crown_mesh: Handle<Mesh>,
    bone_material: Handle<StandardMaterial>,
    metal_material: Handle<StandardMaterial>,
    cloth_material: Handle<StandardMaterial>,
    wood_material: Handle<StandardMaterial>,
    crown_material: Handle<StandardMaterial>,
    glow_material: Handle<StandardMaterial>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarPart {
    Head,
    Torso,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Shirt,
    Pants,
    Hat,
    Cape,
    MainHand,
    OffHand,
    Shoes,
    Gloves,
    Necklace,
    Bag,
    Watch,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewModelPart {
    LeftFist,
    RightFist,
    PrimaryWeapon,
    SecondaryWeapon,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum WeaponVisualSegment {
    Core,
    Detail,
    Grip,
    Accent,
    Pommel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WeaponSurface {
    Steel,
    DarkSteel,
    Wood,
    Leather,
    Brass,
    Glass,
    Paper,
    Ember,
}

#[derive(Component, Default)]
pub struct LookAngles {
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Component)]
pub struct PlayerMotion {
    pub planar_velocity: Vec2,
    pub bob_phase: f32,
    pub move_amount: f32,
    pub sprint_amount: f32,
    pub sprint_stamina: f32,
    pub vertical_velocity: f32,
    pub jump_visual: f32,
    pub landing_dip: f32,
    pub was_grounded: bool,
    pub last_footstep_index: i32,
    pub last_position: Vec3,
    pub last_position_valid: bool,
    pub snag_frames: u8,
}

impl Default for PlayerMotion {
    fn default() -> Self {
        Self {
            planar_velocity: Vec2::ZERO,
            bob_phase: 0.0,
            move_amount: 0.0,
            sprint_amount: 0.0,
            sprint_stamina: 1.0,
            vertical_velocity: 0.0,
            jump_visual: 0.0,
            landing_dip: 0.0,
            was_grounded: false,
            last_footstep_index: -1,
            last_position: Vec3::ZERO,
            last_position_valid: false,
            snag_frames: 0,
        }
    }
}

#[derive(Resource, Clone)]
pub struct WorldCfg {
    pub maze_w: u32,
    pub maze_h: u32,
    pub tile: f32,
    pub seed: u64,
}

#[derive(Resource, Clone, Copy)]
pub struct MovementTuning {
    pub jump_velocity: f32,
    pub gravity: f32,
    pub max_fall_speed: f32,
    pub sprint_drain: f32,
    pub sprint_recover_ground: f32,
    pub sprint_recover_air: f32,
    pub player_radius: f32,
    pub wall_collider_padding: f32,
}

impl Default for MovementTuning {
    fn default() -> Self {
        Self {
            jump_velocity: 3.1,
            gravity: 9.7,
            max_fall_speed: 8.9,
            sprint_drain: 0.32,
            sprint_recover_ground: 0.24,
            sprint_recover_air: 0.09,
            player_radius: 0.165,
            wall_collider_padding: 0.14,
        }
    }
}

#[derive(Resource, Default)]
pub struct CollisionDebugSettings {
    pub enabled: bool,
}

#[derive(Resource, Default)]
pub struct SnagDebugState {
    pub logged_cells: HashSet<(i32, i32)>,
    pub clear_requested: bool,
}

#[derive(Resource, Clone)]
pub struct CollisionDebugAssets {
    pub snag_marker_mesh: Handle<Mesh>,
    pub snag_wall_bar_mesh: Handle<Mesh>,
    pub snag_ns_material: Handle<StandardMaterial>,
    pub snag_ew_material: Handle<StandardMaterial>,
    pub snag_material: Handle<StandardMaterial>,
}

#[derive(Resource, Clone)]
pub struct ActiveMaze(pub Maze);

#[derive(Resource, Default)]
pub struct UIFlags {
    pub inventory_open: bool,
    pub spell_menu_open: bool,
    pub pause_menu_open: bool,
    pub pause_settings_open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuFocusTarget {
    StartScreen,
    Inventory,
    Chest(Entity),
    Pause,
    Spells,
}

#[derive(Resource, Default)]
pub struct MenuFocusState {
    pub target: Option<MenuFocusTarget>,
    pub pending: bool,
}

impl MenuFocusState {
    fn request(&mut self, target: MenuFocusTarget) {
        self.target = Some(target);
        self.pending = true;
    }
}

#[derive(Resource)]
pub struct CameraModeSettings {
    pub third_person_enabled: bool,
    pub shoulder_offset: f32,
    pub follow_distance: f32,
    pub camera_height: f32,
    pub shoulder_side: f32,
    pub collision_enabled: bool,
}

impl Default for CameraModeSettings {
    fn default() -> Self {
        Self {
            third_person_enabled: false,
            shoulder_offset: 0.48,
            follow_distance: 2.55,
            camera_height: 0.32,
            shoulder_side: 1.0,
            collision_enabled: true,
        }
    }
}

#[derive(Resource, Default)]
pub struct ChestSettings { pub per_cells: usize } // chest density control

#[derive(Resource, Default)]
pub struct OpenChest(pub Option<Entity>);

#[derive(Component)]
struct MenuCamera;

#[derive(Component)]
struct WorldEntity;

#[derive(Resource, Clone, Copy)]
pub struct PlayerSpawn(pub Vec3);

#[derive(Resource, Default)]
pub struct PendingRespawn(pub Option<Timer>);

#[derive(Resource, Clone)]
pub struct LandingAudio {
    pub thump: Handle<AudioSource>,
    pub step_left: Handle<AudioSource>,
    pub step_right: Handle<AudioSource>,
}

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

#[derive(Component)]
pub struct Wet { pub timer: Timer }

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

fn player_combat_filter(player: Entity) -> QueryFilter<'static> {
    QueryFilter::default()
        .exclude_collider(player)
        .exclude_rigid_body(player)
}

fn resolve_damageable_entity(
    entity: Entity,
    parents: &Query<&ChildOf>,
    damageables: &Query<(), With<Health>>,
) -> Option<Entity> {
    let mut current = Some(entity);
    for _ in 0..16 {
        let candidate = current?;
        if damageables.get(candidate).is_ok() {
            return Some(candidate);
        }
        current = parents.get(candidate).ok().map(|parent| parent.0);
    }
    None
}

fn apply_damage_to_hit_entity(
    entity: Entity,
    damage: f32,
    parents: &Query<&ChildOf>,
    damageables: &Query<(), With<Health>>,
    q_health: &mut Query<&mut Health>,
) -> Option<Entity> {
    let resolved = resolve_damageable_entity(entity, parents, damageables)?;
    let Ok(mut hp) = q_health.get_mut(resolved) else { return None; };
    hp.apply(-damage);
    Some(resolved)
}

fn skeleton_focus_point(transform: &Transform) -> Vec3 {
    transform.translation + Vec3::new(0.0, 0.8, 0.0)
}

fn is_heavy_cleave_weapon(item: Option<&Item>) -> bool {
    matches!(
        item.and_then(|entry| entry.weapon),
        Some(WeaponKind::DoubleAxe)
            | Some(WeaponKind::GiantHammer)
            | Some(WeaponKind::TwoHandedSword)
            | Some(WeaponKind::Scythe)
    )
}

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

#[derive(Resource, Default)]
struct CombatComboState {
    count: u8,
    remaining: f32,
}

impl CombatComboState {
    fn damage_multiplier(&self) -> f32 {
        1.0 + self.count as f32 * 0.12
    }

    fn register_hit(&mut self) {
        self.count = (self.count + 1).min(3);
        self.remaining = 0.95;
    }

    fn break_chain(&mut self) {
        self.count = 0;
        self.remaining = 0.0;
    }
}

// === Stash: 100 slots ===
#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct Stash { pub slots: Vec<Option<crate::items::Item>> }
impl Default for Stash {
    fn default() -> Self { Self { slots: vec![None; 100] } }
}

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerchantStore { pub slots: Vec<Option<crate::items::Item>> }
impl Default for MerchantStore {
    fn default() -> Self {
        let mut rng = StdRng::seed_from_u64(0xC0FFEE_51A7);
        let mut slots = vec![None; 48];
        for slot in &mut slots {
            if rng.random_bool(0.52) {
                *slot = Some(roll_item(&mut rng));
            }
        }
        Self { slots }
    }
}

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerchantBuyback { pub items: Vec<crate::items::Item> }
impl Default for MerchantBuyback {
    fn default() -> Self { Self { items: Vec::new() } }
}

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlayerWallet { pub gold: i32 }
impl Default for PlayerWallet {
    fn default() -> Self { Self { gold: 180 } }
}

#[derive(Resource, Clone, serde::Serialize, serde::Deserialize)]
pub struct MerchantState {
    pub refreshes_remaining: u8,
    pub refresh_seed: u64,
}
impl Default for MerchantState {
    fn default() -> Self {
        Self {
            refreshes_remaining: 3,
            refresh_seed: 0xA11C_E551,
        }
    }
}

// Which tab is currently active on the Start screen
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StartMenuTab { Main, Stash, Trader, Character }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TraderShelfFilter { All, Weapons, Gear, Consumables, Premium }

#[derive(Resource)]
pub struct StartMenuState {
    pub active: StartMenuTab,
    pub trader_filter: TraderShelfFilter,
}

impl Default for StartMenuState {
    fn default() -> Self {
        Self {
            active: StartMenuTab::Main,
            trader_filter: TraderShelfFilter::All,
        }
    }
}

// === Weapon handling (primary/secondary) ===
#[derive(Resource, Default)]
pub struct ActiveWeapon {
    pub slot: WeaponSlot,
    pub drawn: bool,
}

#[derive(Resource)]
pub struct ViewModelAnimation {
    pub swing: Timer,
    pub active: bool,
    pub recoil_strength: f32,
    pub recovery_strength: f32,
    pub draw_blend: f32,
}

impl Default for ViewModelAnimation {
    fn default() -> Self {
        Self {
            swing: Timer::from_seconds(0.22, TimerMode::Once),
            active: false,
            recoil_strength: 0.0,
            recovery_strength: 0.0,
            draw_blend: 0.0,
        }
    }
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
            .insert_resource(WorldCfg { maze_w: 28, maze_h: 24, tile: 2.3, seed: 1337 })
            .insert_resource(MovementTuning::default())
            .insert_resource(CollisionDebugSettings::default())
            .insert_resource(SnagDebugState::default())
            .insert_resource(StartMenuState::default())
            .insert_resource(PlayerStats::default())
            .insert_resource(BagGrid::new(3*1, 14))
            .insert_resource(Equipment::default())
            .insert_resource(UISelection::default())
            .insert_resource(ChestSettings { per_cells: 14 })
            .insert_resource(OpenChest::default())
            .init_resource::<UIFlags>()
            .insert_resource(MenuFocusState::default())
            .insert_resource(CameraModeSettings::default())
            // ↓↓↓ ADD THIS (must come before start_menu_ui can run)
            .insert_resource(Stash::default())
            .insert_resource(MerchantStore::default())
            .insert_resource(MerchantBuyback::default())
            .insert_resource(MerchantState::default())
            .insert_resource(PlayerWallet::default())
            .insert_resource(ActiveSpell::default())
            .insert_resource(Spellbook::default())
            .insert_resource(CombatComboState::default())
            .insert_resource(ActiveWeapon::default())
            .insert_resource(ViewModelAnimation::default())
            .insert_resource(EnemyRuntimeRng(StdRng::seed_from_u64(0x51A7_3EED)))
            .insert_resource(HitStopState::default())
            .insert_resource(CameraImpulseState::default())

            // NEW: respawn-related resources (PendingRespawn starts empty; PlayerSpawn is set in setup_world)
            .insert_resource(PendingRespawn::default())

            .add_systems(Startup, (load_inventory_persistence, init_landing_audio))
            .add_systems(OnEnter(AppState::Menu), spawn_menu_camera)
            .add_systems(EguiPrimaryContextPass, start_menu_ui.run_if(in_state(AppState::Menu)))
            .add_systems(Update, sync_window_cursor)
            .add_systems(Update, sync_player_stats_from_equipment)

            // When entering the game, set up world AND record spawn
            .add_systems(OnEnter(AppState::InGame), (despawn_menu_camera, setup_world))
            .add_systems(OnExit(AppState::InGame), cleanup_world)

            // Toggle inventory with Tab
            .add_systems(Update, ui_toggle_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, sync_collision_debug_visibility.run_if(in_state(AppState::InGame)))
            .add_systems(Update, clear_snag_markers.run_if(in_state(AppState::InGame)))

            // NEW: fall detection + respawn tick while in-game
            .add_systems(Update, spell_recharge_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, spell_cast_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, spell_channel_tick_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, fireball_projectile_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, spell_visual_decay_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, burn_tick_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, tick_wet_status.run_if(in_state(AppState::InGame)))
            .add_systems(Update, sync_burn_visuals.run_if(in_state(AppState::InGame)))
            .add_systems(Update, update_burn_visuals.run_if(in_state(AppState::InGame)))
            .add_systems(Update, sync_water_stream_visuals.run_if(in_state(AppState::InGame)))
            .add_systems(Update, tick_hit_stop.run_if(in_state(AppState::InGame)))
            .add_systems(Update, tick_combat_combo.run_if(in_state(AppState::InGame)))
            .add_systems(Update, skeleton_ai_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, apply_enemy_hit_recoil.after(skeleton_ai_system).run_if(in_state(AppState::InGame)))
            .add_systems(Update, enemy_projectile_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, cleanup_dead_skeletons.run_if(in_state(AppState::InGame)))
            .add_systems(Update, gold_pickup_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, fall_off_map_detector.run_if(in_state(AppState::InGame)))
            .add_systems(Update, respawn_tick_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, weapon_input_system.run_if(in_state(AppState::InGame)))
            .add_systems(Update, update_damage_popups.run_if(in_state(AppState::InGame)))
            .add_systems(Update, record_snag_cells.before(player_look_and_move).run_if(in_state(AppState::InGame)))
            .add_systems(Update, player_look_and_move.run_if(in_state(AppState::InGame)))
            .add_systems(Update, update_player_avatar_visuals.run_if(in_state(AppState::InGame)))
            .add_systems(Update, update_first_person_viewmodel.run_if(in_state(AppState::InGame)))
            .add_systems(Update, interact_system.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, inventory_ui.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, stamina_hud_ui.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, player_health_hud_ui.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, crate::ui::gold_hud_ui.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, crate::ui::compass_hud_ui.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, enemy_health_bars_ui.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, damage_popup_ui.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, crate::ui::pause_menu_ui.run_if(in_state(AppState::InGame)))
            .add_systems(EguiPrimaryContextPass, crate::ui::spell_menu_ui.run_if(in_state(AppState::InGame)))
            .add_systems(Last, save_inventory_on_app_exit);
    }
}

fn spawn_menu_camera(mut commands: Commands, mut menu_focus: ResMut<MenuFocusState>) {
    // 2D camera is fine for showing egui and a clear color
    menu_focus.request(MenuFocusTarget::StartScreen);
    commands.spawn((Camera2d, PrimaryEguiContext, MenuCamera));
}

fn despawn_menu_camera(mut commands: Commands, q: Query<Entity, With<MenuCamera>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn cleanup_world(mut commands: Commands, q: Query<Entity, With<WorldEntity>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

fn sync_collision_debug_visibility(
    collision_debug: Res<CollisionDebugSettings>,
    camera_mode: Res<CameraModeSettings>,
    cam_q: Query<&GlobalTransform, With<PlayerCamera>>,
    mut q: Query<&mut Visibility, With<CollisionDebugVisual>>,
) {
    let looking_down_enough = cam_q
        .single()
        .map(|cam| cam.forward().y < -0.42)
        .unwrap_or(true);
    let show_debug = collision_debug.enabled && (camera_mode.third_person_enabled || looking_down_enough);
    let visibility = if show_debug {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for mut current in &mut q {
        *current = visibility;
    }
}

pub fn ui_toggle_system(
    kb: Res<ButtonInput<KeyCode>>,
    mut flags: ResMut<UIFlags>,
    open_chest: Res<OpenChest>,
    mut menu_focus: ResMut<MenuFocusState>,
    mut camera_mode: ResMut<CameraModeSettings>,
    mut collision_debug: ResMut<CollisionDebugSettings>,
    mut snag_debug: ResMut<SnagDebugState>,
) {
    // Inventory (Tab)
    if kb.just_pressed(KeyCode::Tab) {
        flags.inventory_open = !flags.inventory_open;
        if flags.inventory_open {
            menu_focus.request(MenuFocusTarget::Inventory);
        } else if let Some(chest) = open_chest.0 {
            menu_focus.request(MenuFocusTarget::Chest(chest));
        }
    }
    // Spells (E)
    if kb.just_pressed(KeyCode::KeyE) {
        flags.spell_menu_open = !flags.spell_menu_open;
        if flags.spell_menu_open {
            menu_focus.request(MenuFocusTarget::Spells);
        } else if let Some(chest) = open_chest.0 {
            menu_focus.request(MenuFocusTarget::Chest(chest));
        } else if flags.inventory_open {
            menu_focus.request(MenuFocusTarget::Inventory);
        }
    }
    // Pause (Esc)
    if kb.just_pressed(KeyCode::Escape) {
        flags.pause_menu_open = !flags.pause_menu_open;
        if flags.pause_menu_open {
            menu_focus.request(MenuFocusTarget::Pause);
        } else {
            flags.pause_settings_open = false;
            if let Some(chest) = open_chest.0 {
                menu_focus.request(MenuFocusTarget::Chest(chest));
            } else if flags.inventory_open {
                menu_focus.request(MenuFocusTarget::Inventory);
            } else if flags.spell_menu_open {
                menu_focus.request(MenuFocusTarget::Spells);
            }
        }
    }
    if kb.just_pressed(KeyCode::KeyV) {
        camera_mode.third_person_enabled = !camera_mode.third_person_enabled;
    }
    if kb.just_pressed(KeyCode::KeyC) {
        camera_mode.shoulder_side *= -1.0;
        if camera_mode.shoulder_side == 0.0 {
            camera_mode.shoulder_side = 1.0;
        }
    }
    if kb.just_pressed(KeyCode::F3) {
        collision_debug.enabled = !collision_debug.enabled;
        println!(
            "Collision debug {}",
            if collision_debug.enabled { "enabled" } else { "disabled" }
        );
    }
    if kb.just_pressed(KeyCode::F4) {
        snag_debug.clear_requested = true;
        println!("Snag markers queued for clear");
    }
}

fn clear_snag_markers(
    mut commands: Commands,
    mut snag_debug: ResMut<SnagDebugState>,
    q: Query<Entity, With<SnagMarker>>,
) {
    if !snag_debug.clear_requested {
        return;
    }

    for entity in &q {
        commands.entity(entity).despawn();
    }
    snag_debug.logged_cells.clear();
    snag_debug.clear_requested = false;
    println!("Snag markers cleared");
}

pub fn setup_world(
    mut commands: Commands,
    cfg: Res<WorldCfg>,
    tuning: Res<MovementTuning>,
    mut enemy_rng: ResMut<EnemyRuntimeRng>,
    mut snag_debug: ResMut<SnagDebugState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let skeleton_visuals = create_skeleton_visual_assets(&mut meshes, &mut materials);
    let projectile_assets = create_enemy_projectile_assets(&mut meshes, &mut materials);
    let spell_visuals = create_spell_visual_assets(&mut meshes, &mut materials);
    let gold_pickup_assets = create_gold_pickup_assets(&mut meshes, &mut materials);
    let procedural_textures = create_procedural_texture_assets(&mut images);
    commands.insert_resource(projectile_assets.clone());
    commands.insert_resource(spell_visuals);
    commands.insert_resource(gold_pickup_assets.clone());
    commands.insert_resource(procedural_textures.clone());

    // Maze
    let maze = Maze::generate_with_three_exits(cfg.maze_w, cfg.maze_h, cfg.seed);
    commands.insert_resource(ActiveMaze(maze.clone()));
    snag_debug.logged_cells.clear();
    let tile = cfg.tile;
    let wall_height = 2.3;
    let wall_thickness = 0.26;
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.42, 0.46, 0.52),
        brightness: 140.0,
        affects_lightmapped_meshes: true,
    });
    let wall_debug_ns_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.48, 0.1, 0.42),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let wall_debug_ns_post_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.9, 0.35, 0.72),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let wall_debug_ew_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.12, 0.72, 1.0, 0.42),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let wall_debug_ew_post_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.65, 0.92, 1.0, 0.72),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let player_debug_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.18, 0.58, 1.0, 0.22),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let snag_ns_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.72, 0.18, 0.8),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let snag_ew_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.28, 0.82, 1.0, 0.8),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let snag_debug_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.18, 0.18, 0.42),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let snag_marker_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile * 0.38, 0.08, tile * 0.38)));
    let snag_wall_bar_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile * 0.34, 0.1, 0.06)));
    commands.insert_resource(CollisionDebugAssets {
        snag_marker_mesh,
        snag_wall_bar_mesh,
        snag_ns_material,
        snag_ew_material,
        snag_material: snag_debug_material,
    });
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.18, 0.17),
        perceptual_roughness: 0.98,
        ..default()
    });
    let moss_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.31, 0.2),
        perceptual_roughness: 1.0,
        ..default()
    });
    let path_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.26, 0.24, 0.19),
        perceptual_roughness: 0.97,
        ..default()
    });
    let stone_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.45, 0.47),
        perceptual_roughness: 0.88,
        metallic: 0.04,
        ..default()
    });
    let branch_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.22, 0.13),
        perceptual_roughness: 1.0,
        ..default()
    });
    let leaf_dark_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.48, 0.24),
        perceptual_roughness: 0.95,
        ..default()
    });
    let leaf_mid_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.33, 0.62, 0.32),
        perceptual_roughness: 0.92,
        ..default()
    });
    let leaf_light_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.48, 0.75, 0.42),
        perceptual_roughness: 0.9,
        ..default()
    });

    // Floor collider (one big plate) + FIXED body
    let total_x = cfg.maze_w as f32 * tile;
    let total_z = cfg.maze_h as f32 * tile;
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(total_x, 0.2, total_z)))),
        MeshMaterial3d(floor_mat.clone()),
        Transform::from_xyz((total_x - tile) * 0.5, -1.1, (total_z - tile) * 0.5),
        WorldEntity,
        RigidBody::Fixed,                                // <-- NEW
        Collider::cuboid(total_x*0.5, 0.1, total_z*0.5),
        Name::new("Floor"),
    ));

    let patch_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile * 0.7, 0.02, tile * 0.7)));
    let pebble_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.18, 0.08, 0.14)));
    let tuft_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.08, 0.34, 0.08)));

    let scatter_count = ((maze.w * maze.h) as usize / 2).max(40);
    for _ in 0..scatter_count {
        let pos = Vec3::new(
            rng_f32(&mut enemy_rng.0, 0.0, total_x - tile),
            -0.995,
            rng_f32(&mut enemy_rng.0, 0.0, total_z - tile),
        );
        let patch_material = if rng_bool_weighted(&mut enemy_rng.0, 0.55) { moss_mat.clone() } else { path_mat.clone() };
        commands.spawn((
            Mesh3d(patch_mesh.clone()),
            MeshMaterial3d(patch_material),
            Transform::from_translation(pos)
                .with_rotation(Quat::from_rotation_y(rng_f32(&mut enemy_rng.0, 0.0, std::f32::consts::TAU)))
                .with_scale(Vec3::new(rng_f32(&mut enemy_rng.0, 0.65, 1.35), 1.0, rng_f32(&mut enemy_rng.0, 0.65, 1.35))),
            WorldEntity,
        ));

        if rng_bool_weighted(&mut enemy_rng.0, 0.4) {
            commands.spawn((
                Mesh3d(pebble_mesh.clone()),
                MeshMaterial3d(stone_mat.clone()),
                Transform::from_translation(pos + Vec3::new(rng_f32(&mut enemy_rng.0, -0.28, 0.28), 0.02, rng_f32(&mut enemy_rng.0, -0.28, 0.28)))
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, rng_f32(&mut enemy_rng.0, -0.2, 0.2), rng_f32(&mut enemy_rng.0, 0.0, std::f32::consts::TAU), rng_f32(&mut enemy_rng.0, -0.2, 0.2)))
                    .with_scale(Vec3::new(rng_f32(&mut enemy_rng.0, 0.8, 1.6), rng_f32(&mut enemy_rng.0, 0.8, 1.4), rng_f32(&mut enemy_rng.0, 0.8, 1.5))),
                WorldEntity,
            ));
        }

        if rng_bool_weighted(&mut enemy_rng.0, 0.28) {
            commands.spawn((
                Mesh3d(tuft_mesh.clone()),
                MeshMaterial3d(leaf_light_mat.clone()),
                Transform::from_translation(pos + Vec3::new(rng_f32(&mut enemy_rng.0, -0.22, 0.22), 0.12, rng_f32(&mut enemy_rng.0, -0.22, 0.22)))
                    .with_rotation(Quat::from_rotation_y(rng_f32(&mut enemy_rng.0, 0.0, std::f32::consts::TAU)))
                    .with_scale(Vec3::new(rng_f32(&mut enemy_rng.0, 0.8, 1.2), rng_f32(&mut enemy_rng.0, 0.8, 1.6), rng_f32(&mut enemy_rng.0, 0.8, 1.2))),
                WorldEntity,
            ));
        }
    }

    // Lights
    commands.spawn((
        DirectionalLight{ illuminance: 42_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(18.0, 34.0, 12.0).looking_at(Vec3::new(total_x * 0.35, 0.0, total_z * 0.35), Vec3::Y),
        WorldEntity,
    ));
    commands.spawn((
        PointLight {
            intensity: 1_400.0,
            color: Color::srgb(1.0, 0.86, 0.66),
            range: 12.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(total_x * 0.18, 3.6, total_z * 0.22),
        WorldEntity,
    ));
    commands.spawn((
        PointLight {
            intensity: 1_200.0,
            color: Color::srgb(0.7, 0.88, 1.0),
            range: 10.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(total_x * 0.72, 3.2, total_z * 0.74),
        WorldEntity,
    ));

    // Walls & floor visuals
    let branch_core_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile, wall_height * 0.6, wall_thickness * 0.42)));
    let leaf_shell_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile * 0.98, wall_height * 0.92, wall_thickness * 0.72)));
    let leaf_top_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile * 0.86, wall_height * 0.36, wall_thickness * 0.88)));
    let twig_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile * 0.92, wall_height * 0.08, wall_thickness * 0.18)));
    let half = tile*0.5;
    let mut placed_walls: HashSet<(i32, i32, bool)> = HashSet::new();
    let wall_collider_padding = tuning.wall_collider_padding;

    // Spawn *fixed* colliders for walls
    for y in 0..maze.h as i32 {
        for x in 0..maze.w as i32 {
            let idx = (y as u32 * maze.w + x as u32) as usize;
            let c = maze.cells[idx];
            let center = Vec3::new(x as f32 * tile, 0.0, y as f32 * tile);

            let mut spawn_wall = |pos: Vec3, rotate: bool| {
                let key = (
                    (pos.x * 1000.0).round() as i32,
                    (pos.z * 1000.0).round() as i32,
                    rotate,
                );
                if !placed_walls.insert(key) {
                    return;
                }

                let mut t = Transform::from_translation(pos);
                if rotate { t.rotate_y(std::f32::consts::FRAC_PI_2); }
                let debug_floor_material = if rotate {
                    wall_debug_ew_material.clone()
                } else {
                    wall_debug_ns_material.clone()
                };
                let debug_post_material = if rotate {
                    wall_debug_ew_post_material.clone()
                } else {
                    wall_debug_ns_post_material.clone()
                };

                // Keep the collider long on local X and thin on local Z.
                // The entity rotation handles turning east/west wall bodies into north/south ones.
                let hx = (tile * 0.5 - wall_collider_padding).max(0.01);
                let hz = (wall_thickness * 0.5 - wall_collider_padding).max(0.01);

                let wall = commands.spawn((
                    t,
                    Visibility::default(),
                    WorldEntity,
                    RigidBody::Fixed,
                    Collider::cuboid(hx, wall_height*0.5, hz),
                    Name::new("BushWall"),
                )).id();

                commands.entity(wall).with_children(|parent| {
                    let debug_floor_height = 0.12;
                    let debug_floor_y = -0.93;
                    let endpoint_offset = Vec3::new((hx - 0.05).max(0.0), debug_floor_y + 0.12, 0.0);
                    parent.spawn((
                        Mesh3d(branch_core_mesh.clone()),
                        MeshMaterial3d(branch_mat.clone()),
                        Transform::from_xyz(0.0, -0.12, 0.0),
                    ));
                    parent.spawn((
                        Mesh3d(leaf_shell_mesh.clone()),
                        MeshMaterial3d(leaf_dark_mat.clone()),
                        Transform::from_xyz(0.0, 0.05, 0.0),
                    ));
                    parent.spawn((
                        Mesh3d(leaf_shell_mesh.clone()),
                        MeshMaterial3d(leaf_mid_mat.clone()),
                        Transform::from_xyz(0.0, 0.12, wall_thickness * 0.12)
                            .with_scale(Vec3::new(0.92, 0.9, 0.8)),
                    ));
                    parent.spawn((
                        Mesh3d(leaf_top_mesh.clone()),
                        MeshMaterial3d(leaf_light_mat.clone()),
                        Transform::from_xyz(0.0, wall_height * 0.28, 0.0),
                    ));
                    parent.spawn((
                        Mesh3d(twig_mesh.clone()),
                        MeshMaterial3d(branch_mat.clone()),
                        Transform::from_xyz(0.0, wall_height * 0.08, wall_thickness * 0.18),
                    ));
                    parent.spawn((
                        Mesh3d(twig_mesh.clone()),
                        MeshMaterial3d(branch_mat.clone()),
                        Transform::from_xyz(0.0, -wall_height * 0.04, -wall_thickness * 0.16)
                            .with_rotation(Quat::from_rotation_x(0.2)),
                    ));
                    parent.spawn((
                        Mesh3d(meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(hx * 2.0, debug_floor_height, hz * 2.0)))),
                        MeshMaterial3d(debug_floor_material.clone()),
                        Transform::from_xyz(0.0, debug_floor_y, 0.0),
                        Visibility::Hidden,
                        CollisionDebugVisual,
                    ));
                    parent.spawn((
                        Mesh3d(meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.07, 0.24, 0.07)))),
                        MeshMaterial3d(debug_post_material.clone()),
                        Transform::from_translation(endpoint_offset),
                        Visibility::Hidden,
                        CollisionDebugVisual,
                    ));
                    parent.spawn((
                        Mesh3d(meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.07, 0.24, 0.07)))),
                        MeshMaterial3d(debug_post_material.clone()),
                        Transform::from_translation(-endpoint_offset),
                        Visibility::Hidden,
                        CollisionDebugVisual,
                    ));
                });
            };

            // Spawn each boundary only once to avoid duplicate colliders on shared cell edges.
            if y == 0 && c.walls[0] { spawn_wall(center + Vec3::new(0.0, 0.0, -half), false); }
            if x == 0 && c.walls[3] { spawn_wall(center + Vec3::new(-half, 0.0, 0.0), true); }
            if c.walls[2] { spawn_wall(center + Vec3::new(0.0, 0.0, half), false); }
            if c.walls[1] { spawn_wall(center + Vec3::new(half, 0.0, 0.0), true); }
        }
    }

    // Exits (unchanged except optional Name)
    let exits = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new((maze.w as f32 -1.0)*tile, 0.0, 0.0),
        Vec3::new((maze.w as f32 -1.0)*tile, 0.0, (maze.h as f32 -1.0)*tile),
    ];
    for epos in exits {
        let eid = commands.spawn((
            Transform::from_translation(epos - Vec3::Y * 0.85),
            Visibility::Hidden,
            WorldEntity,
        )).id();

        commands.entity(eid)
            .insert(ExitMarker)
            .insert(Collider::cuboid(tile*0.5, 0.25, tile*0.5))
            .insert(Sensor)
            .insert(Name::new("Exit"));
    }

    // Player spawn
    let start = Vec3::new(0.0, -0.33, 0.0); // collider center aligned to the floor plane
    let pid = commands.spawn((
        Transform::from_translation(start),
        Visibility::default(),
        WorldEntity,
        Player,
        LookAngles::default(),
        PlayerMotion::default(),
        Health::new(100.0),
        Name::new("Player"),
    )).id();
    commands.insert_resource(PlayerSpawn(start));

    commands.entity(pid)
        .insert(Collider::capsule_y(0.43, tuning.player_radius))
        .insert(KinematicCharacterController {
            offset: CharacterLength::Absolute(0.03),
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.18),
                min_width: CharacterLength::Absolute(0.12),
                include_dynamic_bodies: false,
            }),
            snap_to_ground: Some(CharacterLength::Absolute(0.12)),
            ..default()
        });

    commands.entity(pid).with_children(|c| {
        c.spawn((
            Mesh3d(meshes.add(Mesh::from(bevy::math::primitives::Capsule3d::new(tuning.player_radius, 0.86)))),
            MeshMaterial3d(player_debug_material.clone()),
            Transform::default(),
            Visibility::Hidden,
            CollisionDebugVisual,
        ));
    });

    commands.entity(pid).with_children(|c| {
        spawn_player_avatar(c, &mut meshes, &mut materials, &procedural_textures);
        let camera = c.spawn((
            Camera3d::default(),
            PrimaryEguiContext,
            Msaa::Sample4,
            PlayerCamera,
            Transform {
                translation: Vec3::new(0.0, 0.3, -0.6),
                rotation: Quat::from_rotation_y(std::f32::consts::PI),
                ..default()
            },
        )).id();

        c.commands().entity(camera).with_children(|cam| {
            spawn_first_person_viewmodel(cam, &mut meshes, &mut materials, &procedural_textures);
        });
    });

    // Save the spawn position for respawn:
    commands.insert_resource(PlayerSpawn(start));

    let chest_common_mat = materials.add(Color::srgb_u8(0xc2, 0x90, 0x48));
    let chest_rare_mat = materials.add(Color::srgb_u8(0x5e, 0x93, 0xf0));
    let chest_epic_mat = materials.add(Color::srgb_u8(0x9a, 0x5e, 0xdc));
    let chest_royal_mat = materials.add(Color::srgb_u8(0xe2, 0xc0, 0x61));

    // Spawn chests (density controlled by per_cells)
    let mut rng = StdRng::seed_from_u64(cfg.seed ^ 0xC0FFEE);
    let total_cells = (maze.w * maze.h) as usize;
    let to_spawn = (total_cells / 12).max(8).min(40);

    for _ in 0..to_spawn {
        let x = rng.random_range(0..maze.w);
        let y = rng.random_range(0..maze.h);
        let center = Vec3::new(x as f32 * tile, 0.0, y as f32 * tile);
        let tier = roll_chest_tier(&mut rng);

        // tier-driven loot table
        let mut items = vec![];
        let count = match tier {
            ChestTier::Common => rng.random_range(1..=2),
            ChestTier::Rare => rng.random_range(2..=3),
            ChestTier::Epic => rng.random_range(3..=4),
            ChestTier::Royal => rng.random_range(4..=5),
        };
        for _ in 0..count {
            items.push(roll_chest_item(&mut rng, tier));
        }

        // simple visual cube chest
        let chest_scale = match tier {
            ChestTier::Common => 0.35,
            ChestTier::Rare => 0.38,
            ChestTier::Epic => 0.42,
            ChestTier::Royal => 0.48,
        };
        let chest_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(tile * chest_scale, 0.35, tile * chest_scale)));
        let chest_mat = match tier {
            ChestTier::Common => chest_common_mat.clone(),
            ChestTier::Rare => chest_rare_mat.clone(),
            ChestTier::Epic => chest_epic_mat.clone(),
            ChestTier::Royal => chest_royal_mat.clone(),
        };

        let chest_gold = match tier {
            ChestTier::Common => {
                if rng.random_bool(0.28) { rng.random_range(12..=34) } else { 0 }
            }
            ChestTier::Rare => rng.random_range(22..=58),
            ChestTier::Epic => rng.random_range(48..=96),
            ChestTier::Royal => rng.random_range(95..=170),
        };

        let cid = commands.spawn((
            Mesh3d(chest_mesh.clone()),
            MeshMaterial3d(chest_mat.clone()),
            Transform::from_translation(center - Vec3::Y * 0.8),
            WorldEntity,
        )).id();

        commands.entity(cid)
            .insert(ChestMarker)
            .insert(Chest { items, gold: chest_gold, tier })
            .insert(Interactable)
            .insert(Name::new("Chest"));

        let extra_cache_chance = match tier {
            ChestTier::Common => 0.22,
            ChestTier::Rare => 0.42,
            ChestTier::Epic => 0.65,
            ChestTier::Royal => 1.0,
        };
        if rng.random_bool(extra_cache_chance) {
            let cache_amount = match tier {
                ChestTier::Common => rng.random_range(18..=38),
                ChestTier::Rare => rng.random_range(36..=70),
                ChestTier::Epic => rng.random_range(62..=120),
                ChestTier::Royal => rng.random_range(130..=240),
            };
            spawn_gold_drop(
                &mut commands,
                &gold_pickup_assets,
                center + Vec3::new(rng.random_range(-0.32..0.32), 0.0, rng.random_range(-0.32..0.32)),
                cache_amount,
                GoldDropStyle::ChestCache,
            );
        }
    }

    spawn_skeleton_encounters(
        &mut commands,
        &maze,
        tile,
        &mut enemy_rng.0,
        &skeleton_visuals,
    );
}

fn create_enemy_projectile_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> EnemyProjectileAssets {
    EnemyProjectileAssets {
        arrow_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.05, 0.05, 0.44))),
        bolt_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.16, 0.16, 0.16))),
        fire_material: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.38, 0.16),
            emissive: LinearRgba::rgb(1.8, 0.5, 0.2),
            unlit: true,
            ..default()
        }),
        zap_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.48, 0.9, 1.0),
            emissive: LinearRgba::rgb(0.6, 1.6, 2.0),
            unlit: true,
            ..default()
        }),
        wind_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.7, 1.0, 0.78),
            emissive: LinearRgba::rgb(0.7, 1.2, 0.8),
            unlit: true,
            ..default()
        }),
        arrow_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.66, 0.64, 0.58),
            emissive: LinearRgba::rgb(0.16, 0.16, 0.16),
            unlit: true,
            ..default()
        }),
    }
}

fn create_spell_visual_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> SpellVisualAssets {
    SpellVisualAssets {
        orb_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.24, 0.24, 0.24))),
        beam_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.12, 0.12, 1.0))),
        slash_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.16, 0.42, 1.0))),
        cross_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.1, 1.0, 0.1))),
        ring_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.56, 0.08, 0.56))),
        fireball_material: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.42, 0.18, 0.92),
            emissive: LinearRgba::rgb(2.4, 0.8, 0.24),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        burn_material: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.44, 0.14, 0.5),
            emissive: LinearRgba::rgb(1.4, 0.45, 0.16),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        heal_material: materials.add(StandardMaterial {
            base_color: Color::srgba(0.7, 1.0, 0.78, 0.32),
            emissive: LinearRgba::rgb(0.3, 0.9, 0.45),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        zap_material: materials.add(StandardMaterial {
            base_color: Color::srgba(0.72, 0.94, 1.0, 0.92),
            emissive: LinearRgba::rgb(1.3, 2.1, 2.8),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        wind_material: materials.add(StandardMaterial {
            base_color: Color::srgba(0.86, 0.96, 1.0, 0.68),
            emissive: LinearRgba::rgb(0.75, 1.2, 1.45),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        water_material: materials.add(StandardMaterial {
            base_color: Color::srgba(0.5, 0.82, 1.0, 0.35),
            emissive: LinearRgba::rgb(0.18, 0.46, 0.7),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    }
}

fn create_gold_pickup_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> GoldPickupAssets {
    GoldPickupAssets {
        coin_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.18, 0.05, 0.18))),
        common_material: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(242, 201, 76),
            emissive: LinearRgba::rgb(0.24, 0.18, 0.04),
            metallic: 0.86,
            perceptual_roughness: 0.28,
            reflectance: 0.52,
            ..default()
        }),
        martial_material: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(215, 224, 232),
            emissive: LinearRgba::rgb(0.12, 0.14, 0.16),
            metallic: 0.94,
            perceptual_roughness: 0.2,
            reflectance: 0.58,
            ..default()
        }),
        arcane_material: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(118, 211, 255),
            emissive: LinearRgba::rgb(0.18, 0.42, 0.7),
            metallic: 0.55,
            perceptual_roughness: 0.24,
            reflectance: 0.42,
            ..default()
        }),
        royal_material: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(255, 225, 120),
            emissive: LinearRgba::rgb(0.42, 0.28, 0.06),
            metallic: 0.98,
            perceptual_roughness: 0.14,
            reflectance: 0.64,
            ..default()
        }),
    }
}

fn create_skeleton_visual_assets(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> SkeletonVisualAssets {
    SkeletonVisualAssets {
        skull_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.34, 0.3, 0.28))),
        rib_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.42, 0.6, 0.22))),
        arm_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.1, 0.52, 0.1))),
        leg_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.12, 0.58, 0.12))),
        pelvis_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.3, 0.16, 0.18))),
        bow_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.05, 0.72, 0.05))),
        sword_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.07, 0.72, 0.06))),
        greatsword_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.09, 1.02, 0.07))),
        shield_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.34, 0.44, 0.08))),
        axe_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.22, 0.86, 0.08))),
        staff_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.06, 0.92, 0.06))),
        crown_mesh: meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.28, 0.12, 0.28))),
        bone_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.88, 0.86, 0.76),
            perceptual_roughness: 0.92,
            ..default()
        }),
        metal_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.45, 0.5),
            perceptual_roughness: 0.54,
            metallic: 0.45,
            ..default()
        }),
        cloth_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.18, 0.2),
            perceptual_roughness: 0.96,
            ..default()
        }),
        wood_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.4, 0.28, 0.16),
            perceptual_roughness: 1.0,
            ..default()
        }),
        crown_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.82, 0.28),
            emissive: LinearRgba::rgb(0.3, 0.24, 0.06),
            metallic: 0.75,
            perceptual_roughness: 0.35,
            ..default()
        }),
        glow_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.5, 0.84, 1.0),
            emissive: LinearRgba::rgb(0.4, 1.2, 1.6),
            unlit: true,
            ..default()
        }),
    }
}

#[derive(Clone, Copy)]
struct SkeletonProfile {
    hp: f32,
    scale: f32,
    visual_scale: f32,
    move_speed: f32,
    aggro_range: f32,
    attack_range: f32,
    attack_cooldown: f32,
    melee_damage: f32,
}

fn skeleton_profile(variant: SkeletonVariant) -> SkeletonProfile {
    match variant {
        SkeletonVariant::Archer => SkeletonProfile { hp: 34.0, scale: 1.0, visual_scale: 0.25, move_speed: 2.2, aggro_range: 15.0, attack_range: 12.0, attack_cooldown: 1.4, melee_damage: 0.0 },
        SkeletonVariant::Knight => SkeletonProfile { hp: 54.0, scale: 1.0, visual_scale: 0.25, move_speed: 2.6, aggro_range: 11.0, attack_range: 1.7, attack_cooldown: 1.05, melee_damage: 18.0 },
        SkeletonVariant::SwordShield => SkeletonProfile { hp: 68.0, scale: 1.0, visual_scale: 0.25, move_speed: 2.2, aggro_range: 10.0, attack_range: 1.6, attack_cooldown: 1.15, melee_damage: 16.0 },
        SkeletonVariant::Guard => SkeletonProfile { hp: 110.0, scale: 1.5, visual_scale: 0.375, move_speed: 2.05, aggro_range: 11.5, attack_range: 1.95, attack_cooldown: 1.35, melee_damage: 28.0 },
        SkeletonVariant::Mage => SkeletonProfile { hp: 40.0, scale: 1.0, visual_scale: 0.25, move_speed: 2.0, aggro_range: 14.0, attack_range: 10.0, attack_cooldown: 3.0, melee_damage: 0.0 },
        SkeletonVariant::King => SkeletonProfile { hp: 220.0, scale: 1.5, visual_scale: 0.5, move_speed: 1.95, aggro_range: 15.0, attack_range: 2.15, attack_cooldown: 1.2, melee_damage: 34.0 },
    }
}

fn spawn_skeleton_encounters(
    commands: &mut Commands,
    maze: &Maze,
    tile: f32,
    rng: &mut StdRng,
    visuals: &SkeletonVisualAssets,
) {
    let mut occupied: HashSet<(u32, u32)> = HashSet::new();
    let mut cells: Vec<(u32, u32)> = Vec::new();
    for y in 0..maze.h {
        for x in 0..maze.w {
            let start_safe = x <= 2 && y <= 2;
            let edge_exit = x >= maze.w.saturating_sub(2) && (y == 0 || y >= maze.h.saturating_sub(2));
            if !start_safe && !edge_exit {
                cells.push((x, y));
            }
        }
    }

    let king_index = rng.random_range(0..cells.len());
    let king_cell = cells.swap_remove(king_index);
    occupied.insert(king_cell);
    spawn_skeleton_variant(commands, king_cell, tile, SkeletonVariant::King, visuals);

    let escorts = collect_reachable_cells(maze, king_cell, 6, &occupied);
    let entourage = [
        SkeletonVariant::Guard,
        SkeletonVariant::Guard,
        SkeletonVariant::Archer,
        SkeletonVariant::Knight,
        SkeletonVariant::Knight,
        SkeletonVariant::Knight,
    ];
    for (cell, variant) in escorts.into_iter().zip(entourage) {
        occupied.insert(cell);
        spawn_skeleton_variant(commands, cell, tile, variant, visuals);
    }

    let ambient_spawns = [
        (SkeletonVariant::Archer, 6usize),
        (SkeletonVariant::Knight, 8usize),
        (SkeletonVariant::SwordShield, 6usize),
        (SkeletonVariant::Guard, 4usize),
        (SkeletonVariant::Mage, 4usize),
    ];

    for (variant, count) in ambient_spawns {
        for _ in 0..count {
            if let Some(cell) = take_random_open_cell(rng, &cells, &occupied) {
                occupied.insert(cell);
                spawn_skeleton_variant(commands, cell, tile, variant, visuals);
            }
        }
    }
}

fn take_random_open_cell(
    rng: &mut StdRng,
    cells: &[(u32, u32)],
    occupied: &HashSet<(u32, u32)>,
) -> Option<(u32, u32)> {
    let available: Vec<(u32, u32)> = cells.iter().copied().filter(|cell| !occupied.contains(cell)).collect();
    if available.is_empty() {
        None
    } else {
        Some(available[rng.random_range(0..available.len())])
    }
}

fn collect_reachable_cells(
    maze: &Maze,
    start: (u32, u32),
    needed: usize,
    occupied: &HashSet<(u32, u32)>,
) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut seen: HashSet<(u32, u32)> = HashSet::from([start]);
    let mut frontier = std::collections::VecDeque::from([start]);
    while let Some(cell) = frontier.pop_front() {
        for neighbor in open_neighbors(maze, cell) {
            if seen.insert(neighbor) {
                frontier.push_back(neighbor);
                if !occupied.contains(&neighbor) {
                    out.push(neighbor);
                    if out.len() >= needed {
                        return out;
                    }
                }
            }
        }
    }
    out
}

fn open_neighbors(maze: &Maze, cell: (u32, u32)) -> Vec<(u32, u32)> {
    let (x, y) = cell;
    let idx = (y * maze.w + x) as usize;
    let c = maze.cells[idx];
    let mut neighbors = Vec::new();
    if !c.walls[0] && y > 0 { neighbors.push((x, y - 1)); }
    if !c.walls[1] && x + 1 < maze.w { neighbors.push((x + 1, y)); }
    if !c.walls[2] && y + 1 < maze.h { neighbors.push((x, y + 1)); }
    if !c.walls[3] && x > 0 { neighbors.push((x - 1, y)); }
    neighbors
}

fn spawn_skeleton_variant(
    commands: &mut Commands,
    cell: (u32, u32),
    tile: f32,
    variant: SkeletonVariant,
    visuals: &SkeletonVisualAssets,
) {
    let profile = skeleton_profile(variant);
    let position = Vec3::new(cell.0 as f32 * tile, -0.33, cell.1 as f32 * tile);
    let entity = commands.spawn((
        Transform::from_translation(position),
        Visibility::default(),
        WorldEntity,
        EnemyHealthBarAnchor,
        Health::new(profile.hp),
        SkeletonEnemy {
            variant,
            home: position,
            attack_timer: Timer::from_seconds(profile.attack_cooldown, TimerMode::Repeating),
        },
        Collider::capsule_y(0.42 * profile.scale, 0.16 * profile.scale),
        KinematicCharacterController {
            offset: CharacterLength::Absolute(0.03),
            slide: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.14 * profile.scale.max(1.0)),
                min_width: CharacterLength::Absolute(0.1),
                include_dynamic_bodies: false,
            }),
            snap_to_ground: Some(CharacterLength::Absolute(0.12)),
            ..default()
        },
        Name::new(skeleton_name(variant)),
    )).id();

    commands.entity(entity).with_children(|parent| {
        spawn_skeleton_visual(parent, visuals, variant, profile.visual_scale);
    });
}

fn skeleton_name(variant: SkeletonVariant) -> &'static str {
    match variant {
        SkeletonVariant::Archer => "Skeleton Archer",
        SkeletonVariant::Knight => "Skeleton Knight",
        SkeletonVariant::SwordShield => "Skeleton Swordshield",
        SkeletonVariant::Guard => "Skeleton Guard",
        SkeletonVariant::Mage => "Skeleton Mage",
        SkeletonVariant::King => "Skeleton King",
    }
}

fn spawn_skeleton_visual(
    parent: &mut ChildSpawnerCommands,
    visuals: &SkeletonVisualAssets,
    variant: SkeletonVariant,
    scale: f32,
) {
    let root = parent.spawn((
        Transform {
            translation: Vec3::new(0.0, skeleton_visual_root_y(scale), 0.0),
            scale: Vec3::splat(scale),
            ..default()
        },
        Visibility::Inherited,
    )).id();

    parent.commands().entity(root).with_children(|skel| {
        skel.spawn((Mesh3d(visuals.skull_mesh.clone()), MeshMaterial3d(visuals.bone_material.clone()), Transform::from_xyz(0.0, 1.34, 0.0)));
        skel.spawn((Mesh3d(visuals.rib_mesh.clone()), MeshMaterial3d(visuals.bone_material.clone()), Transform::from_xyz(0.0, 0.86, 0.0)));
        skel.spawn((Mesh3d(visuals.pelvis_mesh.clone()), MeshMaterial3d(visuals.bone_material.clone()), Transform::from_xyz(0.0, 0.48, 0.0)));
        skel.spawn((Mesh3d(visuals.arm_mesh.clone()), MeshMaterial3d(visuals.bone_material.clone()), Transform::from_xyz(-0.28, 0.86, 0.0)));
        skel.spawn((Mesh3d(visuals.arm_mesh.clone()), MeshMaterial3d(visuals.bone_material.clone()), Transform::from_xyz(0.28, 0.86, 0.0)));
        skel.spawn((Mesh3d(visuals.leg_mesh.clone()), MeshMaterial3d(visuals.bone_material.clone()), Transform::from_xyz(-0.11, 0.06, 0.0)));
        skel.spawn((Mesh3d(visuals.leg_mesh.clone()), MeshMaterial3d(visuals.bone_material.clone()), Transform::from_xyz(0.11, 0.06, 0.0)));

        match variant {
            SkeletonVariant::Archer => {
                skel.spawn((Mesh3d(visuals.bow_mesh.clone()), MeshMaterial3d(visuals.wood_material.clone()), Transform::from_xyz(0.36, 0.78, -0.02).with_rotation(Quat::from_rotation_z(-0.2))));
                skel.spawn((Mesh3d(visuals.staff_mesh.clone()), MeshMaterial3d(visuals.wood_material.clone()), Transform::from_xyz(-0.2, 0.76, 0.18).with_scale(Vec3::new(0.2, 0.55, 0.2))));
            }
            SkeletonVariant::Knight => {
                skel.spawn((Mesh3d(visuals.sword_mesh.clone()), MeshMaterial3d(visuals.metal_material.clone()), Transform::from_xyz(0.34, 0.68, 0.0).with_rotation(Quat::from_rotation_z(-0.16))));
                skel.spawn((Mesh3d(visuals.rib_mesh.clone()), MeshMaterial3d(visuals.cloth_material.clone()), Transform::from_xyz(0.0, 0.88, 0.04).with_scale(Vec3::new(1.04, 1.04, 1.08))));
            }
            SkeletonVariant::SwordShield => {
                skel.spawn((Mesh3d(visuals.sword_mesh.clone()), MeshMaterial3d(visuals.metal_material.clone()), Transform::from_xyz(0.34, 0.68, 0.0).with_rotation(Quat::from_rotation_z(-0.12))));
                skel.spawn((Mesh3d(visuals.shield_mesh.clone()), MeshMaterial3d(visuals.metal_material.clone()), Transform::from_xyz(-0.32, 0.72, 0.04).with_rotation(Quat::from_rotation_y(0.18))));
            }
            SkeletonVariant::Guard => {
                skel.spawn((Mesh3d(visuals.axe_mesh.clone()), MeshMaterial3d(visuals.metal_material.clone()), Transform::from_xyz(0.4, 0.74, 0.02).with_rotation(Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, -0.26))));
                skel.spawn((Mesh3d(visuals.rib_mesh.clone()), MeshMaterial3d(visuals.cloth_material.clone()), Transform::from_xyz(0.0, 0.88, 0.04).with_scale(Vec3::new(1.14, 1.08, 1.18))));
            }
            SkeletonVariant::Mage => {
                skel.spawn((Mesh3d(visuals.staff_mesh.clone()), MeshMaterial3d(visuals.wood_material.clone()), Transform::from_xyz(0.34, 0.64, 0.0).with_rotation(Quat::from_rotation_z(-0.14))));
                skel.spawn((Mesh3d(visuals.bow_mesh.clone()), MeshMaterial3d(visuals.glow_material.clone()), Transform::from_xyz(0.34, 1.18, 0.0).with_scale(Vec3::new(0.22, 0.22, 0.22))));
            }
            SkeletonVariant::King => {
                skel.spawn((Mesh3d(visuals.greatsword_mesh.clone()), MeshMaterial3d(visuals.metal_material.clone()), Transform::from_xyz(0.4, 0.74, 0.0).with_rotation(Quat::from_rotation_z(-0.18))));
                skel.spawn((Mesh3d(visuals.crown_mesh.clone()), MeshMaterial3d(visuals.crown_material.clone()), Transform::from_xyz(0.0, 1.56, 0.0)));
                skel.spawn((Mesh3d(visuals.rib_mesh.clone()), MeshMaterial3d(visuals.cloth_material.clone()), Transform::from_xyz(0.0, 0.88, 0.04).with_scale(Vec3::new(1.18, 1.12, 1.2))));
            }
        }
    });
}

fn skeleton_visual_root_y(scale: f32) -> f32 {
    -0.62 + 0.22 * scale
}

fn init_landing_audio(
    mut commands: Commands,
    mut audio_sources: ResMut<Assets<AudioSource>>,
) {
    let thump = audio_sources.add(AudioSource {
        bytes: Arc::from(generate_landing_thump_wav()),
    });
    let step_left = audio_sources.add(AudioSource {
        bytes: Arc::from(generate_footstep_wav(0.19, 144.0, 96.0)),
    });
    let step_right = audio_sources.add(AudioSource {
        bytes: Arc::from(generate_footstep_wav(0.23, 158.0, 104.0)),
    });
    commands.insert_resource(LandingAudio { thump, step_left, step_right });
}

fn generate_landing_thump_wav() -> Vec<u8> {
    let sample_rate = 22_050u32;
    let sample_count = (sample_rate as f32 * 0.12) as usize;
    let mut samples = Vec::with_capacity(sample_count);

    for i in 0..sample_count {
        let t = i as f32 / sample_rate as f32;
        let progress = i as f32 / sample_count as f32;
        let envelope = (1.0 - progress).powf(2.8);
        let low = (t * (92.0 - progress * 34.0) * std::f32::consts::TAU).sin();
        let body = (t * (58.0 - progress * 18.0) * std::f32::consts::TAU).sin();
        let click = ((i as u32).wrapping_mul(73).wrapping_add(19) % 31) as f32 / 15.0 - 1.0;
        let click = click * (1.0 - progress).powf(5.5);
        let value = (low * 0.58 + body * 0.24 + click * 0.18) * envelope;
        samples.push((value.clamp(-1.0, 1.0) * i16::MAX as f32 * 0.72) as i16);
    }

    pcm_to_wav_bytes(&samples, sample_rate)
}

fn generate_footstep_wav(grit: f32, high_freq: f32, low_freq: f32) -> Vec<u8> {
    let sample_rate = 22_050u32;
    let sample_count = (sample_rate as f32 * 0.085) as usize;
    let mut samples = Vec::with_capacity(sample_count);

    for i in 0..sample_count {
        let t = i as f32 / sample_rate as f32;
        let progress = i as f32 / sample_count as f32;
        let envelope = (1.0 - progress).powf(3.5);
        let body = (t * low_freq * std::f32::consts::TAU).sin();
        let snap = (t * (high_freq - progress * 28.0) * std::f32::consts::TAU).sin();
        let grain = (((i as u32).wrapping_mul(97).wrapping_add(13) % 29) as f32 / 14.0 - 1.0) * grit;
        let value = (body * 0.58 + snap * 0.22 + grain * 0.2) * envelope;
        samples.push((value.clamp(-1.0, 1.0) * i16::MAX as f32 * 0.5) as i16);
    }

    pcm_to_wav_bytes(&samples, sample_rate)
}

fn pcm_to_wav_bytes(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let channel_count = 1u16;
    let bits_per_sample = 16u16;
    let byte_rate = sample_rate * channel_count as u32 * (bits_per_sample as u32 / 8);
    let block_align = channel_count * (bits_per_sample / 8);
    let data_len = (samples.len() * std::mem::size_of::<i16>()) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);

    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channel_count.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    bytes
}

fn spawn_player_avatar(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    textures: &ProceduralTextureAssets,
) {
    let root = parent.spawn((
        PlayerAvatar,
        Transform {
            translation: Vec3::new(0.0, -0.605, 0.0),
            scale: Vec3::splat(0.265),
            ..default()
        },
        Visibility::Hidden,
    )).id();

    let head_mesh = meshes.add(Mesh::from(bevy::math::primitives::Sphere::new(0.19)));
    let torso_mesh = meshes.add(Mesh::from(bevy::math::primitives::Capsule3d::new(0.19, 0.56)));
    let limb_mesh = meshes.add(Mesh::from(bevy::math::primitives::Capsule3d::new(0.065, 0.48)));
    let leg_mesh = meshes.add(Mesh::from(bevy::math::primitives::Capsule3d::new(0.075, 0.62)));
    let shirt_mesh = meshes.add(Mesh::from(bevy::math::primitives::Capsule3d::new(0.215, 0.6)));
    let pants_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.5, 0.64, 0.28)));
    let hat_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.4, 0.12, 0.4)));
    let cape_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.7, 1.0, 0.07)));
    let shoe_mesh = meshes.add(create_boot_mesh());
    let glove_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.15, 0.15, 0.17)));
    let necklace_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.22, 0.05, 0.22)));
    let bag_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.3, 0.42, 0.15)));
    let watch_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.08, 0.1, 0.08)));
    let hair_mesh = meshes.add(Mesh::from(bevy::math::primitives::Sphere::new(0.2)));
    let left_hand_mesh = meshes.add(create_hand_mesh(false));
    let right_hand_mesh = meshes.add(create_hand_mesh(true));
    let detail_cube = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(1.0, 1.0, 1.0)));
    let weapon_segment_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(1.0, 1.0, 0.55)));
    let shoulder_mesh = meshes.add(Mesh::from(bevy::math::primitives::Sphere::new(0.095)));
    let neck_mesh = meshes.add(Mesh::from(bevy::math::primitives::Capsule3d::new(0.055, 0.14)));

    let skin = materials.add(StandardMaterial {
        base_color: Color::srgb(0.79, 0.69, 0.61),
        base_color_texture: Some(textures.skin.clone()),
        perceptual_roughness: 0.72,
        ..default()
    });
    let cloth = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.25, 0.3),
        base_color_texture: Some(textures.cloth.clone()),
        perceptual_roughness: 0.88,
        ..default()
    });
    let leather = materials.add(StandardMaterial {
        base_color: Color::srgb(0.29, 0.18, 0.1),
        base_color_texture: Some(textures.leather.clone()),
        perceptual_roughness: 0.78,
        ..default()
    });
    let trim = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.64, 0.68),
        base_color_texture: Some(textures.steel.clone()),
        perceptual_roughness: 0.24,
        metallic: 0.76,
        ..default()
    });
    let hair = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.08, 0.06),
        base_color_texture: Some(textures.hair.clone()),
        perceptual_roughness: 0.92,
        ..default()
    });

    let spawn_part = |avatar: &mut ChildSpawnerCommands,
                      part: AvatarPart,
                      mesh: Handle<Mesh>,
                      material: Handle<StandardMaterial>,
                      transform: Transform| {
        avatar.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            transform,
            part,
        )).id()
    };

    parent.commands().entity(root).with_children(|avatar| {
        let head = spawn_part(avatar, AvatarPart::Head, head_mesh.clone(), skin.clone(), Transform::from_xyz(0.0, 1.58, 0.02).with_scale(Vec3::new(1.0, 1.08, 0.96)));
        let torso = spawn_part(avatar, AvatarPart::Torso, torso_mesh.clone(), cloth.clone(), Transform::from_xyz(0.0, 0.96, 0.0).with_rotation(Quat::from_rotation_z(0.01)));
        let left_arm = spawn_part(avatar, AvatarPart::LeftArm, limb_mesh.clone(), skin.clone(), Transform::from_xyz(-0.36, 0.92, 0.0).with_rotation(Quat::from_rotation_z(0.05)));
        let right_arm = spawn_part(avatar, AvatarPart::RightArm, limb_mesh.clone(), skin.clone(), Transform::from_xyz(0.36, 0.92, 0.0).with_rotation(Quat::from_rotation_z(-0.05)));
        let left_leg = spawn_part(avatar, AvatarPart::LeftLeg, leg_mesh.clone(), cloth.clone(), Transform::from_xyz(-0.14, 0.15, 0.01));
        let right_leg = spawn_part(avatar, AvatarPart::RightLeg, leg_mesh.clone(), cloth.clone(), Transform::from_xyz(0.14, 0.15, 0.01));

        avatar.commands().entity(left_arm).with_children(|arm| {
            arm.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(0.0, -0.06, 0.015).with_scale(Vec3::new(0.2, 0.28, 0.22)),
            ));
            arm.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(leather.clone()),
                Transform::from_xyz(0.0, -0.22, 0.03).with_scale(Vec3::new(0.13, 0.14, 0.18)),
            ));
            arm.spawn((
                Mesh3d(left_hand_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(-0.018, -0.49, 0.005),
            ));
        });
        avatar.commands().entity(right_arm).with_children(|arm| {
            arm.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(0.0, -0.06, 0.015).with_scale(Vec3::new(0.2, 0.28, 0.22)),
            ));
            arm.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(leather.clone()),
                Transform::from_xyz(0.0, -0.22, 0.03).with_scale(Vec3::new(0.13, 0.14, 0.18)),
            ));
            arm.spawn((
                Mesh3d(right_hand_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.018, -0.49, 0.005),
            ));
        });

        let shirt = spawn_part(avatar, AvatarPart::Shirt, shirt_mesh.clone(), cloth.clone(), Transform::from_xyz(0.0, 0.97, 0.02));
        let pants = spawn_part(avatar, AvatarPart::Pants, pants_mesh.clone(), cloth.clone(), Transform::from_xyz(0.0, 0.38, 0.02));
        let hat = spawn_part(avatar, AvatarPart::Hat, hat_mesh.clone(), cloth.clone(), Transform::from_xyz(0.0, 1.82, 0.0));
        let cape = spawn_part(avatar, AvatarPart::Cape, cape_mesh.clone(), cloth.clone(), Transform::from_xyz(0.0, 0.86, 0.16).with_rotation(Quat::from_rotation_x(0.05)));
        let main_hand = avatar.spawn((
            AvatarPart::MainHand,
            Transform::from_xyz(0.57, 0.63, 0.01),
            Visibility::Inherited,
        )).id();
        let off_hand = avatar.spawn((
            AvatarPart::OffHand,
            Transform::from_xyz(-0.57, 0.63, 0.01),
            Visibility::Inherited,
        )).id();
        let left_shoe = spawn_part(avatar, AvatarPart::Shoes, shoe_mesh.clone(), leather.clone(), Transform::from_xyz(-0.145, -0.34, 0.055));
        let right_shoe = spawn_part(avatar, AvatarPart::Shoes, shoe_mesh.clone(), leather.clone(), Transform::from_xyz(0.145, -0.34, 0.055));
        spawn_part(avatar, AvatarPart::Gloves, glove_mesh.clone(), leather.clone(), Transform::from_xyz(-0.36, 0.32, 0.01));
        spawn_part(avatar, AvatarPart::Gloves, glove_mesh.clone(), leather.clone(), Transform::from_xyz(0.36, 0.32, 0.01));
        spawn_part(avatar, AvatarPart::Necklace, necklace_mesh.clone(), trim.clone(), Transform::from_xyz(0.0, 1.27, 0.02));
        let bag = spawn_part(avatar, AvatarPart::Bag, bag_mesh.clone(), leather.clone(), Transform::from_xyz(-0.28, 0.78, 0.18));
        spawn_part(avatar, AvatarPart::Watch, watch_mesh.clone(), trim.clone(), Transform::from_xyz(0.34, 0.38, 0.035));

        avatar.commands().entity(head).with_children(|head_nodes| {
            head_nodes.spawn((
                Mesh3d(hair_mesh.clone()),
                MeshMaterial3d(hair.clone()),
                Transform::from_xyz(0.0, 0.06, -0.035).with_scale(Vec3::new(1.02, 0.84, 0.98)),
            ));
            head_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(hair.clone()),
                Transform::from_xyz(0.0, 0.085, 0.125).with_scale(Vec3::new(0.26, 0.025, 0.03)),
            ));
            for eye_x in [-0.055, 0.055] {
                head_nodes.spawn((
                    Mesh3d(detail_cube.clone()),
                    MeshMaterial3d(trim.clone()),
                    Transform::from_xyz(eye_x, 0.03, 0.18).with_scale(Vec3::new(0.018, 0.028, 0.016)),
                ));
            }
            head_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.0, -0.015, 0.16).with_scale(Vec3::new(0.035, 0.09, 0.06)),
            ));
            head_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.0, -0.12, 0.12).with_scale(Vec3::new(0.14, 0.05, 0.06)),
            ));
            for ear_x in [-0.165, 0.165] {
                head_nodes.spawn((
                    Mesh3d(detail_cube.clone()),
                    MeshMaterial3d(skin.clone()),
                    Transform::from_xyz(ear_x, 0.01, 0.015).with_scale(Vec3::new(0.035, 0.09, 0.028)),
                ));
            }
            head_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(hair.clone()),
                Transform::from_xyz(0.0, -0.085, 0.17).with_scale(Vec3::new(0.08, 0.01, 0.02)),
            ));
        });

        avatar.commands().entity(torso).with_children(|torso_nodes| {
            torso_nodes.spawn((
                Mesh3d(neck_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.0, 0.34, 0.005),
            ));
            torso_nodes.spawn((
                Mesh3d(shoulder_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(-0.235, 0.24, 0.0),
            ));
            torso_nodes.spawn((
                Mesh3d(shoulder_mesh.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.235, 0.24, 0.0),
            ));
            torso_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(trim.clone()),
                Transform::from_xyz(0.0, 0.04, 0.14).with_scale(Vec3::new(0.24, 0.38, 0.045)),
            ));
            torso_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(leather.clone()),
                Transform::from_xyz(0.0, -0.24, 0.125).with_scale(Vec3::new(0.48, 0.055, 0.08)),
            ));
            torso_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(trim.clone()),
                Transform::from_xyz(0.0, -0.15, 0.16).with_scale(Vec3::new(0.055, 0.11, 0.03)),
            ));
        });

        avatar.commands().entity(shirt).with_children(|shirt_nodes| {
            shirt_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(0.0, 0.24, 0.02).with_scale(Vec3::new(0.74, 0.08, 0.68)),
            ));
            shirt_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(trim.clone()),
                Transform::from_xyz(0.0, -0.31, 0.1).with_scale(Vec3::new(0.44, 0.055, 0.04)),
            ));
        });

        avatar.commands().entity(pants).with_children(|pants_nodes| {
            pants_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(leather.clone()),
                Transform::from_xyz(0.0, 0.23, 0.12).with_scale(Vec3::new(0.62, 0.05, 0.06)),
            ));
            pants_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(-0.12, -0.1, 0.06).with_scale(Vec3::new(0.14, 0.18, 0.18)),
            ));
            pants_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(0.12, -0.1, 0.06).with_scale(Vec3::new(0.14, 0.18, 0.18)),
            ));
        });

        avatar.commands().entity(hat).with_children(|hat_nodes| {
            hat_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(0.0, -0.055, 0.0).with_scale(Vec3::new(1.36, 0.12, 1.36)),
            ));
        });

        avatar.commands().entity(cape).with_children(|cape_nodes| {
            cape_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(cloth.clone()),
                Transform::from_xyz(0.0, 0.42, -0.015).with_scale(Vec3::new(0.18, 0.06, 0.22)),
            ));
        });

        avatar.commands().entity(bag).with_children(|bag_nodes| {
            bag_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(leather.clone()),
                Transform::from_xyz(0.0, 0.12, 0.09).with_scale(Vec3::new(0.82, 0.24, 0.18)),
            ));
            bag_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(leather.clone()),
                Transform::from_xyz(0.0, 0.0, -0.045).with_scale(Vec3::new(0.1, 1.16, 0.12)),
            ));
            bag_nodes.spawn((
                Mesh3d(detail_cube.clone()),
                MeshMaterial3d(trim.clone()),
                Transform::from_xyz(0.0, 0.01, 0.115).with_scale(Vec3::new(0.12, 0.14, 0.03)),
            ));
        });

        for leg in [left_leg, right_leg] {
            avatar.commands().entity(leg).with_children(|leg_nodes| {
                leg_nodes.spawn((
                    Mesh3d(detail_cube.clone()),
                    MeshMaterial3d(leather.clone()),
                    Transform::from_xyz(0.0, -0.08, 0.05).with_scale(Vec3::new(0.16, 0.14, 0.14)),
                ));
                leg_nodes.spawn((
                    Mesh3d(detail_cube.clone()),
                    MeshMaterial3d(trim.clone()),
                    Transform::from_xyz(0.0, 0.03, 0.07).with_scale(Vec3::new(0.11, 0.12, 0.035)),
                ));
            });
        }

        for shoe in [left_shoe, right_shoe] {
            avatar.commands().entity(shoe).with_children(|shoe_nodes| {
                shoe_nodes.spawn((
                    Mesh3d(detail_cube.clone()),
                    MeshMaterial3d(trim.clone()),
                    Transform::from_xyz(0.0, 0.0, -0.06).with_scale(Vec3::new(0.32, 0.2, 0.14)),
                ));
            });
        }

        avatar.commands().entity(main_hand).with_children(|weapon_nodes| {
            spawn_weapon_visual_segments(weapon_nodes, weapon_segment_mesh.clone(), materials, textures);
        });
        avatar.commands().entity(off_hand).with_children(|weapon_nodes| {
            spawn_weapon_visual_segments(weapon_nodes, weapon_segment_mesh.clone(), materials, textures);
        });
    });
}

fn spawn_first_person_viewmodel(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    textures: &ProceduralTextureAssets,
) {
    let root = parent.spawn((
        FirstPersonViewModel,
        Transform::default(),
        Visibility::Hidden,
    )).id();

    let left_hand_mesh = meshes.add(create_hand_mesh(false));
    let right_hand_mesh = meshes.add(create_hand_mesh(true));
    let detail_cube = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(1.0, 1.0, 1.0)));
    let weapon_segment_mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(1.0, 1.0, 0.55)));
    let fist_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.79, 0.69, 0.61),
        base_color_texture: Some(textures.skin.clone()),
        perceptual_roughness: 0.74,
        ..default()
    });
    let sleeve_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.24, 0.29),
        base_color_texture: Some(textures.cloth.clone()),
        perceptual_roughness: 0.88,
        ..default()
    });
    let cuff_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.23, 0.15),
        base_color_texture: Some(textures.leather.clone()),
        perceptual_roughness: 0.92,
        ..default()
    });
    parent.commands().entity(root).with_children(|view| {
        let left_fist = view.spawn((
            Mesh3d(left_hand_mesh),
            MeshMaterial3d(fist_mat.clone()),
            Transform::from_xyz(-0.22, -0.22, -0.42)
                .with_rotation(Quat::from_rotation_y(0.35)),
            ViewModelPart::LeftFist,
            Visibility::Hidden,
        )).id();
        let right_fist = view.spawn((
            Mesh3d(right_hand_mesh),
            MeshMaterial3d(fist_mat),
            Transform::from_xyz(0.22, -0.22, -0.42)
                .with_rotation(Quat::from_rotation_y(-0.35)),
            ViewModelPart::RightFist,
            Visibility::Hidden,
        )).id();
        let primary_weapon = view.spawn((
            Transform::from_xyz(0.26, -0.16, -0.62)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, -0.48, 0.12, -0.16)),
            ViewModelPart::PrimaryWeapon,
            Visibility::Hidden,
        )).id();
        let secondary_weapon = view.spawn((
            Transform::from_xyz(-0.25, -0.2, -0.55)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, -0.24, -0.18, 0.28)),
            ViewModelPart::SecondaryWeapon,
            Visibility::Hidden,
        )).id();

        view.commands().entity(primary_weapon).with_children(|weapon_nodes| {
            spawn_weapon_visual_segments(weapon_nodes, weapon_segment_mesh.clone(), materials, textures);
        });
        view.commands().entity(secondary_weapon).with_children(|weapon_nodes| {
            spawn_weapon_visual_segments(weapon_nodes, weapon_segment_mesh.clone(), materials, textures);
        });
        for (fist, x) in [(left_fist, -0.02), (right_fist, 0.02)] {
            view.commands().entity(fist).with_children(|hand_nodes| {
                hand_nodes.spawn((
                    Mesh3d(detail_cube.clone()),
                    MeshMaterial3d(sleeve_mat.clone()),
                    Transform::from_xyz(x, 0.1, 0.14).with_scale(Vec3::new(0.2, 0.24, 0.22)),
                ));
                hand_nodes.spawn((
                    Mesh3d(detail_cube.clone()),
                    MeshMaterial3d(cuff_mat.clone()),
                    Transform::from_xyz(x, 0.0, 0.16).with_scale(Vec3::new(0.18, 0.08, 0.2)),
                ));
            });
        }
    });
}

fn spawn_weapon_visual_segments(
    parent: &mut ChildSpawnerCommands,
    segment_mesh: Handle<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    textures: &ProceduralTextureAssets,
) {
    for segment in [
        WeaponVisualSegment::Core,
        WeaponVisualSegment::Detail,
        WeaponVisualSegment::Grip,
        WeaponVisualSegment::Accent,
        WeaponVisualSegment::Pommel,
    ] {
        parent.spawn((
            Mesh3d(segment_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.45, 0.45, 0.48),
                base_color_texture: Some(textures.steel.clone()),
                perceptual_roughness: 0.55,
                metallic: 0.5,
                ..default()
            })),
            Transform::default(),
            Visibility::Hidden,
            segment,
        ));
    }
}

fn update_player_avatar_visuals(
    time: Res<Time>,
    camera_mode: Res<CameraModeSettings>,
    active_weapon: Res<ActiveWeapon>,
    animation: Res<ViewModelAnimation>,
    equipment: Res<Equipment>,
    player_motion: Query<(&PlayerMotion, &LookAngles), With<Player>>,
    camera_query: Query<&Transform, (With<PlayerCamera>, Without<Player>, Without<AvatarPart>, Without<WeaponVisualSegment>, Without<ViewModelPart>)>,
    avatar_roots: Query<&mut Visibility, (With<PlayerAvatar>, Without<AvatarPart>, Without<WeaponVisualSegment>, Without<ViewModelPart>)>,
    mut avatar_parts: Query<(&AvatarPart, &MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility), (With<AvatarPart>, Without<PlayerAvatar>, Without<PlayerCamera>, With<MeshMaterial3d<StandardMaterial>>, Without<WeaponVisualSegment>)>,
    mut avatar_weapon_roots: Query<(&AvatarPart, &Children, &mut Transform, &mut Visibility), (With<AvatarPart>, Without<PlayerAvatar>, Without<PlayerCamera>, Without<MeshMaterial3d<StandardMaterial>>, Without<WeaponVisualSegment>)>,
    mut weapon_segments: Query<(&WeaponVisualSegment, &MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility), (With<WeaponVisualSegment>, Without<PlayerAvatar>, Without<FirstPersonViewModel>, Without<AvatarPart>, Without<ViewModelPart>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    textures: Res<ProceduralTextureAssets>,
) {
    let show_avatar = camera_mode.third_person_enabled;
    let (move_amount, bob_phase, sprint_amount, look_pitch) = player_motion
        .single()
        .map(|(motion, look)| (motion.move_amount, motion.bob_phase, motion.sprint_amount, look.pitch))
        .unwrap_or((0.0, time.elapsed_secs() * 0.7, 0.0, 0.0));
    let primary_item = equipped_primary_item(&equipment);
    let secondary_item = equipped_secondary_item(&equipment);
    let primary_two_handed = primary_item.map(is_two_handed_item).unwrap_or(false);
    let recoil_strength = animation.recoil_strength;
    let recovery_strength = animation.recovery_strength;
    let avatar_opacity = camera_query
        .single()
        .ok()
        .map(|camera_transform| third_person_avatar_opacity(&camera_mode, camera_transform))
        .unwrap_or(1.0);

    for mut visibility in avatar_roots {
        *visibility = if show_avatar && avatar_opacity > 0.05 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    for (part, material, mut transform, mut visibility) in &mut avatar_parts {
        let (part_visible, color, scale, rotation) = avatar_part_style(*part, &equipment);
        *visibility = if show_avatar && part_visible && avatar_opacity > 0.05 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        transform.scale = scale * avatar_part_pose_scale(*part, bob_phase, move_amount, sprint_amount);
        transform.rotation = rotation * avatar_part_pose_rotation(
            *part,
            move_amount,
            bob_phase,
            sprint_amount,
            look_pitch,
            active_weapon.slot,
            primary_two_handed,
            primary_item,
            secondary_item,
            recoil_strength,
            recovery_strength,
        );

        if let Some(mat) = materials.get_mut(&material.0) {
            mat.base_color = color_with_alpha(color, avatar_opacity);
            mat.base_color_texture = Some(avatar_part_texture(&textures, *part));
            mat.alpha_mode = if avatar_opacity < 0.995 { AlphaMode::Blend } else { AlphaMode::Opaque };
            mat.emissive = avatar_material_emissive(*part, &equipment, color) * avatar_opacity;
            mat.metallic = avatar_material_metallic(*part, &equipment);
            mat.perceptual_roughness = avatar_material_roughness(*part, &equipment);
        }
    }

    for (part, children, mut transform, mut visibility) in &mut avatar_weapon_roots {
        if !matches!(part, AvatarPart::MainHand | AvatarPart::OffHand) {
            continue;
        }
        let item = avatar_part_equipped_item(*part, &equipment);
        let (part_visible, _, scale, rotation) = avatar_item_style(item, false, Color::WHITE, *part);
        *visibility = if show_avatar && part_visible && avatar_opacity > 0.05 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        transform.translation = avatar_weapon_root_translation(*part, bob_phase, move_amount, active_weapon.slot, primary_two_handed, primary_item, secondary_item, recoil_strength, recovery_strength);
        transform.scale = scale;
        transform.rotation = rotation * avatar_weapon_hold_rotation(*part, bob_phase, move_amount, active_weapon.slot, primary_two_handed, primary_item, secondary_item, recoil_strength, recovery_strength);
        update_weapon_visual_children(children, item, &mut weapon_segments, &mut materials, &textures, avatar_opacity);
    }
}

fn avatar_part_pose_rotation(
    part: AvatarPart,
    move_amount: f32,
    bob_phase: f32,
    sprint_amount: f32,
    look_pitch: f32,
    active_slot: WeaponSlot,
    primary_two_handed: bool,
    primary_item: Option<&Item>,
    secondary_item: Option<&Item>,
    recoil_strength: f32,
    recovery_strength: f32,
) -> Quat {
    let arm_swing = bob_phase.sin() * (0.14 + move_amount * 0.24 + sprint_amount * 0.14);
    let leg_swing = bob_phase.sin() * (0.22 + move_amount * 0.34 + sprint_amount * 0.18);
    let torso_sway = bob_phase.cos() * (0.03 + move_amount * 0.05);
    let breathing = (bob_phase * 0.5).sin() * 0.03;
    let attack_rotation = avatar_attack_part_rotation(
        part,
        active_slot,
        primary_two_handed,
        primary_item,
        secondary_item,
        recoil_strength,
        recovery_strength,
    );
    match part {
        AvatarPart::Head => Quat::from_rotation_x(look_pitch * 0.22 + breathing * 0.35) * attack_rotation,
        AvatarPart::Torso => (Quat::from_rotation_z(torso_sway) * Quat::from_rotation_x(-0.04 - sprint_amount * 0.08)) * attack_rotation,
        AvatarPart::LeftArm => {
            let (pitch_bias, yaw_bias, roll_bias) = avatar_arm_weapon_pose(true, primary_item, secondary_item, primary_two_handed);
            Quat::from_euler(EulerRot::XYZ, pitch_bias - arm_swing * 0.8, yaw_bias, roll_bias + torso_sway) * attack_rotation
        }
        AvatarPart::RightArm => {
            let (pitch_bias, yaw_bias, roll_bias) = avatar_arm_weapon_pose(false, primary_item, secondary_item, primary_two_handed);
            Quat::from_euler(EulerRot::XYZ, pitch_bias + arm_swing * 0.82, yaw_bias, roll_bias - torso_sway) * attack_rotation
        }
        AvatarPart::LeftLeg => Quat::from_rotation_x(-leg_swing),
        AvatarPart::RightLeg => Quat::from_rotation_x(leg_swing),
        AvatarPart::Cape => Quat::from_rotation_x(0.08 + move_amount * 0.14 + sprint_amount * 0.12) * attack_rotation,
        AvatarPart::Hat => Quat::from_rotation_z(torso_sway * 0.7),
        AvatarPart::Bag => Quat::from_rotation_z(-0.14 - torso_sway * 1.4) * attack_rotation,
        AvatarPart::Watch => Quat::from_rotation_x(0.45) * Quat::from_rotation_z(-arm_swing * 0.45),
        AvatarPart::Necklace => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 + breathing * 0.6),
        _ => Quat::IDENTITY,
    }
}

fn avatar_part_pose_scale(part: AvatarPart, bob_phase: f32, move_amount: f32, sprint_amount: f32) -> Vec3 {
    let breathing = (bob_phase * 0.5).sin() * (0.012 + sprint_amount * 0.004);
    match part {
        AvatarPart::Head => Vec3::splat(1.0 + breathing * 0.35),
        AvatarPart::Torso | AvatarPart::Shirt => Vec3::new(1.0, 1.0 + breathing, 1.0 + breathing * 0.25),
        AvatarPart::Cape => Vec3::new(1.0, 1.0 + move_amount * 0.05 + sprint_amount * 0.06, 1.0),
        AvatarPart::Hat => Vec3::new(1.0 + breathing * 0.2, 1.0, 1.0 + breathing * 0.12),
        _ => Vec3::ONE,
    }
}

fn avatar_weapon_root_translation(
    part: AvatarPart,
    bob_phase: f32,
    move_amount: f32,
    active_slot: WeaponSlot,
    primary_two_handed: bool,
    primary_item: Option<&Item>,
    secondary_item: Option<&Item>,
    recoil_strength: f32,
    recovery_strength: f32,
) -> Vec3 {
    let sway = bob_phase.sin() * (0.03 + move_amount * 0.04);
    match part {
        AvatarPart::MainHand => {
            let slot_bias = if active_slot == WeaponSlot::Primary { 0.05 } else { -0.02 };
            let x = if primary_two_handed { 0.48 } else { 0.66 };
            let weapon_bias = avatar_weapon_pose_offset(primary_item, part, primary_two_handed);
            Vec3::new(x, 0.66 + sway * 0.35 + slot_bias, 0.02 + sway * 0.2)
                + weapon_bias
                + avatar_attack_weapon_translation(primary_item, part, active_slot, recoil_strength, recovery_strength)
        }
        AvatarPart::OffHand => {
            let slot_bias = if active_slot == WeaponSlot::Secondary { 0.04 } else { -0.01 };
            let x = if primary_two_handed { -0.32 } else { -0.66 };
            let supporting_item = if primary_two_handed { primary_item } else { secondary_item };
            let weapon_bias = avatar_weapon_pose_offset(supporting_item, part, primary_two_handed);
            Vec3::new(x, 0.67 - sway * 0.28 + slot_bias, 0.02 - sway * 0.16)
                + weapon_bias
                + avatar_attack_weapon_translation(supporting_item, part, active_slot, recoil_strength, recovery_strength)
        }
        _ => Vec3::ZERO,
    }
}

fn avatar_weapon_hold_rotation(
    part: AvatarPart,
    bob_phase: f32,
    move_amount: f32,
    active_slot: WeaponSlot,
    primary_two_handed: bool,
    primary_item: Option<&Item>,
    secondary_item: Option<&Item>,
    recoil_strength: f32,
    recovery_strength: f32,
) -> Quat {
    let sway = bob_phase.cos() * (0.05 + move_amount * 0.08);
    match part {
        AvatarPart::MainHand => {
            let slot_roll = if active_slot == WeaponSlot::Primary { -0.08 } else { 0.03 };
            let two_handed_pitch = if primary_two_handed { -0.12 } else { 0.0 };
            let weapon_bias = avatar_weapon_pose_rotation(primary_item, part, primary_two_handed);
            weapon_bias
                * Quat::from_euler(EulerRot::XYZ, two_handed_pitch + sway * 0.3, 0.0, slot_roll + sway)
                * avatar_attack_weapon_rotation(primary_item, part, active_slot, recoil_strength, recovery_strength)
        }
        AvatarPart::OffHand => {
            let slot_roll = if active_slot == WeaponSlot::Secondary { 0.1 } else { -0.02 };
            let two_handed_pitch = if primary_two_handed { -0.24 } else { 0.0 };
            let supporting_item = if primary_two_handed { primary_item } else { secondary_item };
            let weapon_bias = avatar_weapon_pose_rotation(supporting_item, part, primary_two_handed);
            weapon_bias
                * Quat::from_euler(EulerRot::XYZ, two_handed_pitch - sway * 0.24, 0.0, slot_roll - sway * 0.75)
                * avatar_attack_weapon_rotation(supporting_item, part, active_slot, recoil_strength, recovery_strength)
        }
        _ => Quat::IDENTITY,
    }
}

fn update_first_person_viewmodel(
    time: Res<Time>,
    hit_stop: Res<HitStopState>,
    camera_mode: Res<CameraModeSettings>,
    active_weapon: Res<ActiveWeapon>,
    mut animation: ResMut<ViewModelAnimation>,
    equipment: Res<Equipment>,
    player_motion: Query<&PlayerMotion, With<Player>>,
    model_roots: Query<&mut Visibility, (With<FirstPersonViewModel>, Without<PlayerAvatar>, Without<ViewModelPart>, Without<WeaponVisualSegment>)>,
    mut hand_parts: Query<(&ViewModelPart, &mut Transform, &mut Visibility), (With<ViewModelPart>, Without<FirstPersonViewModel>, With<MeshMaterial3d<StandardMaterial>>, Without<WeaponVisualSegment>)>,
    mut weapon_roots: Query<(&ViewModelPart, &Children, &mut Transform, &mut Visibility), (With<ViewModelPart>, Without<FirstPersonViewModel>, Without<MeshMaterial3d<StandardMaterial>>, Without<WeaponVisualSegment>)>,
    mut weapon_segments: Query<(&WeaponVisualSegment, &MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility), (With<WeaponVisualSegment>, Without<PlayerAvatar>, Without<FirstPersonViewModel>, Without<AvatarPart>, Without<ViewModelPart>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    textures: Res<ProceduralTextureAssets>,
) {
    let primary_item = equipped_primary_item(&equipment);
    let secondary_item = equipped_secondary_item(&equipment);
    let draw_target = !camera_mode.third_person_enabled && active_weapon.drawn;
    let delta_secs = time.delta_secs() * hit_stop.time_scale();
    let draw_speed = 10.0 * delta_secs;
    animation.draw_blend = if draw_target {
        (animation.draw_blend + draw_speed).min(1.0)
    } else {
        (animation.draw_blend - draw_speed).max(0.0)
    };
    let show_viewmodel = !camera_mode.third_person_enabled && animation.draw_blend > 0.02;
    let (recoil_strength, recovery_strength) = if animation.active {
        animation.swing.tick(std::time::Duration::from_secs_f32(delta_secs));
        let progress = animation.swing.fraction();
        let value = swing_curve(progress);
        if animation.swing.just_finished() {
            animation.active = false;
        }
        value
    } else {
        (0.0, 0.0)
    };
    animation.recoil_strength = recoil_strength;
    animation.recovery_strength = recovery_strength;
    let player_motion = player_motion.single().ok();
    let bob_phase = player_motion.map(|motion| motion.bob_phase).unwrap_or(time.elapsed_secs() * 0.5);
    let move_amount = player_motion.map(|motion| motion.move_amount).unwrap_or(0.0);
    let jump_visual = player_motion.map(|motion| motion.jump_visual).unwrap_or(0.0);
    let landing_dip = player_motion.map(|motion| motion.landing_dip).unwrap_or(0.0);
    let primary_two_handed = primary_item.map(is_two_handed_item).unwrap_or(false);

    for mut visibility in model_roots {
        *visibility = if show_viewmodel { Visibility::Visible } else { Visibility::Hidden };
    }

    for (part, mut transform, mut visibility) in &mut hand_parts {
        match part {
            ViewModelPart::LeftFist => {
                let show = show_viewmodel
                    && ((active_weapon.slot == WeaponSlot::Primary && (primary_item.is_none() || primary_two_handed))
                        || (active_weapon.slot == WeaponSlot::Secondary && secondary_item.is_none()));
                *visibility = if show { Visibility::Visible } else { Visibility::Hidden };
                let mut base = if active_weapon.slot == WeaponSlot::Primary && primary_two_handed {
                    two_handed_hand_transform(false)
                } else {
                    Transform::from_xyz(-0.22, -0.22, -0.42)
                        .with_rotation(Quat::from_rotation_y(0.35))
                };
                if show {
                    base.translation += idle_bob_offset(bob_phase, move_amount, active_weapon.slot, false);
                    base.rotation *= idle_bob_rotation(bob_phase, move_amount, active_weapon.slot, false);
                    base.translation += fist_swing_offset(recoil_strength, recovery_strength, false, active_weapon.slot, primary_two_handed, primary_item, secondary_item);
                    base.rotation *= fist_swing_rotation(recoil_strength, recovery_strength, false, active_weapon.slot, primary_two_handed, primary_item, secondary_item);
                    base = apply_viewmodel_impact(base, jump_visual, landing_dip, false);
                }
                base = apply_draw_transition(base, active_weapon.slot, animation.draw_blend, false);
                *transform = base;
            }
            ViewModelPart::RightFist => {
                let show = show_viewmodel
                    && active_weapon.slot == WeaponSlot::Primary
                    && (primary_item.is_none() || primary_two_handed);
                *visibility = if show { Visibility::Visible } else { Visibility::Hidden };
                let mut base = if primary_two_handed {
                    two_handed_hand_transform(true)
                } else {
                    Transform::from_xyz(0.22, -0.22, -0.42)
                        .with_rotation(Quat::from_rotation_y(-0.35))
                };
                if show {
                    base.translation += idle_bob_offset(bob_phase, move_amount, active_weapon.slot, true);
                    base.rotation *= idle_bob_rotation(bob_phase, move_amount, active_weapon.slot, true);
                    base.translation += fist_swing_offset(recoil_strength, recovery_strength, true, active_weapon.slot, primary_two_handed, primary_item, secondary_item);
                    base.rotation *= fist_swing_rotation(recoil_strength, recovery_strength, true, active_weapon.slot, primary_two_handed, primary_item, secondary_item);
                    base = apply_viewmodel_impact(base, jump_visual, landing_dip, true);
                }
                base = apply_draw_transition(base, active_weapon.slot, animation.draw_blend, true);
                *transform = base;
            }
            _ => {}
        }
    }

    for (part, children, mut transform, mut visibility) in &mut weapon_roots {
        match part {
            ViewModelPart::PrimaryWeapon => {
                if let Some(item) = primary_item {
                    let show = show_viewmodel && active_weapon.slot == WeaponSlot::Primary;
                    *visibility = if show { Visibility::Visible } else { Visibility::Hidden };
                    let mut base = primary_weapon_view_transform(item);
                    if show {
                        base.translation += idle_bob_offset(bob_phase, move_amount, WeaponSlot::Primary, true);
                        base.rotation *= idle_bob_rotation(bob_phase, move_amount, WeaponSlot::Primary, true);
                        base.translation += weapon_swing_offset(recoil_strength, recovery_strength, WeaponSlot::Primary, primary_two_handed, Some(item));
                        base.rotation *= weapon_swing_rotation(recoil_strength, recovery_strength, WeaponSlot::Primary, primary_two_handed, Some(item));
                        base = apply_viewmodel_impact(base, jump_visual, landing_dip, true);
                    }
                    base = apply_draw_transition(base, WeaponSlot::Primary, animation.draw_blend, true);
                    *transform = base;
                    update_weapon_visual_children(children, Some(item), &mut weapon_segments, &mut materials, &textures, 1.0);
                } else {
                    *visibility = Visibility::Hidden;
                    update_weapon_visual_children(children, None, &mut weapon_segments, &mut materials, &textures, 1.0);
                }
            }
            ViewModelPart::SecondaryWeapon => {
                if let Some(item) = secondary_item {
                    let show = show_viewmodel && active_weapon.slot == WeaponSlot::Secondary;
                    *visibility = if show { Visibility::Visible } else { Visibility::Hidden };
                    let mut base = secondary_weapon_view_transform(item);
                    if show {
                        base.translation += idle_bob_offset(bob_phase, move_amount, WeaponSlot::Secondary, false);
                        base.rotation *= idle_bob_rotation(bob_phase, move_amount, WeaponSlot::Secondary, false);
                        base.translation += weapon_swing_offset(recoil_strength, recovery_strength, WeaponSlot::Secondary, false, Some(item));
                        base.rotation *= weapon_swing_rotation(recoil_strength, recovery_strength, WeaponSlot::Secondary, false, Some(item));
                        base = apply_viewmodel_impact(base, jump_visual, landing_dip, false);
                    }
                    base = apply_draw_transition(base, WeaponSlot::Secondary, animation.draw_blend, false);
                    *transform = base;
                    update_weapon_visual_children(children, Some(item), &mut weapon_segments, &mut materials, &textures, 1.0);
                } else {
                    *visibility = Visibility::Hidden;
                    update_weapon_visual_children(children, None, &mut weapon_segments, &mut materials, &textures, 1.0);
                }
            }
            _ => {}
        }
    }
}

fn equipped_primary_item(equipment: &Equipment) -> Option<&Item> {
    equipment.twohand.as_ref().or(equipment.mainhand.as_ref())
}

fn is_two_handed_item(item: &Item) -> bool {
    item.equip_slot == Some(crate::items::EquipSlot::TwoHanded)
}

fn equipped_secondary_item(equipment: &Equipment) -> Option<&Item> {
    equipment.offhand.as_ref()
}

fn third_person_avatar_opacity(camera_mode: &CameraModeSettings, camera_transform: &Transform) -> f32 {
    if !camera_mode.third_person_enabled {
        return 1.0;
    }

    let desired = Vec3::new(
        camera_mode.shoulder_offset.abs().clamp(0.0, 0.9),
        camera_mode.camera_height.clamp(0.15, 0.95),
        camera_mode.follow_distance.clamp(1.6, 4.2),
    )
    .length();
    let actual = camera_transform.translation.length();
    let restore_threshold = (desired * 0.82).max(1.1);
    let fade_start = restore_threshold * 0.62;
    let opacity = ((actual - fade_start) / (restore_threshold - fade_start).max(0.001)).clamp(0.0, 1.0);
    (0.18 + opacity * 0.82).clamp(0.18, 1.0)
}

fn color_with_alpha(color: Color, alpha: f32) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, alpha.clamp(0.0, 1.0))
}

fn avatar_arm_weapon_pose(
    left_arm: bool,
    primary_item: Option<&Item>,
    secondary_item: Option<&Item>,
    primary_two_handed: bool,
) -> (f32, f32, f32) {
    let held_item = if left_arm {
        if primary_two_handed { primary_item } else { secondary_item }
    } else {
        primary_item
    };

    match held_item.and_then(|item| item.weapon) {
        Some(WeaponKind::Dagger) | Some(WeaponKind::ShortSword) => {
            if left_arm { (-0.34, -0.05, 0.22) } else { (-0.62, 0.04, -0.2) }
        }
        Some(WeaponKind::Hatchet) => {
            if left_arm { (-0.28, -0.03, 0.16) } else { (-0.58, 0.06, -0.28) }
        }
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => {
            if left_arm { (-0.26, -0.08, 0.24) } else { (-0.36, 0.04, -0.08) }
        }
        Some(WeaponKind::MagicStaff) => {
            if left_arm { (-0.64, -0.12, 0.18) } else { (-0.78, 0.1, -0.14) }
        }
        Some(WeaponKind::TwoHandedSword) | Some(WeaponKind::LongSword) => {
            if left_arm { (-0.62, -0.1, 0.2) } else { (-0.84, 0.08, -0.22) }
        }
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => {
            if left_arm { (-0.56, -0.08, 0.14) } else { (-0.74, 0.1, -0.3) }
        }
        Some(WeaponKind::Scythe) => {
            if left_arm { (-0.44, -0.22, 0.28) } else { (-0.8, 0.16, -0.36) }
        }
        None => {
            if left_arm {
                if secondary_item.is_some() || primary_two_handed { (-0.38, 0.0, 0.16) } else { (-0.08, 0.0, 0.16) }
            } else if primary_item.is_some() {
                (-0.52, 0.0, -0.16)
            } else {
                (-0.12, 0.0, -0.16)
            }
        }
    }
}

fn avatar_weapon_pose_offset(item: Option<&Item>, part: AvatarPart, primary_two_handed: bool) -> Vec3 {
    let lateral = if matches!(part, AvatarPart::OffHand) { -1.0 } else { 1.0 };
    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) | Some(WeaponKind::ShortSword) => Vec3::new(0.02 * lateral, 0.02, 0.08),
        Some(WeaponKind::Hatchet) => Vec3::new(0.05 * lateral, 0.03, 0.06),
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => Vec3::new(0.0, 0.01, 0.12),
        Some(WeaponKind::MagicStaff) => Vec3::new(-0.02 * lateral, 0.07, if primary_two_handed { 0.1 } else { 0.04 }),
        Some(WeaponKind::TwoHandedSword) | Some(WeaponKind::LongSword) => Vec3::new(-0.05 * lateral, 0.08, 0.03),
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Vec3::new(0.03 * lateral, -0.02, 0.03),
        Some(WeaponKind::Scythe) => Vec3::new(-0.08 * lateral, 0.06, -0.02),
        None => Vec3::ZERO,
    }
}

fn avatar_weapon_pose_rotation(item: Option<&Item>, part: AvatarPart, primary_two_handed: bool) -> Quat {
    let side = if matches!(part, AvatarPart::OffHand) { -1.0 } else { 1.0 };
    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) | Some(WeaponKind::ShortSword) => {
            Quat::from_euler(EulerRot::XYZ, 0.18, 0.02 * side, -0.14 * side)
        }
        Some(WeaponKind::Hatchet) => {
            Quat::from_euler(EulerRot::XYZ, 0.08, 0.0, -0.22 * side)
        }
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => {
            Quat::from_euler(EulerRot::XYZ, 0.24, -0.05 * side, -0.04 * side)
        }
        Some(WeaponKind::MagicStaff) => {
            Quat::from_euler(EulerRot::XYZ, -0.22 - if primary_two_handed { 0.08 } else { 0.0 }, 0.08 * side, -0.08 * side)
        }
        Some(WeaponKind::TwoHandedSword) | Some(WeaponKind::LongSword) => {
            Quat::from_euler(EulerRot::XYZ, -0.16, 0.1 * side, -0.12 * side)
        }
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => {
            Quat::from_euler(EulerRot::XYZ, -0.04, 0.06 * side, -0.24 * side)
        }
        Some(WeaponKind::Scythe) => {
            Quat::from_euler(EulerRot::XYZ, -0.08, 0.2 * side, -0.42 * side)
        }
        None => Quat::IDENTITY,
    }
}

fn avatar_attack_part_rotation(
    part: AvatarPart,
    active_slot: WeaponSlot,
    primary_two_handed: bool,
    primary_item: Option<&Item>,
    secondary_item: Option<&Item>,
    recoil_strength: f32,
    recovery_strength: f32,
) -> Quat {
    let attack = recoil_strength - recovery_strength * 0.9;
    let active_item = match active_slot {
        WeaponSlot::Primary => primary_item,
        WeaponSlot::Secondary => secondary_item,
    };

    match part {
        AvatarPart::Torso => match active_item.and_then(|item| item.weapon) {
            Some(WeaponKind::Dagger) | Some(WeaponKind::ShortSword) => Quat::from_euler(EulerRot::XYZ, -0.12 * recoil_strength + 0.08 * recovery_strength, 0.1 * attack, -0.08 * attack),
            Some(WeaponKind::Hatchet) => Quat::from_euler(EulerRot::XYZ, -0.18 * recoil_strength + 0.1 * recovery_strength, 0.12 * attack, -0.16 * attack),
            Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Quat::from_euler(EulerRot::XYZ, -0.22 * recoil_strength + 0.14 * recovery_strength, 0.06 * attack, -0.24 * attack),
            Some(WeaponKind::Scythe) => Quat::from_euler(EulerRot::XYZ, -0.14 * recoil_strength + 0.12 * recovery_strength, 0.2 * attack, -0.28 * attack),
            Some(WeaponKind::MagicStaff) | Some(WeaponKind::Book) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Lantern) => Quat::from_euler(EulerRot::XYZ, -0.08 * recoil_strength + 0.12 * recovery_strength, -0.06 * attack, -0.08 * attack),
            Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Quat::from_euler(EulerRot::XYZ, -0.18 * recoil_strength + 0.12 * recovery_strength, 0.08 * attack, -0.18 * attack),
            None => Quat::IDENTITY,
        },
        AvatarPart::Head => Quat::from_euler(EulerRot::XYZ, -0.05 * recoil_strength + 0.04 * recovery_strength, 0.03 * attack, 0.0),
        AvatarPart::LeftArm => {
            let use_active = active_slot == WeaponSlot::Secondary || primary_two_handed;
            if !use_active {
                return Quat::IDENTITY;
            }
            match active_item.and_then(|item| item.weapon) {
                Some(WeaponKind::Scythe) => Quat::from_euler(EulerRot::XYZ, 0.24 * recoil_strength, 0.14 * attack, 0.18 * attack),
                Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Quat::from_euler(EulerRot::XYZ, 0.12 * recoil_strength, 0.08 * attack, 0.1 * attack),
                Some(WeaponKind::MagicStaff) | Some(WeaponKind::Book) => Quat::from_euler(EulerRot::XYZ, 0.08 * recoil_strength, -0.04 * attack, 0.06 * attack),
                Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Quat::from_euler(EulerRot::XYZ, 0.18 * recoil_strength, 0.06 * attack, 0.1 * attack),
                _ => Quat::IDENTITY,
            }
        }
        AvatarPart::RightArm => {
            if active_slot != WeaponSlot::Primary {
                return Quat::IDENTITY;
            }
            match active_item.and_then(|item| item.weapon) {
                Some(WeaponKind::Dagger) => Quat::from_euler(EulerRot::XYZ, -0.45 * recoil_strength + 0.2 * recovery_strength, 0.12 * attack, -0.2 * attack),
                Some(WeaponKind::ShortSword) => Quat::from_euler(EulerRot::XYZ, -0.32 * recoil_strength + 0.18 * recovery_strength, 0.08 * attack, -0.32 * attack),
                Some(WeaponKind::Hatchet) => Quat::from_euler(EulerRot::XYZ, -0.5 * recoil_strength + 0.2 * recovery_strength, 0.14 * attack, -0.52 * attack),
                Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Quat::from_euler(EulerRot::XYZ, -0.26 * recoil_strength + 0.16 * recovery_strength, 0.08 * attack, -0.18 * attack),
                Some(WeaponKind::Scythe) => Quat::from_euler(EulerRot::XYZ, -0.18 * recoil_strength + 0.16 * recovery_strength, 0.22 * attack, -0.62 * attack),
                Some(WeaponKind::MagicStaff) => Quat::from_euler(EulerRot::XYZ, -0.22 * recoil_strength + 0.18 * recovery_strength, -0.04 * attack, -0.16 * attack),
                Some(WeaponKind::Book) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Lantern) => Quat::from_euler(EulerRot::XYZ, -0.14 * recoil_strength + 0.16 * recovery_strength, 0.04 * attack, -0.08 * attack),
                Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Quat::from_euler(EulerRot::XYZ, -0.34 * recoil_strength + 0.2 * recovery_strength, 0.08 * attack, -0.28 * attack),
                None => Quat::IDENTITY,
            }
        }
        AvatarPart::Cape | AvatarPart::Bag => Quat::from_euler(EulerRot::XYZ, 0.16 * recoil_strength - 0.06 * recovery_strength, -0.04 * attack, 0.0),
        _ => Quat::IDENTITY,
    }
}

fn avatar_attack_weapon_translation(
    item: Option<&Item>,
    part: AvatarPart,
    active_slot: WeaponSlot,
    recoil_strength: f32,
    recovery_strength: f32,
) -> Vec3 {
    let attack = recoil_strength - recovery_strength * 0.92;
    let attacking_this_hand = matches!((part, active_slot), (AvatarPart::MainHand, WeaponSlot::Primary) | (AvatarPart::OffHand, WeaponSlot::Secondary));
    if !attacking_this_hand {
        return Vec3::ZERO;
    }

    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) => Vec3::new(0.0, -0.02 * recoil_strength, 0.22 * recoil_strength - 0.18 * recovery_strength),
        Some(WeaponKind::ShortSword) => Vec3::new(0.04 * attack, -0.04 * recoil_strength, 0.18 * recoil_strength - 0.14 * recovery_strength),
        Some(WeaponKind::Hatchet) => Vec3::new(0.08 * attack, -0.08 * recoil_strength, 0.1 * recoil_strength - 0.08 * recovery_strength),
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Vec3::new(0.02 * attack, -0.14 * recoil_strength, 0.18 * recoil_strength - 0.1 * recovery_strength),
        Some(WeaponKind::Scythe) => Vec3::new(0.14 * attack, -0.04 * recoil_strength, 0.08 * recoil_strength - 0.06 * recovery_strength),
        Some(WeaponKind::MagicStaff) => Vec3::new(0.02 * attack, -0.06 * recoil_strength, 0.12 * recoil_strength - 0.14 * recovery_strength),
        Some(WeaponKind::Book) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Lantern) => Vec3::new(0.0, -0.02 * recoil_strength, 0.06 * recoil_strength - 0.08 * recovery_strength),
        Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Vec3::new(0.02 * attack, -0.08 * recoil_strength, 0.16 * recoil_strength - 0.12 * recovery_strength),
        None => Vec3::ZERO,
    }
}

fn avatar_attack_weapon_rotation(
    item: Option<&Item>,
    part: AvatarPart,
    active_slot: WeaponSlot,
    recoil_strength: f32,
    recovery_strength: f32,
) -> Quat {
    let attack = recoil_strength - recovery_strength * 0.88;
    let side = if matches!(part, AvatarPart::OffHand) { -1.0 } else { 1.0 };
    let attacking_this_hand = matches!((part, active_slot), (AvatarPart::MainHand, WeaponSlot::Primary) | (AvatarPart::OffHand, WeaponSlot::Secondary));
    if !attacking_this_hand {
        return Quat::IDENTITY;
    }

    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) => Quat::from_euler(EulerRot::XYZ, -0.55 * recoil_strength + 0.24 * recovery_strength, 0.08 * attack, -0.18 * attack * side),
        Some(WeaponKind::ShortSword) => Quat::from_euler(EulerRot::XYZ, -0.34 * recoil_strength + 0.18 * recovery_strength, 0.06 * attack, -0.42 * attack * side),
        Some(WeaponKind::Hatchet) => Quat::from_euler(EulerRot::XYZ, -0.52 * recoil_strength + 0.22 * recovery_strength, 0.08 * attack, -0.88 * attack * side),
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Quat::from_euler(EulerRot::XYZ, -0.26 * recoil_strength + 0.18 * recovery_strength, 0.04 * attack, -0.24 * attack * side),
        Some(WeaponKind::Scythe) => Quat::from_euler(EulerRot::XYZ, -0.16 * recoil_strength + 0.18 * recovery_strength, 0.24 * attack, -1.08 * attack * side),
        Some(WeaponKind::MagicStaff) => Quat::from_euler(EulerRot::XYZ, -0.28 * recoil_strength + 0.24 * recovery_strength, -0.08 * attack, -0.24 * attack * side),
        Some(WeaponKind::Book) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Lantern) => Quat::from_euler(EulerRot::XYZ, -0.12 * recoil_strength + 0.14 * recovery_strength, 0.02 * attack, -0.06 * attack * side),
        Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Quat::from_euler(EulerRot::XYZ, -0.32 * recoil_strength + 0.2 * recovery_strength, 0.08 * attack, -0.56 * attack * side),
        None => Quat::IDENTITY,
    }
}

fn primary_weapon_view_transform(item: &Item) -> Transform {
    let (translation, rotation, scale) = match item.weapon {
        Some(WeaponKind::Dagger) | Some(WeaponKind::ShortSword) => (
            Vec3::new(0.26, -0.19, -0.55),
            Quat::from_euler(EulerRot::XYZ, -0.4, 0.2, -0.15),
            Vec3::new(0.65, 0.85, 0.65),
        ),
        Some(WeaponKind::Hatchet) => (
            Vec3::new(0.29, -0.16, -0.57),
            Quat::from_euler(EulerRot::XYZ, -0.45, 0.25, -0.35),
            Vec3::new(0.95, 0.95, 0.75),
        ),
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => (
            Vec3::new(0.24, -0.18, -0.5),
            Quat::from_euler(EulerRot::XYZ, -0.2, 0.15, -0.05),
            Vec3::new(0.8, 0.55, 0.8),
        ),
        Some(WeaponKind::TwoHandedSword) | Some(WeaponKind::LongSword) | Some(WeaponKind::DoubleAxe) | Some(WeaponKind::Scythe) | Some(WeaponKind::GiantHammer) | Some(WeaponKind::MagicStaff) => (
            Vec3::new(0.34, -0.16, -0.7),
            Quat::from_euler(EulerRot::XYZ, -0.72, 0.12, -0.12),
            Vec3::new(1.1, 1.4, 0.9),
        ),
        None => (
            Vec3::new(0.28, -0.18, -0.62),
            Quat::from_euler(EulerRot::XYZ, -0.55, 0.15, -0.2),
            Vec3::ONE,
        ),
    };

    Transform {
        translation,
        rotation,
        scale,
    }
}

fn avatar_part_style(part: AvatarPart, equipment: &Equipment) -> (bool, Color, Vec3, Quat) {
    let skin = Color::srgb(0.83, 0.74, 0.66);
    let base_cloth = Color::srgb(0.16, 0.21, 0.28);
    match part {
        AvatarPart::Head => (true, skin, Vec3::new(1.03, 1.02, 1.01), Quat::IDENTITY),
        AvatarPart::Torso => (true, base_cloth, Vec3::new(0.94, 1.0, 0.98), Quat::IDENTITY),
        AvatarPart::LeftArm => (true, skin, Vec3::new(0.98, 1.02, 0.98), Quat::from_rotation_z(0.12)),
        AvatarPart::RightArm => (true, skin, Vec3::new(0.98, 1.02, 0.98), Quat::from_rotation_z(-0.12)),
        AvatarPart::LeftLeg => (true, base_cloth, Vec3::new(0.98, 1.04, 0.98), Quat::from_rotation_z(0.03)),
        AvatarPart::RightLeg => (true, base_cloth, Vec3::new(0.98, 1.04, 0.98), Quat::from_rotation_z(-0.03)),
        AvatarPart::Shirt => avatar_item_style(equipment.shirt.as_ref(), false, base_cloth, part),
        AvatarPart::Pants => avatar_item_style(equipment.pants.as_ref(), false, base_cloth, part),
        AvatarPart::Hat => avatar_item_style(equipment.hat.as_ref(), false, base_cloth, part),
        AvatarPart::Cape => avatar_item_style(equipment.cape.as_ref(), false, base_cloth, part),
        AvatarPart::MainHand => avatar_item_style(equipment.twohand.as_ref().or(equipment.mainhand.as_ref()), false, base_cloth, part),
        AvatarPart::OffHand => avatar_item_style(equipment.offhand.as_ref(), false, base_cloth, part),
        AvatarPart::Shoes => avatar_item_style(equipment.shoes.as_ref(), false, base_cloth, part),
        AvatarPart::Gloves => avatar_item_style(equipment.gloves.as_ref(), false, base_cloth, part),
        AvatarPart::Necklace => avatar_item_style(equipment.necklace.as_ref(), false, base_cloth, part),
        AvatarPart::Bag => avatar_item_style(equipment.bag.as_ref(), false, base_cloth, part),
        AvatarPart::Watch => avatar_item_style(equipment.watch.as_ref(), false, base_cloth, part),
    }
}

fn avatar_item_style(
    item: Option<&crate::items::Item>,
    default_visible: bool,
    default_color: Color,
    part: AvatarPart,
) -> (bool, Color, Vec3, Quat) {
    match item {
        Some(item) => {
            let seed = item_visual_seed(item);
            let color = item_surface_color(item, 0.18 + ((seed >> 5) & 0x3) as f32 * 0.04);
            let (scale, rotation) = avatar_item_transform(item, part, seed);
            (true, color, scale, rotation)
        }
        None => (default_visible, default_color, Vec3::ONE, Quat::IDENTITY),
    }
}

fn avatar_item_transform(item: &Item, part: AvatarPart, seed: u32) -> (Vec3, Quat) {
    let wobble = ((seed & 0x7) as f32 - 3.0) * 0.03;
    match (part, item.weapon) {
        (AvatarPart::Hat, _) => (Vec3::new(1.08 + wobble.abs() * 0.16, 1.0 + wobble.abs() * 0.18, 1.04), Quat::from_rotation_z(wobble * 0.25)),
        (AvatarPart::Cape, _) => (Vec3::new(1.04, 1.12 + wobble.abs() * 0.28, 1.0), Quat::from_rotation_x(0.06 + wobble * 0.16)),
        (AvatarPart::Shirt, _) => (Vec3::new(1.0, 1.02 + wobble.abs() * 0.1, 1.03), Quat::IDENTITY),
        (AvatarPart::Pants, _) => (Vec3::new(1.0, 1.03 + wobble.abs() * 0.12, 1.02), Quat::IDENTITY),
        (AvatarPart::Bag, _) => (Vec3::new(1.0 + rarity_scale(item) * 0.12, 1.0 + rarity_scale(item) * 0.1, 1.0), Quat::from_rotation_z(-0.08 - wobble * 0.22)),
        (AvatarPart::Watch, _) => (Vec3::new(1.0, 1.0, 1.0 + rarity_scale(item) * 0.12), Quat::from_rotation_x(0.36)),
        (AvatarPart::Necklace, _) => (Vec3::new(1.0 + rarity_scale(item) * 0.08, 1.0, 1.0 + rarity_scale(item) * 0.08), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        (AvatarPart::Gloves, _) => (Vec3::new(1.02 + wobble.abs() * 0.1, 1.0, 1.04), Quat::from_rotation_z(wobble * 0.25)),
        (AvatarPart::Shoes, _) => (Vec3::new(1.02 + wobble.abs() * 0.1, 1.0, 1.06), Quat::IDENTITY),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::Dagger)) => (Vec3::new(0.56, 0.96, 0.42), Quat::from_rotation_z(0.12 + wobble * 0.6)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::ShortSword)) => (Vec3::new(0.62, 1.08, 0.42), Quat::from_rotation_z(0.05 + wobble * 0.5)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::Hatchet)) => (Vec3::new(0.88, 1.0, 0.54), Quat::from_rotation_z(-0.16 + wobble * 0.45)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::Lantern)) => (Vec3::new(0.88, 0.7, 0.88), Quat::from_rotation_z(0.08)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::CrystalBall)) => (Vec3::new(0.92, 0.82, 0.92), Quat::IDENTITY),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::Book)) => (Vec3::new(0.96, 0.72, 0.72), Quat::from_rotation_x(0.24) * Quat::from_rotation_z(0.08)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::TwoHandedSword | WeaponKind::LongSword)) => (Vec3::new(0.64, 1.28, 0.38), Quat::from_rotation_z(-0.1 + wobble * 0.4)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::DoubleAxe)) => (Vec3::new(0.92, 1.16, 0.56), Quat::from_rotation_z(-0.18 + wobble * 0.35)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::Scythe)) => (Vec3::new(0.88, 1.3, 0.34), Quat::from_rotation_z(-0.24 + wobble * 0.3)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::GiantHammer)) => (Vec3::new(0.98, 1.08, 0.7), Quat::from_rotation_z(-0.12 + wobble * 0.3)),
        (AvatarPart::MainHand | AvatarPart::OffHand, Some(WeaponKind::MagicStaff)) => (Vec3::new(0.52, 1.34, 0.52), Quat::from_rotation_z(-0.06 + wobble * 0.25)),
        (AvatarPart::MainHand | AvatarPart::OffHand, None) => (Vec3::ONE, Quat::IDENTITY),
        _ => (Vec3::ONE, Quat::IDENTITY),
    }
}

fn avatar_material_emissive(part: AvatarPart, equipment: &Equipment, color: Color) -> LinearRgba {
    let Some(item) = avatar_part_equipped_item(part, equipment) else { return LinearRgba::BLACK; };
    let glow = match item.weapon {
        Some(WeaponKind::CrystalBall) => 0.42,
        Some(WeaponKind::Lantern) => 0.32,
        Some(WeaponKind::MagicStaff) => 0.28,
        _ => rarity_scale(item) * 0.1,
    };
    let srgba = color.to_srgba();
    LinearRgba::rgb(srgba.red * glow, srgba.green * glow, srgba.blue * glow)
}

fn avatar_material_metallic(part: AvatarPart, equipment: &Equipment) -> f32 {
    avatar_part_equipped_item(part, equipment)
        .map(|item| match item.weapon {
            Some(WeaponKind::Book | WeaponKind::Lantern) => 0.2,
            Some(WeaponKind::CrystalBall) => 0.55,
            Some(_) => 0.82,
            None => 0.16 + rarity_scale(item) * 0.24,
        })
        .unwrap_or(0.0)
}

fn avatar_material_roughness(part: AvatarPart, equipment: &Equipment) -> f32 {
    avatar_part_equipped_item(part, equipment)
        .map(|item| match item.weapon {
            Some(WeaponKind::CrystalBall) => 0.12,
            Some(WeaponKind::Lantern) => 0.3,
            Some(_) => 0.34,
            None => 0.46,
        })
        .unwrap_or(0.85)
}

fn avatar_part_equipped_item<'a>(part: AvatarPart, equipment: &'a Equipment) -> Option<&'a Item> {
    match part {
        AvatarPart::Shirt => equipment.shirt.as_ref(),
        AvatarPart::Pants => equipment.pants.as_ref(),
        AvatarPart::Hat => equipment.hat.as_ref(),
        AvatarPart::Cape => equipment.cape.as_ref(),
        AvatarPart::MainHand => equipment.twohand.as_ref().or(equipment.mainhand.as_ref()),
        AvatarPart::OffHand => equipment.offhand.as_ref(),
        AvatarPart::Shoes => equipment.shoes.as_ref(),
        AvatarPart::Gloves => equipment.gloves.as_ref(),
        AvatarPart::Necklace => equipment.necklace.as_ref(),
        AvatarPart::Bag => equipment.bag.as_ref(),
        AvatarPart::Watch => equipment.watch.as_ref(),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct WeaponSegmentDescriptor {
    translation: Vec3,
    scale: Vec3,
    rotation: Quat,
    surface: WeaponSurface,
}

fn update_weapon_visual_children(
    children: &Children,
    item: Option<&Item>,
    segment_parts: &mut Query<(&WeaponVisualSegment, &MeshMaterial3d<StandardMaterial>, &mut Transform, &mut Visibility), (With<WeaponVisualSegment>, Without<PlayerAvatar>, Without<FirstPersonViewModel>, Without<AvatarPart>, Without<ViewModelPart>)>,
    materials: &mut Assets<StandardMaterial>,
    textures: &ProceduralTextureAssets,
    opacity: f32,
) {
    for child in children.iter() {
        let Ok((segment, material, mut transform, mut visibility)) = segment_parts.get_mut(child) else {
            continue;
        };

        let Some(item) = item else {
            *visibility = Visibility::Hidden;
            continue;
        };

        let Some(descriptor) = weapon_segment_descriptor(item.weapon, *segment) else {
            *visibility = Visibility::Hidden;
            continue;
        };

        *visibility = Visibility::Visible;
        *transform = Transform {
            translation: descriptor.translation,
            rotation: descriptor.rotation,
            scale: descriptor.scale,
        };

        if let Some(mat) = materials.get_mut(&material.0) {
            apply_weapon_surface_material(mat, item, descriptor.surface, opacity, textures);
        }
    }
}

fn weapon_segment_descriptor(
    weapon: Option<WeaponKind>,
    segment: WeaponVisualSegment,
) -> Option<WeaponSegmentDescriptor> {
    use WeaponSurface::*;
    use WeaponVisualSegment::*;

    let descriptor = match weapon {
        Some(WeaponKind::Dagger) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.22, 0.0), scale: Vec3::new(0.028, 0.5, 0.016), rotation: Quat::IDENTITY, surface: Steel },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.035, 0.0), scale: Vec3::new(0.11, 0.025, 0.028), rotation: Quat::IDENTITY, surface: DarkSteel },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.19, 0.0), scale: Vec3::new(0.024, 0.19, 0.026), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.45, 0.0), scale: Vec3::new(0.012, 0.12, 0.014), rotation: Quat::from_rotation_z(0.1), surface: Steel },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.32, 0.0), scale: Vec3::new(0.036, 0.05, 0.036), rotation: Quat::from_rotation_z(0.2), surface: Brass },
        },
        Some(WeaponKind::ShortSword) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.24, 0.0), scale: Vec3::new(0.036, 0.76, 0.018), rotation: Quat::IDENTITY, surface: Steel },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.08, 0.0), scale: Vec3::new(0.16, 0.028, 0.038), rotation: Quat::IDENTITY, surface: DarkSteel },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.28, 0.0), scale: Vec3::new(0.028, 0.24, 0.03), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.63, 0.0), scale: Vec3::new(0.014, 0.14, 0.016), rotation: Quat::from_rotation_z(0.08), surface: Steel },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.45, 0.0), scale: Vec3::new(0.04, 0.06, 0.04), rotation: Quat::from_rotation_z(0.22), surface: Brass },
        },
        Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.34, 0.0), scale: Vec3::new(0.046, 1.14, 0.02), rotation: Quat::IDENTITY, surface: Steel },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.23, 0.0), scale: Vec3::new(0.22, 0.028, 0.044), rotation: Quat::IDENTITY, surface: DarkSteel },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.54, 0.0), scale: Vec3::new(0.03, 0.38, 0.03), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.9, 0.0), scale: Vec3::new(0.016, 0.18, 0.016), rotation: Quat::from_rotation_z(0.08), surface: Steel },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.82, 0.0), scale: Vec3::new(0.05, 0.08, 0.05), rotation: Quat::from_rotation_z(0.22), surface: Brass },
        },
        Some(WeaponKind::Hatchet) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.05, 0.0), scale: Vec3::new(0.03, 0.62, 0.03), rotation: Quat::IDENTITY, surface: Wood },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.08, 0.2, 0.0), scale: Vec3::new(0.18, 0.18, 0.036), rotation: Quat::from_rotation_z(0.12), surface: Steel },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.2, 0.0), scale: Vec3::new(0.038, 0.16, 0.038), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(-0.05, 0.16, 0.0), scale: Vec3::new(0.08, 0.12, 0.026), rotation: Quat::from_rotation_z(0.42), surface: DarkSteel },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.38, 0.0), scale: Vec3::new(0.036, 0.05, 0.036), rotation: Quat::IDENTITY, surface: Brass },
        },
        Some(WeaponKind::DoubleAxe) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.0, 0.0), scale: Vec3::new(0.032, 1.02, 0.032), rotation: Quat::IDENTITY, surface: Wood },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(-0.11, 0.26, 0.0), scale: Vec3::new(0.16, 0.24, 0.04), rotation: Quat::from_rotation_z(0.46), surface: Steel },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.28, 0.0), scale: Vec3::new(0.04, 0.22, 0.04), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.11, 0.26, 0.0), scale: Vec3::new(0.16, 0.24, 0.04), rotation: Quat::from_rotation_z(-0.46), surface: Steel },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.48, 0.0), scale: Vec3::new(0.04, 0.06, 0.04), rotation: Quat::IDENTITY, surface: Brass },
        },
        Some(WeaponKind::Scythe) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.0, 0.0), scale: Vec3::new(0.026, 1.18, 0.026), rotation: Quat::IDENTITY, surface: Wood },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.18, 0.54, 0.0), scale: Vec3::new(0.3, 0.04, 0.022), rotation: Quat::from_rotation_z(-1.06), surface: Steel },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.2, 0.0), scale: Vec3::new(0.038, 0.2, 0.038), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.28, 0.47, 0.0), scale: Vec3::new(0.14, 0.025, 0.02), rotation: Quat::from_rotation_z(-0.38), surface: DarkSteel },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.56, 0.0), scale: Vec3::new(0.036, 0.06, 0.036), rotation: Quat::IDENTITY, surface: Brass },
        },
        Some(WeaponKind::GiantHammer) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.03, 0.0), scale: Vec3::new(0.034, 1.1, 0.034), rotation: Quat::IDENTITY, surface: Wood },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.34, 0.0), scale: Vec3::new(0.3, 0.16, 0.12), rotation: Quat::IDENTITY, surface: DarkSteel },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.24, 0.0), scale: Vec3::new(0.042, 0.22, 0.042), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(-0.22, 0.34, 0.0), scale: Vec3::new(0.12, 0.1, 0.05), rotation: Quat::IDENTITY, surface: Steel },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.54, 0.0), scale: Vec3::new(0.045, 0.07, 0.045), rotation: Quat::IDENTITY, surface: Brass },
        },
        Some(WeaponKind::MagicStaff) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.0, 0.0), scale: Vec3::new(0.03, 1.22, 0.03), rotation: Quat::IDENTITY, surface: Wood },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.58, 0.0), scale: Vec3::new(0.14, 0.03, 0.03), rotation: Quat::IDENTITY, surface: Brass },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.22, 0.0), scale: Vec3::new(0.042, 0.18, 0.042), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.76, 0.0), scale: Vec3::new(0.1, 0.1, 0.1), rotation: Quat::IDENTITY, surface: Glass },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.5, 0.0), scale: Vec3::new(0.042, 0.065, 0.042), rotation: Quat::IDENTITY, surface: Brass },
        },
        Some(WeaponKind::Lantern) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.01, 0.0), scale: Vec3::new(0.15, 0.2, 0.15), rotation: Quat::IDENTITY, surface: Brass },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.01, 0.0), scale: Vec3::new(0.08, 0.12, 0.08), rotation: Quat::IDENTITY, surface: Ember },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.19, 0.0), scale: Vec3::new(0.026, 0.1, 0.026), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.225, 0.0), scale: Vec3::new(0.1, 0.018, 0.018), rotation: Quat::IDENTITY, surface: Brass },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.12, 0.0), scale: Vec3::new(0.1, 0.024, 0.1), rotation: Quat::IDENTITY, surface: Brass },
        },
        Some(WeaponKind::CrystalBall) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.03, 0.0), scale: Vec3::new(0.16, 0.05, 0.16), rotation: Quat::IDENTITY, surface: Brass },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.17, 0.0), scale: Vec3::new(0.16, 0.16, 0.16), rotation: Quat::IDENTITY, surface: Glass },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.05, 0.0), scale: Vec3::new(0.032, 0.1, 0.032), rotation: Quat::IDENTITY, surface: DarkSteel },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.17, 0.0), scale: Vec3::new(0.08, 0.08, 0.08), rotation: Quat::IDENTITY, surface: Ember },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.125, 0.0), scale: Vec3::new(0.07, 0.032, 0.07), rotation: Quat::IDENTITY, surface: Brass },
        },
        Some(WeaponKind::Book) => match segment {
            Core => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.0, 0.0), scale: Vec3::new(0.18, 0.26, 0.08), rotation: Quat::IDENTITY, surface: Paper },
            Detail => WeaponSegmentDescriptor { translation: Vec3::new(0.0, 0.0, 0.05), scale: Vec3::new(0.18, 0.26, 0.018), rotation: Quat::IDENTITY, surface: Leather },
            Grip => WeaponSegmentDescriptor { translation: Vec3::new(-0.082, 0.0, 0.0), scale: Vec3::new(0.018, 0.26, 0.084), rotation: Quat::IDENTITY, surface: Leather },
            Accent => WeaponSegmentDescriptor { translation: Vec3::new(0.065, 0.0, 0.048), scale: Vec3::new(0.016, 0.14, 0.016), rotation: Quat::IDENTITY, surface: Brass },
            Pommel => WeaponSegmentDescriptor { translation: Vec3::new(0.0, -0.15, 0.0), scale: Vec3::new(0.08, 0.018, 0.08), rotation: Quat::IDENTITY, surface: DarkSteel },
        },
        None => return None,
    };

    Some(descriptor)
}

fn apply_weapon_surface_material(material: &mut StandardMaterial, item: &Item, surface: WeaponSurface, opacity: f32, textures: &ProceduralTextureAssets) {
    let rarity = rarity_scale(item);
    let color = match surface {
        WeaponSurface::Steel => Color::srgb(0.52 + rarity * 0.035, 0.54 + rarity * 0.03, 0.58 + rarity * 0.03),
        WeaponSurface::DarkSteel => Color::srgb(0.24 + rarity * 0.03, 0.26 + rarity * 0.03, 0.3 + rarity * 0.035),
        WeaponSurface::Wood => Color::srgb(0.31 + rarity * 0.02, 0.21 + rarity * 0.015, 0.12 + rarity * 0.01),
        WeaponSurface::Leather => Color::srgb(0.18 + rarity * 0.012, 0.11 + rarity * 0.008, 0.07 + rarity * 0.008),
        WeaponSurface::Brass => Color::srgb(0.63 + rarity * 0.04, 0.53 + rarity * 0.035, 0.24 + rarity * 0.02),
        WeaponSurface::Glass => Color::srgb(0.54 + rarity * 0.04, 0.68 + rarity * 0.05, 0.8 + rarity * 0.06),
        WeaponSurface::Paper => Color::srgb(0.72 + rarity * 0.02, 0.68 + rarity * 0.02, 0.56 + rarity * 0.015),
        WeaponSurface::Ember => Color::srgb(0.92, 0.62 + rarity * 0.08, 0.18 + rarity * 0.04),
    };

    material.base_color = color_with_alpha(color, opacity);
    material.base_color_texture = Some(weapon_surface_texture(textures, surface));
    material.alpha_mode = if opacity < 0.995 { AlphaMode::Blend } else { AlphaMode::Opaque };
    material.metallic = match surface {
        WeaponSurface::Steel => 0.95,
        WeaponSurface::DarkSteel => 0.9,
        WeaponSurface::Brass => 0.9,
        WeaponSurface::Glass => 0.24,
        WeaponSurface::Ember => 0.0,
        _ => 0.02,
    };
    material.perceptual_roughness = match surface {
        WeaponSurface::Steel => 0.18,
        WeaponSurface::DarkSteel => 0.28,
        WeaponSurface::Brass => 0.22,
        WeaponSurface::Glass => 0.08,
        WeaponSurface::Paper => 0.68,
        WeaponSurface::Ember => 0.22,
        WeaponSurface::Wood => 0.64,
        WeaponSurface::Leather => 0.74,
    };
    material.emissive = match surface {
        WeaponSurface::Glass => LinearRgba::rgb(0.12 + rarity * 0.2, 0.18 + rarity * 0.24, 0.28 + rarity * 0.36),
        WeaponSurface::Ember => LinearRgba::rgb(1.1 + rarity * 0.35, 0.52 + rarity * 0.2, 0.08),
        _ => LinearRgba::BLACK,
    } * opacity;
}

fn item_surface_color(item: &Item, lift: f32) -> Color {
    if let Some(weapon) = item.weapon {
        return realistic_weapon_color(item, weapon, lift);
    }

    let seed = item_visual_seed(item);
    let base = item.rarity.color().to_srgba();
    let accent = ((seed >> 8) & 0xff) as f32 / 255.0;
    let slot_shift = match item.equip_slot {
        Some(crate::items::EquipSlot::Hat | crate::items::EquipSlot::Cape) => (0.12, 0.0, 0.08),
        Some(crate::items::EquipSlot::Shirt | crate::items::EquipSlot::Pants) => (0.05, 0.08, 0.02),
        Some(crate::items::EquipSlot::Bag) => (0.1, 0.05, 0.0),
        _ => (0.0, 0.0, 0.0),
    };
    Color::srgb(
        (base.red * (0.76 + accent * 0.18) + lift + slot_shift.0).clamp(0.0, 1.0),
        (base.green * (0.76 + (1.0 - accent) * 0.12) + lift * 0.75 + slot_shift.1).clamp(0.0, 1.0),
        (base.blue * (0.78 + accent * 0.1) + lift * 0.6 + slot_shift.2).clamp(0.0, 1.0),
    )
}

fn realistic_weapon_color(item: &Item, weapon: WeaponKind, lift: f32) -> Color {
    let rarity_tint = item.rarity.color().to_srgba();
    let accent = rarity_scale(item);
    let (base_r, base_g, base_b) = match weapon {
        WeaponKind::Lantern => (0.58, 0.44, 0.18),
        WeaponKind::Book => (0.34, 0.2, 0.12),
        WeaponKind::MagicStaff => (0.38, 0.27, 0.14),
        WeaponKind::CrystalBall => (0.55, 0.63, 0.72),
        WeaponKind::Hatchet => (0.48, 0.42, 0.34),
        _ => (0.56, 0.58, 0.62),
    };
    let tint_strength = match weapon {
        WeaponKind::CrystalBall => 0.28,
        WeaponKind::MagicStaff | WeaponKind::Lantern | WeaponKind::Book => 0.16,
        _ => 0.1,
    };
    Color::srgb(
        (base_r + rarity_tint.red * tint_strength + lift * 0.45 + accent * 0.05).clamp(0.0, 1.0),
        (base_g + rarity_tint.green * tint_strength + lift * 0.35 + accent * 0.04).clamp(0.0, 1.0),
        (base_b + rarity_tint.blue * tint_strength + lift * 0.25 + accent * 0.03).clamp(0.0, 1.0),
    )
}

fn item_visual_seed(item: &Item) -> u32 {
    let mut hash = 0x811C9DC5u32;
    for byte in item.name.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash ^= item.base_value() as u32;
    hash ^= (item.size.0 as u32) << 24;
    hash ^ ((item.size.1 as u32) << 16)
}

fn rarity_scale(item: &Item) -> f32 {
    match item.rarity {
        crate::items::Rarity::Common => 0.0,
        crate::items::Rarity::Uncommon => 0.14,
        crate::items::Rarity::Rare => 0.26,
        crate::items::Rarity::UltraRare => 0.38,
        crate::items::Rarity::Legendary => 0.52,
        crate::items::Rarity::Unique => 0.68,
    }
}

fn player_look_and_move(
    time: Res<Time>,
    mut commands: Commands,
    kb: Res<ButtonInput<KeyCode>>,
    mut mouse: MessageReader<MouseMotion>,
    rapier_ctx: ReadRapierContext,
    flags: Res<UIFlags>,
    open_chest: Res<OpenChest>,
    mut camera_state: ParamSet<(Res<CameraModeSettings>, ResMut<CameraImpulseState>)>,
    equipment: Res<Equipment>,
    maze: Res<ActiveMaze>,
    cfg: Res<WorldCfg>,
    tuning: Res<MovementTuning>,
    mut viewmodel_and_audio: ParamSet<(Res<ViewModelAnimation>, Res<LandingAudio>)>,
    mut q: Query<(&mut Transform, &mut LookAngles, &mut PlayerMotion), With<Player>>,
    mut cam_q: Query<&mut Transform, (With<PlayerCamera>, With<Camera3d>, Without<Player>)>,
    mut controller_queries: ParamSet<(
        Query<&mut KinematicCharacterController, With<Player>>,
        Query<&KinematicCharacterControllerOutput, With<Player>>,
    )>,
) {
    let rapier = rapier_ctx.single().ok();
    let grounded = controller_queries
        .p1()
        .single()
        .map(|out| out.grounded)
        .unwrap_or(true);
    let Ok((mut t, mut look, mut motion)) = q.single_mut() else { return; };
    let Ok(mut cam_t) = cam_q.single_mut() else { return; };
    let (recoil_strength, recovery_strength) = {
        let viewmodel_animation = viewmodel_and_audio.p0();
        (viewmodel_animation.recoil_strength, viewmodel_animation.recovery_strength)
    };
    let (landing_thump, step_left, step_right) = {
        let landing_audio = viewmodel_and_audio.p1();
        (
            landing_audio.thump.clone(),
            landing_audio.step_left.clone(),
            landing_audio.step_right.clone(),
        )
    };
    let interaction_ui_open = flags.pause_menu_open
        || flags.inventory_open
        || flags.spell_menu_open
        || open_chest.0.is_some();
    let delta_secs = time.delta_secs();
    let (third_person_enabled, shoulder_offset_setting, follow_distance_setting, camera_height_setting, shoulder_side_setting, collision_enabled) = {
        let camera_mode = camera_state.p0();
        (
            camera_mode.third_person_enabled,
            camera_mode.shoulder_offset,
            camera_mode.follow_distance,
            camera_mode.camera_height,
            camera_mode.shoulder_side,
            camera_mode.collision_enabled,
        )
    };
    let (impulse_translation, impulse_pitch, impulse_roll) = {
        let mut camera_impulse = camera_state.p1();
        camera_impulse.decay(delta_secs);
        (camera_impulse.translation, camera_impulse.pitch, camera_impulse.roll)
    };
    let actual_last_frame_move = if motion.last_position_valid {
        Vec2::new(t.translation.x, t.translation.z)
            .distance(Vec2::new(motion.last_position.x, motion.last_position.z))
    } else {
        0.0
    };
    let fall_speed = (-motion.vertical_velocity).max(0.0);
    let just_landed = grounded && !motion.was_grounded && fall_speed > 1.0;
    let current_position = t.translation;

    // mouse look
    let sens = 0.0022;
    for ev in mouse.read() {
        if interaction_ui_open {
            continue;
        }
        look.yaw += -ev.delta.x as f32 * sens;
        look.pitch = (look.pitch - ev.delta.y as f32 * sens).clamp(-1.35, 1.35);
    }

    // Keep the physics body upright; only yaw rotates the collider.
    t.rotation = Quat::from_rotation_y(look.yaw);

    // kinematic move with smoothing and directional weighting
    let mut input = Vec2::ZERO;
    let forward = (t.rotation * -Vec3::Z).normalize();
    let right = (t.rotation * Vec3::X).normalize();
    if !interaction_ui_open {
        if kb.pressed(KeyCode::KeyW) { input.y += 1.0; }
        if kb.pressed(KeyCode::KeyS) { input.y -= 1.0; }
        if kb.pressed(KeyCode::KeyA) { input.x += 1.0; }
        if kb.pressed(KeyCode::KeyD) { input.x -= 1.0; }
    }

    if input.length_squared() > 1.0 {
        input = input.normalize();
    }

    let sprint_pressed = kb.pressed(KeyCode::ShiftLeft) || kb.pressed(KeyCode::ShiftRight);
    let wants_sprint = sprint_pressed && input.y > 0.0 && motion.sprint_stamina > 0.08;
    if grounded {
        if wants_sprint && input.length_squared() > 0.0 {
            motion.sprint_stamina = (motion.sprint_stamina - tuning.sprint_drain * delta_secs).max(0.0);
        } else {
            motion.sprint_stamina = (motion.sprint_stamina + tuning.sprint_recover_ground * delta_secs).min(1.0);
        }
    } else {
        motion.sprint_stamina = (motion.sprint_stamina + tuning.sprint_recover_air * delta_secs).min(1.0);
    }

    motion.sprint_amount = if wants_sprint {
        (motion.sprint_amount + 4.2 * delta_secs).min(1.0)
    } else {
        (motion.sprint_amount - 6.0 * delta_secs).max(0.0)
    };

    let mut max_speed = 5.0 + 1.6 * motion.sprint_amount;
    max_speed *= movement_speed_multiplier(&equipment);
    if input.y < 0.0 {
        max_speed *= 0.84;
    }
    if input.x.abs() > 0.0 {
        max_speed *= 0.94;
    }

    let target_planar = input * max_speed;
    let accel = if grounded {
        if input.length_squared() > 0.0 { 17.0 + 3.5 * motion.sprint_amount } else { 12.0 }
    } else {
        if input.length_squared() > 0.0 { 3.0 } else { 2.0 }
    };
    let blend = 1.0 - (-accel * delta_secs).exp();
    motion.planar_velocity = motion.planar_velocity.lerp(target_planar, blend);
    if motion.planar_velocity.length_squared() < 0.0001 {
        motion.planar_velocity = Vec2::ZERO;
    }
    motion.move_amount = (motion.planar_velocity.length() / max_speed.max(0.001)).clamp(0.0, 1.0);
    if grounded && motion.move_amount > 0.05 {
        motion.bob_phase += delta_secs * (7.2 + 3.0 * motion.move_amount + 1.8 * motion.sprint_amount);
        let step_index = (motion.bob_phase / std::f32::consts::PI).floor() as i32;
        if step_index != motion.last_footstep_index {
            motion.last_footstep_index = step_index;
            let side_is_right = step_index.rem_euclid(2) == 0;
            let step_handle = if side_is_right {
                step_right.clone()
            } else {
                step_left.clone()
            };
            commands.spawn((
                AudioPlayer::new(step_handle),
                PlaybackSettings::DESPAWN
                    .with_volume(Volume::Linear(0.035 + 0.05 * motion.move_amount + 0.025 * motion.sprint_amount))
                    .with_speed(0.96 + 0.16 * motion.sprint_amount + if side_is_right { 0.04 } else { 0.0 }),
            ));
        }
    } else {
        motion.bob_phase = 0.0;
        motion.last_footstep_index = -1;
    }

    if grounded {
        if just_landed {
            let impact = ((fall_speed - 1.2) / 5.8).clamp(0.0, 1.0);
            motion.landing_dip = motion.landing_dip.max(0.16 + 0.24 * impact);
            commands.spawn((
                AudioPlayer::new(landing_thump.clone()),
                PlaybackSettings::DESPAWN
                    .with_volume(Volume::Linear(0.16 + 0.3 * impact))
                    .with_speed(1.08 - 0.22 * impact),
            ));
        }
        motion.vertical_velocity = 0.0;
        if !interaction_ui_open && kb.just_pressed(KeyCode::Space) {
            motion.vertical_velocity = tuning.jump_velocity;
            motion.jump_visual = 1.0;
            motion.landing_dip = motion.landing_dip.max(0.08);
        }
    } else {
        motion.vertical_velocity -= tuning.gravity * delta_secs;
        motion.vertical_velocity = motion.vertical_velocity.max(-tuning.max_fall_speed);
    }

    motion.jump_visual = (motion.jump_visual - 3.8 * delta_secs).max(0.0);
    motion.landing_dip = (motion.landing_dip - 4.8 * delta_secs).max(0.0);

    let jump_wave = (motion.jump_visual * std::f32::consts::PI).sin() * 0.022 - motion.jump_visual * 0.014;
    let landing_offset = -0.05 * motion.landing_dip;
    let landing_pitch = 0.048 * motion.landing_dip;

    // Camera pitch is local to the child camera and doesn't affect collisions.
    if third_person_enabled {
        let shoulder_side = if shoulder_side_setting >= 0.0 { 1.0 } else { -1.0 };
        let shoulder_offset = shoulder_offset_setting.abs().clamp(0.0, 0.9) * shoulder_side;
        let follow_distance = follow_distance_setting.clamp(1.6, 4.2);
        let camera_height = camera_height_setting.clamp(0.15, 0.95);
        let focus = Vec3::new(
            shoulder_offset * 0.18,
            -0.14 + look.pitch * 0.2 - motion.landing_dip * 0.06 + jump_wave * 0.5,
            0.0,
        );
        let desired_local = Vec3::new(
            shoulder_offset,
            camera_height + jump_wave * 0.35 + landing_offset * 0.25,
            -follow_distance,
        );
        let resolved_local = if collision_enabled {
            if let Some(rapier) = rapier {
                let pivot_world = current_position + t.rotation * focus;
                let desired_world = current_position + t.rotation * desired_local;
                let camera_vec = desired_world - pivot_world;
                let desired_distance = camera_vec.length();
                if desired_distance > 0.001 {
                    let dir = camera_vec / desired_distance;
                    let ray_start = pivot_world + dir * 0.22;
                    let ray_distance = desired_distance - 0.22;
                    if ray_distance > 0.001 {
                        if let Some((_hit, toi)) = rapier.cast_ray(
                            ray_start,
                            dir,
                            ray_distance,
                            true,
                            QueryFilter::default(),
                        ) {
                            let allowed = (toi - 0.12).max(0.05);
                            let resolved_world = ray_start + dir * allowed;
                            t.rotation.inverse() * (resolved_world - current_position)
                        } else {
                            desired_local
                        }
                    } else {
                        desired_local
                    }
                } else {
                    desired_local
                }
            } else {
                desired_local
            }
        } else {
            desired_local
        };
        let mut target = Transform::from_translation(resolved_local + impulse_translation);
        target.look_at(focus, Vec3::Y);
        target.rotation *= Quat::from_euler(EulerRot::XYZ, impulse_pitch, 0.0, impulse_roll);
        let camera_blend = 1.0 - (-9.0 * delta_secs).exp();
        cam_t.translation = cam_t.translation.lerp(target.translation, camera_blend);
        cam_t.rotation = cam_t.rotation.slerp(target.rotation, camera_blend).normalize();
    } else {
        let kick_pitch = -0.05 * recoil_strength + 0.03 * recovery_strength + impulse_pitch;
        let kick_roll = -0.018 * recoil_strength + 0.012 * recovery_strength + impulse_roll;
        let kick_translation = Vec3::new(
            0.0,
            -0.012 * recoil_strength,
            0.03 * recovery_strength - 0.025 * recoil_strength,
        ) + impulse_translation * 0.45;
        let jump_translation = Vec3::new(0.0, jump_wave + landing_offset, 0.015 * motion.jump_visual - 0.018 * motion.landing_dip);
        let head_bob = motion_bob_translation(motion.bob_phase, motion.move_amount);
        let head_sway = motion_bob_rotation(motion.bob_phase, motion.move_amount);
        cam_t.translation = Vec3::new(0.0, -0.26, 0.0) + kick_translation + head_bob + jump_translation;
        cam_t.rotation = (Quat::from_rotation_y(std::f32::consts::PI)
            * Quat::from_rotation_x(look.pitch + kick_pitch + 0.022 * motion.jump_visual - landing_pitch)
            * Quat::from_rotation_z(kick_roll)
            * head_sway)
            .normalize();
    }

    let mut controller_query = controller_queries.p0();
    let Ok(mut c) = controller_query.single_mut() else { return; };
    let mut vel = Vec3::ZERO;
    if !interaction_ui_open {
        vel = (right * motion.planar_velocity.x + forward * -motion.planar_velocity.y) * delta_secs;
    }
    let expected_move = motion.planar_velocity.length() * delta_secs;
    let snagged_last_frame = grounded
        && motion.move_amount > 0.34
        && expected_move > 0.03
        && actual_last_frame_move < expected_move * 0.3
        && actual_last_frame_move < 0.025;
    if snagged_last_frame {
        motion.snag_frames = motion.snag_frames.saturating_add(1);
    } else {
        motion.snag_frames = 0;
    }
    if !interaction_ui_open && motion.snag_frames >= 2 {
        let cell_x = (t.translation.x / cfg.tile).round() as i32;
        let cell_y = (t.translation.z / cfg.tile).round() as i32;
        if cell_x >= 0 && cell_y >= 0 && cell_x < maze.0.w as i32 && cell_y < maze.0.h as i32 {
            let cell_center = Vec2::new(cell_x as f32 * cfg.tile, cell_y as f32 * cfg.tile);
            let local = Vec2::new(t.translation.x, t.translation.z) - cell_center;
            let dominant_forward = motion.planar_velocity.y.abs() >= motion.planar_velocity.x.abs();
            let nudge_strength = 0.42 * delta_secs;
            if dominant_forward {
                let side_open = {
                    let cell = maze.0.cells[(cell_y as u32 * maze.0.w + cell_x as u32) as usize];
                    !cell.walls[1] || !cell.walls[3]
                };
                if side_open {
                    vel.x += (-local.x).clamp(-0.22, 0.22) * nudge_strength;
                }
            } else {
                let forward_open = {
                    let cell = maze.0.cells[(cell_y as u32 * maze.0.w + cell_x as u32) as usize];
                    !cell.walls[0] || !cell.walls[2]
                };
                if forward_open {
                    vel.z += (-local.y).clamp(-0.22, 0.22) * nudge_strength;
                }
            }
        }
    }
    if !interaction_ui_open {
        vel.y += motion.vertical_velocity * delta_secs;
        if grounded && motion.vertical_velocity <= 0.0 {
            vel.y -= 0.5 * delta_secs;
        }
    }
    motion.was_grounded = grounded;
    motion.last_position = current_position;
    motion.last_position_valid = true;
    c.translation = Some(vel);
}

fn record_snag_cells(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<WorldCfg>,
    maze: Res<ActiveMaze>,
    collision_debug: Res<CollisionDebugSettings>,
    debug_assets: Res<CollisionDebugAssets>,
    mut snag_debug: ResMut<SnagDebugState>,
    q: Query<(&Transform, &PlayerMotion), With<Player>>,
) {
    let Ok((transform, motion)) = q.single() else { return; };
    if !motion.last_position_valid {
        return;
    }

    let current = Vec2::new(transform.translation.x, transform.translation.z);
    let last = Vec2::new(motion.last_position.x, motion.last_position.z);
    let actual_move = current.distance(last);
    let expected_move = motion.planar_velocity.length() * time.delta_secs();
    let trying_to_move = motion.move_amount > 0.34 || motion.sprint_amount > 0.1;
    let snagged = motion.was_grounded
        && trying_to_move
        && expected_move > 0.03
        && actual_move < expected_move * 0.3
        && actual_move < 0.025;

    if !snagged {
        return;
    }

    let cell_x = (transform.translation.x / cfg.tile).round() as i32;
    let cell_y = (transform.translation.z / cfg.tile).round() as i32;
    if cell_x < 0 || cell_y < 0 || cell_x >= maze.0.w as i32 || cell_y >= maze.0.h as i32 {
        return;
    }

    let cell_key = (cell_x, cell_y);
    if !snag_debug.logged_cells.insert(cell_key) {
        return;
    }

    let idx = (cell_y as u32 * maze.0.w + cell_x as u32) as usize;
    let cell = maze.0.cells[idx];
    println!(
        "Snag detected near maze cell ({}, {}) at ({:.2}, {:.2}); walls N/E/S/W = {:?}",
        cell_x,
        cell_y,
        transform.translation.x,
        transform.translation.z,
        cell.walls,
    );

    commands.spawn((
        Mesh3d(debug_assets.snag_marker_mesh.clone()),
        MeshMaterial3d(debug_assets.snag_material.clone()),
        Transform::from_xyz(cell_x as f32 * cfg.tile, -0.97, cell_y as f32 * cfg.tile),
        if collision_debug.enabled { Visibility::Visible } else { Visibility::Hidden },
        CollisionDebugVisual,
        SnagMarker,
        WorldEntity,
        Name::new(format!("SnagMarker({}, {})", cell_x, cell_y)),
    )).with_children(|parent| {
        let side_offset = cfg.tile * 0.22;
        if cell.walls[0] {
            parent.spawn((
                Mesh3d(debug_assets.snag_wall_bar_mesh.clone()),
                MeshMaterial3d(debug_assets.snag_ns_material.clone()),
                Transform::from_xyz(0.0, 0.07, -side_offset),
                Visibility::Inherited,
            ));
        }
        if cell.walls[2] {
            parent.spawn((
                Mesh3d(debug_assets.snag_wall_bar_mesh.clone()),
                MeshMaterial3d(debug_assets.snag_ns_material.clone()),
                Transform::from_xyz(0.0, 0.07, side_offset),
                Visibility::Inherited,
            ));
        }
        if cell.walls[1] {
            parent.spawn((
                Mesh3d(debug_assets.snag_wall_bar_mesh.clone()),
                MeshMaterial3d(debug_assets.snag_ew_material.clone()),
                Transform::from_xyz(side_offset, 0.07, 0.0)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                Visibility::Inherited,
            ));
        }
        if cell.walls[3] {
            parent.spawn((
                Mesh3d(debug_assets.snag_wall_bar_mesh.clone()),
                MeshMaterial3d(debug_assets.snag_ew_material.clone()),
                Transform::from_xyz(-side_offset, 0.07, 0.0)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                Visibility::Inherited,
            ));
        }
    });
}

fn has_enemy_line_of_sight(
    rapier: &RapierContext,
    origin: Vec3,
    target: Vec3,
    player_e: Entity,
) -> bool {
    let to_target = target - origin;
    let distance = to_target.length();
    if distance <= 0.001 {
        return true;
    }
    let dir = to_target / distance;
    let start = origin + dir * 0.45;
    rapier
        .cast_ray(start, dir, distance.max(0.1), true, QueryFilter::default())
        .map(|(hit, _)| hit == player_e)
        .unwrap_or(true)
}

fn spawn_enemy_projectile(
    commands: &mut Commands,
    assets: &EnemyProjectileAssets,
    owner: Entity,
    origin: Vec3,
    direction: Vec3,
    kind: EnemyProjectileKind,
) {
    let (mesh, material, speed, damage, scale, lifetime) = match kind {
        EnemyProjectileKind::Arrow => (assets.arrow_mesh.clone(), assets.arrow_material.clone(), 10.5, 12.0, Vec3::ONE, 3.4),
        EnemyProjectileKind::Fireball => (assets.bolt_mesh.clone(), assets.fire_material.clone(), 7.2, 15.0, Vec3::splat(1.0), 3.0),
        EnemyProjectileKind::Zap => (assets.bolt_mesh.clone(), assets.zap_material.clone(), 11.0, 10.0, Vec3::new(0.8, 0.8, 1.2), 2.2),
        EnemyProjectileKind::WindSlash => (assets.bolt_mesh.clone(), assets.wind_material.clone(), 8.8, 9.0, Vec3::new(1.4, 0.55, 1.4), 2.6),
    };
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform {
            translation: origin,
            rotation: Quat::from_rotation_arc(Vec3::Z, dir),
            scale,
        },
        EnemyProjectile {
            velocity: dir * speed,
            damage,
            lifetime: Timer::from_seconds(lifetime, TimerMode::Once),
            owner,
            kind,
        },
        WorldEntity,
        Name::new("EnemyProjectile"),
    ));
}

fn skeleton_ai_system(
    time: Res<Time>,
    rapier_ctx: ReadRapierContext,
    projectile_assets: Res<EnemyProjectileAssets>,
    mut enemy_rng: ResMut<EnemyRuntimeRng>,
    mut commands: Commands,
    mut queries: ParamSet<(
        Query<(Entity, &Transform, Option<&Parry>), With<Player>>,
        Query<&mut Health, With<Player>>,
        Query<(Entity, &Transform, &mut KinematicCharacterController, &mut SkeletonEnemy, &Health, Option<&Wet>), With<SkeletonEnemy>>,
    )>,
) {
    let Ok(rapier) = rapier_ctx.single() else { return; };
    let (player_e, player_translation, player_parrying) = {
        let player_query = queries.p0();
        let Ok((player_e, player_t, player_parry)) = player_query.single() else { return; };
        (player_e, player_t.translation, player_parry.is_some())
    };
    let player_target = player_translation + Vec3::new(0.0, 0.86, 0.0);
    let delta_secs = time.delta_secs();
    let mut queued_player_damage = 0.0;
    let mut queued_rotations = Vec::new();

    for (enemy_e, transform, mut controller, mut enemy, health, wet) in &mut queries.p2() {
        if health.hp <= 0.0 {
            controller.translation = Some(Vec3::ZERO);
            continue;
        }

        enemy.attack_timer.tick(time.delta());
        let profile = skeleton_profile(enemy.variant);
        let move_speed = profile.move_speed * if wet.is_some() { 0.68 } else { 1.0 };
        let origin = transform.translation + Vec3::new(0.0, 0.84 * profile.scale, 0.0);
        let to_player = player_target - origin;
        let distance = to_player.length();
        let direction = to_player.normalize_or_zero();
        let line_of_sight = distance <= profile.aggro_range && has_enemy_line_of_sight(&rapier, origin, player_target, player_e);

        if direction != Vec3::ZERO {
            let yaw = direction.x.atan2(direction.z) + std::f32::consts::PI;
            queued_rotations.push((enemy_e, Quat::from_rotation_y(yaw)));
        }

        let mut desired_velocity = Vec3::ZERO;
        match enemy.variant {
            SkeletonVariant::Archer => {
                if line_of_sight && distance <= profile.attack_range && enemy.attack_timer.just_finished() {
                    spawn_enemy_projectile(
                        &mut commands,
                        &projectile_assets,
                        enemy_e,
                        origin + direction * 0.66,
                        direction,
                        EnemyProjectileKind::Arrow,
                    );
                }
                if distance > profile.attack_range * 0.8 {
                    desired_velocity = direction * move_speed;
                } else if distance < 4.0 {
                    desired_velocity = -direction * (move_speed * 0.8);
                }
            }
            SkeletonVariant::Mage => {
                if line_of_sight && distance <= profile.attack_range && enemy.attack_timer.just_finished() {
                    for _ in 0..3 {
                        let kind = match enemy_rng.0.random_range(0..3) {
                            0 => EnemyProjectileKind::Fireball,
                            1 => EnemyProjectileKind::Zap,
                            _ => EnemyProjectileKind::WindSlash,
                        };
                        let spread = enemy_rng.0.random_range(-0.16..0.16);
                        let shot_dir = (direction + Vec3::new(spread, enemy_rng.0.random_range(-0.04..0.08), -spread * 0.2)).normalize_or_zero();
                        spawn_enemy_projectile(
                            &mut commands,
                            &projectile_assets,
                            enemy_e,
                            origin + shot_dir * 0.58,
                            shot_dir,
                            kind,
                        );
                    }
                }
                if distance > profile.attack_range * 0.85 {
                    desired_velocity = direction * move_speed;
                } else if distance < 5.0 {
                    desired_velocity = -direction * (move_speed * 0.65);
                }
            }
            SkeletonVariant::Knight | SkeletonVariant::SwordShield | SkeletonVariant::Guard | SkeletonVariant::King => {
                if distance <= profile.attack_range && enemy.attack_timer.just_finished() {
                    if !player_parrying {
                        queued_player_damage += profile.melee_damage;
                    }
                } else if distance <= profile.aggro_range {
                    desired_velocity = direction * move_speed;
                }
            }
        }

        if distance > profile.aggro_range * 1.4 {
            let home_delta = enemy.home - transform.translation;
            if home_delta.length_squared() > 0.12 {
                desired_velocity = home_delta.normalize() * (move_speed * 0.7);
            }
        }

        controller.translation = Some(desired_velocity * delta_secs);
    }

    if queued_player_damage > 0.0 {
        if let Ok(mut player_hp) = queries.p1().single_mut() {
            player_hp.apply(-queued_player_damage);
        }
    }

    for (enemy_e, rotation) in queued_rotations {
        if let Ok((_, transform, _, _, _, _)) = queries.p2().get(enemy_e) {
            let mut next_transform = transform.clone();
            next_transform.rotation = rotation;
            commands.entity(enemy_e).insert(next_transform);
        }
    }
}

fn enemy_projectile_system(
    mut commands: Commands,
    time: Res<Time>,
    rapier_ctx: ReadRapierContext,
    mut queries: ParamSet<(
        Query<(Entity, &Transform), With<Player>>,
        Query<&mut Health, With<Player>>,
        Query<(Entity, &Transform, &mut EnemyProjectile)>,
    )>,
) {
    let Ok(rapier) = rapier_ctx.single() else { return; };
    let (player_e, player_target) = {
        let player_query = queries.p0();
        let Ok((player_e, player_t)) = player_query.single() else { return; };
        (player_e, player_t.translation + Vec3::new(0.0, 0.75, 0.0))
    };
    let dt = time.delta_secs();
    let mut queued_player_damage = 0.0;
    let mut queued_transforms = Vec::new();

    for (entity, transform, mut projectile) in &mut queries.p2() {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }

        let current = transform.translation;
        let next = current + projectile.velocity * dt;
        let segment = next - current;
        let seg_len = segment.length();
        if seg_len > 0.0001 {
            if let Some((hit, _)) = rapier.cast_ray(current, segment / seg_len, seg_len, true, QueryFilter::default()) {
                if hit == player_e {
                    queued_player_damage += projectile.damage;
                }
                if hit != projectile.owner || hit == player_e {
                    commands.entity(entity).despawn();
                    continue;
                }
            }
        }

        if next.distance(player_target) < 0.55 {
            let damage = match projectile.kind {
                EnemyProjectileKind::Arrow => projectile.damage,
                EnemyProjectileKind::Fireball => projectile.damage + 2.0,
                EnemyProjectileKind::Zap => projectile.damage,
                EnemyProjectileKind::WindSlash => projectile.damage,
            };
            queued_player_damage += damage;
            commands.entity(entity).despawn();
            continue;
        }

        let mut next_transform = transform.clone();
        next_transform.translation = next;
        queued_transforms.push((entity, next_transform));
    }

    if queued_player_damage > 0.0 {
        if let Ok(mut player_hp) = queries.p1().single_mut() {
            player_hp.apply(-queued_player_damage);
        }
    }

    for (entity, transform) in queued_transforms {
        commands.entity(entity).insert(transform);
    }
}

fn cleanup_dead_skeletons(
    mut commands: Commands,
    mut enemy_rng: ResMut<EnemyRuntimeRng>,
    gold_assets: Res<GoldPickupAssets>,
    skeletons: Query<(Entity, &Health, &Transform, &SkeletonEnemy), With<SkeletonEnemy>>,
) {
    for (entity, health, transform, skeleton) in &skeletons {
        if health.hp <= 0.0 {
            spawn_gold_drop(
                &mut commands,
                &gold_assets,
                transform.translation + Vec3::new(0.0, 0.08, 0.0),
                gold_drop_amount(skeleton.variant, &mut enemy_rng.0),
                gold_drop_style_for_variant(skeleton.variant),
            );
            commands.entity(entity).despawn();
        }
    }
}

fn gold_drop_amount(variant: SkeletonVariant, rng: &mut StdRng) -> i32 {
    match variant {
        SkeletonVariant::Archer => rng.random_range(8..=14),
        SkeletonVariant::Knight => rng.random_range(11..=18),
        SkeletonVariant::SwordShield => rng.random_range(10..=16),
        SkeletonVariant::Guard => rng.random_range(16..=24),
        SkeletonVariant::Mage => rng.random_range(18..=28),
        SkeletonVariant::King => rng.random_range(80..=120),
    }
}

#[derive(Clone, Copy)]
enum GoldDropStyle {
    Common,
    Martial,
    Arcane,
    Royal,
    ChestCache,
}

fn gold_drop_style_for_variant(variant: SkeletonVariant) -> GoldDropStyle {
    match variant {
        SkeletonVariant::Archer => GoldDropStyle::Common,
        SkeletonVariant::Knight | SkeletonVariant::SwordShield | SkeletonVariant::Guard => GoldDropStyle::Martial,
        SkeletonVariant::Mage => GoldDropStyle::Arcane,
        SkeletonVariant::King => GoldDropStyle::Royal,
    }
}

fn roll_chest_tier(rng: &mut StdRng) -> ChestTier {
    let roll = rng.random_range(0..100);
    match roll {
        0..=51 => ChestTier::Common,
        52..=79 => ChestTier::Rare,
        80..=94 => ChestTier::Epic,
        _ => ChestTier::Royal,
    }
}

fn roll_chest_item(rng: &mut StdRng, tier: ChestTier) -> Item {
    let attempts = match tier {
        ChestTier::Common => 1,
        ChestTier::Rare => 2,
        ChestTier::Epic => 4,
        ChestTier::Royal => 6,
    };
    let mut best = roll_item(rng);
    for _ in 1..attempts {
        let candidate = roll_item(rng);
        if candidate.base_value() > best.base_value() {
            best = candidate;
        }
    }
    best
}

fn rng_f32(rng: &mut StdRng, min: f32, max: f32) -> f32 {
    rng.random_range(min..max)
}

fn rng_bool_weighted(rng: &mut StdRng, chance: f64) -> bool {
    rng.random_bool(chance)
}

fn spawn_gold_drop(
    commands: &mut Commands,
    assets: &GoldPickupAssets,
    position: Vec3,
    amount: i32,
    style: GoldDropStyle,
) {
    let coin_count = ((amount as f32) / 14.0).ceil().clamp(1.0, 6.0) as usize;
    let (base_y, spin_speed, magnet_radius, material) = match style {
        GoldDropStyle::Common => (-0.88, 2.2, 1.65, assets.common_material.clone()),
        GoldDropStyle::Martial => (-0.86, 2.8, 1.75, assets.martial_material.clone()),
        GoldDropStyle::Arcane => (-0.8, 3.6, 1.95, assets.arcane_material.clone()),
        GoldDropStyle::Royal => (-0.74, 4.1, 2.2, assets.royal_material.clone()),
        GoldDropStyle::ChestCache => (-0.9, 2.4, 1.8, assets.common_material.clone()),
    };
    for index in 0..coin_count {
        let spread = index as f32 - (coin_count as f32 - 1.0) * 0.5;
        commands.spawn((
            Mesh3d(assets.coin_mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform {
                translation: position + Vec3::new(spread * 0.16, base_y + index as f32 * 0.02, spread.abs() * 0.04),
                rotation: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                scale: match style {
                    GoldDropStyle::Royal => Vec3::splat(1.18),
                    GoldDropStyle::Arcane => Vec3::new(0.94, 1.1, 0.94),
                    GoldDropStyle::ChestCache => Vec3::splat(1.06),
                    _ => Vec3::ONE,
                },
                ..default()
            },
            GoldPickup {
                amount: if index == 0 { amount } else { 0 },
                bob_phase: index as f32 * 0.85,
                base_y: base_y + index as f32 * 0.02,
                spin_speed,
                magnet_radius,
            },
            WorldEntity,
            Name::new("GoldPickup"),
        ));
    }
}

fn gold_pickup_system(
    time: Res<Time>,
    mut commands: Commands,
    mut wallet: ResMut<PlayerWallet>,
    player: Query<&Transform, (With<Player>, Without<GoldPickup>)>,
    mut pickups: Query<(Entity, &mut Transform, &mut GoldPickup)>,
) {
    let Ok(player_tf) = player.single() else { return; };
    let player_pos = player_tf.translation;
    let dt = time.delta_secs();

    for (entity, mut transform, mut pickup) in &mut pickups {
        pickup.bob_phase += dt * 2.5;
        transform.translation.y = pickup.base_y + pickup.bob_phase.sin() * 0.05;
        transform.rotate_local_y(dt * pickup.spin_speed);

        let to_player = player_pos - transform.translation;
        let dist = to_player.length();
        if dist < pickup.magnet_radius {
            let pull = to_player.normalize_or_zero() * (3.4 + (pickup.magnet_radius - dist).max(0.0) * 3.6) * dt;
            transform.translation += pull;
        }

        if dist < 0.42 {
            wallet.gold += pickup.amount.max(0);
            if pickup.amount > 0 {
                spawn_gain_popup(&mut commands, transform.translation + Vec3::new(0.0, 0.2, 0.0), pickup.amount as f32);
            }
            commands.entity(entity).despawn();
        }
    }
}

fn spawn_gain_popup(
    commands: &mut Commands,
    position: Vec3,
    amount: f32,
) {
    commands.spawn((
        Transform::from_translation(position),
        Visibility::Hidden,
        DamagePopup {
            amount,
            lifetime: Timer::from_seconds(0.75, TimerMode::Once),
            positive: true,
        },
        WorldEntity,
        Name::new("GoldGainPopup"),
    ));
}

pub fn interact_system(
    kb: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut open: ResMut<OpenChest>,
    mut menu_focus: ResMut<MenuFocusState>,
    mut wallet: ResMut<PlayerWallet>,
    player: Query<&Transform, With<Player>>,
    mut chests: Query<(Entity, &Transform, &mut Chest), (With<ChestMarker>, With<Interactable>)>,
) {
    if !kb.just_pressed(KeyCode::KeyF) { return; }

    let Ok(p) = player.single() else { return; };
    let range2 = 2.25f32 * 2.25;

    // Find nearest chest in range
    let mut nearest: Option<(Entity, f32)> = None;
    for (e, t, _) in chests.iter() {
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
        (Some(current), Some(hit)) if current == hit => None,
        (_, Some(hit)) => {
            if let Ok((_, chest_tf, mut chest)) = chests.get_mut(hit) {
                if chest.gold > 0 {
                    wallet.gold += chest.gold;
                    spawn_gain_popup(&mut commands, chest_tf.translation + Vec3::new(0.0, 0.55, 0.0), chest.gold as f32);
                    chest.gold = 0;
                }
            }
            menu_focus.request(MenuFocusTarget::Chest(hit));
            Some(hit)
        }
        _ => None,
    };
}

pub fn fall_off_map_detector(
    mut pending: ResMut<PendingRespawn>,
    q_player: Query<&Transform, With<Player>>,
) {
    // Don’t stack timers.
    if pending.0.is_some() { return; }

    let Ok(t) = q_player.single() else { return; };

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
    if !timer.just_finished() { return; }

    // Teleport player back to the recorded spawn
    if let Ok(mut t) = q_t.single_mut() {
        t.translation = spawn.0;
        t.rotation = Quat::IDENTITY;
    }
    if let Ok(mut ctrl) = q_ctrl.single_mut() {
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

fn spawn_spell_streak(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    thickness: Vec3,
    lifetime: f32,
    drift: Vec3,
    name: &'static str,
) {
    let segment = end - start;
    let length = segment.length();
    if length <= 0.001 {
        return;
    }
    let direction = segment / length;
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform {
            translation: start + segment * 0.5,
            rotation: Quat::from_rotation_arc(Vec3::Z, direction),
            scale: Vec3::new(thickness.x, thickness.y, length.max(0.05)),
        },
        SpellVisualEffect {
            lifetime: Timer::from_seconds(lifetime, TimerMode::Once),
            drift,
        },
        WorldEntity,
        Name::new(name),
    ));
}

fn spawn_zap_visual(
    commands: &mut Commands,
    visuals: &SpellVisualAssets,
    start: Vec3,
    end: Vec3,
) {
    let full = end - start;
    let len = full.length();
    if len <= 0.001 {
        return;
    }
    let dir = full / len;
    let side = if dir.cross(Vec3::Y).length_squared() > 0.001 {
        dir.cross(Vec3::Y).normalize()
    } else {
        Vec3::X
    };
    let mut points = Vec::new();
    let steps = 6;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let mut point = start.lerp(end, t);
        if i != 0 && i != steps - 1 {
            let swing = if i % 2 == 0 { 1.0 } else { -1.0 };
            point += side * (0.22 * swing);
            point += Vec3::Y * (0.08 * -swing);
        }
        points.push(point);
    }
    for segment in points.windows(2) {
        spawn_spell_streak(
            commands,
            visuals.beam_mesh.clone(),
            visuals.zap_material.clone(),
            segment[0],
            segment[1],
            Vec3::new(0.08, 0.08, 1.0),
            0.18,
            Vec3::ZERO,
            "ZapVisual",
        );
    }
}

fn spawn_heal_visuals(
    commands: &mut Commands,
    visuals: &SpellVisualAssets,
    center: Vec3,
) {
    let offsets = [
        Vec3::new(0.0, 0.45, 0.0),
        Vec3::new(0.6, 0.36, 0.25),
        Vec3::new(-0.55, 0.4, -0.3),
    ];
    for offset in offsets {
        let vertical_start = center + offset;
        let vertical_end = vertical_start + Vec3::Y * 0.8;
        spawn_spell_streak(
            commands,
            visuals.cross_mesh.clone(),
            visuals.heal_material.clone(),
            vertical_start,
            vertical_end,
            Vec3::new(0.18, 0.18, 1.0),
            0.85,
            Vec3::Y * 0.12,
            "HealCrossVertical",
        );
        spawn_spell_streak(
            commands,
            visuals.beam_mesh.clone(),
            visuals.heal_material.clone(),
            center + offset + Vec3::Y * 0.4 + Vec3::new(-0.28, 0.0, 0.0),
            center + offset + Vec3::Y * 0.4 + Vec3::new(0.28, 0.0, 0.0),
            Vec3::new(0.14, 0.14, 1.0),
            0.85,
            Vec3::Y * 0.12,
            "HealCrossHorizontal",
        );
    }
}

fn spawn_fireball_spell_projectile(
    commands: &mut Commands,
    visuals: &SpellVisualAssets,
    origin: Vec3,
    velocity: Vec3,
) {
    commands.spawn((
        Mesh3d(visuals.orb_mesh.clone()),
        MeshMaterial3d(visuals.fireball_material.clone()),
        Transform {
            translation: origin,
            scale: Vec3::splat(1.2),
            ..default()
        },
        FireballSpellProjectile {
            velocity,
            lifetime: Timer::from_seconds(3.2, TimerMode::Once),
        },
        WorldEntity,
        Name::new("PlayerFireball"),
    ));
}

fn spawn_fireball_explosion_visuals(
    commands: &mut Commands,
    visuals: &SpellVisualAssets,
    center: Vec3,
) {
    commands.spawn((
        Mesh3d(visuals.orb_mesh.clone()),
        MeshMaterial3d(visuals.fireball_material.clone()),
        Transform {
            translation: center,
            scale: Vec3::splat(2.2),
            ..default()
        },
        SpellVisualEffect {
            lifetime: Timer::from_seconds(0.35, TimerMode::Once),
            drift: Vec3::Y * 0.15,
        },
        WorldEntity,
        Name::new("FireballExplosion"),
    ));
    commands.spawn((
        Mesh3d(visuals.ring_mesh.clone()),
        MeshMaterial3d(visuals.burn_material.clone()),
        Transform {
            translation: center + Vec3::new(0.0, 0.04, 0.0),
            scale: Vec3::new(2.6, 1.0, 2.6),
            ..default()
        },
        SpellVisualEffect {
            lifetime: Timer::from_seconds(0.55, TimerMode::Once),
            drift: Vec3::ZERO,
        },
        WorldEntity,
        Name::new("FireballSplash"),
    ));
}

fn apply_fireball_explosion(
    commands: &mut Commands,
    rapier: &RapierContext,
    visuals: &SpellVisualAssets,
    impact: Vec3,
    player_e: Entity,
    parents: &Query<&ChildOf>,
    damageables: &Query<(), With<Health>>,
    skeletons: &Query<(), With<SkeletonEnemy>>,
    q_health: &mut Query<&mut Health>,
    book: &mut Spellbook,
) {
    spawn_fireball_explosion_visuals(commands, visuals, impact);
    let mut hits = Vec::new();
    let radius = feet(2.4);
    rapier.intersect_shape(
        impact,
        Quat::IDENTITY,
        &bevy_rapier3d::parry::shape::Ball::new(radius),
        QueryFilter::default(),
        |other_e| {
            hits.push(other_e);
            true
        },
    );

    let mut dealt_damage = false;
    for other_e in hits {
        let Some(target_e) = resolve_damageable_entity(other_e, parents, damageables) else { continue; };
        if target_e == player_e {
            continue;
        }
        if let Ok(mut hp) = q_health.get_mut(target_e) {
            hp.apply(-40.0);
            if skeletons.contains(target_e) {
                commands.entity(target_e).insert(Burn {
                    dps: 2.0,
                    timer: Timer::from_seconds(5.0, TimerMode::Once),
                });
            }
            dealt_damage = true;
        }
    }
    if dealt_damage {
        reset_spell_timers(book);
    }
}

pub fn burn_tick_system(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Health, &mut Burn)>,
    _book: ResMut<Spellbook>, // reserved for damage-triggered recharge resets
    player_q: Query<Entity, With<Player>>,
) {
    let _player_e = player_q.single().ok();

    for (entity, mut hp, mut burn) in q.iter_mut() {
        burn.timer.tick(time.delta());
        if burn.timer.just_finished() {
            commands.entity(entity).remove::<Burn>();
            continue;
        }
        let dps = burn.dps;
        hp.apply(-dps * time.delta_secs());

        if let Ok(_pe) = player_q.single() {
        }
    }
}
fn spell_cast_system(
    _time: Res<Time>,
    kb: Res<ButtonInput<MouseButton>>,
    flags: Res<UIFlags>,
    active: Res<ActiveSpell>,
    mut book: ResMut<Spellbook>,
    rapier_ctx: ReadRapierContext,
    cam_q: Query<&GlobalTransform, With<Camera3d>>,
    player_q: Query<(Entity, &GlobalTransform), With<Player>>,
    parents: Query<&ChildOf>,
    damageables: Query<(), With<Health>>,
    skeletons: Query<(Entity, &Transform, Option<&Wet>), With<SkeletonEnemy>>,
    mut q_health: Query<&mut Health>,
    visuals: Res<SpellVisualAssets>,
    mut commands: Commands,
) {
    if flags.spell_menu_open || flags.inventory_open || flags.pause_menu_open { return; }
    if !kb.just_pressed(MouseButton::Left) { return; }

    let Some(spell) = active.selected else { return; };
    let Ok(rapier) = rapier_ctx.single() else { return; };
    let cam_tf: &GlobalTransform = if let Ok(t) = cam_q.single() { t } else { return; };
    let Ok((player_e, player_tf)) = player_q.single() else { return; };
    let forward: Vec3 = cam_tf.forward().into();
    let origin = cam_tf.translation();
    let query_filter = player_combat_filter(player_e);

    let mut try_consume = |s: Spell| -> bool {
        let entry = book.charges.entry(s).or_insert(0);
        if *entry > 0 { *entry -= 1; true } else { false }
    };
    let hitscan = |ctx: &RapierContext, from: Vec3, dir: Vec3, max_dist: f32, filter: QueryFilter| -> Option<(Entity, Vec3)> {
        ctx.cast_ray(from, dir.normalize(), max_dist, true, filter)
            .map(|(e, toi)| (e, from + dir.normalize() * toi))
    };

    match spell {
        Spell::Fireball => {
            if !try_consume(Spell::Fireball) { return; }
            let fireball_origin = origin + forward * 0.85;
            let velocity = forward.normalize_or_zero() * 8.4 + Vec3::new(0.0, 1.4, 0.0);
            spawn_fireball_spell_projectile(&mut commands, &visuals, fireball_origin, velocity);
        }
        Spell::Zap => {
            if !try_consume(Spell::Zap) { return; }
            let end = hitscan(&rapier, origin, forward, feet(60.0), query_filter)
                .map(|(_, pt)| pt)
                .unwrap_or(origin + forward.normalize_or_zero() * feet(60.0));
            spawn_zap_visual(&mut commands, &visuals, origin, end);
            if let Some((hit_e, hit_pt)) = hitscan(&rapier, origin, forward, feet(60.0), query_filter) {
                let resolved = resolve_damageable_entity(hit_e, &parents, &damageables);
                if let Some(target_e) = resolved {
                    let mut damage = 30.0;
                    let mut chained = false;
                    if let Ok((_entity, target_tf, wet)) = skeletons.get(target_e) {
                        if wet.is_some() {
                            damage = 42.0;
                            if let Some((chain_e, chain_tf, _)) = skeletons
                                .iter()
                                .filter(|(entity, _, _)| *entity != target_e)
                                .find(|(_, transform, _)| transform.translation.distance(target_tf.translation) <= feet(14.0))
                            {
                                if apply_damage_to_hit_entity(chain_e, 18.0, &parents, &damageables, &mut q_health).is_some() {
                                    spawn_zap_visual(&mut commands, &visuals, hit_pt, skeleton_focus_point(chain_tf));
                                    commands.entity(chain_e).insert(enemy_hit_recoil(forward, None, WeaponSlot::Primary, 18.0));
                                    chained = true;
                                }
                            }
                        }
                    }
                    if apply_damage_to_hit_entity(target_e, damage, &parents, &damageables, &mut q_health).is_some() {
                        if chained || damage > 30.0 {
                            commands.entity(target_e).insert(enemy_hit_recoil(forward, None, WeaponSlot::Primary, damage));
                        }
                    }
                    reset_spell_timers(&mut book);
                }
            }
        }
        Spell::WindSlash => {
            if !try_consume(Spell::WindSlash) { return; }
            let end = hitscan(&rapier, origin, forward, feet(24.0), query_filter)
                .map(|(_, pt)| pt)
                .unwrap_or(origin + forward.normalize_or_zero() * feet(24.0));
            spawn_spell_streak(
                &mut commands,
                visuals.slash_mesh.clone(),
                visuals.wind_material.clone(),
                origin,
                end,
                Vec3::new(0.22, 0.48, 1.0),
                0.22,
                Vec3::ZERO,
                "WindSlashVisual",
            );
            if let Some((hit_e, _pt)) = hitscan(&rapier, origin, forward, feet(24.0), query_filter) {
                if let Some(target_e) = apply_damage_to_hit_entity(hit_e, 35.0, &parents, &damageables, &mut q_health) {
                    commands.entity(target_e).insert(enemy_hit_recoil(forward, None, WeaponSlot::Primary, 24.0));
                    reset_spell_timers(&mut book);
                }
            }
        }
        Spell::LightHeal => {
            if !try_consume(Spell::LightHeal) { return; }
            let center = player_tf.translation();
            let radius = feet(3.0);
            let mut heal_targets = Vec::new();
            rapier.intersect_shape(
                center,
                Quat::IDENTITY,
                &bevy_rapier3d::parry::shape::Ball::new(radius),
                query_filter,
                |other_e| {
                    heal_targets.push(other_e);
                    true
                }
            );
            for target in heal_targets {
                if let Ok(mut hp) = q_health.get_mut(target) {
                    hp.apply(25.0);
                }
            }
            spawn_heal_visuals(&mut commands, &visuals, center);
        }
        Spell::WaterGun => {
            if !try_consume(Spell::WaterGun) { return; }
            commands.entity(player_e).insert(WaterChannel { remaining: 10.0 });
        }

    }
}

fn fireball_projectile_system(
    time: Res<Time>,
    rapier_ctx: ReadRapierContext,
    visuals: Res<SpellVisualAssets>,
    mut book: ResMut<Spellbook>,
    player_q: Query<Entity, With<Player>>,
    parents: Query<&ChildOf>,
    damageables: Query<(), With<Health>>,
    skeletons: Query<(), With<SkeletonEnemy>>,
    mut q_health: Query<&mut Health>,
    mut commands: Commands,
    mut projectiles: Query<(Entity, &Transform, &mut FireballSpellProjectile)>,
) {
    let Ok(rapier) = rapier_ctx.single() else { return; };
    let Ok(player_e) = player_q.single() else { return; };
    let dt = time.delta_secs();
    let mut queued_transforms = Vec::new();
    let mut explosions = Vec::new();

    for (entity, transform, mut projectile) in &mut projectiles {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            explosions.push(transform.translation);
            commands.entity(entity).despawn();
            continue;
        }

        projectile.velocity.y -= 7.2 * dt;
        let current = transform.translation;
        let next = current + projectile.velocity * dt;
        let segment = next - current;
        let seg_len = segment.length();

        if seg_len > 0.0001 {
            if let Some((_hit, toi)) = rapier.cast_ray(current, segment / seg_len, seg_len, true, QueryFilter::default()) {
                explosions.push(current + segment.normalize() * toi);
                commands.entity(entity).despawn();
                continue;
            }
        }

        if next.y <= -1.0 {
            explosions.push(next);
            commands.entity(entity).despawn();
            continue;
        }

        let mut next_transform = transform.clone();
        next_transform.translation = next;
        queued_transforms.push((entity, next_transform));
    }

    for (entity, transform) in queued_transforms {
        commands.entity(entity).insert(transform);
    }

    for impact in explosions {
        apply_fireball_explosion(&mut commands, &rapier, &visuals, impact, player_e, &parents, &damageables, &skeletons, &mut q_health, &mut book);
    }
}

fn spell_channel_tick_system(
    time: Res<Time>,
    rapier_ctx: ReadRapierContext,
    cam_q: Query<&GlobalTransform, With<Camera3d>>,
    mut q_player: Query<(Entity, &mut WaterChannel), With<Player>>,
    parents: Query<&ChildOf>,
    damageables: Query<(), With<Health>>,
    skeletons: Query<(), With<SkeletonEnemy>>,
    mut q_health: Query<&mut Health>,
    mut book: ResMut<Spellbook>,
    mut commands: Commands,
) {
    let Ok(rapier) = rapier_ctx.single() else { return; };
    let Ok((pe, mut chan)) = q_player.single_mut() else { return; };
    let Ok(cam_tf) = cam_q.single() else { return; };

    let dt = time.delta_secs();
    chan.remaining -= dt;
    if chan.remaining <= 0.0 {
        commands.entity(pe).remove::<WaterChannel>();
        return;
    }

    if let Some((hit_e, _)) = {
        let origin = cam_tf.translation();
        let forward: Vec3 = cam_tf.forward().into();
        rapier.cast_ray(origin, forward.normalize(), feet(6.0), true, player_combat_filter(pe))
            .and_then(|(e, toi)| Some((e, origin + forward.normalize() * toi)))
    } {
        let forward: Vec3 = cam_tf.forward().into();
        if let Some(target_e) = apply_damage_to_hit_entity(hit_e, 10.0 * dt, &parents, &damageables, &mut q_health) {
            if skeletons.contains(target_e) {
                commands.entity(target_e).insert(Wet {
                    timer: Timer::from_seconds(2.4, TimerMode::Once),
                });
                commands.entity(target_e).insert(enemy_hit_recoil(forward, None, WeaponSlot::Primary, 8.0 * dt));
            }
            reset_spell_timers(&mut book);
        }
    }
}

fn spell_visual_decay_system(
    time: Res<Time>,
    mut commands: Commands,
    mut visuals: Query<(Entity, &mut SpellVisualEffect, &mut Transform)>,
) {
    for (entity, mut effect, mut transform) in &mut visuals {
        effect.lifetime.tick(time.delta());
        if effect.lifetime.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += effect.drift * time.delta_secs();
    }
}

fn sync_burn_visuals(
    mut commands: Commands,
    added_burns: Query<Entity, Added<Burn>>,
    existing: Query<&BurnVisual>,
    visuals: Res<SpellVisualAssets>,
) {
    let existing_owners: HashSet<Entity> = existing.iter().map(|visual| visual.owner).collect();
    for owner in &added_burns {
        if existing_owners.contains(&owner) {
            continue;
        }
        let phase = (owner.to_bits() % 17) as f32 * 0.37;
        commands.spawn((
            Mesh3d(visuals.ring_mesh.clone()),
            MeshMaterial3d(visuals.burn_material.clone()),
            Transform::default(),
            BurnVisual { owner, phase },
            WorldEntity,
            Name::new("BurnVisual"),
        ));
    }
}

fn update_burn_visuals(
    time: Res<Time>,
    mut commands: Commands,
    owners: Query<&GlobalTransform, Without<BurnVisual>>,
    burns: Query<&Burn>,
    mut visuals: Query<(Entity, &BurnVisual, &mut Transform), With<BurnVisual>>,
) {
    let elapsed = time.elapsed_secs();
    for (entity, visual, mut transform) in &mut visuals {
        if burns.get(visual.owner).is_err() {
            commands.entity(entity).despawn();
            continue;
        }
        let Ok(owner_t) = owners.get(visual.owner) else {
            commands.entity(entity).despawn();
            continue;
        };
        let orbit = elapsed * 2.6 + visual.phase;
        transform.translation = owner_t.translation()
            + Vec3::new(orbit.cos() * 0.22, 0.26 + (elapsed * 5.0).sin() * 0.04, orbit.sin() * 0.22);
        transform.rotation = Quat::from_rotation_y(orbit) * Quat::from_rotation_x(0.42);
        transform.scale = Vec3::splat(0.85 + (elapsed * 7.0 + visual.phase).sin().abs() * 0.22);
    }
}

fn sync_water_stream_visuals(
    rapier_ctx: ReadRapierContext,
    cam_q: Query<&GlobalTransform, With<Camera3d>>,
    player_q: Query<Entity, (With<Player>, With<WaterChannel>)>,
    visuals: Res<SpellVisualAssets>,
    stream_q: Query<(Entity, &Transform), With<WaterStreamVisual>>,
    mut commands: Commands,
) {
    let has_channel = player_q.single().ok();
    if has_channel.is_none() {
        for (entity, _) in &stream_q {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Ok(rapier) = rapier_ctx.single() else { return; };
    let Ok(cam_tf) = cam_q.single() else { return; };
    let _player_e = has_channel.unwrap();
    let origin = cam_tf.translation() + cam_tf.forward() * 0.3;
    let forward: Vec3 = cam_tf.forward().into();
    let range = feet(8.0);
    let mut end = origin + forward.normalize_or_zero() * range;
    if let Some((_hit, toi)) = rapier.cast_ray(origin, forward.normalize_or_zero(), range, true, QueryFilter::default()) {
        end = origin + forward.normalize_or_zero() * toi;
    }
    end += Vec3::new(0.0, -0.38, 0.0);
    let segment = end - origin;
    let length = segment.length().max(0.05);
    let next_transform = Transform {
        translation: origin + segment * 0.5,
        rotation: Quat::from_rotation_arc(Vec3::Z, segment / length),
        scale: Vec3::new(0.14, 0.14, length),
    };

    if let Ok((entity, _)) = stream_q.single() {
        commands.entity(entity).insert(next_transform);
    } else {
        commands.spawn((
            Mesh3d(visuals.beam_mesh.clone()),
            MeshMaterial3d(visuals.water_material.clone()),
            next_transform,
            WaterStreamVisual,
            WorldEntity,
            Name::new("WaterStreamVisual"),
        ));
    }
}

fn tick_hit_stop(
    time: Res<Time>,
    mut hit_stop: ResMut<HitStopState>,
) {
    if hit_stop.remaining <= 0.0 {
        hit_stop.remaining = 0.0;
        return;
    }

    hit_stop.remaining = (hit_stop.remaining - time.delta_secs()).max(0.0);
    if hit_stop.remaining <= 0.0 {
        hit_stop.move_scale = 1.0;
    }
}

fn tick_combat_combo(
    time: Res<Time>,
    mut combo: ResMut<CombatComboState>,
) {
    if combo.remaining <= 0.0 {
        combo.break_chain();
        return;
    }

    combo.remaining = (combo.remaining - time.delta_secs()).max(0.0);
    if combo.remaining <= 0.0 {
        combo.break_chain();
    }
}

fn tick_wet_status(
    time: Res<Time>,
    mut commands: Commands,
    mut wet_targets: Query<(Entity, &mut Wet)>,
) {
    for (entity, mut wet) in &mut wet_targets {
        wet.timer.tick(time.delta());
        if wet.timer.is_finished() {
            commands.entity(entity).remove::<Wet>();
        }
    }
}

fn apply_enemy_hit_recoil(
    time: Res<Time>,
    mut commands: Commands,
    mut enemies: Query<(Entity, &mut KinematicCharacterController, &mut EnemyHitRecoil, &mut Transform), With<SkeletonEnemy>>,
) {
    for (entity, mut controller, mut recoil, mut transform) in &mut enemies {
        let dt = time.delta_secs();
        let base_translation = controller.translation.unwrap_or(Vec3::ZERO);
        controller.translation = Some(base_translation + recoil.velocity * dt);
        let tilt_axis = recoil.tilt_axis.normalize_or_zero();
        if tilt_axis != Vec3::ZERO && recoil.tilt.abs() > 0.0001 {
            transform.rotation *= Quat::from_axis_angle(tilt_axis, recoil.tilt);
        }
        recoil.velocity *= (1.0 - 7.5 * dt).clamp(0.0, 1.0);
        recoil.tilt *= (1.0 - 9.5 * dt).clamp(0.0, 1.0);
        recoil.remaining -= dt;
        if recoil.remaining <= 0.0 || (recoil.velocity.length_squared() < 0.0004 && recoil.tilt.abs() < 0.0008) {
            commands.entity(entity).remove::<EnemyHitRecoil>();
        }
    }
}

fn update_damage_popups(
    time: Res<Time>,
    mut commands: Commands,
    mut popups: Query<(Entity, &mut DamagePopup, &mut Transform)>,
) {
    for (entity, mut popup, mut transform) in &mut popups {
        popup.lifetime.tick(time.delta());
        if popup.lifetime.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.y += 0.8 * time.delta_secs();
    }
}

fn spawn_damage_feedback(
    commands: &mut Commands,
    visuals: &SpellVisualAssets,
    position: Vec3,
    damage: f32,
) {
    commands.spawn((
        Transform::from_translation(position + Vec3::new(0.0, 0.28, 0.0)),
        Visibility::Hidden,
        DamagePopup {
            amount: damage,
            lifetime: Timer::from_seconds(0.65, TimerMode::Once),
            positive: false,
        },
        WorldEntity,
        Name::new("DamagePopup"),
    ));

    commands.spawn((
        Mesh3d(visuals.slash_mesh.clone()),
        MeshMaterial3d(visuals.zap_material.clone()),
        Transform {
            translation: position,
            rotation: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            scale: Vec3::new(0.14, 0.14, 0.32),
        },
        SpellVisualEffect {
            lifetime: Timer::from_seconds(0.16, TimerMode::Once),
            drift: Vec3::new(0.0, 0.35, 0.0),
        },
        WorldEntity,
        Name::new("HitSpark"),
    ));
}

#[derive(Clone, Copy)]
struct AttackFeedbackProfile {
    trail_scale: Vec3,
    trail_forward: f32,
    trail_height: f32,
    trail_lateral: f32,
    trail_pitch: f32,
    trail_lifetime: f32,
    trail_drift_forward: f32,
    trail_rise: f32,
    camera_translation: Vec3,
    camera_pitch: f32,
    camera_roll: f32,
    recoil_force: f32,
    recoil_lift: f32,
    recoil_tilt: f32,
}

fn attack_feedback_profile(item: Option<&Item>, slot: WeaponSlot) -> AttackFeedbackProfile {
    let side = match slot {
        WeaponSlot::Primary => 1.0,
        WeaponSlot::Secondary => -1.0,
    };

    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) | Some(WeaponKind::ShortSword) => AttackFeedbackProfile {
            trail_scale: Vec3::new(0.12, 0.16, 1.05),
            trail_forward: 0.72,
            trail_height: 0.18,
            trail_lateral: 0.08,
            trail_pitch: -0.08,
            trail_lifetime: 0.12,
            trail_drift_forward: 0.24,
            trail_rise: 0.16,
            camera_translation: Vec3::new(0.0, -0.008, 0.028),
            camera_pitch: -0.01,
            camera_roll: -0.01 * side,
            recoil_force: 0.82,
            recoil_lift: 0.04,
            recoil_tilt: 0.08,
        },
        Some(WeaponKind::Hatchet) => AttackFeedbackProfile {
            trail_scale: Vec3::new(0.16, 0.18, 1.18),
            trail_forward: 0.76,
            trail_height: 0.2,
            trail_lateral: 0.1,
            trail_pitch: 0.04,
            trail_lifetime: 0.13,
            trail_drift_forward: 0.3,
            trail_rise: 0.14,
            camera_translation: Vec3::new(0.0, -0.012, 0.04),
            camera_pitch: -0.015,
            camera_roll: -0.014 * side,
            recoil_force: 0.98,
            recoil_lift: 0.07,
            recoil_tilt: 0.11,
        },
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => AttackFeedbackProfile {
            trail_scale: Vec3::new(0.24, 0.28, 1.8),
            trail_forward: 0.9,
            trail_height: 0.28,
            trail_lateral: 0.18,
            trail_pitch: 0.18,
            trail_lifetime: 0.18,
            trail_drift_forward: 0.56,
            trail_rise: 0.08,
            camera_translation: Vec3::new(0.0, -0.028, 0.13),
            camera_pitch: -0.045,
            camera_roll: -0.03 * side,
            recoil_force: 1.58,
            recoil_lift: 0.16,
            recoil_tilt: 0.19,
        },
        Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => AttackFeedbackProfile {
            trail_scale: Vec3::new(0.18, 0.24, 1.55),
            trail_forward: 0.86,
            trail_height: 0.24,
            trail_lateral: 0.14,
            trail_pitch: 0.06,
            trail_lifetime: 0.16,
            trail_drift_forward: 0.42,
            trail_rise: 0.1,
            camera_translation: Vec3::new(0.0, -0.018, 0.08),
            camera_pitch: -0.028,
            camera_roll: -0.018 * side,
            recoil_force: 1.24,
            recoil_lift: 0.11,
            recoil_tilt: 0.15,
        },
        Some(WeaponKind::Scythe) => AttackFeedbackProfile {
            trail_scale: Vec3::new(0.2, 0.28, 1.72),
            trail_forward: 0.92,
            trail_height: 0.26,
            trail_lateral: 0.2,
            trail_pitch: 0.22,
            trail_lifetime: 0.17,
            trail_drift_forward: 0.48,
            trail_rise: 0.12,
            camera_translation: Vec3::new(0.0, -0.016, 0.074),
            camera_pitch: -0.025,
            camera_roll: -0.026 * side,
            recoil_force: 1.18,
            recoil_lift: 0.12,
            recoil_tilt: 0.2,
        },
        Some(WeaponKind::MagicStaff) | Some(WeaponKind::Book) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Lantern) => AttackFeedbackProfile {
            trail_scale: Vec3::new(0.14, 0.22, 1.2),
            trail_forward: 0.7,
            trail_height: 0.2,
            trail_lateral: 0.09,
            trail_pitch: -0.12,
            trail_lifetime: 0.14,
            trail_drift_forward: 0.28,
            trail_rise: 0.2,
            camera_translation: Vec3::new(0.0, -0.01, 0.035),
            camera_pitch: -0.012,
            camera_roll: -0.012 * side,
            recoil_force: 0.92,
            recoil_lift: 0.08,
            recoil_tilt: 0.09,
        },
        None => AttackFeedbackProfile {
            trail_scale: Vec3::new(0.1, 0.14, 0.92),
            trail_forward: 0.66,
            trail_height: 0.16,
            trail_lateral: 0.06,
            trail_pitch: -0.14,
            trail_lifetime: 0.1,
            trail_drift_forward: 0.18,
            trail_rise: 0.12,
            camera_translation: Vec3::new(0.0, -0.006, 0.018),
            camera_pitch: -0.008,
            camera_roll: -0.008 * side,
            recoil_force: 0.76,
            recoil_lift: 0.03,
            recoil_tilt: 0.06,
        },
    }
}

fn attack_trail_material(visuals: &SpellVisualAssets, weapon: Option<WeaponKind>) -> Handle<StandardMaterial> {
    match weapon {
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) | Some(WeaponKind::Hatchet) => visuals.fireball_material.clone(),
        Some(WeaponKind::MagicStaff) | Some(WeaponKind::Book) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Lantern) => visuals.zap_material.clone(),
        _ => visuals.wind_material.clone(),
    }
}

fn spawn_attack_trail(
    commands: &mut Commands,
    visuals: &SpellVisualAssets,
    position: Vec3,
    direction: Vec3,
    item: Option<&Item>,
    slot: WeaponSlot,
) {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }

    let feedback = attack_feedback_profile(item, slot);
    let side = match slot {
        WeaponSlot::Primary => 1.0,
        WeaponSlot::Secondary => -1.0,
    };
    let lateral = dir.cross(Vec3::Y).normalize_or_zero() * feedback.trail_lateral * side;
    let mut transform = Transform::from_translation(
        position + dir * feedback.trail_forward + lateral + Vec3::Y * feedback.trail_height,
    );
    transform.look_to(dir, Vec3::Y);
    transform.rotate_local_x(std::f32::consts::FRAC_PI_2 + feedback.trail_pitch);
    transform.scale = feedback.trail_scale;

    commands.spawn((
        Mesh3d(visuals.slash_mesh.clone()),
        MeshMaterial3d(attack_trail_material(visuals, item.and_then(|entry| entry.weapon))),
        transform,
        SpellVisualEffect {
            lifetime: Timer::from_seconds(feedback.trail_lifetime, TimerMode::Once),
            drift: dir * feedback.trail_drift_forward + Vec3::new(0.0, feedback.trail_rise, 0.0),
        },
        WorldEntity,
        Name::new("AttackTrail"),
    ));
}

fn enemy_hit_recoil(direction: Vec3, item: Option<&Item>, slot: WeaponSlot, damage: f32) -> EnemyHitRecoil {
    let dir = direction.normalize_or_zero();
    let feedback = attack_feedback_profile(item, slot);
    let side = match slot {
        WeaponSlot::Primary => 1.0,
        WeaponSlot::Secondary => -1.0,
    };
    let lateral = dir.cross(Vec3::Y).normalize_or_zero() * (0.12 * feedback.recoil_force * side);
    EnemyHitRecoil {
        velocity: dir * (feedback.recoil_force + damage * 0.012) + lateral + Vec3::Y * feedback.recoil_lift,
        remaining: 0.12 + feedback.recoil_force * 0.03 + damage.min(36.0) * 0.0025,
        tilt_axis: (dir.cross(Vec3::Y) + Vec3::new(0.0, 0.0, 0.001)).normalize_or_zero(),
        tilt: feedback.recoil_tilt + damage * 0.0012,
    }
}

fn weapon_input_system(
    kb: Res<ButtonInput<KeyCode>>,
    mb: Res<ButtonInput<MouseButton>>,
    mut active: ResMut<ActiveWeapon>,
    mut animation: ResMut<ViewModelAnimation>,
    mut hit_stop: ResMut<HitStopState>,
    mut camera_impulse: ResMut<CameraImpulseState>,
    mut combo: ResMut<CombatComboState>,
    equipment: Res<Equipment>,
    player_stats: Res<PlayerStats>,
    player_q: Query<(Entity, &GlobalTransform), With<Player>>,
    rapier_ctx: ReadRapierContext,
    visuals: Res<SpellVisualAssets>,
    mut combat_queries: (
        Query<&ChildOf>,
        Query<(), With<Health>>,
        Query<(), With<SkeletonEnemy>>,
        Query<(Entity, &Transform, Option<&Wet>), With<SkeletonEnemy>>,
        Query<&mut Health>,
    ),
    // If you want swing cooldowns/parry times, store them in a resource or component
    mut commands: Commands,
) {
    // 1 toggles primary, 2 toggles secondary/offhand.
    if kb.just_pressed(KeyCode::Digit1) {
        if active.slot == WeaponSlot::Primary && active.drawn {
            active.drawn = false;
        } else {
            active.slot = WeaponSlot::Primary;
            active.drawn = true;
        }
    }
    if kb.just_pressed(KeyCode::Digit2) {
        if active.slot == WeaponSlot::Secondary && active.drawn {
            active.drawn = false;
        } else {
            active.slot = WeaponSlot::Secondary;
            active.drawn = true;
        }
    }

    let Ok(rapier) = rapier_ctx.single() else { return; };
    let Ok((player_e, tf)) = player_q.single() else { return; };
    if !active.drawn {
        return;
    }

    let equipped_item = match active.slot {
        WeaponSlot::Primary => equipped_primary_item(&equipment),
        WeaponSlot::Secondary => equipped_secondary_item(&equipment),
    };
    let mut profile = attack_profile(equipped_item, active.slot, &player_stats);
    profile.damage *= combo.damage_multiplier();

    // Very simple melee: short ray up to ~1.5 m
    if mb.just_pressed(MouseButton::Left) && !animation.active {
        animation.swing = Timer::from_seconds(profile.swing_seconds, TimerMode::Once);
        animation.swing.reset();
        animation.active = true;
        let dir: Vec3 = tf.forward().into();
        let origin = tf.translation() + Vec3::new(0.0, 0.14, 0.0) + dir * 0.55;
        let feedback = attack_feedback_profile(equipped_item, active.slot);
        camera_impulse.trigger(feedback.camera_translation, feedback.camera_pitch, feedback.camera_roll);
        spawn_attack_trail(&mut commands, &visuals, origin, dir, equipped_item, active.slot);
        if let Some((hit_e, hit_toi)) = rapier.cast_ray(origin, dir, profile.reach, true, player_combat_filter(player_e)) {
            let resolved_target = {
                resolve_damageable_entity(hit_e, &combat_queries.0, &combat_queries.1)
            };
            if let Some(target_e) = resolved_target {
                if let Ok(mut hp) = combat_queries.4.get_mut(target_e) {
                    hp.apply(-profile.damage);
                } else {
                    combo.break_chain();
                    return;
                }
                let hit_pos = origin + dir * hit_toi;
                spawn_damage_feedback(&mut commands, &visuals, hit_pos, profile.damage);
                let hit_stop_duration = (0.018 + profile.damage * 0.0012).clamp(0.018, 0.055);
                hit_stop.trigger(hit_stop_duration, 0.08);
                combo.register_hit();
                if combat_queries.2.contains(target_e) {
                    commands.entity(target_e).insert(enemy_hit_recoil(dir, equipped_item, active.slot, profile.damage));
                    if is_heavy_cleave_weapon(equipped_item) {
                        let cleave_targets: Vec<(Entity, Vec3, f32)> = combat_queries
                            .3
                            .iter()
                            .filter_map(|(other_e, other_tf, wet)| {
                                if other_e == target_e {
                                    return None;
                                }
                                let offset = other_tf.translation - hit_pos;
                                if offset.length() > 1.55 || offset.normalize_or_zero().dot(dir) < -0.35 {
                                    return None;
                                }
                                let splash_damage = profile.damage * if wet.is_some() { 0.62 } else { 0.5 };
                                Some((other_e, skeleton_focus_point(other_tf), splash_damage))
                            })
                            .collect();
                        for (other_e, popup_pos, splash_damage) in cleave_targets {
                            if let Ok(mut hp) = combat_queries.4.get_mut(other_e) {
                                hp.apply(-splash_damage);
                                spawn_damage_feedback(&mut commands, &visuals, popup_pos, splash_damage);
                                commands.entity(other_e).insert(enemy_hit_recoil(dir, equipped_item, active.slot, splash_damage));
                            }
                        }
                    }
                }
            } else {
                combo.break_chain();
            }
        } else {
            combo.break_chain();
        }
    }
    // Right click: parry for 0.5s
    if mb.just_pressed(MouseButton::Right) {
        commands.entity(player_e).insert(Parry { timer: Timer::from_seconds(0.5, TimerMode::Once) });
    }
}

#[derive(Clone, Copy)]
struct MeshBoxPart {
    size: Vec3,
    translation: Vec3,
    rotation: Quat,
}

fn mesh_box(size: Vec3, translation: Vec3, rotation: Quat) -> MeshBoxPart {
    MeshBoxPart { size, translation, rotation }
}

fn create_hand_mesh(right: bool) -> Mesh {
    let thumb_x = if right { 0.078 } else { -0.078 };
    let thumb_angle = if right { -0.64 } else { 0.64 };
    composite_box_mesh(&[
        mesh_box(Vec3::new(0.17, 0.11, 0.095), Vec3::new(0.0, -0.01, 0.0), Quat::IDENTITY),
        mesh_box(Vec3::new(0.11, 0.05, 0.08), Vec3::new(0.0, 0.035, -0.005), Quat::IDENTITY),
        mesh_box(Vec3::new(0.024, 0.11, 0.028), Vec3::new(-0.048, -0.12, 0.02), Quat::from_rotation_x(0.08)),
        mesh_box(Vec3::new(0.026, 0.125, 0.03), Vec3::new(-0.016, -0.13, 0.01), Quat::from_rotation_x(0.04)),
        mesh_box(Vec3::new(0.026, 0.13, 0.03), Vec3::new(0.016, -0.135, 0.0), Quat::IDENTITY),
        mesh_box(Vec3::new(0.024, 0.118, 0.028), Vec3::new(0.046, -0.122, -0.006), Quat::from_rotation_x(-0.05)),
        mesh_box(Vec3::new(0.032, 0.082, 0.028), Vec3::new(thumb_x, -0.02, 0.0), Quat::from_rotation_z(thumb_angle)),
        mesh_box(Vec3::new(0.12, 0.03, 0.02), Vec3::new(0.0, -0.06, 0.045), Quat::IDENTITY),
    ])
}

fn create_boot_mesh() -> Mesh {
    composite_box_mesh(&[
        mesh_box(Vec3::new(0.22, 0.16, 0.36), Vec3::new(0.0, -0.01, 0.02), Quat::IDENTITY),
        mesh_box(Vec3::new(0.18, 0.16, 0.16), Vec3::new(0.0, 0.1, -0.06), Quat::IDENTITY),
        mesh_box(Vec3::new(0.2, 0.04, 0.34), Vec3::new(0.0, -0.09, 0.03), Quat::IDENTITY),
        mesh_box(Vec3::new(0.18, 0.08, 0.08), Vec3::new(0.0, 0.03, 0.14), Quat::IDENTITY),
    ])
}

fn composite_box_mesh(parts: &[MeshBoxPart]) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for part in parts {
        append_box_geometry(part, &mut positions, &mut normals, &mut uvs, &mut indices);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn append_box_geometry(
    part: &MeshBoxPart,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let half = part.size * 0.5;
    let faces = [
        (
            Vec3::Z,
            [
                Vec3::new(-half.x, -half.y, half.z),
                Vec3::new(half.x, -half.y, half.z),
                Vec3::new(half.x, half.y, half.z),
                Vec3::new(-half.x, half.y, half.z),
            ],
        ),
        (
            -Vec3::Z,
            [
                Vec3::new(half.x, -half.y, -half.z),
                Vec3::new(-half.x, -half.y, -half.z),
                Vec3::new(-half.x, half.y, -half.z),
                Vec3::new(half.x, half.y, -half.z),
            ],
        ),
        (
            Vec3::X,
            [
                Vec3::new(half.x, -half.y, half.z),
                Vec3::new(half.x, -half.y, -half.z),
                Vec3::new(half.x, half.y, -half.z),
                Vec3::new(half.x, half.y, half.z),
            ],
        ),
        (
            -Vec3::X,
            [
                Vec3::new(-half.x, -half.y, -half.z),
                Vec3::new(-half.x, -half.y, half.z),
                Vec3::new(-half.x, half.y, half.z),
                Vec3::new(-half.x, half.y, -half.z),
            ],
        ),
        (
            Vec3::Y,
            [
                Vec3::new(-half.x, half.y, half.z),
                Vec3::new(half.x, half.y, half.z),
                Vec3::new(half.x, half.y, -half.z),
                Vec3::new(-half.x, half.y, -half.z),
            ],
        ),
        (
            -Vec3::Y,
            [
                Vec3::new(-half.x, -half.y, -half.z),
                Vec3::new(half.x, -half.y, -half.z),
                Vec3::new(half.x, -half.y, half.z),
                Vec3::new(-half.x, -half.y, half.z),
            ],
        ),
    ];

    for (normal, verts) in faces {
        let start = positions.len() as u32;
        for (vertex, uv) in verts.into_iter().zip([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]) {
            let transformed = part.rotation.mul_vec3(vertex) + part.translation;
            let transformed_normal = part.rotation.mul_vec3(normal);
            positions.push(transformed.to_array());
            normals.push(transformed_normal.to_array());
            uvs.push(uv);
        }
        indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
    }
}

fn secondary_weapon_view_transform(item: &Item) -> Transform {
    let (translation, rotation, scale) = match item.weapon {
        Some(WeaponKind::Dagger) | Some(WeaponKind::ShortSword) => (
            Vec3::new(-0.24, -0.21, -0.48),
            Quat::from_euler(EulerRot::XYZ, -0.28, -0.18, 0.55),
            Vec3::new(0.58, 0.75, 0.58),
        ),
        Some(WeaponKind::Hatchet) => (
            Vec3::new(-0.26, -0.18, -0.54),
            Quat::from_euler(EulerRot::XYZ, -0.42, -0.25, 0.7),
            Vec3::new(0.82, 0.82, 0.7),
        ),
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => (
            Vec3::new(-0.2, -0.2, -0.43),
            Quat::from_euler(EulerRot::XYZ, -0.12, -0.2, 0.2),
            Vec3::new(0.72, 0.52, 0.72),
        ),
        Some(WeaponKind::MagicStaff) => (
            Vec3::new(-0.3, -0.15, -0.72),
            Quat::from_euler(EulerRot::XYZ, -0.56, -0.22, 0.45),
            Vec3::new(0.95, 1.2, 0.85),
        ),
        _ => (
            Vec3::new(-0.24, -0.21, -0.48),
            Quat::from_euler(EulerRot::XYZ, -0.28, -0.18, 0.55),
            Vec3::new(0.62, 0.82, 0.62),
        ),
    };

    Transform { translation, rotation, scale }
}

#[derive(Clone, Copy)]
struct AttackProfile {
    swing_seconds: f32,
    reach: f32,
    damage: f32,
}

#[derive(Clone, Copy)]
pub struct WeaponPreviewStats {
    pub damage: f32,
    pub swing_seconds: f32,
    pub reach: f32,
}

struct WeaponTuning {
    damage_value: f32,
    speed_value: f32,
    reach: f32,
    base_swing_seconds: f32,
    uses_magic_scaling: bool,
}

fn attack_profile(item: Option<&Item>, slot: WeaponSlot, player_stats: &PlayerStats) -> AttackProfile {
    let total = player_stats.total();
    let Some(item) = item else {
        return match slot {
            WeaponSlot::Primary => AttackProfile { swing_seconds: 0.14, reach: 1.1, damage: 8.0 },
            WeaponSlot::Secondary => AttackProfile { swing_seconds: 0.15, reach: 1.05, damage: 7.0 },
        };
    };

    let tuning = weapon_tuning(item.weapon);
    let stat_scalar = if tuning.uses_magic_scaling {
        (total.magic.max(1) as f32) / 15.0
    } else {
        (total.strength.max(1) as f32) / 15.0
    };
    let base_damage = if tuning.uses_magic_scaling {
        stable_item_roll(item, 200) as f32
    } else {
        6.0 + tuning.damage_value * 2.2
    };
    let agility_bonus = ((total.agility.max(0) as f32) / 20.0)
        * tuning.speed_value
        * (tuning.base_swing_seconds / 10.0);

    AttackProfile {
        swing_seconds: (tuning.base_swing_seconds - agility_bonus).clamp(0.08, tuning.base_swing_seconds.max(0.08)),
        reach: tuning.reach,
        damage: (base_damage * stat_scalar).max(1.0),
    }
}

fn weapon_tuning(weapon: Option<WeaponKind>) -> WeaponTuning {
    match weapon {
        Some(WeaponKind::DoubleAxe) => WeaponTuning { damage_value: 9.0, speed_value: 3.0, reach: 1.68, base_swing_seconds: 0.34, uses_magic_scaling: false },
        Some(WeaponKind::GiantHammer) => WeaponTuning { damage_value: 10.0, speed_value: 1.0, reach: 1.6, base_swing_seconds: 0.4, uses_magic_scaling: false },
        Some(WeaponKind::LongSword) => WeaponTuning { damage_value: 6.0, speed_value: 5.0, reach: 1.58, base_swing_seconds: 0.23, uses_magic_scaling: false },
        Some(WeaponKind::TwoHandedSword) => WeaponTuning { damage_value: 7.0, speed_value: 4.0, reach: 1.72, base_swing_seconds: 0.27, uses_magic_scaling: false },
        Some(WeaponKind::Scythe) => WeaponTuning { damage_value: 6.0, speed_value: 6.0, reach: 1.82, base_swing_seconds: 0.22, uses_magic_scaling: false },
        Some(WeaponKind::MagicStaff) => WeaponTuning { damage_value: 0.0, speed_value: 4.0, reach: 1.56, base_swing_seconds: 0.25, uses_magic_scaling: true },
        Some(WeaponKind::Book) => WeaponTuning { damage_value: 3.0, speed_value: 7.0, reach: 1.06, base_swing_seconds: 0.17, uses_magic_scaling: false },
        Some(WeaponKind::Dagger) => WeaponTuning { damage_value: 5.0, speed_value: 10.0, reach: 1.08, base_swing_seconds: 0.11, uses_magic_scaling: false },
        Some(WeaponKind::Hatchet) => WeaponTuning { damage_value: 6.0, speed_value: 8.0, reach: 1.24, base_swing_seconds: 0.15, uses_magic_scaling: false },
        Some(WeaponKind::ShortSword) => WeaponTuning { damage_value: 7.0, speed_value: 6.0, reach: 1.36, base_swing_seconds: 0.18, uses_magic_scaling: false },
        Some(WeaponKind::Lantern) => WeaponTuning { damage_value: 2.0, speed_value: 7.0, reach: 1.0, base_swing_seconds: 0.18, uses_magic_scaling: false },
        Some(WeaponKind::CrystalBall) => WeaponTuning { damage_value: 2.0, speed_value: 6.0, reach: 1.0, base_swing_seconds: 0.19, uses_magic_scaling: false },
        None => WeaponTuning { damage_value: 3.0, speed_value: 6.0, reach: 1.0, base_swing_seconds: 0.14, uses_magic_scaling: false },
    }
}

fn sync_player_stats_from_equipment(
    equipment: Res<Equipment>,
    mut player_stats: ResMut<PlayerStats>,
) {
    let mut totals = player_stats.base;
    equipment.sum_mods_into(&mut totals);
    let next_bonus = crate::stats::Stats {
        vigor: totals.vigor - player_stats.base.vigor,
        strength: totals.strength - player_stats.base.strength,
        agility: totals.agility - player_stats.base.agility,
        magic: totals.magic - player_stats.base.magic,
        endurance: totals.endurance - player_stats.base.endurance,
    };

    if player_stats.bonus.vigor != next_bonus.vigor
        || player_stats.bonus.strength != next_bonus.strength
        || player_stats.bonus.agility != next_bonus.agility
        || player_stats.bonus.magic != next_bonus.magic
        || player_stats.bonus.endurance != next_bonus.endurance
    {
        player_stats.bonus = next_bonus;
        player_stats.recompute_limbs();
    }
}

fn movement_speed_multiplier(equipment: &Equipment) -> f32 {
    equipped_hand_items(equipment)
        .filter(|item| item.weapon == Some(WeaponKind::Lantern))
        .map(|item| 1.0 + (stable_item_roll(item, 100) as f32 / 100.0))
        .fold(1.0, f32::max)
}

fn equipped_hand_items(equipment: &Equipment) -> impl Iterator<Item = &Item> {
    [equipment.mainhand.as_ref(), equipment.offhand.as_ref(), equipment.twohand.as_ref()]
        .into_iter()
        .flatten()
}

fn stable_item_roll(item: &Item, max: u32) -> u32 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;

    for byte in item.name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash ^= item.size.0 as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
    hash ^= item.size.1 as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
    hash ^= item.mods.vigor as i64 as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
    hash ^= item.mods.strength as i64 as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
    hash ^= item.mods.agility as i64 as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
    hash ^= item.mods.magic as i64 as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);
    hash ^= item.mods.endurance as i64 as u64;
    hash = hash.wrapping_mul(0x100_0000_01b3);

    for (label, value) in &item.extra_rolls {
        for byte in label.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash ^= *value as i64 as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }

    ((hash % max as u64) + 1) as u32
}

fn apply_draw_transition(mut base: Transform, slot: WeaponSlot, draw_blend: f32, right_side: bool) -> Transform {
    let side = if right_side { 1.0 } else { -1.0 };
    let eased = ease_out_cubic(draw_blend.clamp(0.0, 1.0));
    let stow_offset = match slot {
        WeaponSlot::Primary => Vec3::new(0.22 * side, -0.42, 0.34),
        WeaponSlot::Secondary => Vec3::new(-0.18 * side, -0.38, 0.28),
    };
    let stow_rotation = match slot {
        WeaponSlot::Primary => Quat::from_euler(EulerRot::XYZ, 0.55, 0.18 * side, 0.42 * side),
        WeaponSlot::Secondary => Quat::from_euler(EulerRot::XYZ, 0.48, -0.2 * side, -0.36 * side),
    };

    base.translation += stow_offset * (1.0 - eased);
    base.rotation = stow_rotation.slerp(base.rotation, eased);
    base.scale *= 0.75 + 0.25 * eased;
    base
}

fn swing_curve(progress: f32) -> (f32, f32) {
    let recoil_window = 0.32;
    if progress <= recoil_window {
        let t = (progress / recoil_window).clamp(0.0, 1.0);
        (ease_out_cubic(t), 0.0)
    } else {
        let t = ((progress - recoil_window) / (1.0 - recoil_window)).clamp(0.0, 1.0);
        (1.0, ease_in_out_quad(t))
    }
}

fn motion_bob_translation(phase: f32, move_amount: f32) -> Vec3 {
    if move_amount <= 0.01 {
        return Vec3::ZERO;
    }

    Vec3::new(
        0.0035 * move_amount * (phase * 0.5).sin(),
        0.012 * move_amount * phase.sin().abs(),
        0.004 * move_amount * phase.cos(),
    )
}

fn motion_bob_rotation(phase: f32, move_amount: f32) -> Quat {
    if move_amount <= 0.01 {
        return Quat::IDENTITY;
    }

    Quat::from_euler(
        EulerRot::XYZ,
        0.006 * move_amount * phase.sin().abs(),
        0.004 * move_amount * (phase * 0.5).sin(),
        0.01 * move_amount * phase.cos(),
    )
}

fn idle_bob_offset(phase: f32, move_amount: f32, slot: WeaponSlot, right_side: bool) -> Vec3 {
    let side_phase = if right_side { 0.0_f32 } else { 0.7_f32 };
    let amplitude = match slot {
        WeaponSlot::Primary => Vec3::new(0.008, 0.006, 0.004),
        WeaponSlot::Secondary => Vec3::new(0.006, 0.005, 0.003),
    };
    let strength = move_amount.clamp(0.0, 1.0);
    let bob_phase = phase + side_phase;
    Vec3::new(
        strength * amplitude.x * (bob_phase * 1.5).sin(),
        strength * amplitude.y * (bob_phase * 2.0).cos().abs(),
        strength * amplitude.z * (bob_phase * 1.5).cos(),
    )
}

fn idle_bob_rotation(phase: f32, move_amount: f32, slot: WeaponSlot, right_side: bool) -> Quat {
    let phase = if right_side { phase + 0.15 } else { phase + 0.9 };
    let (pitch, yaw, roll) = match slot {
        WeaponSlot::Primary => (0.006, 0.004, 0.008),
        WeaponSlot::Secondary => (0.005, 0.004, 0.006),
    };
    let strength = move_amount.clamp(0.0, 1.0);
    Quat::from_euler(
        EulerRot::XYZ,
        strength * pitch * (phase * 0.9).sin(),
        strength * yaw * (phase * 0.6).cos(),
        strength * roll * phase.sin(),
    )
}

fn apply_viewmodel_impact(mut transform: Transform, jump_visual: f32, landing_dip: f32, right_side: bool) -> Transform {
    let side = if right_side { 1.0 } else { -1.0 };
    transform.translation += Vec3::new(
        0.008 * side * landing_dip,
        -0.055 * landing_dip + 0.016 * jump_visual,
        -0.012 * jump_visual + 0.032 * landing_dip,
    );
    transform.rotation *= Quat::from_euler(
        EulerRot::XYZ,
        -0.09 * landing_dip + 0.03 * jump_visual,
        0.012 * side * landing_dip,
        -0.028 * side * landing_dip,
    );
    transform
}

fn weapon_swing_offset(recoil: f32, recovery: f32, slot: WeaponSlot, two_handed: bool, item: Option<&Item>) -> Vec3 {
    let attack = recoil - recovery * 0.92;
    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) => Vec3::new(0.06 * attack, -0.04 * recoil + 0.03 * recovery, 0.3 * recoil - 0.2 * recovery),
        Some(WeaponKind::ShortSword) => Vec3::new(0.14 * attack, -0.08 * recoil + 0.05 * recovery, 0.24 * recoil - 0.14 * recovery),
        Some(WeaponKind::Hatchet) => Vec3::new(0.2 * attack, -0.12 * recoil + 0.08 * recovery, 0.12 * recoil - 0.08 * recovery),
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => Vec3::new(0.04 * attack, -0.02 * recoil + 0.04 * recovery, 0.1 * recoil - 0.08 * recovery),
        Some(WeaponKind::MagicStaff) => Vec3::new(0.05 * attack, -0.1 * recoil + 0.12 * recovery, 0.18 * recoil - 0.16 * recovery),
        Some(WeaponKind::Scythe) => Vec3::new(0.22 * attack, -0.02 * recoil + 0.08 * recovery, 0.08 * recoil - 0.06 * recovery),
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Vec3::new(0.04 * attack, -0.18 * recoil + 0.14 * recovery, 0.26 * recoil - 0.18 * recovery),
        Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Vec3::new(0.1 * attack, -0.12 * recoil + 0.1 * recovery, 0.28 * recoil - 0.16 * recovery),
        None => match slot {
            WeaponSlot::Primary if two_handed => Vec3::new(0.08 * attack, -0.06 * recoil + 0.08 * recovery, 0.22 * recoil - 0.14 * recovery),
            WeaponSlot::Primary => Vec3::new(0.18 * attack, -0.1 * recoil + 0.08 * recovery, 0.18 * recoil - 0.1 * recovery),
            WeaponSlot::Secondary => Vec3::new(-0.16 * attack, -0.08 * recoil + 0.06 * recovery, 0.14 * recoil - 0.08 * recovery),
        },
    }
}

fn weapon_swing_rotation(recoil: f32, recovery: f32, slot: WeaponSlot, two_handed: bool, item: Option<&Item>) -> Quat {
    let attack = recoil - recovery * 0.88;
    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) => Quat::from_euler(EulerRot::XYZ, -0.18 * recoil + 0.1 * recovery, 0.08 * attack, -0.28 * attack),
        Some(WeaponKind::ShortSword) => Quat::from_euler(EulerRot::XYZ, -0.28 * recoil + 0.14 * recovery, 0.12 * attack, -0.72 * attack),
        Some(WeaponKind::Hatchet) => Quat::from_euler(EulerRot::XYZ, -0.52 * recoil + 0.22 * recovery, 0.24 * attack, -1.18 * attack),
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => Quat::from_euler(EulerRot::XYZ, -0.14 * recoil + 0.18 * recovery, 0.06 * attack, -0.18 * attack),
        Some(WeaponKind::MagicStaff) => Quat::from_euler(EulerRot::XYZ, -0.48 * recoil + 0.3 * recovery, -0.08 * attack, -0.42 * attack),
        Some(WeaponKind::Scythe) => Quat::from_euler(EulerRot::XYZ, -0.22 * recoil + 0.18 * recovery, 0.34 * attack, -1.26 * attack),
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Quat::from_euler(EulerRot::XYZ, -0.72 * recoil + 0.34 * recovery, 0.18 * attack, -0.62 * attack),
        Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Quat::from_euler(EulerRot::XYZ, -0.58 * recoil + 0.3 * recovery, 0.16 * attack, -0.86 * attack),
        None => match slot {
            WeaponSlot::Primary if two_handed => Quat::from_euler(EulerRot::XYZ, -0.45 * recoil + 0.25 * recovery, 0.12 * attack, -0.55 * attack),
            WeaponSlot::Primary => Quat::from_euler(EulerRot::XYZ, -0.35 * recoil + 0.18 * recovery, 0.2 * attack, -0.95 * attack),
            WeaponSlot::Secondary => Quat::from_euler(EulerRot::XYZ, -0.25 * recoil + 0.16 * recovery, -0.22 * attack, 0.95 * attack),
        },
    }
}

fn fist_swing_offset(recoil: f32, recovery: f32, right: bool, slot: WeaponSlot, two_handed: bool, primary_item: Option<&Item>, secondary_item: Option<&Item>) -> Vec3 {
    let side = if right { 1.0 } else { -1.0 };
    let attack = recoil - recovery * 0.9;
    let item = match slot {
        WeaponSlot::Primary => primary_item,
        WeaponSlot::Secondary => secondary_item,
    };
    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) => Vec3::new(0.06 * attack * side, -0.04 * recoil + 0.04 * recovery, 0.18 * recoil - 0.14 * recovery),
        Some(WeaponKind::ShortSword) => Vec3::new(0.08 * attack * side, -0.05 * recoil + 0.04 * recovery, 0.14 * recoil - 0.1 * recovery),
        Some(WeaponKind::Hatchet) => Vec3::new(0.14 * attack * side, -0.08 * recoil + 0.04 * recovery, 0.08 * recoil - 0.08 * recovery),
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => Vec3::new(0.02 * attack * side, -0.02 * recoil + 0.04 * recovery, 0.08 * recoil - 0.06 * recovery),
        Some(WeaponKind::MagicStaff) => Vec3::new(0.02 * attack * side, -0.06 * recoil + 0.06 * recovery, 0.1 * recoil - 0.1 * recovery),
        Some(WeaponKind::Scythe) => Vec3::new(0.1 * attack * side, -0.04 * recoil + 0.03 * recovery, 0.06 * recoil - 0.06 * recovery),
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Vec3::new(0.04 * attack * side, -0.12 * recoil + 0.08 * recovery, 0.12 * recoil - 0.08 * recovery),
        Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Vec3::new(0.04 * attack * side, -0.08 * recoil + 0.06 * recovery, 0.12 * recoil - 0.08 * recovery),
        None => match slot {
            WeaponSlot::Primary if two_handed => Vec3::new(0.03 * side * attack, -0.04 * recoil + 0.03 * recovery, 0.1 * recoil - 0.08 * recovery),
            WeaponSlot::Primary => Vec3::new(0.12 * attack * side, -0.08 * recoil + 0.06 * recovery, 0.14 * recoil - 0.1 * recovery),
            WeaponSlot::Secondary => Vec3::new(-0.08 * recoil, -0.06 * recoil + 0.04 * recovery, 0.1 * recoil - 0.08 * recovery),
        },
    }
}

fn fist_swing_rotation(recoil: f32, recovery: f32, right: bool, slot: WeaponSlot, two_handed: bool, primary_item: Option<&Item>, secondary_item: Option<&Item>) -> Quat {
    let roll = if right { -0.85 } else { 0.85 };
    let attack = recoil - recovery * 0.86;
    let item = match slot {
        WeaponSlot::Primary => primary_item,
        WeaponSlot::Secondary => secondary_item,
    };
    match item.and_then(|entry| entry.weapon) {
        Some(WeaponKind::Dagger) => Quat::from_euler(EulerRot::XYZ, -0.08 * recoil + 0.05 * recovery, 0.08 * attack, 0.24 * roll * attack),
        Some(WeaponKind::ShortSword) => Quat::from_euler(EulerRot::XYZ, -0.14 * recoil + 0.07 * recovery, 0.02 * attack, 0.58 * roll * attack),
        Some(WeaponKind::Hatchet) => Quat::from_euler(EulerRot::XYZ, -0.24 * recoil + 0.08 * recovery, 0.1 * attack, 0.82 * roll * attack),
        Some(WeaponKind::Lantern) | Some(WeaponKind::CrystalBall) | Some(WeaponKind::Book) => Quat::from_euler(EulerRot::XYZ, -0.1 * recoil + 0.08 * recovery, 0.02 * attack, 0.2 * roll * attack),
        Some(WeaponKind::MagicStaff) => Quat::from_euler(EulerRot::XYZ, -0.18 * recoil + 0.12 * recovery, -0.08 * attack, 0.26 * roll * attack),
        Some(WeaponKind::Scythe) => Quat::from_euler(EulerRot::XYZ, -0.1 * recoil + 0.06 * recovery, 0.22 * attack, 0.42 * roll * attack),
        Some(WeaponKind::DoubleAxe) | Some(WeaponKind::GiantHammer) => Quat::from_euler(EulerRot::XYZ, -0.28 * recoil + 0.1 * recovery, 0.04 * attack, 0.3 * roll * attack),
        Some(WeaponKind::LongSword) | Some(WeaponKind::TwoHandedSword) => Quat::from_euler(EulerRot::XYZ, -0.2 * recoil + 0.08 * recovery, 0.06 * attack, 0.4 * roll * attack),
        None => match slot {
            WeaponSlot::Primary if two_handed => Quat::from_euler(EulerRot::XYZ, -0.12 * recoil + 0.06 * recovery, 0.08 * attack, 0.35 * roll * attack),
            WeaponSlot::Primary => Quat::from_euler(EulerRot::XYZ, -0.18 * recoil + 0.08 * recovery, 0.0, roll * attack),
            WeaponSlot::Secondary => Quat::from_euler(EulerRot::XYZ, -0.12 * recoil + 0.06 * recovery, -0.1 * attack, 0.5 * attack),
        },
    }
}

fn two_handed_hand_transform(right: bool) -> Transform {
    if right {
        Transform::from_xyz(0.14, -0.24, -0.34)
            .with_rotation(Quat::from_euler(EulerRot::XYZ, -0.15, 0.12, -0.28))
    } else {
        Transform::from_xyz(-0.1, -0.19, -0.46)
            .with_rotation(Quat::from_euler(EulerRot::XYZ, -0.08, -0.1, 0.22))
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - ((-2.0 * t + 2.0).powi(2) / 2.0)
    }
}

pub fn weapon_preview_stats(item: Option<&Item>, slot: WeaponSlot, player_stats: &PlayerStats) -> Option<WeaponPreviewStats> {
    let item = item?;
    item.weapon?;
    let profile = attack_profile(Some(item), slot, player_stats);
    Some(WeaponPreviewStats {
        damage: profile.damage,
        swing_seconds: profile.swing_seconds,
        reach: profile.reach,
    })
}

fn sync_window_cursor(
    state: Res<State<AppState>>,
    flags: Res<UIFlags>,
    open_chest: Res<OpenChest>,
    mut menu_focus: ResMut<MenuFocusState>,
    mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    let menu_active = match state.get() {
        AppState::Menu => true,
        AppState::InGame => {
            flags.inventory_open
                || flags.spell_menu_open
                || flags.pause_menu_open
                || open_chest.0.is_some()
        }
    };

    cursor_options.visible = menu_active;
    cursor_options.grab_mode = if menu_active {
        CursorGrabMode::None
    } else {
        CursorGrabMode::Locked
    };

    if menu_active && menu_focus.pending {
        let width = window.width();
        let height = window.height();
        let cursor_target = match menu_focus.target {
            Some(MenuFocusTarget::StartScreen) => Vec2::new(width * 0.5, height * 0.5),
            Some(MenuFocusTarget::Inventory) => Vec2::new(width * 0.18, height * 0.22),
            Some(MenuFocusTarget::Chest(_)) => Vec2::new(width * 0.42, height * 0.24),
            Some(MenuFocusTarget::Pause) => Vec2::new(width * 0.5, height * 0.46),
            Some(MenuFocusTarget::Spells) => Vec2::new(width * 0.78, height * 0.2),
            None => Vec2::new(width * 0.5, height * 0.5),
        };
        window.set_cursor_position(Some(cursor_target));
        menu_focus.pending = false;
    }
}