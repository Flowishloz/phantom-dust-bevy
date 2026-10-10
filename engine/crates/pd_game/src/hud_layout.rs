//! Pure geometry for the battle HUD (`hud.rs`): how big the HUD is in each
//! view layout, where the list of the other players' panels sits, how a health
//! bar divides into ticks, and where the "press a button to capture" hint may
//! go. Nothing here touches Bevy entities or the sim, so it is unit-tested on
//! its own (docs/HUD.md "Four-player HUD").
//!
//! All positions are in the HUD's 1280x720 design space (`hud::DESIGN`),
//! relative to the list origin unless said otherwise.

use crate::views::ViewLayout;
use bevy::prelude::Vec2;

/// Design-space size of one view (the HUD is drawn for 1280x720 and scaled).
pub const DESIGN: Vec2 = Vec2::new(1280.0, 720.0);

/// Whole-HUD boost over a straight fit of the design into the view. Full and
/// half views have always used 1.2 (half-screen views are small).
pub const BOOST_DEFAULT: f32 = 1.2;
/// A quarter view (three or four players) is a quarter of the screen: the
/// panels, names and numbers at the top-left grow to this boost.
pub const BOOST_QUAD_PANELS: f32 = 1.5;
/// The skill dial (bottom right) in a quarter. It is the widest thing in the
/// HUD (ribbons reach 555 design px left of the right edge), so it grows less
/// than the panels or it would cover the middle of the picture.
pub const BOOST_QUAD_DIAL: f32 = 1.35;
/// ... and its skill names, costs, uses and the timer are drawn this much
/// larger than the dial's own scale, so they read at couch distance.
pub const QUAD_DIAL_TEXT: f32 = 1.25;

/// How far left of the view's right edge the dial's ribbons reach (design px:
/// dial centre 314.5 + left knob 56.9 + ribbon 184 minus the knob's 10 px).
pub const DIAL_REACH: f32 = 555.0;
/// How far above the view's bottom edge the dial cluster reaches (design px:
/// dial centre 154 + top knob 57.8 + half a ribbon).
pub const DIAL_RISE: f32 = 224.0;

/// The two scales the HUD draws at: `main` for everything anchored top-left,
/// top-right or centre (panels, names, banners), `dial` for the bottom-right
/// anchored dial cluster (dial, knobs, timer, skill card).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scales {
    pub main: f32,
    pub dial: f32,
    /// Extra factor on the dial's own text (names, costs, uses, timer).
    pub dial_text: f32,
}

impl Scales {
    /// Everything `k` times larger (the HUD size option); the dial's text factor is a ratio and stays.
    pub fn times(self, k: f32) -> Scales {
        Scales { main: self.main * k, dial: self.dial * k, dial_text: self.dial_text }
    }
}

/// The HUD scales for a view of `view` logical pixels. `quad`: a quarter view.
pub fn scales(view: Vec2, quad: bool) -> Scales {
    let fit = (view.x / DESIGN.x).min(view.y / DESIGN.y);
    if quad {
        Scales { main: fit * BOOST_QUAD_PANELS, dial: fit * BOOST_QUAD_DIAL, dial_text: QUAD_DIAL_TEXT }
    } else {
        Scales { main: fit * BOOST_DEFAULT, dial: fit * BOOST_DEFAULT, dial_text: 1.0 }
    }
}

/// One player's HUD: where its view sits in the game area (logical px) and
/// how big everything in it draws. With three players the third view is the
/// whole bottom half, twice as wide as the others, so every player's HUD is
/// fitted to ITS OWN view, never to "the" cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudView {
    /// The view's top-left corner in the game area.
    pub origin: Vec2,
    /// The view's size.
    pub size: Vec2,
    pub scales: Scales,
}

