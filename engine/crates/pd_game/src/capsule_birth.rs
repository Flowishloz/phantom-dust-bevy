//! Capsule regeneration motes: the gold point sprites over an empty capsule
//! spot while its next capsule is on the way (the program's capsule slot state
//! 0, `FUN_14021ba60` drawing `st00` slot 0 `cap01_birth`; docs/
//! CAPSULE-VISUALS.md "Regeneration motes (2026-10-01)"). Render only: it reads
//! `Combat::pending_capsules` and never writes the sim.
//!
//! What is READ from the program:
//! - the animation frame is `elapsed * 420 / wait` (`FUN_14021ba60`), so the
//!   429-frame animation is stretched over the whole wait, white, additive;
//! - the effect sits exactly where the capsule will appear, yawed by the spot
//!   index x 90 degrees (`FUN_14021b3d0`);
//! - at the appear (state 1) slot 0 stops and the coloured flare takes over
//!   (`fx.rs::flare_burst`);
//! - for a viewer who does not own the capsule the same points blend as
//!   `dst * (1 - colour)` instead of adding (`FUN_14028e4e0(0x3001)` in
//!   `FUN_140235da0`), so the opponent sees dark specks.
//!
//! The same file also draws slot 2 `cap03_death` (a normal take) and slot 3
//! `cap04_braek` (a skill erased the capsule): see `CapsuleBurst` below.
//!
//! The six draws, their points, joint animation, size, start and end frames
//! come from `pd_convert capsules` (`<stem>.sprites.json`,
//! `pd_formats::capsprites`). Each point's colour is the strip texture sampled
//! at its own UV plus `(frame - start) / (end - start)` (`capsule_birth.wgsl`).

use crate::frontend::state::{AppState, MatchScene};
use crate::World;
use bevy::asset::{Asset, RenderAssetUsages};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::{Shader, ShaderRef};
use std::collections::HashMap;

static SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();

/// Where the effect sits above the spot: the same lift the idle orb and the
/// flare use (`fx.rs`, `sync_combat_visuals`) so the gather forms where the
/// orb will pop. (The program draws both at the slot position itself with no
/// offset, `FUN_14021b840` / `FUN_14021ba60`; our orb's +5 is inherited.)
pub const ORB_LIFT: f32 = 5.0;

/// The program's vertical clip-space size per unit of the draw's `size` field
/// (`vkPSPRITE_vs` `0.002083` x the constant `10.0` at `0x1402f7f7c` x the
/// display scale 1 at `0x1402f3e18`, `FUN_140235da0`); with the draws' `size`
/// 100 a quad corner moves 2.083 clip units. READ.
const CLIP_PER_SIZE: f32 = 0.002083 * 10.0;
/// Half extent of the quad in corner units. The program's quad table (a
/// constant buffer filled outside the exe) was not recovered: UNCONFIRMED,
/// default 1.0 (corners at +-1); `PD_CAPBIRTH_QUAD` overrides for calibration.
const QUAD_HALF: f32 = 1.0;

#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct BirthSprite {
    /// x = half size factor (clip units), y = 0 add / 1 one-minus-source, z = gain,
    /// w = 1 adds the stored value (gamma-space add; the pickup / erase bursts).
    #[uniform(0)]
    pub params: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub strip: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub dot: Handle<Image>,
}

impl Material for BirthSprite {
    fn vertex_shader() -> ShaderRef {
        SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn alpha_mode(&self) -> AlphaMode {
        if self.params.y > 0.5 {
            AlphaMode::Multiply
        } else {
            AlphaMode::Add
        }
    }
    fn enable_shadows() -> bool {
        false
    }
    fn enable_prepass() -> bool {
        false
    }
}

/// One baked draw of the cloud.
struct CloudDraw {
    /// The draw's size word (`vkPSPRITE` constant c32.x scales with it): 100 for slot 0,
    /// 50 for slots 2 and 3.
    size: f32,
    start: f32,
    end: f32,
    /// x, y, z, u, v per point.
    points: Vec<[f32; 5]>,
    /// Joint world matrix per integer frame, 3x4 row-major, right-handed.
    m: Vec<[f32; 12]>,
}

/// `cap00_cap01_birth.sprites.json` plus the materials.
#[derive(Resource)]
struct BirthData {
    draws: Vec<CloudDraw>,
    /// Animation length in frames (429).
    length: f32,
    add: Handle<BirthSprite>,
    multiply: Handle<BirthSprite>,
}

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn parse_cloud(text: &str) -> Option<(f32, Vec<CloudDraw>)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let f = |x: &serde_json::Value| x.as_f64().map(|n| n as f32);
    let length = f(&v["frames"])?;
    let mut draws = Vec::new();
    for d in v["draws"].as_array()? {
        let points = d["points"].as_array()?.iter().filter_map(|p| p.as_array().map(|a| [0, 1, 2, 3, 4].map(|i| a.get(i).and_then(f).unwrap_or(0.0)))).collect();
        let m = d["m"].as_array()?.iter().filter_map(|m| m.as_array().map(|a| std::array::from_fn(|i| a.get(i).and_then(f).unwrap_or(0.0)))).collect();
        draws.push(CloudDraw { size: f(&d["size"]).unwrap_or(100.0), start: f(&d["start"])?, end: f(&d["end"])?, points, m });
    }
    Some((length, draws))
}

