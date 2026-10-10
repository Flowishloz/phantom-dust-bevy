//! Hanging parts: the game's cloth, plus an older pendulum.
//!
//! Character glTFs name body joints `p0j<N>` and extra-part joints
//! `p<K>j<N>` / `c<K>j<N>` (K >= 1); part roots are grafted onto a body
//! joint (see pd_formats::character). Parts whose joints carry the
//! converter's `cloth_*` extras (every part in the game's files) run the
//! program's own cloth grid ("The program's cloth grid" below,
//! PROGRAM-RE.md "Character parts / cloth"). Anything else falls back to the
//! older pendulum: every `c` joint with exactly one part-joint child swings
//! toward hanging straight down while springing back toward its rest shape.
//!
//! Purely cosmetic: runs after transform propagation and never touches the
//! match simulation.

use bevy::gltf::GltfExtras;
use bevy::prelude::*;
use std::collections::HashMap;

/// 0 = keep the modelled rest shape, 1 = hang straight down.
const HANG: f32 = 0.55;
/// How strongly a segment is pulled toward its target, per second.
/// Lower = heavier, lazier cloth.
const STIFFNESS: f32 = 6.0;
/// Fraction of velocity kept per 1/60 s. Lower = settles faster.
const DAMPING: f32 = 0.7;
/// Maximum tip speed as a fraction of segment length per 1/60 s (stops
/// violent flips when the body turns sharply).
const MAX_STEP: f32 = 0.25;
/// Extra clearance kept between cloth joints and the body capsules.
const SKIN_MARGIN: f32 = 0.6;
/// Share of the pivot's own movement carried straight to the cloth. The
/// rest lags and swings. At 0 (pure world-space pendulum) a running
/// character's tails trailed at a fixed angle and never swayed.
const CARRY: f32 = 0.95;
/// How far beyond its attachment depth a piece is held off a body shape it
/// starts inside (covers the curve of a back under a coat tail).
const ATTACH_CLEARANCE: f32 = 0.4;

/// A body bone's collision capsule (from the converter's node extras):
/// from this joint to `child` (or a sphere if none).
#[derive(Component)]
pub struct BodyCapsule {
    radius: f32,
    child: Option<Entity>,
    owner: Entity,
}

/// The character (top-level spawned entity) a cloth root belongs to.
#[derive(Component)]
pub struct DangleOwner(Entity);

fn top_ancestor(mut e: Entity, parents: &Query<&ChildOf>) -> Entity {
    while let Ok(p) = parents.get(e) {
        e = p.parent();
    }
    e
}

#[derive(Component)]
pub struct DangleRoot;

#[derive(Component)]
pub struct Dangle {
    rest_rot: Quat,
    rest_dir: Vec3,
    len: f32,
    tip: Vec3,
    prev: Vec3,
    last_pivot: Vec3,
    /// ATTACH_CLEARANCE for the game's own cloth parts; 0 for the generated
    /// coat rig (part 9), whose bones start inside the thighs and would flare.
    clearance: f32,
    ready: bool,
}

#[derive(Component)]
pub struct DangleChecked;

/// A joint belonging to an extra part (not the body).
#[derive(Component)]
pub struct PartJoint;

/// Part number from a joint name: "p0j5" -> 0 (body), "c1j3" -> 1 (cloth
/// chain). Rigid part joints ("p1j3") return None so they never swing.
fn part_index(name: &str) -> Option<u32> {
    let rest = name.strip_prefix('c').or_else(|| name.strip_prefix("p0j").map(|_| &name[1..]))?;
    let (k, j) = rest.split_once('j')?;
    j.parse::<u32>().ok()?;
    k.parse().ok()
}

