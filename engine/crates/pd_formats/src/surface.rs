//! Whole-stage collision surface classes: which footstep sound and VFX the
//! original plays on each patch of ground.
//!
//! Evidence: `engine/docs/COLLISION.md` "The runtime footstep class byte"
//! and `engine/docs/FORMATS-STAGES.md` "Header 0x94". Summary: the stage
//! `.dat`'s header-`0x94` block is the stage's own simplified collision mesh
//! (hull chunks in [`crate::stagedat::parse_hull`] shape), and each face
//! chunk's **flags byte** (header byte 0) is the surface class the footstep
//! dispatchers (`FUN_140230cd0` / `FUN_140230d60`) switch on: sand {3, 9,
//! 0xC}, dust {5, 6, 8, 0xB, 0x15}, water {0xD}, everything else silent. The
//! runtime ground probe skips class `0x1F` faces outright.
//!
//! This module is converter-side only and deliberately does NOT touch the
//! [`crate::collision::Collision`] type the game already consumes: the 0x94
//! mesh is a *different* mesh from the `.pdcl` (simplified collision vs ALR
//! geometry, different triangle counts), so its classes cannot ride along as
//! a per-triangle `.pdcl` trailer. Instead `pd_convert stages` writes a
//! separate sidecar (`<stage>.surf.json`) carrying this mesh, and a future
//! sim lane queries it by position ([`SurfaceMesh::class_below`]).
//!
//! Layout of the 0x94 block (reimplementation of `engine/re/wetzone_probe.py`,
//! same validation): a run of 12-byte-header chunks `{u8 flags, i8 id, u8, u8,
//! u32 body size, u32 0}`; id 3 = face chunks (10 f32 bbox + u16 count +
//! pad, then `count x {u16 a,b,c, u16 attr, f32 normal[3]}`); **one** id-8
//! vertex pool (f32 x3 each) after ALL face chunks; then the end marker
//! (id -1, size 0). Face indices address the pool that follows them.

use crate::bytes::*;
use anyhow::{bail, Result};
use std::collections::BTreeMap;

/// Header offset (bytes from `.dat` start) of the whole-stage collision block.
pub const BLOCK_HEADER_OFFSET: usize = 0x94;

/// Class the runtime ground probe skips outright (never produces a footstep).
pub const SKIPPED_CLASS: u8 = 0x1F;

/// Version tag written into every sidecar (`<stage>.surf.json`).
pub const SIDECAR_FORMAT: &str = "surface-v1";

/// Footstep dispatch families (`FUN_140230cd0` / `FUN_140230d60`).
/// Sand {3, 9, 0xC} -> `PlySandType1`, dust {5, 6, 8, 0xB, 0x15} ->
/// `PlyDustType0`, water {0xD} -> `PlyWaterType1`, everything else silent.
/// (Sand classes 3 and 0xC appear in no shipped stage data; source unknown.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Sand,
    Dust,
    Water,
    Silent,
}

/// Which footstep family a surface class belongs to.
pub fn class_kind(class: u8) -> SurfaceKind {
    match class {
        3 | 9 | 0xC => SurfaceKind::Sand,
        5 | 6 | 8 | 0xB | 0x15 => SurfaceKind::Dust,
        0xD => SurfaceKind::Water,
        _ => SurfaceKind::Silent,
    }
}

/// The 0x94 whole-stage collision mesh with one surface class per triangle.
///
/// Triangles are in right-handed export space (game z mirrored), the same
/// space as the `.pdcl` and `props.json`, so positions compare directly.
#[derive(Debug, Clone, Default)]
pub struct SurfaceMesh {
    pub tris: Vec<[glam::Vec3; 3]>,
    /// Surface class (face-chunk flags byte) per triangle.
    pub class: Vec<u8>,
    /// Chunk-run statistics, mirroring `wetzone_probe.py`'s report.
    pub chunks: usize,
    pub faces: usize,
    pub bad: usize,
    pub vertices: usize,
}

impl SurfaceMesh {
    /// Parse the 0x94 block of a stage `.dat` ("ST00" files only).
    pub fn parse(d: &[u8]) -> Result<Self> {
        Ok(Self::parse_with_normals(d)?.0)
    }

