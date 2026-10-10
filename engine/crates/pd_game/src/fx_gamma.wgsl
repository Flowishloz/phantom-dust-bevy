// Skill-effect material (effects.rs `FxMaterial`): a StandardMaterial whose
// draws add the colour the game itself adds, blend the way each draw's own
// material says, and (for billboard pieces) face whichever camera draws them.
//
// Gamma. The original draws its effects on a gamma-space target without tone
// mapping: an additive piece adds its texture x colour values (0-1, as
// stored) straight onto the screen's. Bevy decodes those values to linear
// light, adds in linear light and then tone-maps each fragment, so the same
// piece adds far less -- a dim glow over a mid-grey floor adds about a third
// of what it should, and ten overlapping sparks never reach white.
// Here an additive fragment's colour is re-encoded (x^(1/2.2), exact for a
// product of decoded values) and skips the tone mapping, so it adds the
// stored value. Blended draws keep Bevy's usual processing.
// params.x = the re-encoding exponent (1/2.2; PD_FX_GAMMA=1 gives the old look).
//
// Blending (docs/FORMATS-EFFECTS.md "Blend state"). A piece's material has its
// own source/destination factor and operation (`FUN_1402a6010` ->
// `FUN_14028e4e0`); the pipeline specialisation in effects.rs sets them on the
// fragment target. The fragment stage only has to hand over the right numbers:
// `PDEFTBB*_vs` premultiplies the vertex colour by its alpha
// (`mul o1.xyz, r0.wwww, r0.xyzx`), the pixel shader multiplies texture x that
// colour and outputs alpha = texture a x colour a; alpha otherwise only
// kills. So with ONE,ONE the texture's alpha never weighs the colour, but the
// colour's own alpha does (piece.z = 1 when the shader is of that family;
// the point-sprite and PDMTEX shaders pass the colour straight).
// params.y picks how the colour is encoded for the factor it meets:
//   0  legacy (non-piece draws, old caches): the original Add handling
//   1  additive family (dest ONE; source ONE / SRC_ALPHA / SRC_COLOR): the
//      gamma re-encoded colour, no tone mapping
//   2  DST_COLOR, ONE: dest x (1 + colour) in the game's gamma space is
//      dest_lin x (1 + g)^2.2 in ours, so the factor is (1 + g)^2.2 - 1
//   3  ZERO, 1 - SRC_COLOR (multiply-darken): dest x (1 - g)^2.2
//   4  anything else (alpha blend ...): Bevy's usual processing
//   5  a factor, not a light (ZERO, SRC_COLOR / DST_COLOR, ZERO): the plain
//      linear colour -- the product of two decoded values is exact
//   6  no blending (alpha test only): Bevy's usual processing, alpha 1
//   7  a blend that is not an addition (reverse-subtract, subtract, min, max): the program does
//      it on gamma-space values, which a linear-light blend unit cannot. The piece is drawn in the
//      transmissive pass, reads the scene copy, works its own equation out in gamma space and
//      outputs the difference it makes in linear light; the pipeline subtracts (or, for max,
//      adds) that difference from the target (`effects.rs` `FxGamma::specialize`)
// piece.x = 1: billboard (effect word 19 = 0, `FUN_1401a5ff0`): the quad is
// rebuilt in view space from the camera that draws it -- origin and axis
// lengths of the piece's own matrix, rotated by piece.y (radians, the roll).
// piece.z = 1: premultiply by the colour's alpha (piece.w = that alpha);
// 2: by the full alpha, texture x colour (PDEFTAURA01_ps).
// Shader families (extra.y; docs/FORMATS-EFFECTS.md "Pixel stage"). 0 = the
// plain one-texture path above (Bevy samples the base texture). The others
// sample their own texture slots t0..t3 here, in the game's gamma space, and
// follow their pixel shader instruction for instruction:
//   1 PDEFTBB02 (a stretched flipbook): rgb = colour x mix(t0(cell), t0(cell + 1), m)
//     (`mad o0.xyz, c1.xyz, t1, t0 x (1 - c1)`; m = the cell's fraction), a = t0.a x colour.a
//   2 PDMTEX01 / PDMTEX02: rgb = colour x (t0 + t1) x t1.a, a = colour.a; t0 runs
//     through the flipbook + scroll, t1 sits still
//   3 PDEFTAURA01: rgb = t1 x (t0 x colour x a), a = t0.a x colour.a
//   4 PDEFTBB08: rgb = colour x ((t1 + env) x t3 + env), env = a reflection lookup in t2
//     (approximated, see the shader); PDEFTBB07 ((t1 + env) x env) is not built
// uv0 / uv1 (xy = scale, zw = offset) place the mesh UV for t0 / t1.
// quad = the effect's alpha multiplier per screen quadrant (x top-left, y top-right,
// z bottom-left, w bottom-right; 1 = untouched; only the school aura changes it,
// `effects::FxTint`). The quadrant is the one the view's viewport starts in, picked
// like `dissolve.wgsl` does: bit 0 = left edge off the window's left, bit 1 = top
// edge off the window's top (view 0 is always quadrant 0). It weighs the colour
// too, so additive pieces (which ignore alpha) fade with it.

