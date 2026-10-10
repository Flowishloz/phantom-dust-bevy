//! Player-character lighting: the original's `SWBOS` pixel shader, rebuilt.
//!
//! Replaces Bevy's PBR for fighters. The compiled shader (`SWBOS_ps.cso`,
//! disassembled with `tools/disasm`; instruction-by-instruction in
//! docs/CHAR-LIGHT.md) computes, in **gamma space** with no tone map:
//!
//! ```text
//! rgb = sat( base * ( cS0 + cDL * LUT(N.L).rgb + cP0 * sat(N.Lp) + cS1 * (N.A)^2 )
//!            + sat( vtx.w * normalmap.a * LUT(N.H).a * cSPEC ) )
//! ```
//!
//! `LUT` is `Assets/Data/spec77.dds` (clamp-addressed, bilinear): rgb = the
//! u coordinate itself (so the diffuse term is plain Lambert), alpha = a
//! specular lobe `floor(255 * v^20) / 255` in v (exact over all 256 rows).
//! `N` is the object-space normal map rotated by the skin (`light_tangent`
//! in pd_formats); `A` the hemisphere axis; `Lp` the point-light vector
//! scaled by `w / d^2`; `H` = -(D + 0.5 F) normalised with D the light's travel
//! direction and F the camera forward (`FUN_1402a69c0`).
//!
//! Where the shader lives: `char_light.wgsl` (an import module) is called from
//! `dissolve.wgsl`, the fighters' one material (`cam_fade::DissolveMaterial`),
//! so the approved wall dissolve and this lighting share one fragment path.
//! The light set comes from the stage's script (`stage_look::CharLightSet`).
//!
//! Output is converted gamma -> linear before Bevy's tone mapper (Laurence's
//! chosen default), exactly as the stage's baked vertex colours are.
//!
//! `shade` below is a CPU mirror of the WGSL, used by the tests with golden
//! values computed independently from the assembly (docs/CHAR-LIGHT.md).

use crate::stage_look::CharLightSet;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;
use std::path::Path;

/// `spec77.dds` specular power: alpha row v = floor(255 * (v/255)^20).
#[cfg_attr(not(test), allow(dead_code))]
pub const LUT_POWER: i32 = 20;

/// `spec77.dds` RGB at sample coordinate `c` (0..1 across the 256 texels,
/// clamp address, bilinear): texel i holds i/255.
#[cfg_attr(not(test), allow(dead_code))]
pub fn lut_rgb(c: f32) -> f32 {
    ((c * 256.0 - 0.5) / 255.0).clamp(0.0, 1.0)
}

/// `spec77.dds` alpha at sample coordinate `c`: texel i holds
/// `floor(255 * (i/255)^20) / 255`, interpolated between texels.
#[cfg_attr(not(test), allow(dead_code))]
pub fn lut_alpha(c: f32) -> f32 {
    let t = (c * 256.0 - 0.5).clamp(0.0, 255.0);
    let i = t.floor() as i32;
    let f = t - i as f32;
    let texel = |i: i32| -> f32 {
        let i = i.clamp(0, 255);
        ((255.0 * (i as f32 / 255.0).powi(LUT_POWER)).floor()) / 255.0
    };
    texel(i) * (1.0 - f) + texel(i + 1) * f
}

/// GPU-side block, bound at material binding 101 (`char_light.wgsl::CharLight`).
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct CharLightUniform {
    /// xyz = `cDL_COL` (gamma 0..1, scene-fade applied).
    pub dl_col: Vec4,
    /// xyz = `cDL_VEC`, light travel direction (world, unit).
    pub dl_dir: Vec4,
    /// xyz = `cS0_COLp`.
    pub s0: Vec4,
    /// xyz = `cS1_COLp`.
    pub s1: Vec4,
    /// xyz = hemisphere axis (world, unit).
    pub axis: Vec4,
    /// xyz = `cP0_COLp` (0 = no point light).
    pub p0_col: Vec4,
    /// xyz = `cP0_POS` world position, w = `cP0_POS.w` (intensity / d^2).
    pub p0_pos: Vec4,
    /// xyz = `cSPECULAR` = material specular * 2/255 * `cDL_COL`.
    pub spec: Vec4,
    /// "Effect lights: 4 per fighter" (look.rs): up to three more point lights
    /// in the same form as `p0_col` / `p0_pos`, each added like the first.
    /// All zero in the original look (one light), so the shader adds nothing.
    pub x_col: [Vec4; 3],
    pub x_pos: [Vec4; 3],
}

