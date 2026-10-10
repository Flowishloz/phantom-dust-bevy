//! The lock-on marker (the reticle floating on your target), drawn from the
//! game's own art: sprites of `hud1.png` (`Menu/BtMenuEn.alr`, texture 1).
//! Render only; reads the sim, never writes it. One marker per player view,
//! each on that player's own target.
//!
//! What the program does (`FUN_1401536e0` updates, `FUN_140153a10` draws; the
//! addresses and the sprite table are in docs/HUD.md "Lock-on marker"):
//!
//! * The marker is a 2D sprite group at the target's projected position. Its
//!   pieces are sprite records of the ALR's own table (the "index" below is
//!   the record number the program uses: its table of handles is read at
//!   `+0x268` of the HUD object, byte offset / 4). The rectangles are
//!   re-derived from the ALR's records, not from alpha scans.
//! * A **range band** picks the ring: 1 = within 80 u (one big ring), 2 = 80
//!   to 300 u (a medium ring and a small ring in its centre), 3 = beyond 300 u
//!   (a small crosshair). The same bands and the same 80 / 300 thresholds as
//!   the melee dash (`PROGRAM-RE.md`); Laurence's footage confirms them (the
//!   distance readout on screen is `(distance - 10) * 100`, and the colour
//!   flips at 80 and 300).
//! * A **new target** starts a 10-tick intro (counter 10 down to 1): the
//!   band's three arcs fly in from `2 * counter^2` px away (quadratic ease)
//!   round a fixed centre piece (the three-spoke "Y" icon for the far band).
//!   At 0 the settled marker takes over and the lock cue sounds (sfx.rs,
//!   untouched).
//! * A **band change** while locked runs a 5-tick crossfade (a ramp
//!   0.2, 0.4 .. 1.0 in steps of 0.2 per tick): the bigger of the two rings
//!   grows or shrinks about its centre.
//! * The settled marker is **static**: no spin, no pulse (the "pulse" in an
//!   earlier note was the clip's two lock/unlock cycles).
//! * Two more pieces follow the ring in every band: the dim segmented arc
//!   (sprite 14, 38x56, at (-41, -17)) and the bright sweeping arc, the
//!   "scythe" (sprite 77, 100x100 centred on the target point, at (-50, -48)),
//!   which the program draws ROTATED about its centre by an angle that tracks
//!   the distance: `gauge * 1.8125 - 116` degrees, with `gauge` 0..64 from the
//!   distance within the band (the same value that sizes the readout's
//!   underline). The marker's own ring never turns.
//! * Sizes are in the program's 480-high virtual screen, i.e. `view_height /
//!   480` px per sprite px, in a full view and in a split view alike (the same
//!   figure measured on both of Laurence's recordings: 1P 3.4 = 1640 / 480,
//!   2P 1.6 = 772 / 480).
//!
//! What the program's code does NOT tell us, so it comes from the footage
//! (measured with the atlas pieces fitted over 1P and 2P recordings, see
//! docs/HUD.md): the colour per band (the art is grey; the program's vertex
//! colour could not be traced) and the opacity. Both are named constants
//! below.

use super::HudScale;
use crate::combat::{CHEST, MAX_PLAYERS};
use crate::frontend::ui::{self, MenuArt, PixelText};
use crate::{render_pose, PlayerCamera, World};
use bevy::prelude::*;

/// Footage puts the reticle on the target's upper body (about 70 % of its
/// height); `BODY_HEIGHT` is 20 u. A render anchor, not sim state.
pub const LOCK_TARGET_HEIGHT: f32 = CHEST * 1.15;
/// Peak opacity of every marker sprite over the scene (alpha multiplied into
/// the art's own soft alpha). Measured on Laurence's recordings: a ring pixel
/// over a dark background rises by 0.30-0.34 of the band colour, on 1P and 2P
/// clips and in all three bands. The old marker was drawn at full strength.
pub const LOCK_OPACITY: f32 = 0.32;
/// The program blends in display (gamma) space; Bevy's UI blends in linear
/// light, where the same alpha reads far brighter over a dark scene. This turns
/// the measured opacity into the linear alpha that lands on the same screen
/// values over the dim-to-mid scenery the footage was measured on.
pub fn linear_alpha(opacity: f32) -> f32 {
    opacity.powf(1.7)
}
/// Extra alpha for pieces that are only one or two art pixels wide. On the
/// recordings they read as strong as, or stronger than, the rings: the Y icon's
/// strokes rise 1.7-2x as far above the background as a ring's, the small
/// centre rings and flying arcs about 1.5x, the far crosshair about 1.25x
/// (measured on the same frames, stroke pixels over local background).
pub fn alpha_boost(id: u16) -> f32 {
    match id {
        28 | 133 => 2.4,
        16 | 17 | 19 | 21 | 22 | 23 | 24 | 26 | 27 | 29 | 121 | 122 | 124 | 126 | 127 | 128 | 129 | 131 | 132 | 134 => 1.8,
        25 | 130 => 1.25,
        _ => 1.0,
    }
}
/// Band colours (sRGB 0..1), the colour the sprite is tinted before the
/// opacity above. Far and medium are fitted to the footage (ring colour on dark
/// backgrounds, with the opacity); close (orange) from the 2P Psycho Knife
/// recording. The program's own vertex colours were not traced: unconfirmed.
pub const COLOUR_CLOSE: [f32; 3] = [0.85, 0.48, 0.28];
pub const COLOUR_MEDIUM: [f32; 3] = [1.00, 0.92, 0.24];
pub const COLOUR_FAR: [f32; 3] = [0.73, 1.00, 0.33];
/// The converted HUD art is dark grey (about 80 / 255 at full alpha in all the
/// lock pieces), so a plain tint would draw it at a third of the strength. The
/// sprites are multiplied by `band colour / this grey` (linear light), which is
/// what makes the band colour land on screen as measured.
pub const ART_GREY: f32 = 0.32;
/// Band thresholds in sim units (`PROGRAM-RE.md`, lock section; footage: a
/// readout of 70.3 u was orange, 90.2 u yellow, 317 u green).
pub const RANGE_MEDIUM: f32 = 80.0;
pub const RANGE_FAR: f32 = 300.0;
/// The program's virtual screen height every sprite size is relative to.
pub const VIRTUAL_HEIGHT: f32 = 480.0;
/// Ticks of the intro and of the band-change ramp (30 Hz).
pub const INTRO_TICKS: u32 = 10;
pub const RAMP_START: f32 = 0.2;
pub const RAMP_STEP: f32 = 0.2;
pub const TICK_RATE: f64 = 30.0;
/// The original prints the target's name (and a distance readout) beside the
/// marker in a full view only; in the side-by-side split views of Laurence's
/// recording neither appears. Set true to keep the name in split views.
pub const NAME_IN_SPLIT: bool = false;

