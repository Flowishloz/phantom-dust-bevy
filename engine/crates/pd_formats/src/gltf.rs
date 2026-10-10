//! Export a parsed ALR as a binary glTF (.glb) that Bevy can load directly.
//!
//! The game is Direct3D (left-handed); glTF is right-handed. We mirror Z on
//! everything and flip triangle winding to compensate.

use crate::alr::{self, Alr, Joint};
use crate::stagedat::LightSet;
use crate::texture;
use anyhow::Result;
use glam::{EulerRot, Mat4, Quat, Vec3};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Default)]
pub struct ExportStats {
    pub animations: usize,
    /// Cloth chains generated for a long coat (0 = none).
    pub coat_chains: usize,
    /// Authored shadow hulls exported as child nodes of their joints
    /// (characters, `character::HULL_TAG`).
    pub hulls: usize,
    pub models: usize,
    /// Placed model copies exported (`ExportOptions::instances` with draws).
    pub instances: usize,
    pub draws: usize,
    pub triangles: usize,
    pub textures_ok: usize,
    pub textures_failed: usize,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    /// Bounds ignoring outliers such as sky domes (5th..95th percentile of
    /// per-draw boxes). Useful for framing a camera on the playable area.
    pub core_min: [f32; 3],
    pub core_max: [f32; 3],
    draw_boxes: Vec<(Vec3, Vec3)>,
}

/// Draws larger than this (units, any axis) are ignored for the core bounds.
const CORE_MAX_DRAW_SIZE: f32 = 1500.0;

impl ExportStats {
    fn finish(&mut self) {
        if self.draw_boxes.is_empty() {
            return;
        }
        // Huge draws (sky shells, sea planes, distant backdrops) say nothing
        // about where the arena is; leave them out when enough others remain.
        let small: Vec<(Vec3, Vec3)> = self.draw_boxes.iter().copied().filter(|b| (b.1 - b.0).max_element() < CORE_MAX_DRAW_SIZE).collect();
        let boxes = if small.len() >= 20 { small } else { self.draw_boxes.clone() };
        for k in 0..3 {
            let mut mins: Vec<f32> = boxes.iter().map(|b| b.0[k]).collect();
            let mut maxs: Vec<f32> = boxes.iter().map(|b| b.1[k]).collect();
            mins.sort_by(f32::total_cmp);
            maxs.sort_by(f32::total_cmp);
            self.core_min[k] = mins[mins.len() * 5 / 100];
            self.core_max[k] = maxs[(maxs.len() * 95 / 100).min(maxs.len() - 1)];
        }
    }
}

#[derive(Default)]
pub struct ExportOptions {
    /// Place each draw at its joint's transform (needed for stages; also
    /// gives rigid characters a rough bind pose).
    pub apply_transforms: bool,
    /// Also export position-only (untextured) geometry, for debugging.
    pub include_untextured: bool,
    /// Models not drawn in place (stage object templates: they only appear
    /// through `instances`).
    pub skip_models: Vec<usize>,
    /// Extra copies of models, e.g. stage objects placed by the `.dat`.
    pub instances: Vec<ModelInstance>,
    /// Stages: bake the game's stage lighting (the `.dat` light script's
    /// stage set) into per-vertex colours, the way the `SD` stage shader
    /// lights each vertex. Also draws the sky model's vertex-coloured
    /// backdrop and fades the sky dome by its vertex alpha. `None` = plain
    /// unlit textures (effects, characters' props, older caches).
    pub lighting: Option<LightSet>,
    /// Effects: write each draw's own vertex colours as `COLOR_0` (linear,
    /// from the game's gamma-space BGRA bytes; draws whose colours are all
    /// white get none). The effect vertex shaders multiply the piece colour
    /// by the vertex colour (`PDEFTBB*_vs`: `mul r0, v1, cb0[35]`), and 7,900 of
    /// the 10,000 plain pieces have non-white ones (soft edges, gradients).
    pub vertex_colours: bool,
}

/// Stage model slot holding the sky dome + backdrop box (slot 0 = stage
/// mesh, 1 = background, 2 = sky: modding wiki ALR page, checked on st03 /
/// st06 / st13 / st23: slot 2 is a ~11,000-unit dome plus a 12,400-unit box).
pub const SKY_MODEL: usize = 2;

/// gamma-space value -> linear (the game lights in gamma space: texture x
/// light; storing light^2.2 keeps that product the same after Bevy's sRGB
/// decode). Values above 1 extrapolate.
fn gamma_to_linear(c: f32) -> f32 {
    if c <= 0.0 {
        0.0
    } else if c <= 1.0 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    } else {
        c.powf(2.2)
    }
}

/// Per-vertex RGBA (linear) for one placed draw: the `SD` vertex shader's
/// light (hemisphere ambient between ground and sky colour by N·axis, plus
/// N·−dir × sun colour) times the vertex colour. Vertices without a normal
/// (`SC` / `SVC` shaders) keep their vertex colour unlit. `rot` = the
/// draw's world rotation in game (left-handed) space.
/// ⚠️ the hemisphere blend `t = 0.5 + 0.5·N·axis` is our reading of the
/// shader's `dot(N, cS0_VEC.xyz) + cS0_VEC.w` (constants set at runtime).
pub fn light_vertices(verts: &[alr::VertexLight], set: &LightSet, rot: glam::Mat3, keep_alpha: bool) -> Vec<[f32; 4]> {
    let c = |x: [u8; 3]| Vec3::new(x[0] as f32, x[1] as f32, x[2] as f32) / 255.0;
    // Roles per the program's own block layout (stagedat.rs `LightSet`):
    // the directional colour is the script's first colour, the hemisphere
    // "up" colour its third, "down" its second.
    let (sun, sky, ground) = (c(set.dl_col()), c(set.hemi_up()), c(set.hemi_down()));
    let axis = Vec3::from(set.hemi_axis).normalize_or(Vec3::Y);
    let to_light = -Vec3::from(set.dir).normalize_or(Vec3::NEG_Y);
    verts
        .iter()
        .map(|v| {
            let vc = v.color.map_or(Vec3::ONE, |k| Vec3::new(k[0] as f32, k[1] as f32, k[2] as f32) / 255.0);
            let a = if keep_alpha { v.color.map_or(1.0, |k| k[3] as f32 / 255.0) } else { 1.0 };
            let lit = match v.normal {
                Some(n) => {
                    let n = (rot * Vec3::from(n)).normalize_or_zero();
                    let t = (0.5 + 0.5 * n.dot(axis)).clamp(0.0, 1.0);
                    let amb = ground + (sky - ground) * t;
                    vc * (amb + sun * n.dot(to_light).max(0.0))
                }
                None => vc,
            };
            [gamma_to_linear(lit.x), gamma_to_linear(lit.y), gamma_to_linear(lit.z), a]
        })
        .collect()
}

/// Stage draws the program shades with the bump-mapped pixel shader family
/// `SBTS` (`SBTS_vs`/`SBTS_ps`): vertex format 0x03 (position, normal, UV,
/// colour, tangent, binormal) with material flag 0x20, not a shadow volume.
/// Flag 0x400 + a third texture are the environment-reflection variants
/// (`SBTSREF*`), which share the lighting and add the reflection
/// (docs/STAGE-LIGHT.md).
pub fn is_stage_bump(vertex_format: u8, material_flags: u16) -> bool {
    vertex_format == 0x03 && material_flags & 0x20 != 0 && material_flags & 0x2000 == 0
}

/// One placed copy of an ALR model. Exported as a parent node named `name`
/// (carrying `world_lh`, mirrored to right-handed) with one child per draw.
#[derive(Debug, Clone)]
pub struct ModelInstance {
    pub model: usize,
    /// Object -> world, in the game's left-handed space.
    pub world_lh: Mat4,
    pub name: String,
}

/// Object -> world transform (left-handed game space) for a position and
/// Euler X/Y/Z rotation, using the ALR joint convention (R = Rz·Ry·Rx).
pub fn placement_lh(position: [f32; 3], rotation: [f32; 3]) -> Mat4 {
    let [rx, ry, rz] = rotation;
    Mat4::from_translation(Vec3::from(position)) * Mat4::from_quat(Quat::from_euler(EulerRot::ZYX, rz, ry, rx))
}

/// Left-handed game point -> right-handed export point.
pub fn point_to_rh(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], p[1], -p[2])
}

#[derive(Default)]
pub(crate) struct Builder {
    pub(crate) bin: Vec<u8>,
    buffer_views: Vec<Value>,
    pub(crate) accessors: Vec<Value>,
}

impl Builder {
    pub(crate) fn write_into(&self, doc: &mut Value) {
        doc["accessors"] = json!(self.accessors);
        doc["bufferViews"] = json!(self.buffer_views);
        doc["buffers"] = json!([{ "byteLength": self.bin.len() }]);
    }

    /// Push raw little-endian f32s as an accessor of `ty` ("SCALAR", "VEC3", ...).
    pub(crate) fn push_f32(&mut self, data: &[f32], ty: &str, target: Option<u32>, with_bounds: bool) -> usize {
        let width = match ty {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            _ => 1,
        };
        let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let view = self.push_view(&bytes, target);
        let mut acc = json!({ "bufferView": view, "componentType": 5126, "count": data.len() / width, "type": ty });
        if with_bounds && width <= 4 {
            let mut mn = vec![f32::MAX; width];
            let mut mx = vec![f32::MIN; width];
            for c in data.chunks_exact(width) {
                for k in 0..width {
                    mn[k] = mn[k].min(c[k]);
                    mx[k] = mx[k].max(c[k]);
                }
            }
            acc["min"] = json!(mn);
            acc["max"] = json!(mx);
        }
        self.push_accessor(acc)
    }

