// The school aura's glow: the three filter passes of the original's aura post-process
// (`aura_glow.rs` has every address). One fullscreen quad per pass, drawn into a small
// 8-bit render target; the vertex stage is the quad's own clip position.
//
//   mode 0  MFilter     256x256 silhouette thumbnail -> 128x128 (4 taps, `MFilter_ps`)
//   mode 1  GFilter2    horizontal blur, 8 taps (two draws added: `GFilter2_ps`, tables 0x14048e990 / eb50)
//   mode 2  GFilter2    vertical blur, 8 taps, the first draw blended onto the previous frame's result
//                       scaled by the persistence (state 0xd101: ONE, CONSTANT_ALPHA), the second added
//                       (tables 0x14048ed10 / eed0)
//
// params.x = mode, params.y = the blur width term (block + 0xc4 = script f36 x 0.01), params.z = the
// persistence (script f35 / 255).
#import bevy_pbr::forward_io::{Vertex, VertexOutput, FragmentOutput}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var src: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var prev: texture_2d<f32>;

// `FUN_1402864e0`: weight i = exp(table_i / width), then divided by their sum. The table is
// `DAT_14048f090`: -(T_k + 0.25), T_k = 0, 1, 3, 6, 10, 15, 21, 28, in the program's own order.
const TABLE = array<f32, 8>(-1.25, -6.25, -15.25, -28.25, -0.25, -3.25, -10.25, -21.25);

// The tap offsets of the two draws of a blur pass, in texels (the vertex tables hold 1/32 texel
// units; the left edge / the right edge of the quad), group A: `v1.x` even = weights 0..3,
// group B: odd = weights 4..7.
const LA = array<f32, 4>(-47.0, -111.0, -175.0, -239.0);
const RA = array<f32, 4>(4047.0, 3983.0, 3919.0, 3855.0);
const LB = array<f32, 4>(15.0, 79.0, 143.0, 207.0);
const RB = array<f32, 4>(4111.0, 4175.0, 4239.0, 4303.0);

// `MFilter_ps` weights (`0x1402f8720`, `0x1402f8770`) and the taps' offsets in texels
// (`FUN_140286c60`: cb0[90], cb0[91]).
const DS_W = array<f32, 4>(0.20074599981307983, 0.14962700009346008, 0.20074599981307983, 0.44888100028038025);
const DS_O = array<vec2<f32>, 4>(vec2<f32>(-1.0, -3.0), vec2<f32>(3.0, -3.0), vec2<f32>(-3.0, 1.0), vec2<f32>(1.0, 1.0));

const LUM = vec3<f32>(0.3, 0.6, 0.1);

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(vertex.position.x, vertex.position.y, 0.5, 1.0);
    out.world_position = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out.world_normal = vec3<f32>(0.0, 0.0, 1.0);
    out.uv = vertex.uv;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

fn tap(p: vec2<f32>, size: f32) -> vec3<f32> {
    return textureSampleLevel(src, samp, p / size, 0.0).rgb;
}

// One 8-bit store: what the render target keeps.
fn q8(v: vec3<f32>) -> vec3<f32> {
    return round(clamp(v, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0) / 255.0;
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    let mode = i32(params.x + 0.5);
    let px = floor(in.position.xy);
    if mode == 0 {
        // MFilter: the thumbnail's own texel under each of the 4 taps (the destination texel
        // is 2 x 2 source texels, its centre at 2i + 1).
        let base = px * 2.0 + vec2<f32>(1.0);
        var sum = vec3<f32>(0.0);
        for (var k = 0; k < 4; k++) {
            sum += tap(base + DS_O[k], 256.0) * DS_W[k];
        }
        out.color = vec4<f32>(clamp(sum, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
        return out;
    }
    var w: array<f32, 8>;
    var total = 0.0;
    let s = 1.0 / max(params.y, 0.0001);
    for (var k = 0; k < 8; k++) {
        w[k] = exp(s * TABLE[k]);
        total += w[k];
    }
    for (var k = 0; k < 8; k++) {
        w[k] = w[k] / total;
    }
    let f = (px + vec2<f32>(0.5)) / 128.0;
    // The across-the-blur coordinate is the quad's other axis: 8207 / 32 - 256 = 0.46875 at the
    // top / left edge, +128 at the bottom / right (a half texel of the program's own offset).
    let across = mix(vec2<f32>(0.46875), vec2<f32>(128.46875), f);
    var a = vec3<f32>(0.0);
    var b = vec3<f32>(0.0);
    if mode == 1 {
        for (var k = 0; k < 4; k++) {
            a += tap(vec2<f32>(mix(LA[k], RA[k], f.x) / 32.0, across.y), 128.0) * w[k];
            b += tap(vec2<f32>(mix(LB[k], RB[k], f.x) / 32.0, across.y), 128.0) * w[4 + k];
        }
        out.color = vec4<f32>(clamp(q8(a) + clamp(b, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
        return out;
    }
    for (var k = 0; k < 4; k++) {
        a += tap(vec2<f32>(across.x, mix(LA[k], RA[k], f.y) / 32.0), 128.0) * w[k];
        b += tap(vec2<f32>(across.x, mix(LB[k], RB[k], f.y) / 32.0), 128.0) * w[4 + k];
    }
    let old = textureLoad(prev, vec2<i32>(px), 0).rgb;
    let first = q8(clamp(a, vec3<f32>(0.0), vec3<f32>(1.0)) + old * params.z);
    out.color = vec4<f32>(clamp(first + clamp(b, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    return out;
}
