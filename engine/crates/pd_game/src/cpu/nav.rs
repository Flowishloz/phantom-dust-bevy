//! The CPU's navigation graph (`com\stNN\comdata*.cad`) and the steering code that turns the
//! script's steer target into the pad block (FORMATS-CPU.md section 5; every rule below carries
//! the program address it was read from). Deterministic, no Bevy.
//!
//! Coordinates: the file holds the game's left-handed world space (same as the stage ALR); our
//! sim and collision use the Z-mirrored space (FORMATS-STAGES.md "Z mirror"), so `Cad::parse`
//! mirrors every position it stores when asked (`mirror_z`). The plane-height and in-quad tests
//! are invariant under that mirror (checked in the tests), so the program's formulas run
//! unchanged on the mirrored data.
//!
//! Deviations from the program, all listed in FORMATS-CPU 5.8 / 7 as UNCONFIRMED or unread:
//! - local hazard avoidance (`FUN_140177f60`) is not run (which stage objects answer its queries
//!   is UNCONFIRMED), `Hazards` is a no-op hook;
//! - the load-time "node overlapped by a stage object" test (`FUN_1401e2850`) is not run (same);
//!   the "no floor under the node" half of `FUN_140176710` is (`Cad::block_floorless`);
//! - the ring splice of `FUN_1401796b0` follows the decompile for the blocked-node hop but not the
//!   per-table bookkeeping beyond it.

use glam::Vec3;

#[allow(dead_code)]
pub const FILE_SIZE: usize = 674_312;
const OFF_VCOUNT: usize = 0x0000;
const OFF_VERTS: usize = 0x0004;
const MAX_VERTS: usize = 1000;
const OFF_NCOUNT: usize = 0x2EE4;
const OFF_NODES: usize = 0x2EE8;
const NODE_SIZE: usize = 0x54;
const MAX_NODES: usize = 128;
const OFF_ECOUNT: usize = 0x58E8;
const OFF_EDGES: usize = 0x58EC;
const EDGE_SIZE: usize = 0x2C;
const MAX_EDGES: usize = 512;
const OFF_JCOUNT: usize = 0xB3C0;
const OFF_JPTS: usize = 0xB3C4;
const JP_SIZE: usize = 16;
const MAX_JPTS: usize = 30;
const ROUTE_TABLES: [usize; 5] = [0xB5A4, 0x1B5A4, 0x2B5A4, 0x3B5A4, 0x4B5A4];
const ROUTE_ROW: usize = 128;
const OFF_CLASS: usize = 0x5B5A4;
const OFF_POOL_LEN: usize = 0x5B624;
const OFF_POOL: usize = 0x5B628;

/// Node flag bits (READ, `FUN_140179e30`: `flags & 0xc` = blocked).
pub const NF_TEST: u32 = 1;
pub const NF_JUMPZONE: u32 = 2;
pub const NF_BLOCKED: u32 = 4;
#[allow(dead_code)]
pub const NF_BLOCKED_OBJ: u32 = 8;
/// Edge flag bits (`FUN_140179e30`): one-way, dynamic door, closed.
pub const EF_ONEWAY: u32 = 1;
pub const EF_DYNAMIC: u32 = 2;
pub const EF_CLOSED: u32 = 4;

/// `node_at` accepts a quad whose plane is within this of the point's height (`0x1402f7fc4`).
pub const HEIGHT_TOL: f32 = 20.0;
/// "Within the steer target" distance (`0x1402f7f18`): 5.0 world units.
pub const STEER_ARRIVE: f32 = 5.0;
/// The stuck test: squared distance under which the body "has not moved" (`0x1402f8014`).
pub const STUCK_DIST2: f32 = 50.0;
/// Height tolerance when re-using last tick's next node (`0x1402f7fb0`).
pub const NEXT_NODE_TOL: f32 = 15.0;
/// Yaw units per radian (32768 / 2 pi; `0x1402f7ce0`-adjacent constant 5215.189).
pub const YAW_UNITS_PER_RAD: f32 = 5215.189;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub flags: u32,
    /// Jump-link ids (FORMATS-CPU 5.5), -1 padded.
    pub links: [i32; 8],
    pub corners: [i32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub p0: Vec3,
    pub p1: Vec3,
    pub a: i32,
    pub b: i32,
    pub flags: u32,
    pub length: f32,
    pub gate: i32,
}

#[derive(Clone, Debug)]
pub struct Cad {
    pub verts: Vec<Vec3>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub jumps: Vec<Vec3>,
    routes: Vec<Vec<i32>>,
    classes: Vec<u8>,
    pool: Vec<u8>,
}

fn rd_i32(d: &[u8], o: usize) -> Result<i32, String> {
    d.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| format!("cad: read past end at {o:#x}"))
}
fn rd_f32(d: &[u8], o: usize) -> Result<f32, String> {
    d.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| format!("cad: read past end at {o:#x}"))
}

