//! The school aura's wisps, drawn as the program does (lane fxfollow, 2026-10-01).
//!
//! `eft0600`'s `PDEFTAURA01` pieces are wisp SPAWNERS (`WispSpec`, FORMATS-EFFECTS.md
//! "School aura wisps"). What a wisp is, read from the aura class (`pdEftBone1`'s vtable,
//! `0x14047a740`; update `FUN_14011acd0`, draw `FUN_14011b450`, UV / matrix set-up
//! `FUN_1401a7970`, rope `FUN_1401a0330` / `FUN_1401a03c0`):
//!
//! - **A rope of 9 nodes hangs from the piece's bone** (READ). Every effect frame the wisp's
//!   anchor is set to the bone's world position and each node `i` is pulled toward
//!   `anchor + (0, i, 0)` (node spacing 1 u, before the wisp's scale) by `cur += (target - cur)
//!   x (1 - w_i)`, `w_i = 0.9 x sin(i x 11.25 deg)` -- node 0 sticks to the bone, node 8 keeps 90 %
//!   of where it was, so a wisp on a moving hand trails and bends along the bone's recent path.
//!   (The target also carries a drift vector `x w_i`, written from a global at manager
//!   `+0x11598` whose source is not identified: taken as 0, UNCONFIRMED.) The table the sines come
//!   from (`0x14054ca90`) is filled at start-up; taken as a sine, UNCONFIRMED.
//! - **The wisp mesh is skinned over 8 bones from that rope** (READ; `eft0600`'s draws are format
//!   `0x20`, 160 vertices, 7 palette bones, bind pose all identity). Bone `k` runs from node `k` to node
//!   `k + 1`: its origin is node `k` relative to the anchor and its axes are built from the segment direction
//!   `f` (`FUN_14011b450`): X = `normalize(Z x f)`, Y = `f`, Z = the third (rows of the program's
//!   row-vector matrix; for a straight vertical rope that is X, Y, -Z), then twisted about `f` by the
//!   wisp's random roll (`FUN_1402906e0`, 0 .. word 25 degrees). A vertex blends its two bones' matrices by its
//!   weights (`PDEFTAURA01_vs`).
//! - **The wisp is drawn through the joint's position and scale with world axes** (word 28 = 6)
//!   **scaled by `k x grow`, with no `+ 1`** (`FUN_1401a7970`: `k x word 16..18 + 0.0`, k = the
//!   word-9 curve over the wisp's own clock): a wisp starts as nothing and grows to its random share of
//!   the grow words. The old playback used `1 + grow x k`.
//! - **UV scroll** for this class (`FUN_1401a7970`, constants 0x22 / 0x24 = `c34` / `c36` of
//!   `PDEFTAURA01_vs`): texture 0's UV moves by `(k x word 4, k x word 5 + word 11)` and texture
//!   1's by `(k x word 6, k x word 7)`, `k` = the word-8 curve. (Not words 22 / 23: those belong
//!   to the generic pieces.) Word 11 is 0 in `eft0600`.
//! - **Spawn** (`FUN_14011acd0`): every effect frame each piece makes spawn tries while the shared
//!   pool (80 wisps for everything) has a free slot, stopping at the first failed try; a try
//!   succeeds with chance `(r x word 24) >> 15 == 1`. One piece can therefore have several wisps
//!   alive: each gets its own body here (`FxWispBody`), so the old two-copies stand-in
//!   (`PD_AURA_COPIES`) is gone.
//! - **Hand suppress** (`FUN_14011acd0`, effect instance `+0x88`): when set, no wisp is started on a piece
//!   whose joint is `RHAND` or `LHAND` (names compared by hash, `FUN_14018e850`; the constants are built in
//!   `FUN_14011ab70`). The flag is written once when the effect instance is created
//!   (`FUN_14018a040`): 1 when the fighter's state (`+0x12c70`) is in `0xf ..= 0x1a` except `0x14` and `0x17`,
//!   i.e. the aura was (re)started while the fighter was in a damage / knocked-down state.
//!   Wisps already alive are not touched. Which of OUR actions those states are is UNCONFIRMED
//!   (`aura_fx.rs` uses Hit / Knocked / Down).

use super::*;

