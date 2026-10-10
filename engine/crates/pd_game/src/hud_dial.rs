//! The Aura recharge gauge: the ring of ten orange segments round the skill
//! dial's centre that lights up clockwise while the next Aura point refills
//! (docs/HUD.md "Aura recharge gauge"). Pure geometry and rules, no Bevy
//! entities, so it is unit-tested on its own; `hud.rs` spawns the ten
//! sprites over the dial's centre gauge and shows the first `lit_count`.
//!
//! The rule is read from the program (own-player dial, `FUN_140151f90`,
//! the code after the Aura / Level digits at `0x140152490`..):
//!
//!  * only while Aura is **below Level** (`aura < (float)level`); at full
//!    Aura (aura == Level) no segment is drawn, as in Laurence's footage;
//!  * `t = (aura - (int)aura) * 10.0` (`0x1402f7f7c` = 10.0) and the first
//!    `(int)t + 1` entries of the table at `0x1402f1470` are drawn: so the
//!    first segment lights the moment the Aura point starts refilling and
//!    one more every tenth of a point (up to ten just before the point
//!    lands, when the digit ticks up and the ring empties to one);
//!  * each table entry (12 bytes) is `(piece, x, y)`: hud pieces 58..67
//!    placed `(x, y)` units from the gauge piece's top-left (piece 57, the
//!    56x56 ring at hud0 0,380), clockwise from the top-right arc;
//!  * colour `0xff5a1e02` (the Level digits' colour), plain alpha: the
//!    piece art is white, so a tint is the whole look.
//!
//! Piece 58..67 -> atlas rectangle is not in the exe (the rectangles come
//! from the menu data); the table below is matched from the hud0 art: ten
//! arc/bar pieces at x=60 and x=78 below the gauge whose shapes fit the ring
//! one for one (the gauge piece's own grey segments sit under them), and
//! verified against Laurence's footage (segments light 1 to 10 in this order).

/// Ten segments, as the program's table.
pub const SEGMENTS: usize = 10;

/// `0xff5a1e02` (program, same address range as the Level digits' colour).
pub const LIT_RGB: [u8; 3] = [0x5a, 0x1e, 0x02];

/// Size of the gauge piece (hud0 0,380) the offsets are relative to.
pub const GAUGE_SIZE: f32 = 56.0;

/// Aura is kept in this many units per point (`combat::AURA_TICKS_PER_POINT`).
pub const UNITS_PER_POINT: i32 = crate::combat::AURA_TICKS_PER_POINT;

/// One segment: its hud0 atlas rectangle `(x, y, w, h)` and where it sits in
/// the gauge, `(x, y)` units from the gauge's top-left (program table
/// `0x1402f1470`, entries 0..9 = pieces 58..67).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub atlas: (f32, f32, f32, f32),
    pub at: (f32, f32),
}

pub const TABLE: [Segment; SEGMENTS] = [
    Segment { atlas: (60.0, 398.0, 14.0, 7.0), at: (29.0, 4.0) },  // top, right of centre
    Segment { atlas: (60.0, 408.0, 10.0, 12.0), at: (41.0, 9.0) }, // upper right
    Segment { atlas: (78.0, 438.0, 4.0, 15.0), at: (48.0, 20.0) }, // right
    Segment { atlas: (60.0, 426.0, 10.0, 11.0), at: (42.0, 35.0) }, // lower right
    Segment { atlas: (60.0, 444.0, 13.0, 7.0), at: (29.0, 45.0) }, // bottom, right of centre
    Segment { atlas: (60.0, 454.0, 13.0, 7.0), at: (14.0, 45.0) }, // bottom, left of centre
    Segment { atlas: (60.0, 464.0, 9.0, 11.0), at: (6.0, 36.0) },  // lower left
    Segment { atlas: (78.0, 456.0, 4.0, 15.0), at: (4.0, 20.0) },  // left
    Segment { atlas: (60.0, 482.0, 9.0, 12.0), at: (6.0, 9.0) },   // upper left
    Segment { atlas: (60.0, 500.0, 13.0, 7.0), at: (14.0, 4.0) },  // top, left of centre
];

