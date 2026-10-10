//! pd_game - two-player split-screen prototype on converted Phantom Dust data.
//!
//!   pd_game <cache dir> [--stage st03] [--p1 pc00a] [--p2 pc01a]
//!           [--demo] [--screenshot out.png [--shot-frame N]]
//!
//! Controls (first gamepad = P1, second = P2):
//!   P1 keyboard: WASD move, Space jump, Y lock-on toggle, Q/E turn camera,
//!     skills J K U I, arrows (= D-pad) show a slot's skill info
//!   P2 keyboard: Enter jump, Right Shift lock-on toggle, skills numpad 1 2 4 5 (moves with a pad only)
//!   Gamepad (the original's layout): left stick move, click it to jump; skills / capture
//!     X B Y A (dial left, right, top, bottom); D-pad shows that slot's skill info; right trigger lock-on;
//!     right stick turns the camera (not while locked on), click it to recentre; Start menu
//! Movement is relative to your lock-on to the other player, like the original.

mod anim_args;
mod body;
mod arsenal;
mod bindings;
mod cam_fade;
mod cam_occluders;
mod capsule_birth;
mod char_light;
mod combat;
mod cpu;
mod display;
mod dangle;
mod devtools;
mod effects;
pub(crate) use effects::FxInstance;
mod footsteps;
mod frontend;
mod fx;
mod glow;
mod hud;
mod hud_card;
mod hud_dial;
mod menu;
mod mixer;
mod music;
mod moves;
mod net;
mod online_match;
mod orb_material;
mod props;
mod props_view;
mod prophunt;
mod prophunt_view;
mod aura_fx;
mod aura_glow;
mod aura_pulse;
mod school_aura;
mod rumble;
mod seats;
mod sfx;
mod shadows;
mod sim;
mod sky_scroll;
mod stage_look;
mod stage_material;
mod stairs_under;
mod trace;
mod ground_sfx;
mod surface_ground;
#[cfg(test)]
mod stair_walk;
mod hitstop;
mod look;
mod perf_probe;
mod views;

use bevy::camera::Viewport;
use bevy::prelude::*;

/// Character lighting (lux / cd per m2), tuned against the game's look.
const LIGHT_SUN: f32 = 3000.0;
const LIGHT_AMBIENT: f32 = 500.0;
/// Top/bottom split: how far the look-at point sits BELOW the usual aim
/// height (so the character rides lower in the short strip, feet just above
/// the bottom edge -- Laurence 2026-09-29: it was sitting equidistant,
/// head and feet split the frame evenly, which reads as neither a ground
/// view nor a character view).
///
/// The sign is deliberate and easy to get backwards: dropping the look-at
/// point makes the camera aim DOWN, which pushes the character UP the frame.
/// The old value moved the eye down 14u AND the look-at down 6u, which aimed
/// the camera down twice over and landed the character in the middle. The
/// eye now stays at its normal height for top/bottom; only the look-at
/// moves, so the framing is changed by exactly one control.
///
/// Override for proof/tuning: `PD_TB_LOOK_DROP` (world units, positive =
/// aim lower = character higher in frame). Swept 2/4/6/8 against a 3200x1800
/// top/bottom capture on st03: the response is monotonic, so this is a
/// one-knob dial and the shipped value is the look Laurence picked from the
/// sweep sheet, not a computed number.
const TB_LOOK_DROP: f32 = 4.0;

/// The original's battle camera (GAME.md "LIVE battle camera", 2026-10-02): **LIVE**.
/// Free: 55.0 behind, 13.0 above the fighter, pitch 0. Locked on: 52.0 behind,
/// 31.0 above (pitch -19.1 comes out of the geometry to the foe). Each split
/// half is 16:9 with hFOV 75.0 / vFOV 46.7 degrees (the projection's x scale
/// 1.3032 = cot 37.5 deg). The FOV setting and its per-view rule are
/// `menu::FOV_DEFAULT` / `views::fov_for`.
/// `PD_CAM_OLD=1` restores the earlier boom positions (60 / 30, aim 14) for A/B; the FOV rule is
/// the same either way (the old vertical-FOV meaning of the setting is gone).
const ORIG_DIST_FREE: f32 = 55.0;
const ORIG_UP_FREE: f32 = 13.0;
const ORIG_DIST_LOCK: f32 = 52.0;
const ORIG_UP_LOCK: f32 = 31.0;
const ORIG_LOCK_PITCH_DEG: f32 = 19.1;

fn cam_lock_live() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("PD_CAM_LOCK_LIVE").map(|v| v == "1").unwrap_or(false))
}

fn cam_old() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PD_CAM_OLD").is_some())
}

/// Wall pull-in of the boom: kept (default). READ: the only raycast in the program
/// (`FUN_140256390`) has footstep callers only, so no camera ray-cast was found; but
/// the film takes 1, 60 and 80 show the original's camera coming in near walls, and
/// the LIVE camera run never reached a wall. So the original has *some* pull-in whose
/// rule is UNCONFIRMED; ours (3 u off the wall, 4 u minimum) stays until it is read.
/// `PD_CAM_NOPULLIN=1` turns it off for A/B.
fn cam_pullin() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PD_CAM_NOPULLIN").is_none())
}

/// Tuning override from an environment variable (e.g. PD_SUN=3000).
pub(crate) fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::WindowResized;
use frontend::state::{AppState, GameData, MatchScene, MatchSetup, MatchStats, SavedArsenals, SceneReady, SkillCatalog};
use pd_formats::collision::Collision;
use anim_args::is_turn_clip;
use sim::{Gait, Input, Match, Motion, StrafeDir};
use std::path::PathBuf;
use std::time::Duration;

/// Every clip the game plays (looked up by name in each character's glTF).

/// The 30-skill test arsenal both players carry (Laurence: same set for
/// both): aura particles, Psycho Wave, Laser, Crystal Wall (a barrier).
const TEST_ARSENAL: [(i16, usize); 4] = [(combat::AURA_PARTICLE, 10), (1, 8), (65, 8), (26, 4)];
/// `--arsenal mixed` (tools/bats/PLAY_ALL_SKILLS.bat): 10 aura particles + one of each
/// family, so a match shows every kind of skill. Ids from skills.json.
const MIXED_ARSENAL: [i16; 20] = [
    1,   // Psycho Wave (short-range)
    65,  // Laser ([Arc])
    80,  // Blaster (shell)
    10,  // Bomb ([Parabola])
    200, // Ki Bullet (knocks down)
    147, // Tremor ([Hole])
    20,  // Rain of Stones ([Rain])
    23,  // Timer Mine
    63,  // Photon Wave
    138, // Root of Tree ([Crawl])
    19,  // Meteor ([Fall])
    26,  // Crystal Wall ([Barrier])
    30,  // Psycho Shell ([Shelter])
    92,  // Reflect Barrier
    156, // Gust ([Course])
    216, // Tiger Slayer (erase)
    49,  // Speed Up (status)
    179, // Healing Water (special)
    54,  // Dazzle (special)
    286, // Lunar Force (environment)
];

/// The test set as a full 30-skill deck (also offered as a built-in in the
/// versus setup screen).
fn test_arsenal() -> Vec<i16> {
    TEST_ARSENAL.iter().flat_map(|&(id, n)| std::iter::repeat_n(id, n)).collect()
}

/// `--arsenal mixed`'s deck.
fn mixed_arsenal() -> Vec<i16> {
    std::iter::repeat_n(combat::AURA_PARTICLE, 10).chain(MIXED_ARSENAL).collect()
}

/// `--arsenal school:<skill id>`: a calibration-only 30-card deck, entirely
/// one skill (one school, no possible tie) -- for `school_aura.rs`'s
/// per-school proof sheet, not a real deck.
fn single_school_arsenal(id: i16) -> Vec<i16> {
    std::iter::repeat_n(combat::AURA_PARTICLE, 10).chain(std::iter::repeat_n(id, 20)).collect()
}

/// Each player's dodge move from `cache/chars.json` (`pd_convert chars`);
/// Jump when the file or the character is missing. And each player's body (height and radius,
/// `body.rs`: the charaset record of the character, the default fighter's when the model is unknown).
fn set_dodges(m: &mut Match, cache: &std::path::Path, chars: &[String]) {
    let json: serde_json::Value = std::fs::read_to_string(cache.join("chars.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    for (p, id) in m.players.iter_mut().zip(chars) {
        let entry = json["chars"].as_array().and_then(|l| l.iter().find(|c| c["id"] == id.as_str()));
        p.dodge = sim::Dodge::from_name(entry.and_then(|c| c["dodge"].as_str()).unwrap_or("jump"));
        p.body = body::Body::of(id).unwrap_or(body::Body::DEFAULT);
    }
}

/// The arena a match starts on unless one is picked (Laurence: Lost Palace).
pub const DEFAULT_STAGE: &str = "st03";

/// JUMP_UP / JUMP_DOWN clip lengths (`pc00a.glb`, 30 fps), used only to
/// time-stretch them to the jump's real airtime (below) -- a per-character
/// approximation, like `sim::LAND_TIME`/`HEAVY_LAND_TIME` already are.
const JUMP_UP_CLIP_SECONDS: f32 = 25.0 / 30.0;
const JUMP_DOWN_CLIP_SECONDS: f32 = 10.0 / 30.0;

/// Locked-on strafe clip: TURN_WALK_*/TURN_RUN_* by direction and gait
/// (`anim.rs` names; PROGRAM-RE.md "Movement while locked"). Played without
/// root motion in the original -- our sim already drives position, so no
/// extra handling is needed here.
fn strafe_clip(dir: StrafeDir, gait: Gait) -> &'static str {
    // The program's walk/run clip set (`0x1403faed0`, "normal" row: WAIT0,
    // WALK, RUN, then TURN pairs 21/17, 22/18, 23/19, 24/20) lists each
    // strafe pair walk-first: clip 21 (named TURN_RUN_R) is the *walking*
    // strafe and 17 (TURN_WALK_R) the running one. Laurence's footage
    // confirmed it (locked-on sideways full push looked like a walk).
    match (gait, dir) {
        (Gait::Walk, StrafeDir::Right) => "TURN_RUN_R",
        (Gait::Walk, StrafeDir::Left) => "TURN_RUN_L",
        (Gait::Walk, StrafeDir::BackRight) => "TURN_RUN_BR",
        (Gait::Walk, StrafeDir::BackLeft) => "TURN_RUN_BL",
        (Gait::Run, StrafeDir::Right) => "TURN_WALK_R",
        (Gait::Run, StrafeDir::Left) => "TURN_WALK_L",
        (Gait::Run, StrafeDir::BackRight) => "TURN_WALK_BR",
        (Gait::Run, StrafeDir::BackLeft) => "TURN_WALK_BL",
    }
}

/// Diagnostics only (`PD_FACE_LOG` and the hand-off test): the TURN_* pivot
/// clips carry their own baked hip-joint yaw (`pc00a.glb` node 22, the
/// hip; identical for pc01a/pc02a/pc09a, other characters' hips differ).
/// The renderer does NOT cancel it: like the program, the model faces the
/// sim's facing (heading + the side's fixed offset during a pivot) and the
/// clip's own turn carries the body round, ending -- plus the offset -- on
/// the WALK/RUN pose facing the heading (within 7.5 deg for all 11 playable
/// characters, MOVEMENT-AUDIT.md "Locked-on lane"). (time, yaw-radians)
/// keyframes, downsampled to ~8 points per clip and unwrapped; linearly
/// interpolated by `clip_root_yaw`.
fn turn_clip_keyframes(clip: &str) -> &'static [(f32, f32)] {
    match clip {
        "TURN_WALK_R" => &[(0.0000, -0.6155), (0.1000, -0.5107), (0.2333, -0.3999), (0.3333, -0.5160), (0.4667, -0.5789), (0.5667, -0.9921), (0.7000, -1.4374), (0.8000, -1.2789)],
        "TURN_WALK_L" => &[(0.0000, 0.6155), (0.1667, 0.3428), (0.3000, 0.4725), (0.4667, 0.5789), (0.6333, 1.2987), (0.8000, 1.3001), (0.9333, 1.5036), (1.1000, 1.8433)],
        "TURN_WALK_BR" => &[(0.0000, 0.0774), (0.1333, -0.1647), (0.2333, -0.3793), (0.3667, -1.1598), (0.5000, -2.4716), (0.6333, -2.7352), (0.7333, -2.7771), (0.8667, -2.7902)],
        "TURN_WALK_BL" => &[(0.0000, -0.0774), (0.1667, 0.2112), (0.3333, 0.8204), (0.5000, 2.4909), (0.6667, 2.8614), (0.8333, 2.9844), (1.0000, 3.0816), (1.1667, 3.4930)],
        "TURN_RUN_R" => &[(0.0000, -0.2987), (0.1667, -0.6449), (0.3333, -0.9981), (0.5000, -0.7958), (0.6667, -0.7275), (0.8333, -0.9954), (1.0000, -1.4178), (1.1667, -1.6197)],
        "TURN_RUN_L" => &[(0.0000, 0.2987), (0.2333, 0.9250), (0.4667, 0.8054), (0.7000, 0.7365), (0.9333, 1.2290), (1.1667, 1.5961), (1.4000, 1.4069), (1.6333, 1.5220)],
        "TURN_RUN_BR" => &[(0.0000, 0.0013), (0.1333, -0.1826), (0.2667, -0.7326), (0.4000, -1.5483), (0.5333, -2.4113), (0.6667, -2.9351), (0.8000, -3.2083), (0.9333, -3.1992)],
        "TURN_RUN_BL" => &[(0.0000, -0.0013), (0.2000, 0.4333), (0.4000, 1.5483), (0.6000, 2.6777), (0.8000, 3.2083), (1.0000, 3.0913), (1.2000, 2.9802), (1.4000, 3.0842)],
        _ => &[(0.0, 0.0)],
    }
}

/// The pivot clip's own baked yaw (radians) at `t` seconds into it, linearly
/// interpolated between the measured keyframes (clamped at both ends).
fn clip_root_yaw(clip: &str, t: f32) -> f32 {
    let k = turn_clip_keyframes(clip);
    if t <= k[0].0 {
        return k[0].1;
    }
    for w in k.windows(2) {
        let [(t0, y0), (t1, y1)] = w else { unreachable!() };
        if t <= *t1 {
            let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            return y0 + (y1 - y0) * f;
        }
    }
    k[k.len() - 1].1
}

/// This pivot clip's own length (its last keyframe's time).
fn turn_clip_seconds(clip: &str) -> f32 {
    turn_clip_keyframes(clip).last().unwrap().0
}

/// WALK/RUN's own baked hip yaw at their first frame (radians, `pc00a.glb`
/// node 22: WALK -3.3 deg, RUN 20.1 deg; RUN swings about +/-20 deg over
/// its cycle) -- the pose the pivot hands off to, which starts at frame 0.
/// Diagnostics only, like `turn_clip_keyframes`.
fn gait_clip_yaw(gait: Gait) -> f32 {
    match gait {
        Gait::Walk => -0.058,
        Gait::Run => 0.351,
    }
}

/// Which mood clip set fighter `i` shows (`anim_args::mood`, program
/// `0x14025c980`): speed status +-50 (the modifiers' total, in percent),
/// the confusion status (our inverted-controls status, Brain Concussion or
/// Confused Gravity; unconfirmed that the program's status 35 is exactly it), Health <= 5.
fn fighter_mood(c: &combat::Combat, i: usize) -> anim_args::Mood {
    anim_args::mood(c.stat(i, moves::effects::Stat::Speed), c.inverted(i), c.fighters[i].health)
}

/// The Jump skill (special, s43 = 3, s44 = 0; `FUN_140270200` -> `FUN_140257b90`)
/// while the body is in the air after its take-off: the program lifts the body
/// with its ordinary airborne states, so it shows JUMP_UP rising and JUMP_DOWN
/// falling -- not the skill row's WAIT0 that the cast's own action holds.
/// `None` when it is not that.
fn jump_skill_flight(world: &Match, i: usize) -> Option<&'static str> {
    let c = &world.combat;
    let p = &world.players[i];
    if !matches!(p.motion, Motion::Cast | Motion::Lunge) || p.grounded || p.jump_windup() > 0 {
        return None;
    }
    let d = c.skill(c.fighters[i].cast_skill)?;
    let is_jump = d.capsule == combat::Capsule::Special && d.raw[43] == 3 && d.raw[44] == moves::special::move_kind::JUMP;
    is_jump.then(|| if p.vel.y > 0.0 { "JUMP_UP" } else { "JUMP_DOWN" })
}

/// Which clip a player shows this frame.
fn clip_for(world: &Match, i: usize) -> &'static str {
    let c = &world.combat;
    if let Some(flight) = jump_skill_flight(world, i) {
        return flight;
    }
    // The Jump skill's wind-up (state 4, `0x140257b90`): the JUMP_START clip
    // plays while the body stands rooted before the take-off.
    if world.players[i].jump_windup() > 0 {
        return "JUMP_START";
    }
    let mood = fighter_mood(c, i);
    match world.players[i].motion {
        // The idle / walk / run set of the fighter's mood (normal unless a
        // speed status, confusion or low Health says otherwise).
        Motion::Idle => anim_args::mood_clip(mood, None),
        Motion::Walk => anim_args::mood_clip(mood, Some(Gait::Walk)),
        Motion::Run => anim_args::mood_clip(mood, Some(Gait::Run)),
        // Locked-on pivot: the TURN_* clip of the pivot's side and its
        // current gait (a gait change swaps the pair, `sim::Pivot`).
        Motion::Strafe(dir) => world.players[i].pivot().map_or(strafe_clip(dir, world.players[i].gait), |pv| strafe_clip(pv.dir, pv.gait)),
        // Hanging at the top of a Levitate / Skywalk rise (state 7, float
        // status on, `vy <= 0`): FLOAT_IN first (`0x14025a330`); `float_hold_clip`
        // picks the clip after it.
        Motion::Rise | Motion::Fall if world.players[i].floating() => "FLOAT_IN",
        Motion::Rise => "JUMP_UP",
        Motion::Fall => "JUMP_DOWN",
        Motion::Land => "JUMP_LAND1",
        Motion::HeavyLand => "JUMP_LAND0",
        // A cast -- including a short-range skill's wind-up and dash, which
        // the sim shows as `Lunge` -- plays the action row's own clip from
        // the cast start (`cast_clip_now`); this is only the fallback name.
        Motion::Lunge | Motion::Cast => c.skill(c.fighters[i].cast_skill).map_or("SPE_POWER_UP", cast_clip),
        // Ground dodges reuse the dash clips (the characters' JUMP_* slots
        // hold the same clips): a tumble is DASHEND_A (0.5 s); a cartwheel
        // DASH_A, then DASHEND_A while recovering. Unconfirmed pairing.
        Motion::Dodge if world.players[i].dodge == sim::Dodge::Cartwheel => "DASH_A",
        Motion::Dodge | Motion::Recover => "DASHEND_A",
        // The hit-reaction row's own clip for the hurt/knock-down states
        // (docs/FORMATS-SKILLS.md "Action table and hit-reaction table"):
        // stagger/knock-down pose by hit direction for Hit/Blown (a kind-1
        // collapse's own clip, the WALLHIT clips in flight); for Floored the
        // landing clip of its fall-height class and then the DOWN_* loop, for
        // GetUp the stand-up of that pose (not carried by the row).
        Motion::Hit | Motion::Blown => c.reaction_clip(i),
        Motion::Floored => c.lying_clip(i),
        Motion::GetUp => c.getup_clip(i),
        Motion::Down => "BLOW0_FRONT",
    }
}

/// The clip a floating body shows once FLOAT_IN has played out (state 7,
/// `0x14025b170`): FLOAT_MOVE while the stick moves it, FLOAT_WAIT otherwise.
/// (While FLOAT_IN is still playing the handler leaves it alone either way:
/// its test `(clip - 0x2a) & ~2` is false for clips 42 and 44.) Both loop and
/// start with the 5-frame blend, `FUN_140257310(.., 1, 5.0, 0, ..)`.
fn float_hold_clip(moving: bool) -> &'static str {
    if moving { "FLOAT_MOVE" } else { "FLOAT_WAIT" }
}

/// Whether the player is in a cast (the wind-up/dash of a short-range skill
/// included): its clip is chosen once, at the cast start.
fn casting(world: &Match, i: usize) -> bool {
    matches!(world.players[i].motion, Motion::Cast | Motion::Lunge) && jump_skill_flight(world, i).is_none()
}

/// The clip the cast in progress plays: the skill's action-table row's own
/// animation (w38, `anim_name` in skills.json) -- the program starts exactly
/// that clip at the cast start (`FUN_14025a3e0`, PROGRAM-RE.md "Dispatch
/// map"), for the row the cast uses (ground, or the mid-air row). Falls back
/// to the hand table (`cast_clip`) for a skill without a row or a clip the
/// character doesn't have. Motion-name index = ALR animation slot
/// (FORMATS-ANIMATION.md "Program evidence": slot lengths match the rows'
/// clip lengths, e.g. ATK_SORD 40, ATK_WHAND_SORD 54).
fn cast_clip_now(world: &Match, i: usize, has: impl Fn(&str) -> bool) -> &'static str {
    let c = &world.combat;
    let f = &c.fighters[i];
    let row = c.cast_row(i, f.cast_skill).and_then(|r| pd_formats::anim::ANIM_NAMES.iter().copied().find(|n| *n == r.anim_name));
    match row {
        Some(name) if has(name) => name,
        _ => c.skill(f.cast_skill).map_or("SPE_POWER_UP", cast_clip),
    }
}

/// Cast animation by skill family and motion (table fields s42 / s63; see
/// docs/MOVES.md). Only the fallback now: casts play their action row's own
/// clip (`cast_clip_now`); this covers skills with no row.
fn cast_clip(def: &combat::SkillDef) -> &'static str {
    use combat::Capsule::*;
    match (def.capsule, def.family) {
        (Attack, _) => travel_clip(def),
        (Defence, 82 | 89) => "DEF_SHELTER",
        (Defence, 83) => "DEF_ABSORB",
        (Defence, 84) => "DEF_BRUSH",
        (Defence, 85 | 86) => "DEF_COURSE",
        (Defence, _) => "DEF_SHIELD",
        (Environment, _) => "ENV_SET",
        // Erase / status / special that travel (shells, Holes, mines, dashes)
        // use the attack poses; the rest by what they do.
        _ => match moves::behaviour_of(def) {
            moves::Behaviour::Shoot(_) | moves::Behaviour::Place(_) | moves::Behaviour::Melee(_) => travel_clip(def),
            _ => match def.id {
                179 | 238 | 297 | 177 | 331 => "SPE_HEAL",  // Healing Water, Regenerate, Rebirth, Blessed Water/Rain
                240 => "SPE_DASH",                           // Dash
                57 | 58 | 60 | 304 => "SPE_WARP",            // Return, Surprise Attack, Retreat, Charge
                _ if def.raw[42] == 72 || def.raw[51] == 17 => "SPE_RESET", // Refresh, Neutralization...
                _ => "SPE_POWER_UP",
            },
        },
    }
}

/// Pose for a skill that leaves the caster (shots, lobs, mines, dashes...).
fn travel_clip(def: &combat::SkillDef) -> &'static str {
    use moves::motion;
    use pd_formats::skills::field;
    // Family sound id (s21) splits short-range: 506 punches, 507 kicks, 516 drains.
    let sound = def.raw[21];
    match def.motion() {
        13 if sound == 506 => "ATK_PUNCH",
        13 if sound == 507 => "ATK_KICK",
        13 if sound == 516 => "ATK_DRAIN",
        12 | 13 | motion::PURSUIT => "ATK_SORD",
        motion::ARC | motion::RICOCHET => "ATK_LASER",
        motion::PARABOLA => "ATK_THROW",
        motion::MINE => "ATK_SET_BOMB",
        motion::AROUND_SELF => "ATK_SPHERE",
        motion::RAIN_A | motion::RAIN_B | motion::FALL => "ATK_SKYSHOT_HIGH",
        motion::HOLE_A | motion::HOLE_B => "ATK_STAMP",
        motion::CRAWL | motion::TORNADO => "ATK_GROUND",
        _ if def.family == 10 => "ATK_SORD",
        // Shells by school (s11): 5 Faith, 4 Ki.
        _ => match def.raw[field::SCHOOL] {
            5 => "ATK_FAITH_SHOT",
            4 => "ATK_KI_SHOT",
            _ => "ATK_SHOT",
        },
    }
}

/// Skill numbers from `cache/skills.json` (written by `pd_convert skills`).
/// `only_registered` keeps just the skills a player can actually collect
/// (the arsenal builder's catalog); the match itself needs every skill a
/// saved arsenal might reference, so it passes `false`.
fn load_skills(path: &std::path::Path, only_registered: bool) -> Vec<combat::SkillDef> {
    use pd_formats::skills::field;
    let text = std::fs::read_to_string(path).expect("missing skills.json; run pd_convert skills (see docs/PIPELINE.md)");
    let v: serde_json::Value = serde_json::from_str(&text).expect("bad skills.json");
    let n = |s: &serde_json::Value, k: &str| s[k].as_i64().unwrap_or(0);
    let mut out: Vec<combat::SkillDef> = Vec::new();
    for s in v["skills"].as_array().into_iter().flatten() {
        let id = n(s, "id") as i16;
        // The versus roster is ids 1-340 (the 340 player skills, P0-SKILLS.md
        // "The usable roster": the rest are enemy-only records and boss
        // skills, DECISIONS 2026-10-01).
        if only_registered && (s["registered"].as_bool() != Some(true) || !(1..=340).contains(&id)) {
            continue;
        }
        let Some(capsule) = s["capsule"].as_str().and_then(combat::Capsule::parse) else { continue };
        if out.iter().any(|d| d.id == id) {
            continue; // later duplicates are variants
        }
        let raw = |k: usize| s["raw"][k].as_i64().unwrap_or(0) as i16;
        out.push(combat::SkillDef {
            id,
            name: s["name"].as_str().unwrap_or("?").to_string(),
            capsule,
            family: raw(field::FAMILY),
            cost: n(s, "cost") as i32,
            strength: n(s, "strength") as i32,
            strength_rule: n(s, "strength_rule") as i16,
            requirement: n(s, "requirement") as i16,
            requirement_amount: n(s, "requirement_amount") as i32,
            level_penalty: n(s, "level_penalty") as i32,
            health_cost: n(s, "health_cost") as i32,
            defence: raw(field::DEFENCE) as i32,
            raw: std::array::from_fn(raw),
            s70_73: std::array::from_fn(|k| s["s70_73"][k].as_i64().unwrap_or(0) as i16),
        });
    }
    out
}

/// The records of `skills.json`'s `"skills"` array that name a skill: the
/// FIRST record of each id. Ids 257 (Judge of Anubis) and 294 (Fusion) occur
/// twice; the second copy (records 749 / 750) is a character's dodge move
/// (docs/FORMATS-SKILLS.md "Phase 3"), not an arsenal skill, and collecting
/// the records into a map let it overwrite the real skill. The dodges read
/// no skill row by id (`sim::Dodge` + the clip names in `clip_for`), so
/// nothing is lost by skipping them. `load_skills` and `fx::SkillFx::load`
/// keep the first record the same way.
pub(crate) fn first_skill_records(v: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    let mut seen = std::collections::HashSet::new();
    v["skills"].as_array().into_iter().flatten().filter(move |s| s["id"].as_i64().is_none_or(|id| seen.insert(id)))
}

/// Every skill's action-table rows (docs/FORMATS-SKILLS.md "Action table"),
/// keyed by skill id, for `Combat::set_action_rows`. Kept off `SkillDef`
/// itself so the many places elsewhere in the crate that build one by hand
/// (tests, the front end's stand-ins) don't need to change.
fn load_action_rows(path: &std::path::Path) -> std::collections::HashMap<i16, (Option<combat::ActionRow>, Option<combat::ActionRow>)> {
    let Ok(text) = std::fs::read_to_string(path) else { return std::collections::HashMap::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return std::collections::HashMap::new() };
    first_skill_records(&v)
        .filter_map(|s| Some((s["id"].as_i64()? as i16, (action_row(&s["action"]["ground"]), action_row(&s["action"]["air"])))))
        .collect()
}

/// The combo window of an action row (w45 / w46 / w47 of its `raw`): `FUN_14024fb90`.
fn chain_window(v: &serde_json::Value) -> combat::ChainWindow {
    let w = |k: usize| v["raw"][k].as_i64().unwrap_or(0) as i16;
    combat::ChainWindow { from: w(45), to: w(46), at: w(47) }
}

/// Every skill's combo data from `skills.json` (`"action": {"chain": [...]}`, the rows s14..s17):
/// the ground row's window, and the chained rows with theirs, for `Combat::set_chain_rows`.
/// An older cache without `chain` gives nothing (no combos).
fn load_chain_rows(path: &std::path::Path) -> (std::collections::HashMap<i16, combat::ChainWindow>, std::collections::HashMap<i16, Vec<(combat::ActionRow, combat::ChainWindow)>>) {
    let none = Default::default();
    let Ok(text) = std::fs::read_to_string(path) else { return none };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return none };
    let (mut first, mut rows) = (std::collections::HashMap::new(), std::collections::HashMap::new());
    for s in first_skill_records(&v) {
        let Some(id) = s["id"].as_i64().map(|i| i as i16) else { continue };
        let chain: Vec<_> = s["action"]["chain"].as_array().into_iter().flatten().filter_map(|r| Some((action_row(r)?, chain_window(r)))).collect();
        if chain.is_empty() {
            continue;
        }
        first.insert(id, chain_window(&s["action"]["ground"]));
        rows.insert(id, chain);
    }
    (first, rows)
}

