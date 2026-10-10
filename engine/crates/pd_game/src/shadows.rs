//! Shadows (lane L2): fighters and the stage's authored volumes cast shadows
//! onto floors, walls and fighters, darkened the way the original darkens them.
//!
//! Full write-up, evidence and measurements: `docs/SHADOWS.md`. In short:
//!
//! **What the original does** (`FUN_140299310`, the shader tables
//! `SSHADOW*`/`sshadowch*`): every shadow volume (stage `SSHADOW`/`SSHADOWC`
//! draws with material flag 0x2000, the fighters' `SSHADOWCH` hulls) is drawn
//! into a stencil buffer with z-fail counting; then one full-screen quad is
//! blended over the pixels the stencil marks, `out = C*A + dst*(1-A)` (blend
//! factors DEST_ALPHA / INV_DEST_ALPHA), where `C` is the stage script's
//! shadow colour (`call 02 sub 1`, e.g. st03 (15,18,25)) and `A` is the alpha
//! every surface shader (`SD_ps`, `SBTS_ps`, `SWBOS_ps`) writes into the
//! framebuffer: `A = 2*(0.8*sat(N.L)+0.00806)*(2*tex_alpha-1.5)`.
//!
//! **What this does instead, and why**: the game renders 4x MSAA on Bevy's
//! `Depth32Float` (no stencil, multisampled depth), so a true stencil / z-fail
//! pass would mean changing the depth format of every pipeline or adding a
//! custom multisampled accumulation pass. Bevy's directional shadow maps give
//! the same volume test (is this surface behind a caster along the light?)
//! with the authored volume meshes as the casters: the volumes are exported as
//! named glb nodes (`pd_formats::gltf::VOLUME_TAG`), put on a render layer only
//! the lights see, and the fighters' own meshes cast from their own layer.
//! Receivers (`stage_material.wgsl`, `dissolve.wgsl`) apply the original's
//! blend with the shadow-map lookup standing in for the stencil test, so the
//! darkening colour, the N.L weighting and the gamma-space blend are the
//! original's. Outside a shadow nothing changes (`k == 0` returns the input).
//!
//! Layers (`RenderLayers`): the player cameras gain layer `CAMERA_LAYER`; the
//! two shadow lights carry `CAMERA_LAYER` (so a view lists them) plus their own
//! caster layer -- `VOLUME_LAYER` (stage volumes) or `FIGHTER_LAYER` (fighters).
//! Nothing else is on those layers, so stage meshes, props' visible meshes,
//! effects and orbs cast nothing.
//!
//! Switches: `PD_SHADOWS=0` turns the whole lane off (no lights, no material
//! swap: today's picture exactly), `PD_SHADOW_MAP=<px>` map size (default
//! 2048), `PD_SHADOW_DIST=<units>` stage shadow distance (default 700),
//! `PD_SHADOW_FILTER=gaussian` (default: hardware 2x2, crisp like the
//! original's pixel-exact volumes), `PD_SHADOW_BIAS` / `PD_SHADOW_NBIAS`.
//!
//! Render-side only; the simulation never sees any of it.

#[path = "shadow_grid.rs"]
mod shadow_grid;

use crate::frontend::state::MatchScene;
use crate::stage_material::{light_from_json, mat_from_extras, stage_material, StageLightUniform, StageMaterial};
use crate::{env_f32, PlayerCamera};
use bevy::camera::primitives::MeshAabb;
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::gltf::GltfMaterialExtras;
use bevy::asset::RenderAssetUsages;
use bevy::render::storage::ShaderBuffer;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap, NotShadowCaster, ShadowFilteringMethod};
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// Layer the player cameras gain so their views list the shadow lights
/// (a light only reaches views whose layers intersect its own).
pub const CAMERA_LAYER: usize = 6;
/// Caster layer of the stage's shadow volumes (never rendered by a camera).
pub const VOLUME_LAYER: usize = 7;
/// Caster layer of the fighters' body meshes.
pub const FIGHTER_LAYER: usize = 8;
/// Parking layer for meshes nothing draws and no light sees (the grid node,
/// the stage volumes the grid replaces).
pub const INERT_LAYER: usize = 9;

/// Meshes whose bounding box is at least this wide are the sky shells, never
/// receivers (same threshold as `sky_scroll::MIN_SHELL_EXTENT`).
const SHELL_EXTENT: f32 = crate::sky_scroll::MIN_SHELL_EXTENT;

static SHADOWS_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();
static EMPTY_VOLUME: std::sync::OnceLock<Handle<ShaderBuffer>> = std::sync::OnceLock::new();

/// The storage buffer a receiver binds before the stage volumes are known:
/// an empty grid (triangle count 0). `sync_volume_buffer` swaps in the real
/// one.
pub fn volume_buffer() -> Handle<ShaderBuffer> {
    EMPTY_VOLUME.get().cloned().unwrap_or_default()
}

/// `PD_SHADOW_GRID=0`: the stage own volumes cast through the shadow map
/// again (lane L2 behaviour: every volume is a caster, none a pool).
pub fn grid_enabled() -> bool {
    !crate::stage_look::legacy_light() && std::env::var("PD_SHADOW_GRID").map_or(true, |v| v != "0")
}

/// The stage volume grid, once built from the glb `GRID_NODE` mesh.
#[derive(Resource, Clone)]
pub struct VolumeGrid {
    pub buffer: Handle<ShaderBuffer>,
}

/// `PD_SHADOWS=0` disables the lane entirely.
pub fn enabled() -> bool {
    std::env::var("PD_SHADOWS").map_or(true, |v| v != "0")
}

/// What the stage's light script says about shadows (`stNN.light.json`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLook {
    /// Direction the stage's light travels (set 0), right-handed world space.
    /// Stage volumes were authored for it.
    pub stage_dir: Vec3,
    /// Direction the fighters' light travels (set 1): the light the program
    /// binds for the fighters' shadow hulls (`FUN_140199e70` group 1).
    pub fighter_dir: Vec3,
    /// The shadow colour: script call `02` sub 1 (`DAT_1403e8ba8` ->
    /// `DAT_14057f3bc`, the colour of the shadow pass's full-screen quad), 0..1.
    pub colour: Vec3,
}

