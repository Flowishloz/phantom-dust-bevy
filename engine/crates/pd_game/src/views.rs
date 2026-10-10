//! Split-screen layout: where each player's view sits in the window.
//!
//! One player fills the window; two keep today's two layouts (side by side or
//! top/bottom, from the settings); four share a 2x2 grid; three share the top
//! row (player 1 left, player 2 right) and player 3 takes the whole bottom half
//! (Laurence, 2026-10-03: "3 player split screen must give player 3 the entire
//! bottom half of the screen"; this replaced the 2x2 grid with a map and
//! scoreboard quarter of 2026-10-01). Everything that needs a view's rectangle
//! asks here -- `layout_viewports`, `apply_fov`, the HUD -- so the layouts
//! cannot drift apart. NOT every view is the same size any more: ask for view
//! `i` (`rect`, `rect_f`, `aspect_of`), never "the" cell.
//!
//! The 1- and 2-player rectangles are exactly the old arithmetic
//! (`size.x / 2 * i` and so on, integer division, nothing rounded
//! differently), which `two_player_layouts_equal_the_old_numbers` pins.

use crate::menu::Split;
use bevy::prelude::*;

/// The shape of the view grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One player, the whole window.
    Full,
    SideBySide,
    TopBottom,
    /// 2x2: four players.
    Quad,
    /// Three players: the top row split in two (player 1 left, player 2 right)
    /// and player 3 across the whole bottom half.
    Trio,
}

/// How many players are in the match and how the window is cut for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewLayout {
    pub kind: Kind,
    pub players: usize,
}

/// A view's place in the window, in whatever unit the caller passed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewRect<T> {
    pub pos: T,
    pub size: T,
}

impl ViewLayout {
    /// The layout for `players` (0..=4) under the settings' two-player split.
    pub fn for_players(players: usize, split: Split) -> Self {
        let kind = match players {
            0 | 1 => Kind::Full,
            2 => match split {
                Split::SideBySide => Kind::SideBySide,
                Split::TopBottom => Kind::TopBottom,
            },
            3 => Kind::Trio,
            _ => Kind::Quad,
        };
        ViewLayout { kind, players }
    }

    /// Columns and rows of the grid.
    pub fn grid(&self) -> (u32, u32) {
        match self.kind {
            Kind::Full => (1, 1),
            Kind::SideBySide => (2, 1),
            Kind::TopBottom => (1, 2),
            Kind::Quad | Kind::Trio => (2, 2),
        }
    }

    /// Grid cell (column, row) of view `i`'s top-left corner: row-major, so a
    /// quad reads top-left, top-right, bottom-left, bottom-right, and three
    /// players are top-left, top-right, bottom-left (the start of the wide
    /// bottom view).
    pub fn cell(&self, i: usize) -> (u32, u32) {
        let (cols, _) = self.grid();
        (i as u32 % cols, i as u32 / cols)
    }

    /// How many grid cells (columns, rows) view `i` covers: one, except the
    /// third player's view, which covers both columns of the bottom row.
    pub fn span(&self, i: usize) -> (u32, u32) {
        match (self.kind, i) {
            (Kind::Trio, 2) => (2, 1),
            _ => (1, 1),
        }
    }

    /// The view rectangle in physical pixels (`window` = the window's
    /// physical size). The size is the old `window / n` per cell
    /// (integer division) times the cells covered; a view that covers every
    /// column or row takes the whole window width or height, so the wide
    /// bottom view reaches both edges of an odd-sized window. The 1- and
    /// 2-player layouts cover one cell of a one-dimensional grid and come out
    /// as the old `size / n * index` to the pixel.
    pub fn rect(&self, i: usize, window: UVec2) -> ViewRect<UVec2> {
        let (cols, rows) = self.grid();
        let (c, r) = self.cell(i);
        let (sc, sr) = self.span(i);
        let cell = UVec2::new(window.x / cols, window.y / rows);
        let pos = UVec2::new(cell.x * c, cell.y * r);
        let size = UVec2::new(if sc >= cols { window.x - pos.x } else { cell.x * sc }, if sr >= rows { window.y - pos.y } else { cell.y * sr });
        ViewRect { pos, size }
    }

