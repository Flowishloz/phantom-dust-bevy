// Shadow receiving, shared by the stage material (stage_material.wgsl) and the
// fighters' dissolve material (dissolve.wgsl). Rust side and the evidence for
// every constant: shadows.rs and docs/SHADOWS.md.
//
// The original draws its shadow volumes into a stencil buffer and then blends
// one full-screen quad over every pixel the volumes cover (FUN_140299310):
//
//     out = C * A + dst * (1 - A)        blend = DEST_ALPHA, INV_DEST_ALPHA
//
// `C` is the stage script's shadow colour (call 02 sub 1: st03 = (15,18,25))
// and `A` is the alpha each surface pixel shader wrote into the framebuffer:
// SD_ps / SBTS_ps / SWBOS_ps all write
//
//     A = 2 * (0.8 * sat(N.L) + 0.00806) * (2 * texture_alpha - 1.5)
//
// i.e. the darkening scales with how directly the surface faces the light, and
// it is applied in the game's gamma space. Here the stencil test is a shadow
// map lookup (Bevy's directional shadow maps, fed the authored volume meshes
// and the fighters as casters); the blend is exactly the original's.

#define_import_path pd_game::shadows

#import bevy_pbr::{
    mesh_view_bindings as view_bindings,
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    shadows::fetch_directional_shadow,
}

// The stage's own volumes as a point-in-volume test (shadow_grid.rs): the
// original's stencil semantics. Word 0 = triangle count (0 = no volumes).
// Both receiving materials (the stage's and the fighters') bind it at 103.
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var<storage, read> vol: array<u32>;

// Winding number of the volumes around `p`: a ray straight up, the sum of
// sign(n.y) over the triangles it crosses (outward closed meshes: +1 inside,
// 0 outside). Mirrors shadow_grid.rs::winding.
fn volume_winding(p: vec3<f32>) -> i32 {
    if (vol[0] == 0u) {
        return 0;
    }
    let nx = i32(vol[1]);
    let nz = i32(vol[2]);
    let cell = bitcast<f32>(vol[3]);
    let x0 = bitcast<f32>(vol[4]);
    let z0 = bitcast<f32>(vol[5]);
    let idx_off = vol[6];
    let tri_off = vol[7];
    let cx = i32(floor((p.x - x0) / cell));
    let cz = i32(floor((p.z - z0) / cell));
    if (cx < 0 || cz < 0 || cx >= nx || cz >= nz) {
        return 0;
    }
    let ci = 8u + 2u * (u32(cz) * u32(nx) + u32(cx));
    let start = vol[ci];
    let count = vol[ci + 1u];
    // Fixed sub-millimetre shift in x / z (shadow_grid.rs JITTER): no exact hits on shared edges.
    let q = vec3<f32>(p.x + 0.000731, p.y + 0.05, p.z + 0.000419);
    var w = 0;
    for (var k = 0u; k < count; k = k + 1u) {
        let o = tri_off + vol[idx_off + start + k] * 12u;
        let a = vec3<f32>(bitcast<f32>(vol[o]), bitcast<f32>(vol[o + 1u]), bitcast<f32>(vol[o + 2u]));
        let b = vec3<f32>(bitcast<f32>(vol[o + 4u]), bitcast<f32>(vol[o + 5u]), bitcast<f32>(vol[o + 6u]));
        let c = vec3<f32>(bitcast<f32>(vol[o + 8u]), bitcast<f32>(vol[o + 9u]), bitcast<f32>(vol[o + 10u]));
        let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
        if (abs(d) < 1e-12) {
            continue;
        }
        let l1 = ((b.z - c.z) * (q.x - c.x) + (c.x - b.x) * (q.z - c.z)) / d;
        let l2 = ((c.z - a.z) * (q.x - c.x) + (a.x - c.x) * (q.z - c.z)) / d;
        let l3 = 1.0 - l1 - l2;
        if (l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0 && l1 * a.y + l2 * b.y + l3 * c.y > q.y) {
            w = w + i32(bitcast<f32>(vol[o + 3u]));
        }
    }
    return w;
}

struct ShadowSample {
    // 1 = lit, 0 = inside a shadow volume (minimum over the shadow lights).
    lit: f32,
    // The shadow colour (the shadow lights' colour, linear; black without one).
    colour: vec3<f32>,
}

// Shadow lookup for one fragment. `n` only feeds the shadow-map normal bias.
fn shadow_sample(world_position: vec4<f32>, n: vec3<f32>, frag_xy: vec2<f32>) -> ShadowSample {
    var out: ShadowSample;
    out.lit = 1.0;
    out.colour = vec3<f32>(0.0);
    let view_z = dot(vec4<f32>(
        view_bindings::view.view_from_world[0].z,
        view_bindings::view.view_from_world[1].z,
        view_bindings::view.view_from_world[2].z,
        view_bindings::view.view_from_world[3].z
    ), world_position);
    let n_lights = view_bindings::lights.n_directional_lights;
    for (var i: u32 = 0u; i < n_lights; i = i + 1u) {
        let flags = view_bindings::lights.directional_lights[i].flags;
        if ((flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
            out.lit = min(out.lit, fetch_directional_shadow(i, world_position, n, view_z, frag_xy));
            out.colour = view_bindings::lights.directional_lights[i].color.rgb;
        }
    }
    // The stage's own volumes, exactly: inside any of them = shadowed.
    if (volume_winding(world_position.xyz) != 0) {
        out.lit = 0.0;
    }
    return out;
}

// The framebuffer alpha the surface shaders write (see the header): the most
// the shadow pass can darken this pixel. `n_dot_l` = N . (towards the light),
// `tex_alpha` = the base texture's alpha (1 for opaque surfaces).
fn shadow_weight(n_dot_l: f32, tex_alpha: f32) -> f32 {
    let a = 2.0 * (clamp(n_dot_l, 0.0, 1.0) * 0.8 + 0.00806) * (2.0 * tex_alpha - 1.5);
    return clamp(a, 0.0, 1.0);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// `out = C * k + dst * (1 - k)` in gamma space, back to linear. `k` = 0 returns
// the input untouched (bit for bit): there is no global dimming outside shadows.
fn shadow_darken(colour_linear: vec3<f32>, shadow_linear: vec3<f32>, k: f32) -> vec3<f32> {
    if (k <= 0.0) {
        return colour_linear;
    }
    let g = linear_to_srgb(colour_linear);
    return srgb_to_linear(mix(g, linear_to_srgb(shadow_linear), k));
}