impl Default for ShadowLook {
    /// A stage without a light script: the old hand-set key, black shadows.
    fn default() -> Self {
        let d = -Vec3::new(0.4, 1.0, 0.6).normalize();
        Self { stage_dir: d, fighter_dir: d, colour: Vec3::ZERO }
    }
}

impl ShadowLook {
    /// From a parsed `light.json` (game space is left-handed: Z mirrors).
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        // Component order: the program's (see `stage_look::script_vec`).
        let dir = |s: &serde_json::Value| -> Option<Vec3> {
            let d = crate::stage_look::script_vec(v, &s["dir"])?;
            Some(Vec3::new(d.x, d.y, -d.z).normalize_or(Vec3::NEG_Y))
        };
        let stage_dir = dir(&v["stage"])?;
        let fighter_dir = dir(&v["characters"]).unwrap_or(stage_dir);
        let colour = v["backdrop"].as_array().map_or(Vec3::ZERO, |a| {
            let f = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32 / 255.0;
            Vec3::new(f(0), f(1), f(2))
        });
        Some(Self { stage_dir, fighter_dir, colour })
    }

    pub fn load(cache: &Path, stage: &str) -> Self {
        std::fs::read_to_string(cache.join(format!("{stage}.light.json"))).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| Self::from_json(&v)).unwrap_or_default()
    }
}

/// The stage this match plays and its shadow look; inserted by `spawn_lights`.
#[derive(Resource, Clone, Debug)]
pub struct ShadowStage {
    /// The stage glb's file name in the cache (`st03.glb`).
    pub glb: String,
    pub look: ShadowLook,
    /// The stage's light set 0 as the per-pixel stage shader reads it.
    pub light: StageLightUniform,
}

/// The stage glTF's scene root.
#[derive(Component)]
struct StageRoot;

pub fn map_size() -> usize {
    (env_f32("PD_SHADOW_MAP", 2048.0).max(256.0) as usize).next_power_of_two()
}

/// Spawns the two shadow lights for a match and records the stage.
/// Called from `main.rs::spawn_scene`. Does nothing with `PD_SHADOWS=0`.
///
/// The lights add no illumination (1 lux against the fighters' thousands: they
/// are lit by the SWBOS port and the stage is unlit); they exist to own
/// the shadow maps. Their *colour* carries the stage's shadow colour into the
/// receivers' shaders (`shadows.wgsl` reads it off the light, so no extra
/// binding is needed on the fighters' material).
pub fn spawn_lights(commands: &mut Commands, cache: &Path, stage: &str) {
    if !enabled() {
        return;
    }
    commands.remove_resource::<VolumeGrid>();
    let look = ShadowLook::load(cache, stage);
    let light = std::fs::read_to_string(cache.join(format!("{stage}.light.json"))).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| light_from_json(&v)).unwrap_or_default();
    commands.insert_resource(ShadowStage { glb: format!("{stage}.glb"), look, light });
    let colour = Color::srgb(look.colour.x, look.colour.y, look.colour.z);
    let bias = env_f32("PD_SHADOW_BIAS", 0.05);
    let nbias = env_f32("PD_SHADOW_NBIAS", 1.8);
    let up = |d: Vec3| if d.y.abs() > 0.95 { Vec3::Z } else { Vec3::Y };
    let dist = env_f32("PD_SHADOW_DIST", 700.0);
    // Stage volumes: stage light, four cascades out to the stage distance.
    commands.spawn((
        DirectionalLight { color: colour, illuminance: 1.0, shadow_maps_enabled: true, shadow_depth_bias: bias, shadow_normal_bias: nbias, ..default() },
        Transform::default().looking_to(look.stage_dir, up(look.stage_dir)),
        CascadeShadowConfigBuilder { num_cascades: 4, minimum_distance: 0.1, first_cascade_far_bound: 25.0, maximum_distance: dist, overlap_proportion: 0.2 }.build(),
        RenderLayers::from_layers(&[CAMERA_LAYER, VOLUME_LAYER]),
        MatchScene,
    ));
    // Fighters: the fighters' light, three cascades (fighters are small and
    // only matter near the camera).
    commands.spawn((
        DirectionalLight { color: colour, illuminance: 1.0, shadow_maps_enabled: true, shadow_depth_bias: bias, shadow_normal_bias: nbias, ..default() },
        Transform::default().looking_to(look.fighter_dir, up(look.fighter_dir)),
        CascadeShadowConfigBuilder { num_cascades: 3, minimum_distance: 0.1, first_cascade_far_bound: 25.0, maximum_distance: dist.min(300.0), overlap_proportion: 0.2 }.build(),
        RenderLayers::from_layers(&[CAMERA_LAYER, FIGHTER_LAYER]),
        MatchScene,
    ));
}

