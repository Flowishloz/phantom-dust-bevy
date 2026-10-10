//! The school aura's glow: the halo that hugs a fighter with aura (lane auralook, 2026-10-01).
//!
//! **What the original does** (read from `PDUWP.exe`, `decomp_all.c`; docs/FORMATS-EFFECTS.md
//! "School aura glow"). Not effect 600's wisps: the HUD's aura object (vtable `0x14047b578`:
//! init `FUN_14019aa30`, update `FUN_14019b520`, draw `FUN_14019b920`; scripts
//! `System/auradefault.ssb`, models `System/aura.alr`) renders every aura-on fighter's own model in one
//! flat school colour into a small target, blurs it, and adds it back to the view behind and around the
//! body. Per view, per frame:
//!
//! 1. **Silhouette** (`FUN_14019b920`, `FUN_14019c3d0`, shader `SAURA`): the fighter's model, flat colour
//!    (script table `0x20..0x54` of `auradefault.ssb`, row `0x3c + 4 x school`: Psycho `(160, 80, 80)`,
//!    Optical `(120, 120, 80)`, Nature `(80, 120, 80)`, Ki `(80, 80, 160)`, Faith `(128, 80, 120)`, none
//!    grey 120), into a 256 x 256 tile cleared to 0. The projection is the view's own, but with clip x / y
//!    multiplied by `0.02112 x w` and the fighter's own clip position subtracted, so the fighter is drawn
//!    at a fixed scale about its feet: NDC x = `0.02112 x z0 x P00 (x - x0) / z`, NDC y =
//!    `(0.02112 x z0 x P11 (y - y0) - 0.625 z0) / z` with P00 = cot(37.5 deg), P11 = 853 / 480 P00
//!    (`FUN_140291570` forces 853 x 480 and a 75 deg field for any wide view).
//! 2. **Downsample** (`FUN_140286c60`, `MFilter`): 4 taps, weights 0.2007 / 0.1496 / 0.2007 / 0.4489,
//!    offsets (-1,-3) (3,-3) (-3,1) (1,1) texels, to 128 x 128.
//! 3. **Blur** (`FUN_140287460`, `GFilter2`, vertex tables `0x14048e990` ..): 8 taps horizontally, 8 taps
//!    vertically, weights `exp(T_k / w) / sum` with T_k the triangular numbers + 0.25 and w = script f36 /
//!    100 (a Gaussian of sigma sqrt(w) texels); the first vertical draw is blended onto the previous frame's
//!    result times the persistence (script f35 / 255; state `0xd101` = ONE, CONSTANT_ALPHA), the second added.
//! 4. **Composite** (`FUN_140289140`, shaders `AuraBox`): the tile as a camera-facing quad at the fighter,
//!    `2 H` wide (H = 37.6 tan 37.5 deg = 28.85 u) and from `-0.281 H` to `1.219 H` high, showing the tile's
//!    central 92.5 % x 95 %, vertex colour = grey (model opacity squared / 256), blend `0x1201` (screen +=
//!    colour squared), stencil NOT EQUAL to the fighter's own body (`FUN_14019c980`, refs `40 60 c0 e0`).
//!
//! The script (`auradefault.ssb`, run by the update each logic tick, 30 Hz) sets the persistence and width
//! from the Level tier -- (50, 300) (80, 400) (100, 500) (120, 600) (120, 700) for tiers 1..5, tier =
//! `(Level + 3) / 4` -- and, for 30 ticks after an Aura Particle use (state 8), `(30 - t) x 5 + 60` and
//! `(30 - t) x 5 + 500`. See [`script_params`].
//!
//! **Here** (render-only, never writes the sim): per (view, fighter) a chain of four cameras rendering into
//! 8-bit images -- the silhouette (flat-colour copies of the body's meshes sharing its skin, on a private
//! layer, through a custom projection), `MFilter`, the horizontal and the vertical blur (`aura_glow_filter.wgsl`,
//! the previous vertical result kept by ping-pong) -- run once per sim tick, and a quad in the view's private layer
//! (`aura_glow_comp.wgsl`). The body is left out of the glow the way the program does it, by a per-view render of
//! the fighters' own pixels (codes 1..4, nearest wins) standing in for the stencil buffer ([`MaskView`]).

use std::collections::HashMap;

use bevy::asset::{Asset, RenderAssetUsages};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::{CameraProjection, ClearColorConfig, Projection, RenderTarget, SubCameraView};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::math::Vec3A;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureFormat};
use bevy::render::view::Msaa;
use bevy::shader::{Shader, ShaderRef};

use crate::aura_fx::AuraFxState;
use crate::aura_pulse::PulseGhost;
use crate::cam_fade::DissolveMaterial;
use crate::combat::MAX_PLAYERS;
use crate::frontend::state::{AppState, MatchScene};
use crate::school_aura::{aura_level, school_index};
use crate::{FxInstance, PlayerCamera, PlayerVisual, World};

