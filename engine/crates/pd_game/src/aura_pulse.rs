//! The green body glow when a fighter uses an Aura Particle.
//!
//! **What the original shows** (Laurence's footage `Orb Collect & Aura
//! Consumed.mp4`, 30 fps, frames 163-186; docs/CHAR-LIGHT.md "Aura Particle
//! glow"): the moment the Aura Particle is used (Level 0 -> 1) the whole body
//! is wrapped in a bright pale-green outline glow, brightest hugging the
//! silhouette and fading to nothing about 1.5-2 body-widths out (a tail that
//! also spills onto the floor and walls behind). It pops in at about three
//! quarters of its strength on the first frame, peaks over frames 166-169,
//! then fades out over the next ~17 frames, gone by 186-187 (frame 160 = the press, the
//! Level sound and the HUD's Level 00 -> 01; the glow starts 2.6 frames later). The body's own colours are
//! not tinted. Capturing the capsule shows no glow (the orb ends in a few gold
//! sparks).
//!
//! **What the program does** (read, `decomp_all.c`; see the doc section for
//! the addresses): the use fires skill record 0 (`0x140258180` ->
//! `0x1402626d0`), whose action row 343 names effect 0 and no clip, so no
//! effect is started by the row; the Level +1 reaches `FUN_140270b00` ->
//! `FUN_140271590`, which only plays the Level sound. The tier change starts
//! effect 600 slot 0 (`FUN_14019e6c0`), but those are four hand / wrist
//! wisp meshes (`eft0600.fx.json`), not a body outline. No code that draws a
//! green body halo was found. What the program does have is a flat-colour
//! skinless draw with a screen-space offset (`SAURA_vs`/`SAURA_ps`), the
//! building block of exactly this look; the halo here is built from it.
//!
//! **How it is drawn here** (render-only, never writes the sim): each body
//! piece of the fighter gets [`GHOST_COUNT`] invisible "ghost" children that
//! share its mesh and skin joints; while the use's glow runs they are drawn
//! additively in flat green, only the side of each piece that faces away from
//! the camera, each shifted up to a unit sideways in screen space
//! (`aura_pulse.wgsl`) on a spiral of [`GHOST_COUNT`] points, and pushed back
//! so the body hides them. Stacked, they make a soft halo whose profile is
//! fitted to the footage. Ghosts are created lazily the first time
//! a fighter uses an Aura Particle. The glow runs on its own counter
//! (`Fighter::absorb_age`, sim ticks since the press, `combat::AURA_GLOW_TICKS`), so it
//! does not depend on whether the use holds the fighter; see [`envelope`].

use std::collections::HashMap;

use bevy::asset::{Asset, AssetId};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, VertexAttributeValues};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};

use crate::cam_fade::DissolveMaterial;
use crate::combat::MAX_PLAYERS;
use crate::{env_f32, FxInstance, PlayerVisual, World};

static SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();

/// Ghosts per body piece: points on a golden-angle spiral over a disc of
/// [`DISC_RADIUS_U`], denser toward the middle (radius = R x t^1.4,
/// t = (k + 0.5) / N), each adding [`ghost`]'s weight of green light. Stacked
/// they make a halo whose profile along a straight edge is the fit to the
/// footage (docs/CHAR-LIGHT.md "Aura Particle glow"): about 1.4 (clipped)
/// light at the silhouette, falling off with a 0.28 u scale length in linear
/// light (0.43 u in display space; the fighter is about 17 u tall, the
/// footage's halo reaches about a tenth of the body height), nothing past
/// 1.2 u.
pub const GHOST_COUNT: usize = 64;
/// Radius of the disc the ghosts are spread over (world units at the body's
/// depth); `PD_AP_WIDTH` scales it for calibration.
pub const DISC_RADIUS_U: f32 = 1.2;
/// Width scale applied to the fitted disc (`PD_AP_WIDTH` overrides): the first
/// fit read narrower than the footage next to it (halo reach 14% of the torso
/// width against the footage's 23%), so the whole profile is stretched, by eye.
pub const WIDTH_DEFAULT: f32 = 1.3;
/// Spiral radius exponent (1 = even over the disc, larger = denser near the
/// middle).
const RADIUS_POWER: f32 = 1.4;
/// Linear-light weight of ghost groups of 8 (centres at ghosts 3.5, 11.5 ..),
/// interpolated between centres: a non-negative least-squares fit of the
/// stacked profile to `(1.25 e^(-d / 0.43))^1.5`, rms 0.03 (64 ghosts: 48 left the outermost copies of the limbs showing as steps).
const GROUP_WEIGHTS: [f32; 8] = [0.0185, 0.0586, 0.0622, 0.0696, 0.0541, 0.0329, 0.0144, 0.0277];
/// How far back along the view ray each ghost is pushed (world units): enough
/// that a ghost's far side stays behind the body's own surface (a shifted copy
/// of a rounded chest would otherwise show over the shoulder; 0.45 u did),
/// little enough to still draw over the floor and walls round the feet.
pub const PUSH_BACK_U: f32 = 2.0;
/// The glow colour as the footage's added light, display-space ratios
/// (R : G : B = 0.52 : 1 : 0.46, measured on the unclipped pixels of frame 167).
pub const TINT_DISPLAY: [f32; 3] = [0.52, 1.0, 0.46];
/// Display-space light to scene-linear light for the dim parts of the glow
/// over the brown stages (fitted at 0.1..0.5 display delta over a ~0.15 grey
/// background: `v^1.5`); the envelope and the tint go through it.
pub const DISPLAY_TO_LINEAR: f32 = 1.5;