/// Registers the shader module, the stage material and (unless
/// `PD_SHADOWS=0`) the systems. Called from `main.rs` next to the other
/// material plugins; must come before `cam_fade::plugin` is used by a frame
/// (the dissolve shader imports `pd_game::shadows`).
pub fn plugin(app: &mut App) {
    let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("shadows.wgsl"), "pd_game/shadows.wgsl"));
    let _ = SHADOWS_SHADER.set(shader);
    let empty = app.world_mut().resource_mut::<Assets<ShaderBuffer>>().add(ShaderBuffer::new(&[0u8; 32], RenderAssetUsages::default()));
    let _ = EMPTY_VOLUME.set(empty);
    crate::stage_material::register(app);
    // Proof harness: `PD_SHADOW_CAM=ex,ey,ez,lx,ly,lz` (or `PD_SHADOW_CAM_AT`)
    // parks player 1's camera so a comparison can reuse a view.
    if std::env::var_os("PD_SHADOW_CAM").is_some() || std::env::var_os("PD_SHADOW_CAM_AT").is_some() {
        app.add_systems(PostUpdate, park_camera.before(bevy::transform::TransformSystems::Propagate));
    }
    if std::env::var_os("PD_FRAMETIME").is_some() {
        app.add_systems(Startup, |mut w: Query<&mut Window>| {
            for mut w in &mut w {
                w.present_mode = bevy::window::PresentMode::AutoNoVsync;
            }
        });
        app.add_systems(Last, log_frame_time);
    }
    if !enabled() {
        // Volumes are still in re-exported stage glbs: keep them invisible.
        app.add_systems(PostUpdate, hide_volumes.before(VisibilitySystems::CheckVisibility));
        return;
    }
    app.insert_resource(DirectionalLightShadowMap { size: map_size() });
    app.add_systems(Update, trim_cascades.run_if(resource_exists::<ShadowStage>));
    if std::env::var_os("PD_SHADOW_LOG").is_some() {
        app.add_systems(PostUpdate, log_lights.run_if(resource_exists::<ShadowStage>));
    }
    // PostUpdate, before the visibility check: the glTF scene spawns after
    // Update, and a volume mesh must be on its hidden layer before the first
    // frame that could draw it (else it flashes white for one frame).
    app.add_systems(
        PostUpdate,
        (tag_stage_roots, prepare_stage_meshes, tag_fighter_hulls, tag_fighter_casters, tag_cameras, sync_volume_buffer, skip_non_casters).chain().run_if(resource_exists::<ShadowStage>).before(VisibilitySystems::CheckVisibility),
    );
}

/// `NotShadowCaster` that [`skip_non_casters`] added (it only ever removes its own).
#[derive(Component)]
struct AutoNotCaster;

/// Marks every mesh no shadow light can see as `NotShadowCaster` (overnight perf 2026-10-09).
///
/// Bevy's `check_dir_light_mesh_visibility` walks EVERY mesh without `NotShadowCaster` once per
/// (shadow light, view) -- 8 parallel walks a frame with four players, over the stage's ~550 meshes,
/// the props, every effect piece and orb -- only to drop all but the volumes and fighters at its
/// render-layer test. A mesh whose layers miss every shadow light's layers can never cast (that
/// same test), so marking it changes no picture; it just leaves the walks with the casters. Kept
/// in step when a mesh's layers change (a fighter piece gaining `FIGHTER_LAYER`) or the lights do.
#[allow(clippy::type_complexity)]
fn skip_non_casters(
    mut commands: Commands,
    lights: Query<(&DirectionalLight, Option<&RenderLayers>)>,
    all: Query<(Entity, Option<&RenderLayers>, Has<NotShadowCaster>, Has<AutoNotCaster>), With<Mesh3d>>,
    changed: Query<Entity, (With<Mesh3d>, Or<(Added<Mesh3d>, Changed<RenderLayers>)>)>,
    mut lost_layers: RemovedComponents<RenderLayers>,
    mut seen: Local<Option<RenderLayers>>,
) {
    // What the shadow lights see (a light without layers sees layer 0, Bevy's default).
    let mut reach = RenderLayers::none();
    for (light, layers) in &lights {
        if light.shadow_maps_enabled {
            reach = reach.union(layers.unwrap_or(&RenderLayers::default()));
        }
    }
    let mut decide = |e: Entity| {
        let Ok((e, layers, not_caster, auto)) = all.get(e) else { return };
        let can_cast = reach.intersects(layers.unwrap_or(&RenderLayers::default()));
        if !can_cast && !not_caster {
            commands.entity(e).insert((NotShadowCaster, AutoNotCaster));
        } else if can_cast && auto {
            commands.entity(e).remove::<(NotShadowCaster, AutoNotCaster)>();
        }
    };
    if seen.as_ref() != Some(&reach) {
        // The lights changed (a match started, `PD_SHADOWS` lights spawned): decide every mesh.
        *seen = Some(reach.clone());
        for (e, ..) in &all {
            decide(e);
        }
        lost_layers.clear();
        return;
    }
    for e in changed.iter().chain(lost_layers.read()) {
        decide(e);
    }
}

/// Cascades per shadow light with `views` player views on this machine: Bevy builds every cascade
/// of every shadow light once PER CAMERA (stage light 4 + fighter light 3 = 7 depth passes a
/// view), so three or four views cost 21-28 shadow passes a frame. From three views each light
/// keeps two cascades (Laurence picked B on the A/B sheet, 2026-10-09: distant shadow edges shift
/// slightly, ~0.9 ms a frame saved at four views; PERF.md "Round 3"); one and two views always keep
/// the full set. `PD_SHADOW_CASCADES_TRIM=0` keeps the full set at every view count (the look
/// before 2026-10-09).
pub fn cascades_for(views: usize, full: usize) -> usize {
    cascades_with(views, full, trim_enabled())
}

/// `PD_SHADOW_CASCADES_TRIM=0` turns the 3-4 view trim off. Read once.
fn trim_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("PD_SHADOW_CASCADES_TRIM").map_or(true, |v| v != "0"))
}

fn cascades_with(views: usize, full: usize, trim: bool) -> usize {
    if views >= 3 && trim {
        full.min(2)
    } else {
        full
    }
}

/// The cascade bounds a light gets with `views` player views (`full` = its full cascade count).
pub fn trimmed_config(views: usize, full: usize, first_bound: f32, maximum: f32) -> CascadeShadowConfig {
    config_with(cascades_for(views, full), first_bound, maximum)
}

fn config_with(cascades: usize, first_bound: f32, maximum: f32) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder { num_cascades: cascades, minimum_distance: 0.1, first_cascade_far_bound: first_bound, maximum_distance: maximum, overlap_proportion: 0.2 }.build()
}

/// Keeps the two shadow lights' cascade counts matched to the number of player views on this
/// machine (CPU seats and remote players have none).
fn trim_cascades(cams: Query<&Camera, With<PlayerCamera>>, mut lights: Query<(&DirectionalLight, &mut CascadeShadowConfig)>) {
    let players = cams.iter().filter(|c| c.is_active).count();
    let dist = env_f32("PD_SHADOW_DIST", 700.0);
    for (light, mut cfg) in &mut lights {
        if !light.shadow_maps_enabled {
            continue;
        }
        // The stage light is built with four cascades out to the stage distance, the fighters' with three out to 300.
        let (full, max) = if cfg.bounds.last().is_some_and(|b| *b > 300.5) || cfg.bounds.len() == 4 { (4, dist) } else { (3, dist.min(300.0)) };
        let want = trimmed_config(players, full, 25.0, max);
        if cfg.bounds.len() != want.bounds.len() {
            *cfg = want;
        }
    }
}