impl Cad {
    /// Parse the 674,312-byte memory image (FORMATS-CPU 5.2). `mirror_z` converts the file's
    /// left-handed space to ours (z -> -z) for every stored position.
    pub fn parse(data: &[u8], mirror_z: bool) -> Result<Cad, String> {
        if data.len() < OFF_POOL {
            return Err(format!("cad: {} bytes is too short", data.len()));
        }
        let nv = rd_i32(data, OFF_VCOUNT)?;
        let nn = rd_i32(data, OFF_NCOUNT)?;
        let ne = rd_i32(data, OFF_ECOUNT)?;
        let nj = rd_i32(data, OFF_JCOUNT)?;
        if !(0..=MAX_VERTS as i32).contains(&nv) || !(0..=MAX_NODES as i32).contains(&nn) || !(0..=MAX_EDGES as i32).contains(&ne) || !(0..=MAX_JPTS as i32).contains(&nj) {
            return Err(format!("cad: counts out of range {nv}/{nn}/{ne}/{nj}"));
        }
        let zs = if mirror_z { -1.0 } else { 1.0 };
        let v3 = |o: usize| -> Result<Vec3, String> { Ok(Vec3::new(rd_f32(data, o)?, rd_f32(data, o + 4)?, rd_f32(data, o + 8)? * zs)) };
        let verts = (0..nv as usize).map(|i| v3(OFF_VERTS + 12 * i)).collect::<Result<Vec<_>, _>>()?;
        let mut nodes = Vec::new();
        for i in 0..nn as usize {
            let o = OFF_NODES + NODE_SIZE * i;
            let mut links = [0i32; 8];
            for (k, l) in links.iter_mut().enumerate() {
                *l = rd_i32(data, o + 4 + 4 * k)?;
            }
            let mut corners = [0i32; 4];
            for (k, c) in corners.iter_mut().enumerate() {
                *c = rd_i32(data, o + 0x24 + 4 * k)?;
            }
            nodes.push(Node { flags: data[o] as u32, links, corners });
        }
        let mut edges = Vec::new();
        for i in 0..ne as usize {
            let o = OFF_EDGES + EDGE_SIZE * i;
            edges.push(Edge {
                p0: v3(o)?,
                p1: v3(o + 12)?,
                a: rd_i32(data, o + 0x18)?,
                b: rd_i32(data, o + 0x1c)?,
                flags: rd_i32(data, o + 0x20)? as u32,
                length: rd_f32(data, o + 0x24)?,
                gate: rd_i32(data, o + 0x28)?,
            });
        }
        let jumps = (0..nj as usize).map(|i| v3(OFF_JPTS + JP_SIZE * i)).collect::<Result<Vec<_>, _>>()?;
        let routes = ROUTE_TABLES
            .iter()
            .map(|&t| (0..ROUTE_ROW * ROUTE_ROW).map(|k| rd_i32(data, t + 4 * k)).collect::<Result<Vec<_>, _>>())
            .collect::<Result<Vec<_>, _>>()?;
        let pool_len = (rd_i32(data, OFF_POOL_LEN)?.max(0) as usize).min(data.len() - OFF_POOL);
        let cad = Cad { verts, nodes, edges, jumps, routes, classes: data[OFF_CLASS..OFF_CLASS + MAX_NODES].to_vec(), pool: data[OFF_POOL..OFF_POOL + pool_len].to_vec() };
        for (i, n) in cad.nodes.iter().enumerate() {
            if n.corners.iter().any(|&c| c < 0 || c as usize >= cad.verts.len()) {
                return Err(format!("cad: node {i} corner out of range"));
            }
        }
        Ok(cad)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn corners(&self, n: usize) -> [Vec3; 4] {
        std::array::from_fn(|k| self.verts[self.nodes[n].corners[k] as usize])
    }

    /// Mean of the four corner vertices (`FUN_14020d100`, f37).
    pub fn centroid(&self, n: usize) -> Vec3 {
        let c = self.corners(n);
        (c[0] + c[1] + c[2] + c[3]) * 0.25
    }

    pub fn blocked(&self, n: i32) -> bool {
        n < 0 || self.nodes.get(n as usize).is_none_or(|nd| nd.flags & 0xC != 0)
    }

    /// Hub node of node `n` (`g4.f54`, `0x14020e200`); `None` for 0xff.
    pub fn hub_of(&self, n: usize) -> Option<u8> {
        self.classes.get(n).copied().filter(|&c| c != 0xFF)
    }

    /// `FUN_1401771b0` + `FUN_140177460`: the plane height at `p` when `p` is inside node `n`'s
    /// quad (XZ cross products of one sign) and the node is not blocked.
    pub fn inside(&self, n: usize, p: Vec3) -> Option<f32> {
        if self.blocked(n as i32) {
            return None;
        }
        let v = self.corners(n);
        let (mut pos, mut neg) = (false, false);
        for k in 0..4 {
            let (a, b) = (v[k], v[(k + 1) & 3]);
            let (ex, ez, dx, dz) = (b.x - a.x, b.z - a.z, p.x - a.x, p.z - a.z);
            let cr = ex * dz - dx * ez;
            pos |= cr > 0.0;
            neg |= cr < 0.0;
        }
        if pos && neg {
            return None;
        }
        Some(plane_height(v[0], v[1], v[2], p))
    }

    /// `FUN_140176e90` (strict, f31) / `FUN_140176f90` (loose, f18). -1 = none.
    pub fn node_at(&self, p: Vec3, strict: bool) -> i32 {
        let (mut best, mut best_h) = (-1i32, 0.0f32);
        for n in 0..self.nodes.len() {
            if let Some(h) = self.inside(n, p) {
                if best == -1 || (h - p.y).abs() < (best_h - p.y).abs() {
                    best = n as i32;
                    best_h = h;
                }
            }
        }
        if best != -1 && (best_h - p.y).abs() < HEIGHT_TOL {
            return best;
        }
        if strict {
            return -1;
        }
        // Loose: the nearest unblocked node by the distance to its four boundary segments (`FUN_140193bc0`).
        let (mut near, mut near_d) = (-1i32, 0.0f32);
        for n in 0..self.nodes.len() {
            if self.blocked(n as i32) {
                continue;
            }
            let cs = self.corners(n);
            for k in 0..4 {
                let d = point_segment_distance(p, cs[k], cs[(k + 1) & 3]);
                if near == -1 || d < near_d {
                    near = n as i32;
                    near_d = d;
                }
            }
        }
        near
    }

    /// Stored route `a -> b` in table `t` (0..4): the node list after `a` up to and including `b`.
    pub fn route(&self, t: usize, a: i32, b: i32) -> Option<Vec<u8>> {
        if a < 0 || b < 0 || a as usize >= ROUTE_ROW || b as usize >= ROUTE_ROW {
            return None;
        }
        let off = self.routes[t][a as usize * ROUTE_ROW + b as usize];
        if off < 0 {
            return None;
        }
        let mut out = Vec::new();
        let mut o = off as usize;
        while out.len() <= self.nodes.len() && o < self.pool.len() {
            let x = self.pool[o];
            if x == 0xFF {
                return None;
            }
            out.push(x);
            if x as i32 == b {
                return Some(out);
            }
            o += 1;
        }
        None
    }

    pub fn has_route(&self, t: usize, a: i32, b: i32) -> bool {
        a >= 0 && b >= 0 && (a as usize) < ROUTE_ROW && (b as usize) < ROUTE_ROW && self.routes[t][a as usize * ROUTE_ROW + b as usize] >= 0
    }

    /// The edge joining `a -> b` (`b -> a` only when the edge is not one-way), else `None`.
    pub fn edge_between(&self, a: i32, b: i32) -> Option<usize> {
        self.edges.iter().position(|e| (e.a == a && e.b == b) || (e.b == a && e.a == b && e.flags & EF_ONEWAY == 0))
    }

    /// `FUN_140179e30`: can the route use the step `a -> b`? Blocked nodes no; the same node yes; an
    /// open door / gated edge yes; otherwise only if the T4 table has a route.
    pub fn passable(&self, a: i32, b: i32) -> bool {
        if a < 0 || b < 0 || self.blocked(a) || self.blocked(b) {
            return false;
        }
        if a == b {
            return true;
        }
        if let Some(i) = self.edge_between(a, b) {
            let e = &self.edges[i];
            let door = e.flags & EF_DYNAMIC != 0 || e.gate != 0;
            if door && ((e.gate != 0 && self.nodes[e.gate as usize].flags & NF_BLOCKED == 0) || e.flags & 6 == EF_DYNAMIC) {
                return true;
            }
        }
        self.has_route(4, a, b)
    }

    /// `FUN_1401790e0`: the sum of the stored edge lengths along the stored route `a -> b` (T2 when
    /// the planner style is 2 and it has an entry, else T0); -1.0 when there is none or a step has
    /// no edge.
    pub fn route_len(&self, a: i32, b: i32, style: i32) -> f32 {
        if a == b {
            return -1.0;
        }
        let r = if style == 2 { self.route(2, a, b).or_else(|| self.route(0, a, b)) } else { self.route(0, a, b) };
        let Some(r) = r else { return -1.0 };
        let (mut cur, mut sum) = (a, 0.0);
        for &n in &r {
            let Some(i) = self.edge_between(cur, n as i32) else { return -1.0 };
            sum += self.edges[i].length;
            cur = n as i32;
        }
        sum
    }

    /// `FUN_140179f80` (f24): a straight walkable line between two points along plain portals.
    pub fn ground_ok(&self, a: Vec3, b: Vec3) -> bool {
        let (na, nb) = (self.node_at(a, true), self.node_at(b, true));
        if na == -1 || nb == -1 || self.blocked(na) || self.blocked(nb) {
            return false;
        }
        if na == nb {
            return true;
        }
        let (mut cur, mut last) = (na, [-1i32; 3]);
        for _ in 0..16 {
            let mut found = None;
            for (i, e) in self.edges.iter().enumerate() {
                if last.contains(&(i as i32)) {
                    continue;
                }
                let (fl, gate) = (e.flags, e.gate);
                if (fl & 2 != 0 || gate != 0) && ((gate != 0 && self.nodes[gate as usize].flags & 4 == 0) || fl & 6 == 2) {
                    continue;
                }
                if fl != 0 || (e.a != cur && e.b != cur) {
                    continue;
                }
                if !segments_cross_xz(e.p0, e.p1, a, b) {
                    continue;
                }
                found = Some(i);
                break;
            }
            let Some(i) = found else { return false };
            let e = &self.edges[i];
            let nxt = if e.a == cur { e.b } else { e.a };
            last = [last[1], last[2], i as i32];
            if nxt < 0 || self.nodes[nxt as usize].flags & 0xE != 0 {
                return false;
            }
            cur = nxt;
            if cur == nb {
                return true;
            }
        }
        true
    }

    /// `FUN_140176710`, the floor half: a node with `NF_TEST` needs floor under its centroid (a ray from 10 above
    /// to 20 below, `0x1402f7f7c` / `0x1402f7fc4`), else it is blocked (flag 4). `floor_below(p, down)` is the
    /// stage ray. UNCONFIRMED: the stage-object overlap half (flag 8), not run.
    pub fn block_floorless(&mut self, floor_hit: &dyn Fn(Vec3, Vec3) -> bool) {
        for n in 0..self.nodes.len() {
            if self.nodes[n].flags & NF_TEST == 0 {
                continue;
            }
            let c = self.centroid(n);
            if !floor_hit(c + Vec3::Y * 10.0, c - Vec3::Y * 20.0) {
                self.nodes[n].flags |= NF_BLOCKED;
            }
        }
    }

    /// Dynamic doors (`FUN_140176980`, every tick): a line-of-fire ray across the portal sets / clears the closed bit.
    pub fn update_doors(&mut self, clear: &dyn Fn(Vec3, Vec3) -> bool) {
        for e in &mut self.edges {
            if e.flags & EF_DYNAMIC == 0 {
                continue;
            }
            let mid = (e.p0 + e.p1) * 0.5 + Vec3::Y * 7.5;
            let d = e.p1 - e.p0;
            let n = Vec3::new(d.z, 0.0, -d.x).normalize_or_zero();
            if clear(mid - n * 5.0, mid + n * 5.0) {
                e.flags &= !EF_CLOSED;
            } else {
                e.flags |= EF_CLOSED;
            }
        }
    }

    /// Consistency check (the python `Cad.check`): problems found, empty = fine.
    #[allow(dead_code)] // used by the tests
    pub fn check(&self) -> Vec<String> {
        let mut bad = Vec::new();
        for (i, e) in self.edges.iter().enumerate() {
            for (k, n) in [("a", e.a), ("b", e.b)] {
                if n < 0 || n as usize >= self.nodes.len() {
                    bad.push(format!("edge {i} node {k}={n} out of range"));
                }
            }
            if e.gate != 0 && (e.gate < 0 || e.gate as usize >= self.nodes.len()) {
                bad.push(format!("edge {i} gate {} out of range", e.gate));
            }
        }
        for t in 0..5 {
            for a in 0..self.nodes.len() as i32 {
                for b in 0..self.nodes.len() as i32 {
                    if let Some(r) = self.route(t, a, b) {
                        if r.iter().any(|&x| x as usize >= self.nodes.len()) {
                            bad.push(format!("T{t} {a}->{b} leaves the graph"));
                        }
                    }
                }
            }
        }
        bad
    }
}

/// `FUN_140177460`: the y of the plane through `v0 v1 v2` at the point's (x, z).
pub fn plane_height(v0: Vec3, v1: Vec3, v2: Vec3, p: Vec3) -> f32 {
    let (a, b) = (v1 - v0, v2 - v0);
    let nx = a.y * b.z - a.z * b.y;
    let ny = a.z * b.x - a.x * b.z;
    let nz = a.x * b.y - a.y * b.x;
    if ny == 0.0 {
        return f32::INFINITY;
    }
    v0.y - ((p.z - v0.z) * nz + (p.x - v0.x) * nx) / ny
}

/// `FUN_140196420`: do the XZ segments `p0 p1` and `a b` intersect (inclusive)?
pub fn segments_cross_xz(p0: Vec3, p1: Vec3, a: Vec3, b: Vec3) -> bool {
    let cr = |o: Vec3, u: Vec3, v: Vec3| (u.x - o.x) * (v.z - o.z) - (u.z - o.z) * (v.x - o.x);
    cr(p0, p1, a) * cr(p0, p1, b) <= 0.0 && cr(a, b, p0) * cr(a, b, p1) <= 0.0
}

/// `FUN_140193bc0`: 3-D distance from `p` to the segment `a b`.
pub fn point_segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    p.distance(closest_on_segment(p, a, b))
}