/// Ticks between the press (the Level sound, the HUD Level 00 -> 01, frame 160 of the
/// footage) and the first visible glow (frame 162.5): 2.6 frames, 5 sim ticks.
pub const GLOW_DELAY_TICKS: u16 = 5;

/// Glow strength (0..1) `age` sim ticks after the press (`Fighter::absorb_age`): nothing
/// for the first [`GLOW_DELAY_TICKS`], then the footage's own frame-by-frame envelope,
/// read as the visual brightness of frames 163-187 (`aura_pulse_footage_envelope_*`
/// tests): about three quarters on the first frame, full over frames 166-169, then a fade
/// to nothing by frame 186.5. 2 sim ticks = 1 footage frame.
pub fn envelope(age: u16) -> f32 {
    const KEYS: [(f32, f32); 12] =
        [(0.0, 0.75), (2.0, 0.78), (3.0, 1.0), (6.0, 1.0), (7.0, 0.85), (10.0, 0.65), (13.0, 0.45), (16.0, 0.30), (19.0, 0.14), (21.0, 0.07), (22.5, 0.03), (23.5, 0.0)];
    let Some(since) = age.checked_sub(GLOW_DELAY_TICKS) else { return 0.0 };
    let f = since as f32 / 2.0;
    if f >= KEYS[KEYS.len() - 1].0 {
        return 0.0;
    }
    for w in KEYS.windows(2) {
        let ((f0, v0), (f1, v1)) = (w[0], w[1]);
        if f <= f1 {
            return v0 + (v1 - v0) * ((f - f0) / (f1 - f0)).clamp(0.0, 1.0);
        }
    }
    0.0
}

/// Direction (unit, camera plane), shift (world units at the body's depth, with
/// `PD_AP_WIDTH` = 1) and scene-linear weight of ghost `k` (`0..GHOST_COUNT`).
pub fn ghost(k: usize) -> (Vec2, f32, f32) {
    const GOLDEN: f32 = 2.399_963_2;
    let t = (k as f32 + 0.5) / GHOST_COUNT as f32;
    let r = DISC_RADIUS_U * t.powf(RADIUS_POWER);
    let a = k as f32 * GOLDEN;
    // Group weights, linearly interpolated between the group centres.
    let x = ((k as f32 - 3.5) / 8.0).clamp(0.0, (GROUP_WEIGHTS.len() - 1) as f32);
    let (i, f) = (x.floor() as usize, x.fract());
    let w = GROUP_WEIGHTS[i] + (GROUP_WEIGHTS[(i + 1).min(GROUP_WEIGHTS.len() - 1)] - GROUP_WEIGHTS[i]) * f;
    (Vec2::new(a.cos(), a.sin()), r, w)
}

/// One ghost's material: the light it adds, where it sits, and the piece's own
/// colour texture (only its alpha is read, to keep the authored cut-outs).
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct PulseMaterial {
    #[uniform(0)]
    pub glow: Vec4,
    #[uniform(1)]
    pub shift: Vec4,
    #[texture(2)]
    #[sampler(3)]
    pub tex: Option<Handle<Image>>,
}

