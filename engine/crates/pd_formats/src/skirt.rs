//! Procedural cloth rig for long coats and skirts.
//!
//! In the game data a long coat's lower half is skinned straight to the leg
//! bones, so it moves like trousers. To let it swing we pick out the coat
//! vertices, hang a ring of short bone chains from the hips down to the hem,
//! and move the coat vertices onto the chains (blended with the hips near
//! the waist). The chains are named as cloth (`c9j<N>`) so the game's cloth
//! physics swings them and body capsules push them around the legs.
//!
//! Which vertices are coat is decided by **texture area**: coat and trousers
//! are painted in separate regions of the character's texture, so a vertex
//! is coat when it is skinned to a leg bone *and* its UV lies in the coat
//! region. Distance-from-bone and height rules were tried first and failed:
//! flared trousers sit as far from the bone as the coat, and the coat's hem
//! strips overlap the trousers in height (see engine/docs/GAME.md).
//!
//! All positions here are right-handed bind-pose model space.

use glam::Vec3;

pub const CHAINS: usize = 8;
pub const SEGMENTS: usize = 3;
/// Cloth part number used in joint names for the generated rig.
pub const PART: u32 = 9;
/// Fewer coat vertices than this: not a coat, don't rig.
const MIN_VERTS: usize = 30;

/// A character that gets a coat rig, and where its coat is painted in its
/// texture: UV rectangles `[u_min, v_min, u_max, v_max]` (v down, 0..1).
/// Find the rectangles with `pd_convert uvmap <alr> <out.png> <leg joints>`,
/// which plots leg-skinned vertices on the texture. Opt-in per character;
/// add one only after checking it visually.
pub struct CoatRig {
    pub id: &'static str,
    pub uv_rects: &'static [[f32; 4]],
}

/// PARKED (2026-09-26): no character is rigged; coats move with the legs as in
/// the game data. See engine/docs/HANDOFF.md "Parked: long-coat cloth" for the
/// agreed next approach (hybrid). Known-good pc01a region, for when it resumes:
/// `CoatRig { id: "pc01a", uv_rects: &[[0.40, 0.55, 1.0, 1.0]] }` — trousers
/// are the red block at the top (v < 0.36); coat + hem strips the grey block.
pub const COAT_RIGS: &[CoatRig] = &[];

pub fn coat_rig(character_id: &str) -> Option<&'static CoatRig> {
    COAT_RIGS.iter().find(|r| r.id.eq_ignore_ascii_case(character_id))
}

/// One body vertex as the exporter resolves it.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// Dominant body joint.
    pub joint: u16,
    pub pos: Vec3,
    pub uv: [f32; 2],
}

pub struct SkirtPlan {
    uv_rects: &'static [[f32; 4]],
    /// Body joint the chains hang from (hips).
    pub anchor: usize,
    /// Joint positions per chain, top to bottom (SEGMENTS + 1 each).
    pub chains: Vec<Vec<Vec3>>,
    /// Skin-owning joints of each leg segment.
    legs: Vec<Vec<usize>>,
    center: Vec3,
    top: f32,
    bottom: f32,
}