/// Rope nodes (`FUN_1401a0330(obj, 9)`).
pub(super) const NODES: usize = 9;
/// The shared pool (`FUN_14011acd0`: 0x50 slots, all fighters).
pub(super) const POOL: usize = 80;
/// `DAT_1402f7d90`.
const LAG: f32 = 0.9;
/// Spawn tries per piece per effect frame before giving up (the program stops at the first
/// failure; this only bounds a run of successes).
const MAX_SPAWNS_PER_FRAME: usize = 4;

/// `w_i` of node `i`: `0.9 x sin(i x 0x400 / 0x8000 x 2 pi)` (step `0x2000 / (n - 1)` = 0x400).
pub(super) fn node_lag(i: usize) -> f32 {
    let step = 0x2000 / (NODES as i32 - 1);
    let w = (i as i32 * step) as f32 / 32768.0 * std::f32::consts::TAU;
    (w.sin() * LAG).min(1.0)
}

/// A wisp's rope (`pdEftBone1`): world positions of the 9 nodes and each relative to the
/// anchor at the last update.
#[derive(Clone, Debug)]
pub(super) struct Rope {
    cur: [Vec3; NODES],
    pub rel: [Vec3; NODES],
    fresh: bool,
}

impl Rope {
    pub fn new() -> Self {
        Rope { cur: [Vec3::ZERO; NODES], rel: [Vec3::ZERO; NODES], fresh: true }
    }

    /// One effect frame (`FUN_1401a03c0`): `anchor` = the bone's world position.
    pub fn update(&mut self, anchor: Vec3) {
        for i in 0..NODES {
            let target = anchor + Vec3::Y * i as f32;
            if self.fresh {
                self.cur[i] = target;
            }
            self.cur[i] += (target - self.cur[i]) * (1.0 - node_lag(i));
            self.rel[i] = self.cur[i] - anchor;
        }
        self.fresh = false;
    }
}

/// The axes of one rope segment with direction `d` (`FUN_14011b450`), in our right-handed space:
/// (side, along, depth) such that a bind vertex `(x, y, z)` goes to `x side + y along + z depth`.
/// The program's left-handed rows are `(f.y, -f.x, 0) / n`, `f`, `(f.x f.z, f.y f.z, -n^2) / n`
/// (`n` = `|f.xy|`); mirrored through z they keep the same form (our bind z is already negated).
/// A zero segment is the identity; a segment straight along z has `n = 0`, where the program writes
/// X, `f`, Y (so: X, +-Z, -Y here).
pub(super) fn segment_basis(d: Vec3) -> [Vec3; 3] {
    if d == Vec3::ZERO {
        return [Vec3::X, Vec3::Y, Vec3::Z];
    }
    if d.x == 0.0 && d.y == 0.0 {
        return [Vec3::X, Vec3::new(0.0, 0.0, d.z.signum()), -Vec3::Y];
    }
    let f = d.normalize();
    let n = (f.x * f.x + f.y * f.y).sqrt();
    [Vec3::new(f.y, -f.x, 0.0) / n, f, Vec3::new(f.x * f.z, f.y * f.z, -n * n) / n]
}

/// Twist a basis about its `along` axis by `roll` radians (`FUN_1402906e0` mixes the other two rows).
fn twisted(b: [Vec3; 3], roll: f32) -> [Vec3; 3] {
    let (s, c) = roll.sin_cos();
    [b[0] * c + b[2] * s, b[1], b[2] * c - b[0] * s]
}