impl Material for PulseMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
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
    /// Culling off: front or back is decided in the shader from the piece's own
    /// outward normals, so a mirrored winding cannot flip it.
    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

pub fn plugin(app: &mut App) {
    let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("aura_pulse.wgsl"), "pd_game/aura_pulse.wgsl"));
    let _ = SHADER.set(shader);
    app.add_plugins(MaterialPlugin::<PulseMaterial>::default());
    app.init_resource::<PulseAssets>();
    app.add_systems(Update, drive);
}

/// A ghost of a body piece, for fighter `slot`.
#[derive(Component)]
pub(crate) struct PulseGhost {
    slot: usize,
}

/// Per fighter: the ghost materials (one set of [`GHOST_COUNT`] per colour
/// texture, so a piece's cut-outs are its own) and which `PlayerVisual` root
/// the ghosts were built for (a new match builds a new root, and so new ghosts).
#[derive(Resource, Default)]
pub(crate) struct PulseAssets {
    /// (ghost number, material) of every ghost material of the fighter.
    mats: [Vec<(usize, Handle<PulseMaterial>)>; MAX_PLAYERS],
    by_tex: [HashMap<Option<AssetId<Image>>, Vec<Handle<PulseMaterial>>>; MAX_PLAYERS],
    built_for: [Option<Entity>; MAX_PLAYERS],
    /// The ghost copy of each body mesh (outward normals rebuilt).
    meshes: HashMap<AssetId<Mesh>, Handle<Mesh>>,
    /// Last strength written per fighter (skips redundant material writes).
    last: [f32; MAX_PLAYERS],
}

fn material(k: usize, glow: Vec4, tex: Option<Handle<Image>>) -> PulseMaterial {
    let (dir, r, _) = ghost(k);
    PulseMaterial { glow, shift: Vec4::new(dir.x, dir.y, r * env_f32("PD_AP_WIDTH", WIDTH_DEFAULT), env_f32("PD_AP_PUSH", PUSH_BACK_U)), tex }
}

/// The light every ghost of a fighter adds at `strength` (0..1), scene-linear.
pub fn glow_for(strength: f32, k: usize, gain: f32) -> Vec4 {
    let (_, _, w) = ghost(k);
    let s = strength.clamp(0.0, 1.0).powf(DISPLAY_TO_LINEAR) * w * gain;
    Vec4::new(TINT_DISPLAY[0].powf(DISPLAY_TO_LINEAR) * s, TINT_DISPLAY[1].powf(DISPLAY_TO_LINEAR) * s, TINT_DISPLAY[2].powf(DISPLAY_TO_LINEAR) * s, 0.0)
}

