// The school aura's glow, composite (`aura_glow.rs`): the blurred silhouette tile drawn as a
// camera-facing quad at the fighter (`FUN_140289140`, shader `AuraBox_vs` / `AuraBox_ps`).
//
// `AuraBox_ps`: colour = tile texel x the vertex colour (a grey, the fighter's model opacity squared
// / 256), the pixel killed when its alpha (the tile's luminance) is 0. The blend state is
// `0x1201` = SRC_COLOR, ONE: the screen gains colour squared (`FUN_14019c980`).
#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}
#import bevy_pbr::mesh_view_bindings::view

// x = the vertex grey (0..1), y = the fighter's body code (1..4; 0 = no exclusion).
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> comp: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tile: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var samp: sampler;
// The view's body codes: each fighter's own pixels drawn with (code / 8) in red, the nearest fighter wins.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var mask: texture_2d<f32>;

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    // The stencil test (`FUN_14019c980`: NOT EQUAL to the fighter's own value): the body is left out of its glow.
    if comp.y > 0.5 {
        let m = textureLoad(mask, vec2<i32>(in.position.xy - view.viewport.xy), 0).r;
        if abs(m * 8.0 - comp.y) < 0.5 {
            discard;
        }
    }
    if comp.z > 0.5 {
        let m = textureLoad(mask, vec2<i32>(in.position.xy - view.viewport.xy), 0);
        out.color = vec4<f32>(m.r * 4.0, m.g, 0.0, 0.0);
        return out;
    }
    let v = textureSample(tile, samp, in.uv).rgb;
    if dot(v, vec3<f32>(0.3, 0.6, 0.1)) <= 0.0 {
        discard;
    }
    let s = clamp(v * comp.x, vec3<f32>(0.0), vec3<f32>(1.0));
    // Add blend (`AlphaMode::Add`): the stored value, as every other additive piece (`fx_gamma.wgsl`).
    out.color = vec4<f32>(s * s, 0.0);
    return out;
}
