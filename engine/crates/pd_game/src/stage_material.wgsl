// The stage's receiving material (stage_material.rs).
//
// Every draw: Bevy's own unlit forward fragment for the stage's baked-vertex-
// colour x texture surfaces, plus the original's shadow blend (shadows.wgsl).
// Where no shadow falls the result is the standard material's, bit for bit
// (`shadow_darken` returns its input for k = 0); everything else (alpha cutout,
// vertex colours, fog, tone mapping) is Bevy's standard path, the same tail as
// `pbr.wgsl`.
//
// Bump-mapped draws (stage_mat.params.y = 1: vertex format 0x03, material flag
// 0x20): the original's SBTS pixel shader (docs/STAGE-LIGHT.md), in the game's
// gamma space, replaces the baked colour. SBTS_vs builds tangent-space light
// vectors per vertex; here the directional / hemisphere / half vectors are
// built per pixel from the interpolated frame (the maths are the same, the
// interpolation is not). The POINT light (o4) is the exception: it is computed
// per vertex in `vertex` below, exactly like SBTS_vs, and interpolated, because
// it does not interpolate like a direction (range / d^2 saturates near the light):
// per pixel it lit whole faces near an effect light as hard-edged solid panels
// (the Bomb's "square" behind the flying bomb, 2026-10-09, BUG-GRAVEYARD.md).

#import bevy_pbr::{
    pbr_bindings,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
    mesh_functions,
    view_transformations::position_world_to_clip,
}
#ifdef SKINNED
#import bevy_pbr::skinning
#endif
#import pd_game::shadows::{shadow_sample, shadow_weight, shadow_darken, srgb_to_linear, linear_to_srgb}
#import pd_game::char_light::{lut_rgb, lut_alpha}

struct StageLight {
    dl_col: vec4<f32>,  // cDL_COL
    dl_dir: vec4<f32>,  // cDL_VEC: light travel direction (world)
    s0: vec4<f32>,      // cS0_COLp
    s1: vec4<f32>,      // cS1_COLp
    axis: vec4<f32>,    // hemisphere axis (world)
    p0_col: vec4<f32>,  // cP0_COLp
    p0_pos: vec4<f32>,  // xyz position, w = scalar (intensity / d^2)
}

struct StageMat {
    spec: vec4<f32>,    // specular colour bytes / 255 (xyz)
    params: vec4<f32>,  // x = fresnel constant k, y = 1 for a bump draw, z = 1: point light per pixel (old, PD_STAGE_POINT_PIXEL=1)
}

// xyz = unit vector from the surface towards the stage's light (set 0 of the
// script, right-handed world); w unused.
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> stage_light: vec4<f32>;
// One storage buffer shared by every stage draw (stage_material.rs `light_buffer`), written in place.
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> light: StageLight;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<uniform> stage_mat: StageMat;

// SBTS_vs's point-light varying o4 (before the pixel shader's clamp):
// 0.5 * (-to.T, -to.B, +to.N) * range / d^2 + 0.5, `to` = unit vector from the
// point to the light. Zero light -> 0.5 (decodes to no light).
fn point_o4(pos: vec3<f32>, nrm: vec3<f32>, tan: vec3<f32>, bit: vec3<f32>) -> vec3<f32> {
    if (all(light.p0_col.xyz == vec3<f32>(0.0))) {
        return vec3<f32>(0.5);
    }
    let d = pos - light.p0_pos.xyz;
    let d2 = max(dot(d, d), 1e-6);
    let to = -d / sqrt(d2);
    let p_ts = vec3<f32>(-dot(to, tan), -dot(to, bit), dot(to, nrm)) * (light.p0_pos.w / d2);
    return p_ts * 0.5 + 0.5;
}

// The vertex stage: Bevy's own `mesh.wgsl` vertex (no morph targets: stages have
// none), plus SBTS_vs's o4 for a bump draw, carried in the colour varying's rgb
// (a bump draw's fragment never reads the colour's rgb: SBTS replaces the baked
// colour; its alpha, the cutout's, is left as it was).
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
#ifdef SKINNED
    var world_from_local = skinning::skin_model(vertex.joint_indices, vertex.joint_weights, vertex.instance_index);
#else
    var world_from_local = mesh_world_from_local;
#endif
#ifdef VERTEX_NORMALS
#ifdef SKINNED
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#endif
#endif
#ifdef VERTEX_POSITIONS
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#endif
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex.instance_index);
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#ifdef VERTEX_TANGENTS
#ifdef VERTEX_NORMALS
#ifdef VERTEX_POSITIONS
    if (stage_mat.params.y > 0.5 && stage_mat.params.z < 0.5) {
        let nrm = normalize(out.world_normal);
        let tan = normalize(out.world_tangent.xyz);
        let bit = cross(nrm, tan) * out.world_tangent.w;
        out.color = vec4<f32>(point_o4(out.world_position.xyz, nrm, tan, bit), vertex.color.a);
    }
#endif
#endif
#endif
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(vertex.instance_index, mesh_world_from_local[3]);
#endif
    return out;
}

struct Bump {
    // Gamma-space output (before the framebuffer clamp).
    rgb: vec3<f32>,
    // sat(N.L) of the mapped normal: the shadow pass's weight input.
    nl: f32,
}

