//! The main-menu cloud backdrop scrolls, the way the original's does.
//!
//! READ (`FUN_1402a07b0`, LIVE-confirmed 2026-10-02, FRONTEND.md): the StMenu
//! cloud layer's material record carries six int16 fields (+0x3c..+0x46) =
//! (10, -200, 0, -350, 0, -280); the program multiplies each by its frame
//! counter (30 Hz) and 1e-5 and uploads them as vertex-shader constants
//! c90.xyzw / c91.xy. `SBTSCLOUD2_vs` adds them to three UV sets of the same
//! vertex UV: set 0 (+0.003, -0.060) UV/s, set 1 (0, -0.105), set 2
//! (0, -0.084). Set 0 samples the cloud colour (`st3`), sets 1 and 2 the two
//! normal maps (`st4`, `st5`). No rotation, no pulse: the tilt is fixed.
//!
//! Ours is a flat full-screen quad showing the colour layer at one repeat
//! (the original's mesh tiles it ~4.6 x 2.7 times over a larger model:
//! UNCONFIRMED how much of that is visible, so `PD_CLOUD_TILES` (default 1,
//! the old look) is there for A/B). The bump shading is off by default
//! (`PD_CLOUD_BUMP`, UNCONFIRMED light constants). `PD_CLOUD_FRAME_DT=<s>`
//! freezes the clock to `frame * dt` for evenly spaced proof shots.

use super::ui::MenuArt;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::{Shader, ShaderRef};

/// The StMenu cloud material's six int16 fields, in file order (+0x3c, +0x3e,
/// +0x40, +0x42, +0x44, +0x46): READ from `StMenu.alr` model 0 material 0.
pub const FIELDS: [i16; 6] = [10, -200, 0, -350, 0, -280];
/// Program frame counter rate (LIVE: advanced once per 30 Hz tick).
pub const COUNTER_HZ: f32 = 30.0;
/// `DAT_1402f7c08`.
pub const SCALE: f32 = 1e-5;

/// UV offsets per second of the three sets: (u, v) each.
pub fn rates() -> [Vec2; 3] {
    let k = COUNTER_HZ * SCALE;
    [0, 2, 4].map(|i| Vec2::new(FIELDS[i] as f32 * k, FIELDS[i + 1] as f32 * k))
}

static SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct CloudMaterial {
    #[uniform(0)]
    tint: Vec4,
    #[uniform(0)]
    off01: Vec4,
    #[uniform(0)]
    off2: Vec4,
    #[texture(1)]
    #[sampler(2)]
    t0: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    t1: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    t2: Handle<Image>,
}

impl UiMaterial for CloudMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
}

/// Marks the backdrop node; [`attach`] gives it the material.
#[derive(Component)]
pub struct CloudBackdrop {
    pub tint: Color,
}

fn env_f32(k: &str, d: f32) -> f32 {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn attach(mut commands: Commands, art: Option<Res<MenuArt>>, q: Query<(Entity, &CloudBackdrop), Without<MaterialNode<CloudMaterial>>>, mut mats: ResMut<Assets<CloudMaterial>>) {
    let Some(art) = art else { return };
    for (e, c) in &q {
        let t = c.tint.to_linear();
        let mat = CloudMaterial {
            // The old backdrop's tint was applied in sRGB space by ImageNode; the
            // shader works on the sampled (linear) texel, so hand it linear.
            tint: Vec4::new(t.red, t.green, t.blue, 1.0),
            off01: Vec4::ZERO,
            off2: Vec4::new(0.0, 0.0, env_f32("PD_CLOUD_TILES", 1.0), env_f32("PD_CLOUD_BUMP", 0.0)),
            t0: art.cloud_layers().0,
            t1: art.cloud_layers().1,
            t2: art.cloud_layers().2,
        };
        commands.entity(e).insert(MaterialNode(mats.add(mat)));
    }
}

fn drive(time: Res<Time>, frames: Res<bevy::diagnostic::FrameCount>, q: Query<&MaterialNode<CloudMaterial>>, mut mats: ResMut<Assets<CloudMaterial>>) {
    let t = match std::env::var("PD_CLOUD_FRAME_DT").ok().and_then(|v| v.parse::<f32>().ok()) {
        Some(dt) => frames.0 as f32 * dt,
        None => time.elapsed_secs(),
    };
    let r = rates();
    // Keep each offset inside one repeat (f32 precision over a long session).
    let o = |v: Vec2| Vec2::new((v.x * t).rem_euclid(1.0), (v.y * t).rem_euclid(1.0));
    for h in &q {
        if let Some(mut m) = mats.get_mut(&h.0) {
            let (a, b, c) = (o(r[0]), o(r[1]), o(r[2]));
            m.off01 = Vec4::new(a.x, a.y, b.x, b.y);
            m.off2.x = c.x;
            m.off2.y = c.y;
        }
    }
}

pub fn plugin(app: &mut App) {
    let shader = app.world_mut().resource_mut::<Assets<Shader>>().add(Shader::from_wgsl(include_str!("cloud.wgsl"), "pd_game/frontend/cloud.wgsl"));
    let _ = SHADER.set(shader);
    app.add_plugins(UiMaterialPlugin::<CloudMaterial>::default());
    app.add_systems(Update, (attach, drive).chain());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_the_live_measured_uv_per_second() {
        let r = rates();
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(close(r[0].x, 0.003) && close(r[0].y, -0.060));
        assert!(close(r[1].x, 0.0) && close(r[1].y, -0.105));
        assert!(close(r[2].x, 0.0) && close(r[2].y, -0.084));
    }
}
