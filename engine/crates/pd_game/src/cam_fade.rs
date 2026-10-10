//! Camera-proximity dissolve (lane-fade, O3 play observation, reworked after
//! Laurence's live-play report 2026-09-29).
//!
//! When a fighter ends up very close to its own camera (e.g. backed against
//! a wall -- `follow_cameras` in `main.rs` raycast-pulls the boom in to a
//! ~4-unit floor), the original dematerialises the player model with a
//! grainy effect so you see the scenery through it instead of it filling the
//! screen. This is that: render-only, driven by real camera distances, never
//! touching the sim (`combat.rs`, `sim.rs`, `moves/`).
//!
//! How it works: at spawn every body mesh piece under a `PlayerVisual` root
//! (body AND extra parts -- coat tails etc. -- which are just more meshes
//! under the same root, see `dangle.rs`) gets its own `DissolveMaterial`:
//! its authored `StandardMaterial` as the base (so `Opaque` stays in the
//! opaque pass with depth writes, `Mask` cutout pieces keep their cutoff)
//! plus a per-view coverage uniform (`dissolve.wgsl`). Each frame every
//! (fighter, view) pair eases a coverage value 1.0 -> `PD_FADE_MIN` over the
//! eye-to-head distance (`PD_FADE_START` -> `PD_FADE_END`), and the shader
//! discards fragments above the coverage with interleaved gradient noise --
//! a screen-door grain that thins out evenly, revealing the scenery
//! uniformly instead of hiding whole body parts. Skinning, normal lighting,
//! fog and tone mapping all run the standard path; the `alpha_mode` never
//! changes, so there is no pipeline recompile and no transparent pass at
//! any point, faded or rested.
//!
//! Per-view correctness: each piece stores its owner's coverage for every
//! screen QUADRANT (x, y, z, w = top-left, top-right, bottom-left,
//! bottom-right), and the shader picks the quadrant its split-screen viewport
//! starts in (`step(0.5, viewport.x) + 2 step(0.5, viewport.y)`; the window
//! size is unknown to it). The CPU fills quadrant Q(view) -- read off the
//! camera's actual viewport origin, `views::quadrant_of` -- with that view's
//! eased coverage: side by side, view 1 is quadrant 1; top/bottom, view 1 is
//! quadrant 2; four quarters are quadrants 0-3; view 0 is always 0. In view V
//! each fighter dissolves by its own distance to view V's camera, so your
//! fighter grains in your view while the others stay solid in theirs, for up
//! to four views. (Before four-player this was two values picked by
//! `viewport.x + viewport.y`; the look in a two-player match is unchanged.)
//!
//! Deliberately untouched:
//! - skill-effect meshes, even when parented under the player visual: pieces
//!   under an `effects::FxInstance` ancestor are skipped, as is anything
//!   whose authored mode is not `Opaque`/`Mask` (effect materials are
//!   additive/transparent). The school-aura shell (its own material, not
//!   `StandardMaterial`, so the prep query never matches it).
//! - the sim.
//!
//! Lifecycle (all three were broken before, see the `wall_lifecycle` test):
//! - recovery snaps home: the eased value holds still inside a 0.01
//!   hysteresis band, so it used to stall at ~0.99 while the restore bar sat
//!   at 0.9999 -- stuck ghostly forever. Now any target at/above 0.9999
//!   snaps the live value to exactly 1.0 once inside the band.
//! - newly converted pieces are stamped with their slot's CURRENT coverage,
//!   so material streamed in mid-fade (or a rematch starting mid-fade)
//!   never pops solid for a frame.
//! - a new match (new `PlayerVisual` roots) resets the eased state.
//! - `PD_CAMFADE=0` (including mid-session) puts every piece back on its
//!   ORIGINAL material handle and drops the marker, then leaves materials
//!   alone until re-enabled.
//!
//! Ordering (wired in `main.rs`, same file that registers `plugin`): prep
//! runs after `apply_p2_colour` (so P2's costume swap is already on the
//! handle the conversion clones) and after `school_aura::spawn_aura_meshes`
//! (so no piece misses its aura shell). Same technique as
//! `school_aura.rs::HasAuraShell` and P2's alt-colour swap in
//! `main.rs::apply_p2_colour`.
//!
//! Knobs (all env, read every frame like the other camera overrides):
//! `PD_CAMFADE=0` disables entirely (default enabled),
//! `PD_FADE_START` (default 9.0: fully solid beyond this),
//! `PD_FADE_END` (default 4.0: at `PD_FADE_MIN` below this),
//! `PD_FADE_MIN` (default 0.15: never fully invisible, so the fighter's
//! position still reads), `PD_DISSOLVE_GRAIN` (default 1.0: grain block size
//! in pixels), `PD_FADE_LOG=1` logs per-view distance/target/coverage.

use crate::combat::MAX_PLAYERS;
use crate::{env_f32, views, FxInstance, PlayerCamera, PlayerVisual, World};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::shader::ShaderRef;

/// Fully solid at or beyond this camera distance (units).
fn fade_start() -> f32 {
    env_f32("PD_FADE_START", 9.0)
}
/// At `fade_min` at or below this camera distance (units).
fn fade_end() -> f32 {
    env_f32("PD_FADE_END", 4.0)
}
/// Floor coverage: the fighter grains but never fully vanishes.
fn fade_min() -> f32 {
    env_f32("PD_FADE_MIN", 0.15)
}
/// Grain block size in pixels for the screen-door discard.
fn grain_px() -> f32 {
    env_f32("PD_DISSOLVE_GRAIN", 1.0).max(1.0)
}

/// The grain uniform: block size and the game area's corner.
fn grain_vec() -> Vec4 {
    let o = crate::display::area_origin();
    Vec4::new(grain_px(), o.x as f32, o.y as f32, 0.0)
}

/// Master switch: `PD_CAMFADE=0` disables entirely, default enabled.
fn enabled() -> bool {
    match std::env::var("PD_CAMFADE") {
        Ok(v) => v != "0",
        Err(_) => true,
    }
}

/// Smoothing rate (per second) for the live coverage's pursuit of the target.
const FADE_RATE: f32 = 10.0;
/// Hysteresis band: while the target sits inside this of the live value the
/// live value holds still, so boundary tremor never shimmers the model.
const FADE_BAND: f32 = 0.01;
/// At or above this target the live value snaps to exactly 1.0 once inside
/// the band (see `step_live`): the rest state is exact, never ~0.99.
const REST_SNAP: f32 = 0.9999;