static FILTER_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();
static COMP_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();
static FLAT_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();

// -- what the program does, as numbers (READ: `PDUWP.exe` addresses in the module doc) --------------

/// The silhouette tile (`FUN_14019c3d0`: viewport 256 x 256) and the filtered tile (`FUN_140286c60`).
pub const THUMB: u32 = 256;
pub const TILE: u32 = 128;
/// `_DAT_1402f7c8c`: the fixed-scale factor of the silhouette projection.
pub const THUMB_K: f32 = 0.021_120_001;
/// `DAT_1402f7d4c`: the fighter's feet sit this far below the tile centre, in NDC.
pub const FEET_NDC: f32 = 0.625;
/// The projection the silhouette (and the composite's size) assume: a 75 degree field (`DAT_1402f7df4`,
/// 1.309; half = 0.6545), 853 x 480 (`DAT_1402f8208` / `DAT_1402f8198`, forced for any wide view).
pub const HALF_FOV: f32 = 0.6545;
pub const ASPECT: f32 = 853.0 / 480.0;
/// `0x1402f7ed0`: the composite's half width is this times tan(half field) (`FUN_140289140`).
pub const BOARD_SCALE: f32 = 37.6;
/// The composite's vertical extent in half widths: `(0.625 H +- H) x 640 / 853` (`DAT_1402f81f4`, `DAT_1402f8208`).
pub fn board_y() -> (f32, f32) {
    let k = 640.0 / 853.0;
    ((FEET_NDC - 1.0) * k, (FEET_NDC + 1.0) * k)
}
/// The tile's visible part: u 0.0375 .. 0.9625, v 0.025 .. 0.975 (`param_4 x 0.0375`, `x 0.925`, `x 0.025`, `x 0.95`).
pub const CROP_U: (f32, f32) = (0.0375, 0.9625);
pub const CROP_V: (f32, f32) = (0.025, 0.975);
/// The script's own colour table (`auradefault.ssb` row `0x3c`), ARGB bytes by school index
/// (0 none, 1 Psycho .. 5 Faith).
pub const SILHOUETTE_RGB: [[u8; 3]; 6] = [[120, 120, 120], [160, 80, 80], [120, 120, 80], [80, 120, 80], [80, 80, 160], [128, 80, 120]];

/// What the script gives the draw each logic tick: whether the silhouette is drawn (`f34`), the persistence
/// (`f35` / 255) and the blur width term (`f36` x 0.01).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowParams {
    pub draw: bool,
    pub persist: f32,
    pub width: f32,
}

/// `auradefault.ssb`: `func_0398` (tier dispatch), `func_041c` (state 8, an Aura Particle use), run by `auraMove`
/// each logic tick. `pulse` = the ticks since a state-8 start while it is under 30 (`t`), else None.
pub fn script_params(level: i32, pulse: Option<u32>) -> GlowParams {
    if let Some(t) = pulse.filter(|t| *t < 30) {
        let v = (30 - t) as f32;
        return GlowParams { draw: true, persist: (v * 5.0 + 60.0) / 255.0, width: (v * 5.0 + 500.0) * 0.01 };
    }
    // f39 = Level; (Level + 3) / 4, capped at 5.
    let tier = ((level + 3) / 4).clamp(0, 5);
    let (f35, f36) = match tier {
        0 => return GlowParams { draw: false, persist: 100.0 / 255.0, width: 3.0 },
        1 => (50.0, 300.0),
        2 => (80.0, 400.0),
        3 => (100.0, 500.0),
        4 => (120.0, 600.0),
        _ => (120.0, 700.0),
    };
    GlowParams { draw: true, persist: f35 / 255.0, width: f36 * 0.01 }
}

/// `FUN_1402864e0`: the eight blur weights for width term `w` (`DAT_14048f090` = -(T_k + 0.25) in the program's
/// order), normalised to sum 1.
#[allow(dead_code)]
pub fn blur_weights(w: f32) -> [f32; 8] {
    const TABLE: [f32; 8] = [-1.25, -6.25, -15.25, -28.25, -0.25, -3.25, -10.25, -21.25];
    let mut out = [0.0; 8];
    let mut sum = 0.0;
    for (o, t) in out.iter_mut().zip(TABLE) {
        *o = (t / w).exp();
        sum += *o;
    }
    out.map(|x| x / sum)
}

/// The silhouette's clip transform for a fighter whose feet are at `origin` in view space (x right, y up, the
/// camera looking down -z): `FUN_14019c3d0`'s columns scaled by `0.02112 x z0` and the fighter's own clip
/// position taken off, then 0.625 z0 down. `w` stays the depth; clip z is half of it (no depth to speak of).
pub fn thumb_clip_from_view(origin: Vec3) -> Mat4 {
    let z0 = -origin.z;
    let cot = 1.0 / HALF_FOV.tan();
    let ax = THUMB_K * z0 * cot;
    let ay = THUMB_K * z0 * cot * ASPECT;
    Mat4::from_cols(Vec4::new(ax, 0.0, 0.0, 0.0), Vec4::new(0.0, ay, 0.0, 0.0), Vec4::new(0.0, 0.0, -0.5, -1.0), Vec4::new(-ax * origin.x, -ay * origin.y - FEET_NDC * z0, 0.0, 0.0))
}

