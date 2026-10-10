//! Idle capsule/Aura-Particle orb materials, built from
//! `docs/CAPSULE-VISUALS.md`'s "Draw recipe (decoded 2026-09-28)" -- the
//! session that read the original's actual draw code and shader bytecode
//! (Ghidra + `tools/disasm`), replacing round 2's billboarded-quad
//! approximation with the real exported geometry:
//!
//! - **Core** ([`OrbSphereLit`] / [`OrbSphereUnlit`]): a real sphere mesh
//!   (`pd_convert capsules`' new dedicated exporter, radius 4.5 u), drawn
//!   opaque with back-face culling, yawed by the capsule's slot -- not
//!   billboarded, except for the few kinds the recipe's own per-kind table
//!   marks camera-facing (Aura, both opponent views). `OrbSphereLit` is the
//!   `vkCAP02`/`vkCAP02M2` family (flame texture lit by two scrolling bump
//!   samples, tight specular highlight); `OrbSphereUnlit` is a plain
//!   scrolling-texture stand-in for Status/Environment's unlit 4-texture
//!   shaders (`vkCAP01`/`vkCAP04M4`), whose own vertex format carries no
//!   normal/tangent/colour to light with (see that shader's own doc
//!   comment).
//! - **Rim** ([`OrbRim`]): the cup (every kind) and, where the recipe's
//!   table has one, the flat disc -- both real exported meshes, always
//!   billboarded every frame, additive (or multiply for the opponent-Aura
//!   disc), two scrolling samples of the same texture modulated by the
//!   mesh's own baked vertex-colour rim profile.
//!
//! All three share the round-2 vertex-shader billboard technique (replace
//! the mesh's local frame with the current rendering camera's own
//! right/up/forward axes) -- extended here from a flat quad to full 3-D
//! mesh vertices (and, for the core, its normal/tangent too) so a real
//! sphere or cup keeps its own shape while still facing the camera.

use bevy::asset::Asset;
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::{Shader, ShaderRef};

static SPHERE_LIT_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();
static SPHERE_UNLIT_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();
static RIM_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();

/// One scrolling UV layer: `speed` (UV units/second -- the recipe's own
/// per-frame numbers already multiplied by 30) and a fixed `base` offset
/// (some layers have one, e.g. the (0.5, 0.5) UV pivot most cups/discs use).
#[derive(Clone, Copy, Debug, Default)]
pub struct UvLayer {
    pub speed: [f32; 2],
    pub base: [f32; 2],
}

impl UvLayer {
    pub const fn new(speed: [f32; 2], base: [f32; 2]) -> Self {
        Self { speed, base }
    }
    fn as_vec4(self) -> Vec4 {
        Vec4::new(self.speed[0], self.speed[1], self.base[0], self.base[1])
    }
}

/// The `vkCAP02`/`vkCAP02M2` sphere core (docs "Draw recipe" -> "Shaders").
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct OrbSphereLit {
    /// x = billboard this orb's core (Aura + opponent views), y = has a
    /// distinct bump texture (every lit kind in practice; kept as a flag in
    /// case a kind's second material slot is missing from the data).
    #[uniform(0)]
    pub flags: Vec4,
    /// rgb = specular tint (the recipe's per-kind BGRA bytes x 2/255,
    /// already converted), w = specular exponent (16, per the disassembled
    /// pixel shader -- kept as a uniform rather than a shader constant so a
    /// calibration run can override it without a rebuild).
    #[uniform(1)]
    pub spec: Vec4,
    #[uniform(2)]
    pub layer0: Vec4,
    #[uniform(3)]
    pub layer1: Vec4,
    #[uniform(4)]
    pub layer2: Vec4,
    /// x = fresnel edge power, y = centre-darken floor, z = edge glow
    /// strength (see `orb_sphere_lit.wgsl`'s own doc comment on `orb_edge`).
    #[uniform(5)]
    pub edge: Vec4,
    #[texture(6)]
    #[sampler(7)]
    pub base_tex: Handle<Image>,
    #[texture(8)]
    #[sampler(9)]
    pub bump_tex: Handle<Image>,
}