/// One `"action": {"ground": ..., "air": ...}` row (docs/FORMATS-SKILLS.md
/// "Action table"), or `None` for `null` (no row: no mid-air cast, or a
/// skill built without the action table at all).
fn action_row(v: &serde_json::Value) -> Option<combat::ActionRow> {
    if v.is_null() {
        return None;
    }
    let n = |k: &str| v[k].as_i64().unwrap_or(0) as i16;
    Some(combat::ActionRow {
        anim_name: v["anim_name"].as_str().unwrap_or("").to_string(),
        cast_sound: n("cast_sound"),
        charge_cycle: n("charge_cycle"),
        dash_start: n("dash_start"),
        dash_end: n("dash_end"),
        fire_tick: n("fire_tick"),
        hit_window_end: n("hit_window_end"),
    })
}

/// The hit-reaction table (docs/FORMATS-SKILLS.md "Action table and
/// hit-reaction table"), keyed by row id, for `Combat::set_reaction_rows`:
/// `skills.json`'s top-level `"reactions"` array.
fn load_reaction_rows(path: &std::path::Path) -> std::collections::HashMap<i16, combat::ReactionRow> {
    let Ok(text) = std::fs::read_to_string(path) else { return std::collections::HashMap::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return std::collections::HashMap::new() };
    v["reactions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let n = |k: &str| r[k].as_i64().unwrap_or(0) as i16;
            let w = r["w"].as_array().cloned().unwrap_or_default();
            let w8 = std::array::from_fn(|k| w.get(k).and_then(|x| x.as_i64()).unwrap_or(0) as i16);
            Some((r["row"].as_i64()? as i16, combat::ReactionRow { kind: n("kind"), effect: n("effect"), sound: n("sound"), voice_group: n("voice_group"), launch_speed: n("launch_speed"), launch_angle: n("launch_angle"), w: w8, root: r["raw"][1].as_i64().unwrap_or(0) != 0 }))
        })
        .collect()
}

/// Item 2 (cast-timing lane): each side's own clip lengths, read once at
/// startup from its converted model (`cache/chars/<id>.glb`,
/// `combat::clip_ticks_from_glb`) so the caster is held for its own clip's
/// length rather than a flat guess. A character not yet converted (only
/// pc00a is, as of this writing) falls back to `Combat`'s built-in golden
/// values for the clips it doesn't have.
fn load_clip_ticks(cache: &std::path::Path, chars: &[String]) -> Vec<std::collections::HashMap<String, u16>> {
    chars.iter().map(|id| std::fs::read(cache.join("chars").join(format!("{id}.glb"))).ok().map(|b| combat::clip_ticks_from_glb(&b)).unwrap_or_default()).collect()
}

/// Each side's clip root-motion curves (`cache/chars/<id>.rootmotion.json`,
/// `pd_convert character`): clip name -> the cumulative `[side, forward]` of the root joint at
/// each 30 Hz frame, for `Combat::set_root_curves`. A character without the file has no root
/// motion (reactions2 lane; the program moves the body by it, `0x140257510`).
fn load_root_curves(cache: &std::path::Path, chars: &[String]) -> Vec<std::collections::HashMap<String, Vec<[f32; 2]>>> {
    chars
        .iter()
        .map(|id| {
            let text = std::fs::read_to_string(cache.join("chars").join(format!("{id}.rootmotion.json"))).ok();
            let v = text.and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
            let Some(clips) = v.as_ref().and_then(|v| v["clips"].as_object()) else { return std::collections::HashMap::new() };
            clips
                .iter()
                .filter_map(|(name, c)| {
                    let curve: Vec<[f32; 2]> = c["curve"].as_array()?.iter().map(|p| [p["side"].as_f64().unwrap_or(0.0) as f32, p["forward"].as_f64().unwrap_or(0.0) as f32]).collect();
                    (curve.len() > 1).then(|| (name.clone(), curve))
                })
                .collect()
        })
        .collect()
}

/// Every skill's `SHOTnn` marker offsets (`cache/shotpoints.json`, written by
/// `pd_convert shotpoints`) for `Combat::set_shot_markers`: the markers of the
/// skill's ground effect and of its mid-air effect, found through the action
/// rows' effect numbers in `skills.json`. Empty when either file is missing
/// (shots then start at the chest).
fn load_shot_markers(cache: &std::path::Path) -> std::collections::HashMap<i16, (Vec<Vec3>, Vec<Vec3>)> {
    let read = |name: &str| std::fs::read_to_string(cache.join(name)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let (Some(points), Some(skills)) = (read("shotpoints.json"), read("skills.json")) else { return std::collections::HashMap::new() };
    let markers = |effect: &serde_json::Value| -> Vec<Vec3> {
        let Some(n) = effect.as_i64() else { return Vec::new() };
        points["markers"][n.to_string()]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| Vec3::new(p[0].as_f64().unwrap_or(0.0) as f32, p[1].as_f64().unwrap_or(0.0) as f32, p[2].as_f64().unwrap_or(0.0) as f32))
            .collect()
    };
    first_skill_records(&skills)
        .filter_map(|s| Some((s["id"].as_i64()? as i16, (markers(&s["action"]["ground"]["effect"]), markers(&s["action"]["air"]["effect"])))))
        .filter(|(_, (g, a))| !g.is_empty() || !a.is_empty())
        .collect()
}

/// Each side's right hand through the attack clips (`cache/chars/<id>.hand.json`,
/// `pd_convert shotpoints`) for `Combat::set_hand_paths`; a character without
/// the file gets none (its melee stays centred on the body).
fn load_hand_paths(cache: &std::path::Path, chars: &[String]) -> Vec<std::collections::HashMap<String, combat::HandClip>> {
    let v3 = |a: &serde_json::Value| Vec3::new(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, a[2].as_f64().unwrap_or(0.0) as f32);
    chars
        .iter()
        .map(|id| {
            let Some(v) = std::fs::read_to_string(cache.join("chars").join(format!("{id}.hand.json"))).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return Default::default() };
            v["clips"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(name, c)| (name.clone(), combat::HandClip { pos: c["pos"].as_array().into_iter().flatten().map(v3).collect(), fwd: c["fwd"].as_array().into_iter().flatten().map(v3).collect() }))
                .collect()
        })
        .collect()
}

#[derive(Resource)]
struct Config {
    stage: String,
    chars: [String; 2],
    demo: bool,
    screenshot: Option<PathBuf>,
    shot_frame: u32,
    cache: PathBuf,
    /// A pre-existing CLI option (stage/chars/demo/screenshot/loadout/
    /// arsenal/open-menu) was given: skip the menus and go straight into a
    /// match, exactly like before the front end existed. Automated tests and
    /// screenshot scripts depend on this.
    legacy: bool,
}

#[derive(Resource)]
pub(crate) struct World(pub(crate) Match);

#[derive(Resource)]
struct StageCollision(Collision);

impl StageCollision {
    /// The height of the highest standable surface at or below `(x, z, from)`.
    /// For render-side effects that need to sit *on* the floor rather than at a
    /// character's root (`footsteps.rs`).
    pub(crate) fn ground_below(&self, x: f32, z: f32, from: f32) -> Option<f32> {
        self.0.ground_below(x, z, from)
    }
}

#[derive(Resource, Default)]
struct PendingInputs([Input; combat::MAX_PLAYERS]);

/// Right stick up/down per player (-1..1, up = look up). The camera's pitch
/// changes only the view, so it stays out of the sim.
#[derive(Resource, Default)]
struct CamPitchInput([f32; combat::MAX_PLAYERS]);

/// Which player slot (0..3) a spawned character model belongs to. `pub(crate)`
/// so `school_aura.rs` can find each player's mesh entities the same way
/// `apply_p2_colour` walks up `ChildOf` to find this component.
#[derive(Component)]
pub(crate) struct PlayerVisual(pub(crate) usize);

/// Set by `spawn_scene`: each slot's character and costume colour
/// (`seats::default_colours`: the first of a character wears colour 0, a
/// second one colour 1 -- the old "P2 alternate costume when both picked the
/// same fighter", DECISIONS.md 2026-09-27 -- and so on). `apply_costumes`
/// swaps the texture of every slot whose colour is not 0; colour 0 needs no
/// change and no tint. Both launch paths go through `spawn_scene`, so this
/// resource covers them.
#[derive(Resource, Default)]
struct Costumes {
    chars: Vec<String>,
    colours: Vec<u8>,
}

#[derive(Component)]
struct PlayerCamera(usize);

