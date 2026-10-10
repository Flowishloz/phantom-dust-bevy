//! Export a character ALR as a skinned, animated glTF.
//!
//! Skinned (SWBOS) vertices store two blend weights (bytes 18 and 17 of the
//! vertex, a D3DCOLOR read as BGRA) and two shader register numbers (u16 at
//! 20 and 22). Register 120 + 3k is slot k of the draw's bone palette.

use crate::alr::{self, palette_joint, read_skin_vertices, Alr, Model, SkinVertex as SkinVert, VERTFMT_CHAR_SKIN as VERTFMT_SWBOS};
use crate::anim::{self, ANIM_NAMES};
use crate::gltf::{to_rh, write_glb, Builder, ExportStats, Materials};
use crate::pose::{self, Local, Order};
use crate::skirt;
use anyhow::{bail, Context, Result};
use glam::Vec3;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Name prefix of the authored shadow-hull nodes in a character glb
/// (`shadowhull_d<draw>`, children of their helper joints).
pub const HULL_TAG: &str = "shadowhull";

// The program plays one authored keyframe per 30 Hz logic tick, not 24 fps
// (engine/docs/PROGRAM-RE.md "Animation clock": FUN_140257510 advances the
// clip time by 1.0 per 30 Hz player-update tick; the raw ALR "frames" field
// is exactly that tick count). Confirmed against Laurence's own footage
// (engine/docs/FOOTAGE.md "24-vs-30 fps verdict").
const FPS: f32 = 30.0;

pub struct CharacterOptions {
    pub order: Order,
    /// Animation ids to include (empty = all present).
    pub anims: Vec<usize>,
    /// Extra ALRs whose animation tables share this skeleton (e.g. .ala files).
    pub extra_anim_sources: Vec<(String, Alr)>,
    /// Generate a swinging cloth rig for a long coat: skin farther than this
    /// from the leg bones is coat (see skirt.rs, COAT_RIGS).
    pub coat_rig: Option<&'static skirt::CoatRig>,
}

// SkinVert, read_skin_vertices and palette_joint moved to alr.rs
// (`SkinVertex`/`read_skin_vertices`/`palette_joint`) so `gltf.rs` can reuse
// the same skinned/palette vertex handling for effect pieces that use the
// same layout under a different format id (`VERTFMT_EFFECT_SKIN`,
// docs/FORMATS-EFFECTS.md).

/// Skinned vertices are stored relative to the draw's transform joint (its
/// bind world matrix, left-handed); for most draws that joint is a group node
/// at the origin (identity), but e.g. pc01a's chest badge (draw on joint 15 at
/// (0, 15.26, -2.11)) is modelled around the origin and only lands on the chest
/// once placed. Without this the badge floated at the character's feet.
fn draw_placement(world: &[glam::Mat4], draw: &alr::IndexBuffer) -> glam::Mat4 {
    world.get(draw.transform_idx as usize).copied().unwrap_or(glam::Mat4::IDENTITY)
}

/// (dominant joint, bind position RH) for every vertex of the body's skinned
/// draws, resolved exactly as the export does.
fn body_samples(a: &Alr, model: &Model, order: Order) -> Result<Vec<skirt::Sample>> {
    let mut out = Vec::new();
    let world = pose::world_matrices(model, &pose::bind_locals(model), order);
    let mut cache: HashMap<u16, Vec<SkinVert>> = HashMap::new();
    for draw in &model.draws {
        let Some(vb) = model.vertbufs.get(draw.vertex_buf as usize) else { continue };
        if vb.format != VERTFMT_SWBOS || draw.palette.is_empty() {
            continue;
        }
        if !cache.contains_key(&draw.vertex_buf) {
            cache.insert(draw.vertex_buf, read_skin_vertices(a, vb)?);
        }
        let verts = &cache[&draw.vertex_buf];
        let place = draw_placement(&world, draw);
        let used: std::collections::BTreeSet<u16> = draw.indices.iter().copied().filter(|&i| (i as usize) < verts.len()).collect();
        for vi in used {
            let v = &verts[vi as usize];
            let mut best = (draw.transform_idx, 0.0f32);
            for k in 0..2 {
                if let Some(j) = palette_joint(&draw.palette, v.regs[k]) {
                    if v.weights[k] > best.1 {
                        best = (j, v.weights[k]);
                    }
                }
            }
            let vp = place.transform_point3(Vec3::from(v.pos));
            out.push(skirt::Sample { joint: best.0, pos: Vec3::new(vp.x, vp.y, -vp.z), uv: v.uv });
        }
    }
    Ok(out)
}