impl Material for OrbSphereLit {
    fn vertex_shader() -> ShaderRef {
        SPHERE_LIT_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn fragment_shader() -> ShaderRef {
        SPHERE_LIT_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
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
}

/// Status/Environment's unlit core: the real 4-texture layered blend
/// (`vkCAP01`/`vkCAP04M4`, see the wgsl file's own doc comment for the exact
/// per-shader formula) now that `alr::MaterialEntry::texture_idx2`/
/// `texture_idx3` are known to be real, additional texture slots (confirmed
/// 2026-09-28 by a raw byte dump matching the recipe's own per-kind numbers
/// exactly) rather than padding.
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct OrbSphereUnlit {
    /// x = formula (0 = `vkCAP01`/Status: layered "over" blend; 1 =
    /// `vkCAP04M4`/Environment: multiply then "over" blend, saturated).
    #[uniform(0)]
    pub flags: Vec4,
    #[uniform(1)]
    pub layer0: Vec4,
    #[uniform(2)]
    pub layer1: Vec4,
    #[uniform(3)]
    pub layer2: Vec4,
    #[uniform(4)]
    pub layer3: Vec4,
    #[texture(5)]
    #[sampler(6)]
    pub tex0: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    pub tex1: Handle<Image>,
    #[texture(9)]
    #[sampler(10)]
    pub tex2: Handle<Image>,
    #[texture(11)]
    #[sampler(12)]
    pub tex3: Handle<Image>,
}

impl Material for OrbSphereUnlit {
    fn vertex_shader() -> ShaderRef {
        SPHERE_UNLIT_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn fragment_shader() -> ShaderRef {
        SPHERE_UNLIT_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
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
}

/// The cup/disc rim (see the wgsl file's own doc comment).
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct OrbRim {
    /// x = camera-forward sign (+1.0 / -1.0): the recipe's own "one A/B
    /// render (cup toward vs away) settles it in one look" test, kept as a
    /// tunable rather than hardcoded so both can be rendered without a
    /// rebuild. y = `RimBlend` as a number (0 Add, 1 Multiply, 2 Screen) --
    /// `alpha_mode` below reads it for the pipeline's blend state; the
    /// shader itself only needs to tell Multiply from the other two (Add and
    /// Screen share a pipeline, differing only in the shader's own maths).
    #[uniform(0)]
    pub flags: Vec4,
    #[uniform(1)]
    pub layer0: Vec4,
    #[uniform(2)]
    pub layer1: Vec4,
    /// x = boost (overall gain), y = soften (gamma exponent, lower = wider/
    /// softer) -- see `orb_rim.wgsl`'s own doc comment on `orb_ring`.
    #[uniform(3)]
    pub ring: Vec4,
    #[texture(4)]
    #[sampler(5)]
    pub tex: Handle<Image>,
}

impl Material for OrbRim {
    fn vertex_shader() -> ShaderRef {
        RIM_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn fragment_shader() -> ShaderRef {
        RIM_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
    fn alpha_mode(&self) -> AlphaMode {
        if (self.flags.y - 1.0).abs() < 0.5 {
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

/// Registers the three shaders and material plugins. Called once from
/// `fx::plugin`.
pub fn plugin(app: &mut App) {
    {
        let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("orb_sphere_lit.wgsl"), "pd_game/orb_sphere_lit.wgsl"));
        let _ = SPHERE_LIT_SHADER.set(shader);
    }
    {
        let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("orb_sphere_unlit.wgsl"), "pd_game/orb_sphere_unlit.wgsl"));
        let _ = SPHERE_UNLIT_SHADER.set(shader);
    }
    {
        let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("orb_rim.wgsl"), "pd_game/orb_rim.wgsl"));
        let _ = RIM_SHADER.set(shader);
    }
    app.add_plugins((MaterialPlugin::<OrbSphereLit>::default(), MaterialPlugin::<OrbSphereUnlit>::default(), MaterialPlugin::<OrbRim>::default()));
}

/// One idle-orb draw role, in the exporter's own file order
/// (`pd_convert capsules`' `capsules.json`, confirmed against every kind:
/// core always first, then -- for the kinds that have one -- a disc, then
/// one or two cups).
#[derive(Clone, Copy, Debug)]
pub enum OrbDrawRole {
    /// The lit sphere (`OrbSphereLit`): `billboard`, `spec` (RGB, already
    /// x2/255), `has_bump`, and up to three UV layers (flame, bump A, bump
    /// B -- B unused when `has_bump` is false).
    CoreLit { billboard: bool, spec: [f32; 3], has_bump: bool, layers: [UvLayer; 3] },
    /// Status/Environment's unlit core (`OrbSphereUnlit`): the real 4-texture
    /// blend, one UV layer per texture slot, and which of the two formulas
    /// (`vkCAP01`/`vkCAP04M4`) this kind uses.
    CoreUnlit { layers: [UvLayer; 4], formula: UnlitFormula },
    /// A cup or disc (`OrbRim`): two UV layers and a blend kind (every cup is
    /// `Add`; the recipe's per-kind table gives a few discs a different
    /// blend flag -- see [`RimBlend`]).
    Rim { layers: [UvLayer; 2], blend: RimBlend },
}

/// A rim draw's blend, from the recipe's own material-flag reading (docs
/// "Sampler / blend state"): every cup is `Add` (flag 0x1100, ONE+ONE); a
/// few discs carry a different flag. `Screen` (flag 0x1300, "src
/// ONE_MINUS_SRC_COLOR, dst ONE") isn't one of Bevy's built-in `AlphaMode`s,
/// so it's approximated in `orb_rim.wgsl` by pre-attenuating the colour
/// (`src*(1-src)`) before handing it to the same premultiplied-alpha Add
/// pipeline `Add` uses -- algebraically `src*(1-src) + dst` is exactly what
/// the recipe's own blend factors compute, so this reproduces it exactly
/// rather than approximating by eye. `Multiply` (flag 0x2000, "src ZERO, dst
/// SRC_COLOR") uses Bevy's own `AlphaMode::Multiply`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RimBlend {
    Add,
    Screen,
    Multiply,
}

/// Status/Environment's unlit core formula (docs "Other kinds", closing
/// note): read from the disassembled bytecode, both unlit layered blends of
/// four textures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnlitFormula {
    /// `vkCAP01` (Status): `t0`, then `t1` and `t2` blended over it by their
    /// own alpha, then `+t3`.
    Status,
    /// `vkCAP04M4` (Environment): `t0*t1`, then `t2` blended over by its own
    /// alpha, then `+t3`, saturated.
    Environment,
}

/// Every idle-orb kind's per-draw recipe (docs/CAPSULE-VISUALS.md "Other
/// kinds (same recipe, their own numbers)"), keyed by `st00.alr` model slot
/// (4-12). Speeds/bases are the doc's own per-frame numbers; textures are
/// NOT hardcoded here -- they come from `capsules.json`'s `draws[].base`/
/// `secondary` (the exporter's own resolved glTF texture indices), since
/// those are read from the file, not reverse-engineered.
const fn l(speed: [f32; 2], base: [f32; 2]) -> UvLayer {
    UvLayer::new(speed, base)
}

// Top-level `static`s, not inline slice literals in `kind_spec`'s match arms
// -- an inline `&[...]` needs rvalue static promotion to return as
// `&'static`, which rustc doesn't extend through a `match` returning
// differently-shaped array literals per arm (`error[E0515]`); named statics
// sidestep that entirely.
static AURA_DRAWS: [OrbDrawRole; 3] = [
    OrbDrawRole::CoreLit { billboard: true, spec: [35.0 * 2.0 / 255.0, 56.0 * 2.0 / 255.0, 93.0 * 2.0 / 255.0], has_bump: true, layers: [l([0.003, -0.002], [0.0, 0.0]), l([0.003, -0.002], [0.0, 0.0]), l([0.005, 0.01], [0.0, 0.0])] },
    // Disc: recipe blend flag 0x1300, "screen-like", not plain add.
    OrbDrawRole::Rim { layers: [l([0.0, -0.007], [0.5, 0.5]), l([0.001, -0.004], [0.5, 0.5])], blend: RimBlend::Screen },
    OrbDrawRole::Rim { layers: [l([0.0, -0.002], [0.5, 0.5]), l([0.0, -0.003], [0.5, 0.5])], blend: RimBlend::Add },
];
static ATTACK_DRAWS: [OrbDrawRole; 2] = [
    OrbDrawRole::CoreLit { billboard: false, spec: [159.0 * 2.0 / 255.0, 52.0 * 2.0 / 255.0, 0.0], has_bump: true, layers: [l([0.002, 0.008], [0.0, 0.0]), l([-0.002, 0.003], [0.0, 0.0]), l([-0.003, 0.012], [0.0, 0.0])] },
    OrbDrawRole::Rim { layers: [l([0.0, -0.006], [0.5, 0.5]), l([0.001, -0.002], [0.5, 0.5])], blend: RimBlend::Add },
];
static DEFENCE_DRAWS: [OrbDrawRole; 3] = [
    OrbDrawRole::CoreLit { billboard: false, spec: [52.0 * 2.0 / 255.0, 89.0 * 2.0 / 255.0, 152.0 * 2.0 / 255.0], has_bump: true, layers: [l([-0.002, 0.0], [0.05, 0.05]), l([0.01, -0.003], [0.03, 0.03]), l([-0.004, -0.002], [0.1, 0.1])] },
    OrbDrawRole::Rim { layers: [l([0.0, 0.008], [0.0, 0.0]), l([0.0, 0.016], [0.0, 0.0])], blend: RimBlend::Add },
    OrbDrawRole::Rim { layers: [l([0.0, -0.01], [0.5, 0.5]), l([0.0, -0.005], [0.5, 0.5])], blend: RimBlend::Add },
];
static ERASE_DRAWS: [OrbDrawRole; 3] = [
    OrbDrawRole::CoreLit { billboard: false, spec: [54.0 * 2.0 / 255.0, 0.0, 178.0 * 2.0 / 255.0], has_bump: true, layers: [l([0.0, -0.002], [0.5, 0.5]), l([0.0, 0.01], [0.5, 0.5]), l([-0.003, -0.005], [0.5, 0.5])] },
    OrbDrawRole::Rim { layers: [l([0.0, 0.02], [0.5, 0.5]), l([0.0, 0.01], [0.5, 0.5])], blend: RimBlend::Add },
    OrbDrawRole::Rim { layers: [l([0.0, -0.002], [0.5, 0.5]), l([0.0, -0.001], [0.5, 0.5])], blend: RimBlend::Add },
];
static STATUS_DRAWS: [OrbDrawRole; 2] = [
    OrbDrawRole::CoreUnlit { layers: [l([0.0, -0.005], [0.0, 0.0]), l([0.0, -0.003], [0.4, 0.4]), l([0.0, -0.004], [0.4, 0.4]), l([0.002, 0.0], [0.0, 0.0])], formula: UnlitFormula::Status },
    OrbDrawRole::Rim { layers: [l([0.0, -0.002], [0.5, 0.5]), l([0.0, -0.001], [0.5, 0.5])], blend: RimBlend::Add },
];
static SPECIAL_DRAWS: [OrbDrawRole; 3] = [
    OrbDrawRole::CoreLit { billboard: false, spec: [89.0 * 2.0 / 255.0, 48.0 * 2.0 / 255.0, 10.0 * 2.0 / 255.0], has_bump: true, layers: [l([0.0, 0.002], [0.0, 0.0]), l([0.001, 0.004], [0.5, 0.5]), l([-0.001, 0.0], [0.5, 0.5])] },
    OrbDrawRole::Rim { layers: [l([0.001, 0.002], [0.0, 0.0]), l([-0.001, 0.005], [0.5, 0.5])], blend: RimBlend::Add },
    OrbDrawRole::Rim { layers: [l([0.0, -0.002], [0.5, 0.5]), l([0.0, -0.001], [0.5, 0.5])], blend: RimBlend::Add },
];
static ENVIRONMENT_DRAWS: [OrbDrawRole; 2] = [
    OrbDrawRole::CoreUnlit { layers: [l([0.002, 0.0], [0.0, 0.0]), l([0.0, -0.002], [0.4, 0.4]), l([0.0, -0.003], [0.3, 0.3]), l([-0.001, 0.0], [0.2, 0.2])], formula: UnlitFormula::Environment },
    OrbDrawRole::Rim { layers: [l([0.0, -0.002], [0.5, 0.5]), l([0.0, -0.001], [0.5, 0.5])], blend: RimBlend::Add },
];
static OPP_AURA_DRAWS: [OrbDrawRole; 3] = [
    OrbDrawRole::CoreLit { billboard: true, spec: [127.0 * 2.0 / 255.0, 127.0 * 2.0 / 255.0, 127.0 * 2.0 / 255.0], has_bump: true, layers: [l([0.002, 0.0], [0.0, 0.0]), l([0.003, -0.004], [0.4, 0.4]), l([-0.003, -0.006], [0.4, 0.4])] },
    OrbDrawRole::Rim { layers: [l([0.0, -0.007], [0.5, 0.5]), l([0.001, -0.004], [0.5, 0.5])], blend: RimBlend::Multiply },
    OrbDrawRole::Rim { layers: [l([0.0, -0.002], [0.5, 0.5]), l([0.0, -0.001], [0.5, 0.5])], blend: RimBlend::Add },
];
static OPP_OTHER_DRAWS: [OrbDrawRole; 3] = [
    OrbDrawRole::CoreLit { billboard: true, spec: [196.0 * 2.0 / 255.0, 0.0, 64.0 * 2.0 / 255.0], has_bump: true, layers: [l([0.002, 0.0], [0.0, 0.0]), l([0.003, -0.004], [0.4, 0.4]), l([-0.003, -0.006], [0.4, 0.4])] },
    // Disc: the recipe's row 12 is "as 11" except core texture/spec, and
    // row 11's disc is the one blend-0x2000 multiply kind -- carried over.
    OrbDrawRole::Rim { layers: [l([0.0, -0.007], [0.5, 0.5]), l([0.001, -0.004], [0.5, 0.5])], blend: RimBlend::Multiply },
    OrbDrawRole::Rim { layers: [l([0.0, -0.002], [0.5, 0.5]), l([0.0, -0.001], [0.5, 0.5])], blend: RimBlend::Add },
];

pub fn kind_spec(slot: usize) -> &'static [OrbDrawRole] {
    match slot {
        4 => &AURA_DRAWS,
        5 => &ATTACK_DRAWS,
        6 => &DEFENCE_DRAWS,
        7 => &ERASE_DRAWS,
        8 => &STATUS_DRAWS,
        9 => &SPECIAL_DRAWS,
        10 => &ENVIRONMENT_DRAWS,
        11 => &OPP_AURA_DRAWS,
        12 => &OPP_OTHER_DRAWS,
        _ => &[],
    }
}