    pub(crate) fn push_view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        let mut v = json!({ "buffer": 0, "byteOffset": offset, "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.buffer_views.push(v);
        self.buffer_views.len() - 1
    }

    pub(crate) fn push_accessor(&mut self, a: Value) -> usize {
        self.accessors.push(a);
        self.accessors.len() - 1
    }
}

fn joint_local(j: &Joint) -> Mat4 {
    let [rx, ry, rz] = j.rotation;
    let s = Vec3::from(j.scale);
    let s = if s.length_squared() < 1e-12 { Vec3::ONE } else { s };
    Mat4::from_translation(Vec3::from(j.position))
        * Mat4::from_quat(Quat::from_euler(EulerRot::ZYX, rz, ry, rx))
        * Mat4::from_scale(s)
}

/// World (model-space) bind transform of a joint, in the game's left-handed space.
pub fn joint_world_lh(joints: &[Joint], idx: usize) -> Mat4 {
    joint_world(joints, idx)
}

fn joint_world(joints: &[Joint], mut idx: usize) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    for _ in 0..joints.len() {
        let Some(j) = joints.get(idx) else { break };
        m = joint_local(j) * m;
        if j.parent < 0 || j.parent as usize >= joints.len() {
            break;
        }
        idx = j.parent as usize;
    }
    m
}

/// Every triangle of a stage in right-handed world space, exactly as the
/// render export places it. Used to build collision.
pub fn world_triangles(a: &Alr) -> Vec<[Vec3; 3]> {
    world_triangles_skip(a, &[])
}

/// Like [`world_triangles`] but leaving out some models (stage object
/// templates, which only exist where the `.dat` places them).
pub fn world_triangles_skip(a: &Alr, skip: &[usize]) -> Vec<[Vec3; 3]> {
    let mut out = Vec::new();
    for mi in 0..a.models.len() {
        if !collision_skips_model(a, mi, skip) {
            model_triangles_each(a, mi, Mat4::IDENTITY, &mut |di, t| {
                if collision_keeps_draw(a, mi, di) {
                    out.push(t)
                }
            });
        }
    }
    out
}

/// Is model `mi` left out of the stage collision?
pub fn collision_skips_model(_a: &Alr, mi: usize, skip: &[usize]) -> bool {
    skip.contains(&mi)
}

/// Is draw `di` of model `mi` solid? Everything is, except the stage's
/// shadow volumes (COLLISION.md "Shadow volumes are not solid"), its sky
/// backdrop, and Highway's one oversized floor patch (COLLISION.md "Highway
/// / Panorama Boss far floor").
pub fn collision_keeps_draw(a: &Alr, mi: usize, di: usize) -> bool {
    let Some(model) = a.models.get(mi) else { return true };
    let Some(draw) = model.draws.get(di) else { return true };
    let format = model.vertbufs.get(draw.vertex_buf as usize).map(|v| v.format).unwrap_or(0);
    let flags = model.materials.get(draw.material_idx as usize).map(|m| m.flags).unwrap_or(0);
    if is_shadow_volume(format, flags) || is_sky_backdrop(format) {
        return false;
    }
    // Only position-only-with-flags-0x0003 draws can be an oversized patch
    // (`is_oversized_floor_patch` checks the same thing), so only bother
    // computing a bounding box — one extra pass over this draw's own
    // vertices, not the whole model — when the format matches.
    if format == 0x10 {
        if let Some((lo, hi)) = draw_world_bbox(a, mi, di) {
            if is_oversized_floor_patch(format, flags, (hi.x - lo.x, hi.z - lo.z)) {
                return false;
            }
        }
    }
    true
}

/// Stencil shadow-volume geometry: position-only vertices in the layouts of
/// the game's `SSHADOW` (0x07: position + normal) and `SSHADOWC` (0x10:
/// position) shaders, with material flag 0x2000. Never visible, and not
/// solid in the original (it walls players into their start area and makes
/// floors in mid-air if used as collision). FORMATS-STAGES.md.
pub fn is_shadow_volume(vertex_format: u8, material_flags: u16) -> bool {
    matches!(vertex_format, 0x07 | 0x10) && material_flags & 0x2000 != 0
}

/// Name of a shadow-volume node in a stage glb: `shadowvol_m<model>_d<draw>`
/// for the stage's own models, `<object node>_shadowvol_d<draw>` (a child of
/// the placed object's node, so it moves, hides and goes away with the
/// object) for volumes of a placed object. `pd_game/src/shadows.rs` finds
/// them by [`VOLUME_TAG`].
pub fn volume_node_name(object: Option<&str>, model: usize, draw: usize) -> String {
    match object {
        Some(o) => format!("{o}_{VOLUME_TAG}_d{draw}"),
        None => format!("{VOLUME_TAG}_m{model}_d{draw}"),
    }
}

/// The substring every shadow-volume node name contains.
pub const VOLUME_TAG: &str = "shadowvol";

/// `SSHADOW_vs`, one vertex (game space): a vertex whose normal faces away
/// from the light (`N . -DL < c0.x`, `c0.x = 0`, strict) moves along the
/// light's travel direction `dir` by the material's length (+0x3c); the
/// others stay.
pub fn extrude_volume_vertex(pos: Vec3, normal: Vec3, dir: Vec3, len: f32) -> Vec3 {
    if normal.dot(-dir) < 0.0 {
        pos + dir * len
    } else {
        pos
    }
}

/// Name of the stage glb node holding the stage's own shadow volumes as one
/// world-space (right-handed) triangle soup, already extruded the way the
/// program's vertex shader does it (`SSHADOW_vs`, per-material length at
/// material +0x3c; the position-only `SSHADOWC` volumes are used as stored).
/// pd_game tests pixels against it (the original's stencil semantics: a
/// pixel is in shadow when the volumes' winding number along any ray out of
/// it is non-zero) and never draws it. Not tagged [`VOLUME_TAG`].
pub const GRID_NODE: &str = "pdgrid_volumes";

/// Stage objects placed from the `.dat` are named `prop<NNN>_<name>`
/// (`stagedat::build_stage_props`); the props library (`*.props.glb`, hidden,
/// used for damaged / broken replacements) names its copies `type<NN>_<state>`
/// and gets no volumes.
fn volume_instance(name: &str) -> bool {
    name.starts_with("prop")
}

/// Vertex format 0x0B ("SVC": position + vertex colour at +12, no UVs) is
/// always the stage's sky/backdrop shell (FORMATS-ALR.md "0x0B = position +
/// colour ... the sky model's backdrop"; confirmed on every stage we
/// surveyed: st01/03..08/12/14..17/24, 0x0B draws are always a ~5,000 to
/// 27,000-unit-wide sphere/shell, never small helper geometry). Never
/// visible, and not solid: on Panorama Boss (st24) its bottom hemisphere
/// sits at y ~ -2,005 with nothing else below some start points, so a fall
/// there landed on an enormous flat "ground" far outside the arena instead
/// of respawning (COLLISION.md).
pub fn is_sky_backdrop(vertex_format: u8) -> bool {
    vertex_format == 0x0B
}

/// A position-only (0x10) draw with material flags exactly 0x0003 is
/// normally a small, legitimate collision patch that closes a real gap next
/// to ordinary stage geometry — Refinery's draws 130-171 (largest 284 x 262
/// units), Panorama's draw 14 (220 x 200), Panorama Boss's draw 13 (220 x
/// 56). Highway's model 0 draw 1 (st02/12/22) has the same format and flags
/// but a footprint of 3,123 x 2,983 units: 11x the next-largest 0x0003 draw
/// on any of the 30 stages, and far bigger than Highway's own visible
/// geometry (~1,338 x 1,484). It is a giant invisible floor far below and
/// beyond the road that lets a player walk off the edge indefinitely instead
/// of falling (COLLISION.md "Highway far floor"). `SLAB_LIMIT` sits with a
/// wide margin between the two groups.
const SLAB_LIMIT: f32 = 1000.0;
pub fn is_oversized_floor_patch(vertex_format: u8, material_flags: u16, xz_extent: (f32, f32)) -> bool {
    vertex_format == 0x10 && material_flags == 0x0003 && (xz_extent.0 > SLAB_LIMIT || xz_extent.1 > SLAB_LIMIT)
}