/// Rough collision shape of the body for cloth: for every body joint that
/// owns skin (is the dominant influence of some vertices), a capsule from
/// the joint to its farthest child, with a radius that covers most of those
/// vertices. Written as glTF node extras `{"capsule_child": node, "capsule_radius": r}`
/// (child omitted for a sphere at the joint).
fn add_body_capsules(nodes: &mut [Value], children: &[Vec<usize>], bind_world: &[glam::Mat4], samples: &[(u16, Vec3)]) {
    let n = bind_world.len();
    let pos: Vec<Vec3> = bind_world.iter().map(|m| to_rh(*m).w_axis.truncate()).collect();
    let mut by_joint: Vec<Vec<Vec3>> = vec![Vec::new(); n];
    for &(j, p) in samples {
        if (j as usize) < n {
            by_joint[j as usize].push(p);
        }
    }
    for j in 0..n {
        let verts = &by_joint[j];
        if verts.len() < 8 {
            continue;
        }
        let child = children[j].iter().copied().filter(|&c| c < n).max_by(|&a, &b| pos[a].distance(pos[j]).total_cmp(&pos[b].distance(pos[j])));
        let child = child.filter(|&c| pos[c].distance(pos[j]) > 0.8);
        let (a, b) = (pos[j], child.map(|c| pos[c]).unwrap_or(pos[j]));
        let mut d: Vec<f32> = verts.iter().map(|&p| p.distance(closest_on_segment(p, a, b))).collect();
        d.sort_by(f32::total_cmp);
        // Median: the cloth should be pushed off the body surface, not off
        // loose outer clothing that also belongs to this bone.
        let r = d[d.len() / 2].clamp(0.3, 4.0);
        let mut extras = json!({ "capsule_radius": r });
        if let Some(c) = child {
            extras["capsule_child"] = json!(c);
        }
        nodes[j]["extras"] = extras;
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

/// Length of the longest single-child chain passing through joint `j`
/// (ancestors that have only one child, plus descendants below it).
fn chain_depth(m: &Model, j: usize) -> usize {
    let kids = |p: usize| (0..m.joints.len()).filter(move |&c| m.joints[c].parent == p as i16 && c != p);
    let mut down = 0;
    let mut cur = j;
    loop {
        let k: Vec<usize> = kids(cur).collect();
        match k.as_slice() {
            [only] => {
                down += 1;
                cur = *only;
            }
            [] => break,
            many => {
                // Branch: take the deepest single chain below.
                down += 1 + many.iter().map(|&c| chain_depth_down(m, c)).max().unwrap_or(0);
                break;
            }
        }
    }
    let mut up = 0;
    let mut cur = j;
    while let Some(p) = Some(m.joints[cur].parent).filter(|&p| p >= 0 && (p as usize) < m.joints.len()) {
        if kids(p as usize).count() != 1 {
            break;
        }
        up += 1;
        cur = p as usize;
    }
    up + down + 1
}

fn chain_depth_down(m: &Model, j: usize) -> usize {
    let k: Vec<usize> = (0..m.joints.len()).filter(|&c| m.joints[c].parent == j as i16 && c != j).collect();
    1 + k.iter().map(|&c| chain_depth_down(m, c)).max().unwrap_or(0)
}

/// Joint flag (u16 at +0x24): takes part in the program's cloth / attach
/// system (PROGRAM-RE.md "Character parts / cloth").
const JOINT_CLOTH: u16 = 0x100;

/// One node of a part's cloth grid: a joint named `CL<d><row><col>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClothNode {
    pub row: u32,
    pub col: u32,
    /// Share of the node's velocity kept each 30 Hz tick.
    pub keep: f32,
    /// Most the joint may turn away from its animated rest direction, radians.
    pub limit: f32,
}

/// The program's reading of a cloth joint (FUN_14025e520): flag 0x100 and
/// a name from "CL000" to "CL9FF"; the 4th character is the row (chain),
/// the 5th the column (position along the chain), '0'-'9' then 'A'.. = 10...
/// Parameters at +0x30: velocity kept, angle limit in degrees; when all
/// three floats are zero the program uses 0.9 and 60°.
pub fn cloth_node(j: &alr::Joint) -> Option<ClothNode> {
    const CL000: u32 = 0x52B4_E068;
    if j.flags & JOINT_CLOTH == 0 || j.name.wrapping_sub(CL000) >= 0x92A19 {
        return None;
    }
    let digit = |v: u32| (v % 40).checked_sub(1);
    let (row, col) = (digit(j.name / 1600)?, digit(j.name / 40)?);
    let (keep, limit) = if j.params == [0.0; 3] { (0.9, 60f32.to_radians()) } else { (j.params[0], j.params[1].to_radians()) };
    Some(ClothNode { row, col, keep, limit })
}

/// Body joint a part root attaches to: part root "CLCnn" -> body "CLPnn"
/// (FUN_14025d380 / FUN_14025e520 match them by these names).
fn cloth_host(body: &Model, root: &alr::Joint) -> Option<usize> {
    let name = alr::joint_name(root.name)?;
    let nn = name.strip_prefix("CLC").filter(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()))?;
    let want = format!("CLP{nn}");
    body.joints.iter().position(|b| b.flags & JOINT_CLOTH != 0 && alr::joint_name(b.name).as_deref() == Some(want.as_str()))
}

/// Mirror a left-handed local into right-handed TRS.
fn local_rh(l: &Local, order: Order) -> (Vec3, glam::Quat, Vec3) {
    let (s, r, t) = to_rh(l.matrix(order)).to_scale_rotation_translation();
    (t, r, s)
}

