//! Per-view glow: a bright-pass -> blur -> additive composite run on each
//! player's view after its picture is drawn (`PD_GLOW`, default per
//! `GLOW_DEFAULT_ON`; `PD_GLOW=0` leaves today's picture untouched).
//!
//! THIS IS A MODERN ENHANCEMENT, NOT A PORT. The lane brief assumed the
//! original draws a per-draw tinted "glow mask" into a second render target
//! (`o1 = blend_colour` in every pixel shader) and blurs it back on. Re-reading
//! the program (docs/FORMATS-EFFECTS.md "The glow pass: re-read 2026-10-01")
//! shows there is no such target: `o1` is the D3D11 **dual-source blend**
//! input that the port uses for the original's constant-colour blend factors
//! (engine ids 0x8001-0x8004 = GL CONSTANT_COLOR / ONE_MINUS / CONSTANT_ALPHA /
//! ONE_MINUS -> D3D11 blend ids 0x10-0x13 = SRC1_COLOR / INV_SRC1_COLOR /
//! SRC1_ALPHA / INV_SRC1_ALPHA, `FUN_140020420`), `OMSetRenderTargets` binds
//! exactly one view (`FUN_14001cc60`), and the "0x108 / 0x38 descriptors" are
//! a `D3D11_BLEND_DESC` and a `D3D11_DEPTH_STENCIL_DESC`. `pdvkGlow2` is a
//! per-light glare sprite tested with an occlusion query, and the
//! GFilter / FFilter / MFilter / Filter1 shaders are never referenced by the
//! program. So there is no original glow to be faithful to, and the chain
//! below is built to look good, not to match a capture:
//!
//! 1. bright-pass (max channel above `threshold`, soft knee) of the view's
//!    own rectangle of the main texture, written at half size;
//! 2. a separable 9-texel Gaussian blur (level 0), then the same on a
//!    quarter-size copy (level 1, the wide soft halo);
//! 3. both levels added back onto the view's rectangle (ONE, ONE; alpha
//!    untouched).
//!
//! It runs inside each player camera's own schedule, after the main pass and
//! before the upscale to the window, so every view (1 to 4) gets its own
//! chain over its own viewport and the HUD (a separate camera) is never lit.
//! Tone mapping is untouched: the picture is tone-mapped per fragment by the
//! materials as before, and the glow is added to the finished LDR picture.

use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::FullscreenShader;
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::sync_component::SyncComponent;
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;

/// Whether the glow is on when `PD_GLOW` is not set. The standing decision
/// (DECISIONS.md, 2026-10-01 "glow on by default") assumed the original had
/// a glow pass; it does not (module doc), so this ships OFF and the choice is
/// Laurence's. Flip this one constant to make it default-on.
pub const GLOW_DEFAULT_ON: bool = false;

/// `PD_GLOW`: `0` / `off` / `false` = off, anything else set = on, unset =
/// `GLOW_DEFAULT_ON`.
pub fn glow_enabled_from(value: Option<&str>) -> bool {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        None => GLOW_DEFAULT_ON,
        Some(v) => !matches!(v.as_str(), "0" | "off" | "false" | "no" | ""),
    }
}

pub fn glow_enabled() -> bool {
    glow_enabled_from(std::env::var("PD_GLOW").ok().as_deref())
}

/// On a player camera: runs the glow chain for that view.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Glow {
    /// Brightest-channel level (linear, 0..1) above which a pixel glows.
    pub threshold: f32,
    /// Width of the soft knee below the threshold.
    pub knee: f32,
    /// How much of the half-size blurred layer is added back (tight halo).
    pub strength: f32,
    /// How much of the quarter-size blurred layer is added back (wide halo).
    pub wide_strength: f32,
    /// Blur step in texels of its own level (1 = the plain 9-texel kernel).
    pub radius: f32,
}

impl Default for Glow {
    fn default() -> Self {
        Glow { threshold: env_f32("PD_GLOW_THRESHOLD", 0.55), knee: env_f32("PD_GLOW_KNEE", 0.25), strength: env_f32("PD_GLOW_STRENGTH", 0.6), wide_strength: env_f32("PD_GLOW_WIDE", 0.45), radius: env_f32("PD_GLOW_RADIUS", 1.5) }
    }
}

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Whether the glow is wanted right now (the runtime switch): `sync_cameras`
/// adds / removes `Glow` on every player camera when this changes.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlowOn(pub bool);