/// Player `i`'s HUD view in a game area of `area` logical px; `hud_k` is the
/// HUD size option (`display::HudSize::scale`). A quarter or a wide half (three
/// or four players) grows the panels and dial text ([`scales`] `quad`); the
/// scale follows the view's height (the HUD is drawn for 16:9 and a wide view
/// is height-limited), so the third player's HUD is the size of the other two's,
/// the panels at the left of the wide view and the dial at its bottom right.
pub fn hud_view(layout: &ViewLayout, i: usize, area: Vec2, hud_k: f32) -> HudView {
    let r = layout.rect_f(i, area);
    let single = if r.size.x >= area.x * 0.99 && r.size.y >= area.y * 0.99 { single_view_k() } else { 1.0 };
    HudView { origin: r.pos, size: r.size, scales: scales(r.size, layout.is_grid()).times(hud_k * single) }
}

/// Extra HUD factor when one view fills the whole window (1 player, 1 vs CPUs, the Prop Hunt seeker):
/// Laurence 2026-10-08, "the UI scales too large when in single screen mode", picked D = 65 % from the
/// A-F sheet (`engine/shots/w1008/hudk/hud_size_sheet.png`). On top of the HUD SIZE option.
/// `PD_SINGLE_HUD_K` overrides it (calibration sheets).
pub const SINGLE_VIEW_K: f32 = 0.65;

fn single_view_k() -> f32 {
    static K: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *K.get_or_init(|| std::env::var("PD_SINGLE_HUD_K").ok().and_then(|v| v.parse().ok()).unwrap_or(SINGLE_VIEW_K))
}

/// Most ticks a health bar can have (the Heavy rule's 40).
pub const MAX_TICKS: usize = 40;

/// One tick of a health bar: (offset from the bar's left, width), both in
/// design px. The bar keeps its length whatever the Health: 20 points is one
/// tick each, 40 (Heavy) is twice as many half-pitch ticks, 10 (Puny) and 5
/// (KO Battle) are fewer, chunkier ticks. `tick_w` is the width at the
/// original 20-tick pitch (the bar looks as it always did at 20).
pub fn tick(k: usize, max: i32, bar_w: f32, tick_w: f32, max_w: f32) -> (f32, f32) {
    let max = max.clamp(1, MAX_TICKS as i32) as f32;
    let pitch = bar_w / max;
    let w = (pitch * tick_w / (bar_w / 20.0)).clamp(1.6, max_w);
    (k as f32 * pitch, w)
}

/// Design px per unit of the original panel's own coordinates (the panel
/// art is 134x36 units; the own panel, 141x25 units, is drawn at 224x40 =
/// x1.59, and the other-player panels use the same scale so the two read as
/// one family, as in the original).
pub const FOE_S: f32 = 1.59;

/// One row of the list of the other players.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    /// The player's slot.
    pub who: usize,
    /// The viewer's current lock target: lit (glow, brighter colours); the
    /// panel itself is the same for every row.
    pub big: bool,
    /// Top of the row relative to the list origin.
    pub y: f32,
    /// A one-line strip (name, Health, Level) instead of the panel: the A/B switch `PD_HUD_SLIM_FOES=1`
    /// (UI-DECLUTTER-PROPOSALS.md item 14; off by default, the original draws every panel).
    pub slim: bool,
}

/// Where the list starts (the name line of the first row), design px from the
/// view's top-left corner.
pub const LIST_ORIGIN: Vec2 = Vec2::new(74.0, 126.0);
/// Height of one row: the original steps 53 units per full panel (14 for the
/// name line, the 36-unit panel, 3 spare; program `FUN_140155310`).
pub const ROW_H: f32 = 53.0 * FOE_S;
/// Where the panel starts under its name line.
pub const PANEL_DY: f32 = 14.0 * FOE_S;

/// The rows of `others` (slots in order), the `target` lit.
pub fn rows(others: &[usize], target: Option<usize>) -> Vec<Row> {
    others
        .iter()
        .enumerate()
        .map(|(k, &who)| Row { who, big: Some(who) == target, y: k as f32 * ROW_H, slim: false })
        .collect()
}

/// Height of a slim row (item 14's B): the name line and a little air.
pub const SLIM_H: f32 = 18.0 * FOE_S;

