//! Skill effect playback (render-only). Shared by the game and the viewer.
//!
//! An effect is `cache/fx/eftNNNN.glb` + `.fx.json` (pd_convert effects).
//! Playing one event slot spawns the glb scene; each draw node is driven by
//! its timing block: visible between start/end frame, colour start -> end
//! (ARGB), scale grows by `grow` over its lifetime, flipbook cells from
//! `grid`; each piece blends the way its own material says (source /
//! destination factor and operation, `BlendState`), faces the camera when its
//! word 19 is 0 (billboard, rolled by word 7) and culls back faces unless it is
//! 2 (docs/FORMATS-EFFECTS.md "Render semantics"); pieces drawn by the game's screen-distortion
//! shaders (PDEFTOPT*) are hidden. Field meanings: docs/FORMATS-EFFECTS.md.
//!
//! Also the particle bursts and flying fragment models that arena-object
//! effects use (fx.rs; docs/FORMATS-STAGES.md "Object effects").

use bevy::prelude::*;
use bevy::render::render_resource::Face;
use pd_formats::effect::{BlendFactor, BlendOp, BlendState};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[path = "effects/ms_particle.rs"]
mod ms_particle;
#[path = "effects/aura_wisp.rs"]
mod aura_wisp;
pub use aura_wisp::{FxHandSuppress, FxWispPool};
#[path = "effects/mat_cache.rs"]
mod mat_cache;
#[path = "effects/mesh_reuse.rs"]
mod mesh_reuse;
#[path = "effects/warmup.rs"]
mod warmup;
#[path = "effects/inst.rs"]
mod inst;
#[allow(unused_imports)] // the viewer has no match start to warm at
pub use warmup::{warm_pipelines, FxWarmup};
pub use mat_cache::FxMatCache;
#[allow(unused_imports)] // the viewer has no frame stats
pub use inst::FxInstances;

/// Effect frames per second (unconfirmed; PD_FX_FPS overrides).
pub const FX_FPS: f32 = 30.0;

/// Event slots in the .eft file (meanings partly inferred, see FORMATS-EFFECTS.md).
pub const SLOT_CAST: usize = 0;
pub const SLOT_PROJECTILE: usize = 1;
/// Ground impact: flat rings / fire at y = 0, 8-40 frames (Laser, Blaster,
/// Bomb, Excalibur); Bomb Shot has only slots 0 and 2. Played where a shot
/// ends without hitting anyone and where blasts go off (unconfirmed reading).
pub const SLOT_IMPACT: usize = 2;
pub const SLOT_HIT: usize = 3;
pub const SLOT_BLOCK: usize = 4;

/// Trail size per unit of its `grow` words (x = width, z = length).
/// Unconfirmed: chosen so Laser's (1, 1, 20) reads as a long thin beam.
const TRAIL_WIDTH: f32 = 3.0;
const TRAIL_LENGTH: f32 = 5.0;
/// Frames a trail takes to reach full length (so it doesn't poke out
/// behind the caster at launch).
const TRAIL_GROW_FRAMES: f32 = 3.0;
const KIND_TRAIL: &str = "b8de89ea00000000";
/// Projectile pieces raised more than this above the model origin are caster-side.
const PROJECTILE_PIECE_REACH: f32 = 8.0;

/// LIGHT02 range word -> world-unit radius (unconfirmed; a character is
/// ~`CHEST` x 1.3 tall, so this puts Psycho Knife's 0 -> 1000 range at a
/// ~20-unit wash, a little over a body-width across). `PD_LIGHT_SCALE`
/// overrides for calibration.
const RANGE_SCALE: f32 = 0.02;
/// How high above the floor (y = 0, as `SLOT_IMPACT`'s ring effects assume)
/// the light decal sits, so it doesn't z-fight with the stage mesh.
const LIGHT_DECAL_Y: f32 = 0.03;
/// At most this many light decals drawn at once (cheap quads, but capped
/// per the brief in case of a multi-caster pile-up).
const MAX_LIGHT_DECALS: usize = 12;

/// `std::env::var(name)`, read from the environment once per name for the whole run. The effect
/// switches (`PD_FX_*`, `PD_LIGHT*`, `PD_MSP*`, ...) used to be read per frame, per piece and per
/// particle (each read locks the environment, scans it and allocates); they are set before launch
/// and nothing in the game changes them (lane fxperf, 2026-10-08).
pub(crate) fn env_once(name: &'static str) -> Option<&'static str> {
    static SEEN: std::sync::Mutex<Vec<(&'static str, Option<&'static str>)>> = std::sync::Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, v)) = seen.iter().find(|(n, _)| *n == name) {
        return *v;
    }
    let v: Option<&'static str> = std::env::var(name).ok().map(|s| &*Box::leak(s.into_boxed_str()));
    seen.push((name, v));
    v
}

fn range_scale() -> f32 {
    env_once("PD_LIGHT_SCALE").and_then(|v| v.parse().ok()).unwrap_or(RANGE_SCALE)
}

/// `PD_LIGHT=0`: turn the floor-light decals off entirely (A/B check that
/// stages otherwise look unchanged).
fn light_enabled() -> bool {
    env_once("PD_LIGHT").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0) > 0.0
}

/// VFX-AUDIT.md #2: a cast (slot 0) effect can read as a big flat disc/ring
/// bigger than the character, where the footage shows a compact, body-sized
/// swirl. Confirmed against the effect data, not a guess: Laser's cast
/// model's own rest-pose bbox (`cache/fx/eft0042.bounds.json`) is already
/// ~35 x 32 x 32 units -- 1.5-3x `sim::BODY_HEIGHT` (20) -- before its ~250
/// glow quads grow further over their own life (+110% to +390% at end of
/// life). Its ~250 pieces are placed directly at the character root
/// (`joint: null` in every one of them, per `FORMATS-EFFECTS.md` "Pieces are
/// placed by their transform joint" -- unlike a joint-attached piece such as
/// Psycho Knife's hand flash, which is inherently scoped to a bone and reads
/// correctly), spread along spirals whose radius is baked straight into
/// their own local geometry.
///
/// The rule applied here is **not** "shrink these three named moves": it is
/// a clamp on any slot-0, root-anchored (`joint: null`) piece's own reach
/// from the caster's chest -- the same body-relative envelope the codebase
/// already uses for the LIGHT02 floor decal (`RANGE_SCALE`, "a little over a
/// body-width across"). A piece already inside that envelope (Psycho Wave
/// and Psycho Knife's pieces are joint-attached and never reach this check
/// at all; a correctly-sized root piece on any other effect would measure
/// under `CAST_REACH` and pass through untouched, ratio 1.0) is left alone;
/// only a piece that actually measures too far from the chest gets pulled
/// in and shrunk by the same ratio, proportionally. This is why it isn't
/// gated by effect name: any effect with this same "spiral pinned to the
/// root, radius baked into the mesh" construction gets the same clamp
/// automatically. Confirmed this pass on Laser/Slide Laser (all
/// root-anchored) and Blaster's own root-anchored pieces; Blaster also has
/// `RHAND`-jointed charge quads this rule doesn't reach (joint-attached, by
/// the same reasoning that protects Psycho Knife) -- if those are still
/// oversized, that's a separate, unconfirmed follow-up, not covered here.
/// `CAST_REACH`/`PD_FX_CAST_REACH` is a by-eye calibration, unconfirmed
/// against the program's own screen-space sizing (no such constant found in
/// the disassembly).
const CAST_REACH: f32 = 14.0;
/// The pivot the reach clamp measures and scales about: the codebase's
/// existing chest-height anchor (`combat::CHEST`; effects.rs can't depend on
/// `pd_game::combat` without a cycle, so mirrored here -- same value).
const CAST_PIVOT_Y: f32 = 12.0;

fn cast_reach() -> f32 {
    env_once("PD_FX_CAST_REACH").and_then(|v| v.parse().ok()).unwrap_or(CAST_REACH)
}

#[derive(Clone)]
struct Trail {
    model: usize,
    texture: String,
    start: f32,
    end: f32,
    colors: [u32; 2],
    width: f32,
    length: f32,
}

/// Particle-emitter kinds (0x14 chunks) drawn as a stream of sprites: `a07ac685`
/// (textured sprites, often a flipbook: Swarm of Moths' moths, Hell's Fire's
/// flames) and `90e7fd3e` (soft puffs: smoke, crawlers' dust). Unconfirmed
/// reading (FORMATS-EFFECTS.md "0x14 shader-drawn pieces").
const KIND_SPRITES: &str = "a07ac685";
const KIND_PUFFS: &str = "90e7fd3e";
/// A new sprite every this many effect frames while the emitter is on.
const SPRITE_EVERY: f32 = 2.0;
/// Sprite life (frames) when the emitter's word 8 gives none / too little.
const SPRITE_MIN_LIFE: f32 = 10.0;
/// Sprite size per unit of its size words 30 / 31 (unconfirmed: moths
/// 7 -> 15, flames 5 -> 15, dust 15 -> 20 read as units). Puffs only now:
/// PARTICLE01/02 are read the program's way (`Sprite::points`).
const SPRITE_SIZE: f32 = 1.0;

/// Drawn frames per effect frame. Effect frames count the program's 30 Hz
/// logic ticks, but its particle (PARTICLE01/02), ray-burst (FLAIR01) and
/// blade (BLADE01/02) systems age and move once per *drawn* frame
/// (`FUN_1401a1c00`, `FUN_1401a0dd0`, `FUN_14019ec80` add 1 per call), i.e.
/// at 60 per second. Inferred: Psycho Knife's ray burst (life 4) shows on
/// exactly two 30 fps footage frames (f101-f102), not four. ⚠️
const STEPS_PER_FRAME: f32 = 2.0;

/// PARTICLE01/02 point size (words 16 / 17) -> world units, full width. The
/// program draws them as screen points (`Part1V_vs`: clip-space offset
/// psize × (0.001172, 0.002083) per corner), which is psize × ~0.0012 units
/// either side of the centre at a 60° field of view. ⚠️ (the corner
/// offsets and the game's field of view aren't read).
const POINT_SIZE: f32 = 0.0024;

#[derive(Clone)]
struct Sprite {
    model: usize,
    texture: String,
    start: f32,
    end: f32,
    colors: [u32; 2],
    /// Flipbook grid (words 0, 1).
    grid: [f32; 2],
    /// Size at birth / death (words 30, 31).
    size: [f32; 2],
    /// Frames each sprite lives.
    life: f32,
    /// Drift speed (word 9, per frame).
    drift: f32,
    /// Alpha-blended (puffs: smoke and dust darken what's behind) rather than additive.
    blend: bool,
    /// The emitter's own material adds (destination factor ONE, e.g. state `0x1400`
    /// SRC_ALPHA, ONE): drawn as an additive glow whatever its texture's alpha, never
    /// as the opaque cut-out a translucent puff gets (`puff_is_translucent`).
    additive: bool,
    /// The character bone it rides on (its joint's name) and where on it.
    joint: Option<String>,
    offset: Vec3,
    /// PARTICLE01/02 (`a07ac685`) read the program's way (`pdEftParticle1`,
    /// spawn `FUN_1401a2050`, draw `FUN_1401a1c00` / `FUN_1401a2880`):
    /// `count` points (word 30) on every effect frame from start to end,
    /// each living word 31 drawn frames, flying out in a random direction at
    /// word 7 (minus up to word 24) units per drawn frame, accelerated by
    /// words 8-10 per drawn frame, size word 16 -> 17 (point size;
    /// word 18 set = a random fixed size), colour word 12 -> 13. None for the
    /// MSPARTICLE puffs, which keep the older reading.
    points: Option<Points>,
    /// MSPARTICLE (`90e7fd3e`) read the program's way (`ms_particle.rs`): the
    /// emitter's own words, spawn / update / draw of `MSParticleFunc00/01`.
    ms: Option<ms_particle::MsSpec>,
}

#[derive(Clone, Copy)]
struct Points {
    count: usize,
    /// Speed per drawn frame (word 7) and how much less it can randomly be (word 24).
    speed: [f32; 2],
    /// Per drawn frame (words 8-10, the game's left-handed axes).
    acc: Vec3,
    /// Word 18 set: each point gets a fixed size in [size[0], size[0] + size[1]].
    random_size: bool,
}

/// `BLADE01` / `BLADE02` (kind `4ca477d9...`, the emitter's packed name): a
/// sword's swing ribbon, drawn between the bone the emitter rides on and a
/// point `length` along the bone's +Z, over the last few samples of the
/// bone's path (the program's `pdEftBlade01`, `FUN_14019f450` / draw
/// `FUN_14019ec80`; FORMATS-EFFECTS.md "Blades").
const KIND_BLADE: &str = "4ca477d9";

#[derive(Clone)]
struct Blade {
    model: usize,
    texture: String,
    /// Samples are taken on each effect frame from `start` to `end`.
    start: f32,
    end: f32,
    colors: [u32; 2],
    /// Effect frames the ribbon lingers (frozen in the world, colour fading
    /// `colors[0]` -> `colors[1]`) after `end` (word 0, in drawn frames).
    linger: f32,
    /// Spline steps between two samples (word 1).
    steps: usize,
    /// Tip distance along the bone's axis (word 28; the program negates it
    /// in its left-handed space = +Z in ours).
    length: f32,
    joint: Option<String>,
    offset: Mat4,
    /// Easing of the fade (word 10; `curve`).
    fade_curve: u8,
}

/// `FLAIR01` (kind `650e9921...`): a burst of flat wedge-shaped rays from the
/// emitter's point, each in a random orientation, growing / shrinking and
/// fading over a few frames -- the strike's starburst. The program's
/// `pdEftFlair1` (spawn `FUN_14010c480`, rays `FUN_1401a1170`, geometry
/// `FUN_1401a1620`). Sizes are world units as authored (rays are built in
/// world space, drawn with an identity world matrix). Word 24 set = each
/// ray's length is random: r x (word 6 - word 4) at birth and r' x (word 7 -
/// word 5) at death, r, r' uniform in [0, 1] -- the program multiplies the
/// span without adding the base back; word 25 the same for the width with
/// words 8-11 (FORMATS-EFFECTS.md "Effect units (2026-09-28)").
const KIND_FLARE: &str = "650e9921";

#[derive(Clone)]
struct Flare {
    model: usize,
    texture: String,
    /// New rays on every effect frame from `start` to `end` (inclusive).
    start: f32,
    end: f32,
    colors: [u32; 2],
    /// Rays per burst (word 30) and their life in drawn frames (word 31).
    count: usize,
    life: f32,
    /// Word 28: colours go start -> end over this many drawn frames and
    /// back over the last as many (0 = start -> end over the life).
    fade: f32,
    /// Ray length and tip width at birth / death: words 4, 5 and 8, 9, or,
    /// when `random` is set, the spans (w6 - w4, w7 - w5) / (w10 - w8,
    /// w11 - w9) each ray scales by its own random factor.
    length: [f32; 2],
    width: [f32; 2],
    /// Words 24 (length) and 25 (width): random per ray.
    random: [bool; 2],
    /// Word 20 (as an int; READ `FUN_1401a1620`: `+0x14 += (int)word 20` after every drawn
    /// frame): each ray's first Euler angle turns by this many units of 1 / 0x8000 turn per
    /// drawn frame, i.e. the ray spins in the plane of its length and width axes (the angle
    /// is the last rotation `FUN_140290a40` applies, about the matrix's own X axis).
    spin: i32,
    joint: Option<String>,
    offset: Vec3,
}

/// `LIGHT02` (kind `893e3f31...`): a dynamic point light in the program
/// (`FUN_14010c990`) that fills a slot of the renderer's light table with
/// the emitter's position, colour and range while it plays -- the wash of
/// colour a cast throws onto the floor and nearby walls in the footage
/// (VFX-AUDIT.md #3). It is not a glow sprite (that is a separate system,
/// `pdvkGlowExec`). Our stages are baked into unlit vertex colours
/// (`stage_look.rs`, `main.rs` "Stage materials are unlit and untouched"),
/// so a real `PointLight` would light nothing: drawn instead as a soft
/// additive circle on the floor under the caster, in the light's colour.
/// Fields (READ, `FUN_14010c990`; its emitter pointer is 8 bytes before word 0):
/// lit while start <= frame < end + 1 (words 2 / 3); a fade envelope `f`
/// (`light_fade`: in over word 7 frames, out over the last word 7, word 10's
/// easing, flipped for the fade-out) blends BOTH the range word 0 -> 1 and the
/// colour word 12 -> 13 (`colors`); the colour itself is never dimmed by `f`.
/// Range's world-unit scale for the stand-in floor disc is unconfirmed
/// (`RANGE_SCALE`, `PD_LIGHT_SCALE`); the stage and fighters read the raw range.
const KIND_LIGHT: &str = "893e3f31";

#[derive(Clone)]
struct Light {
    model: usize,
    /// Visible from `start` to `end` (effect frames), like a piece.
    start: f32,
    end: f32,
    colors: [u32; 2],
    /// Word 0 -> 1: light range, blended by the fade envelope (`light_fade`).
    range: [f32; 2],
    /// Word 7: frames of the fade in at the start and out at the end (0 = none).
    /// (Read as word 9 before 2026-10-09: that word is 0 in 659 of the 660 lights,
    /// so lights never faded and the range ran word 0 -> 1 across the whole window.)
    fade: f32,
    /// Word 10: easing of that fade (`light_fade`).
    fade_curve: u8,
}

/// The LIGHT02 fade envelope at effect frame `frame` (READ, `FUN_14010c990`): `None`
/// outside `start <= frame < end + 1` (the record is freed). `x` = frames since the
/// start; during the first `fade` frames, `x / F` (word 10 = 0), `x (x + 1) / F^2` (1)
/// or `x (2F - x - 1) / F^2` (2) (constants 0.5 `0x1402f7d40`, 1.0 `0x1402f7da8`, 2.0
/// `0x1402f2a10`); during the last `fade` frames the same with `x` = end + 1 - frame and
/// easing 1 and 2 swapped; 1 between.
fn light_fade(l: &Light, frame: f32) -> Option<f32> {
    if frame < l.start || frame >= l.end + 1.0 {
        return None;
    }
    let f = l.fade;
    let mut mode = l.fade_curve;
    let mut x = frame - l.start;
    if f <= x {
        let left = l.end + 1.0 - frame;
        if left >= f {
            return Some(1.0);
        }
        x = left;
        mode = match mode {
            1 => 2,
            2 => 1,
            m => m,
        };
    }
    let k = match mode {
        1 => x * (x + 1.0) / (f * f),
        2 => x * (2.0 * f - x - 1.0) / (f * f),
        _ => x / f,
    };
    Some(k.clamp(0.0, 1.0))
}

#[derive(Clone)]
struct Piece {
    model: usize,
    grid: [f32; 2],
    start: f32,
    end: f32,
    colors: [u32; 2],
    grow: Vec3,
    /// Word 20 (as an integer): when the next cell of a cross-fade runs past the
    /// sheet, 0 = hold the last cell, else wrap to the first (`FUN_1401a8510`).
    cell_wrap: bool,
    /// Drawn by a screen-distortion shader (PDEFTOPT05 family): its texture
    /// is a bump-offset map, not a colour (FORMATS-EFFECTS.md "Shaders").
    distort: bool,
    /// A solid, reflective surface (PDEFTOPT06: the mines' shells;
    /// YKREFRECT: Muramasa's and Rail Gun's blades): drawn blended from its
    /// base texture, not added.
    solid: bool,
    /// Its transform joint's name: the character bone it rides on.
    joint: Option<String>,
    /// Easing of the move, scale, colour and flipbook over the piece's life
    /// (words 8, 9, 10, 11: 0 linear, 1 ease in, 2 ease out; `curve`).
    curves: [u8; 4],
    /// Moved this far over its life along its own axes (words 4-6, the
    /// game's left-handed space).
    travel: Vec3,
    /// The draw's blend equation (material flags `+0x0C` bit `0x10` and state
    /// word `+0x14`, `FUN_1402a6010`); None = blending off (alpha test only).
    /// `blend_known` = false for a cache exported before those fields existed
    /// (then the piece is added, as before).
    blend: Option<BlendState>,
    blend_known: bool,
    /// Effect word 19 (`FUN_1401a5ff0`, `FUN_14011bc30`): 0 = billboard (the
    /// piece faces the camera, `billboard`), 1 = a mesh in its own
    /// orientation, 2 = a mesh drawn with no culling; every mode but 2 is
    /// back-face culled (`cull`). Absent (hand-made test data) = a mesh, no culling.
    billboard: bool,
    cull: bool,
    /// Word 7: roll in degrees over the move curve (`roll_radians`), about the
    /// piece's own Z (mesh) or the view axis (billboard).
    roll: f32,
    /// The shader family premultiplies the colour by its alpha in the vertex
    /// stage (`PDEFTBB*_vs`); the point-sprite (`MSEFTBB01S`) and `PDMTEX*`
    /// shaders pass the colour straight.
    premultiply: bool,
    /// The pixel stage multiplies the colour by the FULL alpha (texture a x
    /// colour a): `PDEFTAURA01_ps` (`mul r0.xyz, r0.wwww, r0.xyzx` before the
    /// output), the school aura's wisps. Without it their ONE,ONE draw adds the
    /// whole quad as a flat card.
    premultiply_full: bool,
    /// Word 28: where the piece's matrix comes from (`FUN_1401a5940`): 0 the
    /// joint's live matrix, 1 latched in world space the first frame it is
    /// drawn, 2 latched position with the live scale and world-aligned axes
    /// (`FxLatch`, `latch_pieces`). 3-8 (68 of 14,700 pieces: live root, aura)
    /// are not implemented and play as 0.
    latch: u8,
    /// Flipbook mode (`FUN_1401a5f20`, word 21 as an integer): false = one cell
    /// per effect tick, wrapping; true = the cells stretched over the window.
    stretch: bool,
    /// Words 22 / 23: texture scroll over the move curve (`FUN_1401a5ff0`'s
    /// constant 0x22), added to the UV after the flipbook scale.
    scroll: Vec2,
    /// Words 14 / 15 and 30 (`FUN_1401a9ae0`): with word 30 set the colour pair
    /// (12, 13) is used on even frames and (14, 15) on odd ones.
    colors_odd: [u32; 2],
    flicker: bool,
    /// Word 31: colour fade window in frames (0 = none).
    fade_win: f32,
    /// Shader family (`FAM_*`): how the pixel stage combines its textures.
    family: u8,
    /// The family's texture slots t0..t3 (`fx/<effect>_t<index>.png`, written by
    /// `pd_convert effects`); empty for the plain family.
    textures: [Option<String>; 4],
    /// `PDEFTAURA01` pieces are not timed pieces of the effect: the aura class
    /// (`FUN_14011acd0`) treats each as a wisp spawner (`WispSpec`).
    wisp: Option<WispSpec>,
    /// The wisp mesh with its bone slots and weights (`aura_wisp.rs`); absent in a cache exported before
    /// `pd_convert effects` wrote it (the spawner then draws one flat wisp per piece, as before).
    skin: Option<Arc<aura_wisp::WispSkin>>,
    /// `PDEFTOPT03/05` (`FUN_1401a9020`): the distortion's strength words (`DistSpec`).
    dist: Option<DistSpec>,
}

/// The heat-haze piece's words (`FUN_1401a9020`, the draw of `PDEFTOPT03/05`): over its life
/// (the word-11 curve `k`) the bump matrix is `diag(e, e)` with `e = lerp(w25, w27, k) / 512`
/// (`DAT_1402f7c40` = 1/512) and the vertex stage pushes each vertex `lerp(w24, w26, k)` world
/// units back along its normal before taking the screen position it samples the scene at; the
/// pixel stage (`PDEFTOPT05_ps`) offsets that screen position by `e x 2 (bump - 0.5)` and shows
/// `colour x scene` there, alpha-blended by the colour's alpha.
#[derive(Clone, Copy, Debug)]
struct DistSpec {
    /// Words 24 / 26: the normal offset, start / end.
    normal: [f32; 2],
    /// Words 25 / 27: the bump strength in 1/512 screen units, start / end.
    bump: [f32; 2],
}

impl DistSpec {
    /// (bump offset in screen UV, normal offset in world units) at curve value `k`.
    fn at(&self, k: f32) -> (f32, f32) {
        ((self.bump[0] + (self.bump[1] - self.bump[0]) * k) / 512.0, self.normal[0] + (self.normal[1] - self.normal[0]) * k)
    }
}

/// The school aura's wisps (`pdEftAura` update `FUN_14011acd0`, found 2026-10-01).
/// Each `PDEFTAURA01` piece is a spawner, not a looping piece: on every effect
/// frame it tries once to start a wisp, with chance `(r * w24) >> 15 == 1` for a
/// 15-bit random `r` (about 1 / word 24); a wisp lives `word 26 + rand x word 27`
/// frames on its own clock (first drawn at its frame 1, colour / scale curves
/// run on that clock), with each grow axis (words 16-18) scaled by a random
/// 0.5-1 and a random roll of 0-`word 25` degrees about its chain axis. The
/// program's pool holds 80 wisps for all fighters, so one piece can have several
/// alive at once; here a piece node holds one, and the aura plays two copies of
/// the effect to make up the overlap (`aura_fx.rs`).
#[derive(Clone, Copy, Debug)]
struct WispSpec {
    /// Word 24 as an integer: spawn chance ~ 1 / rate (0 / 1 never spawn).
    rate: i32,
    /// Words 26, 27: life = base + rand x spread, in frames.
    life: [f32; 2],
    /// Word 25: the random roll range, degrees.
    roll: f32,
}

impl WispSpec {
    /// Chance of a spawn on one try: the share of 15-bit `r` with `r * rate` in `[0x8000, 0x10000)`.
    fn chance(&self) -> f32 {
        if self.rate < 2 {
            return 0.0;
        }
        let w = self.rate as i64;
        let (lo, hi) = ((32768 + w - 1) / w, (65535 / w).min(32767));
        (hi - lo + 1).max(0) as f32 / 32768.0
    }
}

/// Shader families (`fx_gamma.wgsl` `extra.y`).
const FAM_PLAIN: u8 = 0;
const FAM_BB02: u8 = 1;
const FAM_MTEX: u8 = 2;
const FAM_AURA: u8 = 3;
const FAM_BB08: u8 = 4;
/// `PDEFTOPT03/05`: the screen-distortion (heat haze) pieces, `fx_gamma.wgsl` family 5.
const FAM_DIST: u8 = 5;

/// How a material shader's pixel stage combines its textures, among the
/// families `fx_gamma.wgsl` implements (disassembled 2026-10-01, docs/
/// FORMATS-EFFECTS.md "Pixel stage"). `PDEFTBB02` only differs from the plain
/// `PDEFTBB01` when its flipbook is stretched (a cell cross-faded with the next);
/// per-tick flipbooks leave its second weight at 0.
fn shader_family(shader: &str, stretched_flipbook: bool) -> u8 {
    match shader {
        "PDEFTBB02" if stretched_flipbook => FAM_BB02,
        "PDMTEX01" | "PDMTEX02" | "PDMTEX01N" => FAM_MTEX,
        "PDEFTAURA01" => FAM_AURA,
        "PDEFTBB08" => FAM_BB08,
        "PDEFTOPT05" | "PDEFTOPT03" => FAM_DIST,
        _ => FAM_PLAIN,
    }
}

/// The program's three easing curves, as it evaluates them for a timed
/// block (`FUN_1401a5ff0` for plain pieces, `FUN_14019ec80` for the blade's
/// fade): with `d` = end - start + 1 frames and `e` = frames since start,
/// 0 = e / d, 1 = e (e + 1) / (d (d + 1)) (slow start), 2 = e (2d - e + 1) /
/// (d (d + 1)) (fast start; reaches 1 at e = d). Corrected 2026-09-30
/// (audits/P0-EFFECTS.md D13): this used `e (2d - e - 1)`, which is the
/// `ykEDGESLUR00` class's formula (`FUN_140122ac0`), not the pieces': the
/// program adds `(2d) - (e - 1)` (`((d + d) - (e - 1)) * e * 0.5 * norm`).
fn curve(kind: u8, e: f32, d: f32) -> f32 {
    let d = d.max(1.0);
    let e = e.clamp(0.0, d);
    match kind {
        1 => e * (e + 1.0) / (d * (d + 1.0)),
        2 => e * (2.0 * d - e + 1.0) / (d * (d + 1.0)),
        _ => e / d,
    }
    .clamp(0.0, 1.0)
}

