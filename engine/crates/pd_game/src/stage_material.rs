//! The stage's receiving material: shadows (lane L2) and the original's
//! per-pixel bump lighting (`SBTS`, lane L3). `shadows.rs` owns the systems.
//!
//! Stages export as `KHR_materials_unlit` textures multiplied by baked vertex
//! colours (the stage light script baked into `COLOR_0`, FORMATS-ALR.md "Stage
//! lighting"), so Bevy's standard material has nowhere to receive a shadow or
//! a per-pixel light. `StageMaterial` is that same `StandardMaterial` (the
//! authored one is the base, untouched: textures, alpha cutout, double
//! sidedness, fog, tone mapping all stay Bevy's) plus uniforms and a fragment
//! shader (`stage_material.wgsl`):
//!
//! * every draw: the original's shadow blend after the standard colour (L2);
//! * bump-mapped draws (`pd_formats::gltf::is_stage_bump`: vertex format 0x03,
//!   material flag 0x20, exported with NORMAL / TANGENT / TEXCOORD_1 and the
//!   normal map): the original's `SBTS_ps`, replacing the baked colour. See
//!   docs/STAGE-LIGHT.md for the shader read instruction by instruction.
//!
//! `PD_STAGE_LIGHT=0` turns the per-pixel path off for every draw (the
//! standard colour with the baked vertex colours, as before this lane);
//! outside a shadow the shader returns that colour bit for bit.

use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;

static STAGE_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();
/// The light set every stage draw binds: ONE storage buffer, written in place (overnight perf
/// 2026-10-09). It used to be a uniform in every stage material, so a playing effect light rewrote
/// every stage material every frame it changed (each re-prepared by the render thread: the
/// per-frame `stage` churn of a fight). `PD_STAGE_LIGHT_BUF=0` is not offered: the values are
/// the same either way, only where they live changed.
static LIGHT_BUFFER: std::sync::OnceLock<Handle<ShaderBuffer>> = std::sync::OnceLock::new();

/// The shared light buffer (a default handle before `register` ran: tests).
pub fn light_buffer() -> Handle<ShaderBuffer> {
    LIGHT_BUFFER.get().cloned().unwrap_or_default()
}

/// Writes the light set into the shared buffer (one in-place write; nothing when unchanged).
pub fn write_light(live: &mut StageLightLive, buffers: &mut Assets<ShaderBuffer>, l: StageLightUniform) {
    if live.1 == Some(l) {
        return;
    }
    if let Some(mut b) = LIGHT_BUFFER.get().and_then(|h| buffers.get_mut(h)) {
        b.set_data(l);
        live.1 = Some(l);
    }
}

/// `PD_STAGE_LIGHT=0`: no per-pixel stage lighting (the baked look).
pub fn per_pixel_enabled() -> bool {
    std::env::var("PD_STAGE_LIGHT").map_or(true, |v| v != "0")
}

/// The stage's light set 0 (what `SBTS_vs`/`SBTS_ps` read), gamma-space colours
/// 0..1, directions in Bevy world space (the game's left-handed Z mirrored).
/// Shared by every draw; also holds the effect light (slot 2 of the table).
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct StageLightUniform {
    /// xyz = `cDL_COL` (the directional light colour), gamma 0..1.
    pub dl_col: Vec4,
    /// xyz = `cDL_VEC`: the directional light's travel direction (world, unit).
    pub dl_dir: Vec4,
    /// xyz = `cS0_COLp`: ambient for normals along the hemisphere axis.
    pub s0: Vec4,
    /// xyz = `cS1_COLp`: ambient for normals against it.
    pub s1: Vec4,
    /// xyz = the hemisphere axis (`cS0_VEC`, world, unit).
    pub axis: Vec4,
    /// xyz = `cP0_COLp` (the point light's colour; 0 = none).
    pub p0_col: Vec4,
    /// xyz = `cP0_POS` world position, w = its scalar (intensity / d^2).
    pub p0_pos: Vec4,
}

/// Per-draw constants of a bump material (`extras.pd_stage` in the glb).
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct StageMatUniform {
    /// xyz = the material's specular colour bytes (R, G, B) / 255, w = the
    /// specular power byte (unused by `SBTS_ps`, whose lobe is the spec77 LUT).
    pub spec: Vec4,
    /// x = `cEYEVEC.w` = fresnel short * 0.01 + 1 (`FUN_1402a69c0`);
    /// y = 1 for a bump-mapped draw, 0 for every other draw;
    /// z = 1: the point light per pixel (`PD_STAGE_POINT_PIXEL=1`, the old A/B look).
    pub params: Vec4,
}