/// [`rows`] with every row but the lock target's drawn as a slim strip, when there are two or more other players
/// (one other player keeps the full panel either way).
pub fn rows_slim(others: &[usize], target: Option<usize>) -> Vec<Row> {
    let mut y = 0.0;
    others
        .iter()
        .map(|&who| {
            let big = Some(who) == target;
            let slim = others.len() >= 2 && !big;
            let row = Row { who, big, y, slim };
            y += if slim { SLIM_H } else { ROW_H };
            row
        })
        .collect()
}

/// The slots in `active` (ascending) other than `me`.
pub fn others_of(active: &[bool], me: usize) -> Vec<usize> {
    (0..active.len()).filter(|&j| j != me && active[j]).collect()
}

// ---- The other players' panel (program `FUN_140155310`, docs/HUD.md) -------

/// Dots in a Health bar: one per point up to this many per "tier"; Health
/// past it is drawn in the next tier's colour over the same bar.
pub const DOTS: usize = 20;

/// Which compass point of the panel's lower-left cell each of our four skill
/// buttons uses: the knob's own compass point on our dial (`hud::KNOBS`:
/// left, right, top, bottom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compass {
    Left,
    Right,
    Top,
    Bottom,
}
pub const SLOT_COMPASS: [Compass; 4] = [Compass::Left, Compass::Right, Compass::Top, Compass::Bottom];

/// Top-left of a skill pill inside the panel, in panel units (the program's
/// per-slot offsets 0x14,0x15 / 0xd,0x18 / 0x1b,0x18 / 0x14,0x1b from the
/// panel's x+6): a diamond.
pub fn pill_unit(c: Compass) -> (f32, f32) {
    match c {
        Compass::Top => (20.0, 21.0),
        Compass::Left => (13.0, 24.0),
        Compass::Right => (27.0, 24.0),
        Compass::Bottom => (20.0, 27.0),
    }
}

/// Which row of the pill column in the HUD atlas (`hud0` x=78, y=402 + 6 row,
/// 7x4 each: red, blue, aqua, magenta, yellow, green) a capsule kind uses. The
/// program's table `0x1402f1348` gives each kind an odd sprite number
/// (attack 3, defence 5, environment 7, erase 9, special 11, status 13), i.e.
/// the column in that order. `kind` is the skill table's s9 (1 attack,
/// 2 defence, 3 erase, 4 status, 5 special, 6 environment).
pub fn pill_row(kind: u8) -> Option<usize> {
    match kind {
        1 => Some(0),
        2 => Some(1),
        6 => Some(2),
        3 => Some(3),
        5 => Some(4),
        4 => Some(5),
        _ => None,
    }
}

/// The smoothed Health the bar trails with (render only; the program keeps
/// it per player and steps it at its 30 Hz frame): it jumps up with Health,
/// and after a drop holds 30 frames, then falls 0.25 Health a frame. The
/// recent loss shows as red dots between Health and this value (checked on the
/// own panel in Laurence's clip: the red trail holds about a second, then
/// drains in about half a second). Returns (lag, hold) after one frame.
pub fn lag_step(lag: f32, hold: u32, health: i32) -> (f32, u32) {
    let h = health.max(0) as f32;
    if h > lag {
        return (h, hold);
    }
    if h == lag {
        return (lag, 30);
    }
    if hold < 1 {
        ((lag - 0.25).max(h), 0)
    } else {
        (lag, hold - 1)
    }
}

/// What a Health bar shows (`FUN_140155310`): `tier` from the trailing value
/// (0 = Health 1-20, 1 = 21-40, ...), the colour of the number; `filled` dots
/// in the tier's colour; if the Health is past the first tier the rest of the
/// 20 dots up to the bar's end show the tier below (`lower`); and red dots
/// from `filled` up to `trail_end` for Health lost a moment ago.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dots {
    pub tier: usize,
    pub filled: usize,
    pub lower: bool,
    pub trail_end: usize,
}