/// A piece's roll in radians (effect word 7 x move curve, `FUN_1401a5ff0`):
/// the degrees are truncated to an integer, turned into a 0x8000-step
/// circle by `deg * 0x8000 / 0x168` (integer division) and the step index
/// taken modulo 0x8000 (`FUN_140290800` looks it up in a sine table).
fn roll_radians(w7: f32, move_curve: f32) -> f32 {
    let deg = (w7 * move_curve) as i64;
    let step = (deg * 0x8000) / 0x168;
    ((step & 0x7fff) as f32) / 32768.0 * std::f32::consts::TAU
}

/// Shaders that draw a solid surface (texture 0 plus an environment
/// reflection) rather than a screen distortion: `PDEFTOPT06` (colour =
/// tex0 mixed with reflections, alpha 1) and `YKREFRECT` (tex0 x colour +
/// reflection, alpha from the colour). Disassembled 2026-09-27.
fn is_solid_shader(s: &str) -> bool {
    s.starts_with("PDEFTOPT06") || s.starts_with("YKREFR")
}

/// Distortion pieces are drawn as a faint additive layer this strong
/// (0 = hidden, the default: a heat-haze ripple isn't reproduced yet).
/// `PD_FX_DISTORT=0.2` shows them faintly, `1` as they were before.
fn distort_strength() -> f32 {
    env_once("PD_FX_DISTORT").and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

/// `PD_FX_BLEND` (A/B switch for the per-piece blend state): `0` = every piece
/// added like before, `1` = the program's own blends, `2` (default) = the new
/// blends except the multiply-darken ones (`ZERO, 1 - SRC_COLOR`), which are
/// added as before. 2 is the default until the original's glow pass exists
/// (P0-LIGHTING lane 3): without it the darken-only pieces read as dark
/// smudges (`shots/fxa_pwopt_sheet.png` D), and Laurence picks from that sheet
/// (DECISIONS 2026-10-01).
fn blend_switch() -> u8 {
    env_once("PD_FX_BLEND").and_then(|v| v.parse().ok()).unwrap_or(2)
}

/// `PD_FX_OFF=blade,flare,points,ms,sprite,trail,piece,haze` (bisecting a look): leave these kinds of
/// effect content out. Dev switch only; the default draws everything.
fn kind_off(kind: &str) -> bool {
    env_once("PD_FX_OFF").is_some_and(|v| v.split(',').any(|n| n.trim() == kind))
}

/// `PD_FX_BILLBOARD=0` / `PD_FX_CULL=0` (A/B switches): pieces keep their
/// model orientation / draw both faces, as before.
fn fx_flag(name: &'static str) -> bool {
    env_once(name).and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0) > 0.0
}

pub struct FxDef {
    slots: Vec<Vec<usize>>,
    pieces: HashMap<String, Piece>,
    trails: Vec<Trail>,
    sprites: Vec<Sprite>,
    blades: Vec<Blade>,
    flares: Vec<Flare>,
    lights: Vec<Light>,
}

/// An emitter's joint matrix from the fx.json (`"matrix"`, column-major).
fn joint_matrix(e: &serde_json::Value) -> Mat4 {
    e["matrix"].as_array().and_then(|a| <[f32; 16]>::try_from(a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>()).ok()).map_or(Mat4::IDENTITY, |c| Mat4::from_cols_array(&c))
}

fn joint_of(v: &serde_json::Value) -> Option<String> {
    v["joint"].as_str().filter(|s| !s.is_empty()).map(String::from)
}