/// Target coverage for a camera distance: 1.0 at/above `start`, `min`
/// at/below `end`, linear between. Pure (unit-tested).
pub(crate) fn fade_target(dist: f32, start: f32, end: f32, min: f32) -> f32 {
    if start <= end {
        // Degenerate config (env typo): fall back to a hard step.
        return if dist < start { min } else { 1.0 };
    }
    if dist >= start {
        1.0
    } else if dist <= end {
        min
    } else {
        min + (1.0 - min) * (dist - end) / (start - end)
    }
}

/// One smoothing step of the live coverage toward the target: exponential
/// pursuit, holding still inside `band` (hysteresis). Pure (unit-tested).
pub(crate) fn smooth_alpha(current: f32, target: f32, dt: f32, rate: f32, band: f32) -> f32 {
    if (target - current).abs() <= band {
        return current;
    }
    let k = 1.0 - (-rate * dt.max(0.0)).exp();
    current + (target - current) * k
}

/// One frame of one (fighter, view) slot: `smooth_alpha` pursuit, except a
/// (near-)opaque target snaps home to exactly 1.0 once inside the band, so
/// walking away from the wall always fully restores. Pure (unit-tested).
/// (`fade_target` returns exactly 1.0 whenever solid, so the snap only ever
/// fires on a genuinely opaque target.)
pub(crate) fn step_live(live: f32, target: f32, dt: f32) -> f32 {
    if target >= REST_SNAP && (target - live).abs() <= FADE_BAND {
        1.0
    } else {
        smooth_alpha(live, target, dt, FADE_RATE, FADE_BAND)
    }
}

// ---------------------------------------------------------------------------
// Dissolve material: the authored StandardMaterial plus a per-view coverage.
// ---------------------------------------------------------------------------

static DISSOLVE_SHADER: std::sync::OnceLock<Handle<Shader>> = std::sync::OnceLock::new();

/// One fighter's per-frame shading state, as `dissolve.wgsl` reads it: its
/// coverage (1 = fully solid) in each screen quadrant -- x = top-left, y =
/// top-right, z = bottom-left, w = bottom-right (see the module docs) -- and
/// its character light (`char_light.rs`).
#[derive(bevy::render::render_resource::ShaderType, Clone, Copy, Debug, PartialEq)]
pub struct FighterSlot {
    pub coverage: Vec4,
    pub light: crate::char_light::CharLightUniform,
}

impl Default for FighterSlot {
    fn default() -> Self {
        Self { coverage: Vec4::ONE, light: default() }
    }
}

/// Every fighter's slot in ONE storage buffer, bound by every fighter piece.
///
/// Before (perf lane, 2026-10-03) these values lived in each piece's own
/// material, so a light or coverage change rewrote ~100 materials a frame and
/// the render thread re-prepared each one (`prepare-assets` 16-20 ms with
/// skills firing at 4 players). Now a change is one in-place write into this
/// buffer (`upload_fighter_block`); the pieces' materials never change.
/// `grain` = x grain block size in pixels (`PD_DISSOLVE_GRAIN`), y, z = the
/// game area's top-left corner in the window (`display::area_origin`: the
/// views sit inside it when a fixed aspect ratio leaves bars, and the shader
/// takes the quadrant relative to it); w unused.
#[derive(bevy::render::render_resource::ShaderType, Clone, Copy, Debug, PartialEq)]
pub struct FighterBlock {
    pub grain: Vec4,
    pub slots: [FighterSlot; MAX_PLAYERS],
}

impl Default for FighterBlock {
    fn default() -> Self {
        Self { grain: Vec4::ZERO, slots: [FighterSlot::default(); MAX_PLAYERS] }
    }
}

/// Per-piece dissolve state: only WHICH fighter the piece belongs to (never
/// changes) plus the shared buffers; everything that moves is in the shared
/// [`FighterBlock`].
#[derive(Asset, bevy::render::render_resource::AsBindGroup, TypePath, Clone, Debug)]
pub struct Dissolve {
    /// x = the fighter's slot (index into `FighterBlock::slots`).
    #[uniform(100)]
    pub(crate) slot: UVec4,
    /// The shared [`FighterBlock`] (all fighters' coverage and light).
    #[storage(101, read_only)]
    pub(crate) block: Handle<bevy::render::storage::ShaderBuffer>,
    /// The stage's shadow volumes (`shadow_grid.rs`; `shadows.rs` points every
    /// fighter piece at the real buffer once the stage's grid is built).
    #[storage(103, read_only)]
    pub(crate) vol: Handle<bevy::render::storage::ShaderBuffer>,
}

/// A character body piece: its own authored `StandardMaterial` (opaque stays
/// opaque, cutout `Mask` keeps its cutoff) rendered by `dissolve.wgsl`, which
/// adds the per-view screen-door discard and otherwise shades standard.
pub type DissolveMaterial = ExtendedMaterial<StandardMaterial, Dissolve>;

impl MaterialExtension for Dissolve {
    fn fragment_shader() -> ShaderRef {
        DISSOLVE_SHADER.get().cloned().map_or(ShaderRef::Default, ShaderRef::Handle)
    }
}

/// Registers the dissolve material pipeline and the shared fade state.
/// Called from `main.rs` next to the other material plugins.
pub fn plugin(app: &mut App) {
    let shader = app
        .world_mut()
        .resource_mut::<Assets<Shader>>()
        .add(Shader::from_wgsl(include_str!("dissolve.wgsl"), "pd_game/dissolve.wgsl"));
    let _ = DISSOLVE_SHADER.set(shader);
    app.add_plugins(MaterialPlugin::<DissolveMaterial>::default());
    app.init_resource::<FadeState>();
}