/// The exported wisp mesh (`pd_formats::effect::SkinMesh`).
#[derive(Debug)]
pub(super) struct WispSkin {
    pub positions: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub slots: Vec<[u8; 2]>,
    pub weights: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl WispSkin {
    pub fn parse(v: &serde_json::Value) -> Option<WispSkin> {
        let nums = |k: &str, w: usize| -> Option<Vec<Vec<f64>>> {
            let a = v[k].as_array()?;
            a.iter().map(|r| r.as_array().filter(|r| r.len() == w).map(|r| r.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())).collect()
        };
        let positions: Vec<[f32; 3]> = nums("positions", 3)?.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
        let uv: Vec<[f32; 2]> = nums("uv", 2)?.iter().map(|p| [p[0] as f32, p[1] as f32]).collect();
        let slots: Vec<[u8; 2]> = nums("slots", 2)?.iter().map(|p| [p[0] as u8, p[1] as u8]).collect();
        let weights: Vec<[f32; 2]> = nums("weights", 2)?.iter().map(|p| [p[0] as f32, p[1] as f32]).collect();
        let indices: Vec<u32> = v["indices"].as_array()?.iter().filter_map(|i| i.as_u64()).map(|i| i as u32).collect();
        let n = positions.len();
        (n > 0 && uv.len() == n && slots.len() == n && weights.len() == n && indices.iter().all(|&i| (i as usize) < n)).then_some(WispSkin { positions, uv, slots, weights, indices })
    }

    /// The vertices posed on `rope`, relative to the anchor, scaled by `scale` (the draw's world matrix
    /// scales x / y / z of the posed position) and then moved to `anchor`.
    pub fn pose(&self, rope: &Rope, roll: f32, scale: Vec3, anchor: Vec3) -> Vec<[f32; 3]> {
        let mut out = Vec::new();
        self.pose_into(rope, roll, scale, anchor, &mut out);
        out
    }

    /// `pose`, written into `out` (emptied first).
    pub fn pose_into(&self, rope: &Rope, roll: f32, scale: Vec3, anchor: Vec3, out: &mut Vec<[f32; 3]>) {
        out.clear();
        let mut bones = [([Vec3::ZERO; 3], Vec3::ZERO); NODES - 1];
        for (k, b) in bones.iter_mut().enumerate() {
            *b = (twisted(segment_basis(rope.rel[k + 1] - rope.rel[k]), roll), rope.rel[k]);
        }
        let posed = self
            .positions
            .iter()
            .zip(&self.slots)
            .zip(&self.weights)
            .map(|((p, slots), w)| {
                let p = Vec3::from(*p);
                let mut acc = Vec3::ZERO;
                let mut used = false;
                for j in 0..2 {
                    let Some((axes, t)) = bones.get(slots[j] as usize) else { continue };
                    acc += (axes[0] * p.x + axes[1] * p.y + axes[2] * p.z + *t) * w[j];
                    used = true;
                }
                if !used {
                    acc = p;
                }
                (anchor + acc * scale).to_array()
            });
        out.extend(posed);
    }

    pub(super) fn mesh(&self) -> Mesh {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology};
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; self.positions.len()])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv.clone())
            .with_inserted_indices(Indices::U32(self.indices.clone()))
    }
}

/// One wisp slot of a spawner piece.
struct Slot {
    active: bool,
    /// The whole effect frame it started on, its life in frames, its random scale axes and roll.
    spawned: i64,
    life: f32,
    grow: Vec3,
    roll: f32,
    rope: Rope,
    /// Its body: root entity, mesh, material (made when the slot first goes live) and its row in the
    /// shared per-instance buffer (`inst.rs`), if it got one.
    body: Option<(Entity, Handle<Mesh>, Handle<FxMaterial>, Option<u32>)>,
}

/// On a `PDEFTAURA01` piece node (the node itself stays hidden): its wisps.
#[derive(Component)]
pub struct FxWispPool {
    seed: u32,
    /// The last whole effect frame whose ageing / spawn tries ran.
    last: i64,
    count: u32,
    slots: Vec<Slot>,
}

impl FxWispPool {
    pub(super) fn new(seed: u32, frame: i64) -> Self {
        FxWispPool { seed, last: frame, count: 0, slots: Vec::new() }
    }
}

/// A wisp's mesh, a world-space draw rebuilt every frame.
#[derive(Component)]
pub struct FxWispBody {
    instance: Entity,
}

/// On an effect root: the instance's hand-suppress flag (see the module doc).
#[derive(Component)]
pub struct FxHandSuppress;

/// Whether a joint name is one `FUN_14011acd0` suppresses.
fn is_hand(joint: Option<&String>) -> bool {
    matches!(joint.map(|s| s.as_str()), Some("RHAND" | "LHAND"))
}