/// The strengths a player camera's `Glow` is given while the glow is on. The
/// bloom option (`look.rs`) sets it (LOW / HIGH); by default it is
/// `Glow::default()`, the glow lane's own values.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct GlowParams(pub Glow);

/// `Glow` as the shader sees it.
#[derive(Component, ShaderType, Clone, Debug)]
pub struct GlowUniforms {
    viewport: Vec4,
    params: Vec4,
    blur: Vec4,
}

impl SyncComponent for Glow {
    type Target = (Self, GlowUniforms);
}

impl ExtractComponent for Glow {
    type QueryData = (&'static Self, &'static Camera);
    type QueryFilter = ();
    type Out = (Self, GlowUniforms);

    fn extract_component((glow, camera): QueryItem<'_, '_, Self::QueryData>) -> Option<Self::Out> {
        let (Some(rect), Some(size), Some(target)) = (camera.physical_viewport_rect(), camera.physical_viewport_size(), camera.physical_target_size()) else { return None };
        if !camera.is_active || size.x == 0 || size.y == 0 {
            return None;
        }
        let u = glow_uniforms(glow, rect.min, size, target);
        Some((glow.clone(), u))
    }
}

/// The uniform block for a view at `origin` of `size` inside a target of
/// `target` pixels.
pub fn glow_uniforms(glow: &Glow, origin: UVec2, size: UVec2, target: UVec2) -> GlowUniforms {
    let t = target.as_vec2().max(Vec2::ONE);
    GlowUniforms { viewport: Vec4::new(origin.x as f32 / t.x, origin.y as f32 / t.y, size.x as f32 / t.x, size.y as f32 / t.y), params: Vec4::new(glow.threshold, glow.knee, glow.strength, glow.wide_strength), blur: Vec4::new(glow.radius, 0.0, 0.0, 0.0) }
}

const GLOW_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

static GLOW_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();

pub fn plugin(app: &mut App) {
    let on = glow_enabled();
    app.insert_resource(GlowOn(on));
    app.insert_resource(GlowParams(Glow::default()));
    app.add_systems(Update, sync_cameras);
    let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("glow.wgsl"), "pd_game/glow.wgsl"));
    let _ = GLOW_SHADER.set(shader);
    app.add_plugins((ExtractComponentPlugin::<Glow>::default(), UniformComponentPlugin::<GlowUniforms>::default()));
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .init_gpu_resource::<SpecializedRenderPipelines<GlowPipeline>>()
        .add_systems(RenderStartup, init_pipeline)
        .add_systems(Render, (prepare_pipelines.in_set(RenderSystems::Prepare), prepare_textures.in_set(RenderSystems::PrepareResources)))
        .add_systems(Core3d, glow.after(tonemapping).in_set(Core3dSystems::PostProcess));
}

/// Keeps `Glow` on exactly the player cameras while `GlowOn` is true, with the
/// strengths in `GlowParams`.
fn sync_cameras(on: Res<GlowOn>, params: Res<GlowParams>, mut commands: Commands, cams: Query<(Entity, Option<&Glow>), With<crate::PlayerCamera>>) {
    for (e, has) in &cams {
        if on.0 && has != Some(&params.0) {
            commands.entity(e).insert(params.0.clone());
        } else if !on.0 && has.is_some() {
            commands.entity(e).remove::<Glow>();
        }
    }
}

#[derive(Resource)]
struct GlowPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    fullscreen: FullscreenShader,
    shader: Handle<Shader>,
}

fn init_pipeline(mut commands: Commands, device: Res<RenderDevice>, fullscreen: Res<FullscreenShader>) {
    let layout = BindGroupLayoutDescriptor::new(
        "pd_glow_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture_2d(TextureSampleType::Float { filterable: true }), sampler(SamplerBindingType::Filtering), uniform_buffer::<GlowUniforms>(true))),
    );
    let sampler = device.create_sampler(&SamplerDescriptor { min_filter: FilterMode::Linear, mag_filter: FilterMode::Linear, address_mode_u: AddressMode::ClampToEdge, address_mode_v: AddressMode::ClampToEdge, ..Default::default() });
    commands.insert_resource(GlowPipeline { layout, sampler, fullscreen: fullscreen.clone(), shader: GLOW_SHADER.get().cloned().unwrap_or_default() });
}