impl FxDef {
    /// `has_texture(path)`: whether a sprite texture was written to the cache.
    fn parse(name: &str, v: &serde_json::Value, has_texture: impl Fn(&str) -> bool) -> Option<Self> {
        let slots = v["slots"].as_array()?.iter().map(|s| s.as_array().map(|a| a.iter().filter_map(|m| m.as_u64()).map(|m| m as usize).collect()).unwrap_or_default()).collect();
        let f = |p: &serde_json::Value, k: &str, i: usize| p[k][i].as_f64().unwrap_or(0.0) as f32;
        let mut pieces = HashMap::new();
        let (switch, billboards, culling) = (blend_switch(), fx_flag("PD_FX_BILLBOARD"), fx_flag("PD_FX_CULL"));
        // A piece's 0x13 word as a float (0 when absent / not finite).
        let word_f = |p: &serde_json::Value, i: usize| {
            let w = p["raw"][i].as_u64().map_or(0.0, |w| f32::from_bits(w as u32));
            if w.is_finite() { w } else { 0.0 }
        };
        for p in v["pieces"].as_array()? {
            let shader = p["shader"].as_str().unwrap_or("");
            let solid = is_solid_shader(shader);
            // Word 19 as a float (see `Piece::billboard`).
            let mode19 = p["raw"][19].as_u64().map(|w| f32::from_bits(w as u32));
            let blend = BlendState::decode(p["mat_flags"].as_u64().unwrap_or(0) as u16, p["mat_state"].as_u64().unwrap_or(0) as u16);
            let grid = [f(p, "grid", 0).max(1.0), f(p, "grid", 1).max(1.0)];
            let stretch = word_f(p, 21) as i32 != 0;
            let family = shader_family(shader, grid[0] * grid[1] > 1.0 && stretch);
            // The family's texture slots, when its images were exported.
            let textures: [Option<String>; 4] = if family == FAM_PLAIN || p["mat_tex"].as_array().is_none() {
                Default::default()
            } else {
                std::array::from_fn(|k| {
                    // `PDEFTBB02` cross-fades a cell with the next one of its own texture (slot 0).
                    let slot = if family == FAM_BB02 { 0 } else { k };
                    let t = p["mat_tex"][slot].as_u64()?;
                    let path = format!("fx/{name}_t{t}.png");
                    has_texture(&path).then_some(path)
                })
            };
            let family = if family != FAM_PLAIN && textures[0].is_none() { FAM_PLAIN } else { family };
            pieces.insert(
                p["node"].as_str()?.to_string(),
                Piece {
                    model: p["model"].as_u64().unwrap_or(0) as usize,
                    grid,
                    start: p["start"].as_f64().unwrap_or(0.0) as f32,
                    end: p["end"].as_f64().unwrap_or(0.0) as f32,
                    colors: [p["colors"][0].as_u64().unwrap_or(0) as u32, p["colors"][1].as_u64().unwrap_or(0) as u32],
                    grow: Vec3::new(f(p, "grow", 0), f(p, "grow", 1), f(p, "grow", 2)),
                    cell_wrap: word_f(p, 20) as i32 != 0,
                    distort: p["shader"].as_str().is_some_and(|s| s.starts_with("PDEFTOPT") && !is_solid_shader(s)) && family != FAM_DIST,
                    solid,
                    joint: joint_of(p),
                    curves: [8, 9, 10, 11].map(|i| {
                        let w = p["raw"][i].as_u64().map_or(0.0, |w| f32::from_bits(w as u32));
                        if w.is_finite() { w.clamp(0.0, 2.0) as u8 } else { 0 }
                    }),
                    travel: Vec3::new(p["raw"][4].as_u64().map_or(0.0, |w| f32::from_bits(w as u32)), p["raw"][5].as_u64().map_or(0.0, |w| f32::from_bits(w as u32)), p["raw"][6].as_u64().map_or(0.0, |w| f32::from_bits(w as u32))),
                    blend_known: p["mat_state"].is_u64() && switch != 0 && !(switch == 2 && blend.is_some_and(|b| b.src == BlendFactor::Zero && b.dst == BlendFactor::OneMinusSrcColor)),
                    blend,
                    billboard: billboards && mode19.is_some_and(|m| m == 0.0) && !solid,
                    // `MSEFTBB01S` (`FUN_140115ca0`) draws a screen point at the
                    // piece's origin, not the quad: a point has no back to cull.
                    cull: culling && mode19.is_some_and(|m| m != 2.0) && p["mat_state"].is_u64() && switch != 0 && !shader.starts_with("MSEFTBB01S"),
                    roll: p["raw"][7].as_u64().map(|w| f32::from_bits(w as u32)).filter(|r| r.is_finite()).unwrap_or(0.0),
                    premultiply: shader.starts_with("PDEFTBB"),
                    premultiply_full: shader.starts_with("PDEFTAURA"),
                    latch: {
                        let m = word_f(p, 28);
                        if m.is_finite() && (1.0..3.0).contains(&m) { m as u8 } else { 0 }
                    },
                    stretch: word_f(p, 21) as i32 != 0,
                    scroll: Vec2::new(word_f(p, 22), word_f(p, 23)).clamp(Vec2::splat(-100.0), Vec2::splat(100.0)),
                    colors_odd: [p["raw"][14].as_u64().map_or(0, |w| w as u32), p["raw"][15].as_u64().map_or(0, |w| w as u32)],
                    flicker: word_f(p, 30) as i32 != 0,
                    fade_win: word_f(p, 31).clamp(0.0, 1000.0),
                    family,
                    textures,
                    dist: (family == FAM_DIST).then(|| DistSpec { normal: [word_f(p, 24), word_f(p, 26)], bump: [word_f(p, 25), word_f(p, 27)] }),
                    wisp: (shader == "PDEFTAURA01" && wisps_enabled()).then(|| WispSpec { rate: word_f(p, 24) as i32, life: [word_f(p, 26).max(1.0), word_f(p, 27).max(0.0)], roll: word_f(p, 25) }),
                    skin: (shader == "PDEFTAURA01" && wisps_enabled()).then(|| aura_wisp::WispSkin::parse(&p["skin"]).map(Arc::new)).flatten(),
                },
            );
        }
        let trails = v["emitters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["kind"].as_str() == Some(KIND_TRAIL))
            .filter_map(|e| {
                Some(Trail {
                    model: e["model"].as_u64()? as usize,
                    texture: format!("fx/{name}_t{}.png", e["texture"].as_u64()?),
                    start: e["start"].as_f64().unwrap_or(0.0) as f32,
                    end: e["end"].as_f64().unwrap_or(0.0) as f32,
                    colors: [e["colors"][0].as_u64().unwrap_or(0) as u32, e["colors"][1].as_u64().unwrap_or(0) as u32],
                    width: f(e, "grow", 0).max(0.1) * TRAIL_WIDTH,
                    length: f(e, "grow", 2).max(1.0) * TRAIL_LENGTH,
                })
            })
            .collect();
        let word = |e: &serde_json::Value, i: usize| e["raw"][i].as_u64().map_or(0.0, |w| f32::from_bits(w as u32));
        let sprites = v["emitters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["kind"].as_str().is_some_and(|k| k.starts_with(KIND_SPRITES) || k.starts_with(KIND_PUFFS)))
            .filter_map(|e| {
                let texture = format!("fx/{name}_t{}.png", e["texture"].as_u64()?);
                if !has_texture(&texture) {
                    return None;
                }
                let sane = |x: f32, lo: f32, hi: f32, or: f32| if x.is_finite() && x >= lo && x <= hi { x } else { or };
                if e["kind"].as_str().is_some_and(|k| k.starts_with(KIND_SPRITES)) {
                    let fin = |x: f32| if x.is_finite() { x } else { 0.0 };
                    return Some(Sprite {
                        model: e["model"].as_u64()? as usize,
                        texture,
                        start: e["start"].as_f64().unwrap_or(0.0) as f32,
                        end: e["end"].as_f64().unwrap_or(0.0) as f32,
                        colors: [e["colors"][0].as_u64().unwrap_or(0) as u32, e["colors"][1].as_u64().unwrap_or(0) as u32],
                        grid: [sane(word(e, 0), 1.0, 16.0, 1.0).round(), sane(word(e, 1), 1.0, 16.0, 1.0).round()],
                        size: [sane(word(e, 16), 0.0, 100_000.0, 1000.0) * POINT_SIZE, sane(word(e, 17), 0.0, 100_000.0, 1000.0) * POINT_SIZE],
                        // Drawn frames -> effect frames.
                        life: sane(word(e, 31), 1.0, 600.0, 10.0) / STEPS_PER_FRAME,
                        drift: 0.0,
                        blend: false,
                        additive: false,
                        joint: joint_of(e),
                        offset: joint_matrix(e).w_axis.truncate(),
                        points: Some(Points {
                            count: sane(word(e, 30), 0.0, 64.0, 1.0) as usize,
                            speed: [fin(word(e, 7)).clamp(-50.0, 50.0), fin(word(e, 24)).clamp(-50.0, 50.0)],
                            acc: Vec3::new(fin(word(e, 8)), fin(word(e, 9)), fin(word(e, 10))).clamp(Vec3::splat(-10.0), Vec3::splat(10.0)),
                            random_size: word(e, 18) != 0.0,
                        }),
                        ms: None,
                    });
                }
                Some(Sprite {
                    model: e["model"].as_u64()? as usize,
                    texture,
                    start: e["start"].as_f64().unwrap_or(0.0) as f32,
                    end: e["end"].as_f64().unwrap_or(0.0) as f32,
                    colors: [e["colors"][0].as_u64().unwrap_or(0) as u32, e["colors"][1].as_u64().unwrap_or(0) as u32],
                    grid: [sane(word(e, 0), 1.0, 16.0, 1.0).round(), sane(word(e, 1), 1.0, 16.0, 1.0).round()],
                    size: [sane(word(e, 30), 0.5, 60.0, 5.0) * SPRITE_SIZE, sane(word(e, 31), 0.5, 80.0, 10.0) * SPRITE_SIZE],
                    life: sane(word(e, 8), 0.0, 90.0, 0.0).max(SPRITE_MIN_LIFE),
                    drift: sane(word(e, 9).abs(), 0.0, 2.0, 0.0),
                    blend: e["kind"].as_str().is_some_and(|k| k.starts_with(KIND_PUFFS)),
                    additive: BlendState::decode(e["mat_flags"].as_u64().unwrap_or(0) as u16, e["mat_state"].as_u64().unwrap_or(0) as u16).is_some_and(|b| b.dst == BlendFactor::One),
                    joint: joint_of(e),
                    offset: joint_matrix(e).w_axis.truncate(),
                    points: None,
                    ms: ms_particle::enabled().then(|| ms_particle::MsSpec::from_emitter(e, joint_matrix(e))).flatten(),
                })
            })
            .collect();
        let blades = v["emitters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["kind"].as_str().is_some_and(|k| k.starts_with(KIND_BLADE)))
            .filter_map(|e| {
                let texture = format!("fx/{name}_t{}.png", e["texture"].as_u64()?);
                if !has_texture(&texture) {
                    return None;
                }
                Some(Blade {
                    model: e["model"].as_u64()? as usize,
                    texture,
                    start: e["start"].as_f64().unwrap_or(0.0) as f32,
                    end: e["end"].as_f64().unwrap_or(0.0) as f32,
                    colors: [e["colors"][0].as_u64().unwrap_or(0) as u32, e["colors"][1].as_u64().unwrap_or(0) as u32],
                    linger: word(e, 0).clamp(1.0, 60.0) / STEPS_PER_FRAME,
                    steps: word(e, 1).clamp(1.0, 32.0) as usize,
                    length: word(e, 28).clamp(0.0, 200.0),
                    joint: joint_of(e),
                    offset: joint_matrix(e),
                    fade_curve: word(e, 10).clamp(0.0, 2.0) as u8,
                })
            })
            .collect();
        let flares = v["emitters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["kind"].as_str().is_some_and(|k| k.starts_with(KIND_FLARE)))
            .filter_map(|e| {
                let texture = format!("fx/{name}_t{}.png", e["texture"].as_u64()?);
                if !has_texture(&texture) {
                    return None;
                }
                let sane = |x: f32, hi: f32| if x.is_finite() { x.clamp(0.0, hi) } else { 0.0 };
                let size = |x: f32| if x.is_finite() { x.clamp(-500.0, 500.0) } else { 0.0 };
                // `FUN_1401a1170`: word 24 / 25 set -> random length / width,
                // r x (max - min) per ray (no base added back); else fixed.
                let random = [word(e, 24) != 0.0, word(e, 25) != 0.0];
                let pair = |lo: usize, hi: usize, rnd: bool| if rnd { [size(word(e, hi) - word(e, lo)), size(word(e, hi + 1) - word(e, lo + 1))] } else { [size(word(e, lo)), size(word(e, lo + 1))] };
                Some(Flare {
                    model: e["model"].as_u64()? as usize,
                    texture,
                    start: e["start"].as_f64().unwrap_or(0.0) as f32,
                    end: e["end"].as_f64().unwrap_or(0.0) as f32,
                    colors: [e["colors"][0].as_u64().unwrap_or(0) as u32, e["colors"][1].as_u64().unwrap_or(0) as u32],
                    count: sane(word(e, 30), 64.0) as usize,
                    life: sane(word(e, 31), 120.0).max(1.0),
                    fade: sane(word(e, 28), 120.0),
                    length: pair(4, 6, random[0]),
                    width: pair(8, 10, random[1]),
                    random,
                    spin: if word(e, 20).is_finite() { word(e, 20).clamp(-32768.0, 32768.0) as i32 } else { 0 },
                    joint: joint_of(e),
                    offset: joint_matrix(e).w_axis.truncate(),
                })
            })
            .filter(|f| f.count > 0)
            .collect();
        let lights = v["emitters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["kind"].as_str().is_some_and(|k| k.starts_with(KIND_LIGHT)))
            .filter_map(|e| {
                Some(Light {
                    model: e["model"].as_u64()? as usize,
                    start: e["start"].as_f64().unwrap_or(0.0) as f32,
                    end: e["end"].as_f64().unwrap_or(0.0) as f32,
                    colors: [e["colors"][0].as_u64().unwrap_or(0) as u32, e["colors"][1].as_u64().unwrap_or(0) as u32],
                    range: [word(e, 0).max(0.0), word(e, 1).max(0.0)],
                    fade: word(e, 7).clamp(0.0, 120.0),
                    fade_curve: word(e, 10).clamp(0.0, 2.0) as u8,
                })
            })
            .collect();
        Some(FxDef { slots, pieces, trails, sprites, blades, flares, lights })
    }

    /// (Used by the skill audit only since the guard stand-in bubble went, `fx.rs`.)
    #[allow(dead_code)]
    pub fn has_slot(&self, slot: usize) -> bool {
        self.slots.get(slot).is_some_and(|s| !s.is_empty())
    }

    /// Whether any of the slot's geometry pieces is drawn (distortion pieces
    /// are hidden unless PD_FX_DISTORT is set).
    fn draws_pieces(&self, slot: usize) -> bool {
        let Some(models) = self.slots.get(slot) else { return false };
        let distort = distort_strength() > 0.0;
        self.pieces.values().any(|p| models.contains(&p.model) && (distort || !p.distort))
    }

    /// Whether playing `slot` shows anything: drawn pieces, trails, or sprite
    /// emitters. A slot can hold models whose only content is hidden
    /// distortion or undrawn emitters (Hell's Fire, Swarm of Moths, the
    /// crawlers' flight before 2026-09-27).
    pub fn shows(&self, slot: usize, _looping: bool) -> bool {
        let Some(models) = self.slots.get(slot) else { return false };
        self.draws_pieces(slot)
            || self.trails.iter().any(|t| models.contains(&t.model))
            || self.sprites.iter().any(|s| models.contains(&s.model))
            || self.blades.iter().any(|b| models.contains(&b.model))
            || self.flares.iter().any(|f| models.contains(&f.model))
            || self.lights.iter().any(|l| models.contains(&l.model))
    }

    /// Sword swing ribbons (BLADE01/02) in `slot`.
    #[cfg(test)]
    pub fn blade_count(&self, slot: usize) -> usize {
        self.slots.get(slot).map_or(0, |models| self.blades.iter().filter(|b| models.contains(&b.model)).count())
    }

    /// What `slot` shows, for logs and the sweep test: drawn pieces,
    /// hidden (distortion) pieces, trails, sprite emitters.
    pub fn census(&self, slot: usize) -> [usize; 4] {
        let Some(models) = self.slots.get(slot) else { return [0; 4] };
        let mine = |m: &usize| models.contains(m);
        let drawn = self.pieces.values().filter(|p| mine(&p.model) && !p.distort).count();
        let hidden = self.pieces.values().filter(|p| mine(&p.model) && p.distort).count();
        [drawn, hidden, self.trails.iter().filter(|t| mine(&t.model)).count(), self.sprites.iter().filter(|s| mine(&s.model)).count()]
    }

    /// Last frame any piece of the slot is visible.
    fn slot_len(&self, slot: usize) -> f32 {
        let models = &self.slots[slot];
        let pieces = self.pieces.values().filter(|p| models.contains(&p.model)).map(|p| p.end);
        let trails = self.trails.iter().filter(|t| models.contains(&t.model)).map(|t| t.end);
        let sprites = self.sprites.iter().filter(|s| models.contains(&s.model)).map(|s| s.end + if s.points.is_some() { 1.0 + s.life } else if let Some(ms) = &s.ms { 1.0 + ms.life() } else { 0.0 });
        // A blade's ribbon lingers after its last sample.
        let blades = self.blades.iter().filter(|b| models.contains(&b.model)).map(|b| b.end + b.linger);
        let flares = self.flares.iter().filter(|f| models.contains(&f.model)).map(|f| f.end + 1.0 + f.life / STEPS_PER_FRAME);
        let lights = self.lights.iter().filter(|l| models.contains(&l.model)).map(|l| l.end);
        pieces.chain(trails).chain(sprites).chain(blades).chain(flares).chain(lights).fold(1.0, f32::max)
    }
}

/// Effect definitions by name ("eft0042"), loaded on first use.
#[derive(Resource)]
pub struct FxLibrary {
    cache: PathBuf,
    defs: HashMap<String, Option<Arc<FxDef>>>,
    /// `fx/bones.json` (pd_convert effects): character id -> named body
    /// bone -> joint index (the glb node `p0j<index>`). Loaded on first use.
    bones: Option<HashMap<String, HashMap<String, usize>>>,
}

impl FxLibrary {
    pub fn new(cache: PathBuf) -> Self {
        FxLibrary { cache, defs: HashMap::new(), bones: None }
    }

    /// Joint index of character `id`'s bone `name` (None: no such bone, or
    /// an old cache without `fx/bones.json`).
    pub fn bone_index(&mut self, id: &str, name: &str) -> Option<usize> {
        let cache = &self.cache;
        let bones = self.bones.get_or_insert_with(|| {
            let v: serde_json::Value = std::fs::read_to_string(cache.join("fx/bones.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            v.as_object()
                .into_iter()
                .flatten()
                .map(|(id, m)| (id.clone(), m.as_object().into_iter().flatten().filter_map(|(n, i)| Some((n.clone(), i.as_u64()? as usize))).collect()))
                .collect()
        });
        bones.get(id)?.get(bone_for(name)).copied()
    }

    pub fn get(&mut self, name: &str) -> Option<Arc<FxDef>> {
        let cache = &self.cache;
        self.defs
            .entry(name.to_string())
            .or_insert_with(|| {
                let text = std::fs::read_to_string(cache.join(format!("fx/{name}.fx.json"))).ok()?;
                FxDef::parse(name, &serde_json::from_str(&text).ok()?, |tex| cache.join(tex).exists()).map(Arc::new)
            })
            .clone()
    }
}

/// The model and textures of the effects a match can play, loaded when the match starts and held
/// until the next one (Laurence 2026-10-07: frame drops when casting). Without these handles a cast
/// loaded its effect's glb and textures from disk, and once the effect ended the last handle went
/// and Bevy freed them, so every later cast of that skill loaded them again. `PD_FX_PRELOAD=0`
/// turns it off (A/B on a PC).
#[derive(Resource, Default)]
pub struct FxPreload {
    /// The effect names the handles are for (sorted), to notice a new match.
    names: Vec<String>,
    handles: Vec<UntypedHandle>,
}

impl FxPreload {
    pub fn enabled() -> bool {
        env_once("PD_FX_PRELOAD").map_or(true, |v| v != "0")
    }

    /// The effects held (sorted; `warmup.rs` warms their pipelines).
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Hold every asset a play of the effects `names` loads (the same paths and loader settings
    /// as `play_from`, `attach_pieces` and `spawn_trails`, so a cast finds them loaded). No-op when
    /// the set is the one already held; the old set is dropped when it changes.
    pub fn hold(&mut self, mut names: Vec<String>, assets: &AssetServer, lib: &mut FxLibrary) {
        names.sort();
        names.dedup();
        if names == self.names {
            return;
        }
        let mut handles = Vec::new();
        for name in &names {
            let Some(def) = lib.get(name) else { continue };
            if !def.pieces.is_empty() {
                handles.push(assets.load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset(format!("fx/{name}.glb"))).untyped());
            }
            for p in def.pieces.values() {
                handles.extend(p.textures.iter().flatten().map(|t| load_fx_image(assets, t).untyped()));
            }
            let plain = def.trails.iter().map(|t| &t.texture).chain(def.sprites.iter().map(|s| &s.texture)).chain(def.blades.iter().map(|b| &b.texture)).chain(def.flares.iter().map(|f| &f.texture));
            handles.extend(plain.map(|t| assets.load::<Image>(t.clone()).untyped()));
        }
        self.names = names;
        self.handles = handles;
    }
}

/// One playing effect slot (the root of its spawned scene).
#[derive(Component)]
pub struct FxInstance {
    def: Arc<FxDef>,
    slot: usize,
    /// Frames since it started.
    age: f32,
    len: f32,
    looping: bool,
    /// Viewer stills: show this frame and never expire.
    fixed: Option<f32>,
    /// Seconds spent waiting for the model (see `FxLive`).
    waited: f32,
}

/// One aura wisp slot on a `PDEFTAURA01` piece node (`WispSpec`).
#[derive(Component)]
pub struct FxWisp {
    seed: u32,
    /// The last whole effect frame whose spawn try / ageing has run.
    last: i64,
    active: bool,
    /// The whole frame it started on, its life in frames, its random grow and roll.
    spawned: i64,
    life: f32,
    grow: Vec3,
    roll: f32,
    count: u32,
}

impl FxWisp {
    /// Run the spawn tries and ageing for every whole frame up to `age` (the program's
    /// update runs once per frame: a live wisp ages, one past its life is freed, a free
    /// slot tries to start one). Returns the wisp's frame (its first drawn frame is 1),
    /// None while idle.
    fn step(&mut self, spec: &WispSpec, base_grow: Vec3, age: f32) -> Option<f32> {
        let whole = age.floor() as i64;
        // Don't replay a long stall.
        if whole - self.last > 90 {
            self.last = whole - 90;
        }
        while self.last < whole {
            self.last += 1;
            if self.active && ((self.last - self.spawned) as f32 + 1.0) > self.life {
                self.active = false;
            }
            if !self.active {
                self.count = self.count.wrapping_add(1);
                let r = |k: u32| hash01(self.seed, self.count.wrapping_mul(8).wrapping_add(k));
                if r(0) < spec.chance() {
                    self.active = true;
                    self.spawned = self.last;
                    self.life = spec.life[0] + r(1) * spec.life[1];
                    // Each grow axis x (0.5 + 0.5 r); a roll of up to word 25 degrees.
                    self.grow = Vec3::new(base_grow.x * (0.5 + 0.5 * r(2)), base_grow.y * (0.5 + 0.5 * r(3)), base_grow.z * (0.5 + 0.5 * r(4)));
                    self.roll = (r(5) * spec.roll).to_radians();
                }
            }
        }
        self.active.then(|| age - self.spawned as f32 + 1.0)
    }
}

/// `PD_AURA_WISPS=0`: the aura's pieces loop together as before (A/B for sheets).
fn wisps_enabled() -> bool {
    env_once("PD_AURA_WISPS").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0) > 0.0
}

#[derive(Component)]
pub struct FxPiece {
    piece: Piece,
    base: Transform,
    material: Handle<FxMaterial>,
    instance: Entity,
    /// Its row in the per-instance buffer (`effects/inst.rs`); None = it animates its own material.
    row: Option<u32>,
}

/// The character an effect instance plays on (the entity whose glTF scene
/// holds its skeleton). Pieces and emitters whose joint is named like one of
/// its bones ride on that bone (`bind_bones`), as in the program: each
/// effect joint takes the world matrix of the caster's (or victim's) bone
/// of the same name, else the effect's own matrix (`FUN_140189420`, bone
/// lookup `FUN_14018aa50`; FORMATS-EFFECTS.md "Where pieces attach").
#[derive(Component)]
pub struct FxHost(pub Entity);

/// The program's attach names that stand for two bones (`FUN_14018aaf0`'s
/// table: "LHRH" = left and right hand, ...): ours follows the first.
fn bone_for(name: &str) -> &str {
    match name {
        "LHRH" => "LHAND",
        "LFRF" => "LFOOT",
        "RKRH" | "RKRA" => "RKATA",
        "LKLH" => "LKATA",
        "B1RH" | "B1LH" | "B1RF" | "B1LF" | "B1RA" | "B1LA" => "BODY1",
        "LARA" => "LADDHA",
        "NERA" | "NELA" => "NECK",
        n => n,
    }
}

/// PD_FX_ROOT_ONLY=1 (A/B test switch): the old playback, every piece on the
/// effect root and no sword ribbons.
fn root_only() -> bool {
    env_once("PD_FX_ROOT_ONLY").is_some()
}

/// A piece that has been put on its bone (or left on the effect root).
#[derive(Component)]
pub struct FxBound;

// The game area's top-left corner in the window (physical pixels), written by `display::update_area` every frame
// (this file is shared with the viewer, which has no `display`, so the mirror lives here; the viewer leaves it 0).
static AREA_ORIGIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn area_origin() -> UVec2 {
    let v = AREA_ORIGIN.load(std::sync::atomic::Ordering::Relaxed);
    UVec2::new((v >> 32) as u32, v as u32)
}

pub fn set_area_origin(o: UVec2) {
    AREA_ORIGIN.store((o.x as u64) << 32 | o.y as u64, std::sync::atomic::Ordering::Relaxed);
}

/// Optional, on an effect instance's root: multiplies every piece's colour by
/// `rgb` and its alpha by `view_alpha[quadrant]` (the screen quadrant of the
/// split-screen viewport the fragment is drawn in: 0 top-left, 1 top-right,
/// 2 bottom-left, 3 bottom-right; chosen in `fx_gamma.wgsl` the way
/// `dissolve.wgsl` does, so up to four views each get their own). The program does both for the school aura: a colour
/// override list multiplies the piece's RGB by the school colour and its alpha
/// by the fighter's model opacity (`FUN_14011b450`, `school_aura.rs`). Nothing
/// else in the game adds this component, so every other effect is unchanged.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FxTint {
    pub rgb: Vec3,
    pub view_alpha: [f32; 4],
}

/// The skill effects' material: a StandardMaterial whose additive draws add
/// the colour values the game adds (`fx_gamma.wgsl`). The original blends in
/// gamma space with no tone mapping, so overlapping or over-the-scene glows
/// reach bright pink / white where Bevy's linear-light blending plus
/// per-fragment tone mapping leaves them dim (Laurence 2026-09-27: "lacking
/// a lot of the brighter FX ... the bright centre").
pub type FxMaterial = bevy::pbr::ExtendedMaterial<StandardMaterial, FxGamma>;

static FX_SHADER: std::sync::OnceLock<Handle<bevy::shader::Shader>> = std::sync::OnceLock::new();

#[derive(Asset, bevy::render::render_resource::AsBindGroup, TypePath, Clone, Debug)]
#[bind_group_data(FxKey)]
#[uniform(100, FxUniform)]
pub struct FxGamma {
    /// x = exponent the additive colour is re-encoded with (1 / 2.2; 1 = off),
    /// y = how a piece's colour is encoded for its blend (`fx_gamma.wgsl`; 0 =
    /// not a piece: the old handling), z, w = the game area's top-left corner in
    /// the window (`display::area_origin`, written with `quad`; zero unless a
    /// fixed aspect ratio leaves bars). The per-view alpha is `quad`.
    params: Vec4,
    /// x = billboard (1) and y = its roll in radians, z = premultiply the
    /// colour by its alpha (the `PDEFTBB*` vertex shaders do), w = that
    /// alpha. Pieces only (`fx_gamma.wgsl`).
    piece: Vec4,
    /// The shader families' own texture slots t0..t3 (`fx_gamma.wgsl`); None =
    /// the fallback image. Plain pieces leave all four empty.
    #[texture(102)]
    #[sampler(103)]
    tex0: Option<Handle<Image>>,
    #[texture(104)]
    #[sampler(105)]
    tex1: Option<Handle<Image>>,
    #[texture(106)]
    #[sampler(107)]
    tex2: Option<Handle<Image>>,
    #[texture(108)]
    #[sampler(109)]
    tex3: Option<Handle<Image>>,
    /// UV placement for t0 / t1 (xy = scale, zw = offset) and the family's
    /// extras (x = cross-fade weight, y = family).
    uv0: Vec4,
    uv1: Vec4,
    extra: Vec4,
    /// This effect's alpha multiplier per screen quadrant (1 = untouched;
    /// `FxTint::view_alpha`, set by `animate_pieces`, read by `fx_gamma.wgsl`).
    quad: Vec4,
    /// A scene-copy blend (`fx_gamma.wgsl` mode 7): x = 1 the result darkens the screen
    /// (reverse-subtract / subtract / min: the pipeline subtracts a delta), 2 it lightens it
    /// (max: adds one); y / z = the source / destination factor codes, w = the operation
    /// (`scene_op`). All zero for every other piece.
    scene: Vec4,
    /// The draw's own blend equation (material flags `+0x0C` / state `+0x14`;
    /// `pd_formats::effect::BlendState`), set on the pipeline by `specialize`.
    /// None = the StandardMaterial's own alpha mode blends.
    blend: Option<BlendState>,
    /// The per-instance rows (`effects/inst.rs`), shared by every effect material: a piece whose mesh
    /// has a `MeshTag` reads its animated values from its row instead of this material.
    #[storage(110, read_only)]
    inst: Handle<bevy::render::storage::ShaderBuffer>,
}

/// The seven vec4s the shader reads from `FxGamma`'s fields, packed into ONE uniform buffer
/// (binding 100, `fx_gamma.wgsl`'s `FxU`). Seven bindings of one vec4 each meant seven GPU
/// buffers per material, and a material the render thread re-prepares every frame (a piece's
/// colour or flipbook cell changes) paid for all seven: with ~100 pieces live that was most of
/// the 15-20 ms `prepare-assets` of a four-player fight (perf lane, 2026-10-03).
#[derive(bevy::render::render_resource::ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct FxUniform {
    pub params: Vec4,
    pub piece: Vec4,
    pub uv0: Vec4,
    pub uv1: Vec4,
    pub extra: Vec4,
    pub quad: Vec4,
    pub scene: Vec4,
}

impl bevy::render::render_resource::AsBindGroupShaderType<FxUniform> for FxGamma {
    fn as_bind_group_shader_type(&self, _images: &bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>) -> FxUniform {
        FxUniform { params: self.params, piece: self.piece, uv0: self.uv0, uv1: self.uv1, extra: self.extra, quad: self.quad, scene: self.scene }
    }
}

/// What selects a pipeline variant: the blend equation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FxKey {
    blend: Option<BlendState>,
    /// A distortion piece (family 5) reads the screen copy and must not write depth.
    dist: bool,
    /// A scene-copy blend piece (`FxGamma::scene`): 0 no, 1 darkens, 2 lightens.
    scene: u8,
}

impl From<&FxGamma> for FxKey {
    fn from(g: &FxGamma) -> Self {
        FxKey { blend: g.blend, dist: g.extra.y > 4.5 && g.extra.y < 5.5, scene: g.scene.x as u8 }
    }
}

impl bevy::pbr::MaterialExtension for FxGamma {
    fn vertex_shader() -> bevy::shader::ShaderRef {
        FX_SHADER.get().cloned().map_or(bevy::shader::ShaderRef::Default, bevy::shader::ShaderRef::Handle)
    }
    fn fragment_shader() -> bevy::shader::ShaderRef {
        FX_SHADER.get().cloned().map_or(bevy::shader::ShaderRef::Default, bevy::shader::ShaderRef::Handle)
    }
    /// The piece's own source / destination factor and operation on the
    /// fragment target (the program's `FUN_14028e4e0`, mapped to wgpu's).
    fn specialize(
        _: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _: &bevy::mesh::MeshVertexBufferLayoutRef,
        key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        if key.bind_group_data.dist || key.bind_group_data.scene != 0 {
            if let Some(ds) = descriptor.depth_stencil.as_mut() {
                ds.depth_write_enabled = Some(false);
            }
        }
        if key.bind_group_data.scene != 0 {
            // The fragment stage works out the game's gamma-space result from the scene copy and
            // hands over the difference it makes in the linear target: subtract it (darkening
            // blends) or add it (max). The alpha channel is left alone.
            use bevy::render::render_resource as rr;
            let op = if key.bind_group_data.scene == 1 { rr::BlendOperation::ReverseSubtract } else { rr::BlendOperation::Add };
            let state = rr::BlendState {
                color: rr::BlendComponent { src_factor: rr::BlendFactor::One, dst_factor: rr::BlendFactor::One, operation: op },
                alpha: rr::BlendComponent { src_factor: rr::BlendFactor::Zero, dst_factor: rr::BlendFactor::One, operation: rr::BlendOperation::Add },
            };
            for target in descriptor.fragment.iter_mut().flat_map(|f| f.targets.iter_mut()).flatten() {
                target.blend = Some(state);
            }
            return Ok(());
        }
        if let Some(b) = key.bind_group_data.blend {
            let state = wgpu_blend(b);
            for target in descriptor.fragment.iter_mut().flat_map(|f| f.targets.iter_mut()).flatten() {
                target.blend = Some(state);
            }
        }
        Ok(())
    }
}

/// A piece's blend equation as wgpu's (colour and alpha alike: the program
/// sets one factor pair). D3D11, like wgpu, ignores the factors for min / max.
fn wgpu_blend(b: BlendState) -> bevy::render::render_resource::BlendState {
    use bevy::render::render_resource as rr;
    let factor = |f: BlendFactor| match f {
        BlendFactor::Zero => rr::BlendFactor::Zero,
        BlendFactor::One | BlendFactor::Constant => rr::BlendFactor::One,
        BlendFactor::SrcColor => rr::BlendFactor::Src,
        BlendFactor::OneMinusSrcColor => rr::BlendFactor::OneMinusSrc,
        BlendFactor::SrcAlpha => rr::BlendFactor::SrcAlpha,
        BlendFactor::OneMinusSrcAlpha => rr::BlendFactor::OneMinusSrcAlpha,
        BlendFactor::DstAlpha => rr::BlendFactor::DstAlpha,
        BlendFactor::OneMinusDstAlpha => rr::BlendFactor::OneMinusDstAlpha,
        BlendFactor::DstColor => rr::BlendFactor::Dst,
        BlendFactor::OneMinusDstColor => rr::BlendFactor::OneMinusDst,
        BlendFactor::SrcAlphaSaturate => rr::BlendFactor::SrcAlphaSaturated,
    };
    let operation = match b.op {
        BlendOp::Add => rr::BlendOperation::Add,
        BlendOp::Subtract => rr::BlendOperation::Subtract,
        BlendOp::ReverseSubtract => rr::BlendOperation::ReverseSubtract,
        BlendOp::Min => rr::BlendOperation::Min,
        BlendOp::Max => rr::BlendOperation::Max,
    };
    let (src_factor, dst_factor) = if matches!(b.op, BlendOp::Min | BlendOp::Max) { (rr::BlendFactor::One, rr::BlendFactor::One) } else { (factor(b.src), factor(b.dst)) };
    let c = rr::BlendComponent { src_factor, dst_factor, operation };
    rr::BlendState { color: c, alpha: c }
}

/// `PD_FX_GAMMA=1` (A/B switch): effects blend the old way (linear light,
/// tone-mapped).
fn fx_material(base: StandardMaterial) -> FxMaterial {
    fx_material_gamma(base, fx_gamma())
}

fn fx_gamma() -> f32 {
    env_once("PD_FX_GAMMA").and_then(|v| v.parse::<f32>().ok()).filter(|g| *g > 0.1).unwrap_or(2.2)
}

/// `gamma` = 1: blended like any StandardMaterial.
fn fx_material_gamma(base: StandardMaterial, gamma: f32) -> FxMaterial {
    bevy::pbr::ExtendedMaterial { base, extension: FxGamma { params: Vec4::new(1.0 / gamma, 0.0, 1.0, 1.0), piece: Vec4::ZERO, quad: Vec4::ONE, tex0: None, tex1: None, tex2: None, tex3: None, uv0: Vec4::new(1.0, 1.0, 0.0, 0.0), uv1: Vec4::new(1.0, 1.0, 0.0, 0.0), extra: Vec4::ZERO, scene: Vec4::ZERO, blend: None, inst: inst::buffer() } }
}

/// How a piece's colour is encoded for the factors its blend meets
/// (`fx_gamma.wgsl`'s `params.y` modes).
fn blend_mode(blend: Option<BlendState>) -> f32 {
    use BlendFactor::*;
    let Some(b) = blend else { return 6.0 };
    match (b.src, b.dst) {
        (One | SrcAlpha | SrcColor | Constant, One) => 1.0,
        (DstColor, One) => 2.0,
        (Zero, OneMinusSrcColor) => 3.0,
        (Zero, SrcColor) | (DstColor, Zero) => 5.0,
        _ => 4.0,
    }
}

/// A piece's material: its own blend equation and, for billboards, the
/// shader's placement and roll (`fx_gamma.wgsl`). `images` = the family's
/// texture slots, loaded (empty for the plain family).
fn piece_material(base: StandardMaterial, p: &Piece, images: [Option<Handle<Image>>; 4]) -> FxMaterial {
    let mut m = fx_material_gamma(base, fx_gamma());
    m.extension.extra.y = p.family as f32;
    let [t0, t1, t2, t3] = images;
    m.extension.tex0 = t0;
    m.extension.tex1 = t1;
    m.extension.tex2 = t2;
    m.extension.tex3 = t3;
    m.extension.params.y = if p.blend_known { blend_mode(p.blend) } else { 0.0 };
    m.extension.piece = Vec4::new(if p.billboard { 1.0 } else { 0.0 }, 0.0, if p.premultiply_full { 2.0 } else if p.premultiply { 1.0 } else { 0.0 }, 1.0);
    m.extension.blend = p.blend.filter(|_| p.blend_known);
    if let Some(sc) = scene_blend(p) {
        m.extension.scene = sc;
        m.extension.params.y = 7.0;
    }
    m
}

/// The factor codes `fx_gamma.wgsl`'s scene blend evaluates (the `BlendFactor` order).
fn factor_code(f: BlendFactor) -> f32 {
    match f {
        BlendFactor::Zero => 0.0,
        BlendFactor::One | BlendFactor::Constant => 1.0,
        BlendFactor::SrcColor => 2.0,
        BlendFactor::OneMinusSrcColor => 3.0,
        BlendFactor::SrcAlpha => 4.0,
        BlendFactor::OneMinusSrcAlpha => 5.0,
        BlendFactor::DstAlpha => 6.0,
        BlendFactor::OneMinusDstAlpha => 7.0,
        BlendFactor::DstColor => 8.0,
        BlendFactor::OneMinusDstColor => 9.0,
        BlendFactor::SrcAlphaSaturate => 10.0,
    }
}

/// `PD_FX_SCENE=0` (A/B switch): every piece blends with the pipeline's own equation, as before.
fn scene_blend_enabled() -> bool {
    fx_flag("PD_FX_SCENE")
}

/// A piece whose blend operation is not ADD (reverse-subtract, subtract, min, max: ~390 of the
/// 26,500 pieces, 22 effects) cannot be done on our linear-light target -- the program does it
/// on gamma-space values (`dst - src^2` is not `dst_lin - src_lin^2`). Such a piece is drawn in the
/// transmissive pass, reads the scene copy there (as the heat haze does), works the program's
/// equation out in gamma space and subtracts / adds the difference
/// (`fx_gamma.wgsl` mode 7). x = 1 darkens, 2 lightens; y, z = factor codes; w = operation.
fn scene_blend(p: &Piece) -> Option<Vec4> {
    if !p.blend_known || p.solid || p.family == FAM_DIST || !scene_blend_enabled() {
        return None;
    }
    let b = p.blend?;
    let (kind, op) = match b.op {
        BlendOp::Add => return None,
        BlendOp::Subtract => (1.0, 1.0),
        BlendOp::ReverseSubtract => (1.0, 2.0),
        BlendOp::Min => (1.0, 3.0),
        BlendOp::Max => (2.0, 4.0),
    };
    Some(Vec4::new(kind, factor_code(b.src), factor_code(b.dst), op))
}

/// Bone attachment and sword ribbons (game and viewer). The rest of
/// playback (spawn_trails ... animate_trails) is registered by the caller.
pub fn plugin(app: &mut App) {
    let shader = app.world_mut().resource_mut::<Assets<bevy::shader::Shader>>().add(bevy::shader::Shader::from_wgsl(include_str!("fx_gamma.wgsl"), "pd_game/fx_gamma.wgsl"));
    let _ = FX_SHADER.set(shader);
    app.add_plugins(MaterialPlugin::<FxMaterial>::default());
    inst::plugin(app);
    app.init_resource::<FxMatCache>().add_systems(First, mat_cache::sweep);
    app.add_systems(Update, ms_particle::tick);
    app.add_systems(Update, (bind_bones.after(attach_pieces).before(animate_pieces), cleanup_detached))
        .add_systems(PostUpdate, (update_blades, update_flares, update_lights, aura_wisp::update).after(bevy::transform::TransformSystems::Propagate))
        .add_systems(Update, aura_wisp::cleanup)
        .add_systems(PostUpdate, latch_pieces.after(bevy::transform::TransformSystems::Propagate).before(bevy::camera::visibility::VisibilitySystems::CheckVisibility));
    if env_once("PD_FX_FRAMELOG").is_some() {
        app.add_systems(Last, frame_log);
    }
}

/// `PD_FX_FRAMELOG=1` (measurement only): frame-to-frame wall time, split by
/// whether any effect is playing, printed to stderr every 60 frames.
fn frame_log(instances: Query<(), With<FxInstance>>, mut last: Local<Option<std::time::Instant>>, mut acc: Local<[(f64, u32); 2]>) {
    let now = std::time::Instant::now();
    let Some(prev) = last.replace(now) else { return };
    let k = usize::from(!instances.is_empty());
    acc[k].0 += (now - prev).as_secs_f64() * 1000.0;
    acc[k].1 += 1;
    if (acc[0].1 + acc[1].1) % 60 == 0 {
        let mean = |a: (f64, u32)| if a.1 > 0 { a.0 / a.1 as f64 } else { 0.0 };
        eprintln!("[fxframe] idle {:.2} ms over {} frames | effect playing {:.2} ms over {} frames", mean(acc[0]), acc[0].1, mean(acc[1]), acc[1].1);
    }
}

/// Bone name -> node entity, per host character (built once its scene has spawned).
#[derive(Default)]
pub struct BoneCache(HashMap<Entity, HashMap<String, Entity>>);

/// `host`'s bone `name` (a node `p0j<index>` of its glTF scene), or None if
/// it has no such bone. `Err(())` = the host's scene isn't there yet: retry.
#[allow(clippy::too_many_arguments)]
fn find_bone(cache: &mut BoneCache, lib: &mut FxLibrary, host: Entity, name: &str, roots: &Query<&WorldAssetRoot>, children: &Query<&Children>, names: &Query<&Name>) -> Result<Option<Entity>, ()> {
    if !cache.0.contains_key(&host) {
        let mut nodes = HashMap::new();
        for e in children.iter_descendants(host) {
            if let Ok(n) = names.get(e) {
                if n.as_str().starts_with("p0j") {
                    nodes.insert(n.as_str().to_string(), e);
                }
            }
        }
        if nodes.is_empty() {
            return Err(());
        }
        cache.0.insert(host, nodes);
    }
    let id = roots.get(host).ok().and_then(|r| r.0.path().and_then(|p| p.path().file_stem().map(|s| s.to_string_lossy().to_string())));
    let Some(id) = id else { return Ok(None) };
    let Some(idx) = lib.bone_index(&id, name) else { return Ok(None) };
    Ok(cache.0[&host].get(&format!("p0j{idx}")).copied())
}

/// Put pieces, sprite emitters and blades of effects played on a character
/// onto the bones their joints name. Pieces are re-parented (they then move
/// with the bone through the swing); emitters and blades read the bone's
/// world matrix when they emit / sample.
#[allow(clippy::too_many_arguments)]
pub fn bind_bones(
    mut commands: Commands,
    lib: Option<ResMut<FxLibrary>>,
    pieces: Query<(Entity, &FxPiece), Without<FxBound>>,
    mut emitters: Query<&mut FxEmitter>,
    mut blades: Query<&mut FxBlade>,
    mut flares: Query<&mut FxFlare>,
    hosts: Query<&FxHost>,
    roots: Query<&WorldAssetRoot>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut cache: Local<BoneCache>,
) {
    let Some(mut lib) = lib else { return };
    cache.0.retain(|h, _| roots.contains(*h));
    // Ok(Some(bone)) / Ok(None) = stay on the root / Err = not ready.
    let mut resolve = |instance: Entity, joint: Option<&String>, lib: &mut FxLibrary| -> Result<Option<Entity>, ()> {
        let (Some(name), Ok(host), false) = (joint, hosts.get(instance), root_only()) else { return Ok(None) };
        find_bone(&mut cache, lib, host.0, name, &roots, &children, &names)
    };
    for (e, p) in &pieces {
        match resolve(p.instance, p.piece.joint.as_ref(), &mut lib) {
            Ok(Some(bone)) => {
                commands.entity(e).insert((FxBound, ChildOf(bone)));
            }
            Ok(None) => {
                commands.entity(e).insert(FxBound);
            }
            Err(()) => {}
        }
    }
    for mut em in emitters.iter_mut().filter(|em| !em.bound) {
        if let Ok(bone) = resolve(em.instance, em.sprite.joint.as_ref(), &mut lib) {
            em.bone = bone;
            em.bound = true;
        }
    }
    for mut b in blades.iter_mut().filter(|b| !b.bound) {
        if let Ok(bone) = resolve(b.instance, b.blade.joint.as_ref(), &mut lib) {
            b.bone = bone;
            b.bound = true;
        }
    }
    for mut f in flares.iter_mut().filter(|f| !f.bound) {
        if let Ok(bone) = resolve(f.instance, f.flare.joint.as_ref(), &mut lib) {
            f.bone = bone;
            f.bound = true;
        }
    }
}

/// Pieces moved onto a bone and blade ribbons aren't under their effect's
/// root any more: they go when it does.
pub fn cleanup_detached(mut commands: Commands, instances: Query<(), With<FxInstance>>, pieces: Query<(Entity, &FxPiece), With<FxBound>>, blades: Query<(Entity, &FxBlade)>, flares: Query<(Entity, &FxFlare)>, lights: Query<(Entity, &FxLight)>) {
    for (e, p) in &pieces {
        if !instances.contains(p.instance) {
            commands.entity(e).despawn();
        }
    }
    for (e, b) in &blades {
        if !instances.contains(b.instance) {
            commands.entity(e).despawn();
        }
    }
    for (e, f) in &flares {
        if !instances.contains(f.instance) {
            commands.entity(e).despawn();
        }
    }
    for (e, l) in &lights {
        if !instances.contains(l.instance) {
            commands.entity(e).despawn();
        }
    }
}

/// One playing sword ribbon (a world-space mesh rebuilt as it samples).
#[derive(Component)]
pub struct FxBlade {
    /// Shared, so the per-frame copy `update_blades` takes is a counter bump, not a string copy.
    blade: Arc<Blade>,
    instance: Entity,
    bone: Option<Entity>,
    bound: bool,
    /// (base, tip) world points, newest first; one per effect frame.
    samples: Vec<(Vec3, Vec3)>,
    /// Effect frame of the last sample.
    last: f32,
    /// A sample came in since the mesh was last written (`mesh_reuse.rs`).
    dirty: bool,
    mesh: Handle<Mesh>,
    material: Handle<FxMaterial>,
    row: Option<u32>,
}

/// Catmull-Rom point between `p1` and `p2` (the program smooths the ribbon
/// between samples, `FUN_14017e120`; the exact spline is unconfirmed).
fn catmull(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let (t2, t3) = (t * t, t * t * t);
    0.5 * ((2.0 * p1) + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
}

/// The ribbon through `samples` (newest first): a strip from base (u = 0)
/// to tip (u = 1), v = 0 at the newest sample to 1 at the oldest, `steps`
/// spline steps between samples; vertex alpha ramps in over the first and
/// out over the last half-sample, like the program's vertex colours.
fn blade_mesh(samples: &[(Vec3, Vec3)], steps: usize) -> Mesh {
    let mut s = mesh_reuse::Strip::default();
    blade_fill(samples, steps, &mut s);
    s.into_mesh()
}

/// `blade_mesh`'s vertices, written into an emptied strip.
fn blade_fill(samples: &[(Vec3, Vec3)], steps: usize, out: &mut mesh_reuse::Strip) {
    let n = samples.len();
    let at = |i: isize| samples[i.clamp(0, n as isize - 1) as usize];
    let mut edge = Vec::new();
    if n >= 2 {
        for i in 0..n - 1 {
            let (a0, a1, a2, a3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
            for s in 0..steps {
                let t = s as f32 / steps as f32;
                edge.push((catmull(a0.0, a1.0, a2.0, a3.0, t), catmull(a0.1, a1.1, a2.1, a3.1, t)));
            }
        }
        edge.push(samples[n - 1]);
    }
    let m = edge.len();
    let ramp = (steps / 2).max(1) as f32;
    let mesh_reuse::Strip { pos, uv, col, idx, .. } = out;
    for (k, (base, tip)) in edge.iter().enumerate() {
        let v = k as f32 / (m.max(2) - 1) as f32;
        let a = (k as f32 / ramp).min((m - 1 - k) as f32 / ramp).clamp(0.0, 1.0);
        pos.push(base.to_array());
        pos.push(tip.to_array());
        uv.push([0.0, v]);
        uv.push([1.0, v]);
        col.push([1.0, 1.0, 1.0, a]);
        col.push([1.0, 1.0, 1.0, a]);
        if k + 1 < m {
            let b = (k * 2) as u32;
            idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
        }
    }
}

/// One playing ray burst (a world-space mesh rebuilt each frame).
#[derive(Component)]
pub struct FxFlare {
    /// Shared (see `FxBlade::blade`).
    flare: Arc<Flare>,
    instance: Entity,
    bone: Option<Entity>,
    bound: bool,
    /// Live rays.
    rays: Vec<FlareRay>,
    /// Effect frame of the last burst, and the last frame seen.
    last: f32,
    seen: f32,
    seed: u32,
    /// The rays changed since the mesh was last written (`mesh_reuse.rs`).
    stale: bool,
    mesh: Handle<Mesh>,
}

/// One playing LIGHT02 floor decal: a flat, world-space quad under the
/// caster (not a child of the effect root, which sits at chest/prop height,
/// not the floor -- see `fx.rs`'s cast transform).
#[derive(Component)]
pub struct FxLight {
    light: Light,
    instance: Entity,
    material: Handle<FxMaterial>,
    row: Option<u32>,
}

/// One live FLAIR01 ray: centre, direction, width axis, age in drawn
/// frames, and its own length / tip width at birth and death.
#[derive(Clone, Copy)]
struct FlareRay {
    c: Vec3,
    dir: Vec3,
    across: Vec3,
    age: f32,
    length: [f32; 2],
    width: [f32; 2],
}

/// A ray's sizes: the emitter's fixed pair, or its random spans times this
/// ray's own factors `r` (length birth, length death, width birth, width
/// death), as `FUN_1401a1170` draws them.
fn ray_sizes(f: &Flare, r: [f32; 4]) -> ([f32; 2], [f32; 2]) {
    let pick = |v: [f32; 2], rnd: bool, a: f32, b: f32| if rnd { [v[0] * a, v[1] * b] } else { v };
    (pick(f.length, f.random[0], r[0], r[1]), pick(f.width, f.random[1], r[2], r[3]))
}

/// A ray's length and width axes after `age` drawn frames of the emitter's word-20 spin: the
/// program adds `(int)word 20` to the ray's first angle after each drawn frame (`FUN_1401a1620`),
/// so the first frame is drawn unturned. Angles are in 1 / 0x8000 turn (`FUN_140290a40`'s table).
fn flare_spin(f: &Flare, dir: Vec3, across: Vec3, age: f32) -> (Vec3, Vec3) {
    if f.spin == 0 {
        return (dir, across);
    }
    let a = (f.spin as f32 * age.max(0.0).floor()) * std::f32::consts::TAU / 32768.0;
    let (s, c) = a.sin_cos();
    (dir * c + across * s, across * c - dir * s)
}

/// The rays as flat wedges: two tip corners `width` apart at `length` from
/// the centre, both base corners at the centre (`FUN_1401a1620`). Colour
/// and alpha per vertex (each ray has its own age). A negative size (the
/// random spans are often max < min) only mirrors the wedge; its magnitude
/// is drawn.
fn flare_mesh(f: &Flare, rays: &[FlareRay]) -> Mesh {
    let mut s = mesh_reuse::Strip::default();
    flare_fill(f, rays, &mut s);
    s.into_mesh()
}

/// `flare_mesh`'s vertices, written into an emptied strip.
fn flare_fill(f: &Flare, rays: &[FlareRay], out: &mut mesh_reuse::Strip) {
    for &FlareRay { c, dir, across, age, length, width } in rays {
        let (dir, across) = flare_spin(f, dir, across, age);
        let k = (age / f.life).clamp(0.0, 1.0);
        let len = (length[0] + (length[1] - length[0]) * k).abs();
        let w = (width[0] + (width[1] - width[0]) * k).abs() * 0.5;
        let tip = c + dir * len;
        let b = out.pos.len() as u32;
        for p in [tip - across * w, tip + across * w, c, c] {
            out.pos.push(p.to_array());
        }
        out.uv.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]);
        let cc = argb(f.colors[0]).lerp(argb(f.colors[1]), flare_fade(f, age));
        let lin = Color::srgba(cc.x, cc.y, cc.z, cc.w).to_linear();
        out.col.extend_from_slice(&[[lin.red, lin.green, lin.blue, lin.alpha]; 4]);
        out.idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
    }
    // Never empty (a zero-length vertex buffer upsets the render allocator).
    out.never_empty();
}

/// How far a ray's colour has gone from start to end (`FUN_1401a1620`):
/// over its life when word 28 is 0; else towards the end colour over the
/// first `fade` drawn frames and back over the last `fade`.
fn flare_fade(f: &Flare, age: f32) -> f32 {
    if f.fade <= 0.0 {
        return (age / f.life).clamp(0.0, 1.0);
    }
    if age < f.fade {
        age / f.fade
    } else {
        ((f.life - age) / f.fade).min(1.0)
    }
    .clamp(0.0, 1.0)
}

/// Burst, age and redraw the flares (after transform propagation, like the blades).
pub fn update_flares(instances: Query<&FxInstance>, globals: Query<&GlobalTransform>, mut flares: Query<(&mut FxFlare, &mut Visibility)>, mut meshes: ResMut<Assets<Mesh>>) {
    for (mut f, mut vis) in &mut flares {
        let Ok(inst) = instances.get(f.instance) else { continue };
        if !f.bound {
            continue;
        }
        let frame = inst.fixed.unwrap_or(inst.age);
        let spec = f.flare.clone();
        // Rays age in drawn frames.
        let dt = (frame - f.seen).max(0.0) * STEPS_PER_FRAME;
        f.seen = frame;
        let before = f.rays.len();
        if dt != 0.0 && before > 0 {
            f.stale = true;
        }
        for r in f.rays.iter_mut() {
            r.age += dt;
        }
        f.rays.retain(|r| r.age < spec.life);
        if f.rays.len() != before {
            f.stale = true;
        }
        // A burst on each effect frame of the window (the program re-arms its
        // ray slots every frame the emitter is on).
        let now = frame.floor();
        if now >= spec.start && now <= spec.end && now > f.last {
            f.last = now;
            if let Ok(g) = globals.get(f.bone.unwrap_or(f.instance)) {
                let c = g.transform_point(spec.offset);
                for i in 0..spec.count as u32 {
                    let n = f.rays.len() as u32 + i + (now as u32) * 97;
                    let dir = cone_dir(f.seed, n * 2, 180.0);
                    let side = dir.cross(cone_dir(f.seed, n * 2 + 1, 180.0)).normalize_or(dir.any_orthonormal_vector());
                    let r = [0, 1, 2, 3].map(|j| hash01(f.seed ^ 0x5eed_f1a4, n * 4 + j));
                    let (length, width) = ray_sizes(&spec, r);
                    f.rays.push(FlareRay { c, dir, across: side, age: (frame - now) * STEPS_PER_FRAME, length, width });
                    f.stale = true;
                }
            }
        }
        *vis = if f.rays.is_empty() { Visibility::Hidden } else { Visibility::Inherited };
        // Rays unchanged since the last write (none left, or no time passed): the mesh already holds them.
        if f.stale || !mesh_reuse::enabled() {
            f.stale = false;
            let rays = &f.rays;
            mesh_reuse::rewrite(&mut meshes, &f.mesh, |s| flare_fill(&spec, rays, s));
        }
    }
}

/// Sample and redraw the sword ribbons. Runs after transform propagation so
/// it reads this frame's bone pose.
pub fn update_blades(instances: Query<&FxInstance>, globals: Query<&GlobalTransform>, mut blades: Query<(&mut FxBlade, &mut Visibility)>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<FxMaterial>>, mut rows: Option<ResMut<inst::FxInstances>>) {
    for (mut b, mut vis) in &mut blades {
        let Ok(inst) = instances.get(b.instance) else { continue };
        if !b.bound {
            continue;
        }
        let frame = inst.fixed.unwrap_or(inst.age);
        let spec = b.blade.clone();
        // A new sample each drawn frame inside the window (the program
        // records one per draw call); the ribbon keeps `end - start + 2` of
        // them (the program's history length), i.e. the last few ticks of the swing.
        let step = (frame * STEPS_PER_FRAME).floor();
        if frame >= spec.start && frame < spec.end && (b.samples.is_empty() || step > b.last) {
            let Ok(g) = globals.get(b.bone.unwrap_or(b.instance)) else { continue };
            let m = g.affine() * bevy::math::Affine3A::from_mat4(spec.offset);
            let sample = (m.transform_point3(Vec3::ZERO), m.transform_point3(Vec3::Z * spec.length));
            b.last = step;
            b.samples.insert(0, sample);
            b.samples.truncate((spec.end - spec.start) as usize + 2);
            b.dirty = true;
        }
        let k = ((frame - spec.end) / spec.linger).clamp(0.0, 1.0);
        let show = b.samples.len() >= 2 && frame >= spec.start && k < 1.0;
        // The fade follows the word-10 easing (`FUN_14019ec80`), in drawn frames.
        let k = curve(spec.fade_curve, (frame - spec.end).max(0.0) * STEPS_PER_FRAME, spec.linger * STEPS_PER_FRAME);
        *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
        if !show {
            continue;
        }
        let c = argb(spec.colors[0]).lerp(argb(spec.colors[1]), k);
        // (Compare before writing: a write through `get_mut` flags the material for re-preparation.)
        let col = Color::srgba(c.x, c.y, c.z, c.w);
        if let (Some(r), Some(rows)) = (b.row, rows.as_deref_mut()) {
            rows.set_color(r, col);
        } else if let Some(mut mat) = mats.get_mut(&b.material) {
            if mat.base.base_color != col {
                mat.base.base_color = col;
            }
        }
        // No new sample since the last write (the swing is over, the ribbon fades): the mesh already holds it.
        if b.dirty || !mesh_reuse::enabled() {
            b.dirty = false;
            let samples = &b.samples;
            mesh_reuse::rewrite(&mut meshes, &b.mesh, |s| blade_fill(samples, spec.steps, s));
        }
    }
}

/// Move, size and colour the LIGHT02 floor decals (after transform
/// propagation, like the blades/flares, so it reads this frame's caster
/// position). The decal isn't parented to the effect root (chest height):
/// it tracks the root's x/z at floor height instead.
pub fn update_lights(instances: Query<&FxInstance>, globals: Query<&GlobalTransform>, mut lights: Query<(&FxLight, &mut Transform, &mut Visibility)>, mut mats: ResMut<Assets<FxMaterial>>, mut rows: Option<ResMut<inst::FxInstances>>) {
    let scale = range_scale();
    for (l, mut t, mut vis) in &mut lights {
        let Ok(inst) = instances.get(l.instance) else { continue };
        let frame = inst.fixed.unwrap_or(inst.age);
        let spec = &l.light;
        // The program's envelope blends the range and the colour (`light_fade`).
        let fade = light_fade(spec, frame);
        *vis = if fade.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        let Some(k) = fade else { continue };
        let Ok(root) = globals.get(l.instance) else { continue };
        let range = (spec.range[0] + (spec.range[1] - spec.range[0]) * k).max(0.0) * scale;
        let p = root.translation();
        // A cast's root is a child of the caster's own visual entity at
        // local `Transform::IDENTITY` (`fx.rs`'s `play_from` + `ChildOf`),
        // i.e. the character rig's own root -- feet/floor level, wherever
        // the stage's own floor height is at that point (a raised block, a
        // step, ...), not chest height (that offset is the barrier bubble's
        // own stand-in transform, a different system).
        t.translation = Vec3::new(p.x, p.y + LIGHT_DECAL_Y, p.z);
        t.scale = Vec3::new(range * 2.0, 1.0, range * 2.0);
        // The light record takes the blended colour's rgb only (`FUN_14010c990` x 1/255):
        // alpha 1 while lit (the stage / fighters read rgb x alpha, `char_light::effect_light_of`).
        let c = argb(spec.colors[0]).lerp(argb(spec.colors[1]), k);
        let col = Color::srgba(c.x, c.y, c.z, 1.0);
        if let (Some(r), Some(rows)) = (l.row, rows.as_deref_mut()) {
            rows.set_color(r, col);
        } else if let Some(mut mat) = mats.get_mut(&l.material) {
            if mat.base.base_color != col {
                mat.base.base_color = col;
            }
        }
    }
}

/// Start playing `slot` of effect `name`. Returns the root entity (parent it
/// to follow something, or move its Transform). None if the slot is empty.
pub fn play(commands: &mut Commands, assets: &AssetServer, lib: &mut FxLibrary, name: &str, slot: usize, at: Transform, looping: bool, fixed: Option<f32>) -> Option<Entity> {
    play_from(commands, assets, lib, name, slot, at, looping, fixed, 0.0)
}

/// Like `play`, starting `from` frames in, for a caller that wants to skip
/// ahead into a slot's timeline. `play` is `from = 0.0`; fx.rs's own
/// cast/hit/impact/block plays are all `play`, since the game now starts
/// every cast effect at the cast start, not partway in (a melee swing used to
/// skip to the fire tick here; PROGRAM-RE.md: "slot 0 at cast start"). The
/// viewer's `--fx-frame` instead freezes on one frame (`fixed`, in `play`).
/// None if the slot shows nothing.
#[allow(clippy::too_many_arguments)]
pub fn play_from(commands: &mut Commands, assets: &AssetServer, lib: &mut FxLibrary, name: &str, slot: usize, at: Transform, looping: bool, fixed: Option<f32>, from: f32) -> Option<Entity> {
    let def = lib.get(name)?;
    if !def.shows(slot, looping) {
        return None;
    }
    let len = def.slot_len(slot);
    let inst = FxInstance { def: def.clone(), slot, age: from, len, looping, fixed, waited: 0.0 };
    if !def.draws_pieces(slot) {
        // Only trails / sprites: no model to load, nothing to hide.
        return Some(commands.spawn((at, Visibility::Inherited, inst, FxLive)).id());
    }
    let scene = assets.load(GltfAssetLabel::Scene(0).from_asset(format!("fx/{name}.glb")));
    // Hidden until attach_pieces has styled its first piece, so the raw
    // glTF materials never flash on screen.
    Some(commands.spawn((WorldAssetRoot(scene), at, Visibility::Hidden, inst)).id())
}

/// An effect texture image, sampled the way the program binds effect textures:
/// linear, repeating (scrolling textures wrap).
fn load_fx_image(assets: &AssetServer, path: &str) -> Handle<Image> {
    use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
    assets
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::Repeat, address_mode_v: ImageAddressMode::Repeat, ..ImageSamplerDescriptor::linear() });
        })
        .load(path.to_string())
}

