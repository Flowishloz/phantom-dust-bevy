//! Live 3D preview of each joined seat's character and costume colour
//! (docs/FRONTEND.md "Character select"): a render-to-texture camera per
//! seat (four), a shared light, and an idling copy of the character glb
//! standing in for the setup screen's flat portrait picture
//! (`frontend/setup.rs`'s `Portrait`). A seat that hasn't joined shows
//! nothing.
//!
//! Independent of `main.rs`'s match scene/animation code, which only exists
//! `in_state(Match)`: this is its own small orbit-camera and one-clip
//! animation setup, alive only `in_state(Setup)` (its entities are tagged
//! `DespawnOnExit(AppState::Setup)`, torn down and rebuilt fresh every visit
//! like the rest of the screen).

use super::state::AppState;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::gltf::{Gltf, GltfAssetLabel};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use std::time::Duration;

/// Render target resolution: the setup screen's portrait window
/// (`setup::WINDOW`, 172x248 design pixels) at twice the size for crispness.
const TEX_SIZE: (u32, u32) = (344, 496);
/// World-space gap between the previews, so one slot's camera (short far
/// plane, below) never renders another slot's model. Simpler and
/// more robust than `RenderLayers`, which doesn't propagate from a spawned
/// glTF scene's root down to its actual mesh children.
const SLOT_GAP: f32 = 4000.0;
const CAM_FAR: f32 = 140.0;
const CAM_FOV_Y: f32 = 0.62;
/// Fraction of the frame's height the character fills: head to toe with a
/// little air above and below (the window is portrait).
const FRAME_FRACTION: f32 = 0.80;
/// Camera eye/look height as a fraction of the character's height: the middle
/// of the body, so the figure sits centred in the tall window.
const EYE_FRACTION: f32 = 0.50;
/// The characters' own forward is +Z (the sim's facing is `(sin yaw, cos yaw)`
/// and the match turns a model with `Quat::from_rotation_y(yaw)`), so a camera
/// on the +Z side looks them in the face: orbit angle pi/2 (`orbit_transform`
/// puts the eye at `(cos a, _, sin a)`).
const FACE_ANGLE: f32 = std::f32::consts::FRAC_PI_2;
/// The model sways this far (radians, each side) either side of facing the
/// viewer, so it feels alive without ever turning side-on.
const SWAY: f32 = 0.30;
/// Radians/second of the sway's phase (one full sway about every 11 s).
const SPIN_SPEED: f32 = 0.55;
/// A short flourish clip (`WAIT_ACT0`, if the character has it) plays for
/// this many seconds whenever a player's pick changes, then blends back to
/// the idle loop (`WAIT0`) -- the brief's "maybe a short flourish", moved
/// from "on confirm" (there isn't one for this row) to "on change".
const FLOURISH_SECONDS: f32 = 0.6;
const BLEND_SECONDS: f32 = 0.15;
// Softer than the old side-on rig: lit from the front, 3000 blew the costumes out to white.
const LIGHT_SUN: f32 = 1500.0;
const LIGHT_AMBIENT: f32 = 350.0;
/// Character height fallback when a `bounds.json` is missing or unreadable
/// (the roster's own rough average -- see `height_from_bounds_json`).
const FALLBACK_HEIGHT: f32 = 19.0;

/// The render targets the setup screen's portrait boxes draw from
/// (`frontend/setup.rs::layout_seats`), one per seat, inserted once when the
/// screen opens.
#[derive(Resource, Clone)]
pub struct PreviewTextures(pub [Handle<Image>; SLOTS]);

/// Seats with a preview.
const SLOTS: usize = super::setup::SEATS;

#[derive(Resource, Default)]
pub(crate) struct CharPreview {
    slots: [Slot; SLOTS],
}