/// `PD_SHADOW_CAM` (proof harness only): player 1's camera at a fixed pose,
/// or `PD_SHADOW_CAM_AT=<slot>,dx,dy,dz,look_dy`: at an offset from a fighter.
fn park_camera(mut cams: Query<(&PlayerCamera, &mut Transform)>, bodies: Query<(&crate::PlayerVisual, &GlobalTransform)>) {
    let pose = if let Some(v) = std::env::var("PD_SHADOW_CAM_AT").ok() {
        let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        let [slot, dx, dy, dz, ly, ..] = n[..] else { return };
        let Some((_, g)) = bodies.iter().find(|(p, _)| p.0 == slot as usize) else { return };
        let at = g.translation();
        Some((at + Vec3::new(dx, dy, dz), at + Vec3::new(0.0, ly, 0.0)))
    } else {
        let v = std::env::var("PD_SHADOW_CAM").unwrap_or_default();
        let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [ex, ey, ez, lx, ly, lz, ..] = n[..] {
            Some((Vec3::new(ex, ey, ez), Vec3::new(lx, ly, lz)))
        } else {
            None
        }
    };
    let Some((eye, look)) = pose else { return };
    for (c, mut t) in &mut cams {
        if c.0 == 0 {
            *t = Transform::from_translation(eye).looking_at(look, Vec3::Y);
        }
    }
}

