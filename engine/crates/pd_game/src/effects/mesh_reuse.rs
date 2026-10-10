//! Effect meshes rewritten in place (lane fxperf, 2026-10-08).
//!
//! The sword blades (`update_blades`), ray bursts (`update_flares`) and MSPARTICLE ribbons
//! (`ms_particle::tick`) used to build a whole new `Mesh` every frame for every live one (fresh
//! vectors for positions, normals, UVs, colours and indices) and swap it in, and the aura wisps
//! (`aura_wisp::update`) a fresh position vector; each swap marks the mesh modified, so the
//! render thread copies and uploads it again. Now:
//! * the same vectors are refilled in place (`Strip::take` / `Strip::put`), same values in the
//!   same order, so the mesh holds exactly what the old builder made;
//! * a mesh whose inputs did not change since it was last written is not touched at all (a blade
//!   holding still after its swing, a burst with no rays left, a paused game).
//!
//! `PD_FX_MESHREUSE=0` restores the old full rebuild every frame.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use std::sync::OnceLock;

/// `PD_FX_MESHREUSE=0`: rebuild the meshes every frame (the old path). Read once.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PD_FX_MESHREUSE").map_or(true, |v| v != "0"))
}

/// An effect strip's vertex data: position, UV and colour per vertex (the normal is always up),
/// and its triangle indices.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Strip {
    pub pos: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub col: Vec<[f32; 4]>,
    pub idx: Vec<u32>,
    normals: Vec<[f32; 3]>,
}

impl Strip {
    /// A zero-length vertex buffer upsets the render allocator: an empty strip becomes one
    /// degenerate triangle (what the builders always did).
    pub fn never_empty(&mut self) {
        if self.pos.is_empty() {
            self.pos.extend_from_slice(&[[0.0; 3]; 3]);
            self.uv.clear();
            self.uv.extend_from_slice(&[[0.0; 2]; 3]);
            self.col.clear();
            self.col.extend_from_slice(&[[0.0; 4]; 3]);
            self.idx.clear();
            self.idx.extend_from_slice(&[0, 1, 2]);
        }
    }

    /// A new mesh holding the strip (the old builders' output).
    pub fn into_mesh(mut self) -> Mesh {
        self.normals.clear();
        self.normals.resize(self.pos.len(), [0.0, 1.0, 0.0]);
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
            .with_inserted_indices(Indices::U32(self.idx))
    }

    /// Take a strip mesh's vectors out of it (emptied, their capacity kept) to be refilled.
    pub fn take(mesh: &mut Mesh) -> Strip {
        let mut s = Strip::default();
        if let Some(VertexAttributeValues::Float32x3(v)) = mesh.remove_attribute(Mesh::ATTRIBUTE_POSITION) {
            s.pos = v;
        }
        if let Some(VertexAttributeValues::Float32x3(v)) = mesh.remove_attribute(Mesh::ATTRIBUTE_NORMAL) {
            s.normals = v;
        }
        if let Some(VertexAttributeValues::Float32x2(v)) = mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0) {
            s.uv = v;
        }
        if let Some(VertexAttributeValues::Float32x4(v)) = mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR) {
            s.col = v;
        }
        if let Some(Indices::U32(v)) = mesh.remove_indices() {
            s.idx = v;
        }
        s.clear();
        s
    }

    pub fn clear(&mut self) {
        self.pos.clear();
        self.uv.clear();
        self.col.clear();
        self.idx.clear();
    }

    /// Put the refilled vectors back into the mesh they were taken from.
    pub fn put(mut self, mesh: &mut Mesh) {
        self.normals.clear();
        self.normals.resize(self.pos.len(), [0.0, 1.0, 0.0]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        mesh.insert_indices(Indices::U32(self.idx));
    }
}

/// Refill the strip mesh behind `handle` with `fill` (which writes into an emptied strip, and
/// keeps it from being empty where its builder did), in place. With the switch off: a new mesh
/// from a new strip, swapped in (the old path).
pub fn rewrite(meshes: &mut Assets<Mesh>, handle: &Handle<Mesh>, fill: impl FnOnce(&mut Strip)) {
    let Some(mut mesh) = meshes.get_mut(handle) else { return };
    if enabled() {
        let mut s = Strip::take(&mut mesh);
        fill(&mut s);
        s.put(&mut mesh);
    } else {
        let mut s = Strip::default();
        fill(&mut s);
        *mesh = s.into_mesh();
    }
}

#[cfg(test)]
pub(super) fn same_mesh(a: &Mesh, b: &Mesh) -> bool {
    let attrs = |m: &Mesh| m.attributes().map(|(id, v)| (id.id, format!("{v:?}"))).collect::<Vec<_>>();
    a.primitive_topology() == b.primitive_topology() && attrs(a) == attrs(b) && format!("{:?}", a.indices()) == format!("{:?}", b.indices())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill_a(s: &mut Strip) {
        for k in 0..5 {
            s.pos.push([k as f32, 1.0, 2.0]);
            s.uv.push([0.1 * k as f32, 0.5]);
            s.col.push([1.0, 0.5, 0.25, k as f32 / 4.0]);
        }
        s.idx.extend_from_slice(&[0, 1, 2, 2, 3, 4]);
    }

    #[test]
    fn a_refilled_strip_is_the_mesh_a_fresh_build_makes() {
        let mut fresh = Strip::default();
        fill_a(&mut fresh);
        let fresh = fresh.into_mesh();
        // Start from a bigger, different mesh and refill it.
        let mut m = Strip::default();
        for k in 0..9 {
            m.pos.push([9.0, k as f32, 0.0]);
            m.uv.push([1.0, 1.0]);
            m.col.push([0.0; 4]);
        }
        m.idx.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8]);
        let mut mesh = m.into_mesh();
        let mut s = Strip::take(&mut mesh);
        assert!(s.pos.is_empty() && s.pos.capacity() >= 9);
        fill_a(&mut s);
        s.never_empty();
        s.put(&mut mesh);
        assert!(same_mesh(&mesh, &fresh));
    }

    #[test]
    fn an_emptied_strip_keeps_one_degenerate_triangle() {
        let mut s = Strip::default();
        fill_a(&mut s);
        let mut mesh = s.into_mesh();
        let mut s = Strip::take(&mut mesh);
        s.never_empty();
        s.put(&mut mesh);
        assert_eq!(mesh.count_vertices(), 3);
        assert_eq!(mesh.indices().map(|i| i.len()), Some(3));
    }
}