#[derive(Default)]
struct Slot {
    char_id: Option<String>,
    /// Costume colour 0..=3 shown (`cache/chars/<id>.c<colour>.png`).
    colour: u8,
    height: f32,
    gltf: Handle<Gltf>,
    graph: Option<Graph>,
    angle: f32,
    /// Bumped on every change of pick: tells the live model from retiring ones.
    gen: u32,
    /// The costume texture this pick waits on (a mesh whose texture has not
    /// loaded is not drawn at all, so the model must not show before it).
    tex: Option<Handle<Image>>,
    /// Seconds of flourish left to play; 0 = the idle loop should be
    /// playing (or the flourish clip has just ended, this tick).
    flourish: f32,
}

struct Graph {
    handle: Handle<AnimationGraph>,
    wait: AnimationNodeIndex,
    flourish: Option<AnimationNodeIndex>,
}

#[derive(Component)]
pub(crate) struct PreviewCam(usize);
#[derive(Component)]
pub(crate) struct PreviewModel(usize, u32);

/// Source of [`Slot::gen`] numbers.
static NEXT_GEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// A model that has been spawned but must not show yet: its scene is still
/// loading, or it is not yet posed / hull-hidden / costume-swapped. While one
/// is pending the previous model of the slot stays up, so a switch is one
/// clean swap (no empty frames, no bind pose, no shadow-hull lumps).
#[derive(Component, Default)]
pub(crate) struct Pending {
    age: u32,
    posed_frames: u32,
}

/// Frames to wait for a model before showing it anyway (no WAIT0 clip etc.).
const PENDING_TIMEOUT: u32 = 240;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnimKind {
    Wait,
    Flourish,
}

/// Marks an `AnimationPlayer` this module has wired up to its slot's graph
/// (inserted by `attach_players`), plus which clip it's actually playing
/// right now so `drive` only calls `.play()` again when that should change.
/// Kept separate from the attach step deliberately: components inserted via
/// `Commands` aren't visible to a query in the *same* system call (they're
/// only applied at the next sync point), so trying to attach-and-play in one
/// system silently never plays anything -- `main.rs`'s real match animation
/// has the same split (`attach_anim_slots` vs. the continuously-running
/// `drive_animation`) for the same reason.
#[derive(Component)]
pub(crate) struct PreviewAnimPlayer {
    slot: usize,
    gen: u32,
    current: Option<AnimKind>,
}

/// Opens with the setup screen: a render-target camera per seat and a shared
/// light. `sync_characters` spawns the actual character models
/// once `GameData`/`SetupState` say which ones to show.
pub fn spawn(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(CharPreview::default());
    commands.insert_resource(GlobalAmbientLight { color: Color::WHITE, brightness: LIGHT_AMBIENT, ..default() });
    commands.spawn((
        DirectionalLight { color: Color::WHITE, illuminance: LIGHT_SUN, shadow_maps_enabled: false, ..default() },
        Transform::default().looking_to(-Vec3::new(0.35, 1.0, 0.55).normalize(), Vec3::Y),
        DespawnOnExit(AppState::Setup),
    ));
    let mut textures: [Handle<Image>; SLOTS] = std::array::from_fn(|_| Handle::default());
    for (i, tex) in textures.iter_mut().enumerate() {
        let image = Image::new_target_texture(TEX_SIZE.0, TEX_SIZE.1, TextureFormat::Bgra8UnormSrgb, None);
        let handle = images.add(image);
        *tex = handle.clone();
        commands.spawn((
            Camera3d::default(),
            Camera { order: 0, clear_color: ClearColorConfig::Custom(Color::srgb(0.02, 0.05, 0.05)), ..default() },
            RenderTarget::from(handle),
            Projection::Perspective(PerspectiveProjection { fov: CAM_FOV_Y, near: 0.1, far: CAM_FAR, ..default() }),
            Transform::default(),
            PreviewCam(i),
            DespawnOnExit(AppState::Setup),
        ));
    }
    commands.insert_resource(PreviewTextures(textures));
}