    /// Like [`SurfaceMesh::parse`], plus each triangle's STORED face normal
    /// (mirrored into export space like the vertices), in triangle order.
    /// The program decides "ground" from this stored, signed normal, never
    /// from the winding.
    pub fn parse_with_normals(d: &[u8]) -> Result<(Self, Vec<glam::Vec3>)> {
        if d.len() < BLOCK_HEADER_OFFSET + 4 || &d[0..4] != b"ST00" {
            bail!("not a stage .dat (ST00)");
        }
        let base = i32_at(d, BLOCK_HEADER_OFFSET)?;
        if base <= 0 || base as usize >= d.len() {
            bail!("0x94 block offset {base} is outside the file");
        }
        let mut off = base as usize;
        // Faces wait for the vertex pool that follows them (one shared pool
        // at the end, not one per chunk): kept per chunk with their flags.
        let mut pending: Vec<(u8, Vec<([u16; 3], [f32; 3])>)> = Vec::new();
        let mut verts: Vec<[f32; 3]> = Vec::new();
        let mut mesh = SurfaceMesh::default();
        let mut normals: Vec<glam::Vec3> = Vec::new();
        for _ in 0..100_000 {
            let flags = u8_at(d, off)?;
            let id = u8_at(d, off + 1)? as i8;
            let size = u32_at(d, off + 4)? as usize;
            let body = off + 12;
            mesh.chunks += 1;
            if id == -1 {
                // End marker: the probe stops here without validating the
                // trailer word (it is nonzero on the marker), so do the same.
                if size != 0 {
                    bail!("end marker has nonzero size at 0x{off:X}");
                }
                break;
            }
            let trailer = u32_at(d, off + 8)?;
            if trailer != 0 {
                bail!("nonzero trailer word at chunk {} (0x{off:X})", mesh.chunks);
            }
            match id {
                3 => {
                    let end = body.checked_add(size).ok_or_else(|| anyhow::anyhow!("chunk size overflows"))?;
                    if end > d.len() || size < 48 || (size - 48) % 20 != 0 {
                        mesh.bad += 1;
                    } else {
                        let n = u16_at(d, body + 40)? as usize;
                        if 48 + n * 20 != size {
                            mesh.bad += 1;
                        } else {
                            let mut faces = Vec::with_capacity(n);
                            for i in 0..n {
                                let e = body + 48 + i * 20;
                                faces.push(([u16_at(d, e)?, u16_at(d, e + 2)?, u16_at(d, e + 4)?], vec3_at(d, e + 8)?));
                            }
                            // (face attr at e+6 is {0x0, 0x34}: not the class)
                            pending.push((flags, faces));
                        }
                    }
                }
                8 => {
                    if size % 12 != 0 {
                        mesh.bad += 1;
                    } else {
                        let base_idx = verts.len();
                        for i in 0..size / 12 {
                            verts.push(vec3_at(d, body + i * 12)?);
                        }
                        // Faces index the pool that follows them.
                        for (flags, faces) in pending.drain(..) {
                            for (idx, normal) in faces {
                                let tri = idx.map(|i| {
                                    let v = verts.get(i as usize + base_idx).copied().unwrap_or([f32::NAN; 3]);
                                    crate::gltf::point_to_rh(v)
                                });
                                mesh.tris.push(tri);
                                mesh.class.push(flags);
                                normals.push(crate::gltf::point_to_rh(normal));
                            }
                        }
                    }
                }
                -1 => break, // (unreachable: handled above)
                other => bail!("unexpected 0x94 chunk id {other} at 0x{off:X}"),
            }
            off = body + size;
            if off >= d.len() {
                bail!("0x94 chunk run leaves the file at 0x{off:X}");
            }
        }
        if !pending.is_empty() {
            // Faces with no following vertex pool: unresolvable, count them
            // (never happens in shipped files: the pool follows all faces).
            mesh.bad += pending.iter().map(|(_, f)| f.len()).sum::<usize>();
        }
        mesh.faces = mesh.tris.len();
        mesh.vertices = verts.len();
        // Drop faces whose indices missed the pool (keeps the mesh usable;
        // shipped files have none: max index == pool size - 1).
        let mut keep = Vec::with_capacity(mesh.tris.len());
        for (i, t) in mesh.tris.iter().enumerate() {
            if t.iter().all(|v| v.x.is_finite()) {
                keep.push(i);
            } else {
                mesh.bad += 1;
            }
        }
        if keep.len() != mesh.tris.len() {
            let mut tris = Vec::with_capacity(keep.len());
            let mut class = Vec::with_capacity(keep.len());
            let mut kept_normals = Vec::with_capacity(keep.len());
            for i in keep {
                tris.push(mesh.tris[i]);
                class.push(mesh.class[i]);
                kept_normals.push(normals[i]);
            }
            mesh.tris = tris;
            mesh.class = class;
            normals = kept_normals;
            mesh.faces = mesh.tris.len();
        }
        Ok((mesh, normals))
    }