pub fn export_character_glb(a: &Alr, opts: &CharacterOptions) -> Result<(Vec<u8>, ExportStats)> {
    // The body is the first skinned model. Further skinned models are extra
    // parts on their own small skeletons (e.g. pc00a's jacket tails, 5 bone
    // chains) whose root sits exactly on a body joint; they are grafted onto
    // the body skeleton at that joint.
    let skinned: Vec<&Model> = a.models.iter().filter(|m| m.has_skeleton && m.draws.iter().any(|d| !d.palette.is_empty())).collect();
    let Some(&model) = skinned.first() else {
        bail!("no skinned model found");
    };
    let mut b = Builder::default();
    let mut mats = Materials::default();
    let mut stats = ExportStats::default();
    let order = opts.order;
    let n = model.joints.len();
    let body_world = pose::world_matrices(model, &pose::bind_locals(model), order);

    // --- Combined skeleton: body joints first, then each part's joints.
    let mut nodes: Vec<Value> = Vec::new();
    let mut bind_world: Vec<glam::Mat4> = Vec::new();
    let mut children: Vec<Vec<usize>> = Vec::new();
    let mut roots = Vec::new();
    let mut offsets = Vec::new();
    for (pi, part) in skinned.iter().enumerate() {
        let off = nodes.len();
        offsets.push(off);
        let locals = pose::bind_locals(part);
        let world = pose::world_matrices(part, &locals, order);
        children.resize(off + part.joints.len(), Vec::new());
        for (i, j) in part.joints.iter().enumerate() {
            let (t, r, s) = local_rh(&locals[i], order);
            let mut local = (t, r, s);
            if j.parent >= 0 && (j.parent as usize) < part.joints.len() && j.parent as usize != i {
                children[off + j.parent as usize].push(off + i);
            } else if pi == 0 {
                roots.push(off + i);
            } else if let Some(h) = cloth_host(model, j).filter(|_| std::env::var_os("PD_PART_HOST").is_none()) {
                // The program's rule (PROGRAM-RE.md "Character parts /
                // cloth", FUN_14025f610): a part root named CLCnn takes the
                // world matrix of the body joint named CLPnn, then its own
                // local rotation and scale (its local position is ignored).
                children[h].push(off + i);
                let mut l = locals[i];
                l.t = Vec3::ZERO;
                local = local_rh(&l, order);
            } else {
                // No CLC/CLP pair: graft onto the body joint at the same bind position.
                let p = world[i].w_axis.truncate();
                let host = (0..n)
                    .filter(|&h| model.joints[h].name != u32::MAX)
                    .min_by(|&x, &y| body_world[x].w_axis.truncate().distance(p).total_cmp(&body_world[y].w_axis.truncate().distance(p)));
                // PD_PART_HOST=<body joint>: calibration override of the graft joint.
                let forced = std::env::var("PD_PART_HOST").ok().and_then(|v| v.parse::<usize>().ok()).filter(|&h| h < n);
                match forced.or(host.filter(|&h| body_world[h].w_axis.truncate().distance(p) < 0.5)) {
                    Some(h) => {
                        children[h].push(off + i);
                        let rel = to_rh(body_world[h]).inverse() * to_rh(world[i]);
                        let (s2, r2, t2) = rel.to_scale_rotation_translation();
                        local = (t2, r2, s2);
                    }
                    None => roots.push(off + i),
                }
            }
            // Joints on a bone chain at least 3 deep in an extra part are
            // named "c..." (cloth: the game swings these); others "p...".
            let prefix = if pi > 0 && chain_depth(part, i) >= 3 { 'c' } else { 'p' };
            let mut node = json!({ "name": format!("{prefix}{pi}j{i}"), "translation": local.0.to_array(), "rotation": local.1.to_array(), "scale": local.2.to_array() });
            // Cloth grid node (the program's CLrc joints): row, column and
            // per-node parameters for pd_game's dangle.rs.
            if pi > 0 {
                if let Some(c) = cloth_node(j) {
                    node["extras"] = json!({ "cloth_row": c.row, "cloth_col": c.col, "cloth_keep": c.keep, "cloth_limit": c.limit });
                }
            }
            nodes.push(node);
            bind_world.push(world[i]);
        }
    }
    // --- Long coats: a generated cloth rig replaces leg skinning on the coat.
    let body_rh: Vec<Vec3> = body_world.iter().map(|m| to_rh(*m).w_axis.truncate()).collect();
    let body_parents: Vec<i16> = model.joints.iter().map(|j| j.parent).collect();
    let body_children: Vec<Vec<usize>> = (0..n).map(|j| children[j].iter().copied().filter(|&c| c < n).collect()).collect();
    let skirt = match opts.coat_rig {
        Some(t) => skirt::SkirtPlan::build(&body_rh, &body_parents, &body_children, &body_samples(a, model, order)?, t),
        None => None,
    };
    let skirt_first = nodes.len() as u16;
    if let Some(plan) = &skirt {
        let anchor_w = to_rh(body_world[plan.anchor]);
        for (k, chain) in plan.chains.iter().enumerate() {
            for (s, &p) in chain.iter().enumerate() {
                let idx = nodes.len();
                let (t, r, sc) = if s == 0 {
                    let (sc, r, t) = (anchor_w.inverse() * glam::Mat4::from_translation(p)).to_scale_rotation_translation();
                    (t, r, sc)
                } else {
                    (p - chain[s - 1], glam::Quat::IDENTITY, Vec3::ONE)
                };
                let name = format!("c{}j{}", skirt::PART, k * (skirt::SEGMENTS + 1) + s);
                nodes.push(json!({ "name": name, "translation": t.to_array(), "rotation": r.to_array(), "scale": sc.to_array() }));
                children.push(Vec::new());
                let parent = if s == 0 { plan.anchor } else { idx - 1 };
                children[parent].push(idx);
                // bind_world is kept left-handed like the rest.
                bind_world.push(to_rh(glam::Mat4::from_translation(p)));
            }
        }
        stats.coat_chains = plan.chains.len();
    }
    for (i, c) in children.iter().enumerate() {
        if !c.is_empty() {
            nodes[i]["children"] = json!(c);
        }
    }
    let joint_count = nodes.len();

    // --- Inverse bind matrices.
    let ibm: Vec<f32> = bind_world.iter().flat_map(|m| to_rh(*m).inverse().to_cols_array()).collect();
    let ibm_acc = b.push_f32(&ibm, "MAT4", None, false);

    // --- Skinned primitives: one per draw, with vertices remapped to joints.
    let mut primitives = Vec::new();
    let flip_winding = std::env::var_os("PD_CHAR_FLIP_WINDING").is_some();
    // (dominant body joint, bind position RH) per body vertex, for collision capsules.
    // The character shader's specular colour bytes (material flag 0x04,
    // entry +0x34..+0x37 = B, G, R, power); written to `asset.extras`.
    let mut char_spec: Option<[u8; 4]> = None;
    let mut skin_samples: Vec<(u16, Vec3)> = Vec::new();
    for (pi, part) in skinned.iter().enumerate() {
    let off = offsets[pi] as u16;
    let model = *part;
    let part_world = pose::world_matrices(model, &pose::bind_locals(model), order);
    let mut vcache: HashMap<u16, Vec<SkinVert>> = HashMap::new();
    // PD_SKIP_DRAWS="a-b": debug, leave out body draws a..=b (by draw index).
    let skip: Option<(usize, usize)> = std::env::var("PD_SKIP_DRAWS").ok().and_then(|s| {
        let (a, b) = s.split_once('-')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    });
    for (di, draw) in model.draws.iter().enumerate() {
        if pi == 0 && skip.is_some_and(|(a, b)| (a..=b).contains(&di)) {
            continue;
        }
        let Some(vb) = model.vertbufs.get(draw.vertex_buf as usize) else { continue };
        if vb.format != VERTFMT_SWBOS || draw.palette.is_empty() {
            continue;
        }
        if !vcache.contains_key(&draw.vertex_buf) {
            vcache.insert(draw.vertex_buf, read_skin_vertices(a, vb)?);
        }
        let verts = &vcache[&draw.vertex_buf];
        let place = draw_placement(&part_world, draw);
        let tris: Vec<[u16; 3]> = alr::triangulate(draw).into_iter().filter(|t| t.iter().all(|&i| (i as usize) < verts.len())).collect();
        if tris.is_empty() {
            continue;
        }
        let mut remap: HashMap<u16, u32> = HashMap::new();
        let (mut pos, mut uv, mut joints, mut weights, mut idx) = (Vec::new(), Vec::new(), Vec::<u16>::new(), Vec::new(), Vec::<u32>::new());
        // Per-vertex lighting extras (SWBOS_vs/ps): the mirrored-axes flag
        // (-> tangent frame) and the specular weight (-> TEXCOORD_1.x).
        let (mut tang, mut spec_uv) = (Vec::<f32>::new(), Vec::<f32>::new());
        for t in &tris {
            // Game order reversed: the Z mirror flips handedness. PD_CHAR_FLIP_WINDING=1: calibration only.
            let order = if flip_winding { [t[0], t[1], t[2]] } else { [t[0], t[2], t[1]] };
            for &vi in &order {
                let next = remap.len() as u32;
                let ni = *remap.entry(vi).or_insert_with(|| {
                    let v = &verts[vi as usize];
                    let vp = place.transform_point3(Vec3::from(v.pos));
                    pos.extend_from_slice(&[vp.x, vp.y, -vp.z]);
                    uv.extend_from_slice(&v.uv);
                    tang.extend_from_slice(&light_tangent(v.mirror));
                    spec_uv.extend_from_slice(&[v.spec_weight, 0.0]);
                    let mut jw = [(0u16, 0.0f32); 2];
                    for k in 0..2 {
                        if let Some(j) = palette_joint(&draw.palette, v.regs[k]) {
                            jw[k] = (j + off, v.weights[k]);
                        }
                    }
                    let sum = jw[0].1 + jw[1].1;
                    if sum <= 1e-4 {
                        // No usable influence: pin to the draw's own joint.
                        if std::env::var_os("PD_DEBUG_PIN").is_some() {
                            eprintln!("pin: part {pi} draw xform {} palette {:?} regs {:?} w {:?} pos {:?}", draw.transform_idx, draw.palette, v.regs, v.weights, v.pos);
                        }
                        jw = [(draw.transform_idx + off, 1.0), (0, 0.0)];
                    } else {
                        jw[0].1 /= sum;
                        jw[1].1 /= sum;
                    }
                    if pi == 0 {
                        let dom = if jw[0].1 >= jw[1].1 { jw[0].0 } else { jw[1].0 };
                        let p = Vec3::new(vp.x, vp.y, -vp.z);
                        if let Some(rs) = skirt.as_ref().and_then(|plan| plan.reskin(dom as usize, p, v.uv, skirt_first)) {
                            joints.extend_from_slice(&[rs[0].0, rs[1].0, rs[2].0, rs[3].0]);
                            weights.extend_from_slice(&[rs[0].1, rs[1].1, rs[2].1, rs[3].1]);
                            return next;
                        }
                        skin_samples.push((dom, p));
                    }
                    joints.extend_from_slice(&[jw[0].0, jw[1].0, 0, 0]);
                    weights.extend_from_slice(&[jw[0].1, jw[1].1, 0.0, 0.0]);
                    next
                });
                idx.push(ni);
            }
        }
        let pa = b.push_f32(&pos, "VEC3", Some(34962), true);
        let ua = b.push_f32(&uv, "VEC2", Some(34962), false);
        let jbytes: Vec<u8> = joints.iter().flat_map(|j| j.to_le_bytes()).collect();
        let jv = b.push_view(&jbytes, Some(34962));
        let ja = b.push_accessor(json!({ "bufferView": jv, "componentType": 5123, "count": joints.len() / 4, "type": "VEC4" }));
        let wa = b.push_f32(&weights, "VEC4", Some(34962), false);
        let ibytes: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
        let iv = b.push_view(&ibytes, Some(34963));
        let ia = b.push_accessor(json!({ "bufferView": iv, "componentType": 5125, "count": idx.len(), "type": "SCALAR" }));
        let m = model.materials.get(draw.material_idx as usize);
        if char_spec.is_none() {
            char_spec = m.filter(|m| m.flags & 0x04 != 0).map(|m| m.spec_bgra);
        }
        let tex = m.map(|m| m.texture_idx as usize).unwrap_or(usize::MAX);
        let normal = m.map(|m| m.normal_idx as usize).filter(|&i| i != tex && i < a.textures.len());
        let mat = mats.get_lit(&mut b, a, tex, normal, &mut stats);
        // The game's normal maps are object-space (bind pose). Giving every
        // vertex the bind-space axes as its tangent frame (T=X, B=Y, N=Z)
        // makes the standard tangent-space normal mapping reproduce
        // object-space lighting, and skinning rotates the frame with the bone.
        // Mirrored-axes vertices (`SWBOS_vs` cbase[95] = (-1,1,-1)) get the
        // frame (-X, +Y, +Z) instead: see `light_tangent`.
        let count = pos.len() / 3;
        let na = b.push_f32(&[0.0, 0.0, 1.0].repeat(count), "VEC3", Some(34962), false);
        let ta = b.push_f32(&tang, "VEC4", Some(34962), false);
        let ua1 = b.push_f32(&spec_uv, "VEC2", Some(34962), false);
        primitives.push(json!({
            "attributes": { "POSITION": pa, "NORMAL": na, "TANGENT": ta, "TEXCOORD_0": ua, "TEXCOORD_1": ua1, "JOINTS_0": ja, "WEIGHTS_0": wa },
            "indices": ia,
            "material": mat,
        }));
        stats.draws += 1;
        stats.triangles += tris.len();
    }
    }
    if primitives.is_empty() {
        bail!("character has no skinned draws");
    }

    add_body_capsules(&mut nodes, &children, &bind_world[..n], &skin_samples);
    let mut mesh_list = vec![json!({ "name": "body", "primitives": primitives })];
    // The authored shadow hulls (docs/SHADOWS.md "Fighter hulls"): rigid
    // format-0x07 draws on the body model's helper joints, material flag
    // 0x2000. The program draws these as the fighter's stencil volumes
    // (`SSHADOWCH`); pd_game uses them as the fighter's shadow casters. One
    // named child node per hull under its joint, in the joint's own frame
    // (they follow the baked animation); never drawn.
    for (di, draw) in model.draws.iter().enumerate() {
        let Some(vb) = model.vertbufs.get(draw.vertex_buf as usize) else { continue };
        if vb.format != 0x07 || !draw.palette.is_empty() || !model.materials.get(draw.material_idx as usize).is_some_and(|m| m.flags & 0x2000 != 0) {
            continue;
        }
        let joint = draw.transform_idx as usize;
        let Ok(verts) = a.read_vertices(vb) else { continue };
        let tris: Vec<[u16; 3]> = alr::triangulate(draw).into_iter().filter(|t| t.iter().all(|&i| (i as usize) < verts.len())).collect();
        if joint >= n || tris.is_empty() {
            continue;
        }
        let mut remap: HashMap<u16, u32> = HashMap::new();
        let (mut pos, mut idx) = (Vec::<f32>::new(), Vec::<u32>::new());
        for t in &tris {
            // Game order reversed: the Z mirror flips handedness.
            for &vi in &[t[0], t[2], t[1]] {
                let next = remap.len() as u32;
                let ni = *remap.entry(vi).or_insert_with(|| {
                    let p = verts[vi as usize].pos;
                    pos.extend_from_slice(&[p[0], p[1], -p[2]]);
                    next
                });
                idx.push(ni);
            }
        }
        let pa = b.push_f32(&pos, "VEC3", Some(34962), true);
        let ibytes: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
        let iv = b.push_view(&ibytes, Some(34963));
        let ia = b.push_accessor(json!({ "bufferView": iv, "componentType": 5125, "count": idx.len(), "type": "SCALAR" }));
        mesh_list.push(json!({ "name": format!("shadowhull_d{di}"), "primitives": [{ "attributes": { "POSITION": pa }, "indices": ia }] }));
        nodes.push(json!({ "name": format!("{}_d{di}", HULL_TAG), "mesh": mesh_list.len() - 1 }));
        let node = nodes.len() - 1;
        match nodes[joint].get_mut("children").and_then(|c| c.as_array_mut()) {
            Some(c) => c.push(json!(node)),
            None => nodes[joint]["children"] = json!([node]),
        }
        stats.hulls += 1;
    }
    let meshes = json!(mesh_list);
    nodes.push(json!({ "name": "body", "mesh": 0, "skin": 0 }));
    let body = nodes.len() - 1;
    let mut root_children = roots.clone();
    root_children.push(body);
    nodes.push(json!({ "name": "character", "children": root_children }));
    let scene_root = nodes.len() - 1;

    // --- Animations (body skeleton only; part chains have no tracks), baked per frame.
    let bind = pose::bind_locals(model);
    let mut animations = Vec::new();
    let mut sources: Vec<(&str, &Alr)> = vec![("", a)];
    for (name, alr) in &opts.extra_anim_sources {
        sources.push((name.as_str(), alr));
    }
    for (prefix, src) in sources {
        let count = crate::bytes::u32_at(&src.data, 16).unwrap_or(0) as usize;
        for id in 0..count {
            if !opts.anims.is_empty() && prefix.is_empty() && !opts.anims.contains(&id) {
                continue;
            }
            let Some(an) = anim::read_animation(src, id)? else { continue };
            if an.tracks.is_empty() {
                continue;
            }
            let name = if prefix.is_empty() { ANIM_NAMES.get(id).map(|s| s.to_string()).unwrap_or(format!("anim{id}")) } else { format!("{prefix}_{id}") };
            if let Some(v) = bake_animation(&mut b, model, &bind, &an, order, &name) {
                animations.push(v);
                stats.animations += 1;
            }
        }
    }

    let mut doc = json!({
        "asset": { "version": "2.0", "generator": "pd_formats" },
        "scene": 0,
        "scenes": [{ "nodes": [scene_root] }],
        "nodes": nodes,
        "meshes": meshes,
        "skins": [{ "joints": (0..joint_count).collect::<Vec<_>>(), "inverseBindMatrices": ibm_acc, "skeleton": roots.first().copied().unwrap_or(0) }],
    });
    if !animations.is_empty() {
        doc["animations"] = json!(animations);
    }
    if let Some([b0, g0, r0, power]) = char_spec {
        // [R, G, B, power] bytes, read by pd_game's char_light.
        doc["asset"]["extras"] = json!({ "pd_spec": [r0, g0, b0, power] });
    }
    mats.write_into(&mut doc);
    b.write_into(&mut doc);

    let (mn, mx) = bind_bounds(&doc, &b);
    stats.bounds_min = mn;
    stats.bounds_max = mx;
    stats.core_min = mn;
    stats.core_max = mx;
    stats.models = 1;
    Ok((write_glb(&doc, &b.bin), stats))
}

