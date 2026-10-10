//! How a stage looks beyond its glTF: fog, and the light the characters get,
//! from the stage's own light script (`cache/stNN.light.json`, written by
//! `pd_convert model/stages`; format evidence in FORMATS-STAGES.md
//! "Lighting script"). The stage itself is lit at conversion time (the
//! light is baked into its vertex colours), so nothing here touches it.
//!
//! Shared with the viewer (`#[path]` include), so it only uses Bevy + serde.

use bevy::prelude::*;
use std::path::Path;

/// Reference brightness for scaling the character lights: a light-script
/// colour of this luminance gives today's tuned `LIGHT_SUN` / `LIGHT_AMBIENT`
/// (0.5 = mid grey). ⚠️ unconfirmed tuning, checked by eye against the
/// Lost Palace reference screenshots.
pub const CHAR_LIGHT_REF: f32 = 0.5;

/// The player-character light set the `SWBOS` pixel shader reads
/// (`cDL_*`, `cS0_*`, `cS1_*`, hemisphere axis), in Bevy world space, colours
/// in the game's gamma space 0..1. Built by `char_light_from_json`; consumed by
/// `char_light.rs`. Derivation and evidence: docs/CHAR-LIGHT.md.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharLightSet {
    /// Directional light colour (`cDL_COL`, scales the LUT's N.L ramp).
    pub dl_col: Vec3,
    /// Constant ambient (`cS0_COLp`).
    pub s0: Vec3,
    /// Hemisphere term colour (`cS1_COLp`, weighted by (N.axis)^2).
    pub s1: Vec3,
    /// Directional light travel direction (`cDL_VEC`), unit.
    pub dir: Vec3,
    /// Hemisphere axis (`cS0_VEC.xyz`), unit.
    pub axis: Vec3,
}

impl CharLightSet {
    /// Used when a stage has no `.light.json`: a soft key from above-front
    /// and a mild ambient (near the old hand-tuned look).
    #[allow(dead_code)] // the viewer includes this file and never lights characters this way
    pub fn fallback() -> Self {
        Self { dl_col: Vec3::splat(0.75), s0: Vec3::splat(0.35), s1: Vec3::splat(0.1), dir: -Vec3::new(0.4, 1.0, 0.6).normalize(), axis: Vec3::Y }
    }
}

/// Reads the script's character set (set 1) into the light the shader uses.
///
/// Which script colour lands in which constant is **not** read from the
/// program's script handler (not located). The block layout IS known
/// (`FUN_140199670`/`FUN_1402a7880`): block+0 -> `cS0_COLp`, +4 -> `cS1_COLp`,
/// +8 -> `cDL_COL`, +0x14 -> direction, +0x20 -> hemisphere axis, and the
/// static default block in the exe has that layout. The script's call
/// pushes (axis, dir, c1, c2, c3); the vectors must land reversed
/// (first-pushed axis -> +0x20, dir -> +0x14, proved by the exe's default
/// block), so the colours are taken reversed too: **c1 -> DL, c2 -> S1,
/// c3 -> S0** (`forward = false`, the default). `forward = true` is the
/// natural-order reading (c1 -> S0, c3 -> DL), kept for A/B
/// (`PD_CHAR_LIGHT_ORDER=forward`).
///
/// `ch` is a set whose vectors are already in program order (a freshly
/// exported `light.json`); see [`char_light_from_set`] for older caches.
#[cfg_attr(not(test), allow(dead_code))]
pub fn char_light_from_json(ch: &serde_json::Value, forward: bool) -> Option<CharLightSet> {
    char_light_from_set(ch, forward, true)
}

/// The script's vectors in the program's component order. The program's
/// handler for the light call (`0x140206850`) stores its arguments
/// last-pushed-first, so the file lists each vector as (z, y, x); `pd_convert`
/// now writes them swapped (`"vec_order": "zyx"` at the top of the json). A
/// json from an older cache has no tag: swap x and z here so the game still
/// agrees with a re-exported stage (docs/FORMATS-STAGES.md "Lighting script").
/// `v` is the whole json, `x` one `[x, y, z]` array.
pub fn script_vec(v: &serde_json::Value, x: &serde_json::Value) -> Option<Vec3> {
    let d = vec3(x)?;
    let program_order = v["vec_order"].as_str() == Some("zyx");
    // `PD_STAGE_LIGHT=0` reads them the way the game did before the order was
    // found (file order): swap back a tagged json, leave an untagged one.
    Some(if program_order != legacy_light() { d } else { Vec3::new(d.z, d.y, d.x) })
}

/// `PD_STAGE_LIGHT=0`: stage lighting as it was before lane L3 (file-order
/// light vectors, no per-pixel stage light, no volume grid, no hull casters,
/// no stage effect light). docs/STAGE-LIGHT.md.
pub fn legacy_light() -> bool {
    std::env::var("PD_STAGE_LIGHT").is_ok_and(|v| v == "0")
}

/// `char_light_from_json` for a set taken from a whole json: `root` is the
/// json root (its `vec_order` tag decides whether the vectors still need the
/// old-cache swap). `root_tagged` must be true when `ch` already holds
/// program-order vectors.
pub fn char_light_from_set(ch: &serde_json::Value, forward: bool, root_tagged: bool) -> Option<CharLightSet> {
    let (c1, c2, c3) = (rgb(&ch["sky"])?, rgb(&ch["ground"])?, rgb(&ch["sun"])?);
    let tag = if root_tagged { serde_json::json!({ "vec_order": "zyx" }) } else { serde_json::Value::Null };
    // (legacy_light() is applied inside script_vec.)
    let v = |x: &serde_json::Value| script_vec(&tag, x).map(|d| Vec3::new(d.x, d.y, -d.z).normalize_or(Vec3::Y));
    let (dl, s0) = if forward { (c3, c1) } else { (c1, c3) };
    Some(CharLightSet { dl_col: Vec3::from(dl), s0: Vec3::from(s0), s1: Vec3::from(c2), dir: v(&ch["dir"])?, axis: v(&ch["hemi_axis"])? })
}