const SLOTS: usize = 8;
/// +1 or -1: which way a positive `Rot2` turns a UI node relative to the
/// program's angle (the program's negative angles turn the scythe counter-clockwise on screen).
const SCREEN_TURN: f32 = 1.0;

/// Atlas rectangles (x, y, w, h) of the sprite records used, in hud1.png.
/// Index = the program's sprite record number.
const RECTS: [(u16, f32, f32, f32, f32); 30] = [
    (14, 102.0, 142.0, 38.0, 56.0),
    (15, 0.0, 256.0, 98.0, 98.0),
    (16, 0.0, 356.0, 82.0, 30.0),
    (17, 102.0, 52.0, 46.0, 70.0),
    (19, 438.0, 136.0, 48.0, 70.0),
    (20, 438.0, 0.0, 60.0, 60.0),
    (21, 438.0, 116.0, 50.0, 18.0),
    (22, 102.0, 438.0, 28.0, 42.0),
    (23, 46.0, 495.0, 14.0, 14.0),
    (24, 438.0, 208.0, 28.0, 42.0),
    (25, 102.0, 314.0, 32.0, 32.0),
    (26, 24.0, 495.0, 20.0, 10.0),
    (27, 84.0, 412.0, 12.0, 18.0),
    (28, 102.0, 348.0, 32.0, 28.0),
    (29, 84.0, 356.0, 14.0, 18.0),
    (120, 0.0, 388.0, 76.0, 76.0),
    (121, 0.0, 469.0, 63.0, 24.0),
    (122, 102.0, 258.0, 36.0, 54.0),
    (124, 102.0, 200.0, 38.0, 56.0),
    (125, 438.0, 62.0, 52.0, 52.0),
    (126, 102.0, 124.0, 44.0, 16.0),
    (127, 438.0, 336.0, 24.0, 36.0),
    (128, 84.0, 376.0, 14.0, 14.0),
    (129, 438.0, 296.0, 26.0, 38.0),
    (77, 0.0, 52.0, 100.0, 100.0),
    (130, 102.0, 378.0, 32.0, 30.0),
    (131, 0.0, 495.0, 22.0, 8.0),
    (132, 84.0, 432.0, 12.0, 17.0),
    (133, 102.0, 410.0, 30.0, 26.0),
    (134, 84.0, 392.0, 14.0, 18.0),
];

/// The dim segmented arc and the bright rotating scythe, drawn in every band.
const ARC: u16 = 14;
const ARC_AT: (f32, f32) = (-41.0, -17.0);
const SCYTHE: u16 = 77;
const SCYTHE_AT: (f32, f32) = (-50.0, -48.0);

/// The scythe's rotation (degrees) for a target `distance` u away, read from
/// `FUN_140153a10` (0x140154b12): the readout value is `distance - 10`
/// (zero up to 0.9); per band `a = (v / range) * span + base` (close 270 / 130 /
/// 50, medium 500 / 105 / 75, far 800 / 70 / 110), gauge `g = (int(a * 0.5) * 2
/// - 50) / 2` clamped to 0..=64, and the angle `g * 1.8125 - 116`.
pub fn scythe_angle(band: Band, distance: f32) -> f32 {
    gauge_raw(band, distance).clamp(0, 64) as f32 * 1.8125 - 116.0
}

/// The gauge before the clamp (`FUN_140153a10`, 0x1401545a2..0x1401545c8): the
/// scythe uses it clamped to 0..=64, the readout also needs to know when it
/// ran past either end.
fn gauge_raw(band: Band, distance: f32) -> i32 {
    let mut v = distance - 10.0;
    if v <= 0.9 {
        v = 0.0;
    }
    let a = match band {
        Band::Close => ((v / 270.0) * 130.0) as i32 + 50,
        Band::Medium => ((v / 500.0) * 105.0) as i32 + 75,
        Band::Far => ((v / 800.0) * 70.0) as i32 + 110,
    };
    ((a as f32) * 0.5) as i32 - 25
}

pub fn rect_of(id: u16) -> (f32, f32, f32, f32) {
    RECTS.iter().find(|r| r.0 == id).map(|r| (r.1, r.2, r.3, r.4)).unwrap_or((0.0, 0.0, 0.0, 0.0))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Band {
    Close,
    Medium,
    Far,
}

/// The program has two sprite sets: one for a full view (one player) and a
/// smaller one for the split views.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Set {
    Full,
    Split,
}

impl Set {
    pub fn for_players(n: usize) -> Set {
        if n <= 1 {
            Set::Full
        } else {
            Set::Split
        }
    }
}

pub fn band_of(distance: f32) -> Band {
    if distance < RANGE_MEDIUM {
        Band::Close
    } else if distance <= RANGE_FAR {
        Band::Medium
    } else {
        Band::Far
    }
}

fn rgb(band: Band) -> [f32; 3] {
    match band {
        Band::Close => COLOUR_CLOSE,
        Band::Medium => COLOUR_MEDIUM,
        Band::Far => COLOUR_FAR,
    }
}