/// `FUN_140193fd0`: the closest point to `p` on the segment `a b`.
pub fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 == 0.0 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    a + ab * t
}

/// The stage ray the steering code needs (`FUN_1401641b0`): is the straight line `a -> b` clear?
pub trait Rays {
    fn clear(&self, a: Vec3, b: Vec3) -> bool;
}

/// What the steering code reads of the body (the program's `P+0x12e74`, `P+0x12f04`, `P+0x12e84`).
#[derive(Clone, Copy, Debug, Default)]
pub struct BodyCtx {
    pub pos: Vec3,
    /// Vertical velocity in units per PROGRAM tick (`P+0x12e84`).
    pub vy_tick: f32,
    /// `P+0x12e74 == 0`.
    pub airborne: bool,
    /// State < 2: standing (0) or running (1) (`FUN_1401776c0` step 2).
    pub standing_or_running: bool,
    /// State in {0, 1, 8}: the only states the dodge button is taken in (`FUN_140178ad0`).
    pub dodge_state_ok: bool,
    /// State 4 (jump wind-up) -> steer lock.
    pub state4: bool,
    /// State 0, 1 or 6 -> steer lock released.
    pub unlock_state: bool,
    /// State 5 (airborne in the program's numbering): the jump-zone rule.
    pub state5: bool,
}