impl SkirtPlan {
    /// `bind`: body joint positions; `parent`/`children`: body hierarchy.
    pub fn build(bind: &[Vec3], parent: &[i16], children: &[Vec<usize>], samples: &[Sample], rig: &CoatRig) -> Option<Self> {
        let n = bind.len();
        let height = samples.iter().map(|s| s.pos.y).fold(f32::MIN, f32::max);
        if height <= 0.0 {
            return None;
        }
        let mut owned = vec![0usize; n];
        for s in samples {
            if (s.joint as usize) < n {
                owned[s.joint as usize] += 1;
            }
        }
        // Skin owned by each joint's whole subtree: the child carrying the
        // most skin is the real continuation of a limb. (The farthest child
        // is wrong: helper joints often sit at the model origin/floor, which
        // once made the spine look like a leg and rigged the collar.)
        let mut subtree = vec![0usize; n];
        fn fill(j: usize, children: &[Vec<usize>], owned: &[usize], out: &mut [usize]) -> usize {
            let mut s = owned[j];
            for &c in &children[j] {
                if c < out.len() {
                    s += fill(c, children, owned, out);
                }
            }
            out[j] = s;
            s
        }
        for j in 0..n {
            if parent[j] < 0 {
                fill(j, children, &owned, &mut subtree);
            }
        }
        // Leg bones: long, pointing down, below chest height, with real skin
        // (counting childless helper joints hanging off the bone, which often
        // own the skin instead of the bone itself).
        let mut legs = Vec::new();
        let mut hip_y = f32::MIN;
        let mut top_leg = None;
        for j in 0..n {
            let Some(c) = children[j].iter().copied().filter(|&c| c < n && subtree[c] > 0).max_by_key(|&c| subtree[c]) else { continue };
            let owners: Vec<usize> = std::iter::once(j).chain(children[j].iter().copied().filter(|&h| h < n && h != c && children[h].is_empty())).collect();
            let skin: usize = owners.iter().map(|&o| owned[o]).sum();
            if bind[j].y - bind[c].y > 0.15 * height && bind[j].y < 0.7 * height && skin >= 20 {
                if bind[j].y > hip_y {
                    hip_y = bind[j].y;
                    top_leg = Some(j);
                }
                legs.push(owners);
            }
        }
        // The hips: parent of the highest leg bone.
        let anchor = usize::try_from(parent[top_leg?]).ok().filter(|&p| p < n)?;
        let mut plan = SkirtPlan { uv_rects: rig.uv_rects, anchor, chains: Vec::new(), legs, center: bind[anchor], top: 0.0, bottom: 0.0 };
        let coat: Vec<Vec3> = samples.iter().filter(|s| plan.is_coat(s.joint as usize, s.uv)).map(|s| s.pos).collect();
        if std::env::var_os("PD_DEBUG_SKIRT").is_some() {
            let ys = coat.iter().map(|p| p.y);
            eprintln!("skirt: {} legs, anchor {anchor}, {} coat verts, y {:.2}..{:.2}", plan.legs.len(), coat.len(), ys.clone().fold(f32::MAX, f32::min), ys.fold(f32::MIN, f32::max));
        }
        if std::env::var_os("PD_DEBUG_SKIRT").is_some() {
            // Coat-area vertices below the hips that are NOT picked (wrong joint?).
            let mut by_joint = std::collections::BTreeMap::<u16, (usize, f32, f32)>::new();
            for s in samples {
                let (u, v) = (s.uv[0].rem_euclid(1.0), s.uv[1].rem_euclid(1.0));
                let in_rect = rig.uv_rects.iter().any(|r| u >= r[0] && v >= r[1] && u <= r[2] && v <= r[3]);
                if in_rect && s.pos.y < bind[anchor].y && !plan.is_coat(s.joint as usize, s.uv) {
                    let e = by_joint.entry(s.joint).or_insert((0, f32::MAX, f32::MIN));
                    e.0 += 1;
                    e.1 = e.1.min(s.pos.y);
                    e.2 = e.2.max(s.pos.y);
                }
            }
            eprintln!("skirt: coat-area verts not picked, by joint (count, y range): {by_joint:?}");
        }
        if coat.len() < MIN_VERTS {
            return None;
        }
        plan.top = coat.iter().map(|p| p.y).fold(f32::MIN, f32::max).min(bind[anchor].y);
        plan.bottom = coat.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        let c = plan.center;
        let near_top: Vec<f32> = coat.iter().filter(|p| p.y > plan.top - 0.25 * (plan.top - plan.bottom)).map(|p| Vec3::new(p.x - c.x, 0.0, p.z - c.z).length()).collect();
        let radius = median(near_top).max(1.0);
        for k in 0..CHAINS {
            let a = k as f32 / CHAINS as f32 * std::f32::consts::TAU;
            let (x, z) = (c.x + a.sin() * radius, c.z + a.cos() * radius);
            plan.chains.push((0..=SEGMENTS).map(|s| Vec3::new(x, plan.top + (plan.bottom - plan.top) * s as f32 / SEGMENTS as f32, z)).collect());
        }
        Some(plan)
    }

    fn is_coat(&self, dom: usize, uv: [f32; 2]) -> bool {
        let (u, v) = (uv[0].rem_euclid(1.0), uv[1].rem_euclid(1.0));
        // Coat skin belongs to the legs or to the hips themselves (pc01a:
        // 33 coat vertices hang from the hips with a secondary thigh weight,
        // which dragged the fringe back with the rear leg).
        (dom == self.anchor || self.legs.iter().any(|l| l.contains(&dom))) && self.uv_rects.iter().any(|r| u >= r[0] && v >= r[1] && u <= r[2] && v <= r[3])
    }

    /// New skinning for a coat vertex: up to 4 (joint, weight) pairs, where
    /// chain joints are numbered `first_joint + chain * (SEGMENTS + 1) + s`.
    pub fn reskin(&self, dom: usize, p: Vec3, uv: [f32; 2], first_joint: u16) -> Option<[(u16, f32); 4]> {
        if !self.is_coat(dom, uv) {
            return None;
        }
        let c = self.center;
        let ang = (p.x - c.x).atan2(p.z - c.z).rem_euclid(std::f32::consts::TAU);
        let f = ang / std::f32::consts::TAU * CHAINS as f32;
        let (ca, cb) = (f.floor() as usize % CHAINS, (f.floor() as usize + 1) % CHAINS);
        let wb = f.fract();
        let t = ((self.top - p.y) / (self.top - self.bottom).max(1e-3)).clamp(0.0, 1.0);
        let s = ((t * SEGMENTS as f32) as usize).min(SEGMENTS - 1);
        // Near the waist, follow the hips; lower down, the cloth.
        let hips = (1.0 - t / 0.3).clamp(0.0, 1.0) * 0.7;
        let joint = |chain: usize| first_joint + (chain * (SEGMENTS + 1) + s) as u16;
        Some([(joint(ca), (1.0 - wb) * (1.0 - hips)), (joint(cb), wb * (1.0 - hips)), (self.anchor as u16, hips), (0, 0.0)])
    }
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}