#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions,
    skinning,
    morph::{morph_position, morph_normal, morph_tangent},
    pbr_types,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_view_bindings::{view, view_transmission_texture, view_transmission_sampler},
    view_transformations::position_world_to_clip,
}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::approximate_inverse_tone_mapping
#endif

// The seven vec4s of effects.rs `FxUniform`, ONE uniform buffer (one GPU buffer per material
// instead of seven; a piece's material is re-prepared whenever its colour or cell changes).
struct FxU {
    params: vec4<f32>,
    piece: vec4<f32>,
    uv0: vec4<f32>,
    uv1: vec4<f32>,
    // x = cross-fade weight (family 1), y = shader family, z / w = the heat haze's strengths
    extra: vec4<f32>,
    quad: vec4<f32>,
    // scene blend (mode 7), see below
    scene: vec4<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> fxu: FxU;
// Per-instance rows (effects/inst.rs `FxInst`): an effect piece whose mesh has a MeshTag reads its
// animated values from fx_inst[tag] (its material, shared with every piece of the same static look,
// holds neutral ones); tag 0 = the material's own values (every other effect draw, as before).
// color = the StandardMaterial base colour (linear RGBA), uv = a plain piece's UV transform
// (xy scale, zw offset); the rest are FxU's vec4s, of which only the animated components are used.
struct FxInst {
    color: vec4<f32>,
    uv: vec4<f32>,
    piece: vec4<f32>,
    uv0: vec4<f32>,
    uv1: vec4<f32>,
    extra: vec4<f32>,
    quad: vec4<f32>,
    params: vec4<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var<storage, read> fx_inst: array<FxInst>;

/// This draw's values: the material's, with a tagged piece's animated ones (roll, alpha, the family
/// UVs and cross-fade, the haze strengths, the quadrant alpha and the game area's corner) from its row.
fn fx_values(tag: u32) -> FxU {
    var u = fxu;
    if tag != 0u {
        let r = fx_inst[tag];
        u.piece = vec4<f32>(fxu.piece.x, r.piece.y, fxu.piece.z, r.piece.w);
        u.uv0 = r.uv0;
        u.uv1 = r.uv1;
        u.extra = vec4<f32>(r.extra.x, fxu.extra.y, r.extra.z, r.extra.w);
        u.quad = r.quad;
        u.params = vec4<f32>(fxu.params.x, fxu.params.y, r.params.z, r.params.w);
    }
    return u;
}
// scene blend (mode 7): x = 1 darkens / 2 lightens, y / z = source / destination factor codes
// (the order of the program's table: ZERO, ONE, SRC_COLOR, 1-SRC_COLOR, SRC_ALPHA, 1-SRC_ALPHA,
// DST_ALPHA, 1-DST_ALPHA, DST_COLOR, 1-DST_COLOR, SRC_ALPHA_SAT), w = the operation
// (1 SUBTRACT, 2 REVERSE_SUBTRACT, 3 MIN, 4 MAX)
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var tex0: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var samp0: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var tex1: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var samp1: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var tex2: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var samp2: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var tex3: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var samp3: sampler;

/// A stored texel in the game's gamma space (the images decode as sRGB).
fn gm(c: vec3<f32>, inv: f32) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(inv));
}

/// A blend factor by code (`scene.y` / `scene.z`), from the source colour / alpha and the
/// destination colour. The destination alpha of the scene copy is 1.
fn blend_factor(code: i32, s: vec3<f32>, sa: f32, d: vec3<f32>) -> vec3<f32> {
    switch code {
        case 0: { return vec3<f32>(0.0); }
        case 2: { return s; }
        case 3: { return vec3<f32>(1.0) - s; }
        case 4: { return vec3<f32>(sa); }
        case 5: { return vec3<f32>(1.0 - sa); }
        case 6: { return vec3<f32>(1.0); }
        case 7: { return vec3<f32>(0.0); }
        case 8: { return d; }
        case 9: { return vec3<f32>(1.0) - d; }
        case 10: { return vec3<f32>(min(sa, 0.0)); }
        default: { return vec3<f32>(1.0); }
    }
}

