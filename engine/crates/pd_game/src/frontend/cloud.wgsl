// The menu's cloud backdrop: StMenu's 967-vertex layer, `SBTSCLOUD2/3` style.
// READ (`SBTSCLOUD2_vs`): three UV sets from the one vertex UV, each plus its own
// scroll constant: set 0 += c90.xy, set 1 += c90.zw, set 2 += c91.xy; they sample
// texture 0 (cloud colour, st3), 1 (st4) and 2 (st5), the last two being
// normal maps whose alpha masks the layers (`SBTSCLOUD2_ps`).
// UNCONFIRMED: the original's light constants, so the bump term is off unless
// PD_CLOUD_BUMP > 0 (A/B only).
#import bevy_ui::ui_vertex_output::UiVertexOutput

struct Params {
    tint: vec4<f32>,
    off01: vec4<f32>,
    off2: vec4<f32>, // xy = set 2 offset, z = tiles across, w = bump strength
};
@group(1) @binding(0) var<uniform> p: Params;
@group(1) @binding(1) var t0: texture_2d<f32>;
@group(1) @binding(2) var s0: sampler;
@group(1) @binding(3) var t1: texture_2d<f32>;
@group(1) @binding(4) var s1: sampler;
@group(1) @binding(5) var t2: texture_2d<f32>;
@group(1) @binding(6) var s2: sampler;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv * p.off2.z;
    let base = textureSample(t0, s0, uv + p.off01.xy);
    var c = base.rgb * p.tint.rgb;
    if (p.off2.w > 0.0) {
        let n1 = textureSample(t1, s1, uv + p.off01.zw).xyz * 2.0 - 1.0;
        let n2 = textureSample(t2, s2, uv + p.off2.xy).xyz * 2.0 - 1.0;
        let n = normalize(n1 + n2);
        let l = normalize(vec3<f32>(-0.35, -0.5, 0.8));
        c = c * (1.0 + p.off2.w * (dot(n, l) - l.z));
    }
    return vec4<f32>(c, 1.0);
}