/// Marks a named node attach_pieces has dealt with (effect piece or not).
#[derive(Component)]
pub struct FxSeen;

/// Tag each effect draw node once its scene and material have loaded, and
/// give it an additive material. Nodes whose material is still loading are
/// retried next frame.
pub fn attach_pieces(
    mut commands: Commands,
    named: Query<(Entity, &Name, &Transform, &Children), Without<FxSeen>>,
    parents: Query<&ChildOf>,
    instances: Query<&FxInstance>,
    mesh_mats: Query<&MeshMaterial3d<StandardMaterial>>,
    mats: Res<Assets<StandardMaterial>>,
    mut fx_mats: ResMut<Assets<FxMaterial>>,
    assets: Res<AssetServer>,
    mut cache: Option<ResMut<FxMatCache>>,
    mut rows: Option<ResMut<inst::FxInstances>>,
) {
    for (e, name, t, children) in &named {
        let Some((root, inst)) = parents.iter_ancestors(e).find_map(|a| instances.get(a).ok().map(|i| (a, i))) else {
            commands.entity(e).insert(FxSeen);
            continue;
        };
        // Scene/group nodes (e.g. "root") are left alone; pieces of other slots are hidden.
        let Some(piece) = inst.def.pieces.get(name.as_str()) else {
            commands.entity(e).insert(FxSeen);
            continue;
        };
        // On a moving projectile, pieces raised well above the model origin
        // (e.g. Laser's glow at the caster's chest height) don't belong on the shot;
        // its tail pieces trail straight behind at y = 0.
        let off_origin = inst.slot == SLOT_PROJECTILE && inst.looping && t.translation.y.abs() > PROJECTILE_PIECE_REACH;
        let hidden_distortion = piece.distort && distort_strength() <= 0.0;
        // `PD_FX_HIDE=m2_d0,m2_d1`: leave these draw nodes out (bisecting a look).
        let hidden_by_switch = env_once("PD_FX_HIDE").is_some_and(|v| v.split(',').any(|n| n == name.as_str())) || kind_off("piece") || (kind_off("haze") && piece.family == FAM_DIST);
        if !inst.def.slots[inst.slot].contains(&piece.model) || off_origin || hidden_distortion || hidden_by_switch {
            commands.entity(e).insert((FxSeen, Visibility::Hidden));
            if hidden_by_switch {
                // A bisect switch must not stop the clock of an effect that only has emitters left.
                commands.entity(root).insert((Visibility::Inherited, FxLive));
            }
            continue;
        }
        let Some((mesh, tex)) = children.iter().find_map(|c| mesh_mats.get(c).ok().map(|mm| (c, mats.get(&mm.0).map(|m| m.base_color_texture.clone())))) else {
            continue;
        };
        let Some(tex) = tex else {
            // Material still loading: keep it out of sight and retry.
            commands.entity(e).insert(Visibility::Hidden);
            continue;
        };
        // No texture = pixel format 8, the screen-distortion pieces' bump map, which the glb
        // does not carry. `PDEFTOPT03/05` pieces read it from their own exported image
        // (`FAM_DIST`); any other texture-less draw stays hidden.
        let tex = match (tex, piece.family) {
            (Some(t), _) => t,
            (None, FAM_DIST) => Handle::default(),
            (None, _) => {
                commands.entity(e).insert((FxSeen, Visibility::Hidden));
                continue;
            }
        };
        // `solid` pieces (PDEFTOPT06/YKREFR mine shells, sword blades) draw
        // opaque in the program ("alpha 1", FORMATS-EFFECTS.md "Shaders")
        // -- not our old double-sided `AlphaMode::Blend`, which is
        // order-dependent within one mesh's own front/back faces and was
        // part of the camera-dependent "dark blob" bug (VFX-AUDIT.md #1):
        // with `cull_mode: None` a convex mesh's back and front triangles
        // blend in whatever order Bevy happens to draw them, so the same
        // piece can look right from one split-screen camera and murky from
        // the other. `Opaque` + normal back-face culling draws a single,
        // camera-independent surface and needs no piece-to-piece sort.
        //
        // VFX-AUDIT.md #2 tried (2026-09-28, reverted same day): a piece
        // whose colour fades *to* opaque, on eft0043's own cast, was drawn
        // with this same alpha-kill treatment on the theory that it matched
        // `PDEFTBB01`'s disassembled behaviour and was the footage's missing
        // "green crescent". Orchestrator review of the resulting screenshots
        // caught the actual result: since the piece's *colour* (not just its
        // alpha) heads toward black as it becomes opaque, `Mask` drew it as
        // a real, solid, dark/black quad over the caster -- the exact
        // "dark blob" look VFX-AUDIT #1 fixed, reintroduced by this change.
        // Left as plain `Add` (below): under addition the same black-trending
        // colour can only ever go dim/invisible, never opaque-dark, so this
        // family is safe again, at the cost of not yet reproducing whatever
        // the footage's crescent actually is (`FORMATS-EFFECTS.md` "Open").
        // Per-piece blend (D1 of audits/P0-EFFECTS.md): the draw's own
        // equation goes on the pipeline (`FxGamma::specialize`); the alpha
        // mode only decides the pass. Blended pieces go to the sorted
        // transparent pass, pieces whose material has blending off draw with
        // an alpha test; a cache without the material words keeps the old
        // additive draw. Culling follows word 19 (`Piece::cull`).
        // A shader family samples its own texture slots (exported as PNGs); its
        // base colour then carries only the piece colour x vertex colour.
        let images: [Option<Handle<Image>>; 4] = std::array::from_fn(|k| piece.textures[k].as_ref().map(|p| load_fx_image(&assets, p)));
        // Animated every frame by `animate_pieces`: through a row of the shared per-instance buffer and a
        // shared, never rewritten material (`effects/inst.rs`), or (`PD_FX_INST=0`, every row taken) a
        // pooled material of its own (`mat_cache.rs`).
        let fresh = piece_fx_material(piece, tex, images);
        let (m, row) = inst::lend(rows.as_deref_mut(), cache.as_deref_mut(), &mut fx_mats, e, fresh, "piece");
        inst::tag(&mut commands, e, mesh, row);
        commands.entity(mesh).remove::<MeshMaterial3d<StandardMaterial>>().insert(MeshMaterial3d(m.clone()));
        commands.entity(e).insert((FxSeen, FxPiece { piece: piece.clone(), base: *t, material: m, instance: root, row }, Visibility::Hidden));
        if piece.wisp.is_some() && piece.skin.is_some() {
            let seed = (root.to_bits() as u32).wrapping_mul(0x9E37_79B1) ^ name.as_str().bytes().fold(17u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
            commands.entity(e).insert(FxWispPool::new(seed, inst.age.floor() as i64));
        } else if piece.wisp.is_some() {
            let seed = (root.to_bits() as u32).wrapping_mul(0x9E37_79B1) ^ name.as_str().bytes().fold(17u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
            commands.entity(e).insert(FxWisp { seed, last: inst.age.floor() as i64, active: false, spawned: 0, life: 0.0, grow: Vec3::ZERO, roll: 0.0, count: 0 });
        }
        commands.entity(root).insert((Visibility::Inherited, FxLive));
    }
}

/// A piece's material as `attach_pieces` makes it (`tex` = its glb texture, `images` = its shader
/// family's texture slots); also what `warmup.rs` warms.
fn piece_fx_material(piece: &Piece, tex: Handle<Image>, images: [Option<Handle<Image>>; 4]) -> FxMaterial {
    let base = StandardMaterial {
        base_color_texture: if piece.family == FAM_PLAIN { Some(tex) } else { None },
        unlit: true,
        alpha_mode: if piece.solid {
            AlphaMode::Opaque
        } else if piece.family == FAM_DIST || scene_blend(piece).is_some() {
            // Drawn in the transmissive pass, the only one that has the scene copy; its
            // own blend equation goes on the pipeline (`FxGamma::specialize`).
            AlphaMode::Mask(0.0)
        } else if !piece.blend_known {
            AlphaMode::Add
        } else if piece.blend.is_some() {
            AlphaMode::Blend
        } else {
            AlphaMode::Mask(0.5)
        },
        double_sided: !(piece.solid || piece.cull),
        cull_mode: if piece.solid || piece.cull { Some(Face::Back) } else { None },
        // Reading the view's scene copy (`reads_view_transmission_texture`) is what
        // `specular_transmission > 0` turns on; nothing here lights the surface.
        specular_transmission: if piece.family == FAM_DIST || scene_blend(piece).is_some() { 1.0 } else { 0.0 },
        ..default()
    };
    if piece.solid {
        fx_material(base)
    } else {
        piece_material(base, piece, images)
    }
}

/// An effect whose model has loaded and shows (or that needs no model): its
/// clock runs. Before that it waits, so an effect whose model is still
/// loading isn't over before it is seen (a first strike's swing was: the
/// model took longer to load than the swing lasts).
#[derive(Component)]
pub struct FxLive;

/// Longest an effect waits for its model (seconds) before its clock runs anyway.
const LOAD_WAIT: f32 = 1.0;

/// Advance effect clocks; finished one-shot effects despawn.
pub fn tick_effects(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut FxInstance, Has<FxLive>)>) {
    let fps = env_once("PD_FX_FPS").and_then(|v| v.parse().ok()).unwrap_or(FX_FPS);
    for (e, mut inst, live) in &mut q {
        if !live && inst.fixed.is_none() && inst.waited < LOAD_WAIT {
            inst.waited += time.delta_secs();
            continue;
        }
        inst.age += time.delta_secs() * fps;
        // The window's last frame is drawn too (`FUN_140188fd0`: removal once the frame
        // passes the duration), hence the whole extra frame.
        if inst.fixed.is_none() && !inst.looping && inst.age >= inst.len + 1.0 {
            commands.entity(e).despawn();
        }
    }
}

#[derive(Component)]
pub struct FxTrail {
    trail: Trail,
    material: Handle<FxMaterial>,
    instance: Entity,
    /// Its row in the per-instance buffer (`effects/inst.rs`); None = it colours its own material.
    row: Option<u32>,
}

/// The additive textured material of trails, blades, flares and light decals.
fn plain_add_material(texture: Handle<Image>) -> FxMaterial {
    fx_material(StandardMaterial { base_color_texture: Some(texture), unlit: true, alpha_mode: AlphaMode::Add, double_sided: true, cull_mode: None, ..default() })
}

/// Two crossed unit quads from the origin back to z = -1 (behind the travel
/// direction), v = 0 at the head and 1 at the tail.
fn trail_mesh() -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let pos = vec![[-0.5, 0.0, 0.0], [0.5, 0.0, 0.0], [0.5, 0.0, -1.0], [-0.5, 0.0, -1.0], [0.0, -0.5, 0.0], [0.0, 0.5, 0.0], [0.0, 0.5, -1.0], [0.0, -0.5, -1.0]];
    let uv = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let normal = vec![[0.0, 1.0, 0.0]; 4].into_iter().chain(vec![[1.0, 0.0, 0.0]; 4]).collect::<Vec<[f32; 3]>>();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normal)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]))
}

/// Give each new effect its trails (shader-drawn in the original).
pub fn spawn_trails(
    mut commands: Commands,
    assets: Res<AssetServer>,
    new: Query<(Entity, &FxInstance), Added<FxInstance>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<FxMaterial>>,
    mut mesh: Local<Option<Handle<Mesh>>>,
    mut images: ResMut<Assets<Image>>,
    mut light_mesh: Local<Option<Handle<Mesh>>>,
    mut light_texture: Local<Option<Handle<Image>>>,
    existing_lights: Query<(), With<FxLight>>,
    mut cache: Option<ResMut<FxMatCache>>,
    mut rows: Option<ResMut<inst::FxInstances>>,
) {
    // Cheap quads, but capped per the brief in case of a multi-caster pile-up.
    let mut light_budget = MAX_LIGHT_DECALS.saturating_sub(existing_lights.iter().count());
    for (root, inst) in &new {
        for t in inst.def.trails.iter().filter(|t| inst.def.slots[inst.slot].contains(&t.model) && !kind_off("trail")) {
            let mesh = mesh.get_or_insert_with(|| meshes.add(trail_mesh())).clone();
            // Coloured by `animate_trails`: a row of the shared buffer (or a pooled material).
            let e = commands.spawn((Mesh3d(mesh), Transform::default(), Visibility::Hidden)).id();
            let (material, row) = inst::lend(rows.as_deref_mut(), cache.as_deref_mut(), &mut mats, e, plain_add_material(assets.load(t.texture.clone())), "trail");
            inst::tag(&mut commands, e, e, row);
            commands.entity(e).insert((MeshMaterial3d(material.clone()), FxTrail { trail: t.clone(), material, instance: root, row }));
            commands.entity(root).add_child(e);
        }
        // Sprite emitters: the shot's body on a flight effect (Swarm of
        // Moths, Hell's Fire, the crawlers), but also cast/hit/impact bursts
        // (2026-09-27, bladefx lane) -- Laser, Blaster and every blade skill
        // author one of these into their slot-0 cast model too (e.g. Psycho
        // Knife's `a07ac685` sprite emitter, frames 12-22), which a
        // `looping`-only gate here hid entirely regardless of timing. A
        // one-shot slot needs no special-casing: `tick_sprites` already only
        // emits between the emitter's own start/end frames, and the instance
        // despawns once its age passes the slot's length.
        for (k, s) in inst.def.sprites.iter().enumerate().filter(|(_, s)| inst.def.slots[inst.slot].contains(&s.model) && !kind_off(if s.ms.is_some() { "ms" } else if s.points.is_some() { "points" } else { "sprite" })) {
            let e = commands.spawn((Transform::default(), Visibility::Inherited, FxEmitter { sprite: Arc::new(s.clone()), instance: root, next: inst.age, texture: assets.load(s.texture.clone()), seed: (root.to_bits() as u32).wrapping_mul(31).wrapping_add(k as u32), count: 0, armed: false, alpha: None, bone: None, bound: false, ms_slot: Mat4::IDENTITY })).id();
            commands.entity(root).add_child(e);
        }
        // Sword ribbons: world-space meshes (not under the root, which moves
        // with the caster; the ribbon stays where the blade passed).
        for b in inst.def.blades.iter().filter(|b| inst.def.slots[inst.slot].contains(&b.model) && !root_only() && !kind_off("blade")) {
            // Never empty: a zero-length vertex buffer upsets the render allocator.
            let mesh = meshes.add(blade_mesh(&[(Vec3::ZERO, Vec3::ZERO); 2], 1));
            // READ (`FUN_14019ec80`): the ribbon sets blend 0x5401 (SRC_ALPHA, 1 - SRC_ALPHA) and
            // `Blade01P_ps` outputs `(colour texture + refracted scene) x vertex colour` with the vertex
            // colour's alpha: with the white colour the data authors that equals scene + alpha x texture, i.e.
            // the additive draw used here (the scene refraction by the second texture is not reproduced).
            // Coloured by `update_blades`: a row of the shared buffer (or a pooled material).
            let e = commands.spawn_empty().id();
            let (material, row) = inst::lend(rows.as_deref_mut(), cache.as_deref_mut(), &mut mats, e, plain_add_material(assets.load(b.texture.clone())), "blade");
            inst::tag(&mut commands, e, e, row);
            commands.entity(e).insert((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::IDENTITY,
                Visibility::Hidden,
                // The mesh changes every frame: keep it out of frustum culling.
                bevy::camera::visibility::NoFrustumCulling,
                FxBlade { blade: Arc::new(b.clone()), instance: root, bone: None, bound: false, samples: Vec::new(), last: -1.0, dirty: false, mesh, material, row },
            ));
        }
        // Ray bursts: world-space too (the rays stay where they flashed).
        for (k, f) in inst.def.flares.iter().enumerate().filter(|(_, f)| inst.def.slots[inst.slot].contains(&f.model) && !root_only() && !kind_off("flare")) {
            let mesh = meshes.add(flare_mesh(f, &[]));
            // Never written after spawn (the rays' colours are vertex colours): shared.
            let material = mat_cache::shared(cache.as_deref_mut(), &mut mats, plain_add_material(assets.load(f.texture.clone())));
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material),
                Transform::IDENTITY,
                Visibility::Hidden,
                bevy::camera::visibility::NoFrustumCulling,
                FxFlare { flare: Arc::new(f.clone()), instance: root, bone: None, bound: false, rays: Vec::new(), last: -1.0, seen: inst.age, seed: (root.to_bits() as u32).wrapping_mul(131).wrapping_add(k as u32), stale: false, mesh },
            ));
        }
        // Floor light decals (LIGHT02): world-space, flat on the ground, not
        // under the root (which sits at chest/prop height -- see `FxLight`).
        // Its own texture/mesh (not `ParticleMeshes`/`setup_particles`, which
        // `fx.rs`'s own `plugin()` sets up but `main.rs` never registers).
        if light_enabled() && !inst.def.lights.is_empty() {
            for l in inst.def.lights.iter().filter(|l| inst.def.slots[inst.slot].contains(&l.model)) {
                if light_budget == 0 {
                    break;
                }
                light_budget -= 1;
                let mesh = light_mesh.get_or_insert_with(|| meshes.add(quads_mesh(&[XZ], FULL_UV))).clone();
                let texture = light_texture.get_or_insert_with(|| images.add(soft_puff())).clone();
                // Coloured by `update_lights`: a pooled material of its own, never a row of the shared buffer --
                // the stage and the fighters read the light's colour from this material
                // (`stage_material::update_effect_light`, `char_light::gather_effect_lights`), and the disc
                // itself is not drawn by default, so a row would save nothing.
                let le = commands.spawn_empty().id();
                let (material, row) = (mat_cache::pooled(cache.as_deref_mut(), &mut mats, plain_add_material(texture)), None);
                // The flat additive floor disc is a stand-in the original does not have
                // (DECISIONS 2026-10-01 "floor light pool dropped"; the program lights the
                // stage per pixel, `stage_material::update_effect_light`). The entity stays as
                // the light's data carrier for the stage and the fighters; the disc is only
                // drawn with `PD_LIGHT_DECAL=1`.
                let mut ent = commands.entity(le);
                ent.insert((MeshMaterial3d(material.clone()), Transform::IDENTITY, Visibility::Hidden, FxLight { light: l.clone(), instance: root, material, row }));
                if env_once("PD_LIGHT_DECAL").is_some_and(|v| v == "1") {
                    ent.insert(Mesh3d(mesh));
                }
            }
        }
    }
}