/// Eased coverage state, shared between prep (stamps new pieces) and update.
#[derive(Resource)]
pub(crate) struct FadeState {
    /// Live coverage per (fighter, view).
    live: [[f32; MAX_PLAYERS]; MAX_PLAYERS],
    /// Last per-quadrant coverage written per fighter.
    written: [Vec4; MAX_PLAYERS],
    /// The screen quadrant each view's camera renders in (`None` = no such camera).
    quad: [Option<usize>; MAX_PLAYERS],
    /// `PlayerVisual` roots the state belongs to; a new match resets it.
    roots: [Option<Entity>; MAX_PLAYERS],
    /// CPU copy of the shared per-fighter buffer every fighter piece binds.
    pub(crate) block: FighterBlock,
    /// The GPU buffer (made on first use; `None` in tests without `Assets<ShaderBuffer>`).
    block_handle: Option<Handle<bevy::render::storage::ShaderBuffer>>,
    /// `block` changed since the last upload.
    block_dirty: bool,
    /// One dissolve material per (fighter slot, source material): every piece of a fighter that shares a
    /// source material shares the converted one too (`prep_fade_materials`).
    shared: std::collections::HashMap<(usize, AssetId<StandardMaterial>), Handle<DissolveMaterial>>,
}

impl Default for FadeState {
    fn default() -> Self {
        Self {
            live: [[1.0; MAX_PLAYERS]; MAX_PLAYERS],
            written: [Vec4::splat(-1.0); MAX_PLAYERS],
            quad: [None; MAX_PLAYERS],
            roots: [None; MAX_PLAYERS],
            block: FighterBlock::default(),
            block_handle: None,
            block_dirty: false,
            shared: Default::default(),
        }
    }
}

impl FadeState {
    /// Sets fighter `slot`'s coverage in the shared block; true when it changed (and so needs an upload).
    pub(crate) fn set_coverage(&mut self, slot: usize, cov: Vec4) -> bool {
        let Some(s) = self.block.slots.get_mut(slot) else { return false };
        if s.coverage == cov {
            return false;
        }
        s.coverage = cov;
        self.block_dirty = true;
        true
    }

    /// Sets the grain / game-area-corner vector; true when it changed.
    pub(crate) fn set_grain(&mut self, grain: Vec4) -> bool {
        if self.block.grain == grain {
            return false;
        }
        self.block.grain = grain;
        self.block_dirty = true;
        true
    }

    /// Sets fighter `slot`'s character light; true when it changed. Writing an equal value touches nothing.
    pub(crate) fn set_light(&mut self, slot: usize, light: crate::char_light::CharLightUniform) -> bool {
        let Some(s) = self.block.slots.get_mut(slot) else { return false };
        if s.light == light {
            return false;
        }
        s.light = light;
        self.block_dirty = true;
        true
    }

    /// Forgets everything eased so far (a new match, or `PD_CAMFADE=0`).
    fn reset(&mut self) {
        self.live = [[1.0; MAX_PLAYERS]; MAX_PLAYERS];
        self.written = [Vec4::splat(-1.0); MAX_PLAYERS];
    }

    /// Fighter `p`'s coverage per screen quadrant: each view's eased value
    /// goes in the quadrant that view's camera renders in (views without a
    /// camera, and quadrants nobody renders in, stay 1.0 = solid).
    fn coverage_vec(&self, p: usize) -> Vec4 {
        let mut cov = Vec4::ONE;
        for v in 0..MAX_PLAYERS {
            if let Some(q) = self.quad[v] {
                cov[q] = self.live[p][v];
            }
        }
        cov
    }
}

/// Marks a body mesh piece under a `PlayerVisual` converted to its own
/// `DissolveMaterial`; carries the player slot plus the ORIGINAL material
/// handle so `PD_CAMFADE=0` restores it exactly.
#[derive(Component)]
pub(crate) struct FadePiece {
    pub(crate) slot: usize,
    orig: Handle<StandardMaterial>,
}

