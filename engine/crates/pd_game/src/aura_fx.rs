//! The school aura, played: effect 600 (`pdEftAura01`) through the ordinary
//! effects path, on each fighter, with the school tint, the Level tier and the
//! per-view alpha the program gives it. Rules live in `school_aura.rs`
//! (tier, slots, colours, dominant school); this file only drives them.
//!
//! Per fighter, every frame (the program does it every fighter tick,
//! `FUN_14024ec80` -> `FUN_14019e6c0`):
//! - Level is read from `Fighter::level`, forced to 0 while invisible
//!   (`aura_level`), turned into a tier, and `step_tier` says whether to stop
//!   the running effect and/or start eft0600's sub-slot 0 / 4 / 5.
//! - The effect is played looping (the program sets the loop/hold flag on the
//!   handle, `puVar8[5] = 1`), as a child of the fighter's `PlayerVisual`
//!   with an `FxHost`, so its pieces ride the bones they name (RHAND, LHAND,
//!   RTEKUB, LTEKUB, RNINOU -- `effects::bind_bones`).
//! - `effects::FxTint` on the effect root carries the colour override (the
//!   school row, `school_aura::school_rgb`) and the per-view alpha (the
//!   fighter's model opacity / 255 in that view).
//!
//! **Per-view alpha.** The program multiplies the aura's alpha by the same
//! per-view model opacity that dematerialises the body near a wall
//! (`FUN_14024f2f0` writes it, `FUN_14011b450` reads it). Here that value is
//! the wall dissolve's per-(fighter, view) coverage. `cam_fade.rs` keeps it
//! in private state, so this file recomputes it from the same pure functions
//! (`cam_fade::fade_target` / `step_live`), the same env knobs and defaults,
//! and the same eye-to-head distance; the two eased values are identical
//! frame for frame (same inputs, same step). A `cam_fade` accessor would let
//! this mirror go (see the lane report's Requests).
//!
//! Render-only: reads `Fighter::level`, `status`, `Combat::arsenals`/`skills`
//! and camera/visual transforms; never writes sim state.

use bevy::prelude::*;

use crate::cam_fade::{fade_target, step_live};
use crate::combat::Action;
use crate::effects::{self, FxHandSuppress, FxHost, FxLibrary, FxTint};
use crate::frontend::state::AppState;
use crate::school_aura::{aura_level, school_index, school_rgb, slot_for_tier, step_tier, tier_for_level, TIER3_NEEDS_RESTART};
use crate::combat::MAX_PLAYERS;
use crate::{env_f32, views, PlayerCamera, PlayerVisual, World};

/// The aura effect (`cache/fx/eft0600.*`, "オーラ", `pdEftAura01`).
pub const AURA_EFFECT: &str = "eft0600";

/// The aura wisps' hand-suppress flag (`FUN_14018a040` writes `1` to the effect instance when the
/// fighter's state `+0x12c70` is in `0xf ..= 0x1a` except `0x14` and `0x17`: damage / knocked-down
/// states). Which of our actions those are is UNCONFIRMED: Hit, Knocked and Down.
pub fn hand_suppress(action: Action) -> bool {
    matches!(action, Action::Hit | Action::Knocked | Action::Down)
}

/// Marks an effect root as a fighter's aura.
#[derive(Component)]
pub struct AuraFx;

/// Per-fighter aura bookkeeping (the program's `+0x40` handle, `+0x48` tier).
#[derive(Default, Clone)]
struct Slot {
    /// The `PlayerVisual` root this state belongs to; a new match spawns new
    /// roots, which resets everything (the old effect went with the old root).
    visual: Option<Entity>,
    /// The running effect root (`None` = no live handle).
    effect: Option<Entity>,
    /// The tier remembered from the last call (`FUN_14019e6c0`'s +0x48).
    tier: u8,
}

#[derive(Resource)]
pub struct AuraFxState {
    slots: [Slot; MAX_PLAYERS],
    /// Mirror of `cam_fade`'s eased coverage per (fighter, view).
    live: [[f32; MAX_PLAYERS]; MAX_PLAYERS],
}