/// What the pad block gets (`P+0x14268` magnitude, `P+0x14270` angle in 1/32768 turns, the dodge button).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadOut {
    pub mag: f32,
    /// Absolute yaw in the program's units (0 = +z); only meaningful when `mag > 0`.
    pub yaw_units: i32,
    /// The dodge button was pressed this tick, facing / stick angle = this (yaw units).
    pub dodge: Option<i32>,
}

/// The nav controller `COM` (FORMATS-CPU 5.7 table).
#[derive(Clone, Debug)]
pub struct Com {
    pub steer_lock: bool,
    pub steer_node: i32,
    pub cand_node: i32,
    pub cur_node: i32,
    pub next_node: i32,
    pub edge: i32,
    pub goal_node: i32,
    pub mag: f32,
    pub los: bool,
    pub dodge_req: bool,
    pub dodge_yaw: i32,
    pub planner_style: i32,
    pub steer: Vec3,
    pub goal: Vec3,
    /// The route ring: `[my node, ..., goal]` (`COM+0x68/0x6c`), empty = no route.
    pub ring: Vec<u8>,
    pub last_graph_pos: Vec3,
    pub off_ticks: i32,
    pub scan_idx: i32,
    pub recov_active: bool,
    pub recov_target: Vec3,
    pub hopeless: bool,
    pub stuck_ref: Vec3,
    pub stuck: i32,
    pub last3: [i32; 3],
    pub since_node_change: i32,
    /// Debug code of the last steering branch (`COM+0x134`, 0..6; 7 = jump link, 8 = recovery).
    pub code: i32,
}

impl Default for Com {
    fn default() -> Self {
        Com {
            steer_lock: false,
            steer_node: -1,
            cand_node: -1,
            cur_node: -1,
            next_node: -1,
            edge: -1,
            goal_node: -1,
            mag: 1.0,
            los: false,
            dodge_req: false,
            dodge_yaw: 0,
            planner_style: 0,
            steer: Vec3::ZERO,
            goal: Vec3::ZERO,
            ring: Vec::new(),
            last_graph_pos: Vec3::ZERO,
            off_ticks: 0,
            scan_idx: 0,
            recov_active: false,
            recov_target: Vec3::ZERO,
            hopeless: false,
            stuck_ref: Vec3::ZERO,
            stuck: 0,
            last3: [-1; 3],
            since_node_change: 0,
            code: 0,
        }
    }
}

impl Com {
    /// A new controller standing at `pos` (the program zeroes `COM` at creation; the steer target is the body itself so the
    /// first tick stands still).
    pub fn at(pos: Vec3) -> Com {
        Com { steer: pos, goal: pos, stuck_ref: pos, last_graph_pos: pos, recov_target: pos, ..Com::default() }
    }

    /// `g4.f19` MOVE_TO: only when the steer lock is off (`0x14020d660`).
    pub fn move_to(&mut self, p: Vec3) {
        if !self.steer_lock {
            self.steer = p;
            self.goal = p;
        }
    }

    /// `g4.f20` nav_step (`0x14020d6b0`, `FUN_14020d100`): walk to node `n`'s centroid unless it is blocked.
    pub fn nav_step(&mut self, cad: &Cad, n: i32) -> bool {
        if n < 0 || n as usize >= cad.node_count() || cad.nodes[n as usize].flags & 0xC != 0 {
            return false;
        }
        self.move_to(cad.centroid(n as usize));
        true
    }

    /// `g4.f49` path49: the dodge request (`0x14020df20`).
    pub fn request_dodge(&mut self, deg: i32) {
        self.dodge_req = true;
        self.dodge_yaw = (deg as f32 * 91.02222) as i32;
    }

