//! Render-pipeline warm-up for the match's effects (lane fxperf, 2026-10-08; Laurence: "frame
//! drops when casting moves").
//!
//! The first time a draw with a new combination of material variant (alpha mode, the `FxKey`
//! blend equation, cull mode, scene-copy use) and vertex layout appears, Bevy has to build and
//! compile a render pipeline for it. It does that in the background, and the draw is skipped
//! until the pipeline is ready, so a skill's first cast in a session could show its effect late
//! (and the compile competes with the frame for the CPU).
//!
//! At match start, once `FxPreload` holds the arsenals' effects, this spawns for every variant
//! those effects can draw (pieces with their own glb meshes, wisp bodies, trails, blades, flares,
//! points, puffs, MSPARTICLE quads and ribbons) ONE warm-up draw: the same material recipe the
//! effect itself uses (`piece_fx_material`, `plain_add_material`, ...; textures left at the
//! default image, which no pipeline depends on) on a copy of the same mesh with EVERY vertex
//! moved to one point. Same vertex layout, topology and material key, so Bevy specialises and
//! compiles the very pipeline the effect will need; but every triangle has zero area, and a
//! zero-area triangle covers no pixel in any pass (colour, depth prepass, shadows), so nothing
//! can appear on screen. The draws stay `HOLD_FRAMES` frames (long enough for the request to
//! reach the render thread; the compile then finishes on its own) and are removed. A variant
//! already warmed in this run is not warmed again.
//!
//! `PD_FX_WARMUP=0` turns it off.

use super::*;
use bevy::gltf::{Gltf, GltfMesh, GltfNode};
use bevy::mesh::VertexAttributeValues;
use std::collections::HashSet;
use std::sync::OnceLock;

/// `PD_FX_WARMUP=0`: no warm-up draws. Read once.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PD_FX_WARMUP").map_or(true, |v| v != "0"))
}

/// Frames a warm-up draw stays.
const HOLD_FRAMES: u32 = 60;
/// Frames to wait for the effects' models before warming what has loaded and giving up on the rest.
const GIVE_UP_FRAMES: u32 = 900;

/// The warm-up's state: the effect set it is for, the effects still waiting for their model, the
/// variants already warmed this run.
#[derive(Resource, Default)]
pub struct FxWarmup {
    names: Vec<String>,
    pending: Vec<(String, Option<Handle<Gltf>>)>,
    waited: u32,
    warmed: HashSet<(mat_cache::StaticKey, String)>,
    /// Warm-up draws spawned (`PD_FRAME_STATS`).
    pub draws: u64,
}

/// A warm-up draw and the frames it has left.
#[derive(Component)]
pub struct FxWarmDraw {
    left: u32,
}

/// `mesh` with every vertex at the origin: the same layout, topology and indices (so the same
/// pipeline), no visible area. None for a mesh whose vertices something else could move apart
/// again (morph targets, skinning) or that cannot be read.
pub fn collapsed(mesh: &Mesh) -> Option<Mesh> {
    if mesh.try_has_morph_targets().ok()? || mesh.try_contains_attribute(Mesh::ATTRIBUTE_JOINT_INDEX).ok()? {
        return None;
    }
    let mut m = mesh.clone();
    match m.try_attribute_mut(Mesh::ATTRIBUTE_POSITION).ok()? {
        VertexAttributeValues::Float32x3(v) => v.iter_mut().for_each(|p| *p = [0.0; 3]),
        _ => return None,
    }
    Some(m)
}

/// What picks the pipeline on the mesh's side: topology and each attribute with its format.
fn layout_key(mesh: &Mesh) -> Option<String> {
    let attrs: Vec<String> = mesh.try_attributes().ok()?.map(|(a, v)| format!("{:?}:{:?}", a.id, bevy::render::render_resource::VertexFormat::from(v))).collect();
    Some(format!("{:?} {} {}", mesh.primitive_topology(), attrs.join(","), mesh.indices().map_or("-", |i| if matches!(i, bevy::mesh::Indices::U16(_)) { "u16" } else { "u32" })))
}