    /// Face count per surface class (compare with `wetzone_probe.py`).
    pub fn class_counts(&self) -> BTreeMap<u8, usize> {
        let mut m = BTreeMap::new();
        for c in &self.class {
            *m.entry(*c).or_default() += 1;
        }
        m
    }

    /// The surface class under `(x, z)`: the class of the highest non-`0x1F`
    /// triangle containing the point at or below `max_y`, if any. This is
    /// the converter-side reference for the query a future sim lane needs
    /// ("what surface is under this player?"); the game will want the same
    /// logic over a spatial index rather than this brute-force scan.
    pub fn class_below(&self, x: f32, z: f32, max_y: f32) -> Option<u8> {
        let mut best: Option<(f32, u8)> = None;
        for (t, c) in self.tris.iter().zip(&self.class) {
            if *c == SKIPPED_CLASS {
                continue;
            }
            if let Some(y) = height_at(t, x, z) {
                if y <= max_y && best.is_none_or(|b| y > b.0) {
                    best = Some((y, *c));
                }
            }
        }
        best.map(|(_, c)| c)
    }

    /// The triangles the body and rays collide with, as the original's
    /// movement code sees them: every face except class `0x1F` (skipped
    /// outright by `FUN_140255020`, `FUN_140255b70`, `FUN_140163950`,
    /// `FUN_140164770`, `FUN_140256210`, `FUN_140256390`), each wound so its
    /// cross-product normal points the way the face's STORED normal does --
    /// the program classifies ground from the stored, signed normal
    /// (`normal.y > 0.643` at `0x140255811`), so the winding must agree with
    /// it or an up-facing ramp would read as an underside. Zero-area faces
    /// and faces with no usable stored normal are dropped.
    ///
    /// In export space the stored normal is opposite to the winding-derived
    /// one on nearly every face (the z mirror reverses winding), so most
    /// triangles come back with two vertices swapped.
    pub fn collision_triangles(&self, normals: &[glam::Vec3]) -> Vec<[glam::Vec3; 3]> {
        let mut out = Vec::with_capacity(self.tris.len());
        for ((t, c), n) in self.tris.iter().zip(&self.class).zip(normals) {
            if *c == SKIPPED_CLASS {
                continue;
            }
            if let Some(w) = self.wound(t, n) {
                out.push(w);
            }
        }
        out
    }

    /// Like `collision_triangles`, but KEEPS the class-`0x1F` faces and returns
    /// each face's class beside it. `Collision` puts the 0x1F ones in a
    /// separate grid that only line-of-fire rays read (`FUN_1401641b0` stops
    /// at every class but 0x10), while bodies and objects never touch them.
    pub fn collision_triangles_classed(&self, normals: &[glam::Vec3]) -> (Vec<[glam::Vec3; 3]>, Vec<u8>) {
        let mut out = Vec::with_capacity(self.tris.len());
        let mut classes = Vec::with_capacity(self.tris.len());
        for ((t, c), n) in self.tris.iter().zip(&self.class).zip(normals) {
            if let Some(w) = self.wound(t, n) {
                out.push(w);
                classes.push(*c);
            }
        }
        (out, classes)
    }

    /// `t` wound to point along the stored normal `n`; none for a face with no
    /// usable normal or no area.
    fn wound(&self, t: &[glam::Vec3; 3], n: &glam::Vec3) -> Option<[glam::Vec3; 3]> {
        if n.length_squared() < 1e-6 || !n.is_finite() {
            return None;
        }
        let w = (t[1] - t[0]).cross(t[2] - t[0]);
        if w.length_squared() < 1e-12 {
            return None;
        }
        Some(if w.dot(*n) >= 0.0 { *t } else { [t[0], t[2], t[1]] })
    }

    /// The `<stage>.surf.json` sidecar document.
    pub fn to_json(&self, stage: &str) -> serde_json::Value {
        let r = |x: f32| (x * 1000.0).round() / 1000.0;
        let mut xyz = Vec::with_capacity(self.tris.len() * 9);
        for t in &self.tris {
            for v in t {
                xyz.push(serde_json::json!([r(v.x), r(v.y), r(v.z)]));
            }
        }
        let counts: BTreeMap<String, usize> = self.class_counts().into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        serde_json::json!({
            "format": SIDECAR_FORMAT,
            "stage": stage,
            "coords": "right-handed export space, same as the .pdcl (game z mirrored); xyz holds one [x,y,z] per triangle vertex in order, class one surface class per triangle",
            "faces": self.faces,
            "chunks": self.chunks,
            "vertices": self.vertices,
            "bad": self.bad,
            "class_counts": counts,
            "xyz": xyz,
            "class": self.class,
        })
    }
}