/// Which pass a pipeline is for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Pass {
    Prefilter,
    Halve,
    BlurH,
    BlurV,
    Composite0,
    Composite1,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct GlowKey {
    pass: Pass,
    /// The main texture's format (composite only; the others write `GLOW_FORMAT`).
    target: TextureFormat,
}

impl SpecializedRenderPipeline for GlowPipeline {
    type Key = GlowKey;

    fn specialize(&self, key: GlowKey) -> RenderPipelineDescriptor {
        let (entry, composite) = match key.pass {
            Pass::Prefilter => ("prefilter", false),
            Pass::Halve => ("halve", false),
            Pass::BlurH => ("blur_h", false),
            Pass::BlurV => ("blur_v", false),
            Pass::Composite0 => ("composite0", true),
            Pass::Composite1 => ("composite1", true),
        };
        let one = BlendComponent { src_factor: BlendFactor::One, dst_factor: BlendFactor::One, operation: BlendOperation::Add };
        let keep = BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add };
        RenderPipelineDescriptor {
            label: Some(format!("pd_glow_{entry}").into()),
            layout: vec![self.layout.clone()],
            vertex: self.fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                entry_point: Some(entry.into()),
                targets: vec![Some(ColorTargetState { format: if composite { key.target } else { GLOW_FORMAT }, blend: composite.then_some(BlendState { color: one, alpha: keep }), write_mask: ColorWrites::ALL })],
                ..default()
            }),
            ..default()
        }
    }
}

#[derive(Component)]
struct GlowPipelineIds {
    prefilter: CachedRenderPipelineId,
    halve: CachedRenderPipelineId,
    blur_h: CachedRenderPipelineId,
    blur_v: CachedRenderPipelineId,
    composite0: CachedRenderPipelineId,
    composite1: CachedRenderPipelineId,
}

fn prepare_pipelines(mut commands: Commands, cache: Res<PipelineCache>, mut pipelines: ResMut<SpecializedRenderPipelines<GlowPipeline>>, pipeline: Res<GlowPipeline>, views: Query<(Entity, &ExtractedView), With<Glow>>) {
    for (e, view) in &views {
        let mut id = |pass| pipelines.specialize(&cache, &pipeline, GlowKey { pass, target: view.target_format });
        let ids = GlowPipelineIds { prefilter: id(Pass::Prefilter), halve: id(Pass::Halve), blur_h: id(Pass::BlurH), blur_v: id(Pass::BlurV), composite0: id(Pass::Composite0), composite1: id(Pass::Composite1) };
        commands.entity(e).insert(ids);
    }
}

/// The view's two blur levels: (half size, half size scratch, quarter size, quarter scratch).
#[derive(Component)]
struct GlowTextures {
    half: [CachedTexture; 2],
    quarter: [CachedTexture; 2],
}

/// Sizes of the two levels for a view of `size` pixels (never below one texel).
pub fn level_sizes(size: UVec2) -> (UVec2, UVec2) {
    let half = (size / 2).max(UVec2::ONE);
    (half, (half / 2).max(UVec2::ONE))
}

fn prepare_textures(mut commands: Commands, mut cache: ResMut<TextureCache>, device: Res<RenderDevice>, views: Query<(Entity, &ExtractedCamera), With<Glow>>) {
    for (e, camera) in &views {
        let Some(size) = camera.physical_viewport_size else { continue };
        let (half, quarter) = level_sizes(size);
        let mut tex = |label: &'static str, s: UVec2| {
            cache.get(&device, TextureDescriptor { label: Some(label), size: Extent3d { width: s.x, height: s.y, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: TextureDimension::D2, format: GLOW_FORMAT, usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING, view_formats: &[] })
        };
        let t = GlowTextures { half: [tex("pd_glow_half_a", half), tex("pd_glow_half_b", half)], quarter: [tex("pd_glow_quarter_a", quarter), tex("pd_glow_quarter_b", quarter)] };
        commands.entity(e).insert(t);
    }
}