/// Sampler 0 repeats (the strip scrolls past 1.0), sampler 3 mirrors
/// (`FUN_140235da0`: `FUN_140020c40(0, 3/4, 1)` and `(3, 3/4, 2)`).
fn load_tex(assets: &AssetServer, path: &str, mode: bevy::image::ImageAddressMode) -> Handle<Image> {
    use bevy::image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
    assets
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: mode, address_mode_v: mode, ..ImageSamplerDescriptor::linear() });
        })
        .load(path.to_string())
}

fn load_birth(mut commands: Commands, cfg: Res<crate::Config>, assets: Res<AssetServer>, mut mats: ResMut<Assets<BirthSprite>>) {
    // The baked cloud lives next to the other capsule files; `PD_CAPBIRTH_DIR`
    // points at a freshly converted folder (a lane's own output) instead.
    let dir = std::env::var_os("PD_CAPBIRTH_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| cfg.cache.join("fx/capsules"));
    let Ok(text) = std::fs::read_to_string(dir.join("cap00_cap01_birth.sprites.json")) else {
        warn!("no cap00_cap01_birth.sprites.json: run `pd_convert capsules` (no regeneration motes)");
        return;
    };
    let Some((length, draws)) = parse_cloud(&text) else {
        warn!("cap00_cap01_birth.sprites.json unreadable (regeneration motes off)");
        return;
    };
    use bevy::image::ImageAddressMode;
    let strip = load_tex(&assets, "fx/capsules/tex00.png", ImageAddressMode::Repeat);
    let dot = load_tex(&assets, "fx/capsules/tex01.png", ImageAddressMode::MirrorRepeat);
    let half = CLIP_PER_SIZE * 100.0 * env_f32("PD_CAPBIRTH_QUAD", QUAD_HALF);
    let gain = env_f32("PD_CAPBIRTH_GAIN", 1.0);
    let add = mats.add(BirthSprite { params: Vec4::new(half, 0.0, gain, 0.0), strip: strip.clone(), dot: dot.clone() });
    // `PD_CAPBIRTH_OPPONENT=add` draws the owner's additive look for opponents too
    // (the read says they get `dst * (1 - colour)`, dark specks; this is the A/B switch).
    let opponent_mode = if std::env::var("PD_CAPBIRTH_OPPONENT").is_ok_and(|v| v == "add") { 0.0 } else { 1.0 };
    let multiply = mats.add(BirthSprite { params: Vec4::new(half, opponent_mode, gain, 0.0), strip, dot });
    commands.insert_resource(BirthData { draws, length, add, multiply });
}

/// Joint matrix at a fractional frame: the two nearest baked integer frames blended.
fn pose_at(m: &[[f32; 12]], frame: f32) -> [f32; 12] {
    if m.is_empty() {
        return [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    }
    let f = frame.clamp(0.0, (m.len() - 1) as f32);
    let (i, k) = (f.floor() as usize, f.fract());
    let (a, b) = (&m[i], &m[(i + 1).min(m.len() - 1)]);
    std::array::from_fn(|j| a[j] + (b[j] - a[j]) * k)
}

/// The program's frame for `elapsed` of `wait` ticks: `elapsed * 420 / wait`
/// (`FUN_14021ba60`), kept inside the animation (`length - 0.001` when it
/// would reach the end, `FUN_14021ba60`'s clamp).
pub fn program_frame(elapsed: f32, wait: f32, length: f32) -> f32 {
    let f = if wait > 0.0 { elapsed * 420.0 / wait } else { 0.0 };
    if f >= length {
        length - 0.001
    } else {
        f.max(0.0)
    }
}

/// Mesh of the whole cloud at `frame`: four vertices per point, all at the
/// point's world position (the shader spreads the corners), UV_0 the corner,
/// UV_1 the strip coordinate.
fn cloud_mesh(draws: &[CloudDraw], frame: f32, at: Vec3, yaw: Quat) -> Mesh {
    let total: usize = draws.iter().map(|d| d.points.len()).sum();
    let (mut pos, mut uv0, mut uv1, mut idx) = (Vec::with_capacity(total * 4), Vec::with_capacity(total * 4), Vec::with_capacity(total * 4), Vec::with_capacity(total * 6));
    for d in draws {
        let m = pose_at(&d.m, frame);
        let progress = ((frame - d.start).max(0.0)) / (d.end - d.start).max(1e-3);
        for p in &d.points {
            let w = Vec3::new(
                m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
                m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
                m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
            );
            let c = (at + yaw * w).to_array();
            let b = pos.len() as u32;
            pos.extend_from_slice(&[c; 4]);
            uv0.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
            uv1.extend_from_slice(&[[p[3] + progress, p[4]]; 4]);
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
    }
    if pos.is_empty() {
        // Never empty (a zero-length vertex buffer upsets the render allocator).
        pos = vec![[0.0; 3]; 3];
        uv0 = vec![[0.0; 2]; 3];
        uv1 = vec![[0.0; 2]; 3];
        idx = vec![0, 1, 2];
    }
    let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv0)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
        .with_inserted_indices(Indices::U32(idx))
}

/// The yaw of a spot's effect: `spot * 90` degrees about Y in the program's
/// left-handed space (`FUN_14021b3d0`: `FUN_1402906e0` with `slot * 8192` of
/// 32768 per turn); mirroring z turns it the other way round here. Derived
/// from the program's matrix rows, not seen on screen (the big clouds are
/// symmetric about Y).
fn spot_yaw(spot: usize) -> Quat {
    Quat::from_rotation_y(-(spot as f32) * std::f32::consts::FRAC_PI_2)
}

/// One mesh per waiting spot, drawn for its owner (adds) and for everyone else (darkens).
#[derive(Component)]
struct BirthCloud {
    mesh: Handle<Mesh>,
}

fn sync_birth(
    mut commands: Commands,
    world: Res<World>,
    fixed: Res<Time<Fixed>>,
    data: Option<Res<BirthData>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut live: Local<HashMap<(usize, usize), Vec<Entity>>>,
    clouds: Query<&BirthCloud>,
) {
    let Some(data) = data else { return };
    // Leaving a match despawned them (`MatchScene`): forget those entries.
    live.retain(|_, es| es.iter().all(|e| clouds.contains(*e)));
    let pending = world.0.combat.pending_capsules();
    let over = fixed.overstep_fraction();
    let log = std::env::var_os("PD_CAPBIRTH_LOG").is_some();
    let mut seen = Vec::new();
    for p in &pending {
        let key = (p.owner, p.spot);
        seen.push(key);
        let frame = program_frame(p.elapsed as f32 + over, p.wait as f32, data.length);
        if log {
            eprintln!("[capbirth] tick {} owner {} spot {} elapsed {}/{} frame {:.1}", world.0.tick, p.owner, p.spot, p.elapsed, p.wait, frame);
        }
        let mesh = cloud_mesh(&data.draws, frame, p.pos + Vec3::Y * ORB_LIFT, spot_yaw(p.spot));
        match live.get(&key).and_then(|es| es.first()).and_then(|e| clouds.get(*e).ok()) {
            Some(c) => {
                if let Some(mut m) = meshes.get_mut(&c.mesh) {
                    *m = mesh;
                }
            }
            None => {
                use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
                let handle = meshes.add(mesh);
                let layers = [1 + p.owner, crate::fx::GHOST_LAYER_BASE + p.owner];
                let mats = [data.add.clone(), data.multiply.clone()];
                let es = (0..2)
                    .map(|k| commands.spawn((Mesh3d(handle.clone()), MeshMaterial3d(mats[k].clone()), Transform::IDENTITY, Visibility::Inherited, RenderLayers::layer(layers[k]), NoFrustumCulling, BirthCloud { mesh: handle.clone() }, MatchScene)).id())
                    .collect();
                live.insert(key, es);
            }
        }
    }
    live.retain(|k, es| {
        let keep = seen.contains(k);
        if !keep {
            for e in es.iter() {
                commands.entity(*e).despawn();
            }
        }
        keep
    });
}

// ---------------------------------------------------------------------------
// Pickup (slot 2) and erase (slot 3) bursts.
//
// READ (`FUN_14014fd10` -> `FUN_14021b4f0`, docs/CAPSULE-VISUALS.md "Pickup and
// erase bursts"): taking a capsule spawns an effect record of `st00` slot 2
// `cap03_death` at the orb (mode 3), a skill erasing it slot 3 `cap04_braek`
// (mode 4, with common/92, played by `sfx.rs`), each with a fresh random
// rotation (three independent uniform angles, `FUN_140290a40` fed from `rand()`).
// The record (`FUN_14022e7d0`, updated by `FUN_14022ea00`, drawn by
// `FUN_14022eb30`) starts at frame 0, gains 1 per program tick (30 Hz) and ends
// when the frame passes the animation's last (89): 90 program ticks, drawn once
// (loop count 1), no fade (alpha 255). Both slots are the same `vkPSPRITE`
// point-sprite material as slot 0 (blend 0x1100 = ONE + ONE; first texture
// `st00` tex4 the orange strip, second tex1 the dot: read off the material
// records), seven draws of 3 points each on joints 3, 5, .. 15 with
// `size 50, start 0, end 90` from each draw's 0x13 chunk, so a point's strip
// scroll is `frame / 90`. Colour word `(owner has a view) << 24 | 0xffffff`
// (`FUN_14021b4f0` `local_54`): top byte 1 = ONE + ONE for everyone; every
// player of this build has a camera, so it is always the additive gold.
// (UNCONFIRMED: whether the original gives an AI player's capsules the byte 0,
// dark specks like the opponent's regeneration cloud.)

/// Sim ticks per program tick (the program updates at 30 Hz; the regeneration
/// wait is 840 sim ticks for 420 program ticks).
const SIM_TICKS_PER_PROGRAM_TICK: f32 = (crate::sim::HZ / 30.0) as f32;

/// One pickup / erase burst: spawned by `fx.rs::sync_combat_visuals` when a
/// capsule vanishes; `drive_bursts` builds and animates its mesh, then removes it.
#[derive(Component)]
pub struct CapsuleBurst {
    /// 2 = pickup (`cap03_death`), 3 = erase (`cap04_braek`).
    slot: u8,
    at: Vec3,
    /// Sim tick it started on.
    born: u64,
    rot: Quat,
}

/// The components of a burst entity at `at` (already lifted to the orb's centre),
/// started on sim tick `born`; `seed` picks the random rotation.
pub fn capsule_burst(slot: u8, at: Vec3, born: u64, seed: u32) -> impl Bundle {
    (CapsuleBurst { slot, at, born, rot: random_rotation(seed ^ (born as u32).wrapping_mul(2_654_435_761)) }, crate::frontend::state::MatchScene)
}

/// Three independent uniform angles composed (the program: three `rand()` angles
/// in `FUN_140290a40`); the seed stands in for `rand()`.
fn random_rotation(seed: u32) -> Quat {
    let mut x = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453) | 1;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
    };
    let (a, b, c) = (next(), next(), next());
    Quat::from_rotation_z(a) * Quat::from_rotation_y(b) * Quat::from_rotation_x(c)
}

