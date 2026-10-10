//! Ground surface underfoot: which footstep sound the original plays on each
//! patch of ground, wired render-side from the converted sidecar.
//!
//! Evidence: `engine/docs/COLLISION.md` "The runtime footstep class byte…" and
//! `engine/docs/FORMATS-AUDIO.md` "Third emitter constant". Summary: the stage
//! `.dat`'s header-`0x94` block carries one surface class byte per collision
//! triangle; `pd_convert stages` exports it as `cache/<stage>.surf.json`
//! (`"format": "surface-v1"`, `xyz` per triangle vertex + one `class` per
//! triangle, in right-handed export space like the `.pdcl`). Water class `0xD`
//! plays the water-class pair, everything else plays the dry default.
//!
//! Decision (DECISIONS.md 2026-09-29, Laurence): on wet ground, cycle through
//! the wet candidates — common bank ids **52** and **54** (53/55 are
//! byte-identical duplicates of them). The regular step is common **35**,
//! identified by Laurence as `audition/27_id035.wav`.
//!
//! The cycle is a per-player rotating index, NOT an unseeded random: the sim
//! must stay replay-clean, and this is render-side anyway. The original also
//! layers the dry sound under the material sound — deliberately NOT done here
//! (Laurence asked for the wet clip on wet ground); layering stays a one-line
//! option at the call site in `footsteps.rs`.
//!
//! Render/audio only: this module never writes sim state. The brute-force
//! `class_below` scan is fine at step rates (~10 Hz; 2,723 faces on st03) —
//! the converter-side reference (`pd_formats::surface`) says the same, and a
//! spatial index can replace it later without touching callers.

use glam::Vec3;
use pd_formats::surface::{class_kind, SurfaceKind, SurfaceMesh};
use std::collections::BTreeMap;

/// The wet-step pair, `common` bank ids (DECISIONS.md 2026-09-29).
pub const WET_IDS: [u32; 2] = [52, 54];

/// Per-player wet-step rotation. Render-side only (lives in a Bevy `Local`),
/// so determinism-clean by construction: no wall clock, no unseeded random.
#[derive(Default)]
pub struct WetCycle {
    next: [u8; crate::combat::MAX_PLAYERS],
}

impl WetCycle {
    /// The next wet id for `player` (0..3; anything higher reads as the last slot),
    /// advancing that player's rotation 52 → 54 → 52 ….
    pub fn next_id(&mut self, player: usize) -> u32 {
        let slot = player.min(crate::combat::MAX_PLAYERS - 1);
        let id = WET_IDS[self.next[slot] as usize % WET_IDS.len()];
        self.next[slot] = self.next[slot].wrapping_add(1);
        id
    }
}

/// Which step sound a ground class plays: water cycles the wet pair through
/// `wet`, anything else (sand, dust, silent classes, or no ground at all)
/// plays the regular default — `sfx::SFX_FOOTSTEP` (common 35).
pub fn step_sound_for_class(class: Option<u8>, wet: &mut WetCycle, player: usize) -> u32 {
    match class.map(class_kind) {
        Some(SurfaceKind::Water) => wet.next_id(player),
        _ => crate::sfx::SFX_FOOTSTEP,
    }
}

/// A stage's loaded surface sidecar: the class mesh behind `class_below`.
#[derive(Default)]
pub struct SurfaceGround {
    mesh: SurfaceMesh,
}

impl SurfaceGround {
    /// Load `cache/<stage>.surf.json`. `None` when the file is missing or
    /// unusable (stages with no `.dat`, e.g. `st00` and the `st09_0x` areas,
    /// get no sidecar) — the caller then plays the dry default everywhere,
    /// exactly as before this lane.
    pub fn load(cache: &std::path::Path, stage: &str) -> Option<Self> {
        let text = std::fs::read_to_string(cache.join(format!("{stage}.surf.json"))).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        Self::from_json(&v)
    }