/// Sprite tint: the band colour (blended from the previous band's while the
/// ramp runs) and the marker's opacity as a linear-light alpha.
pub fn tint(band: Band, prev: Option<Band>, ramp: f32) -> [f32; 4] {
    let a = rgb(band);
    let c = match prev {
        Some(p) if ramp < 1.0 => {
            let b = rgb(p);
            [b[0] + (a[0] - b[0]) * ramp, b[1] + (a[1] - b[1]) * ramp, b[2] + (a[2] - b[2]) * ramp]
        }
        _ => a,
    };
    [c[0], c[1], c[2], linear_alpha(LOCK_OPACITY)]
}

/// The colour the UI image multiplies the (dark grey) art by: the measured band
/// colour divided by the art's grey, in linear light, at the marker's opacity.
pub fn image_colour(t: [f32; 4]) -> Color {
    let grey = Color::srgb(ART_GREY, ART_GREY, ART_GREY).to_linear().red;
    let c = |v: f32| Color::srgb(v, v, v).to_linear().red / grey;
    Color::linear_rgba(c(t[0]), c(t[1]), c(t[2]), t[3])
}

/// Ticks of intro left: 10 on the tick the lock lands, down to 1; 0 = settled.
pub fn intro_counter(age_secs: f64) -> u32 {
    let ticks = (age_secs.max(0.0) * TICK_RATE).floor() as u32;
    INTRO_TICKS.saturating_sub(ticks)
}

/// Band-change ramp after `secs` seconds: 0.2 on the tick of the change, then
/// +0.2 a tick, 1.0 (settled) from the fifth tick.
pub fn ramp_after(secs: f64) -> f32 {
    let n = (secs.max(0.0) * TICK_RATE).floor() as u32;
    (RAMP_START + RAMP_STEP * n as f32).min(1.0)
}

/// One sprite to draw: record id, top-left (x, y) in sprite px from the target
/// point, and a scale about the sprite's own centre.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Draw {
    pub id: u16,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    /// The program's rotation angle for this sprite in degrees (negative turns
    /// it counter-clockwise on screen); 0 for everything but the scythe.
    pub angle: f32,
}

fn d(id: u16, x: f32, y: f32) -> Draw {
    Draw { id, x, y, scale: 1.0, angle: 0.0 }
}

/// The settled pieces of a band (before the scythe), in draw order.
pub fn settled(set: Set, band: Band) -> Vec<Draw> {
    match (set, band) {
        (Set::Full, Band::Close) => vec![d(15, -49.0, -48.0)],
        (Set::Full, Band::Medium) => vec![d(20, -31.0, -30.0), d(23, -7.0, -7.0)],
        (Set::Full, Band::Far) => vec![d(25, -16.0, -13.0)],
        (Set::Split, Band::Close) => vec![d(120, -40.0, -36.0)],
        (Set::Split, Band::Medium) => vec![d(125, -30.0, -26.0), d(128, -10.0, -5.0)],
        (Set::Split, Band::Far) => vec![d(130, -18.0, -11.0)],
    }
}

/// The ring that grows or shrinks on a band change (its id, size in sprite px).
fn ring(set: Set, band: Band) -> Draw {
    settled(set, band)[0]
}

fn ring_size(set: Set, band: Band) -> f32 {
    rect_of(ring(set, band).id).2
}

/// The intro pieces for a band at counter `c` (1..=10): arcs flying in from
/// `2 c^2` px round the centre piece. Read from `FUN_140153a10`.
pub fn intro(set: Set, band: Band, c: u32) -> Vec<Draw> {
    let e = 2.0 * (c * c) as f32;
    match (set, band) {
        (Set::Full, Band::Close) => vec![d(16, -41.0, -e - 50.0), d(17, e + 3.0, e - 21.0), d(19, -e - 50.0, e - 21.0)],
        (Set::Full, Band::Medium) => vec![d(21, -24.0, -e - 30.0), d(22, e + 2.0, e - 12.0), d(23, -7.0, -7.0), d(24, -e - 28.0, e - 12.0)],
        (Set::Full, Band::Far) => vec![d(26, -10.0, -e - 16.0), d(27, e, e - 4.0), d(28, -16.0, -10.0), d(29, -e - 15.0, e - 4.0)],
        (Set::Split, Band::Close) => vec![d(121, -31.0, -e - 36.0), d(122, e + 2.0, e - 17.0), d(124, -e - 38.0, e - 18.0)],
        (Set::Split, Band::Medium) => vec![d(126, -24.0, -e - 26.0), d(127, e - 2.0, e - 11.0), d(128, -9.0, -5.0), d(129, -e - 26.0, e - 12.0)],
        (Set::Split, Band::Far) => vec![d(131, -10.0, -e - 12.0), d(132, e + 1.0, e - 5.0), d(133, -14.0, -9.0), d(134, -e - 13.0, e - 5.0)],
    }
}

/// Everything drawn for one marker this frame. `counter` > 0 plays the intro;
/// otherwise `prev` + `ramp` < 1 plays the band-change crossfade; otherwise
/// the settled marker. The two arcs join every state but the intro.
pub fn layout(set: Set, band: Band, prev: Option<Band>, counter: u32, ramp: f32, distance: f32) -> Vec<Draw> {
    if counter > 0 {
        return intro(set, band, counter.min(INTRO_TICKS));
    }
    let mut out = Vec::new();
    match prev {
        Some(p) if p != band && ramp < 1.0 => {
            if ring_size(set, band) > ring_size(set, p) {
                // Growing into a bigger ring over the old small one.
                out.extend(settled(set, p));
                let mut g = ring(set, band);
                g.scale = ramp;
                out.push(g);
            } else {
                // Shrinking: the old big ring closes in on the new one.
                out.extend(settled(set, band));
                let mut s = ring(set, p);
                s.scale = 1.0 - ramp;
                out.push(s);
            }
        }
        _ => out.extend(settled(set, band)),
    }
    out.push(d(ARC, ARC_AT.0, ARC_AT.1));
    out.push(Draw { angle: scythe_angle(band, distance), ..d(SCYTHE, SCYTHE_AT.0, SCYTHE_AT.1) });
    out
}