/// Links a character's AnimationPlayer back to its player slot.
#[derive(Component)]
struct AnimSlot {
    player: usize,
    /// Clip playing, plus the cast counter so a second cast restarts it and
    /// the cast phase (0 none, 1 charging, 2 cast) so a charge's release does.
    current: Option<(&'static str, u16, u8)>,
    /// The clip picked for cast number `.0` (`cast_clip_now`).
    cast: Option<(u16, &'static str)>,
    /// FLOAT_IN has played out in this float (`0x14025a330` starts it once;
    /// the queued FLOAT_WAIT then follows, `0x140257510`).
    float_in_done: bool,
}

#[derive(Resource)]
struct CharacterAnims {
    gltf: Vec<Handle<bevy::gltf::Gltf>>,
    graphs: Vec<Option<(Handle<AnimationGraph>, Vec<(&'static str, AnimationNodeIndex)>)>>,
    /// Each clip's length in seconds (for the phase-keeping WALK/RUN swap).
    lengths: Vec<Vec<(&'static str, f32)>>,
}

impl CharacterAnims {
    /// Nothing built yet for one model per entry of `gltf`.
    fn new(gltf: Vec<Handle<bevy::gltf::Gltf>>) -> Self {
        let n = gltf.len();
        CharacterAnims { gltf, graphs: (0..n).map(|_| None).collect(), lengths: vec![Vec::new(); n] }
    }
}

impl AnimSlot {
    /// The player slot this character animation belongs to. Bevy puts
    /// `AnimationPlayer` on the glTF's own scene entity, which is NOT the
    /// `PlayerVisual` root, so the only reliable link is `AnimSlot` -- the same
    /// one `drive_animation` uses. This is what lets `footsteps.rs` find the
    /// clock for a given player.
    pub(crate) fn player_index(&self) -> usize {
        self.player
    }
}

impl CharacterAnims {
    /// Seconds the currently playing clip has been running, for systems outside
    /// this module that need the animation clock -- `footsteps.rs` reads the walk
    /// cycle to place footfalls.
    ///
    /// `None` when nothing is playing, or when more than one node is active (a
    /// cross-fade), where the clock is ambiguous and the caller should skip
    /// rather than guess. The clip *name* is deliberately not returned: it is
    /// private `AnimSlot` state, and the caller already knows the motion it
    /// asked for. Bevy's `ActiveAnimation` has no duration accessor, and the
    /// node index type is private to its prelude, so the lookup happens here
    /// and only plain data crosses the boundary.
    pub(crate) fn playing_elapsed(player: &AnimationPlayer) -> Option<f32> {
        let mut it = player.playing_animations();
        let (_, active) = it.next()?;
        if it.next().is_some() {
            return None;
        }
        Some(active.elapsed())
    }
}

/// An overlay camera that only draws UI that is often all hidden (the settings menu, the dev tools):
/// it sleeps while nothing that targets it is visible, so a match does not pay a full overlay pass
/// (2D pass, UI pass, blit) per idle camera every frame (overnight perf 2026-10-09).
#[derive(Component)]
pub struct SleepWhenEmpty;

fn sleep_empty_overlays(mut cams: Query<(Entity, &mut Camera), With<SleepWhenEmpty>>, ui: Query<(&UiTargetCamera, &Visibility)>) {
    for (cam, mut c) in &mut cams {
        let wanted = ui.iter().any(|(t, v)| t.entity() == cam && *v != Visibility::Hidden);
        if c.is_active != wanted {
            c.is_active = wanted;
        }
    }
}

/// Light clustering on the CPU instead of Bevy's default GPU clustering (overnight perf 2026-10-09).
/// The game spawns no point lights, spot lights, light probes or clustered decals (the only things
/// clustering sorts), so every cluster is always empty; GPU clustering still ran three compute passes
/// and a raster pass per player view per frame, and rebuilt its buffers each frame (~0.25 ms of
/// render-thread encoding per view). The CPU path with nothing to sort is a no-op. `PD_GPU_CLUSTERING=1`
/// restores Bevy's default.
fn cpu_light_clustering(mut settings: ResMut<bevy::light::cluster::GlobalClusterSettings>) {
    if std::env::var_os("PD_GPU_CLUSTERING").is_none() {
        settings.gpu_clustering = None;
    }
}

/// Each camera's render schedules (`Core3d`, `Core2d`) run on the single-threaded executor
/// (overnight perf 2026-10-09). Bevy runs them once per camera per frame -- four player views, the
/// HUD -- and each holds ~45 systems that mostly find nothing to do for this game (no SSAO, TAA, DoF,
/// atmosphere, deferred ...); the multi-threaded executor's per-system task hand-off cost more than
/// the few real passes gained by encoding in parallel. The command buffers are queued in the same
/// topological system order either way (`RenderContextState` is applied in system order), so the
/// GPU receives the same work. `PD_VIEW_MT=1` restores Bevy's multi-threaded executor.
fn single_thread_view_schedules(app: &mut App) {
    if std::env::var_os("PD_VIEW_MT").is_some() {
        return;
    }
    let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) else { return };
    render.edit_schedule(bevy::core_pipeline::Core3d, |s| {
        s.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
    render.edit_schedule(bevy::core_pipeline::Core2d, |s| {
        s.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
    // Opt-in (overnight perf 2026-10-09): `PD_RENDER_ST=1` also runs the main `Render` schedule
    // single-threaded. Measured with the OLED screensaver busy: 4P -0.6 ms (dev), -0.3 ms (play
    // profile); not adopted until re-measured on an idle machine (its prepare systems do run in
    // parallel). Single-threading `ExtractSchedule` gained nothing.
    if std::env::var_os("PD_RENDER_ST").is_some() {
        render.edit_schedule(bevy::render::Render, |s| {
            s.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
        });
    }
}

/// Automated screenshot runs open their window off-screen, behind everything,
/// unfocused and off the taskbar, so they don't pop up over Laurence's work.
/// `PD_SHOW_WINDOW=1` shows it normally.
fn background_if(background: bool, window: Window) -> Window {
    // PD_FRAME_STATS: measure frame time without the display's refresh capping it (PD_VSYNC=1 keeps
    // the normal present mode, to measure what a player sees).
    let window = if std::env::var_os("PD_FRAME_STATS").is_some() && std::env::var_os("PD_VSYNC").is_none() { Window { present_mode: bevy::window::PresentMode::AutoNoVsync, ..window } } else { window };
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
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let has = |name: &str| args.iter().any(|a| a == name);
    let cache = PathBuf::from(args.first().cloned().unwrap_or_else(|| "cache".into()));
    let stage = opt("--stage").unwrap_or_else(|| DEFAULT_STAGE.into());

    // A pre-existing option: skip the menus, go straight into a match, same
    // as before the front end existed (automated tests/screenshots rely on
    // this). `--open-screen` / `--menu-script` are new, hidden switches for
    // this lane's own screenshots and take the front end instead.
    let hidden = has("--open-screen") || has("--menu-script");
    let legacy = !hidden && ["--p1", "--p2", "--players", "--stage", "--props-state", "--near", "--demo", "--screenshot", "--shot-frame", "--loadout", "--arsenal", "--open-menu", "--start-level", "--start-hand", "--cpu", "--mode", "--view-seat", "--online-host", "--online-join"].iter().any(|f| has(f));
    let initial_state = if legacy {
        AppState::Match
    } else {
        match opt("--open-screen").as_deref() {
            Some("setup") => AppState::Setup,
            Some("arsenal") => AppState::Arsenal,
            Some("online") => AppState::Online,
            Some("result") => AppState::Result,
            _ => AppState::Title,
        }
    };

    // Test hook for three- and four-player matches without pads: `--players N`
    // (1-4, default 2) starts N fighters on the stage's own start points, with
    // `--p1`..`--p4` for their characters and `--teams 0,0,1,1` for teams
    // (default: everyone their own team). Nobody moves unless `--demo` or the
    // `PD_TEST_*` scripts drive them.
    let players_n = opt("--players").and_then(|v| v.parse::<usize>().ok()).unwrap_or(2).clamp(1, combat::MAX_PLAYERS);
    let default_chars = ["pc00a", "pc01a", "pc02a", "pc03a"];
    let cli_chars: Vec<String> = (0..players_n).map(|i| opt(&format!("--p{}", i + 1)).unwrap_or_else(|| default_chars[i].into())).collect();
    let cli_teams: Vec<u8> = opt("--teams").map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect()).unwrap_or_default();
    let config = Config {
        chars: [opt("--p1").unwrap_or_else(|| "pc00a".into()), opt("--p2").unwrap_or_else(|| "pc01a".into())],
        demo: has("--demo"),
        screenshot: opt("--screenshot").map(PathBuf::from),
        shot_frame: opt("--shot-frame").and_then(|v| v.parse().ok()).unwrap_or(300),
        stage: stage.clone(),
        cache: cache.clone(),
        legacy,
    };

    // A placeholder match (today's defaults); real interactive play
    // overwrites this via `enter_match` once the versus setup screen starts
    // one, but it's always valid so these resources need no `Option`.
    // The menus don't need this stage: with no options, a cache without it
    // still opens on the title (an empty placeholder until a match starts).
    let col = match std::fs::read(cache.join(format!("{stage}.pdcl"))) {
        Ok(bytes) => stairs_under::load(&bytes, &stage),
        Err(_) if !legacy => Collision::new(Vec::new()),
        Err(_) => panic!("missing {stage}.pdcl; run pd_convert collision (see docs/PIPELINE.md)"),
    };
    let layout = stage_layout(&cache, &stage, players_n);
    // --near (test runs): both players close together near the stage centre
    // (the survey) instead of the original start points far apart.
    let spawns: Vec<Vec3> = match layout.as_ref() {
        Some(l) if !has("--near") => l.spawns.clone(),
        // --near: survey the floor the original start points stand on (st03's
        // hall floor is exactly y 50, which the plain survey skipped and put
        // players on the roof outside the hall).
        l => spread_spawns(pick_spawns(&col, &cache.join(format!("{stage}.bounds.json")), l.and_then(|l| col.ground_below(l.spawns[0].x, l.spawns[0].z, l.spawns[0].y + 12.0))), players_n),
    };
    let spawns = test_spawn(&col, spawns);
    let spawns = film_spawns(&col, spawns);
    let arsenal = match opt("--arsenal").as_deref() {
        Some("mixed") => mixed_arsenal(),
        Some(s) if s.starts_with("school:") => single_school_arsenal(s["school:".len()..].parse().unwrap_or(1)),
        _ => test_arsenal(),
    };
    let players = seats::MatchPlayers::with_teams(&cli_chars, &vec![arsenal; players_n], &cli_teams);
    let mut combat = combat::Combat::with_roster(load_skills(&cache.join("skills.json"), false), players.roster(), players.arsenals());
    combat.set_clip_ticks(load_clip_ticks(&cache, &cli_chars));
    combat.set_root_curves(load_root_curves(&cache, &cli_chars));
    combat.set_action_rows(load_action_rows(&cache.join("skills.json")));
    let (first, rows) = load_chain_rows(&cache.join("skills.json"));
    combat.set_chain_rows(first, rows);
    combat.set_shot_markers(load_shot_markers(&cache));
    combat.set_hand_paths(load_hand_paths(&cache, &cli_chars));
    combat.set_reaction_rows(load_reaction_rows(&cache.join("skills.json")));
    let rumble_tables = rumble::RumbleTables::load(&cache.join("skills.json"));
    if let Some(list) = opt("--loadout") {
        // Test runs: slots always hold these four skill ids (e.g. 1,65,26,65) at level 8.
        let ids: Vec<i16> = list.split(',').filter_map(|s| s.parse().ok()).collect();
        if let Ok(slots) = <[i16; 4]>::try_from(ids) {
            // No dealt skills: the loadout only fills empty buttons.
            combat.set_rules(combat::Rules { start_hand: 0, ..combat::Rules::default() });
            combat.test_loadout = Some((slots, 8));
        }
    }
    // Test runs: PD_TEST_HEALTH=n keeps everyone's Health at or below n (knock-out captures).
    combat.test_health = std::env::var("PD_TEST_HEALTH").ok().and_then(|v| v.parse().ok());
    if let Some(l) = &layout {
        combat.set_capsule_spots(l.capsule_spots.clone());
        combat.props = l.props.clone();
    }
    // --start-level N / --start-hand N: the setup screen's rule options.
    if has("--start-level") || has("--start-hand") {
        let d = combat::Rules::default();
        let start_level = opt("--start-level").and_then(|v| v.parse().ok()).unwrap_or(d.start_level);
        let start_hand = opt("--start-hand").and_then(|v| v.parse().ok()).unwrap_or(d.start_hand);
        combat.set_rules(combat::Rules { start_level, start_hand, ..d });
    }
    // --mode N: play game mode N of `frontend::modes::MODES` (its rules and fixed hand, as the GAMEMODES row starts it).
    let cli_mode = opt("--mode").and_then(|v| frontend::modes::find(&v));
    if let Some(mode) = cli_mode {
        combat.set_rules(mode.rules());
        combat.set_arsenal_kinds([Some(arsenal::DEFAULT_KIND); combat::MAX_PLAYERS]);
    }
    // Screenshot tests: every breakable object starts damaged or broken.
    if let Some(state) = opt("--props-state") {
        let state = if state == "broken" { props::PropState::Broken } else { props::PropState::Damaged };
        for p in combat.props.iter_mut().filter(|p| p.breakable()) {
            p.state = if state == props::PropState::Broken && !p.has_broken { props::PropState::Damaged } else { state };
            if p.state == props::PropState::Damaged && !p.has_damaged {
                p.state = props::PropState::Intact;
            }
        }
    }
    let mut legacy_match = Match::with_spawns(&spawns, combat);
    // --mode prop_hunt: Prop Hunt from the command line (screenshots), with `--ph-time <seconds>` and `--ph-kills N`
    // standing in for the setup's ROUND TIME / KILLS TO WIN rows.
    // Never online: Prop Hunt is local split screen only.
    if cli_mode.is_some_and(|m| m.is_prop_hunt()) && !has("--online-host") && !has("--online-join") {
        let d = prophunt::Config::default();
        let hunt_ticks = opt("--ph-time").and_then(|v| v.parse::<u32>().ok()).map_or(d.hunt_ticks, |secs| secs * sim::HZ as u32);
        let kills_to_win = opt("--ph-kills").and_then(|v| v.parse().ok()).unwrap_or(d.kills_to_win);
        legacy_match.start_prop_hunt(prophunt::Config { hunt_ticks, kills_to_win });
    }
    // Screenshot tests: PD_FREE_CAM=1 starts both players not locked on.
    if std::env::var_os("PD_FREE_CAM").is_some() {
        for p in &mut legacy_match.players {
            p.locked = false;
        }
    }
    legacy_match.jump_zones = layout.map(|l| l.jump_zones).unwrap_or_default();
    set_dodges(&mut legacy_match, &cache, &cli_chars);

    // What the versus setup and arsenal builder screens offer (frontend/discovery.rs).
    let registered_skills = load_skills(&cache.join("skills.json"), true);
    let notes = frontend::discovery::load_notes(&cache.join("skills.json"));
    let decks_json: serde_json::Value = std::fs::read_to_string(cache.join("skills.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut builtins = frontend::discovery::school_decks(&decks_json, &registered_skills);
    builtins.push(frontend::discovery::BuiltinArsenal { name: "Test Set".into(), skills: test_arsenal(), kind: None });
    builtins.push(frontend::discovery::BuiltinArsenal { name: "Mixed (one of each)".into(), skills: mixed_arsenal(), kind: None });
    let game_data = GameData { characters: frontend::discovery::list_characters(&cache), stages: frontend::discovery::list_stages(&cache), builtins };
    let saved_arsenals = SavedArsenals(frontend::arsenals::ArsenalStore::new(&cache).load());

    // PD_SETTINGS=<file> keeps a test run's settings out of the real settings.json (a capture never writes it at all).
    let settings_path = std::env::var_os("PD_SETTINGS").map(PathBuf::from).unwrap_or_else(|| cache.join("settings.json"));
    let mut settings = menu::Settings::load(settings_path);
    if config.screenshot.is_some() {
        settings.disable_saving();
        // A capture window is windowed whatever the first-run default is (and `apply_display` never touches the display).
        if !settings.display_chosen {
            settings.display = display::DisplaySettings::default();
        }
    }
    // Screenshot tests: PD_TEST_SPLIT=sbs|tb overrides the saved split layout
    // without touching settings.json (that file is shared with Laurence's own
    // saved preference in the main checkout's cache).
    if let Some(v) = std::env::var("PD_TEST_SPLIT").ok() {
        settings.split = if v == "tb" { menu::Split::TopBottom } else { menu::Split::SideBySide };
    }

    // Screenshot tests: PD_TEST_ASPECT=auto|16:9|16:10|21:9|4:3 and PD_TEST_HUD=75|100|125|150 override the saved
    // display options the same way (settings.json is never touched by a capture).
    if let Ok(v) = std::env::var("PD_TEST_ASPECT") {
        if let Some(a) = display::Aspect::parse(&v) {
            settings.display.aspect = a;
        }
    }
    if let Some(h) = std::env::var("PD_TEST_HUD").ok().and_then(|v| v.parse::<u32>().ok()).and_then(|p| display::HudSize::ALL.into_iter().find(|h| h.percent() == p)) {
        settings.display.hud = h;
    }
    // Lighting and look (look.rs): PD_TEST_LOOK=original|enhanced|max and PD_TEST_BLOOM / _SHADOWS / _TONE / _LIGHTS
    // override single rows for a capture, never saved.
    settings.look = settings.look.with_test_env();
    // A saved fullscreen choice starts fullscreen (never in a capture run, which opens off-screen).
    let start_mode = if config.screenshot.is_some() { bevy::window::WindowMode::Windowed } else { display::startup_mode(&settings.display) };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(AssetPlugin { file_path: cache.to_string_lossy().into_owned(), ..default() }).set(WindowPlugin {
        primary_window: Some(background_if(config.screenshot.is_some(), Window { title: "Phantom Dust - split-screen prototype".into(), resolution: film_window().into(), mode: start_mode, ..default() })),
        ..default()
    }))
    .insert_resource(Time::<Fixed>::from_hz(sim::HZ))
    .insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
    .insert_resource(settings)
    .insert_resource({
        let mut m = menu::MenuState::opened(has("--open-menu"));
        // Capture switch: PD_TEST_LOOK_PAGE=<row> opens the overlay on the Lighting and look page.
        if let Some(row) = std::env::var("PD_TEST_LOOK_PAGE").ok().and_then(|v| v.parse::<usize>().ok()) {
            m.show_look_page(row);
        }
        // Capture switch: PD_TEST_OPTIONS_TAB=video|sound|controls|buttons opens the overlay on that tab
        // (buttons = the CONTROLS tab's button-mapping page).
        match std::env::var("PD_TEST_OPTIONS_TAB").ok().as_deref() {
            Some("buttons") => m.show_buttons_page(),
            Some(t) => {
                if let Some(tab) = menu::Tab::parse(t) {
                    m.open = true;
                    m.show_tab(tab);
                }
            }
            None => {}
        }
        m
    })
    // Render-side systems outside the combat event stream (footsteps) push
    // their cues here; `sfx.rs` owns playback and drains it each frame.
    .insert_resource(sfx::CueQueue::default())
    .insert_resource(config)
    .insert_resource(World(legacy_match))
    .insert_resource(cam_occluders::CamOccluders::load(&cache, &stage))
    .insert_resource(StageCollision(col))
    .insert_resource(effects::FxLibrary::new(cache.clone()))
    .insert_resource(fx::SkillFx::load(&cache.join("skills.json")))
    .init_resource::<effects::FxPreload>()
    .init_resource::<effects::FxWarmup>()
    .insert_resource(players)
    .insert_resource(rumble_tables)
    .init_resource::<seats::Seats>()
    .init_resource::<seats::RequestedPlayers>()
    // `--view-seat N` (1-based, like `--p1`): this machine shows only seat N's
    // view, full window (test hook for the online single-screen view).
    .insert_resource({
        // `--view-seat N` shows one seat; otherwise only the human seats get views (`--cpu` seats have none).
        let kinds = cpu::parse_cli(opt("--cpu").as_deref());
        match opt("--view-seat").and_then(|v| v.parse::<usize>().ok()).filter(|n| (1..=players_n).contains(n)) {
            Some(n) => seats::LocalViews(Some(vec![n - 1])),
            None => seats::LocalViews::for_kinds(&kinds, players_n),
        }
    })
    // CPU seats (cpuhost lane): `--cpu 2:<row>` = seat 2 is the computer opponent on difficulty row <row>.
    .insert_resource(cpu::parse_cli(opt("--cpu").as_deref()))
    .init_resource::<cpu::CpuRuntime>()
    // Display options (display.rs): the game area inside the window, the monitor.
    .init_resource::<display::GameArea>()
    .init_resource::<display::CurrentMonitor>()
    .init_resource::<display::Screens>()
    .init_resource::<display::WindowedSize>()
    .init_resource::<Costumes>()
    .init_resource::<PendingInputs>()
    .init_resource::<CamPitchInput>()
    .init_resource::<hud::HudScale>()
    .init_resource::<hud::SkillInfoView>()
    .insert_resource(hud::SkillNotes::load(&cache.join("skills.json")))
    // Front end: screens, and what they offer (frontend/state.rs, discovery.rs).
    .insert_state(initial_state)
    .insert_resource(game_data)
    .insert_resource(SkillCatalog { skills: registered_skills, notes })
    .insert_resource(saved_arsenals)
    .insert_resource(MatchSetup::default())
    .init_resource::<hitstop::HitStop>()
    .init_resource::<SceneReady>()
    .init_resource::<MatchStats>()
    .init_resource::<frontend::title::TitleState>()
    .init_resource::<frontend::setup::SetupState>()
    .init_resource::<frontend::online::OnlineState>()
    .init_resource::<frontend::builder::BuilderState>()
    .add_plugins(sfx::plugin)
    .add_plugins(rumble::plugin)
    .add_plugins(frontend::sound::plugin)
    .add_plugins(mixer::plugin)
    .add_plugins(frontend::cloud::plugin)
    .add_plugins(frontend::ui::plugin)
    .add_plugins(aura_fx::plugin)
    .add_plugins(aura_pulse::plugin)
    .add_plugins(aura_glow::plugin)
    .add_plugins(shadows::plugin)
    .add_plugins(char_light::plugin)
    .add_plugins(cam_fade::plugin)
    .add_plugins(devtools::plugin)
    .add_plugins(online_match::plugin)
    .add_plugins(frontend::online::steam_plugin)
    .add_plugins(capsule_birth::plugin)
    .add_plugins(glow::plugin)
    // Dev only: `PD_GPU_TIMING=1` prints each render pass's GPU cost per frame at the end of a `PD_FRAME_STATS` run.
    .add_plugins(perf_probe::plugin)
    .add_plugins(single_thread_view_schedules)
    .add_systems(Startup, (menu::spawn_menu, display::spawn_bars_camera, cpu_light_clustering))
    // The Lighting and look options (look.rs): glow, shadow filter and map, tone curve, effect lights.
    .add_systems(Update, look::apply_look)
    // The game area first, so every layout system sees this frame's window.
    .add_systems(PreUpdate, (display::track_screens, display::track_monitor, display::update_area).chain())
    .add_systems(Update, (display::fit_cameras, display::apply_display, display::log_display))
    .add_systems(Startup, (setup, fx::spawn_fx, hud::spawn_hud).run_if(|cfg: Res<Config>| cfg.legacy))
    .add_systems(OnEnter(AppState::Title), frontend::title::spawn)
    .add_systems(Update, frontend::title::input.run_if(in_state(AppState::Title)).run_if(devtools::not_typing))
    .add_systems(OnEnter(AppState::Setup), (frontend::setup::spawn, frontend::charselect::spawn))
    .add_systems(
        Update,
        (frontend::setup::input.run_if(devtools::not_typing), frontend::charselect::sync_characters, frontend::charselect::build_graphs, frontend::charselect::attach_players, frontend::charselect::drive, frontend::charselect::orbit, frontend::charselect::apply_colours, frontend::charselect::reveal, frontend::setup::layout_rows, frontend::setup::layout_seats, frontend::setup::layout_shared)
            .chain()
            .run_if(in_state(AppState::Setup)),
    )
    .add_systems(OnEnter(AppState::Online), frontend::online::enter)
    .add_systems(OnExit(AppState::Online), frontend::online::cleanup)
    .add_systems(Update, (frontend::online::input.run_if(devtools::not_typing), frontend::online::begin_match, frontend::online::layout).chain().run_if(in_state(AppState::Online)))
    .add_systems(OnEnter(AppState::Arsenal), frontend::builder::spawn)
    .add_systems(Update, (frontend::builder::input.run_if(devtools::not_typing), frontend::builder::layout).chain().run_if(in_state(AppState::Arsenal)))
    .add_systems(OnEnter(AppState::Result), frontend::result::spawn)
    .add_systems(Update, frontend::result::input.run_if(in_state(AppState::Result)).run_if(devtools::not_typing))
    .add_systems(OnEnter(AppState::Match), (enter_match.run_if(|cfg: Res<Config>| !cfg.legacy), fx::spawn_fx.run_if(first_match), hud::spawn_hud.run_if(first_match), mark_scene_ready, reset_match_stats).chain())
    .add_systems(Update, (check_match_end, frontend::state::track_stats, frontend::script::match_tokens, frontend::script::test_ko).run_if(in_state(AppState::Match)))
    .add_systems(OnExit(AppState::Match), leave_match)
    // Global regardless of screen: the settings overlay (Esc/Start), matte
    // materials, and the screenshot/exit test harness.
    .add_systems(Update, (menu::menu_input.run_if(devtools::not_typing), menu::menu_view, matte_characters, stage_look::sky_without_fog, auto_screenshot, frame_stats, frontend::ui::update_fade, frontend::script::drive))
    .add_systems(FixedUpdate, (film_setup, cpu::drive_system, online_match::lockstep, step_sim, online_match::after_step, film_log).chain().run_if(in_state(AppState::Match)))
    .add_systems(Last, film_frame_marker)
    .add_systems(FixedUpdate, char_light::step_fade.run_if(in_state(AppState::Match)))
    .add_systems(PostUpdate, (dangle::setup_dangles, dangle::swing_dangles.after(bevy::transform::TransformSystems::Propagate)))
    // The sky's cloud layer drifts. PostUpdate, not Update: an asset write in
    // a normal system is deferred to the next frame's apply (Bevy b0004), which
    // an Added<> filter never sees again. See sky_scroll.rs.
    .add_systems(PostUpdate, sky_scroll::scroll)
    .add_systems(PostUpdate, sleep_empty_overlays)
    // A puff under each footfall, placed by the walk cycle's own clock.
    .add_systems(PostUpdate, footsteps::footsteps)
    // Hit-stop / KO slow-motion drives the clock itself. It must run BEFORE
    // the sim's FixedUpdate, not in Update: bevy_app 0.19's main schedule
    // order is First, PreUpdate, RunFixedMainLoop, Update, ... so a system
    // in Update is always one frame late -- the tick that would have been
    // frozen has already run. PreUpdate is the last variable-rate schedule
    // before the fixed loop, and a hit landed on the previous tick has
    // therefore already slowed the clock before `gather_input` reads the
    // pads and before FixedUpdate decides whether to run the sim at all.
    .add_systems(PreUpdate, hitstop::drive.run_if(in_state(AppState::Match)))
    .add_systems(
        Update,
        (
            seats::update_seats,
            gather_input.after(seats::update_seats).run_if(devtools::not_typing),
            sync_visuals,
            apply_costumes,
            fx::sync_combat_visuals,
            props_view::sync_props,
            (fx::preload_match_effects, fx::skill_effects, effects::spawn_trails, effects::attach_pieces, effects::tick_effects, effects::animate_pieces, effects::animate_trails).chain(),
            effects::warm_pipelines.after(fx::preload_match_effects),
            (hud::update_hud, hud::layout_hud, hud::hud_marker::update_lock_marker).chain().after(follow_cameras),
            follow_cameras,
            (cam_fade::prep_fade_materials, cam_fade::update_fade, char_light::gather_effect_lights, char_light::update_char_light, cam_fade::upload_fighter_block).chain().after(follow_cameras).after(apply_costumes).after(layout_viewports),
            (layout_viewports, apply_fov).chain(),
            build_graphs,
            attach_anim_slots,
            drive_animation,
        )
            .run_if(in_state(AppState::Match)),
    );
    if let Some(spec) = opt("--menu-script") {
        app.insert_resource(frontend::script::MenuScript::new(&spec));
    }
    // PD_SHOT_TICKS (screenshot runs only): every app update advances the
    // clock by exactly one sim tick, so one tick is stepped per rendered
    // frame and render-side clocks (effects, animation) advance by the same
    // amount however slow the machine is -- a capture after tick N shows the
    // same picture on every run (`auto_screenshot`).
    if config_shot_ticks().is_some() && has("--screenshot") {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / sim::HZ)));
    }
    music::build(&mut app);
    prophunt_view::plugin(&mut app);
    app.run();
}

/// The stage's own track from `cache/stages.json` ("music": "music/st06.wav",
/// matched by the stages lane â€” PIPELINE.md), else the default `music.wav`.
fn stage_music(stage: &str) -> String {
    let cache = std::env::args().nth(1).map(PathBuf::from).unwrap_or_default();
    let list: serde_json::Value = std::fs::read_to_string(cache.join("stages.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    list.as_array()
        .into_iter()
        .flatten()
        .find(|s| s["id"] == stage)
        .and_then(|s| s["music"].as_str())
        .filter(|m| cache.join(m).exists())
        .map_or_else(|| "music.wav".to_string(), String::from)
}

fn spawn_scene(commands: &mut Commands, assets: &AssetServer, cache: &std::path::Path, stage: &str, players: &seats::MatchPlayers, local: &seats::LocalViews) -> Vec<Handle<bevy::gltf::Gltf>> {
    let chars = players.chars();
    commands.insert_resource(Costumes { chars: chars.clone(), colours: players.colours() });
    props_view::PropView::spawn(commands, assets, cache, stage);
    commands.insert_resource(sky_scroll::SkyFields::load(cache, stage));
    commands.spawn((WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(format!("{stage}.glb")))), MatchScene));
    // The battle's track is chosen here, once (music.rs); the old per-stage WAV is the fallback.
    let (stage_id, fallback) = (stage.to_string(), stage_music(stage));
    commands.queue(move |w: &mut bevy::ecs::world::World| {
        w.resource_mut::<music::Music>().start_match(&stage_id, fallback);
    });
    // Characters are lit (normal-mapped); stages are unlit with their light
    // baked in (pd_convert, from the stage's light script), so these lights
    // only shade characters. Colour, strength and direction follow the
    // script's character light set where the cache has it (stage_look.rs).
    let look = stage_look::StageLook::load(cache, stage);
    let (sun_col, sun_k, sun_dir) = look.sun.unwrap_or((Color::WHITE, 1.0, -Vec3::new(0.4, 1.0, 0.6).normalize()));
    commands.spawn((
        DirectionalLight { color: sun_col, illuminance: env_f32("PD_SUN", LIGHT_SUN) * sun_k, shadow_maps_enabled: false, ..default() },
        Transform::default().looking_to(sun_dir, Vec3::Y),
        MatchScene,
    ));
    // Fighters are shaded by the original's SWBOS model (char_light.rs), fed
    // from the stage script's character set and each glb's specular colour.
    commands.insert_resource(char_light::CharLight::new(look.char_light, std::array::from_fn(|i| chars.get(i).map_or(Vec3::splat(0.5), |c| char_light::read_spec(cache, c)))));
    let (amb_col, amb_k) = look.ambient.unwrap_or((Color::WHITE, 1.0));
    commands.insert_resource(GlobalAmbientLight { color: amb_col, brightness: env_f32("PD_AMBIENT", LIGHT_AMBIENT) * amb_k, ..default() });
    // The shadow lights (stage volumes, fighters) and the stage's shadow look (shadows.rs).
    shadows::spawn_lights(commands, cache, stage);
    let mut gltf = Vec::new();
    // Only the slots viewed on this machine get a camera.
    let local_slots = local.slots(&players.roster());
    for i in 0..chars.len() {
        let path = format!("chars/{}.glb", chars[i]);
        gltf.push(assets.load(path.clone()));
        // `Visibility` here so `sync_visuals` can blink it during the
        // post-knock-down invulnerability (PROGRAM-RE.md "Knock-down physics").
        commands.spawn((WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(path))), Transform::default(), Visibility::default(), PlayerVisual(i), MatchScene));
        if !local_slots.contains(&i) {
            continue;
        }
        // 4x MSAA, stated explicitly rather than left to the default.
        //
        // `Msaa` IS a required component of `Camera3d` in Bevy 0.19 and its
        // default is `Sample4`, so the game has been rendering at 4x all
        // along (measured, not assumed: a probe reading the component off
        // both match cameras reported `Some(4)` with no MSAA code in this
        // codebase). Stating it here documents the choice and stops a future
        // reader -- including me -- "discovering" it as a hidden default and
        // offering to remove it.
        //
        // Do NOT substitute `Msaa::Off` for this as a faithfulness gesture.
        // The 2004 picture really was aliased, but 4x is what this game has
        // always shipped here and what Laurence plays, and it is what makes
        // the flat picture read as deliberate on a 4K panel rather than as
        // unfinished. An AA toggle was built and then parked (2026-09-29)
        // after the toggle's "off" state broke the HUD's compositing over the
        // match views on the first match -- `Some(Msaa::Off)` is not the same
        // as no `Msaa` component, because the component also governs MSAA
        // writeback between cameras sharing a target.
        let cam = commands.spawn((
            Camera3d::default(),
            bevy::render::view::Msaa::Sample4,
            Camera { order: i as isize, ..default() },
            Projection::Perspective(PerspectiveProjection { far: 200_000.0, fov: 0.9, ..default() }),
            Transform::default(),
            PlayerCamera(i),
            // Layer 0 = the shared world; 1 + i = things only this player sees;
            // GHOST_LAYER_BASE + j = player j's capsules as everyone else sees them.
            fx::camera_layers(i, chars.len()),
            MatchScene,
        )).id();
        // The stage's own fog (light script; only with PD_STAGE_FOG=1, stage_look.rs).
        if let Some(fog) = look.fog.clone() {
            commands.entity(cam).insert(fog);
        }
        // PD_TONEMAP=none: no tone-mapping, like the original's picture (A/B test switch).
        if std::env::var("PD_TONEMAP").is_ok_and(|v| v == "none") {
            commands.entity(cam).insert(bevy::core_pipeline::tonemapping::Tonemapping::None);
        }
    }
    gltf
}

/// TEST ONLY: `PD_TEST_SPAWN=x,z[,y][;x2,z2[,y2]]` moves P1 (and optionally
/// P2) to a spot on the stage floor, so a scripted capture can start at, say,
/// the foot of the Palace stairs (`--near` cannot place a player there). `y`
/// is the height to look down from (default: the start point's own height
/// plus a step); the player stands on the highest floor at or below it.
/// Unset, or a spot with no floor: the spawns are returned unchanged.
fn test_spawn(col: &Collision, spawns: Vec<Vec3>) -> Vec<Vec3> {
    let Ok(v) = std::env::var("PD_TEST_SPAWN") else { return spawns };
    let mut out = spawns.clone();
    for (i, spec) in v.split(';').take(spawns.len()).enumerate() {
        let n: Vec<f32> = spec.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [x, z, ref rest @ ..] = n[..] {
            let from = rest.first().copied().unwrap_or(spawns[i].y + 12.0);
            if let Some(y) = col.ground_below(x, z, from) {
                out[i] = Vec3::new(x, y, z);
            }
        }
    }
    out
}

/// Legacy CLI launch (`cfg.legacy`): spawns the scene from `Config` once at
/// Startup, exactly like before the front end existed.
fn setup(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<Config>, players: Res<seats::MatchPlayers>, local: Res<seats::LocalViews>) {
    let gltf = spawn_scene(&mut commands, &assets, &cfg.cache, &cfg.stage, &players, &local);
    commands.insert_resource(CharacterAnims::new(gltf));
}

/// A match started through the versus setup screen: tear down the previous
/// scene (if any â€” none on the very first match) and rebuild the World,
/// stage collision and 3D scene from what was picked.
#[allow(clippy::too_many_arguments)]
fn enter_match(mut commands: Commands, assets: Res<AssetServer>, cfg: Res<Config>, match_setup: Res<MatchSetup>, mut requested: ResMut<seats::RequestedPlayers>, mut world: ResMut<World>, mut col: ResMut<StageCollision>, mut cam_occ: ResMut<cam_occluders::CamOccluders>, local: Res<seats::LocalViews>, old: Query<Entity, With<MatchScene>>) {
    for e in &old {
        commands.entity(e).despawn();
    }
    // Who plays: the list the front end (or a test hook) asked for, else the
    // two fighters of the old setup screen (`MatchSetup`).
    let players = match requested.0.take() {
        Some(list) if !list.is_empty() => seats::MatchPlayers(list),
        _ => seats::MatchPlayers::free_for_all(&match_setup.chars, &match_setup.arsenals),
    };
    let col_bytes = std::fs::read(cfg.cache.join(format!("{}.pdcl", match_setup.stage))).expect("missing collision; run pd_convert collision");
    let collision = stairs_under::load(&col_bytes, &match_setup.stage);
    let layout = stage_layout(&cfg.cache, &match_setup.stage, players.len());
    let spawns: Vec<Vec3> = layout.as_ref().map_or_else(|| spread_spawns(pick_spawns(&collision, &cfg.cache.join(format!("{}.bounds.json", match_setup.stage)), None), players.len()), |l| l.spawns.clone());
    let spawns = test_spawn(&collision, spawns);
    let mut combat = combat::Combat::with_roster(load_skills(&cfg.cache.join("skills.json"), false), players.roster(), players.arsenals());
    combat.set_clip_ticks(load_clip_ticks(&cfg.cache, &players.chars()));
    combat.set_root_curves(load_root_curves(&cfg.cache, &players.chars()));
    combat.set_action_rows(load_action_rows(&cfg.cache.join("skills.json")));
    let (first, rows) = load_chain_rows(&cfg.cache.join("skills.json"));
    combat.set_chain_rows(first, rows);
    combat.set_shot_markers(load_shot_markers(&cfg.cache));
    combat.set_hand_paths(load_hand_paths(&cfg.cache, &players.chars()));
    combat.set_reaction_rows(load_reaction_rows(&cfg.cache.join("skills.json")));
    // The setup's rule, Level and hand, time limit, friendly fire and arsenal
    // kinds, all set explicitly (frontend/state.rs `apply_to_combat`).
    match_setup.apply_to_combat(&mut combat);
    if let Some(l) = &layout {
        combat.set_capsule_spots(l.capsule_spots.clone());
        combat.props = l.props.clone();
    }
    let mut m = Match::with_spawns(&spawns, combat);
    m.jump_zones = layout.map(|l| l.jump_zones).unwrap_or_default();
    set_dodges(&mut m, &cfg.cache, &players.chars());
    // Prop Hunt's rounds and roles (other modes add nothing).
    match_setup.start_mode(&mut m);
    *world = World(m);
    *cam_occ = cam_occluders::CamOccluders::load(&cfg.cache, &match_setup.stage);
    *col = StageCollision(collision);
    let gltf = spawn_scene(&mut commands, &assets, &cfg.cache, &match_setup.stage, &players, &local);
    commands.insert_resource(CharacterAnims::new(gltf));
    commands.insert_resource(players);
}

/// Leaving a match (to the result screen or the title) removes its stage,
/// characters, cameras and music; the next match builds fresh ones.
fn leave_match(mut commands: Commands, old: Query<Entity, With<MatchScene>>) {
    for e in &old {
        commands.entity(e).despawn();
    }
}

/// The HUD and fx assets/cameras (`hud::spawn_hud`, `fx::spawn_fx`) are
/// spawned once and reused by every match; only the interactive front end's
/// first match needs to spawn them (the legacy CLI path does it at Startup).
fn first_match(cfg: Res<Config>, ready: Res<SceneReady>) -> bool {
    !cfg.legacy && !ready.0
}

fn mark_scene_ready(mut ready: ResMut<SceneReady>) {
    ready.0 = true;
}

/// A new match (including a rematch from Setup) starts its "fun stats"
/// fresh; rounds restarting within the same match don't re-enter this state
/// so they keep accumulating (`frontend::state::track_stats`, docs GAME.md
/// "Result screen").
fn reset_match_stats(mut stats: ResMut<MatchStats>) {
    *stats = MatchStats::default();
}

/// Ends the match once a player reaches the rounds-to-win target (the
/// versus setup screen's default, "Endless", never does â€” today's
/// behaviour, rounds just keep restarting).
fn check_match_end(cfg: Res<Config>, match_setup: Res<MatchSetup>, world: Res<World>, mut next: ResMut<NextState<AppState>>) {
    // Direct-to-match test runs (--demo...) keep restarting rounds.
    if cfg.legacy {
        return;
    }
    // Prop Hunt ends on its own terms (first to 5 kills, or the round limit), after its last standings card.
    if let Some(ph) = &world.0.prophunt {
        if ph.match_over() {
            next.set(AppState::Result);
        }
        return;
    }
    let Some(target) = match_setup.rounds_to_win else { return };
    // After the round-end banner has shown (the round restarts at 1). A round
    // won by a player scores for their team (one player per round is credited,
    // so a team's score is `team_wins`), and the match ends when that team
    // reaches the target; a drawn single battle ends as a draw
    // (`frontend::state::match_ended`).
    if frontend::state::match_ended(&world.0.combat, target) {
        next.set(AppState::Result);
    }
}

/// What the stage's own layout (`stNN.props.json`, pd_convert stages) says
/// for a match of `players`: the first start set's first `players` start
/// points (most stages carry four per set -- two players use the first two,
/// as before -- and the stages with smaller sets continue into the next set),
/// each start's three capsule spots, and the jump zones.
struct StageLayout {
    spawns: Vec<Vec3>,
    capsule_spots: Vec<Vec<Vec3>>,
    jump_zones: Vec<(Vec3, Vec3, f32)>,
    /// The stage's objects (props.rs).
    props: Vec<props::Prop>,
}

fn stage_layout(cache: &std::path::Path, stage: &str, players: usize) -> Option<StageLayout> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(cache.join(format!("{stage}.props.json"))).ok()?).ok()?;
    let vec3 = |a: &serde_json::Value| Some(Vec3::new(a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32));
    let starts: Vec<&serde_json::Value> = v["start_sets"].as_array()?.iter().flat_map(|set| set["starts"].as_array().into_iter().flatten()).take(players.max(2)).collect();
    // Fewer start points than players (or than the two a duel needs): the
    // caller spreads players over the floor instead.
    if starts.len() < players.max(2) {
        return None;
    }
    let spawns = starts.iter().take(players).map(|s| vec3(&s["position"])).collect::<Option<Vec<_>>>()?;
    let capsule_spots = starts.iter().take(players).map(|s| s["capsules"].as_array().into_iter().flatten().filter_map(vec3).collect()).collect();
    let jump_zones = v["objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["class"] == 4 && o["kind"] == 16)
        .filter_map(|o| Some((vec3(&o["bbox_min"])?, vec3(&o["bbox_max"])?, o["param"].as_f64()? as f32)))
        .collect();
    Some(StageLayout { spawns, capsule_spots, jump_zones, props: props::from_json(&v) })
}

/// Two spawn points on the most common standable level near the stage
/// centre, facing each other across it (when the stage has no layout file,
/// or for `--near`). `floor_hint`: survey the level nearest this height
/// instead (the floor under an original start point).
fn pick_spawns(col: &Collision, bounds: &std::path::Path, floor_hint: Option<f32>) -> [Vec3; 2] {
    let v: serde_json::Value = std::fs::read_to_string(bounds).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let f = |k: &str, i: usize| v[k][i].as_f64().unwrap_or(0.0) as f32;
    let (lo, hi) = (Vec3::new(f("min", 0), 0.0, f("min", 2)), Vec3::new(f("max", 0), 0.0, f("max", 2)));
    // Survey the top surface on a grid and bucket by height (ignoring the
    // sea and anything far below the structure).
    let step = 12.5;
    let mut buckets: std::collections::HashMap<i32, Vec<Vec3>> = Default::default();
    let mut x = lo.x;
    while x <= hi.x {
        let mut z = lo.z;
        while z <= hi.z {
            if let Some(y) = col.ground_below(x, z, floor_hint.map_or(f("max", 1), |h| h + 1.0)) {
                if floor_hint.map_or(y > 50.0, |h| (y - h).abs() < 5.0) {
                    buckets.entry((y / 10.0).round() as i32).or_default().push(Vec3::new(x, y, z));
                }
            }
            z += step;
        }
        x += step;
    }
    // The arena floor is the height with the most open area.
    let Some(floor) = buckets.values().max_by_key(|v| v.len()) else {
        return [Vec3::new(-70.0, 0.0, 0.0), Vec3::new(70.0, 0.0, 0.0)];
    };
    let c = floor.iter().fold(Vec3::ZERO, |a, p| a + *p) / floor.len() as f32;
    // Two floor points ~100-160 apart, clear of walls, that can see each
    // other at head height; prefer pairs whose midpoint is near the centre.
    let head = Vec3::Y * 16.0;
    let open = |p: Vec3| col.push_out_of_walls(p, 10.0, 7.0, 20.0).distance(p) < 0.1;
    let mut pts: Vec<Vec3> = floor.iter().copied().filter(|p| open(*p)).collect();
    pts.sort_by(|a, b| a.distance_squared(c).total_cmp(&b.distance_squared(c)));
    pts.truncate(400);
    let mut best: Option<(f32, [Vec3; 2])> = None;
    for (i, a) in pts.iter().enumerate() {
        for b in &pts[i + 1..] {
            let d = a.distance(*b);
            if !(100.0..=160.0).contains(&d) || (a.y - b.y).abs() > 2.0 {
                continue;
            }
            let dir = (*b - *a) / d;
            if col.raycast(*a + head, dir, d).is_some() {
                continue;
            }
            let score = ((*a + *b) * 0.5).distance(c);
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, [*a, *b]));
            }
        }
    }
    // No clear pair: any two open floor points, else any two floor points.
    let pool = if pts.is_empty() { floor.clone() } else { pts };
    best.map(|(_, p)| p).unwrap_or([pool[0], pool[pool.len() / 2]])
}

/// `n` spawn points from the surveyed duel pair: one or two players use the
/// pair as it is; three or four stand round the pair's midpoint on the same
/// floor, the pair's distance apart (for stages with no start-point layout
/// and for `--near` test runs).
fn spread_spawns(pair: [Vec3; 2], n: usize) -> Vec<Vec3> {
    if n <= 2 {
        return pair[..n.max(1)].to_vec();
    }
    let mid = (pair[0] + pair[1]) * 0.5;
    let r = pair[0].distance(pair[1]) * 0.5;
    let a0 = (pair[0].z - mid.z).atan2(pair[0].x - mid.x);
    (0..n).map(|k| {
        let a = a0 + std::f32::consts::TAU * k as f32 / n as f32;
        Vec3::new(mid.x + r * a.cos(), pair[0].y, mid.z + r * a.sin())
    }).collect()
}

