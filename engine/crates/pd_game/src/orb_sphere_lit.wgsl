// Idle-orb sphere core, lit variant (docs/CAPSULE-VISUALS.md "Draw recipe
// (decoded 2026-09-28)": shader `vkCAP02`/`vkCAP02M2`): the base "flame"
// texture times a moving, bump-perturbed light term, plus a tight
// `(N.H)^16` specular highlight in the kind's own colour. Used by every
// idle-orb kind except Status/Environment, whose core shader
// (`vkCAP01`/`vkCAP04M4`) is unlit and uses a simpler vertex format with no
// normal/tangent/colour at all -- see `orb_sphere_unlit.wgsl`.
//
// Two mesh shapes feed this shader: format 0x1D ("M2" kinds: Attack, Erase)
// carries two independent UV sets (uv0 for the flame, uv1 for both bump
// samples); format 0x03 (every other lit kind) has only uv0, and the
// recipe's own per-kind table has the bump samples reuse it at their own
// scroll speeds instead. Handled here by reading `uv_b` when the mesh has a
// second UV set (`VERTEX_UVS_B`) and falling back to `uv` otherwise.
//
// Billboarding (Aura + both opponent-view kinds, `orb_flags.x`): the game
// replaces a joint's rotation with the camera view's own basis every frame
// (docs "Billboarding": local Z becomes camera-forward). We have one rigid
// mesh per orb rather than a skinned joint, so the equivalent here is
// rotating the whole mesh's vertices/normal/tangent into the camera's basis
// in the vertex shader -- the same effect for a single-joint object, and
// it's what keeps the sphere's own (partial) UV unwrap always showing its
// best side to the camera, which is the point of doing it at all.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> orb_flags: vec4<f32>;   // x = billboard (0/1), y = has_bump (0/1)
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> orb_spec: vec4<f32>;    // rgb = specular tint, w = specular exponent
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> orb_layer0: vec4<f32>;  // flame: speed.xy, fixed uv offset.xy
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> orb_layer1: vec4<f32>;  // bump sample A
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> orb_layer2: vec4<f32>;  // bump sample B
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<uniform> orb_edge: vec4<f32>;    // x = edge power, y = centre darken, z = edge glow strength
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var orb_base_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var orb_base_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var orb_bump_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var orb_bump_sampler: sampler;

// Stand-in fixed key light (docs "Remaining unknowns": the scene's real
// light values are stage-dependent and weren't recovered this pass). Named
// constants, unconfirmed against the original -- tuned by eye against
// Laurence's own footage instead.
const ORB_LIGHT_DIR: vec3<f32> = vec3<f32>(0.35, 0.75, 0.4);
const ORB_LIGHT_COLOR: vec3<f32> = vec3<f32>(0.8, 0.76, 0.68);
const ORB_SKY_COLOR: vec3<f32> = vec3<f32>(0.12, 0.11, 0.12);
const ORB_GROUND_COLOR: vec3<f32> = vec3<f32>(0.03, 0.02, 0.02);
// Gain on the bump-map perturbation before it's folded into the normal --
// tuned up from a literal 1x reading of the disassembled formula so
// neighbouring texels swing between lit and unlit sharply enough to read as
// the footage's "bright streaks and black gaps crawling across the flame"
// rather than a smoothly, uniformly lit ball (this doc's own reading: "N.L
// swings hard from texel to texel" -- the exact per-texel swing depends on
// the bump texture's own authored contrast, which we can't measure without
// the original's tonemapping pipeline, so this is tuned by eye).
const ORB_BUMP_GAIN: f32 = 3.4;
// View-dependent edge brightening (Laurence, on `attack_round3b.png`: "the
// original is darker in the middle and brighter on the sides, as if it's
// hotter and is emitting more light, making the edges less defined"). The
// disassembled core shader has no fresnel/rim term of its own (docs
// "Shaders": "No fresnel / rim term in the core shader") -- but it also
// writes to a second render target (`blend_colour`) the doc never chased
// down ("What the glow buffer... adds; may contribute to the footage's soft
// halo"), which is the likely real source of this. Approximated here as a
// straightforward view-dependent (Fresnel) term via the `orb_edge` uniform
// (x = power, y = centre darken floor, z = edge glow strength): darkens the
// centre, brightens/warms the silhouette. Values come from `OrbTuning` in
// Rust so the proof sheet can render labelled A/B/C variants without a
// rebuild.

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world_pos: vec3<f32>;
    var world_normal: vec3<f32>;
    if orb_flags.x > 0.5 {
        let centre = (world_from_local * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
        let scale = length(world_from_local[0].xyz);
        let cam_right = normalize(view.world_from_view[0].xyz);
        let cam_up = normalize(view.world_from_view[1].xyz);
        let cam_fwd = -normalize(view.world_from_view[2].xyz);
        world_pos = centre + (cam_right * vertex.position.x + cam_up * vertex.position.y + cam_fwd * vertex.position.z) * scale;
        world_normal = normalize(cam_right * vertex.normal.x + cam_up * vertex.normal.y + cam_fwd * vertex.normal.z);
    } else {
        world_pos = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0)).xyz;
        world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
    }
    out.world_position = vec4<f32>(world_pos, 1.0);
    out.position = position_world_to_clip(world_pos);
    out.world_normal = world_normal;
    out.uv = vertex.uv;
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    if orb_flags.x > 0.5 {
        let cam_right = normalize(view.world_from_view[0].xyz);
        let cam_up = normalize(view.world_from_view[1].xyz);
        let cam_fwd = -normalize(view.world_from_view[2].xyz);
        let t = cam_right * vertex.tangent.x + cam_up * vertex.tangent.y + cam_fwd * vertex.tangent.z;
        out.world_tangent = vec4<f32>(normalize(t), vertex.tangent.w);
    } else {
        out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex.instance_index);
    }
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    let t = globals.time * 30.0; // frames at 30 fps (docs: T advances ~1/game-update)
    let uv0 = in.uv + orb_layer0.xy * t + orb_layer0.zw;
    let flame = textureSample(orb_base_tex, orb_base_sampler, uv0).rgb;

    let n_smooth = normalize(in.world_normal);
    let view_dir = normalize(view.world_position - in.world_position.xyz);
    // Fresnel edge term computed on the smooth (pre-bump) normal, not the
    // noisy bump-perturbed one below -- a clean, silhouette-shaped falloff
    // rather than a streaky one.
    let ndotv = clamp(dot(n_smooth, view_dir), 0.0, 1.0);
    let edge = pow(1.0 - ndotv, max(orb_edge.x, 0.01));

    var n = n_smooth;