fn bind_bounds(doc: &Value, b: &Builder) -> ([f32; 3], [f32; 3]) {
    let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in doc["meshes"][0]["primitives"].as_array().into_iter().flatten() {
        let acc = &b.accessors[p["attributes"]["POSITION"].as_u64().unwrap_or(0) as usize];
        for k in 0..3 {
            mn[k] = mn[k].min(acc["min"][k].as_f64().unwrap_or(0.0) as f32);
            mx[k] = mx[k].max(acc["max"][k].as_f64().unwrap_or(0.0) as f32);
        }
    }
    (mn, mx)
}

fn bake_animation(b: &mut Builder, model: &Model, bind: &[Local], an: &anim::Animation, order: Order, name: &str) -> Option<Value> {
    let len = an.length().max(1.0);
    let frames = len.ceil() as usize + 1;
    let times: Vec<f32> = (0..frames).map(|f| f as f32 / FPS).collect();
    let time_acc = b.push_f32(&times, "SCALAR", None, true);
    let mut samplers = Vec::new();
    let mut channels = Vec::new();
    for t in &an.tracks {
        let j = t.joint as usize;
        if j >= model.joints.len() {
            continue;
        }
        let (mut tr, mut rot) = (Vec::with_capacity(frames * 3), Vec::with_capacity(frames * 4));
        for f in 0..frames {
            let l = pose::track_local(bind[j], t, (f as f32).min(len));
            let (tt, rr, _) = local_rh(&l, order);
            tr.extend_from_slice(&tt.to_array());
            rot.extend_from_slice(&rr.to_array());
        }
        // Keep quaternions on one hemisphere so interpolation doesn't spin.
        for f in 1..frames {
            let (p, c) = (f * 4 - 4, f * 4);
            let dot: f32 = (0..4).map(|k| rot[p + k] * rot[c + k]).sum();
            if dot < 0.0 {
                for k in 0..4 {
                    rot[c + k] = -rot[c + k];
                }
            }
        }
        // Root joints carry root motion (xz drift); gameplay owns position.
        let is_root = model.joints[j].parent < 0;
        if !t.translation.is_empty() && !is_root {
            let out = b.push_f32(&tr, "VEC3", None, false);
            samplers.push(json!({ "input": time_acc, "output": out, "interpolation": "LINEAR" }));
            channels.push(json!({ "sampler": samplers.len() - 1, "target": { "node": j, "path": "translation" } }));
        }
        if !t.rotation.is_empty() {
            let out = b.push_f32(&rot, "VEC4", None, false);
            samplers.push(json!({ "input": time_acc, "output": out, "interpolation": "LINEAR" }));
            channels.push(json!({ "sampler": samplers.len() - 1, "target": { "node": j, "path": "rotation" } }));
        }
    }
    if channels.is_empty() {
        return None;
    }
    Some(json!({ "name": name, "samplers": samplers, "channels": channels }))
}