/// The world-space bounding box of one draw of model `mi`, placed exactly as
/// [`model_triangles_each`] places it (so it matches the real collision
/// triangles), but touching only that draw's own vertices — cheap to call
/// once per draw, unlike re-running `model_triangles_each` (which visits
/// every draw of the model) for each one.
fn draw_world_bbox(a: &Alr, mi: usize, di: usize) -> Option<(Vec3, Vec3)> {
    let model = a.models.get(mi)?;
    let draw = model.draws.get(di)?;
    let vb = model.vertbufs.get(draw.vertex_buf as usize)?;
    let verts: Vec<Vec3> = a.read_vertices(vb).ok()?.iter().map(|v| Vec3::from(v.pos)).collect();
    let m = if model.joints.is_empty() { to_rh(Mat4::IDENTITY) } else { to_rh(Mat4::IDENTITY) * to_rh(joint_world(&model.joints, draw.transform_idx as usize)) };
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut any = false;
    for t in alr::triangulate(draw) {
        if t.iter().any(|&i| i as usize >= verts.len()) {
            continue;
        }
        any = true;
        for i in t {
            let v = verts[i as usize];
            let p = m.transform_point3(Vec3::new(v.x, v.y, -v.z));
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    any.then_some((lo, hi))
}

/// Every triangle of one model placed with `world_lh` (left-handed game
/// space), appended in right-handed space.
pub fn model_triangles(a: &Alr, mi: usize, world_lh: Mat4, out: &mut Vec<[Vec3; 3]>) {
    model_triangles_each(a, mi, world_lh, &mut |_, t| out.push(t));
}

/// Like [`model_triangles`], calling `f(draw index, triangle)` per triangle
/// (diagnostics: which draw a blocking triangle came from).
pub fn model_triangles_each(a: &Alr, mi: usize, world_lh: Mat4, f: &mut dyn FnMut(usize, [Vec3; 3])) {
    let Some(model) = a.models.get(mi) else { return };
    let place = to_rh(world_lh);
    let mut cache: HashMap<u16, Vec<Vec3>> = HashMap::new();
    for (di, draw) in model.draws.iter().enumerate() {
        let Some(vb) = model.vertbufs.get(draw.vertex_buf as usize) else { continue };
        let verts = cache
            .entry(draw.vertex_buf)
            .or_insert_with(|| a.read_vertices(vb).map(|v| v.iter().map(|v| Vec3::from(v.pos)).collect()).unwrap_or_default());
        let m = if model.joints.is_empty() { place } else { place * to_rh(joint_world(&model.joints, draw.transform_idx as usize)) };
        for t in alr::triangulate(draw) {
            if t.iter().any(|&i| i as usize >= verts.len()) {
                continue;
            }
            f(
                di,
                t.map(|i| {
                    let v = verts[i as usize];
                    m.transform_point3(Vec3::new(v.x, v.y, -v.z))
                }),
            );
        }
    }
}

/// Mirror a left-handed matrix into right-handed space (Z flip on both sides).
pub(crate) fn to_rh(m: Mat4) -> Mat4 {
    let s = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
    s * m * s
}

/// Weighted-blend a skinned/palette draw's vertices (`alr::VERTFMT_CHAR_SKIN`
/// / `alr::VERTFMT_EFFECT_SKIN`) across its bone palette, the same way the
/// game's vertex shader would: each vertex's exported position is the
/// weighted sum of its up-to-two palette joints' bind-pose transforms
/// applied to its local position (falling back to the untransformed local
/// position if neither register resolves to a palette joint, e.g. a
/// zero-weight slot). This is the effect-export equivalent of what
/// `character.rs` already does per-vertex for a character body (there it
/// only needs the *dominant* joint, to classify vertices for the cloth rig;
/// here we need the actual blended position to place the mesh).
///
/// Established on `eft0600.alr` ("aura" glow, docs/FORMATS-EFFECTS.md,
/// docs/CAPSULE-VISUALS.md "Probe results 2026-09-28" lead 1): its four
/// draws each reference a private 7-joint chain whose bind pose is all
/// identity, so blending is a no-op there and the draw's own placement
/// joint (`draw.transform_idx`, applied afterwards as the node matrix, same
/// as any other piece) does all the work — but the blend is computed in
/// general so a palette with real bind offsets is handled correctly too.
fn skin_blended_vertices(a: &Alr, model: &alr::Model, vb: &alr::VertBufEntry, palette: &[u8]) -> Result<Vec<alr::StdVertex>> {
    let sk = alr::read_skin_vertices(a, vb)?;
    let world = crate::pose::world_matrices(model, &crate::pose::bind_locals(model), crate::pose::Order::GAME);
    Ok(sk
        .into_iter()
        .map(|v| {
            let p = Vec3::from(v.pos);
            let mut acc = Vec3::ZERO;
            let mut total = 0.0f32;
            for k in 0..2 {
                if v.weights[k] <= 0.0 {
                    continue;
                }
                if let Some(m) = alr::palette_joint(palette, v.regs[k]).and_then(|j| world.get(j as usize)) {
                    acc += m.transform_point3(p) * v.weights[k];
                    total += v.weights[k];
                }
            }
            let pos = if total > 0.0 { acc / total } else { p };
            alr::StdVertex { pos: pos.to_array(), uv: Some(v.uv) }
        })
        .collect())
}

pub fn export_glb(a: &Alr, opts: &ExportOptions) -> Result<(Vec<u8>, ExportStats)> {
    let mut b = Builder::default();
    let mut stats = ExportStats { bounds_min: [f32::MAX; 3], bounds_max: [f32::MIN; 3], ..Default::default() };
    let mut mats = Materials::default();

    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    // Top-level nodes (children of "root") and, per instance, its draw nodes.
    let mut top: Vec<usize> = Vec::new();
    let mut inst_children: Vec<Vec<usize>> = vec![Vec::new(); opts.instances.len()];
    // Shadow-volume nodes per placed object (`export_shadow_volumes`); only
    // attached when the object has drawn geometry of its own.
    let mut inst_volumes: Vec<Vec<usize>> = vec![Vec::new(); opts.instances.len()];
    // The stage's own volumes, extruded, in world space (right-handed): see `GRID_NODE`.
    let mut grid_tris: Vec<[Vec3; 3]> = Vec::new();

    for (mi, model) in a.models.iter().enumerate() {
        if model.draws.is_empty() {
            continue;
        }
        let placed = !opts.skip_models.contains(&mi);
        let has_instances = opts.instances.iter().any(|i| i.model == mi);
        if !placed && !has_instances {
            continue;
        }
        stats.models += 1;
        // One position/uv accessor pair per vertex buffer, shared by its
        // draws — except a skinned/palette draw (second key component != 0,
        // keyed by draw index so it never collides), which gets its own:
        // its positions depend on its own bone palette, not just its vertex
        // buffer (see `skin_blended_vertices`).
        let mut vb_accessors: HashMap<(u16, u16), (usize, Option<usize>, usize)> = HashMap::new();
        // Lighting inputs per vertex buffer (only with `opts.lighting`).
        let mut vb_light: HashMap<u16, Vec<alr::VertexLight>> = HashMap::new();
        // Colour-only draws already exported (the sky backdrop is drawn twice
        // in the file with two materials; once is enough for us).
        let mut seen_colour_only: Vec<(u16, &[u16])> = Vec::new();
        // Baked colour accessors by (vertex buffer, quantised rotation).
        let mut colour_cache: HashMap<(u16, [i32; 9]), usize> = HashMap::new();
        let sky = mi == SKY_MODEL;
        // Effect vertex colours (`opts.vertex_colours`): accessor by vertex buffer.
        let mut vb_vcolour: HashMap<u16, Option<usize>> = HashMap::new();
        // Bump-mapped stage draws: (normal, tangent, mask uv) accessors by vertex buffer.
        let mut vb_bump: HashMap<u16, (usize, usize, usize)> = HashMap::new();
        for (di, draw) in model.draws.iter().enumerate() {
            let Some(vb) = model.vertbufs.get(draw.vertex_buf as usize) else { continue };
            // Skinned/palette formats (character body parts, and a few
            // effect pieces that reuse the same layout under a different
            // format id: `alr::VERTFMT_EFFECT_SKIN`, docs/FORMATS-EFFECTS.md)
            // need their vertices weighted-blended across the draw's own
            // bone palette rather than read at face value.
            let skinned = matches!(vb.format, alr::VERTFMT_CHAR_SKIN | alr::VERTFMT_EFFECT_SKIN) && !draw.palette.is_empty();
            let vkey: (u16, u16) = (draw.vertex_buf, if skinned { di as u16 + 1 } else { 0 });
            if !vb_accessors.contains_key(&vkey) {
                let Ok(verts) = (if skinned { skin_blended_vertices(a, model, vb, &draw.palette) } else { a.read_vertices(vb) }) else { continue };
                if verts.is_empty() {
                    continue;
                }
                let mut pos = Vec::with_capacity(verts.len() * 12);
                let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
                for v in &verts {
                    let p = [v.pos[0], v.pos[1], -v.pos[2]];
                    for k in 0..3 {
                        mn[k] = mn[k].min(p[k]);
                        mx[k] = mx[k].max(p[k]);
                        pos.extend_from_slice(&p[k].to_le_bytes());
                    }
                }
                let pv = b.push_view(&pos, Some(34962));
                let pa = b.push_accessor(json!({ "bufferView": pv, "componentType": 5126, "count": verts.len(), "type": "VEC3", "min": mn, "max": mx }));
                let ua = if verts[0].uv.is_some() {
                    let mut uv = Vec::with_capacity(verts.len() * 8);
                    for v in &verts {
                        let t = v.uv.unwrap_or([0.0, 0.0]);
                        uv.extend_from_slice(&t[0].to_le_bytes());
                        uv.extend_from_slice(&t[1].to_le_bytes());
                    }
                    let uvv = b.push_view(&uv, Some(34962));
                    Some(b.push_accessor(json!({ "bufferView": uvv, "componentType": 5126, "count": verts.len(), "type": "VEC2" })))
                } else {
                    None
                };
                vb_accessors.insert(vkey, (pa, ua, verts.len()));
                if !skinned && opts.lighting.is_some() {
                    vb_light.insert(draw.vertex_buf, a.read_vertex_light(vb).unwrap_or_default());
                }
            }
            let (pa, ua, vcount) = vb_accessors[&vkey];
            let light = opts.lighting.as_ref().and_then(|set| vb_light.get(&draw.vertex_buf).filter(|l| l.len() == vcount).map(|l| (set, l)));
            // Shadow volumes (the original's stencil volumes, material flag
            // 0x2000): never drawn, but exported as their own named nodes so
            // the game can use them as shadow casters (pd_game `shadows.rs`).
            // Stage exports only (`lighting`), and they stay out of the bounds
            // and statistics so nothing else (camera framing, spawn picking)
            // changes. See `volume_node_name`.
            let material_flags = model.materials.get(draw.material_idx as usize).map_or(0, |m| m.flags);
            if opts.lighting.is_some() && is_shadow_volume(vb.format, material_flags) {
                let tris: Vec<[u16; 3]> = alr::triangulate(draw).into_iter().filter(|t| t.iter().all(|&i| (i as usize) < vcount)).collect();
                if !tris.is_empty() {
                    let mut idx = Vec::with_capacity(tris.len() * 12);
                    for t in &tris {
                        for i in [t[0], t[2], t[1]] {
                            idx.extend_from_slice(&(i as u32).to_le_bytes());
                        }
                    }
                    let iv = b.push_view(&idx, Some(34963));
                    let ia = b.push_accessor(json!({ "bufferView": iv, "componentType": 5125, "count": tris.len() * 3, "type": "SCALAR" }));
                    let mat = mats.get_ex(&mut b, a, None, false, &mut stats);
                    meshes.push(json!({ "primitives": [{ "attributes": { "POSITION": pa }, "indices": ia, "material": mat }] }));
                    let mesh = meshes.len() - 1;
                    let local = (opts.apply_transforms && !model.joints.is_empty()).then(|| to_rh(joint_world(&model.joints, draw.transform_idx as usize)));
                    let node_for = |name: String| {
                        let mut node = json!({ "mesh": mesh, "name": name });
                        if let Some(m) = local {
                            node["matrix"] = json!(m.to_cols_array());
                        }
                        node
                    };
                    if placed {
                        nodes.push(node_for(volume_node_name(None, mi, di)));
                        top.push(nodes.len() - 1);
                    }
                    for (k, inst) in opts.instances.iter().enumerate().filter(|(_, i)| i.model == mi && volume_instance(&i.name)) {
                        nodes.push(node_for(volume_node_name(Some(&inst.name), mi, di)));
                        inst_volumes[k].push(nodes.len() - 1);
                    }
                }
                // The stage's own volumes also go, extruded exactly as the program's
                // vertex shader does, into the world-space triangle soup the game tests
                // pixels against (`GRID_NODE`).
                if placed {
                    if let (Some((set, l)), Ok(verts)) = (light, a.read_vertices(vb)) {
                        let joint = if opts.apply_transforms && !model.joints.is_empty() { joint_world(&model.joints, draw.transform_idx as usize) } else { Mat4::IDENTITY };
                        let mat_e = model.materials.get(draw.material_idx as usize).map_or(0.0, |m| m.extrude);
                        let dir = Vec3::from(set.dir).normalize_or_zero();
                        let world = |i: u16| -> Vec3 {
                            let p = joint.transform_point3(Vec3::from(verts[i as usize].pos));
                            let n = l[i as usize].normal.map(|n| joint.transform_vector3(Vec3::from(n)));
                            let p = if vb.format == 0x07 { extrude_volume_vertex(p, n.unwrap_or(Vec3::ZERO), dir, mat_e) } else { p };
                            Vec3::new(p.x, p.y, -p.z)
                        };
                        for t in alr::triangulate(draw).into_iter().filter(|t| t.iter().all(|&i| (i as usize) < vcount)) {
                            grid_tris.push([world(t[0]), world(t[2]), world(t[1])]);
                        }
                    }
                }
                continue;
            }
            // Vertex-coloured, untextured geometry (the `SVC` shader: the
            // sky model's backdrop box) is drawn when lighting is baked.
            let colour_only = ua.is_none() && light.is_some_and(|(_, l)| l.first().is_some_and(|v| v.color.is_some() && v.normal.is_none()));
            // Sky model, material flag 0x10: an inner ellipsoid shell (radius
            // ~6,100, inside the ~11,700 cloud dome; vertex-coloured white /
            // yellow in st03, black in st23, textured near-black in st13)
            // drawn with a blend we haven't identified ❓. Drawn opaque it
            // hides the whole sky, so with lighting on it is left out. (The
            // cloud dome also has bit 0x10 but carries normals: kept.)
            let no_normals = light.is_some_and(|(_, l)| l.first().is_some_and(|v| v.normal.is_none()));
            if sky && no_normals && model.materials.get(draw.material_idx as usize).is_some_and(|m| m.flags & 0x10 != 0) {
                continue;
            }
            if colour_only {
                if seen_colour_only.iter().any(|(v, i)| *v == draw.vertex_buf && *i == draw.indices.as_slice()) {
                    continue;
                }
                seen_colour_only.push((draw.vertex_buf, draw.indices.as_slice()));
            }
            // Position-only buffers (no UVs) are never textured in game:
            // collision, shadow and helper geometry. Keep them out of the
            // render; `world_triangles` still includes them for collision.
            if ua.is_none() && !opts.include_untextured && !colour_only {
                continue;
            }

            // Triangles, with winding flipped for the Z mirror.
            let tris: Vec<[u16; 3]> = alr::triangulate(draw)
                .into_iter()
                .filter(|t| t.iter().all(|&i| (i as usize) < vcount))
                .collect();
            if tris.is_empty() {
                continue;
            }
            let mut idx = Vec::with_capacity(tris.len() * 12);
            for t in &tris {
                for i in [t[0], t[2], t[1]] {
                    idx.extend_from_slice(&(i as u32).to_le_bytes());
                }
            }
            let iv = b.push_view(&idx, Some(34963));
            let ia = b.push_accessor(json!({ "bufferView": iv, "componentType": 5125, "count": tris.len() * 3, "type": "SCALAR" }));

            let tex_idx = if colour_only { None } else { Some(model.materials.get(draw.material_idx as usize).map(|m| m.texture_idx as usize).unwrap_or(usize::MAX)) };
            // The sky dome fades out by vertex alpha into the (opaque)
            // backdrop box behind it: blend the sky draws that have
            // vertex alpha below 255 (only one blended layer, so no sorting).
            let fades = light.is_some_and(|(_, l)| l.iter().any(|v| v.color.is_some_and(|c| c[3] < 255)));
            // Bump-mapped stage draws (`SBTS`): the material also carries the
            // normal map and the per-draw specular constants, the mesh the
            // frame (NORMAL / TANGENT) and the per-vertex mask (TEXCOORD_1).
            let bump = (opts.lighting.is_some() && !sky && !colour_only)
                .then(|| model.materials.get(draw.material_idx as usize))
                .flatten()
                .filter(|m| is_stage_bump(vb.format, m.flags) && ua.is_some() && (m.normal_idx as usize) < a.textures.len() && (m.texture_idx as usize) < a.textures.len());
            let mat = match bump {
                Some(m) => mats.get_stage_bump(&mut b, a, m, &mut stats),
                None => mats.get_ex(&mut b, a, tex_idx, sky && fades, &mut stats),
            };

            let mut attrs = json!({ "POSITION": pa });
            if let Some(u) = ua {
                attrs["TEXCOORD_0"] = json!(u);
            }
            if bump.is_some() {
                let acc = match vb_bump.get(&draw.vertex_buf) {
                    Some(&acc) => Some(acc),
                    None => a.read_capsule_vertices(vb).ok().filter(|v| v.len() == vcount && v.iter().all(|v| v.normal.is_some() && v.tangent.is_some() && v.color.is_some())).map(|verts| {
                        let (mut nrm, mut tan, mut uv1) = (Vec::with_capacity(verts.len() * 12), Vec::with_capacity(verts.len() * 16), Vec::with_capacity(verts.len() * 8));
                        for v in &verts {
                            let (n, t, c) = (v.normal.unwrap(), v.tangent.unwrap(), v.color.unwrap());
                            // Game space is left-handed: mirror Z like the positions. The
                            // authored frames have B = -(N x T) in the game space, so
                            // after the mirror B = cross(N, T): glTF handedness w = +1
                            // on every vertex (checked over st03/05/06/07: 0 exceptions).
                            for x in [n[0], n[1], -n[2]] {
                                nrm.extend_from_slice(&x.to_le_bytes());
                            }
                            for x in [t[0], t[1], -t[2], 1.0] {
                                tan.extend_from_slice(&x.to_le_bytes());
                            }
                            // COLOR0.x = the ambient-occlusion style multiplier the pixel
                            // shader applies, COLOR0.w = the specular weight: raw bytes / 255
                            // (the shader works in gamma space).
                            for x in [c[0] as f32 / 255.0, c[3] as f32 / 255.0] {
                                uv1.extend_from_slice(&x.to_le_bytes());
                            }
                        }
                        let nv = b.push_view(&nrm, Some(34962));
                        let tv = b.push_view(&tan, Some(34962));
                        let uv = b.push_view(&uv1, Some(34962));
                        (
                            b.push_accessor(json!({ "bufferView": nv, "componentType": 5126, "count": verts.len(), "type": "VEC3" })),
                            b.push_accessor(json!({ "bufferView": tv, "componentType": 5126, "count": verts.len(), "type": "VEC4" })),
                            b.push_accessor(json!({ "bufferView": uv, "componentType": 5126, "count": verts.len(), "type": "VEC2" })),
                        )
                    }),
                };
                if let Some((n, t, u1)) = acc {
                    vb_bump.insert(draw.vertex_buf, (n, t, u1));
                    attrs["NORMAL"] = json!(n);
                    attrs["TANGENT"] = json!(t);
                    attrs["TEXCOORD_1"] = json!(u1);
                }
            }
            if opts.vertex_colours && !skinned && light.is_none() {
                let acc = *vb_vcolour.entry(draw.vertex_buf).or_insert_with(|| {
                    let l = a.read_vertex_light(vb).ok().filter(|l| l.len() == vcount)?;
                    if !l.iter().any(|v| v.color.is_some_and(|c| c != [255; 4])) {
                        return None;
                    }
                    let cols: Vec<f32> = l
                        .iter()
                        .flat_map(|v| {
                            let k = v.color.unwrap_or([255; 4]);
                            [gamma_to_linear(k[0] as f32 / 255.0), gamma_to_linear(k[1] as f32 / 255.0), gamma_to_linear(k[2] as f32 / 255.0), k[3] as f32 / 255.0]
                        })
                        .collect();
                    Some(b.push_f32(&cols, "VEC4", Some(34962), false))
                });
                if let Some(acc) = acc {
                    attrs["COLOR_0"] = json!(acc);
                }
            }
            let joint_lh = (opts.apply_transforms && !model.joints.is_empty()).then(|| joint_world(&model.joints, draw.transform_idx as usize));
            // Mesh for one placement: baked colours depend on where the
            // copy faces, so lit copies get their own COLOR_0.
            let mut mesh_for = |b: &mut Builder, meshes: &mut Vec<Value>, world_lh: Mat4| -> usize {
                let mut at = attrs.clone();
                if let Some((set, l)) = light {
                    // One colour accessor per (vertex buffer, rotation): the
                    // stage's draws share a few big buffers and joints, and
                    // one accessor per draw made glbs of 300-800 MB that
                    // loaded too slowly (st07 black, st08 patches).
                    let rot = glam::Mat3::from_mat4(world_lh);
                    let key = (draw.vertex_buf, rot.to_cols_array().map(|x| (x * 1e4).round() as i32));
                    if let Some(&acc) = colour_cache.get(&key) {
                        at["COLOR_0"] = json!(acc);
                        meshes.push(json!({ "primitives": [{ "attributes": at, "indices": ia, "material": mat }] }));
                        return meshes.len() - 1;
                    }
                    let cols: Vec<f32> = if sky {
                        // The sky keeps its own vertex colours and alpha (the
                        // script's sky light set is not applied ❓).
                        l.iter()
                            .flat_map(|v| {
                                let k = v.color.unwrap_or([255; 4]);
                                [gamma_to_linear(k[0] as f32 / 255.0), gamma_to_linear(k[1] as f32 / 255.0), gamma_to_linear(k[2] as f32 / 255.0), k[3] as f32 / 255.0]
                            })
                            .collect()
                    } else {
                        light_vertices(l, set, rot, false).into_iter().flatten().collect()
                    };
                    let acc = b.push_f32(&cols, "VEC4", Some(34962), false);
                    colour_cache.insert(key, acc);
                    at["COLOR_0"] = json!(acc);
                }
                meshes.push(json!({ "primitives": [{ "attributes": at, "indices": ia, "material": mat }] }));
                meshes.len() - 1
            };
            let local = joint_lh.map(to_rh);
            let mut shared: Option<usize> = None;

            if placed {
                let mesh = mesh_for(&mut b, &mut meshes, joint_lh.unwrap_or(Mat4::IDENTITY));
                if light.is_none() {
                    shared = Some(mesh);
                }
                let mut node = json!({ "mesh": mesh, "name": format!("m{mi}_d{di}") });
                if let Some(m) = local {
                    node["matrix"] = json!(m.to_cols_array());
                    track_bounds(&mut stats, m, &b.accessors[pa]);
                } else {
                    track_bounds(&mut stats, Mat4::IDENTITY, &b.accessors[pa]);
                }
                nodes.push(node);
                top.push(nodes.len() - 1);
                stats.draws += 1;
                stats.triangles += tris.len();
            }
            // Placed copies share one mesh (lit copies get their own). They
            // count towards the bounds: they stand in the playable area,
            // which keeps the "core" percentile box on the arena now that
            // the object templates no longer sit at the origin.
            for (k, inst) in opts.instances.iter().enumerate().filter(|(_, i)| i.model == mi) {
                let mesh = if light.is_some() {
                    mesh_for(&mut b, &mut meshes, inst.world_lh * joint_lh.unwrap_or(Mat4::IDENTITY))
                } else {
                    *shared.get_or_insert_with(|| mesh_for(&mut b, &mut meshes, Mat4::IDENTITY))
                };
                let mut node = json!({ "mesh": mesh, "name": format!("{}_d{di}", inst.name) });
                if let Some(m) = local {
                    node["matrix"] = json!(m.to_cols_array());
                }
                track_bounds(&mut stats, to_rh(inst.world_lh) * local.unwrap_or(Mat4::IDENTITY), &b.accessors[pa]);
                nodes.push(node);
                inst_children[k].push(nodes.len() - 1);
                stats.draws += 1;
                stats.triangles += tris.len();
            }
        }
    }

    for (k, inst) in opts.instances.iter().enumerate() {
        if inst_children[k].is_empty() {
            continue;
        }
        let mut children = std::mem::take(&mut inst_children[k]);
        children.append(&mut inst_volumes[k]);
        nodes.push(json!({ "name": inst.name, "matrix": to_rh(inst.world_lh).to_cols_array(), "children": children }));
        top.push(nodes.len() - 1);
        stats.instances += 1;
    }

    // One extra mesh with every static volume triangle, extruded: the game
    // tests pixels against it (stencil-volume semantics) and never draws it.
    if !grid_tris.is_empty() {
        let mut pos = Vec::with_capacity(grid_tris.len() * 36);
        let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
        for t in &grid_tris {
            for p in t {
                for (k, c) in [p.x, p.y, p.z].into_iter().enumerate() {
                    mn[k] = mn[k].min(c);
                    mx[k] = mx[k].max(c);
                    pos.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        let n = grid_tris.len() * 3;
        let idx: Vec<u8> = (0..n as u32).flat_map(|i| i.to_le_bytes()).collect();
        let pv = b.push_view(&pos, Some(34962));
        let pa = b.push_accessor(json!({ "bufferView": pv, "componentType": 5126, "count": n, "type": "VEC3", "min": mn, "max": mx }));
        let iv = b.push_view(&idx, Some(34963));
        let ia = b.push_accessor(json!({ "bufferView": iv, "componentType": 5125, "count": n, "type": "SCALAR" }));
        let mat = mats.get_ex(&mut b, a, None, false, &mut stats);
        meshes.push(json!({ "primitives": [{ "attributes": { "POSITION": pa }, "indices": ia, "material": mat }] }));
        nodes.push(json!({ "mesh": meshes.len() - 1, "name": GRID_NODE }));
        top.push(nodes.len() - 1);
    }

    stats.finish();
    nodes.push(json!({ "name": "root", "children": top }));
    let root = nodes.len() - 1;

    let mut doc = json!({
        "asset": { "version": "2.0", "generator": "pd_formats" },
        "scene": 0,
        "scenes": [{ "nodes": [root] }],
        "nodes": nodes,
        "meshes": meshes,
    });
    mats.write_into(&mut doc);
    b.write_into(&mut doc);
    Ok((write_glb(&doc, &b.bin), stats))
}

/// Unlit materials keyed by ALR texture, with textures embedded as PNG.
#[derive(Default)]
pub(crate) struct Materials {
    images: Vec<Value>,
    textures: Vec<Value>,
    tex_map: HashMap<usize, Option<(usize, bool)>>,
    materials: Vec<Value>,
    mat_map: HashMap<(usize, bool), usize>,
    lit_map: HashMap<(usize, Option<usize>), usize>,
    /// Stage bump materials (`get_stage_bump`).
    bump_map: HashMap<(usize, usize, [u8; 4], i16, u16), usize>,
    /// Untextured / blended stage materials (`get_ex`).
    special_map: HashMap<(usize, u8), usize>,
}

impl Materials {
    /// Like [`Self::get`], with `None` = untextured white (vertex colour
    /// only) and `blend` = alpha-blended (sky dome fading by vertex alpha).
    pub(crate) fn get_ex(&mut self, b: &mut Builder, a: &Alr, tex_idx: Option<usize>, blend: bool, stats: &mut ExportStats) -> usize {
        if let (Some(t), false) = (tex_idx, blend) {
            return self.get(b, a, t, stats);
        }
        // Decode (or reuse) the texture through `get`'s cache.
        let tex = tex_idx.and_then(|t| {
            self.get(b, a, t, stats);
            self.tex_map.get(&t).copied().flatten()
        });
        let key = (tex.map_or(usize::MAX, |t| t.0), blend as u8);
        if let Some(&m) = self.special_map.get(&key) {
            return m;
        }
        let mut m = json!({
            "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "doubleSided": true,
            "extensions": { "KHR_materials_unlit": {} },
        });
        if let Some((t, _)) = tex {
            m["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": t });
        }
        if blend {
            m["alphaMode"] = json!("BLEND");
        }
        self.materials.push(m);
        self.special_map.insert(key, self.materials.len() - 1);
        self.materials.len() - 1
    }

    pub(crate) fn get(&mut self, b: &mut Builder, a: &Alr, tex_idx: usize, stats: &mut ExportStats) -> usize {
        let tex = match self.tex_map.get(&tex_idx) {
            Some(t) => *t,
            None => {
                let t = a.textures.get(tex_idx).and_then(|t| match texture::decode(a, t).and_then(|img| Ok((img.to_png()?, img.has_alpha()))) {
                    Ok((png, alpha)) => {
                        stats.textures_ok += 1;
                        let view = b.push_view(&png, None);
                        self.images.push(json!({ "bufferView": view, "mimeType": "image/png" }));
                        self.textures.push(json!({ "source": self.images.len() - 1, "sampler": 0 }));
                        Some((self.textures.len() - 1, alpha))
                    }
                    Err(_) => {
                        stats.textures_failed += 1;
                        None
                    }
                });
                self.tex_map.insert(tex_idx, t);
                t
            }
        };
        let key = tex.unwrap_or((usize::MAX, false));
        if let Some(&m) = self.mat_map.get(&key) {
            return m;
        }
        let mut m = json!({
            "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "doubleSided": true,
            "extensions": { "KHR_materials_unlit": {} },
        });
        if let Some((t, alpha)) = tex {
            m["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": t });
            if alpha {
                m["alphaMode"] = json!("MASK");
                m["alphaCutoff"] = json!(0.5);
            }
        } else {
            m["pbrMetallicRoughness"]["baseColorFactor"] = json!([0.8, 0.2, 0.8, 1.0]);
        }
        self.materials.push(m);
        self.mat_map.insert(key, self.materials.len() - 1);
        self.materials.len() - 1
    }

    /// Stage bump material (`SBTS`): the unlit base texture (so a build that
    /// ignores the extras still draws today's picture), plus the stage normal
    /// map as the glTF `normalTexture` (kept as stored, alpha = the specular
    /// mask) and `extras.pd_stage` carrying the per-draw constants the pixel
    /// shader reads: the specular colour bytes (R, G, B, power; zero when
    /// material flag 0x04 is clear: `FUN_1402a75e0`), the +0x40 short, flags.
    pub(crate) fn get_stage_bump(&mut self, b: &mut Builder, a: &Alr, m: &alr::MaterialEntry, stats: &mut ExportStats) -> usize {
        let key = (m.texture_idx as usize, m.normal_idx as usize, m.spec_bgra, m.fresnel, m.flags);
        if let Some(&i) = self.bump_map.get(&key) {
            return i;
        }
        let base = self.texture(b, a, m.texture_idx as usize, stats, false);
        let normal = self.texture(b, a, m.normal_idx as usize, stats, true);
        let mut mat = json!({
            "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "doubleSided": true,
            "extensions": { "KHR_materials_unlit": {} },
        });
        if let Some((t, alpha)) = base {
            mat["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": t });
            if alpha {
                mat["alphaMode"] = json!("MASK");
                mat["alphaCutoff"] = json!(0.5);
            }
        } else {
            mat["pbrMetallicRoughness"]["baseColorFactor"] = json!([0.8, 0.2, 0.8, 1.0]);
        }
        if let Some((t, _)) = normal {
            mat["normalTexture"] = json!({ "index": t });
        }
        let spec = if m.flags & 0x04 != 0 { [m.spec_bgra[2], m.spec_bgra[1], m.spec_bgra[0], m.spec_bgra[3]] } else { [0, 0, 0, 0] };
        mat["extras"] = json!({ "pd_stage": { "spec": spec, "fresnel": m.fresnel, "flags": m.flags } });
        self.materials.push(mat);
        self.bump_map.insert(key, self.materials.len() - 1);
        self.materials.len() - 1
    }

    /// Lit material (characters): base colour + the game's object-space
    /// normal map, used through a bind-axes tangent frame (see
    /// `character::light_tangent`). The map is exported as the game stores it
    /// (no channel flip: the Z mirror cancels the shader's own Z sign, see
    /// `SWBOS_vs` cbase[94]) and its alpha -- the specular mask `SWBOS_ps`
    /// multiplies in -- is kept.
    pub(crate) fn get_lit(&mut self, b: &mut Builder, a: &Alr, tex_idx: usize, normal_idx: Option<usize>, stats: &mut ExportStats) -> usize {
        if let Some(&m) = self.lit_map.get(&(tex_idx, normal_idx)) {
            return m;
        }
        let base = self.texture(b, a, tex_idx, stats, false);
        let normal = normal_idx.and_then(|n| self.texture(b, a, n, stats, true));
        // Single-sided (back faces culled): character models carry inner
        // linings as separate faces lying on the outer shell with the
        // opposite winding (pc00a's jacket: 90 of 104 lining faces point
        // inward, 178 of 202 textured faces outward), so drawing both sides
        // shows the flat lining colour through the jacket (FORMATS-ALR.md
        // "Characters: one-sided faces"). PD_CHAR_DOUBLE_SIDED=1 = old look.
        let mut m = json!({
            "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.85 },
            "doubleSided": std::env::var_os("PD_CHAR_DOUBLE_SIDED").is_some(),
        });
        if let Some((t, alpha)) = base {
            m["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": t });
            if alpha {
                m["alphaMode"] = json!("MASK");
                m["alphaCutoff"] = json!(0.5);
            }
        }
        if let Some((t, _)) = normal {
            m["normalTexture"] = json!({ "index": t });
        }
        self.materials.push(m);
        self.lit_map.insert((tex_idx, normal_idx), self.materials.len() - 1);
        self.materials.len() - 1
    }

    /// Two-texture material with **no** normal-map flip on the second slot
    /// (unlike [`Self::get_lit`]): the idle-orb sphere's "bump map" isn't a
    /// standard tangent-space normal map consumed by ordinary PBR shading --
    /// it's a plain noise texture sampled twice at independent UV offsets
    /// and combined by a bespoke formula (docs/CAPSULE-VISUALS.md
    /// `vkCAP02M2`) -- so flipping its blue channel the way a real normal
    /// map needs would corrupt it. The glTF `normalTexture` slot is reused
    /// purely as a second-texture carrier; `pd_game`'s own shader reads both
    /// textures directly and never runs Bevy's standard normal-mapped PBR
    /// path against this material (the mesh is hidden and rebuilt with a
    /// custom material -- see `fx.rs`).
    pub(crate) fn get_dual(&mut self, b: &mut Builder, a: &Alr, tex_idx: usize, tex2_idx: Option<usize>, stats: &mut ExportStats) -> usize {
        let key = (tex_idx, tex2_idx);
        if let Some(&m) = self.lit_map.get(&key) {
            return m;
        }
        let base = self.texture(b, a, tex_idx, stats, false);
        let normal = tex2_idx.and_then(|n| self.texture(b, a, n, stats, false));
        let mut m = json!({
            "pbrMetallicRoughness": { "baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0 },
            "doubleSided": false,
        });
        if let Some((t, alpha)) = base {
            m["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": t });
            if alpha {
                m["alphaMode"] = json!("MASK");
                m["alphaCutoff"] = json!(0.5);
            }
        }
        if let Some((t, _)) = normal {
            m["normalTexture"] = json!({ "index": t });
        }
        self.materials.push(m);
        self.lit_map.insert(key, self.materials.len() - 1);
        self.materials.len() - 1
    }

    /// The glTF texture index an ALR texture resolved to, if it's been
    /// decoded yet (by [`Self::get`], [`Self::get_lit`] or
    /// [`Self::get_dual`]) -- for recording which texture a capsule draw
    /// actually used (`export_capsule_slot_glb`), since `pd_game`'s per-kind
    /// shading table (docs/CAPSULE-VISUALS.md's per-kind recipe) needs to
    /// know the resolved texture, not just the ALR index.
    pub(crate) fn resolved_tex_index(&self, alr_tex_idx: usize) -> Option<usize> {
        self.tex_map.get(&alr_tex_idx).copied().flatten().map(|(t, _)| t)
    }

    /// Decode + embed one ALR texture (cached). `as_normal`: an object-space
    /// normal map (kept as stored, alpha included; never treated as cutout).
    fn texture(&mut self, b: &mut Builder, a: &Alr, idx: usize, stats: &mut ExportStats, as_normal: bool) -> Option<(usize, bool)> {
        let key = if as_normal { idx | (1 << 40) } else { idx };
        if let Some(t) = self.tex_map.get(&key) {
            return *t;
        }
        let t = a.textures.get(idx).and_then(|t| {
            // Object-space normal maps are kept exactly as stored, alpha
            // included (the specular mask); `as_normal` only keys the cache
            // apart from the colour textures.
            let img = texture::decode(a, t).ok()?;
            let alpha = !as_normal && img.has_alpha();
            let png = img.to_png().ok()?;
            stats.textures_ok += 1;
            let view = b.push_view(&png, None);
            self.images.push(json!({ "bufferView": view, "mimeType": "image/png" }));
            self.textures.push(json!({ "source": self.images.len() - 1, "sampler": 0 }));
            Some((self.textures.len() - 1, alpha))
        });
        self.tex_map.insert(key, t);
        t
    }

    pub(crate) fn write_into(&self, doc: &mut Value) {
        doc["extensionsUsed"] = json!(["KHR_materials_unlit"]);
        doc["materials"] = json!(self.materials);
        if !self.images.is_empty() {
            doc["images"] = json!(self.images);
            doc["textures"] = json!(self.textures);
            doc["samplers"] = json!([{ "magFilter": 9729, "minFilter": 9729, "wrapS": 10497, "wrapT": 10497 }]);
        }
    }
}

fn track_bounds(stats: &mut ExportStats, m: Mat4, accessor: &Value) {
    let get = |k: &str| -> Vec3 {
        let a = &accessor[k];
        Vec3::new(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, a[2].as_f64().unwrap_or(0.0) as f32)
    };
    let (mn, mx) = (get("min"), get("max"));
    let (mut bmin, mut bmax) = (Vec3::MAX, Vec3::MIN);
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { mn.x } else { mx.x },
            if i & 2 == 0 { mn.y } else { mx.y },
            if i & 4 == 0 { mn.z } else { mx.z },
        );
        let p = m.transform_point3(c);
        bmin = bmin.min(p);
        bmax = bmax.max(p);
        for k in 0..3 {
            stats.bounds_min[k] = stats.bounds_min[k].min(p[k]);
            stats.bounds_max[k] = stats.bounds_max[k].max(p[k]);
        }
    }
    stats.draw_boxes.push((bmin, bmax));
}