    /// Build from an already-parsed sidecar document. `None` on a version
    /// mismatch or malformed arrays (never panic on cache contents).
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        if v["format"].as_str() != Some(pd_formats::surface::SIDECAR_FORMAT) {
            return None;
        }
        let xyz = v["xyz"].as_array()?;
        let class = v["class"].as_array()?;
        if xyz.len() != class.len() * 3 {
            return None;
        }
        let mut tris = Vec::with_capacity(class.len());
        let mut classes = Vec::with_capacity(class.len());
        let pt = |p: &serde_json::Value| {
            let a = p.as_array()?;
            Some(Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32))
        };
        for (tri, c) in xyz.chunks_exact(3).zip(class.iter()) {
            let (Some(a), Some(b), Some(cc)) = (pt(&tri[0]), pt(&tri[1]), pt(&tri[2])) else { return None };
            tris.push([a, b, cc]);
            classes.push(c.as_u64()? as u8);
        }
        let faces = tris.len();
        let vertices = faces * 3;
        Some(Self { mesh: SurfaceMesh { tris, class: classes, chunks: 0, faces, bad: 0, vertices } })
    }

    /// Face count per surface class (logged on load under `PD_SURFACE_LOG`;
    /// diagnostics, parity with the converter).
    pub fn class_counts(&self) -> BTreeMap<u8, usize> {
        self.mesh.class_counts()
    }

    /// The surface class under `(x, z)`: the class of the highest non-`0x1F`
    /// triangle containing the point at or below `max_y`, if any — the same
    /// answer as the converter-side reference (`SurfaceMesh::class_below`),
    /// over the sidecar mesh instead of the `.dat`.
    pub fn class_below(&self, x: f32, z: f32, max_y: f32) -> Option<u8> {
        self.mesh.class_below(x, z, max_y)
    }

    /// The surface class of the nearest triangle a segment `from -> to` crosses (two-sided, like the
    /// program's segment-vs-face test `FUN_1401d22e0`; not counting the probe-excluded 0x1F faces):
    /// what a shot that ended on the stage between those two points hit.
    pub fn class_along(&self, from: Vec3, to: Vec3) -> Option<u8> {
        let d = to - from;
        let mut best: Option<(f32, u8)> = None;
        for (t, &c) in self.mesh.tris.iter().zip(&self.mesh.class) {
            if c == pd_formats::surface::SKIPPED_CLASS {
                continue;
            }
            if let Some(h) = segment_triangle(from, d, t) {
                if best.is_none_or(|b| h < b.0) {
                    best = Some((h, c));
                }
            }
        }
        best.map(|(_, c)| c)
    }

    /// The first triangle, in the stage file's own order, that a sphere touches (centre closer than
    /// `radius`, either side), with its class: what the program's sphere probe answers. READ: the stage
    /// collision list `DAT_1405322c0` is built from the `.dat` 0x94 chunks in file order
    /// (`FUN_1401d4070`), the sphere-vs-chunk test `FUN_1401d7fd0` is a two-sided plane distance below
    /// the radius plus an inside-or-edge test, and the per-tick walker `FUN_1402021f0` keeps, for a
    /// sphere probe, the contact of the FIRST chunk in that list (not the nearest). No class is skipped:
    /// a 0x1F face can win and then plays nothing. Triangles keep chunk order, so the lowest index wins.
    pub fn first_touching(&self, centre: Vec3, radius: f32) -> Option<(usize, u8)> {
        self.mesh.tris.iter().zip(&self.mesh.class).enumerate().find(|(_, (t, _))| dist_point_triangle(centre, t) < radius).map(|(i, (_, &c))| (i, c))
    }

    /// Where the program puts the impact mark of a sphere contact on triangle `i`: the centre projected
    /// onto the face's plane (`FUN_1401d7fd0` stores centre - n * distance), then one unit along the face
    /// normal (`FUN_14021d3f0` adds contact +0x38). The stored normal's sign is not in the sidecar: the
    /// side the centre is on is used (UNCONFIRMED, at most 2 units).
    pub fn mark_point(&self, i: usize, centre: Vec3) -> Option<Vec3> {
        let [a, b, c] = *self.mesh.tris.get(i)?;
        let n = (b - a).cross(c - a).normalize_or_zero();
        let d = (centre - a).dot(n);
        let side = if d < 0.0 { -n } else { n };
        Some(centre - n * d + side)
    }

    /// The surface class of the triangle nearest `p` within `reach` units (not counting the
    /// probe-excluded 0x1F faces): what a shot or a blast touching the stage at `p` hits, as
    /// opposed to what is under a body's feet. Ties go to the first triangle in file order.
    pub fn class_nearest(&self, p: Vec3, reach: f32) -> Option<u8> {
        let mut best: Option<(f32, u8)> = None;
        for (t, &c) in self.mesh.tris.iter().zip(&self.mesh.class) {
            if c == pd_formats::surface::SKIPPED_CLASS {
                continue;
            }
            let d = dist_point_triangle(p, t);
            if d <= reach && best.is_none_or(|b| d < b.0) {
                best = Some((d, c));
            }
        }
        best.map(|(_, c)| c)
    }
}