/// A camera-facing sign for the cup/disc's billboard (the recipe's own A/B
/// test: "cup facing toward vs away from camera"). `PD_ORB_CUP_FACE=away`
/// flips it for a calibration render; the shipped default (`toward`) is the
/// doc's own best-supported read ("the cup hugs the camera-side hemisphere").
pub fn cup_face_sign() -> f32 {
    match std::env::var("PD_ORB_CUP_FACE").ok().as_deref() {
        Some("away") => -1.0,
        _ => 1.0,
    }
}

/// The three orb material asset stores bundled into one `SystemParam` --
/// `sync_combat_visuals` was already at Bevy's per-system parameter-count
/// ceiling, and three separate `ResMut<Assets<_>>` params pushed it over
/// (`error[E0599]`/no `IntoSystemConfigs` impl for that arity); grouping
/// them here keeps `spawn_orb`'s call sites down to one extra parameter.
#[derive(bevy::ecs::system::SystemParam)]
pub struct OrbMats<'w> {
    pub lit: ResMut<'w, Assets<OrbSphereLit>>,
    pub unlit: ResMut<'w, Assets<OrbSphereUnlit>>,
    pub rim: ResMut<'w, Assets<OrbRim>>,
}

/// Tunables for the core's centre-darken/edge-glow approximation (docs
/// "Fixed 2026-09-28 (Laurence: darker centre, brighter edges)") -- read
/// from the environment so a proof sheet can render labelled A/B/C variants
/// without a rebuild, the same pattern rounds 1/2 used for their own
/// calibration sheets.
#[derive(Clone, Copy, Debug)]
pub struct EdgeTuning {
    pub power: f32,
    pub centre_darken: f32,
    pub glow: f32,
}