/// The silhouette camera's projection: a ready matrix.
#[derive(Clone, Debug)]
struct ThumbProjection {
    clip_from_view: Mat4,
}

impl CameraProjection for ThumbProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        self.clip_from_view
    }
    fn get_clip_from_view_for_sub(&self, _: &SubCameraView) -> Mat4 {
        self.clip_from_view
    }
    fn update(&mut self, _: f32, _: f32) {}
    fn far(&self) -> f32 {
        1.0e6
    }
    fn get_frustum_corners(&self, _: f32, _: f32) -> [Vec3A; 8] {
        [Vec3A::ZERO; 8]
    }
}

// -- materials ---------------------------------------------------------------------------------------

/// One filter pass (`aura_glow_filter.wgsl`).
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct GlowFilter {
    /// x = mode (0 downsample, 1 horizontal blur, 2 vertical blur), y = width term, z = persistence.
    #[uniform(0)]
    pub params: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub src: Option<Handle<Image>>,
    #[texture(3)]
    pub prev: Option<Handle<Image>>,
}

impl Material for GlowFilter {
    fn vertex_shader() -> ShaderRef {
        FILTER_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn fragment_shader() -> ShaderRef {
        FILTER_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
    fn enable_shadows() -> bool {
        false
    }
    fn enable_prepass() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// A flat colour (`aura_glow_flat.wgsl`): the silhouette copies of the body.
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct GlowFlat {
    #[uniform(0)]
    pub colour: Vec4,
}

impl Material for GlowFlat {
    fn fragment_shader() -> ShaderRef {
        FLAT_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
    fn enable_shadows() -> bool {
        false
    }
    fn enable_prepass() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The composite quad (`aura_glow_comp.wgsl`).
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct GlowComposite {
    /// x = the vertex grey (model opacity squared / 256, 0..1), y = the fighter's body code (1..4).
    #[uniform(0)]
    pub comp: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub tile: Option<Handle<Image>>,
    /// The view's body codes (`MaskView`).
    #[texture(3)]
    pub mask: Option<Handle<Image>>,
}

impl Material for GlowComposite {
    fn fragment_shader() -> ShaderRef {
        COMP_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
    fn enable_shadows() -> bool {
        false
    }
    fn enable_prepass() -> bool {
        false
    }
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

// -- state -------------------------------------------------------------------------------------------

/// A silhouette copy of one body mesh, for fighter `slot`.
#[derive(Component)]
struct GlowGhost {
    slot: usize,
}

/// One (view, fighter) pipeline.
#[allow(dead_code)] // handles kept so the images live as long as the chain
struct Chain {
    thumb: Handle<Image>,
    ds: Handle<Image>,
    hb: Handle<Image>,
    v: [Handle<Image>; 2],
    /// Which `v` the vertical blur writes next; the other is last tick's.
    next: usize,
    /// Cameras: silhouette, downsample, horizontal, vertical.
    cams: [Entity; 4],
    filters: [Handle<GlowFilter>; 3],
    comp_mat: Handle<GlowComposite>,
    board: Entity,
    last_tick: u64,
    idle: u32,
    shown: bool,
}

/// A view's body codes: every fighter's own pixels, drawn with the main camera's own transform / projection into an
/// image of the view's size, flat, red = (fighter + 1) / 8, the nearest fighter winning -- what the program's
/// stencil buffer holds (the characters' own draws write it; `FUN_14019c980` tests it).
struct MaskView {
    image: Handle<Image>,
    cam: Entity,
    size: UVec2,
}

#[derive(Resource, Default)]
struct GlowState {
    chains: HashMap<(usize, usize), Chain>,
    /// Per fighter: the root the ghosts were built for, the ghost material, the colour written to it.
    built_for: [Option<Entity>; MAX_PLAYERS],
    material: [Handle<GlowFlat>; MAX_PLAYERS],
    colour: [Option<usize>; MAX_PLAYERS],
    meshes: usize,
    /// Per fighter: the flat body-code material (code / 8 in red) of the mask copies.
    mask_material: [Handle<GlowFlat>; MAX_PLAYERS],
    /// Per view: the body-code render.
    masks: HashMap<usize, MaskView>,
    board_mesh: Option<Handle<Mesh>>,
    quad_mesh: Option<Handle<Mesh>>,
}

pub fn plugin(app: &mut App) {
    let mut shaders = app.world_mut().resource_mut::<Assets<Shader>>();
    let _ = FILTER_SHADER.set(shaders.add(Shader::from_wgsl(include_str!("aura_glow_filter.wgsl"), "pd_game/aura_glow_filter.wgsl")));
    let _ = COMP_SHADER.set(shaders.add(Shader::from_wgsl(include_str!("aura_glow_comp.wgsl"), "pd_game/aura_glow_comp.wgsl")));
    let _ = FLAT_SHADER.set(shaders.add(Shader::from_wgsl(include_str!("aura_glow_flat.wgsl"), "pd_game/aura_glow_flat.wgsl")));
    app.add_plugins((MaterialPlugin::<GlowFilter>::default(), MaterialPlugin::<GlowComposite>::default(), MaterialPlugin::<GlowFlat>::default()));
    app.init_resource::<GlowState>();
    app.add_systems(Update, build_ghosts.run_if(in_state(AppState::Match)));
    app.add_systems(PostUpdate, drive.after(bevy::transform::TransformSystems::Propagate).run_if(in_state(AppState::Match)));
}

/// Off by default: a live trace showed the original never shows this object's target in the versus
/// picture (skipping `FUN_14019b920` from boot leaves the halo unchanged; MOVE-RECORDER.md
/// "Elimination of the halo's source"). `PD_AURA_GLOW=1` turns it on for A/B.
fn enabled() -> bool {
    std::env::var("PD_AURA_GLOW").map_or(false, |v| v != "0")
}

/// Layers: the silhouettes of fighter `f` are on `GHOST_LAYER + f`, a chain's filter quads on `QUAD_LAYER + n`;
/// the composite lives on the view's private layer `1 + view`.
pub(crate) const GHOST_LAYER: usize = 16;
pub(crate) const QUAD_LAYER: usize = 24;
/// The camera orders (all below the match cameras, 0..).
const ORDER_BASE: isize = -1000;
/// The layer every fighter's body-code copies share, and the mask cameras' order.
pub(crate) const MASK_LAYER: usize = 12;
const MASK_ORDER: isize = -1100;

/// Build each fighter's silhouette copies once (and again for a new match's new root): one flat child per body
/// mesh, sharing the piece's mesh and skin joints, on the fighter's private layer.
#[allow(clippy::too_many_arguments)]
fn build_ghosts(
    mut commands: Commands,
    mut state: ResMut<GlowState>,
    mut flats: ResMut<Assets<GlowFlat>>,
    materials: Res<Assets<StandardMaterial>>,
    visuals: Query<(Entity, &PlayerVisual)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    fx_roots: Query<(), With<FxInstance>>,
    pieces: Query<(&Mesh3d, Option<&SkinnedMesh>, Option<&RenderLayers>, Option<&MeshMaterial3d<DissolveMaterial>>, Option<&MeshMaterial3d<StandardMaterial>>), (Without<GlowGhost>, Without<PulseGhost>)>,
    old: Query<(Entity, &GlowGhost)>,
    dissolve: Res<Assets<DissolveMaterial>>,
    ) {
    if !enabled() {
        return;
    }
    for (root, vis) in &visuals {
        let slot = vis.0;
        if slot >= MAX_PLAYERS || state.built_for[slot] == Some(root) {
            continue;
        }
        // Wait for the glTF scene to spawn its meshes.
        let meshes: Vec<(Entity, Mesh3d, Option<SkinnedMesh>)> = children
            .iter_descendants(root)
            .filter_map(|e| {
                let (m, skin, layers, dis, std) = pieces.get(e).ok()?;
                // Effect meshes under the fighter are not the body.
                if parents.iter_ancestors(e).any(|a| fx_roots.get(a).is_ok()) {
                    return None;
                }
                // The authored shadow hulls sit on a caster-only layer no camera draws.
                if layers.is_some_and(|l| !l.intersects(&RenderLayers::layer(0))) {
                    return None;
                }
                // The body is Opaque or alpha-cutout (Mask); blended and additive pieces are effects.
                let mode = match (dis.and_then(|m| dissolve.get(&m.0)), std.and_then(|m| materials.get(&m.0))) {
                    (Some(m), _) => Some(m.base.alpha_mode),
                    (_, Some(m)) => Some(m.alpha_mode),
                    _ => None,
                };
                if !matches!(mode, Some(AlphaMode::Opaque | AlphaMode::Mask(_))) {
                    return None;
                }
                Some((e, m.clone(), skin.cloned()))
            })
            .collect();
        if meshes.is_empty() {
            continue;
        }
        for (g, ghost) in &old {
            if ghost.slot == slot {
                commands.entity(g).despawn();
            }
        }
        let handle = flats.add(GlowFlat { colour: Vec4::new(0.0, 0.0, 0.0, 1.0) });
        state.material[slot] = handle.clone();
        state.colour[slot] = None;
        let code = flats.add(GlowFlat { colour: Vec4::new((slot + 1) as f32 / 8.0, 0.0, 0.0, 1.0) });
        state.mask_material[slot] = code.clone();
        for (e, mesh, skin) in meshes {
            // A skinned piece is posed by its joints alone, so its copy hangs from the fighter's root; a rigid
            // one rides its piece.
            let mut c = commands.spawn((Mesh3d(mesh.0.clone()), MeshMaterial3d(handle.clone()), Transform::IDENTITY, Visibility::Inherited, NoFrustumCulling, RenderLayers::layer(GHOST_LAYER + slot), GlowGhost { slot }, ChildOf(if skin.is_some() { root } else { e })));
            if let Some(skin) = &skin {
                c.insert(skin.clone());
            }
            let mut m = commands.spawn((Mesh3d(mesh.0.clone()), MeshMaterial3d(code.clone()), Transform::IDENTITY, Visibility::Inherited, NoFrustumCulling, RenderLayers::layer(MASK_LAYER), GlowGhost { slot }, ChildOf(if skin.is_some() { root } else { e })));
            if let Some(skin) = skin {
                m.insert(skin);
            }
            state.meshes += 1;
        }
        state.built_for[slot] = Some(root);
        if std::env::var_os("PD_GLOW_LOG").is_some() {
            eprintln!("[aura_glow] fighter {slot}: silhouette copies built");
        }
    }
}

fn target_image(images: &mut Assets<Image>, size: u32) -> Handle<Image> {
    images.add(Image::new_target_texture(size, size, TextureFormat::Rgba8Unorm, None))
}

/// The view's body-code image and its camera (made on first use, remade when the view's size changes).
fn ensure_mask(state: &mut GlowState, commands: &mut Commands, images: &mut Assets<Image>, view: usize, size: UVec2) -> (Handle<Image>, Entity) {
    if let Some(m) = state.masks.get(&view) {
        if m.size == size {
            return (m.image.clone(), m.cam);
        }
    }
    let image = images.add(Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8Unorm, None));
    match state.masks.get_mut(&view) {
        Some(m) => {
            m.image = image.clone();
            m.size = size;
            commands.entity(m.cam).insert(RenderTarget::from(image.clone()));
            (image, m.cam)
        }
        None => {
            let cam = commands
                .spawn((
                    Camera3d::default(),
                    Camera { order: MASK_ORDER + view as isize, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                    RenderTarget::from(image.clone()),
                    Projection::Perspective(PerspectiveProjection::default()),
                    Transform::default(),
                    RenderLayers::layer(MASK_LAYER),
                    Msaa::Off,
                    Tonemapping::None,
                    DebandDither::Disabled,
                    MatchScene,
                ))
                .id();
            state.masks.insert(view, MaskView { image: image.clone(), cam, size });
            (image, cam)
        }
    }
}

fn quad_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-1.0f32, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]]);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 4]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
    m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    m
}

/// The composite quad in units of H: x from -1 to 1, y from `board_y().0` to `.1`, showing the tile's crop.
fn board_mesh() -> Mesh {
    let (y0, y1) = board_y();
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-1.0f32, y0, 0.0], [1.0, y0, 0.0], [1.0, y1, 0.0], [-1.0, y1, 0.0]]);
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 4]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[CROP_U.0, CROP_V.1], [CROP_U.1, CROP_V.1], [CROP_U.1, CROP_V.0], [CROP_U.0, CROP_V.0]]);
    m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    m
}

