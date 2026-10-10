//! Stage collision: triangle soup bucketed in a uniform XZ grid.
//!
//! Floors (standable) and walls (steep) are kept apart: floors answer
//! "where is the ground under me", walls push players out sideways, and both
//! block camera rays. Pure data with no engine types, so the simulation uses
//! it identically on every machine.
//!
//! Two classification rules, chosen per file:
//!
//! * legacy (no `WIND` marker): every triangle is a floor if `|normal.y|` is
//!   at least [`WALKABLE_NORMAL_Y`], either winding -- used for collision
//!   built from ALR render geometry, whose winding means nothing;
//! * signed (`WIND` marker, stage triangles built from the `.dat` header-0x94
//!   mesh wound to their stored normals): a STAGE triangle is ground only if
//!   its normal points up by at least [`STAGE_FLOOR_NORMAL_Y`]
//!   (`0x140255811`); one that points down that steeply is a ceiling (never
//!   stood on, never pushes sideways, still blocks rays); the rest are walls.
//!   Object hulls keep the legacy rule either way.
//!
//! Triangles can belong to a stage object (`stNN.props.json` index): an
//! optional `OBJT` trailer in the file tags each one. The simulation switches
//! an object's triangles off when it breaks or is thrown (`set_object_gone`).
//!
//! Every triangle can also carry the program's surface CLASS (the hull chunk's
//! flags byte): an optional `CLAS` trailer, one byte per triangle, after
//! `OBJT` / `WIND`. The program's queries ignore some classes (COLLISION.md
//! "Per-triangle class"):
//!
//! * fighters' bodies (walls, ground) skip class `0x1F` (and `0x14` while
//!   airborne, `FUN_140255020`);
//! * objects -- shots, crawlers, tornadoes, area markers -- skip `0x10`,
//!   `0x12`, `0x14` and `0x1F` (`FUN_140163e80`, `FUN_140163cb0`,
//!   `FUN_140164330`: mask `0x80150000 >> class`);
//! * the line-of-fire ray every hit test uses (`FUN_1401641b0`: blast, body
//!   hit, area) is stopped by every class EXCEPT `0x10`.
//!
//! Class-`0x1F` faces therefore live in their own grid (`hidden`): bodies and
//! objects never see them, line-of-fire rays do. Files with no `CLAS` trailer
//! load with class 0 everywhere (never skipped), exactly as before.

use anyhow::{bail, Result};
use glam::Vec3;
use std::collections::HashMap;

/// Surfaces steeper than this (normal.y below it) are not standable.
pub const WALKABLE_NORMAL_Y: f32 = 0.6;
/// The program's ground test: a stored face normal with y above this is
/// ground (`FUN_140255020` at `0x140255811`, constant `0x1402f7d50` = 0.643).
pub const STAGE_FLOOR_NORMAL_Y: f32 = 0.643;
const CELL: f32 = 32.0;

/// Owner tag of a triangle that belongs to the stage itself.
pub const NO_OBJECT: u32 = u32::MAX;

/// Class of a triangle in a file with no `CLAS` trailer (nothing is skipped).
pub const CLASS_NONE: u8 = 0;
/// The huge outer boundary box of a stage: the line-of-fire ray passes it.
pub const CLASS_WORLD_BOX: u8 = 0x10;
/// Faces no body and no object ever touches; only line-of-fire rays do.
pub const CLASS_HIDDEN: u8 = 0x1F;
/// A triangle that is NOT from the program's collision: a steep render
/// triangle added by `pd_convert ... --collision dat+walls`.
pub const CLASS_RENDER_WALL: u8 = 0xFE;
/// The object queries' skip mask, indexed by class (`0x80150000` at
/// `FUN_140163e80`, `FUN_140163cb0`, `FUN_140163b50`, `FUN_140164330`):
/// classes 0x10, 0x12, 0x14, 0x1F.
pub const OBJECT_SKIP_MASK: u32 = 0x8015_0000;

/// Does an object query (shot / crawler / tornado / area marker) ignore a
/// face of this class?
pub fn object_skips(class: u8) -> bool {
    class < 32 && (OBJECT_SKIP_MASK >> class) & 1 != 0
}

/// Does the line-of-fire ray (`FUN_1401641b0`) pass a face of this class?
/// Only `0x10` -- the stage's outer box -- is transparent to it.
pub fn los_passes(class: u8) -> bool {
    class == CLASS_WORLD_BOX
}

#[derive(Default, Clone)]
struct Grid {
    tris: Vec<[Vec3; 3]>,
    /// Object each triangle belongs to (`NO_OBJECT` = the stage).
    owner: Vec<u32>,
    /// Surface class of each triangle (`CLASS_NONE` = unknown).
    class: Vec<u8>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl Grid {
    fn insert(&mut self, t: [Vec3; 3], owner: u32, class: u8) {
        let (mut lo, mut hi) = (t[0], t[0]);
        for v in &t {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        // Skip giant triangles (sky domes and sea planes span the world).
        if (hi.x - lo.x) > CELL * 256.0 || (hi.z - lo.z) > CELL * 256.0 {
            return;
        }
        let i = self.tris.len() as u32;
        self.tris.push(t);
        self.owner.push(owner);
        self.class.push(class);
        for cx in cell_of(lo.x)..=cell_of(hi.x) {
            for cz in cell_of(lo.z)..=cell_of(hi.z) {
                self.cells.entry((cx, cz)).or_default().push(i);
            }
        }
    }

    fn at(&self, x: f32, z: f32) -> impl Iterator<Item = u32> + '_ {
        self.cells.get(&(cell_of(x), cell_of(z))).into_iter().flatten().copied()
    }