/// Point-sprite draws (vertex format 0x18: position + D3DCOLOR, no UV;
/// docs/CAPSULE-VISUALS.md "Sparkles, pickup and erase bursts") a model's
/// draws hold. These are `st00.alr`'s materialise-sparkle / pickup-burst /
/// erase-burst pieces (capsule slots 0, 2, 3) -- point lists, not triangle
/// meshes, so they are read directly rather than through `export_glb`'s
/// triangle path (which drops them: no UV, so `include_untextured` would
/// otherwise triangulate garbage from what are really independent points).
/// Right-handed, Z-mirrored, matching `export_glb`'s convention.
pub fn point_sprites(a: &Alr, model: &alr::Model) -> Vec<(Vec3, [u8; 4])> {
    let mut out = Vec::new();
    for draw in &model.draws {
        let Some(vb) = model.vertbufs.get(draw.vertex_buf as usize) else { continue };
        if vb.format != 0x18 {
            continue;
        }
        let (Ok(verts), Ok(light)) = (a.read_vertices(vb), a.read_vertex_light(vb)) else { continue };
        for (v, l) in verts.iter().zip(light.iter()) {
            if let Some(c) = l.color {
                out.push((Vec3::new(v.pos[0], v.pos[1], -v.pos[2]), c));
            }
        }
    }
    out
}