    /// `g4.f51` path51(avoid, goal) (`FUN_14017a1a0`): route to the goal while treating the node of the avoided point as blocked.
    /// Returns 1 when a route was written. READ at decompile level (FORMATS-CPU 5.4); the "avoided point is in my own node" branch
    /// follows the doc text.
    pub fn path51(&mut self, cad: &Cad, me: Vec3, avoid: Vec3, goal: Vec3) -> bool {
        let ni = cad.node_at(avoid, false);
        let nme = cad.node_at(me, false);
        self.cur_node = if nme != -1 { nme } else { self.cur_node };
        if ni == -1 || nme == -1 {
            return false;
        }
        let mut ng = cad.node_at(goal, false);
        let mut goal_pt = goal;
        if ng == -1 {
            match cad.hub_of(ni as usize) {
                Some(h) => {
                    ng = h as i32;
                    goal_pt = cad.centroid(h as usize);
                }
                None => return false,
            }
        }
        if ni == nme {
            // Among the nodes joined to `ni` by a usable edge, the one whose portal lies most opposite the avoided point as seen from me.
            let mut best: Option<(f32, i32, Vec3)> = None;
            for e in &cad.edges {
                let other = if e.a == ni && true { e.b } else if e.b == ni && e.flags & EF_ONEWAY == 0 { e.a } else { continue };
                let mid = (e.p0 + e.p1) * 0.5;
                let (u, v) = ((avoid - me).normalize_or_zero(), (mid - me).normalize_or_zero());
                let d = u.dot(v);
                if best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, other, cad.centroid(other.max(0) as usize)));
                }
            }
            let Some((_, node, pt)) = best else { return false };
            if cad.blocked(node) {
                return false;
            }
            self.ring = vec![nme as u8, node as u8];
            if !self.steer_lock {
                self.steer = pt;
                self.goal = pt;
            }
            return true;
        }
        // Route nme -> ng with `ni` blocked: T0, T1, T3, T4 (when the planner style is non-zero).
        let tables: &[usize] = if self.planner_style != 0 { &[0, 1, 3, 4] } else { &[0, 1, 3] };
        for &t in tables {
            if let Some(r) = cad.route(t, nme, ng) {
                if r.iter().all(|&n| n as i32 != ni) {
                    let mut ring = vec![nme as u8];
                    ring.extend(r);
                    self.ring = ring;
                    if !self.steer_lock {
                        self.steer = goal_pt;
                        self.goal = goal_pt;
                    }
                    return true;
                }
            }
        }
        self.ring.clear();
        false
    }

    /// `g4.f60` path_step (`0x14020e380`): 0 no route, 1 following, 2 arrived (my node is the goal's and < 7.0 from the goal point).
    pub fn path_step(&mut self, cad: &Cad, pos: Vec3) -> i32 {
        let g = cad.node_at(self.goal, false);
        if g == -1 || cad.blocked(g) {
            return 0;
        }
        if self.cur_node == g {
            self.steer = self.goal;
            let d = Vec3::new(self.goal.x - pos.x, 0.0, self.goal.z - pos.z).length();
            return if d < 7.0 { 2 } else { 1 };
        }
        if !self.los {
            let Some(&last) = self.ring.last() else { return 0 };
            let _ = last;
            if self.ring.last().map(|&n| n as i32) != Some(self.cur_node) {
                if !self.steer_lock {
                    self.steer = self.goal;
                }
                return 1;
            }
            // (the program's further branch is unreachable, FORMATS-CPU 5.4)
            return 1;
        }
        if !self.steer_lock {
            self.steer = self.goal;
        }
        1
    }

    // ---- the per-tick steering (FUN_1401775b0 tail) ----------------------------------------------------------

    /// What the program does after the script returned (`FUN_1401775b0`): `COM+0x3c`, the steer-target selection while the lock is
    /// off, the lock update, the pad output.
    pub fn after_script(&mut self, cad: &Cad, body: &BodyCtx, rays: &dyn Rays) -> PadOut {
        self.los = cad.ground_ok(body.pos, self.steer);
        if !self.steer_lock {
            self.select_steer(cad, body, rays);
        }
        // Hazards (`FUN_140177f60`) are not run here (UNCONFIRMED object set); see the module docs.
        if body.airborne || body.state4 {
            self.steer_lock = true;
        } else if body.unlock_state {
            self.steer_lock = false;
        }
        self.pad_out(body)
    }

    /// `FUN_1401776c0` (FORMATS-CPU 5.7 steps 1-5).
    fn select_steer(&mut self, cad: &Cad, body: &BodyCtx, rays: &dyn Rays) {
        let pos = body.pos;
        let n_me = cad.node_at(pos, true);
        if n_me != -1 {
            self.last_graph_pos = pos;
            self.off_ticks = 0;
            self.recov_active = false;
            self.hopeless = false;
            self.recov_target = pos;
            self.cand_node = n_me;
        }
        if body.standing_or_running {
            if (pos - self.stuck_ref).length_squared() <= STUCK_DIST2 {
                self.stuck += 1;
            } else {
                self.stuck_ref = pos;
                self.stuck = 0;
            }
            self.since_node_change += 1;
            if n_me != -1 && n_me != self.last3[2] {
                self.last3 = [self.last3[1], self.last3[2], n_me];
                self.since_node_change = 0;
            }
        }
        if self.los {
            if n_me != -1 {
                self.cur_node = n_me;
            }
            return;
        }
        if pos.distance(self.steer) < STEER_ARRIVE {
            return;
        }
        if n_me == -1 && body.standing_or_running {
            self.off_ticks += 1;
            if self.off_ticks >= 30 {
                self.recover(cad, body, rays);
                return;
            }
        }
        self.steer_node = cad.node_at(self.steer, false);
        if self.steer_node != -1 {
            self.goal_node = self.steer_node;
        }
        // My node: last tick's next node while still inside and within 15 of my height, else the cached node, else the loose lookup.
        let mut me_node = -1;
        if self.next_node >= 0 {
            if let Some(h) = cad.inside(self.next_node as usize, pos) {
                if (h - pos.y).abs() < NEXT_NODE_TOL {
                    me_node = self.next_node;
                }
            }
        }
        if me_node == -1 {
            me_node = if self.cand_node != -1 { self.cand_node } else { cad.node_at(pos, false) };
        }
        if me_node != -1 {
            self.cur_node = me_node;
        }
        let cur = self.cur_node;
        if cur >= 0 && cad.nodes[cur as usize].flags & NF_JUMPZONE != 0 && !body.state5 {
            self.steer = cad.centroid(cur as usize);
            self.code = 2;
            return;
        }
        if self.goal_node != -1 && (self.ring.is_empty() || !self.ring_ok(cad, cur, self.goal_node)) {
            self.plan(cad, 0);
        }
        if cur == self.goal_node {
            self.steer = self.goal;
            self.next_node = -1;
            self.code = 3;
            return;
        }
        if self.ring.is_empty() || self.ring.first().is_none() {
            self.steer = pos;
            self.next_node = -1;
            self.code = 1;
            return;
        }
        let next = self.ring_next(cur);
        self.next_node = next;
        if next < 0 {
            self.steer = pos;
            self.code = 1;
            return;
        }
        // Jump links (FORMATS-CPU 5.5): the next node has the landing entry 0x1f + k, this node the departure entry k.
        if cur >= 0 {
            for &l in &cad.nodes[cur as usize].links {
                if (0..=30).contains(&l) && (l as usize) < cad.jumps.len() && cad.nodes[next as usize].links.contains(&(0x1f + l)) {
                    self.steer_lock = true;
                    self.steer = cad.jumps[l as usize];
                    self.code = 7;
                    return;
                }
            }
        }
        let edge = cad.edge_between(cur, next);
        self.edge = edge.map_or(-1, |e| e as i32);
        let aim = if next == self.goal_node && edge.is_some() { self.goal } else { cad.centroid(next as usize) };
        match edge {
            Some(i) => {
                let e = &cad.edges[i];
                let xz = |v: Vec3| (v.x, v.z);
                let (ax, az) = (pos.x - aim.x, pos.z - aim.z);
                let (bx, bz) = (e.p0.x - aim.x, e.p0.z - aim.z);
                let (cx, cz) = (e.p1.x - aim.x, e.p1.z - aim.z);
                let _ = xz;
                let cross = |u: (f32, f32), v: (f32, f32)| u.0 * v.1 - u.1 * v.0;
                let (a, b, c) = ((ax, az), (bx, bz), (cx, cz));
                let through = cross(c, b) * cross(a, b) >= 0.0 && cross(a, c) * cross(a, b) <= 0.0;
                if through {
                    self.steer = aim;
                    self.code = 4;
                } else {
                    let mid = (e.p0 + e.p1) * 0.5;
                    let flat = Vec3::new(mid.x - pos.x, 0.0, mid.z - pos.z).normalize_or_zero();
                    self.steer = mid + flat * STEER_ARRIVE;
                    self.code = 5;
                }
            }
            None => {
                self.steer = aim;
                self.code = 6;
            }
        }
    }

    /// `FUN_1401792b0(COM, cur, goal)`: the ring still starts at (or passes) `cur` and reaches `goal` over passable steps.
    /// On success the ring is trimmed so it starts at `cur`.
    fn ring_ok(&mut self, cad: &Cad, cur: i32, goal: i32) -> bool {
        let Some(start) = self.ring.iter().position(|&n| n as i32 == cur) else { return false };
        let tail = &self.ring[start..];
        for w in tail.windows(2) {
            if !cad.passable(w[0] as i32, w[1] as i32) && cad.edge_between(w[0] as i32, w[1] as i32).is_some() {
                return false;
            }
        }
        if tail.last().map(|&n| n as i32) != Some(goal) {
            return false;
        }
        self.ring = tail.to_vec();
        true
    }

    /// `FUN_1401791f0(COM, n)`: the node after `n` in the ring, the first node when `n` is not in it.
    fn ring_next(&self, n: i32) -> i32 {
        if self.ring.is_empty() {
            return -1;
        }
        match self.ring.iter().position(|&x| x as i32 == n) {
            Some(i) => self.ring.get(i + 1).map_or(-1, |&x| x as i32),
            None => self.ring[0] as i32,
        }
    }

    /// `FUN_140178d00(COM, mode)`: fill the ring from my node to the goal node. Mode 0 follows the stored route (T2 for style 2, else T0)
    /// and takes a door-edge shortcut when one leads strictly closer; mode 1 copies the T4 (jump) route; mode 0 falling back to 1.
    pub fn plan(&mut self, cad: &Cad, mode: i32) {
        let (from, goal) = (self.cur_node, self.goal_node);
        self.ring.clear();
        if from < 0 || goal < 0 {
            return;
        }
        if mode == 0 {
            let first = if self.planner_style == 2 { cad.route(2, from, goal).or_else(|| cad.route(0, from, goal)) } else { cad.route(0, from, goal) };
            if first.is_none() {
                return self.plan(cad, 1);
            }
            let mut ring = vec![from as u8];
            let mut cur = from;
            let mut route = first.unwrap();
            let mut guard = 0;
            loop {
                guard += 1;
                if guard > cad.node_count() + 2 {
                    break;
                }
                // The door-edge shortcut (`FUN_140178d00` inner loop): the first open door / gated edge from `cur` to a node strictly
                // closer to the goal by stored route length continues from that node.
                let mut hopped = false;
                for e in &cad.edges {
                    let door = e.flags & EF_DYNAMIC != 0 || e.gate != 0;
                    let open = (e.gate == 0 || cad.nodes[e.gate as usize].flags & NF_BLOCKED != 0) && e.flags & 6 != EF_DYNAMIC;
                    if !(door && open) {
                        continue;
                    }
                    if !(matches!(self.planner_style, 2 | 3) || e.flags & EF_ONEWAY == 0) {
                        continue;
                    }
                    let w = if e.a == cur {
                        e.b
                    } else if e.b == cur && e.flags & EF_ONEWAY == 0 {
                        e.a
                    } else {
                        continue;
                    };
                    if w == goal {
                        ring.push(w as u8);
                        self.ring = ring;
                        return self.finish_ring(cad);
                    }
                    let (lw, lc) = (cad.route_len(w, goal, self.planner_style), cad.route_len(cur, goal, self.planner_style));
                    if lw > 0.0 && lw < lc {
                        ring.push(w as u8);
                        cur = w;
                        route = if self.planner_style == 2 { cad.route(2, w, goal).or_else(|| cad.route(0, w, goal)) } else { cad.route(0, w, goal) }.unwrap_or_default();
                        hopped = true;
                        break;
                    }
                }
                if hopped {
                    if route.is_empty() {
                        break;
                    }
                    continue;
                }
                ring.extend(route.iter().copied());
                break;
            }
            self.ring = ring;
            self.finish_ring(cad);
            if self.ring.is_empty() {
                self.plan(cad, 1);
            }
        } else {
            match cad.route(4, from, goal) {
                Some(r) => {
                    let mut ring = vec![from as u8];
                    ring.extend(r);
                    self.ring = ring;
                }
                None => self.ring.clear(),
            }
        }
    }

    /// `FUN_1401796b0`: when the ring passes a blocked node, bridge from the last good node to the next good one with a stored route
    /// (T0, T1, T3, then T4 when `COM+0x48 != 0`). A ring that cannot be mended is dropped.
    fn finish_ring(&mut self, cad: &Cad) {
        if !self.ring.iter().any(|&n| cad.blocked(n as i32)) {
            return;
        }
        let ring = std::mem::take(&mut self.ring);
        let mut out: Vec<u8> = Vec::new();
        let mut i = 0;
        while i < ring.len() {
            let n = ring[i];
            if !cad.blocked(n as i32) {
                out.push(n);
                i += 1;
                continue;
            }
            let Some(&last_good) = out.last() else { return };
            let Some(j) = (i..ring.len()).find(|&j| !cad.blocked(ring[j] as i32)) else { return };
            let next_good = ring[j];
            let mut bridged = None;
            for t in [0usize, 1, 3, 4] {
                if t == 4 && self.planner_style == 0 {
                    continue;
                }
                if let Some(r) = cad.route(t, last_good as i32, next_good as i32) {
                    if r.iter().all(|&x| !cad.blocked(x as i32)) {
                        bridged = Some(r);
                        break;
                    }
                }
            }
            let Some(r) = bridged else { return };
            out.extend(r);
            i = j + 1;
        }
        self.ring = out;
    }

    /// Off-graph recovery (`FUN_140178470`, FORMATS-CPU 5.8).
    fn recover(&mut self, cad: &Cad, body: &BodyCtx, rays: &dyn Rays) {
        self.code = 8;
        let pos = body.pos;
        if self.hopeless {
            self.steer = pos;
            return;
        }
        let up = Vec3::Y * 7.5;
        let unit_xz = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or_zero();
        if self.off_ticks < 90 {
            if self.recov_active {
                self.steer = self.recov_target;
                return;
            }
            let (a, b) = (self.last_graph_pos + up, pos + up);
            if rays.clear(a, b) {
                self.recov_target = self.last_graph_pos + unit_xz(self.last_graph_pos - pos) * 6.0;
                self.recov_active = true;
                self.off_ticks = 30;
                self.steer = self.recov_target;
                return;
            }
            let ln = cad.node_at(pos, false);
            if ln >= 0 {
                let c = cad.centroid(ln as usize) + up;
                if rays.clear(c, pos + up) {
                    self.recov_target = cad.centroid(ln as usize) + unit_xz(cad.centroid(ln as usize) - pos) * 6.0;
                    self.recov_active = true;
                    self.off_ticks = 30;
                    self.steer = self.recov_target;
                    return;
                }
            }
        }
        // (d) scan one node per tick for the closest boundary point within 100 u with a clear ray.
        let idx = self.scan_idx.max(0) as usize;
        if idx < cad.node_count() {
            if !cad.blocked(idx as i32) {
                let cs = cad.corners(idx);
                let mut best: Option<(f32, Vec3)> = None;
                for k in 0..4 {
                    let q = closest_on_segment(pos, cs[k], cs[(k + 1) & 3]);
                    let d = q.distance(pos);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, q));
                    }
                }
                if let Some((d, q)) = best {
                    if d <= 100.0 && rays.clear(pos + up, q + up) {
                        self.recov_target = q + unit_xz(q - pos) * 6.0;
                        self.recov_active = true;
                        self.off_ticks = 30;
                        self.steer = self.recov_target;
                        self.scan_idx += 1;
                        return;
                    }
                }
            }
            self.scan_idx += 1;
        } else {
            self.hopeless = true;
        }
    }

    /// `FUN_140178ad0`: steer target -> stick, or the dodge button.
    fn pad_out(&mut self, body: &BodyCtx) -> PadOut {
        let (p, s) = (body.pos, self.steer);
        let mut out = PadOut::default();
        let flat = ((s.x - p.x).powi(2) + (s.z - p.z).powi(2)).sqrt();
        let mut push = true;
        if body.airborne {
            // Ballistic estimate (g = 0.12 per tick^2, run speed 1.85185 u/tick): keep pushing only if the flight ends after I could
            // cover the horizontal distance... (FORMATS-CPU 5.9).
            let g = 0.12f32;
            let t_up = body.vy_tick / g;
            let apex = body.vy_tick * t_up * 0.5 + p.y;
            if s.y < apex {
                let t_air = t_up + (2.0 * (apex - s.y) / g).sqrt();
                let t_run = flat / 1.85185;
                push = t_air <= t_run;
            } else {
                push = false;
            }
            if !push {
                return out;
            }
        }
        let _ = push;
        if !self.dodge_req {
            if flat > STEER_ARRIVE {
                out.mag = self.mag;
                out.yaw_units = ((s.x - p.x).atan2(s.z - p.z) * YAW_UNITS_PER_RAD) as i32;
            }
        } else if body.dodge_state_ok {
            out.mag = 1.0;
            out.yaw_units = self.dodge_yaw;
            out.dodge = Some(self.dodge_yaw);
        }
        out
    }
}