/// Run the spawn tries and ageing for every whole effect frame up to `whole`, like the program's
/// per-piece update does (`FUN_14011acd0`): a live wisp ages, one past its life is freed and its rope
/// follows the bone; free slots try to start wisps while the shared pool has room. `live` is the
/// count over every pool.
#[allow(clippy::too_many_arguments)]
fn step_pool(pool: &mut FxWispPool, spec: &WispSpec, base_grow: Vec3, whole: i64, anchor: Vec3, suppress: bool, live: &mut usize) {
    if whole - pool.last > 90 {
        pool.last = whole - 90;
    }
    while pool.last < whole {
        pool.last += 1;
        let now = pool.last;
        for s in pool.slots.iter_mut().filter(|s| s.active) {
            if ((now - s.spawned) as f32 + 1.0) > s.life {
                s.active = false;
                *live = live.saturating_sub(1);
            } else {
                s.rope.update(anchor);
            }
        }
        if suppress {
            continue;
        }
        for _ in 0..MAX_SPAWNS_PER_FRAME {
            if *live >= POOL {
                break;
            }
            pool.count = pool.count.wrapping_add(1);
            let (seed, count) = (pool.seed, pool.count);
            let r = |k: u32| hash01(seed, count.wrapping_mul(8).wrapping_add(k));
            if r(0) >= spec.chance() {
                break;
            }
            let idx = match pool.slots.iter().position(|s| !s.active) {
                Some(i) => i,
                None => {
                    pool.slots.push(Slot { active: false, spawned: 0, life: 0.0, grow: Vec3::ZERO, roll: 0.0, rope: Rope::new(), body: None });
                    pool.slots.len() - 1
                }
            };
            let s = &mut pool.slots[idx];
            s.active = true;
            s.spawned = now;
            s.life = spec.life[0] + r(1) * spec.life[1];
            // Each grow axis x (0.5 + 0.5 r); a roll of up to word 25 degrees.
            s.grow = Vec3::new(base_grow.x * (0.5 + 0.5 * r(2)), base_grow.y * (0.5 + 0.5 * r(3)), base_grow.z * (0.5 + 0.5 * r(4)));
            s.roll = (r(5) * spec.roll).to_radians();
            s.rope = Rope::new();
            s.rope.update(anchor);
            *live += 1;
        }
    }
}