impl StageMatUniform {
    /// From the glb's `extras.pd_stage`: `spec` = [R, G, B, power] bytes (zero
    /// when material flag 0x04 is clear), `fresnel` = the +0x40 short.
    pub fn bump(spec: [u8; 4], fresnel: i16) -> Self {
        let f = |b: u8| b as f32 / 255.0;
        Self { spec: Vec4::new(f(spec[0]), f(spec[1]), f(spec[2]), spec[3] as f32), params: Vec4::new(fresnel as f32 * 0.01 + 1.0, 1.0, 0.0, 0.0) }
    }
}

/// The extension: the stage light's direction for the shadow weight, the
/// light set, and the draw's own constants.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct StageShadow {
    /// xyz = unit vector from a surface towards the stage's light (set 0 of
    /// the light script, right-handed world space), for non-bump draws' shadow
    /// weight (their face normal stands in for a mapped one).
    #[uniform(100)]
    pub to_light: Vec4,
    /// The light set (`light_buffer`, shared by every stage draw).
    #[storage(101, read_only)]
    pub light: Handle<ShaderBuffer>,
    #[uniform(102)]
    pub mat: StageMatUniform,
    /// The stage's shadow volumes (`shadow_grid.rs`), shared with every other
    /// receiver; the empty buffer until the stage's grid is built.
    #[storage(103, read_only)]
    pub vol: Handle<ShaderBuffer>,
}

/// A stage mesh piece: its authored `StandardMaterial` + the receiving shader.
pub type StageMaterial = ExtendedMaterial<StandardMaterial, StageShadow>;

impl MaterialExtension for StageShadow {
    /// `SBTS_vs`'s point light is a per-vertex varying (o4): see `point_o4`.
    fn vertex_shader() -> ShaderRef {
        STAGE_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn fragment_shader() -> ShaderRef {
        STAGE_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
}

/// `PD_STAGE_POINT_PIXEL=1`: the effect light on the stage evaluated per pixel
/// (before 2026-10-09), not per vertex like `SBTS_vs`. For A/B only.
pub fn point_light_per_pixel() -> bool {
    std::env::var("PD_STAGE_POINT_PIXEL").is_ok_and(|v| v == "1")
}

/// Registers the shader, the material pipeline and the effect-light system.
/// Called by `shadows::plugin`.
pub fn register(app: &mut App) {
    let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("stage_material.wgsl"), "pd_game/stage_material.wgsl"));
    let _ = STAGE_SHADER.set(shader);
    app.add_plugins(MaterialPlugin::<StageMaterial>::default());
    app.init_resource::<StageLightLive>();
    if let Some(mut buffers) = app.world_mut().get_resource_mut::<Assets<ShaderBuffer>>() {
        let h = buffers.add(ShaderBuffer::from(StageLightUniform::default()));
        let _ = LIGHT_BUFFER.set(h);
    }
    if per_pixel_enabled() && std::env::var("PD_STAGE_EFFECT_LIGHT").map_or(true, |v| v != "0") {
        app.add_systems(PostUpdate, update_effect_light.after(crate::effects::update_lights).run_if(resource_exists::<crate::shadows::ShadowStage>));
    }
}

/// The light set the stage materials currently carry (script set 0 plus the
/// playing effect light, slot 2 of the table). Written by `update_effect_light`;
/// new materials start from it.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct StageLightLive(pub Option<StageLightUniform>, pub Option<StageLightUniform>);

/// Height above the effect root at which a LIGHT02 lights the stage. The
/// program uses the emitter node's own world position (`FUN_14010c990`:
/// `*(emitter + 0x30..0x38)`); our effect root sits at the fighter's feet
/// (`effects::LIGHT_DECAL_Y`), so without a lift a floor would never face the
/// light. ~10 units = hand height of a 20-unit fighter. UNVERIFIED (audit R: "LIGHT02
/// height"); `PD_STAGE_LIGHT_Y` overrides.
const EFFECT_LIGHT_LIFT: f32 = 10.0;