impl AuraFxState {
    /// The wall dissolve's eased coverage of fighter `p` in view `v` (0.15 .. 1), the model opacity the program
    /// gives the aura's alpha and the glow's vertex grey.
    pub(crate) fn coverage(&self, p: usize, v: usize) -> f32 {
        self.live.get(p).and_then(|r| r.get(v)).copied().unwrap_or(1.0)
    }
}

impl Default for AuraFxState {
    fn default() -> Self {
        Self { slots: std::array::from_fn(|_| Slot::default()), live: [[1.0; MAX_PLAYERS]; MAX_PLAYERS] }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<AuraFxState>();
    app.add_systems(Update, update_aura_fx.run_if(in_state(AppState::Match)).before(effects::animate_pieces));
}

// -- the wall-dissolve coverage, mirrored (see the module doc) ---------------

fn fade_enabled() -> bool {
    std::env::var("PD_CAMFADE").map_or(true, |v| v != "0")
}

/// `cam_fade`'s target coverage for a camera-to-head distance, with its env knobs.
fn coverage_target(dist: f32) -> f32 {
    if !fade_enabled() {
        return 1.0;
    }
    fade_target(dist, env_f32("PD_FADE_START", 9.0), env_f32("PD_FADE_END", 4.0), env_f32("PD_FADE_MIN", 0.15))
}

/// Forced test override: `PD_AURA_ALPHA=<0..1>` multiplies every view's alpha
/// (a dissolve stand-in for screenshots that can't back a fighter into a wall).
fn alpha_override() -> Option<f32> {
    std::env::var("PD_AURA_ALPHA").ok().and_then(|v| v.parse().ok())
}

/// The fighter's Level as the aura sees it: invisible -> 0. `PD_AURA_LEVEL=<n>`
/// (test-only) forces it for both fighters, like `--start-level` but also
/// mid-match and past the start-rule's cap.
pub(crate) fn test_level() -> Option<i32> {
    std::env::var("PD_AURA_LEVEL").ok().and_then(|v| v.parse().ok())
}

#[allow(clippy::too_many_arguments)]
fn update_aura_fx(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AssetServer>,
    lib: Option<ResMut<FxLibrary>>,
    world: Res<World>,
    visuals: Query<(Entity, &PlayerVisual, &GlobalTransform)>,
    cams: Query<(&PlayerCamera, &GlobalTransform, Option<&Camera>)>,
    auras: Query<(), With<AuraFx>>,
    mut state: ResMut<AuraFxState>,
    mut tints: Query<&mut FxTint>,
) {
    let Some(mut lib) = lib else { return };
    let combat = &world.0.combat;
    let dt = time.delta_secs();
    let mut campos: [Option<Vec3>; MAX_PLAYERS] = [None; MAX_PLAYERS];
    // The screen quadrant each view renders in (`fx_gamma.wgsl` picks the alpha by it).
    let mut quad: [Option<usize>; MAX_PLAYERS] = [None; MAX_PLAYERS];
    for (c, gt, cam) in &cams {
        if c.0 < MAX_PLAYERS {
            campos[c.0] = Some(gt.translation());
            quad[c.0] = Some(views::quadrant_of(cam, c.0));
        }
    }
    for p in 0..MAX_PLAYERS {
        let vis = visuals.iter().find(|(_, v, _)| v.0 == p);
        let Some((root, _, gt)) = vis else {
            state.slots[p] = Slot::default();
            state.live[p] = [1.0; MAX_PLAYERS];
            continue;
        };
        if state.slots[p].visual != Some(root) {
            // New match / new model: the old effect (a child of the old root) is gone.
            state.slots[p] = Slot { visual: Some(root), ..Slot::default() };
            state.live[p] = [1.0; MAX_PLAYERS];
        }
        // The effect went away by itself (despawned with a rebuilt root): no handle.
        if state.slots[p].effect.is_some_and(|e| !auras.contains(e)) {
            state.slots[p].effect = None;
        }
        // The wall dissolve's per-view coverage, eased exactly as `cam_fade` does.
        let body = gt.translation();
        for v in 0..MAX_PLAYERS {
            let dist = campos[v].map_or(f32::INFINITY, |c| c.distance(body + Vec3::Y * 16.0));
            state.live[p][v] = step_live(state.live[p][v], coverage_target(dist), dt);
        }
        let Some(fighter) = combat.fighters.get(p) else { continue };
        let level = test_level().unwrap_or(fighter.level);
        let tier = tier_for_level(aura_level(level, fighter.status.invisible_ticks > 0));
        let running = state.slots[p].effect.is_some();
        let step = step_tier(state.slots[p].tier, running, tier, TIER3_NEEDS_RESTART);
        if step.stop {
            if let Some(e) = state.slots[p].effect.take() {
                commands.entity(e).despawn();
            }
        }
        if let Some(slot) = step.start {
            debug_assert_eq!(Some(slot), slot_for_tier(tier));
            if let Some(e) = effects::play(&mut commands, &assets, &mut lib, AURA_EFFECT, slot, Transform::IDENTITY, true, None) {
                let rgb = Vec3::from(school_rgb(school_index(&combat.arsenals[p], &combat.skills)));
                commands.entity(e).insert((ChildOf(root), FxHost(root), AuraFx, FxTint { rgb, view_alpha: [1.0; 4] }));
                state.slots[p].effect = Some(e);
                // The program writes the hand-suppress flag once, when the effect instance is made
                // (`FUN_14018a040`), from the fighter's state (module doc of `effects/aura_wisp.rs`).
                if hand_suppress(fighter.action) {
                    commands.entity(e).insert(FxHandSuppress);
                }
                if std::env::var_os("PD_AURA_LOG").is_some() {
                    eprintln!("[aura] tick {} p{p} start {AURA_EFFECT} slot {slot} (Level {level}, tier {tier}) rgb {rgb:?}", world.0.tick);
                }
            }
        }
        if step.stop && std::env::var_os("PD_AURA_LOG").is_some() {
            eprintln!("[aura] tick {} p{p} stop (Level {level}, tier {tier})", world.0.tick);
        }
        state.slots[p].tier = tier;
        // Tint + alpha every frame (the program re-registers the override each tick, `FUN_14019e5d0`).
        if let Some(e) = state.slots[p].effect {
            if let Ok(mut t) = tints.get_mut(e) {
                let rgb = Vec3::from(school_rgb(school_index(&combat.arsenals[p], &combat.skills)));
                let k = alpha_override().unwrap_or(1.0);
                // Each view's coverage goes in the quadrant that view renders in; the rest stay solid.
                let mut view_alpha = [1.0f32; 4];
                for v in 0..MAX_PLAYERS {
                    if let Some(q) = quad[v] {
                        view_alpha[q] = state.live[p][v] * k;
                    }
                }
                let want = FxTint { rgb, view_alpha };
                if *t != want {
                    *t = want;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The aura's alpha is the wall dissolve's coverage: solid at rest, at the
    /// floor (0.15) against a wall, linear between -- the same function.
    #[test]
    fn alpha_follows_the_wall_dissolve_coverage() {
        assert_eq!(fade_target(60.0, 9.0, 4.0, 0.15), 1.0);
        assert_eq!(coverage_target(60.0), 1.0, "far: untouched");
        assert!((coverage_target(2.0) - 0.15).abs() < 1e-6, "at the wall: the floor");
        assert!((coverage_target(6.5) - 0.575).abs() < 1e-6, "midway: midway");
        assert_eq!(coverage_target(6.5), fade_target(6.5, 9.0, 4.0, 0.15));
    }

    /// Eased exactly like `cam_fade::update_fade`: a few steps toward the floor
    /// then snap home to exactly 1.0 (an aura stuck at 0.99 would be a leak).
    #[test]
    fn mirror_eases_and_snaps_home_like_the_dissolve() {
        let mut live = 1.0;
        for _ in 0..600 {
            live = step_live(live, coverage_target(2.0), 0.016);
        }
        assert!((live - 0.15).abs() <= 0.01, "settled at the floor, got {live}");
        for _ in 0..600 {
            live = step_live(live, coverage_target(60.0), 0.016);
        }
        assert_eq!(live, 1.0, "full recovery is exact");
    }

    #[test]
    fn a_tint_carries_colour_and_per_view_alpha() {
        let t = FxTint { rgb: Vec3::from(school_rgb(1)), view_alpha: [1.0, 0.15, 1.0, 1.0] };
        assert_eq!(t.rgb.to_array(), school_rgb(1));
        assert_eq!(t.view_alpha, [1.0, 0.15, 1.0, 1.0]);
    }
}