/// Y of triangle `t` at (x, z), if the point lies inside it in XZ.
fn height_at(t: &[glam::Vec3; 3], x: f32, z: f32) -> Option<f32> {
    let (a, b, c) = (t[0], t[1], t[2]);
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-9 {
        return None;
    }
    let w0 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let w1 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    let w2 = 1.0 - w0 - w1;
    let eps = -1e-4;
    (w0 >= eps && w1 >= eps && w2 >= eps).then(|| w0 * a.y + w1 * b.y + w2 * c.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_chunk(d: &mut Vec<u8>, flags: u8, id: u8, body: &[u8]) {
        d.push(flags);
        d.push(id);
        d.extend_from_slice(&[3, 0xFF]);
        d.extend_from_slice(&(body.len() as u32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(body);
    }

    fn face_chunk_body(bbox: [f32; 10], faces: &[(u16, u16, u16)]) -> Vec<u8> {
        let mut b = Vec::new();
        for x in bbox {
            b.extend_from_slice(&x.to_le_bytes());
        }
        b.extend_from_slice(&(faces.len() as u16).to_le_bytes());
        b.extend_from_slice(&[0u8; 6]);
        for (a, bb, c) in faces {
            b.extend_from_slice(&a.to_le_bytes());
            b.extend_from_slice(&bb.to_le_bytes());
            b.extend_from_slice(&c.to_le_bytes());
            b.extend_from_slice(&0u16.to_le_bytes()); // attr (not the class)
            for x in [0.0f32, 1.0, 0.0] {
                b.extend_from_slice(&x.to_le_bytes());
            }
        }
        b
    }

    /// Synthetic .dat: two face chunks (water + dust) sharing ONE trailing
    /// vertex pool, like the real 0x94 block.
    fn synthetic() -> Vec<u8> {
        let mut d = vec![0u8; BLOCK_HEADER_OFFSET + 4];
        d[0..4].copy_from_slice(b"ST00");
        let base = 0x200usize;
        d[BLOCK_HEADER_OFFSET..BLOCK_HEADER_OFFSET + 4].copy_from_slice(&(base as u32).to_le_bytes());
        d.resize(base, 0);
        let bbox = [0.0, 50.0, 0.0, 100.0, -100.0, 0.0, -100.0, 100.0, 0.0, 100.0];
        push_chunk(&mut d, 0xD, 3, &face_chunk_body(bbox, &[(0, 1, 2)]));
        push_chunk(&mut d, 8, 3, &face_chunk_body(bbox, &[(0, 2, 3), (0, 3, 1)]));
        let mut pool = Vec::new();
        for v in [[-100.0f32, 50.0, -100.0], [100.0, 50.0, -100.0], [100.0, 50.0, 100.0], [-100.0, 50.0, 100.0]] {
            for x in v {
                pool.extend_from_slice(&x.to_le_bytes());
            }
        }
        push_chunk(&mut d, 0x21, 8, &pool);
        push_chunk(&mut d, 0, 0xFF, &[]); // end marker: id -1, size 0
        d
    }

    #[test]
    fn parses_shared_pool_with_per_chunk_classes() {
        let m = SurfaceMesh::parse(&synthetic()).unwrap();
        assert_eq!((m.chunks, m.faces, m.vertices, m.bad), (4, 3, 4, 0));
        assert_eq!(m.class, vec![0xD, 8, 8]);
        // Right-handed export: game z is mirrored.
        assert_eq!(m.tris[0][0], glam::Vec3::new(-100.0, 50.0, 100.0));
        let counts = m.class_counts();
        assert_eq!((counts[&0xD], counts[&8]), (1, 2));
    }

    #[test]
    fn class_below_skips_0x1f_and_picks_the_top_face() {
        let m = SurfaceMesh::parse(&synthetic()).unwrap();
        // All three faces are coplanar at y 50; the first (water) wins ties.
        assert_eq!(m.class_below(0.0, 0.0, 60.0), Some(0xD));
        assert_eq!(m.class_below(0.0, 0.0, 40.0), None);
        assert_eq!(m.class_below(500.0, 500.0, 60.0), None);
    }

    #[test]
    fn skipped_class_faces_are_invisible_to_the_query() {
        let mut d = synthetic();
        d[0x200] = 0x1F; // first face chunk's flags byte -> skipped class
        let m = SurfaceMesh::parse(&d).unwrap();
        assert_eq!(m.class[0], 0x1F);
        // The water face is skipped, so the dust face underneath answers.
        assert_eq!(m.class_below(0.0, 0.0, 60.0), Some(8));
    }

    #[test]
    fn class_kind_matches_the_footstep_dispatch() {
        for c in [3, 9, 0xC] {
            assert_eq!(class_kind(c), SurfaceKind::Sand, "class {c:#X}");
        }
        for c in [5, 6, 8, 0xB, 0x15] {
            assert_eq!(class_kind(c), SurfaceKind::Dust, "class {c:#X}");
        }
        assert_eq!(class_kind(0xD), SurfaceKind::Water);
        for c in [0, 1, 2, 4, 7, 0xA, 0x10, 0x14, 0x1F, 0x20] {
            assert_eq!(class_kind(c), SurfaceKind::Silent, "class {c:#X}");
        }
    }

    #[test]
    fn malformed_blocks_are_errors_not_panics() {
        assert!(SurfaceMesh::parse(b"nope").is_err());
        let mut ar = vec![0u8; BLOCK_HEADER_OFFSET + 4];
        ar[0..4].copy_from_slice(b"AR01");
        assert!(SurfaceMesh::parse(&ar).is_err());
        // Bad face-chunk size counts as bad, not faces.
        let mut d = synthetic();
        d.truncate(d.len() - 12 - 48 - 20); // cut into the pool chunk
        assert!(SurfaceMesh::parse(&d).is_err());
    }

    #[test]
    fn sidecar_json_carries_mesh_and_counts() {
        let m = SurfaceMesh::parse(&synthetic()).unwrap();
        let j = m.to_json("st99");
        assert_eq!(j["format"], SIDECAR_FORMAT);
        assert_eq!(j["faces"], 3);
        assert_eq!(j["bad"], 0);
        assert_eq!(j["class_counts"]["13"], 1);
        assert_eq!(j["class"].as_array().unwrap().len(), 3);
        assert_eq!(j["xyz"].as_array().unwrap().len(), 9);
    }

    #[test]
    fn parse_with_normals_mirrors_the_stored_normal_like_the_vertices() {
        let (m, n) = SurfaceMesh::parse_with_normals(&synthetic()).unwrap();
        assert_eq!(n.len(), m.tris.len());
        // Stored (0, 1, 0): z mirrors to -z, y untouched.
        assert_eq!(n[0], glam::Vec3::new(0.0, 1.0, -0.0));
        assert_eq!(SurfaceMesh::parse(&synthetic()).unwrap().tris, m.tris);
    }

    #[test]
    fn collision_triangles_follow_the_stored_normal_and_drop_class_1f() {
        use glam::Vec3;
        let up = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -10.0)]; // cross.y > 0
        assert!((up[1] - up[0]).cross(up[2] - up[0]).y > 0.0);
        let mesh = SurfaceMesh { tris: vec![up, up, up, up], class: vec![8, 8, SKIPPED_CLASS, 8], ..Default::default() };
        let normals = [Vec3::Y, -Vec3::Y, Vec3::Y, Vec3::ZERO];
        let out = mesh.collision_triangles(&normals);
        // The 0x1F face and the no-normal face are gone; the other two face
        // the way their stored normals say (the second is flipped).
        assert_eq!(out.len(), 2);
        assert!((out[0][1] - out[0][0]).cross(out[0][2] - out[0][0]).y > 0.0, "up-facing stays up");
        assert!((out[1][1] - out[1][0]).cross(out[1][2] - out[1][0]).y < 0.0, "a stored down normal is wound down");
        assert_eq!(out[1][0], up[0]);
        assert_eq!((out[1][1], out[1][2]), (up[2], up[1]));
    }

    #[test]
    fn classed_collision_triangles_keep_class_1f_and_report_every_class() {
        use glam::Vec3;
        let up = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -10.0)];
        let mesh = SurfaceMesh { tris: vec![up, up, up], class: vec![8, SKIPPED_CLASS, 0x12], ..Default::default() };
        let normals = [Vec3::Y, Vec3::Y, Vec3::ZERO];
        let (tris, classes) = mesh.collision_triangles_classed(&normals);
        // The 0x1F face stays (line-of-fire rays use it); the no-normal face is dropped.
        assert_eq!((tris.len(), classes), (2, vec![8, SKIPPED_CLASS]));
        assert_eq!(mesh.collision_triangles(&normals).len(), 1, "the old export still drops class 0x1F");
    }
}