/// The stage's effect light. In the program the stage (light group 0, `FUN_140199970`)
/// has ONE slot-2 light: the LIGHT02 record allocated last (`DAT_140527cc8`, set by
/// `FUN_14010c990` when an effect first allocates its record); it holds
/// colour lerp(word 12, 13), the emitter's position and the range scalar lerp(word 0,
/// 1) -- the same values the fighters' point light uses (`char_light::effect_light_of`)
/// -- and the point light is `colour^2 * sat(N.Lp)`, `|Lp| = range / d^2` (SBTS_ps).
/// Each view allocates its own record but they carry the same content, so
/// the stage sees the same light in every view. Here: the decal that started
/// playing last. Writes into every stage material only when it changes.
#[allow(clippy::too_many_arguments)]
fn update_effect_light(
    stage: Res<crate::shadows::ShadowStage>,
    decals: Query<(Entity, &crate::effects::FxLight, &Transform, &Visibility, &MeshMaterial3d<crate::effects::FxMaterial>)>,
    fx_mats: Res<Assets<crate::effects::FxMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut live: ResMut<StageLightLive>,
    mut seen: Local<(u64, std::collections::HashMap<Entity, u64>)>,
) {
    let scale = std::env::var("PD_LIGHT_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.02);
    let lift = std::env::var("PD_STAGE_LIGHT_Y").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(EFFECT_LIGHT_LIFT);
    seen.0 += 1;
    let tick = seen.0;
    let mut newest: Option<(u64, u32, crate::char_light::EffectLight)> = None;
    let mut alive = std::collections::HashSet::new();
    for (e, _, t, vis, m) in &decals {
        alive.insert(e);
        let first = *seen.1.entry(e).or_insert(tick);
        if *vis == Visibility::Hidden {
            continue;
        }
        let Some(mat) = fx_mats.get(&m.0) else { continue };
        let Some(mut light) = crate::char_light::effect_light_of(t, mat, scale) else { continue };
        light.pos.y += lift;
        if newest.as_ref().is_none_or(|(f, i, _)| (first, e.index_u32()) > (*f, *i)) {
            newest = Some((first, e.index_u32(), light));
        }
    }
    seen.1.retain(|e, _| alive.contains(e));
    // Proof harness: `PD_STAGE_TEST_LIGHT=x,y,z,range,r,g,b` (world position, range
    // scalar, gamma colour 0..1) holds a fixed effect light.
    if let Some(v) = std::env::var("PD_STAGE_TEST_LIGHT").ok() {
        let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [x, y, z, range, r, g, b, ..] = n[..] {
            newest = Some((0, 0, crate::char_light::EffectLight { colour: Vec3::new(r, g, b), pos: Vec3::new(x, y, z), range }));
        }
    }
    let mut want = stage.light;
    if let Some((_, _, e)) = newest {
        want.p0_col = e.colour.extend(0.0);
        want.p0_pos = e.pos.extend(e.range);
    }
    if live.0 != Some(want) {
        if std::env::var_os("PD_STAGE_LIGHT_LOG").is_some() {
            eprintln!("[stage_light] effect light col {:?} pos {:?} (decals {})", want.p0_col, want.p0_pos, decals.iter().count());
            for (e, _, t, vis, m) in &decals {
                let l = fx_mats.get(&m.0).and_then(|mat| crate::char_light::effect_light_of(t, mat, scale));
                eprintln!("[stage_light]   decal {e:?} first {:?} {vis:?} at {:?} light {:?}", seen.1.get(&e), t.translation, l.map(|l| (l.colour, l.range)));
            }
        }
        live.0 = Some(want);
        // One in-place write of the shared buffer (it used to be a rewrite of every stage material).
        write_light(&mut live, &mut buffers, want);
    }
}

/// Wraps an authored stage material. `bump`: the draw's `extras.pd_stage`
/// (`None` for every non-bump draw; ignored with `PD_STAGE_LIGHT=0`).
/// The light set itself goes into the shared buffer (`write_light`), not the material.
pub fn stage_material(base: StandardMaterial, to_light: Vec3, bump: Option<StageMatUniform>, vol: Handle<ShaderBuffer>) -> StageMaterial {
    let mut mat = bump.filter(|_| per_pixel_enabled()).unwrap_or_default();
    if point_light_per_pixel() {
        mat.params.z = 1.0;
    }
    StageMaterial { base, extension: StageShadow { to_light: to_light.extend(0.0), light: light_buffer(), mat, vol } }
}

/// The stage's light set 0 from `stNN.light.json` (written by `pd_convert`; an
/// older cache's file is read in file order, `stage_look::script_vec`).
/// Roles per the program's own block layout (stagedat.rs `LightSet`): the
/// script's first colour is the directional light (`cDL_COL`), the third the
/// hemisphere "up" colour (`cS0`), the second the "down" colour (`cS1`).
pub fn light_from_json(v: &serde_json::Value) -> Option<StageLightUniform> {
    let set = &v["stage"];
    let col = |x: &serde_json::Value| -> Option<Vec3> {
        let a = x.as_array()?;
        Some(Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32) / 255.0)
    };
    // Game space is left-handed: mirror Z into Bevy's world.
    let world = |x: &serde_json::Value| crate::stage_look::script_vec(v, x).map(|d| Vec3::new(d.x, d.y, -d.z));
    Some(StageLightUniform {
        dl_col: col(&set["sky"])?.extend(0.0),
        dl_dir: world(&set["dir"])?.normalize_or(Vec3::NEG_Y).extend(0.0),
        s0: col(&set["sun"])?.extend(0.0),
        s1: col(&set["ground"])?.extend(0.0),
        axis: world(&set["hemi_axis"])?.normalize_or(Vec3::Y).extend(0.0),
        p0_col: Vec4::ZERO,
        p0_pos: Vec4::ZERO,
    })
}