/// Every (material, mesh) pair the effect `def` can draw, built the way the effect builds them.
/// `node_meshes` = a piece node's glb meshes (None: its model is not available).
fn variants(def: &FxDef, node_meshes: &dyn Fn(&str) -> Vec<Mesh>) -> Vec<(FxMaterial, Mesh)> {
    let tex = Handle::<Image>::default;
    let mut out = Vec::new();
    for (name, piece) in &def.pieces {
        if piece.distort && distort_strength() <= 0.0 {
            continue;
        }
        let images: [Option<Handle<Image>>; 4] = std::array::from_fn(|k| piece.textures[k].as_ref().map(|_| tex()));
        let mat = piece_fx_material(piece, tex(), images);
        if let (Some(skin), Some(_)) = (piece.skin.as_ref(), piece.wisp) {
            // Drawn through its wisp bodies (`aura_wisp`), a clone of this material on the skin's mesh.
            out.push((mat, skin.mesh()));
            continue;
        }
        for mesh in node_meshes(name) {
            out.push((mat.clone(), mesh));
        }
    }
    for _ in def.trails.iter() {
        out.push((plain_add_material(tex()), trail_mesh()));
    }
    for _ in def.blades.iter() {
        out.push((plain_add_material(tex()), blade_mesh(&[(Vec3::ZERO, Vec3::ZERO); 2], 1)));
    }
    for f in def.flares.iter() {
        out.push((plain_add_material(tex()), flare_mesh(f, &[])));
    }
    if light_enabled() && env_once("PD_LIGHT_DECAL").is_some_and(|v| v == "1") && !def.lights.is_empty() {
        out.push((plain_add_material(tex()), quads_mesh(&[XZ], FULL_UV)));
    }
    for s in def.sprites.iter() {
        if let Some(ms) = &s.ms {
            if ms.is_ribbon() {
                out.push((ms_particle::ribbon_material((ms.draw_mode() == 3).then(tex), ms.blend()), ms_particle::empty_ribbon_mesh()));
            } else {
                out.push((ms_particle::quad_material(Color::WHITE, tex(), ms.blend()), quads_mesh(&[XY], FULL_UV)));
            }
        } else if s.points.is_some() {
            let billboard = point_billboard();
            let mesh = if billboard { quads_mesh(&[XY], FULL_UV) } else { quads_mesh(&[XY, ZY, XZ], FULL_UV) };
            out.push((point_material(Vec4::ONE, tex(), billboard), mesh));
        } else {
            // Whether a puff is cut out depends on its texture's alpha (known once loaded): both.
            out.push((puff_material(Vec4::ONE, tex(), false), quads_mesh(&[XY, ZY, XZ], FULL_UV)));
            if puff_is_translucent(s, Some(true)) {
                out.push((puff_material(Vec4::ONE, tex(), true), quads_mesh(&[XY, ZY, XZ], FULL_UV)));
            }
        }
    }
    out
}

/// The particles' variants (`spawn_burst` / `spawn_point_burst`; an `FxMaterial` too since overnight
/// perf 2026-10-09): additive and blended, with and without a texture, on their quad layout.
fn particle_variants() -> Vec<(FxMaterial, Mesh)> {
    let mut out = Vec::new();
    for additive in [false, true] {
        for texture in [None, Some(Handle::<Image>::default())] {
            let base = StandardMaterial { base_color_texture: texture, unlit: true, alpha_mode: if additive { AlphaMode::Add } else { AlphaMode::Blend }, double_sided: true, cull_mode: None, ..default() };
            out.push((inst::neutral(particle_material(base)), quads_mesh(&[XY], FULL_UV)));
        }
    }
    out
}

/// The glb meshes under the node named `name` of a loaded effect model.
fn node_meshes(gltf: &Gltf, nodes: &Assets<GltfNode>, gmeshes: &Assets<GltfMesh>, meshes: &Assets<Mesh>, name: &str) -> Vec<Mesh> {
    let Some(node) = gltf.named_nodes.get(name).and_then(|h| nodes.get(h)) else { return Vec::new() };
    let Some(gm) = node.mesh.as_ref().and_then(|h| gmeshes.get(h)) else { return Vec::new() };
    gm.primitives.iter().filter_map(|p| meshes.get(&p.mesh).cloned()).collect()
}