/// Everything per frame: the script's parameters, the silhouette cameras' projections, the filter passes, the quads.
#[allow(clippy::too_many_arguments)]
fn drive(
    mut commands: Commands,
    mut state: ResMut<GlowState>,
    world: Option<Res<World>>,
    aura: Option<Res<AuraFxState>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut flats: ResMut<Assets<GlowFlat>>,
    mut filters: ResMut<Assets<GlowFilter>>,
    mut comps: ResMut<Assets<GlowComposite>>,
    visuals: Query<(Entity, &PlayerVisual, &GlobalTransform)>,
    cams: Query<(&PlayerCamera, &GlobalTransform, &Camera, &Projection)>,
    mut cameras: Query<&mut Camera, Without<PlayerCamera>>,
    mut xforms: Query<(&mut Transform, &mut GlobalTransform), (Without<PlayerCamera>, Without<PlayerVisual>)>,
    mut projections: Query<&mut Projection, Without<PlayerCamera>>,
    mut visibility: Query<&mut Visibility>,
) {
    if !enabled() {
        return;
    }
    let Some(world) = world else { return };
    let combat = &world.0.combat;
    let tick = world.0.tick;
    let log = std::env::var_os("PD_GLOW_LOG").is_some();
    let level_override = crate::aura_fx::test_level();
    // The chains of a finished match: their cameras are gone.
    state.chains.retain(|_, c| cameras.get(c.cams[0]).is_ok());
    state.masks.retain(|_, m| cameras.get(m.cam).is_ok());
    // Meshes shared by every chain.
    if state.quad_mesh.is_none() {
        state.quad_mesh = Some(meshes.add(quad_mesh()));
        state.board_mesh = Some(meshes.add(board_mesh()));
    }
    let (quad, board) = (state.quad_mesh.clone().unwrap(), state.board_mesh.clone().unwrap());
    for f in 0..MAX_PLAYERS {
        let Some(fighter) = combat.fighters.get(f) else { continue };
        let Some((_, _, fgt)) = visuals.iter().find(|(_, v, _)| v.0 == f) else { continue };
        let level = aura_level(level_override.unwrap_or(fighter.level), fighter.status.invisible_ticks > 0);
        // An Aura Particle use: state 8 for 30 logic ticks (60 sim ticks).
        let p = fighter.absorb_age().map(|a| u32::from(a) / 2);
        let params = script_params(level, p);
        // The silhouette colour: the fighter's school, the table's row.
        let school = school_index(&combat.arsenals[f], &combat.skills);
        if state.colour[f] != Some(school) {
            if let Some(mut m) = flats.get_mut(&state.material[f]) {
                let [r, g, b] = SILHOUETTE_RGB[school.min(5)];
                m.colour = Vec4::new(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 1.0);
                state.colour[f] = Some(school);
            }
        }
        let feet = fgt.translation();
        for (view, cgt, main_cam, main_proj) in cams.iter().map(|(c, g, cm, pr)| (c.0, g, cm, pr)) {
            if view >= MAX_PLAYERS {
                continue;
            }
            let key = (view, f);
            let ready = state.built_for[f].is_some();
            // Start a chain the first tick the fighter's aura draws.
            if !state.chains.contains_key(&key) {
                if !(params.draw && ready) {
                    continue;
                }
                let n = view * MAX_PLAYERS + f;
                let order = ORDER_BASE + 4 * n as isize;
                let thumb = target_image(&mut images, THUMB);
                let ds = target_image(&mut images, TILE);
                let hb = target_image(&mut images, TILE);
                let v = [target_image(&mut images, TILE), target_image(&mut images, TILE)];
                let filter = |mode: f32, src: &Handle<Image>, prev: Option<Handle<Image>>, filters: &mut Assets<GlowFilter>| filters.add(GlowFilter { params: Vec4::new(mode, 3.0, 0.0, 0.0), src: Some(src.clone()), prev });
                let fs = [filter(0.0, &thumb, None, &mut filters), filter(1.0, &ds, None, &mut filters), filter(2.0, &hb, Some(v[0].clone()), &mut filters)];
                let cam = |commands: &mut Commands, order: isize, target: &Handle<Image>, layer: usize, proj: Projection| {
                    commands
                        .spawn((
                            Camera3d::default(),
                            Camera { order, clear_color: ClearColorConfig::Custom(Color::NONE), is_active: false, ..default() },
                            RenderTarget::from(target.clone()),
                            proj,
                            Transform::default(),
                            RenderLayers::layer(layer),
                            Msaa::Off,
                            Tonemapping::None,
                            DebandDither::Disabled,
                            MatchScene,
                        ))
                        .id()
                };
                let c0 = cam(&mut commands, order, &thumb, GHOST_LAYER + f, Projection::custom(ThumbProjection { clip_from_view: Mat4::IDENTITY }));
                let flat_proj = || Projection::Perspective(PerspectiveProjection::default());
                let c1 = cam(&mut commands, order + 1, &ds, QUAD_LAYER + n * 3, flat_proj());
                let c2 = cam(&mut commands, order + 2, &hb, QUAD_LAYER + n * 3 + 1, flat_proj());
                let c3 = cam(&mut commands, order + 3, &v[1], QUAD_LAYER + n * 3 + 2, flat_proj());
                for (k, fh) in fs.iter().enumerate() {
                    // Each pass's quad on a layer only its own camera shows.
                    commands.spawn((Mesh3d(quad.clone()), MeshMaterial3d(fh.clone()), Transform::IDENTITY, Visibility::Inherited, NoFrustumCulling, RenderLayers::layer(QUAD_LAYER + n * 3 + k), MatchScene));
                }
                let comp_mat = comps.add(GlowComposite { comp: Vec4::new(1.0, (f + 1) as f32, 0.0, 0.0), tile: Some(v[1].clone()), mask: None });
                let board_entity = commands.spawn((Mesh3d(board.clone()), MeshMaterial3d(comp_mat.clone()), Transform::IDENTITY, Visibility::Hidden, NoFrustumCulling, RenderLayers::layer(1 + view), MatchScene)).id();
                state.chains.insert(key, Chain { thumb, ds, hb, v, next: 1, cams: [c0, c1, c2, c3], filters: fs, comp_mat, board: board_entity, last_tick: u64::MAX, idle: 0, shown: false });
                if log {
                    eprintln!("[aura_glow] tick {tick} chain view {view} fighter {f}");
                }
            }
            if !state.chains.contains_key(&key) {
                continue;
            }
            // The view's body codes, drawn the way this view's camera sees them.
            let size = main_cam.physical_viewport_size().unwrap_or(UVec2::new(1280, 720)).max(UVec2::splat(16));
            let (mask_image, mask_cam) = ensure_mask(&mut state, &mut commands, &mut images, view, size);
            if let Ok((mut t, mut g)) = xforms.get_mut(mask_cam) {
                *t = Transform::from_matrix(cgt.to_matrix());
                *g = *cgt;
            }
            if let Ok(mut p) = projections.get_mut(mask_cam) {
                *p = main_proj.clone();
            }
            let Some(chain) = state.chains.get_mut(&key) else { continue };
            if let Some(mut m) = comps.get_mut(&chain.comp_mat) {
                if m.mask.as_ref() != Some(&mask_image) {
                    m.mask = Some(mask_image);
                }
            }
            // One step per sim tick.
            let step = chain.last_tick != tick;
            if step {
                chain.last_tick = tick;
                chain.idle = if params.draw { 0 } else { chain.idle + 1 };
            }
            let running = step && chain.idle < 120;
            let n = view * MAX_PLAYERS + f;
            for (k, &c) in chain.cams.iter().enumerate() {
                if let Ok(mut cm) = cameras.get_mut(c) {
                    // Silhouette only while the script draws; the filters while anything is left in them.
                    let on = running && (k != 0 || params.draw);
                    if cm.is_active != on {
                        cm.is_active = on;
                    }
                }
            }
            // The view-space position of the fighter's feet.
            let view_from_world = cgt.affine().inverse();
            let o = view_from_world.transform_point3(feet);
            if o.z >= -0.5 {
                // Behind or at the camera: nothing to show.
                if let Ok(mut v) = visibility.get_mut(chain.board) {
                    *v = Visibility::Hidden;
                }
                continue;
            }
            if running {
                if let Ok((mut t, mut g)) = xforms.get_mut(chain.cams[0]) {
                    *t = Transform::from_matrix(cgt.to_matrix());
                    *g = *cgt;
                }
                for &c in &chain.cams[1..] {
                    if let Ok((mut t, mut g)) = xforms.get_mut(c) {
                        *t = Transform::IDENTITY;
                        *g = GlobalTransform::IDENTITY;
                    }
                }
                if let Ok(mut p) = projections.get_mut(chain.cams[0]) {
                    *p = Projection::custom(ThumbProjection { clip_from_view: thumb_clip_from_view(o) });
                }
                // The filter parameters; the vertical pass reads last tick's result and writes the other image.
                let prev = chain.v[chain.next ^ 1].clone();
                let w = params.width;
                if let Some(mut m) = filters.get_mut(&chain.filters[0]) {
                    m.params = Vec4::new(0.0, w, 0.0, 0.0);
                }
                if let Some(mut m) = filters.get_mut(&chain.filters[1]) {
                    m.params = Vec4::new(1.0, w, 0.0, 0.0);
                }
                if let Some(mut m) = filters.get_mut(&chain.filters[2]) {
                    m.params = Vec4::new(2.0, w, params.persist, 0.0);
                    m.prev = Some(prev);
                }
                // The last camera writes into `v[next]`.
                let target = chain.v[chain.next].clone();
                commands.entity(chain.cams[3]).insert(RenderTarget::from(target.clone()));
                if let Some(mut m) = comps.get_mut(&chain.comp_mat) {
                    m.tile = Some(target);
                }
                chain.next ^= 1;
                chain.shown = true;
            }
            let _ = n;
            // The composite quad: camera-facing, centred on the feet, upright.
            if chain.shown {
                let h = BOARD_SCALE * HALF_FOV.tan();
                let right = cgt.right().as_vec3();
                let up = Vec3::Y;
                let z = right.cross(up).normalize_or_zero();
                if let Ok((mut t, mut g)) = xforms.get_mut(chain.board) {
                    let rot = Quat::from_mat3(&Mat3::from_cols(right, up, z));
                    *t = Transform { translation: feet, rotation: rot, scale: Vec3::splat(h) };
                    *g = GlobalTransform::from(*t);
                }
                // The model opacity as the program feeds it: squared / 256.
                let cover = aura.as_ref().map_or(1.0, |a| a.coverage(f, view));
                let op = (cover * 255.0).round().clamp(0.0, 255.0);
                let grey = (op * op / 256.0).floor() / 255.0;
                if let Some(mut m) = comps.get_mut(&chain.comp_mat) {
                    m.comp.x = grey;
                    m.comp.z = if std::env::var_os("PD_GLOW_DEBUG").is_some() { 1.0 } else { 0.0 };
                }
                if let Ok(mut v) = visibility.get_mut(chain.board) {
                    *v = if chain.idle < 120 { Visibility::Inherited } else { Visibility::Hidden };
                }
            }
        }
    }
    // A view's body-code render only runs while one of its chains still shows something.
    let live: Vec<usize> = state.chains.iter().filter(|(_, c)| c.shown && c.idle < 120).map(|(k, _)| k.0).collect();
    for (view, m) in &state.masks {
        if let Ok(mut c) = cameras.get_mut(m.cam) {
            let on = live.contains(view);
            if c.is_active != on {
                c.is_active = on;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tier table of `auradefault.ssb` and the Aura Particle pulse (`func_041c`).
    #[test]
    fn the_script_gives_the_levels_tier_and_the_pulse() {
        assert!(!script_params(0, None).draw, "Level 0 draws nothing");
        for (level, f35, f36) in [(1, 50.0, 300.0), (4, 50.0, 300.0), (5, 80.0, 400.0), (9, 100.0, 500.0), (13, 120.0, 600.0), (17, 120.0, 700.0), (60, 120.0, 700.0)] {
            let p = script_params(level, None);
            assert!(p.draw);
            assert!((p.persist - f35 / 255.0).abs() < 1e-6, "Level {level}");
            assert!((p.width - f36 * 0.01).abs() < 1e-5, "Level {level}");
        }
        let first = script_params(0, Some(0));
        assert!(first.draw && (first.persist - 210.0 / 255.0).abs() < 1e-6 && (first.width - 6.5).abs() < 1e-5);
        let last = script_params(0, Some(29));
        assert!((last.persist - 65.0 / 255.0).abs() < 1e-6 && (last.width - 5.05).abs() < 1e-5);
        assert!(!script_params(0, Some(30)).draw, "after 30 ticks the tier takes over");
    }

    #[test]
    fn the_blur_weights_are_a_normalised_gaussian() {
        for w in [3.0, 5.0, 6.5] {
            let k = blur_weights(w);
            assert!((k.iter().sum::<f32>() - 1.0).abs() < 1e-5);
            // Group A (indices 0..4) sits at 1.5, 3.5, 5.5, 7.5 texels, group B at 0.5, 2.5, 4.5, 6.5.
            let at = |x: f32| (-(x * x / 2.0 + 0.125) / w).exp();
            let total: f32 = [1.5, 3.5, 5.5, 7.5, 0.5, 2.5, 4.5, 6.5].iter().map(|&x| at(x)).sum();
            for (i, x) in [1.5f32, 3.5, 5.5, 7.5, 0.5, 2.5, 4.5, 6.5].iter().enumerate() {
                assert!((k[i] - at(*x) / total).abs() < 1e-5, "w {w} tap {i}");
            }
        }
        // A wider term is a wider blur: the outer taps gain, the inner ones lose.
        assert!(blur_weights(6.5)[3] > blur_weights(3.0)[3]);
        assert!(blur_weights(6.5)[4] < blur_weights(3.0)[4]);
    }

    /// The silhouette's projection puts the feet 0.625 below the tile centre and a 20 u body about half
    /// the tile's height; a point to the side moves 0.0275 NDC per unit, a point above 0.0489.
    #[test]
    fn the_silhouette_tile_is_a_fixed_scale_about_the_feet() {
        let project = |m: Mat4, p: Vec3| {
            let c = m * p.extend(1.0);
            Vec2::new(c.x / c.w, c.y / c.w)
        };
        for z in [10.0f32, 25.0, 60.0] {
            let origin = Vec3::new(3.0, -4.0, -z);
            let m = thumb_clip_from_view(origin);
            let feet = project(m, origin);
            assert!(feet.x.abs() < 1e-5 && (feet.y + 0.625).abs() < 1e-5, "feet at z {z}: {feet:?}");
            let head = project(m, origin + Vec3::new(0.0, 20.0, 0.0));
            assert!((head.y - (-0.625 + 20.0 * 0.0489)).abs() < 0.002, "head {head:?}");
            let side = project(m, origin + Vec3::new(10.0, 0.0, 0.0));
            assert!((side.x - 0.2752).abs() < 0.002, "10 u to the right {side:?}");
        }
    }

    /// The composite's size: H = 37.6 tan 37.5 deg, from -0.281 H to 1.219 H, 1.5005 H tall.
    #[test]
    fn the_composite_board_is_28_85_wide_each_way() {
        let h = BOARD_SCALE * HALF_FOV.tan();
        assert!((h - 28.85).abs() < 0.05, "{h}");
        let (y0, y1) = board_y();
        assert!((y0 + 0.28136).abs() < 1e-4 && (y1 - 1.21922).abs() < 1e-4);
        assert!(((y1 - y0) - 1.5006).abs() < 1e-3);
    }

    /// The silhouette colours are the script's own table.
    #[test]
    fn the_silhouette_colours_are_the_scripts() {
        assert_eq!(SILHOUETTE_RGB[1], [160, 80, 80]);
        assert_eq!(SILHOUETTE_RGB[3], [80, 120, 80]);
        assert_eq!(SILHOUETTE_RGB[0], [120, 120, 120]);
    }
}