/// `extras.pd_stage` of a glb material (`pd_formats::gltf::Materials::
/// get_stage_bump`): `{ "pd_stage": { "spec": [R, G, B, power], "fresnel": n } }`.
pub fn mat_from_extras(extras: &str) -> Option<StageMatUniform> {
    let v: serde_json::Value = serde_json::from_str(extras).ok()?;
    let s = &v["pd_stage"];
    let a = s["spec"].as_array()?;
    let b = |i: usize| a.get(i).and_then(|x| x.as_u64()).map(|x| x.min(255) as u8);
    Some(StageMatUniform::bump([b(0)?, b(1)?, b(2)?, b(3)?], s["fresnel"].as_i64()? as i16))
}

// ---------------------------------------------------------------------------
// The original's maths, ported (SBTS_vs / SBTS_ps; tested against the
// disassembly with golden values, see the tests and docs/STAGE-LIGHT.md).
// ---------------------------------------------------------------------------

/// One fragment's inputs to `SBTS_ps` (what `stage_material.wgsl::bump` builds).
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct BumpFrag {
    /// Base texture, gamma space (`t0.rgb`) and its alpha.
    pub base: Vec3,
    pub base_a: f32,
    /// Decoded normal map `2 * (t1.rgb - 0.5)`, tangent space, unnormalised.
    pub n: Vec3,
    /// The normal map's alpha: the specular mask (`t1.a`).
    pub nm_a: f32,
    /// Vertex colour R (clamped to [-1, 1]): the multiplier on the lit colour.
    pub ao: f32,
    /// Vertex colour alpha: the specular weight.
    pub spec_w: f32,
    /// The frame, world space: tangent, bitangent, normal (unit).
    pub t: Vec3,
    pub b: Vec3,
    pub nrm: Vec3,
    pub world_pos: Vec3,
    pub eye_pos: Vec3,
}

/// What `SBTS_ps` writes: the lit colour (gamma space; the framebuffer clamps
/// it) and the alpha the shadow pass blends with.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct BumpOut {
    pub rgb: Vec3,
    pub alpha: f32,
}

/// `SBTS_vs`'s point-light output o4 at a VERTEX (`stage_material.wgsl::point_o4`):
/// `0.5 * (-to.T, -to.B, +to.N) * range / d^2 + 0.5`, `to` = unit vector from the
/// vertex to the light (READ, `SBTS_vs`: `add r1.xyz, v0, -cb0[7]` ... `mad o4.xyz`).
/// It is a varying: the rasteriser interpolates it between the vertices and only
/// then does `SBTS_ps` clamp it (`point_lp`). No light (colour 0) gives 0.5 = none.
pub fn point_o4(l: &StageLightUniform, pos: Vec3, t: Vec3, b: Vec3, nrm: Vec3) -> Vec3 {
    if l.p0_col.truncate() == Vec3::ZERO {
        return Vec3::splat(0.5);
    }
    let d = pos - l.p0_pos.truncate();
    let d2 = d.length_squared().max(1e-6);
    let to = -d / d2.sqrt();
    Vec3::new(-to.dot(t), -to.dot(b), to.dot(nrm)) * (l.p0_pos.w / d2) * 0.5 + Vec3::splat(0.5)
}