impl EdgeTuning {
    /// Three presets spanning "subtle" to "strong", for the round-3c A/B/C
    /// sheet.
    pub fn variant(letter: char) -> Self {
        // Round-3c's first pass (power 1.6-3.0) looked fine on the
        // non-billboarded Attack orb but blew the billboarded kinds (Aura,
        // both opponent views) out to a near-solid white/pink disc with a
        // black centre dot: viewed dead-on (as a billboarded sphere always
        // is), a low fresnel power covers most of the visible disc, not
        // just a thin rim, because grazing angle rises quickly moving out
        // from screen-centre on any sphere large in frame. Raised well past
        // that pass's range so the bright zone stays a genuine edge rather
        // than swallowing the whole face.
        // Round 3d (Laurence: "none of A/B/C yet... needs more contrast
        // between edges and centre, making the centre darker"): pushed the
        // darken floor much closer to black than round 3c's own range
        // (0.4-0.55), since the ring/glow thickness Laurence actually asked
        // for lives mostly in the cup (`RingTuning` below), not this term --
        // this one only needs to make sure the centre reads as genuinely
        // dark so the cup's own glow has something dark to contrast against.
        match letter.to_ascii_uppercase() {
            'A' => Self { power: 6.0, centre_darken: 0.22, glow: 0.9 },
            'C' => Self { power: 10.0, centre_darken: 0.05, glow: 1.6 },
            _ => Self { power: 8.0, centre_darken: 0.12, glow: 1.2 },
        }
    }