/// Which texture index is the character's costume-colour skin: the
/// `texture_idx` most `IndexBuffer`s' materials point at (by vertex-index
/// count, same method as `pd_convert texprobe`'s "used-indices"). Every
/// playable character (`pc00a`..`pc10a`) has exactly two textures -- this
/// one (a 512x512 Dxt1, ~9,000+ used indices: the body/clothing skin) and a
/// second, unused by any `texture_idx` (an object-space normal map,
/// referenced only via `MaterialEntry::normal_idx`) -- and it is always
/// index 0 in every character checked, but this is computed rather than
/// assumed in case a character differs. This is the texture PIPELINE.md's
/// "Player/<id>/<id>0..3.alr" describes as the one ~175KB chunk that swaps
/// between the four costume-colour siblings (confirmed on all 11 playable
/// characters: `pd_convert texprobe` on colour 0 vs 1 vs 2 vs 3 shows only
/// this texture's mean colour changing, same format/size/pointer).
pub fn base_colour_texture_index(a: &Alr) -> Option<usize> {
    let mut uses: HashMap<usize, usize> = HashMap::new();
    for m in &a.models {
        for d in &m.draws {
            if let Some(mat) = m.materials.get(d.material_idx as usize) {
                *uses.entry(mat.texture_idx as usize).or_default() += d.indices.len();
            }
        }
    }
    uses.into_iter().max_by_key(|&(_, n)| n).map(|(i, _)| i)
}