/// Every frame: for each fighter whose Aura Particle use is running, make sure
/// its body pieces have ghosts, light them by the envelope, and show them;
/// hide them otherwise. Reads the sim (`Fighter::absorb_age`) only.
#[allow(clippy::too_many_arguments)]
fn drive(
    world: Option<Res<World>>,
    mut commands: Commands,
    mut assets: ResMut<PulseAssets>,
    mut materials: ResMut<Assets<PulseMaterial>>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    visuals: Query<(Entity, &PlayerVisual)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    fx_roots: Query<(), With<FxInstance>>,
    pieces: Query<(&Mesh3d, Option<&SkinnedMesh>, Option<&RenderLayers>, Option<&MeshMaterial3d<DissolveMaterial>>, Option<&MeshMaterial3d<StandardMaterial>>), Without<PulseGhost>>,
    dissolve: Res<Assets<DissolveMaterial>>,
    standard: Res<Assets<StandardMaterial>>,
    mut ghosts: Query<(&PulseGhost, &mut Visibility)>,
) {
    let Some(world) = world else { return };
    // Superseded by `aura_glow.rs`, which draws the program's own glow (this was an approximation of it):
    // `PD_AP_LEGACY=1` brings the old ghosts back for A/B.
    if std::env::var_os("PD_AP_LEGACY").is_none() {
        return;
    }
    let gain = env_f32("PD_AP_GAIN", 1.0);
    let force = std::env::var("PD_AP_FORCE").ok().and_then(|v| v.parse::<u16>().ok());
    let log = std::env::var_os("PD_AP_LOG").is_some();
    for slot in 0..MAX_PLAYERS {
        let age = force.filter(|_| slot == 0).or_else(|| world.0.combat.fighters.get(slot).and_then(|f| f.absorb_age()));
        let strength = age.map_or(0.0, envelope);
        if log && age.is_some() {
            eprintln!("[aura_pulse] tick {} slot {slot} age {age:?} strength {strength:.2}", world.0.tick);
        }
        if strength > 0.0 {
            // First use (or a new match): build the ghosts.
            if let Some(root) = visuals.iter().find(|(_, v)| v.0 == slot).map(|(e, _)| e) {
                if assets.built_for[slot] != Some(root) {
                    assets.mats[slot].clear();
                    assets.by_tex[slot].clear();
                    let mut made = 0;
                    for e in children.iter_descendants(root) {
                        let Ok((mesh, skin, layers, dis, std)) = pieces.get(e) else { continue };
                        // Effect meshes under the fighter are not the body.
                        if parents.iter_ancestors(e).any(|a| fx_roots.get(a).is_ok()) {
                            continue;
                        }
                        // The authored shadow hulls ride the fighter too but sit
                        // on a caster-only layer (shadows.rs); no camera draws them.
                        if layers.is_some_and(|l| !l.intersects(&RenderLayers::layer(0))) {
                            continue;
                        }
                        // The body is Opaque or alpha-cutout (Mask); blended and
                        // additive pieces are effects.
                        let (mode, tex) = match (dis.and_then(|m| dissolve.get(&m.0)), std.and_then(|m| standard.get(&m.0))) {
                            (Some(m), _) => (Some(m.base.alpha_mode), m.base.base_color_texture.clone()),
                            (_, Some(m)) => (Some(m.alpha_mode), m.base_color_texture.clone()),
                            _ => (None, None),
                        };
                        if !matches!(mode, Some(AlphaMode::Opaque | AlphaMode::Mask(_))) {
                            continue;
                        }
                        let ghost_mesh = match assets.meshes.get(&mesh.0.id()) {
                            Some(h) => h.clone(),
                            None => {
                                let Some(m) = mesh_assets.get(&mesh.0).and_then(ghost_mesh) else { continue };
                                let h = mesh_assets.add(m);
                                assets.meshes.insert(mesh.0.id(), h.clone());
                                h
                            }
                        };
                        let key = tex.as_ref().map(|t| t.id());
                        let handles = match assets.by_tex[slot].get(&key) {
                            Some(h) => h.clone(),
                            None => {
                                let h: Vec<Handle<PulseMaterial>> = (0..GHOST_COUNT).map(|k| materials.add(material(k, Vec4::ZERO, tex.clone()))).collect();
                                assets.mats[slot].extend(h.iter().cloned().enumerate());
                                assets.by_tex[slot].insert(key, h.clone());
                                h
                            }
                        };
                        // Layer 0 spelled out: the fighter's own pieces carry a
                        // caster layer too, and a child would otherwise inherit
                        // nothing a camera lists. A skinned piece is posed by its
                        // joints alone, so its ghosts hang from the fighter's root
                        // (a piece entity has no visibility of its own to
                        // inherit); a rigid one rides its piece.
                        for h in &handles {
                            let mut c = commands.spawn((Mesh3d(ghost_mesh.clone()), MeshMaterial3d(h.clone()), Transform::IDENTITY, Visibility::Hidden, NoFrustumCulling, RenderLayers::layer(0), PulseGhost { slot }, ChildOf(if skin.is_some() { root } else { e })));
                            if let Some(skin) = skin {
                                c.insert(skin.clone());
                            }
                        }
                        made += 1;
                    }
                    if made > 0 {
                        assets.built_for[slot] = Some(root);
                    }
                    if log {
                        eprintln!("[aura_pulse] slot {slot}: ghosts for {made} body pieces, {} textures", assets.by_tex[slot].len());
                    }
                }
            }
        }
        if (assets.last[slot] - strength).abs() > 1e-4 {
            assets.last[slot] = strength;
            for (k, h) in &assets.mats[slot] {
                if let Some(mut m) = materials.get_mut(h) {
                    m.glow = glow_for(strength, *k, gain);
                    m.shift.z = ghost(*k).1 * env_f32("PD_AP_WIDTH", WIDTH_DEFAULT);
                }
            }
        }
        let want = if strength > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        for (g, mut vis) in &mut ghosts {
            if g.slot == slot && *vis != want {
                *vis = want;
            }
        }
    }
}