#ifdef VERTEX_TANGENTS
    if orb_flags.y > 0.5 {
        let tangent = normalize(in.world_tangent.xyz);
        let bitangent = cross(n, tangent) * in.world_tangent.w;
#ifdef VERTEX_UVS_B
        let bump_base = in.uv_b;
#else
        let bump_base = in.uv;
#endif
        let uvA = bump_base + orb_layer1.xy * t + orb_layer1.zw;
        let uvB = bump_base + orb_layer2.xy * t + orb_layer2.zw;
        let nA = textureSample(orb_bump_tex, orb_bump_sampler, uvA).rgb;
        let nB = textureSample(orb_bump_tex, orb_bump_sampler, uvB).rgb;
        // (nA-0.5)*2 + (nB-0.5)*2, then one cheap renormalise step -- exact
        // formula from the disassembled pixel shader (docs "Shaders").
        var tn = ((nA - vec3<f32>(0.5)) * 2.0 + (nB - vec3<f32>(0.5)) * 2.0) * ORB_BUMP_GAIN;
        tn = tn + tn * 0.5 * (1.0 - dot(tn, tn));
        n = normalize(tangent * tn.x + bitangent * tn.y + n * tn.z);
    }
#endif

    let light_dir = normalize(ORB_LIGHT_DIR);
    let half_dir = normalize(light_dir + view_dir);
#ifdef VERTEX_COLORS
    let vcol_a = in.color.a;
#else
    let vcol_a = 1.0;
#endif

    let diffuse = clamp(dot(n, light_dir), 0.0, 1.0) * ORB_LIGHT_COLOR;
    let d = clamp(dot(n, vec3<f32>(0.0, 1.0, 0.0)) * 0.5 + 0.5, 0.0, 1.0);
    let ambient = d * ORB_SKY_COLOR + (1.0 - d) * ORB_GROUND_COLOR;
    let lit = (diffuse + ambient) * vcol_a;
    let spec = clamp(pow(clamp(dot(n, half_dir), 0.0, 1.0), max(orb_spec.w, 1.0)), 0.0, 1.0) * orb_spec.rgb * vcol_a;

    // Centre-darken / edge-brighten (see `orb_edge`'s own doc comment above):
    // scale the lit flame down toward the centre (`orb_edge.y` floor) and up
    // toward the silhouette, then add a separate warm glow that only shows
    // near the silhouette -- approximating the unread glow-buffer render
    // target rather than a real fresnel term in the disassembled shader.
    // Billboarded kinds (Aura, both opponent views, `orb_flags.x`) are
    // always viewed dead-on, so a "near the silhouette" band covers a much
    // bigger share of the visible disc than on a core seen from an
    // incidental angle. A damped (not zero) version of this effect *still*
    // darkened their centres by the same floor regardless of the damping --
    // `edge` is 0 dead-centre no matter how much `edge_strength` shrinks its
    // ramp-up, and `centre_scale = mix(orb_edge.y, 1.0, 0 * anything)` is
    // still `orb_edge.y` -- which is why the damped version still blew these
    // three out to a flat, washed disc with a black centre dot instead of
    // the round-3b look Laurence asked to keep for them. Fixed by skipping
    // the whole effect outright for billboarded cores rather than damping
    // it: centre_scale forced to 1 and no edge glow at all.
    let is_billboard = orb_flags.x > 0.5;
    let centre_scale = select(mix(orb_edge.y, 1.0, edge), 1.0, is_billboard);
    let warm = mix(orb_spec.rgb, vec3<f32>(1.0, 0.55, 0.15), 0.5);
    let edge_glow = select(warm * pow(edge, 2.0) * orb_edge.z * vcol_a, vec3<f32>(0.0), is_billboard);

    out.color = vec4<f32>(lit * flame * centre_scale + spec + edge_glow, 1.0);
    return out;
}