/// Decode one costume-colour ALR's base-colour texture (`index`, from
/// `base_colour_texture_index` on the colour-0/base file so every sibling
/// uses the same index -- they share layout, only the texture bytes at that
/// slot differ) to a PNG. Used to export `cache/chars/<id>.c1.png` etc.
/// alongside the colour-0 glb, without duplicating the whole model per
/// colour (FRONTEND.md Requests, "Colour-variant character textures").
pub fn export_base_colour_png(a: &Alr, index: usize) -> Result<Vec<u8>> {
    let t = a.textures.get(index).context("costume-colour texture index out of range")?;
    crate::texture::decode(a, t)?.to_png()
}

/// Per-clip root-joint drift for the body model, for a later sim lane.
///
/// The program adds the root track's rotated x/z change each tick to the
/// player's position for clips started with the root-motion flag, and
/// ignores the root's y (PROGRAM-RE.md "Root motion"). We still **strip**
/// root translation from the baked glTF (`bake_animation`'s `is_root`
/// check) and play every clip in place, as today -- gameplay owns
/// position. This is a separate report of what that stripped motion was,
/// in our exported (right-handed, mirrored-Z) units, so the sim can apply
/// it later: `cache/chars/<id>.rootmotion.json`.
///
/// `forward` is the clip's net displacement along the character's facing:
/// +Z in our exported right-handed space, which is also the direction
/// `pd_game::sim`'s zero-yaw `fwd` points (Δz needs no extra sign flip
/// here; see the sign check in the code below). `side` is Δx (+ = the
/// character's right); `up` is Δy. `curve` is
/// the same three numbers sampled at every frame (30 fps, frame 0 = bind),
/// **cumulative from frame 0** -- a later frame's rotation is not
/// re-applied, matching how `bake_animation` samples the raw track (the
/// game itself re-rotates the per-tick delta by the *current* facing,
/// which can differ once the sim is turning the player during the clip;
/// unconfirmed whether that matters for any clip here ⚠️).
pub fn root_motion(a: &Alr, opts: &CharacterOptions) -> Result<Value> {
    let skinned: Vec<&Model> = a.models.iter().filter(|m| m.has_skeleton && m.draws.iter().any(|d| !d.palette.is_empty())).collect();
    let Some(&model) = skinned.first() else {
        bail!("no skinned model found");
    };
    let order = opts.order;
    let bind = pose::bind_locals(model);
    // Several joints can have parent < 0: helper/attachment nodes sit at the
    // top level too (e.g. pc00a0's joints 0, 1, 2, 19 -- all static, name
    // 0xFFFFFFFF), alongside the one real animated skeleton root (joint 20
    // there, parent of the spine chain). Pick the parentless joint that is
    // the ultimate ancestor of the most *skinned* joints, not just the first
    // one in the list.
    let mut ancestor_votes: HashMap<usize, usize> = HashMap::new();
    let skinned_joints: std::collections::HashSet<usize> = model.draws.iter().flat_map(|d| d.palette.iter().map(|&p| p as usize)).collect();
    for &j in &skinned_joints {
        let mut cur = j;
        for _ in 0..model.joints.len() {
            match model.joints.get(cur) {
                Some(joint) if joint.parent >= 0 => cur = joint.parent as usize,
                _ => break,
            }
        }
        *ancestor_votes.entry(cur).or_default() += 1;
    }
    let Some(root_j) = ancestor_votes.iter().max_by_key(|&(_, v)| v).map(|(&k, _)| k).or_else(|| model.joints.iter().position(|j| j.parent < 0)) else {
        return Ok(json!({ "root_joint": Value::Null, "clips": {} }));
    };
    let mut clips = serde_json::Map::new();
    for (id, name) in ANIM_NAMES.iter().enumerate() {
        let Some(an) = anim::read_animation(a, id)? else { continue };
        let Some(t) = an.tracks.iter().find(|t| t.joint as usize == root_j) else {
            continue;
        };
        if t.translation.is_empty() {
            continue;
        }
        let len = an.length().max(1.0);
        let frame_count = len.ceil() as usize + 1;
        let mut curve = Vec::with_capacity(frame_count);
        for f in 0..frame_count {
            let l = pose::track_local(bind[root_j], t, (f as f32).min(len));
            let (tt, _, _) = local_rh(&l, order);
            curve.push([tt.x, tt.y, tt.z]);
        }
        let first = curve[0];
        let last = *curve.last().unwrap();
        // `local_rh` already mirrors Z into our right-handed, +Z-forward
        // space (gltf::to_rh; pd_game::sim's `fwd` is +Z at yaw 0), so the
        // exported Z delta *is* "forward" with no extra sign flip. Checked
        // against PROGRAM-RE.md's "Root motion" numbers (raw, unmirrored
        // game-space z, where the character faces −Z so its own negative-z
        // readings are forward): ATK_STAMP doc −42 vs this joint's mirrored
        // +42.27; ATK_SHOT doc +11 ("a step back") vs mirrored −10.90 --
        // both match once the mirror is accounted for.
        let (forward, side, up) = (last[2] - first[2], last[0] - first[0], last[1] - first[1]);
        clips.insert(
            (*name).to_string(),
            json!({
                "frames": frame_count - 1,
                "forward": forward,
                "side": side,
                "up": up,
                // The absolute root-track translation at frame 0 (x side, y up, z forward; the
                // curve below is relative to it): where the program's own pose puts the root
                // joint against the fighter's position.
                "start": [first[0], first[1], first[2]],
                "curve": curve.iter().map(|c| json!({ "forward": c[2] - first[2], "side": c[0] - first[0], "up": c[1] - first[1] })).collect::<Vec<_>>(),
            }),
        );
    }
    Ok(json!({ "root_joint": root_j, "clips": clips }))
}