/// Distance a camera with [`CAM_FOV_Y`] needs so a character `height` tall
/// fills [`FRAME_FRACTION`] of the vertical frame (all of them, head to toe).
fn frame_distance(height: f32) -> f32 {
    (height.max(1.0) / FRAME_FRACTION * 0.5) / (CAM_FOV_Y * 0.5).tan()
}

/// A character's height from its converted `chars/<id>.bounds.json`
/// (`pd_convert character`'s own `core_min`/`core_max`, PIPELINE.md).
fn height_from_bounds_json(v: &serde_json::Value) -> f32 {
    let min_y = v["min"][1].as_f64();
    let max_y = v["max"][1].as_f64();
    match (min_y, max_y) {
        (Some(lo), Some(hi)) if hi > lo => (hi - lo) as f32,
        _ => FALLBACK_HEIGHT,
    }
}

fn char_height(cache: &std::path::Path, id: &str) -> f32 {
    let v: serde_json::Value = std::fs::read_to_string(cache.join(format!("chars/{id}.bounds.json"))).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    height_from_bounds_json(&v)
}

/// The camera's orbit angle for a slot whose sway is at `phase`: always within
/// [`SWAY`] of straight in front of the model.
fn view_angle(phase: f32) -> f32 {
    FACE_ANGLE + SWAY * phase.sin()
}

/// The camera transform for a slot at world-space `center`, orbiting at
/// `angle` radians.
fn orbit_transform(center: Vec3, height: f32, angle: f32) -> Transform {
    let dist = frame_distance(height);
    let eye_y = height * EYE_FRACTION;
    let eye = center + Vec3::new(angle.cos() * dist, eye_y, angle.sin() * dist);
    let look = center + Vec3::Y * eye_y;
    Transform::from_translation(eye).looking_at(look, Vec3::Y)
}

/// The costume texture a seat's colour swaps in (`pd_convert character`'s
/// `chars/<id>.c<colour>.png`); colour 0 is already baked into the model, so
/// it needs no swap.
pub(crate) fn colour_texture(id: &str, colour: u8) -> Option<String> {
    (colour > 0).then(|| format!("chars/{id}.c{colour}.png"))
}

/// Keeps each slot's spawned model matching its seat's character and colour
/// (`SetupState::previews`), (re)loading and re-centring it when the pick
/// changes; a seat that isn't joined has no model.
pub fn sync_characters(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<crate::Config>, data: Res<super::state::GameData>, state: Res<super::setup::SetupState>, mut preview: ResMut<CharPreview>, existing: Query<(Entity, &PreviewModel, Has<Pending>)>) {
    sync_slots(&mut commands, &assets, &cfg.cache, &data.characters, &state.previews(), &mut preview, &existing);
}

/// The body of [`sync_characters`] on plain values (the tests drive it with scripted picks).
fn sync_slots(commands: &mut Commands, assets: &AssetServer, cache: &std::path::Path, characters: &[super::discovery::CharacterEntry], picks: &[Option<(usize, u8)>; SLOTS], preview: &mut CharPreview, existing: &Query<(Entity, &PreviewModel, Has<Pending>)>) {
    for i in 0..SLOTS {
        let want = picks[i].and_then(|(c, colour)| characters.get(c).map(|c| (c.id.clone(), colour)));
        let shown = preview.slots[i].char_id.clone().map(|id| (id, preview.slots[i].colour));
        if want == shown {
            continue;
        }
        // A model still pending is dropped at once; the shown one stays until
        // its replacement is ready (`reveal`), or goes now if the seat is empty.
        for (e, m, pending) in existing {
            if m.0 == i && (pending || want.is_none()) {
                commands.entity(e).despawn();
            }
        }
        let slot = &mut preview.slots[i];
        // Unique across visits to the screen too (a visit's counter used to restart at 1, so a leftover model could share a number with a new one).
        slot.gen = NEXT_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        slot.tex = None;
        let was_shown_before = slot.char_id.is_some();
        slot.char_id = want.as_ref().map(|w| w.0.clone());
        slot.colour = want.as_ref().map_or(0, |w| w.1);
        slot.graph = None;
        // Facing the viewer from the start (the sway phase starts at zero), a
        // little out of step per seat so fighters don't all move as one.
        slot.angle = i as f32 * 1.7;
        slot.flourish = if was_shown_before { FLOURISH_SECONDS } else { 0.0 };
        let Some((id, _)) = want else { continue };
        slot.height = char_height(cache, &id);
        let path = format!("chars/{id}.glb");
        slot.gltf = assets.load(path.clone());
        let center = Vec3::new(i as f32 * SLOT_GAP, 0.0, 0.0);
        commands.spawn((WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(path))), Transform::from_translation(center), Visibility::Hidden, PreviewModel(i, slot.gen), Pending::default(), DespawnOnExit(AppState::Setup)));
    }
}