/// A sprite emitter on a playing effect (child of its root).
#[derive(Component)]
pub struct FxEmitter {
    /// Shared, so the per-frame copy `tick_sprites` takes is a counter bump, not a deep copy.
    sprite: Arc<Sprite>,
    instance: Entity,
    /// Effect age (frames) of the next sprite.
    next: f32,
    texture: Handle<Image>,
    seed: u32,
    count: u32,
    /// False until its root's world transform has been computed once.
    armed: bool,
    /// Whether its texture has transparency (known once it has loaded;
    /// until then it waits): puffs with one are blended, the rest added.
    alpha: Option<bool>,
    /// The bone it rides on (`bind_bones`; None = the effect root) and
    /// whether that has been decided yet.
    bone: Option<Entity>,
    bound: bool,
    /// MSPARTICLE: the matrix its attached particles are drawn through (the
    /// emitter's matrix at its latest spawn, `ms_particle.rs`).
    ms_slot: Mat4,
}

/// One sprite: lives in the world (left behind as the shot moves on), kept
/// as a child of the effect root so it goes when the effect does.
#[derive(Component)]
pub struct FxSpark {
    at: Vec3,
    vel: Vec3,
    /// Units / s² (PARTICLE01/02's words 8-10).
    acc: Vec3,
    /// Frames.
    age: f32,
    life: f32,
    size: [f32; 2],
    colors: [u32; 2],
    grid: [f32; 2],
    material: Handle<FxMaterial>,
    instance: Entity,
    row: Option<u32>,
}

/// The single camera-facing quad the PARTICLE01/02 points are drawn on.
static POINT_QUAD: std::sync::Mutex<Option<Handle<Mesh>>> = std::sync::Mutex::new(None);

fn point_billboard() -> bool {
    env_once("PD_POINT_BILLBOARD").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0) > 0.0
}

/// PARTICLE01/02 (`Sprite::points`): `count` points on each effect frame
/// of the window (inclusive, as the program's `start <= frame <= end`), born
/// at the emitter, flying out in a random direction (`FUN_1401a2050`, mode 0;
/// modes 1 and 2, `FUN_1401a2bc0` / `FUN_1401a34a0`, are drawn the same way ⚠️).
#[allow(clippy::too_many_arguments)]
fn emit_points(
    commands: &mut Commands,
    em: &mut FxEmitter,
    inst: &FxInstance,
    root: &GlobalTransform,
    origin: Vec3,
    s: &Sprite,
    pts: Points,
    fps: f32,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<FxMaterial>,
    mesh: &mut Option<Handle<Mesh>>,
    mut cache: Option<&mut FxMatCache>,
    mut rows: Option<&mut inst::FxInstances>,
) {
    // Drawn frames per second.
    let rate = STEPS_PER_FRAME * fps;
    em.next = em.next.max((inst.age - 2.0).floor());
    while em.next <= inst.age {
        let frame = if inst.looping { em.next % inst.len } else { em.next };
        em.next = em.next.floor() + 1.0;
        if frame < s.start || frame > s.end {
            continue;
        }
        for _ in 0..pts.count {
            em.count += 1;
            let (seed, i) = (em.seed, em.count);
            let dir = cone_dir(seed, i * 5, 180.0);
            let speed = pts.speed[0] - hash01(seed, i * 5 + 3) * pts.speed[1];
            let size = if pts.random_size { let z = s.size[0] + hash01(seed, i * 5 + 4) * s.size[1]; [z, z] } else { s.size };
            let c = argb(s.colors[0]);
            // READ: PARTICLE01/02 are D3D point sprites (`Part1V_vs` expands each point to a quad from
            // `quad_pos[vertex_id]` in clip space), so every particle is ONE screen-facing quad. Ours were
            // three crossed planes, which from the side read as squares / plus signs
            // (`PD_POINT_BILLBOARD=0` restores them for A/B). The billboard is rebuilt per camera in
            // `fx_gamma.wgsl` (piece.x = 1), so each split-screen view faces its own camera.
            let billboard = point_billboard();
            // Coloured (and its cell picked) by `tick_sprites` every frame: a row of the shared buffer
            // (or a pooled material).
            let pe = commands.spawn_empty().id();
            let (material, row) = inst::lend(rows.as_deref_mut(), cache.as_deref_mut(), mats, pe, point_material(c, em.texture.clone(), billboard), "point");
            inst::tag(commands, pe, pe, row);
            let m = if billboard {
                let mut slot = POINT_QUAD.lock().unwrap_or_else(|e| e.into_inner());
                slot.get_or_insert_with(|| meshes.add(quads_mesh(&[XY], FULL_UV))).clone()
            } else {
                mesh.get_or_insert_with(|| meshes.add(quads_mesh(&[XY, ZY, XZ], FULL_UV))).clone()
            };
            let local = root.affine().inverse().transform_point3(origin);
            // Left-handed words -> ours: z flips.
            let acc = Vec3::new(pts.acc.x, pts.acc.y, -pts.acc.z) * rate * rate;
            let e = commands
                .entity(pe)
                .insert((
                    Mesh3d(m),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(local).with_scale(Vec3::splat(size[0].max(0.05))),
                    Visibility::Inherited,
                    FxSpark { at: origin, vel: dir * speed * rate, acc, age: 0.0, life: s.life, size, colors: s.colors, grid: s.grid, material, instance: em.instance, row },
                ))
                .id();
            commands.entity(em.instance).add_child(e);
        }
    }
}

/// A PARTICLE01/02 point's material (`emit_points`; `c` = its first colour).
fn point_material(c: Vec4, texture: Handle<Image>, billboard: bool) -> FxMaterial {
    let mut material = fx_material(StandardMaterial {
        base_color: Color::srgba(c.x, c.y, c.z, c.w),
        base_color_texture: Some(texture),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    if billboard {
        material.extension.piece = Vec4::new(1.0, 0.0, 0.0, 1.0);
    }
    material
}

/// A sprite puff's material (`tick_sprites`; `c` = its first colour).
fn puff_material(c: Vec4, texture: Handle<Image>, translucent: bool) -> FxMaterial {
    fx_material_gamma(
        StandardMaterial {
            base_color: Color::srgba(c.x, c.y, c.z, c.w),
            base_color_texture: Some(texture),
            unlit: true,
            alpha_mode: if translucent { AlphaMode::Mask(0.5) } else { AlphaMode::Add },
            double_sided: !translucent,
            cull_mode: if translucent { Some(Face::Back) } else { None },
            ..default()
        },
        1.0,
    )
}

/// Whether a puff is drawn as the translucent cut-out (alpha-tested, depth-written)
/// rather than added. A puff whose own material adds (dest factor ONE) is a glow:
/// cutting its dark texture out at alpha 0.5 and writing depth made an opaque
/// stepped slab of Bomb's x6 blast (its `0x1400` puffs, texture 3 = a dark red
/// disc with a bright core), where the program just adds the colour.
fn puff_is_translucent(s: &Sprite, texture_has_alpha: Option<bool>) -> bool {
    s.blend && !s.additive && texture_has_alpha == Some(true)
}

/// Emit and move sprites (see `FxEmitter`). World positions are turned into
/// the root's local space each frame so the sprites stay where they were born.
#[allow(clippy::too_many_arguments)]
fn tick_sprites(
    commands: &mut Commands,
    dt: f32,
    fps: f32,
    instances: &Query<&FxInstance>,
    globals: &Query<&GlobalTransform>,
    emitters: &mut Query<(Entity, &mut FxEmitter)>,
    sparks: &mut Query<(Entity, &mut FxSpark, &mut Transform), Without<FxTrail>>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<FxMaterial>,
    images: &Assets<Image>,
    mesh: &mut Option<Handle<Mesh>>,
    mut cache: Option<&mut FxMatCache>,
    mut rows: Option<&mut inst::FxInstances>,
) {
    for (em_entity, mut em) in emitters.iter_mut() {
        if !em.bound {
            continue;
        }
        let (Ok(inst), Ok(root), Ok(from)) = (instances.get(em.instance), globals.get(em.instance), globals.get(em.bone.unwrap_or(em.instance))) else { continue };
        // Where sprites are born: the emitter's joint on its bone (or on the root).
        let origin = from.transform_point(em.sprite.offset);
        if em.alpha.is_none() {
            let Some(img) = images.get(&em.texture) else { continue };
            em.alpha = Some(img.data.as_ref().is_some_and(|d| d.chunks_exact(4).any(|p| p[3] < 250)));
        }
        if !em.armed {
            em.armed = true;
            continue;
        }
        let s = em.sprite.clone();
        if let Some(ms) = &s.ms {
            let m = Mat4::from(from.affine()) * ms.joint_m;
            ms_particle::emit(commands, &mut em, em_entity, inst, root, ms, m, meshes, mats, mesh, cache.as_deref_mut(), rows.as_deref_mut());
            continue;
        }
        if let Some(pts) = s.points {
            emit_points(commands, &mut em, inst, root, origin, &s, pts, fps, meshes, mats, mesh, cache.as_deref_mut(), rows.as_deref_mut());
            continue;
        }
        // Don't flood after a stall.
        em.next = em.next.max(inst.age - 4.0 * SPRITE_EVERY);
        while em.next <= inst.age {
            let frame = if inst.looping { em.next % inst.len } else { em.next };
            em.next += SPRITE_EVERY;
            if frame < s.start || frame >= s.end {
                continue;
            }
            em.count += 1;
            let (seed, i) = (em.seed, em.count);
            let jitter = (Vec3::new(hash01(seed, i * 3), hash01(seed, i * 3 + 1), hash01(seed, i * 3 + 2)) * 2.0 - 1.0) * s.size[0] * 0.3;
            let dir = cone_dir(seed, i + 7, 180.0);
            let vel = dir * s.drift * fps + Vec3::Y * s.drift * fps * 0.5;
            let c = argb(s.colors[0]);
            // MSPARTICLE puffs keep the old blending: their sizes and counts
            // are still an unconfirmed reading (the program's `MSParticleFunc`,
            // `FUN_14010f710`, differs from PARTICLE01's), and at full
            // brightness Psycho Knife's hit puffs read as a big white cloud the
            // footage doesn't have.
            // Puffs are drawn on a 3-crossed-plane "volumetric sprite" mesh
            // (`[XY, ZY, XZ]`, 6 faces once double-sided): with the old
            // `AlphaMode::Blend` + `cull_mode: None` the six overlapping
            // faces have no fixed draw order, so the same puff blended
            // differently depending on which split-screen camera looked at
            // it -- Laser/Slide Laser's cast charge "dark blob from one
            // camera, fine from the other" (VFX-AUDIT.md #1). `Mask` draws
            // in the opaque pass (per-pixel depth test/write, not sorted),
            // so the six faces resolve the same way from any camera.
            let translucent = puff_is_translucent(&s, em.alpha);
            // Coloured (and its cell picked) below every frame: a row of the shared buffer (or a pooled material).
            let pe = commands.spawn_empty().id();
            let (material, row) = inst::lend(rows.as_deref_mut(), cache.as_deref_mut(), mats, pe, puff_material(c, em.texture.clone(), translucent), "puff");
            inst::tag(commands, pe, pe, row);
            let m = mesh.get_or_insert_with(|| meshes.add(quads_mesh(&[XY, ZY, XZ], FULL_UV))).clone();
            // Puffs sit on the ground rather than half inside it (crawlers run along the floor).
            // (Only at the effect's own origin: one on a bone is born there.)
            let lift = if s.blend && em.bone.is_none() { s.size[0] * 0.4 } else { 0.0 };
            let at = origin + jitter + Vec3::Y * lift;
            let local = root.affine().inverse().transform_point3(at);
            let e = commands
                .entity(pe)
                .insert((
                    Mesh3d(m),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(local).with_scale(Vec3::splat(s.size[0])),
                    Visibility::Inherited,
                    FxSpark { at, vel, acc: Vec3::ZERO, age: 0.0, life: s.life, size: s.size, colors: s.colors, grid: s.grid, material, instance: em.instance, row },
                ))
                .id();
            commands.entity(em.instance).add_child(e);
        }
    }
    for (e, mut sp, mut t) in sparks.iter_mut() {
        sp.age += dt * fps;
        let Ok(root) = globals.get(sp.instance) else { continue };
        if sp.age >= sp.life {
            commands.entity(e).despawn();
            continue;
        }
        let acc = sp.acc;
        sp.vel += acc * dt;
        let step = sp.vel * dt;
        sp.at += step;
        let k = sp.age / sp.life;
        let inv = root.affine().inverse();
        t.translation = inv.transform_point3(sp.at);
        // Keep the sprite upright in the world, whatever way the shot faces.
        t.rotation = root.to_scale_rotation_translation().1.inverse();
        t.scale = Vec3::splat((sp.size[0] + (sp.size[1] - sp.size[0]) * k).max(0.05));
        let c = argb(sp.colors[0]).lerp(argb(sp.colors[1]), k);
        let col = Color::srgba(c.x, c.y, c.z, c.w);
        let (cols, nrows) = (sp.grid[0], sp.grid[1]);
        let uv = (cols * nrows > 1.0).then(|| {
            let cell = (k * cols * nrows).floor().min(cols * nrows - 1.0);
            bevy::math::Affine2::from_scale_angle_translation(Vec2::new(1.0 / cols, 1.0 / nrows), 0.0, Vec2::new((cell % cols) / cols, (cell / cols).floor() / nrows))
        });
        if let (Some(r), Some(rows)) = (sp.row, rows.as_deref_mut()) {
            // A row of the shared buffer: no material is rewritten (they used to be, every frame).
            rows.set_color(r, col);
            if let Some(t) = uv {
                rows.set_uv(r, t);
            }
        } else if let Some(mut m) = mats.get_mut(&sp.material) {
            let m = &mut m.base;
            m.base_color = col;
            if let Some(t) = uv {
                m.uv_transform = t;
            }
        }
    }
}

/// Trails, plus the sprite emitters (`tick_sprites`).
#[allow(clippy::too_many_arguments)]
pub fn animate_trails(
    mut commands: Commands,
    time: Res<Time>,
    instances: Query<&FxInstance>,
    globals: Query<&GlobalTransform>,
    mut q: Query<(&FxTrail, &mut Transform, &mut Visibility), Without<FxSpark>>,
    mut emitters: Query<(Entity, &mut FxEmitter)>,
    mut sparks: Query<(Entity, &mut FxSpark, &mut Transform), Without<FxTrail>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<FxMaterial>>,
    images: Res<Assets<Image>>,
    mut spark_mesh: Local<Option<Handle<Mesh>>>,
    mut cache: Option<ResMut<FxMatCache>>,
    mut rows: Option<ResMut<inst::FxInstances>>,
) {
    let fps = env_once("PD_FX_FPS").and_then(|v| v.parse().ok()).unwrap_or(FX_FPS);
    tick_sprites(&mut commands, time.delta_secs(), fps, &instances, &globals, &mut emitters, &mut sparks, &mut meshes, &mut mats, &images, &mut spark_mesh, cache.as_deref_mut(), rows.as_deref_mut());
    if env_once("PD_FX_LOG").is_some() && sparks.iter().count() > 0 {
        eprintln!("[fxlog] sparks {} emitters {}", sparks.iter().count(), emitters.iter().count());
    }
    for (fx, mut t, mut vis) in &mut q {
        let Ok(inst) = instances.get(fx.instance) else { continue };
        let frame = inst.fixed.unwrap_or(if inst.looping { inst.age % inst.len } else { inst.age });
        let tr = &fx.trail;
        let on = frame >= tr.start && frame < tr.end;
        *vis = if on { Visibility::Inherited } else { Visibility::Hidden };
        if !on {
            continue;
        }
        let grown = if inst.fixed.is_some() { 1.0 } else { (inst.age / TRAIL_GROW_FRAMES).min(1.0) };
        t.scale = Vec3::new(tr.width, tr.width, tr.length * grown);
        let k = ((frame - tr.start) / (tr.end - tr.start).max(1.0)).clamp(0.0, 1.0);
        let c = argb(tr.colors[0]).lerp(argb(tr.colors[1]), k);
        let col = Color::srgba(c.x, c.y, c.z, c.w);
        if let (Some(r), Some(rows)) = (fx.row, rows.as_deref_mut()) {
            rows.set_color(r, col);
        } else if let Some(mut m) = mats.get_mut(&fx.material) {
            if m.base.base_color != col {
                m.base.base_color = col;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Particles: the stage files' shader-drawn emitters (debris chips, smoke,
// sparks) and flying fragment models, used by arena-object effects
// (fx.rs, docs/FORMATS-STAGES.md "Object effects"). Render-only.

/// How a particle is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// One flat quad tumbling (a debris chip).
    Chip,
    /// Three crossed quads (reads as a soft ball from any side: smoke).
    Puff,
    /// Crossed quads stretched along the flight (a spark streak).
    Streak,
}

/// One burst of particles.
#[derive(Clone, Debug)]
pub struct BurstSpec {
    pub shape: Shape,
    pub texture: Option<Handle<Image>>,
    /// Chips: use one quadrant of the texture (the stage debris atlases hold
    /// a few chip sprites side by side; the top two quadrants are chips).
    pub quadrants: bool,
    pub count: u32,
    /// Seconds each particle lives.
    pub life: f32,
    /// Spread over this many seconds (smoke keeps puffing).
    pub emit_over: f32,
    /// Launch speed (units/s), cone half-angle around +Y (degrees), extra
    /// upward speed, gravity (units/sÂ², down), constant rise (smoke).
    pub speed: f32,
    pub spread_deg: f32,
    pub up: f32,
    pub gravity: f32,
    pub rise: f32,
    /// Size (units) at birth and at death.
    pub size: [f32; 2],
    /// Linear RGBA at birth and at death.
    pub colour: [Vec4; 2],
    pub additive: bool,
    /// Spin, radians/s.
    pub spin: f32,
    /// Birth positions are scattered this far (units) around the origin.
    pub jitter: f32,
}

#[derive(Component)]
pub struct Particle {
    vel: Vec3,
    gravity: f32,
    rise: f32,
    /// Seconds; negative = not born yet.
    age: f32,
    life: f32,
    size: [f32; 2],
    colour: [Vec4; 2],
    spin_axis: Vec3,
    spin: f32,
    shape: Shape,
    material: Handle<FxMaterial>,
    /// Its row in the shared per-instance buffer (`inst.rs`), if it got one.
    row: Option<u32>,
}

/// What particles draw with (overnight perf 2026-10-09). A particle used to own a
/// `StandardMaterial` that `tick_particles` rewrote every frame as its colour faded (~13 material
/// re-preparations a frame in a four-player fight: footstep puffs, prop debris, capsule sparkles).
/// Now it draws with an `FxMaterial` in the "blended like any StandardMaterial" mode
/// (`fx_material_gamma(.., 1.0)`: `fx_gamma.wgsl` mode 0 without the gamma re-encoding is Bevy's
/// unlit `StandardMaterial` fragment, base colour x texture, then the usual post-lighting
/// processing), shared by every particle with the same static look, with its colour in a row of
/// the shared buffer. No free row (or `PD_FX_INST=0`, `PD_FX_INST_SKIP=particle`): a material of
/// its own, as before.
#[derive(bevy::ecs::system::SystemParam)]
pub struct PartMats<'w> {
    mats: ResMut<'w, Assets<FxMaterial>>,
    rows: Option<ResMut<'w, inst::FxInstances>>,
    cache: Option<ResMut<'w, FxMatCache>>,
}

/// A particle's material recipe: `base` blended exactly as a `StandardMaterial` would be.
fn particle_material(base: StandardMaterial) -> FxMaterial {
    fx_material_gamma(base, 1.0)
}

/// The material and row for particle `owner` (spawned without a material): the shared one of its
/// static look plus a row holding `base`'s colour, or (no row) a material of its own.
fn particle_look(commands: &mut Commands, pm: &mut PartMats, owner: Entity, base: StandardMaterial) -> (Handle<FxMaterial>, Option<u32>) {
    let fresh = particle_material(base);
    let skipped = env_once("PD_FX_INST_SKIP").is_some_and(|v| v.split(',').any(|k| k.trim() == "particle"));
    let row = pm.rows.as_deref_mut().filter(|_| inst::enabled() && !skipped).and_then(|r| r.take(owner, inst::FxInst::of(&fresh)));
    let material = match row {
        Some(_) => mat_cache::shared(pm.cache.as_deref_mut(), &mut pm.mats, inst::neutral(fresh)),
        None => pm.mats.add(fresh),
    };
    commands.entity(owner).insert(MeshMaterial3d(material.clone()));
    inst::tag(commands, owner, owner, row);
    (material, row)
}

/// Shared meshes for particles (built once).
#[derive(Resource)]
pub struct ParticleMeshes {
    quad: Handle<Mesh>,
    puff: Handle<Mesh>,
    streak: Handle<Mesh>,
    /// Quadrant UV variants of the quad (top-left, top-right).
    quad_cells: [Handle<Mesh>; 2],
    /// A soft round puff, for smoke (the original draws it in a shader).
    pub soft: Handle<Image>,
}

fn quads_mesh(quads: &[[[f32; 3]; 4]], uv: [[f32; 2]; 4]) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let mut pos = Vec::new();
    let mut uvs = Vec::new();
    let mut idx = Vec::new();
    for q in quads {
        let b = pos.len() as u32;
        pos.extend_from_slice(q);
        uvs.extend_from_slice(&uv);
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    let normal = vec![[0.0, 1.0, 0.0]; pos.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normal)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(idx))
}

const FULL_UV: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
const XY: [[f32; 3]; 4] = [[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]];
const ZY: [[f32; 3]; 4] = [[0.0, -0.5, -0.5], [0.0, -0.5, 0.5], [0.0, 0.5, 0.5], [0.0, 0.5, -0.5]];
const XZ: [[f32; 3]; 4] = [[-0.5, 0.0, -0.5], [0.5, 0.0, -0.5], [0.5, 0.0, 0.5], [-0.5, 0.0, 0.5]];

/// A soft radial puff with a little noise, white with alpha.
fn soft_puff() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: usize = 64;
    let mut px = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let (u, v) = ((x as f32 + 0.5) / N as f32 * 2.0 - 1.0, (y as f32 + 0.5) / N as f32 * 2.0 - 1.0);
            let r = (u * u + v * v).sqrt();
            // Cheap value noise so the puff isn't a perfect disc.
            let n = ((x as f32 * 12.9898 + y as f32 * 78.233).sin() * 43758.547).fract().abs();
            let a = ((1.0 - r).clamp(0.0, 1.0)).powf(1.6) * (0.8 + 0.2 * n);
            px.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

pub fn setup_particles(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>) {
    let cell = |c: usize| {
        let (u0, u1) = (c as f32 * 0.5, c as f32 * 0.5 + 0.5);
        [[u0, 0.5], [u1, 0.5], [u1, 0.0], [u0, 0.0]]
    };
    // Streak: long along +Z (the flight direction), head at +0.5.
    let streak = [
        [[-0.5, 0.0, -0.5], [0.5, 0.0, -0.5], [0.5, 0.0, 0.5], [-0.5, 0.0, 0.5]],
        [[0.0, -0.5, -0.5], [0.0, 0.5, -0.5], [0.0, 0.5, 0.5], [0.0, -0.5, 0.5]],
    ];
    commands.insert_resource(ParticleMeshes {
        quad: meshes.add(quads_mesh(&[XY], FULL_UV)),
        puff: meshes.add(quads_mesh(&[XY, ZY, XZ], FULL_UV)),
        streak: meshes.add(quads_mesh(&streak, [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]])),
        quad_cells: [meshes.add(quads_mesh(&[XY], cell(0))), meshes.add(quads_mesh(&[XY], cell(1)))],
        soft: images.add(soft_puff()),
    });
}

/// Small deterministic hash -> [0, 1) (render-only variety).
pub fn hash01(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFF_FFFF) as f32 / 16_777_216.0
}

/// A direction inside a cone of `half_deg` around +Y.
fn cone_dir(seed: u32, i: u32, half_deg: f32) -> Vec3 {
    let cos_max = half_deg.clamp(0.0, 180.0).to_radians().cos();
    let c = 1.0 - hash01(seed, i * 3) * (1.0 - cos_max);
    let s = (1.0 - c * c).max(0.0).sqrt();
    let a = hash01(seed, i * 3 + 1) * std::f32::consts::TAU;
    Vec3::new(s * a.cos(), c, s * a.sin())
}

/// Spawn a burst at `at` (world). Returns the particle entities.
pub fn spawn_burst(commands: &mut Commands, pm: &ParticleMeshes, mats: &mut PartMats, spec: &BurstSpec, at: Vec3, seed: u32) -> Vec<Entity> {
    let mut out = Vec::new();
    for i in 0..spec.count {
        let dir = cone_dir(seed, i, spec.spread_deg);
        let speed = spec.speed * (0.6 + 0.4 * hash01(seed, i * 7 + 2));
        let vel = dir * speed + Vec3::Y * spec.up;
        let jitter = (Vec3::new(hash01(seed, i * 5 + 3), hash01(seed, i * 5 + 4), hash01(seed, i * 5 + 5)) * 2.0 - 1.0) * spec.jitter;
        let delay = if spec.count > 1 { spec.emit_over * i as f32 / (spec.count - 1) as f32 } else { 0.0 };
        let texture = spec.texture.clone().or_else(|| (spec.shape == Shape::Puff).then(|| pm.soft.clone()));
        let c = spec.colour[0];
        let base = StandardMaterial {
            base_color: Color::linear_rgba(c.x, c.y, c.z, c.w),
            base_color_texture: texture,
            unlit: true,
            alpha_mode: if spec.additive { AlphaMode::Add } else { AlphaMode::Blend },
            double_sided: true,
            cull_mode: None,
            ..default()
        };
        let mesh = match spec.shape {
            Shape::Chip if spec.quadrants => pm.quad_cells[(hash01(seed, i * 11 + 6) * 2.0) as usize % 2].clone(),
            Shape::Chip => pm.quad.clone(),
            Shape::Puff => pm.puff.clone(),
            Shape::Streak => pm.streak.clone(),
        };
        let spin_axis = (Vec3::new(hash01(seed, i * 13 + 7), hash01(seed, i * 13 + 8), hash01(seed, i * 13 + 9)) * 2.0 - 1.0).normalize_or(Vec3::Y);
        let rot = Quat::from_axis_angle(spin_axis, hash01(seed, i * 17) * std::f32::consts::TAU);
        let e = commands.spawn((Mesh3d(mesh), Transform::from_translation(at + jitter).with_rotation(rot).with_scale(Vec3::splat(spec.size[0].max(0.01))), Visibility::Hidden)).id();
        let (material, row) = particle_look(commands, mats, e, base);
        commands.entity(e).insert(Particle { vel, gravity: spec.gravity, rise: spec.rise, age: -delay, life: spec.life.max(0.05), size: spec.size, colour: spec.colour, spin_axis, spin: spec.spin, shape: spec.shape, material, row });
        out.push(e);
    }
    out
}

/// Spawn a burst from the game's own recorded point-sprite data (position +
/// colour, `fx.rs::load_points`'s `.points.json` sidecars) instead of
/// `spawn_burst`'s procedural cone -- for the capsule materialise sparkles,
/// whose real shape (a gathering swirl, not an even circular spray) only the
/// actual ~1,700-point data carries (CAPSULE-VISUALS.md "Sparkles, pickup
/// and erase bursts"). `points` are centred on the orb's own origin; `max`
/// caps how many are instanced, evenly strided across the whole recording
/// (not just its first `max`) so the sampled subset still reads as the same
/// shape. Reuses `Particle`/`tick_particles` for fade-in/out and drift.
#[allow(clippy::too_many_arguments)]
pub fn spawn_point_burst(commands: &mut Commands, pm: &ParticleMeshes, mats: &mut PartMats, points: &[(Vec3, Color)], at: Vec3, life: f32, size: f32, texture: Option<Handle<Image>>, seed: u32, max: usize) -> Vec<Entity> {
    let mut out = Vec::new();
    if points.is_empty() || max == 0 {
        return out;
    }
    let stride = (points.len() / max).max(1);
    for (i, &(pos, colour)) in points.iter().step_by(stride).enumerate() {
        let lin = colour.to_linear();
        let base = StandardMaterial {
            base_color: colour,
            base_color_texture: texture.clone().or_else(|| Some(pm.soft.clone())),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            double_sided: true,
            cull_mode: None,
            ..default()
        };
        let i = i as u32;
        // A little outward drift per point, not the wide cone `spawn_burst`
        // uses -- these already have their own real spread from the source
        // data, they just need to visibly gather/settle rather than sit dead still.
        let drift = (Vec3::new(hash01(seed, i * 5 + 3), hash01(seed, i * 5 + 4), hash01(seed, i * 5 + 5)) * 2.0 - 1.0) * 0.6;
        let delay = hash01(seed, i * 9 + 1) * life * 0.4;
        let e = commands.spawn((Mesh3d(pm.quad.clone()), Transform::from_translation(at + pos).with_scale(Vec3::splat(size.max(0.01))), Visibility::Hidden)).id();
        let (material, row) = particle_look(commands, mats, e, base);
        commands.entity(e).insert(Particle {
                    vel: drift,
                    gravity: 0.0,
                    rise: 0.0,
                    age: -delay,
                    life: life.max(0.05),
                    size: [size, size * 0.15],
                    colour: [Vec4::new(lin.red, lin.green, lin.blue, lin.alpha), Vec4::new(lin.red, lin.green, lin.blue, 0.0)],
                    spin_axis: Vec3::Y,
                    spin: 0.0,
                    shape: Shape::Chip,
                    material,
                    row,
                });
        out.push(e);
    }
    out
}

pub fn tick_particles(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Particle, &mut Transform, &mut Visibility)>, mut mats: ResMut<Assets<FxMaterial>>, mut rows: Option<ResMut<inst::FxInstances>>) {
    let dt = time.delta_secs();
    for (e, mut p, mut t, mut vis) in &mut q {
        p.age += dt;
        if p.age < 0.0 {
            continue;
        }
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        *vis = Visibility::Inherited;
        let k = p.age / p.life;
        p.vel.y -= p.gravity * dt;
        t.translation += (p.vel + Vec3::Y * p.rise) * dt;
        let size = p.size[0] + (p.size[1] - p.size[0]) * k;
        match p.shape {
            Shape::Streak => {
                // Long along the flight, thin across.
                let dir = p.vel.normalize_or(Vec3::Y);
                t.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
                t.scale = Vec3::new(size * 0.25, size * 0.25, size * (1.0 + p.vel.length() * 0.02));
            }
            _ => {
                t.rotate(Quat::from_axis_angle(p.spin_axis, p.spin * dt));
                t.scale = Vec3::splat(size.max(0.01));
            }
        }
        let c = p.colour[0].lerp(p.colour[1], k);
        let col = Color::linear_rgba(c.x, c.y, c.z, c.w);
        if let (Some(r), Some(rows)) = (p.row, rows.as_deref_mut()) {
            rows.set_color(r, col);
        } else if let Some(mut m) = mats.get_mut(&p.material) {
            if m.base.base_color != col {
                m.base.base_color = col;
            }
        }
    }
}