/// The caster's right hand through every attack clip, for the sim: the
/// program's weapon / spawn point when a skill's effect carries no `SHOTnn`
/// marker (`FUN_14018a310` falls back to the owner's `RHAND` bone matrix,
/// name `0xADB07630`; melee reads the same matrix every tick of its hit
/// window, `FUN_14026c340` / `FUN_14026c730`). docs/FORMATS-EFFECTS.md
/// "Shot markers".
///
/// Per clip named `ATK_*` (every cast clip of an attack skill), one sample per
/// clip frame (30 fps, frame 0 = the clip's first pose; the same frames
/// `bake_animation` writes): `pos` = the bone's position and `fwd` = its local
/// +Z axis, both in the character's own right-handed frame (feet at the
/// origin, +Z forward at yaw 0, the frame of the exported glb). Root joints
/// keep their bind translation, as in the glb (gameplay owns position), so
/// the hand sits where the rendered clip shows it. `fwd` is the axis the
/// program's sword test extends the blade along (`FUN_14026c730`: from the
/// attached point along row 2 of its matrix), converted like every vector
/// (z mirrored).
pub fn hand_paths(a: &Alr, opts: &CharacterOptions) -> Result<Value> {
    let skinned: Vec<&Model> = a.models.iter().filter(|m| m.has_skeleton && m.draws.iter().any(|d| !d.palette.is_empty())).collect();
    let Some(&model) = skinned.first() else {
        bail!("no skinned model found");
    };
    let Some(hand) = model.joints.iter().position(|j| j.name == 0xADB0_7630) else {
        bail!("no RHAND joint");
    };
    let order = opts.order;
    let bind = pose::bind_locals(model);
    let r2 = |v: f32| (v * 100.0).round() / 100.0;
    let r3 = |v: f32| (v * 1000.0).round() / 1000.0;
    let mut clips = serde_json::Map::new();
    for (id, name) in ANIM_NAMES.iter().enumerate() {
        if !name.starts_with("ATK_") {
            continue;
        }
        let Some(an) = anim::read_animation(a, id)? else { continue };
        let len = an.length().max(1.0);
        let frames = len.ceil() as usize + 1;
        let (mut pos, mut fwd) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
        for f in 0..frames {
            let mut locals = pose::animated_locals(model, &an, (f as f32).min(len));
            for (j, l) in locals.iter_mut().enumerate() {
                if model.joints[j].parent < 0 {
                    l.t = bind[j].t;
                }
            }
            let world = pose::world_matrices(model, &locals, order)[hand];
            let rh = to_rh(world);
            let p = rh.w_axis;
            // Local +Z of the bone, as a direction in the (left-handed) game
            // space the program works in, then mirrored into ours.
            let z = world.z_axis.truncate().normalize_or_zero();
            pos.push([r2(p.x), r2(p.y), r2(p.z)]);
            fwd.push([r3(z.x), r3(z.y), r3(-z.z)]);
        }
        clips.insert((*name).to_string(), json!({ "frames": frames - 1, "pos": pos, "fwd": fwd }));
    }
    Ok(json!({ "joint": "RHAND", "fps": FPS, "clips": clips }))
}