impl CharLightUniform {
    /// `spec_rgb`: the material's specular colour as 0..1 (byte / 255).
    pub fn new(set: &CharLightSet, spec_rgb: Vec3) -> Self {
        // FUN_1402a75e0: R = byte * DL.r * (2/255)  (DAT_1402f7c68 = 2/255).
        let spec = spec_rgb * 2.0 * set.dl_col;
        Self {
            dl_col: set.dl_col.extend(0.0),
            dl_dir: set.dir.extend(0.0),
            s0: set.s0.extend(0.0),
            s1: set.s1.extend(0.0),
            axis: set.axis.extend(0.0),
            p0_col: Vec4::ZERO,
            p0_pos: Vec4::ZERO,
            spec: spec.extend(0.0),
            x_col: [Vec4::ZERO; 3],
            x_pos: [Vec4::ZERO; 3],
        }
    }
}

/// How many effect lights a fighter can have at once with "Effect lights: 4 per fighter" (the first is the original's slot-2 light).
pub const MAX_EFFECT_LIGHTS: usize = 4;

/// One fragment's inputs (what `char_light.wgsl::shade` is handed).
#[cfg_attr(not(test), allow(dead_code))]
pub struct Frag {
    /// Base colour texture, gamma space.
    pub base: Vec3,
    /// Mapped normal in world space (the normal map through the skin frame).
    pub n: Vec3,
    /// `COLOR0.w`: the per-vertex specular weight (TEXCOORD_1.x).
    pub spec_w: f32,
    /// Normal-map alpha (the specular mask).
    pub nm_a: f32,
    pub world_pos: Vec3,
    /// Camera forward (world).
    pub view_fwd: Vec3,
}

/// CPU mirror of the WGSL `shade`: returns the gamma-space output rgb.
#[cfg_attr(not(test), allow(dead_code))]
pub fn shade(l: &CharLightUniform, f: &Frag) -> Vec3 {
    let to_light = -l.dl_dir.truncate();
    // t3 coordinates (SWBOS_ps): u = N.L, v = N.H (clamped by the sampler).
    let u = f.n.dot(to_light);
    let s = (l.dl_dir.truncate() + 0.5 * f.view_fwd).normalize_or_zero();
    let v = -f.n.dot(s);
    let ramp = lut_rgb(u);
    let lobe = lut_alpha(v);
    // Point light: direction-to-light scaled by w / d^2, each component kept
    // as the pixel shader sees it after the COLOR interpolant encode/clamp
    // (e = 0.5 x + 0.5 clamped to [-1,1], decoded 2e - 1).
    let mut p = Vec3::ZERO;
    let lights = std::iter::once((l.p0_col, l.p0_pos)).chain(l.x_col.iter().copied().zip(l.x_pos.iter().copied()));
    for (col, pos) in lights {
        let pc = col.truncate();
        if pc != Vec3::ZERO {
            let d = f.world_pos - pos.truncate();
            let d2 = d.length_squared().max(1e-6);
            let lp = (-d / d2.sqrt()) * (pos.w / d2);
            let e = (lp * 0.5 + 0.5).clamp(Vec3::splat(-1.0), Vec3::ONE);
            let dec = e * 2.0 - 1.0;
            p += pc * f.n.dot(dec).clamp(0.0, 1.0);
        }
    }
    // r1.y = 2 w (N.A - 0.5) + w with w = 0.5 (FUN_1402a7880 sets c5.w = 0.5).
    let h = f.n.dot(l.axis.truncate());
    let hemi = h * h;
    let lit = l.s0.truncate() + l.dl_col.truncate() * ramp + p + l.s1.truncate() * hemi;
    let spec = (l.spec.truncate() * (f.spec_w * f.nm_a * lobe)).clamp(Vec3::ZERO, Vec3::ONE);
    (f.base * lit + spec).clamp(Vec3::ZERO, Vec3::ONE)
}

/// A scene-wide light flash / dim: `FUN_14022e1d0` multiplies the colours of
/// every light group by `(target - 1) * f + 1` per channel, where `f` is a
/// triangle envelope over `total` frames that peaks at `peak * total`
/// (`+0x1d4`, `+0x1dc`, frame counter `+0x1d8`). Targets seen in the program:
/// `FUN_14017ba00` (1.5, 1.5, 2.0 over 15 frames, peak 0.05) and
/// `FUN_14021bc20` (0.2, 0.1, 0.6 over 45 frames). What triggers them is
/// unconfirmed (audit R6), so nothing in the game calls `trigger` yet.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct CharLightFade {
    pub active: bool,
    pub frame: u32,
    pub total: u32,
    pub peak: f32,
    pub target: Vec3,
}

impl CharLightFade {
    #[allow(dead_code)]
    pub fn trigger(target: Vec3, total: u32, peak: f32) -> Self {
        Self { active: total > 0, frame: 0, total, peak, target }
    }

    /// Envelope value 0..1 at the current frame (the program's two branches).
    pub fn envelope(&self) -> f32 {
        if !self.active || self.total == 0 {
            return 0.0;
        }
        let total = self.total as f32;
        let t = self.frame as f32;
        let peak_frames = self.peak * total;
        let f = if self.peak <= t / total {
            let i = peak_frames as i32 as f32; // (int) cast in the program
            1.0 - (t - i) / (total - i)
        } else {
            t / peak_frames
        };
        f.clamp(0.0, 1.0)
    }