/// `PD_SHADOWS=0`: nothing casts, but a stage glb carries its volume meshes
/// (plain white geometry): never draw them.
fn hide_volumes(mut commands: Commands, new: Query<Entity, Added<Mesh3d>>, names: Query<&Name>, parents: Query<&ChildOf>) {
    for e in &new {
        if is_volume_node(e, &names, &parents) {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
}

/// Marks the stage glTF's scene root (not the props library, not fighters).
fn tag_stage_roots(mut commands: Commands, roots: Query<(Entity, &WorldAssetRoot), Added<WorldAssetRoot>>, assets: Res<AssetServer>, stage: Res<ShadowStage>) {
    for (e, root) in &roots {
        if assets.get_path(root.0.id()).is_some_and(|p| p.path() == Path::new(&stage.glb)) {
            commands.entity(e).insert(StageRoot);
        }
    }
}

/// Does this entity own name, or its parent (Bevy names the node entity
/// and calls the primitive own entity "Mesh"), satisfy `f`?
fn is_named(e: Entity, names: &Query<&Name>, parents: &Query<&ChildOf>, f: impl Fn(&str) -> bool) -> bool {
    let named = |e: Entity| names.get(e).is_ok_and(|n| f(n.as_str()));
    named(e) || parents.get(e).is_ok_and(|p| named(p.parent()))
}

/// A mesh triangle list (`TriangleList`, indexed or not).
fn mesh_triangles(mesh: &Mesh) -> Vec<[Vec3; 3]> {
    use bevy::mesh::{Indices, VertexAttributeValues};
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return Vec::new() };
    let p = |i: usize| Vec3::from(pos[i]);
    match mesh.indices() {
        Some(Indices::U32(ix)) => ix.chunks_exact(3).map(|t| [p(t[0] as usize), p(t[1] as usize), p(t[2] as usize)]).collect(),
        Some(Indices::U16(ix)) => ix.chunks_exact(3).map(|t| [p(t[0] as usize), p(t[1] as usize), p(t[2] as usize)]).collect(),
        None => (0..pos.len() / 3).map(|k| [p(k * 3), p(k * 3 + 1), p(k * 3 + 2)]).collect(),
    }
}

/// Points every receiver (stage and fighter materials) at the stage volume
/// buffer: the real grid once built, the empty one otherwise. Only touches an
/// asset whose handle differs.
fn sync_volume_buffer(grid: Option<Res<VolumeGrid>>, mut stage: ResMut<Assets<StageMaterial>>, mut fighters: ResMut<Assets<crate::cam_fade::DissolveMaterial>>) {
    let want = grid.map(|g| g.buffer.clone()).unwrap_or_else(volume_buffer);
    let ids: Vec<_> = stage.iter().filter(|(_, m)| m.extension.vol != want).map(|(id, _)| id).collect();
    for id in ids {
        if let Some(mut m) = stage.get_mut(id) {
            m.extension.vol = want.clone();
        }
    }
    let ids: Vec<_> = fighters.iter().filter(|(_, m)| m.extension.vol != want).map(|(id, _)| id).collect();
    for id in ids {
        if let Some(mut m) = fighters.get_mut(id) {
            m.extension.vol = want.clone();
        }
    }
}

/// Is this entity (or its parent: Bevy names the node entity and calls the
/// primitive's own entity "Mesh") a shadow-volume node?
fn is_volume_node(e: Entity, names: &Query<&Name>, parents: &Query<&ChildOf>) -> bool {
    let tagged = |e: Entity| names.get(e).is_ok_and(|n| n.as_str().contains(pd_formats::gltf::VOLUME_TAG));
    tagged(e) || parents.get(e).is_ok_and(|p| tagged(p.parent()))
}

/// Stage meshes, once: shadow volumes go on the hidden caster layer; every
/// other piece swaps its `StandardMaterial` for the receiving `StageMaterial`
/// (same base material, plus the shadow blend). The sky shells keep theirs.
#[allow(clippy::too_many_arguments)]
fn prepare_stage_meshes(
    mut commands: Commands,
    stage: Res<ShadowStage>,
    new: Query<Entity, Added<Mesh3d>>,
    current: Query<(&Mesh3d, Option<&MeshMaterial3d<StandardMaterial>>, Option<&GltfMaterialExtras>)>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<StageRoot>>,
    names: Query<&Name>,
    mesh_assets: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    mut stage_mats: ResMut<Assets<StageMaterial>>,
    mut live: ResMut<crate::stage_material::StageLightLive>,
    grid: Option<Res<VolumeGrid>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut shared: Local<HashMap<AssetId<StandardMaterial>, Handle<StageMaterial>>>,
    mut pending: Local<Vec<Entity>>,
) {
    if stage.is_changed() {
        shared.clear();
    }
    pending.extend(new.iter());
    pending.sort_unstable();
    pending.dedup();
    let todo = std::mem::take(&mut *pending);
    let to_light = -stage.look.stage_dir;
    // The stage volume grid (a node the glb carries, never drawn): build it
    // first, so the volumes it replaces are parked below and the materials
    // created in this pass bind it.
    let mut grid_handle = grid.as_ref().map(|g| g.buffer.clone());
    if grid_enabled() {
        for &e in &todo {
            let Ok((mesh, _, _)) = current.get(e) else { continue };
            if !is_named(e, &names, &parents, |n| n == pd_formats::gltf::GRID_NODE) || !parents.iter_ancestors(e).any(|a| roots.get(a).is_ok()) {
                continue;
            }
            commands.entity(e).insert((Visibility::Hidden, RenderLayers::layer(INERT_LAYER)));
            let Some(mesh) = mesh_assets.get(&mesh.0) else { continue };
            let tris = mesh_triangles(mesh);
            match shadow_grid::build(&tris) {
                Some(words) => {
                    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                    let handle = buffers.add(ShaderBuffer::new(&bytes, RenderAssetUsages::default()));
                    eprintln!("[shadows] volume grid: {} triangles, {} KiB", tris.len(), bytes.len() / 1024);
                    commands.insert_resource(VolumeGrid { buffer: handle.clone() });
                    grid_handle = Some(handle);
                }
                None => eprintln!("[shadows] volume grid node has no usable triangles"),
            }
        }
    }
    let volume = grid_handle.clone().unwrap_or_else(volume_buffer);
    if !grid_enabled() {
        // Never drawn either way.
        for &e in &todo {
            if is_named(e, &names, &parents, |n| n == pd_formats::gltf::GRID_NODE) {
                commands.entity(e).insert((Visibility::Hidden, RenderLayers::layer(INERT_LAYER)));
            }
        }
    }
    for e in todo {
        let Ok((mesh, material, extras)) = current.get(e) else { continue }; // despawned meanwhile
        if !parents.iter_ancestors(e).any(|a| roots.get(a).is_ok()) {
            continue;
        }
        if is_named(e, &names, &parents, |n| n == pd_formats::gltf::GRID_NODE) {
            continue; // handled above
        }
        if is_volume_node(e, &names, &parents) {
            // The stage own volumes (`shadowvol_m<model>_d<draw>`) are in the
            // grid, exactly: they do not cast through the shadow map as well.
            // A placed object volumes (`<obj>_shadowvol_d<draw>`) still do.
            let stage_own = is_named(e, &names, &parents, |n| n.starts_with(pd_formats::gltf::VOLUME_TAG));
            let layer = if stage_own && grid_handle.is_some() { INERT_LAYER } else { VOLUME_LAYER };
            commands.entity(e).insert(RenderLayers::layer(layer));
            continue;
        }
        let Some(material) = material else { continue };
        let Some(base) = materials.get(&material.0) else {
            pending.push(e); // material asset still streaming
            continue;
        };
        if mesh_assets.get(&mesh.0).and_then(|m| m.compute_aabb()).is_some_and(|b| b.half_extents.length() * 2.0 >= SHELL_EXTENT) {
            continue; // the sky shells keep their own material (sky_scroll)
        }
        // A bump-mapped draw's own constants ride in the glb material's extras
        // (`gltf::Materials::get_stage_bump`); every other draw has none.
        let bump = extras.and_then(|x| mat_from_extras(&x.value));
        let light = live.0.unwrap_or(stage.light);
        crate::stage_material::write_light(&mut live, &mut buffers, light);
        let handle = shared.entry(material.0.id()).or_insert_with(|| stage_mats.add(stage_material(base.clone(), to_light, bump, volume.clone()))).clone();
        commands.entity(e).insert(MeshMaterial3d(handle)).remove::<MeshMaterial3d<StandardMaterial>>();
    }
}

/// A character scene whose glb carries the authored shadow hulls
/// (`pd_formats::character::HULL_TAG`): its hulls are the casters, not its
/// body pieces. Put on the glTF scene root.
#[derive(Component)]
struct HasHulls;

/// `PD_SHADOW_HULLS=0`: fighters cast from their body meshes again (lane L2).
pub fn hulls_enabled() -> bool {
    !crate::stage_look::legacy_light() && std::env::var("PD_SHADOW_HULLS").map_or(true, |v| v != "0")
}

/// The nearest ancestor that is a glTF scene root (a character, a stage).
fn scene_root(e: Entity, parents: &Query<&ChildOf>, roots: &Query<(), With<WorldAssetRoot>>) -> Option<Entity> {
    std::iter::once(e).chain(parents.iter_ancestors(e)).find(|a| roots.get(*a).is_ok())
}

/// Authored fighter hulls (child nodes of the character glb, never drawn):
/// they join the fighters' caster layer only, and their scene is marked so its
/// body pieces stop casting. With `PD_SHADOW_HULLS=0` they are parked.
fn tag_fighter_hulls(
    mut commands: Commands,
    new: Query<Entity, Added<Mesh3d>>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<WorldAssetRoot>>,
    pieces: Query<(Entity, &RenderLayers), With<MeshMaterial3d<crate::cam_fade::DissolveMaterial>>>,
    fresh: Query<Entity, Added<HasHulls>>,
) {
    let mut marked: Vec<Entity> = Vec::new();
    for e in &new {
        if !is_named(e, &names, &parents, |n| n.starts_with(pd_formats::character::HULL_TAG)) {
            continue;
        }
        if !hulls_enabled() {
            commands.entity(e).insert((Visibility::Hidden, RenderLayers::layer(INERT_LAYER)));
            continue;
        }
        commands.entity(e).insert(RenderLayers::layer(FIGHTER_LAYER));
        if let Some(root) = scene_root(e, &parents, &roots) {
            if !marked.contains(&root) {
                marked.push(root);
                commands.entity(root).insert(HasHulls);
            }
        }
    }
    // A scene whose pieces were already tagged as casters before its hulls
    // appeared: take them back off the caster layer (the frame after the marker lands).
    let fresh: Vec<Entity> = fresh.iter().collect();
    if !fresh.is_empty() {
        for (e, layers) in &pieces {
            if scene_root(e, &parents, &roots).is_some_and(|r| fresh.contains(&r)) && layers.intersects(&RenderLayers::layer(FIGHTER_LAYER)) {
                commands.entity(e).insert(layers.clone().without(FIGHTER_LAYER));
            }
        }
    }
}

/// Fighters' body pieces (the dissolve material, `cam_fade.rs`) join the
/// fighters' caster layer, keeping layer 0 so every camera still draws them
/// -- unless their character glb carries the authored hulls, which cast
/// instead (`tag_fighter_hulls`).
fn tag_fighter_casters(
    mut commands: Commands,
    new: Query<(Entity, Option<&RenderLayers>), Added<MeshMaterial3d<crate::cam_fade::DissolveMaterial>>>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<WorldAssetRoot>>,
    hulled: Query<(), With<HasHulls>>,
) {
    for (e, layers) in &new {
        if scene_root(e, &parents, &roots).is_some_and(|r| hulled.get(r).is_ok()) {
            continue;
        }
        let layers = layers.cloned().unwrap_or_default().with(FIGHTER_LAYER);
        if std::env::var_os("PD_SHADOW_LOG").is_some() {
            eprintln!("[shadows] fighter piece {e} -> layers {layers:?}");
        }
        commands.entity(e).insert(layers);
    }
}

/// `PD_FRAMETIME=<from>,<to>` (proof harness): vsync off, and the real frame
/// time between app frames `from`..`to` (mean / p50 / p95, ms) on stderr.
fn log_frame_time(time: Res<Time<Real>>, mut samples: Local<(u32, Vec<f32>)>) {
    let v = std::env::var("PD_FRAMETIME").unwrap_or_default();
    let n: Vec<u32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
    let (from, to) = (n.first().copied().unwrap_or(200), n.get(1).copied().unwrap_or(500));
    samples.0 += 1;
    if samples.0 > from && samples.0 <= to {
        samples.1.push(time.delta_secs() * 1000.0);
    }
    if samples.0 == to {
        let mut s = samples.1.clone();
        s.sort_by(f32::total_cmp);
        let mean = s.iter().sum::<f32>() / s.len().max(1) as f32;
        eprintln!("[frametime] frames {from}..{to}: mean {mean:.2} ms  p50 {:.2}  p95 {:.2}  ({} frames)", s[s.len() / 2], s[s.len() * 95 / 100], s.len());
    }
}

/// `PD_SHADOW_LOG=1`: what each shadow light sees (caster counts per cascade).
fn log_lights(lights: Query<(&DirectionalLight, &RenderLayers, &bevy::camera::visibility::CascadesVisibleEntities)>, mut n: Local<u32>) {
    *n += 1;
    if *n % 60 != 0 {
        return;
    }
    for (l, layers, vis) in &lights {
        let per: Vec<Vec<usize>> = vis.entities.values().map(|v| v.iter().map(|c| c.entities.len()).collect()).collect();
        eprintln!("[shadows] light shadows {} layers {layers:?} casters per view/cascade {per:?}", l.shadow_maps_enabled);
    }
}

/// The shadow filter of the original look: crisp 2x2 hardware (the volumes were
/// pixel-exact), or `PD_SHADOW_FILTER=gaussian`. The "Shadow quality: High"
/// option (`look.rs`) uses the Gaussian one.
pub fn original_filter() -> ShadowFilteringMethod {
    match std::env::var("PD_SHADOW_FILTER").as_deref() {
        Ok("gaussian") => ShadowFilteringMethod::Gaussian,
        _ => ShadowFilteringMethod::Hardware2x2,
    }
}

/// The player cameras list the shadow lights (layer) and filter crisply.
///
/// ADDS `CAMERA_LAYER` to the camera's own mask (world, own things, other
/// seats' capsule ghosts, `fx::camera_layers`); it used to REPLACE it with
/// `[0, 1 + view, CAMERA_LAYER]`, which dropped every ghost layer and, with the
/// ghost layers numbered 5..8 (6 = this layer), made seat 2's camera draw its
/// own capsules in the opponent's look (BUG-GRAVEYARD, capsulep2).
fn tag_cameras(mut commands: Commands, new: Query<(Entity, &PlayerCamera, Option<&RenderLayers>), Added<PlayerCamera>>) {
    let filter = original_filter();
    for (e, cam, own) in &new {
        let layers = own.cloned().unwrap_or_else(|| RenderLayers::from_layers(&[0, 1 + cam.0])).with(CAMERA_LAYER);
        commands.entity(e).insert((layers, filter));
    }
}

// ---------------------------------------------------------------------------
// The original's maths, ported (tested below against the disassembly).
// ---------------------------------------------------------------------------

/// `SSHADOW_vs` / `sshadowch_vs` (extrusion of a volume vertex), instruction
/// by instruction, in the program's model space:
///
/// ```text
/// dp3 r0.x, v1.xyz, -cb0[4].xyz        ; N . -DL   (DL = light travel direction)
/// lt  r0.x, r0.x, cb0[0].x             ; back-facing? (c0.x = 0)
/// and r0.x, r0.x, 1.0
/// mul r0.yzw, cb0[4].xyz, cb0[4].w     ; DL * length
/// mad r0.xyz, r0.x, r0.yzw, v0.xyz     ; pos + flag * DL * length
/// ```
///
/// (`sshadowch_vs` first adds `DL.xyz` to the position: a one-unit bias along
/// the light, `bias = true`.) `length` is `cb0[4].w`, written per draw from
/// the draw record's float at +0x3c (`FUN_1402a3bd0`); its value is not read
/// from the files yet, so the game does not extrude (see docs/SHADOWS.md).
#[cfg_attr(not(test), allow(dead_code))]
pub fn extrude_vertex(pos: Vec3, normal: Vec3, dl_dir: Vec3, dl_length: f32, c0_x: f32, bias: bool) -> Vec3 {
    let back_facing = normal.dot(-dl_dir) < c0_x;
    let base = if bias { pos + dl_dir } else { pos };
    if back_facing {
        base + dl_dir * dl_length
    } else {
        base
    }
}

/// The alpha the surface shaders write (`SD_ps`, `SBTS_ps`, `SWBOS_ps`): the
/// most the shadow blend can darken a pixel. Mirrors `shadows.wgsl::shadow_weight`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn shadow_weight(n_dot_l: f32, tex_alpha: f32) -> f32 {
    let a = 2.0 * (n_dot_l.clamp(0.0, 1.0) * 0.8 + 0.00806) * (2.0 * tex_alpha - 1.5);
    a.clamp(0.0, 1.0)
}