/// Screen size of one sprite px in a view `view_h` px tall.
pub fn px_per_sprite(view_h: f32) -> f32 {
    view_h / VIRTUAL_HEIGHT
}

/// Top-left and size in view px of a draw, for a target point `at`.
pub fn place(draw: Draw, at: Vec2, view_h: f32) -> (Vec2, Vec2) {
    let s = px_per_sprite(view_h);
    let (_, _, w, h) = rect_of(draw.id);
    let size = Vec2::new(w, h) * s * draw.scale;
    let centre = at + Vec2::new(draw.x + w * 0.5, draw.y + h * 0.5) * s;
    (centre - size * 0.5, size)
}


// ---- The distance readout (`FUN_140153a10`, 0x1401544fc..0x1401548dc) ----

/// The program's colour of the band (the readout, its underline, the name),
/// ARGB words read at 0x140154642 (close), 0x14015463b (medium), 0x140154634
/// (far): 0xffff7725, 0xffffd525, 0xffa5ff25. READ.
pub const READOUT_CLOSE: [u8; 3] = [0xff, 0x77, 0x25];
pub const READOUT_MEDIUM: [u8; 3] = [0xff, 0xd5, 0x25];
pub const READOUT_FAR: [u8; 3] = [0xa5, 0xff, 0x25];
/// Where the readout rides the gauge: 65 (x, y) byte pairs at 0x1402f1370,
/// indexed by `64 - gauge`. READ.
const READOUT_TAB: [(i8, i8); 65] = [
    (14, 7), (14, 7), (13, 9), (13, 11), (12, 12), (11, 14), (11, 16), (11, 18), (11, 19), (11, 21), (11, 23), (9, 25), (11, 26), (11, 28), (11, 30), (11, 31),
    (9, 33), (9, 35), (9, 37), (10, 39), (11, 40), (11, 42), (11, 44), (12, 45), (12, 47), (13, 48), (13, 50), (14, 52), (14, 54), (15, 56), (16, 57), (16, 58),
    (18, 59), (19, 62), (20, 63), (20, 64), (21, 65), (23, 44), (24, 46), (25, 48), (26, 48), (27, 49), (28, 50), (29, 52), (30, 54), (30, 49), (31, 51), (33, 52),
    (37, 56), (38, 56), (39, 58), (41, 59), (42, 59), (44, 60), (46, 61), (48, 61), (49, 61), (50, 62), (52, 62), (54, 62), (56, 62), (56, 63), (57, 63), (59, 63),
    (62, 63),
];
/// Glyph widths of ' '..='~' (0x1402f2890); the pen advances `width + 1`
/// (10 when that is not positive). READ.
const GLYPH_WIDTH: [i8; 96] = [
    6, 4, 7, 8, 8, 10, 8, 4, 6, 6, 8, 8, 4, 8, 4, 7, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 4, 4, 7, 8, 7, 8, 12, 8, 8, 8, 8, 8, 8, 8, 8, 5, 8, 8, 8, 11, 8, 9, 8, 8, 8, 8, 9, 8, 8, 11, 10,
    9, 8, 5, 8, 4, 8, 8, 4, 8, 8, 8, 8, 8, 8, 8, 8, 5, 7, 9, 5, 11, 8, 9, 8, 8, 8, 8, 8, 8, 8, 11, 8, 8, 8, 6, 4, 6, 8, 16,
];
/// Underline sprite (record 76 of the ALR table: hud1 texture, a 64 x 1 line).
const UNDERLINE: (f32, f32, f32, f32) = (0.0, 466.0, 64.0, 1.0);
/// Set true to print the readout in split views too. The program prints it
/// unless its view mode is 2 (`cmp [rsi+0x2a0], 2` at 0x14015464b) and
/// Laurence's side-by-side recording shows none, so this follows the
/// recording.
pub const READOUT_IN_SPLIT: bool = false;

/// The band's colour word.
pub fn readout_rgb(band: Band) -> [u8; 3] {
    match band {
        Band::Close => READOUT_CLOSE,
        Band::Medium => READOUT_MEDIUM,
        Band::Far => READOUT_FAR,
    }
}

/// The number the program prints: `sprintf("%5d", int((distance - 10) * 100))`
/// with the value zeroed up to 0.9 (0x1402f7d90).
pub fn readout_text(distance: f32) -> String {
    let mut v = distance - 10.0;
    if v <= 0.9 {
        v = 0.0;
    }
    format!("{:5}", (v * 100.0) as i32)
}

/// Pen advance of a character in the readout's font (virtual px).
pub fn glyph_advance(ch: char) -> f32 {
    let w = (ch as usize).checked_sub(0x20).and_then(|i| GLYPH_WIDTH.get(i)).map_or(0, |&w| w as i32) + 1;
    if w > 0 { w as f32 } else { 10.0 }
}

/// Top-left of the readout text and of its underline, in virtual px from the
/// marker's target point. The gauge picks a table row (`64 - g`); the text sits
/// `tab - gl - 50`, `tab - ly - 15` from the point and the underline
/// `tab - gl - 63`, `tab - ly` (`gl` 64, or 63 once the gauge reaches 64; `ly`
/// 7 below gauge 28, 29 from 28, 30 at 64). READ.
pub fn readout_places(band: Band, distance: f32) -> (Vec2, Vec2) {
    let raw = gauge_raw(band, distance);
    let g = raw.clamp(0, 64);
    let (tx, ty) = READOUT_TAB[(64 - g) as usize];
    let (gl, ly) = if raw >= 64 {
        (63.0, 30.0)
    } else if raw >= 28 {
        (64.0, 29.0)
    } else {
        (64.0, 7.0)
    };
    let (tx, ty) = (tx as f32, ty as f32);
    (Vec2::new(tx - gl - 50.0, ty - ly - 15.0), Vec2::new(tx - gl - 63.0, ty - ly))
}

/// The readout number, in the pixel font at the program's own advances with its
/// black shadow (one px right and down), rebuilt when the string, colour or
/// size changes. (The target's name beside it is `MarkerName`: the program draws
/// names in a different, smooth font that we do not have; docs/HUD.md.)
#[derive(Component)]
pub struct MarkerText {
    view: usize,
    built: Option<(String, [u8; 3], f32)>,
}