/// Give every character body mesh piece under a `PlayerVisual` its own
/// dissolve material, once. Only `Opaque`/`Mask` pieces convert (effect
/// meshes under the visual are additive/transparent or sit under an
/// `FxInstance` root -- both are skipped); pieces whose material asset isn't
/// loaded yet wait in `pending` and are retried next frame. `FadePiece`
/// stops reprocessing. New clones are stamped with the slot's current
/// coverage, so mid-fade spawns never pop. Mirrors
/// `school_aura.rs::spawn_aura_meshes`.
///
/// Must run after `apply_p2_colour` and `school_aura::spawn_aura_meshes`
/// (ordered in `main.rs`): the clone carries P2's swapped costume texture,
/// and the aura shell pass still sees the original `StandardMaterial`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prep_fade_materials(
    mut commands: Commands,
    mut state: ResMut<FadeState>,
    mut buffers: Option<ResMut<Assets<bevy::render::storage::ShaderBuffer>>>,
    new_mats: Query<(Entity, &MeshMaterial3d<StandardMaterial>), (Added<MeshMaterial3d<StandardMaterial>>, Without<FadePiece>)>,
    unmarked: Query<(), Without<FadePiece>>,
    current_mat: Query<&MeshMaterial3d<StandardMaterial>>,
    parents: Query<&ChildOf>,
    roots: Query<&PlayerVisual>,
    fx_roots: Query<&FxInstance>,
    mut pending: Local<Vec<Entity>>,
    // Full-scan source, used ONLY on the disabled -> enabled edge: while
    // disabled this system's change ticks keep advancing past the restore
    // inserts, so their `Added` triggers are spent and re-enable must
    // re-seed explicitly (one frame; the normal path filters still apply).
    rescan: Query<Entity, (With<MeshMaterial3d<StandardMaterial>>, Without<FadePiece>)>,
    mut was_enabled: Local<bool>,
    materials: Res<Assets<StandardMaterial>>,
    mut dissolve: ResMut<Assets<DissolveMaterial>>,
) {
    let on = enabled();
    if on && !*was_enabled {
        pending.extend(rescan.iter());
    }
    *was_enabled = on;
    if !on {
        return;
    }
    pending.extend(new_mats.iter().map(|(e, _)| e));
    pending.sort_unstable();
    pending.dedup();
    let todo = std::mem::take(&mut *pending);
    for e in todo {
        if unmarked.get(e).is_err() {
            continue; // despawned, or already processed, meanwhile
        }
        // Effect meshes parented under the fighter (emitters, blades,
        // projectiles) belong to the effects lane: never touch them. The
        // walk matches `effects::attach_pieces`' own root search, so both
        // systems agree on what an effect is.
        if parents.iter_ancestors(e).any(|a| fx_roots.get(a).is_ok()) {
            if std::env::var_os("PD_FADE_LOG").is_some() {
                eprintln!("[fade] skip effect-rooted piece");
            }
            continue;
        }
        let mut cur = e;
        let slot = loop {
            if let Ok(v) = roots.get(cur) {
                break Some(v.0);
            }
            match parents.get(cur) {
                Ok(p) => cur = p.parent(),
                Err(_) => break None,
            }
        };
        let Some(slot) = slot.filter(|&s| s < MAX_PLAYERS) else { continue };
        // The live handle off the entity itself (pending entities from an
        // earlier frame no longer match `Added`, so it must be re-read).
        let src = current_mat.get(e).ok().and_then(|m| materials.get(&m.0));
        let Some(src) = src.cloned() else {
            // Material asset not loaded yet (glTF still streaming): the
            // `Added` trigger is spent, so re-queue explicitly.
            pending.push(e);
            continue;
        };
        // Body pieces are authored `Opaque` (most) or `Mask` (alpha-cutout
        // quads every character glb carries). Anything else under the visual
        // is an effect leftover -- leave it for its own system.
        if !matches!(src.alpha_mode, AlphaMode::Opaque | AlphaMode::Mask(_)) {
            if std::env::var_os("PD_FADE_LOG").is_some() {
                eprintln!("[fade] skip slot {slot} mode {:?}", src.alpha_mode);
            }
            continue;
        }
        let orig = current_mat.get(e).map(|m| m.0.clone()).expect("handle re-read");
        // Same matte treatment `main.rs::matte_characters` gives the source
        // materials (the clone is inline in the extended material, so the
        // asset-event pass never sees it): no specular sheen.
        let mut base = src;
        if !base.unlit {
            base.reflectance = 0.0;
            base.perceptual_roughness = 1.0;
        }
        // The shared per-fighter buffer, made on first use.
        if state.block_handle.is_none() {
            if let Some(b) = buffers.as_mut() {
                // Written in place whenever it changes (`upload_fighter_block`): same size, `COPY_DST`.
                state.block_handle = Some(b.add(bevy::render::storage::ShaderBuffer::from(state.block)));
                state.block_dirty = false;
            }
        }
        let block = state.block_handle.clone().unwrap_or_default();
        let cov = state.coverage_vec(slot);
        // Pieces of one fighter that share a source material share the dissolve material too: nothing in it
        // is per piece (the coverage and light live in the shared `FighterBlock`), and one material lets
        // Bevy draw all of them together instead of one bind group per piece -- a fighter's ~39 cut-out
        // pieces were ~39 separate alpha-mask draws in every view (overnight perf 2026-10-09).
        // `PD_FADE_SHARE=0` gives every piece its own again.
        let share = std::env::var("PD_FADE_SHARE").map_or(true, |v| v != "0");
        let key = (slot, orig.id());
        let make = |dissolve: &mut Assets<DissolveMaterial>| dissolve.add(DissolveMaterial { base, extension: Dissolve { slot: UVec4::new(slot as u32, 0, 0, 0), block, vol: crate::shadows::volume_buffer() } });
        let handle = match state.shared.get(&key).filter(|h| share && dissolve.contains(*h)) {
            Some(h) => h.clone(),
            None => {
                let h = make(&mut dissolve);
                if share {
                    state.shared.insert(key, h.clone());
                }
                h
            }
        };
        if std::env::var_os("PD_FADE_LOG").is_some() {
            eprintln!("[fade] convert slot {slot} cov ({:.2},{:.2},{:.2},{:.2})", cov.x, cov.y, cov.z, cov.w);
        }
        commands.entity(e).insert((MeshMaterial3d(handle), FadePiece { slot, orig })).remove::<MeshMaterial3d<StandardMaterial>>();
    }
}

/// Per-frame: coverage per (fighter, view) from that view's camera distance
/// to that fighter's head, eased with hysteresis, written into each piece's
/// dissolve uniform. With `PD_CAMFADE=0`, every piece goes back on its
/// original handle once, then materials are left alone. Reads render state
/// only (cameras/player transforms); never touches the sim.
pub(crate) fn update_fade(
    mut commands: Commands,
    time: Res<Time>,
    world: Res<World>,
    mut state: ResMut<FadeState>,
    cams: Query<(&PlayerCamera, &GlobalTransform, Option<&Camera>)>,
    roots: Query<(Entity, &PlayerVisual, &GlobalTransform)>,
    pieces: Query<(Entity, &FadePiece, &MeshMaterial3d<DissolveMaterial>)>,
) {
    if !enabled() {
        // Mid-session disable (or disable from the start): put every piece
        // back on its exact original handle immediately, then stop touching
        // materials entirely until the switch is turned back on.
        if !pieces.is_empty() {
            for (e, piece, _) in &pieces {
                commands.entity(e).insert(MeshMaterial3d(piece.orig.clone())).remove::<FadePiece>().remove::<MeshMaterial3d<DissolveMaterial>>();
            }
        }
        state.reset();
        state.roots = [None; MAX_PLAYERS];
        return;
    }
    // A new match spawns new `PlayerVisual` roots: reset the eased state so
    // last match's fade never leaks into this one.
    let mut now: [Option<Entity>; MAX_PLAYERS] = [None; MAX_PLAYERS];
    for (e, v, _) in &roots {
        if v.0 < MAX_PLAYERS {
            now[v.0] = Some(e);
        }
    }
    if now != state.roots {
        state.reset();
        state.roots = now;
    }
    let (start, end, min) = (fade_start(), fade_end(), fade_min());
    let mut campos: [Option<Vec3>; MAX_PLAYERS] = [None; MAX_PLAYERS];
    let mut quad: [Option<usize>; MAX_PLAYERS] = [None; MAX_PLAYERS];
    for (c, gt, cam) in &cams {
        if c.0 < MAX_PLAYERS {
            campos[c.0] = Some(gt.translation());
            quad[c.0] = Some(views::quadrant_of(cam, c.0));
        }
    }
    state.quad = quad;
    let mut bodypos: [Option<Vec3>; MAX_PLAYERS] = [None; MAX_PLAYERS];
    for (_, v, gt) in &roots {
        if v.0 < MAX_PLAYERS {
            bodypos[v.0] = Some(gt.translation());
        }
    }
    let dt = time.delta_secs();
    let log = std::env::var_os("PD_FADE_LOG").is_some();
    for p in 0..MAX_PLAYERS {
        for v in 0..MAX_PLAYERS {
            // Eye-to-head: the boom orbits the head, so this is the boom
            // length. Head height matches `follow_cameras` (+16.0).
            let dist = match (campos[v], bodypos[p]) {
                (Some(c), Some(b)) => c.distance(b + Vec3::Y * 16.0),
                _ => f32::INFINITY,
            };
            let target = fade_target(dist, start, end, min);
            state.live[p][v] = step_live(state.live[p][v], target, dt);
            if log && dist.is_finite() {
                eprintln!("[fade] t{} p{p} v{v} q{:?} dist {dist:.1} target {target:.2} live {:.3}", world.0.tick, quad[v], state.live[p][v]);
            }
        }
    }
    // Grain size, and the game area's corner (it moves with the aspect ratio or window size).
    state.set_grain(grain_vec());
    for p in 0..MAX_PLAYERS {
        let cov = state.coverage_vec(p);
        if cov == state.written[p] {
            continue;
        }
        state.written[p] = cov;
        state.set_coverage(p, cov);
    }
}