/// A model whose draw nodes fly apart (arena-object fragment sets such as
/// st03's table pieces). Spawned as a glTF scene root.
#[derive(Component)]
pub struct FragmentBurst {
    /// Node names to animate.
    pub nodes: Vec<String>,
    pub life: f32,
    pub speed: f32,
    pub seed: u32,
    /// Gravity in the root's local space (units/sÂ²).
    pub gravity_local: Vec3,
    pub age: f32,
}

#[derive(Component)]
pub struct Fragment {
    root: Entity,
    base: Transform,
    vel: Vec3,
    spin_axis: Vec3,
    spin: f32,
}

/// Give each fragment node its flight once the scene has spawned.
///
/// Walks only the bursts' own descendants (overnight perf 2026-10-09: it used to walk every named
/// entity in the world -- every bone, stage node and HUD node -- and their ancestors, every frame,
/// ~0.6 ms with four players even with no burst alive). A node still belongs to its NEAREST burst
/// ancestor, as before.
pub fn attach_fragments(mut commands: Commands, bursts: Query<(Entity, &FragmentBurst)>, children: Query<&Children>, named: Query<(Entity, &Name, &Transform), Without<Fragment>>, parents: Query<&ChildOf>) {
    for (burst, _) in &bursts {
        for (e, name, t) in children.iter_descendants(burst).filter_map(|d| named.get(d).ok()) {
            let Some((root, b)) = parents.iter_ancestors(e).find_map(|a| bursts.get(a).ok()) else { continue };
            if root != burst {
                continue;
            }
            let Some(i) = b.nodes.iter().position(|n| n == name.as_str()) else { continue };
            let i = i as u32;
            // Out from the model's centre, mostly up and sideways.
            let out = Vec3::new(t.translation.x, 0.0, t.translation.z).normalize_or(cone_dir(b.seed, i, 60.0));
            let vel = (out * (0.5 + 0.5 * hash01(b.seed, i * 3)) + Vec3::Y * (0.6 + 0.6 * hash01(b.seed, i * 3 + 1))) * b.speed;
            let spin_axis = (Vec3::new(hash01(b.seed, i * 5), hash01(b.seed, i * 5 + 1), hash01(b.seed, i * 5 + 2)) * 2.0 - 1.0).normalize_or(Vec3::X);
            commands.entity(e).insert(Fragment { root, base: *t, vel, spin_axis, spin: 4.0 + 6.0 * hash01(b.seed, i * 7) });
        }
    }
}

pub fn tick_fragments(mut commands: Commands, time: Res<Time>, mut bursts: Query<(Entity, &mut FragmentBurst)>, mut frags: Query<(&Fragment, &mut Transform)>) {
    let dt = time.delta_secs();
    for (e, mut b) in &mut bursts {
        b.age += dt;
        if b.age >= b.life {
            commands.entity(e).despawn();
        }
    }
    for (f, mut t) in &mut frags {
        let Ok((_, b)) = bursts.get(f.root) else { continue };
        let a = b.age;
        t.translation = f.base.translation + f.vel * a + 0.5 * b.gravity_local * a * a;
        t.rotation = Quat::from_axis_angle(f.spin_axis, f.spin * a) * f.base.rotation;
        // Shrink away over the last quarter.
        let k = ((b.life - a) / (b.life * 0.25)).clamp(0.0, 1.0);
        t.scale = f.base.scale * k;
    }
}

fn argb(c: u32) -> Vec4 {
    let b = |s: u32| ((c >> s) & 0xFF) as f32 / 255.0;
    Vec4::new(b(16), b(8), b(0), b(24))
}

/// `PD_FX_STEP=1` (A/B switch): effects advance in whole 30 Hz ticks, as the
/// program's integer effect frame does (`FUN_1401899b0` adds 1.0 per update, the
/// only writer of the step). Default: the clock runs smoothly with wall time (the
/// piece windows, flipbook cells and flicker parity still follow the whole tick
/// either way), until Laurence picks (audits/P0-EFFECTS.md question 3).
fn fx_step() -> bool {
    env_once("PD_FX_STEP").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) > 0.0
}

/// The effect frame a piece is drawn at: the instance clock, whole ticks when
/// `step`.
fn effect_frame(age: f32, step: bool) -> f32 {
    if step { age.floor() } else { age }
}

/// A piece is drawn while its whole frame lies in `start ..= end`, the end
/// included (`FUN_140188fd0`: `start <= frame && frame <= end`; frames are
/// integers there, so `frame < end + 1` for a smooth clock).
fn in_window(frame: f32, start: f32, end: f32) -> bool {
    let f = frame.floor();
    f >= start && f <= end
}

/// The flipbook cell of a piece and its fraction (`FUN_1401a5f20` / `1401a8510`):
/// `stretch` = word 21 (as an integer) set: the cells are spread over the
/// window `d = end - start + 1`, `cell = trunc((frame - start) / d * cells)`
/// and the remainder is the cross-fade weight of the next cell; otherwise
/// one cell per tick, `(floor(frame) - start) mod cells`, no fraction (the
/// program's frame is whole).
fn flipbook_cell(frame: f32, start: f32, end: f32, cells: f32, stretch: bool) -> (f32, f32) {
    let cells = cells.max(1.0);
    if stretch {
        let d = (end - start + 1.0).max(1.0);
        let x = ((frame - start) / d * cells).max(0.0);
        let cell = x.floor().min(cells - 1.0);
        (cell, if cell < cells - 1.0 { x - cell } else { 0.0 })
    } else {
        let x = (frame.floor() - start).max(0.0);
        (if x >= cells { x % cells } else { x }, 0.0)
    }
}

/// A piece's UV transform for one flipbook cell: `uv * scale + offset` with
/// `scale = 1 / (cols, rows)`, the cell's column / row, and the scroll
/// (`FUN_1401a5ff0` writes (1/cols, 1/rows, col, row) and (w22, w23) x move
/// curve; the vertex shader computes `scale * (uv + cell) + scroll`).
fn cell_uv(cell: f32, grid: [f32; 2], scroll: Vec2) -> Vec4 {
    let (cols, rows) = (grid[0].max(1.0), grid[1].max(1.0));
    let (cx, cy) = (cell % cols, (cell / cols).floor());
    Vec4::new(1.0 / cols, 1.0 / rows, cx / cols + scroll.x, cy / rows + scroll.y)
}

/// A piece's colour at `frame` (`FUN_1401a9ae0`): the (start, end) pair is
/// (words 12, 13), or (14, 15) on odd whole frames when word 30 is set;
/// the blend factor is the colour curve over the window, or with word 31 > 0 a
/// fade: start -> end over the first `w31` frames, held at the end colour, back
/// over the last `w31` (the ramp out runs the curve with ease-in / ease-out
/// swapped).
fn piece_colour(p: &Piece, frame: f32) -> Vec4 {
    let odd = p.flicker && (frame.floor() as i64) & 1 == 1;
    let pair = if odd { p.colors_odd } else { p.colors };
    let (e, d) = (frame - p.start, p.end - p.start + 1.0);
    let kind = p.curves[2];
    let k = if p.fade_win > 0.0 {
        let w = p.fade_win;
        if e < w {
            curve(kind, e, w)
        } else {
            let rem = p.end - frame;
            if rem < w {
                let flipped = if kind != 0 { u8::from(kind == 1) + 1 } else { 0 };
                curve(flipped, rem, w)
            } else {
                1.0
            }
        }
    } else {
        curve(kind, e, d)
    };
    argb(pair[0]).lerp(argb(pair[1]), k)
}

/// Only pieces `bind_bones` has placed (on their bone or the root) show.
pub fn animate_pieces(instances: Query<&FxInstance>, tints: Query<&FxTint>, mut q: Query<(&FxPiece, &mut Transform, &mut Visibility, Option<&mut FxWisp>), With<FxBound>>, mut mats: ResMut<Assets<FxMaterial>>, mut rows: Option<ResMut<inst::FxInstances>>) {
    let step = fx_step();
    for (fx, mut t, mut vis, wisp) in &mut q {
        let Ok(inst) = instances.get(fx.instance) else { continue };
        let p = &fx.piece;
        // A skinned wisp spawner draws through its own bodies (`aura_wisp::update`).
        if p.skin.is_some() && p.wisp.is_some() {
            *vis = Visibility::Hidden;
            continue;
        }
        // An aura wisp spawner: the piece runs on its wisp's own clock, hidden between wisps.
        let mut wisp_look: Option<(Vec3, f32)> = None;
        let frame = match (p.wisp, wisp) {
            (Some(spec), Some(mut w)) if inst.fixed.is_none() => match w.step(&spec, p.grow, effect_frame(inst.age, step)) {
                Some(f) => {
                    wisp_look = Some((w.grow, w.roll));
                    f
                }
                None => {
                    *vis = Visibility::Hidden;
                    continue;
                }
            },
            _ => effect_frame(inst.fixed.unwrap_or(if inst.looping { inst.age % inst.len } else { inst.age }), step),
        };
        let on = in_window(frame, p.start, p.end);
        *vis = if on { Visibility::Inherited } else { Visibility::Hidden };
        if !on {
            continue;
        }
        // Move, scale and colour each follow their own easing (words 8-10, `curve`).
        let (e, d) = (frame - p.start, p.end - p.start + 1.0);
        let [c_move, c_scale, _, _] = p.curves.map(|kind| curve(kind, e, d));
        *t = fx.base;
        let grow = wisp_look.map_or(p.grow, |(g, _)| g);
        t.scale = fx.base.scale * (Vec3::ONE + grow * c_scale);
        if let Some((_, roll)) = wisp_look {
            // The wisp's own roll about its chain (the mesh's long axis, X).
            t.rotation = fx.base.rotation * Quat::from_rotation_x(roll);
        }
        let travel = Vec3::new(p.travel.x, p.travel.y, -p.travel.z) * c_move;
        if travel.is_finite() && travel.length_squared() < 1.0e6 {
            t.translation += fx.base.rotation * (fx.base.scale * travel);
        }
        // VFX-AUDIT.md #2 (see `CAST_REACH`'s doc comment): a slot-0,
        // root-anchored piece (`joint: null`) that sits further than
        // `CAST_REACH` from the caster's chest is pulled in to that
        // distance and shrunk by the same ratio -- a piece already inside
        // the envelope (ratio 1.0) is untouched, so this can only ever
        // shrink something that measures too big, never something already
        // right, and never touches a joint-attached piece (Psycho Wave,
        // Psycho Knife's hand flash, Blaster's own `RHAND` charge quads)
        // at all.
        if inst.slot == SLOT_CAST && p.joint.is_none() {
            let pivot = Vec3::Y * CAST_PIVOT_Y;
            let off = t.translation - pivot;
            let dist = off.length();
            let reach = cast_reach();
            if dist > reach && dist.is_finite() && dist > 0.0 {
                let k = reach / dist;
                t.translation = pivot + off * k;
                t.scale *= k;
            }
        }
        let mut c = piece_colour(p, frame);
        if p.distort {
            c.w *= distort_strength();
        }
        let tint = tints.get(fx.instance).ok();
        if let Some(tt) = tint {
            c = Vec4::new(c.x * tt.rgb.x, c.y * tt.rgb.y, c.z * tt.rgb.z, c.w);
        }
        // Roll (word 7): a mesh turns about its own Z, a billboard about the
        // view axis (the shader; `FxGamma::piece`).
        let roll = if p.roll != 0.0 && !p.solid { roll_radians(p.roll, c_move) } else { 0.0 };
        if roll != 0.0 && !p.billboard {
            t.rotation = fx.base.rotation * Quat::from_rotation_z(roll);
        }
        // Work out every value this frame writes, then touch the asset only if one differs from
        // what it already holds: any write through `get_mut`'s `DerefMut` flags the material as
        // modified and the render thread re-prepares it (a new bind group), so a piece that holds
        // still (a steady colour, a cell that has not turned over, a hidden-then-shown piece with
        // the same look) must not be written at all.
        let o = area_origin();
        let mut w = PieceWrite {
            quad: tint.map(|tt| Vec4::from(tt.view_alpha)),
            origin: tint.map(|_| (o.x as f32, o.y as f32)),
            roll,
            alpha: c.w,
            dist: p.dist.map(|ds| ds.at(curve(p.curves[3], e, d))),
            uv: PieceUv::None,
            color: Color::srgba(c.x, c.y, c.z, c.w),
        };
        // Flipbook cell and UV scroll (words 0 / 1, 21, 22 / 23).
        let cells = p.grid[0] * p.grid[1];
        let scroll = p.scroll * c_move;
        let (cell, frac) = flipbook_cell(frame, p.start, p.end, cells, p.stretch);
        let uv_a = cell_uv(cell, p.grid, scroll);
        if p.family == FAM_PLAIN {
            if cells > 1.0 || scroll != Vec2::ZERO {
                w.uv = PieceUv::Plain(bevy::math::Affine2::from_scale_angle_translation(Vec2::new(uv_a.x, uv_a.y), 0.0, Vec2::new(uv_a.z, uv_a.w)));
            }
        } else {
            let uv1 = match p.family {
                // The next cell of the same sheet (wrapping to the first, or holding the last).
                FAM_BB02 => {
                    let mut next = cell + 1.0;
                    if next >= cells {
                        next = if p.cell_wrap { 0.0 } else { cell };
                    }
                    cell_uv(next, p.grid, scroll)
                }
                // The second texture of PDMTEX sits still (`PDMTEX01_vs`: o2.xy = uv).
                _ => Vec4::new(1.0, 1.0, 0.0, 0.0),
            };
            let uv0 = if p.family == FAM_AURA { Vec4::new(1.0, 1.0, 0.0, 0.0) } else { uv_a };
            w.uv = PieceUv::Family { uv0, uv1, cross: frac };
        }
        if let (Some(r), Some(rows)) = (fx.row, rows.as_deref_mut()) {
            // A row of the shared buffer: one in-place buffer write for every piece together.
            if rows.row_mut(r).apply(&w) {
                rows.mark();
            }
        } else if let Some(mut ext) = mats.get_mut(&fx.material) {
            if w.differs(&ext) {
                w.apply(&mut ext);
            }
        }
    }
}

/// What `animate_pieces` writes into a piece's material in one frame.
#[derive(Clone, Debug, PartialEq)]
struct PieceWrite {
    /// The effect's per-quadrant alpha (`FxTint::view_alpha`); `None` = no tint on this effect.
    quad: Option<Vec4>,
    /// The game area's corner, written with `quad`.
    origin: Option<(f32, f32)>,
    roll: f32,
    alpha: f32,
    /// The heat haze's (bump, normal) strengths.
    dist: Option<(f32, f32)>,
    uv: PieceUv,
    color: Color,
}

#[derive(Clone, Debug, PartialEq)]
enum PieceUv {
    /// Nothing is written (a plain piece with a single cell and no scroll).
    None,
    /// A plain piece's own UV transform.
    Plain(bevy::math::Affine2),
    /// A shader family's UV placements and cross-fade weight.
    Family { uv0: Vec4, uv1: Vec4, cross: f32 },
}

impl PieceWrite {
    /// Whether applying this would change the material (read only: does not flag the asset).
    fn differs(&self, m: &FxMaterial) -> bool {
        let x = &m.extension;
        if let Some(q) = self.quad {
            if x.quad != q {
                return true;
            }
        }
        if let Some((ox, oy)) = self.origin {
            if x.params.z != ox || x.params.w != oy {
                return true;
            }
        }
        if x.piece.y != self.roll || x.piece.w != self.alpha || m.base.base_color != self.color {
            return true;
        }
        if let Some((bump, normal)) = self.dist {
            if x.extra.z != bump || x.extra.w != normal {
                return true;
            }
        }
        match self.uv {
            PieceUv::None => false,
            PieceUv::Plain(t) => m.base.uv_transform != t,
            PieceUv::Family { uv0, uv1, cross } => x.uv0 != uv0 || x.uv1 != uv1 || x.extra.x != cross,
        }
    }

    fn apply(&self, m: &mut FxMaterial) {
        let x = &mut m.extension;
        if let Some(q) = self.quad {
            x.quad = q;
        }
        if let Some((ox, oy)) = self.origin {
            x.params.z = ox;
            x.params.w = oy;
        }
        x.piece.y = self.roll;
        x.piece.w = self.alpha;
        if let Some((bump, normal)) = self.dist {
            x.extra.z = bump;
            x.extra.w = normal;
        }
        match self.uv {
            PieceUv::None => {}
            PieceUv::Plain(t) => m.base.uv_transform = t,
            PieceUv::Family { uv0, uv1, cross } => {
                x.uv0 = uv0;
                x.uv1 = uv1;
                x.extra.x = cross;
            }
        }
        m.base.base_color = self.color;
    }
}

/// A piece latched in world space (word 28 = 1 / 2): the world matrix of its
/// parent (the bone or the effect root) the first time it was drawn.
/// `FUN_1401a5940` looks the piece up by (effect instance, draw) in a table of 256
/// matrices (`FUN_14018e880`), storing the joint's current matrix on a miss
/// (`FUN_14018a9b0`) and drawing from the stored copy afterwards.
#[derive(Component)]
pub struct FxLatch(Mat4);

/// Word-28 pieces after transform propagation: mode 1 puts the piece at
/// `latched parent x local` (the whole matrix stays where it was at its first
/// drawn frame, the piece's own move / grow / roll still run in its own axes);
/// mode 2 keeps only the latched position, with the live scale of the joint's
/// matrix and world-aligned axes (`o+0x4c = 1`: translation of the latched
/// matrix, axis lengths of the live one).
#[allow(clippy::type_complexity)]
pub fn latch_pieces(
    mut commands: Commands,
    pieces: Query<(Entity, &FxPiece, &Transform, &Visibility, Option<&ChildOf>, Option<&Children>, Option<&FxLatch>), With<FxBound>>,
    mut piece_globals: Query<&mut GlobalTransform, With<FxPiece>>,
    mut others: Query<&mut GlobalTransform, Without<FxPiece>>,
    child_locals: Query<&Transform, Without<FxPiece>>,
    mut enabled: Local<Option<bool>>,
) {
    // `PD_FX_LATCH=0` (A/B switch): every piece follows its bone, as before word 28 was read.
    if !*enabled.get_or_insert_with(|| fx_flag("PD_FX_LATCH")) {
        return;
    }
    for (e, fx, local, vis, parent, children, latch) in &pieces {
        if fx.piece.latch == 0 || *vis == Visibility::Hidden {
            continue;
        }
        let Some(parent) = parent else { continue };
        let Ok(parent_now) = others.get(parent.parent()).map(|g| g.to_matrix()) else { continue };
        let Some(latch) = latch else {
            // First drawn frame: remember the parent's world matrix, draw as usual.
            commands.entity(e).insert(FxLatch(parent_now));
            continue;
        };
        let anim = local.to_matrix();
        let world = latched_world(fx.piece.latch, latch.0, parent_now, fx.base.to_matrix(), anim);
        if !world.is_finite() {
            continue;
        }
        let g = GlobalTransform::from(world);
        if let Ok(mut pg) = piece_globals.get_mut(e) {
            *pg = g;
        }
        for &c in children.map(|c| &**c).unwrap_or(&[]) {
            if let Ok(mut cg) = others.get_mut(c) {
                let cl = child_locals.get(c).copied().unwrap_or(Transform::IDENTITY);
                *cg = GlobalTransform::from(world * cl.to_matrix());
            }
        }
    }
}

/// The world matrix of a latched piece (`mode` = word 28, 1 or 2): `latched`
/// and `live` = the parent's world matrix at the first drawn frame / now,
/// `base` = the piece's own node matrix (the joint's local transform), `anim` =
/// its local matrix this frame (`base` x its move, grow and roll).
fn latched_world(mode: u8, latched: Mat4, live: Mat4, base: Mat4, anim: Mat4) -> Mat4 {
    if mode == 1 {
        latched * anim
    } else {
        let latched_base = latched * base;
        let live_base = live * base;
        let scale = Mat4::from_scale(Vec3::new(live_base.x_axis.truncate().length(), live_base.y_axis.truncate().length(), live_base.z_axis.truncate().length()));
        let rel = if base.determinant().abs() > 1e-9 { base.inverse() * anim } else { anim };
        Mat4::from_translation(latched_base.w_axis.truncate()) * scale * rel
    }
}

#[cfg(test)]
mod tests {
    fn sample_write() -> PieceWrite {
        PieceWrite { quad: Some(Vec4::new(1.0, 0.5, 1.0, 1.0)), origin: Some((3.0, 4.0)), roll: 0.25, alpha: 0.5, dist: Some((0.1, 0.2)), uv: PieceUv::Family { uv0: Vec4::new(0.5, 0.5, 0.25, 0.0), uv1: Vec4::new(0.5, 0.5, 0.5, 0.0), cross: 0.3 }, color: Color::srgba(0.1, 0.2, 0.3, 0.5) }
    }

    /// `animate_pieces` writes a material only when a value changed: the same write twice
    /// differs the first time and never the second, and every single field is noticed.
    #[test]
    fn a_piece_write_is_noticed_only_when_a_value_changes() {
        let mut m = fx_material_gamma(StandardMaterial::default(), 2.2);
        let w = sample_write();
        assert!(w.differs(&m), "a fresh material differs");
        w.apply(&mut m);
        assert!(!w.differs(&m), "the same write again changes nothing");
        let mut v = w.clone();
        v.roll = 0.5;
        assert!(v.differs(&m), "roll");
        let mut v = w.clone();
        v.alpha = 0.6;
        assert!(v.differs(&m), "alpha");
        let mut v = w.clone();
        v.color = Color::srgba(0.1, 0.2, 0.3, 0.6);
        assert!(v.differs(&m), "colour");
        let mut v = w.clone();
        v.quad = Some(Vec4::ONE);
        assert!(v.differs(&m), "view alpha");
        let mut v = w.clone();
        v.origin = Some((3.0, 5.0));
        assert!(v.differs(&m), "game area corner");
        let mut v = w.clone();
        v.dist = Some((0.1, 0.3));
        assert!(v.differs(&m), "haze strength");
        let mut v = w.clone();
        v.uv = PieceUv::Family { uv0: Vec4::new(0.5, 0.5, 0.25, 0.0), uv1: Vec4::new(0.5, 0.5, 0.5, 0.0), cross: 0.4 };
        assert!(v.differs(&m), "cross-fade weight");
        // A plain piece: the UV transform counts only when the piece writes one.
        let mut plain = w.clone();
        plain.uv = PieceUv::None;
        assert!(!plain.differs(&m));
        plain.uv = PieceUv::Plain(bevy::math::Affine2::from_scale_angle_translation(Vec2::new(0.5, 0.5), 0.0, Vec2::new(0.5, 0.0)));
        assert!(plain.differs(&m));
        plain.apply(&mut m);
        assert!(!plain.differs(&m));
    }

