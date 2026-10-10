// Capsule regeneration motes (`vkPSPRITE`, docs/CAPSULE-VISUALS.md
// "Regeneration motes (2026-10-01)").
//
// Vertex (`vkPSPRITE_vs`, read from the disassembly): the point is moved to
// clip space like any vertex, then each of its quad corners is pushed out in
// CLIP space by `quad_pos * c32.x * (0.001172, 0.002083)` -- so the quad's
// size is constant in the projection, not in pixels. We put the offset in
// view space (`corner * K / P11`), which is the same thing for the vertical
// size; the horizontal size follows the vertical so the quad stays square in
// any split-screen viewport (the program's constant assumes a 16:9 viewport).
//
// Pixel (`vkPSPRITE_ps`): colour = strip(uv_b) * dot(uv) (rgb), alpha =
// strip.a * dot.a, discarded at alpha 0; the strip's UV is the point's own
// UV plus the draw's scroll (computed on the CPU, mesh UV_1). The game adds
// these in display space; we decode (x^2.2) so our linear framebuffer adds the
// same amounts, as the orb rim does.
#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sprite: vec4<f32>; // x = half size K (view units * P11), y = blend (0 add, 1 one-minus-src-colour), z = gain, w = 1: add the stored value (gamma-space add)
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var strip_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var strip_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var dot_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var dot_samp: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let wp = world_from_local * vec4<f32>(vertex.position, 1.0);
    var vp = view.view_from_world * wp;
    let corner = vertex.uv * 2.0 - vec2<f32>(1.0, 1.0);
    let p11 = view.clip_from_view[1][1];
    vp = vec4<f32>(vp.xy + corner * (sprite.x / p11), vp.z, 1.0);
    out.position = view.clip_from_view * vp;
    out.world_position = view.world_from_view * vp;
    out.world_normal = vec3<f32>(0.0, 1.0, 0.0);
    out.uv = vertex.uv;
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
#ifdef VERTEX_UVS_B
    let s0 = textureSample(strip_tex, strip_samp, in.uv_b);
#else
    let s0 = textureSample(strip_tex, strip_samp, in.uv);
#endif
    let s3 = textureSample(dot_tex, dot_samp, in.uv);
    if s0.a * s3.a <= 0.0 {
        discard;
    }
    let inv = vec3<f32>(1.0 / 2.2);
    let disp = clamp(pow(max(s0.rgb, vec3<f32>(0.0)), inv) * pow(max(s3.rgb, vec3<f32>(0.0)), inv) * sprite.z, vec3<f32>(0.0), vec3<f32>(1.0));
    if sprite.y > 0.5 {
        // Opponent's view: blend ZERO / ONE_MINUS_SRC_COLOUR (`FUN_14028e4e0(0x3001)`),
        // i.e. dst * (1 - colour); the multiply pipeline wants src.a = 1.
        out.color = vec4<f32>(pow(vec3<f32>(1.0) - disp, vec3<f32>(2.2)), 1.0);
    } else {
        // Owner's view: ONE + ONE (material blend 0x1100); alpha 0 so Bevy's
        // premultiplied-add pipeline keeps all of dst and adds src.
        // sprite.w = 0: add the decoded (linear) product, as the regeneration motes always did.
        // sprite.w = 1 (the pickup / erase bursts): add the stored value itself, the way
        // `fx_gamma.wgsl` does for every skill effect, because the game adds in gamma space
        // (docs/FORMATS-EFFECTS.md "Blending: the game adds in gamma space"): over a lit floor
        // a decoded add is a fraction of what the original adds, which left the bursts invisible.
        out.color = vec4<f32>(select(pow(disp, vec3<f32>(2.2)), disp, sprite.w > 0.5), 0.0);
    }
    return out;
}
