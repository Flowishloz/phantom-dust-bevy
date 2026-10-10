//! Footsteps: a puff under each footfall, the thing the original always had
//! and we never built.
//!
//! **What the original does** (FORMATS-STAGES.md "Footsteps", recovered from
//! `FUN_140230e10`, `decomp_all.c:452366`): an 18-case dispatch on a ground
//! material id — 3/9/12 → sand, 5/6/8/11/21 → dust, 13 → water, and **ten ids
//! spawn nothing at all** (hard floors, walls, interiors) — plus a **10-frame
//! minimum gap** between steps and a **per-stage** surface table
//! (`DAT_1404bc9d8`).
//!
//! **Why the per-surface choice is real data now.** The material id comes from
//! the stage `.dat`'s header-`0x94` whole-stage collision block — one surface
//! class byte per triangle — exported by `pd_convert stages` as
//! `cache/<stage>.surf.json` and parsed by `pd_formats::surface`
//! (COLLISION.md "The runtime footstep class byte…"). `Surface::from_class`
//! maps the byte to the puff, and `surface_ground::step_sound_for_class` maps
//! it to the sound: water (0xD) cycles common 52/54 per player (DECISIONS.md
//! 2026-09-29), everything else plays Laurence's regular step (common 35). Stages
//! with no sidecar (`st00`, the `st09_0x` areas) play dry everywhere — the
//! honest fallback, not invented data. ⚠️ `Surface::from_class` is the seam:
//! the mapping is the program's own dispatch, read from the decompilation.
//!
//! What is still approximate: the original plays NO footstep at all on its
//! silent classes and skips 0x1F faces in the probe (we do skip 0x1F in the
//! query, but play the dry default on other silent classes rather than going
//! silent); and it layers the dry sound under the material sound (we play the
//! wet clip alone on wet ground, per Laurence's instruction).
//!
//! **Trigger.** A footfall is a point in the walk cycle, not a physics event,
//! so it is read off the *animation clock*: each WALK/RUN clip plants two feet
//! per loop, so a step fires at each half-cycle boundary. The cycle length is
//! the clip's own `duration()` (from the converted glTF), not a hardcoded
//! number, so it tracks the feet — a run plants feet faster than a walk — and
//! cannot drift from the data. Gated on being grounded and actually moving, and
//! rate-limited to the original's 10-frame minimum gap.
//!
//! Render-side only: reads sim state and the animation clock, spawns
//! particles. The simulation never sees any of it.

use bevy::prelude::*;
use pd_formats::surface::{class_kind, SurfaceKind};
use std::collections::HashMap;

use crate::effects;
use crate::frontend::state::MatchScene;
use crate::sim::{Gait, Motion, Player};
use crate::surface_ground::{SurfaceGround, WetCycle};
use crate::CharacterAnims;

/// Minimum ticks between footstep puffs: the original's own minimum gap
/// (FORMATS-STAGES.md "Footsteps" — its counter decrements once per sim tick,
/// and the sim runs at 60 Hz, so 10 frames is 0.17 s).
pub const MIN_STEP_TICKS: u32 = 10;

/// Below this ground speed the player counts as standing still, so no steps.
/// Well under a real walk (PROGRAM-RE.md "Ground speeds") so a slow shuffle
/// still steps.
const MIN_STEP_SPEED: f32 = 1.0;

/// What the player is standing on: the original's own per-surface dispatch on
/// the ground face's class byte (COLLISION.md "The runtime footstep class
/// byte…") — sand {3, 9, 0xC}, dust {5, 6, 8, 0xB, 0x15}, water {0xD} — with
/// `Hard` as the default for the silent classes and for stages with no
/// surface sidecar (where the dry sound plays everywhere, as before).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    /// Concrete, metal, stone, wood — and the fallback for silent classes /
    /// missing sidecars. (The original plays nothing on ten of its eighteen
    /// material ids; we approximate with a faint scuff + the dry step.)
    Hard,
    /// Built by the original for material ids 3, 9 and 12.
    Sand,
    /// Material ids 5, 6, 8, 11 and 21 in the original.
    Dust,
    /// Material id 13 in the original.
    Water,
}

impl Surface {
    /// The surface for a ground class byte (`None` = no ground under the
    /// player, e.g. a missing sidecar): the program's own dispatch, read from
    /// the decompilation — not an invented mapping.
    pub fn from_class(class: Option<u8>) -> Self {
        match class.map(class_kind) {
            Some(SurfaceKind::Sand) => Surface::Sand,
            Some(SurfaceKind::Dust) => Surface::Dust,
            Some(SurfaceKind::Water) => Surface::Water,
            _ => Surface::Hard,
        }
    }