/// A billboard vertex: the piece's origin and scale from its matrix, placed
/// in the view that is drawing it (local x right, y up, as in the game's view
/// space; `FUN_1401a5ff0` keeps only the scale, rolls by `roll`, then uses the
/// view-space position).
fn billboard_world_position(local: vec3<f32>, world_from_local: mat4x4<f32>, roll: f32) -> vec4<f32> {
    let sx = length(world_from_local[0].xyz);
    let sy = length(world_from_local[1].xyz);
    let sz = length(world_from_local[2].xyz);
    let o = (view.view_from_world * vec4<f32>(world_from_local[3].xyz, 1.0)).xyz;
    let c = cos(roll);
    let s = sin(roll);
    // Rotate in the quad's plane, then scale (the program rolls the scaled rows).
    let r = vec2<f32>(c * local.x - s * local.y, s * local.x + c * local.y);
    let v = o + vec3<f32>(r.x * sx, r.y * sy, local.z * sz);
    return view.world_from_view * vec4<f32>(v, 1.0);
}

#ifdef MORPH_TARGETS
fn morph_vertex(vertex_in: Vertex, instance_index: u32) -> Vertex {
    var vertex = vertex_in;
    let first_vertex = mesh[instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;

    let weight_count = bevy_pbr::morph::layer_count(instance_index);
    for (var i: u32 = 0u; i < weight_count; i ++) {
        let weight = bevy_pbr::morph::weight_at(i, instance_index);
        if weight == 0.0 {
            continue;
        }
        vertex.position += weight * morph_position(vertex_index, i, instance_index);
#ifdef VERTEX_NORMALS
        vertex.normal += weight * morph_normal(vertex_index, i, instance_index);
#endif
#ifdef VERTEX_TANGENTS
        vertex.tangent += vec4(weight * morph_tangent(vertex_index, i, instance_index), 0.0);
#endif
    }
    return vertex;
}
#endif

// Bevy's own mesh vertex stage (bevy_pbr render/mesh.wgsl), plus the
// billboard placement above.
@vertex
fn vertex(vertex_no_morph: Vertex) -> VertexOutput {
    var out: VertexOutput;

#ifdef MORPH_TARGETS
    var vertex = morph_vertex(vertex_no_morph, vertex_no_morph.instance_index);
#else
    var vertex = vertex_no_morph;
#endif

    let mesh_world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);

#ifdef SKINNED
    var world_from_local = skinning::skin_model(
        vertex.joint_indices,
        vertex.joint_weights,
        vertex_no_morph.instance_index
    );
#else
    var world_from_local = mesh_world_from_local;
#endif

#ifdef VERTEX_NORMALS
#ifdef SKINNED
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex_no_morph.instance_index
    );
#endif
#endif

#ifdef VERTEX_POSITIONS
    let u = fx_values(mesh_functions::get_tag(vertex_no_morph.instance_index));
    if u.piece.x > 0.5 {
        out.world_position = billboard_world_position(vertex.position, world_from_local, u.piece.y);
    } else {
        out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    }
    out.position = position_world_to_clip(out.world_position.xyz);
#endif

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        vertex_no_morph.instance_index
    );
#endif

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex_no_morph.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex_no_morph.instance_index, mesh_world_from_local[3]);
#endif

    return out;
}