#ifdef VERTEX_TANGENTS
#ifdef VERTEX_UVS_B
// SBTS_vs + SBTS_ps. `tex_a` = the base texture's alpha.
fn bump(in: VertexOutput) -> Bump {
    let nrm = normalize(in.world_normal);
    let tan = normalize(in.world_tangent.xyz);
    let bit = cross(nrm, tan) * in.world_tangent.w;
    let pos = in.world_position.xyz;

    // ---- SBTS_vs ----
    let to_l = -light.dl_dir.xyz;
    let v = normalize(view.world_position.xyz - pos);
    let h = normalize(v + to_l);
    let l_ts = vec3<f32>(dot(to_l, tan), dot(to_l, bit), dot(to_l, nrm));      // o5
    let h_ts = vec3<f32>(dot(h, tan), dot(h, bit), dot(h, nrm));               // o6
    let ax = light.axis.xyz;
    let a_ts = vec3<f32>(-dot(ax, tan), -dot(ax, bit), dot(ax, nrm));          // o3 (hemisphere)
    var lp = vec3<f32>(0.0);                                                   // o4 (point light), decoded
    if (any(light.p0_col.xyz != vec3<f32>(0.0))) {
        // The vertex stage's interpolated o4 (SBTS_vs), or the old per-pixel one.
        var o4 = point_o4(pos, nrm, tan, bit);
#ifdef VERTEX_COLORS
        if (stage_mat.params.z < 0.5) {
            o4 = in.color.rgb;
        }
#endif
        lp = clamp(o4, vec3<f32>(-1.0), vec3<f32>(1.0)) * 2.0 - 1.0;
    }
    let spec_w = in.uv_b.y;                                                    // COLOR0.w
    let fres = clamp(spec_w * (stage_mat.params.x - max(dot(nrm, v), 0.0)), -1.0, 1.0);
    let ao = clamp(in.uv_b.x, -1.0, 1.0);                                      // COLOR0.x

    // ---- SBTS_ps ----
    let tex = textureSampleBias(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, in.uv, view.mip_bias);
    let nm = textureSampleBias(pbr_bindings::normal_map_texture, pbr_bindings::normal_map_sampler, in.uv, view.mip_bias);
    let base = linear_to_srgb(tex.rgb);
    let n = 2.0 * (nm.rgb - 0.5);
    let lut_u = lut_rgb(dot(n, l_ts));        // t3 (N.L, N.H): rgb is the ramp itself,
    let lut_a = lut_alpha(dot(n, h_ts));      // alpha the specular lobe
    let p = clamp(dot(lp, n), 0.0, 1.0);
    let dl_term = clamp(vec3<f32>(lut_u) * light.dl_col.xyz, vec3<f32>(0.0), vec3<f32>(1.0));
    let h_raw = dot(a_ts, n) * 0.5 + 0.5;
    let amb = clamp(light.s0.xyz * clamp(h_raw, 0.0, 1.0) + light.s1.xyz * (1.0 - h_raw), vec3<f32>(0.0), vec3<f32>(1.0));
    let pt = clamp(light.p0_col.xyz * p, vec3<f32>(0.0), vec3<f32>(1.0));
    let dlp = clamp(pt * light.p0_col.xyz + dl_term, vec3<f32>(0.0), vec3<f32>(1.0));
    let lit = clamp(dlp * base + base * amb, vec3<f32>(0.0), vec3<f32>(1.0)) * ao;
    let c_spec = stage_mat.spec.xyz * 2.0 * light.dl_col.xyz;
    let spec = clamp(c_spec * (nm.a * lut_a * fres), vec3<f32>(0.0), vec3<f32>(1.0));
    var out: Bump;
    out.rgb = max(lit, vec3<f32>(0.0)) + spec;
    out.nl = clamp(lut_u, 0.0, 1.0);
    return out;
}
#endif
#endif

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    // Derivatives first (uniform control flow): the stage carries no normals,
    // so the face normal is the shadow bias normal and the N.L of the weight.
    let dpx = dpdx(in.world_position.xyz);
    let dpy = dpdy(in.world_position.xyz);
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: FragmentOutput;
#ifdef VERTEX_TANGENTS
#ifdef VERTEX_UVS_B
    if (stage_mat.params.y > 0.5) {
        let b = bump(in);
        var geo = normalize(in.world_normal);
        if (!is_front) {
            geo = -geo;
        }
        let a = pbr_input.material.base_color.a;
        let sh = shadow_sample(in.world_position, geo, in.position.xy);
        let lit = srgb_to_linear(clamp(b.rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
        out.color = vec4<f32>(shadow_darken(lit, sh.colour, shadow_weight(b.nl, a) * (1.0 - sh.lit)), a);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }
#endif
#endif
    if ((pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u) {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    var n = normalize(cross(dpx, dpy));
    let to_eye = view.world_position.xyz - in.world_position.xyz;
    if (dot(n, to_eye) < 0.0) {
        n = -n;
    }
    let weight = shadow_weight(dot(n, stage_light.xyz), pbr_input.material.base_color.a);
    let sh = shadow_sample(in.world_position, n, in.position.xy);
    out.color = vec4<f32>(shadow_darken(out.color.rgb, sh.colour, weight * (1.0 - sh.lit)), out.color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
