// Cup and disc rim layers (docs/CAPSULE-VISUALS.md "Draw recipe": shaders
// `vkCAPM2A`/`vkCAPM2AM2`/`vkCAPM2MM2`): out = vcol.a * vcol.rgb * (sum of
// two independently-scrolled samples of the same cloud-noise texture),
// additive (or multiply for the opponent-Aura disc, chosen at the Rust
// level via `alpha_mode()`, not in this shader), no depth write, always
// camera-facing every frame (joint flag 0x0204 -- unlike the core, which is
// only billboarded for a few kinds, the cup/disc always is). The vertex
// colours are the file's own baked rim profile (dark inner edge, a hot peak
// just inside the core's silhouette, a dim tail outward) -- not a runtime
// tint.
#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> orb_flags: vec4<f32>;  // x = camera-forward sign (+1/-1); y = RimBlend as a number (0 Add, 1 Multiply, 2 Screen)
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> orb_layer0: vec4<f32>; // speed.xy, fixed uv offset.xy
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> orb_layer1: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> orb_ring: vec4<f32>;   // x = boost (overall gain), y = soften (gamma exponent, lower = wider/softer)
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var orb_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var orb_sampler: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let centre = (world_from_local * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
    let scale = length(world_from_local[0].xyz);
    let cam_right = normalize(view.world_from_view[0].xyz);
    let cam_up = normalize(view.world_from_view[1].xyz);
    let cam_fwd = orb_flags.x * normalize(view.world_from_view[2].xyz);
    let world_pos = centre + (cam_right * vertex.position.x + cam_up * vertex.position.y + cam_fwd * vertex.position.z) * scale;
    out.world_position = vec4<f32>(world_pos, 1.0);
    out.position = position_world_to_clip(world_pos);
    out.world_normal = -cam_fwd;
    out.uv = vertex.uv;
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
    let t = globals.time * 30.0;
    let uv0 = in.uv + orb_layer0.xy * t + orb_layer0.zw;
    let uv1 = in.uv + orb_layer1.xy * t + orb_layer1.zw;
    let s = textureSample(orb_tex, orb_sampler, uv0).rgb + textureSample(orb_tex, orb_sampler, uv1).rgb;
#ifdef VERTEX_COLORS
    let vcol = in.color;
#else
    let vcol = vec4<f32>(1.0, 1.0, 1.0, 1.0);
#endif
    // The exported vertex colour is the file's raw byte/255 value, meant to
    // be added in display/gamma space (docs "Build recipe" step 4: "the
    // game adds these 0-1 values in display space... use `FxMaterial`-style
    // re-encoding for the cup"). Our fragment output is otherwise treated as
    // scene-linear light and gamma-encoded again at display -- decoding here
    // (x^2.2) cancels that second encode so the rim's baked profile (dark
    // inner edge, hot peak, dim tail) reads at its intended brightness
    // instead of doubly brightened.
    let vcol_rgb = pow(max(vcol.rgb, vec3<f32>(0.0)), vec3<f32>(2.2));
    var rgb = vcol.a * vcol_rgb * s;
    // Thicker/looser ring, more emitted glow (Laurence, round 3d: "the red
    // outline is not thick enough and not loose enough... not enough glow
    // emitting... the outer ring [should be] thicker and looser"). Two
    // knobs: `orb_ring.y` (soften) is a gamma lift -- lower values raise the
    // dim tail relatively more than the hot peak, spreading the visible
    // glow much further out towards the mesh's own edge without new
    // geometry past its silhouette; `orb_ring.x` (boost) is a flat
    // multiplier on top for "more glow emitting" overall. Tunable via
    // `OrbTuning::RingTuning`/`PD_ORB_RING_VARIANT=A/B/C` for the round-3d
    // proof sheet.
    rgb = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(orb_ring.y)) * orb_ring.x;
    // Bevy's generic Material pipeline gives `AlphaMode::Add` and
    // `AlphaMode::Multiply` different blend states, and (since we bypass
    // the standard PBR fragment path entirely) it's on us to premultiply
    // correctly for each -- `bevy_pbr::pbr_functions::premultiply_alpha`'s
    // own derivation, replicated here since our shader never calls it:
    //   Add (BLEND_PREMULTIPLIED_ALPHA, `result = src + (1-src.a)*dst`):
    //   output alpha 0 so the blend keeps 100% of dst and adds all of src.
    //   Feeding 1.0 here (this pass's first attempt) made `result = src`
    //   outright -- every fragment, including the profile's black inner
    //   ring, REPLACED the destination instead of adding to it, which is
    //   the "opaque black annulus hiding the background" bug.
    //   Multiply (BLEND_MULTIPLY, `result = src.a*(src*dst) + (1-src.a)*dst`):
    //   output alpha 1 so the blend is a full `dst * src` darken (the
    //   opponent-Aura disc's own 0x2000 "src ZERO, dst SRC_COLOR" blend);
    //   clamped to 1 so a bright rim pixel can't brighten the background.
    if abs(orb_flags.y - 1.0) < 0.5 {
        // Multiply (recipe blend 0x2000, "src ZERO, dst SRC_COLOR"): full
        // `dst * src` darken.
        out.color = vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    } else if abs(orb_flags.y - 2.0) < 0.5 {
        // Screen (recipe blend 0x1300, "src ONE_MINUS_SRC_COLOR, dst ONE"):
        // `result = src*(1-src) + dst` -- self-attenuating so a bright pixel
        // doesn't blow all the way to solid white the way flat Add does.
        // Only Aura's disc uses this. Laurence: "the grey one-sided smudge" --
        // the mesh's own baked vertex colours here read white/grey but
        // unevenly per-vertex (a byte dump found alternating 0xFFFFFFFF/
        // 0x747474FF, not a clean radial taper), which triangulated into a
        // lopsided smudge instead of a centred halo. Replaced with a clean,
        // camera-centred radial falloff from the mesh's own UV (v maps
        // radially, same convention as the cup) instead of the baked
        // colours, and a flat white tint (the doc's own read of this
        // texture: "white liquid swirl") instead of the uneven vcol.
        //
        // Fixed 2026-09-28 (Laurence, `all_kinds_round3d.png`: "Aura
        // Particle's white halo too big -- shrink"): this disc's own vertex
        // format (0x15) is exported by `pd_formats::alr::vertex_layout` with
        // the wrong UV divisor -- `i16::MAX` (32767), inherited from an
        // unrelated skinned/palette format that happens to share this format
        // id, instead of the 4096 every other capsule vertex format uses.
        // Confirmed with a raw byte dump (`pd_convert drawinfo` on
        // `Map/st00.alr` model 4 draw 1, a temporary `PD_RAWDUMP=1`, removed
        // after this probe): read with the correct divisor, v is a clean
        // radial map, 0.6846 at the inner ring (r 3.93, hugging the core)
        // out to 0.9954 at the outer ring (r 7.96) -- exactly the cup's own
        // "v maps radially" convention, just a different absolute range.
        // Read with the file's wrong divisor (as `in.uv` is here), every
        // vertex's v lands in 0.0856..0.1244 -- entirely below the old
        // radial cutoff of 0.4, so `radial` was 0 (i.e. full glow) on every
        // single vertex: not a taper at all, a flat wash the shape of
        // whatever the noise texture happened to sample -- the reported
        // "flat grey-white disc filling half the frame". Not fixed in
        // `vertex_layout` itself (a shared table entry other, unverified
        // geometry may also read, and out of this pass's render-only scope)
        // -- undone here instead by multiplying back the same wrong/right
        // divisor ratio (32767 / 4096 = 8.0) before mapping radially over
        // the real measured range, then tapered with a tighter power and a
        // lower gain than the old (buggy) constants so the result reads as a
        // small, soft glow hugging the core's own silhouette rather than a
        // second disc the width of the whole mesh.
        let v_true = in.uv.y * 8.0;
        let radial = clamp((v_true - 0.70) / 0.28, 0.0, 1.0);
        let glow = pow(1.0 - radial, 3.0);
        let src = s * glow * 0.35;
        let attenuated = src * (vec3<f32>(1.0) - clamp(src, vec3<f32>(0.0), vec3<f32>(1.0)));
        out.color = vec4<f32>(attenuated, 0.0);
    } else {
        // Add (recipe blend 0x1100, "ONE+ONE"): see the derivation above --
        // alpha must be 0 so the blend adds to dst instead of replacing it.
        out.color = vec4<f32>(rgb, 0.0);
    }
    return out;
}
