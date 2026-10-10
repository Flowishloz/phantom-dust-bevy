// Per-view glow (glow.rs): bright-pass -> separable blur -> additive composite.
// All passes are fullscreen triangles; `uv` is 0..1 over the pass's own target.
//
// Values are linear light here (the main target is an sRGB texture, so a
// sampled texel is already linear and an additive write is re-encoded by the
// hardware). The look is tuned in `glow.rs::Glow`.

struct GlowUniforms {
    // xy = this view's origin inside the main texture, zw = its size (both as
    // a fraction of the main texture), exactly Bevy's Bloom `viewport`.
    viewport: vec4<f32>,
    // x = threshold (max channel), y = soft knee width, z = level-0 strength,
    // w = level-1 strength.
    params: vec4<f32>,
    // x = blur radius multiplier, yzw unused.
    blur: vec4<f32>,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var<uniform> u: GlowUniforms;

/// What a texel contributes to the glow: everything above `threshold` in its
/// brightest channel, with a quadratic knee below it (a hard step is jumpy
/// when an effect fades through the threshold).
fn bright(c: vec3<f32>) -> vec3<f32> {
    let b = max(c.r, max(c.g, c.b));
    let knee = max(u.params.y, 0.0001);
    let soft = clamp(b - u.params.x + knee, 0.0, 2.0 * knee);
    let soft2 = soft * soft / (4.0 * knee);
    let contribution = max(b - u.params.x, soft2) / max(b, 0.0001);
    return c * contribution;
}

/// 4 bilinear taps over a 4x4 source footprint, then the bright-pass. Writes
/// a quarter of the area of the view (half each way).
@fragment
fn prefilter(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let p = u.viewport.xy + uv * u.viewport.zw;
    let ts = 1.0 / vec2<f32>(textureDimensions(input_texture));
    var c = textureSample(input_texture, s, p + ts * vec2<f32>(-1.0, -1.0)).rgb;
    c += textureSample(input_texture, s, p + ts * vec2<f32>(1.0, -1.0)).rgb;
    c += textureSample(input_texture, s, p + ts * vec2<f32>(-1.0, 1.0)).rgb;
    c += textureSample(input_texture, s, p + ts * vec2<f32>(1.0, 1.0)).rgb;
    c *= 0.25;
    // Clamp so one stray hot pixel cannot flood the blur.
    return vec4<f32>(min(bright(c), vec3<f32>(4.0)), 1.0);
}

/// Straight bilinear halving (level 0 -> level 1).
@fragment
fn halve(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let ts = 1.0 / vec2<f32>(textureDimensions(input_texture));
    var c = textureSample(input_texture, s, uv + ts * vec2<f32>(-0.5, -0.5)).rgb;
    c += textureSample(input_texture, s, uv + ts * vec2<f32>(0.5, -0.5)).rgb;
    c += textureSample(input_texture, s, uv + ts * vec2<f32>(-0.5, 0.5)).rgb;
    c += textureSample(input_texture, s, uv + ts * vec2<f32>(0.5, 0.5)).rgb;
    return vec4<f32>(c * 0.25, 1.0);
}

// The classic 5-tap bilinear Gaussian (9 texels): offsets 0, 1.3846, 3.2308
// with weights 0.2270, 0.3162, 0.0703 per side. Weights sum to 1.
fn blur(uv: vec2<f32>, dir: vec2<f32>) -> vec3<f32> {
    let step = dir * u.blur.x / vec2<f32>(textureDimensions(input_texture));
    var c = textureSample(input_texture, s, uv).rgb * 0.2270270270;
    c += textureSample(input_texture, s, uv + step * 1.3846153846).rgb * 0.3162162162;
    c += textureSample(input_texture, s, uv - step * 1.3846153846).rgb * 0.3162162162;
    c += textureSample(input_texture, s, uv + step * 3.2307692308).rgb * 0.0702702703;
    c += textureSample(input_texture, s, uv - step * 3.2307692308).rgb * 0.0702702703;
    return c;
}

@fragment
fn blur_h(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(blur(uv, vec2<f32>(1.0, 0.0)), 1.0);
}

@fragment
fn blur_v(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(blur(uv, vec2<f32>(0.0, 1.0)), 1.0);
}

// Composite: the blend state is ONE, ONE (additive) on the colour and leaves
// alpha alone; the shader only scales by the level's strength.
@fragment
fn composite0(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(input_texture, s, uv).rgb * u.params.z, 0.0);
}

@fragment
fn composite1(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(input_texture, s, uv).rgb * u.params.w, 0.0);
}