/// The shadow pass's blend, one channel, in gamma space:
/// `C * k + dst * (1 - k)` (blend factors DEST_ALPHA, INV_DEST_ALPHA with
/// dest alpha = `k`). `k = 0` (outside every volume) returns `dst` exactly.
#[cfg_attr(not(test), allow(dead_code))]
pub fn shadow_blend(dst: f32, colour: f32, k: f32) -> f32 {
    if k <= 0.0 {
        dst
    } else {
        colour * k + dst * (1.0 - k)
    }
}

#[cfg(test)]
mod tests {
    /// One and two views keep every cascade; three and four views keep two per light (Laurence's
    /// pick, 2026-10-09); with the trim off (`PD_SHADOW_CASCADES_TRIM=0`) every count keeps all.
    #[test]
    fn cascades_trim_from_three_views_only() {
        for views in 1..=2 {
            assert_eq!(super::cascades_with(views, 4, true), 4, "{views} views, stage light");
            assert_eq!(super::cascades_with(views, 3, true), 3, "{views} views, fighter light");
        }
        for views in 3..=4 {
            assert_eq!(super::cascades_with(views, 4, true), 2, "{views} views, stage light");
            assert_eq!(super::cascades_with(views, 3, true), 2, "{views} views, fighter light");
        }
        for views in 1..=4 {
            assert_eq!(super::cascades_with(views, 4, false), 4);
            assert_eq!(super::cascades_with(views, 3, false), 3);
        }
        // The full set is what spawn_lights builds.
        assert_eq!(super::config_with(4, 25.0, 700.0).bounds, CascadeShadowConfigBuilder { num_cascades: 4, minimum_distance: 0.1, first_cascade_far_bound: 25.0, maximum_distance: 700.0, overlap_proportion: 0.2 }.build().bounds);
    }