    /// Per-channel colour multiplier.
    pub fn scale(&self) -> Vec3 {
        if !self.active {
            return Vec3::ONE;
        }
        (self.target - Vec3::ONE) * self.envelope() + Vec3::ONE
    }

    /// One sim tick: the program advances the counter every tick and stops
    /// once it passes `total`.
    pub fn step(&mut self) {
        if !self.active {
            return;
        }
        self.frame += 1;
        if self.frame > self.total {
            self.active = false;
        }
    }
}

/// An effect light (LIGHT02) as the character shader sees it (slot 2 of the
/// light table, `FUN_140199970`/`FUN_140199e70`): colour (gamma 0..1), world
/// position, and the point light's scalar (`cP0_POS.w`, the light's range
/// word: intensity is `w / d^2`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectLight {
    pub colour: Vec3,
    pub pos: Vec3,
    pub range: f32,
}

/// The fighters' light state. `set` comes from the stage's script; `spec` is
/// each fighter's material specular colour (0..1) from its glb.
#[derive(Resource, Clone, Debug)]
pub struct CharLight {
    pub set: CharLightSet,
    pub spec: [Vec3; crate::combat::MAX_PLAYERS],
    pub fade: CharLightFade,
    /// The effect light currently replacing slot 2 for each fighter.
    pub effect: [Option<EffectLight>; crate::combat::MAX_PLAYERS],
    /// The next-strongest effect lights per fighter ("Effect lights: 4 per
    /// fighter", look.rs); always empty with `max_lights == 1`.
    pub extra: [[Option<EffectLight>; MAX_EFFECT_LIGHTS - 1]; crate::combat::MAX_PLAYERS],
    /// Effect lights per fighter: 1 = the original's single slot (default).
    pub max_lights: usize,
}

impl CharLight {
    pub fn new(set: Option<CharLightSet>, spec: [Vec3; crate::combat::MAX_PLAYERS]) -> Self {
        let mut set = set.unwrap_or_else(CharLightSet::fallback);
        // Debug / turntable: rotate the light (and the hemisphere axis' XZ
        // tilt) about the vertical axis by PD_CHAR_LIGHT_YAW degrees.
        if let Some(deg) = std::env::var("PD_CHAR_LIGHT_YAW").ok().and_then(|v| v.parse::<f32>().ok()) {
            let q = Quat::from_rotation_y(deg.to_radians());
            set.dir = q * set.dir;
            set.axis = q * set.axis;
        }
        Self { set, spec, fade: CharLightFade::default(), effect: [None; crate::combat::MAX_PLAYERS], extra: [[None; MAX_EFFECT_LIGHTS - 1]; crate::combat::MAX_PLAYERS], max_lights: 1 }
    }

    pub fn uniform(&self, slot: usize) -> CharLightUniform {
        let slot = slot.min(crate::combat::MAX_PLAYERS - 1);
        let k = self.fade.scale();
        let faded = CharLightSet { dl_col: self.set.dl_col * k, s0: self.set.s0 * k, s1: self.set.s1 * k, ..self.set };
        let mut u = CharLightUniform::new(&faded, self.spec[slot]);
        if let Some(e) = self.effect[slot] {
            u.p0_col = e.colour.extend(0.0);
            u.p0_pos = e.pos.extend(e.range);
        }
        for (i, e) in self.extra[slot].iter().enumerate().take(self.max_lights.saturating_sub(1)) {
            if let Some(e) = e {
                u.x_col[i] = e.colour.extend(0.0);
                u.x_pos[i] = e.pos.extend(e.range);
            }
        }
        u
    }
}

/// One tick of the scene-light fade (sim rate, like the program's).
pub fn step_fade(mut light: Option<ResMut<CharLight>>) {
    if let Some(l) = light.as_mut() {
        if l.fade.active {
            l.fade.step();
        }
    }
}