#[allow(clippy::too_many_arguments)]
fn gather_input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    seats: Res<seats::Seats>,
    cfg: Res<Config>,
    time: Res<Time>,
    world: Res<World>,
    mut pending: ResMut<PendingInputs>,
    mut info: ResMut<hud::SkillInfoView>,
    mut demo: Local<[DemoWalker; combat::MAX_PLAYERS]>,
    mut pitch_in: ResMut<CamPitchInput>,
    menu: Res<menu::MenuState>,
    settings: Option<Res<menu::Settings>>,
) {
    const N: usize = combat::MAX_PLAYERS;
    // The roster's players (slots 0..n): the keyboard plays seat 1 only (decided
    // 2026-10-01); a two-player match also keeps the old half-keyboard for
    // seat 2 (Enter, Right Shift, numpad skills -- no movement) so a lone
    // tester can still poke the other fighter. Pads play the seats they hold.
    let players = world.0.combat.roster.count();
    let key_axes = |l: KeyCode, r: KeyCode, d: KeyCode, u: KeyCode| {
        let x = keys.pressed(r) as i32 - keys.pressed(l) as i32;
        let y = keys.pressed(u) as i32 - keys.pressed(d) as i32;
        Vec2::new(x as f32, y as f32)
    };
    // Seat 2 moves with a gamepad only: the arrow keys are seat 1's D-pad.
    let mut axes = [Vec2::ZERO; N];
    axes[0] = key_axes(KeyCode::KeyA, KeyCode::KeyD, KeyCode::KeyS, KeyCode::KeyW);
    // Held, or tapped since the last frame (a quick tap between two slow
    // frames would otherwise never be seen as held).
    let key = |k: KeyCode| keys.pressed(k) || keys.just_pressed(k);
    // The buttons go through the player's mapping (Options > CONTROLS > BUTTON MAPPING,
    // `bindings.rs`; the default is the old fixed layout). Shared by every pad; the keys are
    // keyboard player 1's. Only the resulting `Input` reaches the sim.
    use bindings::Action;
    let map = settings.as_deref().map(|s| s.bindings).unwrap_or_default();
    let bound = |a: Action| map.key(a).is_some_and(key);
    let half = players == 2;
    let mut jump = [false; N];
    let mut lock = [false; N];
    (jump[0], lock[0]) = (bound(Action::Dodge) || bound(Action::Dodge2), bound(Action::LockOn));
    if half {
        (jump[1], lock[1]) = (key(KeyCode::Enter), key(KeyCode::ShiftRight));
    }
    let mut cam = [0.0f32; N];
    cam[0] = (keys.pressed(KeyCode::KeyE) as i32 - keys.pressed(KeyCode::KeyQ) as i32) as f32;
    let mut recenter = [false; N];
    // Keyboard seat 1: R / F look up / down (free camera); T switches the lock target.
    let mut pitch = [0.0f32; N];
    pitch[0] = (keys.pressed(KeyCode::KeyR) as i32 - keys.pressed(KeyCode::KeyF) as i32) as f32;
    let mut switch = [false; N];
    switch[0] = bound(Action::SwitchTarget);
    let bits = |held: [bool; 4]| held.iter().enumerate().fold(0u8, |b, (k, &h)| b | ((h as u8) << k));
    // Skill slots follow the dial: left, right, top, bottom. The D-pad (and
    // P1's arrow keys) shows the skill in that direction's slot, like the
    // original ("D-pad direction corresponds to the skill button").
    let dpad = |held: [bool; 4]| held.iter().position(|&h| h);
    info.0 = [None; N];
    info.0[0] = dpad([KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown].map(|k| keys.pressed(k)));
    // Test switch for screenshots: PD_INFO_SLOT=2 holds P1's D-pad on slot 2.
    if let Some(k) = std::env::var("PD_INFO_SLOT").ok().and_then(|v| v.parse().ok()) {
        info.0[0] = Some(k);
    }
    let mut skills = [0u8; N];
    skills[0] = bits([Action::SkillLeft, Action::SkillRight, Action::SkillTop, Action::SkillBottom].map(|a| map.key(a).is_some_and(|k| keys.pressed(k))));
    if half {
        skills[1] = bits([KeyCode::Numpad1, KeyCode::Numpad2, KeyCode::Numpad4, KeyCode::Numpad5].map(|k| keys.pressed(k)));
    }
    for (i, pad) in (0..N).filter_map(|i| seats.pad[i].and_then(|e| pads.get(e).ok()).map(|pad| (i, pad))) {
        let s = pad.left_stick();
        if s.length() > 0.2 {
            axes[i] = s;
        }
        // The original's layout (its own tutorial text): click the left stick
        // to jump, right trigger locks on, right stick moves the camera,
        // skills on X B Y A = dial left, right, top, bottom.
        let button = |b: GamepadButton| pad.pressed(b) || pad.just_pressed(b);
        // Xbox One / PC re-release: the left bumper dodges too.
        jump[i] |= button(map.pad(Action::Dodge)) || button(map.pad(Action::Dodge2));
        lock[i] |= button(map.pad(Action::LockOn));
        // Switch target: the program's input mask 0x400 (`0x1402531d0`); which
        // physical button feeds it is unread (audit R1: the pad-to-mask
        // conversion is behind Windows.Gaming.Input calls), so this is the LEFT
        // TRIGGER the fan guides name (GAME.md "Not built") -- UNCONFIRMED.
        switch[i] |= pad.pressed(map.pad(Action::SwitchTarget));
        let r = pad.right_stick();
        if r.x.abs() > 0.2 {
            cam[i] = r.x;
        }
        if r.y.abs() > 0.2 {
            pitch[i] = r.y;
        }
        recenter[i] |= pad.pressed(GamepadButton::RightThumb);
        skills[i] |= bits([Action::SkillLeft, Action::SkillRight, Action::SkillTop, Action::SkillBottom].map(|a| pad.pressed(map.pad(a))));
        info.0[i] = info.0[i].or(dpad([GamepadButton::DPadLeft, GamepadButton::DPadRight, GamepadButton::DPadUp, GamepadButton::DPadDown].map(|b| pad.pressed(b))));
    }
    // The Options overlay's CONTROLS tab: camera sensitivity and invert Y on the players' own
    // look input (keyboard and pads; the scripted test switches below are left alone). The turn
    // is shaped here, before it is quantised into `Input::cam`, so an online match sends the
    // shaped number and both PCs step on it; the pitch never leaves this PC (`CamPitchInput`).
    if let Some(s) = settings.as_deref() {
        for i in 0..N {
            (cam[i], pitch[i]) = s.shape_look(cam[i], pitch[i]);
        }
    }
    if cfg.demo {
        // Scripted inputs for unattended test runs (not an opponent AI):
        // each player runs to their nearest capsule while a slot is free,
        // otherwise circles the other, and taps a filled slot twice a second.
        let t = time.elapsed_secs();
        let m = &world.0;
        for i in m.combat.roster.slots() {
            // The fighter they circle is whoever the lock target is (the other
            // one, in a duel).
            let (me, other) = (m.players[i].pos, m.players[m.combat.aim_at(i, m.players[i].target)].pos);
            let f = &m.combat.fighters[i];
            let goal = m.combat.capsules.iter().filter(|c| c.owner == i && c.pos.distance(me) < 90.0).min_by(|a, b| a.pos.distance_squared(me).total_cmp(&b.pos.distance_squared(me)));
            let side = if i % 2 == 0 { 0.7 } else { -0.7 };
            // Stuck against something while trying to close in: jump and
            // side-step round it for a moment (alternating sides).
            let d = &mut demo[i];
            let tick = m.tick;
            if tick >= d.check_at {
                let moved = Vec3::new(me.x - d.last.x, 0.0, me.z - d.last.z).length();
                if moved < 6.0 && d.wanted && tick >= d.dodge_until {
                    d.dodge_until = tick + 50;
                    d.dodge_side = if d.dodge_side > 0.0 { -1.0 } else { 1.0 };
                }
                (d.last, d.check_at) = (me, tick + 30);
            }
            let dodging = tick < d.dodge_until;
            if i == 0 && tick % 120 == 0 && std::env::var_os("PD_DEMO_LOG").is_some() {
                eprintln!("[demo] {:.0}s apart {:.0} health {} / {}", tick as f32 / 60.0, me.distance(other), m.combat.fighters[0].health, m.combat.fighters[1].health);
            }
            d.wanted = f.action == combat::Action::Ready && !m.combat.counting_down();
            if dodging {
                axes[i] = Vec2::new(d.dodge_side, 0.3);
                jump[i] = tick + 40 > d.dodge_until && tick + 30 < d.dodge_until;
            }
            axes[i] = if dodging { axes[i] } else { match goal {
                Some(c) if f.slots.iter().any(Option::is_none) => {
                    let fwd = sim::toward(me, other);
                    let d = sim::toward(me, c.pos);
                    Vec2::new(d.dot(Vec3::new(-fwd.z, 0.0, fwd.x)), d.dot(fwd))
                }
                // Close in, then circle at short range.
                _ if me.distance(other) > 60.0 => Vec2::new(side * 0.4, 1.0),
                _ => Vec2::new(side, 0.0),
            } };
            // PD_DEMO_DODGE=1: also dodge every 1.5 s (screenshot tests of the dodge moves).
            if std::env::var_os("PD_DEMO_DODGE").is_some() && tick % 90 < 3 {
                jump[i] = true;
            }
            let tap = (t * 2.0 + i as f32 * 0.5).fract() < 0.2;
            let filled: Vec<usize> = (0..4).filter(|&k| f.slots[k].is_some()).collect();
            let empty = (0..4).find(|&k| f.slots[k].is_none());
            skills[i] = match (m.combat.capsule_in_reach(i, &m.players[i]), empty) {
                // Standing at a capsule with a free slot: capture it there.
                (Some(_), Some(k)) if tap => 1 << k,
                (Some(_), Some(_)) => 0,
                // Build Level first (aura particles), then use whatever's there.
                _ if tap && f.level < 6 && f.slots.iter().any(|s| *s == Some(combat::AURA_PARTICLE)) => {
                    1 << f.slots.iter().position(|s| *s == Some(combat::AURA_PARTICLE)).unwrap_or(0)
                }
                _ => match filled.get(((t * 2.0) as usize) % filled.len().max(1)) {
                    Some(&k) if tap => 1 << k,
                    _ => 0,
                },
            };
        }
    }
    // Tests: PD_TEST_PRESS=<tick>,<slot>[,<walk ticks>] -- P1 walks toward
    // P2 until <walk ticks>, then presses <slot> for a few ticks from <tick>.
    if let Some(v) = std::env::var("PD_TEST_PRESS").ok() {
        let n: Vec<u64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        let tick = world.0.tick;
        if let [at, slot, ref rest @ ..] = n[..] {
            if rest.first().is_some_and(|&w| tick < w) {
                axes[0] = Vec2::new(0.0, 1.0);
            }
            // Widened from 3 to 10 ticks (cast-timing lane): screenshot runs
            // can skip a slow frame, and 3 ticks is easy to miss now that
            // presses matter to the row's own fire/dash timing.
            if tick >= at && tick < at + 10 {
                skills[0] |= 1 << slot;
            }
        }
    }
    // Film comparison (`engine/re/film_compare.py`): PD_FILM_OPP=<tick>,<slot> -- P2 presses
    // <slot> for 10 ticks from <tick> (the opponent's skill in a defence take).
    if let Some(v) = std::env::var("PD_FILM_OPP").ok() {
        let n: Vec<u64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        if let [at, slot] = n[..] {
            if world.0.tick >= at && world.0.tick < at + 10 && players > 1 {
                skills[1] |= 1 << slot;
            }
        }
    }
    // Tests: PD_TEST_PRESS_B=<tick>,<slot> -- a second scripted press by P1 (a follow-up
    // skill, e.g. a Pursuit once the first skill has knocked the foe down).
    if let Some(v) = std::env::var("PD_TEST_PRESS_B").ok() {
        let n: Vec<u64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        if let [at, slot] = n[..] {
            if world.0.tick >= at && world.0.tick < at + 10 {
                skills[0] |= 1 << slot;
            }
        }
    }
    // Tests: PD_TEST_TAPS=<slot>,<tick>,<tick>,... -- P1 taps <slot> for 2 ticks from each listed
    // tick (a chain's follow-up presses, lane combos: High-speed Punch / Rapid Cannon).
    // PD_TEST_TAPS2 / 3 / 4 tap for players 2 / 3 / 4 the same way (Prop Hunt hider captures).
    for (seat, var) in [(0, "PD_TEST_TAPS"), (1, "PD_TEST_TAPS2"), (2, "PD_TEST_TAPS3"), (3, "PD_TEST_TAPS4")] {
        if let Some(v) = std::env::var(var).ok() {
            let n: Vec<u64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            if let [slot, ref ticks @ ..] = n[..] {
                if ticks.iter().any(|&t| world.0.tick >= t && world.0.tick < t + 2) {
                    skills[seat] |= 1 << slot;
                }
            }
        }
    }
    // Tests: PD_TEST_STICK=<mag>,<angle_deg>[,<start_tick>[,<end_tick>[,<wobble_deg>[,<wobble_period>[,<mag_wobble>]]]]] --
    // P1's left stick held at this magnitude (0..1) and angle (degrees; 0 =
    // forward toward the target/camera, 90 = right, 180 = back -- sim.rs's
    // own x=right/y=forward frame, so this is camera- or opponent-relative
    // same as any other stick push) from <start_tick> (default 0) up to but
    // excluding <end_tick> (default: never) -- e.g. to prove a strafe's
    // facing returns to the target once the stick is released. The optional
    // wobble (a real thumb trembles: angle swings +/-<wobble_deg> around the
    // base angle, magnitude swings +/-<mag_wobble>, both with a
    // <wobble_period>-tick sine period) reproduces stick-tremor bugs no
    // static angle can. PD_TEST_STICK2 / 3 / 4 drive players 2 / 3 / 4 the same
    // way (four-player captures, lane fourview).
    for (seat, var) in [(0, "PD_TEST_STICK"), (1, "PD_TEST_STICK2"), (2, "PD_TEST_STICK3"), (3, "PD_TEST_STICK4")] {
      if let Some(v) = std::env::var(var).ok() {
        // Lane wall-dissolve capture control: `;` separates independent
        // windows (e.g. back to a wall, out, back again), each parsed as
        // above; later windows win while they overlap.
        for spec in v.split(';') {
            let n: Vec<f32> = spec.split(',').filter_map(|x| x.parse().ok()).collect();
        if let [mag, angle_deg, ref rest @ ..] = n[..] {
            let start = rest.first().copied().unwrap_or(0.0) as u64;
            let end = rest.get(1).copied().map(|e| e as u64);
            if world.0.tick >= start && end.is_none_or(|e| world.0.tick < e) {
                let wobble = rest.get(2).copied().unwrap_or(0.0);
                let period = rest.get(3).copied().unwrap_or(1.0).max(1.0);
                let mag_wobble = rest.get(4).copied().unwrap_or(0.0);
                let phase = std::f32::consts::TAU * (world.0.tick - start) as f32 / period;
                let angle = angle_deg + wobble * phase.sin();
                let m = (mag + mag_wobble * phase.sin()).max(0.0);
                let a = angle.to_radians();
                axes[seat] = Vec2::new(m * a.sin(), m * a.cos());
            }
        }
        }
      }
    }
    // PD_TRACE_SCRIPT=<file>: P1's stick, jump/dodge and lock from a
    // `tick,stick_x,stick_y,buttons` script (docs/GOLDEN-TRACE.md). The run
    // ends after the last row so every scenario can be batched.
    if let Some(path) = std::env::var_os("PD_TRACE_SCRIPT") {
        static SCRIPT: std::sync::OnceLock<Vec<trace::ScriptRow>> = std::sync::OnceLock::new();
        let rows = SCRIPT.get_or_init(|| trace::parse_script(&std::fs::read_to_string(&path).unwrap_or_default()));
        if let Some(last) = rows.last() {
            if world.0.tick > last.tick {
                std::process::exit(0);
            }
        }
        let r = trace::script_at(rows, world.0.tick).unwrap_or(trace::ScriptRow { tick: world.0.tick, x: 0.0, y: 0.0, buttons: 0 });
        axes[0] = Vec2::new(r.x, r.y);
        jump[0] = r.buttons & (1 | 4) != 0;
        lock[0] = r.buttons & 2 != 0;
    }
    // Screenshot tests: PD_TEST_PITCH=1 / -1 holds P1's look up / down.
    if let Some(v) = std::env::var("PD_TEST_PITCH").ok().and_then(|v| v.parse::<f32>().ok()) {
        pitch[0] = v;
    }
    // Tests: PD_TEST_LOCK=<tick>[,<ticks>] -- holds P1's lock-on button from
    // <tick> for <ticks> (default 3) so tick-exact runs can toggle lock-on
    // mid-run (the sim toggles on the press edge, so this must be released
    // again -- it is a hold, not a latch).
    if let Some(v) = std::env::var("PD_TEST_LOCK").ok() {
        let n: Vec<u64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        if let [at, ref rest @ ..] = n[..] {
            let held = rest.first().copied().unwrap_or(3);
            let tick = world.0.tick;
            if tick >= at && tick < at + held {
                lock[0] = true;
            }
        }
    }
    // Tests: PD_TEST_SWITCH=<player 1-4>,<tick>[,<ticks>] -- holds that
    // player's switch-target button from <tick> for <ticks> (default 3): one
    // press = one rising edge = the lock moves to the next valid target.
    if let Some(v) = std::env::var("PD_TEST_SWITCH").ok() {
        let n: Vec<u64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        if let [who, at, ref rest @ ..] = n[..] {
            let held = rest.first().copied().unwrap_or(3);
            let tick = world.0.tick;
            if (1..=N as u64).contains(&who) && tick >= at && tick < at + held {
                switch[who as usize - 1] = true;
            }
        }
    }
    // Tests: PD_TEST_CAM=<tilt>,<start_tick>[,<end_tick>] -- holds P1's
    // free-camera turn (right stick x, -1..1) over that tick window, so a
    // tick-exact run can aim the free camera away before locking on.
    if let Some(v) = std::env::var("PD_TEST_CAM").ok() {
        let n: Vec<f32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        if let [tilt, ref rest @ ..] = n[..] {
            let start = rest.first().copied().unwrap_or(0.0) as u64;
            let end = rest.get(1).copied().map(|e| e as u64);
            let tick = world.0.tick;
            if tick >= start && end.is_none_or(|e| tick < e) {
                cam[0] = tilt;
            }
        }
    }
    pitch_in.0 = pitch;
    // PD_MOVE_LOG=1: per-tick gait/motion/clip for P1 (moves lane debugging).
    if std::env::var_os("PD_MOVE_LOG").is_some() {
        let p = &world.0.players[0];
        eprintln!(
            "[move] t {} stick {:?} mag {:.2} locked {} gait {:?} motion {:?} clip {}",
            world.0.tick,
            axes[0],
            axes[0].length(),
            p.locked,
            p.gait,
            p.motion,
            clip_for(&world.0, 0)
        );
    }
    pending.0 = std::array::from_fn(|i| Input {
        skills: skills[i],
        cam: Input::cam_value(cam[i]).0,
        cam_fast: Input::cam_value(cam[i]).1,
        recenter: recenter[i],
        switch_target: switch[i],
        ..Input::from_axes(axes[i].x, axes[i].y, jump[i], lock[i])
    });
    // Paused (the pause menu is open, or a seat's pad dropped): the pads and
    // keys belong to the menu. Nothing reaches the sim or the skill-info view
    // (the D-pad drives the menu cursor, not "show my bound skills"), and the
    // free camera stops following the right stick.
    if menu.open || seats.paused(players) {
        silence_while_paused(&mut info, &mut pitch_in, &mut pending);
    }
    // PD_INPUT_LOG=1: what the game receives, twice a second (control debugging).
    let busy = pending.0[0] != Input::default() || !world.0.players[0].grounded || pads.iter().any(|g| g.right_stick().length() > 0.2 || g.pressed(GamepadButton::LeftThumb));
    if std::env::var_os("PD_INPUT_LOG").is_some() && (busy || (time.elapsed_secs() * 2.0).fract() < (time.delta_secs() * 2.0)) {
        let p = &world.0.players[0];
        let pads: Vec<String> = pads.iter().map(|g| format!("L{:?} R{:?} L3 {} RT {}", g.left_stick(), g.right_stick(), g.pressed(GamepadButton::LeftThumb), g.pressed(GamepadButton::RightTrigger2))).collect();
        eprintln!(
            "[input] t {:.1} pads {} {:?} | P1 in {:?} | P1 y {:.1} grounded {} locked {} cam_yaw {:.2} action {:?}",
            time.elapsed_secs(), pads.len(), pads, pending.0[0], p.pos.y, p.grounded, p.locked, p.cam_yaw, world.0.combat.fighters[0].action
        );
    }
}

/// Drops every in-match input for a paused frame (`gather_input`): the
/// D-pad's skill-info display, the free-camera look and the sim's pending
/// inputs. Pure so the pause test can pin it.
fn silence_while_paused(info: &mut hud::SkillInfoView, pitch: &mut CamPitchInput, pending: &mut PendingInputs) {
    info.0 = [None; combat::MAX_PLAYERS];
    pitch.0 = [0.0; combat::MAX_PLAYERS];
    pending.0 = [Input::default(); combat::MAX_PLAYERS];
}

/// The demo script's memory for one player: where they were half a second
/// ago, and a side-step in progress when they got stuck.
#[derive(Default, Clone, Copy)]
struct DemoWalker {
    last: Vec3,
    check_at: u64,
    wanted: bool,
    dodge_until: u64,
    dodge_side: f32,
}

fn step_sim(mut world: ResMut<World>, pending: Res<PendingInputs>, mut col: ResMut<StageCollision>, menu: Res<menu::MenuState>, seats: Res<seats::Seats>, mut dev: ResMut<devtools::DevTools>, online: Res<online_match::OnlineGate>, mut trace_state: Local<trace::TraceState>) {
    // Online: the tick runs only once both PCs' inputs are in, and the
    // settings menu does not stop it (the other PC keeps playing).
    if online.active && !online.ready {
        return;
    }
    // The match pauses while the settings menu is open, while a seat's
    // pad has dropped (it resumes when a pad takes the seat back, `Seats`),
    // and when the dev tools pause it (F2 / F3 single step / an open F8 bug
    // report, `devtools.rs`).
    if (menu.open && !online.active) || seats.paused(world.0.combat.roster.count()) || !dev.take_tick() {
        return;
    }
    world.0.step(pending.0, &mut col.0);
    if std::env::var_os("PD_TRACE").is_some() {
        let i = pending.0[0];
        let buttons = i.jump as u8 | (i.lock as u8) << 1;
        let stick = [i.stick[0] as f32 / 127.0, i.stick[1] as f32 / 127.0];
        trace::write_row(&mut trace_state, &world.0, stick, buttons, clip_for(&world.0, 0));
    }
}