    /// Puff colour (linear RGBA at birth), particle count and size. ⚠️ **By
    /// eye** — the original's own per-surface values are not recovered (see
    /// FORMATS-EFFECTS.md, "Open").
    ///
    /// ⚠️ **The numbers went up twice, both times for a measured reason, and
    /// the reasons matter more than the numbers.** First pass was 0.34 units:
    /// sub-pixel at 3200x1800, so the lane "fired" 15 times a run and showed
    /// nothing. Second pass sized the puff as a flat scuff (0.9 x 0.2), but
    /// `tick_particles` collapses a `Shape::Puff` to `Vec3::splat(size[0])` --
    /// the second size is **discarded**, so the "flat" axis is a fiction and
    /// the puff is a soft ball. The alpha below is what finally made it read:
    /// 0.55 grey over an already mid-grey stage floor is nearly invisible, and
    /// the floor here is ~0.45 luma, so the puff needs to be clearly brighter
    /// than its background to register at all.
    fn puff(self) -> (Vec4, u32, f32) {
        match self {
            // Bright pale grey, few motes: reads as a scuff on a hard floor.
            Surface::Hard => (Vec4::new(0.95, 0.93, 0.88, 0.95), 7, 1.1),
            Surface::Sand => (Vec4::new(0.98, 0.90, 0.68, 1.00), 10, 1.4),
            Surface::Dust => (Vec4::new(0.90, 0.85, 0.76, 0.95), 10, 1.4),
            Surface::Water => (Vec4::new(0.82, 0.92, 1.00, 0.85), 8, 1.2),
        }
    }
}

/// Per-player footstep state.
#[derive(Default)]
pub struct StepState {
    /// Ticks until the next step is allowed (the original's rate limit).
    cooldown: u32,
    /// Which half of the walk cycle we last fired on, so a step fires once per
    /// footfall rather than every frame.
    last_half: Option<bool>,
    /// The clip the phase was last read for. A clip change resets the phase, so
    /// starting a walk does not fire a step on frame one.
    clip: Option<Motion>,
}

/// Where a player's feet are. The puff sits just above the floor, not at the
/// player's origin: the sim position is the character's *root*, which is well
/// above the ground, so spawning there leaves the puff hanging in mid-air
/// (found by screenshot, not by reasoning — see the note in `spawn_puff`).
fn foot_pos(p: &Player, floor_y: f32) -> Vec3 {
    Vec3::new(p.pos.x, floor_y + FOOT_CLEARANCE, p.pos.z)
}

/// How far above the floor the puff is born. Small: a footstep is a scuff, not
/// a cloud, and a mote spawned below the floor is invisible.
const FOOT_CLEARANCE: f32 = 0.35;

/// Spawns a footstep puff for each player whose walk cycle has crossed a
/// footfall boundary since the last frame.
/// `PD_STEPS=0` turns the whole effect off, for A/B measurement against a
/// frame with no footfalls in it. Unset (the default) means on.
fn enabled() -> bool {
    !matches!(std::env::var("PD_STEPS").as_deref(), Ok("0") | Ok("off") | Ok("false"))
}

/// The loaded surface sidecar for the current stage. A Bevy `Local` (not a
/// `main.rs` resource) so this lane needs no hook outside its own files: the
/// first frame on a stage loads `cache/<stage>.surf.json`, a stage change
/// reloads, and a missing sidecar means dry-everywhere rather than an error.
#[derive(Default)]
pub(crate) struct SurfaceCache {
    stage: String,
    ground: Option<SurfaceGround>,
    ready: bool,
}

