// Status/Environment's idle-orb core (docs/CAPSULE-VISUALS.md "Draw
// recipe": shaders `vkCAP01`/`vkCAP04M4`, both unlit 4-texture layered
// blends). Their own vertex format (0x17) carries only position + one UV
// set -- no normal, tangent or vertex colour -- so `orb_sphere_lit.wgsl`'s
// bump-mapped/specular math doesn't apply. All four texture slots
// (`alr::MaterialEntry::texture_idx`/`normal_idx`/`texture_idx2`/
// `texture_idx3`) are real, distinct layers -- confirmed 2026-09-28 by a
// raw byte dump matching the recipe's own per-kind numbers (Status
// 18,19,19,20; Environment 25,26,26,27) exactly -- so this now implements
// the real formula from the disassembled bytecode (docs "Other kinds",
// closing note), reading each layer at its own UV (independent scroll per
// the recipe's per-layer speeds), rather than an ad hoc 2-layer average.
#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::globals,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> orb_flags: vec4<f32>;  // x = formula (0 Status/vkCAP01, 1 Environment/vkCAP04M4)
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> orb_layer0: vec4<f32>; // speed.xy, fixed uv offset.xy, per texture slot
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> orb_layer1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> orb_layer2: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> orb_layer3: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var orb_tex0: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var orb_sampler0: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var orb_tex1: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var orb_sampler1: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var orb_tex2: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var orb_sampler2: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var orb_tex3: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var orb_sampler3: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world_pos = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.world_position = world_pos;
    out.position = position_world_to_clip(world_pos.xyz);
    out.world_normal = vec3<f32>(0.0, 1.0, 0.0);
    out.uv = vertex.uv;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    let t = globals.time * 30.0;
    let s0 = textureSample(orb_tex0, orb_sampler0, in.uv + orb_layer0.xy * t + orb_layer0.zw);
    let s1 = textureSample(orb_tex1, orb_sampler1, in.uv + orb_layer1.xy * t + orb_layer1.zw);
    let s2 = textureSample(orb_tex2, orb_sampler2, in.uv + orb_layer2.xy * t + orb_layer2.zw);
    let s3 = textureSample(orb_tex3, orb_sampler3, in.uv + orb_layer3.xy * t + orb_layer3.zw);

    // Debug: PD_ORB_UNLIT_LAYER=0..3 shows one texture layer alone (its own
    // scroll, no blend formula) -- for isolating which layer produces the
    // hard horizontal stripes (docs "Fixed 2026-09-28 (Laurence, round 3d)").
    if orb_flags.z > -0.5 {
        var out2: FragmentOutput;
        if orb_flags.z < 0.5 { out2.color = vec4<f32>(s0.rgb, 1.0); }
        else if orb_flags.z < 1.5 { out2.color = vec4<f32>(s1.rgb, 1.0); }
        else if orb_flags.z < 2.5 { out2.color = vec4<f32>(s2.rgb, 1.0); }
        else { out2.color = vec4<f32>(s3.rgb, 1.0); }
        return out2;
    }

    var c: vec3<f32>;
    if orb_flags.x > 0.5 {
        // vkCAP04M4 (Environment): t0*t1, t2 over by its own alpha, +t3,
        // saturated.
        c = s0.rgb * s1.rgb;
        c = mix(c, s2.rgb, s2.a);
        c = saturate(c + s3.rgb);
    } else {
        // vkCAP01 (Status): t0, then t1 and t2 blended over it by their own
        // alpha, then +t3. (The doc's own formula also multiplies by the
        // mesh's vertex colour throughout -- this format has none, so that
        // factor is 1.)
        c = s0.rgb;
        c = mix(c, s1.rgb, s1.a);
        c = mix(c, s2.rgb, s2.a);
        c = c + s3.rgb;
    }
    out.color = vec4<f32>(c, 1.0);
    return out;
}
