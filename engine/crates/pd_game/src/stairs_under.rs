//! Quality-of-life change (Laurence, 2026-10-03; departs from the original): the Lost
//! Palace's two big stair flights no longer have an invisible solid block under them.
//!
//! "The staircase on the main arena has a bad hitbox underneath (in the original game
//! and in ours). Walking underneath has a thick hitbox that obscures the player when
//! nothing visually should be." DEFAULT ON; `PD_STAIRS_ORIGINAL=1` restores the
//! original's collision exactly. Read once at load, the same for every player, so the
//! sim stays deterministic.
//!
//! **What it is (MEASURED, COLLISION.md "Stairs, the invisible block").** Each flight
//! (st03 / st13 / st23: x -113..-87 and +87..+113, z -233..-125, y 50 -> 109) is a thin
//! slab on the render: the underside is open air, you see the hall floor and the far wall
//! through it. The program's 0x94 collision has the slab's soffit (an underside, a
//! ceiling) PLUS three class-0x12 walls that fill the wedge between soffit and floor on
//! the open side: a side wall along the soffit's open edge and an end wall at z -204.6.
//! No render triangle is at any of them (checked against the render mesh: the only things
//! under the soffit are the soffit itself and the floor). A body walking in meets an
//! invisible wall; shots stop on it; and the battle camera's wall pull-in (it ray-casts
//! the collision) is pulled in by it, so the player is shown from a few units away and
//! the near-wall dissolve fades him: the "thick hitbox that obscures the player".
//!
//! **What changes (load-time filter on the stage `.pdcl`, no cache regeneration).** Per
//! flight, the open-side wall and the end wall are removed and replaced by the part of
//! them that is really needed: the pocket where the soffit is lower than a fighter's head
//! (`sim::BODY_HEIGHT`, the tallest character). There the slab itself is in the way (a body
//! would stand inside it), so a side wall (the removed wall's own lower-left triangle)
//! and a cross wall stay. The cross wall stands one wall radius deeper than the head line,
//! because walls hold a body's centre that far off: a body walking in from the hall stops
//! with its head just under the soffit (Laurence 2026-10-07: the first cut, a wall at 22 u of
//! soffit, stopped him with a visible gap of about 5 u over his head). The outer side (against the hall's own
//! wall, where the render has a wall too), the soffit, the ramp (the walkable stairs)
//! and the handrail hulls are untouched. A file that does not contain exactly the
//! expected triangles is left as it is.

use glam::Vec3;
use pd_formats::collision::{Collision, RawCollision, NO_OBJECT};

/// The program's class for these walls (`0x12` = 18, the rail class).
const WALL_CLASS: u8 = 0x12;

/// The head line: a fighter's height (`sim::BODY_HEIGHT` = the tallest character, 20 u).
const HEAD_HEIGHT: f32 = crate::sim::BODY_HEIGHT;

/// How far stage walls hold a body's centre off (half the contact size, 5 u for every character).
const WALL_RADIUS: f32 = crate::body::Body::DEFAULT.contact * 0.5;

/// Stage files with this geometry (Palace family; the three `.pdcl` files are byte-identical).
const STAGES: [&str; 3] = ["st03", "st13", "st23"];

/// One flight's wedge. Numbers are read from the stage file (`st03.pdcl`, stage triangles
/// 506/507/541/747 west, 2042/2089/2255/2256 east; export space: right-handed, z mirrored).
struct Flight {
    /// x of the open side's wall (faces the hall); removed, then re-cut.
    open_x: f32,
    /// x of the outer side (the hall's own wall: kept).
    outer_x: f32,
    /// z where the soffit meets the floor (the low end), and where the end wall stands.
    z_low: f32,
    z_end: f32,
    /// Floor height, and soffit height at `z_end`.
    y_floor: f32,
    y_end: f32,
}

const FLIGHTS: [Flight; 2] = [
    Flight { open_x: -86.654, outer_x: -112.654, z_low: -148.3, z_end: -204.569, y_floor: 49.85, y_end: 82.431 },
    Flight { open_x: 87.168, outer_x: 113.168, z_low: -148.2, z_end: -204.569, y_floor: 49.85, y_end: 82.431 },
];

const EPS: f32 = 0.06;

/// What `apply` did (tests, the doc).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    pub removed: usize,
    pub added: usize,
}

/// `PD_STAIRS_ORIGINAL=1` (anything but empty / `0`): keep the original's collision.
/// Read once.
pub(crate) fn original() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PD_STAIRS_ORIGINAL").is_some_and(|v| !v.is_empty() && v != "0"))
}