/// A copy of `src` whose normals are rebuilt from its own triangles:
/// area-weighted face normals, averaged over every vertex sharing a position
/// (welds UV / material seams), flipped if the mesh's signed volume says its
/// winding is inside out. The converter writes every character vertex normal as
/// the constant (0,0,1) (the real one lives in the normal map,
/// `pd_formats/src/character.rs`), so the glow needs its own to tell the side
/// of a piece that faces the camera from the side that faces away.
fn ghost_mesh(src: &Mesh) -> Option<Mesh> {
    let Ok(VertexAttributeValues::Float32x3(pos)) = src.try_attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let idx: Vec<usize> = match src.try_indices().ok()? {
        Indices::U16(v) => v.iter().map(|&i| i as usize).collect(),
        Indices::U32(v) => v.iter().map(|&i| i as usize).collect(),
    };
    let normals = welded_normals(pos, &idx)?;
    let mut out = src.clone();
    out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    Some(out)
}

/// The normal computation behind [`ghost_mesh`], split out for the tests.
fn welded_normals(pos: &[[f32; 3]], idx: &[usize]) -> Option<Vec<[f32; 3]>> {
    let key = |p: [f32; 3]| [(p[0] * 256.0).round() as i32, (p[1] * 256.0).round() as i32, (p[2] * 256.0).round() as i32];
    let mut group: HashMap<[i32; 3], usize> = HashMap::new();
    let slot: Vec<usize> = pos
        .iter()
        .map(|&p| {
            let n = group.len();
            *group.entry(key(p)).or_insert(n)
        })
        .collect();
    let mut acc = vec![Vec3::ZERO; group.len()];
    let mut volume = 0.0f32;
    for t in idx.chunks_exact(3) {
        if t.iter().any(|&i| i >= pos.len()) {
            return None;
        }
        let (a, b, c) = (Vec3::from(pos[t[0]]), Vec3::from(pos[t[1]]), Vec3::from(pos[t[2]]));
        let n = (b - a).cross(c - a); // length = 2 x area: area-weighted
        volume += a.dot(b.cross(c));
        for &i in t {
            acc[slot[i]] += n;
        }
    }
    let sign = if volume < 0.0 { -1.0 } else { 1.0 };
    Some(slot.iter().map(|&s| (acc[s].normalize_or(Vec3::Y) * sign).to_array()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing for the first 5 ticks after the press, then the footage's 23.5
    /// frames, gone by the end of the glow counter.
    #[test]
    fn aura_pulse_footage_envelope_runs_the_measured_length() {
        assert_eq!(crate::combat::AURA_GLOW_TICKS, 53);
        for t in 0..GLOW_DELAY_TICKS {
            assert_eq!(envelope(t), 0.0, "tick {t} after the press: not yet");
        }
        let first = envelope(GLOW_DELAY_TICKS);
        assert!((0.7..0.85).contains(&first), "pops in at about three quarters (footage frame 163): {first}");
        assert!(envelope(GLOW_DELAY_TICKS + 45) > 0.0 && envelope(GLOW_DELAY_TICKS + 45) < 0.05, "faint on the last tick");
        assert_eq!(envelope(crate::combat::AURA_GLOW_TICKS), 0.0, "gone when the counter ends");
        assert_eq!(envelope(200), 0.0);
    }

    /// Full strength over footage frames 166-169 (6 to 12 ticks after the first
    /// glow frame), a monotone fade after, half strength about frame 176.
    #[test]
    fn aura_pulse_footage_envelope_peaks_then_fades() {
        for t in 6..=12 {
            assert!(envelope(GLOW_DELAY_TICKS + t) >= 0.99, "tick {t} after the glow starts: peak");
        }
        let mut last = 1.0;
        for t in (GLOW_DELAY_TICKS + 12)..crate::combat::AURA_GLOW_TICKS {
            let e = envelope(t);
            assert!(e <= last + 1e-6, "tick {t}: monotone fade ({e} after {last})");
            last = e;
        }
        let e = envelope(GLOW_DELAY_TICKS + 26);
        assert!((0.38..=0.5).contains(&e), "frame 176: {e}");
    }

    /// Ghost directions are unit, inside the disc, denser toward the middle.
    #[test]
    fn ghosts_are_unit_directions_inside_the_disc() {
        assert_eq!(GHOST_COUNT, 64);
        let mut inner = 0;
        for k in 0..GHOST_COUNT {
            let (d, r, w) = ghost(k);
            assert!((d.length() - 1.0).abs() < 1e-5);
            assert!(r > 0.0 && r <= DISC_RADIUS_U);
            assert!(w > 0.0);
            if r < DISC_RADIUS_U * 0.5 {
                inner += 1;
            }
        }
        assert!(inner > GHOST_COUNT / 2, "denser near the middle ({inner} of {GHOST_COUNT} inside half the radius)");
    }

    /// The stacked ghosts reproduce the footage's halo profile along a straight
    /// edge: about 1.4 (clipped) light at the silhouette, falling off with the
    /// fitted 0.28 u scale length, nothing past the disc.
    #[test]
    fn stacked_ghosts_fit_the_footage_profile() {
        let at = |d: f32| -> f32 {
            (0..GHOST_COUNT)
                .map(|k| {
                    let (dir, r, w) = ghost(k);
                    if r * dir.x >= d { w } else { 0.0 }
                })
                .sum()
        };
        let edge = at(0.0);
        assert!((1.2..1.7).contains(&edge), "edge light {edge}");
        assert!((0.35..0.65).contains(&at(0.32)), "0.32 u: {}", at(0.32));
        assert!(at(0.8) < 0.15 && at(0.8) > 0.01, "0.8 u: {}", at(0.8));
        assert_eq!(at(DISC_RADIUS_U + 0.01), 0.0, "nothing past the disc");
        let mut last = f32::MAX;
        for i in 0..130 {
            let v = at(i as f32 * 0.01);
            assert!(v <= last + 1e-6, "profile never rises with distance");
            last = v;
        }
    }

    /// The colour is pale green: G strongest, R and B about half of it in
    /// display space.
    #[test]
    fn the_glow_is_pale_green() {
        let g = glow_for(1.0, 5, 1.0);
        assert!(g.y > g.x && g.y > g.z && g.x > 0.0 && g.z > 0.0);
        assert_eq!(glow_for(0.0, 5, 1.0), Vec4::ZERO);
        assert!(glow_for(0.5, 5, 1.0).y < glow_for(1.0, 5, 1.0).y);
    }

    /// The rebuilt normals point outward whichever way the mesh is wound, and
    /// split (UV seam) vertices are welded.
    #[test]
    fn ghost_normals_point_outward_either_winding() {
        let c = |x: f32, y: f32, z: f32| [x, y, z];
        let faces: [[[f32; 3]; 4]; 6] = [
            [c(1., -1., -1.), c(1., 1., -1.), c(1., 1., 1.), c(1., -1., 1.)],
            [c(-1., -1., 1.), c(-1., 1., 1.), c(-1., 1., -1.), c(-1., -1., -1.)],
            [c(-1., 1., -1.), c(-1., 1., 1.), c(1., 1., 1.), c(1., 1., -1.)],
            [c(-1., -1., 1.), c(-1., -1., -1.), c(1., -1., -1.), c(1., -1., 1.)],
            [c(-1., -1., 1.), c(1., -1., 1.), c(1., 1., 1.), c(-1., 1., 1.)],
            [c(1., -1., -1.), c(-1., -1., -1.), c(-1., 1., -1.), c(1., 1., -1.)],
        ];
        let pos: Vec<[f32; 3]> = faces.iter().flatten().copied().collect();
        let mut idx = Vec::new();
        for f in 0..6 {
            let b = f * 4;
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        let reversed: Vec<usize> = idx.chunks(3).flat_map(|t| [t[0], t[2], t[1]]).collect();
        for tris in [&idx, &reversed] {
            let n = welded_normals(&pos, tris).unwrap();
            for (p, n) in pos.iter().zip(&n) {
                let p = Vec3::from(*p).normalize();
                assert!(Vec3::from(*n).dot(p) > 0.99, "normal {n:?} at {p:?} not outward");
            }
        }
    }
}