#[derive(Component)]
pub struct MarkerName(usize);

#[derive(Component)]
pub struct MarkerUnderline(usize);

fn spawn_readout_children(commands: &mut Commands, art: &MenuArt, e: Entity, text: &str, rgb: [u8; 3], s: f32) {
    let font = art.font_image();
    let colour = Color::srgb_u8(rgb[0], rgb[1], rgb[2]);
    let mut children = Vec::new();
    for (shade, off) in [(Color::BLACK, 1.0), (colour, 0.0)] {
        let mut pen = 0.0;
        for ch in text.chars() {
            if let (Some((cx, cy, ..)), true) = (art.glyph_ink(ch), ch != ' ') {
                let node = Node { position_type: PositionType::Absolute, left: Val::Px((pen + off) * s), top: Val::Px(off * s), width: Val::Px(ui::CELL * s), height: Val::Px(ui::CELL * s), ..default() };
                let img = ImageNode { image: font.clone(), rect: Some(Rect::new(cx, cy, cx + ui::CELL, cy + ui::CELL)), color: shade, image_mode: NodeImageMode::Stretch, ..default() };
                children.push(commands.spawn((node, img)).id());
            }
            pen += glyph_advance(ch);
        }
    }
    commands.entity(e).despawn_children().add_children(&children);
}

#[derive(Component)]
pub struct MarkerSprite {
    view: usize,
    slot: usize,
}

#[derive(Component)]

#[derive(Default, Clone, Copy)]
struct ViewState {
    since: Option<f64>,
    target: usize,
    band: Option<Band>,
    prev_band: Option<Band>,
    band_since: f64,
}

#[derive(Default)]
pub struct MarkerState {
    view: [ViewState; MAX_PLAYERS],
}

/// Spawn one view's marker pieces as children of that view's HUD root.
pub fn spawn(commands: &mut Commands, root: Entity, view: usize, atlas: &Handle<Image>, art: &MenuArt) {
    for slot in 0..SLOTS {
        let id = commands
            .spawn((
                MarkerSprite { view, slot },
                Node { position_type: PositionType::Absolute, ..default() },
                UiTransform::default(),
                ImageNode { image: atlas.clone(), rect: Some(Rect::new(0.0, 0.0, 1.0, 1.0)), color: Color::NONE, ..default() },
                Visibility::Hidden,
            ))
            .id();
        commands.entity(root).add_child(id);
    }
    let (x, y, w, h) = UNDERLINE;
    let line = commands
        .spawn((
            MarkerUnderline(view),
            Node { position_type: PositionType::Absolute, ..default() },
            ImageNode { image: atlas.clone(), rect: Some(Rect::new(x, y, x + w, y + h)), color: Color::WHITE, image_mode: NodeImageMode::Stretch, ..default() },
            Visibility::Hidden,
        ))
        .id();
    commands.entity(root).add_child(line);
    let text = commands.spawn((MarkerText { view, built: None }, Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden)).id();
    commands.entity(root).add_child(text);
    let mut name = commands.spawn((MarkerName(view), Visibility::Hidden));
    ui::glyphs_on(&mut name, art, "", Color::WHITE, ui::pixel_scale(14.0));
    let id = name.id();
    commands.entity(root).add_child(id);
}

fn env_tick() -> Option<u32> {
    std::env::var("PD_TEST_LOCK_MARKER_TICK").ok().and_then(|v| v.parse().ok())
}

fn env_band() -> Option<Band> {
    match std::env::var("PD_TEST_LOCK_MARKER_BAND").ok()?.as_str() {
        "close" => Some(Band::Close),
        "medium" => Some(Band::Medium),
        "far" => Some(Band::Far),
        _ => None,
    }
}

