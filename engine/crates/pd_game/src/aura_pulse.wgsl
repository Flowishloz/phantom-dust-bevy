// The Aura Particle use glow (`aura_pulse.rs`): one "ghost" of a body mesh
// piece, drawn flat and additive, shifted sideways in screen space.
//
// The original's own flat-colour draw (`SAURA_vs`: skinless position through
// a matrix, `add o0.xy, r0.xy, cb0[24].xy` -- a screen offset added to the
// clip position -- and `SAURA_ps`: a constant colour) has exactly this shape:
// the same silhouette drawn several times at small screen offsets in one
// colour stacks into a soft halo round the body and spills over whatever is
// behind it. Here the ghost is the fighter's own skinned piece (the same
// joints, so it follows every frame of the animation), the offset is added to
// the clip xy scaled by the projection so it is a fixed distance in the
// camera plane (`shift.z` world units, whatever the range), and the ghost is
// pushed back a little along the view ray so the body's own depth hides it:
// what remains is the band just outside the silhouette.
#import bevy_pbr::{
    mesh_functions,
    skinning,
    mesh_view_bindings::view,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    view_transformations::position_world_to_clip,
}

// rgb = scene-linear light this ghost adds where it shows (already weighted).
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> glow: vec4<f32>;
// xy = unit direction of the shift in the camera plane, z = distance in world
// units, w = how far back along the view ray the ghost sits (world units).
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> shift: vec4<f32>;
// The piece's own colour texture: only its alpha is read, to keep the cut-outs
// (the body is alpha-tested at 0.5; a piece without a texture reads white).
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var tex_sampler: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
#ifdef SKINNED
    let world_from_local = skinning::skin_model(vertex.joint_indices, vertex.joint_weights, vertex.instance_index);
#else
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
#endif
    var p = (world_from_local * vec4<f32>(vertex.position, 1.0)).xyz;
#ifdef SKINNED
    let n = normalize(skinning::skin_normals(world_from_local, vertex.normal));
#else
    let n = normalize(mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index));
#endif
    p += normalize(p - view.world_position) * shift.w;
    var clip = position_world_to_clip(p);
    // Clip xy / w = the screen position, so adding `d * P` to clip xy moves the
    // picture by `d * P / w`, which is `d` world units at this depth.
    clip.x += shift.x * shift.z * view.clip_from_view[0][0];
    clip.y += shift.y * shift.z * view.clip_from_view[1][1];
    out.position = clip;
    out.world_position = vec4<f32>(p, 1.0);
    out.world_normal = n;
    out.uv = vertex.uv;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    // Only the side of the piece that faces away from the camera: it sits behind
    // the body's own front, so what shows is the band outside the silhouette and
    // the gaps between limbs, never a film over the body itself.
    let v = view.world_position - in.world_position.xyz;
    if dot(normalize(in.world_normal), v) > 0.0 {
        discard;
    }
    if textureSample(tex, tex_sampler, in.uv).a < 0.5 {
        discard;
    }
    // Add blend (`AlphaMode::Add`): alpha 0 keeps the framebuffer and adds this.
    out.color = vec4<f32>(glow.rgb, 0.0);
    return out;
}
