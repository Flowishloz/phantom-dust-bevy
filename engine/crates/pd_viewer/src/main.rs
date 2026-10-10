//! pd_viewer - proof-of-concept Bevy scene: one converted arena, one
//! character and one music track from the player's own copy of Phantom Dust.
//!
//!   pd_viewer <cache dir> <stage> <character> [--screenshot out.png]
//!             [--close <dist>] [--angle <radians>] [--anim NAME [--anim-time secs]]
//!             [--fx eftNNNN [--fx-slot N] [--fx-frame F]]  (skill effect, see fx.rs)
//!             [--eye x,y,z --look x,y,z]  (fixed camera)  [--hide-props]  (hide the
//!             stage objects placed from the .dat, for before/after)  [--no-char]
//!
//! Expects <cache>/<stage>.glb, <cache>/<character>.glb (+ .bounds.json) and
//! <cache>/music.wav, as produced by pd_convert.

// Cloth physics shared with the game (cosmetic only).
#[path = "../../pd_game/src/dangle.rs"]
mod dangle;
#[path = "../../pd_game/src/effects.rs"]
#[allow(dead_code)]
mod effects;
mod fx;
#[path = "../../pd_game/src/stage_look.rs"]
mod stage_look;

use bevy::gltf::Gltf;
use bevy::prelude::*;

/// Character lighting (lux / cd per m2), tuned against the game's look.
const LIGHT_SUN: f32 = 3000.0;
const LIGHT_AMBIENT: f32 = 500.0;

/// Tuning override from an environment variable (e.g. PD_SUN=3000).
fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use std::path::PathBuf;

#[derive(Resource)]
struct Setup {
    cache: PathBuf,
    stage: String,
    character: String,
    center: Vec3,
    radius: f32,
    char_pos: Vec3,
    char_scale: f32,
    screenshot: Option<PathBuf>,
    angle: Option<f32>,
    anim: Option<String>,
    anim_time: Option<f32>,
    /// --eye x,y,z --look x,y,z: fixed camera (overrides the orbit).
    eye: Option<Vec3>,
    look: Option<Vec3>,
    /// --hide-props: hide the stage objects placed from the .dat (nodes
    /// named "prop..."), for before/after comparisons.
    hide_props: bool,
    /// --no-char: don't spawn the character.
    no_char: bool,
}

#[derive(Resource)]
struct CharacterGltf(Handle<Gltf>);

#[derive(Resource)]
struct AnimSetup {
    graph: Handle<AnimationGraph>,
    node: AnimationNodeIndex,
}

#[derive(Component)]
struct Orbit;

#[derive(Component)]
struct Walker;

/// Distance walked so far along +X (PD_WALK speed x time).
fn walked(time: &Time) -> Vec3 {
    Vec3::X * env_f32("PD_WALK", 0.0) * time.elapsed_secs()
}

fn walk(time: Res<Time>, s: Res<Setup>, mut q: Query<&mut Transform, With<Walker>>) {
    for mut t in &mut q {
        t.translation = s.char_pos + walked(&time);
    }
}

fn read_bounds(path: &std::path::Path) -> (Vec3, Vec3) {
    let v: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let f = |k: &str, i: usize| v[k][i].as_f64().unwrap_or(0.0) as f32;
    (Vec3::new(f("min", 0), f("min", 1), f("min", 2)), Vec3::new(f("max", 0), f("max", 1), f("max", 2)))
}