    /// The rectangle inside a game area at `area_pos` of size `area_size` (physical
    /// pixels, `display::GameArea`): the same arithmetic as [`rect`](Self::rect),
    /// moved by the area's offset. With the area equal to the window this IS `rect`.
    pub fn rect_in(&self, i: usize, area_pos: UVec2, area_size: UVec2) -> ViewRect<UVec2> {
        let r = self.rect(i, area_size);
        ViewRect { pos: area_pos + r.pos, size: r.size }
    }

    /// The same rectangle in logical pixels (the HUD's units).
    pub fn rect_f(&self, i: usize, window: Vec2) -> ViewRect<Vec2> {
        let (cols, rows) = self.grid();
        let (c, r) = self.cell(i);
        let (sc, sr) = self.span(i);
        let cell = Vec2::new(window.x / cols as f32, window.y / rows as f32);
        ViewRect { pos: Vec2::new(cell.x * c as f32, cell.y * r as f32), size: Vec2::new(cell.x * sc as f32, cell.y * sr as f32) }
    }

    /// The aspect ratio (width / height) of the layout's FIRST view in a
    /// window of `window`. With three players the views differ -- the third is
    /// twice as wide -- so anything per view asks [`aspect_of`](Self::aspect_of).
    pub fn aspect(&self, window: Vec2) -> f32 {
        self.aspect_of(0, window)
    }

    /// View `i`'s aspect ratio (width / height) in a window of `window`.
    pub fn aspect_of(&self, i: usize, window: Vec2) -> f32 {
        let size = self.rect_f(i, window).size;
        size.x / size.y.max(1.0)
    }

    /// Whether the views are small cells of a two-row grid (three or four
    /// players): the HUD grows its panels and dial text for them.
    pub fn is_grid(&self) -> bool {
        matches!(self.kind, Kind::Quad | Kind::Trio)
    }

    /// Whether the two-player "strip" framing rules apply (a short, wide
    /// view: the top/bottom camera drop).
    pub fn is_strip(&self) -> bool {
        self.kind == Kind::TopBottom
    }
}

/// Which screen quadrant a view's viewport starts in: bit 0 = its left edge
/// is not at the window's left, bit 1 = its top edge is not at the window's
/// top. This is exactly how the shaders tell the views apart
/// (`dissolve.wgsl`, `fx_gamma.wgsl`: `step(0.5, view.viewport.x)` plus twice
/// `step(0.5, view.viewport.y)`), so the CPU fills per-quadrant values with
/// the same rule. Side by side, view 1 is quadrant 1; top/bottom, view 1 is
/// quadrant 2; a quad is 0..3; with three players the wide bottom view starts
/// at the left edge of the bottom half, so it is quadrant 2 (and quadrant 3 is
/// nobody's); view 0 is always quadrant 0. A shader only reads WHERE a view
/// starts, never how big it is, so the wide view needs nothing more.
///
/// With bars round the game area (`display.rs`) the views start at the area's
/// corner, not the window's: the origin is taken relative to it
/// (`display::area_origin`), and the shaders subtract the same corner (they get
/// it in their material uniforms), so the two always agree.
pub fn quadrant_of_origin(origin: UVec2) -> usize {
    quadrant_in(origin, crate::display::area_origin())
}

/// [`quadrant_of_origin`] for a game area whose corner is `area_origin`.
pub fn quadrant_in(origin: UVec2, area_origin: UVec2) -> usize {
    let rel = origin.as_ivec2() - area_origin.as_ivec2();
    (rel.x > 0) as usize + 2 * (rel.y > 0) as usize
}

/// `quadrant_of_origin` for a player camera. A camera with no viewport yet
/// (before the first layout, or a headless test) counts as view `view` of a
/// side-by-side pair (quadrant `view`, at most 3), like the two-player game
/// always did.
pub fn quadrant_of(camera: Option<&Camera>, view: usize) -> usize {
    match camera.and_then(|c| c.viewport.as_ref()) {
        Some(v) => quadrant_of_origin(v.physical_position),
        None => view.min(3),
    }
}

/// The shape of the original's views: every view of its picture is 16:9 (LIVE projection, GAME.md).
pub const REF_ASPECT: f32 = 16.0 / 9.0;