/// Tag part joints once their scene has spawned. Parts built as the
/// program's cloth grid (see "The program's cloth grid" below) get a
/// ClothPart instead and are left alone by the pendulum.
pub fn setup_dangles(
    mut commands: Commands,
    new: Query<(Entity, &Name, &Transform, Option<&Children>, &ChildOf, Option<&GltfExtras>), Without<DangleChecked>>,
    names: Query<(&Name, &Transform)>,
    parents: Query<&ChildOf>,
    tree: Query<(&Transform, Option<&Children>, Option<&GltfExtras>, &Name)>,
) {
    let cloth_on = cloth_mode() != "pendulum";
    for (e, name, t, children, parent, extras) in &new {
        commands.entity(e).insert(DangleChecked);
        if cloth_on && is_part_root(name.as_str(), names.get(parent.parent()).ok().map(|(n, _)| n.as_str())) {
            build_cloth(&mut commands, e, parent.parent(), &tree);
        }
        if let Some(x) = extras.and_then(|x| serde_json::from_str::<serde_json::Value>(&x.value).ok()) {
            if let Some(r) = x["capsule_radius"].as_f64() {
                let child = x["capsule_child"].as_u64().and_then(|ci| {
                    let want = format!("j{ci}");
                    children?.iter().find(|&c| names.get(c).is_ok_and(|(n, _)| n.as_str().starts_with('p') && n.as_str().ends_with(&want) && n.as_str()[1..].split_once('j').is_some_and(|(k, _)| k == "0")))
                });
                commands.entity(e).insert(BodyCapsule { radius: r as f32, child, owner: top_ancestor(e, &parents) });
            }
        }
        let Some(k) = part_index(name.as_str()) else { continue };
        if k == 0 {
            continue;
        }
        commands.entity(e).insert(PartJoint);
        if names.get(parent.parent()).is_ok_and(|(n, _)| n.as_str().starts_with('p')) {
            commands.entity(e).insert((DangleRoot, DangleOwner(top_ancestor(e, &parents))));
        }
        let kids: Vec<Entity> = children
            .map(|c| c.iter().filter(|&c| names.get(c).is_ok_and(|(n, _)| part_index(n.as_str()).is_some_and(|x| x >= 1))).collect())
            .unwrap_or_default();
        if let [child] = kids.as_slice() {
            let offset = names.get(*child).map(|(_, ct)| ct.translation).unwrap_or(Vec3::X);
            let len = offset.length();
            if len > 1e-3 {
                commands.entity(e).insert(Dangle { rest_rot: t.rotation, rest_dir: offset / len, len, tip: Vec3::ZERO, prev: Vec3::ZERO, last_pivot: Vec3::ZERO, clearance: if k == pd_coat_part() { 0.0 } else { ATTACH_CLEARANCE }, ready: false });
            }
        }
    }
}