/// The stage collision as the game uses it: the file, with the invisible walls under the
/// Palace flights opened unless `PD_STAIRS_ORIGINAL` is set.
pub(crate) fn load(bytes: &[u8], stage: &str) -> Collision {
    let mut raw = RawCollision::parse(bytes).expect("bad collision file");
    if !original() {
        apply(&mut raw, stage);
    }
    Collision::from_raw(raw)
}

/// Is stage triangle `t` one of `f`'s wedge walls: class 0x12, stage-owned, vertical, on the
/// open side's plane or the end plane, and inside the wedge?
fn wedge_wall(f: &Flight, t: &[Vec3; 3], owner: u32, class: u8) -> bool {
    if owner != NO_OBJECT || class != WALL_CLASS {
        return false;
    }
    let n = (t[1] - t[0]).cross(t[2] - t[0]);
    if n.length() < 1e-6 || (n.y / n.length()).abs() > 0.05 {
        return false;
    }
    let (x0, x1) = (f.open_x.min(f.outer_x) - EPS, f.open_x.max(f.outer_x) + EPS);
    let inside = t.iter().all(|v| v.x >= x0 && v.x <= x1 && v.z >= f.z_end - EPS && v.z <= f.z_low + 0.3 && v.y >= f.y_floor - 0.2 && v.y <= f.y_end + EPS);
    if !inside {
        return false;
    }
    let on_open_side = t.iter().all(|v| (v.x - f.open_x).abs() < EPS);
    let on_end = t.iter().all(|v| (v.z - f.z_end).abs() < EPS);
    on_open_side || on_end
}

/// The z at which the soffit is `h` above the floor.
fn soffit_z(f: &Flight, h: f32) -> f32 {
    f.z_low + (f.z_end - f.z_low) * h / (f.y_end - f.y_floor)
}

/// The soffit's height above the floor at `z`.
fn soffit_height(f: &Flight, z: f32) -> f32 {
    (z - f.z_low) / (f.z_end - f.z_low) * (f.y_end - f.y_floor)
}

/// The head line (soffit at `HEAD_HEIGHT`): where a body's centre may come to at most.
fn head_z(f: &Flight) -> f32 {
    soffit_z(f, HEAD_HEIGHT)
}

/// The cross wall: one wall radius from the head line toward the low end, so a body walking in
/// from the high end stops with its centre on the head line.
fn cross_z(f: &Flight) -> f32 {
    head_z(f) + WALL_RADIUS * (f.z_low - f.z_end).signum()
}