/// How many segments are lit: none at full Aura (`aura_units >= level`),
/// else `floor(fraction * 10) + 1` of the point being refilled (1..=10).
pub fn lit_count(aura_units: i32, level: i32) -> usize {
    if aura_units >= level * UNITS_PER_POINT {
        return 0;
    }
    let part = aura_units.rem_euclid(UNITS_PER_POINT);
    ((part * SEGMENTS as i32 / UNITS_PER_POINT) as usize + 1).min(SEGMENTS)
}

/// Where segment `k` is drawn when the gauge's top-left is at `origin` and
/// one gauge unit is `scale` design px: `(x, y, w, h)`.
pub fn rect(k: usize, origin: (f32, f32), scale: f32) -> (f32, f32, f32, f32) {
    let s = TABLE[k];
    (origin.0 + s.at.0 * scale, origin.1 + s.at.1 * scale, s.atlas.2 * scale, s.atlas.3 * scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: i32 = UNITS_PER_POINT;

    #[test]
    fn nothing_lit_at_full_aura() {
        // FUN_140151f90: the segments are only drawn while aura < level.
        assert_eq!(lit_count(5 * P, 5), 0);
        assert_eq!(lit_count(0, 0), 0);
        assert_eq!(lit_count(11 * P, 11), 0);
    }

    #[test]
    fn first_segment_lights_as_soon_as_a_point_starts_refilling() {
        // (int)((aura - (int)aura) * 10) + 1 entries: 1 at fraction 0.
        assert_eq!(lit_count(0, 1), 1);
        assert_eq!(lit_count(9 * P, 11), 1);
        assert_eq!(lit_count(9 * P + P / 10 - 1, 11), 1);
    }

    #[test]
    fn one_more_segment_every_tenth_of_a_point() {
        for t in 0..10 {
            assert_eq!(lit_count(3 * P + t * P / 10, 11), t as usize + 1, "tenth {t}");
            assert_eq!(lit_count(3 * P + (t + 1) * P / 10 - 1, 11), t as usize + 1);
        }
    }

    #[test]
    fn a_point_landing_resets_the_ring_to_one() {
        // Just before the point lands the ring is full; the next unit is the
        // next point's first segment (or nothing if that was the last one).
        assert_eq!(lit_count(10 * P - 1, 11), 10);
        assert_eq!(lit_count(10 * P, 11), 1);
        assert_eq!(lit_count(11 * P - 1, 11), 10);
        assert_eq!(lit_count(11 * P, 11), 0);
    }

    #[test]
    fn table_runs_clockwise_from_the_top_and_is_mirror_symmetric() {
        let c = (GAUGE_SIZE / 2.0, GAUGE_SIZE / 2.0);
        let centre = |s: &Segment| (s.at.0 + s.atlas.2 / 2.0 - c.0, s.at.1 + s.atlas.3 / 2.0 - c.1);
        // Angle clockwise from straight up (screen y points down).
        let ang = |s: &Segment| {
            let (x, y) = centre(s);
            x.atan2(-y).rem_euclid(std::f32::consts::TAU)
        };
        for k in 1..SEGMENTS {
            assert!(ang(&TABLE[k]) > ang(&TABLE[k - 1]), "segment {k} must follow {} clockwise", k - 1);
        }
        assert!(ang(&TABLE[0]) < 0.5, "first segment is just right of top centre");
        // Entry k mirrors entry 9-k about the vertical axis.
        for k in 0..SEGMENTS {
            let a = centre(&TABLE[k]);
            let b = centre(&TABLE[SEGMENTS - 1 - k]);
            assert!((a.0 + b.0).abs() < 2.0, "{k}: {a:?} vs {b:?}");
            assert!((a.1 - b.1).abs() < 2.0, "{k}: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn every_segment_lies_inside_the_gauge() {
        for (k, s) in TABLE.iter().enumerate() {
            assert!(s.at.0 >= 0.0 && s.at.0 + s.atlas.2 <= GAUGE_SIZE, "{k}");
            assert!(s.at.1 >= 0.0 && s.at.1 + s.atlas.3 <= GAUGE_SIZE, "{k}");
        }
    }

    #[test]
    fn rect_scales_the_program_offsets() {
        let r = rect(0, (100.0, 200.0), 2.0);
        assert_eq!(r, (100.0 + 58.0, 200.0 + 8.0, 28.0, 14.0));
    }
}