/// Uploads the shared per-fighter block when `update_fade` or
/// `char_light::update_char_light` changed it: ONE in-place buffer write
/// instead of a rewrite of every fighter piece's material.
pub(crate) fn upload_fighter_block(mut state: ResMut<FadeState>, buffers: Option<ResMut<Assets<bevy::render::storage::ShaderBuffer>>>) {
    if !state.block_dirty {
        return;
    }
    let (Some(h), Some(mut buffers)) = (state.block_handle.clone(), buffers) else { return };
    if let Some(mut b) = buffers.get_mut(&h) {
        b.set_data(state.block);
        state.block_dirty = false;
    };
}

#[cfg(test)]
mod tests {
    /// Some tests set `PD_CAMFADE` for the whole process; cargo runs tests on
    /// parallel threads, so every test here takes this lock first (otherwise a
    /// test can read the switch another one set -- seen as a rare failure).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    use super::*;

    /// The shared per-fighter block reports a change only when a value differs, so an unchanged
    /// frame never marks it for upload (and never touches a piece's material, which no longer
    /// holds any of these values).
    #[test]
    fn the_fighter_block_flags_only_real_changes() {
        let mut st = FadeState::default();
        assert!(!st.set_coverage(0, Vec4::ONE), "solid again is no change");
        assert!(!st.block_dirty);
        assert!(st.set_coverage(1, Vec4::new(0.5, 1.0, 1.0, 1.0)));
        assert!(st.block_dirty);
        st.block_dirty = false;
        assert!(!st.set_coverage(1, Vec4::new(0.5, 1.0, 1.0, 1.0)), "same coverage twice");
        assert!(!st.set_grain(Vec4::ZERO) && !st.block_dirty, "the default grain is the default");
        assert!(st.set_grain(Vec4::new(1.0, 0.0, 0.0, 0.0)));
        st.block_dirty = false;
        let l = crate::char_light::CharLightUniform { dl_col: Vec4::new(0.1, 0.2, 0.3, 0.0), ..default() };
        assert!(st.set_light(2, l));
        st.block_dirty = false;
        assert!(!st.set_light(2, l) && !st.block_dirty, "same light twice");
        assert!(!st.set_light(MAX_PLAYERS, l), "no such fighter");
        assert_eq!(st.block.slots[2].light, l);
        assert_eq!(st.block.slots[1].coverage.x, 0.5);
    }

    /// The block's GPU layout: the WGSL `FighterBlock` is `grain` then four slots of
    /// (coverage, CharLight), tightly in vec4 units (`CharLight` is 8 + 3 + 3 vec4s).
    #[test]
    fn the_fighter_block_layout_matches_the_shader() {
        use bevy::render::render_resource::ShaderType;
        let slot = 16 + 14 * 16;
        assert_eq!(FighterSlot::min_size().get(), slot as u64);
        assert_eq!(FighterBlock::min_size().get(), (16 + 4 * slot) as u64);
    }

    /// An upload happens only when the block is dirty, and clears the flag.
    #[test]
    fn upload_writes_the_block_once_per_change() {
        use bevy::render::storage::ShaderBuffer;
        let mut app = App::new();
        app.init_resource::<Assets<ShaderBuffer>>().init_resource::<FadeState>();
        app.add_systems(Update, upload_fighter_block);
        let h = app.world_mut().resource_mut::<Assets<ShaderBuffer>>().add(ShaderBuffer::from(FighterBlock::default()));
        app.world_mut().resource_mut::<FadeState>().block_handle = Some(h.clone());
        app.update();
        let bytes = |app: &App| app.world().resource::<Assets<ShaderBuffer>>().get(&h).and_then(|b| b.data.clone()).expect("data");
        let before = bytes(&app);
        app.world_mut().resource_mut::<FadeState>().set_coverage(3, Vec4::splat(0.25));
        app.update();
        assert!(!app.world().resource::<FadeState>().block_dirty, "flag cleared");
        let after = bytes(&app);
        assert_eq!(before.len(), after.len(), "same size, so the GPU buffer is written in place");
        assert_ne!(before, after);
        // The last slot's coverage sits at 16 + 3 * slot.
        let at = 16 + 3 * (16 + 14 * 16);
        assert_eq!(&after[at..at + 4], &0.25f32.to_le_bytes());
    }

    #[test]
    fn fade_is_opaque_beyond_start() {
        let _env = env_lock();
        assert_eq!(fade_target(60.0, 9.0, 4.0, 0.15), 1.0);
        assert_eq!(fade_target(9.0, 9.0, 4.0, 0.15), 1.0);
        assert_eq!(fade_target(200.0, 9.0, 4.0, 0.15), 1.0);
    }

    #[test]
    fn fade_hits_min_at_end() {
        let _env = env_lock();
        assert_eq!(fade_target(4.0, 9.0, 4.0, 0.15), 0.15);
        assert_eq!(fade_target(0.0, 9.0, 4.0, 0.15), 0.15);
    }

    #[test]
    fn fade_interpolates_linearly_between() {
        let _env = env_lock();
        // Midway (6.5) between 9.0 and 4.0 -> midway between 1.0 and 0.15.
        let a = fade_target(6.5, 9.0, 4.0, 0.15);
        assert!((a - 0.575).abs() < 1e-6, "mid alpha {a}");
        // Monotone: nearer is never more opaque.
        let near = fade_target(5.0, 9.0, 4.0, 0.15);
        let far = fade_target(8.0, 9.0, 4.0, 0.15);
        assert!(near < a && a < far);
        // Custom env-scale values behave the same way.
        assert_eq!(fade_target(30.0, 30.0, 15.0, 0.2), 1.0);
        assert_eq!(fade_target(15.0, 30.0, 15.0, 0.2), 0.2);
    }

