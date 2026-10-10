//! Lost Palace (st03) stair walk, on the REAL stage collision file.
//!
//! Laurence: "the stairs in the default arena can't be walked up, only
//! jumped" (docs/audits/P0-COLLISION.md). The two big flights (x -110..-90
//! and +90..+110, z -125..-233, y 50 -> 109) climb to the balcony. Built
//! from the ALR render mesh their treads have bevels steeper than the floor
//! limit, leaving gaps with no floor; the body drops into the hollow under
//! the stairs and a riser shoves it back, every tick, forever. The original
//! collides with the `.dat` header-0x94 mesh, where the flight is one smooth
//! 30 degree ramp (COLLISION.md "The stage collision is the 0x94 block").
//!
//! The tests need the converted cache (`PD_CACHE`, else `engine/cache` next
//! to this crate) and skip, loudly, without an st03 file.

use crate::combat::Combat;
use crate::sim::{Input, Match};
use glam::Vec3;
use pd_formats::collision::Collision;

fn load() -> Option<Collision> {
    let dir = std::env::var_os("PD_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
    let bytes = std::fs::read(dir.join("st03.pdcl")).ok()?;
    Some(Collision::from_bytes(&bytes).expect("st03.pdcl parses"))
}

/// Result of a scripted walk: the path as (x, y, z) per tick, after the
/// countdown.
struct Walk {
    path: Vec<Vec3>,
}

impl Walk {
    fn end(&self) -> Vec3 {
        *self.path.last().unwrap()
    }
    fn highest(&self) -> f32 {
        self.path.iter().map(|p| p.y).fold(f32::MIN, f32::max)
    }
    fn lowest(&self) -> f32 {
        self.path.iter().map(|p| p.y).fold(f32::MAX, f32::min)
    }
}

/// P1 (free camera, looking along `look_z` = -1 or +1) holds the stick
/// straight ahead at `stick_y` (1.0 = run, about 0.55 = walk) for `ticks`
/// after the 180-tick countdown. P2 stands far off on the hall floor.
fn walk_from(col: &mut Collision, start: Vec3, look_z: f32, stick_y: f32, ticks: usize) -> Walk {
    let ground = col.ground_below(start.x, start.z, start.y + 12.0).expect("start stands on something");
    let combat = Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
    let mut m = Match::new([Vec3::new(start.x, ground, start.z), Vec3::new(0.0, 50.0, 200.0)], combat);
    m.players[0].locked = false;
    m.players[0].cam_yaw = if look_z < 0.0 { std::f32::consts::PI } else { 0.0 };
    for _ in 0..180 {
        m.step([Input::default(), Input::default()], col);
    }
    let mut path = Vec::new();
    for _ in 0..ticks {
        m.step([Input::from_axes(0.0, stick_y, false, false), Input::default()], col);
        path.push(m.players[0].pos);
    }
    Walk { path }
}

const RUN: f32 = 1.0;
/// Processed stick magnitude about 0.25: under the 0.4 walk/run hysteresis.
const WALK: f32 = 0.55;
const BALCONY_Y: f32 = 108.5;

/// Every flight x, both gaits: the body goes from the hall floor to the
/// balcony by walking (no jump) and ends at the top wall.
#[test]
fn both_gaits_walk_up_the_big_stairs() {
    let Some(mut col) = load() else {
        eprintln!("stair_walk: no st03.pdcl in the cache: skipped");
        return;
    };
    let mut failures = Vec::new();
    for (gait, stick, ticks) in [("run", RUN, 400), ("walk", WALK, 900)] {
        for x in [-95.0, -100.0, -105.0, 95.0, 100.0, 105.0] {
            let w = walk_from(&mut col, Vec3::new(x, 50.0, -118.0), -1.0, stick, ticks);
            let e = w.end();
            eprintln!("stairs up   {gait:4} x {x:6.1}: end ({:.1}, {:.1}, {:.1}), highest {:.1}", e.x, e.y, e.z, w.highest());
            if w.highest() < BALCONY_Y {
                failures.push(format!("{gait} x {x}: stuck at y {:.1} z {:.1}", e.y, e.z));
            }
            // On the way up the body rides the ramp: it never sinks back
            // more than a hair (the trap dropped it 1.9 u every other tick).
            let mut top = f32::MIN;
            for p in w.path.iter().take_while(|p| p.y < BALCONY_Y) {
                top = top.max(p.y);
                if p.y < top - 1.0 {
                    failures.push(format!("{gait} x {x}: sank from {top:.1} to {:.1} at z {:.1}", p.y, p.z));
                    break;
                }
            }
        }
    }
    assert!(failures.is_empty(), "cannot walk up the stairs (is st03.pdcl built from render geometry? regenerate it: pd_convert collision <Map>/st03.alr <cache>/st03.pdcl): {failures:?}");
}

/// Coming back down is walkable too (it always was; pinned so a fix that
/// breaks the descent is caught).
#[test]
fn both_gaits_walk_down_the_big_stairs() {
    let Some(mut col) = load() else {
        eprintln!("stair_walk: no st03.pdcl in the cache: skipped");
        return;
    };
    for (gait, stick, ticks) in [("run", RUN, 400), ("walk", WALK, 900)] {
        for x in [-95.0, -100.0, -105.0, 95.0, 100.0, 105.0] {
            // Start on the balcony, just past the top of the flight (z -233).
            let start = Vec3::new(x, 109.0, -240.0);
            assert!(col.ground_below(start.x, start.z, start.y + 12.0).is_some_and(|g| g > 100.0), "x {x}: the balcony has floor at the top");
            let w = walk_from(&mut col, start, 1.0, stick, ticks);
            let e = w.end();
            eprintln!("stairs down {gait:4} x {x:6.1}: end ({:.1}, {:.1}, {:.1}), lowest {:.1}", e.x, e.y, e.z, w.lowest());
            assert!(w.lowest() <= 51.5, "{gait} x {x}: never reached the hall floor (lowest {:.1})", w.lowest());
        }
    }
}

/// The program's stage collision has the flight as one straight 30 degree
/// ramp (stored normal.y 0.866 > 0.643, `0x140255811`): the ground under
/// x = -100 follows 49.9 + 0.577 x (-125 - z) for z in -226..-130 (audit
/// D1, measured in the 0x94 block).
#[test]
fn the_flight_is_one_straight_ramp() {
    let Some(col) = load() else {
        eprintln!("stair_walk: no st03.pdcl in the cache: skipped");
        return;
    };
    for x in [-105.0, -100.0, -95.0, 95.0, 100.0, 105.0] {
        let mut z = -226.0;
        while z <= -130.0 {
            let want = 49.9 + 0.577 * (-125.0 - z);
            let got = col.ground_below(x, z, want + 4.0).expect("ramp has floor");
            assert!((got - want).abs() < 1.5, "x {x} z {z}: ground {got:.2}, ramp {want:.2}");
            z += 4.0;
        }
    }
}

/// Stair soffits (undersides) and class 0x1F faces are not collision: no
/// standable layer hangs between the hall floor and the ramp over it, and
/// the stage part of the file is the 0x94 mesh minus its 0x1F faces exactly.
#[test]
fn undersides_and_class_1f_faces_are_not_collision() {
    let Some(col) = load() else {
        eprintln!("stair_walk: no st03.pdcl in the cache: skipped");
        return;
    };
    let layers: Vec<f32> = col.layers_at(-100.0, -200.0).into_iter().filter(|y| *y < 140.0).collect();
    assert!(layers.iter().all(|y| *y < 52.0 || *y > 88.0), "a floor floats inside the hollow under the stairs: {layers:?}");
    assert!(layers.iter().any(|y| (y - 49.9).abs() < 0.5) && layers.iter().any(|y| (y - 92.0).abs() < 3.0), "hall floor and ramp: {layers:?}");

    let dir = std::env::var_os("PD_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
    let (Ok(pdcl), Ok(surf)) = (std::fs::read(dir.join("st03.pdcl")), std::fs::read_to_string(dir.join("st03.surf.json"))) else {
        eprintln!("stair_walk: no st03.surf.json in the cache: class check skipped");
        return;
    };
    let n = u32::from_le_bytes(pdcl[4..8].try_into().unwrap()) as usize;
    let t = 8 + n * 36;
    assert_eq!(&pdcl[t..t + 4], b"OBJT");
    let stage_tris = (0..n).filter(|i| u32::from_le_bytes(pdcl[t + 4 + i * 4..t + 8 + i * 4].try_into().unwrap()) == u32::MAX).count();
    assert_eq!(&pdcl[t + 4 + n * 4..t + 8 + n * 4], b"WIND", "a stage built from the 0x94 mesh carries the signed-winding marker");
    let j: serde_json::Value = serde_json::from_str(&surf).unwrap();
    let classes = j["class"].as_array().unwrap();
    let skipped = classes.iter().filter(|c| c.as_u64() == Some(0x1F)).count();
    assert_eq!(skipped, 309, "st03 has 309 class-0x1F faces (COLLISION.md)");
    // `pd_convert collision` (the default, the program's rule) gives exactly
    // the 0x94 faces (class 0x1F ones included, tagged: only line-of-fire rays
    // read them); `--collision dat+walls` (retired 2026-10-03) added the
    // render's steep faces on top.
    assert!(stage_tris >= classes.len() - skipped, "stage triangles ({stage_tris}) include every 0x94 face but the class 0x1F ones");
}

/// The shipped stage files carry the class trailer, in the program's classes:
/// the stage part is exactly the 0x94 mesh (no `dat+walls` render triangle, class
/// 0xFE) and its per-class counts equal the surface sidecar's.
#[test]
fn the_shipped_stage_file_is_the_programs_mesh_with_its_classes() {
    let dir = std::env::var_os("PD_CACHE").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
    let (Ok(pdcl), Ok(surf)) = (std::fs::read(dir.join("st03.pdcl")), std::fs::read_to_string(dir.join("st03.surf.json"))) else {
        eprintln!("stair_walk: no st03 files in the cache: skipped");
        return;
    };
    let n = u32::from_le_bytes(pdcl[4..8].try_into().unwrap()) as usize;
    let t = 8 + n * 36 + 4 + n * 4 + 4; // OBJT, WIND
    if &pdcl[t..(t + 4).min(pdcl.len())] != b"CLAS" {
        eprintln!("stair_walk: the cache's st03.pdcl predates the class trailer: skipped");
        return;
    }
    let classes = &pdcl[t + 4..t + 4 + n];
    let owners = |i: usize| u32::from_le_bytes(pdcl[8 + n * 36 + 4 + i * 4..8 + n * 36 + 8 + i * 4].try_into().unwrap());
    assert!(classes.iter().all(|c| *c != pd_formats::collision::CLASS_RENDER_WALL), "no non-program render wall in the shipped file");
    let j: serde_json::Value = serde_json::from_str(&surf).unwrap();
    let sidecar = j["class"].as_array().unwrap();
    for class in [0x10u64, 0x12, 0x1F] {
        let want = sidecar.iter().filter(|c| c.as_u64() == Some(class)).count();
        let got = (0..n).filter(|&i| owners(i) == u32::MAX && classes[i] as u64 == class).count();
        assert_eq!(got, want, "class {class:#x}: the file's stage faces vs the 0x94 mesh");
    }
}