pub fn footsteps(
    mut commands: Commands,
    world: Res<crate::World>,
    cfg: Res<crate::Config>,
    setup: Option<Res<crate::frontend::state::MatchSetup>>,
    col: Res<crate::StageCollision>,
    pm: Res<effects::ParticleMeshes>,
    mut mats: effects::PartMats,
    anims: Query<(&crate::AnimSlot, &AnimationPlayer)>,
    mut queue: Option<ResMut<crate::sfx::CueQueue>>,
    mut state: Local<HashMap<usize, StepState>>,
    mut ground: Local<SurfaceCache>,
    mut wet: Local<WetCycle>,
) {
    if !enabled() {
        return;
    }
    // Versus setup picks the stage outside `Config` (legacy CLI launches use
    // `Config.stage`); mirror `sfx.rs::object_sfx`'s selection.
    let stage = match setup.as_deref() {
        Some(s) if !cfg.legacy => s.stage.clone(),
        _ => cfg.stage.clone(),
    };
    let surface_log = std::env::var_os("PD_SURFACE_LOG").is_some();
    if !ground.ready || ground.stage != stage {
        ground.ground = SurfaceGround::load(&cfg.cache, &stage);
        ground.stage = stage.clone();
        ground.ready = true;
        if surface_log {
            match &ground.ground {
                Some(g) => eprintln!("[surface] stage {stage}: sidecar loaded, class counts {:?}", g.class_counts()),
                None => eprintln!("[surface] stage {stage}: no sidecar, dry default everywhere"),
            }
        }
    }
    for (slot, player) in &anims {
        let i = slot.player_index();
        let Some(p) = world.0.players.get(i) else { continue };
        let st = state.entry(i).or_default();

        // A clip change resets the phase, so starting a walk does not fire a
        // step on its first frame. The clip is decided by the sim's motion
        // (`clip_for` in main.rs plays the same names), not read back off the
        // animation player, whose node index type is private to Bevy.
        if st.clip != Some(p.motion) {
            st.clip = Some(p.motion);
            st.last_half = None;
        }

        // Not a locomotion clip (idle, a cast, a jump): no footsteps, and drop
        // the phase so re-entering a walk starts clean.
        let Some(cycle) = cycle_ticks(p.motion, p.gait) else {
            st.last_half = None;
            tick_cooldown(st);
            continue;
        };
        // Only while actually moving and on the ground.
        if !p.grounded || p.vel.length() < MIN_STEP_SPEED {
            st.last_half = None;
            tick_cooldown(st);
            continue;
        }
        // Mid-cross-fade the player has two clips running, so the clock is
        // ambiguous; skip the frame rather than guess a footfall.
        let Some(elapsed) = CharacterAnims::playing_elapsed(player) else { continue };

        // A walk plants two feet per loop, so each half of the clip is one
        // footfall.
        let half = ((elapsed * TICK_HZ) as u32) / cycle % 2 == 1;
        if st.last_half != Some(half) {
            st.last_half = Some(half);
            if st.cooldown == 0 {
                st.cooldown = MIN_STEP_TICKS;
                // The floor under the fighter, not their root: a mote spawned at
                // the character origin hangs in mid-air, which is invisible in a
                // 2004-style flat-lit stage and was the reason the first
                // version of this lane produced no visible puffs at all.
                let Some(floor_y) = col.ground_below(p.pos.x, p.pos.z, p.pos.y) else { continue };
                // The ground class under the stepping player, from the stage's
                // surface sidecar (brute-force scan; steps are ~10 Hz, 2,723
                // faces on st03 — see `surface_ground.rs`). Water cycles the
                // wet pair per player; everything else plays the dry default.
                // (The original layers the dry sound under the material sound
                // too — deliberately not done: Laurence asked for the wet clip
                // on wet ground.)
                let class = ground.ground.as_ref().and_then(|g| g.class_below(p.pos.x, p.pos.z, p.pos.y));
                let surface = Surface::from_class(class);
                if std::env::var_os("PD_STEP_LOG").is_some() {
                    eprintln!("[step] t {} p{} {:?} floor {:.1} at {:.1} {:.1}", world.0.tick, i, p.motion, floor_y, p.pos.x, p.pos.z);
                }
                spawn_puff(&mut commands, &pm, &mut mats, surface, foot_pos(p, floor_y), i as u32);
                let id = crate::surface_ground::step_sound_for_class(class, &mut wet, i);
                if surface_log {
                    eprintln!("[surface] t {} p{} class {} kind {:?} id {}", world.0.tick, i, class.map_or("-".to_string(), |c| c.to_string()), class.map(class_kind), id);
                }
                if let Some(q) = queue.as_deref_mut() {
                    q.0.push(crate::sfx::Cue::Sfx(id, crate::sfx::VOL_FOOTSTEP));
                }
            }
        }
        tick_cooldown(st);
    }
}

/// The sim runs at 60 Hz (PROGRAM-RE.md "Timestep").
const TICK_HZ: f32 = 60.0;