/// Swing every chain, writing both local and global transforms so the
/// skinned mesh sees this frame's result.
pub fn swing_dangles(
    time: Res<Time>,
    roots: Query<(Entity, &ChildOf, &DangleOwner), (With<DangleRoot>, Without<ClothJoint>)>,
    globals: Query<&GlobalTransform, (Without<PartJoint>, Without<ClothJoint>)>,
    capsules: Query<(&BodyCapsule, &GlobalTransform), (Without<PartJoint>, Without<ClothJoint>)>,
    mut joints: Query<(&mut Transform, &mut GlobalTransform, Option<&mut Dangle>, Option<&Children>), (With<PartJoint>, Without<ClothJoint>)>,
    mut parts: Query<&mut ClothPart>,
    mut cloth_joints: Query<(&mut Transform, &mut GlobalTransform), With<ClothJoint>>,
) {
    swing_cloth(time.delta_secs(), &mut parts, &globals, &mut cloth_joints);
    let dt = time.delta_secs().min(1.0 / 20.0);
    if dt <= 0.0 {
        return;
    }
    let pull = 1.0 - (-STIFFNESS * dt).exp();
    let keep = DAMPING.powf(dt * 60.0);
    // This frame's body capsules in world space, grouped by character.
    let mut shapes: HashMap<Entity, Vec<(Vec3, Vec3, f32)>> = HashMap::new();
    for (cap, g) in &capsules {
        let a = g.translation();
        let b = cap.child.and_then(|c| globals.get(c).ok()).map(|g| g.translation()).unwrap_or(a);
        shapes.entry(cap.owner).or_default().push((a, b, cap.radius));
    }
    let empty = Vec::new();
    for (root, parent, owner) in &roots {
        let body = shapes.get(&owner.0).unwrap_or(&empty);
        let Ok(host) = globals.get(parent.parent()) else { continue };
        let mut stack = vec![(root, host.compute_transform())];
        while let Some((e, parent_world)) = stack.pop() {
            let Ok((mut local, mut global, dangle, children)) = joints.get_mut(e) else { continue };
            if let Some(mut d) = dangle {
                let rest_world_rot = parent_world.rotation * d.rest_rot;
                let pivot = parent_world.transform_point(local.translation);
                let rest_dir = rest_world_rot * d.rest_dir;
                let hang = std::env::var("PD_CLOTH_HANG").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(HANG);
                let want = (rest_dir.lerp(Vec3::NEG_Y, hang)).normalize_or(rest_dir);
                let target = pivot + want * d.len;
                if !d.ready || d.tip.distance(pivot) > d.len * 8.0 {
                    d.tip = target;
                    d.prev = target;
                    d.last_pivot = pivot;
                    d.ready = true;
                }
                // Carry most of the body's movement straight through (tip and
                // its history move together, so no velocity is injected).
                let carried = (pivot - d.last_pivot) * CARRY;
                d.tip += carried;
                d.prev += carried;
                d.last_pivot = pivot;
                let vel = (d.tip - d.prev) * keep;
                let mut step = vel + (target - d.tip) * pull;
                let cap = d.len * MAX_STEP * dt * 60.0;
                if step.length() > cap {
                    step = step.normalize() * cap;
                }
                let mut next = d.tip + step;
                next = pivot + (next - pivot).normalize_or(want) * d.len;
                // Keep the tip outside the body, then restore segment length.
                for _ in 0..2 {
                    for &(a, b, r) in body {
                        // Never deeper into the body than the point the piece
                        // hangs from: a strip attached inside a shape (coat
                        // bones inside the thighs, a coat tail hung just
                        // inside the back) is held at its attachment depth
                        // instead of being flung out or ignored.
                        let pivot_depth = pivot.distance(closest_on_segment(pivot, a, b));
                        let min = (r + SKIN_MARGIN).min(pivot_depth + d.clearance);
                        if min < 0.05 {
                            continue;
                        }
                        let q = closest_on_segment(next, a, b);
                        let off = next - q;
                        if off.length_squared() < min * min {
                            next = q + off.normalize_or(Vec3::Z) * min;
                        }
                        // Also keep the middle of the segment out of the body
                        // (a leg can pass between pivot and tip).
                        let mid = (pivot + next) * 0.5;
                        let qm = closest_on_segment(mid, a, b);
                        let offm = mid - qm;
                        if offm.length_squared() < min * min {
                            next += (qm + offm.normalize_or(Vec3::Z) * min - mid) * 2.0;
                        }
                    }
                    next = pivot + (next - pivot).normalize_or(want) * d.len;
                }
                d.prev = d.tip;
                d.tip = next;
                let dir = (d.tip - pivot).normalize_or(want);
                let world_rot = Quat::from_rotation_arc(rest_dir, dir) * rest_world_rot;
                local.rotation = parent_world.rotation.inverse() * world_rot;
            }
            let world = parent_world.mul_transform(*local);
            *global = GlobalTransform::from(world);
            if let Some(children) = children {
                for c in children.iter() {
                    stack.push((c, world));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The program's cloth grid (PROGRAM-RE.md "Character parts / cloth").
//
// Every extra part in the game's character files is a cloth: its root joint
// (CLCnn) follows the body joint CLPnn, and its joints named CL<d><row><col>
// form a grid -- rows are strips, columns run down each strip. The converter
// writes each node's row, column, velocity kept per tick and angle limit as
// node extras. Per 30 Hz tick (FUN_14025ec30): column 0 is pinned to its
// animated position; every other node moves by its velocity plus gravity
// (0.12 u/tick², the fighter's own gravity); then, down each strip, the node
// is put back at its rest distance and the joint above it turns toward it by
// at most that joint's angle limit (relative to its animated rest direction),
// which places the node; neighbouring strips are pulled to their rest spacing
// (half each, from the middle strip outward); velocity = movement × kept
// share, capped at 1 u/tick. Finally each joint aims straight at the next
// node. No body collision is modelled here (pc00a has none; COLF/COLP shapes
// of other characters ❓).
//
// PD_CLOTH=game (default) | pendulum (the older swing above) | free (no angle
// limits) | nocross (no strip-to-strip links) | unlock (0° limits widened) --
// calibration switches (rigid: pd_viewer --no-cloth).

const CLOTH_TICK: f32 = 1.0 / 30.0;
/// u/tick² (FUN_14025d380: 1.0 × the constant at 0x1402f7ce4).
const CLOTH_GRAVITY: f32 = 0.12;
/// u/tick (FUN_14024ec80 passes 1.0).
const CLOTH_MAX_SPEED: f32 = 1.0;

fn cloth_mode() -> String {
    std::env::var("PD_CLOTH").unwrap_or_else(|_| "game".into())
}

/// A grid node read from the converter's extras.
#[derive(Clone, Copy)]
struct ClothNodeInfo {
    row: usize,
    col: usize,
    keep: f32,
    limit: f32,
}

/// A joint of a part driven by the cloth grid (the pendulum skips these).
#[derive(Component)]
pub struct ClothJoint;

/// One cloth part, on its root joint.
#[derive(Component)]
pub struct ClothPart {
    host: Entity,
    joints: Vec<Entity>,
    parent: Vec<Option<usize>>,
    local: Vec<bevy::math::Affine3A>,
    grid: Vec<Vec<Option<usize>>>,
    keep: Vec<f32>,
    limit: Vec<f32>,
    along: Vec<Vec<f32>>,
    across: Vec<Vec<f32>>,
    pos: Vec<Vec<Vec3>>,
    prev: Vec<Vec<Vec3>>,
    vel: Vec<Vec<Vec3>>,
    acc: f32,
    ready: bool,
    last_host: Vec3,
}

fn cloth_info(extras: Option<&GltfExtras>) -> Option<ClothNodeInfo> {
    let x: serde_json::Value = serde_json::from_str(&extras?.value).ok()?;
    Some(ClothNodeInfo {
        row: x["cloth_row"].as_u64()? as usize,
        col: x["cloth_col"].as_u64()? as usize,
        keep: x["cloth_keep"].as_f64()? as f32,
        limit: x["cloth_limit"].as_f64()? as f32,
    })
}

/// A part root: named "p<K>j<N>" / "c<K>j<N>" with K >= 1, hung under a body joint "p0j<N>".
fn is_part_root(name: &str, parent: Option<&str>) -> bool {
    let is_part = name.len() > 1 && (name.starts_with('p') || name.starts_with('c')) && name[1..].split_once('j').is_some_and(|(k, j)| j.parse::<u32>().is_ok() && k.parse::<u32>().is_ok_and(|k| k >= 1));
    is_part && parent.is_some_and(|p| p.starts_with("p0j"))
}

/// Build a ClothPart on a part root if its subtree has grid nodes.
fn build_cloth(commands: &mut Commands, root: Entity, host: Entity, tree: &Query<(&Transform, Option<&Children>, Option<&GltfExtras>, &Name)>) {
    // Depth first, parents before children.
    let (mut joints, mut par, mut local, mut infos) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut stack = vec![(root, None::<usize>)];
    while let Some((j, p)) = stack.pop() {
        let Ok((t, children, extras, _)) = tree.get(j) else { continue };
        let idx = joints.len();
        joints.push(j);
        par.push(p);
        local.push(t.compute_affine());
        infos.push(cloth_info(extras));
        for c in children.into_iter().flat_map(|ch| ch.iter()) {
            if tree.get(c).is_ok_and(|(.., n)| !n.as_str().starts_with("p0j")) {
                stack.push((c, Some(idx)));
            }
        }
    }
    let rows = infos.iter().flatten().map(|i: &ClothNodeInfo| i.row + 1).max().unwrap_or(0);
    let cols = infos.iter().flatten().map(|i: &ClothNodeInfo| i.col + 1).max().unwrap_or(0);
    if rows == 0 || cols == 0 {
        return;
    }
    let mut grid = vec![vec![None; cols]; rows];
    let (mut keep, mut limit) = (vec![1.0; joints.len()], vec![0.0; joints.len()]);
    for (k, i) in infos.iter().enumerate() {
        if let Some(i) = i {
            grid[i.row][i.col] = Some(k);
            keep[k] = i.keep;
            limit[k] = i.limit;
        }
    }
    // PD_CLOTH=unlock (calibration only): a node whose file limit is 0°
    // (pc00a's CL030, which the program keeps rigid) gets its column's widest limit.
    if cloth_mode() == "unlock" {
        for c in 0..cols {
            let widest = (0..rows).filter_map(|r| grid[r][c]).map(|k| limit[k]).fold(0.0f32, f32::max);
            for r in 0..rows {
                if let Some(k) = grid[r][c].filter(|&k| limit[k] <= 0.0) {
                    limit[k] = widest;
                }
            }
        }
    }
    // Rest lengths from the bind pose (the program measures them once, FUN_14025fc30).
    let mut bind = vec![bevy::math::Affine3A::IDENTITY; joints.len()];
    for k in 0..joints.len() {
        bind[k] = par[k].map(|p| bind[p]).unwrap_or_default() * local[k];
    }
    let at = |r: usize, c: usize| grid.get(r).and_then(|row: &Vec<Option<usize>>| row.get(c).copied().flatten()).map(|k| Vec3::from(bind[k].translation));
    let along = (0..rows).map(|r| (0..cols).map(|c| if c == 0 { 0.0 } else { at(r, c - 1).zip(at(r, c)).map_or(0.0, |(a, b)| a.distance(b)) }).collect()).collect();
    let across = (0..rows).map(|r| (0..cols).map(|c| at(r, c).zip(at(r + 1, c)).map_or(0.0, |(a, b)| a.distance(b))).collect()).collect();
    for &j in &joints {
        commands.entity(j).insert(ClothJoint);
    }
    let zero = vec![vec![Vec3::ZERO; cols]; rows];
    commands.entity(root).insert(ClothPart {
        host,
        joints,
        parent: par,
        local,
        grid,
        keep,
        limit,
        along,
        across,
        pos: zero.clone(),
        prev: zero.clone(),
        vel: zero,
        acc: 0.0,
        ready: false,
        last_host: Vec3::ZERO,
    });
}

/// Turn `m`'s rotation so that direction `from` points along `to`, by at most `limit` radians.
fn aim(m: &mut bevy::math::Affine3A, from: Vec3, to: Vec3, limit: f32) {
    let (a, b) = (from.normalize_or_zero(), to.normalize_or_zero());
    if a == Vec3::ZERO || b == Vec3::ZERO {
        return;
    }
    let axis = a.cross(b);
    if axis.length_squared() < 1e-12 {
        return;
    }
    let angle = a.dot(b).clamp(-1.0, 1.0).acos().min(limit);
    let q = Quat::from_axis_angle(axis.normalize(), angle);
    m.matrix3 = bevy::math::Mat3A::from_quat(q) * m.matrix3;
}

impl ClothPart {
    fn fk(&self, host: bevy::math::Affine3A) -> Vec<bevy::math::Affine3A> {
        let mut w = vec![bevy::math::Affine3A::IDENTITY; self.joints.len()];
        for k in 0..self.joints.len() {
            w[k] = self.parent[k].map(|p| w[p]).unwrap_or(host) * self.local[k];
        }
        w
    }

    fn reset(&mut self, host: bevy::math::Affine3A) {
        let w = self.fk(host);
        for (r, row) in self.grid.iter().enumerate() {
            for (c, k) in row.iter().enumerate() {
                if let Some(k) = k {
                    self.pos[r][c] = w[*k].translation.into();
                    self.prev[r][c] = self.pos[r][c];
                    self.vel[r][c] = Vec3::ZERO;
                }
            }
        }
        self.ready = true;
    }

    /// One 30 Hz step of the program's cloth (FUN_14025ec30, mode 0).
    fn tick(&mut self, host: bevy::math::Affine3A, limits: bool, cross: bool) {
        let mut w = self.fk(host);
        let (rows, cols) = (self.grid.len(), self.grid.first().map_or(0, |r| r.len()));
        for r in 0..rows {
            for c in 0..cols {
                let Some(k) = self.grid[r][c] else { continue };
                if c == 0 {
                    self.pos[r][c] = w[k].translation.into();
                    self.prev[r][c] = self.pos[r][c];
                } else {
                    self.pos[r][c] += self.vel[r][c] + Vec3::new(0.0, -CLOTH_GRAVITY, 0.0);
                }
            }
        }
        // Down each strip: rest distance, then the limited turn of the joint above.
        for r in 0..rows {
            for c in 1..cols {
                let (Some(kp), Some(kc)) = (self.grid[r][c - 1], self.grid[r][c]) else { continue };
                let d = self.pos[r][c] - self.pos[r][c - 1];
                if d.length_squared() > 1e-12 {
                    self.pos[r][c] = self.pos[r][c - 1] + d.normalize() * self.along[r][c];
                }
                let mut m = self.parent[kp].map(|p| w[p]).unwrap_or(host) * self.local[kp];
                let bind_dir = m.matrix3 * bevy::math::Vec3A::from(self.local[kc].translation);
                let lim = if limits { self.limit[kp] } else { std::f32::consts::TAU };
                aim(&mut m, bind_dir.into(), self.pos[r][c] - self.pos[r][c - 1], lim);
                w[kp] = m;
                self.pos[r][c] = m.transform_point3(self.local[kc].translation.into());
            }
        }
        // Neighbouring strips keep their spacing (FUN_14025f920).
        if cross && rows >= 2 {
            let m = (rows - 1) / 2;
            let link = |a: usize, b: usize, s: &mut Self| {
                for c in 1..cols {
                    if s.grid[a][c].is_none() || s.grid[b][c].is_none() {
                        continue;
                    }
                    let d = s.pos[b][c] - s.pos[a][c];
                    let n = d.length();
                    if n <= 0.0 {
                        continue;
                    }
                    let corr = d * ((n - s.across[a][c]) / n) * 0.5;
                    s.pos[a][c] += corr;
                    s.pos[b][c] -= corr;
                }
            };
            if rows % 2 == 0 {
                link(m, m + 1, self);
            }
            for i in 0..m {
                link(m - i - 1, m - i, self);
                link(rows - m + i - 1, rows - m + i, self);
            }
        }
        for r in 0..rows {
            for c in 0..cols {
                let Some(k) = self.grid[r][c] else { continue };
                let v = (self.pos[r][c] - self.prev[r][c]) * self.keep[k];
                self.vel[r][c] = v.clamp_length_max(CLOTH_MAX_SPEED);
                self.prev[r][c] = self.pos[r][c];
            }
        }
    }

    /// Joint world matrices for drawing: animated rest pose, each node's
    /// joint aimed straight at the next node down its strip.
    fn pose(&self, host: bevy::math::Affine3A) -> Vec<bevy::math::Affine3A> {
        let mut at = vec![None; self.joints.len()];
        for (r, row) in self.grid.iter().enumerate() {
            for (c, k) in row.iter().enumerate() {
                if let Some(k) = k {
                    at[*k] = Some((r, c));
                }
            }
        }
        let mut w = vec![bevy::math::Affine3A::IDENTITY; self.joints.len()];
        for k in 0..self.joints.len() {
            w[k] = self.parent[k].map(|p| w[p]).unwrap_or(host) * self.local[k];
            if let Some((r, c)) = at[k] {
                if let Some(kc) = self.grid[r].get(c + 1).copied().flatten() {
                    let from = w[k].matrix3 * bevy::math::Vec3A::from(self.local[kc].translation);
                    aim(&mut w[k], from.into(), self.pos[r][c + 1] - self.pos[r][c], std::f32::consts::TAU);
                }
            }
        }
        w
    }
}

/// Run the cloth parts at the program's 30 Hz and pose their joints
/// (called from swing_dangles, which the apps already schedule).
fn swing_cloth(
    dt: f32,
    parts: &mut Query<&mut ClothPart>,
    globals: &Query<&GlobalTransform, (Without<PartJoint>, Without<ClothJoint>)>,
    joints: &mut Query<(&mut Transform, &mut GlobalTransform), With<ClothJoint>>,
) {
    let mode = cloth_mode();
    let (limits, cross) = (mode != "free", mode != "nocross");
    let dt = dt.clamp(0.0, 0.25);
    for mut part in parts.iter_mut() {
        let Ok(host) = globals.get(part.host) else { continue };
        let host = host.affine();
        let hp: Vec3 = host.translation.into();
        if !part.ready || part.last_host.distance(hp) > 60.0 {
            part.reset(host);
            part.acc = 0.0;
        }
        part.last_host = hp;
        part.acc += dt;
        while part.acc >= CLOTH_TICK {
            part.acc -= CLOTH_TICK;
            part.tick(host, limits, cross);
        }
        let w = part.pose(host);
        for k in 0..part.joints.len() {
            let Ok((mut local, mut global)) = joints.get_mut(part.joints[k]) else { continue };
            let pw = part.parent[k].map(|p| w[p]).unwrap_or(host);
            let (s, r, t) = (pw.inverse() * w[k]).to_scale_rotation_translation();
            *local = Transform { translation: t, rotation: r, scale: s };
            *global = GlobalTransform::from(w[k]);
        }
    }
}

fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let l = ab.length_squared();
    if l < 1e-8 {
        return a;
    }
    a + ab * ((p - a).dot(ab) / l).clamp(0.0, 1.0)
}

/// Part number the converter uses for the generated coat rig
/// (pd_formats::skirt::PART; duplicated to keep this file engine-only).
const fn pd_coat_part() -> u32 {
    9
}
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Affine3A;

    /// One strip of two joints sticking out sideways (+X) from the host.
    fn strip(limit_deg: f32) -> ClothPart {
        let local = vec![Affine3A::IDENTITY, Affine3A::IDENTITY, Affine3A::from_translation(Vec3::new(2.0, 0.0, 0.0))];
        ClothPart {
            host: Entity::PLACEHOLDER,
            joints: vec![Entity::PLACEHOLDER; 3],
            parent: vec![None, Some(0), Some(1)],
            local,
            grid: vec![vec![Some(1), Some(2)]],
            keep: vec![1.0, 1.0, 0.5],
            limit: vec![0.0, limit_deg.to_radians(), 0.0],
            along: vec![vec![0.0, 2.0]],
            across: vec![vec![0.0, 0.0]],
            pos: vec![vec![Vec3::ZERO; 2]],
            prev: vec![vec![Vec3::ZERO; 2]],
            vel: vec![vec![Vec3::ZERO; 2]],
            acc: 0.0,
            ready: false,
            last_host: Vec3::ZERO,
        }
    }

    fn droop_deg(p: &ClothPart) -> f32 {
        let d = p.pos[0][1] - p.pos[0][0];
        (-d.y).atan2(d.x).to_degrees()
    }

    #[test]
    fn cloth_turns_no_further_than_its_angle_limit() {
        let mut p = strip(10.0);
        p.reset(Affine3A::IDENTITY);
        for _ in 0..120 {
            p.tick(Affine3A::IDENTITY, true, true);
        }
        assert!((droop_deg(&p) - 10.0).abs() < 0.5, "droop {}", droop_deg(&p));
        assert!((p.pos[0][1] - p.pos[0][0]).length() - 2.0 < 1e-3);
        // The drawn pose aims the joint at the node.
        let w = p.pose(Affine3A::IDENTITY);
        let tip = w[1].transform_point3(Vec3::new(2.0, 0.0, 0.0));
        assert!(tip.distance(p.pos[0][1]) < 1e-3);
    }

    #[test]
    fn without_limits_cloth_hangs_down() {
        let mut p = strip(10.0);
        p.reset(Affine3A::IDENTITY);
        for _ in 0..600 {
            p.tick(Affine3A::IDENTITY, false, true);
        }
        assert!(droop_deg(&p) > 80.0, "droop {}", droop_deg(&p));
    }

    #[test]
    fn part_roots_are_recognised() {
        assert!(is_part_root("p1j0", Some("p0j77")));
        assert!(is_part_root("c2j0", Some("p0j5")));
        assert!(!is_part_root("p0j3", Some("p0j2")));
        assert!(!is_part_root("c1j1", Some("p1j0")));
        assert!(!is_part_root("body", Some("p0j1")));
    }
}
