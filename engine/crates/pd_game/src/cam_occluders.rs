//! Camera-only occluders (presentation; the sim never reads them).
//!
//! The battle camera pulls its boom in to 3 u in front of any wall between it
//! and the fighter (`follow_cameras` in `main.rs`; the near-wall dissolve in
//! `cam_fade.rs` is driven by the resulting eye-to-head distance). Until
//! 2026-10-03 the camera ray-cast the stage collision, which then included
//! the render's steep triangles (rails, edge closers, pillars, machinery:
//! `--collision dat+walls`). The collclass lane retired those from the
//! collision because the program's own collision is the 0x94 mesh and the
//! object hulls only (COLLISION.md "dat+walls vs the program"), so fighters
//! and shots follow it. The camera, whose rule is UNCONFIRMED in the original
//! (GAME.md "Camera matched to the original"), keeps what Laurence approved by
//! playing: its occluders are the program collision PLUS those render walls.
//!
//! The camera-only set lives in a separate cache file, `<stage>.camcol`,
//! written by `pd_convert stages` (a `PDCL` triangle file with owner tags):
//! the render's steep faces, the model triangles of hull-less objects (st01's
//! walls and stairs, st04 / st05's signs) and the class-0x1F faces of object
//! hulls, i.e. everything the old collision had that the program's does not.
//! Gameplay never loads it; a ray against "program collision + this set" is the
//! nearer of the two rays, so it equals the old camera ray exactly. An object
//! that breaks or is thrown goes away from the camera set too (`sync_gone`).

use pd_formats::collision::Collision;
use bevy::prelude::*;
use std::path::Path;

/// The stage's camera-only occluders (the render's steep faces). Empty when
/// the stage has no `.camcol` file (a stage whose collision already is the
/// render triangles, or an old cache), in which case the camera sees only the
/// program collision.
#[derive(Resource)]
pub(crate) struct CamOccluders(pub(crate) Collision, Vec<u32>);

impl Default for CamOccluders {
    fn default() -> Self {
        Self(Collision::new(Vec::new()), Vec::new())
    }
}

impl CamOccluders {
    /// `<cache>/<stage>.camcol`, or an empty set if absent / unreadable.
    pub(crate) fn load(cache: &Path, stage: &str) -> Self {
        match std::fs::read(cache.join(format!("{stage}.camcol"))).ok().and_then(|b| Collision::from_bytes(&b).ok()) {
            Some(c) => {
                let mut owners: Vec<u32> = c.object_triangles().map(|(o, _)| o).collect();
                owners.sort_unstable();
                owners.dedup();
                Self(c, owners)
            }
            None => Self::default(),
        }
    }

    /// Mirror the sim's broken / thrown objects (`col`'s gone flags) onto the
    /// camera set, so a smashed object stops pulling the camera in.
    pub(crate) fn sync_gone(&mut self, col: &Collision) {
        for &o in &self.1 {
            let gone = col.object_gone(o);
            if self.0.object_gone(o) != gone {
                self.0.set_object_gone(o, gone);
            }
        }
    }