/// Picks, per fighter, the playing LIGHT02 decal that lights it most
/// (`range / d^2`) and makes it the point light: the program swaps the
/// effect-light record into slot 2 (`FUN_140199970`, `FUN_140199e70`); its
/// position is the effect root's, colour and range the decal's own current
/// values (`effects::update_lights` computes them; read here from the decal
/// entity so effects.rs stays untouched). `PD_CHAR_EFFECT_LIGHT=0` disables.
pub fn gather_effect_lights(
    light: Option<ResMut<CharLight>>,
    decals: Query<(&crate::effects::FxLight, &Transform, &Visibility, &MeshMaterial3d<crate::effects::FxMaterial>)>,
    fx_mats: Res<Assets<crate::effects::FxMaterial>>,
    bodies: Query<(&crate::PlayerVisual, &GlobalTransform)>,
) {
    let Some(mut light) = light else { return };
    let scale = crate::effects::env_once("PD_LIGHT_SCALE").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.02);
    let off = crate::effects::env_once("PD_CHAR_EFFECT_LIGHT").is_some_and(|v| v == "0");
    let mut found: Vec<EffectLight> = Vec::new();
    if !off {
        for (_, t, vis, m) in &decals {
            if *vis == Visibility::Hidden {
                continue;
            }
            let Some(mat) = fx_mats.get(&m.0) else { continue };
            if let Some(e) = effect_light_of(t, mat, scale) {
                found.push(e);
            }
        }
    }
    let mut want: [Option<EffectLight>; crate::combat::MAX_PLAYERS] = [None; crate::combat::MAX_PLAYERS];
    let mut want_extra = [[None; MAX_EFFECT_LIGHTS - 1]; crate::combat::MAX_PLAYERS];
    let extras = light.max_lights.clamp(1, MAX_EFFECT_LIGHTS) - 1;
    for (v, gt) in &bodies {
        if v.0 >= crate::combat::MAX_PLAYERS {
            continue;
        }
        let p = gt.translation();
        if extras == 0 {
            want[v.0] = found.iter().copied().max_by(|a, b| effect_score(a, p).total_cmp(&effect_score(b, p)));
        } else {
            // The strongest first (the original's slot), then the next ones.
            let ranked = rank_lights(&found, p, extras + 1);
            want[v.0] = ranked.first().copied();
            for (i, e) in ranked.iter().skip(1).enumerate() {
                want_extra[v.0][i] = Some(*e);
            }
        }
    }
    if light.effect != want || light.extra != want_extra {
        if std::env::var_os("PD_CHAR_LIGHT_LOG").is_some() {
            eprintln!("[char_light] decals {} effect lights {:?} extra {:?}", decals.iter().count(), want, want_extra);
        }
        light.effect = want;
        light.extra = want_extra;
    }
}

/// The `n` lights that light `at` most, strongest first (`effect_score`).
pub fn rank_lights(found: &[EffectLight], at: Vec3, n: usize) -> Vec<EffectLight> {
    let mut v: Vec<EffectLight> = found.to_vec();
    v.sort_by(|a, b| effect_score(b, at).total_cmp(&effect_score(a, at)));
    v.truncate(n);
    v
}

/// One LIGHT02 decal as a light: colour (gamma 0..1) times the decal's
/// current intensity, the decal's world position, and the raw range scalar
/// (`cP0_POS.w`: the decal's scale undoes `effects::range_scale`). `None`
/// while the light is faded out. Shared with the stage (`stage_light`).
pub fn effect_light_of(t: &Transform, mat: &crate::effects::FxMaterial, scale: f32) -> Option<EffectLight> {
    let c = mat.base.base_color.to_srgba();
    let intensity = c.alpha;
    if intensity <= 0.0 {
        return None;
    }
    Some(EffectLight { colour: Vec3::new(c.red, c.green, c.blue) * intensity, pos: t.translation, range: t.scale.x * 0.5 / scale.max(1e-6) })
}

fn effect_score(e: &EffectLight, at: Vec3) -> f32 {
    e.range * e.colour.max_element() / (e.pos.distance_squared(at) + 1.0)
}

static CHAR_LIGHT_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();

/// Registers the `pd_game::char_light` WGSL import module (used by
/// `dissolve.wgsl`). Call once, before the first fighter renders.
pub fn plugin(app: &mut App) {
    let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("char_light.wgsl"), "pd_game/char_light.wgsl"));
    let _ = CHAR_LIGHT_SHADER.set(shader);
}

/// Per frame: write each fighter's light into the shared per-fighter block
/// (`cam_fade::FighterBlock`, bound by every body piece). Only a changed value
/// marks the block for upload, and the piece materials are never touched.
pub fn update_char_light(light: Option<Res<CharLight>>, mut state: ResMut<crate::cam_fade::FadeState>) {
    let Some(light) = light else { return };
    for slot in 0..crate::combat::MAX_PLAYERS {
        state.set_light(slot, light.uniform(slot));
    }
}

/// The specular colour the exporter wrote into `asset.extras.pd_spec`
/// ([R, G, B, power] bytes, from the SWBOS material's +0x34 bytes), as 0..1.
/// 0.5 (byte 0x7f-ish) when missing -- the common value.
pub fn read_spec(cache: &Path, id: &str) -> Vec3 {
    let fallback = Vec3::splat(0.498);
    let Ok(bytes) = std::fs::read(cache.join("chars").join(format!("{id}.glb"))) else { return fallback };
    spec_from_glb(&bytes).unwrap_or(fallback)
}