/// The vertical field of view (radians) of ONE view, from that view's own aspect (viewport
/// width / height).
///
/// `h16_deg` is the FOV setting: the horizontal angle of a 16:9 view. Its vertical angle there is
/// `v16`. A view at least as wide as 16:9 keeps `v16` (Hor+: a wider view sees more to the sides,
/// so a top/bottom strip or an ultrawide window is not a thin telephoto slice). A narrower view
/// keeps the 16:9 horizontal angle (Vert+: it sees more above and below, as in a side-by-side
/// half). So every view shows at least the original's 16:9 picture, in every layout. The two
/// branches meet at 16:9.
pub fn fov_for(h16_deg: f32, aspect: f32) -> f32 {
    let half_h = (h16_deg.to_radians() / 2.0).tan();
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { REF_ASPECT };
    2.0 * (half_h / aspect.min(REF_ASPECT)).atan()
}

/// A camera's aspect from its viewport, or `None` before the first layout.
pub fn viewport_aspect(camera: &Camera) -> Option<f32> {
    let v = camera.viewport.as_ref()?;
    (v.physical_size.x > 0 && v.physical_size.y > 0).then(|| v.physical_size.x as f32 / v.physical_size.y as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: UVec2 = UVec2::new(1600, 900);

    /// The old `layout_viewports` arithmetic, copied verbatim as the oracle.
    fn old_rect(split: Split, i: u32, size: UVec2) -> (UVec2, UVec2) {
        match split {
            Split::SideBySide => (UVec2::new(size.x / 2 * i, 0), UVec2::new(size.x / 2, size.y)),
            Split::TopBottom => (UVec2::new(0, size.y / 2 * i), UVec2::new(size.x, size.y / 2)),
        }
    }

    #[test]
    fn two_player_layouts_equal_the_old_numbers() {
        // Odd sizes too: the integer division must round exactly as it did.
        for size in [W, UVec2::new(1281, 721), UVec2::new(3840, 2160), UVec2::new(1919, 1079)] {
            for split in [Split::SideBySide, Split::TopBottom] {
                let l = ViewLayout::for_players(2, split);
                for i in 0..2u32 {
                    let (pos, dim) = old_rect(split, i, size);
                    assert_eq!(l.rect(i as usize, size), ViewRect { pos, size: dim }, "{split:?} view {i} at {size}");
                }
            }
        }
    }

    #[test]
    fn layout_kind_follows_the_player_count() {
        let sbs = Split::SideBySide;
        assert_eq!(ViewLayout::for_players(1, sbs).kind, Kind::Full);
        assert_eq!(ViewLayout::for_players(2, sbs).kind, Kind::SideBySide);
        assert_eq!(ViewLayout::for_players(2, Split::TopBottom).kind, Kind::TopBottom);
        assert_eq!(ViewLayout::for_players(3, Split::TopBottom).kind, Kind::Trio, "the two-player split setting is ignored from three players");
        assert_eq!(ViewLayout::for_players(3, sbs).kind, Kind::Trio);
        assert_eq!(ViewLayout::for_players(4, sbs).kind, Kind::Quad);
        assert_eq!(ViewLayout::for_players(4, Split::TopBottom).kind, Kind::Quad);
    }

    #[test]
    fn one_player_fills_the_window() {
        let l = ViewLayout::for_players(1, Split::SideBySide);
        assert_eq!(l.rect(0, W), ViewRect { pos: UVec2::ZERO, size: W });
    }

    /// With bars the views sit inside the game area: the same cells, moved by
    /// the area's corner, never touching the bars.
    #[test]
    fn views_sit_inside_the_game_area() {
        let (pos, size) = (UVec2::new(480, 0), UVec2::new(1440, 1080));
        for split in [Split::SideBySide, Split::TopBottom] {
            for players in 1..=4 {
                let l = ViewLayout::for_players(players, split);
                for i in 0..players {
                    let r = l.rect_in(i, pos, size);
                    assert!(r.pos.x >= pos.x && r.pos.y >= pos.y && r.pos.x + r.size.x <= pos.x + size.x && r.pos.y + r.size.y <= pos.y + size.y, "{split:?} {players}p view {i}: {r:?}");
                    // The old numbers, shifted.
                    let old = l.rect(i, size);
                    assert_eq!((r.pos, r.size), (old.pos + pos, old.size));
                }
            }
        }
        // Side by side in a 4:3 area (1440x1080): two 720x1080 halves, side by side across the area.
        let sbs = ViewLayout::for_players(2, Split::SideBySide);
        assert_eq!(sbs.rect_in(0, pos, size), ViewRect { pos: UVec2::new(480, 0), size: UVec2::new(720, 1080) });
        assert_eq!(sbs.rect_in(1, pos, size), ViewRect { pos: UVec2::new(1200, 0), size: UVec2::new(720, 1080) });
        // Top/bottom in a 21:9 area under bars above and below.
        let (pos, size) = (UVec2::new(0, 257), UVec2::new(3840, 1646));
        let tb = ViewLayout::for_players(2, Split::TopBottom);
        assert_eq!(tb.rect_in(0, pos, size), ViewRect { pos: UVec2::new(0, 257), size: UVec2::new(3840, 823) });
        assert_eq!(tb.rect_in(1, pos, size), ViewRect { pos: UVec2::new(0, 1080), size: UVec2::new(3840, 823) });
        // With the area equal to the window nothing moves.
        let l = ViewLayout::for_players(4, Split::SideBySide);
        assert_eq!(l.rect_in(3, UVec2::ZERO, W), l.rect(3, W));
    }

    /// Bars must not change which quadrant a view counts as: the shaders pick
    /// per-view values by it (`dissolve.wgsl`, `fx_gamma.wgsl`).
    #[test]
    fn quadrants_are_relative_to_the_game_area() {
        let area = (UVec2::new(240, 0), UVec2::new(1120, 900)); // 1600x900 window, 4:3 pillarbox... (any area)
        let q = |l: ViewLayout, n: usize| (0..n).map(|i| quadrant_in(l.rect_in(i, area.0, area.1).pos, area.0)).collect::<Vec<_>>();
        assert_eq!(q(ViewLayout::for_players(2, Split::SideBySide), 2), [0, 1]);
        assert_eq!(q(ViewLayout::for_players(2, Split::TopBottom), 2), [0, 2]);
        assert_eq!(q(ViewLayout::for_players(4, Split::SideBySide), 4), [0, 1, 2, 3]);
        let area = (UVec2::new(0, 257), UVec2::new(3840, 1646));
        let q = |l: ViewLayout, n: usize| (0..n).map(|i| quadrant_in(l.rect_in(i, area.0, area.1).pos, area.0)).collect::<Vec<_>>();
        assert_eq!(q(ViewLayout::for_players(2, Split::TopBottom), 2), [0, 2], "letterbox bars above: view 0 is still quadrant 0");
        assert_eq!(q(ViewLayout::for_players(3, Split::TopBottom), 3), [0, 1, 2]);
        // Without the area correction a pillarboxed side-by-side pair would collide in quadrant 1: the bug this prevents.
        let l = ViewLayout::for_players(2, Split::SideBySide);
        let naive = |i: usize| quadrant_in(l.rect_in(i, UVec2::new(240, 0), UVec2::new(1120, 900)).pos, UVec2::ZERO);
        assert_eq!((naive(0), naive(1)), (1, 1));
    }

    #[test]
    fn quad_cells_tile_the_window_row_major() {
        let l = ViewLayout::for_players(4, Split::SideBySide);
        let q = UVec2::new(800, 450);
        assert_eq!(l.rect(0, W), ViewRect { pos: UVec2::new(0, 0), size: q });
        assert_eq!(l.rect(1, W), ViewRect { pos: UVec2::new(800, 0), size: q });
        assert_eq!(l.rect(2, W), ViewRect { pos: UVec2::new(0, 450), size: q });
        assert_eq!(l.rect(3, W), ViewRect { pos: UVec2::new(800, 450), size: q });
        // Odd window: the rectangles never overlap and never leave the window.
        let odd = UVec2::new(1281, 721);
        for i in 0..4 {
            let r = l.rect(i, odd);
            assert!(r.pos.x + r.size.x <= odd.x && r.pos.y + r.size.y <= odd.y);
        }
    }

    #[test]
    fn logical_rectangles_match_the_physical_ones_at_scale_one() {
        for players in 1..=4 {
            let l = ViewLayout::for_players(players, Split::TopBottom);
            for i in 0..players {
                let (p, f) = (l.rect(i, W), l.rect_f(i, Vec2::new(1600.0, 900.0)));
                assert_eq!((p.pos.as_vec2(), p.size.as_vec2()), (f.pos, f.size), "{players} players view {i}");
            }
        }
    }

    /// Laurence, 2026-10-03: "3 player split screen must give player 3 the
    /// entire bottom half of the screen." Player 1 top-left, player 2 top-right,
    /// player 3 the whole bottom half, edge to edge.
    #[test]
    fn three_players_give_player_three_the_whole_bottom_half() {
        let l = ViewLayout::for_players(3, Split::SideBySide);
        assert_eq!(l.rect(0, W), ViewRect { pos: UVec2::new(0, 0), size: UVec2::new(800, 450) });
        assert_eq!(l.rect(1, W), ViewRect { pos: UVec2::new(800, 0), size: UVec2::new(800, 450) });
        assert_eq!(l.rect(2, W), ViewRect { pos: UVec2::new(0, 450), size: UVec2::new(1600, 450) }, "the bottom half of the window, all of it");
        // The same at the two sizes Laurence's PC plays at.
        let l = ViewLayout::for_players(3, Split::TopBottom);
        assert_eq!(l.rect(2, UVec2::new(1920, 1080)), ViewRect { pos: UVec2::new(0, 540), size: UVec2::new(1920, 540) });
        assert_eq!(l.rect(2, UVec2::new(3840, 2160)), ViewRect { pos: UVec2::new(0, 1080), size: UVec2::new(3840, 1080) });
    }

    /// The three views tile the window without overlapping or leaving it, odd
    /// sizes included; player 3 always reaches both side edges.
    #[test]
    fn three_player_views_never_overlap_or_leave_the_window() {
        let l = ViewLayout::for_players(3, Split::SideBySide);
        for win in [W, UVec2::new(1281, 721), UVec2::new(1919, 1079), UVec2::new(3440, 1440), UVec2::new(1000, 1001)] {
            let r: Vec<_> = (0..3).map(|i| l.rect(i, win)).collect();
            for (i, v) in r.iter().enumerate() {
                assert!(v.size.x > 0 && v.size.y > 0, "{win}: view {i} has an area");
                assert!(v.pos.x + v.size.x <= win.x && v.pos.y + v.size.y <= win.y, "{win}: view {i} inside the window");
            }
            // The top views end where the bottom one starts, and do not touch each other.
            assert_eq!(r[0].pos.y + r[0].size.y, r[2].pos.y, "{win}");
            assert_eq!(r[1].pos.y + r[1].size.y, r[2].pos.y, "{win}");
            assert_eq!(r[0].pos.x + r[0].size.x, r[1].pos.x, "{win}: the two top views meet");
            assert_eq!((r[2].pos.x, r[2].pos.x + r[2].size.x), (0, win.x), "{win}: the bottom view spans the whole width");
        }
    }

    /// Four players are the 2x2 grid they always were; the third view is the
    /// only one the three-player layout changed.
    #[test]
    fn four_players_keep_the_2x2_grid_and_the_first_two_views_of_three_are_its_top_cells() {
        let four = ViewLayout::for_players(4, Split::SideBySide);
        let three = ViewLayout::for_players(3, Split::SideBySide);
        for win in [W, UVec2::new(1281, 721), UVec2::new(3840, 2160)] {
            for i in 0..2 {
                assert_eq!(three.rect(i, win), four.rect(i, win), "{win}: top cells agree");
            }
            let h = win.y / 2;
            assert_eq!(four.rect(2, win).size, UVec2::new(win.x / 2, h), "{win}: a quad cell is still a quarter");
            assert_eq!(three.rect(2, win).pos, four.rect(2, win).pos, "{win}: the wide view starts where the quad's third cell does");
        }
    }

    /// Not every view is the same size with three players: the third is twice
    /// as wide, so its aspect is double the others'.
    #[test]
    fn each_view_has_its_own_aspect() {
        let win = Vec2::new(1920.0, 1080.0);
        let three = ViewLayout::for_players(3, Split::SideBySide);
        let (a0, a1, a2) = (three.aspect_of(0, win), three.aspect_of(1, win), three.aspect_of(2, win));
        assert!((a0 - 960.0 / 540.0).abs() < 1e-5 && (a0 - a1).abs() < 1e-6);
        assert!((a2 - 1920.0 / 540.0).abs() < 1e-5, "32:9 at 1920x1080: {a2}");
        assert!((a2 - 2.0 * a0).abs() < 1e-5);
        assert_eq!(three.aspect(win), a0, "`aspect` is the first view's");
        // Everything else has one aspect.
        for players in [1, 2, 4] {
            for split in [Split::SideBySide, Split::TopBottom] {
                let l = ViewLayout::for_players(players, split);
                for i in 0..players {
                    assert_eq!(l.aspect_of(i, win), l.aspect(win), "{players} players {split:?} view {i}");
                }
            }
        }
    }

    /// With bars round the game area the wide view is still the bottom half of
    /// the AREA (not of the window), and it is quadrant 2 there as well.
    #[test]
    fn the_wide_view_sits_in_the_bottom_half_of_the_game_area() {
        let l = ViewLayout::for_players(3, Split::SideBySide);
        let (pos, size) = (UVec2::new(480, 0), UVec2::new(1440, 1080)); // 4:3 pillarbox on a 1080p window
        assert_eq!(l.rect_in(2, pos, size), ViewRect { pos: UVec2::new(480, 540), size: UVec2::new(1440, 540) });
        let (pos, size) = (UVec2::new(0, 257), UVec2::new(3840, 1646)); // 21:9 letterbox on a 4K window
        assert_eq!(l.rect_in(2, pos, size), ViewRect { pos: UVec2::new(0, 257 + 823), size: UVec2::new(3840, 823) });
        for area in [(UVec2::new(480, 0), UVec2::new(1440, 1080)), (UVec2::new(0, 257), UVec2::new(3840, 1646)), (UVec2::ZERO, UVec2::new(1920, 1080))] {
            let q: Vec<usize> = (0..3).map(|i| quadrant_in(l.rect_in(i, area.0, area.1).pos, area.0)).collect();
            assert_eq!(q, [0, 1, 2], "{area:?}: the wide view starts at the left edge of the bottom half: quadrant 2");
        }
    }

    /// The CPU's quadrant for a view must be the shader's
    /// (`step(0.5, vp.x) + 2 step(0.5, vp.y)` of the viewport origin).
    #[test]
    fn a_views_quadrant_is_its_viewport_origin() {
        let quad = |l: ViewLayout, n: usize| (0..n).map(|i| quadrant_of_origin(l.rect(i, W).pos)).collect::<Vec<_>>();
        assert_eq!(quad(ViewLayout::for_players(1, Split::SideBySide), 1), [0]);
        assert_eq!(quad(ViewLayout::for_players(2, Split::SideBySide), 2), [0, 1], "side by side: view 1 is quadrant 1");
        assert_eq!(quad(ViewLayout::for_players(2, Split::TopBottom), 2), [0, 2], "top/bottom: view 1 is quadrant 2");
        assert_eq!(quad(ViewLayout::for_players(4, Split::SideBySide), 4), [0, 1, 2, 3]);
        assert_eq!(quad(ViewLayout::for_players(3, Split::SideBySide), 3), [0, 1, 2], "three players: the wide bottom view starts at the left edge of the bottom half, quadrant 2");
        // A camera with no viewport counts as its own side-by-side slot.
        assert_eq!([0, 1, 2, 3, 9].map(|v| quadrant_of(None, v)), [0, 1, 2, 3, 3]);
    }

    /// The horizontal angle a view of `aspect` ends up with at vertical angle `v`.
    fn horizontal(v: f32, aspect: f32) -> f32 {
        2.0 * ((v / 2.0).tan() * aspect).atan()
    }

    /// The FOV rule per aspect: 16:9 and wider keep the 16:9 vertical angle, narrower keep the 16:9
    /// horizontal angle (so every view shows at least the original's picture).
    #[test]
    fn fov_rule_per_aspect() {
        for h in [50.0f32, 75.0, 110.0] {
            let hr = h.to_radians();
            let v16 = fov_for(h, REF_ASPECT);
            assert!((horizontal(v16, REF_ASPECT) - hr).abs() < 1e-5, "{h}: 16:9 has exactly the setting as its horizontal angle");
            // 32:9 (a top/bottom strip of a 16:9 window, or the 3-player bottom cell): same vertical, wider.
            assert_eq!(fov_for(h, 32.0 / 9.0), v16, "{h}: 32:9 keeps the 16:9 vertical angle");
            assert!(horizontal(fov_for(h, 32.0 / 9.0), 32.0 / 9.0) > hr, "{h}: and sees more sideways");
            assert_eq!(fov_for(h, 21.0 / 9.0), v16, "{h}: ultrawide too");
            // 8:9 (a side-by-side half) and 4:3: the horizontal angle is kept, so the view is taller.
            for a in [8.0 / 9.0, 4.0 / 3.0, 1.0] {
                let v = fov_for(h, a);
                assert!((horizontal(v, a) - hr).abs() < 1e-5, "{h} at aspect {a}: horizontal kept");
                assert!(v > v16, "{h} at aspect {a}: taller than the 16:9 view");
            }
        }
    }

    /// The default is the original's picture: LIVE projection x scale 1.3032, y scale 2.3159 (GAME.md).
    #[test]
    fn the_default_is_the_originals_projection() {
        let v = fov_for(crate::menu::FOV_DEFAULT, REF_ASPECT);
        let (x_scale, y_scale) = (1.0 / (crate::menu::FOV_DEFAULT.to_radians() / 2.0).tan(), 1.0 / (v / 2.0).tan());
        assert!((x_scale - 1.3032).abs() < 1e-3, "x scale {x_scale}");
        assert!((y_scale - 2.3159).abs() < 2.3159 * 0.0005, "y scale {y_scale}");
        assert!((v.to_degrees() - 46.7).abs() < 0.05, "vertical {}", v.to_degrees());
    }

    /// Never a thinner slice than the original's, and monotonic in the setting.
    #[test]
    fn no_view_shows_less_than_the_16_9_picture() {
        for h in [50.0f32, 62.5, 75.0, 90.0, 110.0] {
            let v16 = fov_for(h, REF_ASPECT);
            for i in 1..=80 {
                let a = i as f32 * 0.1;
                let v = fov_for(h, a);
                assert!(v >= v16 - 1e-6, "{h} at {a}");
                assert!(horizontal(v, a) >= h.to_radians() - 1e-5, "{h} at {a}");
            }
        }
        assert!(fov_for(110.0, 1.0) > fov_for(50.0, 1.0));
        assert!(fov_for(110.0, 3.0) > fov_for(50.0, 3.0));
    }

    /// Garbage aspects (a zero-sized viewport) fall back to 16:9.
    #[test]
    fn a_bad_aspect_falls_back_to_16_9() {
        for a in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(fov_for(75.0, a), fov_for(75.0, REF_ASPECT));
        }
    }

    /// `LocalViews(None)` is today's split screen: the layout and each slot's
    /// view are what the fighter count gave.
    #[test]
    fn local_views_none_matches_the_fighter_count() {
        use crate::combat::Roster;
        use crate::seats::LocalViews;
        for n in 1..=4usize {
            let roster = Roster::with_teams(&vec![0u8; n]);
            for split in [Split::SideBySide, Split::TopBottom] {
                let lv = LocalViews(None);
                assert_eq!(lv.layout(&roster, split), ViewLayout::for_players(n, split), "{n} players");
                for slot in 0..n {
                    assert_eq!(lv.view_of(slot, &roster), Some(slot));
                }
                assert_eq!(lv.view_of(n, &roster), None);
            }
        }
    }

    /// One local view of a two-fighter match is the full window, showing that slot.
    #[test]
    fn one_local_view_of_two_fighters_is_full_window() {
        use crate::combat::Roster;
        use crate::seats::LocalViews;
        let roster = Roster::with_teams(&[0, 1]);
        let lv = LocalViews(Some(vec![1]));
        let l = lv.layout(&roster, Split::SideBySide);
        assert_eq!(l.kind, Kind::Full);
        assert_eq!(lv.view_of(1, &roster), Some(0));
        assert_eq!(lv.view_of(0, &roster), None, "the other slot has no view here");
        assert_eq!(l.rect(0, W), ViewRect { pos: UVec2::ZERO, size: W });
    }
}
