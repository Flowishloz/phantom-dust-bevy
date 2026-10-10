// A flat colour, unlit: the silhouette of a fighter's body in the glow's own colour, or its body code
// (`aura_glow.rs`; the original's `SAURA_ps` is a constant colour, alpha 1). The default vertex stage
// poses the skinned mesh.
#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> colour: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.color = vec4<f32>(colour.rgb, 1.0);
    return out;
}