    /// The point of it: an unchanged write raises no `AssetEvent::Modified` (the event that makes
    /// the render thread re-prepare the material), a changed one raises exactly one.
    #[test]
    fn an_unchanged_piece_write_raises_no_modified_event() {
        use bevy::asset::AssetEvent;
        let mut app = App::new();
        app.add_plugins(bevy::asset::AssetPlugin::default()).init_asset::<FxMaterial>();
        let h = app.world_mut().resource_mut::<Assets<FxMaterial>>().add(fx_material_gamma(StandardMaterial::default(), 2.2));
        let w = sample_write();
        let write = |app: &mut App, w: &PieceWrite| {
            let mut mats = app.world_mut().resource_mut::<Assets<FxMaterial>>();
            if let Some(mut m) = mats.get_mut(&h) {
                if w.differs(&m) {
                    w.apply(&mut m);
                }
            };
        };
        let modified = |app: &mut App| {
            app.update();
            let msgs = app.world().resource::<Messages<AssetEvent<FxMaterial>>>();
            msgs.iter_current_update_messages().filter(|e| matches!(e, AssetEvent::Modified { .. })).count()
        };
        write(&mut app, &w);
        assert_eq!(modified(&mut app), 1, "the first write changes the material");
        write(&mut app, &w);
        assert_eq!(modified(&mut app), 0, "the same values again: no event");
        let mut v = w.clone();
        v.alpha = 0.9;
        write(&mut app, &v);
        assert_eq!(modified(&mut app), 1, "a changed value: one event");
    }

    use super::*;

    fn def(pieces: &[(&str, usize, &str)], emitters: &[(&str, usize)]) -> FxDef {
        let v = serde_json::json!({
            "slots": [[0], [1], [], [], [], []],
            "pieces": pieces.iter().map(|(node, model, shader)| serde_json::json!({ "node": node, "model": model, "shader": shader, "start": 0.0, "end": 30.0 })).collect::<Vec<_>>(),
            "emitters": emitters.iter().map(|(kind, model)| serde_json::json!({ "kind": kind, "model": model, "texture": 3, "start": 0.0, "end": 60.0, "raw": vec![0u32; 32] })).collect::<Vec<_>>(),
        });
        FxDef::parse("eft9999", &v, |_| true).unwrap()
    }

    #[test]
    fn a_flight_of_only_hidden_distortion_shows_nothing_but_its_sprites_do() {
        // Hell's Fire / Swarm of Moths: the flight slot is emitters only.
        let hells = def(&[("m1_d0", 1, "PDEFTOPT05")], &[("a07ac6858891fcc0", 1), ("893e3f31124f8000", 1)]);
        assert!(!hells.draws_pieces(1));
        assert!(hells.shows(1, true), "sprite emitters show on a flight");
        // A cast/hit/impact slot (played with `looping = false`) shows its
        // sprite emitters too (2026-09-27, bladefx lane): Laser, Blaster and
        // every blade skill author one into their slot-0 cast model (e.g.
        // Psycho Knife's `a07ac685`, frames 12-22), and `tick_sprites`
        // already bounds emission to the emitter's own start/end frames
        // whether or not the slot loops.
        assert!(hells.shows(1, false), "sprite emitters show on a one-shot slot too");
        assert_eq!(hells.census(1), [0, 1, 0, 1], "the light emitter isn't a sprite");
        // Only a distortion piece: the stand-in ball is used instead.
        assert!(!def(&[("m1_d0", 1, "PDEFTOPT05")], &[]).shows(1, true));
    }

    /// An emitter JSON with raw words set (the rest 0).
    fn emitter(kind: &str, model: usize, joint: &str, words: &[(usize, f32)]) -> serde_json::Value {
        let mut raw = vec![0u32; 32];
        for &(i, w) in words {
            raw[i] = w.to_bits();
        }
        serde_json::json!({ "kind": kind, "model": model, "texture": 4, "start": 17.0, "end": 23.0, "colors": [0xffff_ffffu32, 0x00ff_ffffu32], "raw": raw, "joint": joint, "matrix": null })
    }

    #[test]
    fn flare_rays_spin_by_word_20_per_drawn_frame() {
        let mut f = Flare { model: 0, texture: String::new(), start: 0.0, end: 0.0, colors: [0; 2], count: 1, life: 10.0, fade: 0.0, length: [1.0; 2], width: [1.0; 2], random: [false; 2], spin: 0, joint: None, offset: Vec3::ZERO };
        assert_eq!(flare_spin(&f, Vec3::Y, Vec3::Z, 5.0), (Vec3::Y, Vec3::Z));
        // 8192 units = a quarter turn per drawn frame: the first frame is unturned, the next is 90 degrees.
        f.spin = 8192;
        let (d0, _) = flare_spin(&f, Vec3::Y, Vec3::Z, 0.0);
        let (d1, a1) = flare_spin(&f, Vec3::Y, Vec3::Z, 1.0);
        assert!((d0 - Vec3::Y).length() < 1e-5);
        assert!((d1 - Vec3::Z).length() < 1e-5 && (a1 + Vec3::Y).length() < 1e-5);
        // Whole drawn frames only (a ray born part-way through a frame turns on the next one).
        assert!((flare_spin(&f, Vec3::Y, Vec3::Z, 1.9).0 - Vec3::Z).length() < 1e-5);
    }

    #[test]
    fn blades_and_flares_are_read_from_their_emitters() {
        // Psycho Knife's BLADE01 (eft0003): words 0, 1, 28 = 8, 10, 13, on RSWORD;
        // its FLAIR01 (packed words): 16 rays, 4 frames, lengths 50 / 50 -> 10 / 10
        // and widths 20 / 0 -> 5 / 5 with the random flags (words 24, 25) set.
        let v = serde_json::json!({
            "slots": [[0], [], [], [], [], []],
            "pieces": [{ "node": "m0_d0", "model": 0, "shader": "PDEFTBB01", "start": 19.0, "end": 25.0, "joint": "RSWORD" }],
            "emitters": [
                emitter("4ca477d90c350000", 0, "RSWORD", &[(0, 8.0), (1, 10.0), (28, 13.0)]),
                emitter("650e99210c350000", 0, "RHAND", &[(4, 50.0), (5, 50.0), (6, 10.0), (7, 10.0), (8, 20.0), (9, 0.0), (10, 5.0), (11, 5.0), (24, 1.0), (25, 1.0), (30, 16.0), (31, 4.0)]),
                // A typical FLAIR01 (eft0046): min 0, max 40 / 20, random -> spans 40 / 20.
                emitter("650e99210c350000", 1, "RHAND", &[(6, 40.0), (7, 20.0), (10, 15.0), (11, 7.0), (24, 1.0), (25, 1.0), (30, 10.0), (31, 5.0)]),
                // Flags clear: the fixed words 4 / 5 and 8 / 9 as they are.
                emitter("650e99210c350000", 1, "RHAND", &[(4, 30.0), (5, 20.0), (8, 6.0), (9, 2.0), (30, 1.0), (31, 5.0)]),
            ],
        });
        let d = FxDef::parse("eft0003", &v, |_| true).unwrap();
        assert_eq!(d.blade_count(0), 1);
        let b = &d.blades[0];
        // Word 0 = 8 drawn frames = 4 effect frames.
        assert_eq!((b.linger, b.steps, b.length, b.joint.as_deref()), (4.0, 10, 13.0, Some("RSWORD")));
        let f = &d.flares[0];
        // World units as authored, no per-effect scale (FORMATS-EFFECTS.md
        // "Effect units"): the random spans are max - min, per `FUN_1401a1170`.
        assert_eq!((f.count, f.life, f.length, f.width, f.random, f.joint.as_deref()), (16, 4.0, [-40.0, -40.0], [-15.0, 5.0], [true, true], Some("RHAND")));
        // Each ray scales the spans by its own factors; drawn size is the magnitude.
        assert_eq!(ray_sizes(f, [0.5, 0.25, 1.0, 0.0]), ([-20.0, -10.0], [-15.0, 0.0]));
        assert_eq!((d.flares[1].length, d.flares[1].width), ([40.0, 20.0], [15.0, 7.0]));
        let fixed = &d.flares[2];
        assert_eq!((fixed.length, fixed.width, fixed.random), ([30.0, 20.0], [6.0, 2.0], [false, false]));
        assert_eq!(ray_sizes(fixed, [0.1, 0.2, 0.3, 0.4]), ([30.0, 20.0], [6.0, 2.0]));

        assert_eq!(d.pieces["m0_d0"].joint.as_deref(), Some("RSWORD"));
        // The slot lasts until the ribbon has faded (23 + 4) and the rays are
        // gone (14 + 1 + 4 / 2), not just the last piece (25).
        assert_eq!(d.slot_len(0), 27.0);
        assert!(d.shows(0, false));
    }