#[cfg(test)]
pub mod testkit {
    use super::*;

    pub struct AllClear;
    impl Rays for AllClear {
        fn clear(&self, _: Vec3, _: Vec3) -> bool {
            true
        }
    }

    /// A synthetic `.cad` image (the python `build_cad`): nodes = `(flags, corner indices)`, edges = `(p0, p1, a, b, flags, gate)`,
    /// routes = `(table, a, b, node list)`.
    #[allow(clippy::type_complexity)]
    pub fn build_cad(verts: &[[f32; 3]], nodes: &[(u8, [i32; 4])], edges: &[([f32; 3], [f32; 3], i32, i32, u32, i32)], routes: &[(usize, usize, usize, Vec<u8>)]) -> Vec<u8> {
        let mut d = vec![0u8; FILE_SIZE];
        let put_i = |d: &mut Vec<u8>, o: usize, v: i32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        let put_f = |d: &mut Vec<u8>, o: usize, v: f32| d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put_i(&mut d, OFF_VCOUNT, verts.len() as i32);
        for (i, v) in verts.iter().enumerate() {
            for k in 0..3 {
                put_f(&mut d, OFF_VERTS + 12 * i + 4 * k, v[k]);
            }
        }
        put_i(&mut d, OFF_NCOUNT, nodes.len() as i32);
        for (i, (fl, cs)) in nodes.iter().enumerate() {
            let o = OFF_NODES + NODE_SIZE * i;
            d[o] = *fl;
            for k in 0..8 {
                put_i(&mut d, o + 4 + 4 * k, -1);
            }
            for k in 0..4 {
                put_i(&mut d, o + 0x24 + 4 * k, cs[k]);
            }
        }
        put_i(&mut d, OFF_ECOUNT, edges.len() as i32);
        for (i, (p0, p1, a, b, fl, gate)) in edges.iter().enumerate() {
            let o = OFF_EDGES + EDGE_SIZE * i;
            for k in 0..3 {
                put_f(&mut d, o + 4 * k, p0[k]);
                put_f(&mut d, o + 12 + 4 * k, p1[k]);
            }
            put_i(&mut d, o + 0x18, *a);
            put_i(&mut d, o + 0x1c, *b);
            put_i(&mut d, o + 0x20, *fl as i32);
            let len = Vec3::from(*p0).distance(Vec3::from(*p1));
            put_f(&mut d, o + 0x24, len);
            put_i(&mut d, o + 0x28, *gate);
        }
        for &t in &ROUTE_TABLES {
            for k in 0..ROUTE_ROW * ROUTE_ROW {
                put_i(&mut d, t + 4 * k, -1);
            }
        }
        let mut pool = Vec::new();
        for (t, a, b, r) in routes {
            put_i(&mut d, ROUTE_TABLES[*t] + (a * ROUTE_ROW + b) * 4, pool.len() as i32);
            pool.extend_from_slice(r);
            pool.push(0xFF);
        }
        d[OFF_POOL..OFF_POOL + pool.len()].copy_from_slice(&pool);
        put_i(&mut d, OFF_POOL_LEN, pool.len() as i32);
        for n in 0..MAX_NODES {
            d[OFF_CLASS + n] = 0xFF;
        }
        d
    }

