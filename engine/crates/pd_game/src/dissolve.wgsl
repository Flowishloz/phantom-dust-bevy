// Character wall-dissolve (cam_fade.rs `DissolveMaterial`): the standard PBR
// forward fragment plus a per-view screen-door discard, so a fighter backed
// against a wall dematerialises with grain instead of ghosting.
//
// Why this and not alpha blending: `AlphaMode::Blend` fades every pixel
// uniformly, which reads as a ghost and -- worse -- needs the transparent
// pass (no depth write, sort-dependent against coat tails and effects).
// The original's effect is grainy: the body breaks into pixels that thin out
// evenly until the scenery shows through, then fills back in. That is a
// per-pixel keep/discard decision driven by a coverage value, rendered in
// the opaque (or alpha-mask, for the cutout pieces) pass with full depth
// writes and the normal lit PBR path below.
//
// How it works: each converted piece owns a `DissolveMaterial` whose base
// is its own authored `StandardMaterial` (opaque stays opaque, cutout
// `Mask` keeps its cutoff via the usual `alpha_discard` first) and whose
// extension uniform holds this fighter's coverage in each screen QUADRANT
// (x, y, z, w = top-left, top-right, bottom-left, bottom-right). The
// fragment picks the quadrant its split-screen viewport starts in
// (`view.viewport` origin: bit 0 = left edge off the window's left, bit 1 =
// top edge off the window's top; so view 0 is always quadrant 0, side by
// side view 1 is quadrant 1, top/bottom view 1 is quadrant 2, and a 2x2
// grid fills 0-3) and discards fragments whose interleaved-gradient-noise
// value is above that coverage. The CPU (`cam_fade.rs`) writes each view's
// coverage into its own quadrant from the camera's real viewport origin.
// At coverage 1.0 nothing is ever discarded, so the rest state renders
// exactly the authored look. The grain block size in pixels
// (`PD_DISSOLVE_GRAIN`, default 1) is its own uniform.
//
// Lighting: lit pieces are shaded by `pd_game::char_light::shade` (the
// original's SWBOS pixel shader, char_light.wgsl) instead of Bevy's PBR;
// unlit ones keep their base colour. Then `main_pass_post_lighting_processing`
// (fog, tonemapping) -- the same tail as Bevy's own `pbr.wgsl`.

#import bevy_pbr::{
    pbr_types,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import pd_game::char_light::{CharLight, shade, normal_map_alpha}
#import pd_game::shadows::{shadow_sample, shadow_weight, shadow_darken}

// This piece's fighter (x = slot). Everything that changes per frame -- the
// fighter's coverage per screen quadrant and its light -- lives in ONE shared
// storage buffer (cam_fade.rs `FighterBlock`), written in place, so a change
// never makes the render thread re-prepare the piece's material.
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> dissolve_slot: vec4<u32>;

// Coverage per screen quadrant (1 = fully solid): x top-left, y top-right,
// z bottom-left, w bottom-right; and the fighter's light (char_light.rs
// `CharLightUniform`): the original's SWBOS pixel shader replaces Bevy's PBR
// lighting for lit pieces.
struct FighterSlot {
    coverage: vec4<f32>,
    light: CharLight,
}
struct FighterBlock {
    // x = grain block size in pixels; y, z = the game area's top-left corner in the window
    // (physical pixels; zero unless a fixed aspect ratio leaves bars); w unused.
    grain: vec4<f32>,
    slots: array<FighterSlot, 4>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> fighters: FighterBlock;

// Interleaved gradient noise: stable per pixel, no temporal flicker, cheap.
// (Jimenez 2014; the same family Bevy uses for visibility-range dithering.)
fn ign(pixel: vec2<f32>) -> f32 {
    let magic = vec3<f32>(0.06711056, 0.00583715, 52.9829189);
    return fract(magic.z * fract(dot(pixel, magic.xy)));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // The authored cutout first: Mask pieces keep their cutoff, so straps,
    // lacing and other alpha-cutout quads never draw solid.
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    // Then the wall dissolve, by the quadrant this view's viewport starts in.
    let rel = view.viewport.xy - fighters.grain.yz;
    let quadrant = u32(step(0.5, rel.x) + 2.0 * step(0.5, rel.y));
    let slot = min(dissolve_slot.x, 3u);
    let coverage = fighters.slots[slot].coverage[quadrant];
    if (coverage < 1.0) {
        let grain = max(fighters.grain.x, 1.0);
        if (ign(floor(in.position.xy / grain)) >= coverage) {
            discard;
        }
    }
    var out: FragmentOutput;
    if ((pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u) {
        var nm_a = 1.0;
        var spec_w = 0.0;
#ifdef VERTEX_UVS_A
        nm_a = normal_map_alpha(in.uv);
#endif
#ifdef VERTEX_UVS_B
        spec_w = in.uv_b.x;
#endif
        let char_light = fighters.slots[slot].light;
        var lit = shade(char_light, pbr_input.N, pbr_input.material.base_color.rgb, nm_a, spec_w, in.world_position.xyz);
        // Shadows (lane L2, shadows.wgsl): the original's blend over the
        // shadow-map test, weighted by the SWBOS pixel shader's own alpha.
        // Unchanged where no shadow falls.
        let sh = shadow_sample(in.world_position, pbr_input.N, in.position.xy);
        let k = shadow_weight(dot(pbr_input.N, -char_light.dl_dir.xyz), pbr_input.material.base_color.a) * (1.0 - sh.lit);
        lit = shadow_darken(lit, sh.colour, k);
        out.color = vec4<f32>(lit, pbr_input.material.base_color.a);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