/// Tangent (xyz, w = handedness) that gives a vertex the bind-space frame
/// its object-space normal map is read in.
///
/// `SWBOS_vs` multiplies the bind-space light vectors by `cbase[94 + z]`
/// (`DAT_1402f7b28` = (1,1,-1), (-1,1,-1); `z` = blend-weight byte 16, 255
/// for the mirrored/left-side parts) before `SWBOS_ps` dots them with the raw
/// normal map, so in the game's left-handed space N = (sx*dx, dy, -dz) with
/// sx = +-1. Our export mirrors Z, which cancels the -dz: N = (sx*dx, dy, dz).
/// Bevy builds N = t.x*T + t.y*B + t.z*Nrm with B = cross(Nrm, T) * w, so
/// normal (0,0,1), tangent (1,0,0,1) gives (dx,dy,dz) and tangent
/// (-1,0,0,-1) gives (-dx,dy,dz). Measured on pc00a: mean cosine between the
/// decoded map and the mesh normals is 0.46 for this convention, -0.10 for
/// the old Z-flipped one; the two side-mirrored limb draws are 0.98 / 0.98
/// with the matching flip and 0.68 without.
pub fn light_tangent(mirror: bool) -> [f32; 4] {
    if mirror {
        [-1.0, 0.0, 0.0, -1.0]
    } else {
        [1.0, 0.0, 0.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bevy's mikktspace frame: N_w = t.x*T + t.y*B + t.z*Nrm, B = cross(Nrm, T)*w.
    /// Plain vertices read the map as (dx,dy,dz); mirrored ones as (-dx,dy,dz)
    /// (`SWBOS_vs` cbase[94]/[95] = (1,1,-1) / (-1,1,-1), then our Z mirror).
    #[test]
    fn light_tangent_gives_the_shader_frames() {
        let frame = |t: [f32; 4], d: Vec3| {
            let n = Vec3::Z;
            let tv = Vec3::new(t[0], t[1], t[2]);
            let b = n.cross(tv) * t[3];
            d.x * tv + d.y * b + d.z * n
        };
        let d = Vec3::new(0.3, -0.5, 0.8);
        assert!((frame(light_tangent(false), d) - Vec3::new(0.3, -0.5, 0.8)).length() < 1e-6);
        assert!((frame(light_tangent(true), d) - Vec3::new(-0.3, -0.5, 0.8)).length() < 1e-6);
    }

    /// Real data (skipped without the game files): a fighter exports its 19
    /// authored shadow hulls (pc04a too) as child nodes of their helper joints,
    /// each with geometry and no skin.
    #[test]
    fn the_authored_shadow_hulls_are_exported_under_their_joints() {
        let dir = std::env::var_os("PD_GAME_FILES_PLAYER").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"..\..\..\game_files\Assets\Data\Player"));
        for (id, want) in [("pc00a", 19usize), ("pc04a", 19)] {
            let path = dir.join(id).join(format!("{id}0.alr"));
            if !path.is_file() {
                eprintln!("skipped: no game files at {}", path.display());
                return;
            }
            let a = Alr::load(&path).unwrap();
            let opts = CharacterOptions { order: Order::GAME, anims: vec![0], extra_anim_sources: Vec::new(), coat_rig: None };
            let (glb, stats) = export_character_glb(&a, &opts).unwrap();
            assert_eq!(stats.hulls, want, "{id}");
            let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
            let doc: Value = serde_json::from_slice(&glb[20..20 + len]).unwrap();
            let nodes = doc["nodes"].as_array().unwrap();
            let hulls: Vec<(usize, &Value)> = nodes.iter().enumerate().filter(|(_, n)| n["name"].as_str().is_some_and(|s| s.starts_with(HULL_TAG))).collect();
            assert_eq!(hulls.len(), want);
            for (i, h) in &hulls {
                assert!(h["mesh"].is_u64() && h["skin"].is_null(), "{id}: hulls are rigid");
                let parents = nodes.iter().filter(|n| n["children"].as_array().is_some_and(|c| c.iter().any(|x| x.as_u64() == Some(*i as u64)))).count();
                assert_eq!(parents, 1, "{id}: one joint parent");
            }
        }
    }

    /// Probe (ignored; needs the game files): the BODY1 bone's world position in the idle clip (WAIT0, frames
    /// 0 / 10 / 20, 30 fps) per playable model, root joint at its bind translation and animated. The
    /// program's target point is that bone's world x / y / z (`FUN_1402513a0`); the heights printed here are
    /// `pd_game::body::point_height` (MEASURED 2026-10-09). Run:
    /// `cargo test -p pd_formats body1_height_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn body1_height_probe() {
        // The third name of the program's five-bone hash table at 0x1402dcdd8 (LHAND, RHAND, BODY1, ALPHA, HEAD).
        const BODY1: u32 = 0x4D1D_1710;
        let dir = std::env::var_os("PD_GAME_FILES_PLAYER").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"..\..\..\game_files\Assets\Data\Player"));
        for id in ["pc00a", "pc01a", "pc02a", "pc03a", "pc04a", "pc05a", "pc06a", "pc07a", "pc08a", "pc09a", "pc10a"] {
            let Ok(a) = Alr::load(&dir.join(id).join(format!("{id}0.alr"))) else {
                eprintln!("skipped: no game files for {id}");
                continue;
            };
            let Some(model) = a.models.iter().find(|m| m.has_skeleton && m.draws.iter().any(|d| !d.palette.is_empty())) else { continue };
            let Some(j) = model.joints.iter().position(|j| j.name == BODY1) else { continue };
            let bind = pose::bind_locals(model);
            let an = anim::read_animation(&a, 0).unwrap().unwrap();
            let mut out = String::new();
            for f in [0.0f32, 10.0, 20.0] {
                let mut locals = pose::animated_locals(model, &an, f.min(an.length()));
                let root = to_rh(pose::world_matrices(model, &locals, Order::GAME)[j]).w_axis;
                for (k, l) in locals.iter_mut().enumerate() {
                    if model.joints[k].parent < 0 {
                        l.t = bind[k].t;
                    }
                }
                let p = to_rh(pose::world_matrices(model, &locals, Order::GAME)[j]).w_axis;
                out += &format!(" f{f}: ({:.2}, {:.2}, {:.2}) root animated ({:.2}, {:.2}, {:.2})", p.x, p.y, p.z, root.x, root.y, root.z);
            }
            eprintln!("{id} BODY1{out}");
        }
    }

    fn joint(name: u32, flags: u16, params: [f32; 3]) -> alr::Joint {
        alr::Joint { position: [0.0; 3], rotation: [0.0; 3], scale: [1.0; 3], parent: -1, name, flags, params }
    }

    #[test]
    fn cloth_nodes_read_like_the_program() {
        // pc00a0 model 1: CL021 = row 2, column 1; kept 0.8, limit 10°.
        let n = cloth_node(&joint(0x52B4_ED10, 0x132, [0.8, 10.0, 0.0])).unwrap();
        assert_eq!((n.row, n.col), (2, 1));
        assert!((n.keep - 0.8).abs() < 1e-6 && (n.limit - 10f32.to_radians()).abs() < 1e-6);
        // All-zero parameters: the program's defaults.
        let d = cloth_node(&joint(0x52B4_E068, 0x132, [0.0; 3])).unwrap();
        assert_eq!((d.row, d.col), (0, 0));
        assert!((d.keep - 0.9).abs() < 1e-6 && (d.limit - 60f32.to_radians()).abs() < 1e-6);
        // Needs flag 0x100; CLC00 (the part root) and HEAD are not nodes.
        assert!(cloth_node(&joint(0x52B4_ED10, 0x032, [0.8, 10.0, 0.0])).is_none());
        assert!(cloth_node(&joint(0x52C0_9868, 0x110, [0.0; 3])).is_none());
        assert!(cloth_node(&joint(0x7032_0580, 0x513, [0.0; 3])).is_none());
    }
}