/// Age out the warm-up draws; when the match's effect set changes, warm its pipelines (see the module doc).
#[allow(clippy::too_many_arguments)]
pub fn warm_pipelines(
    mut commands: Commands,
    preload: Res<FxPreload>,
    mut warm: ResMut<FxWarmup>,
    mut lib: ResMut<FxLibrary>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    (nodes, gmeshes): (Res<Assets<GltfNode>>, Res<Assets<GltfMesh>>),
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<FxMaterial>>,
    mut draws: Query<(Entity, &mut FxWarmDraw)>,
) {
    for (e, mut d) in &mut draws {
        d.left = d.left.saturating_sub(1);
        if d.left == 0 {
            commands.entity(e).despawn();
        }
    }
    if !enabled() {
        return;
    }
    if preload.names() != warm.names.as_slice() {
        warm.names = preload.names().to_vec();
        warm.pending = warm
            .names
            .iter()
            .filter_map(|n| {
                let def = lib.get(n)?;
                let model = (!def.pieces.is_empty()).then(|| assets.load::<Gltf>(format!("fx/{n}.glb")));
                Some((n.clone(), model))
            })
            .collect();
        warm.waited = 0;
    }
    if warm.pending.is_empty() {
        return;
    }
    warm.waited += 1;
    let give_up = warm.waited > GIVE_UP_FRAMES;
    let pending = std::mem::take(&mut warm.pending);
    for (name, model) in pending {
        let loaded = model.as_ref().is_none_or(|h| assets.is_loaded_with_dependencies(h));
        if !loaded && !give_up {
            warm.pending.push((name, model));
            continue;
        }
        let Some(def) = lib.get(&name) else { continue };
        let gltf = model.as_ref().and_then(|h| gltfs.get(h));
        let found = |node: &str| gltf.map_or_else(Vec::new, |g| node_meshes(g, &nodes, &gmeshes, &meshes, node));
        // The particles' variants ride along with the first effect warmed (deduplicated like the rest).
        let parts = if warm.warmed.is_empty() { particle_variants() } else { Vec::new() };
        for (mat, mesh) in variants(&def, &found).into_iter().chain(parts) {
            let (Some(layout), Some(flat)) = (layout_key(&mesh), collapsed(&mesh)) else { continue };
            if !warm.warmed.insert((mat_cache::StaticKey::of(&mat), layout)) {
                continue;
            }
            warm.draws += 1;
            commands.spawn((
                Mesh3d(meshes.add(flat)),
                MeshMaterial3d(mats.add(mat)),
                Transform::IDENTITY,
                Visibility::Visible,
                bevy::camera::visibility::NoFrustumCulling,
                FxWarmDraw { left: HOLD_FRAMES },
                Name::new("fx pipeline warm-up"),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_area_sum(m: &Mesh) -> f32 {
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        let idx: Vec<usize> = m.indices().unwrap().iter().collect();
        idx.chunks(3).map(|t| (Vec3::from(p[t[1]]) - Vec3::from(p[t[0]])).cross(Vec3::from(p[t[2]]) - Vec3::from(p[t[0]])).length()).sum()
    }

    #[test]
    fn a_collapsed_mesh_keeps_its_layout_and_has_no_area() {
        for mesh in [quads_mesh(&[XY, ZY, XZ], FULL_UV), trail_mesh(), blade_mesh(&[(Vec3::ZERO, Vec3::Y), (Vec3::X, Vec3::X + Vec3::Y)], 2), ms_particle::empty_ribbon_mesh()] {
            assert!(quad_area_sum(&mesh) >= 0.0);
            let flat = collapsed(&mesh).unwrap();
            assert_eq!(layout_key(&flat), layout_key(&mesh));
            assert_eq!(flat.count_vertices(), mesh.count_vertices());
            assert_eq!(format!("{:?}", flat.indices()), format!("{:?}", mesh.indices()));
            assert_eq!(quad_area_sum(&flat), 0.0);
            // Every attribute but the positions is untouched.
            for ((a, v), (b, w)) in flat.attributes().zip(mesh.attributes()) {
                assert_eq!(a.id, b.id);
                if a.id != Mesh::ATTRIBUTE_POSITION.id {
                    assert_eq!(format!("{v:?}"), format!("{w:?}"));
                }
            }
        }
        assert!(quad_area_sum(&quads_mesh(&[XY], FULL_UV)) > 0.0);
    }

    #[test]
    fn a_billboard_of_a_collapsed_quad_stays_one_point() {
        // `fx_gamma.wgsl`'s billboard_world_position: o + (roll(local.xy) * scale, local.z * scale) -- a
        // function of the local position alone, so equal positions give equal points.
        let bb = |local: Vec3, roll: f32, s: Vec3| {
            let (sn, c) = roll.sin_cos();
            Vec3::new((c * local.x - sn * local.y) * s.x, (sn * local.x + c * local.y) * s.y, local.z * s.z)
        };
        let flat = collapsed(&quads_mesh(&[XY], FULL_UV)).unwrap();
        let Some(VertexAttributeValues::Float32x3(p)) = flat.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        let pts: Vec<Vec3> = p.iter().map(|q| bb(Vec3::from(*q), 0.7, Vec3::new(3.0, 2.0, 1.0))).collect();
        assert!(pts.iter().all(|q| *q == pts[0]));
    }

    #[test]
    fn warm_variants_use_the_effects_own_material_recipes() {
        let v = serde_json::json!({
            "slots": [[0], [], [], [], [], []],
            "pieces": [],
            "emitters": [],
        });
        let mut def = FxDef::parse("eft9999", &v, |_| true).unwrap();
        def.flares.push(Flare { model: 0, texture: String::new(), start: 0.0, end: 4.0, colors: [0; 2], count: 1, life: 4.0, fade: 0.0, length: [1.0; 2], width: [1.0; 2], random: [false; 2], spin: 0, joint: None, offset: Vec3::ZERO });
        let out = variants(&def, &|_| Vec::new());
        assert_eq!(out.len(), 1);
        let (mat, mesh) = &out[0];
        assert_eq!(format!("{mat:?}"), format!("{:?}", plain_add_material(Handle::default())));
        assert_eq!(layout_key(mesh), layout_key(&flare_mesh(&def.flares[0], &[])));
    }
}