/// Where a player is drawn this frame: blended between the last two
/// simulation ticks, so motion is smooth at any refresh rate.
fn render_pose(world: &World, fixed: &Time<Fixed>, i: usize) -> (Vec3, f32) {
    let p = &world.0.players[i];
    let a = fixed.overstep_fraction();
    let dyaw = (p.yaw - p.prev_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    (p.prev_pos.lerp(p.pos, a), p.prev_yaw + dyaw * a)
}

/// Whether the model faces exactly the sim's facing this frame (blended
/// between ticks, `render_pose`). The program draws the model at its stored
/// facing every tick with no smoothing (`FUN_14024ec80`: the model matrix is
/// built from `+0x12e68`), and every state's facing already turns at the
/// program's rate or jumps only where the program's does: pivot entry,
/// release and hand-off (masked by the clips themselves), a cast's turn toward
/// its aim, and now the reactions -- a knock-down's launch turns the body to
/// face (or turn its back on) the attacker in one write, and a wall hit or a
/// big landing turns it again (`Combat::start_knockdown`, `knock_flight`);
/// hits, collapses and get-ups leave the facing alone. So nothing is eased.
fn faces_sim_exactly(_motion: Motion) -> bool {
    true
}

/// The model's yaw this frame: the sim's (blended) facing, or an ease toward
/// it while the model is still catching up after an eased state. Returns
/// the new rotation and whether it is still catching up.
fn model_rotation(cur: Quat, sim_yaw: f32, exact: bool, catching_up: bool, dt: f32) -> (Quat, bool) {
    let target = Quat::from_rotation_y(sim_yaw);
    if exact && !catching_up {
        return (target, false);
    }
    let k = 1.0 - (-14.0 * dt).exp();
    let next = cur.slerp(target, k);
    (next, !exact || next.angle_between(target) > 1f32.to_radians())
}

fn sync_visuals(world: Res<World>, fixed: Res<Time<Fixed>>, time: Res<Time>, mut catching_up: Local<[bool; combat::MAX_PLAYERS]>, mut q: Query<(&PlayerVisual, &mut Transform, &mut Visibility)>) {
    for (v, mut t, mut vis) in &mut q {
        let (pos, yaw) = render_pose(&world, &fixed, v.0);
        t.translation = pos;
        let p = &world.0.players[v.0];
        let (rot, still) = model_rotation(t.rotation, yaw, faces_sim_exactly(p.motion), catching_up[v.0], time.delta_secs());
        t.rotation = rot;
        catching_up[v.0] = still;
        // The program draws on even protect-timer values only (`anim_args`).
        let blinking = anim_args::protect_hides_model(world.0.combat.fighters[v.0].invuln_ticks);
        // Prop Hunt: a disguised hider's fighter is not drawn at all (no body, shadow or aura); `prophunt_view` draws
        // the object they became in its place.
        let disguised = prophunt_view::disguised(&world.0, v.0);
        *vis = if blinking || disguised { Visibility::Hidden } else { Visibility::Inherited };
        // PD_FACE_LOG=1: P1's sim facing, the model's yaw and the clip's own
        // baked hip yaw (pc00a table: the TURN_* curve at the pivot's clock,
        // WALK/RUN's first-frame value otherwise), and their sum -- the body's
        // rendered facing, which must not jump at a pivot's entry or hand-off.
        if v.0 == 0 && std::env::var_os("PD_FACE_LOG").is_some() {
            let (visual_yaw, _, _) = t.rotation.to_euler(EulerRot::YXZ);
            let clip_now = match (p.motion, p.pivot()) {
                (Motion::Strafe(_), Some(pv)) => clip_root_yaw(strafe_clip(pv.dir, pv.gait), pv.time / 30.0),
                (Motion::Walk, _) => gait_clip_yaw(Gait::Walk),
                (Motion::Run, _) => gait_clip_yaw(Gait::Run),
                _ => 0.0,
            };
            let wrap = |a: f32| (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            eprintln!(
                "[face] t {} motion {:?} sim_yaw {:.3} heading {:.3} visual_yaw {:.3} clip_yaw {:.3} effective {:.3} vel {:?}",
                world.0.tick,
                p.motion,
                wrap(yaw),
                wrap(p.heading()),
                visual_yaw,
                clip_now,
                wrap(visual_yaw + clip_now),
                p.vel
            );
        }
    }
}

/// Free-camera pitch: right stick up/down, radians per second at full tilt.
const CAM_PITCH_SPEED: f32 = 1.6;
/// Locked on, the camera aims at this height on the foe (their chest).
const LOCK_AIM_HEIGHT: f32 = 12.0;
/// Closer than this the foe is beside you: keep the usual framing.
const LOCK_AIM_MIN_DIST: f32 = 25.0;
/// Pitch limits (radians, + = camera higher, looking down).
///
/// Decoded 2026-09-29 from `setup/configdata.dat` (312 bytes = 78 floats = 6
/// blocks of 13, no header). Four columns are identical in every block
/// (20, 10, 13, 13), which is how the structural constants separate from the
/// per-mode tuning; the up/down limit is the symmetric pair, **±70°** in the
/// five normal camera modes (the sixth, an odd outlier block, is 25/-32.5).
///
/// These replace a guessed pair (`-1.15` / `+0.85` rad, i.e. roughly -66° up
/// and +49° down) that had been set by eye on 2026-09-27. The shipped cap is
/// now the file's, so the camera can tilt as far as the original's did.
///
/// ⚠️ The column identity is inferred from the values' symmetry and from
/// matching our old constants, NOT program-verified — PROGRAM-RE.md still
/// carries "which float is which" as an open question. If the camera ever
/// misbehaves at the extremes, this mapping is the first thing to re-check.
const CAM_PITCH_MIN: f32 = -1.2217; // -70 deg
const CAM_PITCH_MAX: f32 = 1.2217; // 70 deg
/// Locked on and this close (or closer), cap how far the camera is allowed to
/// pitch down toward the foe's chest height (Laurence 2026-09-27: at point-
/// blank range the chest-aim math wants a steep angle even though `look`
/// below already stops aiming AT the chest -- this keeps the boom's pitch
/// itself level too, instead of tipping toward their torso). Eases out to the
/// full `CAM_PITCH_MAX` by `LOCK_AIM_MIN_DIST`, matching where `look` below
/// switches back to full chest-aim.
const LOCK_PITCH_DOWN_NEAR: f32 = 0.12;
/// Unlocked (free camera), how far the camera's own *position* is allowed to
/// orbit round the head as it pitches (separate from CAM_PITCH_MIN/MAX, which
/// also bound how far the *view* tilts). Orbiting the position the full way
/// up to CAM_PITCH_MIN would swing the camera under the player and into the
/// floor/ceiling (found by screenshotting the widened range -- `[cam]` debug
/// trace showed `dir` flip to mostly -Y and `allowed` collapse to ~15). Kept
/// at the old, proven-safe range; the view still tilts further via `look`.
/// NOT widened to match CAM_PITCH_MAX: the view already reaches the full
/// ±70° through `tilt` (which uses the unclamped `*pitch`), so a wider eye
/// orbit would buy nothing while re-introducing the documented floor/ceiling
/// clip. Only the view range is the file's; the eye range is ours.
const FREE_EYE_PITCH_MIN: f32 = -0.6;
const FREE_EYE_PITCH_MAX: f32 = 0.85;


fn follow_cameras(
    world: Res<World>,
    col: Res<StageCollision>,
    mut cam_occ: ResMut<cam_occluders::CamOccluders>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    settings: Res<menu::Settings>,
    pitch_in: Res<CamPitchInput>,
    local: Res<seats::LocalViews>,
    mut q: Query<(&PlayerCamera, &mut Transform)>,
    mut booms: Local<[f32; combat::MAX_PLAYERS]>,
    mut pitches: Local<[f32; combat::MAX_PLAYERS]>,
    mut last_scale: Local<f32>,
) {
    let layout = local.layout(&world.0.combat.roster, settings.split);
    cam_occ.sync_gone(&col.0);
    // The camera-distance setting (a percentage; 100 = the original's boom): read every frame, never
    // cached. It scales the boom AND every height offset by one factor, so the camera's angle to the
    // fighter is kept. Exactly 1.0 at 100, so default positions are bit-for-bit the unscaled ones.
    let k = cam_dist_scale(env_f32("PD_CAM_DIST_PCT", settings.cam_dist_pct));
    // A change of the setting snaps the boom (no slow ease-out) so a slider shows its effect at once.
    let scale_changed = *last_scale != k;
    *last_scale = k;
    for (c, mut t) in &mut q {
        let (me, _) = render_pose(&world, &fixed, c.0);
        let player = &world.0.players[c.0];
        let cam_yaw = player.cam_yaw;
        let fwd = Vec3::new(cam_yaw.sin(), 0.0, cam_yaw.cos());
        let head = me + Vec3::Y * 16.0;
        // Free camera: the right stick also looks up / down (orbiting the
        // camera round the head). Locked on: the boom tilts to the foe's
        // height (Laurence: a foe above was off the top of the screen) and
        // the camera looks straight at them, so they stay centred.
        // Whoever this player has locked (their lock target, cycled with the
        // switch-target button; a duel's only other fighter).
        let (foe, _) = render_pose(&world, &fixed, world.0.combat.aim_at(c.0, player.target));
        let foe_chest = foe + Vec3::Y * LOCK_AIM_HEIGHT;
        let to_foe = foe_chest - head;
        let pitch = &mut pitches[c.0];
        // PD_CAM_PITCH_MIN: calibration override for how far up the free
        // camera can look (A-F sheets); ships at CAM_PITCH_MIN.
        let pitch_min = env_f32("PD_CAM_PITCH_MIN", CAM_PITCH_MIN);
        if player.locked {
            // Negative pitch = camera lower, looking up. The raw chest-aim
            // angle blows up as the horizontal gap shrinks (a foe standing
            // right next to you but a bit lower/higher than your head still
            // divides by a near-zero distance), which used to tip the camera
            // down toward their torso at point-blank range. Ease the allowed
            // downward pitch in from LOCK_PITCH_DOWN_NEAR (near level) at
            // point-blank to the full CAM_PITCH_MAX by LOCK_AIM_MIN_DIST,
            // where `look` below also switches back to full chest-aim.
            let horiz = Vec2::new(to_foe.x, to_foe.z).length();
            let want_raw = -to_foe.y.atan2(horiz.max(1.0));
            let near_t = (horiz / LOCK_AIM_MIN_DIST).clamp(0.0, 1.0);
            let max_down = LOCK_PITCH_DOWN_NEAR + near_t * (CAM_PITCH_MAX - LOCK_PITCH_DOWN_NEAR);
            let want = want_raw.clamp(pitch_min, max_down);
            *pitch += (want - *pitch) * (1.0 - (-6.0 * time.delta_secs()).exp());
        } else {
            *pitch = (*pitch - pitch_in.0[c.0] * CAM_PITCH_SPEED * time.delta_secs()).clamp(pitch_min, CAM_PITCH_MAX);
        }
        let axis = Vec3::new(fwd.z, 0.0, -fwd.x);
        // `tilt` (full pitch) aims the view; locked on, it also swings the
        // camera's own position (orbiting under/over to track a foe above or
        // below). Unlocked, the position instead uses `eye_tilt`, clamped to
        // the old safe range -- see FREE_EYE_PITCH_MIN/MAX -- so looking far
        // up/down re-aims the view without swinging the camera itself under
        // the player and into the floor.
        let tilt = Quat::from_axis_angle(axis, *pitch);
        let eye_pitch = if player.locked { *pitch } else { pitch.clamp(FREE_EYE_PITCH_MIN, FREE_EYE_PITCH_MAX) };
        let eye_tilt = Quat::from_axis_angle(axis, eye_pitch);
        // PD_CAM_DIST / PD_CAM_UP: debug overrides for close-up captures.
        // Top/bottom split sits lower by the chosen camera height, because
        // the short, wide strip would otherwise crop your own character.
        //
        // The drop comes from the settings row (`menu::CameraHeight`, five
        // positions, `MID` by default). `PD_TB_CAM_DROP` still overrides it
        // for measurement, which is how the five positions were calibrated.
        // Locked on keeps the framing Laurence approved on his pad (60 / 30,
        // chest-aim) unless PD_CAM_LOCK_LIVE=1: the camcloud lock values
        // (52 / 31, fixed -19.1 pitch) tipped the view at the floor in play
        // (Laurence, 2026-10-02) and the pitch is distance-dependent in the
        // trace (GAME.md), so it is not a constant (UNCONFIRMED rule).
        let lock_old = cam_old() || !cam_lock_live();
        let (def_dist, def_up) = if cam_old() || (player.locked && lock_old) { (60.0, 30.0) } else if player.locked { (ORIG_DIST_LOCK, ORIG_UP_LOCK) } else { (ORIG_DIST_FREE, ORIG_UP_FREE) };
        // The camera-height row: in a strip as before; in every other layout it is an offset from MID (0 at the
        // default, so the approved framings are untouched), so the row works in any layout.
        let height_drop = if layout.is_strip() { env_f32("PD_TB_CAM_DROP", settings.camera_height.drop() - if cam_old() || (player.locked && lock_old) { 0.0 } else { menu::CameraHeight::Centre.drop() }) } else { settings.camera_height.drop() - menu::CameraHeight::Centre.drop() };
        let cam_up = (env_f32("PD_CAM_UP", def_up) - height_drop) * k;
        let ideal = head + eye_tilt * (-fwd * (env_f32("PD_CAM_DIST", def_dist) * k) + Vec3::Y * (cam_up - 16.0));
        // Keep the camera in front of scenery between it and the player. The 3 u clearance and the 4 u
        // minimum boom stay in absolute units on purpose: the near-wall dissolve (`cam_fade.rs`, 9 -> 4 u
        // eye-to-head, approved) is defined on those distances, so a wall behind the fighter dissolves it
        // the same at every distance setting. The ray itself is as long as the scaled boom.
        // Pull in immediately when blocked, ease back out slowly, so a view
        // that grazes an edge doesn't pop back and forth.
        let back = ideal - head;
        let full = back.length();
        let dir = back / full;
        // Film comparison: `PD_FILM_NOBOOM=1` skips the pull-in (the original never raycasts the camera, PROGRAM-RE.md), so a
        // fighter that starts with its back to a wall is framed like the take. Inert when unset.
        let allowed = if film_no_boom() || !cam_pullin() { full } else { cam_occ.raycast(&col.0, head, dir, full).map(|hit| (hit - 3.0).max(4.0)).unwrap_or(full) };
        let boom = &mut booms[c.0];
        if *boom <= 0.0 || allowed < *boom || scale_changed {
            *boom = allowed;
        } else {
            *boom += (allowed - *boom) * (1.0 - (-2.0 * time.delta_secs()).exp());
        }
        let eye = head + dir * *boom;
        if c.0 == 0 && std::env::var_os("PD_CAM_DEBUG").is_some() {
            eprintln!("[cam] pitch {:.3} head {:?} ideal {:?} full {:.1} allowed {:.1} boom {:.1} eye {:?} dir {:?}", *pitch, head, ideal, full, allowed, *boom, eye, dir);
        }
        // The original looks level (free) or at the foe (locked): no extra look-down.
        let drop = if layout.is_strip() && (cam_old() || (player.locked && lock_old)) { env_f32("PD_TB_LOOK_DROP", TB_LOOK_DROP) * k } else { 0.0 };
        let look = if player.locked && to_foe.length() > LOCK_AIM_MIN_DIST && !lock_old {
            // Original, locked on, foe at range: the view holds a steady -19.1 degree
            // pitch toward the foe (LIVE, GAME.md), so the foe rides above centre and
            // your own fighter falls below the frame; not the chest-aim angle.
            let h = Vec3::new(to_foe.x, 0.0, to_foe.z).normalize_or_zero();
            let p = ORIG_LOCK_PITCH_DEG.to_radians();
            eye + (h * p.cos() - Vec3::Y * p.sin()) * 100.0
        } else if player.locked && to_foe.length() > LOCK_AIM_MIN_DIST {
            foe_chest - Vec3::Y * drop
        } else {
            // Old: aim 14 above the feet. Original: pitch 0 = level with the eye.
            let aim_up = if cam_old() || (player.locked && lock_old) { 14.0 * k } else { cam_up };
            head + tilt * (Vec3::Y * (aim_up - drop - 16.0) + fwd * (env_f32("PD_CAM_AHEAD", 30.0) * k))
        };
        let target = Transform::from_translation(eye).looking_at(look, Vec3::Y);
        let k = 1.0 - (-8.0 * time.delta_secs()).exp();
        t.translation = t.translation.lerp(target.translation, k);
        t.rotation = t.rotation.slerp(target.rotation, k);
    }
}

/// Split the window per the player count and the settings; recomputed on
/// resize, a settings change, a new match (new cameras) or a change in the
/// number of players. Every view's rectangle comes from `views::ViewLayout`
/// (one player fills the window, two keep the side-by-side or top/bottom
/// setting, four share a 2x2 grid, with three the third player has the whole
/// bottom half: the views are NOT all the same size).
fn layout_viewports(
    area: Res<display::GameArea>,
    mut resized: MessageReader<WindowResized>,
    settings: Res<menu::Settings>,
    world: Res<World>,
    local: Res<seats::LocalViews>,
    mut cams: Query<(&PlayerCamera, &mut Camera)>,
    added: Query<(), Added<PlayerCamera>>,
    mut first: Local<bool>,
    mut last: Local<Option<views::ViewLayout>>,
) {
    let layout = local.layout(&world.0.combat.roster, settings.split);
    // Film comparison: PD_FILM_VIEW=1 gives P1's camera the whole window and switches P2's off.
    let film_view = std::env::var_os("PD_FILM_VIEW").is_some();
    // Each match spawns new cameras: lay them out too, not only on resize.
    if resized.read().count() == 0 && *first && !settings.is_changed() && !area.is_changed() && !local.is_changed() && added.is_empty() && *last == Some(layout) {
        return;
    }
    if !area.ready() {
        return;
    }
    *first = true;
    *last = Some(layout);
    // Every view sits inside the game area (the whole window unless a fixed aspect ratio leaves bars).
    for (c, mut cam) in &mut cams {
        if film_view {
            cam.is_active = c.0 == 0;
            cam.viewport = Some(Viewport { physical_position: area.pos, physical_size: area.size, ..default() });
            continue;
        }
        // The view index of this camera's slot; a slot with no view here is switched off.
        let Some(view) = local.view_of(c.0, &world.0.combat.roster) else {
            cam.is_active = false;
            continue;
        };
        cam.is_active = true;
        let r = layout.rect_in(view, area.pos, area.size);
        cam.viewport = Some(Viewport { physical_position: r.pos, physical_size: r.size, ..default() });
    }
}

/// The percentage setting as a factor (100 -> exactly 1.0).
fn cam_dist_scale(pct: f32) -> f32 {
    pct.clamp(menu::CAM_DIST_MIN, menu::CAM_DIST_MAX) / menu::CAM_DIST_DEFAULT
}

/// Field of view from the settings, per camera (`views::fov_for`): the setting is the horizontal
/// angle of a 16:9 view, and each camera's vertical angle follows from its OWN viewport's aspect,
/// so a 32:9 strip, an 8:9 half and a 16:9 quarter each get their right angle (and a layout with
/// cells of different shapes needs no special case). Runs every frame: a settings change shows on
/// the next one. `PD_CAM_HFOV=<deg>` overrides the setting for a capture (never saved).
fn apply_fov(settings: Res<menu::Settings>, mut cams: Query<(&Camera, &mut Projection), With<PlayerCamera>>) {
    let h16 = env_f32("PD_CAM_HFOV", settings.fov_deg);
    for (cam, mut p) in &mut cams {
        // No viewport yet (before the first layout): leave the camera as it is.
        let Some(aspect) = views::viewport_aspect(cam) else { continue };
        let fov = views::fov_for(h16, aspect);
        if let Projection::Perspective(pp) = p.as_mut() {
            if (pp.fov - fov).abs() > 1e-4 {
                pp.fov = fov;
            }
        }
    }
}

fn build_graphs(mut anims: ResMut<CharacterAnims>, gltfs: Res<Assets<bevy::gltf::Gltf>>, clips: Res<Assets<AnimationClip>>, mut graphs: ResMut<Assets<AnimationGraph>>) {
    for i in 0..anims.gltf.len() {
        if anims.graphs[i].is_some() {
            continue;
        }
        let Some(g) = gltfs.get(&anims.gltf[i]) else { continue };
        let mut graph = AnimationGraph::new();
        let mut nodes = Vec::new();
        let mut lengths = Vec::new();
        // Every clip the game has (cast poses, knock-down, get-up...): an
        // earlier short list left most of them out, so they never played.
        for &name in pd_formats::anim::ANIM_NAMES {
            if let Some(clip) = g.named_animations.get(name) {
                nodes.push((name, graph.add_clip(clip.clone(), 1.0, graph.root)));
                if let Some(c) = clips.get(clip) {
                    lengths.push((name, c.duration()));
                }
            }
        }
        anims.graphs[i] = Some((graphs.add(graph), nodes));
        anims.lengths[i] = lengths;
    }
}

fn attach_anim_slots(
    mut commands: Commands,
    anims: Res<CharacterAnims>,
    new_players: Query<Entity, (With<AnimationPlayer>, Without<AnimSlot>)>,
    parents: Query<&ChildOf>,
    visuals: Query<&PlayerVisual>,
) {
    for e in &new_players {
        let mut cur = e;
        let slot = loop {
            if let Ok(v) = visuals.get(cur) {
                break Some(v.0);
            }
            match parents.get(cur) {
                Ok(p) => cur = p.parent(),
                Err(_) => break None,
            }
        };
        let Some(slot) = slot else { continue };
        let Some((graph, _)) = &anims.graphs[slot] else { continue };
        commands.entity(e).insert((AnimSlot { player: slot, current: None, cast: None, float_in_done: false }, AnimationGraphHandle(graph.clone()), AnimationTransitions::new()));
    }
}

/// Casts play their action row's clip from the cast start (the program:
/// `FUN_14025a3e0`), through a short-range skill's wind-up and dash too --
/// Laurence's footage shows the sword clip's raised-knee stride during the
/// dash-in and its swing at the hit (FORMATS-ANIMATION.md "Cast clips").
///
/// Every clip start passes the program's own arguments (`anim_args`: blend
/// cut or 5 frames, loop, seek, hold loop). A held charge plays the row's
/// clip to frame w39 and then cycles w39..w40; the release restarts it at
/// w40 with a cut.
fn drive_animation(
    world: Res<World>,
    anims: Res<CharacterAnims>,
    cfg: Res<Config>,
    players: Res<seats::MatchPlayers>,
    fixed: Res<Time<Fixed>>,
    mut tables: Local<Option<anim_args::Tables>>,
    mut q: Query<(&mut AnimSlot, &mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let tables = tables.get_or_insert_with(|| anim_args::Tables::load(&cfg.cache.join("skills.json")));
    for (mut slot, mut player, mut transitions) in &mut q {
        let i = slot.player;
        let Some((_, nodes)) = &anims.graphs[i] else { continue };
        let f = &world.0.combat.fighters[i];
        let cast = casting(&world.0, i);
        // 0 = not casting, 1 = holding a charge, 2 = the cast proper.
        let phase: u8 = if !cast { 0 } else if f.charge.is_some() { 1 } else { 2 };
        // The Jump skill is a cast whose first part is the rooted wind-up
        // (state 4): its start clip plays, not the cast row's.
        let winding_up = world.0.players[i].jump_windup() > 0;
        let clip = if winding_up {
            "JUMP_START"
        } else if cast {
            // Picked once per cast, so a change of ground/air state mid-cast
            // doesn't swap the clip.
            match slot.cast {
                Some((seq, name)) if seq == f.cast_seq => name,
                _ => {
                    let name = cast_clip_now(&world.0, i, |n| nodes.iter().any(|(c, _)| *c == n));
                    slot.cast = Some((f.cast_seq, name));
                    name
                }
            }
        } else {
            let raw = clip_for(&world.0, i);
            if raw == "FLOAT_IN" {
                let p = &world.0.players[i];
                let moving = Vec2::new(p.pos.x - p.prev_pos.x, p.pos.z - p.prev_pos.z).length() > 0.05;
                if slot.float_in_done { float_hold_clip(moving) } else { raw }
            } else {
                slot.float_in_done = false;
                raw
            }
        };
        // The charge skill's hold loop and release frame, from the row the
        // cast plays (ground or air, by the skill's s40 as the sim picks it).
        let s40 = world.0.combat.skill(f.cast_skill).map_or(0, |d| d.raw[40]);
        let airborne = world.0.combat.airborne[i];
        // A bind / lift reaction (kinds 8 / 9) loops frames w43..w44 of its clip the same way
        // (`0x140258b87`, `0x140258c95`: the clip start's hold-loop arguments).
        let hold = if phase == 1 { tables.hold_loop(f.cast_skill, s40, airborne) } else { world.0.combat.reaction_hold(i) };
        // A pivot's TURN_* clip plays at the sim's own clip clock (`sim::Pivot`
        // time, blended between ticks like the pose), so the clip ends on the
        // very tick the sim hands the body over to the heading, and a gait
        // swap lands at the sim's retimed progress.
        let pivot_seconds = match (world.0.players[i].motion, world.0.players[i].pivot()) {
            (Motion::Strafe(_), Some(pv)) if is_turn_clip(clip) => {
                let per_tick = (30.0 / sim::HZ) as f32;
                Some((pv.time - per_tick * (1.0 - fixed.overstep_fraction())).max(0.0) / 30.0)
            }
            _ => None,
        };
        // Same key for the wind-up/dash (`Lunge`) and the strike (`Cast`) of
        // one cast: the clip keeps playing through.
        // A reaction restarts when it is hit again even if it plays the same
        // clip: the program starts the clip afresh on every reaction.
        let motion = world.0.players[i].motion;
        let seq = if cast {
            f.cast_seq
        } else if matches!(motion, Motion::Hit | Motion::Blown | Motion::Floored | Motion::GetUp | Motion::Down) {
            f.hit_seq
        } else {
            0
        };
        let want = (clip, seq, phase);
        if slot.current == Some(want) {
            let playing = nodes.iter().find(|(n, _)| *n == clip).map(|&(_, n)| n);
            // FLOAT_IN plays once; when its clip has run out the float goes on
            // with FLOAT_WAIT / FLOAT_MOVE (queued clip, `0x140257510`).
            if clip == "FLOAT_IN" {
                let len = anims.lengths[i].iter().find(|(n, _)| *n == clip).map(|&(_, l)| l);
                let t = playing.and_then(|n| player.animation(n)).map(|a| a.seek_time());
                if let (Some(len), Some(t)) = (len, t) {
                    if t >= len - 1e-3 {
                        slot.float_in_done = true;
                    }
                }
            }
            if let (Some(t), Some(node)) = (pivot_seconds, playing) {
                if let Some(active) = player.animation_mut(node) {
                    active.seek_to(t);
                }
            }
            // A held charge cycles frames w39..w40 (`0x140257560`-`0x14025758d`).
            if let (Some(h), Some(node)) = (hold, playing) {
                if let Some(active) = player.animation_mut(node) {
                    if let Some(t) = anim_args::hold_wrap(active.seek_time(), h) {
                        active.seek_to(t);
                    }
                }
            }
            continue;
        }
        let Some(&(_, node)) = nodes.iter().find(|(n, _)| *n == clip) else { continue };
        let old = slot.current.map(|(c, _, _)| c);
        let released = cast && phase == 2 && slot.current.is_some_and(|(_, seq, p)| seq == f.cast_seq && p == 1);
        let cx = anim_args::StartCx {
            motion: world.0.players[i].motion,
            from: old,
            to: clip,
            cast_blend_frames: anim_args::cast_blend_frames(players.0.get(i).map_or("", |p| p.char.as_str())),
            hold_loop: hold,
            release_seek: released.then(|| tables.release_frame(f.cast_skill, s40, airborne)),
            flinch_blend: tables.flinch_blend(f.reaction_row),
        };
        let start = anim_args::start_for(&cx);
        // PD_ANIM_LOG=1: every real clip change for P1, with the program's
        // start arguments it was played with.
        // (`PD_ANIM_LOG=all` adds P2, `[anim2]`.)
        if std::env::var("PD_ANIM_LOG").is_ok_and(|v| i == 0 || v == "all") {
            eprintln!(
                "[anim{}] t {} motion {:?} clip {} (was {:?}) blend {} loop {} seek {} hold {:?} root {} keep_phase {}",
                if i == 0 { "" } else { "2" },
                world.0.tick, world.0.players[i].motion, clip, old, start.blend_frames, start.looped, start.seek_frames, start.hold_loop, start.root_motion, start.keep_phase
            );
        }
        // WALK <-> RUN keeps the foot phase (`0x14025ad50`: new start time =
        // old time / old length * new length, no blend).
        let length = |name: &str| anims.lengths[i].iter().find(|(n, _)| *n == name).map(|&(_, l)| l);
        let keep_phase = old.filter(|_| start.keep_phase).and_then(|o| {
            let t = nodes.iter().find(|(n, _)| *n == o).and_then(|&(_, n)| player.animation(n))?.seek_time();
            let (lo, ln) = (length(o)?, length(clip)?);
            (lo > 0.0).then(|| (t.rem_euclid(lo) / lo) * ln)
        });
        let fade = if old.is_none() { Duration::ZERO } else { start.fade() };
        let active = transitions.play(&mut player, node, fade);
        // Only idle and the gaits loop: TURN_* is a one-shot pivot, not a loop
        // (PROGRAM-RE.md "Lock-on": looping it was the reported bug), and
        // casts, reactions, landings, get-ups and dodges play once.
        if start.looped {
            active.repeat();
        }
        if start.seek_frames > 0.0 {
            active.seek_to(start.seek_seconds());
        }
        if let Some(t) = keep_phase {
            active.seek_to(t);
        }
        if let Some(t) = pivot_seconds {
            // Driven by the sim's clock (seeked every frame above), not by
            // its own playback.
            active.seek_to(t);
            active.set_speed(0.0);
        }
        // Time-stretched to the jump's real airtime: the up clip takes
        // exactly the time to apex, the down clip the same length again
        // (PROGRAM-RE.md "Jumps"). `rise_seconds` is 0 for an ordinary fall
        // (walked off a ledge, no tracked launch) -- natural clip speed then.
        let rise = world.0.players[i].rise_seconds;
        if rise > 0.0 {
            if clip == "JUMP_UP" {
                active.set_speed(JUMP_UP_CLIP_SECONDS / rise);
            } else if clip == "JUMP_DOWN" {
                active.set_speed(JUMP_DOWN_CLIP_SECONDS / rise);
            }
        }
        slot.current = Some(want);
    }
}

/// `PD_FRAME_STATS=<frames>[,<skip>]` (screenshot runs): wall-clock frame time
/// over <frames> frames after <skip> warm-up frames (default 200), printed as
/// `[frame-stats] ...` (mean / median / p95 in ms, and the match's player
/// count) and the run ends. For comparing 2 and 4 views.
fn frame_stats(world: Res<World>, windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut seen: Local<Vec<f32>>, mut n: Local<u32>, mut last: Local<Option<std::time::Instant>>, mut exit: MessageWriter<AppExit>) {
    let Some(v) = std::env::var("PD_FRAME_STATS").ok() else { return };
    let parts: Vec<u32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    let (frames, skip) = (parts.first().copied().unwrap_or(300), parts.get(1).copied().unwrap_or(200));
    // Wall clock, not Bevy's clock: with `PD_SHOT_TICKS` the clock is a fixed 60 Hz step per frame
    // (so the match is the same every run), which would read 16.67 ms however slow the frame was.
    let now = std::time::Instant::now();
    let delta_ms = last.replace(now).map_or(0.0, |p| (now - p).as_secs_f32() * 1000.0);
    *n += 1;
    if *n <= skip {
        return;
    }
    seen.push(delta_ms);
    // A third number logs every frame slower than that many ms with its tick (hitch hunting).
    if parts.get(2).is_some_and(|&ms| delta_ms > ms as f32) {
        let disguised: Vec<usize> = (0..world.0.players.len()).filter(|&i| prophunt_view::disguised(&world.0, i)).collect();
        eprintln!("[frame-spike] tick {} {:.1} ms disguised {:?}", world.0.tick, delta_ms, disguised);
    }
    if seen.len() as u32 >= frames {
        let mut s = seen.clone();
        s.sort_by(|a, b| a.total_cmp(b));
        let mean = s.iter().sum::<f32>() / s.len() as f32;
        let win = windows.single().map_or(UVec2::ZERO, |w| UVec2::new(w.physical_width(), w.physical_height()));
        eprintln!("[frame-stats] players {} frames {} mean {:.2} ms median {:.2} ms p95 {:.2} ms p99 {:.2} ms max {:.2} ms ({:.0} fps mean) window {}x{}", world.0.combat.roster.count(), s.len(), mean, s[s.len() / 2], s[s.len() * 95 / 100], s[(s.len() * 99 / 100).min(s.len() - 1)], s[s.len() - 1], 1000.0 / mean, win.x, win.y);
        exit.write(AppExit::Success);
    }
}

/// `PD_SHOT_TICKS=a,b,c`: sim ticks (60 per second) to photograph. Plain
/// numbers count from the start of the match (`Match::tick`); `p`-prefixed
/// ones (`p0,p36`) from the `PD_TEST_PRESS` press tick. Returns the absolute
/// ticks, sorted, with the label each file gets (`<stem>_t<label>.png`).
fn config_shot_ticks() -> Option<Vec<(u64, String)>> {
    let v = std::env::var("PD_SHOT_TICKS").ok()?;
    let press = std::env::var("PD_TEST_PRESS").ok().and_then(|p| p.split(',').next().and_then(|t| t.trim().parse::<u64>().ok())).unwrap_or(0);
    parse_shot_ticks(&v, press)
}

/// `PD_SHOT_TICKS`'s list against a press tick (see `config_shot_ticks`).
fn parse_shot_ticks(v: &str, press: u64) -> Option<Vec<(u64, String)>> {
    let mut out: Vec<(u64, String)> = v
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            match s.strip_prefix('p') {
                Some(rel) => rel.parse::<u64>().ok().map(|r| (press + r, format!("p{r:03}"))),
                None => s.parse::<u64>().ok().map(|t| (t, format!("{t:04}"))),
            }
        })
        .collect();
    out.sort();
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod four_player_tests {
    use super::*;

    #[test]
    fn spread_spawns_keep_a_duel_pair_and_ring_three_or_four() {
        let pair = [Vec3::new(-50.0, 7.0, 0.0), Vec3::new(50.0, 7.0, 0.0)];
        assert_eq!(spread_spawns(pair, 1), vec![pair[0]]);
        assert_eq!(spread_spawns(pair, 2), pair.to_vec());
        for n in [3, 4] {
            let ring = spread_spawns(pair, n);
            assert_eq!(ring.len(), n);
            assert!(ring.iter().all(|p| p.y == 7.0 && (p.distance(Vec3::new(0.0, 7.0, 0.0)) - 50.0).abs() < 1e-3), "{n}: on the circle round the midpoint");
            assert!((0..n).all(|i| (0..i).all(|j| ring[i].distance(ring[j]) > 20.0)), "{n}: distinct places");
        }
    }

    /// The stage's own start points feed the match: two players take the first
    /// two (as before), three and four take the first set's next ones, each
    /// with its three capsule spots; no two share a place.
    #[test]
    fn stage_layouts_give_each_player_their_own_start_point() {
        let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache");
        if !cache.join("st03.props.json").exists() {
            eprintln!("cache absent; skipped");
            return;
        }
        let two = stage_layout(&cache, "st03", 2).expect("st03 has start points");
        assert_eq!((two.spawns.len(), two.capsule_spots.len()), (2, 2));
        for n in [3, 4] {
            let l = stage_layout(&cache, "st03", n).expect("a set of four starts");
            assert_eq!((l.spawns.len(), l.capsule_spots.len()), (n, n));
            assert_eq!(&l.spawns[..2], &two.spawns[..], "the duel's two starts stay the first two");
            assert!((0..n).all(|i| (0..i).all(|j| l.spawns[i].distance(l.spawns[j]) > 10.0)), "{n}: distinct starts");
            assert!(l.capsule_spots.iter().all(|c| c.len() == 3), "three capsule spots per start");
        }
        // st01 carries sets of two: four players continue into the second set.
        if cache.join("st01.props.json").exists() {
            assert_eq!(stage_layout(&cache, "st01", 4).map(|l| l.spawns.len()), Some(4));
        }
    }
}

#[cfg(test)]
mod shot_tick_tests {
    #[test]
    fn shot_ticks_are_absolute_or_after_the_press_and_sorted() {
        let t = super::parse_shot_ticks("p36, 300,p0,bad", 240).unwrap();
        assert_eq!(t, vec![(240, "p000".to_string()), (276, "p036".to_string()), (300, "0300".to_string())]);
        assert!(super::parse_shot_ticks("", 0).is_none());
    }
}

fn auto_screenshot(mut commands: Commands, cfg: Res<Config>, world: Res<World>, mut frames: Local<u32>, mut ko_shot: Local<bool>, mut thrown_at: Local<Option<u32>>, mut shot_ticks: Local<Option<Vec<(u64, String)>>>, mut ticks_done: Local<Option<u32>>, mut exit: MessageWriter<AppExit>) {
    let Some(path) = &cfg.screenshot else { return };
    *frames += 1;
    // PD_SHOT_TICKS: tick-exact captures (see `config_shot_ticks`; main()
    // makes each frame step exactly one tick). The frame rendered right after
    // a listed tick is saved; the run exits 30 frames after the last one.
    // Replaces the frame-counted burst below when set.
    if shot_ticks.is_none() {
        *shot_ticks = Some(config_shot_ticks().unwrap_or_default());
    }
    let ticks = shot_ticks.as_mut().expect("set above");
    if std::env::var_os("PD_SHOT_TICKS").is_some() {
        let tick = world.0.tick;
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        // Each listed tick is taken once (the tick can stand still for a
        // frame or two while the match is still loading).
        ticks.retain(|(t, label)| {
            if *t == tick {
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.with_file_name(format!("{stem}_t{label}.png"))));
            }
            *t > tick
        });
        if ticks.is_empty() {
            let done = ticks_done.get_or_insert(*frames);
            if *frames >= *done + 30 {
                exit.write(AppExit::Success);
            }
        }
        return;
    }
    // PD_SHOT_ON_THROW=1: also capture 4 frames once an object is first thrown
    // (Psycho Kinesis / Power), 5 frames apart: lifted, held, flying.
    if std::env::var("PD_SHOT_ON_THROW").is_ok() {
        if thrown_at.is_none() && world.0.combat.props.iter().any(|p| p.state == props::PropState::Thrown) {
            *thrown_at = Some(*frames);
        }
        if let Some(t) = *thrown_at {
            let k = (*frames - t) / 5;
            if (*frames - t) % 5 == 0 && k < 4 {
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.with_file_name(format!("{}_throw{k}.png", path.file_stem().unwrap_or_default().to_string_lossy()))));
            }
        }
    }
    // PD_SHOT_ON_KO=1: also capture the first knock-out, half a second in.
    if !*ko_shot && std::env::var("PD_SHOT_ON_KO").is_ok() && world.0.combat.round_over.is_some_and(|(_, left)| left < combat::ROUND_END_TICKS - 30) {
        *ko_shot = true;
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.with_file_name(format!("{}_ko.png", path.file_stem().unwrap_or_default().to_string_lossy()))));
    }
    // PD_SHOT_COUNT / PD_SHOT_EVERY: a burst of numbered screenshots.
    let count = env_f32("PD_SHOT_COUNT", 1.0) as u32;
    let every = env_f32("PD_SHOT_EVERY", 10.0) as u32;
    for k in 0..count {
        if *frames == cfg.shot_frame + k * every {
            let out = if count == 1 { path.clone() } else { path.with_file_name(format!("{}_{k}.png", path.file_stem().unwrap_or_default().to_string_lossy())) };
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out));
        }
    }
    if *frames == cfg.shot_frame + count * every + 60 {
        exit.write(AppExit::Success);
    }
}