/// Open the invisible wedge under each flight of `stage` (no-op for other stages, and for a
/// file that does not hold exactly three wedge walls per flight). Returns what changed.
pub(crate) fn apply(raw: &mut RawCollision, stage: &str) -> Option<Edit> {
    if !STAGES.contains(&stage) {
        return None;
    }
    let counts: Vec<usize> = FLIGHTS.iter().map(|f| (0..raw.tris.len()).filter(|&i| wedge_wall(f, &raw.tris[i], raw.owner(i), raw.class(i))).count()).collect();
    if counts.iter().any(|&c| c != 3) {
        return None;
    }
    let removed = raw.retain(|_, t, owner, class| !FLIGHTS.iter().any(|f| wedge_wall(f, t, owner, class)));
    let mut added = 0;
    for f in &FLIGHTS {
        let (zs, ys) = (head_z(f), f.y_floor + HEAD_HEIGHT);
        let (zc, yc) = (cross_z(f), f.y_floor + soffit_height(f, cross_z(f)));
        let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        // The open side's wall, cut to the head line (the removed triangle's own corner): a body
        // coming in from the side is held a wall radius outside the slab's edge.
        raw.push([v(f.open_x, f.y_floor, f.z_low), v(f.open_x, f.y_floor, zs), v(f.open_x, ys, zs)], NO_OBJECT, WALL_CLASS);
        // The cross wall, floor to soffit, across the flight.
        raw.push([v(f.open_x, f.y_floor, zc), v(f.outer_x, f.y_floor, zc), v(f.outer_x, yc, zc)], NO_OBJECT, WALL_CLASS);
        raw.push([v(f.open_x, f.y_floor, zc), v(f.outer_x, yc, zc), v(f.open_x, yc, zc)], NO_OBJECT, WALL_CLASS);
        added += 3;
    }
    Some(Edit { removed, added })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Combat;
    use crate::sim::{Input, Match};

    fn raw() -> Option<RawCollision> {
        let dir = std::env::var_os("PD_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
        let bytes = std::fs::read(dir.join("st03.pdcl")).ok()?;
        Some(RawCollision::parse(&bytes).expect("st03.pdcl parses"))
    }

    /// Walk P1 (free camera) from `start` along `yaw` for `ticks` after the countdown.
    fn walk(col: &mut Collision, start: Vec3, yaw: f32, ticks: usize) -> Vec<Vec3> {
    walk_at(col, start, yaw, 1.0, ticks)
}

/// `walk` with the stick held at `stick_y` (1.0 run, about 0.55 walk).
fn walk_at(col: &mut Collision, start: Vec3, yaw: f32, stick_y: f32, ticks: usize) -> Vec<Vec3> {
        let ground = col.ground_below(start.x, start.z, start.y + 12.0).expect("start stands on something");
        let combat = Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
        let mut m = Match::new([Vec3::new(start.x, ground, start.z), Vec3::new(0.0, 50.0, 200.0)], combat);
        m.players[0].locked = false;
        m.players[0].cam_yaw = yaw;
        for _ in 0..180 {
            m.step([Input::default(), Input::default()], col);
        }
        (0..ticks)
            .map(|_| {
                m.step([Input::from_axes(0.0, stick_y, false, false), Input::default()], col);
                m.players[0].pos
            })
            .collect()
    }

    /// The new walls' numbers (no cache needed): the head line is where the soffit is 20 u up
    /// (z about -182.9), the cross wall one wall radius (5 u) further toward the low end, where the
    /// soffit is about 17.1 u up; the first cut's wall stood at the 22 u line (z -186.3) and held
    /// a body's centre at z -191.3, under 24.9 u of soffit: about 5 u of air over the head.
    #[test]
    fn the_cross_wall_sits_a_wall_radius_past_the_head_line() {
        for f in &FLIGHTS {
            assert!((soffit_height(f, head_z(f)) - 20.0).abs() < 1e-3);
            assert!((cross_z(f) - head_z(f) - 5.0).abs() < 1e-3, "toward the low end (+z)");
            assert!((head_z(f) - -182.85).abs() < 0.2, "head line z {}", head_z(f));
            assert!((soffit_height(f, cross_z(f)) - 17.1).abs() < 0.1, "cross wall top {}", soffit_height(f, cross_z(f)));
            let old_stop = soffit_z(f, 22.0) - 5.0;
            assert!(soffit_height(f, old_stop) - 20.0 > 4.5, "the first cut left {} u over the head", soffit_height(f, old_stop) - 20.0);
        }
    }

    /// The wedge walls are exactly where the table says (3 per flight) on the real file.
    #[test]
    fn the_real_file_holds_the_expected_wedge_walls() {
        let Some(raw) = raw() else {
            eprintln!("stairs_under: no st03.pdcl in the cache: skipped");
            return;
        };
        for f in &FLIGHTS {
            let n = (0..raw.tris.len()).filter(|&i| wedge_wall(f, &raw.tris[i], raw.owner(i), raw.class(i))).count();
            assert_eq!(n, 3, "flight at open_x {}: two end-wall triangles and the open-side wall", f.open_x);
        }
    }

    /// `apply` removes 6 triangles, adds 6, only on the Palace family, and only once.
    #[test]
    fn apply_edits_only_the_palace_family_and_only_once() {
        let Some(base) = raw() else { return };
        let mut other = base.clone();
        assert_eq!(apply(&mut other, "st06"), None);
        assert_eq!(other.tris.len(), base.tris.len());
        for stage in STAGES {
            let mut r = base.clone();
            assert_eq!(apply(&mut r, stage), Some(Edit { removed: 6, added: 6 }));
            assert_eq!(r.tris.len(), base.tris.len());
            assert_eq!(r.owners.len(), r.tris.len());
            assert_eq!(r.classes.len(), r.tris.len());
            assert!(r.signed, "the signed-winding marker survives");
            assert_eq!(apply(&mut r, stage), None, "a second pass finds nothing to remove");
        }
    }

    /// The pocket (headroom under a fighter's height) keeps its walls, the rest opens: a body
    /// walks in from the hall under the flight where the soffit is high enough, and stops at the
    /// headroom line; in the original it stopped at the open edge of the flight.
    #[test]
    fn a_body_can_walk_under_the_flight_where_there_is_headroom() {
        let Some(base) = raw() else { return };
        let mut open = base.clone();
        apply(&mut open, "st03").unwrap();
        let mut new_col = Collision::from_raw(open);
        let mut old_col = Collision::from_raw(base);
        let west = -std::f32::consts::FRAC_PI_2;
        for (name, x0, z, yaw) in [("west flight", -60.0f32, -196.0f32, west), ("east flight", 60.0, -196.0, -west)] {
            let old_end = *walk(&mut old_col, Vec3::new(x0, 50.0, z), yaw, 160).last().unwrap();
            let new_path = walk(&mut new_col, Vec3::new(x0, 50.0, z), yaw, 160);
            let new_end = *new_path.last().unwrap();
            eprintln!("{name}: original ends x {:.1}, opened ends x {:.1}", old_end.x, new_end.x);
            // Original: stopped by the invisible wall on the open edge (the wall at |x| 86.7 / 87.2 less the body radius 5).
            assert!(old_end.x.abs() < 84.0 && old_end.x.abs() > 80.0, "{name}: original stops at the open edge, x {}", old_end.x);
            // Opened: it walks under, up to the hall's own wall on the far side (|x| 112.7 / 113.2 less the body radius).
            assert!(new_end.x.abs() > 100.0, "{name}: walks under the slab, x {}", new_end.x);
            assert!(new_path.iter().all(|p| p.y < 51.0), "{name}: stays on the hall floor");
        }
    }

    /// Where the soffit is lower than a fighter the pocket still stops him (he would stand
    /// inside the slab), and the low end of the flight (z -148 .. -186) stays closed.
    #[test]
    fn the_low_pocket_stays_closed() {
        let Some(base) = raw() else { return };
        let mut open = base.clone();
        apply(&mut open, "st03").unwrap();
        let mut col = Collision::from_raw(open);
        for z in [-155.0f32, -165.0, -175.0, -181.0] {
            let end = *walk(&mut col, Vec3::new(-60.0, 50.0, z), -std::f32::consts::FRAC_PI_2, 160).last().unwrap();
            assert!(end.x > -85.0, "z {z}: the headroom-short pocket is closed, x {}", end.x);
        }
        // Past the head line the flight is open.
        let end = *walk(&mut col, Vec3::new(-60.0, 50.0, -192.0), -std::f32::consts::FRAC_PI_2, 160).last().unwrap();
        assert!(end.x < -100.0, "z -192: open, x {}", end.x);
    }

    /// Walking in from the hall end (z -200) toward the low end, a body stops with its head just
    /// under the soffit (centre on the head line), not a wall radius short of a 22 u line as the
    /// first cut did (Laurence 2026-10-07: "an invisible barrier when walking under").
    #[test]
    fn walking_in_from_the_high_end_stops_where_the_head_meets_the_soffit() {
        let Some(base) = raw() else { return };
        let mut open = base.clone();
        apply(&mut open, "st03").unwrap();
        let mut col = Collision::from_raw(open);
        for f in &FLIGHTS {
            let x = (f.open_x + f.outer_x) * 0.5;
            // Facing +z (toward the low end): yaw 0 walks along +z in this sim (see `walk`).
            let path = walk(&mut col, Vec3::new(x, 50.0, -200.0), 0.0, 200);
            let end = *path.last().unwrap();
            let over_head = soffit_height(f, end.z) - HEAD_HEIGHT;
            eprintln!("flight x {x}: stops at z {:.2}, soffit {:.2} over the head", end.z, over_head);
            assert!(over_head.abs() < 1.0, "x {x}: head {over_head:.2} u from the soffit at z {:.2}", end.z);
        }
    }

    /// The camera boom (a ray from a fighter's head up and back, 3 u off the first hit) is no
    /// longer cut short by the invisible end wall: standing under the high part of the flight and
    /// looking toward the hall, the ray runs out to the soffit / floor only.
    #[test]
    fn the_camera_ray_no_longer_meets_the_invisible_end_wall() {
        let Some(base) = raw() else { return };
        let mut open = base.clone();
        apply(&mut open, "st03").unwrap();
        let (new_col, old_col) = (Collision::from_raw(open), Collision::from_raw(base));
        // Head of a fighter at (-100, 50, -196) under the wedge's high end; camera behind and above,
        // out toward +z (the end wall at z -204.6 is on the other side) and toward +x (the open side).
        let head = Vec3::new(-100.0, 66.0, -196.0);
        let toward_hall = Vec3::new(0.95, 0.1, 0.1).normalize();
        let old = old_col.raycast(head, toward_hall, 60.0);
        let new = new_col.raycast(head, toward_hall, 60.0);
        eprintln!("boom toward the hall: original {old:?}, opened {new:?}");
        assert!(old.is_some_and(|d| d < 20.0), "original: the open-side wall is in the way, {old:?}");
        assert!(new.is_none_or(|d| d > old.unwrap() + 10.0), "opened: not cut short by the wall, {new:?}");
    }

    /// The stairs themselves are untouched: with the wedge opened the body still walks (run and
    /// walk) from the hall floor up both flights to the balcony, no jump (the `stair_walk` tests'
    /// walks, on the opened file), and the ramp is still one straight 30 degree slope.
    #[test]
    fn the_stairs_still_walk_up_with_the_wedge_opened() {
        let Some(base) = raw() else { return };
        let mut open = base.clone();
        apply(&mut open, "st03").unwrap();
        let mut col = Collision::from_raw(open);
        let mut failures = Vec::new();
        for (gait, stick, ticks) in [("run", 1.0f32, 400usize), ("walk", 0.55, 900)] {
            for x in [-105.0f32, -100.0, -95.0, 95.0, 100.0, 105.0] {
                let path = walk_at(&mut col, Vec3::new(x, 50.0, -118.0), std::f32::consts::PI, stick, ticks);
                let top = path.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                eprintln!("opened, up {gait:4} x {x:6.1}: highest {top:.1}");
                if top < 108.5 {
                    failures.push(format!("{gait} x {x}: stuck at {top:.1}"));
                }
            }
        }
        assert!(failures.is_empty(), "cannot walk up the opened stairs: {failures:?}");
        for x in [-105.0f32, -100.0, -95.0, 95.0, 100.0, 105.0] {
            let mut z = -226.0f32;
            while z <= -130.0 {
                let want = 49.9 + 0.577 * (-125.0 - z);
                let got = col.ground_below(x, z, want + 4.0).expect("ramp has floor");
                assert!((got - want).abs() < 1.5, "x {x} z {z}: ground {got:.2}, ramp {want:.2}");
                z += 4.0;
            }
        }
    }

    /// `PD_STAIRS_ORIGINAL`'s meaning: the shipped file loads untouched (what `original()` selects);
    /// the default loader opens it. (The switch itself is read once per process, so this checks
    /// the two builders, not the env var.)
    #[test]
    fn the_original_file_is_untouched_by_the_builder_that_the_switch_selects() {
        let Some(base) = raw() else { return };
        let dir = std::env::var_os("PD_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
        let bytes = std::fs::read(dir.join("st03.pdcl")).unwrap();
        let orig = Collision::from_bytes(&bytes).unwrap();
        let mut open = base.clone();
        apply(&mut open, "st03").unwrap();
        let opened = Collision::from_raw(open);
        // A fighter-height ray along the open edge: blocked in the original, free once opened.
        let from = Vec3::new(-70.0, 60.0, -196.0);
        assert!(orig.raycast(from, Vec3::NEG_X, 40.0).is_some_and(|d| (d - 16.0).abs() < 1.0));
        assert!(opened.raycast(from, Vec3::NEG_X, 40.0).is_none_or(|d| d > 25.0));
    }

    /// Panorama (st04 / st14) was surveyed for the same block and needs no change: its ramps' wedge
    /// (x 80..110, z 114.7..159.7) is already only the headroom pocket. The soffit is 23.6 u over the
    /// floor at its high end (z 114.7) and under a fighter's 22 u from z 117.8 on, so the walls there
    /// (class 0x12 end wall at z 114.7, sides coincident with render walls) enclose only space nobody
    /// fits in. Pins the measurement; `apply` leaves the file alone.
    #[test]
    fn panorama_ramp_wedge_is_already_just_the_headroom_pocket() {
        let dir = std::env::var_os("PD_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
        let Ok(bytes) = std::fs::read(dir.join("st04.pdcl")) else { return };
        let mut r = RawCollision::parse(&bytes).unwrap();
        assert_eq!(apply(&mut r, "st04"), None);
        let col = Collision::from_raw(r);
        let clearance = |z: f32| col.raycast(Vec3::new(95.0, 57.5, z), Vec3::Y, 60.0).map(|d| d + 0.5);
        let high = clearance(115.5).expect("soffit over z 115.5");
        let mid = clearance(120.0).expect("soffit over z 120");
        assert!(high > 22.0, "high end {high}");
        assert!(mid < 22.0, "z 120: {mid}");
    }

    /// Proof helper: `PD_STAIRS_DUMP=<file>` writes the opened st03 collision file (for the
    /// before / after slice pictures; never into the cache).
    #[test]
    fn dump_opened_file() {
        let (Some(path), Some(mut r)) = (std::env::var_os("PD_STAIRS_DUMP"), raw()) else { return };
        apply(&mut r, "st03").unwrap();
        std::fs::write(path, Collision::to_bytes_classed(&r.tris, &r.owners, r.signed, &r.classes)).unwrap();
    }
}