    use super::*;

    /// capsulep2 (Laurence: "Player 2's orbs look like opponent orbs"): the
    /// camera mask AFTER `tag_cameras` must still be `camera_layers` + the
    /// light layer, for every seat of 2-4 players: sees its own capsule layer,
    /// every OTHER seat's ghost layer, never its own ghost layer. The old
    /// system replaced the mask and ghost layer 6 collided with `CAMERA_LAYER`.
    #[test]
    fn tagged_cameras_keep_own_orbs_own_and_see_others_as_ghosts() {
        use crate::fx::{camera_layers, GHOST_LAYER_BASE};
        for players in 2..=crate::combat::MAX_PLAYERS {
            let mut app = App::new();
            app.add_systems(Update, tag_cameras);
            let cams: Vec<Entity> = (0..players)
                .map(|v| app.world_mut().spawn((PlayerCamera(v), camera_layers(v, players))).id())
                .collect();
            app.update();
            for (v, &e) in cams.iter().enumerate() {
                let mask = app.world().get::<RenderLayers>(e).unwrap().clone();
                assert!(mask.intersects(&RenderLayers::layer(CAMERA_LAYER)), "seat {v} lists the shadow lights");
                for owner in 0..players {
                    let own = mask.intersects(&RenderLayers::layer(1 + owner));
                    let ghost = mask.intersects(&RenderLayers::layer(GHOST_LAYER_BASE + owner));
                    assert_eq!((own, ghost), (owner == v, owner != v), "{players} players, camera {v}, capsule of {owner}");
                }
            }
        }
    }

    /// The capsule ghost layers share a number with nothing else.
    #[test]
    fn layer_numbers_never_collide() {
        use crate::aura_glow::{GHOST_LAYER, MASK_LAYER, QUAD_LAYER};
        use crate::fx::GHOST_LAYER_BASE;
        let n = crate::combat::MAX_PLAYERS;
        let mut others = vec![0, CAMERA_LAYER, VOLUME_LAYER, FIGHTER_LAYER, INERT_LAYER, MASK_LAYER];
        others.extend(1..=n);
        others.extend(GHOST_LAYER..GHOST_LAYER + n);
        others.extend(QUAD_LAYER..QUAD_LAYER + 3 * 4);
        for j in 0..n {
            assert!(!others.contains(&(GHOST_LAYER_BASE + j)), "capsule ghost layer {} collides", GHOST_LAYER_BASE + j);
        }
    }

    /// st03's script, as cached: stage/characters direction, backdrop colour.
    fn st03() -> serde_json::Value {
        // As `pd_convert` writes it now: vectors in the program's order
        // (the file lists st03's direction as (-0.4925, -0.6773, 0.5461); the
        // program's x is the third float).
        serde_json::json!({
            "vec_order": "zyx",
            "backdrop": [15, 18, 25],
            "stage": { "dir": [0.5460720062255859, -0.6772599816322327, -0.4925040006637573] },
            "characters": { "dir": [0.5460720062255859, -0.6772599816322327, -0.4925040006637573] },
        })
    }