/// Shows a pending model once its player is playing its clip (the pose is
/// applied in the same frame's PostUpdate) and the scene's hull/costume fixes
/// have landed, and in that same frame removes the slot's older models.
pub fn reveal(mut commands: Commands, assets: Res<AssetServer>, preview: Res<CharPreview>, players: Query<&PreviewAnimPlayer>, mut pending: Query<(Entity, &PreviewModel, &mut Pending)>, all: Query<(Entity, &PreviewModel), Without<Pending>>) {
    for (e, m, mut p) in &mut pending {
        p.age += 1;
        let posed = players.iter().any(|a| a.slot == m.0 && a.gen == m.1 && a.current.is_some());
        let tex_ready = {
            let slot = &preview.slots[m.0];
            slot.colour == 0 || slot.tex.as_ref().is_some_and(|t| assets.is_loaded_with_dependencies(t))
        };
        if posed && tex_ready {
            p.posed_frames += 1;
        }
        if p.posed_frames < 2 && p.age < PENDING_TIMEOUT {
            continue;
        }
        if preview.slots[m.0].gen != m.1 {
            continue;
        }
        commands.entity(e).insert(Visibility::Inherited).remove::<Pending>();
        for (old, om) in &all {
            if om.0 == m.0 && om.1 != m.1 {
                commands.entity(old).despawn();
            }
        }
    }
}

/// Builds each slot's one-or-two-clip animation graph (`WAIT0` idle,
/// `WAIT_ACT0` flourish if the character has it) once its glTF has loaded.
pub fn build_graphs(mut preview: ResMut<CharPreview>, gltfs: Res<Assets<Gltf>>, mut graphs: ResMut<Assets<AnimationGraph>>) {
    for slot in &mut preview.slots {
        if slot.graph.is_some() || slot.char_id.is_none() {
            continue;
        }
        let Some(g) = gltfs.get(&slot.gltf) else { continue };
        let Some(wait_clip) = g.named_animations.get("WAIT0") else { continue };
        let mut graph = AnimationGraph::new();
        let wait = graph.add_clip(wait_clip.clone(), 1.0, graph.root);
        let flourish = g.named_animations.get("WAIT_ACT0").map(|clip| graph.add_clip(clip.clone(), 1.0, graph.root));
        slot.graph = Some(Graph { handle: graphs.add(graph), wait, flourish });
    }
}

