//! The stage's own shadow volumes as a point-in-volume test (lane L3).
//!
//! The original draws every shadow volume into a stencil buffer (z-fail,
//! `FUN_140299310`) and darkens the pixels whose count is non-zero: a pixel is
//! in shadow when the volumes' **winding number** around it is non-zero. The
//! exporter (`pd_formats::gltf::GRID_NODE`) writes the stage's own volumes as
//! one world-space triangle soup, extruded like `SSHADOW_vs` (fmt 0x07, by the
//! material's own length) or as stored (fmt 0x10, the big window-light pools of
//! st03/st05/st07: axis-aligned boxes, **not** extrusions of anything).
//! Bevy's shadow maps cannot answer "is this point inside a closed volume":
//! they shadow everything behind the nearest caster, so a stored box would
//! shade a long strip along the light and an extruded volume would never end.
//! This module bins the triangles in an xz grid and `shadows.wgsl` casts a ray
//! straight up from each shaded point: the sum of `sign(n.y)` over the
//! triangles above it is the winding number (outward-oriented closed meshes:
//! +1 per volume containing the point, 0 outside).
//!
//! Layout of the storage buffer (u32 words, floats bit-cast), read by
//! `shadows.wgsl::volume_winding`:
//!
//! ```text
//! 0 tri_count   1 nx   2 nz   3 cell size (f32)   4 x0 (f32)   5 z0 (f32)
//! 6 index list offset   7 triangle table offset   (word indices)
//! 8.. cells: (start, count) per cell, row-major (z * nx + x), `start` into the index list
//! then the index list (triangle numbers), then 12 words per triangle:
//!   a.xyz, sign(n.y)  b.xyz, 0  c.xyz, 0
//! ```

use bevy::math::{Vec2, Vec3};

/// Words in the header.
const HEADER: usize = 8;

/// Triangles with an xz-projected area below this are walls seen edge-on: the
/// vertical ray never crosses them.
const MIN_XZ_AREA: f32 = 1e-5;

/// A fixed sub-millimetre shift of the tested point in x / z: a point exactly
/// on an edge two triangles share (the diagonal of a box face: the centre of a
/// square floor tile in the data) would count twice. Same constants in
/// `shadows.wgsl`.
pub const JITTER: (f32, f32) = (0.000_731, 0.000_419);

/// Lift of the tested point (units) so a floor lying exactly on a volume's
/// bottom face counts as inside the volume above it, not below it.
pub const LIFT: f32 = 0.05;

/// Builds the grid buffer; `None` when there is nothing to test.
pub fn build(tris: &[[Vec3; 3]]) -> Option<Vec<u32>> {
    let kept: Vec<(usize, f32)> = tris
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            (n.y.abs() * 0.5 > MIN_XZ_AREA && n.is_finite()).then_some((i, n.y.signum()))
        })
        .collect();
    if kept.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for &(i, _) in &kept {
        for p in tris[i] {
            lo = lo.min(Vec2::new(p.x, p.z));
            hi = hi.max(Vec2::new(p.x, p.z));
        }
    }
    // About 100k cells at most, never finer than 2 units.
    let extent = (hi - lo).max(Vec2::splat(1.0));
    let cell = (extent.x * extent.y / 100_000.0).sqrt().max(2.0);
    let (nx, nz) = ((extent.x / cell).ceil() as usize + 1, (extent.y / cell).ceil() as usize + 1);
    let mut lists: Vec<Vec<u32>> = vec![Vec::new(); nx * nz];
    for (k, &(i, _)) in kept.iter().enumerate() {
        let t = tris[i];
        let p: [Vec2; 3] = std::array::from_fn(|j| Vec2::new(t[j].x, t[j].z));
        let (tmin, tmax) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
        let cx = |v: f32| (((v - lo.x) / cell).floor() as isize).clamp(0, nx as isize - 1) as usize;
        let cz = |v: f32| (((v - lo.y) / cell).floor() as isize).clamp(0, nz as isize - 1) as usize;
        for z in cz(tmin.y)..=cz(tmax.y) {
            for x in cx(tmin.x)..=cx(tmax.x) {
                let rect_lo = lo + Vec2::new(x as f32, z as f32) * cell;
                if triangle_touches_rect(&p, rect_lo, rect_lo + Vec2::splat(cell)) {
                    lists[z * nx + x].push(k as u32);
                }
            }
        }
    }
    let idx_len: usize = lists.iter().map(Vec::len).sum();
    let idx_off = HEADER + nx * nz * 2;
    let tri_off = idx_off + idx_len;
    let mut w: Vec<u32> = Vec::with_capacity(tri_off + kept.len() * 12);
    w.extend_from_slice(&[kept.len() as u32, nx as u32, nz as u32, cell.to_bits(), lo.x.to_bits(), lo.y.to_bits(), idx_off as u32, tri_off as u32]);
    let mut start = 0u32;
    for l in &lists {
        w.extend_from_slice(&[start, l.len() as u32]);
        start += l.len() as u32;
    }
    for l in &lists {
        w.extend_from_slice(l);
    }
    for &(i, sign) in &kept {
        let t = tris[i];
        for (j, p) in t.iter().enumerate() {
            w.extend_from_slice(&[p.x.to_bits(), p.y.to_bits(), p.z.to_bits(), if j == 0 { sign.to_bits() } else { 0 }]);
        }
    }
    Some(w)
}