    #[test]
    fn fade_degenerate_config_steps_instead_of_dividing_by_zero() {
        let _env = env_lock();
        assert_eq!(fade_target(5.0, 4.0, 4.0, 0.15), 1.0);
        assert_eq!(fade_target(3.9, 4.0, 4.0, 0.15), 0.15);
    }

    #[test]
    fn smooth_holds_inside_the_hysteresis_band() {
        let _env = env_lock();
        // Boundary tremor (+/-0.005 around 0.5) must not move the model at all.
        assert_eq!(smooth_alpha(0.5, 0.505, 0.016, 10.0, 0.01), 0.5);
        assert_eq!(smooth_alpha(0.5, 0.495, 0.016, 10.0, 0.01), 0.5);
    }

    #[test]
    fn smooth_pursues_outside_the_band_and_settles() {
        let _env = env_lock();
        // One step moves toward the target but does not jump to it.
        let step = smooth_alpha(1.0, 0.15, 0.016, 10.0, 0.01);
        assert!(step < 1.0 && step > 0.15, "one step {step}");
        // Sustained pursuit settles inside the band and then holds (no orbit).
        let mut v = 1.0;
        for _ in 0..600 {
            v = smooth_alpha(v, 0.15, 0.016, 10.0, 0.01);
        }
        assert!((v - 0.15).abs() <= 0.01, "settled {v}");
        let held = smooth_alpha(v, 0.15, 0.016, 10.0, 0.01);
        assert_eq!(held, v);
        // Recovery back toward opaque approaches but (old behaviour) stalls
        // just short -- the `step_live` test below pins the fixed recovery.
        for _ in 0..600 {
            v = smooth_alpha(v, 1.0, 0.016, 10.0, 0.01);
        }
        assert!((v - 1.0).abs() <= 0.01, "recovered {v}");
    }

    #[test]
    fn step_live_snaps_opaque_home() {
        let _env = env_lock();
        // The reported bug: smoothing alone stalls at ~0.99 (inside the
        // band but below any sane restore bar), so the fighter never came
        // fully back. An opaque target snaps to exactly 1.0.
        let mut v = 0.15;
        for _ in 0..600 {
            v = step_live(v, 1.0, 1.0 / 60.0);
        }
        assert_eq!(v, 1.0, "recovery stalled at {v}");
        // Mid-range targets still ease (no snap, no jump).
        let step = step_live(1.0, 0.15, 1.0 / 60.0);
        assert!(step < 1.0 && step > 0.15, "one step {step}");
        // Tremor at rest stays exactly put.
        assert_eq!(step_live(1.0, 1.0, 1.0 / 60.0), 1.0);
    }

    /// Full wall lifecycle through the REAL per-slot step (`step_live` over
    /// `fade_target` outputs, the same values `update_fade` writes):
    /// approach -> hold at the wall -> walk away -> second approach ->
    /// second recovery. Every leg must settle to its rest value within a
    /// bounded number of frames. Failed before the fix (recovery stalled
    /// below opaque forever: smoothing stops within 0.01 of target while
    /// restore needed >= 0.9999).
    #[test]
    fn wall_lifecycle_approach_recover_twice() {
        let _env = env_lock();
        let dt = 1.0 / 60.0;
        // One leg toward `target`: wants `want` coverage; returns frames
        // consumed, or None when the leg never settles.
        fn run_leg(live: &mut f32, dt: f32, target: f32, want: f32) -> Option<u32> {
            for frame in 0..600 {
                *live = step_live(*live, target, dt);
                let settled = if want >= REST_SNAP {
                    *live == 1.0
                } else {
                    (*live - want).abs() <= FADE_BAND
                };
                if settled {
                    return Some(frame);
                }
            }
            None
        }
        let mut live = 1.0f32;
        // Leg 1: approach the wall until fully faded.
        let target = fade_target(4.0, 9.0, 4.0, 0.15);
        assert!(target < 1.0);
        assert!(run_leg(&mut live, dt, target, target).is_some(), "approach never fades");
        // Leg 2: walk away; must come ALL the way back, not stall at 0.99.
        assert!(run_leg(&mut live, dt, 1.0, 1.0).is_some(), "first recovery stuck at {live}");
        assert_eq!(live, 1.0, "recovery left a ghost coverage");
        // Legs 3-4: a second approach must fade again and recover again
        // (sticky state from leg 2 would break either).
        assert!(run_leg(&mut live, dt, target, target).is_some(), "second approach never fades");
        assert!(run_leg(&mut live, dt, 1.0, 1.0).is_some(), "second recovery stuck at {live}");
        assert_eq!(live, 1.0);
    }