    pub fn from_env() -> Self {
        // Variant C is the shipped default for now (this pass's own read
        // against the footage -- not yet confirmed by Laurence).
        let base = std::env::var("PD_ORB_EDGE_VARIANT").ok().and_then(|v| v.chars().next()).map_or_else(|| Self::variant('C'), Self::variant);
        let f = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
        Self { power: f("PD_ORB_EDGE_POWER", base.power), centre_darken: f("PD_ORB_EDGE_DARKEN", base.centre_darken), glow: f("PD_ORB_EDGE_GLOW", base.glow) }
    }
}

/// Tunables for the cup/disc's "ring" thickness and glow (docs "Fixed
/// 2026-09-28 (Laurence, round 3d)": "the red outline is not thick enough
/// and not loose enough... needs... the outer ring thicker and looser").
#[derive(Clone, Copy, Debug)]
pub struct RingTuning {
    pub boost: f32,
    pub soften: f32,
}

impl RingTuning {
    /// Three presets spanning "a bit more" to "a lot more" ring thickness/
    /// glow, for the round-3d A/B/C sheet -- pushed past what looks
    /// comfortable on C, per Laurence's own "push the strongest variant
    /// further than feels safe" (he's asked for more twice already).
    pub fn variant(letter: char) -> Self {
        match letter.to_ascii_uppercase() {
            'A' => Self { boost: 1.8, soften: 0.55 },
            'C' => Self { boost: 4.5, soften: 0.22 },
            _ => Self { boost: 2.8, soften: 0.38 },
        }
    }