    /// Triangles in every cell touching the XZ box [lo, hi], without duplicates.
    fn in_box(&self, lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        for cx in cell_of(lo.x)..=cell_of(hi.x) {
            for cz in cell_of(lo.z)..=cell_of(hi.z) {
                if let Some(v) = self.cells.get(&(cx, cz)) {
                    out.extend_from_slice(v);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

enum Kind {
    Floor,
    Wall,
    Ceiling,
    Hidden,
}

fn cell_of(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

/// A collision file as stored, before bucketing: the triangles with their optional
/// owner / class trailers. Load-time filters edit this and then call
/// `Collision::from_raw` (`pd_game::stairs_under` removes the Lost Palace's invisible
/// walls this way). `owners` / `classes` are empty when the file has no such trailer.
#[derive(Clone, Debug, Default)]
pub struct RawCollision {
    pub tris: Vec<[Vec3; 3]>,
    pub owners: Vec<u32>,
    /// The `WIND` marker: stage triangles are classified by signed normal.
    pub signed: bool,
    pub classes: Vec<u8>,
}

impl RawCollision {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.len() < 8 || &d[..4] != b"PDCL" {
            bail!("not a collision file");
        }
        let n = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize;
        if d.len() < 8 + n * 36 {
            bail!("collision file truncated");
        }
        let f = |o: usize| f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let tris = (0..n)
            .map(|i| {
                let b = 8 + i * 36;
                [0, 1, 2].map(|k| Vec3::new(f(b + k * 12), f(b + k * 12 + 4), f(b + k * 12 + 8)))
            })
            .collect();
        let t = 8 + n * 36;
        let owners: Vec<u32> = if d.len() >= t + 4 + n * 4 && &d[t..t + 4] == b"OBJT" {
            (0..n).map(|i| u32::from_le_bytes(d[t + 4 + i * 4..t + 8 + i * 4].try_into().unwrap())).collect()
        } else {
            Vec::new()
        };
        // The signed-winding marker follows the OBJT block (when there is one).
        let m = t + if owners.is_empty() { 0 } else { 4 + n * 4 };
        let signed = !owners.is_empty() && d.len() >= m + 4 && &d[m..m + 4] == b"WIND";
        // The class trailer follows whatever came before it.
        let c = m + if signed { 4 } else { 0 };
        let classes: Vec<u8> = if d.len() >= c + 4 + n && &d[c..c + 4] == b"CLAS" { d[c + 4..c + 4 + n].to_vec() } else { Vec::new() };
        Ok(RawCollision { tris, owners, signed, classes })
    }

    /// Owner of triangle `i` (`NO_OBJECT` when the file has no `OBJT` trailer).
    pub fn owner(&self, i: usize) -> u32 {
        self.owners.get(i).copied().unwrap_or(NO_OBJECT)
    }

    /// Class of triangle `i` (`CLASS_NONE` when the file has no `CLAS` trailer).
    pub fn class(&self, i: usize) -> u8 {
        self.classes.get(i).copied().unwrap_or(CLASS_NONE)
    }

    /// Keep only the triangles `keep` accepts (`index, triangle, owner, class`), the
    /// trailers staying aligned. Returns how many were dropped.
    pub fn retain(&mut self, mut keep: impl FnMut(usize, &[Vec3; 3], u32, u8) -> bool) -> usize {
        let n = self.tris.len();
        let flags: Vec<bool> = (0..n).map(|i| keep(i, &self.tris[i], self.owner(i), self.class(i))).collect();
        let (has_o, has_c) = (!self.owners.is_empty(), !self.classes.is_empty());
        let mut i = 0;
        self.tris.retain(|_| {
            i += 1;
            flags[i - 1]
        });
        if has_o {
            let mut i = 0;
            self.owners.retain(|_| {
                i += 1;
                flags[i - 1]
            });
        }
        if has_c {
            let mut i = 0;
            self.classes.retain(|_| {
                i += 1;
                flags[i - 1]
            });
        }
        n - self.tris.len()
    }

    /// Append a triangle (aligning the trailers: a stage triangle of the given class).
    pub fn push(&mut self, tri: [Vec3; 3], owner: u32, class: u8) {
        let n = self.tris.len();
        if self.owners.is_empty() && owner != NO_OBJECT {
            self.owners = vec![NO_OBJECT; n];
        }
        if self.classes.is_empty() && class != CLASS_NONE {
            self.classes = vec![CLASS_NONE; n];
        }
        self.tris.push(tri);
        if !self.owners.is_empty() {
            self.owners.push(owner);
        }
        if !self.classes.is_empty() {
            self.classes.push(class);
        }
    }
}

pub struct Collision {
    floors: Grid,
    walls: Grid,
    /// Signed files only: stage faces that point down (undersides). Not
    /// ground, not walls; rays still hit them.
    ceilings: Grid,
    /// Class-`0x1F` faces: line-of-fire rays only (see the module doc).
    hidden: Grid,
    pub min: Vec3,
    pub max: Vec3,
    /// Objects whose triangles are switched off (broken, thrown).
    gone: Vec<bool>,
    /// The file's `WIND` marker: stage triangles are wound to their stored normals.
    signed: bool,
}

impl Collision {
    pub fn to_bytes(tris: &[[Vec3; 3]]) -> Vec<u8> {
        Self::to_bytes_tagged(tris, &[])
    }

    /// Like `to_bytes`, plus an `OBJT` trailer: one owner per triangle
    /// (`NO_OBJECT` for the stage). `owners` may be empty (no trailer).
    pub fn to_bytes_tagged(tris: &[[Vec3; 3]], owners: &[u32]) -> Vec<u8> {
        Self::to_bytes_full(tris, owners, false)
    }

    /// Like `to_bytes_tagged`; `signed` adds the `WIND` marker saying the
    /// stage triangles' winding is authoritative (built from the `.dat`
    /// 0x94 mesh wound to its stored normals), so the loader classifies
    /// them by signed normal. Needs `owners` (the marker follows the
    /// `OBJT` block).
    pub fn to_bytes_full(tris: &[[Vec3; 3]], owners: &[u32], signed: bool) -> Vec<u8> {
        Self::to_bytes_classed(tris, owners, signed, &[])
    }

    /// Like `to_bytes_full`, plus a `CLAS` trailer: one surface class per
    /// triangle (the program's hull-chunk flags byte; `CLASS_NONE` = unknown).
    /// `classes` may be empty (no trailer).
    pub fn to_bytes_classed(tris: &[[Vec3; 3]], owners: &[u32], signed: bool, classes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + tris.len() * 40 + 4);
        out.extend_from_slice(b"PDCL");
        out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
        for t in tris {
            for v in t {
                for c in v.to_array() {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        if !owners.is_empty() {
            assert_eq!(owners.len(), tris.len(), "one owner per triangle");
            out.extend_from_slice(b"OBJT");
            for o in owners {
                out.extend_from_slice(&o.to_le_bytes());
            }
        }
        if signed {
            assert!(!owners.is_empty(), "the signed marker rides behind the OBJT block");
            out.extend_from_slice(b"WIND");
        }
        if !classes.is_empty() {
            assert_eq!(classes.len(), tris.len(), "one class per triangle");
            out.extend_from_slice(b"CLAS");
            out.extend_from_slice(classes);
        }
        out
    }

    pub fn from_bytes(d: &[u8]) -> Result<Self> {
        Ok(Self::from_raw(RawCollision::parse(d)?))
    }

    /// Bucket a parsed file (`RawCollision::parse`, possibly edited) into the grids.
    pub fn from_raw(raw: RawCollision) -> Self {
        Self::build(raw.tris, &raw.owners, raw.signed, &raw.classes)
    }

    pub fn new(all: Vec<[Vec3; 3]>) -> Self {
        Self::new_tagged(all, &[])
    }

    /// `owners[i]` = the object triangle `i` belongs to (missing = the stage).
    pub fn new_tagged(all: Vec<[Vec3; 3]>, owners: &[u32]) -> Self {
        Self::build(all, owners, false, &[])
    }

    /// Like `new_tagged`, with the stage triangles' winding authoritative:
    /// classified by signed normal (see the module doc). What a file with
    /// the `WIND` marker loads as.
    pub fn new_signed(all: Vec<[Vec3; 3]>, owners: &[u32]) -> Self {
        Self::build(all, owners, true, &[])
    }

    /// `new_signed` / `new_tagged` with a surface class per triangle (what a
    /// file with a `CLAS` trailer loads as).
    pub fn new_classed(all: Vec<[Vec3; 3]>, owners: &[u32], signed: bool, classes: &[u8]) -> Self {
        Self::build(all, owners, signed, classes)
    }

    fn build(all: Vec<[Vec3; 3]>, owners: &[u32], signed: bool, classes: &[u8]) -> Self {
        let mut c = Collision { floors: Grid::default(), walls: Grid::default(), ceilings: Grid::default(), hidden: Grid::default(), min: Vec3::MAX, max: Vec3::MIN, gone: Vec::new(), signed };
        for (k, t) in all.into_iter().enumerate() {
            let owner = owners.get(k).copied().unwrap_or(NO_OBJECT);
            let class = classes.get(k).copied().unwrap_or(CLASS_NONE);
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            let len = n.length();
            if len < 1e-6 {
                continue;
            }
            let ny = n.y / len;
            let kind = if class == CLASS_HIDDEN {
                // No body and no object touches these; only line-of-fire rays.
                Kind::Hidden
            } else if signed && owner == NO_OBJECT {
                if ny >= STAGE_FLOOR_NORMAL_Y {
                    Kind::Floor
                } else if ny <= -STAGE_FLOOR_NORMAL_Y {
                    Kind::Ceiling
                } else {
                    Kind::Wall
                }
            } else if ny.abs() >= WALKABLE_NORMAL_Y {
                // Legacy: either winding may face up after the mirror; accept both.
                Kind::Floor
            } else {
                Kind::Wall
            };
            match kind {
                Kind::Floor => {
                    for v in &t {
                        c.min = c.min.min(*v);
                        c.max = c.max.max(*v);
                    }
                    c.floors.insert(t, owner, class);
                }
                Kind::Wall => c.walls.insert(t, owner, class),
                Kind::Ceiling => c.ceilings.insert(t, owner, class),
                Kind::Hidden => c.hidden.insert(t, owner, class),
            }
        }
        c
    }

    /// Switch object `obj`'s triangles off (`true`) or back on.
    pub fn set_object_gone(&mut self, obj: u32, gone: bool) {
        let i = obj as usize;
        if i >= self.gone.len() {
            if !gone {
                return;
            }
            self.gone.resize(i + 1, false);
        }
        self.gone[i] = gone;
    }

    /// Is object `obj`'s collision switched off?
    pub fn object_gone(&self, obj: u32) -> bool {
        self.gone.get(obj as usize).copied().unwrap_or(false)
    }

    fn live(&self, owner: u32) -> bool {
        owner == NO_OBJECT || !self.object_gone(owner)
    }

    /// Every object's triangles, as (object, triangle): the sim sizes objects from them.
    pub fn object_triangles(&self) -> impl Iterator<Item = (u32, &[Vec3; 3])> {
        [&self.floors, &self.walls].into_iter().flat_map(|g| g.owner.iter().zip(&g.tris)).filter(|(o, _)| **o != NO_OBJECT).map(|(o, t)| (*o, t))
    }

    /// Height of the highest standable surface under (x, z) at or below
    /// `max_y`, if any.
    pub fn ground_below(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for i in self.floors.at(x, z) {
            if !self.live(self.floors.owner[i as usize]) {
                continue;
            }
            if let Some(y) = height_at(&self.floors.tris[i as usize], x, z) {
                if y <= max_y && best.is_none_or(|b| y > b) {
                    best = Some(y);
                }
            }
        }
        best
    }

    /// Every standable surface height under (x, z), highest first.
    pub fn layers_at(&self, x: f32, z: f32) -> Vec<f32> {
        let mut v: Vec<f32> = self.floors.at(x, z).filter(|&i| self.live(self.floors.owner[i as usize])).filter_map(|i| height_at(&self.floors.tris[i as usize], x, z)).collect();
        v.sort_by(|a, b| b.total_cmp(a));
        v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        v
    }

    /// Push a vertical cylinder (feet at `pos`, given radius and height) out
    /// of walls, horizontally. Returns the corrected position.
    pub fn push_out_of_walls(&self, pos: Vec3, radius: f32, bottom: f32, top: f32) -> Vec3 {
        let mut p = pos;
        let mut ids = Vec::new();
        let r = Vec3::new(radius, 0.0, radius);
        // A few relaxation passes settle corners.
        for _ in 0..3 {
            self.walls.in_box(p - r, p + r, &mut ids);
            let mut moved = false;
            for &i in &ids {
                if !self.live(self.walls.owner[i as usize]) {
                    continue;
                }
                let t = &self.walls.tris[i as usize];
                for h in [bottom, (bottom + top) * 0.5, top] {
                    let c = Vec3::new(p.x, p.y + h, p.z);
                    let q = closest_on_triangle(c, t);
                    let d = Vec3::new(c.x - q.x, 0.0, c.z - q.z);
                    let dy = (c.y - q.y).abs();
                    let dist = d.length();
                    if dy < 1.0 && dist < radius && dist > 1e-4 {
                        p += d / dist * (radius - dist);
                        moved = true;
                    }
                }
            }
            if !moved {
                break;
            }
        }
        p
    }

    /// Distance along `dir` (unit) from `from` to the first surface within
    /// `max`, if any. Used to keep cameras in front of scenery.
    pub fn raycast(&self, from: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.raycast_owner(from, dir, max).map(|(t, _)| t)
    }

    /// Like `raycast`, plus what was hit: an object index, or `NO_OBJECT`.
    pub fn raycast_owner(&self, from: Vec3, dir: Vec3, max: f32) -> Option<(f32, u32)> {
        self.raycast_ignoring(from, dir, max, None)
    }

    /// Like `raycast_owner`, passing through object `ignore` (a thrown object's own hull).
    /// What a body (or a generic ray: dash, camera, navigation) sees: every
    /// face except the class-`0x1F` ones.
    pub fn raycast_ignoring(&self, from: Vec3, dir: Vec3, max: f32, ignore: Option<u32>) -> Option<(f32, u32)> {
        self.first_hit(from, dir, max, ignore, &[&self.floors, &self.walls, &self.ceilings], |_| false).map(|(t, o, _)| (t, o))
    }

    /// Like `raycast_ignoring`, plus the unit normal of the face that was hit,
    /// turned to face the ray's origin (a ricochet reflects off it).
    pub fn raycast_normal(&self, from: Vec3, dir: Vec3, max: f32, ignore: Option<u32>) -> Option<(f32, u32, Vec3)> {
        self.first_hit(from, dir, max, ignore, &[&self.floors, &self.walls, &self.ceilings], |_| false).map(|(t, o, tri)| (t, o, facing(tri, dir)))
    }

    /// The OBJECT wall sweep (`FUN_14026ca50` -> `FUN_1401633b0` /
    /// `FUN_140163680` -> `FUN_140163e80`, two-sided like `FUN_1401d22e0`):
    /// the first face a shot / crawler / tornado / area marker's segment
    /// touches, ignoring hull classes 0x10, 0x12, 0x14 and 0x1F (rails and the
    /// stage's outer box stop fighters, not objects). Distance, owner, normal.
    pub fn raycast_object_normal(&self, from: Vec3, dir: Vec3, max: f32, ignore: Option<u32>) -> Option<(f32, u32, Vec3)> {
        self.first_hit(from, dir, max, ignore, &[&self.floors, &self.walls, &self.ceilings], object_skips).map(|(t, o, tri)| (t, o, facing(tri, dir)))
    }

    /// Like `raycast_object_normal`, but the face's own normal as it is wound (not turned to face the ray) and
    /// whether that winding is the program's stored normal (a stage triangle of a `WIND` file; object hulls keep
    /// their raw winding). The program's object wall sweep hands back the face's STORED normal (`FUN_140163e80`:
    /// contact record `+0x38`), and `FUN_14026ca50` ignores a contact the object moves along.
    pub fn raycast_object_face(&self, from: Vec3, dir: Vec3, max: f32, ignore: Option<u32>) -> Option<(f32, u32, Vec3, bool)> {
        self.first_hit(from, dir, max, ignore, &[&self.floors, &self.walls, &self.ceilings], object_skips)
            .map(|(t, o, tri)| (t, o, (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or(Vec3::Y), self.signed && o == NO_OBJECT))
    }

    /// Distance only: `raycast_object_normal`.
    pub fn raycast_object(&self, from: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.raycast_object_normal(from, dir, max, None).map(|h| h.0)
    }

    /// The line-of-fire ray (`FUN_1401641b0`, used by every hit test: bodies,
    /// blasts, areas): stopped by any face of any class EXCEPT 0x10 -- the
    /// class-0x1F faces and the rails that objects pass DO block it.
    pub fn raycast_los(&self, from: Vec3, dir: Vec3, max: f32, ignore: Option<u32>) -> Option<(f32, u32)> {
        self.first_hit(from, dir, max, ignore, &[&self.floors, &self.walls, &self.ceilings, &self.hidden], los_passes).map(|(t, o, _)| (t, o))
    }

    /// Like `raycast_ignoring`, plus the hit face's class (audits).
    pub fn raycast_class(&self, from: Vec3, dir: Vec3, max: f32) -> Option<(f32, u32, u8)> {
        self.first_hit_class(from, dir, max, None, &[&self.floors, &self.walls, &self.ceilings], |_| false).map(|(t, o, _, c)| (t, o, c))
    }

    fn first_hit<'a>(&'a self, from: Vec3, dir: Vec3, max: f32, ignore: Option<u32>, grids: &[&'a Grid], skip: impl Fn(u8) -> bool) -> Option<(f32, u32, &'a [Vec3; 3])> {
        self.first_hit_class(from, dir, max, ignore, grids, skip).map(|(t, o, tri, _)| (t, o, tri))
    }

    /// The nearest face along the ray among `grids`, skipping classes for
    /// which `skip` is true, gone objects and `ignore`.
    fn first_hit_class<'a>(&'a self, from: Vec3, dir: Vec3, max: f32, ignore: Option<u32>, grids: &[&'a Grid], skip: impl Fn(u8) -> bool) -> Option<(f32, u32, &'a [Vec3; 3], u8)> {
        let to = from + dir * max;
        let (lo, hi) = (from.min(to), from.max(to));
        let mut best: Option<(f32, u32, &[Vec3; 3], u8)> = None;
        let mut ids = Vec::new();
        for grid in grids {
            grid.in_box(lo, hi, &mut ids);
            for &i in &ids {
                let owner = grid.owner[i as usize];
                if !self.live(owner) || ignore == Some(owner) || skip(grid.class[i as usize]) {
                    continue;
                }
                let tri = &grid.tris[i as usize];
                if let Some(t) = ray_triangle(from, dir, tri) {
                    if t <= max && best.is_none_or(|b| t < b.0) {
                        best = Some((t, owner, tri, grid.class[i as usize]));
                    }
                }
            }
        }
        best
    }

    /// `ground_below` for OBJECTS (`FUN_140163cb0` crawler, `FUN_140164330`
    /// tornado / hole / dodge): the same floors, but hull classes 0x10, 0x12,
    /// 0x14 and 0x1F are not ground to them.
    pub fn ground_below_object(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for i in self.floors.at(x, z) {
            if !self.live(self.floors.owner[i as usize]) || object_skips(self.floors.class[i as usize]) {
                continue;
            }
            if let Some(y) = height_at(&self.floors.tris[i as usize], x, z) {
                if y <= max_y && best.is_none_or(|b| y > b) {
                    best = Some(y);
                }
            }
        }
        best
    }

    /// Which wall faces a body cylinder is inside, for audits: (owner, class,
    /// horizontal push needed) per triangle the cylinder overlaps, same
    /// geometry as `push_out_of_walls`.
    pub fn wall_contacts(&self, pos: Vec3, radius: f32, bottom: f32, top: f32) -> Vec<(u32, u8, f32)> {
        let mut out = Vec::new();
        let mut ids = Vec::new();
        let r = Vec3::new(radius, 0.0, radius);
        self.walls.in_box(pos - r, pos + r, &mut ids);
        for &i in &ids {
            let owner = self.walls.owner[i as usize];
            if !self.live(owner) {
                continue;
            }
            let t = &self.walls.tris[i as usize];
            let mut worst = 0.0f32;
            for h in [bottom, (bottom + top) * 0.5, top] {
                let c = Vec3::new(pos.x, pos.y + h, pos.z);
                let q = closest_on_triangle(c, t);
                let d = Vec3::new(c.x - q.x, 0.0, c.z - q.z);
                if (c.y - q.y).abs() < 1.0 && d.length() < radius {
                    worst = worst.max(radius - d.length());
                }
            }
            if worst > 0.0 {
                out.push((owner, self.walls.class[i as usize], worst));
            }
        }
        out
    }

    pub fn triangle_count(&self) -> usize {
        self.floors.tris.len()
    }
}

/// Unit normal of `t`, turned to face against the ray direction `dir`.
fn facing(t: &[Vec3; 3], dir: Vec3) -> Vec3 {
    let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or(Vec3::Y);
    if n.dot(dir) > 0.0 { -n } else { n }
}

/// Y of triangle `t` at (x, z), if the point lies inside it in XZ.
fn height_at(t: &[Vec3; 3], x: f32, z: f32) -> Option<f32> {
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

/// Closest point on triangle to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_triangle(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Möller–Trumbore, double-sided. Returns distance along `dir`.
fn ray_triangle(o: Vec3, dir: Vec3, t: &[Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let d = e2.dot(q) * inv;
    (d > 0.0).then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(y: f32, s: f32) -> [[Vec3; 3]; 2] {
        [[Vec3::new(-s, y, -s), Vec3::new(s, y, -s), Vec3::new(s, y, s)], [Vec3::new(-s, y, -s), Vec3::new(s, y, s), Vec3::new(-s, y, s)]]
    }

    #[test]
    fn object_tags_survive_the_file_and_can_be_switched_off() {
        // Stage floor at 0, an object's slab at 10 over it.
        let tris: Vec<_> = quad(0.0, 100.0).into_iter().chain(quad(10.0, 20.0)).collect();
        let owners = [NO_OBJECT, NO_OBJECT, 7, 7];
        let mut c = Collision::from_bytes(&Collision::to_bytes_tagged(&tris, &owners)).unwrap();
        assert_eq!(c.ground_below(0.0, 0.0, 50.0), Some(10.0));
        assert_eq!(c.raycast_owner(Vec3::new(0.0, 50.0, 0.0), -Vec3::Y, 100.0).map(|h| h.1), Some(7));
        c.set_object_gone(7, true);
        assert_eq!(c.ground_below(0.0, 0.0, 50.0), Some(0.0), "the slab is gone");
        assert_eq!(c.raycast_owner(Vec3::new(0.0, 50.0, 0.0), -Vec3::Y, 100.0).map(|h| h.1), Some(NO_OBJECT));
        c.set_object_gone(7, false);
        assert_eq!(c.ground_below(0.0, 0.0, 50.0), Some(10.0));
    }

    #[test]
    fn untagged_files_still_load() {
        let tris: Vec<_> = quad(0.0, 100.0).into();
        let c = Collision::from_bytes(&Collision::to_bytes(&tris)).unwrap();
        assert_eq!(c.ground_below(0.0, 0.0, 50.0), Some(0.0));
        assert_eq!(c.object_triangles().count(), 0);
    }

    /// A 30 degree ramp from (z 0, y 0) up to (z -20, y 11.547), 10 wide,
    /// wound so its normal points up (or down, `up == false`).
    fn ramp(up: bool) -> [[Vec3; 3]; 2] {
        let (a, b, c, d) = (Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0), Vec3::new(5.0, 11.547, -20.0), Vec3::new(-5.0, 11.547, -20.0));
        let t = [[a, b, c], [a, c, d]];
        // (b - a) x (c - a) has +y for this order (checked in the test below).
        if up { t } else { [[a, c, b], [a, d, c]] }
    }

    #[test]
    fn signed_files_stand_on_up_facing_stage_faces_only() {
        let owners = [NO_OBJECT; 2];
        let n = |t: &[Vec3; 3]| (t[1] - t[0]).cross(t[2] - t[0]).normalize();
        assert!(n(&ramp(true)[0]).y > 0.8 && n(&ramp(false)[0]).y < -0.8);
        let legacy_up = Collision::new_tagged(ramp(true).to_vec(), &owners);
        let legacy_down = Collision::new_tagged(ramp(false).to_vec(), &owners);
        assert!(legacy_up.ground_below(0.0, -10.0, 50.0).is_some() && legacy_down.ground_below(0.0, -10.0, 50.0).is_some(), "legacy: either winding stands");
        let up = Collision::new_signed(ramp(true).to_vec(), &owners);
        let down = Collision::new_signed(ramp(false).to_vec(), &owners);
        let y = up.ground_below(0.0, -10.0, 50.0).unwrap();
        assert!((y - 5.7735).abs() < 0.01, "ramp height {y}");
        assert_eq!(down.ground_below(0.0, -10.0, 50.0), None, "an underside is never ground");
        assert!(down.layers_at(0.0, -10.0).is_empty());
        // It is not a wall either (no sideways push), but a ray still hits it.
        let p = Vec3::new(0.0, 4.0, -10.0);
        assert_eq!(down.push_out_of_walls(p, 4.5, 7.0, 20.0), p);
        assert!(down.raycast(Vec3::new(0.0, 30.0, -10.0), -Vec3::Y, 100.0).is_some(), "rays hit undersides");
    }

    #[test]
    fn the_floor_limit_is_the_programs_0_643() {
        // Slope with normal.y = cos(angle): 49 degrees -> 0.656 (floor),
        // 51 degrees -> 0.629 (wall) -- both above the old 0.6.
        let ramp_at = |deg: f32| {
            let (run, rise) = (20.0 * deg.to_radians().cos(), 20.0 * deg.to_radians().sin());
            // Normal.y of a ramp rising `rise` over `run` is run / length.
            [[Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0), Vec3::new(5.0, rise, -run)], [Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, rise, -run), Vec3::new(-5.0, rise, -run)]]
        };
        let sign = |t: [[Vec3; 3]; 2]| {
            let n = (t[0][1] - t[0][0]).cross(t[0][2] - t[0][0]);
            if n.y < 0.0 { [[t[0][0], t[0][2], t[0][1]], [t[1][0], t[1][2], t[1][1]]] } else { t }
        };
        let gentle = Collision::new_signed(sign(ramp_at(40.0)).to_vec(), &[NO_OBJECT; 2]);
        let steep = Collision::new_signed(sign(ramp_at(51.0)).to_vec(), &[NO_OBJECT; 2]);
        assert!(gentle.ground_below(0.0, -5.0, 50.0).is_some(), "40 degrees stands");
        assert!(steep.ground_below(0.0, -5.0, 50.0).is_none(), "51 degrees (normal.y 0.629) is a wall");
        let legacy_steep = Collision::new_tagged(sign(ramp_at(51.0)).to_vec(), &[NO_OBJECT; 2]);
        assert!(legacy_steep.ground_below(0.0, -5.0, 50.0).is_some(), "legacy files keep the 0.6 limit");
    }

    #[test]
    fn objects_keep_the_either_winding_rule_in_signed_files() {
        // An object slab wound downward is still standable; the same tris as stage are not.
        let as_object = Collision::new_signed(ramp(false).to_vec(), &[3, 3]);
        assert!(as_object.ground_below(0.0, -10.0, 50.0).is_some());
        assert!(as_object.object_triangles().count() == 2);
    }

    #[test]
    fn the_wind_marker_survives_the_file_and_only_signed_files_carry_it() {
        let tris: Vec<_> = ramp(false).to_vec();
        let owners = [NO_OBJECT; 2];
        let signed = Collision::from_bytes(&Collision::to_bytes_full(&tris, &owners, true)).unwrap();
        assert_eq!(signed.ground_below(0.0, -10.0, 50.0), None, "WIND: the underside is not ground");
        let plain = Collision::from_bytes(&Collision::to_bytes_full(&tris, &owners, false)).unwrap();
        assert!(plain.ground_below(0.0, -10.0, 50.0).is_some(), "no marker: legacy rule");
        // Object tags and the marker coexist.
        let tagged = Collision::from_bytes(&Collision::to_bytes_full(&[ramp(true)[0], ramp(true)[1]], &[NO_OBJECT, 4], true)).unwrap();
        assert_eq!(tagged.object_triangles().count(), 1);
    }

    /// A vertical wall (normal along x) 20 wide and 30 tall at x = `x`, spanning z -10..10, y 0..30.
    fn wall(x: f32) -> [[Vec3; 3]; 2] {
        let (a, b, c, d) = (Vec3::new(x, 0.0, -10.0), Vec3::new(x, 0.0, 10.0), Vec3::new(x, 30.0, 10.0), Vec3::new(x, 30.0, -10.0));
        [[a, b, c], [a, c, d]]
    }

    fn classed(tris: &[[Vec3; 3]], class: u8) -> Collision {
        let owners = vec![NO_OBJECT; tris.len()];
        Collision::from_bytes(&Collision::to_bytes_classed(tris, &owners, true, &vec![class; tris.len()])).unwrap()
    }

    #[test]
    fn the_class_trailer_survives_the_file_and_old_files_still_load() {
        let tris = wall(10.0);
        let owners = [NO_OBJECT; 2];
        let bytes = Collision::to_bytes_classed(&tris, &owners, true, &[0x12, 0x12]);
        assert_eq!(&bytes[bytes.len() - 6..bytes.len() - 2], b"CLAS");
        // A file with no trailer is the same file minus 6 bytes: class 0 everywhere.
        let old = Collision::from_bytes(&bytes[..bytes.len() - 6]).unwrap();
        let from = Vec3::new(0.0, 15.0, 0.0);
        assert!(old.raycast_object(from, Vec3::X, 50.0).is_some(), "class 0 is never skipped");
        let new = Collision::from_bytes(&bytes).unwrap();
        assert!(new.raycast_object(from, Vec3::X, 50.0).is_none(), "class 0x12 is invisible to objects");
        // The marker order: OBJT, WIND, CLAS (stair_walk.rs reads the first two at fixed places).
        let t = 8 + 2 * 36;
        assert_eq!(&bytes[t..t + 4], b"OBJT");
        assert_eq!(&bytes[t + 4 + 8..t + 4 + 8 + 4], b"WIND");
    }

    #[test]
    fn objects_skip_classes_10_12_14_1f_and_fighters_do_not() {
        let from = Vec3::new(0.0, 15.0, 0.0);
        for class in [0x10u8, 0x12, 0x14] {
            let c = classed(&wall(10.0), class);
            assert!(c.raycast_object(from, Vec3::X, 50.0).is_none(), "class {class:#x}: a shot passes");
            assert!(c.raycast(from, Vec3::X, 50.0).is_some(), "class {class:#x}: a generic (body) ray stops");
            assert!(c.push_out_of_walls(Vec3::new(8.0, 0.0, 0.0), 4.5, 7.0, 20.0).x < 8.0, "class {class:#x}: a fighter is pushed");
        }
        for class in [0x01u8, 0x02, 0x11, 0x13, 0x15, 0x1E] {
            assert!(classed(&wall(10.0), class).raycast_object(from, Vec3::X, 50.0).is_some(), "class {class:#x}: a shot stops");
        }
        assert!(object_skips(0x10) && object_skips(0x12) && object_skips(0x14) && object_skips(0x1F));
        assert!(!object_skips(0x11) && !object_skips(0x13) && !object_skips(0x20) && !object_skips(0xFE) && !object_skips(0));
    }

    #[test]
    fn a_hidden_0x1f_face_blocks_only_the_line_of_fire() {
        let c = classed(&wall(10.0), CLASS_HIDDEN);
        let from = Vec3::new(0.0, 15.0, 0.0);
        assert!(c.raycast_los(from, Vec3::X, 50.0, None).is_some(), "the line-of-fire ray stops at it");
        assert!(c.raycast_object(from, Vec3::X, 50.0).is_none() && c.raycast(from, Vec3::X, 50.0).is_none(), "shots and body rays pass");
        assert_eq!(c.push_out_of_walls(Vec3::new(8.0, 0.0, 0.0), 4.5, 7.0, 20.0), Vec3::new(8.0, 0.0, 0.0), "no body is pushed by it");
        // The stage's outer box (0x10) is the one class the line-of-fire ray passes.
        let boxed = classed(&wall(10.0), CLASS_WORLD_BOX);
        assert!(boxed.raycast_los(from, Vec3::X, 50.0, None).is_none());
        // Every other class stops it, rails included.
        for class in [0x12u8, 0x14, 0x02, CLASS_NONE, CLASS_RENDER_WALL] {
            assert!(classed(&wall(10.0), class).raycast_los(from, Vec3::X, 50.0, None).is_some(), "class {class:#x} blocks the line of fire");
        }
    }

    #[test]
    fn a_crawler_does_not_stand_on_a_rail_floor_but_a_fighter_does() {
        // A square floor at y 5, wound so its normal points up (a signed file stands on up faces only).
        let q = quad(5.0, 20.0);
        let floor: Vec<[Vec3; 3]> = q.iter().map(|t| if (t[1] - t[0]).cross(t[2] - t[0]).y >= 0.0 { *t } else { [t[0], t[2], t[1]] }).collect();
        let rail = Collision::from_bytes(&Collision::to_bytes_classed(&floor, &[NO_OBJECT; 2], true, &[0x12, 0x12])).unwrap();
        assert_eq!(rail.ground_below(0.0, 0.0, 50.0), Some(5.0), "a fighter stands on it");
        assert_eq!(rail.ground_below_object(0.0, 0.0, 50.0), None, "a crawler / tornado / hole marker does not");
        let plain = Collision::from_bytes(&Collision::to_bytes_classed(&floor, &[NO_OBJECT; 2], true, &[0x02, 0x02])).unwrap();
        assert_eq!(plain.ground_below_object(0.0, 0.0, 50.0), Some(5.0));
    }

    #[test]
    fn the_object_sweep_returns_the_nearest_face_it_does_not_skip() {
        // A rail (0x12) at x = 10 in front of a solid wall (0x02) at x = 30: the shot reaches the wall.
        let tris: Vec<_> = wall(10.0).into_iter().chain(wall(30.0)).collect();
        let owners = [NO_OBJECT; 4];
        let c = Collision::from_bytes(&Collision::to_bytes_classed(&tris, &owners, true, &[0x12, 0x12, 0x02, 0x02])).unwrap();
        let from = Vec3::new(0.0, 15.0, 0.0);
        let (t, _, n) = c.raycast_object_normal(from, Vec3::X, 100.0, None).unwrap();
        assert!((t - 30.0).abs() < 1e-3, "{t}");
        assert!(n.x < -0.9, "the normal faces the shooter");
        // The generic ray stops at the rail.
        assert!((c.raycast(from, Vec3::X, 100.0).unwrap() - 10.0).abs() < 1e-3);
    }
}