/// `SBTS_ps`'s decode of the interpolated o4: `2 * clamp(o4, -1, 1) - 1`
/// (`max v4, -1` / `min .., 1` then `mad x2 - 1`).
pub fn point_lp(o4: Vec3) -> Vec3 {
    o4.clamp(Vec3::splat(-1.0), Vec3::ONE) * 2.0 - Vec3::ONE
}

/// CPU mirror of `stage_material.wgsl::bump`, instruction by instruction from
/// `SBTS_vs` + `SBTS_ps` (docs/STAGE-LIGHT.md). `mat.params.x` is the fresnel
/// constant, `mat.spec` the specular colour bytes / 255; `cSPECULAR` =
/// that * 2 * `cDL_COL` (`FUN_1402a75e0`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn bump(l: &StageLightUniform, mat: &StageMatUniform, f: &BumpFrag) -> BumpOut {
    use crate::char_light::{lut_alpha, lut_rgb};
    let to_l = -l.dl_dir.truncate();
    // SBTS_vs: tangent-space vectors in the vertex frame (T, B, N).
    let ts = |v: Vec3, sx: f32, sy: f32, sz: f32| Vec3::new(sx * v.dot(f.t), sy * v.dot(f.b), sz * v.dot(f.nrm));
    let v = (f.eye_pos - f.world_pos).normalize_or_zero();
    let h = (v + to_l).normalize_or_zero();
    let l_ts = ts(to_l, 1.0, 1.0, 1.0); // o5 = (-DL.T, -DL.B, -DL.N)
    let h_ts = ts(h, 1.0, 1.0, 1.0); // o6 = normalize(V - DL) in the same frame
    let a_ts = ts(l.axis.truncate(), -1.0, -1.0, 1.0); // o3 = 0.5 * (-S0.T, -S0.B, +S0.N) + 0.5
    // Point light: o4 at this point (a vertex here; between vertices the
    // shader interpolates the vertices' o4, `point_o4`), decoded.
    let pc = l.p0_col.truncate();
    let lp = point_lp(point_o4(l, f.world_pos, f.t, f.b, f.nrm));
    // o3.w = a * (k - max(N.V, 0)), clamped by the pixel shader.
    let fres = (f.spec_w * (mat.params.x - f.nrm.dot(v).max(0.0))).clamp(-1.0, 1.0);

    // SBTS_ps.
    let lut_u = lut_rgb(f.n.dot(l_ts)); // t3 (N.L, N.H): rgb = the ramp itself
    let lut_a = lut_alpha(f.n.dot(h_ts));
    let nl = lut_u.clamp(0.0, 1.0);
    let alpha = 2.0 * (nl * 0.8 + 0.008_064_516) * (2.0 * f.base_a - 1.5);
    let p = f.n.dot(lp).clamp(0.0, 1.0);
    let dl_term = (Vec3::splat(lut_u) * l.dl_col.truncate()).clamp(Vec3::ZERO, Vec3::ONE);
    let h_raw = f.n.dot(a_ts) * 0.5 + 0.5;
    let amb = (l.s0.truncate() * h_raw.clamp(0.0, 1.0) + l.s1.truncate() * (1.0 - h_raw)).clamp(Vec3::ZERO, Vec3::ONE);
    let pt = (pc * p).clamp(Vec3::ZERO, Vec3::ONE);
    let dlp = (pt * pc + dl_term).clamp(Vec3::ZERO, Vec3::ONE);
    let lit = (dlp * f.base + f.base * amb).clamp(Vec3::ZERO, Vec3::ONE) * f.ao.clamp(-1.0, 1.0);
    let c_spec = mat.spec.truncate() * 2.0 * l.dl_col.truncate();
    let spec = (c_spec * (f.nm_a * lut_a * fres)).clamp(Vec3::ZERO, Vec3::ONE);
    BumpOut { rgb: lit.max(Vec3::ZERO) + spec, alpha }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every expected number below comes from `scratch` python that follows the
    // SBTS_vs / SBTS_ps disassembly one instruction at a time and samples the
    // real spec77.dds (clamp, bilinear) -- docs/STAGE-LIGHT.md "Golden values".

    /// A patch with T = +X, B = +Y, N = +Z, camera on the normal at 10 units.
    fn frame() -> BumpFrag {
        BumpFrag {
            base: Vec3::splat(0.5),
            base_a: 1.0,
            n: Vec3::new(0.0, 0.0, 1.0),
            nm_a: 1.0,
            ao: 1.0,
            spec_w: 1.0,
            t: Vec3::X,
            b: Vec3::Y,
            nrm: Vec3::Z,
            world_pos: Vec3::ZERO,
            eye_pos: Vec3::new(0.0, 0.0, 10.0),
        }
    }

    const DL: Vec3 = Vec3::new(0.8, 0.6, 0.4);
    const S0: Vec3 = Vec3::new(0.2, 0.25, 0.3);
    const S1: Vec3 = Vec3::new(0.1, 0.1, 0.05);

    fn set() -> StageLightUniform {
        StageLightUniform { dl_col: DL.extend(0.0), dl_dir: Vec4::new(0.0, 0.0, -1.0, 0.0), s0: S0.extend(0.0), s1: S1.extend(0.0), axis: Vec4::new(0.0, 0.0, 1.0, 0.0), ..Default::default() }
    }

    fn only(mut s: StageLightUniform, dl: bool, s0: bool, s1: bool) -> StageLightUniform {
        if !dl {
            s.dl_col = Vec4::ZERO;
        }
        if !s0 {
            s.s0 = Vec4::ZERO;
        }
        if !s1 {
            s.s1 = Vec4::ZERO;
        }
        s
    }

    fn nomat() -> StageMatUniform {
        StageMatUniform::bump([0, 0, 0, 0], 0)
    }

    fn close(a: Vec3, b: [f32; 3], eps: f32) {
        assert!((a - Vec3::from(b)).abs().max_element() < eps, "{a:?} vs {b:?}");
    }

    /// N.L = 1, flat map, hemisphere axis along the normal: out = base * (cDL + cS0).
    #[test]
    fn facing_the_light() {
        let o = bump(&set(), &nomat(), &frame());
        close(o.rgb, [0.5, 0.425, 0.35], 1e-4);
        assert!((o.alpha - 0.808_065).abs() < 1e-5); // A = 2 * (0.8 + 0.00806) * 0.5
    }

    /// The hemisphere term is `0.5 N.A + 0.5` (c7.w = 0.5): a map tilted 60
    /// degrees off the surface gives h = 0.75.
    #[test]
    fn hemisphere_blend_is_half_n_dot_axis_plus_half() {
        let mut f = frame();
        f.n = Vec3::new(0.866_025, 0.0, 0.5);
        let o = bump(&only(set(), false, true, true), &nomat(), &f);
        close(o.rgb, [0.0875, 0.10625, 0.11875], 1e-4);
        assert!((o.alpha - 0.408_065).abs() < 1e-4);
    }

    /// The directional light and the half vector use (+T, +B, +N); the
    /// hemisphere axis and the point light (-T, -B, +N) (SBTS_vs o3/o4 vs o5/o6).
    #[test]
    fn frame_signs_differ_between_the_light_families() {
        let mut s = only(set(), true, false, false);
        s.dl_dir = Vec4::new(-0.6, 0.0, -0.8, 0.0); // to_light (0.6, 0, 0.8)
        let mut f = frame();
        f.n = Vec3::new(0.7071, 0.0, 0.7071);
        close(bump(&s, &nomat(), &f).rgb, [0.396_745, 0.297_558, 0.198_372], 1e-3);
        f.n = Vec3::new(-0.7071, 0.0, 0.7071);
        close(bump(&s, &nomat(), &f).rgb, [0.056_006, 0.042_004, 0.028_003], 1e-3);
        // Hemisphere axis tilted towards +X, only cS0 = 1: a map tilted the same way sees less.
        let mut h = only(set(), false, true, false);
        h.s0 = Vec4::ONE;
        h.axis = Vec4::new(0.6, 0.0, 0.8, 0.0);
        f.n = Vec3::new(0.7071, 0.0, 0.7071);
        close(bump(&h, &nomat(), &f).rgb, [0.285_355; 3], 1e-3);
        f.n = Vec3::new(-0.7071, 0.0, 0.7071);
        close(bump(&h, &nomat(), &f).rgb, [0.497_485; 3], 1e-3);
    }

    /// Point light: colour * sat(N.Lp) * colour (the pixel shader squares the
    /// colour: `mad_sat r1.xzw, r1.xxzw, cb0[5].xxyz, r2.xxyz`), |Lp| = range / d^2.
    #[test]
    fn the_point_light_is_interpolated_between_vertices_then_clamped() {
        // The Bomb's flight light (eft0031 LIGHT02 of model 1: range 1500) 8 u in front of the
        // middle of a 120 u wide face. Per pixel (the old build) the middle saturates: P = 1, a
        // fully lit panel with the face's hard edges. SBTS_vs makes o4 at the VERTICES (all
        // ~85 u away: o4.z ~ 0.5 + 0.5 * 1500 / 85^2 * 0.09) and the rasteriser blends those,
        // so the middle of the face is barely lit, as in the program.
        let mut s = set();
        s.p0_col = Vec4::new(1.0, 0.5, 0.25, 0.0);
        s.p0_pos = Vec4::new(0.0, 0.0, 8.0, 1500.0);
        let (t, b, n) = (Vec3::X, Vec3::Y, Vec3::Z);
        let corners = [Vec3::new(-60.0, -60.0, 0.0), Vec3::new(60.0, -60.0, 0.0), Vec3::new(60.0, 60.0, 0.0), Vec3::new(-60.0, 60.0, 0.0)];
        let centre_o4 = corners.iter().map(|&c| point_o4(&s, c, t, b, n)).sum::<Vec3>() / 4.0;
        let per_vertex = point_lp(centre_o4).dot(Vec3::Z).clamp(0.0, 1.0);
        let per_pixel = point_lp(point_o4(&s, Vec3::ZERO, t, b, n)).dot(Vec3::Z).clamp(0.0, 1.0);
        assert_eq!(per_pixel, 1.0, "per pixel the face under the light saturates");
        assert!(per_vertex < 0.15, "interpolated from the far corners the middle stays dim: {per_vertex}");
        // No light: o4 = 0.5 decodes to nothing.
        assert_eq!(point_lp(point_o4(&set(), Vec3::ZERO, t, b, n)), Vec3::ZERO);
    }

    #[test]
    fn point_light_is_squared_and_falls_off_with_the_distance_squared() {
        let mut s = only(set(), false, false, false);
        s.p0_col = DL.extend(0.0);
        s.p0_pos = Vec4::new(0.0, 0.0, 10.0, 50.0); // straight above at d = 10
        close(bump(&s, &nomat(), &frame()).rgb, [0.16, 0.09, 0.04], 1e-4);
        s.p0_pos = Vec4::new(0.0, 0.0, 20.0, 50.0); // twice as far: a quarter
        close(bump(&s, &nomat(), &frame()).rgb, [0.04, 0.0225, 0.01], 1e-4);
    }

    /// Specular: sat(nm.a * lut.a(N.H) * a * (k - N.V) * cSPEC), cSPEC = byte *
    /// 2/255 * cDL_COL. Camera and light on the normal: N.H = 1, lobe 1; k = 2
    /// (short 100), N.V = 1 -> weight 1. The sum is not saturated here (the
    /// framebuffer does that).
    #[test]
    fn specular_lobe_and_fresnel_weight() {
        let s = only(set(), true, false, false);
        let m = StageMatUniform::bump([0xff, 0x80, 0x00, 20], 100);
        close(bump(&s, &m, &frame()).rgb, [1.4, 0.902_353, 0.2], 2e-3);
        let mut f = frame();
        f.spec_w = 0.0; // the vertex alpha kills the lobe
        close(bump(&s, &m, &f).rgb, [0.4, 0.3, 0.2], 1e-5);
    }

    /// The vertex colour's R multiplies the lit colour (not the specular).
    #[test]
    fn vertex_r_multiplies_the_lit_colour() {
        let mut f = frame();
        f.ao = 0.5;
        close(bump(&set(), &nomat(), &f).rgb, [0.25, 0.2125, 0.175], 1e-4);
    }

    /// Alpha = 2 (0.8 sat(N.L) + 0.00806) (2a - 1.5): facing away leaves 0.00806
    /// (a = 1), a texture alpha of 0.75 writes 0 (the shader's own cutout edge).
    #[test]
    fn alpha_is_the_shadow_passes_weight() {
        let mut f = frame();
        f.n = Vec3::new(0.0, 0.0, -1.0);
        let o = bump(&set(), &nomat(), &f);
        assert!((o.alpha - 0.008_065).abs() < 1e-5);
        close(o.rgb, [0.05, 0.05, 0.025], 1e-4);
        let mut f = frame();
        f.base_a = 0.75;
        assert!(bump(&set(), &nomat(), &f).alpha.abs() < 1e-6);
    }

    /// A st03-like case: oblique frame, tilted map, a light from the stage script
    /// (dir (0.546, -0.677, -0.4925) in the program's order), camera off to the side.
    #[test]
    fn oblique_frame_with_the_stage_script_light() {
        let t = Vec3::new(1.0, 0.0, 0.1).normalize();
        let n = Vec3::new(-0.1, 0.0, 1.0).normalize();
        let f = BumpFrag {
            base: Vec3::new(0.4, 0.5, 0.6),
            base_a: 1.0,
            n: Vec3::new(0.2, -0.3, 0.93).normalize(),
            nm_a: 1.0,
            ao: 0.9,
            spec_w: 0.8,
            t,
            b: Vec3::Y,
            nrm: n,
            world_pos: Vec3::new(3.0, 4.0, 5.0),
            eye_pos: Vec3::new(40.0, 30.0, 60.0),
        };
        let l = StageLightUniform {
            dl_col: (Vec3::new(105.0, 120.0, 130.0) / 255.0).extend(0.0),
            dl_dir: Vec3::new(0.546, -0.677, -0.4925).normalize().extend(0.0),
            s0: (Vec3::new(125.0, 130.0, 135.0) / 255.0).extend(0.0),
            s1: (Vec3::new(20.0, 27.0, 30.0) / 255.0).extend(0.0),
            axis: Vec3::new(0.0076, 0.9961, -0.0876).normalize().extend(0.0),
            ..Default::default()
        };
        let o = bump(&l, &StageMatUniform::bump([0x4c, 0x4c, 0x4c, 20], 50), &f);
        // The python reference used spec colour 0.3 flat (0x4c = 0.298) and k = 1.5.
        close(o.rgb, [0.148_709, 0.201_484, 0.255_028], 4e-3);
        assert!((o.alpha - 0.171_096).abs() < 2e-3);
    }

    /// st03's light set 0 as the cache stores it: the script's file lists
    /// each vector as (z, y, x), the program's world x is the third float.
    #[test]
    fn st03_light_set_roles_and_order() {
        let file_order = serde_json::json!({
            "stage": { "dir": [-0.4925040006637573, -0.6772599816322327, 0.5460720062255859],
                       "hemi_axis": [-0.08756399899721146, 0.9961299896240234, 0.007577999960631132],
                       "sky": [105, 120, 130], "ground": [20, 27, 30], "sun": [125, 130, 135] },
        });
        let u = light_from_json(&file_order).unwrap();
        assert!((u.dl_col.truncate() - Vec3::new(105.0, 120.0, 130.0) / 255.0).length() < 1e-6, "first colour = the directional colour");
        assert!((u.s0.truncate() - Vec3::new(125.0, 130.0, 135.0) / 255.0).length() < 1e-6, "third colour = hemisphere up");
        assert!((u.s1.truncate() - Vec3::new(20.0, 27.0, 30.0) / 255.0).length() < 1e-6, "second colour = hemisphere down");
        // Travel direction LH (0.546, -0.677, -0.4925) -> world (x, y, -z).
        assert!((u.dl_dir.truncate() - Vec3::new(0.546_072, -0.677_26, 0.492_504)).length() < 1e-3);
        assert!((u.axis.truncate() - Vec3::new(0.007_578, 0.996_13, 0.087_564)).length() < 1e-3);
        // A re-exported json (tagged, already swapped) gives the same.
        let tagged = serde_json::json!({
            "vec_order": "zyx",
            "stage": { "dir": [0.5460720062255859, -0.6772599816322327, -0.4925040006637573],
                       "hemi_axis": [0.007577999960631132, 0.9961299896240234, -0.08756399899721146],
                       "sky": [105, 120, 130], "ground": [20, 27, 30], "sun": [125, 130, 135] },
        });
        assert_eq!(light_from_json(&tagged).unwrap(), u);
    }

    /// The material extras decode to the shader constants.
    #[test]
    fn material_constants_decode_like_the_program() {
        let m = StageMatUniform::bump([0x4c, 0x4c, 0x4c, 0x14], 100);
        assert!((m.spec.x - 76.0 / 255.0).abs() < 1e-6 && m.spec.w == 20.0);
        assert!((m.params.x - 2.0).abs() < 1e-6 && m.params.y == 1.0);
        assert!((StageMatUniform::bump([0; 4], -100).params.x).abs() < 1e-6);
        let e = mat_from_extras(r#"{"pd_stage":{"spec":[76,76,76,20],"fresnel":100,"flags":17453}}"#).unwrap();
        assert_eq!(e, m);
        assert!(mat_from_extras("not json").is_none());
    }
}