/// Automated screenshot runs open their window off-screen, behind everything,
/// unfocused and off the taskbar, so they don't pop up over Laurence's work.
/// `PD_SHOW_WINDOW=1` shows it normally.
fn background_if(background: bool, window: Window) -> Window {
    if !background || std::env::var("PD_SHOW_WINDOW").is_ok() {
        return window;
    }
    Window {
        position: bevy::window::WindowPosition::At(IVec2::new(-20000, -20000)),
        focused: false,
        window_level: bevy::window::WindowLevel::AlwaysOnBottom,
        skip_taskbar: true,
        ..window
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cache = PathBuf::from(args.first().cloned().unwrap_or_else(|| "cache".into()));
    let stage = args.get(1).cloned().unwrap_or_else(|| "st06".into());
    let character = args.get(2).cloned().unwrap_or_else(|| "pc00a".into());
    let screenshot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);

    let (smin, smax) = read_bounds(&cache.join(format!("{stage}.bounds.json")));
    let (cmin, cmax) = read_bounds(&cache.join(format!("{character}.bounds.json")));
    let center = (smin + smax) * 0.5;
    let radius = ((smax - smin).length() * 0.5).max(1.0);
    // Characters and stages share the game's units, so keep native scale.
    let _ = cmax;
    let char_scale = 1.0;
    let vec_arg = |name: &str| {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| {
            let p: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            (p.len() == 3).then(|| Vec3::new(p[0], p[1], p[2]))
        })
    };
    let at = vec_arg("--at");
    let (eye, look) = (vec_arg("--eye"), vec_arg("--look"));
    let hide_props = args.iter().any(|a| a == "--hide-props");
    let no_char = args.iter().any(|a| a == "--no-char");
    let char_pos = at.unwrap_or(Vec3::new(center.x, 0.0, center.z)) - Vec3::Y * cmin.y;
    // --close: orbit near the character instead of the whole arena.
    let (center, radius) = match args.iter().position(|a| a == "--close").and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<f32>().ok()) {
        Some(d) => (char_pos, d),
        None => (center, radius),
    };
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let angle = opt("--angle").and_then(|v| v.parse().ok());
    let anim = opt("--anim");
    let anim_time = opt("--anim-time").and_then(|v| v.parse().ok());
    let fx = opt("--fx").map(|name| fx::FxSetup { name, pos: char_pos, slot: opt("--fx-slot").and_then(|v| v.parse().ok()).unwrap_or(0), frame: opt("--fx-frame").and_then(|v| v.parse().ok()) });

    let mut app = App::new();
    if let Some(fx) = fx {
        app.insert_resource(fx).insert_resource(effects::FxLibrary::new(cache.clone()));
    }
    app
        .add_plugins(DefaultPlugins.set(AssetPlugin { file_path: cache.to_string_lossy().into_owned(), ..default() }).set(WindowPlugin {
            primary_window: Some(background_if(screenshot.is_some(), Window { title: "Phantom Dust asset proof".into(), resolution: (1600u32, 900u32).into(), ..default() })),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
        .insert_resource(Setup { cache: cache.clone(), stage, character, center, radius, char_pos, char_scale, screenshot, angle, anim, anim_time, eye, look, hide_props, no_char })
        .add_plugins(effects::plugin)
        .add_systems(Startup, (setup, fx::spawn))
        .add_systems(Update, (effects::spawn_trails, effects::attach_pieces, effects::tick_effects, effects::animate_pieces, effects::animate_trails).chain())
        .add_systems(Update, (matte_characters, stage_look::sky_without_fog))
        .add_systems(Update, (walk, orbit, auto_screenshot, prepare_animation, start_animation, hide_prop_nodes))
        .add_systems(
            PostUpdate,
            (dangle::setup_dangles, dangle::swing_dangles.after(bevy::transform::TransformSystems::Propagate)).run_if(|| !std::env::args().any(|a| a == "--no-cloth")),
        )
        .run();
}

/// Once the character glTF is loaded, build a one-clip graph for --anim.
fn prepare_animation(
    mut commands: Commands,
    s: Res<Setup>,
    gltf: Option<Res<CharacterGltf>>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut done: Local<bool>,
) {
    let (Some(name), Some(gltf)) = (&s.anim, gltf) else { return };
    if *done {
        return;
    }
    let Some(g) = gltfs.get(&gltf.0) else { return };
    *done = true;
    let Some(clip) = g.named_animations.get(name.as_str()) else {
        let mut names: Vec<_> = g.named_animations.keys().collect();
        names.sort();
        error!("no animation named {name}; available: {names:?}");
        return;
    };
    let (graph, node) = AnimationGraph::from_clip(clip.clone());
    commands.insert_resource(AnimSetup { graph: graphs.add(graph), node });
}

fn start_animation(
    mut commands: Commands,
    s: Res<Setup>,
    setup: Option<Res<AnimSetup>>,
    mut players: Query<(Entity, &mut AnimationPlayer), Without<AnimationGraphHandle>>,
) {
    let Some(setup) = setup else { return };
    for (e, mut player) in &mut players {
        let active = player.play(setup.node);
        active.repeat();
        if let Some(t) = s.anim_time {
            active.seek_to(t).pause();
        }
        commands.entity(e).insert(AnimationGraphHandle(setup.graph.clone()));
    }
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, s: Res<Setup>) {
    commands.insert_resource(CharacterGltf(assets.load(format!("{}.glb", s.character))));
    commands.spawn(WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(format!("{}.glb", s.stage)))));
    // PD_WALK=<speed>: walk the character along +X (facing it) so cloth
    // physics sees real motion; the camera tracks alongside.
    let walking = env_f32("PD_WALK", 0.0) != 0.0;
    let facing = if walking { Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) } else { Quat::IDENTITY };
    if !s.no_char {
        commands.spawn((
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(format!("{}.glb", s.character)))),
            Transform::from_translation(s.char_pos).with_rotation(facing).with_scale(Vec3::splat(s.char_scale)),
            Walker,
        ));
    }
    commands.spawn((AudioPlayer::new(assets.load("music.wav")), PlaybackSettings::LOOP));
    // Characters are lit (normal-mapped); stages are unlit with baked
    // lighting, so these lights only shade characters.
    // Same stage look as the game (stage_look.rs): the stage's fog and
    // character light from its light script (fog only with PD_STAGE_FOG=1).
    let look = stage_look::StageLook::load(&s.cache, &s.stage);
    let (sun_col, sun_k, sun_dir) = look.sun.unwrap_or((Color::WHITE, 1.0, -Vec3::new(0.4, 1.0, 0.6).normalize()));
    commands.spawn((
        DirectionalLight { color: sun_col, illuminance: env_f32("PD_SUN", LIGHT_SUN) * sun_k, shadow_maps_enabled: false, ..default() },
        Transform::default().looking_to(sun_dir, Vec3::Y),
    ));
    let (amb_col, amb_k) = look.ambient.unwrap_or((Color::WHITE, 1.0));
    commands.insert_resource(GlobalAmbientLight { color: amb_col, brightness: env_f32("PD_AMBIENT", LIGHT_AMBIENT) * amb_k, ..default() });
    let cam = commands
        .spawn((
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection { far: 200_000.0, ..default() }),
            Transform::default(),
            Orbit,
        ))
        .id();
    if let Some(fog) = look.fog {
        commands.entity(cam).insert(fog);
    }
    // --tonemap-none: raw colours (the original has no tone mapping), for A/B sheets.
    if std::env::args().any(|a| a == "--tonemap-none") {
        commands.entity(cam).insert(bevy::core_pipeline::tonemapping::Tonemapping::None);
    }
}