    /// ECS lifecycle through the REAL production systems: a synthetic
    /// character root with an opaque body piece, a `Mask` cutout piece, a
    /// nested extra part and a `Blend`-authored effect leftover (the
    /// `FxInstance`-ancestor branch is proven by production log instead:
    /// `FxInstance`'s fields are private to `effects.rs`, so no test outside
    /// it can construct one; a demo run logged 1798 effect-rooted skips, 0
    /// mode skips). Pins: every converted body mesh carries EXACTLY ONE
    /// material component (the extended one -- leaving the original
    /// `StandardMaterial` on the entity double-draws order-dependently and
    /// can hide the dither); the leftover stays standard; disable restores
    /// the EXACT original fields and component state; re-enable reconverts;
    /// a fresh `PlayerVisual` resets coverage. Fails before the
    /// remove-`StandardMaterial` fix (converted pieces keep both).
    #[test]
    fn ecs_convert_fade_disable_reenable_reset() {
        let _env = env_lock();
        use crate::combat::Combat;
        use crate::sim::Match;

        fn types<E: bevy::ecs::component::Component>(w: &bevy::ecs::world::World, e: Entity) -> bool {
            w.get::<E>(e).is_some()
        }
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<DissolveMaterial>>()
            .init_resource::<FadeState>()
            .insert_resource(Time::<()>::default())
            .insert_resource(crate::World(Match::new(
                [Vec3::ZERO, Vec3::X * 100.0],
                Combat::new(vec![], vec![]),
            )));
        app.add_systems(Update, (prep_fade_materials, update_fade).chain());

        let mut std_mats = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let body_src = StandardMaterial {
            base_color: Color::srgb(0.8, 0.1, 0.1),
            alpha_mode: AlphaMode::Opaque,
            perceptual_roughness: 0.37,
            cull_mode: None,
            ..default()
        };
        let cut_src = StandardMaterial {
            base_color: Color::srgb(0.1, 0.8, 0.1),
            alpha_mode: AlphaMode::Mask(0.35),
            ..default()
        };
        let fx_src = StandardMaterial {
            base_color: Color::srgba(0.1, 0.1, 0.8, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        };
        let body_h = std_mats.add(body_src.clone());
        let cut_h = std_mats.add(cut_src.clone());
        let fx_h = std_mats.add(fx_src.clone());
        drop(std_mats);

        let root = app.world_mut().spawn((PlayerVisual(0), GlobalTransform::from_translation(Vec3::ZERO))).id();
        let body = app.world_mut().spawn((MeshMaterial3d(body_h.clone()), ChildOf(root))).id();
        let cut = app.world_mut().spawn((MeshMaterial3d(cut_h.clone()), ChildOf(root))).id();
        let extra = app.world_mut().spawn((MeshMaterial3d(body_h.clone()), ChildOf(body))).id();
        let leftover = app.world_mut().spawn((MeshMaterial3d(fx_h.clone()), ChildOf(root))).id();
        // View 0 far (dist 100, solid), view 1 far too.
        let cam0 = app.world_mut().spawn((PlayerCamera(0), GlobalTransform::from_translation(Vec3::new(0.0, 16.0, 100.0)))).id();
        app.world_mut().spawn((PlayerCamera(1), GlobalTransform::from_translation(Vec3::new(300.0, 16.0, 0.0))));

        app.update();
        app.update();
        {
            let w = app.world();
            for (e, name) in [(body, "body"), (cut, "cutout"), (extra, "extra")] {
                assert!(types::<FadePiece>(w, e), "{name} missing FadePiece");
                assert!(types::<MeshMaterial3d<DissolveMaterial>>(w, e), "{name} missing dissolve material");
                assert!(!types::<MeshMaterial3d<StandardMaterial>>(w, e), "{name} keeps BOTH material components (double-draw hides dither)");
                let slot = w.get::<FadePiece>(e).expect("marked").slot;
                assert_eq!(slot, 0);
            }
            // The blend-authored leftover is not bodywork: untouched.
            assert!(!types::<FadePiece>(w, leftover), "leftover gained FadePiece");
            assert!(types::<MeshMaterial3d<StandardMaterial>>(w, leftover), "leftover lost its material");
            assert!(!types::<MeshMaterial3d<DissolveMaterial>>(w, leftover), "leftover converted");
            // Authored modes survive inside the extended base (+ matte).
            let dz = w.resource::<Assets<DissolveMaterial>>();
            let hm = |e: Entity| w.get::<MeshMaterial3d<DissolveMaterial>>(e).expect("dissolve").0.clone();
            let b = dz.get(&hm(body)).expect("body asset");
            assert!(matches!(b.base.alpha_mode, AlphaMode::Opaque));
            assert_eq!(b.base.perceptual_roughness, 1.0);
            assert_eq!(b.base.reflectance, 0.0);
            assert_eq!(w.resource::<FadeState>().block.slots[0].coverage, Vec4::ONE, "every quadrant solid");
            assert_eq!(w.resource::<FadeState>().block.grain, Vec4::new(1.0, 0.0, 0.0, 0.0));
            assert_eq!(b.extension.slot.x, 0, "the piece names its fighter");
            let c = dz.get(&hm(cut)).expect("cut asset");
            assert!(matches!(c.base.alpha_mode, AlphaMode::Mask(_)));
        }

        // Fade: camera 0 to dist 5, ease ~30 frames.
        *app.world_mut().get_mut::<GlobalTransform>(cam0).expect("cam") =
            GlobalTransform::from_translation(Vec3::new(0.0, 16.0, 5.0));
        for _ in 0..30 {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
            app.update();
        }
        {
            let w = app.world();
            let live = w.resource::<FadeState>().live[0][0];
            assert!(live < 1.0, "no fade happened, live {live}");
            let cov = w.resource::<FadeState>().block.slots[0].coverage;
            assert!((cov.x - live).abs() < 1e-6, "uniform {cov} != state {live}");
            assert_eq!(cov.y, 1.0, "far view must stay solid");
        }

        // Disable: every piece back on its EXACT original handle and fields,
        // extended component + marker gone.
        std::env::set_var("PD_CAMFADE", "0");
        app.update();
        app.update();
        {
            let w = app.world();
            let std = w.resource::<Assets<StandardMaterial>>();
            for (e, h, src, name) in [(body, &body_h, &body_src, "body"), (cut, &cut_h, &cut_src, "cutout"), (extra, &body_h, &body_src, "extra")] {
                assert!(!types::<FadePiece>(w, e), "{name} keeps marker while disabled");
                assert!(!types::<MeshMaterial3d<DissolveMaterial>>(w, e), "{name} keeps extended while disabled");
                let got = w.get::<MeshMaterial3d<StandardMaterial>>(e).unwrap_or_else(|| panic!("{name} missing restored material"));
                assert_eq!(got.0, *h, "{name} restored a different handle");
                let m = std.get(&got.0).expect("restored asset");
                assert_eq!(m.base_color, src.base_color, "{name} base_color changed");
                assert_eq!(format!("{:?}", m.alpha_mode), format!("{:?}", src.alpha_mode), "{name} alpha_mode changed");
                assert_eq!(m.cull_mode, src.cull_mode, "{name} cull_mode changed");
                assert_eq!(m.perceptual_roughness, src.perceptual_roughness, "{name} roughness changed");
                assert_eq!(m.reflectance, src.reflectance, "{name} reflectance changed");
                assert_eq!(m.unlit, src.unlit, "{name} unlit changed");
            }
        }

        // Re-enable: the restored `StandardMaterial` re-fires `Added` and
        // both body pieces convert again, standard component gone again.
        std::env::remove_var("PD_CAMFADE");
        app.update();
        app.update();
        {
            let w = app.world();
            for (e, name) in [(body, "body"), (cut, "cutout")] {
                assert!(types::<FadePiece>(w, e), "{name} not reconverted");
                assert!(types::<MeshMaterial3d<DissolveMaterial>>(w, e), "{name} missing dissolve after re-enable");
                assert!(!types::<MeshMaterial3d<StandardMaterial>>(w, e), "{name} keeps both after re-enable");
            }
        }

        // Fresh match (new root entity): eased coverage resets to solid even
        // though the previous match faded. Park the camera far first so the
        // post-reset frames hold (rather than re-fade), making the reset
        // exactly observable.
        *app.world_mut().get_mut::<GlobalTransform>(cam0).expect("cam") =
            GlobalTransform::from_translation(Vec3::new(0.0, 16.0, 100.0));
        app.world_mut().despawn(root);
        let root2 = app.world_mut().spawn((PlayerVisual(0), GlobalTransform::from_translation(Vec3::ZERO))).id();
        app.world_mut().spawn((MeshMaterial3d(body_h.clone()), ChildOf(root2)));
        app.update();
        app.update();
        {
            let w = app.world();
            assert_eq!(w.resource::<FadeState>().live, [[1.0; MAX_PLAYERS]; MAX_PLAYERS], "coverage leaked across matches");
        }
        std::env::remove_var("PD_CAMFADE");
    }

    /// A one-fighter, many-camera scene: fighter 0's body piece at the origin
    /// and one `PlayerCamera` per entry of `cams` (view index, camera position,
    /// viewport origin). Returns the app and the piece.
    fn quadrant_scene(cams: &[(usize, Vec3, UVec2)]) -> (App, Entity, Vec<Entity>) {
        use crate::combat::Combat;
        use crate::sim::Match;
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<DissolveMaterial>>()
            .init_resource::<FadeState>()
            .insert_resource(Time::<()>::default())
            .insert_resource(crate::World(Match::new([Vec3::ZERO, Vec3::X * 100.0], Combat::new(vec![], vec![]))));
        app.add_systems(Update, (prep_fade_materials, update_fade).chain());
        let src = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::default());
        let root = app.world_mut().spawn((PlayerVisual(0), GlobalTransform::from_translation(Vec3::ZERO))).id();
        let piece = app.world_mut().spawn((MeshMaterial3d(src), ChildOf(root))).id();
        let ids = cams
            .iter()
            .map(|&(view, at, origin)| {
                let vp = bevy::camera::Viewport { physical_position: origin, physical_size: UVec2::new(800, 450), ..default() };
                app.world_mut().spawn((PlayerCamera(view), GlobalTransform::from_translation(at), Camera { viewport: Some(vp), ..default() })).id()
            })
            .collect();
        (app, piece, ids)
    }