struct BurstSet {
    draws: Vec<CloudDraw>,
    length: f32,
    material: Handle<BirthSprite>,
}

#[derive(Resource)]
struct BurstData {
    pickup: BurstSet,
    erase: BurstSet,
}

#[derive(Component)]
struct BurstMesh(Handle<Mesh>);

fn load_bursts(mut commands: Commands, cfg: Res<crate::Config>, assets: Res<AssetServer>, mut mats: ResMut<Assets<BirthSprite>>) {
    let dir = std::env::var_os("PD_CAPBIRTH_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| cfg.cache.join("fx/capsules"));
    let strip = load_tex(&assets, "fx/capsules/tex04.png", bevy::image::ImageAddressMode::Repeat);
    let dot = load_tex(&assets, "fx/capsules/tex01.png", bevy::image::ImageAddressMode::MirrorRepeat);
    let mut load_set = |file: &str| -> Option<BurstSet> {
        let text = std::fs::read_to_string(dir.join(file)).ok()?;
        let (length, draws) = parse_cloud(&text)?;
        let size = draws.first()?.size;
        let half = CLIP_PER_SIZE * size * env_f32("PD_CAPBIRTH_QUAD", QUAD_HALF);
        let material = mats.add(BirthSprite { params: Vec4::new(half, 0.0, env_f32("PD_CAPBIRTH_GAIN", 1.0), env_f32("PD_CAPBURST_GAMMA", 1.0)), strip: strip.clone(), dot: dot.clone() });
        Some(BurstSet { draws, length, material })
    };
    let (Some(pickup), Some(erase)) = (load_set("cap02_cap03_death.sprites.json"), load_set("cap03_cap04_braek.sprites.json")) else {
        warn!("no cap02_cap03_death / cap03_cap04_braek .sprites.json: run `pd_convert capsules` (no pickup / erase bursts)");
        return;
    };
    commands.insert_resource(BurstData { pickup, erase });
}

fn drive_bursts(
    mut commands: Commands,
    world: Res<World>,
    fixed: Res<Time<Fixed>>,
    data: Option<Res<BurstData>>,
    mut meshes: ResMut<Assets<Mesh>>,
    bursts: Query<(Entity, &CapsuleBurst, Option<&BurstMesh>)>,
) {
    let Some(data) = data else { return };
    let over = fixed.overstep_fraction();
    for (e, b, built) in &bursts {
        let set = if b.slot == 3 { &data.erase } else { &data.pickup };
        let age = world.0.tick.saturating_sub(b.born) as f32 + over;
        let frame = age / SIM_TICKS_PER_PROGRAM_TICK;
        // The record ends when the frame passes the animation's last (`FUN_14022ea00`).
        if frame > set.length {
            commands.entity(e).despawn();
            continue;
        }
        let mesh = cloud_mesh(&set.draws, frame, b.at, b.rot);
        match built {
            Some(BurstMesh(h)) => {
                if let Some(mut m) = meshes.get_mut(h) {
                    *m = mesh;
                }
            }
            None => {
                if std::env::var_os("PD_CAPBIRTH_LOG").is_some() {
                    eprintln!("[capburst] tick {} slot {} ({}) at {:?}", world.0.tick, b.slot, if b.slot == 3 { "erase" } else { "pickup" }, b.at);
                }
                let h = meshes.add(mesh);
                use bevy::camera::visibility::NoFrustumCulling;
                commands.entity(e).insert((Mesh3d(h.clone()), MeshMaterial3d(set.material.clone()), Transform::IDENTITY, Visibility::Inherited, NoFrustumCulling, BurstMesh(h)));
            }
        }
    }
}

pub fn plugin(app: &mut App) {
    {
        let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("capsule_birth.wgsl"), "pd_game/capsule_birth.wgsl"));
        let _ = SHADER.set(shader);
    }
    app.add_plugins(MaterialPlugin::<BirthSprite>::default()).add_systems(Startup, (load_birth, load_bursts)).add_systems(Update, (sync_birth, drive_bursts).run_if(in_state(AppState::Match)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_is_elapsed_times_420_over_the_wait() {
        // FUN_14021ba60: (wait - remaining) * 420 / wait, over the whole wait.
        assert_eq!(program_frame(0.0, 840.0, 429.0), 0.0);
        assert!((program_frame(420.0, 840.0, 429.0) - 210.0).abs() < 1e-4);
        assert!((program_frame(839.0, 840.0, 429.0) - 419.5).abs() < 1e-3);
        // a longer wait (environment status) stretches the same 420 frames
        assert!((program_frame(500.0, 1000.0, 429.0) - 210.0).abs() < 1e-4);
        assert!(program_frame(10_000.0, 840.0, 429.0) < 429.0, "kept inside the animation");
    }

    #[test]
    fn pose_blends_neighbouring_frames_and_holds_the_ends() {
        let mut a = [0.0; 12];
        let mut b = [0.0; 12];
        a[3] = 1.0;
        b[3] = 3.0;
        let m = vec![a, b];
        assert_eq!(pose_at(&m, 0.5)[3], 2.0);
        assert_eq!(pose_at(&m, 9.0)[3], 3.0);
        assert_eq!(pose_at(&[], 3.0)[0], 1.0, "identity when nothing is baked");
    }

    #[test]
    fn colour_scroll_starts_at_the_draws_start_frame() {
        let identity = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let d = CloudDraw { size: 100.0, start: 350.0, end: 430.0, points: vec![[0.0, 0.0, 0.0, 0.01, 0.5]], m: vec![identity; 2] };
        let uv = |frame: f32| {
            let mesh = cloud_mesh(std::slice::from_ref(&d), frame, Vec3::ZERO, Quat::IDENTITY);
            match mesh.attribute(Mesh::ATTRIBUTE_UV_1) {
                Some(bevy::mesh::VertexAttributeValues::Float32x2(v)) => v[0],
                _ => panic!("no uv_b"),
            }
        };
        assert_eq!(uv(100.0)[0], 0.01, "dark strip end until the start frame");
        assert!((uv(390.0)[0] - 0.51).abs() < 1e-4, "halfway: u + 0.5");
        assert_eq!(uv(100.0)[1], 0.5);
    }

    /// With the converted file at hand (`PD_CAPBIRTH_DIR`, else the cache): the
    /// cloud starts as a wide flat scatter and ends as a ball about the size of
    /// the orb (st00.dat's joint scales; ~11 u across at the last frame).
    #[test]
    fn the_real_cloud_gathers_into_a_ball() {
        let dir = std::env::var_os("PD_CAPBIRTH_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/fx/capsules"));
        let Ok(text) = std::fs::read_to_string(dir.join("cap00_cap01_birth.sprites.json")) else {
            eprintln!("no baked cloud: skipped");
            return;
        };
        let (length, draws) = parse_cloud(&text).expect("parses");
        assert_eq!(length, 429.0);
        assert_eq!(draws.len(), 6);
        assert_eq!(draws.iter().map(|d| d.points.len()).sum::<usize>(), 285, "every point of the buffer is used once");
        let extent = |frame: f32| {
            let mesh = cloud_mesh(&draws, frame, Vec3::ZERO, Quat::IDENTITY);
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!("no positions") };
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for p in v {
                lo = lo.min(Vec3::from(*p));
                hi = hi.max(Vec3::from(*p));
            }
            hi - lo
        };
        let (start, end) = (extent(0.0), extent(419.5));
        eprintln!("extent at frame 0 {start:?}, at 419.5 {end:?}");
        assert!(start.x > 40.0 && start.z > 40.0 && start.y < 25.0, "a wide, flat scatter: {start:?}");
        assert!(end.max_element() < 14.0, "a ball about the orb's size: {end:?}");
    }

    /// The pickup / erase bursts' baked data: seven draws of three points, size 50,
    /// start 0, end 90 (each draw's 0x13 chunk), a 90-frame animation, both slots alike.
    #[test]
    fn the_real_bursts_are_seven_small_draws_over_ninety_frames() {
        let dir = std::env::var_os("PD_CAPBIRTH_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/fx/capsules"));
        for file in ["cap02_cap03_death.sprites.json", "cap03_cap04_braek.sprites.json"] {
            let Ok(text) = std::fs::read_to_string(dir.join(file)) else {
                eprintln!("no baked burst {file}: skipped");
                continue;
            };
            let (length, draws) = parse_cloud(&text).expect("parses");
            assert_eq!(length, 89.0, "{file}");
            assert_eq!(draws.len(), 7, "{file}");
            for d in &draws {
                assert_eq!((d.points.len(), d.size, d.start, d.end), (3, 50.0, 0.0, 90.0), "{file}");
                assert_eq!(d.m.len(), 90, "{file}: a matrix per integer frame 0..=89");
            }
        }
    }

    #[test]
    fn a_burst_lasts_ninety_program_ticks_and_turns_a_new_way_each_time() {
        // `FUN_14022ea00`: +1 per 30 Hz tick, over when the frame passes 89.
        assert_eq!(SIM_TICKS_PER_PROGRAM_TICK, 2.0);
        let (a, b) = (random_rotation(1), random_rotation(2));
        assert!(a.angle_between(b) > 1e-3, "two bursts do not share a rotation");
        assert_eq!(random_rotation(7), random_rotation(7), "deterministic per seed");
        assert!((random_rotation(5).length() - 1.0).abs() < 1e-5, "a proper rotation");
    }

    #[test]
    fn the_spots_yaw_is_a_quarter_turn_per_index() {
        let p = spot_yaw(2) * Vec3::X;
        assert!((p + Vec3::X).length() < 1e-4, "{p:?}");
        assert!((spot_yaw(0) * Vec3::Z - Vec3::Z).length() < 1e-5);
    }
}