pub fn dots(health: i32, lag: f32) -> Dots {
    let h = health.max(0);
    let l = (lag as i32).max(h);
    let tier = if l > 0 { ((l - 1) / 20) as usize } else { 0 };
    let mut trail = l % 20;
    let filled;
    if h / 20 < l / 20 && trail > 0 {
        filled = 0;
    } else {
        let mut f = if l < 21 { h } else { h % 20 };
        if l >= 21 && f == 0 && h > 20 && l == h {
            f = 20;
        }
        filled = f;
        if trail == 0 && h < l {
            trail = 20;
        }
    }
    Dots { tier, filled: filled as usize, lower: l > 20 && filled < 20, trail_end: trail as usize }
}

/// Where the capture hint ("Laser (aura 4) - press a skill button to
/// capture") may go: the horizontal span (logical px from the view's left
/// edge) it is centred in. It sits at the centre of the view's height; in a
/// short view the dial's ribbons reach that height and the hint stops short of
/// them instead of printing over their names.
pub fn hint_span(view: Vec2, s: Scales, hint_h_design: f32) -> (f32, f32) {
    let margin = 12.0;
    let hint_top = view.y / 2.0 + 150.0 * s.main;
    let hint_bottom = hint_top + hint_h_design * s.main;
    let dial_top = view.y - DIAL_RISE * s.dial;
    if hint_bottom + 4.0 < dial_top {
        (margin, view.x - margin)
    } else {
        (margin, (view.x - DIAL_REACH * s.dial - margin).max(margin + 200.0))
    }
}