/// Separating-axis test of a triangle (xz) against an axis-aligned rectangle
/// (the caller has already checked the two bounding boxes overlap).
fn triangle_touches_rect(p: &[Vec2; 3], lo: Vec2, hi: Vec2) -> bool {
    let corners = [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)];
    for j in 0..3 {
        let e = p[(j + 1) % 3] - p[j];
        let axis = Vec2::new(-e.y, e.x);
        let tri: Vec<f32> = p.iter().map(|q| q.dot(axis)).collect();
        let rect: Vec<f32> = corners.iter().map(|q| q.dot(axis)).collect();
        let (tl, th) = (tri.iter().cloned().fold(f32::MAX, f32::min), tri.iter().cloned().fold(f32::MIN, f32::max));
        let (rl, rh) = (rect.iter().cloned().fold(f32::MAX, f32::min), rect.iter().cloned().fold(f32::MIN, f32::max));
        if th < rl || rh < tl {
            return false;
        }
    }
    true
}

/// CPU mirror of `shadows.wgsl::volume_winding`: the winding number of the
/// volumes around `p` (non-zero = inside a volume).
#[cfg_attr(not(test), allow(dead_code))]
pub fn winding(words: &[u32], p: Vec3) -> i32 {
    if words.first().copied().unwrap_or(0) == 0 {
        return 0;
    }
    let f = |i: usize| f32::from_bits(words[i]);
    let (nx, nz, cell, x0, z0) = (words[1] as i32, words[2] as i32, f(3), f(4), f(5));
    let (idx_off, tri_off) = (words[6] as usize, words[7] as usize);
    let (cx, cz) = (((p.x - x0) / cell).floor() as i32, ((p.z - z0) / cell).floor() as i32);
    if cx < 0 || cz < 0 || cx >= nx || cz >= nz {
        return 0;
    }
    let ci = HEADER + 2 * (cz as usize * nx as usize + cx as usize);
    let (start, count) = (words[ci] as usize, words[ci + 1] as usize);
    let q = Vec3::new(p.x + JITTER.0, p.y + LIFT, p.z + JITTER.1);
    let mut w = 0;
    for k in 0..count {
        let o = tri_off + words[idx_off + start + k] as usize * 12;
        let v = |j: usize| Vec3::new(f(o + j * 4), f(o + j * 4 + 1), f(o + j * 4 + 2));
        let (a, b, c) = (v(0), v(1), v(2));
        let sign = f(o + 3);
        // Barycentric coordinates of q in the xz projection.
        let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
        if d.abs() < 1e-12 {
            continue;
        }
        let l1 = ((b.z - c.z) * (q.x - c.x) + (c.x - b.x) * (q.z - c.z)) / d;
        let l2 = ((c.z - a.z) * (q.x - c.x) + (a.x - c.x) * (q.z - c.z)) / d;
        let l3 = 1.0 - l1 - l2;
        if l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0 && l1 * a.y + l2 * b.y + l3 * c.y > q.y {
            w += sign as i32;
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed axis-aligned box as 12 triangles, outward-oriented (CCW seen
    /// from outside).
    fn cuboid(lo: Vec3, hi: Vec3, outward: bool) -> Vec<[Vec3; 3]> {
        let c = |x: bool, y: bool, z: bool| Vec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z });
        let quad = |a: Vec3, b: Vec3, cc: Vec3, d: Vec3| vec![[a, b, cc], [a, cc, d]];
        let mut t = Vec::new();
        t.extend(quad(c(false, false, true), c(true, false, true), c(true, true, true), c(false, true, true))); // +z
        t.extend(quad(c(true, false, false), c(false, false, false), c(false, true, false), c(true, true, false))); // -z
        t.extend(quad(c(true, false, true), c(true, false, false), c(true, true, false), c(true, true, true))); // +x
        t.extend(quad(c(false, false, false), c(false, false, true), c(false, true, true), c(false, true, false))); // -x
        t.extend(quad(c(false, true, true), c(true, true, true), c(true, true, false), c(false, true, false))); // +y
        t.extend(quad(c(false, false, false), c(true, false, false), c(true, false, true), c(false, false, true))); // -y
        if !outward {
            for tri in &mut t {
                tri.swap(1, 2);
            }
        }
        t
    }

    fn grid(tris: &[[Vec3; 3]]) -> Vec<u32> {
        build(tris).unwrap()
    }

    /// Inside a closed volume the winding number is 1; outside (beside, below,
    /// above) it is 0: the stencil's non-zero test, not "behind a caster".
    #[test]
    fn inside_a_closed_box_only() {
        let g = grid(&cuboid(Vec3::new(10.0, 40.0, 10.0), Vec3::new(50.0, 90.0, 60.0), true));
        assert_eq!(winding(&g, Vec3::new(30.0, 60.0, 30.0)), 1);
        assert_eq!(winding(&g, Vec3::new(30.0, 10.0, 30.0)), 0, "below the box");
        assert_eq!(winding(&g, Vec3::new(30.0, 120.0, 30.0)), 0, "above it");
        assert_eq!(winding(&g, Vec3::new(5.0, 60.0, 30.0)), 0, "beside it");
        assert_eq!(winding(&g, Vec3::new(30.0, 60.0, 70.0)), 0);
        assert_eq!(winding(&g, Vec3::new(500.0, 60.0, 30.0)), 0, "off the grid");
    }

    /// A floor exactly on the box's bottom face is inside the box above it.
    #[test]
    fn a_floor_on_the_bottom_face_is_inside() {
        let g = grid(&cuboid(Vec3::new(0.0, 50.0, 0.0), Vec3::new(40.0, 120.0, 40.0), true));
        assert_eq!(winding(&g, Vec3::new(20.0, 50.0, 20.0)), 1);
    }

    /// Overlapping volumes add (the stencil counts them): 2 inside both, and
    /// the `!= 0` test holds either way.
    #[test]
    fn overlapping_volumes_add() {
        let mut t = cuboid(Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 40.0, 40.0), true);
        t.extend(cuboid(Vec3::new(20.0, 0.0, 20.0), Vec3::new(60.0, 40.0, 60.0), true));
        let g = grid(&t);
        assert_eq!(winding(&g, Vec3::new(30.0, 20.0, 30.0)), 2);
        assert_eq!(winding(&g, Vec3::new(10.0, 20.0, 10.0)), 1);
        assert_eq!(winding(&g, Vec3::new(50.0, 20.0, 50.0)), 1);
    }

    /// An inward-oriented mesh gives -1: still non-zero, so still shadow
    /// (`NOTEQUAL 0`), as the original's counting does.
    #[test]
    fn inverted_orientation_is_still_non_zero() {
        let g = grid(&cuboid(Vec3::ZERO, Vec3::new(30.0, 30.0, 30.0), false));
        assert_eq!(winding(&g, Vec3::new(15.0, 15.0, 15.0)), -1);
    }

    /// Cell binning: a long thin diagonal volume far from the origin is found
    /// from every cell it covers (no misses at cell borders).
    #[test]
    fn binning_finds_every_covered_cell() {
        // A slab rotated 30 degrees about Y, 200 x 6 footprint.
        let (s, c) = (0.5f32, 0.866_025_4f32);
        let rot = |v: Vec3| Vec3::new(c * v.x + s * v.z, v.y, -s * v.x + c * v.z) + Vec3::new(300.0, 0.0, -150.0);
        let t: Vec<[Vec3; 3]> = cuboid(Vec3::new(-100.0, 0.0, -3.0), Vec3::new(100.0, 20.0, 3.0), true).into_iter().map(|[a, b, cc]| [rot(a), rot(b), rot(cc)]).collect();
        let g = grid(&t);
        for k in -95..=95 {
            let p = rot(Vec3::new(k as f32, 5.0, 1.0));
            assert_eq!(winding(&g, p), 1, "k = {k}");
        }
        assert_eq!(winding(&g, rot(Vec3::new(0.0, 5.0, 5.0))), 0);
    }

    /// Edge-on walls never count; a degenerate soup builds nothing.
    #[test]
    fn walls_are_ignored_and_empty_input_builds_nothing() {
        let wall = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 0.0)];
        assert!(build(&[wall]).is_none());
        assert!(build(&[]).is_none());
        assert_eq!(winding(&[], Vec3::ZERO), 0);
    }
}