fn spec_from_glb(bytes: &[u8]) -> Option<Vec3> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return None;
    }
    let len = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    let json: serde_json::Value = serde_json::from_slice(bytes.get(20..20 + len)?).ok()?;
    let a = json["asset"]["extras"]["pd_spec"].as_array()?;
    Some(Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> CharLightSet {
        // A simple known set: key straight down, hemisphere axis up.
        CharLightSet { dl_col: Vec3::new(0.8, 0.6, 0.4), s0: Vec3::new(0.2, 0.25, 0.3), s1: Vec3::new(0.1, 0.1, 0.05), dir: Vec3::NEG_Y, axis: Vec3::Y }
    }
    fn frag(n: Vec3) -> Frag {
        Frag { base: Vec3::new(0.5, 0.5, 0.5), n, spec_w: 1.0, nm_a: 1.0, world_pos: Vec3::ZERO, view_fwd: Vec3::NEG_Z }
    }
    fn close(a: Vec3, b: [f32; 3], eps: f32) {
        assert!((a - Vec3::from(b)).abs().max_element() < eps, "{a:?} vs {b:?}");
    }

    /// spec77.dds (256x256 BGRA8, clamp/bilinear sampler: `FUN_1402a4170`
    /// sets sampler 3 to address 3 = clamp, filter 2 = linear) is a pure
    /// ramp in x: texel i = (i,i,i)/255, for every row.
    #[test]
    fn lut_x_ramp_is_identity() {
        assert_eq!(lut_rgb(-0.5), 0.0); // negative N.L clamps: no back-face light
        assert_eq!(lut_rgb(1.0), 1.0);
        assert!((lut_rgb(0.5) - (127.5 / 255.0)).abs() < 1e-6);
        // texel centres: texel 200 sits at (200 + 0.5) / 256
        assert!((lut_rgb(200.5 / 256.0) - 200.0 / 255.0).abs() < 1e-6);
        // Within 0.002 of plain N.L across the usable range.
        for k in 1..100 {
            let c = k as f32 / 100.0;
            assert!((lut_rgb(c) - c).abs() < 0.0025, "c={c}");
        }
    }

    /// The alpha column of spec77.dds is floor(255 * (v/255)^20), exactly
    /// (checked against every one of the 256 rows of the real file).
    #[test]
    fn lut_alpha_lobe_is_the_twentieth_power() {
        // Golden texels read from the file: rows 200, 220, 240, 250, 254, 255.
        for (row, want) in [(100, 0.0), (190, 0.0), (200, 1.0), (220, 13.0), (240, 75.0), (250, 171.0), (254, 235.0), (255, 255.0)] {
            let c = (row as f32 + 0.5) / 256.0;
            assert!((lut_alpha(c) - want / 255.0).abs() < 1e-5, "row {row}");
        }
        // Lobe only near N.H = 1: half way up it is zero.
        assert_eq!(lut_alpha(0.5), 0.0);
        assert!(lut_alpha(0.99) > 0.6);
        // File check (skipped without the game files).
        let p = Path::new(r"..\..\..\game_files\Assets\Data\spec77.dds");
        if let Ok(b) = std::fs::read(p) {
            for row in 0..256usize {
                let px = &b[128 + row * 256 * 4..][..4];
                // BGRA: alpha = floor(255 (row/255)^20)
                let want = ((255.0 * (row as f32 / 255.0).powi(20)).floor()) as u8;
                assert_eq!(px[3], want, "alpha row {row}");
            }
        }
    }

    /// Golden values from the `SWBOS_ps` instruction sequence, worked by an
    /// independent script (docs/CHAR-LIGHT.md "Golden values"): a surface
    /// facing straight up under a light travelling straight down, camera
    /// looking along -Z, no point light, vertex weight and mask 1.
    /// N.L = 1 -> ramp 1 (lut_rgb(1) = 1); hemisphere (N.A)^2 = 1;
    /// rgb = 0.5 * (s0 + dl + s1) + spec.
    /// S = normalize((0,-1,0) + 0.5 (0,0,-1)); N.H = -N.S = 0.894427;
    /// lobe(0.894427) = 0.109595 (texels 228/229 = 27/255, 29/255, f = 0.4715).
    #[test]
    fn golden_up_facing_under_a_top_light() {
        let s = set();
        let u = CharLightUniform::new(&s, Vec3::splat(127.0 / 255.0));
        let f = frag(Vec3::Y);
        let lobe = lut_alpha(0.894427191);
        assert!((lobe - 0.109595).abs() < 2e-5, "lobe {lobe}");
        let out = shade(&u, &f);
        // spec = sat(1 * 1 * lobe * 2*127/255 * dl_col)
        let k = 2.0 * 127.0 / 255.0;
        let want_spec = [lobe * k * 0.8, lobe * k * 0.6, lobe * k * 0.4];
        // diffuse part: 0.5 * ((0.2,0.25,0.3) + (0.8,0.6,0.4) + (0.1,0.1,0.05))
        let want = [0.5 * 1.1 + want_spec[0], 0.5 * 0.95 + want_spec[1], 0.5 * 0.75 + want_spec[2]];
        close(out, want, 1e-5);
        // and the literal numbers (independent script):
        close(out, [0.637332, 0.540499, 0.418666], 2e-5);
    }

    /// Facing away from the light: Lambert term 0 (LUT clamps), the hemisphere
    /// term still applies (squared, so it is symmetric) -- the program's own
    /// quirk, `(2 w N.A)^2` with w = 0.5.
    #[test]
    fn golden_facing_away_keeps_ambient_and_squared_hemisphere() {
        let s = set();
        let u = CharLightUniform::new(&s, Vec3::splat(0.5));
        let out = shade(&u, &Frag { spec_w: 0.0, ..frag(Vec3::NEG_Y) });
        // ramp = 0, h = 1: 0.5 * (s0 + s1)
        close(out, [0.5 * 0.3, 0.5 * 0.35, 0.5 * 0.35], 1e-6);
        // Sideways: N.L = 0 and N.A = 0 -> only s0.
        let side = shade(&u, &Frag { spec_w: 0.0, ..frag(Vec3::X) });
        close(side, [0.5 * 0.2, 0.5 * 0.25, 0.5 * 0.3], 1e-6);
    }

    /// Partial light: N at 60 degrees from the light -> N.L = 0.5; the LUT
    /// (texel 127 -> 127/255 at c = 0.5 is 127.5/255 - 0.0000...) and
    /// hemisphere (N.A)^2 = 0.25.
    #[test]
    fn golden_half_lambert() {
        let s = set();
        let u = CharLightUniform::new(&s, Vec3::splat(0.5));
        let n = Vec3::new(0.866_025_4, 0.5, 0.0);
        let out = shade(&u, &Frag { spec_w: 0.0, ..frag(n) });
        let ramp = 127.5 / 255.0;
        let want = [0.5 * (0.2 + 0.8 * ramp + 0.1 * 0.25), 0.5 * (0.25 + 0.6 * ramp + 0.1 * 0.25), 0.5 * (0.3 + 0.4 * ramp + 0.05 * 0.25)];
        close(out, want, 1e-5);
    }

    /// The point-light term: sat(N.Lp) with |Lp| = w / d^2 (`SWBOS_vs`
    /// o2: direction * cP0_POS.w / d^2, encoded 0.5 x + 0.5, PS decodes).
    #[test]
    fn golden_point_light_inverse_square() {
        let mut u = CharLightUniform::new(&set(), Vec3::splat(0.5));
        u.dl_col = Vec4::ZERO;
        u.s0 = Vec4::ZERO;
        u.s1 = Vec4::ZERO;
        u.p0_col = Vec4::new(0.5, 0.0, 0.0, 0.0);
        u.p0_pos = Vec4::new(0.0, 10.0, 0.0, 50.0); // light 10 above, w = 50
        // Surface at the origin facing up: d = 10, |Lp| = 50/100 = 0.5,
        // direction to light (0,1,0): N.Lp = 0.5, rgb = base * 0.5 * 0.5.
        let out = shade(&u, &Frag { spec_w: 0.0, ..frag(Vec3::Y) });
        close(out, [0.5 * 0.25, 0.0, 0.0], 1e-6);
        // Twice as far: 50/400 = 0.125 -> quarter of the brightness... x (1/4).
        u.p0_pos = Vec4::new(0.0, 20.0, 0.0, 50.0);
        let far = shade(&u, &Frag { spec_w: 0.0, ..frag(Vec3::Y) });
        close(far, [0.5 * 0.5 * 0.125, 0.0, 0.0], 1e-6);
        // Very close: |Lp| > 1 clamps per component to 1 (the PS clamp).
        u.p0_pos = Vec4::new(0.0, 1.0, 0.0, 50.0);
        let near = shade(&u, &Frag { spec_w: 0.0, ..frag(Vec3::Y) });
        close(near, [0.5 * 0.5 * 1.0, 0.0, 0.0], 1e-6);
    }

    /// "Effect lights: 4 per fighter" (look.rs): extra lights add into the same
    /// term; with the default of one light the extras never reach the uniform,
    /// whatever `extra` holds, and a zero extra adds nothing.
    #[test]
    fn extra_effect_lights_add_and_the_original_count_ignores_them() {
        let el = |x: f32| EffectLight { colour: Vec3::new(0.5, 0.0, 0.0), pos: Vec3::new(x, 10.0, 0.0), range: 50.0 };
        let mut l = CharLight::new(Some(set()), [Vec3::splat(0.5); crate::combat::MAX_PLAYERS]);
        l.effect[0] = Some(el(0.0));
        l.extra[0] = [Some(el(0.0)), Some(el(0.0)), None];
        // One light (the default): the extras stay out of the uniform.
        assert_eq!(l.max_lights, 1);
        let u1 = l.uniform(0);
        assert!(u1.x_col.iter().all(|c| *c == Vec4::ZERO));
        // Four: two extras land in slots 0 and 1, the third is empty.
        l.max_lights = MAX_EFFECT_LIGHTS;
        let u4 = l.uniform(0);
        assert_eq!((u4.x_col[0].truncate(), u4.x_col[1].truncate(), u4.x_col[2]), (Vec3::new(0.5, 0.0, 0.0), Vec3::new(0.5, 0.0, 0.0), Vec4::ZERO));
        // Same light three times at the same place: three times one light's term (before the clamp to 1).
        let mut base = u4;
        base.dl_col = Vec4::ZERO;
        base.s0 = Vec4::ZERO;
        base.s1 = Vec4::ZERO;
        let f = Frag { spec_w: 0.0, ..frag(Vec3::Y) };
        let three = shade(&base, &f)[0];
        let mut one = base;
        one.x_col = [Vec4::ZERO; 3];
        let single = shade(&one, &f)[0];
        assert!((three - 3.0 * single).abs() < 1e-5, "{three} vs 3 x {single}");
    }

    #[test]
    fn rank_lights_orders_by_how_much_each_lights_the_spot() {
        let at = Vec3::ZERO;
        let near = EffectLight { colour: Vec3::splat(0.5), pos: Vec3::new(0.0, 5.0, 0.0), range: 50.0 };
        let far = EffectLight { colour: Vec3::splat(0.5), pos: Vec3::new(0.0, 50.0, 0.0), range: 50.0 };
        let dim = EffectLight { colour: Vec3::splat(0.05), pos: Vec3::new(0.0, 5.0, 0.0), range: 50.0 };
        let r = rank_lights(&[far, dim, near], at, 2);
        assert_eq!(r, vec![near, dim]);
        assert_eq!(rank_lights(&[far, near], at, 1), vec![near]);
        assert!(rank_lights(&[], at, 4).is_empty());
    }

    /// The specular needs all four factors (vertex weight, map alpha, lobe,
    /// colour); zero in any one kills it (`mul r1.w, r1.w, r2.w` ... `mul_sat`).
    #[test]
    fn specular_needs_weight_mask_and_lobe() {
        let u = CharLightUniform::new(&set(), Vec3::splat(0.5));
        let base = shade(&u, &Frag { spec_w: 0.0, ..frag(Vec3::Y) });
        let with = shade(&u, &frag(Vec3::Y));
        assert!(with.x > base.x);
        let no_mask = shade(&u, &Frag { nm_a: 0.0, ..frag(Vec3::Y) });
        assert_eq!(no_mask, base);
        // Facing sideways: N.H ~ 0 -> no lobe.
        let side = shade(&u, &frag(Vec3::X));
        let side_off = shade(&u, &Frag { spec_w: 0.0, ..frag(Vec3::X) });
        assert_eq!(side, side_off);
    }

    #[test]
    fn output_is_saturated() {
        let s = CharLightSet { dl_col: Vec3::splat(2.0), s0: Vec3::splat(1.0), s1: Vec3::splat(1.0), ..set() };
        let out = shade(&CharLightUniform::new(&s, Vec3::splat(1.0)), &frag(Vec3::Y));
        assert_eq!(out, Vec3::ONE);
    }

    /// `FUN_1402a75e0`: cSPECULAR = byte * 2/255 * DL colour (DAT_1402f7c68 =
    /// 0.00784314). 0x7f -> ~1.0 x DL, 0xcc -> 1.6 x DL.
    #[test]
    fn specular_colour_scales_like_the_program() {
        let s = set();
        let u = CharLightUniform::new(&s, Vec3::splat(0xcc as f32 / 255.0));
        assert!((u.spec.x - 1.6 * 0.8).abs() < 1e-5);
        let u = CharLightUniform::new(&s, Vec3::splat(0x7f as f32 / 255.0));
        assert!((u.spec.x - 0.996 * 0.8).abs() < 2e-3);
    }

    /// Slot binding (`FUN_140199e70` + `FUN_1402a7490/7880`): PS c3 <- light
    /// record 1 colour (= script colour landing at block+8), c6 <- record 0
    /// colour1 (block+0), c7 <- record 0 colour2 (block+4); VS cDL_VEC <- record
    /// 1 vector (block+0x14), cS0_VEC <- record 0 vector (block+0x20). The
    /// script pushes (axis, dir, c1, c2, c3) and the exe's static default
    /// block has direction at +0x14, axis at +0x20: so dir/axis are not
    /// swapped (json keys already match), and colours reverse.
    #[test]
    fn script_colours_land_as_the_block_layout_says() {
        let ch = serde_json::json!({
            "hemi_axis": [0.0, 1.0, 0.0], "dir": [0.0, -1.0, 0.0],
            "sky": [255, 0, 0], "ground": [0, 255, 0], "sun": [0, 0, 255], "fog": {},
        });
        let rev = crate::stage_look::char_light_from_json(&ch, false).unwrap();
        assert_eq!(rev.dl_col, Vec3::new(1.0, 0.0, 0.0)); // c1 -> cDL_COL
        assert_eq!(rev.s1, Vec3::new(0.0, 1.0, 0.0)); // c2 -> cS1
        assert_eq!(rev.s0, Vec3::new(0.0, 0.0, 1.0)); // c3 -> cS0
        let fwd = crate::stage_look::char_light_from_json(&ch, true).unwrap();
        assert_eq!(fwd.dl_col, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(fwd.s0, Vec3::new(1.0, 0.0, 0.0));
        // Game space is left-handed: Z mirrored into Bevy's world.
        let ch2 = serde_json::json!({ "hemi_axis": [0.0, 0.6, 0.8], "dir": [0.0, -0.6, 0.8], "sky": [1,1,1], "ground": [1,1,1], "sun": [1,1,1] });
        let m = crate::stage_look::char_light_from_json(&ch2, false).unwrap();
        assert!((m.dir - Vec3::new(0.0, -0.6, -0.8)).length() < 1e-6);
        assert!((m.axis - Vec3::new(0.0, 0.6, -0.8)).length() < 1e-6);
    }

    /// `FUN_14022e1d0`: f = t / (peak*total) before the peak, then
    /// 1 - (t - (int)(peak*total)) / (total - (int)(peak*total)); colours scale
    /// by (target - 1) f + 1. Numbers for the 15-frame flash (peak 0.05,
    /// peak frame 0.75 -> (int) 0): frame 0 -> f 0; frame 1 -> 1 - 1/15;
    /// frame 15 -> 0. The 45-frame dim (peak 0.05, (int)2.25 = 2): frame 1 ->
    /// 1/2.25 = 0.444; frame 2 -> 0.889; frame 3 -> 1 - 1/43.
    #[test]
    fn scene_fade_envelope_matches_the_program() {
        let mut f = CharLightFade::trigger(Vec3::new(1.5, 1.5, 2.0), 15, 0.05);
        assert_eq!(f.envelope(), 0.0);
        f.frame = 1;
        assert!((f.envelope() - (1.0 - 1.0 / 15.0)).abs() < 1e-6);
        assert!((f.scale().z - (1.0 * (1.0 - 1.0 / 15.0) + 1.0)).abs() < 1e-6);
        f.frame = 15;
        assert!(f.envelope().abs() < 1e-6);
        let mut d = CharLightFade::trigger(Vec3::new(0.2, 0.1, 0.6), 45, 0.05);
        d.frame = 1;
        assert!((d.envelope() - 1.0 / 2.25).abs() < 1e-6);
        d.frame = 2;
        assert!((d.envelope() - 2.0 / 2.25).abs() < 1e-6);
        d.frame = 3;
        assert!((d.envelope() - (1.0 - 1.0 / 43.0)).abs() < 1e-6);
        // Scale at the dim's deepest point ~ the target.
        d.frame = 2;
        assert!(d.scale().x < 0.3);
        // Ends after `total` ticks, then scale is exactly 1.
        let mut e = CharLightFade::trigger(Vec3::splat(2.0), 3, 0.5);
        for _ in 0..5 {
            e.step();
        }
        assert!(!e.active);
        assert_eq!(e.scale(), Vec3::ONE);
        // Inactive = identity.
        assert_eq!(CharLightFade::default().scale(), Vec3::ONE);
    }

    /// Slot binding: an effect light replaces slot 2 (the point light) and
    /// only that; the scene fade scales DL/S0/S1 colours and the specular.
    #[test]
    fn effect_light_takes_slot_two_and_fade_scales_colours() {
        let mut l = CharLight::new(Some(set()), [Vec3::splat(0.5); crate::combat::MAX_PLAYERS]);
        assert_eq!(l.uniform(0).p0_col, Vec4::ZERO);
        l.effect[1] = Some(EffectLight { colour: Vec3::new(0.8, 0.2, 0.9), pos: Vec3::new(1.0, 2.0, 3.0), range: 1000.0 });
        assert_eq!(l.uniform(0).p0_col, Vec4::ZERO); // other fighter unaffected
        let u = l.uniform(1);
        assert_eq!(u.p0_col.truncate(), Vec3::new(0.8, 0.2, 0.9));
        assert_eq!(u.p0_pos, Vec4::new(1.0, 2.0, 3.0, 1000.0));
        // DL colour untouched by the effect light.
        assert_eq!(u.dl_col.truncate(), Vec3::new(0.8, 0.6, 0.4));
        l.fade = CharLightFade { active: true, frame: 1, total: 4, peak: 0.25, target: Vec3::new(2.0, 2.0, 2.0) };
        let f = l.uniform(0);
        assert!((f.dl_col.x - 1.6).abs() < 1e-6); // envelope 1 at the peak frame: x2
        assert!((f.s0.y - 0.5).abs() < 1e-6);
    }

    #[test]
    fn spec_colour_rides_in_the_glb_asset_extras() {
        let json = br#"{"asset":{"version":"2.0","extras":{"pd_spec":[204,204,204,20]}}}"#;
        let mut glb = b"glTF".to_vec();
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&((20 + json.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(json);
        let v = spec_from_glb(&glb).unwrap();
        assert!((v.x - 0.8).abs() < 1e-6);
        assert!(spec_from_glb(b"nope").is_none());
    }
}