    #[test]
    fn look_reads_direction_and_shadow_colour() {
        let l = ShadowLook::from_json(&st03()).unwrap();
        // Game space is left-handed: Z mirrors.
        assert!((l.stage_dir - Vec3::new(0.54607, -0.67726, 0.4925)).length() < 1e-3);
        // An older cache's json (no vec_order tag) holds file order: same result.
        let old = serde_json::json!({
            "backdrop": [15, 18, 25],
            "stage": { "dir": [-0.4925040006637573, -0.6772599816322327, 0.5460720062255859] },
            "characters": { "dir": [-0.4925040006637573, -0.6772599816322327, 0.5460720062255859] },
        });
        assert!((ShadowLook::from_json(&old).unwrap().stage_dir - l.stage_dir).length() < 1e-6);
        assert_eq!(l.stage_dir, l.fighter_dir);
        // call 02 sub 1 (st03 0x0f1219).
        assert!((l.colour - Vec3::new(15.0, 18.0, 25.0) / 255.0).length() < 1e-6);
    }

    #[test]
    fn stage_and_fighter_lights_differ_where_the_script_says() {
        // st07: stage (0.873,-0.388,-0.295), characters (0.727,-0.649,-0.223)
        // in the script's file order.
        let v = serde_json::json!({
            "backdrop": [3, 0, 14],
            "stage": { "dir": [0.873, -0.388, -0.295] },
            "characters": { "dir": [0.727, -0.649, -0.223] },
        });
        let l = ShadowLook::from_json(&v).unwrap();
        assert!(l.stage_dir.dot(l.fighter_dir) < 0.98);
        assert_eq!(l.colour, Vec3::new(3.0, 0.0, 14.0) / 255.0);
    }

    #[test]
    fn missing_script_gives_black_shadows() {
        assert_eq!(ShadowLook::default().colour, Vec3::ZERO);
        assert!(ShadowLook::from_json(&serde_json::json!({})).is_none());
    }

    /// `SSHADOW_vs`: a vertex whose normal faces away from the light
    /// (N . -DL < c0.x = 0) moves by DL * length; one facing it stays.
    #[test]
    fn extrusion_moves_only_back_facing_vertices_along_the_light_by_the_length() {
        let dl = Vec3::new(-0.4925, -0.6773, 0.5461).normalize();
        let p = Vec3::new(10.0, 20.0, 30.0);
        // Normal along the light travel direction: N . -DL = -1 < 0, back-facing.
        let moved = extrude_vertex(p, dl, dl, 100.0, 0.0, false);
        assert!((moved - (p + dl * 100.0)).length() < 1e-4);
        assert!(((moved - p).length() - 100.0).abs() < 1e-3, "length is exactly DL.w for a unit direction");
        // Normal towards the light: N . -DL = 1, front-facing, unmoved.
        assert_eq!(extrude_vertex(p, -dl, dl, 100.0, 0.0, false), p);
        // Exactly perpendicular: `lt` is strict, so it stays.
        let perp = dl.any_orthonormal_vector();
        assert_eq!(extrude_vertex(p, perp, dl, 100.0, 0.0, false), p);
    }

    /// `sshadowch_vs` adds DL.xyz to every vertex first (the one-unit bias).
    #[test]
    fn fighter_volumes_are_biased_one_unit_along_the_light() {
        let dl = Vec3::new(0.0, -1.0, 0.0);
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(extrude_vertex(p, -dl, dl, 50.0, 0.0, true), p + dl);
        assert_eq!(extrude_vertex(p, dl, dl, 50.0, 0.0, true), p + dl + dl * 50.0);
    }

    /// `SD_ps`: A = 2*(0.8*NL+0.00806)*(2a-1.5); a = 1 gives 0.8*NL + 0.00806.
    #[test]
    fn shadow_weight_is_the_framebuffer_alpha_of_the_surface_shaders() {
        assert!((shadow_weight(1.0, 1.0) - 0.80806).abs() < 1e-6);
        // st03's floor: N.L = 0.6773 -> A = 0.5499.
        assert!((shadow_weight(0.6773, 1.0) - 0.54990).abs() < 1e-4);
        // Facing away: only the +0.00806 is left.
        assert!((shadow_weight(-0.5, 1.0) - 0.00806).abs() < 1e-6);
        // Alpha-test edge: texture alpha 0.75 writes alpha 0 (the shader's own cut).
        assert_eq!(shadow_weight(1.0, 0.75), 0.0);
    }

    /// Outside every volume nothing changes at all: no global dimming.
    #[test]
    fn shadow_term_is_identity_outside_volumes() {
        for dst in [0.0, 0.013, 0.4, 0.999, 1.0, 1.7] {
            assert_eq!(shadow_blend(dst, 0.06, 0.0).to_bits(), dst.to_bits());
        }
        // Inside with the st03 floor weight: 45% of the lit colour plus a little of C.
        let a = shadow_weight(0.6773, 1.0);
        let lit = 0.5;
        let s = shadow_blend(lit, 15.0 / 255.0, a);
        assert!((s - (lit * (1.0 - a) + (15.0 / 255.0) * a)).abs() < 1e-7);
        assert!(s < lit && s > 0.2);
    }

    #[test]
    fn caster_layers_do_not_collide_with_the_views() {
        // Views use 0 and 1 + player (fx.rs::view_layer); the lane's are above.
        for l in [CAMERA_LAYER, VOLUME_LAYER, FIGHTER_LAYER] {
            assert!(l > 2);
        }
        let cam = RenderLayers::from_layers(&[0, 1, CAMERA_LAYER]);
        let stage_light = RenderLayers::from_layers(&[CAMERA_LAYER, VOLUME_LAYER]);
        let fighter_light = RenderLayers::from_layers(&[CAMERA_LAYER, FIGHTER_LAYER]);
        assert!(cam.intersects(&stage_light) && cam.intersects(&fighter_light), "a view lists both lights");
        let stage_mesh = RenderLayers::layer(0);
        let volume = RenderLayers::layer(VOLUME_LAYER);
        let fighter = RenderLayers::from_layers(&[0, FIGHTER_LAYER]);
        assert!(!stage_light.intersects(&stage_mesh), "stage meshes are not casters");
        assert!(stage_light.intersects(&volume) && !fighter_light.intersects(&volume));
        assert!(fighter_light.intersects(&fighter) && !stage_light.intersects(&fighter));
        assert!(!cam.intersects(&volume), "no camera draws a volume");
    }
}