fn orbit(time: Res<Time>, s: Res<Setup>, mut q: Query<&mut Transform, With<Orbit>>) {
    let a = s.angle.unwrap_or(time.elapsed_secs() * 0.15 + 0.8);
    let r = s.radius * 0.55;
    let eye_up = std::env::args().skip_while(|a| a != "--look-y").nth(1).and_then(|v| v.parse::<f32>().ok()).map(|y| y - 10.0).unwrap_or(0.0);
    let eye = s.center + walked(&time) + Vec3::new(a.cos() * r, s.radius * 0.28 + eye_up, a.sin() * r);
    let look_y = std::env::args().skip_while(|a| a != "--look-y").nth(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(10.0);
    let look = s.char_pos + walked(&time) + Vec3::Y * look_y;
    let (eye, look) = (s.eye.unwrap_or(eye), s.look.unwrap_or(look));
    for mut t in &mut q {
        *t = Transform::from_translation(eye).looking_at(look, Vec3::Y);
    }
}

/// --hide-props: hide every stage object node ("propNNN_...").
fn hide_prop_nodes(s: Res<Setup>, mut q: Query<(&Name, &mut Visibility), Added<Name>>) {
    if !s.hide_props {
        return;
    }
    for (name, mut vis) in &mut q {
        if name.as_str().starts_with("prop") {
            *vis = Visibility::Hidden;
        }
    }
}

fn auto_screenshot(mut commands: Commands, s: Res<Setup>, mut frames: Local<u32>, mut exit: MessageWriter<AppExit>) {
    let Some(path) = &s.screenshot else { return };
    *frames += 1;
    // Give the asset server time to load and upload everything.
    // PD_SHOT_COUNT / PD_SHOT_EVERY: a burst of numbered screenshots.
    let count = env_f32("PD_SHOT_COUNT", 1.0) as u32;
    let every = env_f32("PD_SHOT_EVERY", 10.0) as u32;
    for k in 0..count {
        if *frames == 240 + k * every {
            let out = if count == 1 { path.clone() } else { path.with_file_name(format!("{}_{k}.png", path.file_stem().unwrap_or_default().to_string_lossy())) };
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out));
        }
    }
    if *frames == 300 + count * every {
        exit.write(AppExit::Success);
    }
}

/// Character materials are matte: the game has no general specular sheen,
/// and Bevy's default 4% reflectance turns near-black cloth pale grey.
/// Stage materials are unlit and untouched.
fn matte_characters(mut events: MessageReader<AssetEvent<StandardMaterial>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    for ev in events.read() {
        if let AssetEvent::Added { id } = ev {
            if let Some(mut m) = mats.get_mut(*id) {
                if !m.unlit {
                    m.reflectance = 0.0;
                    m.perceptual_roughness = 1.0;
                }
            }
        }
    }
}