    /// Distance along `dir` (unit) to the first thing the CAMERA treats as a
    /// wall within `max`: the program collision `col` or a render wall.
    pub(crate) fn raycast(&self, col: &Collision, from: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        match (col.raycast(from, dir, max), self.0.raycast(from, dir, max)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_at_x(x: f32) -> Vec<[Vec3; 3]> {
        vec![[Vec3::new(x, -50.0, -50.0), Vec3::new(x, 50.0, -50.0), Vec3::new(x, 0.0, 50.0)]]
    }

    #[test]
    fn camera_sees_a_render_wall_the_program_collision_lacks() {
        let col = Collision::new(Vec::new());
        let cam = CamOccluders(Collision::new(wall_at_x(30.0)), Vec::new());
        // Gameplay collision: open. Camera: the wall 30 u away.
        assert!(col.raycast(Vec3::ZERO, Vec3::X, 60.0).is_none());
        let hit = cam.raycast(&col, Vec3::ZERO, Vec3::X, 60.0).expect("camera ray hits the render wall");
        assert!((hit - 30.0).abs() < 1e-3);
        // Nothing behind the camera.
        assert!(cam.raycast(&col, Vec3::ZERO, -Vec3::X, 60.0).is_none());
    }

    #[test]
    fn nearer_of_program_and_render_wall_wins() {
        let col = Collision::new(wall_at_x(40.0));
        let cam = CamOccluders(Collision::new(wall_at_x(25.0)), Vec::new());
        assert!((cam.raycast(&col, Vec3::ZERO, Vec3::X, 60.0).unwrap() - 25.0).abs() < 1e-3);
        let cam_far = CamOccluders(Collision::new(wall_at_x(55.0)), Vec::new());
        assert!((cam_far.raycast(&col, Vec3::ZERO, Vec3::X, 60.0).unwrap() - 40.0).abs() < 1e-3);
    }

    #[test]
    fn missing_file_means_program_collision_only() {
        let cam = CamOccluders::load(Path::new("definitely/not/a/cache"), "st99");
        let col = Collision::new(wall_at_x(20.0));
        assert!((cam.raycast(&col, Vec3::ZERO, Vec3::X, 60.0).unwrap() - 20.0).abs() < 1e-3);
        assert!(CamOccluders::default().raycast(&Collision::new(Vec::new()), Vec3::ZERO, Vec3::X, 60.0).is_none());
    }

    #[test]
    fn a_broken_object_leaves_the_camera_set_too() {
        // The camera file carries owner tags; a file round-trip keeps them.
        let bytes = Collision::to_bytes_tagged(&wall_at_x(20.0), &[7]);
        let dir = std::env::temp_dir().join(format!("pd_camocc_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("st99.camcol"), bytes).unwrap();
        let mut cam = CamOccluders::load(&dir, "st99");
        let mut sim_col = Collision::new(Vec::new());
        assert!(cam.raycast(&sim_col, Vec3::ZERO, Vec3::X, 60.0).is_some());
        sim_col.set_object_gone(7, true);
        cam.sync_gone(&sim_col);
        assert!(cam.raycast(&sim_col, Vec3::ZERO, Vec3::X, 60.0).is_none());
        sim_col.set_object_gone(7, false);
        cam.sync_gone(&sim_col);
        assert!(cam.raycast(&sim_col, Vec3::ZERO, Vec3::X, 60.0).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Needs game data: `PD_CAMCOL_OLD` = a folder of the pre-2026-10-03 `.pdcl`
    /// files (dat+walls), `PD_CAMCOL_CACHE` = a cache with the new `.pdcl` and
    /// `.camcol`. Skipped when unset. For every ray the camera could cast, the
    /// old collision and "program collision + camera set" must agree exactly.
    #[test]
    fn camera_ray_equals_the_old_collision_ray_on_real_stages() {
        let (Some(old_dir), Some(cache)) = (std::env::var_os("PD_CAMCOL_OLD"), std::env::var_os("PD_CAMCOL_CACHE")) else { return };
        let (old_dir, cache) = (std::path::PathBuf::from(old_dir), std::path::PathBuf::from(cache));
        let mut seed = 0x1234_5678_9abc_def0u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f32 / (1u64 << 53) as f32
        };
        let mut stages = 0;
        for e in std::fs::read_dir(&old_dir).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(stage) = name.strip_suffix(".pdcl") else { continue };
            let old = Collision::from_bytes(&std::fs::read(e.path()).unwrap()).unwrap();
            let new = Collision::from_bytes(&std::fs::read(cache.join(&name)).unwrap()).unwrap();
            let cam = CamOccluders::load(&cache, stage);
            let (lo, hi) = (old.min, old.max);
            if lo.x > hi.x {
                continue;
            }
            let (mut rays, mut hits) = (0, 0);
            for _ in 0..4000 {
                let from = Vec3::new(lo.x + rnd() * (hi.x - lo.x), lo.y + rnd() * (hi.y - lo.y) + 16.0, lo.z + rnd() * (hi.z - lo.z));
                let a = rnd() * std::f32::consts::TAU;
                let p = (rnd() - 0.5) * 1.2;
                let dir = Vec3::new(a.sin() * p.cos(), p.sin(), a.cos() * p.cos());
                let max = 40.0 + rnd() * 40.0;
                let (want, got) = (old.raycast(from, dir, max), cam.raycast(&new, from, dir, max));
                assert_eq!(want, got, "{stage}: ray from {from:?} dir {dir:?} max {max}");
                rays += 1;
                hits += want.is_some() as i32;
            }
            eprintln!("{stage}: {rays} rays, {hits} hits, identical");
            stages += 1;
        }
        assert!(stages > 0);
    }
}
