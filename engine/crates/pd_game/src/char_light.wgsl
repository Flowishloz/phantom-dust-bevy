// Player-character lighting: the original's SWBOS pixel shader (see
// char_light.rs for the formula and its evidence). An import module, called
// from dissolve.wgsl (the fighters' one material) so the wall dissolve and
// this lighting share a single fragment path.
//
// Everything below is in the game's gamma space: the base colour is converted
// back to gamma, lit, saturated, and converted to linear for Bevy's tone
// mapper (Laurence's kept default) -- the same way the stages' baked vertex
// colours are stored.

#define_import_path pd_game::char_light

#import bevy_pbr::{
    pbr_bindings,
    mesh_view_bindings::view,
    forward_io::VertexOutput,
}

struct CharLight {
    dl_col: vec4<f32>,  // cDL_COL
    dl_dir: vec4<f32>,  // cDL_VEC: light travel direction (world)
    amb: vec4<f32>,     // cS0_COLp
    hemi_col: vec4<f32>, // cS1_COLp
    axis: vec4<f32>,    // hemisphere axis (world)
    pt_col: vec4<f32>,  // cP0_COLp
    pt_pos: vec4<f32>,  // xyz position, w = intensity (scaled by 1/d^2)
    spec: vec4<f32>,    // cSPECULAR
    x_col: array<vec4<f32>, 3>, // extra effect lights ("4 per fighter", look.rs): zero = none
    x_pos: array<vec4<f32>, 3>,
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// spec77.dds sampled at coordinate c (clamp, bilinear, 256 texels).
// RGB: texel i holds i/255 -> a ramp.
fn lut_rgb(c: f32) -> f32 {
    return clamp((c * 256.0 - 0.5) / 255.0, 0.0, 1.0);
}

// spec77.dds alpha: texel i holds floor(255 * (i/255)^20) / 255.
fn lut_texel_alpha(i: i32) -> f32 {
    let k = f32(clamp(i, 0, 255));
    return floor(255.0 * pow(k / 255.0, 20.0)) / 255.0;
}

fn lut_alpha(c: f32) -> f32 {
    let t = clamp(c * 256.0 - 0.5, 0.0, 255.0);
    let i = i32(floor(t));
    let f = t - f32(i);
    return mix(lut_texel_alpha(i), lut_texel_alpha(i + 1), f);
}

// One effect (point) light's contribution: the original's slot-2 maths.
fn point_light(n: vec3<f32>, world_pos: vec3<f32>, col: vec3<f32>, pos: vec4<f32>) -> vec3<f32> {
    if (all(col == vec3<f32>(0.0))) {
        return vec3<f32>(0.0);
    }
    let d = world_pos - pos.xyz;
    let d2 = max(dot(d, d), 1e-6);
    let lp = (-d / sqrt(d2)) * (pos.w / d2);
    let e = clamp(lp * 0.5 + 0.5, vec3<f32>(-1.0), vec3<f32>(1.0));
    let dec = e * 2.0 - 1.0;
    return col * clamp(dot(n, dec), 0.0, 1.0);
}

// N: mapped world normal, base: linear base colour (from the texture),
// nm_a: the normal map's alpha (specular mask), spec_w: the per-vertex
// specular weight. Returns linear rgb (pre tone map).
fn shade(
    l: CharLight,
    n: vec3<f32>,
    base_linear: vec3<f32>,
    nm_a: f32,
    spec_w: f32,
    world_pos: vec3<f32>,
) -> vec3<f32> {
    let base = linear_to_srgb(base_linear);
    let fwd = -view.world_from_view[2].xyz;
    let dir = l.dl_dir.xyz;
    let u = dot(n, -dir);
    let s = normalize(dir + 0.5 * fwd);
    let v = -dot(n, s);
    let ramp = lut_rgb(u);
    let lobe = lut_alpha(v);
    var p = point_light(n, world_pos, l.pt_col.xyz, l.pt_pos);
    for (var i = 0; i < 3; i = i + 1) {
        p = p + point_light(n, world_pos, l.x_col[i].xyz, l.x_pos[i]);
    }
    let h = dot(n, l.axis.xyz);
    let lit = l.amb.xyz + l.dl_col.xyz * ramp + p + l.hemi_col.xyz * (h * h);
    let spec = clamp(l.spec.xyz * (spec_w * nm_a * lobe), vec3<f32>(0.0), vec3<f32>(1.0));
    let out_g = clamp(base * lit + spec, vec3<f32>(0.0), vec3<f32>(1.0));
    return srgb_to_linear(out_g);
}

// The normal map's alpha at this fragment (1 when the mesh has no map: the
// material's fallback texture is opaque white).
fn normal_map_alpha(uv: vec2<f32>) -> f32 {
    return textureSampleBias(pbr_bindings::normal_map_texture, pbr_bindings::normal_map_sampler, uv, view.mip_bias).a;
}