/// The costume swap for a real match: a fighter whose costume colour is not
/// 0 (`Costumes`, set by `spawn_scene`: a second fighter of the same character
/// wears colour 1, a third 2, a fourth 3 -- for two players the old "P2's
/// alternate costume when both picked the same fighter", DECISIONS.md
/// 2026-09-27) gets newly spawned mesh materials under their model
/// (`PlayerVisual(slot)`) as their own copy with `base_color_texture` swapped
/// to that character's colour texture (`cache/chars/<id>.c<colour>.png`,
/// `pd_convert character`'s costume-colour export) -- not a mutation of the
/// shared asset (the first fighter's copy must stay untouched), and no change
/// at all for colour 0. Same technique as the setup screen's live preview
/// (`frontend::charselect::apply_colours`), which this mirrors but can't
/// share code with directly: that one keys off `PreviewModel`, this one off
/// `PlayerVisual`.
fn apply_costumes(mut commands: Commands, assets: Res<AssetServer>, costumes: Res<Costumes>, new_mats: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Added<MeshMaterial3d<StandardMaterial>>>, parents: Query<&ChildOf>, roots: Query<&PlayerVisual>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if costumes.colours.iter().all(|&c| c == 0) {
        return;
    }
    for (e, mat) in &new_mats {
        let mut cur = e;
        let slot_idx = loop {
            if let Ok(v) = roots.get(cur) {
                break Some(v.0);
            }
            match parents.get(cur) {
                Ok(p) => cur = p.parent(),
                Err(_) => break None,
            }
        };
        let Some(slot) = slot_idx else { continue };
        let (Some(id), Some(&colour)) = (costumes.chars.get(slot), costumes.colours.get(slot)) else { continue };
        if colour == 0 {
            continue;
        }
        let tex: Handle<Image> = assets.load(format!("chars/{id}.c{colour}.png"));
        if let Some(src) = materials.get(&mat.0) {
            let mut swapped = src.clone();
            swapped.base_color_texture = Some(tex);
            let handle = materials.add(swapped);
            commands.entity(e).insert(MeshMaterial3d(handle));
        }
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

/// Audit (2026-09-29): how many skills actually reach a real behaviour?
///
/// `moves::behaviour_of` ends in a catch-all arm that hands back
/// `Shell::PLACEHOLDER` -- a fixed speed/homing stub. Skills landing there
/// DO fly, but with no family logic, so "generalise skills by family" in
/// docs/ROADMAP.md is a real remaining task only to the extent those are
/// unimplemented. This prints the distribution so that claim can be checked
/// against the code rather than assumed. Measurement, not a gate: it
/// asserts nothing and always passes.
#[cfg(test)]
mod skill_coverage_audit {
    use super::*;

    #[test]
    fn report_skills_per_behaviour_arm() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/skills.json");
        if !path.exists() {
            eprintln!("skills.json absent; run pd_convert skills. Audit skipped.");
            return;
        }
        let defs = load_skills(&path, false);
        let mut by_arm: std::collections::BTreeMap<&'static str, Vec<i16>> = Default::default();
        for def in &defs {
            let arm = match moves::behaviour_of(def) {
                moves::Behaviour::AuraParticle => "AuraParticle",
                moves::Behaviour::Melee(_) => "Melee",
                // A Shell that carries the table's own speed/homing is real;
                // one that IS the PLACEHOLDER constant is the catch-all stub.
                moves::Behaviour::Shoot(moves::ShotKind::Shell(s)) => {
                    if s == moves::shell::Shell::PLACEHOLDER {
                        "Shoot(PLACEHOLDER)"
                    } else {
                        "Shoot(Shell)"
                    }
                }
                moves::Behaviour::Shoot(moves::ShotKind::Parabola(_)) => "Shoot(Parabola)",
                moves::Behaviour::Shoot(moves::ShotKind::Arc(_)) => "Shoot(Arc)",
                moves::Behaviour::Shoot(moves::ShotKind::Crawl(_)) => "Shoot(Crawl)",
                moves::Behaviour::Shoot(moves::ShotKind::Fall(_)) => "Shoot(Fall)",
                moves::Behaviour::Shoot(moves::ShotKind::Ricochet(_)) => "Shoot(Ricochet)",
                moves::Behaviour::Place(_) => "Place(area)",
                moves::Behaviour::Guard(_) => "Guard",
                moves::Behaviour::Effect(_) => "Effect",
                _ => "NotBuilt",
            };
            by_arm.entry(arm).or_default().push(def.id);
        }
        println!("\n=== SKILL COVERAGE: {} skills ===", defs.len());
        let mut rows: Vec<_> = by_arm.into_iter().collect();
        rows.sort_by_key(|(_, ids)| std::cmp::Reverse(ids.len()));
        for (arm, ids) in &rows {
            let sample: Vec<String> = ids.iter().take(10).map(|i| i.to_string()).collect();
            println!("  {:17} {:4} ({:5.1}%)  ids: {}", arm, ids.len(), ids.len() as f32 * 100.0 / defs.len() as f32, sample.join(","));
        }
    }
}

/// Ids 257 (Judge of Anubis) and 294 (Fusion) occur twice in `skills.json`;
/// the second record (records 749 / 750) is a character's dodge move. Every
/// loader keeps the FIRST record of an id, as `load_skills` does, so the
/// real skills are not overwritten by the dodges; the dodges themselves read
/// no skill row by id, so they are untouched.
#[cfg(test)]
mod first_record_loaders {
    use super::*;

    fn rec(id: i64, record: i64, anim: &str, effect: i64, voice: i64) -> serde_json::Value {
        let row = serde_json::json!({"anim_name": anim, "effect": effect, "voice_group": voice, "fire_tick": record});
        serde_json::json!({"id": id, "record": record, "name": format!("r{record}"), "capsule": "attack", "action": {"ground": row, "air": null}})
    }

    #[test]
    fn the_first_record_of_an_id_wins_in_every_loader() {
        let json = serde_json::json!({"skills": [rec(257, 76, "ATK_FAITH_SHOT", 77, 3), rec(300, 5, "ATK_X", 0, 0), rec(257, 749, "WAIT0", 257, 1)], "reactions": []});
        let ids: Vec<i64> = first_skill_records(&json).map(|s| s["record"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![76, 5], "the second 257 is skipped, the rest keep their order");
        let path = std::env::temp_dir().join(format!("pd_first_record_{}.json", std::process::id()));
        std::fs::write(&path, json.to_string()).unwrap();
        let rows = load_action_rows(&path);
        assert_eq!(rows[&257].0.as_ref().unwrap().anim_name, "ATK_FAITH_SHOT");
        assert_eq!(rows[&257].0.as_ref().unwrap().fire_tick, 76);
        assert_eq!(rows.len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn real_table_judge_of_anubis_and_fusion_read_their_own_rows_and_dodges_are_unchanged() {
        let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache");
        let path = cache.join("skills.json");
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        // The two ids really are doubled, and the doubles are the dodge records.
        for (id, first, dodge) in [(257, 76, 749), (294, 242, 750)] {
            let recs: Vec<i64> = json["skills"].as_array().unwrap().iter().filter(|s| s["id"] == id).map(|s| s["record"].as_i64().unwrap()).collect();
            assert_eq!(recs, vec![first, dodge], "id {id}");
        }
        let rows = load_action_rows(&path);
        let ground = |id: i16| rows[&id].0.clone().unwrap();
        assert_eq!(ground(257).anim_name, "ATK_FAITH_SHOT", "Judge of Anubis: the skill's own record, not the dodge's WAIT0");
        assert!(ground(257).fire_tick > 1);
        assert_eq!(rows[&257].1.as_ref().unwrap().anim_name, "ATK_SKYSHOT_HIGH", "and its own mid-air row");
        assert_eq!(ground(294).anim_name, "ATK_DRAIN", "Fusion: the skill's own record, not the dodge's DASH_A");
        // load_skills (always first-wins) and the loaders agree on whose rows these are.
        let defs = load_skills(&path, false);
        assert_eq!(defs.iter().find(|d| d.id == 257).unwrap().name, "Judge of Anubis");
        assert_eq!(defs.iter().find(|d| d.id == 294).unwrap().name, "Fusion");
        assert_eq!(defs.iter().filter(|d| d.id == 257 || d.id == 294).count(), 2);
        // Every id keeps exactly one row, the first record's.
        let first_of = |id: i64| json["skills"].as_array().unwrap().iter().find(|s| s["id"] == id).unwrap();
        for id in [257i64, 294] {
            assert_eq!(rows[&(id as i16)].0.as_ref().map(|r| r.fire_tick), action_row(&first_of(id)["action"]["ground"]).map(|r| r.fire_tick));
        }
        // Reactions are keyed by their own row and had no duplicates to lose.
        assert_eq!(load_reaction_rows(&path).len(), json["reactions"].as_array().unwrap().len());
        // The dodges read no skill row by id: the ground dodges' clips are the
        // same with the loaded rows in place.
        let mut combat = combat::Combat::new(load_skills(&path, false), vec![combat::AURA_PARTICLE; 30]);
        combat.set_action_rows(rows);
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat);
        m.players[0].motion = Motion::Dodge;
        m.players[0].dodge = sim::Dodge::Cartwheel;
        assert_eq!(clip_for(&m, 0), "DASH_A");
        m.players[0].dodge = sim::Dodge::Tumble;
        assert_eq!(clip_for(&m, 0), "DASHEND_A");
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;

    fn row(anim: &str) -> combat::ActionRow {
        combat::ActionRow { anim_name: anim.into(), cast_sound: 0, charge_cycle: 0, dash_start: 12, dash_end: 22, fire_tick: 18, hit_window_end: 22 }
    }

    fn casting_match(skill: i16, anim: &str) -> Match {
        let mut combat = combat::Combat::new(Vec::new(), vec![combat::AURA_PARTICLE; 30]);
        combat.set_action_row(skill, Some(row(anim)), None);
        combat.fighters[0].cast_skill = skill;
        Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat)
    }

    /// Laurence (2026-09-27): blades looked like kicks. Casts now play their
    /// action row's own clip (Psycho Knife row 3 ATK_SORD; Excalibur row 4 and
    /// the other two-handed swords ATK_WHAND_SORD, which the hand table used
    /// to replace with ATK_SORD), from the cast start.
    #[test]
    fn a_cast_plays_its_action_rows_clip() {
        let all = |_: &str| true;
        assert_eq!(cast_clip_now(&casting_match(3, "ATK_SORD"), 0, all), "ATK_SORD");
        assert_eq!(cast_clip_now(&casting_match(5, "ATK_WHAND_SORD"), 0, all), "ATK_WHAND_SORD");
        assert_eq!(cast_clip_now(&casting_match(16, "ATK_KICK"), 0, all), "ATK_KICK");
        // A clip the character lacks, or a name outside the motion table:
        // the hand-table fallback (no skill def here: SPE_POWER_UP).
        assert_eq!(cast_clip_now(&casting_match(5, "ATK_WHAND_SORD"), 0, |n: &str| n != "ATK_WHAND_SORD"), "SPE_POWER_UP");
        assert_eq!(cast_clip_now(&casting_match(5, "NOT_A_CLIP"), 0, all), "SPE_POWER_UP");
    }

    /// The Jump skill's wind-up (state 4) plays JUMP_START while the body
    /// stands rooted (`Player::jump_windup`), then the ordinary rise.
    #[test]
    fn the_jump_wind_up_plays_jump_start() {
        let mut m = casting_match(3, "ATK_SORD");
        assert_eq!(clip_for(&m, 0), "WAIT0");
        m.players[0].begin_jump(26, 100.0);
        assert_eq!(clip_for(&m, 0), "JUMP_START");
    }

    /// Reactions lane request: the Jump skill (special, s43 = 3, s44 = 0) is a
    /// cast whose action holds the body after take-off; it showed
    /// the row's WAIT0 in the air. In the air it shows JUMP_UP rising and JUMP_DOWN
    /// falling, and is not "casting" for the clip chooser; on the ground
    /// again it is the cast row's clip as before.
    #[test]
    fn the_jump_skill_shows_jump_up_and_down_in_the_air_not_wait0() {
        let mut combat = combat::Combat::new(vec![combat::testkit::def(239, combat::Capsule::Special, 1, &[(43, 3), (44, 0)])], vec![combat::AURA_PARTICLE; 30]);
        combat.set_action_row(239, Some(row("WAIT0")), None);
        combat.fighters[0].cast_skill = 239;
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat);
        m.players[0].motion = Motion::Cast;
        m.players[0].grounded = true;
        assert!(!clip_for(&m, 0).starts_with("JUMP"), "on the ground: the cast's own clip");
        assert!(casting(&m, 0));
        m.players[0].grounded = false;
        m.players[0].vel.y = 80.0;
        assert_eq!(clip_for(&m, 0), "JUMP_UP");
        assert!(!casting(&m, 0), "the airborne Jump is not a cast clip");
        m.players[0].vel.y = -20.0;
        assert_eq!(clip_for(&m, 0), "JUMP_DOWN");
        // Any other cast in the air keeps its own clip (a mid-air shot).
        m.combat.fighters[0].cast_skill = 3;
        assert!(casting(&m, 0));
    }

    /// Levitate / Skywalk at the top of the rise (state 7, float status on, `vy <= 0`,
    /// `0x14025a330`): the float clips, not the fall clip: FLOAT_IN, then (after it has played
    /// out, `drive_animation`) FLOAT_WAIT, or FLOAT_MOVE while the stick moves the body
    /// (`0x14025b170`). While rising it is still JUMP_UP; once the status ends, JUMP_DOWN.
    #[test]
    fn a_hanging_float_shows_the_float_clips_not_jump_down() {
        let s = 1000.0;
        let mut col = pd_formats::collision::Collision::new(vec![
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, -s), Vec3::new(s, 0.0, s)],
            [Vec3::new(-s, 0.0, -s), Vec3::new(s, 0.0, s), Vec3::new(-s, 0.0, s)],
        ]);
        let combat = combat::Combat::new(Vec::new(), vec![combat::AURA_PARTICLE; 30]);
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat);
        m.combat.round_ticks = combat::COUNTDOWN_TICKS;
        m.combat.fighters[0].status.hover_ticks = 400;
        m.players[0].begin_float(2, 60.0);
        let mut seen: Vec<&str> = Vec::new();
        for _ in 0..600 {
            m.step([sim::Input::default(); 2], &mut col);
            let c = clip_for(&m, 0);
            if seen.last() != Some(&c) {
                seen.push(c);
            }
        }
        assert_eq!(seen[..2], ["JUMP_START", "JUMP_UP"], "{seen:?}");
        assert!(seen.contains(&"FLOAT_IN"), "{seen:?}");
        let at = |name: &str| seen.iter().position(|c| *c == name);
        assert!(at("FLOAT_IN") < at("JUMP_DOWN"), "the hang comes before the fall: {seen:?}");
        assert!(at("JUMP_DOWN").is_some(), "falls with the ordinary clip once the float ends: {seen:?}");
        assert_eq!(seen.last(), Some(&"WAIT0"), "landed after the status ended: {seen:?}");
        assert_eq!(float_hold_clip(true), "FLOAT_MOVE");
        assert_eq!(float_hold_clip(false), "FLOAT_WAIT");
    }

    /// The converted shot points (`pd_convert shotpoints`; `PD_CACHE` or the
    /// main checkout's cache, skipped where they have not been converted):
    /// Laser's marker is the program's (0, 14.38, 10) on its ground effect 42
    /// (`eft0042`), Double Laser has two at (+-13, 12, 0), and the right hand
    /// stays within arm's reach of the body through every attack clip of
    /// pc00a, reaching forward by the fire frame of a shot clip.
    #[test]
    fn the_converted_shot_points_read_back_sane() {
        let cache = std::env::var_os("PD_CACHE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cache"));
        if !cache.join("shotpoints.json").exists() || !cache.join("chars/pc00a.hand.json").exists() {
            eprintln!("skipped: no shotpoints.json / pc00a.hand.json in {}", cache.display());
            return;
        }
        let markers = load_shot_markers(&cache);
        let (laser, _) = &markers[&65];
        assert_eq!(laser.len(), 1);
        assert!((laser[0] - Vec3::new(0.0, 14.38, 10.0)).length() < 0.01, "{:?}", laser);
        let (double, _) = markers.iter().find(|(id, _)| **id != 65 && !markers.is_empty() && markers[*id].0.len() == 2 && markers[*id].0[0].x == 13.0).map(|(_, v)| v).expect("a two-marker skill");
        assert_eq!((double[0].x, double[1].x), (13.0, -13.0));
        let hands = load_hand_paths(&cache, &["pc00a".to_string()]);
        let clips = &hands[0];
        assert!(clips.len() >= 20 && clips.contains_key("ATK_SORD") && clips.contains_key("ATK_SHOT"), "{} clips", clips.len());
        for (name, clip) in clips {
            assert!(clip.pos.len() > 10 && clip.pos.len() == clip.fwd.len(), "{name}");
            for p in &clip.pos {
                assert!(p.y > -5.0 && p.y < 45.0 && p.x.abs() < 40.0 && p.z.abs() < 45.0, "{name}: hand at {p}");
            }
        }
        let (at, _) = clips["ATK_SHOT"].at(20.0).unwrap();
        assert!(at.z > 3.0 && at.y > 8.0, "ATK_SHOT: hand held out in front at the fire frame: {at}");
    }

    /// A KO'd body with the reaction table loaded lies in its landing / DOWN pose,
    /// not the old fixed BLOW0_FRONT.
    #[test]
    fn a_fallen_fighter_lies_in_the_lying_clip_not_the_old_fixed_pose() {
        let mut m = casting_match(3, "ATK_SORD");
        m.players[0].motion = Motion::Floored;
        assert!(["BLOW_LAND_SF", "DOWN_FRONT"].contains(&clip_for(&m, 0)), "{}", clip_for(&m, 0));
        m.players[0].motion = Motion::Down;
        assert_eq!(clip_for(&m, 0), "BLOW0_FRONT", "no reaction to play: the old static pose");
    }

    /// The hand-off, as rendered: on the last pivot tick the model faces
    /// heading + the side's offset and shows the TURN clip's last pose; on
    /// the next it faces the heading and shows WALK/RUN's first frame, with
    /// no blend. With pc00a's measured hip yaw the body's rendered facing
    /// moves by under 5 deg for all eight clips (the helper measurement
    /// found <= 7.5 deg for all 11 playable characters) -- the spin/spasm
    /// came from blending that switch while the facing jumped.
    #[test]
    fn pivot_hand_off_keeps_the_rendered_facing_continuous() {
        for dir in [StrafeDir::Right, StrafeDir::Left, StrafeDir::BackRight, StrafeDir::BackLeft] {
            for gait in [Gait::Walk, Gait::Run] {
                let clip = strafe_clip(dir, gait);
                let heading = 0.7f32;
                let before = heading + sim::pivot_offset(dir) + clip_root_yaw(clip, turn_clip_seconds(clip));
                let after = heading + gait_clip_yaw(gait);
                let d = (before - after + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                assert!(d.abs().to_degrees() < 5.0, "{clip}: rendered facing jumps {} deg at the hand-off", d.to_degrees());
            }
        }
    }

    /// The table lengths agree with the sim's pivot clock.
    #[test]
    fn pivot_clip_tables_match_the_sim_lengths() {
        for dir in [StrafeDir::Right, StrafeDir::Left, StrafeDir::BackRight, StrafeDir::BackLeft] {
            for gait in [Gait::Walk, Gait::Run] {
                let clip = strafe_clip(dir, gait);
                assert!((turn_clip_seconds(clip) * 30.0 - sim::pivot_len_30hz(dir, gait) as f32).abs() < 0.01, "{clip}");
            }
        }
    }

    #[test]
    fn every_state_faces_the_sim_exactly_reactions_included() {
        for m in [Motion::Idle, Motion::Walk, Motion::Run, Motion::Strafe(StrafeDir::Left), Motion::Rise, Motion::Land, Motion::Cast, Motion::Lunge, Motion::Hit, Motion::Blown, Motion::Floored, Motion::GetUp, Motion::Down] {
            assert!(faces_sim_exactly(m), "{m:?}");
        }
        // Exact: the rotation is the sim's yaw, whatever it was before.
        let (r, catching) = model_rotation(Quat::from_rotation_y(2.0), 0.5, true, false, 1.0 / 60.0);
        assert!(r.angle_between(Quat::from_rotation_y(0.5)) < 1e-5 && !catching);
        // After an eased state the model catches up smoothly, then locks on.
        let mut rot = Quat::from_rotation_y(1.5);
        let mut catching = true;
        let mut frames = 0;
        while catching {
            let prev = rot;
            (rot, catching) = model_rotation(rot, 0.0, true, catching, 1.0 / 60.0);
            assert!(prev.angle_between(rot) < 0.4, "no jump while catching up");
            frames += 1;
            assert!(frames < 120, "never caught up");
        }
        let (r, _) = model_rotation(rot, 0.0, true, catching, 1.0 / 60.0);
        assert!(r.angle_between(Quat::IDENTITY) < 1e-5);
    }
}

// ---- Film comparison hooks (`engine/re/film_compare.py`; test runs only, all inert unless the
// env vars are set). The same scenario as a take of the original's move recorder: a skill in a
// slot, a distance, Level / Health, an optional opponent skill; plus a per-tick log.

/// Film comparison: `PD_WINDOW=<w>x<h>` sets the window size (default 1600x900) so a capture can match the
/// original footage's 1440x900. Inert when unset.
fn film_window() -> (u32, u32) {
    std::env::var("PD_WINDOW")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?))))
        .filter(|&(w, h)| w >= 64 && h >= 64)
        .unwrap_or((1600, 900))
}

fn film_no_boom() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PD_FILM_NOBOOM").is_some())
}

/// `PD_FILM_DIST=<d>`: P2 is moved to horizontal distance <d> from P1, along the line between
/// their start points, standing on the floor there (both then face each other, `reset_positions`).
fn film_spawns(col: &Collision, mut spawns: Vec<Vec3>) -> Vec<Vec3> {
    let Some(d) = std::env::var("PD_FILM_DIST").ok().and_then(|v| v.parse::<f32>().ok()) else { return spawns };
    if spawns.len() >= 2 {
        let dir = sim::toward(spawns[0], spawns[1]);
        let (x, z) = (spawns[0].x + dir.x * d, spawns[0].z + dir.z * d);
        if let Some(y) = col.ground_below(x, z, spawns[0].y + 12.0) {
            spawns[1] = Vec3::new(x, y, z);
        } else {
            eprintln!("[film] no floor {d} away from P1; P2 left at its start point");
        }
    }
    spawns
}

/// `PD_FILM=<skill>,<slot>,<level>,<hp0>,<hp1>,<lock 0/1>[,<opp skill or -1>,<opp slot>[,<aura>]]`, applied once
/// before the first tick: both fighters get exactly that Level, full Aura (= Level) and only the Aura Particle on the other buttons and no capsules; P1 holds <skill> in <slot>, P2 <opp skill> in <opp slot>.
fn film_setup(mut world: ResMut<World>, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    let Some(v) = std::env::var("PD_FILM").ok() else { return };
    let n: Vec<i32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if n.len() < 6 {
        return;
    }
    let (skill, slot, level, hp0, hp1, lock) = (n[0], n[1] as usize, n[2], n[3], n[4], n[5] != 0);
    let opp = if n.len() >= 8 && n[6] >= 0 { Some((n[6], n[7] as usize)) } else { None };
    let aura = n.get(8).copied().unwrap_or(level);
    let m = &mut world.0;
    for (i, hp) in [(0usize, hp0), (1usize, hp1)] {
        let f = &mut m.combat.fighters[i];
        f.level_max = f.level_max.max(level);
        f.level_min = f.level_min.min(level);
        f.level = level;
        f.aura_ticks = aura * combat::AURA_TICKS_PER_POINT;
        f.max_health = f.max_health.max(hp);
        f.health = hp;
        f.arsenal.clear();
        // The other buttons hold the Aura Particle (as the original's takes do): a fighter with no
        // skill at all would be drained by the Powerless rule.
        for k in 0..f.slots.len() {
            f.set_slot(k, Some(combat::AURA_PARTICLE));
        }
    }
    m.combat.fighters[0].set_slot(slot.min(3), Some(skill as i16));
    if let Some((os, oslot)) = opp {
        m.combat.fighters[1].set_slot(oslot.min(3), Some(os as i16));
    }
    m.players[0].locked = lock;
    // `PD_FILM_P1LOCK=0/1`: whether the second fighter is locked on (the take's own `p1_lock` at the press). A
    // locked fighter turns to face the foe; the takes' unlocked P1 keeps its start facing, and every object
    // test / knock-back reads where that body points (`Body::lock`). Unset = the old default (locked).
    if let Some(v) = std::env::var("PD_FILM_P1LOCK").ok() {
        m.players[1].locked = v.trim() != "0";
    }
    // `PD_FILM_FIELD=<p0 Aura Particles>,<p1 Aura Particles>[,<p0 other capsules>,<p1 other capsules>]`: the capsules the ORIGINAL
    // take's stage held on each fighter's own spots when it started (the trace does not log the field; `film_compare.py` infers the
    // counts from what the skill did: Meltdown's Level loss, Overheat's STR, Evaporate / Violent Change's erase sounds). They lie on
    // the owner's stage spots in order (particles first), up to 3 each, takeable (`appear` 0). Inert when unset.
    if let Some(v) = std::env::var("PD_FILM_FIELD").ok() {
        let n: Vec<usize> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        let bodies = [m.players[0].pos, m.players[1].pos];
        let c = &mut m.combat;
        for i in 0..2usize {
            let (particles, other) = (n.get(i).copied().unwrap_or(0), n.get(2 + i).copied().unwrap_or(0));
            for k in 0..(particles + other).min(combat::MAX_CAPSULES_PER_PLAYER) {
                c.place_field_capsule(i, k, if k < particles { combat::AURA_PARTICLE } else { 1 }, &bodies);
            }
        }
    }
    // `PD_FILM_YAW=<p0 yaw>,<p1 yaw>` (radians, `sim::yaw_of` convention): both fighters start facing that way, camera behind
    // them, like the take's first trace row (the takes' unlocked starts do not face the foe). Inert when unset.
    if let Some(v) = std::env::var("PD_FILM_YAW").ok() {
        let a: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        for (i, y) in a.iter().take(2).enumerate() {
            let p = &mut m.players[i];
            (p.yaw, p.prev_yaw, p.cam_yaw) = (*y, *y, *y);
        }
    }
}

/// `PD_FILM_LOG=<file>`: after every tick one `T,...` row (both fighters' state) and one `E,...` row per
/// new combat event (fires, hits, ...). The file is truncated at the first tick.
fn film_log(world: Res<World>, mut seen: Local<u32>, mut file: Local<Option<std::fs::File>>) {
    use std::io::Write;
    let Some(path) = std::env::var_os("PD_FILM_LOG") else { return };
    if file.is_none() {
        *file = std::fs::File::create(&path).ok();
        if let Some(f) = file.as_mut() {
            let _ = writeln!(f, "T,tick,p0_health,p0_level,p0_aura,p0_action,p0_skill,p0_x,p0_z,p1_health,p1_level,p1_aura,p1_action,p1_skill,p1_x,p1_z,p0_y,p1_y,p0_yaw,p1_yaw");
        }
    }
    let Some(f) = file.as_mut() else { return };
    let (m, c) = (&world.0, &world.0.combat);
    let per = combat::AURA_TICKS_PER_POINT as f32;
    let a = |i: usize| {
        let x = &c.fighters[i];
        format!("{},{},{:.2},{:?}/{},{},{:.1},{:.1}", x.health, x.level, x.aura_ticks as f32 / per, x.action, x.action_ticks, x.cast_skill, m.players[i].pos.x, m.players[i].pos.z)
    };
    let _ = writeln!(f, "T,{},{},{},{:.2},{:.2},{:.4},{:.4}", m.tick, a(0), a(1), m.players[0].pos.y, m.players[1].pos.y, m.players[0].yaw, m.players[1].yaw);
    let from = *seen;
    for (seq, ev) in c.events.iter().filter(|(s, _)| *s > from) {
        *seen = (*seen).max(*seq);
        let _ = writeln!(f, "E,{},{:?}", m.tick, ev);
    }
}

/// PD_FILM_LOG runs: end-of-frame marker for the sound / effect log lines above it, and one
/// `[film] fx eftNNNN` line per effect scene model spawned this frame (effects with no model, only
/// trails or sprites, are not seen here).
fn film_frame_marker(world: Option<Res<World>>, roots: Query<&WorldAssetRoot, Added<WorldAssetRoot>>, assets: Res<AssetServer>) {
    if std::env::var_os("PD_FILM_LOG").is_none() {
        return;
    }
    let Some(world) = world else { return };
    for r in &roots {
        if let Some(p) = assets.get_path(r.0.id()) {
            let s = p.to_string();
            if s.starts_with("fx/") || s.starts_with("fx\\") {
                eprintln!("[film] fx {}", s);
            }
        }
    }
    eprintln!("[film] frame tick {}", world.0.tick);
}

/// Laurence (2026-10-02, playing with a controller): "Player 1 can't use the
/// pause menu with the d-pad or stick; the d-pad opens info about the bound
/// moves." These drive the REAL `gather_input` and `menu_input` systems with
/// mocked pads (docs/BUG-GRAVEYARD.md, "Pause menu ignored the pad").
#[cfg(test)]
mod pause_menu_input_tests {
    use super::*;
    use bevy::input::gamepad::{GamepadAxis, GamepadInput};
    use frontend::sound::{Cue, MenuSfxQueue};

    struct Rig {
        app: App,
        /// Pad entities in spawn (= query) order.
        pads: Vec<Entity>,
        /// This rig's settings file.
        path: PathBuf,
    }