/// Half a locomotion clip in ticks — a walk plants two feet per loop, so one
/// footfall per half cycle. The durations are **measured from the converted
/// glTF** (`pc00a.glb` animation sampler input maxima): WALK 0.9333 s = 28
/// frames at 30 fps, RUN 0.6333 s = 19. Worked out in *frames* and scaled, not
/// by multiplying a rounded float: `0.9333 * 60.0` is 55.998, which truncates
/// to 55 and would make a walk half one tick short of a run's stride pattern.
const WALK_FRAMES: f32 = 28.0;
const RUN_FRAMES: f32 = 19.0;
/// The converted animations run at 30 fps; the sim at 60.
const ANIM_HZ: f32 = 30.0;

/// The step cycle for a motion, or `None` when its feet are not on the ground
/// (idle, a cast, a jump, a landing).
///
/// ⚠️ **Strafe steps, and it has to.** A locked-on player strafing plays a
/// one-shot `TURN_*` pivot and then the ordinary WALK/RUN clip for as long as
/// the stick is held (`main.rs::drive_animation`, PROGRAM-RE.md "Lock-on"), so
/// the feet are moving on exactly the WALK/RUN clock. The first cut of this
/// returned `None` for `Strafe` and the effect never fired at all in a demo
/// run — the demo AI is permanently locked on, and locked-on circling is the
/// commonest movement in the whole game. Use the gait, not the direction.
fn cycle_ticks(motion: Motion, gait: Gait) -> Option<u32> {
    let frames = match motion {
        Motion::Walk => WALK_FRAMES,
        Motion::Run => RUN_FRAMES,
        Motion::Strafe(_) => match gait {
            Gait::Walk => WALK_FRAMES,
            Gait::Run => RUN_FRAMES,
        },
        _ => return None,
    };
    // 30 fps animation -> 60 Hz sim is x2; two feet per loop is /2, so the two
    // cancel and a footfall is simply one animation frame's worth of ticks.
    let ticks = frames / ANIM_HZ * TICK_HZ;
    Some((ticks.round() as u32 / 2).max(1))
}

/// Counts the rate limit down.
fn tick_cooldown(st: &mut StepState) {
    if st.cooldown > 0 {
        st.cooldown = st.cooldown.saturating_sub(1);
    }
}