    #[test]
    fn a_light02_fades_its_range_by_word_7_and_never_dims_its_colour() {
        // Bomb's cast light (eft0031 model 0): range 0 -> 3000, frames 0-30, word 7 = 15,
        // word 10 = 2 (word 9 = 0: the old read made it never fade, the range climbing
        // 0 -> 3000 across the window and peaking at frame 30, while the bomb flies).
        let l = Light { model: 0, start: 0.0, end: 30.0, colors: [0xffff7f3f; 2], range: [0.0, 3000.0], fade: 15.0, fade_curve: 2 };
        assert_eq!(light_fade(&l, -0.5), None);
        assert_eq!(light_fade(&l, 0.0), Some(0.0));
        // Fade in, easing 2: x (2F - x - 1) / F^2.
        assert!((light_fade(&l, 5.0).unwrap() - 5.0 * 24.0 / 225.0).abs() < 1e-6);
        assert_eq!(light_fade(&l, 15.0), Some(1.0));
        assert_eq!(light_fade(&l, 16.0), Some(1.0));
        // Fade out over the last 15 frames with x = end + 1 - frame and the easing flipped to 1.
        assert!((light_fade(&l, 26.0).unwrap() - 5.0 * 6.0 / 225.0).abs() < 1e-6);
        assert!((light_fade(&l, 30.5).unwrap() - 0.5 * 1.5 / 225.0).abs() < 1e-6);
        assert_eq!(light_fade(&l, 31.0), None);
        // No fade: on (1) through the window.
        let flat = Light { fade: 0.0, ..l.clone() };
        assert_eq!(light_fade(&flat, 0.0), Some(1.0));
        assert_eq!(light_fade(&flat, 30.9), Some(1.0));
        // Linear easing.
        let lin = Light { fade_curve: 0, ..l };
        assert!((light_fade(&lin, 3.0).unwrap() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn the_light_fade_length_is_word_7() {
        // Built from the emitter json the way the cache stores it (raw = the 32 words).
        let mut raw = vec![0u32; 32];
        raw[1] = 3000f32.to_bits();
        raw[3] = 30f32.to_bits();
        raw[7] = 15f32.to_bits();
        raw[10] = 2f32.to_bits();
        let v = serde_json::json!({"slots": [[0]], "pieces": [], "emitters": [{"kind": "893e3f31124f8000", "model": 0, "start": 0.0, "end": 30.0, "colors": [0xffff7f3fu32, 0xffff7f3fu32, 0, 0], "raw": raw}]});
        let d = FxDef::parse("eft9998", &v, |_| true).expect("def");
        assert_eq!(d.lights.len(), 1);
        assert_eq!(d.lights[0].fade, 15.0);
        assert_eq!(d.lights[0].fade_curve, 2);
        assert_eq!(d.lights[0].range, [0.0, 3000.0]);
    }

    #[test]
    fn the_programs_easing_curves() {
        // d = 16 frames (Psycho Knife's round flash, 13-29), 3 frames in.
        assert!((curve(0, 3.0, 16.0) - 3.0 / 16.0).abs() < 1e-6);
        assert!((curve(1, 3.0, 16.0) - 12.0 / 272.0).abs() < 1e-6);
        // Curve 2 is e (2d - e + 1) / (d (d + 1)) (`FUN_1401a5ff0`'s
        // `((d + d) - (e - 1)) * e * 0.5 * norm`), not the `ykEDGESLUR00`
        // class's e (2d - e - 1): 3 x 30 = 90 (P0-EFFECTS.md D13).
        assert!((curve(2, 3.0, 16.0) - 90.0 / 272.0).abs() < 1e-6);
        // Both eased curves, like the linear one, arrive at 1 on the last frame.
        for kind in 0..3 {
            assert!((curve(kind, 16.0, 16.0) - 1.0).abs() < 1e-6, "curve {kind} ends at 1");
            assert_eq!(curve(kind, 0.0, 16.0), 0.0);
        }
        // Fast start runs ahead of linear, slow start behind.
        assert!(curve(2, 8.0, 16.0) > curve(0, 8.0, 16.0) && curve(1, 8.0, 16.0) < curve(0, 8.0, 16.0));
    }

    /// A piece's JSON with the given raw words (the rest 0) and material words.
    fn piece_json(node: &str, shader: &str, words: &[(usize, f32)], mat: Option<(u16, u16)>) -> serde_json::Value {
        let mut raw = vec![0u32; 32];
        for &(i, w) in words {
            raw[i] = w.to_bits();
        }
        let mut v = serde_json::json!({ "node": node, "model": 0, "shader": shader, "start": 0.0, "end": 30.0, "raw": raw, "colors": [0xffff_ffffu32, 0u32] });
        if let Some((flags, state)) = mat {
            v["mat_flags"] = flags.into();
            v["mat_state"] = state.into();
        }
        v
    }

    fn def_of(pieces: Vec<serde_json::Value>) -> FxDef {
        FxDef::parse("eft9999", &serde_json::json!({ "slots": [[0], [], [], [], [], []], "pieces": pieces, "emitters": [] }), |_| true).unwrap()
    }

    /// Bomb's x6 blast was an opaque stepped slab: its slot-2 puffs (eft0031 emitters
    /// 4 / 5, kind MSPARTICLE01, state `0x1400` = SRC_ALPHA, ONE, texture 3 a dark red
    /// disc with alpha) were cut out at alpha 0.5 and depth-written like smoke. A puff
    /// whose own material adds (destination factor ONE) is a glow and is added; one
    /// whose state alpha-blends (`0x5400`, SRC_ALPHA, ONE_MINUS_SRC_ALPHA) stays the
    /// translucent smoke cut-out. Timer Mine shared the slab.
    #[test]
    fn an_additive_puff_is_never_the_opaque_cutout() {
        let emitter = |state: u64| {
            serde_json::json!({
                "kind": "90e7fd3e760929ea", "model": 2, "texture": 3, "start": 0.0, "end": 5.0,
                "colors": [0, 0xffff_ffffu64], "joint": null, "mat_flags": 0x4011, "mat_state": state,
                "matrix": [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
                "raw": (0..32).map(|i| if i == 30 { 15.0f32.to_bits() } else if i == 31 { 25.0f32.to_bits() } else { 0 }).collect::<Vec<u32>>(),
            })
        };
        let d = FxDef::parse("eft9999", &serde_json::json!({ "slots": [[], [], [2], [], [], []], "pieces": [], "emitters": [emitter(0x1400), emitter(0x5400)] }), |_| true).unwrap();
        assert_eq!(d.sprites.len(), 2);
        let (add, smoke) = (&d.sprites[0], &d.sprites[1]);
        assert_eq!((add.blend, add.additive, smoke.additive), (true, true, false));
        assert!(!puff_is_translucent(add, Some(true)), "an additive puff with an alpha texture must be added, not alpha-cut");
        assert!(puff_is_translucent(smoke, Some(true)), "an alpha-blended puff keeps the translucent cut-out");
        assert!(!puff_is_translucent(smoke, Some(false)), "an opaque-textured puff is added as before");
    }

    #[test]
    fn a_piece_reads_its_blend_billboard_and_cull_words() {
        // Word 19 is 0 / 1 / 2 (`FUN_1401a5ff0` billboards on 0; `FUN_14011bc30`
        // turns culling off only on 2.0, back-face culls otherwise).
        let d = def_of(vec![
            piece_json("m0_d0", "PDEFTBB01", &[(19, 0.0), (7, 45.0)], Some((0x4011, 0x1800))),
            piece_json("m0_d1", "PDEFTBB01", &[(19, 1.0)], Some((0x4011, 0x1100))),
            piece_json("m0_d2", "PDEFTBB01", &[(19, 2.0)], Some((0x11, 0x3000))),
            // Blend flag clear: the state word is ignored, blending off.
            piece_json("m0_d3", "PDEFTBB01", &[(19, 2.0)], Some((0x1, 0x1100))),
            // An old cache (no material words) keeps the additive draw, two-sided.
            piece_json("m0_d4", "PDEFTBB01", &[(19, 1.0)], None),
            // Hand-made data without raw words: no billboard, no culling.
            serde_json::json!({ "node": "m0_d5", "model": 0, "shader": "PDEFTBB01", "start": 0.0, "end": 30.0 }),
        ]);
        let p = |n: &str| &d.pieces[n];
        assert_eq!((p("m0_d0").billboard, p("m0_d0").cull, p("m0_d0").roll), (true, true, 45.0));
        assert_eq!(p("m0_d0").blend, Some(BlendState { src: BlendFactor::DstColor, dst: BlendFactor::One, op: BlendOp::Add }));
        assert_eq!((p("m0_d1").billboard, p("m0_d1").cull), (false, true));
        assert_eq!((p("m0_d2").billboard, p("m0_d2").cull), (false, false));
        assert_eq!(p("m0_d2").blend, Some(BlendState { src: BlendFactor::Zero, dst: BlendFactor::OneMinusSrcColor, op: BlendOp::Add }));
        assert!(p("m0_d3").blend.is_none() && p("m0_d3").blend_known);
        assert!(!p("m0_d4").blend_known && !p("m0_d4").cull && !p("m0_d4").billboard);
        assert_eq!((p("m0_d5").billboard, p("m0_d5").cull, p("m0_d5").blend_known), (false, false, false));
    }

    #[test]
    fn the_alpha_rule_follows_the_shader_family_and_the_blend() {
        // `PDEFTBB*_vs` premultiplies the colour by its alpha; the point-sprite
        // and PDMTEX shaders pass it straight (disassembled 2026-09-30).
        let d = def_of(vec![
            piece_json("m0_d0", "PDEFTBB01", &[(19, 2.0)], Some((0x11, 0x1100))),
            piece_json("m0_d1", "PDEFTBB08", &[(19, 2.0)], Some((0x11, 0x1400))),
            piece_json("m0_d2", "MSEFTBB01S", &[(19, 0.0)], Some((0x4011, 0x1800))),
            piece_json("m0_d3", "PDMTEX01", &[(19, 2.0)], Some((0x11, 0x1100))),
        ]);
        let flags = |n: &str| (d.pieces[n].premultiply, d.pieces[n].billboard, d.pieces[n].cull);
        assert_eq!(flags("m0_d0"), (true, false, false));
        assert_eq!(flags("m0_d1"), (true, false, false));
        assert_eq!(flags("m0_d2"), (false, true, false), "the 240 Laser charge sprites: points, billboards, no premultiply, nothing to cull");
        assert_eq!(flags("m0_d3"), (false, false, false));
        // How the colour is encoded for the blend it meets (`fx_gamma.wgsl` modes).
        let mode = |flags, state| blend_mode(BlendState::decode(flags, state));
        assert_eq!(mode(0x11, 0x1100), 1.0, "ONE, ONE: additive, colour only");
        assert_eq!(mode(0x11, 0x1400), 1.0, "SRC_ALPHA, ONE");
        assert_eq!(mode(0x11, 0x1200), 1.0, "SRC_COLOR, ONE");
        assert_eq!(mode(0x11, 0x1800), 2.0, "DST_COLOR, ONE");
        assert_eq!(mode(0x11, 0x3000), 3.0, "ZERO, 1 - SRC_COLOR darkens");
        assert_eq!(mode(0x11, 0x2000), 5.0, "ZERO, SRC_COLOR is a plain factor");
        assert_eq!(mode(0x11, 0x5400), 4.0, "alpha blend");
        assert_eq!(mode(0x1, 0x1100), 6.0, "blend flag clear: alpha test only");
    }

    #[test]
    fn blend_equations_reach_wgpu_unchanged() {
        use bevy::render::render_resource as rr;
        let b = |flags, state| wgpu_blend(BlendState::decode(flags, state).unwrap());
        let darken = b(0x11, 0x3000).color;
        assert_eq!((darken.src_factor, darken.dst_factor, darken.operation), (rr::BlendFactor::Zero, rr::BlendFactor::OneMinusSrc, rr::BlendOperation::Add));
        let dst_add = b(0x11, 0x1800).color;
        assert_eq!((dst_add.src_factor, dst_add.dst_factor), (rr::BlendFactor::Dst, rr::BlendFactor::One));
        let alpha = b(0x11, 0x5400).color;
        assert_eq!((alpha.src_factor, alpha.dst_factor), (rr::BlendFactor::SrcAlpha, rr::BlendFactor::OneMinusSrcAlpha));
        assert_eq!(b(0x11, 0x1240).color.operation, rr::BlendOperation::ReverseSubtract);
        // min / max ignore their factors (D3D11 does too; wgpu insists on One).
        let max = b(0x11, 0x5480).color;
        assert_eq!((max.operation, max.src_factor, max.dst_factor), (rr::BlendOperation::Max, rr::BlendFactor::One, rr::BlendFactor::One));
    }

    /// CPU twin of `fx_gamma.wgsl`'s `billboard_world_position`: the piece's
    /// origin into the drawing camera's view space, the quad rotated by the roll
    /// in the view plane, then scaled by the matrix's axis lengths.
    fn billboard_vertex(view_from_world: Mat4, world_from_local: Mat4, local: Vec3, roll: f32) -> Vec3 {
        let (sx, sy, sz) = (world_from_local.x_axis.truncate().length(), world_from_local.y_axis.truncate().length(), world_from_local.z_axis.truncate().length());
        let o = view_from_world.transform_point3(world_from_local.w_axis.truncate());
        let (s, c) = roll.sin_cos();
        let r = Vec2::new(c * local.x - s * local.y, s * local.x + c * local.y);
        view_from_world.inverse().transform_point3(o + Vec3::new(r.x * sx, r.y * sy, local.z * sz))
    }

    #[test]
    fn a_billboard_faces_whichever_camera_draws_it() {
        // A piece at (4, 2, -6), scale 3, a 10-unit quad (corners +-5) as in
        // the Laser charge sprites; two cameras that look at it from different sides.
        let model = Mat4::from_scale_rotation_translation(Vec3::splat(3.0), Quat::from_rotation_y(0.7), Vec3::new(4.0, 2.0, -6.0));
        let corner = Vec3::new(5.0, 5.0, 0.0);
        for eye in [Vec3::new(0.0, 5.0, 10.0), Vec3::new(-30.0, 1.0, -50.0)] {
            let view = Transform::from_translation(eye).looking_at(Vec3::new(4.0, 2.0, -6.0), Vec3::Y);
            let view_from_world = view.to_matrix().inverse();
            let p = |roll: f32, local: Vec3| billboard_vertex(view_from_world, model, local, roll);
            let centre = p(0.0, Vec3::ZERO);
            assert!((centre - Vec3::new(4.0, 2.0, -6.0)).length() < 1e-3, "origin stays put");
            // The quad lies in the camera's plane: a corner minus the centre has no view-depth.
            let d = view_from_world.transform_vector3(p(0.0, corner) - centre);
            assert!(d.z.abs() < 1e-3, "quad is flat to the camera, depth {}", d.z);
            // Right / up on screen, scaled by 3: (15, 15).
            assert!((d.x - 15.0).abs() < 1e-3 && (d.y - 15.0).abs() < 1e-3, "{d:?}");
            // A quarter-turn roll takes the corner from (+x, +y) to (-y, +x) on screen: counter-clockwise.
            let q = view_from_world.transform_vector3(p(std::f32::consts::FRAC_PI_2, corner) - centre);
            assert!((q.x + 15.0).abs() < 1e-3 && (q.y - 15.0).abs() < 1e-3, "{q:?}");
        }
    }

    #[test]
    fn the_roll_is_the_programs_integer_step() {
        // word 7 x curve, truncated to whole degrees, then deg * 0x8000 / 0x168.
        let tau = std::f32::consts::TAU;
        assert!((roll_radians(90.0, 1.0) - tau / 4.0).abs() < 1e-5);
        assert!((roll_radians(360.0, 1.0)).abs() < 1e-5, "a full turn wraps to 0");
        assert!((roll_radians(90.9, 1.0) - tau / 4.0).abs() < 1e-3, "degrees truncate: 90.9 is 90");
        assert!((roll_radians(180.0, 0.5) - tau / 4.0).abs() < 1e-5, "x the move curve");
        assert!((roll_radians(-90.0, 1.0) - 0.75 * tau).abs() < 1e-5, "negative turns wrap the circle");
        assert_eq!(roll_radians(0.0, 1.0), 0.0);
    }

    #[test]
    fn particle01_is_read_as_count_life_and_point_size() {
        // Psycho Knife's PARTICLE01: 15 points a frame, 15 drawn frames each,
        // size 500 -> 2500, speed 0.2 per drawn frame.
        let v = serde_json::json!({
            "slots": [[0], [], [], [], [], []],
            "pieces": [],
            "emitters": [emitter("a07ac6858891f680", 0, "RHAND", &[(0, 1.0), (1, 1.0), (7, 0.2), (16, 500.0), (17, 2500.0), (30, 15.0), (31, 15.0)])],
        });
        let d = FxDef::parse("eft0003", &v, |_| true).unwrap();
        let s = &d.sprites[0];
        let p = s.points.expect("PARTICLE01 is read the program's way");
        assert_eq!((p.count, p.speed[0]), (15, 0.2));
        assert_eq!(s.life, 15.0 / STEPS_PER_FRAME);
        assert_eq!(s.size, [500.0 * POINT_SIZE, 2500.0 * POINT_SIZE]);
        // A ray burst fades by word 28 when set: towards the end colour, then back.
        let f = Flare { model: 0, texture: String::new(), start: 0.0, end: 0.0, colors: [0; 2], count: 1, life: 10.0, fade: 2.0, length: [1.0; 2], width: [1.0; 2], random: [false; 2], spin: 0, joint: None, offset: Vec3::ZERO };
        assert_eq!((flare_fade(&f, 1.0), flare_fade(&f, 5.0), flare_fade(&f, 9.0)), (0.5, 1.0, 0.5));
    }

    #[test]
    fn pair_names_follow_their_first_bone() {
        assert_eq!(bone_for("LHRH"), "LHAND");
        assert_eq!(bone_for("B1RA"), "BODY1");
        assert_eq!(bone_for("RSWORD"), "RSWORD");
    }

    #[test]
    fn a_blade_ribbon_is_a_strip_through_its_samples() {
        let s = [(Vec3::ZERO, Vec3::Z), (Vec3::X, Vec3::X + Vec3::Z), (Vec3::X * 2.0, Vec3::X * 2.0 + Vec3::Z)];
        let m = blade_mesh(&s, 4);
        // (samples - 1) * steps + 1 pairs of vertices.
        assert_eq!(m.count_vertices(), 2 * (2 * 4 + 1));
        let f = Flare { model: 0, texture: String::new(), start: 0.0, end: 0.0, colors: [0; 2], count: 1, life: 4.0, fade: 0.0, length: [1.0; 2], width: [1.0; 2], random: [false; 2], spin: 0, joint: None, offset: Vec3::ZERO };
        let ray = FlareRay { c: Vec3::ZERO, dir: Vec3::Y, across: Vec3::X, age: 0.0, length: [-2.0; 2], width: [1.0; 2] };
        let m = flare_mesh(&f, &[ray]);
        assert_eq!(m.count_vertices(), 4);
        // A negative (max < min) length is drawn by its magnitude along the ray.
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        assert_eq!(p[0], [-0.5, 2.0, 0.0]);
    }

    // ---- mesh_reuse (lane fxperf): the in-place writers make exactly what the old builders made ----

    // The builders as they were before `mesh_reuse.rs` (verbatim), the reference for the tests below.
    fn legacy_blade_mesh(samples: &[(Vec3, Vec3)], steps: usize) -> Mesh {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology};
        let n = samples.len();
        let at = |i: isize| samples[i.clamp(0, n as isize - 1) as usize];
        let mut edge = Vec::new();
        if n >= 2 {
            for i in 0..n - 1 {
                let (a0, a1, a2, a3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
                for s in 0..steps {
                    let t = s as f32 / steps as f32;
                    edge.push((catmull(a0.0, a1.0, a2.0, a3.0, t), catmull(a0.1, a1.1, a2.1, a3.1, t)));
                }
            }
            edge.push(samples[n - 1]);
        }
        let m = edge.len();
        let ramp = (steps / 2).max(1) as f32;
        let (mut pos, mut uv, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (k, (base, tip)) in edge.iter().enumerate() {
            let v = k as f32 / (m.max(2) - 1) as f32;
            let a = (k as f32 / ramp).min((m - 1 - k) as f32 / ramp).clamp(0.0, 1.0);
            pos.push(base.to_array());
            pos.push(tip.to_array());
            uv.push([0.0, v]);
            uv.push([1.0, v]);
            col.push([1.0, 1.0, 1.0, a]);
            col.push([1.0, 1.0, 1.0, a]);
            if k + 1 < m {
                let b = (k * 2) as u32;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        }
        let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_indices(Indices::U32(idx))
    }
    fn legacy_flare_mesh(f: &Flare, rays: &[FlareRay]) -> Mesh {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology};
        let (mut pos, mut uv, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for &FlareRay { c, dir, across, age, length, width } in rays {
            let (dir, across) = flare_spin(f, dir, across, age);
            let k = (age / f.life).clamp(0.0, 1.0);
            let len = (length[0] + (length[1] - length[0]) * k).abs();
            let w = (width[0] + (width[1] - width[0]) * k).abs() * 0.5;
            let tip = c + dir * len;
            let b = pos.len() as u32;
            for p in [tip - across * w, tip + across * w, c, c] {
                pos.push(p.to_array());
            }
            uv.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]);
            let cc = argb(f.colors[0]).lerp(argb(f.colors[1]), flare_fade(f, age));
            let lin = Color::srgba(cc.x, cc.y, cc.z, cc.w).to_linear();
            col.extend_from_slice(&[[lin.red, lin.green, lin.blue, lin.alpha]; 4]);
            idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
        }
        if pos.is_empty() {
            // Never empty (a zero-length vertex buffer upsets the render allocator).
            pos = vec![[0.0; 3]; 3];
            uv = vec![[0.0; 2]; 3];
            col = vec![[0.0; 4]; 3];
            idx = vec![0, 1, 2];
        }
        let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_indices(Indices::U32(idx))
    }

    fn some_blade_samples(n: usize, seed: u32) -> Vec<(Vec3, Vec3)> {
        (0..n).map(|i| (Vec3::new(hash01(seed, i as u32 * 6), hash01(seed, i as u32 * 6 + 1), hash01(seed, i as u32 * 6 + 2)) * 10.0, Vec3::new(hash01(seed, i as u32 * 6 + 3), hash01(seed, i as u32 * 6 + 4) + 2.0, hash01(seed, i as u32 * 6 + 5)) * 10.0)).collect()
    }

    #[test]
    fn blades_rewritten_in_place_match_the_old_builder() {
        // One mesh refilled through a sequence of sample sets (growing, then shrinking), each compared with
        // a fresh old-style build.
        let mut meshes = Assets::<Mesh>::default();
        let h = meshes.add(legacy_blade_mesh(&[(Vec3::ZERO, Vec3::ZERO); 2], 1));
        for (k, (n, steps)) in [(2, 1), (3, 4), (6, 4), (9, 8), (4, 2), (2, 4)].into_iter().enumerate() {
            let samples = some_blade_samples(n, k as u32 + 1);
            mesh_reuse::rewrite(&mut meshes, &h, |s| blade_fill(&samples, steps, s));
            assert!(mesh_reuse::same_mesh(meshes.get(&h).unwrap(), &legacy_blade_mesh(&samples, steps)), "{n} samples, {steps} steps");
            assert!(mesh_reuse::same_mesh(&blade_mesh(&samples, steps), &legacy_blade_mesh(&samples, steps)));
        }
    }

    #[test]
    fn flares_rewritten_in_place_match_the_old_builder() {
        let f = Flare { model: 0, texture: String::new(), start: 0.0, end: 0.0, colors: [0x80ff4020, 0x10204080], count: 1, life: 6.0, fade: 2.0, length: [1.0, -3.0], width: [0.5, 2.0], random: [false; 2], spin: 900, joint: None, offset: Vec3::ZERO };
        let ray = |i: u32, age: f32| FlareRay { c: Vec3::splat(i as f32), dir: cone_dir(7, i, 180.0), across: cone_dir(9, i, 180.0), age, length: [1.0 + i as f32, -2.0], width: [0.3, 1.0 + i as f32] };
        let mut meshes = Assets::<Mesh>::default();
        let h = meshes.add(legacy_flare_mesh(&f, &[]));
        let sets: Vec<Vec<FlareRay>> = vec![vec![ray(0, 0.0)], (0..5).map(|i| ray(i, i as f32 * 0.7)).collect(), vec![], (0..3).map(|i| ray(i + 3, 5.5)).collect(), vec![]];
        for rays in &sets {
            mesh_reuse::rewrite(&mut meshes, &h, |s| flare_fill(&f, rays, s));
            assert!(mesh_reuse::same_mesh(meshes.get(&h).unwrap(), &legacy_flare_mesh(&f, rays)), "{} rays", rays.len());
            assert!(mesh_reuse::same_mesh(&flare_mesh(&f, rays), &legacy_flare_mesh(&f, rays)));
        }
    }

    /// A particle's `FxMaterial` takes `fx_gamma.wgsl`'s mode 0 without the gamma re-encoding (Bevy's
    /// unlit `StandardMaterial` fragment), keeps every `StandardMaterial` field, sets no blend of its
    /// own, and its row starts with the colour the old material had.
    #[test]
    fn a_particle_material_draws_like_its_standard_material() {
        let base = StandardMaterial { base_color: Color::linear_rgba(0.4, 0.3, 0.2, 0.5), unlit: true, alpha_mode: AlphaMode::Blend, double_sided: true, cull_mode: None, ..default() };
        let m = particle_material(base.clone());
        assert_eq!(m.extension.params.x, 1.0, "no gamma re-encoding");
        assert_eq!(m.extension.params.y, 0.0, "mode 0: the StandardMaterial path");
        assert!(m.extension.blend.is_none() && m.extension.scene == Vec4::ZERO && m.extension.quad == Vec4::ONE);
        assert_eq!(format!("{:?}", m.base), format!("{base:?}"));
        assert_eq!(inst::FxInst::of(&m).color, Vec4::new(0.4, 0.3, 0.2, 0.5));
        assert_eq!(inst::neutral(m).base.base_color, Color::WHITE);
    }

    #[test]
    fn the_factored_material_recipes_make_what_the_inline_ones_made() {
        // The literals as they stood inline in spawn_trails / emit_points / tick_sprites before warmup.rs.
        let mut images = Assets::<Image>::default();
        let t = images.add(Image::default());
        let dbg = |m: &FxMaterial| format!("{m:?}");
        let plain = fx_material(StandardMaterial { base_color_texture: Some(t.clone()), unlit: true, alpha_mode: AlphaMode::Add, double_sided: true, cull_mode: None, ..default() });
        assert_eq!(dbg(&plain_add_material(t.clone())), dbg(&plain));
        let c = Vec4::new(0.9, 0.4, 0.2, 0.7);
        for billboard in [false, true] {
            let mut m = fx_material(StandardMaterial { base_color: Color::srgba(c.x, c.y, c.z, c.w), base_color_texture: Some(t.clone()), unlit: true, alpha_mode: AlphaMode::Add, double_sided: true, cull_mode: None, ..default() });
            if billboard {
                m.extension.piece = Vec4::new(1.0, 0.0, 0.0, 1.0);
            }
            assert_eq!(dbg(&point_material(c, t.clone(), billboard)), dbg(&m));
        }
        for translucent in [false, true] {
            let m = fx_material_gamma(
                StandardMaterial {
                    base_color: Color::srgba(c.x, c.y, c.z, c.w),
                    base_color_texture: Some(t.clone()),
                    unlit: true,
                    alpha_mode: if translucent { AlphaMode::Mask(0.5) } else { AlphaMode::Add },
                    double_sided: !translucent,
                    cull_mode: if translucent { Some(Face::Back) } else { None },
                    ..default()
                },
                1.0,
            );
            assert_eq!(dbg(&puff_material(c, t.clone(), translucent)), dbg(&m));
        }
    }


    #[test]
    fn an_environment_switch_is_read_once_per_run() {
        std::env::set_var("PD_FXPERF_TEST_ONCE", "7");
        assert_eq!(env_once("PD_FXPERF_TEST_ONCE"), Some("7"));
        // Later changes are not seen (nothing in the game sets these; tests that toggle a switch do not use it).
        std::env::set_var("PD_FXPERF_TEST_ONCE", "8");
        assert_eq!(env_once("PD_FXPERF_TEST_ONCE"), Some("7"));
        assert_eq!(env_once("PD_FXPERF_TEST_UNSET_SWITCH"), None);
        std::env::remove_var("PD_FXPERF_TEST_ONCE");
    }


    #[test]
    fn reflective_surfaces_are_drawn_not_hidden() {
        // Muramasa's blade (YKREFRECT00) and the mines' shells (PDEFTOPT06).
        let blade = def(&[("m0_d0", 0, "YKREFRECT00"), ("m1_d0", 1, "PDEFTOPT06"), ("m1_d1", 1, "PDEFTOPT03")], &[]);
        assert!(blade.shows(0, false) && blade.shows(1, true));
        assert_eq!(blade.census(1), [1, 1, 0, 0]);
        assert!(blade.pieces["m1_d0"].solid && !blade.pieces["m1_d1"].solid);
    }

    // ---- Lane FX-B: the piece timeline (audits/P0-EFFECTS.md D3, D6, D7, D8, D13) ----

    /// A parsed piece with raw words set, window `start ..= end`, colours white -> clear.
    fn timed(words: &[(usize, f32)], start: f32, end: f32) -> Piece {
        let mut j = piece_json("m0_d0", "PDEFTBB01", words, Some((0x11, 0x1100)));
        j["start"] = start.into();
        j["end"] = end.into();
        def_of(vec![j]).pieces["m0_d0"].clone()
    }

    #[test]
    fn aura_wisps_spawn_about_once_per_word_24_frames_and_live_26_to_30() {
        // eft0600 model 2: words 24 = 35, 25 = 360, 26 = 25, 27 = 5.
        let spec = WispSpec { rate: 35, life: [25.0, 5.0], roll: 360.0 };
        let c = spec.chance();
        assert!((c - 1.0 / 35.0).abs() < 0.001, "chance {c}");
        assert_eq!(WispSpec { rate: 1, ..spec }.chance(), 0.0, "(r * 1) >> 15 is never 1");
        assert_eq!(WispSpec { rate: 0, ..spec }.chance(), 0.0);
        let mut w = FxWisp { seed: 7, last: 0, active: false, spawned: 0, life: 0.0, grow: Vec3::ZERO, roll: 0.0, count: 0 };
        let (mut alive, mut spawns, mut was) = (0u32, 0u32, false);
        for f in 1..=4000 {
            let on = w.step(&spec, Vec3::splat(3.0), f as f32).is_some();
            alive += u32::from(on);
            spawns += u32::from(on && !was);
            was = on;
            if on {
                assert!((25.0..=30.0).contains(&w.life) && w.roll <= std::f32::consts::TAU + 1e-4);
                assert!(w.grow.x >= 1.5 && w.grow.x <= 3.0, "each grow axis 0.5-1 of the word");
            }
        }
        // One wisp per node: 4000 frames hold about 4000 / (35 + 27) of them, each ~27 long.
        assert!((50..80).contains(&spawns), "{spawns} wisps");
        assert!((alive as f32 / 4000.0 - 0.43).abs() < 0.08, "alive {}", alive as f32 / 4000.0);
        // Its first drawn frame is 1 and it is idle (None) until a spawn.
        let mut w2 = FxWisp { seed: 3, last: 0, active: false, spawned: 0, life: 0.0, grow: Vec3::ZERO, roll: 0.0, count: 0 };
        let first = (1..2000).find_map(|f| w2.step(&spec, Vec3::ONE, f as f32 + 0.25));
        assert_eq!(first, Some(1.25));
    }

    #[test]
    fn the_heat_haze_strength_words_run_over_the_curve() {
        // eft0097's PDEFTOPT05 pieces: words 24 / 26 = 0.5, 25 / 27 = 8 -> bump 8 / 512 = 1/64.
        let d = DistSpec { normal: [0.5, 0.5], bump: [8.0, 8.0] };
        assert_eq!(d.at(0.0), (1.0 / 64.0, 0.5));
        assert_eq!(d.at(1.0), (1.0 / 64.0, 0.5));
        // Words 25 / 27 = 4 -> 20 lerp over the curve, /512.
        let d = DistSpec { normal: [0.0, 1.0], bump: [4.0, 20.0] };
        let (b, n) = d.at(0.5);
        assert!((b - 12.0 / 512.0).abs() < 1e-7 && (n - 0.5).abs() < 1e-7);
        assert_eq!(shader_family("PDEFTOPT05", false), FAM_DIST);
        assert_eq!(shader_family("PDEFTOPT03", false), FAM_DIST);
        assert_eq!(shader_family("PDEFTOPT06", false), FAM_PLAIN, "the mine shells stay on the plain path");
    }

    #[test]
    fn the_piece_window_includes_its_last_frame() {
        // `FUN_140188fd0`: `start <= frame && frame <= end` on whole frames.
        assert!(!in_window(4.9, 5.0, 20.0));
        assert!(in_window(5.0, 5.0, 20.0));
        assert!(in_window(20.0, 5.0, 20.0), "the end frame is drawn");
        assert!(in_window(20.99, 5.0, 20.0), "a smooth clock stays on it for the whole tick");
        assert!(!in_window(21.0, 5.0, 20.0));
    }

    #[test]
    fn a_per_tick_flipbook_runs_one_cell_per_tick_and_wraps() {
        // Word 21 = 0 (`FUN_1401a5f20`): cell = (frame - start) mod cells, whole ticks.
        // A 4 x 2 lightning sheet (Psycho Wave's) drawn from frame 18.
        let cell = |frame| flipbook_cell(frame, 18.0, 30.0, 8.0, false);
        assert_eq!(cell(18.0), (0.0, 0.0));
        assert_eq!(cell(19.0), (1.0, 0.0));
        assert_eq!(cell(25.0), (7.0, 0.0));
        assert_eq!(cell(26.0), (0.0, 0.0), "wraps after the last cell");
        assert_eq!(cell(30.0), (4.0, 0.0));
        assert_eq!(cell(19.7), (1.0, 0.0), "a smooth clock holds the cell for the whole tick");
        // The cell's place on the sheet: column = cell mod 4, row = cell / 4 (`cell_uv`).
        assert_eq!(cell_uv(5.0, [4.0, 2.0], Vec2::ZERO), Vec4::new(0.25, 0.5, 0.25, 0.5));
    }

    #[test]
    fn a_stretched_flipbook_spreads_its_cells_over_the_window() {
        // Word 21 != 0: cell = trunc((frame - start) / d * cells), d = end - start + 1;
        // the remainder is the next cell's cross-fade weight (`PDEFTBB02`).
        let (cell, frac) = flipbook_cell(14.0, 10.0, 17.0, 16.0, true); // d = 8, e = 4: 4 / 8 * 16 = 8
        assert_eq!((cell, frac), (8.0, 0.0));
        let (cell, frac) = flipbook_cell(14.25, 10.0, 17.0, 16.0, true); // 4.25 / 8 * 16 = 8.5
        assert_eq!((cell, frac), (8.0, 0.5));
        // The last frame of the window does not reach the last cell's end: (d - 1) / d.
        assert_eq!(flipbook_cell(17.0, 10.0, 17.0, 16.0, true).0, 14.0);
    }

    #[test]
    fn uv_scroll_is_words_22_23_times_the_move_curve() {
        // `FUN_1401a5ff0`: constant 0x22 = (w22, w23) x move curve, added after the flipbook scale.
        let scroll = Vec2::new(0.5, -0.25);
        let at = |k: f32| cell_uv(0.0, [1.0, 1.0], scroll * k);
        assert_eq!(at(0.0), Vec4::new(1.0, 1.0, 0.0, 0.0));
        assert_eq!(at(1.0), Vec4::new(1.0, 1.0, 0.5, -0.25));
        // With a flipbook the scroll is in texture units, not cells.
        assert_eq!(cell_uv(1.0, [2.0, 2.0], Vec2::new(0.1, 0.0)), Vec4::new(0.5, 0.5, 0.6, 0.0));
        // Parsed from the words, and the move curve (word 8) drives it.
        let p = timed(&[(22, 0.5), (23, -0.25), (8, 1.0)], 0.0, 9.0);
        assert_eq!(p.scroll, Vec2::new(0.5, -0.25));
        // d = 10, e = 5, curve 1 (slow start): 5 x 6 / (10 x 11).
        let k = curve(p.curves[0], 5.0, 10.0);
        assert!((k - 30.0 / 110.0).abs() < 1e-6);
    }

    #[test]
    fn the_colour_flickers_on_odd_frames_when_word_30_is_set() {
        // `FUN_1401a9ae0`: word 30 != 0 and an odd whole frame -> the pair (14, 15), else (12, 13).
        let mut p = timed(&[(30, 1.0)], 0.0, 9.0);
        p.colors = [0xFFFF0000, 0xFFFF0000];
        p.colors_odd = [0xFF0000FF, 0xFF0000FF];
        assert_eq!(piece_colour(&p, 2.0), argb(0xFFFF0000));
        assert_eq!(piece_colour(&p, 3.0), argb(0xFF0000FF));
        assert_eq!(piece_colour(&p, 3.9), argb(0xFF0000FF), "parity of the whole tick");
        p.flicker = false;
        assert_eq!(piece_colour(&p, 3.0), argb(0xFFFF0000));
    }

    #[test]
    fn the_colour_fades_in_holds_and_fades_out_over_word_31_frames() {
        // Word 31 = 4 on frames 10 ..= 29: start -> end over the first 4 frames, the end colour
        // held, then back over the last 4 (`FUN_1401a9ae0`, ease swapped on the way out).
        let mut p = timed(&[(31, 4.0)], 10.0, 29.0);
        p.colors = [0xFF000000, 0xFFFFFFFF];
        let lum = |p: &Piece, frame: f32| piece_colour(p, frame).x;
        assert_eq!(lum(&p, 10.0), 0.0);
        assert!((lum(&p, 12.0) - 0.5).abs() < 1e-6, "linear: e / w");
        assert_eq!(lum(&p, 14.0), 1.0);
        assert_eq!(lum(&p, 20.0), 1.0, "held");
        assert_eq!(lum(&p, 25.0), 1.0);
        assert!((lum(&p, 27.0) - 0.5).abs() < 1e-6, "(end - frame) / w on the way out");
        assert_eq!(lum(&p, 29.0), 0.0);
        // Eased ramps swap their ease on the way out: slow start in, fast start out (curve 1 <-> 2).
        p.curves[2] = 1;
        let k_in = curve(1, 2.0, 4.0);
        assert!((lum(&p, 12.0) - k_in).abs() < 1e-6);
        let k_out = curve(2, 2.0, 4.0);
        assert!((lum(&p, 27.0) - k_out).abs() < 1e-6);
        assert!(k_out > k_in);
    }

    #[test]
    fn word_28_latches_the_pieces_matrix_in_world_space() {
        let base = Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)).with_scale(Vec3::splat(2.0)).to_matrix();
        let anim = base * Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)) * Mat4::from_scale(Vec3::splat(1.5));
        // The hand (the piece's parent) where it was at the first drawn frame, and later.
        let first = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let later = Mat4::from_rotation_y(1.0) * Mat4::from_translation(Vec3::new(40.0, 3.0, 7.0));
        // Mode 1: the world matrix stays at `first x anim` however the parent moved.
        assert_eq!(latched_world(1, first, later, base, anim), first * anim);
        assert_eq!(latched_world(1, first, first, base, anim), first * anim);
        // Mode 2: the latched position (`first x base`), world-aligned axes, the live scale; the
        // piece's own travel / grow still apply in its own frame (`base^-1 x anim`).
        let w = latched_world(2, first, later, base, anim);
        let latched_pos = (first * base).w_axis.truncate();
        assert!((w.transform_point3(Vec3::ZERO) - (latched_pos + Vec3::new(1.0 * 2.0, 0.0, 0.0))).length() < 1e-4, "origin = latched position + travel along the world x axis, scaled");
        assert!((w.x_axis.truncate().length() - 2.0 * 1.5).abs() < 1e-4, "live scale x grow");
        assert!(w.x_axis.truncate().normalize().abs_diff_eq(Vec3::X, 1e-4), "axes stay world-aligned, however the hand turned");
        // The parent turning does not move a mode-2 piece either.
        let w2 = latched_world(2, first, Mat4::from_translation(Vec3::new(-9.0, 9.0, 9.0)) * later, base, anim);
        assert!(w2.w_axis.abs_diff_eq(w.w_axis, 1e-4));
    }

    #[test]
    fn a_piece_reads_its_timeline_words() {
        let p = timed(&[(28, 1.0), (21, 1.0), (22, 0.25), (23, 0.5), (14, 0.0), (30, 1.0), (31, 7.0), (20, 1.0)], 3.0, 12.0);
        assert_eq!((p.latch, p.stretch, p.scroll, p.flicker, p.fade_win, p.cell_wrap), (1, true, Vec2::new(0.25, 0.5), true, 7.0, true));
        // Word 21 = 0.5 truncates to 0 (`(int)`): a per-tick flipbook.
        assert!(!timed(&[(21, 0.5)], 0.0, 5.0).stretch);
        // Modes 3-8 are not implemented: they play as 0.
        assert_eq!(timed(&[(28, 2.0)], 0.0, 5.0).latch, 2);
        assert_eq!(timed(&[(28, 6.0)], 0.0, 5.0).latch, 0);
        assert_eq!(timed(&[], 0.0, 5.0).latch, 0);
    }

    #[test]
    fn the_shader_family_picks_the_pixel_stage() {
        assert_eq!(shader_family("PDEFTBB01", false), FAM_PLAIN);
        assert_eq!(shader_family("PDEFTBB02", false), FAM_PLAIN, "a per-tick PDEFTBB02 has weight 0 on its second cell");
        assert_eq!(shader_family("PDEFTBB02", true), FAM_BB02);
        assert_eq!(shader_family("PDMTEX01", false), FAM_MTEX);
        assert_eq!(shader_family("PDMTEX02", false), FAM_MTEX);
        assert_eq!(shader_family("PDEFTAURA01", false), FAM_AURA);
        assert_eq!(shader_family("PDEFTBB08", false), FAM_BB08);
        assert_eq!(shader_family("MSEFTBB01S", false), FAM_PLAIN);
        // A family piece reads its four texture slots from the effect's exported PNGs.
        let mut j = piece_json("m0_d0", "PDMTEX01", &[], Some((0x11, 0x3000)));
        j["mat_tex"] = serde_json::json!([2, 3, 0, 0]);
        let d = def_of(vec![j]);
        let p = &d.pieces["m0_d0"];
        assert_eq!(p.family, FAM_MTEX);
        assert_eq!(p.textures[0].as_deref(), Some("fx/eft9999_t2.png"));
        assert_eq!(p.textures[1].as_deref(), Some("fx/eft9999_t3.png"));
        // No exported PNG for slot 0 (an old cache): back to the plain path.
        let mut j = piece_json("m0_d0", "PDMTEX01", &[], Some((0x11, 0x3000)));
        j["mat_tex"] = serde_json::json!([2, 3, 0, 0]);
        let none = FxDef::parse("eft9999", &serde_json::json!({ "slots": [[0], [], [], [], [], []], "pieces": [j], "emitters": [] }), |_| false).unwrap();
        assert_eq!(none.pieces["m0_d0"].family, FAM_PLAIN);
    }

    // The pixel stages as the disassembly (tools/disasm, 2026-10-01) writes them, on gamma-space
    // numbers; `fx_gamma.wgsl` runs the same lines per fragment.

    /// `PDMTEX01_ps` / `PDMTEX02_ps`: `add r0.xyz, t0, t1; mul r0.xyz, t1.w, r0; mov r0.w, 1;
    /// mul r0, colour, r0` -- rgb = colour x (t0 + t1) x t1.a, alpha = colour.a.
    fn mtex_rgb(col: [f32; 4], t0: [f32; 3], t1: [f32; 4]) -> [f32; 3] {
        std::array::from_fn(|i| (col[i] * (t0[i] + t1[i]) * t1[3]).min(1.0))
    }

    /// `PDEFTBB02_ps`: rgb = colour x (t0 x (1 - c1) + c1 x t1) -- `mul r0, t0, col; mul r1, t1, col;
    /// add r2, -c1, 1; mul r0, r0, r2; mad o0, c1, r1, r0`; c1 = the cell's fraction.
    fn bb02_rgb(col: [f32; 3], t0: [f32; 3], t1: [f32; 3], c1: f32) -> [f32; 3] {
        std::array::from_fn(|i| t0[i] * col[i] * (1.0 - c1) + c1 * t1[i] * col[i])
    }

    /// `PDEFTAURA01_ps`: `mul r0, t0, col; mul r0.xyz, r0.w, r0.xyz; mul o0.xyz, t1, r0.xyz`;
    /// alpha = t0.a x colour.a.
    fn aura_rgb(col: [f32; 4], t0: [f32; 4], t1: [f32; 3]) -> ([f32; 3], f32) {
        let a = t0[3] * col[3];
        (std::array::from_fn(|i| t1[i] * t0[i] * col[i] * a), a)
    }

    /// `PDEFTBB08_ps`: `mad o0.xyz, (t1 x col + env x col), t3, env x col`.
    fn bb08_rgb(col: [f32; 3], t1: [f32; 3], env: [f32; 3], t3: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| (t1[i] * col[i] + env[i] * col[i]) * t3[i] + env[i] * col[i])
    }

    #[test]
    fn the_family_pixel_stages_follow_their_disassembly() {
        // PDMTEX: both textures add, the second one's alpha scales the sum, the colour's alpha is the output's.
        let rgb = mtex_rgb([1.0, 0.5, 0.0, 0.8], [0.2, 0.2, 0.2], [0.3, 0.1, 0.9, 0.5]);
        assert!((rgb[0] - 0.25).abs() < 1e-6 && (rgb[1] - 0.5 * 0.3 * 0.5).abs() < 1e-6 && rgb[2] == 0.0);
        assert_eq!(mtex_rgb([1.0; 4], [1.0; 3], [1.0; 4]), [1.0; 3], "saturates like the 8-bit target");
        // PDEFTBB02: fraction 0 = the first cell only; 1 = the second; halfway = their mean.
        let col = [1.0, 0.5, 0.25];
        let (a, b) = ([0.8, 0.8, 0.8], [0.2, 0.4, 0.6]);
        assert_eq!(bb02_rgb(col, a, b, 0.0), [0.8, 0.4, 0.2]);
        assert_eq!(bb02_rgb(col, a, b, 1.0), [0.2, 0.2, 0.15]);
        assert!((bb02_rgb(col, a, b, 0.5)[0] - 0.5).abs() < 1e-6);
        // PDEFTAURA01: tex1 modulates tex0 x colour x (tex0.a x colour.a); alpha is the product.
        let (rgb, alpha) = aura_rgb([1.0, 0.5, 0.5, 0.5], [0.8, 0.8, 0.8, 0.5], [0.5, 1.0, 1.0]);
        assert!((alpha - 0.25).abs() < 1e-6 && (rgb[0] - 0.5 * 0.8 * 1.0 * 0.25).abs() < 1e-6 && (rgb[1] - 0.8 * 0.5 * 0.25).abs() < 1e-6);
        // PDEFTBB08: (t1 + env) x col x t3 + env x col; with no environment it is t1 x col x t3.
        assert_eq!(bb08_rgb([0.5; 3], [0.5; 3], [0.0; 3], [0.5; 3]), [0.125; 3]);
        assert!((bb08_rgb([1.0; 3], [0.5; 3], [0.2; 3], [0.5; 3])[0] - ((0.5 + 0.2) * 0.5 + 0.2)).abs() < 1e-6);
    }
}