#[allow(clippy::too_many_arguments)]
fn glow(view: ViewQuery<(&ExtractedCamera, &ViewTarget, &GlowTextures, &GlowPipelineIds, &DynamicUniformIndex<GlowUniforms>)>, pipeline: Res<GlowPipeline>, cache: Res<PipelineCache>, uniforms: Res<ComponentUniforms<GlowUniforms>>, mut ctx: RenderContext) {
    let (camera, target, tex, ids, index) = view.into_inner();
    let Some(ubind) = uniforms.binding() else { return };
    let get = |id: CachedRenderPipelineId| cache.get_render_pipeline(id);
    let (Some(p_pre), Some(p_halve), Some(p_h), Some(p_v), Some(p_c0), Some(p_c1)) = (get(ids.prefilter), get(ids.halve), get(ids.blur_h), get(ids.blur_v), get(ids.composite0), get(ids.composite1)) else { return };
    let layout = cache.get_bind_group_layout(&pipeline.layout);
    let device = ctx.render_device().clone();
    let bg = |label: &'static str, view: &TextureView| device.create_bind_group(label, &layout, &BindGroupEntries::sequential((view, &pipeline.sampler, ubind.clone())));

    let src = bg("pd_glow_src", target.main_texture_view());
    let half_a = bg("pd_glow_half_a", &tex.half[0].default_view);
    let half_b = bg("pd_glow_half_b", &tex.half[1].default_view);
    let quarter_a = bg("pd_glow_quarter_a", &tex.quarter[0].default_view);
    let quarter_b = bg("pd_glow_quarter_b", &tex.quarter[1].default_view);

    let enc = ctx.command_encoder();
    enc.push_debug_group("pd_glow");
    let mut run = |label: &'static str, out: &TextureView, pipe: &RenderPipeline, input: &BindGroup| {
        let mut pass = enc.begin_render_pass(&RenderPassDescriptor { label: Some(label), color_attachments: &[Some(RenderPassColorAttachment { view: out, depth_slice: None, resolve_target: None, ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, input, &[index.index()]);
        pass.draw(0..3, 0..1);
    };
    // Level 0: bright-pass at half size, blurred there.
    run("pd_glow_prefilter", &tex.half[0].default_view, p_pre, &src);
    run("pd_glow_blur_h0", &tex.half[1].default_view, p_h, &half_a);
    run("pd_glow_blur_v0", &tex.half[0].default_view, p_v, &half_b);
    // Level 1: the blurred half-size image halved again and blurred wider.
    run("pd_glow_halve", &tex.quarter[0].default_view, p_halve, &half_a);
    run("pd_glow_blur_h1", &tex.quarter[1].default_view, p_h, &quarter_a);
    run("pd_glow_blur_v1", &tex.quarter[0].default_view, p_v, &quarter_b);
    // Both levels added onto the view's rectangle of the picture.
    for (label, pipe, input) in [("pd_glow_composite0", p_c0, &half_a), ("pd_glow_composite1", p_c1, &quarter_a)] {
        let mut pass = enc.begin_render_pass(&RenderPassDescriptor { label: Some(label), color_attachments: &[Some(target.get_unsampled_color_attachment())], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
        if let Some(v) = camera.viewport.as_ref() {
            pass.set_viewport(v.physical_position.x as f32, v.physical_position.y as f32, v.physical_size.x as f32, v.physical_size.y as f32, v.depth.start, v.depth.end);
        }
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, input, &[index.index()]);
        pass.draw(0..3, 0..1);
    }
    enc.pop_debug_group();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader's bright-pass, in Rust (same arithmetic as `bright` in glow.wgsl).
    fn bright(c: Vec3, threshold: f32, knee: f32) -> Vec3 {
        let b = c.x.max(c.y).max(c.z);
        let knee = knee.max(0.0001);
        let soft = (b - threshold + knee).clamp(0.0, 2.0 * knee);
        let soft2 = soft * soft / (4.0 * knee);
        c * ((b - threshold).max(soft2) / b.max(0.0001))
    }

    /// The shader's 5-tap bilinear kernel as the 9 texel weights it equals.
    fn nine_texel_weights() -> [f32; 9] {
        let taps = [(0.0f32, 0.2270270270f32), (1.3846153846, 0.3162162162), (-1.3846153846, 0.3162162162), (3.2307692308, 0.0702702703), (-3.2307692308, 0.0702702703)];
        let mut w = [0.0f32; 9];
        for (off, weight) in taps {
            // A bilinear tap at `off` texels splits its weight over the two
            // texels either side, by distance.
            let lo = off.floor();
            let f = off - lo;
            let i = (lo as i32 + 4) as usize;
            w[i] += weight * (1.0 - f);
            w[i + 1] += weight * f;
        }
        w
    }

    #[test]
    fn the_blur_kernel_is_a_normalised_symmetric_nine_texel_gaussian() {
        let w = nine_texel_weights();
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5, "weights sum {}", w.iter().sum::<f32>());
        // The standard binomial-like 9-tap Gaussian (Rastergrid's table).
        let want = [0.0162162162, 0.0540540541, 0.1216216216, 0.1945945946, 0.2270270270, 0.1945945946, 0.1216216216, 0.0540540541, 0.0162162162];
        for (a, b) in w.iter().zip(want) {
            assert!((a - b).abs() < 1e-4, "{w:?} vs {want:?}");
        }
        for i in 0..4 {
            assert!((w[i] - w[8 - i]).abs() < 1e-5, "symmetric");
        }
    }

    #[test]
    fn the_bright_pass_is_zero_below_the_knee_and_the_colour_above_it() {
        let (t, k) = (0.55, 0.25);
        assert_eq!(bright(Vec3::splat(0.2), t, k), Vec3::ZERO, "well below the threshold");
        assert_eq!(bright(Vec3::new(0.30, 0.1, 0.0), t, k), Vec3::ZERO, "just under threshold - knee");
        let hot = Vec3::new(1.0, 0.4, 0.2);
        let out = bright(hot, t, k);
        // Above threshold + knee: the colour scaled by (b - t) / b (hue kept).
        assert!((out - hot * ((1.0 - t) / 1.0)).length() < 1e-6, "{out:?}");
        let g = bright(Vec3::new(0.0, 0.9, 0.0), t, k);
        assert!(g.x == 0.0 && g.z == 0.0 && g.y > 0.0, "hue kept: {g:?}");
    }

    #[test]
    fn the_bright_pass_is_continuous_and_monotone_through_the_knee() {
        let (t, k) = (0.55, 0.25);
        let mut last = 0.0f32;
        for i in 0..=200 {
            let v = i as f32 / 200.0;
            let o = bright(Vec3::splat(v), t, k).x;
            assert!(o >= last - 1e-6, "monotone at {v}");
            assert!(o - last < 0.02, "no jump at {v}: {last} -> {o}");
            last = o;
        }
    }

    #[test]
    fn composite_adds_and_the_levels_scale_by_their_strengths() {
        // ONE, ONE: result = scene + glow x strength (the target saturates).
        let g = Glow { threshold: 0.5, knee: 0.2, strength: 0.75, wide_strength: 0.5, radius: 1.0 };
        let u = glow_uniforms(&g, UVec2::ZERO, UVec2::new(100, 100), UVec2::new(100, 100));
        assert_eq!((u.params.z, u.params.w), (0.75, 0.5));
        let scene = 0.4f32;
        let added = (scene + 0.5 * u.params.z + 0.25 * u.params.w).min(1.0);
        assert!((added - 0.4 - 0.375 - 0.125).abs() < 1e-6 || added == 1.0);
    }

    #[test]
    fn a_views_rectangle_maps_into_the_window_texture() {
        let g = Glow::default();
        let t = UVec2::new(1600, 900);
        // The four quarters of a 2x2 layout (views.rs): origins at the halves.
        let q = [(0, 0), (800, 0), (0, 450), (800, 450)].map(|(x, y)| glow_uniforms(&g, UVec2::new(x, y), UVec2::new(800, 450), t).viewport);
        assert_eq!(q[0], Vec4::new(0.0, 0.0, 0.5, 0.5));
        assert_eq!(q[1], Vec4::new(0.5, 0.0, 0.5, 0.5));
        assert_eq!(q[2], Vec4::new(0.0, 0.5, 0.5, 0.5));
        assert_eq!(q[3], Vec4::new(0.5, 0.5, 0.5, 0.5));
    }

    #[test]
    fn level_sizes_halve_and_never_vanish() {
        assert_eq!(level_sizes(UVec2::new(800, 450)), (UVec2::new(400, 225), UVec2::new(200, 112)));
        assert_eq!(level_sizes(UVec2::new(1, 1)), (UVec2::ONE, UVec2::ONE));
    }

    #[test]
    fn the_switch_reads_pd_glow() {
        assert_eq!(glow_enabled_from(None), GLOW_DEFAULT_ON);
        for off in ["0", "off", "OFF", "false", "no", ""] {
            assert!(!glow_enabled_from(Some(off)), "{off:?}");
        }
        for on in ["1", "on", "yes", "2"] {
            assert!(glow_enabled_from(Some(on)), "{on:?}");
        }
    }
}