@fragment
fn fragment(in_raw: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    let tag = mesh[in_raw.instance_index].tag;
#else
    let tag = 0u;
#endif
    let u = fx_values(tag);
    var in = in_raw;
#ifdef VERTEX_UVS_A
    if tag != 0u && i32(u.extra.y + 0.5) == 0 {
        // A plain piece's flipbook / scroll from its row (the shared material's UV transform is the identity).
        let r = fx_inst[tag];
        in.uv = in_raw.uv * r.uv.xy + r.uv.zw;
    }
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    if tag != 0u {
        // The row's colour (the shared material's base colour is white).
        pbr_input.material.base_color = pbr_input.material.base_color * fx_inst[tag].color;
    }
    // Mask mode (the transmissive-pass pieces) turns the alpha into 1 after the cut: keep it.
    let raw_alpha = pbr_input.material.base_color.a;
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var c = pbr_input.material.base_color;
    if i32(u.params.y + 0.5) == 7 {
        c.a = raw_alpha;
    }
    // Relative to the game area's corner (params.zw; zero unless a fixed aspect ratio leaves bars).
    let rel = view.viewport.xy - u.params.zw;
    let quadrant = u32(step(0.5, rel.x) + 2.0 * step(0.5, rel.y));
    let view_alpha = u.quad[quadrant];
    c.a = c.a * view_alpha;
    var out: FragmentOutput;
    let mode = i32(u.params.y + 0.5);
    if mode == 0 {
        // Legacy (non-piece draws).
        let alpha_mode = pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
        if alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD && u.params.x != 1.0 {
            let g = pow(max(c.rgb, vec3<f32>(0.0)), vec3<f32>(u.params.x));
            // Additive = premultiplied by alpha, alpha 0 (as Bevy does for Add).
            out.color = vec4<f32>(g * c.a, 0.0);
        } else {
            out.color = main_pass_post_lighting_processing(pbr_input, c);
        }
        return out;
    }
    // A piece: rgb = texture x colour (x colour alpha for the PDEFTBB family),
    // alpha = texture a x colour a; the fixed-function blend does the rest.
    // The colour alpha is a gamma-space weight in the game: applied to the
    // re-encoded value in the gamma modes, as its power-2.2 twin in linear ones.
    let inv = u.params.x;
    let rgb = max(c.rgb, vec3<f32>(0.0));
    let fam = i32(u.extra.y + 0.5);
    // PDEFTBB01_ps / PDEFTBB04_ps (the plain PDEFTBB* pieces): discard `a <= alpharef`, where
    // a = texture a x colour a and the alpha reference is 0 (`FUN_1402a6010` enables the test
    // with `FUN_140020660(0x3d, 0)`). A blended draw therefore never draws a fully transparent
    // texel's colour -- which a subtractive or additive halo of an alpha-edged texture needs
    // (Bomb's fireball disc, state 0x1240, drew a dark stepped ring from them).
    if fam == 0 && u.piece.z > 0.5 && u.piece.z < 1.5 && c.a <= 0.0 {
        discard;
    }
    if fam == 5 {
        // PDEFTOPT05_ps: the heat haze. The vertex stage (`PDEFTOPT05_vs`) takes the screen
        // position of the vertex pushed `extra.w` back along its normal, as a texture
        // coordinate; the pixel stage offsets it by `extra.z x 2 (bump - 0.5)` (the bump map
        // t0 is read through channels z and y; both bytes of this map are equal, so one value
        // moves both axes) and outputs colour x the scene copy there, alpha = the colour's alpha.
        let p0 = in.uv * u.uv0.xy + u.uv0.zw;
        let bump = gm(textureSampleLevel(tex0, samp0, p0, 0.0).rgb, inv);
        let b = vec2<f32>(2.0 * (bump.b - 0.5), 2.0 * (bump.g - 0.5));
        var wp = in.world_position.xyz;
#ifdef VERTEX_NORMALS
        wp = wp - normalize(in.world_normal) * u.extra.w;
#endif
        let clip = view.clip_from_world * vec4<f32>(wp, 1.0);
        let sub = (clip.xy / clip.w) * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5) + u.extra.z * b;
        let full_size = vec2<f32>(textureDimensions(view_transmission_texture));
        let at = (sub * view.viewport.zw + view.viewport.xy) / full_size;
        var scene = textureSampleLevel(view_transmission_texture, view_transmission_sampler, at, 0.0);
#ifdef TONEMAP_IN_SHADER
        scene = approximate_inverse_tone_mapping(scene, view.color_grading);
#endif
        var o: FragmentOutput;
        o.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * scene.rgb, c.a));
        return o;
    }
    // g = the gamma-space colour with its alpha weight applied; out_a = alpha.
    var g: vec3<f32>;
    var out_a = c.a;
    var lin: vec3<f32>;
    if fam == 0 {
        var weight = view_alpha;
        if u.piece.z > 1.5 {
            // PDEFTAURA01_ps (no per-family path): colour x (texture a x colour a).
            weight = c.a;
        } else if u.piece.z > 0.5 {
            weight = weight * u.piece.w;
#ifdef VERTEX_COLORS
            // The vertex colour's alpha is part of the colour the vertex shader premultiplies.
            weight = weight * in.color.a;
#endif
        }
        g = pow(rgb, vec3<f32>(inv)) * weight;
        lin = rgb * pow(weight, 1.0 / inv);
    } else {
        // Families sample their own textures (the base colour here is the
        // piece colour x vertex colour, no texture).
        let colg = gm(rgb, inv);
        let p0 = in.uv * u.uv0.xy + u.uv0.zw;
        let p1 = in.uv * u.uv1.xy + u.uv1.zw;
        let t0 = textureSampleLevel(tex0, samp0, p0, 0.0);
        if fam == 1 {
            let t1 = textureSampleLevel(tex1, samp1, p1, 0.0);
            var w = view_alpha * u.piece.w;
#ifdef VERTEX_COLORS
            w = w * in.color.a;
#endif
            g = colg * mix(gm(t0.rgb, inv), gm(t1.rgb, inv), u.extra.x) * w;
            out_a = t0.a * c.a;
        } else if fam == 2 {
            let t1 = textureSampleLevel(tex1, samp1, p1, 0.0);
            g = min(colg * (gm(t0.rgb, inv) + gm(t1.rgb, inv)) * t1.a, vec3<f32>(1.0));
            out_a = c.a;
        } else if fam == 3 {
            let t1 = textureSampleLevel(tex1, samp1, p1, 0.0);
            let a = t0.a * c.a;
            g = gm(t1.rgb, inv) * (gm(t0.rgb, inv) * colg * a);
            out_a = a;
        } else if fam == 4 {
            // PDEFTBB08_ps: colour (premultiplied by its alpha in the vertex stage) x
            // ((t1 + env) x t3 + env). t1 and t3 share the flipbook UV; `env` is the
            // reflection lookup in t2 -- the reflection vector (`PDEFTBB08_vs`: 2 (N.V) N - V
            // with the engine's own constants) is not reproduced, so t2 is read at its
            // middle (the environment textures are dim blobs, a small term).
            let cp = colg * c.a;
            let t1 = textureSampleLevel(tex1, samp1, p0, 0.0);
            let t3 = textureSampleLevel(tex3, samp3, p0, 0.0);
            let e = gm(textureSampleLevel(tex2, samp2, vec2<f32>(0.5, 0.5), 0.0).rgb, inv) * cp;
            g = min((gm(t1.rgb, inv) * cp + e) * gm(t3.rgb, inv) + e, vec3<f32>(1.0));
            out_a = c.a;
        } else {
            g = colg;
        }
        lin = pow(g, vec3<f32>(1.0 / inv));
    }
    if mode == 7 {
        // The scene copy under this fragment, back in the game's gamma space (the target is
        // linear light; `D` is what the original's target would hold there).
        let full_size = vec2<f32>(textureDimensions(view_transmission_texture));
        let d_lin = textureSampleLevel(view_transmission_texture, view_transmission_sampler, in.position.xy / full_size, 0.0).rgb;
        let d_g = pow(max(d_lin, vec3<f32>(0.0)), vec3<f32>(inv));
        let s_g = clamp(g, vec3<f32>(0.0), vec3<f32>(1.0));
        let fs = blend_factor(i32(u.scene.y + 0.5), s_g, out_a, d_g);
        let fd = blend_factor(i32(u.scene.z + 0.5), s_g, out_a, d_g);
        let op = i32(u.scene.w + 0.5);
        var res_g: vec3<f32>;
        if op == 1 {
            res_g = s_g * fs - d_g * fd;
        } else if op == 2 {
            res_g = d_g * fd - s_g * fs;
        } else if op == 3 {
            res_g = min(s_g, d_g);
        } else {
            res_g = max(s_g, d_g);
        }
        let res_lin = pow(clamp(res_g, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / inv));
        var delta: vec3<f32>;
        if u.scene.x < 1.5 {
            delta = max(d_lin - res_lin, vec3<f32>(0.0));
        } else {
            delta = max(res_lin - d_lin, vec3<f32>(0.0));
        }
        out.color = vec4<f32>(delta, 0.0);
        return out;
    }
    if mode >= 1 && mode <= 3 {
        if mode == 2 {
            out.color = vec4<f32>(pow(vec3<f32>(1.0) + g, vec3<f32>(1.0 / inv)) - vec3<f32>(1.0), out_a);
        } else if mode == 3 {
            g = min(g, vec3<f32>(1.0));
            out.color = vec4<f32>(vec3<f32>(1.0) - pow(vec3<f32>(1.0) - g, vec3<f32>(1.0 / inv)), out_a);
        } else {
            out.color = vec4<f32>(g, out_a);
        }
        return out;
    }
    if mode == 5 {
        out.color = vec4<f32>(lin, out_a);
    } else if mode == 6 {
        out.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(lin, 1.0));
    } else {
        out.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(lin, out_a));
    }
    return out;
}