    fn coverage_of(app: &App, piece: Entity) -> Vec4 {
        let w = app.world();
        let slot = w.get::<FadePiece>(piece).expect("converted").slot;
        w.resource::<FadeState>().block.slots[slot].coverage
    }

    fn run(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
            app.update();
        }
    }

    /// Four views, fighter 0 backed to a wall in view 2 only: only quadrant 2's
    /// coverage drops (the bottom-left quarter), the other three stay solid --
    /// the wrong-quarter bug the old viewport-origin heuristic would have had.
    #[test]
    fn four_views_dissolve_independently_each_in_its_own_quadrant() {
        let _env = env_lock();
        let far = |x: f32| Vec3::new(x, 16.0, 200.0);
        let (mut app, piece, _) = quadrant_scene(&[
            (0, far(0.0), UVec2::new(0, 0)),
            (1, far(10.0), UVec2::new(800, 0)),
            (2, Vec3::new(0.0, 16.0, 5.0), UVec2::new(0, 450)),
            (3, far(30.0), UVec2::new(800, 450)),
        ]);
        run(&mut app, 60);
        let cov = coverage_of(&app, piece);
        assert_eq!((cov.x, cov.y, cov.w), (1.0, 1.0, 1.0), "views 0, 1, 3 stay solid: {cov}");
        assert!(cov.z < 0.5, "view 2 (bottom-left, quadrant 2) dissolves: {cov}");
        // And each other quadrant on its own: view 3 near -> only quadrant 3.
        let (mut app, piece, _) = quadrant_scene(&[
            (0, far(0.0), UVec2::new(0, 0)),
            (1, far(10.0), UVec2::new(800, 0)),
            (2, far(20.0), UVec2::new(0, 450)),
            (3, Vec3::new(0.0, 16.0, 5.0), UVec2::new(800, 450)),
        ]);
        run(&mut app, 60);
        let cov = coverage_of(&app, piece);
        assert_eq!((cov.x, cov.y, cov.z), (1.0, 1.0, 1.0), "{cov}");
        assert!(cov.w < 0.5, "{cov}");
    }

    /// Two players switching the split layout while view 1 is faded: the same
    /// eased value moves from quadrant 1 (side by side) to quadrant 2
    /// (top/bottom) with no pop -- no frame where the value resets or the old
    /// quadrant keeps it.
    #[test]
    fn a_layout_switch_moves_view_one_between_quadrants_without_a_pop() {
        let _env = env_lock();
        let (mut app, piece, cams) = quadrant_scene(&[(0, Vec3::new(0.0, 16.0, 200.0), UVec2::new(0, 0)), (1, Vec3::new(0.0, 16.0, 5.0), UVec2::new(800, 0))]);
        run(&mut app, 60);
        let before = coverage_of(&app, piece);
        assert_eq!((before.x, before.z, before.w), (1.0, 1.0, 1.0));
        assert!(before.y < 0.5, "side by side: view 1 is quadrant 1: {before}");
        // Top/bottom: view 1's viewport now starts at (0, 450).
        let vp = bevy::camera::Viewport { physical_position: UVec2::new(0, 450), physical_size: UVec2::new(1600, 450), ..default() };
        app.world_mut().get_mut::<Camera>(cams[1]).expect("camera").viewport = Some(vp);
        run(&mut app, 1);
        let after = coverage_of(&app, piece);
        assert_eq!((after.x, after.y, after.w), (1.0, 1.0, 1.0), "the old quadrant is solid again: {after}");
        assert!((after.z - before.y).abs() < 0.02, "the same eased value, one frame later: {before} -> {after}");
    }
}