#[derive(Clone, Debug, Default)]
pub struct StageLook {
    /// The character shader's light set (set 1 of the light script).
    pub char_light: Option<CharLightSet>,
    /// Distance fog for the player cameras (the script's stage set); only
    /// with `PD_STAGE_FOG=1` (see `load`).
    pub fog: Option<DistanceFog>,
    /// Character key light: colour, brightness factor (x LIGHT_SUN), travel
    /// direction (right-handed world space).
    pub sun: Option<(Color, f32, Vec3)>,
    /// Character ambient: colour, brightness factor (x LIGHT_AMBIENT).
    pub ambient: Option<(Color, f32)>,
}

fn rgb(v: &serde_json::Value) -> Option<[f32; 3]> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()? as f32 / 255.0, a.get(1)?.as_f64()? as f32 / 255.0, a.get(2)?.as_f64()? as f32 / 255.0])
}

fn vec3(v: &serde_json::Value) -> Option<Vec3> {
    let a = v.as_array()?;
    Some(Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32))
}

/// Luminance of a gamma-space colour (Rec. 601 weights, as in the era's games).
fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Brightest channel scaled to 1, so hue and brightness are set separately.
fn hue(c: [f32; 3]) -> Color {
    let m = c[0].max(c[1]).max(c[2]).max(1e-4);
    Color::srgb(c[0] / m, c[1] / m, c[2] / m)
}

impl StageLook {
    /// Reads `<cache>/<stage>.light.json`; a stage without one (older cache)
    /// gets the default look (no fog, the tuned white character lights).
    pub fn load(cache: &Path, stage: &str) -> Self {
        let Some(v) = std::fs::read_to_string(cache.join(format!("{stage}.light.json"))).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
            return Self::default();
        };
        let mut look = Self::default();
        let f = &v["stage"]["fog"];
        let (a, b) = (f["a"].as_f64().unwrap_or(0.0) as f32, f["b"].as_f64().unwrap_or(0.0) as f32);
        let falloff = match f["mode"].as_u64() {
            // D3DFOGMODE: 3 linear (end, start), 1 exp, 2 exp2 (density).
            // A negative linear start (st03: -319) means some haze already
            // at distance 0; Bevy's linear falloff handles that as is.
            Some(3) if a > b => Some(FogFalloff::Linear { start: b, end: a }),
            Some(1) if a > 0.0 => Some(FogFalloff::Exponential { density: a }),
            Some(2) if a > 0.0 => Some(FogFalloff::ExponentialSquared { density: a }),
            _ => None,
        };
        // Off unless PD_STAGE_FOG=1: every stage pixel shader of the PC
        // version declares the FOG input but never reads it (disassembly,
        // "Used" column blank in SD/SBTS/SVC/SDYFOG/SBTSCLOUD2), and the
        // reference screenshots show no haze. Kept for A/B comparisons.
        let wanted = std::env::var_os("PD_STAGE_FOG").is_some();
        if let (true, Some(falloff), Some(c)) = (wanted, falloff, rgb(&f["color"])) {
            look.fog = Some(DistanceFog { color: Color::srgb(c[0], c[1], c[2]), falloff, ..default() });
        }
        let ch = &v["characters"];
        let forward = std::env::var("PD_CHAR_LIGHT_ORDER").is_ok_and(|v| v.eq_ignore_ascii_case("forward"));
        let tagged = v["vec_order"].as_str() == Some("zyx");
        look.char_light = char_light_from_set(ch, forward, tagged);
        if let (Some(sun), Some(dir)) = (rgb(&ch["sun"]), script_vec(&v, &ch["dir"])) {
            // Game space is left-handed: mirror Z.
            let d = Vec3::new(dir.x, dir.y, -dir.z).normalize_or(Vec3::NEG_Y);
            look.sun = Some((hue(sun), luma(sun) / CHAR_LIGHT_REF, d));
        }
        if let (Some(sky), Some(ground)) = (rgb(&ch["sky"]), rgb(&ch["ground"])) {
            let avg = [(sky[0] + ground[0]) * 0.5, (sky[1] + ground[1]) * 0.5, (sky[2] + ground[2]) * 0.5];
            look.ambient = Some((hue(avg), luma(avg) / CHAR_LIGHT_REF));
        }
        look
    }
}

/// The sky model's nodes (`m2_d*`, gltf.rs `SKY_MODEL`) sit ~11,000 units
/// out: the stage fog would swallow them whole, and the game fogs its sky
/// with a separate, far set (FORMATS-STAGES.md). Turn fog off on their
/// materials (sky materials are not shared with the stage).
pub fn sky_without_fog(
    added: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Added<MeshMaterial3d<StandardMaterial>>>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for (e, mat) in &added {
        let own = names.get(e).ok();
        let parent = parents.get(e).ok().and_then(|p| names.get(p.parent()).ok());
        if own.into_iter().chain(parent).any(|n| n.as_str().starts_with("m2_")) {
            if let Some(mut m) = mats.get_mut(&mat.0) {
                m.fog_enabled = false;
            }
        }
    }
}