    impl Rig {
        /// A match with `n_pads` pads; `seat_pad[s]` is the index into `pads` that plays seat `s`.
        fn new(n_pads: usize, seat_pad: &[usize]) -> Self {
            let mut app = App::new();
            let combat = combat::Combat::new(Vec::new(), vec![combat::AURA_PARTICLE; 30]);
            let mut seats = seats::Seats::default();
            let pads: Vec<Entity> = (0..n_pads).map(|_| app.world_mut().spawn(Gamepad::default()).id()).collect();
            for (s, &p) in seat_pad.iter().enumerate() {
                seats.pad[s] = Some(pads[p]);
            }
            // One file per rig: the tests run in parallel threads and each one saves its settings.
            static RIG_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let settings_path = std::env::temp_dir().join(format!("pd_pause_test_{}_{}.json", std::process::id(), RIG_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            let _ = std::fs::remove_file(&settings_path);
            let mut settings = menu::Settings::load(settings_path.clone());
            // A windowed start (the file-less default is a borderless first run).
            settings.display = display::DisplaySettings::default();
            app.init_resource::<ButtonInput<KeyCode>>()
                .init_resource::<Time>()
                .init_resource::<PendingInputs>()
                .init_resource::<hud::SkillInfoView>()
                .init_resource::<CamPitchInput>()
                .init_resource::<MenuSfxQueue>()
                .init_resource::<display::CurrentMonitor>()
                .init_resource::<display::Screens>()
                .init_resource::<NextState<AppState>>()
                .insert_resource(State::new(AppState::Match))
                .insert_resource(seats)
                .insert_resource(settings)
                .insert_resource(menu::MenuState::opened(true))
                .insert_resource(World(Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat)))
                .insert_resource(Config { stage: String::new(), chars: [String::new(), String::new()], demo: false, screenshot: None, shot_frame: 0, cache: PathBuf::new(), legacy: false })
                .add_systems(Update, (gather_input, menu::menu_input).chain());
            Rig { app, pads, path: settings_path }
        }

        fn open(&mut self, open: bool) {
            self.app.world_mut().resource_mut::<menu::MenuState>().open = open;
        }

        /// One frame: clear last frame's edges, apply `set` to the pad `pad`, run the systems.
        fn frame(&mut self, pad: usize, set: impl Fn(&mut Gamepad)) {
            self.app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
            for &e in &self.pads.clone() {
                let mut g = self.app.world_mut().get_mut::<Gamepad>(e).unwrap();
                // A frame starts with everything released and the stick centred.
                g.digital_mut().release_all();
                g.digital_mut().clear();
                g.analog_mut().set(GamepadInput::Axis(GamepadAxis::LeftStickY), 0.0);
            }
            if let Some(&e) = self.pads.get(pad) {
                set(&mut self.app.world_mut().get_mut::<Gamepad>(e).unwrap());
            }
            self.app.update();
        }

        fn row(&self) -> usize {
            self.app.world().resource::<menu::MenuState>().row()
        }

        fn settings_path(&self) -> PathBuf {
            self.path.clone()
        }

        fn settings(&self) -> &menu::Settings {
            self.app.world().resource::<menu::Settings>()
        }

        fn browsing(&self) -> Option<display::DisplaySettings> {
            self.app.world().resource::<menu::MenuState>().browsing()
        }

        /// A 4K TV whose desktop runs at 1080p, as the OS would report it.
        fn with_tv(mut self) -> Self {
            let vm = |w, h, hz: u32| bevy::window::VideoMode { physical_size: UVec2::new(w, h), bit_depth: 32, refresh_rate_millihertz: hz * 1000 };
            self.app.world_mut().resource_mut::<display::CurrentMonitor>().0 = Some(display::MonitorInfo { size: UVec2::new(1920, 1080), scale: 1.0, modes: vec![vm(3840, 2160, 60), vm(2560, 1440, 60), vm(1920, 1080, 60), vm(1280, 720, 60)] });
            self
        }

        /// One frame with `b` held down: the first call presses it, later calls keep it held (no new press edge).
        fn hold(&mut self, b: GamepadButton) {
            self.app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
            let e = self.pads[0];
            let mut g = self.app.world_mut().get_mut::<Gamepad>(e).unwrap();
            g.digital_mut().clear();
            g.digital_mut().press(b);
            self.app.update();
        }

        /// Let go of everything and run a frame.
        fn release(&mut self) {
            self.frame(0, |_| {});
        }

        /// Down `n` rows (one tap each).
        fn down(&mut self, n: usize) {
            for _ in 0..n {
                tap(self, GamepadButton::DPadDown);
            }
        }

        fn info(&self) -> [Option<usize>; combat::MAX_PLAYERS] {
            self.app.world().resource::<hud::SkillInfoView>().0
        }

        fn cues(&mut self) -> Vec<Cue> {
            std::mem::take(&mut self.app.world_mut().resource_mut::<MenuSfxQueue>().0)
        }
    }

    fn dpad(b: GamepadButton) -> impl Fn(&mut Gamepad) {
        move |g| g.digital_mut().press(b)
    }
    fn stick(y: f32) -> impl Fn(&mut Gamepad) {
        move |g| {
            g.analog_mut().set(GamepadInput::Axis(GamepadAxis::LeftStickY), y);
        }
    }

    #[test]
    fn p1_dpad_down_moves_the_pause_menu_and_shows_no_skill_info() {
        // Seat 1's pad is NOT the first pad Bevy lists (a spare pad is): the
        // old menu read "pad 0" and so ignored it; the D-pad also lit the
        // skill-info panel, paused or not.
        let mut r = Rig::new(2, &[1, 0]);
        r.frame(1, dpad(GamepadButton::DPadDown));
        assert_eq!(r.row(), 1, "P1's D-pad down moves the cursor");
        assert_eq!(r.info(), [None; combat::MAX_PLAYERS], "the D-pad must not open skill info while paused");
        assert_eq!(r.app.world().resource::<PendingInputs>().0, [Input::default(); combat::MAX_PLAYERS], "nothing leaks into the sim");
        assert!(r.cues().contains(&Cue::Move), "the menu's move sound plays");
    }

    #[test]
    fn left_stick_navigates_with_a_deadzone_and_repeats_when_held() {
        let mut r = Rig::new(1, &[0]);
        r.frame(0, stick(0.3));
        assert_eq!(r.row(), 0, "inside the deadzone: nothing");
        r.frame(0, stick(0.0));
        r.frame(0, stick(-1.0));
        assert_eq!(r.row(), 1, "stick down = one step on the first frame");
        for _ in 0..10 {
            r.frame(0, stick(-1.0));
        }
        assert_eq!(r.row(), 1, "held: no repeat before the delay");
        for _ in 0..40 {
            r.frame(0, stick(-1.0));
        }
        // 51 held frames: pulses on frames 1, 27, 33, 39, 45, 51 = 6 steps; the in-match rows wrap.
        let wrapped = 6 % menu::ROWS;
        assert_eq!(r.row(), wrapped, "hold-to-repeat, same cadence as the other menus");
        r.frame(0, stick(1.0));
        assert_eq!(r.row(), wrapped - 1, "stick up steps back");
        assert_eq!(r.info(), [None; combat::MAX_PLAYERS]);
    }

    #[test]
    fn player_two_pad_and_the_keyboard_work_too() {
        let mut r = Rig::new(2, &[0, 1]);
        r.frame(1, dpad(GamepadButton::DPadDown));
        assert_eq!(r.row(), 1, "P2's D-pad");
        r.frame(1, stick(0.0));
        r.frame(1, stick(-1.0));
        assert_eq!(r.row(), 2, "P2's stick");
        r.app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        r.app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ArrowDown);
        r.app.update();
        assert_eq!(r.row(), 3, "keyboard arrows still drive the menu");
        assert_eq!(r.info(), [None; combat::MAX_PLAYERS], "arrows are P1's skill-info keys in a match, silent while paused");
    }

    #[test]
    fn a_closed_menu_still_shows_skill_info_and_sends_inputs_to_the_sim() {
        let mut r = Rig::new(1, &[0]);
        r.open(false);
        r.frame(0, |g| {
            g.digital_mut().press(GamepadButton::DPadRight);
            g.digital_mut().press(GamepadButton::South);
        });
        assert_eq!(r.info()[0], Some(1), "D-pad right shows slot 1's skill while playing");
        assert_ne!(r.app.world().resource::<PendingInputs>().0[0], Input::default(), "a skill press reaches the sim");
        assert_eq!(r.row(), 0);
    }

    fn press(b: GamepadButton) -> impl Fn(&mut Gamepad) {
        move |g| g.digital_mut().press(b)
    }

    /// One press, then a frame with nothing held (so the next press is a fresh one, not a repeat).
    fn tap(r: &mut Rig, b: GamepadButton) {
        r.frame(0, dpad(b));
        r.frame(0, stick(0.0));
    }

    /// Down to the "Lighting and look" row (after the HUD size row; the tenth, index 9) and confirm.
    fn open_look_page(r: &mut Rig) {
        for _ in 0..menu::ROWS {
            if r.row() == 9 {
                break;
            }
            tap(r, GamepadButton::DPadDown);
        }
        assert_eq!(r.row(), 9, "the Lighting and look row is the tenth");
        r.frame(0, press(GamepadButton::South));
    }

    fn settings_look(r: &Rig) -> look::LookSettings {
        r.app.world().resource::<menu::Settings>().look
    }

    #[test]
    fn confirm_on_lighting_and_look_opens_its_page_and_b_goes_back_to_that_row() {
        let mut r = Rig::new(1, &[0]);
        assert_eq!(r.app.world().resource::<menu::MenuState>().page(), menu::Page::Main);
        open_look_page(&mut r);
        assert_eq!(r.app.world().resource::<menu::MenuState>().page(), menu::Page::Look);
        assert_eq!(r.row(), 0, "the page starts on its first row");
        assert!(r.cues().contains(&Cue::Confirm));
        // The page's rows wrap on its own length (6), not the main list's.
        for _ in 0..look::LOOK_ROWS.len() {
            tap(&mut r, GamepadButton::DPadDown);
        }
        assert_eq!(r.row(), 0, "six downs wrap back to the top");
        tap(&mut r, GamepadButton::DPadUp);
        assert_eq!(r.row(), look::LOOK_ROWS.len() - 1, "up from the top lands on BACK");
        // B (and confirm on BACK) go back to the main page, on the Lighting and look row, not out of the overlay.
        r.frame(0, press(GamepadButton::East));
        assert_eq!((r.app.world().resource::<menu::MenuState>().page(), r.row(), r.app.world().resource::<menu::MenuState>().open), (menu::Page::Main, 9, true));
        assert!(r.cues().contains(&Cue::Back));
        open_look_page(&mut r);
        tap(&mut r, GamepadButton::DPadUp);
        r.frame(0, press(GamepadButton::South));
        assert_eq!((r.app.world().resource::<menu::MenuState>().page(), r.row()), (menu::Page::Main, 9), "confirm on BACK");
        // Start closes the whole overlay from the page and the next opening is the main page.
        open_look_page(&mut r);
        r.frame(0, press(GamepadButton::Start));
        assert!(!r.app.world().resource::<menu::MenuState>().open);
        assert_eq!(r.app.world().resource::<menu::MenuState>().page(), menu::Page::Main);
    }

    #[test]
    fn the_look_rows_change_settings_at_once() {
        let mut r = Rig::new(1, &[0]);
        assert_eq!(settings_look(&r), look::LookSettings::default(), "the defaults are the original look");
        open_look_page(&mut r);
        // Row 0, the preset: right = ENHANCED.
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(settings_look(&r).preset(), look::Preset::Enhanced);
        // Row 1, bloom: left steps LOW -> OFF, so the preset reads CUSTOM
        // (LOW -> HIGH would now be MAX: MAX differs from ENHANCED only in bloom).
        tap(&mut r, GamepadButton::DPadDown);
        tap(&mut r, GamepadButton::DPadLeft);
        assert_eq!(settings_look(&r).bloom, look::Bloom::Off);
        assert_eq!(settings_look(&r).preset(), look::Preset::Custom);
        // Back to the preset row and left: from CUSTOM it picks the last preset, MAX.
        tap(&mut r, GamepadButton::DPadUp);
        tap(&mut r, GamepadButton::DPadLeft);
        assert_eq!(settings_look(&r).preset(), look::Preset::Max);
        // ORIGINAL again with two more steps (MAX -> ORIGINAL wraps).
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(settings_look(&r), look::LookSettings::default());
    }

    #[test]
    fn the_main_page_has_no_look_changes_and_the_old_rows_still_work() {
        let mut r = Rig::new(1, &[0]);
        // Left/right on a main row (FOV, row 1) changes the FOV and leaves the look alone.
        tap(&mut r, GamepadButton::DPadDown);
        let fov = r.app.world().resource::<menu::Settings>().fov_deg;
        tap(&mut r, GamepadButton::DPadRight);
        assert_ne!(r.app.world().resource::<menu::Settings>().fov_deg, fov);
        assert_eq!(settings_look(&r), look::LookSettings::default());
    }

    // ---- Display rows browse and A applies; sliders; everything applies in a match (lane settingsux, 2026-10-03) ----

    use display::{Aspect, DisplayMode, DisplaySettings, HudSize, Resolution};

    /// The rows by index (menu::ROW_LIST): split 0, FOV 1, distance 2, height 3, mode 4, resolution 5, aspect 6, HUD 7, look 8, quit 9.
    const R_FOV: usize = 1;
    const R_DIST: usize = 2;
    const R_HEIGHT: usize = 3;
    const R_MODE: usize = 4;
    const R_SCREEN: usize = 5;
    const R_RES: usize = 6;
    const R_ASPECT: usize = 7;
    const R_HUD: usize = 8;