/// Place and tint the marker each frame. Test hooks (screenshots only):
/// `PD_TEST_LOCK_MARKER=1` marks the opponent in every view, `..._TICK=N`
/// freezes the intro/age at N ticks, `..._BAND=close|medium|far` forces the band, `..._AT=fx,fy` moves the anchor to a view fraction, `..._LOG=<file>` writes the anchor.
#[allow(clippy::too_many_arguments)]
pub fn update_lock_marker(
    mut commands: Commands,
    art: Option<Res<MenuArt>>,
    world: Res<World>,
    fixed: Res<Time<Fixed>>,
    scale: Res<HudScale>,
    local: Res<crate::seats::LocalViews>,
    mut state: Local<MarkerState>,
    cams: Query<(&PlayerCamera, &Camera, &GlobalTransform)>,
    mut sprites: Query<(&MarkerSprite, &mut Node, &mut UiTransform, &mut ImageNode, &mut Visibility), (Without<MarkerText>, Without<MarkerUnderline>, Without<MarkerName>)>,
    mut texts: Query<(Entity, &mut MarkerText, &mut Node, &mut Visibility), (Without<MarkerSprite>, Without<MarkerUnderline>, Without<MarkerName>)>,
    mut lines: Query<(&MarkerUnderline, &mut Node, &mut ImageNode, &mut Visibility), (Without<MarkerSprite>, Without<MarkerText>, Without<MarkerName>)>,
    mut names: Query<(Entity, &MarkerName, &mut Node, &mut Visibility, &mut PixelText), (Without<MarkerSprite>, Without<MarkerText>, Without<MarkerUnderline>)>,
) {
    if scale.of(0).view.y <= 1.0 {
        return; // layout has not sized the HUD yet
    }
    let c = &world.0.combat;
    let players = c.roster.count();
    let force = std::env::var_os("PD_TEST_LOCK_MARKER").is_some();
    let forced_tick = env_tick();
    let forced_band = env_band();
    let now = fixed.elapsed_secs() as f64;
    // Full-screen form when this machine shows one view, even of a 2+ fighter match.
    let set = Set::for_players(local.slots(&c.roster).len());
    // Each view's own height sizes its sprites: with three players the third view is not the size of the others'.
    let view_h_of = |v: usize| scale.of(v).view.y;
    // Per view: what to draw (None = hidden).
    let mut plan: [Option<(Vec<Draw>, Vec2, [f32; 4], Band, usize)>; MAX_PLAYERS] = Default::default();
    // Per view: the readout's string and the text / underline top-lefts in sprite px from the anchor.
    let mut readout: [Option<(String, Vec2, Vec2)>; MAX_PLAYERS] = Default::default();
    let readout_split = READOUT_IN_SPLIT || std::env::var_os("PD_TEST_READOUT_SPLIT").is_some();
    for (pc, cam, gt) in &cams {
        let i = pc.0;
        let shown = i < players && (world.0.players[i].locked || force);
        let st = &mut state.view[i];
        if !shown {
            *st = ViewState::default();
            continue;
        }
        let foe = c.aim_at(i, world.0.players[i].target);
        if st.since.is_none() || st.target != foe {
            st.since = Some(now);
            st.target = foe;
            st.band = None;
            st.prev_band = None;
        }
        let distance = world.0.players[i].pos.distance(world.0.players[foe].pos);
        let band = forced_band.unwrap_or_else(|| band_of(distance));
        match st.band {
            Some(b) if b != band => {
                st.prev_band = Some(b);
                st.band_since = now;
                st.band = Some(band);
            }
            None => st.band = Some(band),
            _ => {}
        }
        let age = forced_tick.map(|t| t as f64 / TICK_RATE).unwrap_or(now - st.since.unwrap_or(now));
        let counter = intro_counter(age);
        let ramp = ramp_after(now - st.band_since);
        let prev = if ramp < 1.0 { st.prev_band } else { None };
        let shown_distance = match (forced_band, band) {
            (None, _) => distance,
            (Some(_), Band::Close) => 60.0,
            (Some(_), Band::Medium) => 237.0,
            (Some(_), Band::Far) => 317.0,
        };
        let view_h = view_h_of(i);
        let draws = layout(set, band, prev, counter, ramp, shown_distance);
        let (pos, _) = render_pose(&world, &fixed, foe);
        let origin = cam.logical_viewport_rect().map_or(Vec2::ZERO, |r| r.min);
        let Some(at) = cam.world_to_viewport(gt, pos + Vec3::Y * LOCK_TARGET_HEIGHT).ok().map(|p| p - origin) else { continue };
        if !at.is_finite() {
            continue;
        }
        // Capture aid: `PD_TEST_LOCK_MARKER_AT=fx,fy` puts the anchor at that fraction of the view (plain background).
        let at = std::env::var("PD_TEST_LOCK_MARKER_AT")
            .ok()
            .and_then(|v| {
                let mut it = v.split(',').map(|n| n.trim().parse::<f32>().ok());
                Some(Vec2::new(it.next()??, it.next()??))
            })
            .map_or(at, |f| f * scale.of(i).view);
        if let Some(path) = std::env::var_os("PD_TEST_LOCK_MARKER_LOG") {
            // Capture aid: where this view's marker anchor is, in view px (`<file>.<view>`, overwritten each frame).
            let _ = std::fs::write(format!("{}.{i}", path.to_string_lossy()), format!("view {i} anchor {:.1} {:.1} view_h {:.1} px_per_sprite {:.4} set {set:?} ids {:?}
", at.x, at.y, view_h, px_per_sprite(view_h), draws.iter().map(|dr| dr.id).collect::<Vec<_>>()));
        }
        let t = tint(band, prev, ramp);
        if counter == 0 && (set == Set::Full || readout_split) {
            let (text_at, under_at) = readout_places(band, shown_distance);
            readout[i] = Some((readout_text(shown_distance), text_at, under_at));
        }
        plan[i] = Some((draws, at, t, band, foe));
    }
    for (m, mut node, mut xf, mut img, mut vis) in &mut sprites {
        let entry = plan[m.view].as_ref().and_then(|(draws, at, t, _, _)| draws.get(m.slot).map(|dr| (*dr, *at, *t)));
        match entry {
            Some((dr, at, t)) => {
                let (pos, size) = place(dr, at, view_h_of(m.view));
                let (x, y, w, h) = rect_of(dr.id);
                node.left = Val::Px(pos.x);
                node.top = Val::Px(pos.y);
                node.width = Val::Px(size.x);
                node.height = Val::Px(size.y);
                let rot = Rot2::degrees(dr.angle * SCREEN_TURN);
                if xf.rotation != rot {
                    xf.rotation = rot;
                }
                let rect = Some(Rect::new(x, y, x + w, y + h));
                if img.rect != rect {
                    img.rect = rect;
                }
                let mut t = t;
                t[3] = (t[3] * alpha_boost(dr.id)).min(1.0);
                let colour = image_colour(t);
                if img.color != colour {
                    img.color = colour;
                }
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
    for (m, mut node, mut img, mut vis) in &mut lines {
        let s = px_per_sprite(view_h_of(m.0));
        let entry = plan[m.0].as_ref().zip(readout[m.0].as_ref());
        match entry {
            Some(((_, at, _, band, _), (_, _, under))) => {
                let (_, _, w, h) = UNDERLINE;
                node.left = Val::Px(at.x + under.x * s);
                node.top = Val::Px(at.y + under.y * s);
                node.width = Val::Px(w * s);
                node.height = Val::Px((h * s).max(1.0));
                let rgb = readout_rgb(*band);
                let colour = Color::srgb_u8(rgb[0], rgb[1], rgb[2]);
                if img.color != colour {
                    img.color = colour;
                }
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
    let Some(art) = art else { return };
    for (entity, mut m, mut node, mut vis) in &mut texts {
        let s = px_per_sprite(view_h_of(m.view));
        let want: Option<(String, Vec2, [u8; 3])> = match plan[m.view].as_ref() {
            Some((_, at, _, band, _)) => readout[m.view].as_ref().map(|(text, pos, _)| (text.clone(), *at + *pos * s, readout_rgb(*band))),
            None => None,
        };
        match want {
            Some((text, pos, rgb)) => {
                node.left = Val::Px(pos.x);
                node.top = Val::Px(pos.y);
                let key = (text, rgb, s);
                if m.built.as_ref() != Some(&key) {
                    spawn_readout_children(&mut commands, &art, entity, &key.0, key.1, key.2);
                    m.built = Some(key);
                }
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
    for (entity, m, mut node, mut vis, mut pt) in &mut names {
        let s = px_per_sprite(view_h_of(m.0));
        // The name belongs to the settled marker of a full view.
        let entry = plan[m.0].as_ref().filter(|(draws, ..)| draws.iter().any(|dr| dr.id == SCYTHE) && (set == Set::Full || NAME_IN_SPLIT));
        match entry {
            Some((_, at, t, _, foe)) => {
                let size_px = 14.0 * s;
                node.left = Val::Px(at.x + 4.0 * s);
                node.top = Val::Px(at.y + 30.0 * s);
                node.height = Val::Px(ui::CELL * ui::pixel_scale(size_px));
                let colour = Color::srgb(t[0], t[1], t[2]);
                ui::set_pixel(&mut commands, &art, entity, &mut pt, &crate::seats::seat_name(*foe), colour, ui::pixel_scale(size_px));
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readout_is_distance_minus_ten_times_100_in_five_columns() {
        assert_eq!(readout_text(60.0), " 5000");
        assert_eq!(readout_text(310.0), "30000");
        assert_eq!(readout_text(10.5), "    0"); // up to 0.9 past 10 it reads 0
        assert_eq!(readout_text(0.0), "    0");
    }

    #[test]
    fn readout_rides_the_gauge_like_the_footage() {
        // Laurence's Laser clip: medium band, readout 22888 (distance 238.9 u),
        // text 100 sprite px left of the target point and 10 below it.
        let (text, under) = readout_places(Band::Medium, 238.9);
        assert_eq!(text, Vec2::new(-100.0, 10.0));
        assert_eq!(under, Vec2::new(-113.0, 25.0));
        // Past the gauge's top end the program switches to gl 63, ly 30 and row 0.
        let (text, _) = readout_places(Band::Close, 400.0);
        assert_eq!(text, Vec2::new(14.0 - 63.0 - 50.0, 7.0 - 30.0 - 15.0));
        // Point-blank: gauge 0, row 64.
        let (text, _) = readout_places(Band::Close, 0.0);
        assert_eq!(text, Vec2::new(62.0 - 64.0 - 50.0, 63.0 - 7.0 - 15.0));
    }

    #[test]
    fn glyph_advance_follows_the_programs_width_table() {
        assert_eq!(glyph_advance('0'), 9.0);
        assert_eq!(glyph_advance(' '), 7.0);
        assert_eq!(glyph_advance('M'), 12.0);
        assert_eq!(glyph_advance('i'), 6.0);
    }

    #[test]
    fn bands_follow_the_programs_80_and_300() {
        assert_eq!(band_of(0.0), Band::Close);
        assert_eq!(band_of(70.3), Band::Close); // footage: readout 7053 / 6028 -> orange
        assert_eq!(band_of(79.99), Band::Close);
        assert_eq!(band_of(80.0), Band::Medium);
        assert_eq!(band_of(237.0), Band::Medium); // footage: readout 22706 -> yellow
        assert_eq!(band_of(300.0), Band::Medium);
        assert_eq!(band_of(317.3), Band::Far); // footage: readout 30727 -> green
    }

    #[test]
    fn intro_counts_ten_ticks_down_then_settles() {
        assert_eq!(intro_counter(0.0), 10);
        assert_eq!(intro_counter(1.0 / 30.0), 9);
        assert_eq!(intro_counter(9.5 / 30.0), 1);
        assert_eq!(intro_counter(10.0 / 30.0), 0);
        assert_eq!(intro_counter(5.0), 0);
    }

    #[test]
    fn ramp_is_point_two_steps_and_done_on_the_fifth_tick() {
        assert!((ramp_after(0.0) - 0.2).abs() < 1e-6);
        assert!((ramp_after(1.0 / 30.0) - 0.4).abs() < 1e-6);
        assert!((ramp_after(3.0 / 30.0) - 0.8).abs() < 1e-6);
        assert_eq!(ramp_after(4.0 / 30.0), 1.0);
        assert_eq!(ramp_after(10.0), 1.0);
    }

    #[test]
    fn settled_marker_is_static() {
        // No spin and no pulse: from the settled tick on the draw list never changes.
        for set in [Set::Full, Set::Split] {
            for band in [Band::Close, Band::Medium, Band::Far] {
                let first = layout(set, band, None, 0, 1.0, 237.0);
                for _ in 0..5 {
                    assert_eq!(layout(set, band, None, 0, 1.0, 237.0), first);
                }
                assert_eq!(first.last().map(|dr| dr.id), Some(SCYTHE));
                assert!(first.iter().all(|dr| dr.scale == 1.0));
            }
        }
    }

    #[test]
    fn intro_arcs_converge_by_two_c_squared() {
        // Far band, full view: arc 26 sits 2 c^2 above its final place (-16 - ... = -e - 16).
        let at = |c: u32| intro(Set::Full, Band::Far, c).iter().find(|dr| dr.id == 26).unwrap().y;
        assert_eq!(at(10), -216.0);
        assert_eq!(at(4), -48.0);
        assert_eq!(at(1), -18.0);
        // The Y icon (28) never moves.
        for c in 1..=10 {
            let y = intro(Set::Full, Band::Far, c).iter().find(|dr| dr.id == 28).copied().unwrap();
            assert_eq!((y.x, y.y), (-16.0, -10.0));
        }
        // No scythe in the intro.
        assert!(intro(Set::Split, Band::Medium, 3).iter().all(|dr| dr.id != SCYTHE));
    }

    #[test]
    fn band_change_grows_or_shrinks_the_big_ring() {
        let grow = layout(Set::Full, Band::Close, Some(Band::Far), 0, 0.4, 70.0);
        let big = grow.iter().find(|dr| dr.id == 15).unwrap();
        assert!((big.scale - 0.4).abs() < 1e-6);
        assert!(grow.iter().any(|dr| dr.id == 25 && dr.scale == 1.0));
        let shrink = layout(Set::Full, Band::Far, Some(Band::Close), 0, 0.4, 317.0);
        let old = shrink.iter().find(|dr| dr.id == 15).unwrap();
        assert!((old.scale - 0.6).abs() < 1e-6);
        // Once the ramp is done only the new band's pieces remain.
        assert_eq!(layout(Set::Full, Band::Far, Some(Band::Close), 0, 1.0, 317.0), layout(Set::Full, Band::Far, None, 0, 1.0, 317.0));
    }

    #[test]
    fn scythe_turns_with_distance_as_the_program_does() {
        // Values worked from the program's formula and checked against the
        // footage readouts: 307.27 -> far, 227.06 -> medium, 60.28 -> close
        // (readout = (distance - 10) * 100).
        assert!((scythe_angle(Band::Far, 317.27) - (43.0 * 1.8125 - 116.0)).abs() < 1e-3);
        assert!((scythe_angle(Band::Medium, 237.06) - (36.0 * 1.8125 - 116.0)).abs() < 1e-3);
        assert!((scythe_angle(Band::Close, 70.28) - (14.0 * 1.8125 - 116.0)).abs() < 1e-3);
        // Clamps: touching range gives 0 gauge, a huge distance caps at 64 (0 degrees).
        assert_eq!(scythe_angle(Band::Close, 0.0), -116.0);
        assert!((scythe_angle(Band::Far, 100000.0) - 0.0).abs() < 1e-3);
        // Only the scythe turns; the rings never do.
        let l = layout(Set::Split, Band::Far, None, 0, 1.0, 317.0);
        assert!(l.iter().filter(|dr| dr.angle != 0.0).all(|dr| dr.id == SCYTHE));
        assert!(l.iter().any(|dr| dr.id == SCYTHE && dr.angle < 0.0));
    }

    #[test]
    fn opacity_is_a_translucent_third_not_full_strength() {
        // Laurence: the old marker was too opaque. The footage fit is ~0.3.
        assert!(LOCK_OPACITY > 0.2 && LOCK_OPACITY < 0.45);
        for band in [Band::Close, Band::Medium, Band::Far] {
            let a = tint(band, None, 1.0)[3];
            assert_eq!(a, linear_alpha(LOCK_OPACITY));
            assert!(a > 0.05 && a < LOCK_OPACITY, "linear-light alpha is smaller than the gamma-space opacity");
        }
        // The crossfade colour moves from the old band to the new one.
        let mid = tint(Band::Far, Some(Band::Close), 0.6);
        assert!(mid[0] < COLOUR_CLOSE[0] && mid[0] > COLOUR_FAR[0]);
    }

    #[test]
    fn sizes_follow_view_height_over_480() {
        // 1P recording: ring 60 sprite px = 204 px in a 1640-high view (3.4/px).
        assert!((px_per_sprite(1640.0) * 60.0 - 205.0).abs() < 3.0);
        // 2P recording: crosshair 32 px = 51 px in a 772-high view.
        assert!((px_per_sprite(772.0) * 32.0 - 51.5).abs() < 1.0);
        // The ring is centred on its sprite: shrinking keeps the centre.
        let full = place(d(20, -31.0, -30.0), Vec2::ZERO, 480.0);
        let half = place(Draw { scale: 0.5, ..d(20, -31.0, -30.0) }, Vec2::ZERO, 480.0);
        assert!((full.0 + full.1 * 0.5 - (half.0 + half.1 * 0.5)).length() < 1e-4);
    }

    #[test]
    fn thin_centre_pieces_are_boosted_but_the_scythe_and_big_rings_are_not() {
        for id in [23u16, 28, 128, 133] {
            assert!(alpha_boost(id) > 1.4, "centre piece {id}");
        }
        for id in [15u16, 20, 77, 14, 120, 125] {
            assert_eq!(alpha_boost(id), 1.0, "ring {id}");
        }
        // Even the strongest piece stays translucent.
        for id in [28u16, 133] {
            assert!(alpha_boost(id) * linear_alpha(LOCK_OPACITY) < 0.5);
        }
    }

    #[test]
    fn every_piece_of_every_layout_is_drawn_with_some_alpha() {
        // The centre pieces (Y icon, small rings, far crosshair) are in the draw
        // lists of every state they belong to, never dropped.
        let has = |l: &[Draw], id: u16| l.iter().any(|dr| dr.id == id);
        for c in 1..=10 {
            assert!(has(&intro(Set::Split, Band::Far, c), 133));
            assert!(has(&intro(Set::Full, Band::Far, c), 28));
            assert!(has(&intro(Set::Split, Band::Medium, c), 128));
            assert!(has(&intro(Set::Full, Band::Medium, c), 23));
        }
        assert!(has(&settled(Set::Split, Band::Medium), 128));
        assert!(has(&settled(Set::Full, Band::Medium), 23));
        assert!(has(&settled(Set::Split, Band::Far), 130));
    }

    #[test]
    fn every_drawn_id_has_an_atlas_rect() {
        for set in [Set::Full, Set::Split] {
            for band in [Band::Close, Band::Medium, Band::Far] {
                for c in 0..=10 {
                    for dr in layout(set, band, None, c, 1.0, 237.0) {
                        assert!(rect_of(dr.id).2 > 0.0, "missing rect for sprite {}", dr.id);
                    }
                }
            }
        }
    }

    #[test]
    fn split_views_get_the_smaller_set() {
        assert_eq!(Set::for_players(1), Set::Full);
        assert_eq!(Set::for_players(2), Set::Split);
        assert_eq!(Set::for_players(4), Set::Split);
        assert_eq!(settled(Set::Split, Band::Far)[0].id, 130);
        assert_eq!(settled(Set::Full, Band::Far)[0].id, 25);
    }
}