/// Resolved glTF texture index(es) for one exported capsule draw
/// (`export_capsule_slot_glb`): `base` is the draw's own material texture
/// (the flame/water/nebula/etc. sphere texture, or the cup/disc's cloud
/// texture); `secondary` is `alr::MaterialEntry::normal_idx` when it names a
/// distinct texture -- the sphere
/// core's bump map (docs/CAPSULE-VISUALS.md: sampled twice at different UVs
/// by `vkCAP02M2`, not consumed as a standard tangent-space normal map even
/// though it rides in the glTF material's `normalTexture` slot as a
/// carrier). `tertiary`/`quaternary` are the material's remaining two
/// texture slots (`texture_idx2`/`texture_idx3`), confirmed 2026-09-28 to be
/// real, additional textures -- not padding -- on Status/Environment's own
/// unlit 4-texture materials (`vkCAP01`/`vkCAP04M4`).
#[derive(Debug, Clone, Copy, Default)]
pub struct CapsuleDrawTex {
    pub base: Option<usize>,
    pub secondary: Option<usize>,
    pub tertiary: Option<usize>,
    pub quaternary: Option<usize>,
}

/// One `st00.alr` model slot (a capsule/Aura-Particle idle orb, or the
/// flare/burst pieces in slots 0-3), exported with the full vertex data the
/// idle-orb shaders need (docs/CAPSULE-VISUALS.md "Draw recipe (decoded
/// 2026-09-28)" -> "Build recipe", step 1): TEXCOORD_1, TANGENT (handedness
/// baked in from the file's own separately-stored binormal, standard
/// glTF/Bevy convention: `bitangent = cross(normal, tangent.xyz) *
/// tangent.w`) and vertex-colour alpha for the sphere core (format 0x1D,
/// stride 0x24); vertex colour (the cup/disc's baked rim profile) for the
/// cup/disc rim (format 0x09, stride 0x14). `export_glb`'s shared path
/// (every stage/character/effect export and its 272 tests) only ever
/// carries TEXCOORD_0 and, when baking stage lighting, COLOR_0 -- not
/// enough here, so capsules get their own exporter instead of growing that
/// shared one. One node/mesh per draw, in file order, named `"d{index}"`;
/// no instancing, no lighting bake, no skinning (idle orbs have none).
/// Point-sprite draws (format 0x18) are skipped here -- read via
/// [`point_sprites`] instead, as before.
pub fn export_capsule_slot_glb(a: &Alr, model_idx: usize) -> Result<(Vec<u8>, ExportStats, Vec<CapsuleDrawTex>)> {
    let mut b = Builder::default();
    let mut stats = ExportStats { bounds_min: [f32::MAX; 3], bounds_max: [f32::MIN; 3], ..Default::default() };
    let mut mats = Materials::default();
    let mut draw_tex = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();

    if let Some(model) = a.models.get(model_idx) {
        for (di, draw) in model.draws.iter().enumerate() {
            let Some(vb) = model.vertbufs.get(draw.vertex_buf as usize) else { continue };
            // 0x1D/0x03: sphere core (with/without a second UV set --
            // "M2"/base `vkCAP02` shader variants). 0x09: cup. 0x15: flat
            // disc. 0x17: Status/Environment's simpler unlit core
            // (`vkCAP01`/`vkCAP04M4`; pos+uv only in our reader -- see
            // `fx.rs`'s per-kind table for how these two are approximated).
            if !matches!(vb.format, 0x1D | 0x09 | 0x15 | 0x03 | 0x17) {
                continue;
            }
            let Ok(verts) = a.read_capsule_vertices(vb) else { continue };
            if verts.is_empty() {
                continue;
            }
            let tris: Vec<[u16; 3]> = alr::triangulate(draw).into_iter().filter(|t| t.iter().all(|&i| (i as usize) < verts.len())).collect();
            if tris.is_empty() {
                continue;
            }

            let mut pos = Vec::with_capacity(verts.len() * 12);
            let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &verts {
                let p = [v.pos[0], v.pos[1], -v.pos[2]];
                for k in 0..3 {
                    mn[k] = mn[k].min(p[k]);
                    mx[k] = mx[k].max(p[k]);
                    pos.extend_from_slice(&p[k].to_le_bytes());
                }
            }
            let pv = b.push_view(&pos, Some(34962));
            let pa = b.push_accessor(json!({ "bufferView": pv, "componentType": 5126, "count": verts.len(), "type": "VEC3", "min": mn, "max": mx }));
            let mut attrs = json!({ "POSITION": pa });

            if verts[0].uv0.is_some() {
                let mut uv = Vec::with_capacity(verts.len() * 8);
                for v in &verts {
                    let t = v.uv0.unwrap_or([0.0, 0.0]);
                    uv.extend_from_slice(&t[0].to_le_bytes());
                    uv.extend_from_slice(&t[1].to_le_bytes());
                }
                let uvv = b.push_view(&uv, Some(34962));
                attrs["TEXCOORD_0"] = json!(b.push_accessor(json!({ "bufferView": uvv, "componentType": 5126, "count": verts.len(), "type": "VEC2" })));
            }
            if verts[0].normal.is_some() {
                let mut n = Vec::with_capacity(verts.len() * 12);
                for v in &verts {
                    let nn = v.normal.unwrap_or([0.0, 1.0, 0.0]);
                    n.extend_from_slice(&nn[0].to_le_bytes());
                    n.extend_from_slice(&nn[1].to_le_bytes());
                    n.extend_from_slice(&(-nn[2]).to_le_bytes());
                }
                let nv = b.push_view(&n, Some(34962));
                attrs["NORMAL"] = json!(b.push_accessor(json!({ "bufferView": nv, "componentType": 5126, "count": verts.len(), "type": "VEC3" })));
            }
            if verts[0].uv1.is_some() {
                let mut uv = Vec::with_capacity(verts.len() * 8);
                for v in &verts {
                    let t = v.uv1.unwrap_or([0.0, 0.0]);
                    uv.extend_from_slice(&t[0].to_le_bytes());
                    uv.extend_from_slice(&t[1].to_le_bytes());
                }
                let uvv = b.push_view(&uv, Some(34962));
                attrs["TEXCOORD_1"] = json!(b.push_accessor(json!({ "bufferView": uvv, "componentType": 5126, "count": verts.len(), "type": "VEC2" })));
            }
            if let (Some(n0), Some(t0), Some(bn0)) = (verts[0].normal, verts[0].tangent, verts[0].binormal) {
                let _ = (n0, t0, bn0);
                let mut t = Vec::with_capacity(verts.len() * 16);
                for v in &verts {
                    let n = Vec3::from(v.normal.unwrap()) * Vec3::new(1.0, 1.0, -1.0);
                    let tg = Vec3::from(v.tangent.unwrap()) * Vec3::new(1.0, 1.0, -1.0);
                    let bn = Vec3::from(v.binormal.unwrap()) * Vec3::new(1.0, 1.0, -1.0);
                    let w = if n.cross(tg).dot(bn) < 0.0 { -1.0f32 } else { 1.0f32 };
                    t.extend_from_slice(&tg.x.to_le_bytes());
                    t.extend_from_slice(&tg.y.to_le_bytes());
                    t.extend_from_slice(&tg.z.to_le_bytes());
                    t.extend_from_slice(&w.to_le_bytes());
                }
                let tv = b.push_view(&t, Some(34962));
                attrs["TANGENT"] = json!(b.push_accessor(json!({ "bufferView": tv, "componentType": 5126, "count": verts.len(), "type": "VEC4" })));
            }
            if verts[0].color.is_some() {
                let mut c = Vec::with_capacity(verts.len() * 16);
                for v in &verts {
                    let k = v.color.unwrap_or([255; 4]);
                    for ch in k {
                        c.extend_from_slice(&(ch as f32 / 255.0).to_le_bytes());
                    }
                }
                let cv = b.push_view(&c, Some(34962));
                attrs["COLOR_0"] = json!(b.push_accessor(json!({ "bufferView": cv, "componentType": 5126, "count": verts.len(), "type": "VEC4" })));
            }

            let mut idx = Vec::with_capacity(tris.len() * 12);
            for t in &tris {
                for i in [t[0], t[2], t[1]] {
                    idx.extend_from_slice(&(i as u32).to_le_bytes());
                }
            }
            let iv = b.push_view(&idx, Some(34963));
            let ia = b.push_accessor(json!({ "bufferView": iv, "componentType": 5125, "count": tris.len() * 3, "type": "SCALAR" }));

            let matent = model.materials.get(draw.material_idx as usize);
            let tex_idx = matent.map(|m| m.texture_idx as usize).unwrap_or(usize::MAX);
            let normal_idx = matent.and_then(|m| {
                let n = m.normal_idx as usize;
                (n != tex_idx && n < a.textures.len()).then_some(n)
            });
            let mat = if normal_idx.is_some() { mats.get_dual(&mut b, a, tex_idx, normal_idx, &mut stats) } else { mats.get(&mut b, a, tex_idx, &mut stats) };
            // Slots 3/4 (`texture_idx2`/`texture_idx3`): confirmed 2026-09-28
            // on Status/Environment's own materials to be real, additional
            // textures for their 4-texture unlit shaders (`vkCAP01`/
            // `vkCAP04M4`), not padding -- a byte dump matched the recipe's
            // own per-kind numbers (18,19,19,20 and 25,26,26,27) exactly.
            // Embedded here (even though no *material* in this glb
            // references them) purely so `pd_game` can load them directly
            // by glTF texture index.
            let tertiary = matent.and_then(|m| {
                let n = m.texture_idx2 as usize;
                (n < a.textures.len()).then_some(n)
            });
            let quaternary = matent.and_then(|m| {
                let n = m.texture_idx3 as usize;
                (n < a.textures.len()).then_some(n)
            });
            let tertiary_glb = tertiary.and_then(|n| mats.texture(&mut b, a, n, &mut stats, false)).map(|(t, _)| t);
            let quaternary_glb = quaternary.and_then(|n| mats.texture(&mut b, a, n, &mut stats, false)).map(|(t, _)| t);
            draw_tex.push(CapsuleDrawTex { base: mats.resolved_tex_index(tex_idx), secondary: normal_idx.and_then(|n| mats.resolved_tex_index(n)), tertiary: tertiary_glb, quaternary: quaternary_glb });

            meshes.push(json!({ "primitives": [{ "attributes": attrs, "indices": ia, "material": mat }] }));
            nodes.push(json!({ "mesh": meshes.len() - 1, "name": format!("d{di}") }));
            track_bounds(&mut stats, Mat4::IDENTITY, &b.accessors[pa]);
            stats.draws += 1;
            stats.triangles += tris.len();
        }
        if !model.draws.is_empty() {
            stats.models = 1;
        }
    }
    stats.finish();
    let top: Vec<usize> = (0..nodes.len()).collect();
    nodes.push(json!({ "name": "root", "children": top }));
    let root = nodes.len() - 1;
    let mut doc = json!({
        "asset": { "version": "2.0", "generator": "pd_formats" },
        "scene": 0,
        "scenes": [{ "nodes": [root] }],
        "nodes": nodes,
        "meshes": meshes,
    });
    mats.write_into(&mut doc);
    b.write_into(&mut doc);
    Ok((write_glb(&doc, &b.bin), stats, draw_tex))
}