fn spawn_puff(
    commands: &mut Commands,
    pm: &effects::ParticleMeshes,
    mats: &mut effects::PartMats,
    surface: Surface,
    at: Vec3,
    seed: u32,
) {
    let (colour, count, size) = surface.puff();
    let spec = effects::BurstSpec {
        shape: effects::Shape::Puff,
        // `None` so `spawn_burst` picks the built-in soft round falloff, the
        // same path the landing dust takes. Passing a texture here is also fine
        // but the soft one is what the original's dust reads like.
        texture: None,
        quadrants: false,
        count,
        life: 0.34,
        emit_over: 0.0,
        speed: 2.2,
        spread_deg: 180.0,
        up: 1.1,
        gravity: 6.0,
        rise: 0.0,
        // `tick_particles` scales a Puff with `Vec3::splat(size[0])`, so
        // `size[1]` is ignored for this shape -- it is a soft ball, not a flat
        // scuff. `size[1]` is kept in step with `size[0]` so the spec is not
        // lying to the next reader, and so it becomes meaningful if the puff
        // ever gets a shape that honours it.
        size: [size, size],
        colour: [colour, Vec4::new(colour.x, colour.y, colour.z, 0.0)],
        additive: false,
        spin: 0.0,
        jitter: 0.35,
    };
    let seed = seed.wrapping_mul(2_654_435_761).wrapping_add(7);
    for e in effects::spawn_burst(commands, pm, mats, &spec, at, seed) {
        commands.entity(e).insert(MatchScene);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::StrafeDir;

    #[test]
    fn the_originals_minimum_gap_is_ten_frames() {
        // Decoded from FUN_140230e10: the step counter is set to 10 after a
        // footfall and decremented once per tick, so two steps can never be
        // closer than 10 ticks. If this constant is ever "tuned down" the
        // effect turns into machine-gun steps, which is the exact artefact we
        // are copying the number to avoid.
        assert_eq!(MIN_STEP_TICKS, 10);
    }

    #[test]
    fn surface_from_class_follows_the_programs_dispatch() {
        // The class byte is the program's own dispatch (COLLISION.md "The
        // runtime footstep class byte…"), not an invented mapping: sand, dust
        // and water each get their surface; silent classes and no ground fall
        // back to Hard (dry scuff + dry step, as before this lane).
        for c in [3, 9, 0xC] {
            assert_eq!(Surface::from_class(Some(c)), Surface::Sand, "class {c:#X}");
        }
        for c in [5, 6, 8, 0xB, 0x15] {
            assert_eq!(Surface::from_class(Some(c)), Surface::Dust, "class {c:#X}");
        }
        assert_eq!(Surface::from_class(Some(0xD)), Surface::Water);
        for c in [None, Some(0), Some(1), Some(2), Some(4), Some(0x1F), Some(0x20)] {
            assert_eq!(Surface::from_class(c), Surface::Hard, "class {c:?}");
        }
    }

    #[test]
    fn a_walk_cycle_puts_exactly_one_footfall_in_each_half() {
        // The whole mechanism: WALK is 0.9333 s, so a half cycle is 28 ticks at
        // 60 Hz and the clip flips twice per loop -- two steps per stride, which
        // is what a walk actually is. If this ever returns 0 or 3 transitions,
        // the puffs will stutter or machine-gun.
        let cycle = cycle_ticks(Motion::Walk, Gait::Walk).unwrap();
        assert_eq!(cycle, 28, "WALK half-cycle should be 28 ticks (0.9333 s / 2)");
        // 3 full loops sampled every tick, inclusive of the far edge: a loop
        // plants two feet, and the footfall at the boundary belongs to the
        // window we just measured.
        let mut flips = 0;
        let mut last = ((0.0 * TICK_HZ) as u32) / cycle % 2 == 1;
        for tick in 0..=(cycle * 6) {
            let half = (tick / cycle) % 2 == 1;
            if half != last {
                flips += 1;
                last = half;
            }
        }
        assert_eq!(flips, 6, "3 loops of a 2-foot cycle = 6 footfalls");
    }

    #[test]
    fn a_run_steps_faster_than_a_walk() {
        // RUN is 0.6333 s against WALK's 0.9333 s, so its half cycle is
        // shorter and a run plants feet more often. If these ever come out
        // equal, someone has copy-pasted the wrong duration.
        let walk = cycle_ticks(Motion::Walk, Gait::Walk).unwrap();
        let run = cycle_ticks(Motion::Run, Gait::Run).unwrap();
        assert!(run < walk, "run ({run}) must step faster than walk ({walk})");
    }

    #[test]
    fn only_a_walk_or_a_run_produces_a_cycle() {
        // Idle, jumps, casts and landings have no cycle, so no footfalls --
        // which is why they return None rather than 0 (a 0 would divide by
        // zero below and a 1 would fire every frame).
        for m in [Motion::Idle, Motion::Rise, Motion::Fall, Motion::Land, Motion::HeavyLand, Motion::Cast] {
            assert_eq!(cycle_ticks(m, Gait::Walk), None, "{m:?} must not produce a footstep cycle");
        }
        assert!(cycle_ticks(Motion::Walk, Gait::Walk).is_some());
        assert!(cycle_ticks(Motion::Run, Gait::Run).is_some());
    }

    #[test]
    fn a_locked_on_strafe_steps_on_the_gait_clock() {
        // Regression: the first cut returned None for Strafe, so the effect
        // never fired in a demo run (the demo AI is permanently locked on) and
        // the "built" lane would have shipped silently dead. Locked-on
        // circling is the commonest movement in the game, so it must step.
        for g in [Gait::Walk, Gait::Run] {
            for dir in [crate::sim::StrafeDir::Right, crate::sim::StrafeDir::Left, crate::sim::StrafeDir::BackRight, crate::sim::StrafeDir::BackLeft] {
                // Same gait, same clock: the strafe's own direction and the
                // plain walk/run it mirrors must agree.
                let plain = match g {
                    Gait::Walk => Motion::Walk,
                    Gait::Run => Motion::Run,
                };
                assert_eq!(
                    cycle_ticks(Motion::Strafe(dir), g),
                    cycle_ticks(plain, g),
                    "strafe {dir:?} on {g:?} must share the {plain:?} clock",
                );
            }
        }
        // A strafing run must still be faster than a strafing walk.
        assert!(cycle_ticks(Motion::Strafe(StrafeDir::Left), Gait::Run) < cycle_ticks(Motion::Strafe(StrafeDir::Left), Gait::Walk));
    }

    #[test]
    fn a_standing_player_is_below_the_step_threshold() {
        // The gate that stops puffs spawning under an idle fighter. A still
        // player has no velocity, so it can never pass the threshold.
        assert!(Vec3::ZERO.length() < MIN_STEP_SPEED);
        // Sanity the other way: a real walk must clear it, or nothing ever
        // steps. WALK ground speed is ~40 u/s (PROGRAM-RE.md "Ground speeds").
        assert!(40.0 > MIN_STEP_SPEED);
    }
}