/// Wires a freshly spawned `AnimationPlayer` (from the character scene) to
/// its slot's graph, once that graph exists. Does *not* start playback
/// itself -- `drive` (which runs every frame, not just once at attach time)
/// owns that, so a graph that isn't ready yet at attach time still gets
/// driven correctly the moment it is.
pub fn attach_players(mut commands: Commands, preview: Res<CharPreview>, new_players: Query<Entity, (With<AnimationPlayer>, Without<PreviewAnimPlayer>)>, parents: Query<&ChildOf>, roots: Query<&PreviewModel>) {
    for e in &new_players {
        let mut cur = e;
        let slot_idx = loop {
            if let Ok(r) = roots.get(cur) {
                break Some((r.0, r.1));
            }
            match parents.get(cur) {
                Ok(p) => cur = p.parent(),
                Err(_) => break None,
            }
        };
        let Some((i, gen)) = slot_idx else { continue };
        if preview.slots[i].gen != gen {
            continue;
        }
        let Some(graph) = &preview.slots[i].graph else { continue };
        commands.entity(e).insert((PreviewAnimPlayer { slot: i, gen, current: None }, AnimationGraphHandle(graph.handle.clone()), AnimationTransitions::new()));
    }
}

/// Counts down a slot's flourish timer and blends back to the idle loop
/// once it ends. Runs every frame (not just once at attach time) and always
/// checks the current clip against what should be playing, so a player
/// attached before its graph was ready, or one whose flourish timer expires
/// between attaches, still gets the right clip -- the same shape as
/// `main.rs`'s real match `drive_animation`, for the same reason (see
/// `PreviewAnimPlayer`'s doc comment).
pub fn drive(time: Res<Time>, mut preview: ResMut<CharPreview>, mut players: Query<(&mut PreviewAnimPlayer, &mut AnimationPlayer, &mut AnimationTransitions)>) {
    let dt = time.delta_secs();
    for slot in &mut preview.slots {
        if slot.flourish > 0.0 {
            slot.flourish = (slot.flourish - dt).max(0.0);
        }
    }
    for (mut marker, mut player, mut transitions) in &mut players {
        if preview.slots[marker.slot].gen != marker.gen {
            continue;
        }
        let Some(graph) = &preview.slots[marker.slot].graph else { continue };
        let want = if preview.slots[marker.slot].flourish > 0.0 && graph.flourish.is_some() { AnimKind::Flourish } else { AnimKind::Wait };
        if marker.current == Some(want) {
            continue;
        }
        let fade = if marker.current.is_none() { Duration::ZERO } else { Duration::from_secs_f32(BLEND_SECONDS) };
        match want {
            AnimKind::Flourish => {
                transitions.play(&mut player, graph.flourish.expect("checked above"), fade);
            }
            AnimKind::Wait => {
                transitions.play(&mut player, graph.wait, fade).repeat();
            }
        }
        marker.current = Some(want);
    }
}

/// Turns each slot's camera slowly around its character.
pub fn orbit(time: Res<Time>, mut preview: ResMut<CharPreview>, mut cams: Query<(&PreviewCam, &mut Transform)>) {
    let dt = time.delta_secs();
    for slot in &mut preview.slots {
        slot.angle += dt * SPIN_SPEED;
    }
    for (cam, mut t) in &mut cams {
        let slot = &preview.slots[cam.0];
        let center = Vec3::new(cam.0 as f32 * SLOT_GAP, 0.0, 0.0);
        *t = orbit_transform(center, slot.height.max(1.0), view_angle(slot.angle));
    }
}