/// Step every spawner piece and pose / colour its wisps (after transform propagation: the bone's world
/// position is the anchor).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update(
    mut commands: Commands,
    instances: Query<(&FxInstance, Option<&FxTint>, Has<FxHandSuppress>)>,
    globals: Query<&GlobalTransform>,
    mut pools: Query<(Entity, &FxPiece, &mut FxWispPool), With<FxBound>>,
    mut visibilities: Query<&mut Visibility, With<FxWispBody>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<FxMaterial>>,
    mut posed: Local<Vec<[f32; 3]>>,
    mut rows: Option<ResMut<super::inst::FxInstances>>,
    mut cache: Option<ResMut<FxMatCache>>,
) {
    let step = fx_step();
    let mut live: usize = pools.iter().map(|(_, _, p)| p.slots.iter().filter(|s| s.active).count()).sum();
    for (node, fx, mut pool) in &mut pools {
        let Ok((inst, tint, suppress)) = instances.get(fx.instance) else { continue };
        let p = &fx.piece;
        let (Some(spec), Some(skin)) = (p.wisp, p.skin.as_ref()) else { continue };
        let Ok(g) = globals.get(node) else { continue };
        let anchor = g.translation();
        // The joint's own scale (mode 6: position and scale, world axes).
        let m = g.to_matrix();
        let joint_scale = Vec3::new(m.x_axis.truncate().length(), m.y_axis.truncate().length(), m.z_axis.truncate().length());
        if inst.fixed.is_some() || !anchor.is_finite() {
            continue;
        }
        let age = effect_frame(inst.age, step);
        if env_once("PD_WISP_LOG").is_some() && (age as i64) % 30 == 0 {
            eprintln!("[wisp] age {age:.1} joint {:?} anchor {anchor:?} joint scale {joint_scale:?} live {live}", p.joint);
        }
        step_pool(&mut pool, &spec, p.grow, age.floor() as i64, anchor, suppress && is_hand(p.joint.as_ref()), &mut live);
        for s in pool.slots.iter_mut() {
            let wf = age - s.spawned as f32 + 1.0;
            let show = s.active && in_window(wf, p.start, p.end);
            if show && s.body.is_none() {
                let Some(base) = mats.get(&fx.material).map(|m| m.clone()) else { continue };
                let mesh = meshes.add(skin.mesh());
                let e = commands
                    .spawn((Mesh3d(mesh.clone()), Transform::IDENTITY, Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling, FxWispBody { instance: fx.instance }))
                    .id();
                // Overnight perf 2026-10-09: a row in the shared per-instance buffer holds what this
                // body animates every frame (colour, alpha, scroll, view alpha), and the body draws
                // with the shared neutral material of its static look, so a live aura no longer
                // re-prepares one material per wisp per frame. No free row (or `PD_FX_INST=0`,
                // `PD_FX_INST_SKIP=wisp`): its own material, as before.
                let skipped = env_once("PD_FX_INST_SKIP").is_some_and(|v| v.split(',').any(|k| k.trim() == "wisp"));
                let row = rows.as_deref_mut().filter(|_| super::inst::enabled() && !skipped).and_then(|r| r.take(e, super::inst::FxInst::of(&base)));
                let material = match row {
                    Some(_) => super::mat_cache::shared(cache.as_deref_mut(), &mut mats, super::inst::neutral(base)),
                    None => mats.add(base),
                };
                commands.entity(e).insert(MeshMaterial3d(material.clone()));
                super::inst::tag(&mut commands, e, e, row);
                s.body = Some((e, mesh, material, row));
            }
            let Some((body, mesh, material, row)) = &s.body else { continue };
            if let Ok(mut v) = visibilities.get_mut(*body) {
                let want = if show { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
            if !show {
                continue;
            }
            let (e, d) = (wf - p.start, p.end - p.start + 1.0);
            let [c_move, c_scale, _, _] = p.curves.map(|kind| curve(kind, e, d));
            if !mesh_reuse::enabled() {
                if let Some(mut mm) = meshes.get_mut(mesh) {
                    mm.insert_attribute(Mesh::ATTRIBUTE_POSITION, skin.pose(&s.rope, s.roll, s.grow * c_scale * joint_scale, anchor));
                }
            } else {
                // The same positions written in place, and not at all when the wisp did not move
                // (a write flags the mesh for a new upload; `mesh_reuse.rs`).
                skin.pose_into(&s.rope, s.roll, s.grow * c_scale * joint_scale, anchor, &mut posed);
                if let Some(mut mm) = meshes.get_mut(mesh) {
                    let same = matches!(mm.attribute(Mesh::ATTRIBUTE_POSITION), Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) if *v == *posed);
                    if !same {
                        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) = mm.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
                            v.clear();
                            v.extend_from_slice(&posed);
                        } else {
                            mm.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.clone());
                        }
                    }
                }
            }
            let mut c = piece_colour(p, wf);
            if let Some(tt) = tint {
                c = Vec4::new(c.x * tt.rgb.x, c.y * tt.rgb.y, c.z * tt.rgb.z, c.w);
            }
            // t0 scrolls by (word 4, word 5) and t1 by (word 6, word 7), x the word-8 curve.
            let (uv0, uv1) = (Vec4::new(1.0, 1.0, c_move * p.travel.x, c_move * p.travel.y), Vec4::new(1.0, 1.0, c_move * p.travel.z, c_move * p.roll));
            let col = Color::srgba(c.x, c.y, c.z, c.w);
            let quad = tint.map(|tt| Vec4::from(tt.view_alpha));
            if let (Some(r), Some(rows)) = (*row, rows.as_deref_mut()) {
                // The same values the material path below writes, into the body's row.
                let mut v = *rows.row_mut(r);
                let before = v;
                if let Some(q) = quad {
                    v.quad = q;
                }
                v.piece.w = c.w;
                v.uv0 = uv0;
                v.uv1 = uv1;
                let l = LinearRgba::from(col);
                v.color = Vec4::new(l.red, l.green, l.blue, l.alpha);
                if v != before {
                    *rows.row_mut(r) = v;
                    rows.mark();
                }
            } else if let Some(mut ext) = mats.get_mut(material) {
                // Write only what changed: any write flags the material for re-preparation.
                let x = &ext.extension;
                let differs = quad.is_some_and(|q| x.quad != q) || x.piece.w != c.w || x.uv0 != uv0 || x.uv1 != uv1 || ext.base.base_color != col;
                if differs {
                    if let Some(q) = quad {
                        ext.extension.quad = q;
                    }
                    ext.extension.piece.w = c.w;
                    ext.extension.uv0 = uv0;
                    ext.extension.uv1 = uv1;
                    ext.base.base_color = col;
                }
            }
        }
    }
}