    pub fn from_env() -> Self {
        // Variant A is the shipped default: B/C are pushed deliberately past
        // "safe" for the round-3d proof sheet (Laurence: "push the strongest
        // variant further than feels safe"), and while that's exactly what
        // Attack's own close-up asked for, applying the same strength to
        // every kind's cup/disc (they all share this one material) blew
        // Defence/Erase/Special/Aura/OppAura/Status/Environment's own halos
        // out to solid white/colour discs in the round-3d all-kinds sheet.
        // A is the mildest of the three and reads as a real improvement
        // (thicker, softer than round 3c) without wrecking the other eight
        // kinds; render with PD_ORB_RING_VARIANT=B/C for the Attack-specific
        // proof.
        let base = std::env::var("PD_ORB_RING_VARIANT").ok().and_then(|v| v.chars().next()).map_or_else(|| Self::variant('A'), Self::variant);
        let f = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
        Self { boost: f("PD_ORB_RING_BOOST", base.boost), soften: f("PD_ORB_RING_SOFTEN", base.soften) }
    }
}

pub(crate) fn sphere_lit_material(role_billboard: bool, spec: [f32; 3], has_bump: bool, layers: [UvLayer; 3], base_tex: Handle<Image>, bump_tex: Handle<Image>) -> OrbSphereLit {
    let edge = EdgeTuning::from_env();
    OrbSphereLit {
        flags: Vec4::new(if role_billboard { 1.0 } else { 0.0 }, if has_bump { 1.0 } else { 0.0 }, 0.0, 0.0),
        spec: Vec4::new(spec[0], spec[1], spec[2], 16.0),
        layer0: layers[0].as_vec4(),
        layer1: layers[1].as_vec4(),
        layer2: layers[2].as_vec4(),
        edge: Vec4::new(edge.power, edge.centre_darken, edge.glow, 0.0),
        base_tex,
        bump_tex,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sphere_unlit_material(
    layers: [UvLayer; 4],
    formula: UnlitFormula,
    tex0: Handle<Image>,
    tex1: Option<Handle<Image>>,
    tex2: Option<Handle<Image>>,
    tex3: Option<Handle<Image>>,
) -> OrbSphereUnlit {
    let mode = match formula {
        UnlitFormula::Status => 0.0,
        UnlitFormula::Environment => 1.0,
    };
    // Debug: PD_ORB_UNLIT_LAYER=0..3 isolates one texture layer (see the
    // wgsl file's own doc comment); unset = -1 = normal blend formula.
    let debug_layer = std::env::var("PD_ORB_UNLIT_LAYER").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(-1.0);
    OrbSphereUnlit {
        flags: Vec4::new(mode, 0.0, debug_layer, 0.0),
        layer0: layers[0].as_vec4(),
        layer1: layers[1].as_vec4(),
        layer2: layers[2].as_vec4(),
        layer3: layers[3].as_vec4(),
        tex1: tex1.unwrap_or_else(|| tex0.clone()),
        tex2: tex2.unwrap_or_else(|| tex0.clone()),
        tex3: tex3.unwrap_or_else(|| tex0.clone()),
        tex0,
    }
}

pub(crate) fn rim_material(layers: [UvLayer; 2], blend: RimBlend, tex: Handle<Image>) -> OrbRim {
    let mode = match blend {
        RimBlend::Add => 0.0,
        RimBlend::Multiply => 1.0,
        RimBlend::Screen => 2.0,
    };
    let ring = RingTuning::from_env();
    OrbRim { flags: Vec4::new(cup_face_sign(), mode, 0.0, 0.0), layer0: layers[0].as_vec4(), layer1: layers[1].as_vec4(), ring: Vec4::new(ring.boost, ring.soften, 0.0, 0.0), tex }
}