/// A seat's costume colour for the setup screen's live preview: newly spawned
/// mesh materials under a slot's model whose colour is not 0 get their own
/// copy with `base_color_texture` swapped to that colour's texture
/// (`cache/chars/<id>.c<colour>.png`, `pd_convert character`'s costume-colour
/// export) -- not a mutation of the shared asset (another seat's model of the
/// same character must stay as it is). The model is respawned whenever the
/// colour changes (`sync_characters`), so only new materials need it.
pub fn apply_colours(mut commands: Commands, mut preview: ResMut<CharPreview>, assets: Res<AssetServer>, new_meshes: Query<Entity, Added<Mesh3d>>, names: Query<&Name>, new_mats: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Added<MeshMaterial3d<StandardMaterial>>>, parents: Query<&ChildOf>, roots: Query<&PreviewModel>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // The glb also carries the authored shadow hulls (`shadowhull_dN`, no
    // material) as child nodes. A match keeps them off every camera
    // (`shadows::tag_fighter_hulls`); here they would draw as white
    // lumps over the body, so they are hidden.
    for e in &new_meshes {
        let mut cur = e;
        let hull = loop {
            if names.get(cur).is_ok_and(|n| n.starts_with(pd_formats::character::HULL_TAG)) {
                break true;
            }
            if roots.get(cur).is_ok() {
                break false;
            }
            match parents.get(cur) {
                Ok(p) => cur = p.parent(),
                Err(_) => break false,
            }
        };
        if hull {
            commands.entity(e).insert(Visibility::Hidden);
        }
    }
    for (e, mat) in &new_mats {
        let mut cur = e;
        let slot_idx = loop {
            if let Ok(r) = roots.get(cur) {
                break Some(r.0);
            }
            match parents.get(cur) {
                Ok(p) => cur = p.parent(),
                Err(_) => break None,
            }
        };
        let Some(slot) = slot_idx.and_then(|i| preview.slots.get_mut(i)) else { continue };
        let Some(path) = slot.char_id.as_deref().and_then(|id| colour_texture(id, slot.colour)) else { continue };
        let tex: Handle<Image> = assets.load(path);
        slot.tex = Some(tex.clone());
        if let Some(src) = materials.get(&mat.0) {
            let mut swapped = src.clone();
            swapped.base_color_texture = Some(tex.clone());
            let handle = materials.add(swapped);
            commands.entity(e).insert(MeshMaterial3d(handle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_from_bounds_reads_min_max_y() {
        let v = serde_json::json!({"min": [-9.5, -0.02, -2.0], "max": [9.5, 18.86, 1.7]});
        let h = height_from_bounds_json(&v);
        assert!((h - 18.88).abs() < 0.01, "{h}");
    }

    #[test]
    fn height_from_bounds_falls_back_when_missing_or_inverted() {
        assert_eq!(height_from_bounds_json(&serde_json::json!({})), FALLBACK_HEIGHT);
        assert_eq!(height_from_bounds_json(&serde_json::json!({"min": [0, 5, 0], "max": [0, 1, 0]})), FALLBACK_HEIGHT);
    }

    #[test]
    fn frame_distance_grows_with_height() {
        assert!(frame_distance(20.0) > frame_distance(10.0));
        assert!(frame_distance(19.0) > 0.0);
    }

    #[test]
    fn the_camera_always_looks_the_fighter_in_the_face() {
        // Model forward is +Z: the eye must be on the +Z side, never beside it.
        let center = Vec3::new(8000.0, 0.0, 0.0);
        for k in 0..200 {
            let a = view_angle(k as f32 * 0.1);
            assert!((a - FACE_ANGLE).abs() <= SWAY + 1e-5);
            let t = orbit_transform(center, 18.0, a);
            let off = t.translation - center;
            assert!(off.z > 0.0 && off.z > off.x.abs() * 2.5, "front-on, not side-on: {off:?}");
            // ...and the camera looks back at the model, along -Z.
            assert!(t.forward().z < -0.9, "{:?}", t.forward());
        }
        assert!((view_angle(0.0) - FACE_ANGLE).abs() < 1e-6, "starts dead ahead");
    }

    #[test]
    fn the_framing_shows_the_whole_fighter_in_the_tall_window() {
        // At the camera's distance the frame is `height / FRAME_FRACTION` tall:
        // head and toe both inside, with the eye at mid-height.
        let h = 19.0;
        let d = frame_distance(h);
        let half = d * (CAM_FOV_Y * 0.5).tan();
        let eye = h * EYE_FRACTION;
        assert!(eye - half < 0.0 && eye + half > h, "feet and head inside the frame");
        let aspect = TEX_SIZE.0 as f32 / TEX_SIZE.1 as f32;
        assert!(aspect < 0.8, "portrait render target");
        assert!((aspect - 172.0 / 248.0).abs() < 0.01, "same shape as setup::WINDOW");
    }

    #[test]
    fn orbit_transform_keeps_camera_at_constant_height_and_radius() {
        let center = Vec3::new(4000.0, 0.0, 0.0);
        let height = 18.0;
        let a = orbit_transform(center, height, 0.0);
        let b = orbit_transform(center, height, std::f32::consts::FRAC_PI_2);
        assert!((a.translation.y - b.translation.y).abs() < 1e-4);
        let ra = (a.translation - center).with_y(0.0).length();
        let rb = (b.translation - center).with_y(0.0).length();
        assert!((ra - rb).abs() < 1e-3, "orbit radius should not change with angle: {ra} vs {rb}");
    }

    #[test]
    fn only_a_non_zero_colour_swaps_the_costume_texture() {
        assert_eq!(colour_texture("pc00a", 0), None, "colour 0 is baked into the model");
        assert_eq!(colour_texture("pc00a", 1).as_deref(), Some("chars/pc00a.c1.png"));
        assert_eq!(colour_texture("pc10a", 3).as_deref(), Some("chars/pc10a.c3.png"));
    }
}

/// The screen's lifecycle with scripted picks and no assets on disk: every model
/// waits out [`PENDING_TIMEOUT`] and is then shown, so these tests run the real
/// `sync_slots` / `reveal` through whole visits to the setup screen.
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::frontend::discovery::CharacterEntry;
    use bevy::asset::AssetPlugin;
    use bevy::state::app::StatesPlugin;

    #[derive(Resource, Clone, Default)]
    struct Picks([Option<(usize, u8)>; SLOTS]);

    fn characters() -> Vec<CharacterEntry> {
        ["pc00a", "pc01a", "pc02a"].iter().map(|id| CharacterEntry { id: id.to_string(), label: id.to_string() }).collect()
    }

    fn drive(mut commands: Commands, assets: Res<AssetServer>, picks: Res<Picks>, mut preview: ResMut<CharPreview>, existing: Query<(Entity, &PreviewModel, Has<Pending>)>) {
        sync_slots(&mut commands, &assets, std::path::Path::new("no-cache"), &characters(), &picks.0, &mut preview, &existing);
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .init_asset::<Image>()
            .init_asset::<bevy::gltf::Gltf>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .init_state::<AppState>()
            .init_resource::<Picks>()
            .add_systems(OnEnter(AppState::Setup), spawn)
            .add_systems(Update, (drive, reveal).run_if(in_state(AppState::Setup)));
        app.update();
        app
    }

    fn go(app: &mut App, to: AppState) {
        app.world_mut().resource_mut::<NextState<AppState>>().set(to);
        app.update();
        app.update();
    }

    fn pick(app: &mut App, seat: usize, p: Option<(usize, u8)>) {
        app.world_mut().resource_mut::<Picks>().0[seat] = p;
    }

    fn frames(app: &mut App, n: usize) {
        for _ in 0..n {
            app.update();
        }
    }

    /// (all models, shown ones) of a slot.
    fn models(app: &mut App, slot: usize) -> (usize, usize) {
        let mut q = app.world_mut().query::<(&PreviewModel, &Visibility, Has<Pending>)>();
        let mine: Vec<_> = q.iter(app.world()).filter(|(m, _, _)| m.0 == slot).collect();
        (mine.len(), mine.iter().filter(|(_, v, pending)| !*pending && **v != Visibility::Hidden).count())
    }

    const SETTLE: usize = PENDING_TIMEOUT as usize + 20;

    #[test]
    fn one_model_per_joined_seat_after_the_models_have_loaded() {
        let mut app = app();
        go(&mut app, AppState::Setup);
        pick(&mut app, 0, Some((0, 0)));
        pick(&mut app, 1, Some((1, 0)));
        frames(&mut app, SETTLE);
        assert_eq!(models(&mut app, 0), (1, 1));
        assert_eq!(models(&mut app, 1), (1, 1));
        assert_eq!(models(&mut app, 2), (0, 0), "an empty seat shows nothing");
    }

    #[test]
    fn leaving_the_screen_takes_its_models_with_it() {
        // They stayed alive in the match and in the next visit (at x = seat * 4000, so slot 0's stood at the stage origin).
        let mut app = app();
        go(&mut app, AppState::Setup);
        pick(&mut app, 0, Some((0, 0)));
        frames(&mut app, SETTLE);
        assert_eq!(models(&mut app, 0), (1, 1));
        go(&mut app, AppState::Match);
        assert_eq!(models(&mut app, 0).0, 0, "no preview model outlives the setup screen");
    }

    #[test]
    fn coming_back_to_the_screen_never_doubles_a_model() {
        // The second visit's models spawned beside the first visit's leftovers, one on top of the other.
        let mut app = app();
        for visit in 0..3 {
            go(&mut app, AppState::Setup);
            pick(&mut app, 0, Some((0, 0)));
            pick(&mut app, 1, Some((1, 0)));
            frames(&mut app, SETTLE);
            assert_eq!(models(&mut app, 0), (1, 1), "visit {visit}, seat 1");
            assert_eq!(models(&mut app, 1), (1, 1), "visit {visit}, seat 2");
            // ...and a change of pick on the later visits still leaves one.
            pick(&mut app, 0, Some((2, 0)));
            frames(&mut app, SETTLE);
            assert_eq!(models(&mut app, 0), (1, 1), "visit {visit}, after a change");
            go(&mut app, AppState::Match);
            pick(&mut app, 0, None);
            pick(&mut app, 1, None);
        }
    }

    #[test]
    fn fast_changes_back_and_forth_settle_on_one_model() {
        let mut app = app();
        go(&mut app, AppState::Setup);
        // Two seats flick through characters and costume colours every frame or two, over and over.
        let script: [(usize, u8); 7] = [(0, 0), (1, 0), (0, 0), (2, 1), (1, 2), (0, 3), (2, 0)];
        for step in 0..60 {
            pick(&mut app, 0, Some(script[step % script.len()]));
            pick(&mut app, 1, Some(script[(step * 3 + 1) % script.len()]));
            frames(&mut app, 1 + step % 3);
        }
        // One of them drops out mid-flick and comes back.
        pick(&mut app, 1, None);
        frames(&mut app, 3);
        pick(&mut app, 1, Some((1, 0)));
        frames(&mut app, 2);
        pick(&mut app, 0, Some((2, 0)));
        frames(&mut app, SETTLE);
        assert_eq!(models(&mut app, 0), (1, 1));
        assert_eq!(models(&mut app, 1), (1, 1));
        let live = app.world().resource::<CharPreview>();
        assert_eq!(live.slots[0].char_id.as_deref(), Some("pc02a"));
        assert_eq!(live.slots[1].char_id.as_deref(), Some("pc01a"));
    }

    #[test]
    fn a_pick_changed_while_the_old_model_is_still_pending_keeps_one_model() {
        let mut app = app();
        go(&mut app, AppState::Setup);
        pick(&mut app, 0, Some((0, 0)));
        frames(&mut app, SETTLE);
        // B is requested, then A again before B could show; a shown model is kept until its successor is ready.
        pick(&mut app, 0, Some((1, 0)));
        frames(&mut app, 30);
        pick(&mut app, 0, Some((0, 0)));
        frames(&mut app, 30);
        assert_eq!(models(&mut app, 0).1, 1, "the shown model stays up while replacements load");
        frames(&mut app, SETTLE);
        assert_eq!(models(&mut app, 0), (1, 1));
    }
}