    #[test]
    fn the_screen_row_lists_each_connected_screen_and_a_saves_the_choice() {
        let mut r = Rig::new(1, &[0]).with_tv();
        let e = |n: u32| Entity::from_raw_u32(n).unwrap();
        let screen = |n: u32, x: i32, w: u32, h: u32| display::ScreenInfo { entity: e(n), pos: IVec2::new(x, 0), size: UVec2::new(w, h), reach: UVec2::new(w, h), primary: n == 1 };
        r.app.insert_resource(display::Screens(vec![screen(1, 0, 2880, 1800), screen(2, 2880, 3840, 2160)]));
        r.down(R_SCREEN);
        assert_eq!(r.row(), R_SCREEN);
        // Auto -> screen 1 -> screen 2, shown but not applied until A.
        tap(&mut r, GamepadButton::DPadRight);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.browsing().map(|d| d.screen), Some(display::Screen::Number(2)));
        assert_eq!(r.settings().display.screen, display::Screen::Auto, "browsing never applies");
        assert!(!r.settings_path().exists(), "nothing is written while browsing");
        r.frame(0, press(GamepadButton::South));
        assert_eq!(r.settings().display.screen, display::Screen::Number(2), "A applies it");
        let saved = std::fs::read_to_string(r.settings_path()).expect("saved when applied");
        assert!(saved.contains("\"screen\": \"2\"") && saved.contains("\"display_chosen\": true"), "{saved}");
    }

    #[test]
    fn browsing_a_resolution_shows_it_and_a_applies_it_at_once() {
        let mut r = Rig::new(1, &[0]).with_tv();
        r.down(R_RES);
        assert_eq!(r.row(), R_RES);
        tap(&mut r, GamepadButton::DPadRight);
        // The candidate is 4K (the TV's biggest mode, though its desktop is 1080p); nothing is applied and nothing saved yet.
        assert_eq!(r.browsing().map(|d| d.resolution), Some(Resolution::Fixed(3840, 2160)));
        assert_eq!(r.settings().display.resolution, Resolution::Native, "browsing never applies");
        assert!(!r.settings().display_chosen);
        assert!(!r.settings_path().exists(), "nothing is written while browsing");
        r.cues();
        r.frame(0, press(GamepadButton::South));
        assert_eq!(r.settings().display.resolution, Resolution::Fixed(3840, 2160), "A applies it");
        assert!(r.settings().display_chosen && r.browsing().is_none());
        assert!(r.cues().contains(&Cue::Confirm));
        let saved = std::fs::read_to_string(r.settings_path()).expect("saved when applied");
        assert!(saved.contains("3840x2160") && saved.contains("\"display_chosen\": true"), "{saved}");
        // A second A with nothing browsed does nothing.
        r.frame(0, press(GamepadButton::South));
        assert_eq!(r.settings().display.resolution, Resolution::Fixed(3840, 2160));
    }

    #[test]
    fn leaving_a_browsed_row_drops_the_choice_and_changes_nothing() {
        let mut r = Rig::new(1, &[0]).with_tv();
        let before = r.settings().display;
        r.down(R_RES);
        tap(&mut r, GamepadButton::DPadRight);
        assert!(r.browsing().is_some());
        tap(&mut r, GamepadButton::DPadDown);
        assert_eq!((r.browsing(), r.settings().display), (None, before), "moving to the next row drops the candidate");
        // Back on the row it starts from what is applied again, and A with no candidate does nothing.
        tap(&mut r, GamepadButton::DPadUp);
        r.frame(0, press(GamepadButton::South));
        assert_eq!(r.settings().display, before);
        // B (close the overlay) drops it too.
        tap(&mut r, GamepadButton::DPadRight);
        assert!(r.browsing().is_some());
        r.frame(0, press(GamepadButton::East));
        assert!(!r.app.world().resource::<menu::MenuState>().open);
        assert_eq!((r.browsing(), r.settings().display), (None, before));
        assert!(!r.settings_path().exists(), "nothing was ever written");
    }

    #[test]
    fn every_display_row_browses_then_applies() {
        let mut r = Rig::new(1, &[0]).with_tv();
        let apply = |r: &mut Rig| r.frame(0, press(GamepadButton::South));
        r.down(R_MODE);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!((r.browsing().map(|d| d.mode), r.settings().display.mode), (Some(DisplayMode::Borderless), DisplayMode::Windowed));
        apply(&mut r);
        assert_eq!(r.settings().display.mode, DisplayMode::Borderless);
        // One screen listed (none here): nothing to browse on the screen row.
        r.down(1);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.browsing(), None);
        // Borderless is the desktop's size: nothing to browse on the resolution row.
        r.down(1);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.browsing(), None);
        r.down(1);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.browsing().map(|d| d.aspect), Some(Aspect::W16H9));
        assert_eq!(r.settings().display.aspect, Aspect::Auto);
        apply(&mut r);
        assert_eq!(r.settings().display.aspect, Aspect::W16H9);
        r.down(1);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.settings().display.hud, HudSize::P100, "not yet");
        apply(&mut r);
        assert_eq!(r.settings().display.hud, HudSize::P125);
        // Exclusive: the resolution row lists the TV's modes and A switches to one.
        r.app.world_mut().resource_mut::<menu::Settings>().display.mode = DisplayMode::Exclusive;
        tap(&mut r, GamepadButton::DPadUp);
        tap(&mut r, GamepadButton::DPadUp);
        assert_eq!(r.row(), R_RES);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.browsing().map(|d| d.resolution), Some(Resolution::Fixed(3840, 2160)));
        apply(&mut r);
        assert_eq!(r.settings().display.window_part(), (DisplayMode::Exclusive, Resolution::Fixed(3840, 2160)));
    }

    #[test]
    fn the_sliders_move_by_one_on_a_tap_and_speed_up_when_held() {
        let mut r = Rig::new(1, &[0]);
        r.down(R_FOV);
        let fov = r.settings().fov_deg;
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.settings().fov_deg, fov + 1.0, "a tap is one degree");
        tap(&mut r, GamepadButton::DPadLeft);
        assert_eq!(r.settings().fov_deg, fov);
        // Held: one step on the first frame, then it repeats and accelerates.
        let mut last = r.settings().fov_deg;
        let mut gaps = Vec::new();
        for _ in 0..70 {
            r.hold(GamepadButton::DPadRight);
            let now = r.settings().fov_deg;
            if now != last {
                gaps.push(now - last);
                last = now;
            }
        }
        assert_eq!(gaps[0], 1.0);
        assert!(gaps.windows(2).all(|w| w[1] >= w[0] || last >= menu::FOV_MAX), "never slows down: {gaps:?}");
        assert!(gaps.iter().any(|&g| g >= 2.0), "speeds up: {gaps:?}");
        r.release();
        // The camera-distance slider is the next row, in percent, and clamps at its ends.
        tap(&mut r, GamepadButton::DPadDown);
        assert_eq!(r.row(), R_DIST);
        assert_eq!(r.settings().cam_dist_pct, menu::CAM_DIST_DEFAULT);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.settings().cam_dist_pct, menu::CAM_DIST_DEFAULT + 1.0);
        for _ in 0..400 {
            r.hold(GamepadButton::DPadLeft);
        }
        assert_eq!(r.settings().cam_dist_pct, menu::CAM_DIST_MIN, "held to the end and stopped there");
        r.release();
    }

    #[test]
    fn a_slider_is_saved_when_it_is_let_go_not_on_every_step() {
        let mut r = Rig::new(1, &[0]);
        r.down(R_FOV);
        r.hold(GamepadButton::DPadRight);
        r.hold(GamepadButton::DPadRight);
        assert!(!r.settings_path().exists(), "no write while the direction is held");
        let fov = r.settings().fov_deg;
        r.release();
        let text = std::fs::read_to_string(r.settings_path()).expect("saved on release");
        assert!(text.contains(&format!("\"fov_h16\": {fov}")), "{text}");
    }

    #[test]
    fn every_main_row_takes_effect_at_once_in_a_match() {
        let mut r = Rig::new(1, &[0]).with_tv();
        assert_eq!(*r.app.world().resource::<State<AppState>>().get(), AppState::Match);
        let s0 = r.settings().clone();
        // Split (row 0), the two sliders and camera height: at once.
        tap(&mut r, GamepadButton::DPadRight);
        assert_ne!(r.settings().split, s0.split);
        r.down(R_FOV);
        tap(&mut r, GamepadButton::DPadRight);
        assert_ne!(r.settings().fov_deg, s0.fov_deg);
        r.down(1);
        tap(&mut r, GamepadButton::DPadRight);
        assert_ne!(r.settings().cam_dist_pct, s0.cam_dist_pct);
        r.down(1);
        assert_eq!(r.row(), R_HEIGHT);
        tap(&mut r, GamepadButton::DPadRight);
        assert_ne!(r.settings().camera_height, s0.camera_height);
        // The display rows: A applies each.
        let rows: [(usize, fn(&DisplaySettings) -> (DisplayMode, Resolution, Aspect, HudSize)); 3] = [(R_MODE, |d| (d.mode, d.resolution, d.aspect, d.hud)), (R_ASPECT, |d| (d.mode, d.resolution, d.aspect, d.hud)), (R_HUD, |d| (d.mode, d.resolution, d.aspect, d.hud))];
        for (row, key) in rows {
            while r.row() != row {
                tap(&mut r, GamepadButton::DPadDown);
            }
            let before = key(&r.settings().display);
            tap(&mut r, GamepadButton::DPadRight);
            r.frame(0, press(GamepadButton::South));
            assert_ne!(key(&r.settings().display), before, "row {row} applied in a match");
        }
        // Lighting and look: every row changes at once.
        open_look_page(&mut r);
        for row in 0..look::LOOK_ROWS.len() - 1 {
            let before = settings_look(&r);
            tap(&mut r, GamepadButton::DPadRight);
            assert_ne!(settings_look(&r), before, "look row {row}");
            tap(&mut r, GamepadButton::DPadDown);
        }
    }

    /// The render side of "at once": with the overlay open the cameras follow the settings the same frame.
    #[test]
    fn split_fov_and_the_look_reach_the_cameras_while_paused() {
        use bevy::core_pipeline::tonemapping::Tonemapping;
        let mut app = App::new();
        let combat = combat::Combat::new(Vec::new(), vec![combat::AURA_PARTICLE; 30]);
        let path = std::env::temp_dir().join(format!("pd_cam_test_{}.json", std::process::id()));
        let mut settings = menu::Settings::load(path);
        settings.disable_saving();
        settings.fov_deg = 60.0;
        app.add_message::<WindowResized>()
            .insert_resource(settings)
            .init_resource::<seats::LocalViews>()
            .insert_resource(World(Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat)))
            .insert_resource(display::GameArea::from_window(UVec2::new(1600, 900), 1.0, Aspect::Auto))
            .add_systems(Update, (layout_viewports, apply_fov, look::apply_look));
        let cams: Vec<Entity> = (0..2).map(|i| app.world_mut().spawn((PlayerCamera(i), Camera::default(), Projection::Perspective(PerspectiveProjection::default()))).id()).collect();
        app.update();
        let rect = |app: &App, e: Entity| app.world().get::<Camera>(e).unwrap().viewport.clone().map(|v| (v.physical_position, v.physical_size)).unwrap();
        assert_eq!(rect(&app, cams[0]).1, UVec2::new(800, 900), "side by side");
        let fov = |app: &App, e: Entity| match app.world().get::<Projection>(e).unwrap() {
            Projection::Perspective(p) => p.fov,
            _ => 0.0,
        };
        let f0 = fov(&app, cams[0]);
        // The player changes everything from the menu: split, FOV, a look row.
        {
            let mut s = app.world_mut().resource_mut::<menu::Settings>();
            s.split = menu::Split::TopBottom;
            s.fov_deg = 90.0;
            s.look.tone = look::Tone::Filmic;
        }
        app.update();
        assert_eq!(rect(&app, cams[0]).1, UVec2::new(1600, 450), "top and bottom, the same frame");
        assert_eq!(rect(&app, cams[1]).0, UVec2::new(0, 450));
        // Back to side by side: the slider's value now shows in the field of view.
        app.world_mut().resource_mut::<menu::Settings>().split = menu::Split::SideBySide;
        app.update();
        assert!(fov(&app, cams[0]) > f0, "90 degrees is wider than 60");
        assert_eq!(app.world().get::<Tonemapping>(cams[0]), Some(&Tonemapping::AgX));
    }

    #[test]
    fn the_menu_script_can_open_the_overlay_in_a_match_and_drive_its_rows() {
        let mut r = Rig::new(1, &[0]).with_tv();
        r.open(false);
        r.app.insert_resource(frontend::script::MenuScript::new("start,down,down,down,down,down,down,right,confirm"));
        // One token per 12 frames.
        for _ in 0..9 * 13 {
            r.frame(0, |_| {});
        }
        assert!(r.app.world().resource::<menu::MenuState>().open, "start opened it");
        assert_eq!(r.row(), R_RES);
        assert_eq!(r.settings().display.resolution, Resolution::Fixed(3840, 2160), "right browsed, confirm applied");
    }

    // ---- Options tabs, sound, camera and button mapping (lane settingstabs, 2026-10-08) ----

    fn state(r: &Rig) -> &menu::MenuState {
        r.app.world().resource::<menu::MenuState>()
    }

    /// One frame with keyboard key `k` pressed (pads idle), then one with it released.
    fn key_tap(r: &mut Rig, k: KeyCode) {
        key_down(r, k);
        let mut keys = r.app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release_all();
        keys.clear();
        r.app.update();
    }

    /// One frame with only `k` held (first frame: just pressed).
    fn key_down(r: &mut Rig, k: KeyCode) {
        for e in r.pads.clone() {
            let mut g = r.app.world_mut().get_mut::<Gamepad>(e).unwrap();
            g.digital_mut().release_all();
            g.digital_mut().clear();
        }
        let mut keys = r.app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release_all();
        keys.clear();
        keys.press(k);
        r.app.update();
    }

    fn pending(r: &Rig) -> Input {
        r.app.world().resource::<PendingInputs>().0[0]
    }

    fn pitch(r: &Rig) -> f32 {
        r.app.world().resource::<CamPitchInput>().0[0]
    }

    #[test]
    fn lb_and_rb_switch_tabs_both_ways_and_wrap() {
        let mut r = Rig::new(1, &[0]);
        assert_eq!(state(&r).tab(), menu::Tab::Video, "the overlay opens on VIDEO SETTINGS");
        r.down(3);
        r.cues();
        // RB: Video -> Sound -> Controls -> Video, each on its first row, with the program's switch-list sound.
        for want in [menu::Tab::Sound, menu::Tab::Controls, menu::Tab::Video] {
            tap(&mut r, GamepadButton::RightTrigger);
            assert_eq!((state(&r).tab(), r.row()), (want, 0));
            assert_eq!(r.cues(), vec![Cue::List]);
        }
        // LB the other way, wrapping from Video to Controls.
        for want in [menu::Tab::Controls, menu::Tab::Sound, menu::Tab::Video] {
            tap(&mut r, GamepadButton::LeftTrigger);
            assert_eq!(state(&r).tab(), want);
        }
        // The keyboard: E right, Q left. Any pad works (player 2's).
        key_tap(&mut r, KeyCode::KeyE);
        assert_eq!(state(&r).tab(), menu::Tab::Sound);
        key_tap(&mut r, KeyCode::KeyQ);
        assert_eq!(state(&r).tab(), menu::Tab::Video);
        let mut r2 = Rig::new(2, &[0, 1]);
        r2.frame(1, press(GamepadButton::RightTrigger));
        assert_eq!(state(&r2).tab(), menu::Tab::Sound, "player 2's RB");
        // From the Lighting and look page, a tab switch lands on the other tab's main page.
        open_look_page(&mut r);
        tap(&mut r, GamepadButton::RightTrigger);
        assert_eq!((state(&r).tab(), state(&r).page()), (menu::Tab::Sound, menu::Page::Main));
        // Closing keeps the tab: the overlay reopens where it was left.
        r.frame(0, press(GamepadButton::Start));
        r.frame(0, press(GamepadButton::Start));
        assert!(state(&r).open);
        assert_eq!(state(&r).tab(), menu::Tab::Sound);
    }

    #[test]
    fn every_tab_ends_with_quit_to_title_in_a_match() {
        // Rows per tab in a match: Video 10 + quit, Sound 3 + quit, Controls 4 + quit.
        for (presses, rows) in [(0, 11), (1, 4), (2, 5)] {
            let mut r = Rig::new(1, &[0]);
            for _ in 0..presses {
                tap(&mut r, GamepadButton::RightTrigger);
            }
            // Up from the top wraps to the last row: quit.
            tap(&mut r, GamepadButton::DPadUp);
            assert_eq!(r.row(), rows - 1, "tab {presses}");
            tap(&mut r, GamepadButton::DPadDown);
            assert_eq!(r.row(), 0, "rows wrap on the tab's own length");
            tap(&mut r, GamepadButton::DPadUp);
            r.cues();
            r.frame(0, press(GamepadButton::South));
            assert!(!state(&r).open, "quit closes the overlay");
            assert!(matches!(*r.app.world().resource::<NextState<AppState>>(), NextState::Pending(AppState::Title)), "and goes to the title");
        }
    }

    #[test]
    fn the_video_tab_has_no_vibration_and_the_controls_tab_does() {
        let mut r = Rig::new(1, &[0]);
        // Video row 9 is quit (vibration used to be there).
        let rumble = r.settings().rumble;
        r.down(9);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.settings().rumble, rumble, "nothing on the Video tab touches vibration");
        // Controls row 0: vibration, toggled and saved at once.
        tap(&mut r, GamepadButton::RightTrigger);
        tap(&mut r, GamepadButton::RightTrigger);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.settings().rumble, !rumble);
        let saved = std::fs::read_to_string(r.settings_path()).unwrap();
        assert!(saved.contains(&format!("\"rumble\": {}", !rumble)), "{saved}");
        // Row 1: invert camera Y.
        tap(&mut r, GamepadButton::DPadDown);
        tap(&mut r, GamepadButton::DPadLeft);
        assert!(r.settings().invert_cam_y);
        // Row 2: sensitivity, a slider.
        tap(&mut r, GamepadButton::DPadDown);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.settings().cam_sens_pct, menu::SENS_DEFAULT + 1.0);
    }

    #[test]
    fn the_sound_sliders_move_and_save_when_let_go() {
        let mut r = Rig::new(1, &[0]);
        tap(&mut r, GamepadButton::RightTrigger);
        assert_eq!((r.settings().master_vol, r.settings().music_vol, r.settings().effects_vol), (100.0, 100.0, 100.0));
        tap(&mut r, GamepadButton::DPadLeft);
        assert_eq!(r.settings().master_vol, 99.0);
        tap(&mut r, GamepadButton::DPadRight);
        tap(&mut r, GamepadButton::DPadRight);
        assert_eq!(r.settings().master_vol, 100.0, "100 is the top");
        tap(&mut r, GamepadButton::DPadDown);
        for _ in 0..400 {
            r.hold(GamepadButton::DPadLeft);
        }
        r.release();
        assert_eq!(r.settings().music_vol, 0.0, "held down to silent");
        let saved = std::fs::read_to_string(r.settings_path()).unwrap();
        assert!(saved.contains("\"vol_music\": 0.0"), "{saved}");
        tap(&mut r, GamepadButton::DPadDown);
        tap(&mut r, GamepadButton::DPadLeft);
        assert_eq!(r.settings().effects_vol, 99.0);
    }

    /// Every pad button and key, through the real `gather_input`, with the default settings: exactly the `Input`
    /// the old fixed code made (sensitivity 100 %, invert off, the default mapping).
    #[test]
    fn the_default_settings_give_byte_identical_inputs_for_every_button() {
        let mut r = Rig::new(1, &[0]);
        r.open(false);
        let want = |jump: bool, lock: bool, skills: u8, recenter: bool, switch_target: bool| Input { skills, recenter, switch_target, ..Input::from_axes(0.0, 0.0, jump, lock) };
        let pads: [(GamepadButton, Input); 18] = [
            (GamepadButton::South, want(false, false, 8, false, false)),
            (GamepadButton::East, want(false, false, 2, false, false)),
            (GamepadButton::West, want(false, false, 1, false, false)),
            (GamepadButton::North, want(false, false, 4, false, false)),
            (GamepadButton::LeftThumb, want(true, false, 0, false, false)),
            (GamepadButton::LeftTrigger, want(true, false, 0, false, false)),
            (GamepadButton::RightTrigger2, want(false, true, 0, false, false)),
            (GamepadButton::LeftTrigger2, want(false, false, 0, false, true)),
            (GamepadButton::RightThumb, want(false, false, 0, true, false)),
            (GamepadButton::RightTrigger, Input::default()),
            (GamepadButton::Select, Input::default()),
            (GamepadButton::Mode, Input::default()),
            (GamepadButton::C, Input::default()),
            (GamepadButton::Z, Input::default()),
            (GamepadButton::DPadUp, Input::default()),
            (GamepadButton::DPadDown, Input::default()),
            (GamepadButton::DPadLeft, Input::default()),
            (GamepadButton::DPadRight, Input::default()),
        ];
        for (b, input) in pads {
            r.frame(0, press(b));
            assert_eq!(pending(&r), input, "{b:?}");
        }
        let keys: [(KeyCode, Input); 14] = [
            (KeyCode::Space, want(true, false, 0, false, false)),
            (KeyCode::KeyY, want(false, true, 0, false, false)),
            (KeyCode::KeyT, want(false, false, 0, false, true)),
            (KeyCode::KeyJ, want(false, false, 1, false, false)),
            (KeyCode::KeyK, want(false, false, 2, false, false)),
            (KeyCode::KeyU, want(false, false, 4, false, false)),
            (KeyCode::KeyI, want(false, false, 8, false, false)),
            (KeyCode::KeyE, Input { cam: 127, ..Input::default() }),
            (KeyCode::KeyQ, Input { cam: -127, ..Input::default() }),
            (KeyCode::KeyW, Input::from_axes(0.0, 1.0, false, false)),
            (KeyCode::KeyC, Input::default()),
            (KeyCode::KeyB, Input::default()),
            (KeyCode::Digit1, Input::default()),
            (KeyCode::ShiftLeft, Input::default()),
        ];
        for (k, input) in keys {
            key_down(&mut r, k);
            assert_eq!(pending(&r), input, "{k:?}");
        }
        key_down(&mut r, KeyCode::KeyR);
        assert_eq!(pitch(&r), 1.0, "R looks up, as before");
    }

    #[test]
    fn sensitivity_and_invert_change_the_free_camera_input_only() {
        use bevy::input::gamepad::{GamepadAxis, GamepadInput};
        let mut r = Rig::new(1, &[0]);
        r.open(false);
        let right_stick = |x: f32, y: f32| {
            move |g: &mut Gamepad| {
                g.analog_mut().set(GamepadInput::Axis(GamepadAxis::RightStickX), x);
                g.analog_mut().set(GamepadInput::Axis(GamepadAxis::RightStickY), y);
            }
        };
        r.frame(0, right_stick(0.5, 0.5));
        let (cam100, pitch100) = (pending(&r).cam, pitch(&r));
        assert_eq!((cam100, pitch100), (64, 0.5), "100 %: the old numbers");
        // 50 %: half the turn (quantised) and half the tilt.
        r.app.world_mut().resource_mut::<menu::Settings>().cam_sens_pct = 50.0;
        r.frame(0, right_stick(0.5, 0.5));
        assert_eq!((pending(&r).cam, pitch(&r)), (32, 0.25));
        // 200 %: half tilt = the original's top speed (normal scale); full tilt = twice it (double scale); the tilt doubles.
        r.app.world_mut().resource_mut::<menu::Settings>().cam_sens_pct = 200.0;
        r.frame(0, right_stick(0.5, 0.5));
        assert_eq!((pending(&r).cam, pending(&r).cam_fast, pitch(&r)), (127, false, 1.0));
        r.frame(0, right_stick(1.0, 1.0));
        assert_eq!((pending(&r).cam, pending(&r).cam_fast, pitch(&r)), (127, true, 2.0));
        // Invert: up looks down; the turn and the sim's other inputs are untouched.
        {
            let mut s = r.app.world_mut().resource_mut::<menu::Settings>();
            s.cam_sens_pct = 100.0;
            s.invert_cam_y = true;
        }
        r.frame(0, right_stick(0.5, 0.5));
        assert_eq!((pending(&r).cam, pitch(&r)), (64, -0.5));
        r.frame(0, right_stick(0.0, 0.0));
        key_down(&mut r, KeyCode::KeyR);
        assert_eq!(pitch(&r), -1.0, "keyboard R too");
        // Paused, nothing goes through (as before).
        r.open(true);
        r.frame(0, right_stick(1.0, 1.0));
        assert_eq!((pending(&r), pitch(&r)), (Input::default(), 0.0));
    }

    /// Down to CONTROLS > BUTTON MAPPING and A.
    fn open_buttons_page(r: &mut Rig) {
        tap(r, GamepadButton::LeftTrigger);
        assert_eq!(state(r).tab(), menu::Tab::Controls);
        r.down(3);
        r.frame(0, press(GamepadButton::South));
        assert_eq!(state(r).page(), menu::Page::Buttons);
        assert_eq!(r.row(), 0);
        r.release();
    }

    #[test]
    fn a_on_a_mapping_row_listens_and_the_next_button_is_bound_with_a_swap() {
        use bindings::Action;
        let mut r = Rig::new(1, &[0]);
        open_buttons_page(&mut r);
        // Row 0, DODGE: A listens; X is pressed: dodge is X and skill left takes the old L3.
        r.frame(0, press(GamepadButton::South));
        assert_eq!(state(&r).listening(), Some(Action::Dodge));
        r.release();
        assert_eq!(state(&r).listening(), Some(Action::Dodge), "still listening while nothing is pressed");
        r.cues();
        r.frame(0, press(GamepadButton::West));
        assert_eq!(state(&r).listening(), None);
        assert_eq!((r.settings().bindings.pad(Action::Dodge), r.settings().bindings.pad(Action::SkillLeft)), (GamepadButton::West, GamepadButton::LeftThumb));
        assert!(r.cues().contains(&Cue::Confirm));
        let saved = std::fs::read_to_string(r.settings_path()).unwrap();
        assert!(saved.contains("\"pad\": \"West\""), "{saved}");
        r.release();
        // A key for the keyboard column: SKILL LEFT (row 2) onto K swaps with SKILL RIGHT.
        r.down(2);
        r.frame(0, press(GamepadButton::South));
        key_tap(&mut r, KeyCode::KeyK);
        assert_eq!((r.settings().bindings.key(Action::SkillLeft), r.settings().bindings.key(Action::SkillRight)), (Some(KeyCode::KeyK), Some(KeyCode::KeyJ)));
        // B can be bound like any button while listening (it does not back out).
        r.frame(0, press(GamepadButton::South));
        r.release();
        r.frame(0, press(GamepadButton::East));
        assert_eq!(state(&r).page(), menu::Page::Buttons, "B was bound, not back");
        assert_eq!(r.settings().bindings.pad(Action::SkillLeft), GamepadButton::East);
        assert_eq!(r.settings().bindings.pad(Action::SkillRight), GamepadButton::LeftThumb, "swapped");
        r.release();
        // A fixed button is refused (the denied sound), and listening goes on; Start cancels without closing the overlay.
        r.frame(0, press(GamepadButton::South));
        r.release();
        r.cues();
        let before = r.settings().bindings;
        r.frame(0, press(GamepadButton::DPadUp));
        assert_eq!(state(&r).listening(), Some(Action::SkillLeft));
        assert!(r.cues().contains(&Cue::Denied));
        key_tap(&mut r, KeyCode::KeyW);
        assert_eq!(state(&r).listening(), Some(Action::SkillLeft), "W moves: refused");
        r.frame(0, press(GamepadButton::Start));
        assert_eq!((state(&r).listening(), state(&r).open), (None, true));
        assert_eq!(r.settings().bindings, before);
        // Esc cancels too, and so does waiting.
        r.frame(0, press(GamepadButton::South));
        key_tap(&mut r, KeyCode::Escape);
        assert_eq!((state(&r).listening(), state(&r).open), (None, true));
        r.frame(0, press(GamepadButton::South));
        for _ in 0..menu::LISTEN_FRAMES + 2 {
            r.release();
        }
        assert_eq!(state(&r).listening(), None, "gives up after about five seconds");
        assert_eq!(r.settings().bindings, before);
    }

    #[test]
    fn reset_puts_the_old_buttons_back_and_b_goes_back_to_controls() {
        use bindings::Action;
        let mut r = Rig::new(1, &[0]);
        r.app.world_mut().resource_mut::<menu::Settings>().bindings.bind_pad(Action::LockOn, GamepadButton::South).unwrap();
        open_buttons_page(&mut r);
        r.down(8);
        assert_eq!(menu::button_row(r.row()), menu::ButtonRow::Reset);
        r.frame(0, press(GamepadButton::South));
        assert!(r.settings().bindings.is_default());
        assert!(std::fs::read_to_string(r.settings_path()).unwrap().contains("\"pad\": \"RightTrigger2\""));
        r.release();
        r.frame(0, press(GamepadButton::East));
        assert_eq!((state(&r).page(), state(&r).tab(), r.row()), (menu::Page::Main, menu::Tab::Controls, 3), "back on the BUTTON MAPPING row");
        // No BACK row (2026-10-09, de-clutter): up from the top wraps to RESET, the last row.
        r.frame(0, press(GamepadButton::South));
        r.release();
        tap(&mut r, GamepadButton::DPadUp);
        assert_eq!(menu::button_row(r.row()), menu::ButtonRow::Reset);
    }

    #[test]
    fn a_remapped_button_drives_the_match_and_the_menus_keep_a_b_start() {
        use bindings::Action;
        let mut r = Rig::new(1, &[0]);
        {
            let mut s = r.app.world_mut().resource_mut::<menu::Settings>();
            // Dodge on X (skill left takes L3), skill bottom on RB, skill right on LT (switch target takes B), Space -> skill top.
            s.bindings.bind_pad(Action::Dodge, GamepadButton::West).unwrap();
            s.bindings.bind_pad(Action::SkillBottom, GamepadButton::RightTrigger).unwrap();
            s.bindings.bind_pad(Action::SkillRight, GamepadButton::LeftTrigger2).unwrap();
            s.bindings.bind_key(Action::SkillTop, KeyCode::Space).unwrap();
        }
        r.open(false);
        r.frame(0, press(GamepadButton::West));
        assert_eq!(pending(&r), Input::from_axes(0.0, 0.0, true, false), "X is a jump now, not a skill");
        r.frame(0, press(GamepadButton::LeftThumb));
        assert_eq!(pending(&r).skills, 1, "L3 is skill left");
        r.frame(0, press(GamepadButton::South));
        assert_eq!(pending(&r), Input::default(), "A is free now");
        r.frame(0, press(GamepadButton::RightTrigger));
        assert_eq!(pending(&r).skills, 8);
        r.frame(0, press(GamepadButton::East));
        assert!(pending(&r).switch_target && pending(&r).skills == 0);
        key_down(&mut r, KeyCode::Space);
        assert_eq!((pending(&r).jump, pending(&r).skills), (false, 4));
        key_down(&mut r, KeyCode::KeyU);
        assert_eq!(pending(&r).jump, true, "Space's old action took U");
        // The menus: Start opens, the D-pad moves, A confirms, B backs out, whatever the mapping says.
        r.frame(0, press(GamepadButton::Start));
        assert!(state(&r).open);
        open_look_page(&mut r);
        assert_eq!(state(&r).page(), menu::Page::Look, "A still confirms");
        r.frame(0, press(GamepadButton::East));
        assert_eq!(state(&r).page(), menu::Page::Main, "B still goes back");
        r.frame(0, press(GamepadButton::East));
        assert!(!state(&r).open, "B still closes");
    }

    #[test]
    fn the_menu_script_switches_tabs_and_binds_with_its_pad_buttons() {
        let mut r = Rig::new(1, &[0]);
        r.open(false);
        r.app.insert_resource(frontend::script::MenuScript::new("start,lb,down,down,down,confirm,confirm,y"));
        for _ in 0..8 * 13 {
            r.frame(0, |_| {});
        }
        assert_eq!((state(&r).tab(), state(&r).page()), (menu::Tab::Controls, menu::Page::Buttons));
        assert_eq!(r.settings().bindings.pad(bindings::Action::Dodge), GamepadButton::North, "y bound to dodge");
    }
}

/// The camera distance and FOV settings, through the real `follow_cameras` / `apply_fov` systems
/// (lane camfov, 2026-10-03).
#[cfg(test)]
mod camera_setting_tests {
    use super::*;
    use std::time::Duration;

    struct CamRig {
        app: App,
    }

    impl CamRig {
        /// A duel (P1 at the origin, P2 200 units ahead) with one camera per entry of `views` (viewport sizes).
        fn new(split: menu::Split, views: &[(u32, u32)]) -> Self {
            let mut app = App::new();
            let combat = combat::Combat::new(Vec::new(), vec![combat::AURA_PARTICLE; 30]);
            let path = std::env::temp_dir().join(format!("pd_camset_test_{}_{:?}.json", std::process::id(), split));
            let _ = std::fs::remove_file(&path);
            let mut settings = menu::Settings::load(path);
            settings.split = split;
            app.init_resource::<Time>()
                .init_resource::<Time<Fixed>>()
                .init_resource::<CamPitchInput>()
                .init_resource::<cam_occluders::CamOccluders>()
                .init_resource::<seats::LocalViews>()
                .insert_resource(settings)
                .insert_resource(StageCollision(Collision::new(Vec::new())))
                .insert_resource(World(Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat)))
                .add_systems(Update, (follow_cameras, apply_fov));
            for (i, &(w, h)) in views.iter().enumerate() {
                let vp = Viewport { physical_size: UVec2::new(w, h), ..default() };
                app.world_mut().spawn((PlayerCamera(i), Transform::default(), Camera { viewport: Some(vp), ..default() }, Projection::Perspective(PerspectiveProjection::default())));
            }
            // Free camera by default (the sim starts a duel locked on).
            app.world_mut().resource_mut::<World>().0.players[0].locked = false;
            CamRig { app }
        }

        /// One frame long enough that the camera's smoothing has fully settled.
        fn frame(&mut self) {
            self.app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(10));
            self.app.update();
        }

        fn settings(&mut self) -> Mut<'_, menu::Settings> {
            self.app.world_mut().resource_mut::<menu::Settings>()
        }

        fn eye(&mut self, i: usize) -> Vec3 {
            let mut q = self.app.world_mut().query::<(&PlayerCamera, &Transform)>();
            q.iter(self.app.world()).find(|(c, _)| c.0 == i).map(|(_, t)| t.translation).unwrap()
        }

        fn fov(&mut self, i: usize) -> f32 {
            let mut q = self.app.world_mut().query::<(&PlayerCamera, &Projection)>();
            match q.iter(self.app.world()).find(|(c, _)| c.0 == i).map(|(_, p)| p).unwrap() {
                Projection::Perspective(p) => p.fov,
                _ => panic!("perspective"),
            }
        }

        fn me_fwd(&self) -> (Vec3, Vec3) {
            let w = &self.app.world().resource::<World>().0;
            let yaw = w.players[0].cam_yaw;
            (w.players[0].pos, Vec3::new(yaw.sin(), 0.0, yaw.cos()))
        }
    }

    const SBS: (u32, u32) = (960, 1080);
    const STRIP: (u32, u32) = (1920, 540);

    /// At 100 the free camera is exactly where it was before the setting existed: 55 behind and 13
    /// above the fighter, level (top/bottom at the default MID height is the same).
    #[test]
    fn distance_100_is_todays_free_camera() {
        for (split, views) in [(menu::Split::SideBySide, [SBS, SBS]), (menu::Split::TopBottom, [STRIP, STRIP])] {
            let mut r = CamRig::new(split, &views);
            assert_eq!(r.settings().cam_dist_pct, 100.0, "the default");
            r.frame();
            let (me, fwd) = r.me_fwd();
            let today = me + Vec3::new(-fwd.x * ORIG_DIST_FREE, ORIG_UP_FREE, -fwd.z * ORIG_DIST_FREE);
            let eye = r.eye(0);
            assert!(eye.distance(today) < 1e-3, "{split:?}: eye {eye:?} vs today {today:?}");
        }
    }

    /// At 100 the locked camera (the approved 60 / 30, chest-aim) is the old formula too.
    #[test]
    fn distance_100_is_todays_locked_camera() {
        let mut r = CamRig::new(menu::Split::SideBySide, &[SBS, SBS]);
        r.app.world_mut().resource_mut::<World>().0.players[0].locked = true;
        r.frame();
        let (me, fwd) = r.me_fwd();
        let (head, foe_chest) = (me + Vec3::Y * 16.0, Vec3::new(0.0, LOCK_AIM_HEIGHT, 200.0));
        let to_foe = foe_chest - head;
        let pitch = -to_foe.y.atan2(Vec2::new(to_foe.x, to_foe.z).length().max(1.0));
        let tilt = Quat::from_axis_angle(Vec3::new(fwd.z, 0.0, -fwd.x), pitch);
        // The code as it was before the distance setting (60 behind, 30 above).
        let today = head + tilt * (-fwd * 60.0 + Vec3::Y * (30.0 - 16.0));
        let eye = r.eye(0);
        assert!(eye.distance(today) < 1e-3, "eye {eye:?} vs today {today:?}");
    }

    /// The setting scales the boom and the height by one factor: the camera keeps its angle to the
    /// fighter (free camera, level), in side-by-side and in top/bottom.
    #[test]
    fn distance_scales_the_boom_and_the_height_together() {
        for (split, views) in [(menu::Split::SideBySide, [SBS, SBS]), (menu::Split::TopBottom, [STRIP, STRIP])] {
            let mut r = CamRig::new(split, &views);
            for pct in [70.0f32, 100.0, 130.0, 160.0] {
                r.settings().cam_dist_pct = pct;
                r.frame();
                let (me, fwd) = r.me_fwd();
                let k = pct / 100.0;
                let rel = r.eye(0) - me;
                let want = Vec3::new(-fwd.x * ORIG_DIST_FREE * k, ORIG_UP_FREE * k, -fwd.z * ORIG_DIST_FREE * k);
                assert!(rel.distance(want) < 1e-2, "{split:?} {pct}: {rel:?} vs {want:?}");
                let angle = (rel.y / Vec2::new(rel.x, rel.z).length()).atan();
                assert!((angle - (ORIG_UP_FREE / ORIG_DIST_FREE).atan()).abs() < 1e-4, "{split:?} {pct}: the angle to the fighter is kept");
            }
        }
    }

    /// The top/bottom camera-height drop scales with the rest (a MID default is unchanged).
    #[test]
    fn the_strip_height_drop_scales_too() {
        let mut r = CamRig::new(menu::Split::TopBottom, &[STRIP, STRIP]);
        r.settings().camera_height = menu::CameraHeight::VeryLow;
        for pct in [100.0f32, 160.0] {
            r.settings().cam_dist_pct = pct;
            r.frame();
            let (me, _) = r.me_fwd();
            let k = pct / 100.0;
            // VERY LOW is 7 below MID: 13 - 7 = 6 above the fighter at 100.
            assert!(((r.eye(0) - me).y - 6.0 * k).abs() < 1e-2, "{pct}");
        }
    }

    /// Both settings take effect on the next frame, with no cached value.
    #[test]
    fn changes_apply_on_the_next_frame() {
        let mut r = CamRig::new(menu::Split::SideBySide, &[SBS, SBS]);
        r.frame();
        let (me, _) = r.me_fwd();
        let (eye0, fov0) = (r.eye(0), r.fov(0));
        r.settings().cam_dist_pct = 160.0;
        r.settings().fov_deg = 110.0;
        r.frame();
        assert!(((r.eye(0) - me).length() / (eye0 - me).length() - 1.6).abs() < 1e-3, "the boom is 1.6 x as long");
        assert!(r.fov(0) > fov0, "the wider setting widens the view");
        assert!((r.fov(0) - views::fov_for(110.0, SBS.0 as f32 / SBS.1 as f32)).abs() < 1e-6);
        // And back: the camera does not stick.
        r.settings().cam_dist_pct = 100.0;
        r.settings().fov_deg = menu::FOV_DEFAULT;
        r.frame();
        assert!(r.eye(0).distance(eye0) < 1e-3);
        assert!((r.fov(0) - fov0).abs() < 1e-6);
    }

    /// Each camera takes its FOV from its own viewport: a 16:9 view, a 32:9 strip and an 8:9
    /// half each get their own vertical angle.
    #[test]
    fn each_camera_uses_its_own_viewport_aspect() {
        let mut r = CamRig::new(menu::Split::SideBySide, &[(1920, 1080), (3840, 1080)]);
        r.frame();
        let v16 = views::fov_for(menu::FOV_DEFAULT, views::REF_ASPECT);
        assert!((r.fov(0) - v16).abs() < 1e-6 && (r.fov(1) - v16).abs() < 1e-6, "16:9 and 32:9 share the 16:9 vertical angle");
        let mut r = CamRig::new(menu::Split::SideBySide, &[(960, 1080), (1920, 1080)]);
        r.frame();
        assert!(r.fov(0) > r.fov(1), "the narrow half is taller than the 16:9 view");
        assert!((r.fov(1) - v16).abs() < 1e-6);
    }

    /// The FOV setting file: new key, range, and an old `fov_deg` is ignored.
    #[test]
    fn the_fov_setting_file() {
        let path = std::env::temp_dir().join(format!("pd_fov_key_test_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(menu::Settings::load(path.clone()).fov_deg, menu::FOV_DEFAULT, "no file: the default");
        std::fs::write(&path, r#"{"fov_deg":52.0}"#).unwrap();
        assert_eq!(menu::Settings::load(path.clone()).fov_deg, menu::FOV_DEFAULT, "an old fov_deg (a vertical angle) is not misread");
        std::fs::write(&path, r#"{"fov_h16":90.0}"#).unwrap();
        assert_eq!(menu::Settings::load(path.clone()).fov_deg, 90.0);
        std::fs::write(&path, r#"{"fov_h16":10.0}"#).unwrap();
        assert_eq!(menu::Settings::load(path.clone()).fov_deg, menu::FOV_MIN, "clamped");
        std::fs::write(&path, r#"{"fov_h16":500.0}"#).unwrap();
        assert_eq!(menu::Settings::load(path.clone()).fov_deg, menu::FOV_MAX, "clamped");
        let mut s = menu::Settings::load(path.clone());
        s.fov_deg = 100.0;
        s.save();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("fov_h16") && !text.contains("fov_deg"), "{text}");
        assert_eq!(menu::Settings::load(path.clone()).fov_deg, 100.0);
        assert_eq!((menu::FOV_MIN, menu::FOV_MAX), (50.0, 110.0));
        let _ = std::fs::remove_file(&path);
    }
}

/// Prop Hunt's view switch through the real `layout_viewports` (lane prophunt): during the countdown every player has
/// their split view; once the seeker is released the seeker's camera fills the window and the hiders' cameras are off;
/// between rounds the split comes back.
#[cfg(test)]
mod prop_hunt_view_tests {
    use super::*;

    #[test]
    fn the_seekers_view_fills_the_window_on_release_and_the_hiders_views_vanish() {
        let mut app = App::new();
        let mut c = combat::Combat::with_roster(Vec::new(), combat::Roster::free_for_all(3), std::array::from_fn(|_| Vec::new()));
        c.set_rules(frontend::modes::MODES[1].rules());
        let mut m = Match::with_spawns(&[Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 100.0)], c);
        m.start_prop_hunt(prophunt::Config::default());
        let path = std::env::temp_dir().join(format!("pd_ph_views_{}.json", std::process::id()));
        let mut settings = menu::Settings::load(path);
        settings.disable_saving();
        app.add_message::<WindowResized>()
            .insert_resource(settings)
            .init_resource::<seats::LocalViews>()
            .init_resource::<prophunt_view::ViewBase>()
            .insert_resource(World(m))
            .insert_resource(display::GameArea::from_window(UVec2::new(1600, 900), 1.0, display::Aspect::Auto))
            .add_systems(Update, (prophunt_view::drive_views, layout_viewports).chain());
        let cams: Vec<Entity> = (0..3).map(|i| app.world_mut().spawn((PlayerCamera(i), Camera::default())).id()).collect();
        let view = |app: &App, e: Entity| {
            let cam = app.world().get::<Camera>(e).unwrap();
            (cam.is_active, cam.viewport.clone().map(|v| (v.physical_position, v.physical_size)))
        };
        app.update();
        // Countdown: three views (the 3-player layout), the seeker's included (the overlay covers it black).
        for &e in &cams {
            assert!(view(&app, e).0, "everyone's view is on during the countdown");
        }
        assert_eq!(view(&app, cams[0]).1, Some((UVec2::ZERO, UVec2::new(800, 450))));
        // Released.
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().phase = prophunt::Phase::Hunt { left: 100 };
        app.update();
        assert_eq!(view(&app, cams[0]), (true, Some((UVec2::ZERO, UVec2::new(1600, 900)))), "the seeker: the whole window");
        assert!(!view(&app, cams[1]).0 && !view(&app, cams[2]).0, "the hiders' views are gone");
        // The round's card, then the next round's countdown: the split is back.
        app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().phase = prophunt::Phase::Card { left: 10, why: prophunt::RoundEnd::TimeUp };
        app.update();
        assert!(cams.iter().all(|&e| view(&app, e).0));
        assert_eq!(view(&app, cams[2]).1, Some((UVec2::new(0, 450), UVec2::new(1600, 450))), "player 3's wide bottom half");
    }
}