    /// Three square nodes in a row along +x (each 100 wide, 100 deep, floor y 0), joined by two plain edges; routes 0->2 = [1, 2].
    pub fn corridor(mirror_z: bool) -> Cad {
        let mut verts = Vec::new();
        for i in 0..4 {
            verts.push([100.0 * i as f32, 0.0, 0.0]);
            verts.push([100.0 * i as f32, 0.0, 100.0]);
        }
        // node i uses verts (2i, 2i+1, 2i+3, 2i+2)
        let nodes: Vec<(u8, [i32; 4])> = (0..3).map(|i| (0u8, [2 * i, 2 * i + 1, 2 * i + 3, 2 * i + 2])).collect();
        let edges = [([100.0, 0.0, 0.0], [100.0, 0.0, 100.0], 0, 1, 0, 0), ([200.0, 0.0, 0.0], [200.0, 0.0, 100.0], 1, 2, 0, 0)];
        let routes = [(0, 0, 2, vec![1, 2]), (0, 2, 0, vec![1, 0]), (0, 0, 1, vec![1]), (0, 1, 2, vec![2]), (0, 2, 1, vec![1]), (0, 1, 0, vec![0])];
        Cad::parse(&build_cad(&verts, &nodes, &edges, &routes), mirror_z).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn plane_height_and_node_at_match_the_program_rules() {
        // FUN_140177460 / FUN_1401771b0 / FUN_140176e90: a point inside the quad takes the plane height; a point > 20 above is not accepted strictly.
        let cad = corridor(false);
        assert_eq!(cad.node_count(), 3);
        assert_eq!(cad.node_at(Vec3::new(50.0, 5.0, 50.0), true), 0);
        assert_eq!(cad.node_at(Vec3::new(250.0, 5.0, 50.0), true), 2);
        assert_eq!(cad.node_at(Vec3::new(50.0, 25.0, 50.0), true), -1, "plane within 20.0 only (0x1402f7fc4)");
        assert_eq!(cad.node_at(Vec3::new(50.0, 25.0, 50.0), false), 0, "loose: the nearest node whatever the height");
        assert_eq!(cad.node_at(Vec3::new(500.0, 0.0, 50.0), false), 2, "loose off the graph: nearest boundary");
    }

    #[test]
    fn z_mirror_does_not_change_the_node_lookup() {
        let (a, b) = (corridor(false), corridor(true));
        assert_eq!(b.node_at(Vec3::new(150.0, 0.0, -50.0), true), 1);
        assert_eq!(a.node_at(Vec3::new(150.0, 0.0, 50.0), true), 1);
        assert!((b.centroid(1) - Vec3::new(150.0, 0.0, -50.0)).length() < 1e-3);
    }

    #[test]
    fn stored_routes_and_ground_line() {
        let cad = corridor(false);
        assert_eq!(cad.route(0, 0, 2), Some(vec![1, 2]));
        assert_eq!(cad.route(3, 0, 2), None);
        // FUN_140179f80: straight line across both plain portals; the 16-hop loop ends on the target node.
        assert!(cad.ground_ok(Vec3::new(50.0, 0.0, 50.0), Vec3::new(250.0, 0.0, 50.0)));
        assert!(!cad.ground_ok(Vec3::new(50.0, 0.0, 50.0), Vec3::new(250.0, 0.0, 500.0)), "target off the graph (strict node -1)");
        // FUN_1401790e0: stored edge lengths along the route: 100 + 100 -> -1 when a step has no edge.
        assert!((cad.route_len(0, 2, 0) - 200.0).abs() < 1e-3);
        assert_eq!(cad.route_len(0, 0, 0), -1.0);
    }

    #[test]
    fn steering_follows_the_ring_to_the_next_node_centroid() {
        // FORMATS-CPU 5.7: no line of sight, steer target 5+ away: plan, then aim at the next node's centroid (code 6 without a
        // matching edge test, 4 when the line passes through the portal).
        let cad = corridor(false);
        let mut com = Com::at(Vec3::new(50.0, 0.0, 50.0));
        com.steer = Vec3::new(250.0, 0.0, 50.0);
        com.goal = com.steer;
        let body = BodyCtx { pos: Vec3::new(50.0, 0.0, 50.0), standing_or_running: true, dodge_state_ok: true, unlock_state: true, ..Default::default() };
        // Force "no line of sight": block the ground line by pretending the portal test failed (a wall).
        let mut cad2 = cad.clone();
        cad2.edges[0].flags = 1; // one-way edges break `ground_ok` (flags != 0 are skipped)
        let pad = com.after_script(&cad2, &body, &AllClear);
        assert!(!com.los);
        assert_eq!(com.ring, vec![0, 1, 2], "ring = [my node, route...]");
        assert_eq!(com.goal_node, 2);
        assert_eq!(pad.mag, 1.0);
        // Toward +x (yaw 90 degrees = 8192 units; atan2(dx, dz) with dz = 0).
        assert!((pad.yaw_units - 8192).abs() < 400, "yaw units {}", pad.yaw_units);
    }

    #[test]
    fn stick_is_zero_inside_five_units_and_full_outside() {
        // FUN_140178ad0: |steer - me| (xz) > 5.0 -> magnitude COM+0x38 = 1.0, else 0.
        let cad = corridor(false);
        let mut com = Com::at(Vec3::new(50.0, 0.0, 50.0));
        let mut body = BodyCtx { pos: Vec3::new(50.0, 0.0, 50.0), standing_or_running: true, dodge_state_ok: true, unlock_state: true, ..Default::default() };
        com.steer = Vec3::new(53.0, 0.0, 50.0);
        assert_eq!(com.after_script(&cad, &body, &AllClear).mag, 0.0);
        com.steer = Vec3::new(57.0, 0.0, 50.0);
        assert_eq!(com.after_script(&cad, &body, &AllClear).mag, 1.0);
        // Airborne with the target above the apex: released (no air control).
        body.airborne = true;
        body.vy_tick = 0.5;
        com.steer = Vec3::new(120.0, 80.0, 50.0);
        assert_eq!(com.after_script(&cad, &body, &AllClear).mag, 0.0);
    }

    #[test]
    fn dodge_request_presses_the_button_only_in_states_0_1_8() {
        // FUN_140178ad0 + FUN_14024ec80: path49(deg) -> button 0x1000000 and the angle COM+0x44 = deg * 91.0222; dropped in other states.
        let cad = corridor(false);
        let mut com = Com::at(Vec3::new(50.0, 0.0, 50.0));
        com.request_dodge(90);
        assert_eq!(com.dodge_yaw, 8191, "(int)(90 * 91.02222) truncates");
        let mut body = BodyCtx { pos: Vec3::new(50.0, 0.0, 50.0), standing_or_running: true, dodge_state_ok: true, unlock_state: true, ..Default::default() };
        let pad = com.after_script(&cad, &body, &AllClear);
        assert_eq!((pad.dodge, pad.mag), (Some(8191), 1.0));
        body.dodge_state_ok = false;
        assert_eq!(com.after_script(&cad, &body, &AllClear).dodge, None);
    }

    #[test]
    fn move_to_and_nav_step_respect_the_steer_lock_and_blocked_nodes() {
        let mut cad = corridor(false);
        let mut com = Com::at(Vec3::ZERO);
        com.steer_lock = true;
        com.move_to(Vec3::new(9.0, 9.0, 9.0));
        assert_eq!(com.steer, Vec3::ZERO, "0x14020d660: only when COM+0x10 == 0");
        com.steer_lock = false;
        assert!(com.nav_step(&cad, 1));
        assert!((com.steer - Vec3::new(150.0, 0.0, 50.0)).length() < 1e-3);
        cad.nodes[2].flags |= NF_BLOCKED;
        assert!(!com.nav_step(&cad, 2), "0x14020d6b0: a blocked node (flags & 0xc) is refused");
    }

    #[test]
    fn path_step_reports_arrival_only_in_the_goal_node_within_seven_units() {
        // FORMATS-CPU 5.4 (`FUN_14020e380`): 0 no goal node, 1 following, 2 arrived.
        let cad = corridor(false);
        let mut com = Com::at(Vec3::new(50.0, 0.0, 50.0));
        com.goal = Vec3::new(250.0, 0.0, 50.0);
        com.cur_node = 0;
        com.los = true;
        assert_eq!(com.path_step(&cad, Vec3::new(50.0, 0.0, 50.0)), 1, "line of sight: steer := goal");
        com.los = false;
        com.cur_node = 2;
        assert_eq!(com.path_step(&cad, Vec3::new(244.0, 0.0, 50.0)), 2);
        assert_eq!(com.path_step(&cad, Vec3::new(200.0, 0.0, 50.0)), 1);
        com.goal = Vec3::new(900.0, 0.0, 50.0);
        com.cur_node = 0;
        assert_eq!(com.path_step(&cad, Vec3::new(50.0, 0.0, 50.0)), 0, "no line of sight and an empty ring: 0 (the loose lookup still finds a goal node)");
        com.ring = vec![0, 1, 2];
        assert_eq!(com.path_step(&cad, Vec3::new(50.0, 0.0, 50.0)), 1, "ring's last node is not my node: following");
    }

    #[test]
    fn blocked_node_in_the_ring_is_bridged_or_dropped() {
        // FUN_1401796b0: blocked nodes in the ring are bridged by a stored route between the last good and the next good node.
        let mut cad = corridor(false);
        cad.nodes[1].flags |= NF_BLOCKED;
        let mut com = Com::at(Vec3::ZERO);
        com.cur_node = 0;
        com.goal_node = 2;
        com.plan(&cad, 0);
        assert!(com.ring.is_empty(), "no route avoids the blocked middle node: the ring is dropped (then mode 1: T4 has none)");
    }
}