pub(crate) fn write_glb(doc: &Value, bin: &[u8]) -> Vec<u8> {
    let mut js = serde_json::to_vec(doc).unwrap();
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    let mut bin = bin.to_vec();
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

/// Summary of an ALR for quick inspection.
pub fn describe(a: &Alr) -> String {
    let mut s = format!(
        "version 0x{:X}, {} chunks, resource buffer @0x{:X}, {} textures, {} models\n",
        a.version,
        a.chunks.len(),
        a.resbuf_offset,
        a.textures.len(),
        a.models.len()
    );
    let mut fmts: HashMap<String, usize> = HashMap::new();
    for t in &a.textures {
        *fmts.entry(format!("{:?}", t.format)).or_default() += 1;
    }
    s += &format!("  texture formats: {fmts:?}\n");
    let mut vfmts: HashMap<u8, usize> = HashMap::new();
    for m in &a.models {
        for v in &m.vertbufs {
            *vfmts.entry(v.format).or_default() += 1;
        }
    }
    s += &format!("  vertex formats: {vfmts:?}\n");
    s
}

#[cfg(test)]
mod collision_filter_tests {
    use super::{is_oversized_floor_patch, is_shadow_volume, is_sky_backdrop, volume_instance, volume_node_name, VOLUME_TAG};
    use crate::alr::Alr;

    /// The JSON chunk of a binary glTF.
    fn glb_json(glb: &[u8]) -> serde_json::Value {
        let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        serde_json::from_slice(&glb[20..20 + len]).unwrap()
    }

    /// Real data (skipped without the game files): st05 exports the volume
    /// grid node (its stage-own volumes, world space), bump materials with
    /// the specular extras, and the frame attributes of bump draws.
    #[test]
    fn stage_export_carries_the_volume_grid_and_bump_draws() {
        let dir = std::env::var_os("PD_GAME_FILES").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"..\..\..\game_files\Assets\Data\Map"));
        let (alr, dat) = (dir.join("st05.alr"), dir.join("st05.dat"));
        if !alr.is_file() || !dat.is_file() {
            eprintln!("skipped: no game files at {}", dir.display());
            return;
        }
        let a = Alr::load(&alr).unwrap();
        let lights = crate::stagedat::parse_light_script(&std::fs::read(&dat).unwrap()).unwrap();
        let (glb, stats) = super::export_glb(&a, &super::ExportOptions { apply_transforms: true, lighting: Some(lights.sets[0]), ..Default::default() }).unwrap();
        let doc = glb_json(&glb);
        let nodes = doc["nodes"].as_array().unwrap();
        let grid = nodes.iter().find(|n| n["name"] == super::GRID_NODE).expect("the grid node");
        let mesh = &doc["meshes"][grid["mesh"].as_u64().unwrap() as usize]["primitives"][0];
        let count = doc["accessors"][mesh["indices"].as_u64().unwrap() as usize]["count"].as_u64().unwrap();
        // st05's model-0 volume alone is 2,312 triangles (docs/SHADOWS.md); the
        // object templates' volumes come on top.
        assert!(count / 3 >= 2312, "grid triangles {}", count / 3);
        assert!(stats.draws > 0);
        // A bump material: normal map + extras with the specular bytes and the +0x40 short.
        let bump = doc["materials"].as_array().unwrap().iter().find(|m| m["extras"]["pd_stage"].is_object() && m["normalTexture"].is_object()).expect("a bump material");
        assert!(bump["extras"]["pd_stage"]["spec"].as_array().unwrap().len() == 4);
        // Its mesh carries NORMAL / TANGENT (w = +1 everywhere) and the mask UV.
        let prim = doc["meshes"].as_array().unwrap().iter().flat_map(|m| m["primitives"].as_array().unwrap()).find(|p| p["attributes"]["TANGENT"].is_u64()).expect("a bump primitive");
        assert!(prim["attributes"]["NORMAL"].is_u64() && prim["attributes"]["TEXCOORD_1"].is_u64() && prim["attributes"]["COLOR_0"].is_u64());
        // And a stage with the old light-less export has none of it.
        let (plain, _) = super::export_glb(&a, &super::ExportOptions { apply_transforms: true, ..Default::default() }).unwrap();
        assert!(!glb_json(&plain)["nodes"].as_array().unwrap().iter().any(|n| n["name"] == super::GRID_NODE));
    }

    /// `SSHADOW_vs`: only vertices facing away from the light move, by exactly
    /// the material's length along the light's travel direction.
    #[test]
    fn volume_extrusion_follows_the_vertex_shader() {
        use super::extrude_volume_vertex;
        use glam::Vec3;
        let dir = Vec3::new(0.546, -0.677, -0.4925).normalize();
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(extrude_volume_vertex(p, -dir, dir, 60.0), p, "faces the light: stays");
        let moved = extrude_volume_vertex(p, dir, dir, 60.0);
        assert!(((moved - p).length() - 60.0).abs() < 1e-3 && (moved - p).normalize().dot(dir) > 0.999999);
        // Exactly sideways: the compare is strict, so it stays.
        assert_eq!(extrude_volume_vertex(p, dir.any_orthonormal_vector(), dir, 60.0), p);
        // Length 0 (the position-only SSHADOWC volumes carry 0): nothing moves.
        assert_eq!(extrude_volume_vertex(p, dir, dir, 0.0), p);
    }

    #[test]
    fn only_flagged_position_only_draws_are_shadow_volumes() {
        // st03 model 0 draw 1 (fmt 0x10, flags 0x200b), st04 draw 2 (0x07,
        // 0x200b), st07 draw 160 (0x10, 0x200f): shadow volumes.
        assert!(is_shadow_volume(0x10, 0x200b));
        assert!(is_shadow_volume(0x07, 0x200b));
        assert!(is_shadow_volume(0x10, 0x200f));
        // Same layout without the flag (st06 draws 130-141, st02 draw 1):
        // stays a candidate for the other filters below, not this one.
        assert!(!is_shadow_volume(0x10, 0x0003));
        // "Suspected collision" position-only format 0x08 and ordinary
        // textured stage formats stay solid whatever the flags.
        assert!(!is_shadow_volume(0x08, 0x2009));
        assert!(!is_shadow_volume(0x01, 0x200d));
        assert!(!is_shadow_volume(0x03, 0x402d));
    }

    #[test]
    fn format_0x0b_is_always_the_sky_backdrop() {
        // st01/03/04/05/06/07/08/12/14/15/16/17/24: always this format,
        // never seen on a small helper draw.
        assert!(is_sky_backdrop(0x0b));
        assert!(!is_sky_backdrop(0x10));
        assert!(!is_sky_backdrop(0x09)); // textured backdrop variant (st02/22's sky): stays solid by this rule
        assert!(!is_sky_backdrop(0x01));
    }

    #[test]
    fn only_highways_huge_0x0003_draw_is_an_oversized_floor_patch() {
        // st02/12/22 model 0 draw 1: 3,123 x 2,983 units.
        assert!(is_oversized_floor_patch(0x10, 0x0003, (3123.0, 2983.0)));
        // Every other flags-0x0003 draw on any of the 30 stages tops out at
        // st06/16/26 draw 163 (284 x 262): stays solid.
        assert!(!is_oversized_floor_patch(0x10, 0x0003, (284.0, 262.0)));
        assert!(!is_oversized_floor_patch(0x10, 0x0003, (220.0, 200.0))); // st04/14 draw 14
        assert!(!is_oversized_floor_patch(0x10, 0x0003, (220.0, 56.0))); // st24 draw 13
        assert!(!is_oversized_floor_patch(0x10, 0x0003, (65.0, 56.0))); // st02/12/22 draw 2
        // A big draw with a different format or flags isn't this rule's business.
        assert!(!is_oversized_floor_patch(0x01, 0x0003, (3123.0, 2983.0)));
        assert!(!is_oversized_floor_patch(0x10, 0x200b, (3123.0, 2983.0)));
    }

    #[test]
    fn volume_node_names_are_tagged_and_distinct() {
        assert_eq!(volume_node_name(None, 0, 1), "shadowvol_m0_d1");
        assert_eq!(volume_node_name(Some("prop012_deskA"), 31, 4), "prop012_deskA_shadowvol_d4");
        for n in [volume_node_name(None, 5, 2), volume_node_name(Some("prop001_x"), 5, 2)] {
            assert!(n.contains(VOLUME_TAG));
        }
        // Only placed objects get volumes; the props library's copies do not.
        assert!(volume_instance("prop012_deskA") && !volume_instance("type03_broken"));
    }

    /// Which stage files carry shadow-volume draws (material flag 0x2000, vertex
    /// format 0x07 / 0x10) at all, read from the real ALRs; skipped where the
    /// game files are not on this machine. Measured 2026-10-01 (docs/SHADOWS.md):
    /// 20 of the 30 files. The P0 lighting audit's list (st02-05/07/08/12-15/
    /// 17/22-24 + st09_02/08) misses st06/16/26 (Refinery) and st25.
    #[test]
    fn shadow_volumes_exist_on_exactly_these_stage_files() {
        let dir = std::env::var_os("PD_GAME_FILES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"..\..\..\game_files\Assets\Data\Map"));
        if !dir.is_dir() {
            eprintln!("skipped: no game files at {}", dir.display());
            return;
        }
        let mut with: Vec<String> = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "alr")).collect();
        files.sort();
        let mut seen = 0;
        for f in files {
            let id = f.file_stem().unwrap().to_string_lossy().to_string();
            if id.ends_with("cam") {
                continue;
            }
            seen += 1;
            let a = Alr::load(&f).unwrap();
            let has = a.models.iter().any(|m| {
                m.draws.iter().any(|d| {
                    let fmt = m.vertbufs.get(d.vertex_buf as usize).map_or(0, |v| v.format);
                    let flags = m.materials.get(d.material_idx as usize).map_or(0, |x| x.flags);
                    is_shadow_volume(fmt, flags)
                })
            });
            if has {
                with.push(id);
            }
        }
        assert_eq!(seen, 30, "30 stage files");
        let expect = ["st02", "st03", "st04", "st05", "st06", "st07", "st08", "st09_02", "st09_08", "st12", "st13", "st14", "st15", "st16", "st17", "st22", "st23", "st24", "st25", "st26"];
        assert_eq!(with, expect);
    }
}