/// Where the segment `origin + t * dir`, 0 <= t <= 1, crosses triangle `tri` (Moeller-Trumbore, no
/// facing test): the parameter t, or `None`.
fn segment_triangle(origin: Vec3, dir: Vec3, tri: &[Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (tri[1] - tri[0], tri[2] - tri[0]);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - tri[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (0.0..=1.0).contains(&t).then_some(t)
}

/// Distance from `p` to the triangle `t` (Ericson, Real-Time Collision Detection 5.1.5).
fn dist_point_triangle(p: Vec3, t: &[Vec3; 3]) -> f32 {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return ap.length();
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return bp.length();
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (p - (a + ab * v)).length();
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return cp.length();
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (p - (a + ac * w)).length();
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (p - (b + (c - b) * w)).length();
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    (p - (a + ab * v + ac * w)).length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sidecar() -> serde_json::Value {
        // One water triangle at y 50 over x/z −100…100, one dust triangle
        // above it (y 60), one skipped-class (0x1F) triangle higher still.
        json!({
            "format": "surface-v1",
            "xyz": [
                [-100.0, 50.0, -100.0], [100.0, 50.0, -100.0], [100.0, 50.0, 100.0],
                [-100.0, 60.0, -100.0], [100.0, 60.0, -100.0], [100.0, 60.0, 100.0],
                [-100.0, 70.0, -100.0], [100.0, 70.0, -100.0], [100.0, 70.0, 100.0],
            ],
            "class": [0xD, 8, 0x1F],
        })
    }

    #[test]
    fn wet_steps_cycle_52_54_per_player() {
        // DECISIONS.md 2026-09-29: cycle the wet candidates per wet step.
        let mut wet = WetCycle::default();
        assert_eq!((wet.next_id(0), wet.next_id(0), wet.next_id(0), wet.next_id(0)), (52, 54, 52, 54));
        // P2 has its own rotation: untouched by P1's four steps.
        assert_eq!((wet.next_id(1), wet.next_id(1)), (52, 54));
        // And P1's rotation kept going where it left off.
        assert_eq!(wet.next_id(0), 52);
    }

    #[test]
    fn players_three_and_four_keep_their_own_wet_rotation() {
        let mut wet = WetCycle::default();
        assert_eq!((wet.next_id(2), wet.next_id(2), wet.next_id(2)), (52, 54, 52));
        assert_eq!((wet.next_id(3), wet.next_id(1)), (52, 52), "players 4 and 2 start their own rotations, untouched by player 3's steps");
        assert_eq!(wet.next_id(2), 54);
    }

    #[test]
    fn step_sound_is_wet_only_on_water() {
        let mut wet = WetCycle::default();
        // Water cycles the pair.
        assert_eq!(step_sound_for_class(Some(0xD), &mut wet, 0), 52);
        assert_eq!(step_sound_for_class(Some(0xD), &mut wet, 0), 54);
        // Sand, dust, silent classes and no ground: the dry default, untouched.
        for c in [Some(3), Some(9), Some(0xC), Some(5), Some(6), Some(8), Some(0xB), Some(0x15), Some(0), Some(2), Some(0x1F), Some(0x20), None] {
            assert_eq!(step_sound_for_class(c, &mut wet, 0), crate::sfx::SFX_FOOTSTEP, "class {c:?}");
        }
        // Laurence's exact audition-file identification, not the emitter id.
        assert_eq!(crate::sfx::SFX_FOOTSTEP, 35);
    }

    #[test]
    fn sidecar_loads_and_answers_class_below() {
        let g = SurfaceGround::from_json(&sidecar()).unwrap();
        assert_eq!(g.class_counts(), BTreeMap::from([(0xD, 1), (8, 1), (0x1F, 1)]));
        // The 0x1F face is skipped: the dust face below it answers.
        assert_eq!(g.class_below(0.0, 0.0, 100.0), Some(8));
        // Below the dust: the water face.
        assert_eq!(g.class_below(0.0, 0.0, 55.0), Some(0xD));
        // Above the water but below the dust: nothing (max_y gates).
        assert_eq!(g.class_below(0.0, 0.0, 40.0), None);
        // Off the mesh: nothing.
        assert_eq!(g.class_below(500.0, 500.0, 100.0), None);
    }

    /// What a shot or blast touches: the nearest non-0x1F triangle within reach, any orientation.
    #[test]
    fn class_nearest_finds_the_closest_face_within_reach() {
        let g = SurfaceGround::from_json(&sidecar()).unwrap();
        // Above the water (y 50) face, between it and the dust (y 60) face: the nearer one wins.
        assert_eq!(g.class_nearest(Vec3::new(0.0, 53.0, 0.0), 10.0), Some(0xD));
        assert_eq!(g.class_nearest(Vec3::new(0.0, 57.0, 0.0), 10.0), Some(8));
        // The 0x1F face at y 70 is never answered: from y 69 the dust face (9 away) is the nearest.
        assert_eq!(g.class_nearest(Vec3::new(0.0, 69.0, 0.0), 20.0), Some(8));
        // Out of reach / off the mesh.
        assert_eq!(g.class_nearest(Vec3::new(0.0, 53.0, 0.0), 2.0), None);
        assert_eq!(g.class_nearest(Vec3::new(500.0, 50.0, 500.0), 20.0), None);
        // Off the side of a triangle: the distance is to its edge.
        assert_eq!(g.class_nearest(Vec3::new(0.0, 50.0, -110.0), 12.0), Some(0xD));
    }

    /// The sphere probe answers the first touched triangle in file order, not the nearest, and a 0x1F
    /// face counts.
    #[test]
    fn first_touching_takes_file_order_not_distance() {
        let g = SurfaceGround::from_json(&sidecar()).unwrap();
        // Touching the water (y 50, index 0) and the dust (y 60, index 1): the water is first in the file,
        // although the dust face is nearer.
        assert_eq!(g.first_touching(Vec3::new(0.0, 58.0, 0.0), 10.0), Some((0, 0xD)));
        // Only the dust and the 0x1F faces in reach: the dust comes first.
        assert_eq!(g.first_touching(Vec3::new(0.0, 66.0, 0.0), 8.0), Some((1, 8)));
        // Only the 0x1F face in reach: it still answers.
        assert_eq!(g.first_touching(Vec3::new(0.0, 72.0, 0.0), 5.0), Some((2, 0x1F)));
        // Strictly closer than the radius: exactly at the radius is no touch.
        assert_eq!(g.first_touching(Vec3::new(0.0, 40.0, 0.0), 10.0), None);
        assert_eq!(g.first_touching(Vec3::new(500.0, 50.0, 500.0), 20.0), None);
    }

    /// A shot's last step: the nearest non-0x1F face the segment crosses, from either side.
    #[test]
    fn class_along_finds_the_first_face_a_step_crosses() {
        let g = SurfaceGround::from_json(&sidecar()).unwrap();
        // Straight down from above everything: the 0x1F face at y 70 is skipped, the dust face (y 60) is first.
        assert_eq!(g.class_along(Vec3::new(0.0, 100.0, 0.0), Vec3::new(0.0, 40.0, 0.0)), Some(8));
        // Up from below: the water face at y 50 is crossed first.
        assert_eq!(g.class_along(Vec3::new(0.0, 40.0, 0.0), Vec3::new(0.0, 100.0, 0.0)), Some(0xD));
        // A step that stops short, or beside the faces, crosses nothing.
        assert_eq!(g.class_along(Vec3::new(0.0, 100.0, 0.0), Vec3::new(0.0, 65.0, 0.0)), None);
        assert_eq!(g.class_along(Vec3::new(500.0, 100.0, 0.0), Vec3::new(500.0, 0.0, 0.0)), None);
    }

    #[test]
    fn malformed_sidecars_are_none_not_panics() {
        assert!(SurfaceGround::from_json(&json!({"format": "surface-v0", "xyz": [], "class": []})).is_none());
        assert!(SurfaceGround::from_json(&json!({"format": "surface-v1", "xyz": [[0.0, 0.0, 0.0]], "class": []})).is_none());
        assert!(SurfaceGround::from_json(&json!({"format": "surface-v1", "xyz": [["a", 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]], "class": [13]})).is_none());
        assert!(SurfaceGround::from_json(&json!({})).is_none());
    }
}