/// Bodies of an effect that is gone.
pub(super) fn cleanup(mut commands: Commands, instances: Query<(), With<FxInstance>>, bodies: Query<(Entity, &FxWispBody)>) {
    for (e, b) in &bodies {
        if !instances.contains(b.instance) {
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lag_runs_from_a_stuck_node_to_ninety_percent() {
        assert_eq!(node_lag(0), 0.0);
        assert!((node_lag(8) - 0.9).abs() < 1e-6);
        assert!((1..NODES).all(|i| node_lag(i) > node_lag(i - 1)));
    }

    #[test]
    fn a_resting_rope_hangs_straight_up_and_a_moving_anchor_drags_the_top_behind() {
        let mut r = Rope::new();
        r.update(Vec3::new(5.0, 3.0, 2.0));
        for i in 0..NODES {
            assert!((r.rel[i] - Vec3::Y * i as f32).length() < 1e-5, "node {i}");
        }
        // The anchor runs 1 u per frame along x for 12 frames: the top lags well behind, node 0 never does.
        let mut a = Vec3::new(5.0, 3.0, 2.0);
        for _ in 0..12 {
            a.x += 1.0;
            r.update(a);
        }
        assert!(r.rel[0].length() < 1e-5);
        assert!(r.rel[8].x < -4.0, "the tip trails: {:?}", r.rel[8]);
        assert!((r.rel[8].y - 8.0).abs() < 1e-4, "the height is kept while the anchor stays level");
        assert!((1..NODES).all(|i| r.rel[i].x <= r.rel[i - 1].x + 1e-5), "monotone trail");
    }

    #[test]
    fn a_segment_basis_is_orthonormal_and_vertical_is_x_y_minus_z() {
        let [s, f, u] = segment_basis(Vec3::Y);
        assert!((s - Vec3::X).length() < 1e-6 && (f - Vec3::Y).length() < 1e-6 && (u + Vec3::Z).length() < 1e-6);
        for d in [Vec3::new(1.0, 2.0, 0.5), Vec3::new(-0.3, 1.0, -2.0), Vec3::new(2.0, 0.1, 0.0), Vec3::new(0.0, 0.0, 3.0)] {
            let [s, f, u] = segment_basis(d);
            assert!((s.length() - 1.0).abs() < 1e-5 && (f.length() - 1.0).abs() < 1e-5 && (u.length() - 1.0).abs() < 1e-5);
            assert!(s.dot(f).abs() < 1e-5 && f.dot(u).abs() < 1e-5 && s.dot(u).abs() < 1e-5, "{d:?}");
            assert!(f.dot(d) > 0.0, "points along the segment");
        }
        assert_eq!(segment_basis(Vec3::ZERO), [Vec3::X, Vec3::Y, Vec3::Z]);
    }

    #[test]
    fn posing_into_a_used_buffer_gives_the_fresh_pose() {
        let skin = WispSkin { positions: vec![[0.5, 0.0, 0.2], [0.0, 1.0, 0.0], [-0.3, 0.4, 0.1]], uv: vec![[0.0; 2]; 3], slots: vec![[0, 1], [3, 255], [6, 2]], weights: vec![[0.7, 0.3], [1.0, 0.0], [0.5, 0.5]], indices: vec![0, 1, 2] };
        let mut r = Rope::new();
        let mut a = Vec3::new(1.0, 2.0, 3.0);
        r.update(a);
        let mut buf = vec![[9.0; 3]; 7];
        for k in 0..5 {
            a += Vec3::new(0.7, 0.1 * k as f32, -0.4);
            r.update(a);
            skin.pose_into(&r, 0.3 * k as f32, Vec3::new(1.0, 1.5, 0.5), a, &mut buf);
            assert_eq!(buf, skin.pose(&r, 0.3 * k as f32, Vec3::new(1.0, 1.5, 0.5), a));
            // The mesh written in place holds what the old `insert_attribute` put there.
            let mut old = skin.mesh();
            old.insert_attribute(Mesh::ATTRIBUTE_POSITION, skin.pose(&r, 0.3 * k as f32, Vec3::new(1.0, 1.5, 0.5), a));
            let mut new = skin.mesh();
            if let Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) = new.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
                v.clear();
                v.extend_from_slice(&buf);
            }
            assert_eq!(format!("{:?}", old.attribute(Mesh::ATTRIBUTE_POSITION)), format!("{:?}", new.attribute(Mesh::ATTRIBUTE_POSITION)));
        }
    }


    #[test]
    fn a_straight_rope_stretches_the_mesh_up_the_bones_and_the_scale_has_no_plus_one() {
        // One vertex per bone, bound only to it, at the bind origin: it lands on its node.
        let skin = WispSkin {
            positions: vec![[0.0; 3]; 3],
            uv: vec![[0.0; 2]; 3],
            slots: vec![[0, 255], [3, 255], [6, 255]],
            weights: vec![[1.0, 0.0]; 3],
            indices: vec![0, 1, 2],
        };
        let mut r = Rope::new();
        let anchor = Vec3::new(1.0, 2.0, 3.0);
        r.update(anchor);
        let out = skin.pose(&r, 0.0, Vec3::splat(2.0), anchor);
        for (v, k) in out.iter().zip([0.0, 3.0, 6.0]) {
            assert!((Vec3::from(*v) - (anchor + Vec3::Y * k * 2.0)).length() < 1e-5, "{v:?}");
        }
        // k = 0 (the wisp's first instant): everything collapses on the anchor.
        let out = skin.pose(&r, 0.0, Vec3::ZERO, anchor);
        assert!(out.iter().all(|v| (Vec3::from(*v) - anchor).length() < 1e-6));
    }

    #[test]
    fn a_pool_spawns_about_one_wisp_per_rate_frames_and_holds_several_at_once() {
        let spec = WispSpec { rate: 35, life: [25.0, 5.0], roll: 360.0 };
        let mut pool = FxWispPool::new(11, 0);
        let mut live = 0usize;
        let (mut max_live, mut spawned) = (0usize, 0usize);
        for f in 1..=6000i64 {
            let before = pool.slots.iter().filter(|s| s.active && s.spawned == f).count();
            let _ = before;
            step_pool(&mut pool, &spec, Vec3::splat(3.0), f, Vec3::ZERO, false, &mut live);
            spawned += pool.slots.iter().filter(|s| s.active && s.spawned == f).count();
            max_live = max_live.max(pool.slots.iter().filter(|s| s.active).count());
            assert_eq!(live, pool.slots.iter().filter(|s| s.active).count(), "the live count follows the slots");
        }
        // chance ~ 1 / 35 per try; wisps live ~27 frames: about 0.77 alive on average, sometimes 2-3.
        assert!((130..230).contains(&spawned), "{spawned} wisps in 6000 frames");
        assert!(max_live >= 2, "a piece can hold several wisps at once, max {max_live}");
        // The hand-suppress flag stops new wisps.
        let mut pool = FxWispPool::new(11, 0);
        let mut live = 0usize;
        step_pool(&mut pool, &spec, Vec3::splat(3.0), 3000, Vec3::ZERO, true, &mut live);
        assert!(pool.slots.is_empty() && live == 0);
    }

    #[test]
    fn the_shared_pool_is_capped_at_eighty() {
        let spec = WispSpec { rate: 2, life: [30.0, 0.0], roll: 0.0 };
        let mut live = 0usize;
        let mut pools: Vec<FxWispPool> = (0..120).map(|i| FxWispPool::new(i, 0)).collect();
        for f in 1..=60i64 {
            for p in pools.iter_mut() {
                step_pool(p, &spec, Vec3::ONE, f, Vec3::ZERO, false, &mut live);
            }
            assert!(live <= POOL, "{live}");
        }
        assert_eq!(live, POOL, "a busy aura fills the pool");
    }
}