/// The pause banner's second line: which seats lost their pad ("PLAYER 2" or
/// "PLAYERS 2 AND 4"), 1-based.
pub fn lost_line(lost: &[usize]) -> String {
    match lost {
        [] => String::new(),
        [a] => format!("PLAYER {} PAD DISCONNECTED", a + 1),
        [rest @ .., last] => {
            let head: Vec<String> = rest.iter().map(|s| (s + 1).to_string()).collect();
            format!("PLAYERS {} AND {} PADS DISCONNECTED", head.join(", "), last + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_view_scales_are_the_old_ones() {
        // 1600x900 side by side: the old `min(view/1280x720) * 1.2`.
        let s = scales(Vec2::new(800.0, 900.0), false);
        assert_eq!((s.main, s.dial, s.dial_text), (0.75, 0.75, 1.0));
        let tb = scales(Vec2::new(1600.0, 450.0), false);
        assert_eq!((tb.main, tb.dial), (0.75, 0.75));
    }

    #[test]
    fn the_hud_size_option_scales_panels_and_dial_together() {
        let v = Vec2::new(800.0, 900.0);
        let base = scales(v, false);
        assert_eq!(base.times(1.0), base, "100% changes nothing");
        let big = base.times(1.5);
        assert_eq!((big.main, big.dial, big.dial_text), (base.main * 1.5, base.dial * 1.5, 1.0));
        let q = scales(Vec2::new(960.0, 540.0), true).times(1.25);
        assert_eq!(q.dial_text, QUAD_DIAL_TEXT, "the quarter's extra dial-text factor is a ratio and stays");
    }

    #[test]
    fn a_quarter_grows_the_panels_more_than_the_dial() {
        // A 1080p quarter is 960x540.
        let s = scales(Vec2::new(960.0, 540.0), true);
        assert!((s.main - 1.125).abs() < 1e-5 && (s.dial - 1.0125).abs() < 1e-5);
        assert!(s.main > scales(Vec2::new(960.0, 540.0), false).main);
    }

    use crate::menu::Split;
    use crate::views::ViewLayout;

    /// The dial's centre in the game area: the view's bottom-right corner plus
    /// the dial's design offset (`hud::DIAL_AT`, -314.5 / -154) at the dial scale.
    fn dial_centre(v: &HudView) -> Vec2 {
        v.origin + v.size + Vec2::new(-314.5, -154.0) * v.scales.dial
    }

    /// Laurence, 2026-10-03: the third player's whole bottom half. Its HUD is
    /// fitted to that wide view: panels at the left, the dial at the bottom
    /// right of the view (and of the window), as big as the other two players'.
    #[test]
    fn the_third_players_hud_fits_the_wide_bottom_view() {
        for (win, quarter_h) in [(Vec2::new(1920.0, 1080.0), 540.0), (Vec2::new(3840.0, 2160.0), 1080.0), (Vec2::new(3440.0, 1440.0), 720.0)] {
            let l = ViewLayout::for_players(3, Split::SideBySide);
            let (p1, p2, p3) = (hud_view(&l, 0, win, 1.0), hud_view(&l, 1, win, 1.0), hud_view(&l, 2, win, 1.0));
            // The view: the whole bottom half.
            assert_eq!((p3.origin, p3.size), (Vec2::new(0.0, quarter_h), Vec2::new(win.x, quarter_h)), "{win}");
            // The three HUDs are the same size: a wide view is height-limited.
            assert_eq!(p3.scales, p1.scales, "{win}: the wide view draws its HUD as big as the top views' (same height)");
            assert_eq!(p1.scales, p2.scales);
            // The dial sits at the bottom right of the wide view, inside the window, clear of the middle.
            let dial = dial_centre(&p3);
            assert!(dial.x > win.x * 0.8 && dial.x < win.x, "{win}: dial centre x {}", dial.x);
            assert!(dial.y > win.y * 0.85 && dial.y < win.y, "{win}: dial centre y {}", dial.y);
            // Its left ribbons stay well right of the view's middle (a quarter's would have crossed it).
            assert!(p3.size.x - DIAL_REACH * p3.scales.dial > p3.size.x * 0.5, "{win}: the ribbons stop short of the centre");
            // The panels list starts at the view's left, top: inside the view, at the same offset as in the top-left view.
            assert_eq!(p3.origin.x, p1.origin.x);
            // Player 1 and 2 keep their quarters.
            assert_eq!((p1.origin, p1.size), (Vec2::ZERO, Vec2::new(win.x / 2.0, quarter_h)));
            assert_eq!((p2.origin, p2.size), (Vec2::new(win.x / 2.0, 0.0), Vec2::new(win.x / 2.0, quarter_h)));
        }
    }

    /// One and two players are exactly the old numbers (a straight fit x 1.2), four are the
    /// quarter HUD.
    #[test]
    fn hud_views_for_one_two_and_four_players_are_unchanged() {
        let win = Vec2::new(1600.0, 900.0);
        for split in [Split::SideBySide, Split::TopBottom] {
            for players in [1usize, 2] {
                let l = ViewLayout::for_players(players, split);
                for i in 0..players {
                    let v = hud_view(&l, i, win, 1.0);
                    // One view filling the window draws at SINGLE_VIEW_K (Laurence 2026-10-08); split views as before.
                    let k = if players == 1 { SINGLE_VIEW_K } else { 1.0 };
                    assert_eq!(v.scales, scales(l.rect_f(0, win).size, false).times(k), "{players} players {split:?} view {i}: the old fit x 1.2, one scale for every view");
                    assert_eq!(v.size, l.rect_f(i, win).size);
                }
            }
        }
        let quad = ViewLayout::for_players(4, Split::SideBySide);
        for i in 0..4 {
            let v = hud_view(&quad, i, win, 1.0);
            assert_eq!(v.scales, scales(Vec2::new(800.0, 450.0), true), "four players: every view is a quarter");
        }
        // The HUD size option multiplies every view's scales.
        let big = hud_view(&quad, 3, win, 1.5);
        assert_eq!(big.scales, scales(Vec2::new(800.0, 450.0), true).times(1.5));
    }

    /// A wide view has room to keep the capture hint away from the dial's ribbons: it sits in the
    /// free width at the left, as in a top/bottom strip, never over the ribbons.
    #[test]
    fn the_capture_hint_stays_off_the_dial_in_the_wide_view() {
        let view = Vec2::new(1920.0, 540.0);
        let s = scales(view, true);
        let (x0, x1) = hint_span(view, s, 30.0);
        assert!(x1 <= view.x - DIAL_REACH * s.dial, "the hint ends before the ribbons start: {x1}");
        assert!(x1 - x0 > 600.0, "and has room for its line: {}", x1 - x0);
    }

    #[test]
    fn health_ticks_keep_the_bar_length() {
        // At 20 points: the original pitch and width (own panel: pitch 7, width 4).
        let (x, w) = tick(3, 20, 140.0, 4.0, 10.0);
        assert!((x - 21.0).abs() < 1e-5 && (w - 4.0).abs() < 1e-5);
        // Heavy: forty ticks across the same bar, half the pitch.
        let (x, w) = tick(39, 40, 140.0, 4.0, 10.0);
        assert!((x - 136.5).abs() < 1e-5 && (w - 2.0).abs() < 1e-5);
        // KO Battle: five chunky ticks, capped.
        let (x, w) = tick(4, 5, 140.0, 4.0, 10.0);
        assert!((x - 112.0).abs() < 1e-5 && (w - 10.0).abs() < 1e-5);
        // The last tick of any bar ends inside the bar.
        for max in [5, 10, 20, 40] {
            let (x, w) = tick(max as usize - 1, max, 140.0, 4.0, 10.0);
            assert!(x + w <= 140.0 + 1e-3, "max {max}");
        }
    }

    #[test]
    fn the_list_lights_the_target_and_stacks_equal_rows() {
        let r = rows(&[0, 2, 3], Some(2));
        assert_eq!(r.iter().map(|r| r.big).collect::<Vec<_>>(), [false, true, false]);
        assert_eq!(r[0].y, 0.0);
        assert_eq!(r[1].y, ROW_H);
        assert_eq!(r[2].y, 2.0 * ROW_H);
        // A duel: one row, big, where the old foe panel was.
        let d = rows(&[1], Some(1));
        assert_eq!((d[0].y, d[0].big), (0.0, true));
        assert_eq!(LIST_ORIGIN, Vec2::new(74.0, 126.0));
    }

    #[test]
    fn others_skip_me_and_the_inactive() {
        assert_eq!(others_of(&[true, true, true, false], 1), vec![0, 2]);
        assert_eq!(others_of(&[true, true, false, false], 0), vec![1]);
    }

    #[test]
    fn the_hint_stops_short_of_the_dial_in_a_short_view_only() {
        let quad = Vec2::new(960.0, 540.0);
        let (x0, x1) = hint_span(quad, scales(quad, true), 30.0);
        assert!(x1 < quad.x - 400.0 && x0 < x1, "{x0} {x1}");
        let tall = Vec2::new(960.0, 1080.0);
        let (_, x1) = hint_span(tall, scales(tall, false), 30.0);
        assert!(x1 > 900.0, "a tall view keeps the full width");
    }

    #[test]
    fn the_pause_line_names_the_seats() {
        assert_eq!(lost_line(&[]), "");
        assert_eq!(lost_line(&[1]), "PLAYER 2 PAD DISCONNECTED");
        assert_eq!(lost_line(&[1, 3]), "PLAYERS 2 AND 4 PADS DISCONNECTED");
        assert_eq!(lost_line(&[0, 1, 3]), "PLAYERS 1, 2 AND 4 PADS DISCONNECTED");
    }

    #[test]
    fn pills_form_a_diamond_in_the_cell() {
        let (t, l, r, b) = (pill_unit(Compass::Top), pill_unit(Compass::Left), pill_unit(Compass::Right), pill_unit(Compass::Bottom));
        assert_eq!(t.0, b.0, "top and bottom share a column");
        assert_eq!(l.1, r.1, "left and right share a row");
        assert!(t.1 < l.1 && l.1 < b.1 && l.0 < t.0 && t.0 < r.0);
        // All four inside the panel's lower-left cell, below the Health row.
        for (x, y) in [t, l, r, b] {
            assert!(x + 7.0 < 44.0 && y >= 20.0 && y + 4.0 <= 36.0);
        }
        // Our slots use the knobs' compass points.
        assert_eq!(SLOT_COMPASS, [Compass::Left, Compass::Right, Compass::Top, Compass::Bottom]);
    }

    #[test]
    fn pill_colours_follow_the_program_table() {
        // attack red, defence blue, environment aqua, erase magenta, special yellow, status green.
        assert_eq!([1u8, 2, 6, 3, 5, 4].map(|k| pill_row(k).unwrap()), [0, 1, 2, 3, 4, 5]);
        assert_eq!(pill_row(0), None, "the Aura Particle has no pill row");
        assert_eq!(pill_row(7), None);
    }

    #[test]
    fn one_dot_per_health_point_up_to_twenty() {
        let d = dots(9, 9.0);
        assert_eq!((d.tier, d.filled, d.lower), (0, 9, false));
        assert!(d.trail_end <= d.filled, "no red dots when nothing was lost");
        let d = dots(20, 20.0);
        assert_eq!((d.tier, d.filled, d.lower), (0, 20, false));
        assert!(d.trail_end <= d.filled);
        assert_eq!(dots(0, 0.0).filled, 0);
        assert_eq!(dots(5, 5.0).filled, 5, "KO Battle: five dots, not stretched");
    }

    #[test]
    fn health_past_twenty_paints_the_next_tier_over_the_bar() {
        // Laurence's 1v1 clip: Health 37 = 17 dots of tier 1, the other 3 in tier 0's colour.
        let d = dots(37, 37.0);
        assert_eq!((d.tier, d.filled, d.lower), (1, 17, true));
        assert!(d.trail_end <= d.filled);
        // Exactly 40: a full bar of tier 1, nothing underneath.
        let d = dots(40, 40.0);
        assert_eq!((d.tier, d.filled, d.lower), (1, 20, false));
        // 21 is one dot of tier 1.
        assert_eq!((dots(21, 21.0).tier, dots(21, 21.0).filled), (1, 1));
    }

    #[test]
    fn lost_health_leaves_a_red_trail_that_holds_then_drains() {
        // Seven to three: red dots 3..7 while the trail holds.
        let d = dots(3, 7.0);
        assert_eq!((d.filled, d.trail_end), (3, 7));
        // Crossing a tier (25 -> 15 while the lag is still 25): tier-0 colour with the loss red.
        let d = dots(15, 25.0);
        assert_eq!((d.tier, d.filled, d.lower, d.trail_end), (1, 0, true, 5));
        // Trail timing: up -> instant, equal -> hold 30, drop -> hold 30 frames then 0.25 a frame.
        let (mut lag, mut hold) = lag_step(3.0, 0, 7);
        assert_eq!(lag, 7.0);
        (lag, hold) = lag_step(lag, hold, 7);
        assert_eq!((lag, hold), (7.0, 30));
        for _ in 0..30 {
            (lag, hold) = lag_step(lag, hold, 3);
        }
        assert_eq!((lag, hold), (7.0, 0));
        for _ in 0..16 {
            (lag, hold) = lag_step(lag, hold, 3);
        }
        assert_eq!(lag, 3.0, "four points at a quarter a frame: 16 frames (half a second at 30 Hz)");
        assert_eq!(dots(3, lag).trail_end, 3);
    }

    #[test]
    fn slim_rows_keep_the_target_full_and_stack_the_others_tight() {
        let r = rows_slim(&[1, 2, 3], Some(2));
        assert_eq!(r.iter().map(|r| (r.who, r.big, r.slim)).collect::<Vec<_>>(), [(1, false, true), (2, true, false), (3, false, true)]);
        assert_eq!((r[0].y, r[1].y, r[2].y), (0.0, SLIM_H, SLIM_H + ROW_H));
        assert!(!rows_slim(&[1], Some(1))[0].slim && !rows_slim(&[1], None)[0].slim, "one other player keeps the panel");
        assert!(rows(&[1, 2], None).iter().all(|r| !r.slim));
    }
}
