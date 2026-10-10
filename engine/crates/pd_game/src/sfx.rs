//! Sound effects and battle voices, played render-side from sim state (like
//! fx.rs does for effects). Reads `cache/sfx/index.json` + WAVs written by
//! `pd_convert sounds` (docs/FORMATS-AUDIO.md); without that folder it does
//! nothing.
//!
//! - a new cast (`cast_seq` changes): the action row's cast sound (w3) and
//!   the caster's battle voice from the row's voice group (w4), as the
//!   program's cast start does (`FUN_14025a3e0`); the skill's s19 sound plays
//!   later, at its fire tick (`CombatEvent::SkillFired`, `FUN_1402626d0`);
//! - while a skill's projectiles fly: its travel hum (s20, the looped 6xx sounds);
//! - `CombatEvent::Hit` / `Burst`: the skill's impact (s21, 5xx), common/6 and the
//!   victim's hurt voice (from the ten-entry list of the hit-reaction row's
//!   voice group); `Blocked`: the impact at full level, plus common/14 when the
//!   guard broke; the lane's other events (capsule appear / erase, environment,
//!   Aura tick, landing, get-up, blast, status effect ...) map in `cues_in_batch`;
//! - a knock-out: the fallen fighter's KO voice, the moment it falls.
//!
//! Which field is which sound was read in the program (FORMATS-AUDIO.md
//! "Skill table -> sound ids", audits/P0-SOUND.md); the program has no KO voice, ours
//! uses group 6 at Laurence's request.

use crate::combat::{CombatEvent, MAX_PLAYERS};
use crate::{Config, World};
use bevy::audio::Volume;
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// Skill-table fields holding sound ids (in the `common` bank). s8 is not
/// one: the program never plays it (FORMATS-AUDIO.md, 2026-09-27).
const FIELD_CAST_B: usize = 19;
const FIELD_TRAVEL: usize = 20;
const FIELD_IMPACT: usize = 21;

/// The program picks a battle voice from a "voice group" (1-6) -- the action
/// row's w4 for the caster at cast start (`0x14025a5c6`), the hit-reaction
/// row's own w4 (`Combat::reaction_voice_group`) for the victim -- through a
/// 10-entry list per group (the game's `gsdata_en.dat` at file offset
/// `0x33194` = the program's table `0x33004`, six rows of ten int32s,
/// read 2026-10-01). 0 = a silent pick; ids of other groups occur. The ids
/// are the voice bank's clip numbers (10-12, 20-22 ... 60-62).
pub const VOICE_LISTS: [[u32; 10]; 6] = [
    [10, 11, 10, 12, 11, 0, 20, 10, 11, 12],
    [20, 0, 21, 22, 21, 20, 30, 22, 20, 21],
    [31, 30, 22, 32, 31, 30, 32, 31, 40, 30],
    [42, 41, 50, 40, 42, 41, 40, 40, 41, 0],
    [50, 52, 51, 60, 40, 50, 51, 50, 52, 51],
    [61, 60, 51, 62, 60, 61, 60, 62, 60, 61],
];

/// The voice clip a roll picks from group `group` (1-6): `None` for a silent
/// entry or a group out of range.
pub fn voice_pick(group: u32, roll: u32) -> Option<u32> {
    let list = VOICE_LISTS.get((group as usize).checked_sub(1)?)?;
    Some(list[roll as usize % 10]).filter(|&id| id > 0)
}

/// The hurt voice's group when no reaction row is loaded (tests, or a skill
/// whose reaction row isn't in the table): ordinary hits are group 4,
/// knock-downs group 5 (the reaction table).
const VOICE_HURT: u32 = 4;
/// The KO voice group: the hit-reaction rows 14-19 (w4 = 6).
const VOICE_KO: u32 = 6;

/// A voice group number (1-6) from a table w4 value, `None` for none.
pub fn voice_of_group(group: i16) -> Option<u32> {
    (1..=6).contains(&group).then(|| group as u32)
}

pub(crate) const VOL_SFX: f32 = 0.55;
const VOL_TRAVEL: f32 = 0.25;
const VOL_VOICE: f32 = 0.7;

/// Footsteps sit under the fight rather than in it: quieter than a skill impact
/// (`VOL_SFX` 0.55) but audible over ambience. ⚠️ By ear — the original mixes
/// these through its own per-emitter priority (the `2` argument at the footstep
/// call site), which has no equivalent here.
pub const VOL_FOOTSTEP: f32 = 0.38;
/// One-shot sounds allowed at once (multi-hit skills stack fast).
const MAX_PLAYING: usize = 24;

pub fn plugin(app: &mut App) {
    // `PD_MUTE=1`: scripted capture runs play nothing aloud (Laurence is at this PC); the
    // sounds are still started and `PD_SFX_LOG=1` logs each one.
    if std::env::var_os("PD_MUTE").is_some() {
        app.insert_resource(bevy::audio::GlobalVolume::new(Volume::Linear(0.0)));
    }
    app.init_resource::<crate::ground_sfx::StageSound>()
        .add_systems(Startup, load_sfx)
        .add_systems(Update, (crate::ground_sfx::ground_cues.before(play_sfx), play_sfx, object_sfx, environment_hum).run_if(resource_exists::<SfxLib>));
    // The effects lane's arena-object effects ride along with this plugin
    // (main.rs already registers it).
    crate::fx::plugin(app);
}

/// Sound ids a skill uses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillSounds {
    /// The action row's own cast sound (w3), played at cast start.
    pub cast: Vec<u32>,
    /// s19, played at the fire tick of an attack-kind skill (`0x140262caa`).
    pub fire: Option<u32>,
    pub travel: Option<u32>,
    pub impact: Option<u32>,
    /// The caster's voice group at cast start (action row w4), 1-6.
    pub voice: Option<u32>,
    /// A skill that is not an attack (erase / status / special / environment) makes no
    /// hit reaction on its victim, so no hurt voice and no reaction sound.
    pub no_reaction: bool,
    /// The skill has an effect number (action row w2): the victim's reaction-row sound
    /// plays only together with that effect (`FUN_140259310`).
    pub has_effect: bool,
    /// The body-hit sound a shot plays when it hits a fighter (`0x14026d144`, event 2 of
    /// `FUN_14026cd70`): common/6 or common/7, only for travelling shots (see `hit_body_for`).
    pub hit_body: Option<u32>,
}

/// Which body-hit sound a skill's shot plays on a fighter. READ: event 2 of `FUN_14026cd70`
/// (`0x14026d144`) plays common/6 or 7 from the shot, not the melee/wave/mine/tornado/quake
/// objects. LIVE (all 341 takes of `.pdtools/moverec/takes_full`, every take whose log has 0.6
/// / 0.7 against the skill's s43 / s63): the shells (motion 0) and the Crawl shots (5) give
/// common/6; the Parabola (1), Fall (2) and Rain-from-above (20) shots give common/7; the Arc
/// beams (6), swords, punches, kicks, waves, mines, tornados, quakes (3, 4, 7-17) give neither.
/// An exploding shot (s51 = 5) is handled by the blast batch.
pub fn hit_body_for(attack_object: bool, motion: Option<i16>) -> Option<u32> {
    if !attack_object {
        return None;
    }
    match motion? {
        0 | 5 => Some(SFX_HIT_CHARACTER),
        1 | 2 | 20 => Some(SFX_HIT_OBJECT),
        _ => None,
    }
}

/// Reads the sound fields of a skill's raw table row plus its action row's
/// own cast sound, keeping ids the bank has. `cast_sound` is the ground
/// action-table row's `cast_sound` (0 = none): the sound actually played at
/// cast start (docs/FORMATS-SKILLS.md "Action table"), not s13 -- that field
/// is the mid-air action row, not a sound (PROGRAM-RE.md "Validation check":
/// s13 = 305 on every laser used to be read as its cast sound, but 305 is
/// really Fire Storm's; the lasers' own cast sound is `common/242`/`243`).
pub fn skill_sounds(raw: &[i16], cast_sound: i16, voice_group: i16, has: impl Fn(u32) -> bool) -> SkillSounds {
    let id = |f: usize, range: std::ops::RangeInclusive<u32>| {
        let v = *raw.get(f)? as i32;
        (v > 0 && range.contains(&(v as u32)) && has(v as u32)).then_some(v as u32)
    };
    let cast_sound = (cast_sound > 0 && has(cast_sound as u32)).then_some(cast_sound as u32);
    // s19 (313/314/618, the [Rain] whistle ...): the program plays it when the
    // attack object is made at the fire tick (`FUN_1402626d0`, `0x140262caa`),
    // for attack skills only (s43 = 0, motion != 4), not at the cast.
    let attack_object = raw.get(43).copied() == Some(0) && raw.get(pd_formats::skills::field::MOTION).copied() != Some(4);
    SkillSounds {
        cast: cast_sound.into_iter().collect(),
        fire: id(FIELD_CAST_B, 300..=699).filter(|_| attack_object),
        travel: id(FIELD_TRAVEL, 600..=699),
        impact: id(FIELD_IMPACT, 500..=599),
        voice: voice_of_group(voice_group),
        no_reaction: !matches!(raw.get(pd_formats::skills::field::CAPSULE).copied(), None | Some(0) | Some(1)),
        has_effect: false,
        hit_body: hit_body_for(attack_object, raw.get(pd_formats::skills::field::MOTION).copied()),
    }
}

/// Something to play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// A `common` bank sound at a volume.
    Sfx(u32, f32),
    /// A voice group (1-6) of a player's character: the clip is picked from the
    /// program's 10-entry list for the group (`voice_pick`).
    Voice(usize, u32),
}

/// Program ids of the layers the events lane added (all `common` bank, all
/// played through `FUN_140276330(0, id, id, 0x1000, 0x1000, ..)` at the
/// address named).
pub const SFX_AURA_TICK: u32 = 5;
/// Character hit (`0x14026d144`'s sibling for a playable victim).
pub const SFX_HIT_CHARACTER: u32 = 6;
/// Non-character (arena object) hit, `0x14026d144`.
pub const SFX_HIT_OBJECT: u32 = 7;
/// Defence destroyed, `0x14026d527`.
pub const SFX_GUARD_BROKEN: u32 = 14;
/// Getting up from a knock-down, `0x14025c4ee`.
pub const SFX_GET_UP: u32 = 81;
/// A knock-down flight touching down, `0x140254e3f`.
pub const SFX_KNOCK_LANDED: u32 = 82;
/// The landing sound of floor class 2 / 3 / 5, played on the same tick as the knock-down landing (82):
/// READ (`FUN_140254b30`, called at the end of `FUN_140254c80`; `ground_sfx::landing_sound` has the
/// whole class table). LIVE: 35 of the 36 landings in the 341 original takes (`sound:0.70`; 71 on
/// class 4 / 6). Played by `ground_sfx` from the surface under the body; this id is its fallback
/// where the surface sidecar has no triangle (the class most landings in the takes were on).
pub const SFX_KNOCK_GROUND: u32 = 70;
/// Environment crystal set, `0x14017bdc9`; its hum (2 loop layers), `0x14017b966`;
/// its end, `0x14017bb6d`; taken, `0x14017bf70`.
pub const SFX_ENV_SET: u32 = 93;
pub const SFX_ENV_HUM: u32 = 87;
pub const SFX_ENV_END: u32 = 94;
pub const SFX_CRYSTAL_TAKEN: u32 = 88;
/// An attack erased a button, `0x14026fb8e`.
pub const SFX_BUTTON_ERASED: u32 = 91;
/// A skill erased a capsule, `0x14014edc7` / `0x14014ff07`.
pub const SFX_CAPSULE_ERASED: u32 = 92;
/// A blast started, `0x14026dca4`.
pub const SFX_EXPLOSION: u32 = 99;

/// What one frame's new events say about each other: a blast's victims are hurt
/// by the blast (no impact sound, no common/6 -- those belong to the shot's own
/// hit, `FUN_14026cd70` codes 1-3 / 8), and a mine's or wave's blast plays common/99
/// only, never the skill's s21.
#[derive(Default)]
pub struct Batch {
    /// Skills whose shot exploded this frame (s51 = 5).
    pub shot_blasts: Vec<i16>,
    /// Mine / Photon Wave blasts this frame: (skill, centre).
    pub area_blasts: Vec<(i16, Vec3)>,
}

impl Batch {
    pub fn of(events: &[CombatEvent]) -> Self {
        let mut b = Batch::default();
        for ev in events {
            if let CombatEvent::BlastStarted { skill, pos, area, .. } = *ev {
                if area {
                    b.area_blasts.push((skill, pos));
                } else {
                    b.shot_blasts.push(skill);
                }
            }
        }
        b
    }

    fn blast_of(&self, skill: i16) -> bool {
        self.shot_blasts.contains(&skill) || self.area_blasts.iter().any(|b| b.0 == skill)
    }

    /// Blasts that started on an earlier frame and are still alive: `(skill, area)`. A blast lives
    /// `BLAST_LIFE_TICKS` and tests each player every program tick (`FUN_14026dbd0`; the sim's
    /// `Blast`), so a mine's victim (the hit comes tens of ticks after `BlastStarted`) and a
    /// shell's later victims are hurt by the BLAST, not by a body hit of the shot itself.
    pub fn with_active(mut self, active: &[(i16, bool)]) -> Self {
        for &(skill, area) in active {
            if area {
                self.area_blasts.push((skill, Vec3::splat(f32::NAN)));
            } else {
                self.shot_blasts.push(skill);
            }
        }
        self
    }
}

/// How long a blast keeps hurting players, in sim ticks: 30 program ticks of life (`FUN_14026dbd0`
/// state 3, life 30) at two sim ticks each, plus the arming tick and one of slack.
pub const BLAST_LIFE_TICKS: u64 = 64;

/// The sound of a status effect taking hold (`FUN_140271590`, `0x14027161f`): kind 1
/// (Level) 89 for a rise, 90 for a drop; 4-7 (attack / speed / homing / Aura rate) 95 up,
/// 96 down; 8 -> 98; 10 -> 89; 0x12 and 0x17 -> 97. Anything else is silent.
pub fn status_effect_sound(kind: i16, amount: i32) -> Option<u32> {
    match kind {
        1 => Some(if amount >= 1 { 89 } else { 90 }),
        4..=7 => Some(if amount >= 1 { 95 } else { 96 }),
        8 => Some(98),
        10 => Some(89),
        0x12 | 0x17 => Some(97),
        _ => None,
    }
}

/// The sound of an "Add" effect taking hold (`FUN_1402738d0`, end of `FUN_140272bc0`; code = s51,
/// amount = s52). READ: codes 2 and 4 negate the amount, then (like 0x2c, the Level add) play 89 for
/// a rise (>= 1) and 90 otherwise; 0xb, 0x2f, 0x31 -> 90; 0xc-0xf (attack / speed / homing / Aura rate)
/// 95 for a rise and 96 otherwise; 0x11, 0x27 -> 98; 0x33 -> 97; any other code is silent. LIVE
/// (takes_full): Interference (2, 2) 90, Level Berserk (4, -2) 89, Panic Gas (4, 2) 90, Meltdown (0x31) 90,
/// Song of Succubus (0xb) 90, Alleviate (0xd, 30) 95, Burden (0xd, -20) 96, Friendship (0xc, 1) 95,
/// Boost Mine / Quantum Amp (0x2c, 2) 89.
pub fn add_effect_sound(code: i16, amount: i32) -> Option<u32> {
    match code {
        2 | 4 => Some(if -amount >= 1 { 89 } else { 90 }),
        0x2c => Some(if amount >= 1 { 89 } else { 90 }),
        0xb | 0x2f | 0x31 => Some(90),
        0xc..=0xf => Some(if amount >= 1 { 95 } else { 96 }),
        0x11 | 0x27 => Some(98),
        0x33 => Some(97),
        _ => None,
    }
}

/// The take-off sound of a skill jump (`FUN_1402549e0`, called from `FUN_14025a0b0`): chosen by the
/// floor material, 61 for materials 2, 3 and 5. LIVE: every Jump / Levitate / Skywalk / Demon's Wing
/// take (stage st06) logs common/61 on the first tick of state 5. The other classes' ids (62-69, 141-150)
/// are READ and played from the surface sidecar by `ground_sfx::takeoff_sound`; 61 is the fallback
/// where the sidecar has no triangle under the body.
pub const SFX_JUMP_TAKEOFF: u32 = 61;

/// What a combat event sounds like.
/// `reaction(victim)`: (voice group 1-6, row sound id) of that player's current
/// hit-reaction row (`Combat::reaction_voice_group` / `reaction_sound`; 0 = none).
#[allow(dead_code)] // the game plays through `cues_in_batch`; this is the one-event form the tests use
pub fn cues_for_event(ev: &CombatEvent, sounds: impl Fn(i16) -> SkillSounds, reaction: impl Fn(usize) -> (i16, i16)) -> Vec<Cue> {
    cues_in_batch(ev, sounds, reaction, &Batch::default())
}

pub fn cues_in_batch(ev: &CombatEvent, sounds: impl Fn(i16) -> SkillSounds, reaction: impl Fn(usize) -> (i16, i16), batch: &Batch) -> Vec<Cue> {
    let mut out = Vec::new();
    match *ev {
        // A hit (`FUN_14026cd70` code 2): the skill's impact sound (s21) and common/6 for
        // a player character (every playable fighter: the character-table flag is 0; 7 is
        // for arena objects, `object_sfx`), then what the hit reaction plays: the victim's
        // voice from its row's list, and the row's own sound with the attacker's effect
        // (common/100 on the bind rows). A blast's victims get the reaction only.
        CombatEvent::Hit { victim, skill, .. } => {
            let s = sounds(skill);
            if !batch.blast_of(skill) {
                out.extend(s.impact.map(|n| Cue::Sfx(n, VOL_SFX)));
                out.extend(s.hit_body.map(|n| Cue::Sfx(n, VOL_SFX)));
            }
            if !s.no_reaction {
                let (group, row_sound) = reaction(victim);
                out.push(Cue::Voice(victim, voice_of_group(group).unwrap_or(VOICE_HURT)));
                if s.has_effect && (1..=9999).contains(&row_sound) {
                    out.push(Cue::Sfx(row_sound as u32, VOL_SFX));
                }
            }
        }
        // A block shares the impact branch of a hit (`FUN_14026cd70`): the skill's
        // impact at full level -- the 0.3 we used had no basis. A broken defence adds
        // common/14 (`0x14026d527`, code 10).
        CombatEvent::Blocked { skill, broken, .. } => {
            out.extend(sounds(skill).impact.map(|s| Cue::Sfx(s, VOL_SFX)));
            if broken {
                out.push(Cue::Sfx(SFX_GUARD_BROKEN, VOL_SFX));
            }
        }
        // Where something burst: the shot's impact (a mine's or wave's blast has none).
        CombatEvent::Burst { skill, pos, .. } => {
            if !batch.area_blasts.iter().any(|b| b.0 == skill && b.1.distance(pos) < 0.01) {
                out.extend(sounds(skill).impact.map(|s| Cue::Sfx(s, VOL_SFX)));
            }
        }
        CombatEvent::BlastStarted { .. } => out.push(Cue::Sfx(SFX_EXPLOSION, VOL_SFX)),
        // Capsule capture: common/86 (`0x140261bc6`; Aura Particle captures the same).
        CombatEvent::Capsule { .. } => out.push(Cue::Sfx(capsule_pickup_id(), VOL_SFX)),
        // The orb appears on a respawn only; the opening fill is silent.
        CombatEvent::CapsuleAppeared { respawn, .. } => {
            if respawn {
                out.push(Cue::Sfx(capsule_appear_id(), VOL_SFX));
            }
        }
        CombatEvent::CapsuleErased { .. } => out.push(Cue::Sfx(SFX_CAPSULE_ERASED, VOL_SFX)),
        CombatEvent::Denied { .. } => out.push(Cue::Sfx(SFX_DENIED, VOL_SFX)),
        CombatEvent::CrystalSet { .. } => out.push(Cue::Sfx(SFX_ENV_SET, VOL_SFX)),
        CombatEvent::CrystalTaken { .. } => out.push(Cue::Sfx(SFX_CRYSTAL_TAKEN, VOL_SFX)),
        CombatEvent::EnvironmentEnded { .. } => out.push(Cue::Sfx(SFX_ENV_END, VOL_SFX)),
        CombatEvent::AuraTick { .. } => out.push(Cue::Sfx(SFX_AURA_TICK, VOL_SFX)),
        // The touch-down sound itself (82); the floor material's landing sound (70 on class 2 / 3 / 5)
        // and the surface-hit sound are `ground_sfx`'s, which reads the surface under the body.
        CombatEvent::KnockLanded { .. } => out.push(Cue::Sfx(SFX_KNOCK_LANDED, VOL_SFX)),
        CombatEvent::GotUp { .. } => out.push(Cue::Sfx(SFX_GET_UP, VOL_SFX)),
        CombatEvent::ButtonErased { .. } => out.push(Cue::Sfx(SFX_BUTTON_ERASED, VOL_SFX)),
        // The take-off sound is the floor material's: `ground_sfx` (61 on class 2 / 3 / 5).
        CombatEvent::JumpTakeoff { .. } => {}
        // The landing sound of an airborne landing is the floor material's: `ground_sfx`.
        CombatEvent::Landed { .. } => {}
        // Impact code 0: silent (it only keeps `object_sfx` from voicing the end as a wall).
        CombatEvent::ShotExpired { .. } => {}
        // `FUN_140261a00` with its sound flag: the same common/86 as a capsule pickup.
        CombatEvent::SkillCaptured { .. } => out.push(Cue::Sfx(capsule_pickup_id(), VOL_SFX)),
        CombatEvent::AddEffect { code, amount, .. } => out.extend(add_effect_sound(code, amount).map(|n| Cue::Sfx(n, VOL_SFX))),
        CombatEvent::SkillFired { skill, .. } => out.extend(sounds(skill).fire.map(|s| Cue::Sfx(s, VOL_SFX))),
        // The KO voice. NOTE: the program has no KO sound or voice at all (state 0x12 /
        // 0x1b have no sound call, read 2026-10-01); Laurence asked for the voice at the
        // fall in 3-4 player matches, so it stays, from the group the KO rows use.
        CombatEvent::Knockout { who } => out.push(Cue::Voice(who, VOICE_KO)),
        CombatEvent::StatusEffect { kind, amount, .. } => out.extend(status_effect_sound(kind, amount).map(|n| Cue::Sfx(n, VOL_SFX))),
        // An Aura Particle use plays only the Level sound (89, via StatusEffect). A footage audio
        // fit once suggested 90 as well, but the live trace on the PC original logged only 89 at
        // the use (`.pdtools/moverec/takes/000_Aura_Particle_held_stick_trace.csv`, 2026-10-01).
        CombatEvent::AuraParticleUsed { .. } => {}
        // Effects only (fx.rs).
        CombatEvent::ChargeHeld { .. } | CombatEvent::ChargeReleased { .. } | CombatEvent::RainDrop { .. } => {}
    }
    out
}

/// The `common` bank id the original plays on a footfall.
///
/// ⚠️ **Recovered 2026-09-29 from the machine code, not the decompilation**, and
/// the two disagree. `FUN_1401cbbc0`'s 4th argument is a **packed** value, not a
/// raw id: `id = arg >> 20 & 0xfff`. The footstep dispatch passes
/// `0x1e0c350`, which decodes to **30** (Ghidra renders the constant whole, so
/// the decompiled source reads like a nonsense address). Verified at byte level
/// at `0x140230ece`: `mov r9d, 0x1e0c350`. So `common/30.wav`, 0.78 s.
///
/// Playback choice: Laurence identified `audition/27_id035.wav` as the
/// regular step, and reiterated that correction on 2026-09-29. Its bytes
/// match `cache/sfx/common/35.wav`. The packed emitter id above is historical
/// program evidence, NOT the id to play for ordinary footsteps.
pub const SFX_FOOTSTEP: u32 = 35;

/// The `common` bank id the original plays when you lock on (lockon lane O4,
/// 2026-09-29).
///
/// Proven two independent ways. (1) Footage: cross-correlating Laurence's
/// `Lock-On Animation.mp4` against all 553 one-shot `common` clips finds
/// exactly one match -- id 4, score 0.84 at 0.35 s (everything else is noise
/// at 0.28 or less), right as the green reticle appears; the clip's only
/// event is the lock press. (2) Program: the HUD lock-marker updater
/// `FUN_1401536e0` plays id 4 through the general SFX call
/// (`FUN_140276330(0,4,4,0x1000,0x1000,0)`) ten frames after the marker's
/// target changes -- which is why the cue lands ~1/3 s after the press,
/// not on it. Called from `FUN_140150640` (the per-frame battle HUD) for
/// each player.
///
/// ⚠️ ON-transition only. The same function re-arms on any target change,
/// and whether an unlock (target -1) passes its validity gate could not be
/// settled from the decompilation alone -- the struct test at `+0x13f10`
/// reads both ways. Shipped as lock-ON only (the observed case); if Laurence
/// hears the original blip on unlock too, play it there as well.
pub const SFX_LOCKON: u32 = 4;

/// Next render-side lock-toggle state + cue for one fighter.
///
/// `seen` is what the renderer has already counted, `counter` the sim's
/// `Player::lock_on_events`, `locked` the sim's `locked`. Returns the new
/// seen count and the cue to play (if any): one blip per ON-toggle, silence
/// on held ticks, on unlock, and across a match restart (whose counters
/// start over at 0 -- that reads as `counter < seen`, not as a toggle).
pub fn lock_on_cue(seen: u32, counter: u32, locked: bool) -> (u32, Option<Cue>) {
    if counter <= seen {
        return (counter, None);
    }
    (counter, locked.then_some(Cue::Sfx(SFX_LOCKON, VOL_SFX)))
}

/// How long after an ON-toggle the lock-on blip plays (seconds). The
/// program doesn't play it on the press: its HUD lock-marker updater counts
/// down 10 frames after the marker's target changes and plays id 4 when the
/// count hits 0 (`FUN_1401536e0`, called per frame from `FUN_140150640`) --
/// which is why the cue in Laurence's Lock-On clip lands ~1/3 s after the
/// press, on top of the already-visible reticle, instead of with it.
/// ⚠️ The frame basis (render frames vs sim ticks) is unconfirmed: 10/60 s
/// here; use `PD_LOCKON_DELAY` to retune (seconds).
pub const LOCKON_CUE_DELAY_S: f32 = 10.0 / 60.0;

/// The `common` bank id the original plays when a capsule orb appears
/// (materialises) on the field: **85, CONFIRMED by Laurence's ear**
/// (FORMATS-AUDIO.md "Audition pass 2"). Overridable with
/// `PD_CAPSULE_APPEAR=<id>`; the render side plays one cue per newly appeared
/// orb (`capsule_appeared`, drained through the one playback path, so several
/// orbs on one frame still play once).
pub const SFX_CAPSULE_APPEAR: u32 = 85;

/// The `common` bank id played when a capsule is captured into a button:
/// **86**, read from the program: the capture call at `0x140251b8d` passes
/// `r9d = 1` to `FUN_140261a00`, which then plays `FUN_140276330(0, 0x56, ..)`
/// at `0x140261bc6` (audits/P0-SOUND.md). Aura Particle captures take the same
/// path, so they sound the same (Laurence, 2026-09-30). Not yet ear-checked.
/// Overridable with `PD_CAPSULE_PICKUP=<id>`.
///
/// ⚠️ **34 (not a sound, see `SFX_CAPSULE_PICKUP_RETIRED`) and 92 (the
/// erase sound, `0x14014ff07`; Laurence ruled it out for pickup) must not
/// come back.**
pub const SFX_CAPSULE_PICKUP: u32 = 86;

/// The `common` bank id the program plays when a pressed skill can't be used
/// (can't pay, or a button-disabling status): **2**, at `0x140261fa0`
/// (`FUN_140261e90`) and `0x140251b32` (P0-SOUND.md D4). Replaces our own
/// synthesized buzz, which stood in while the id was unidentified.
pub const SFX_DENIED: u32 = 2;

/// The id previously wired to the capsule pickup, kept as a guard so nobody
/// restores it. ⚠️ **Not a sound.** `FUN_14024d4d0` does not play audio: it
/// returns a signed *intensity*, which the caller uses as `intensity + 0x1a4`
/// to index a voice slot. I read a query as a play and wired an intensity
/// value as a sound id, which is why the pickup played the wrong clip. Proof
/// it is not a play call: **none** of the 12 functions on the capsule-take
/// path call the sound emitter `FUN_1401cbbc0` at all, so the real pickup
/// audio goes through an entry point that is still unidentified.
#[allow(dead_code)] // guard const: used by tests, kept so nobody restores it
pub const SFX_CAPSULE_PICKUP_RETIRED: u32 = 34;

/// Env override for a capsule cue id (`PD_CAPSULE_APPEAR` / `PD_CAPSULE_PICKUP`):
/// a parseable `common` id wins, anything else (unset, empty, garbage) keeps
/// the default. Pure for testability — `*_id()` below reads the environment.
fn capsule_id_from_env(text: Option<&str>, fallback: u32) -> u32 {
    text.and_then(|v| v.parse::<u32>().ok()).unwrap_or(fallback)
}

/// `SFX_CAPSULE_APPEAR`, or `PD_CAPSULE_APPEAR=<id>` when set.
pub fn capsule_appear_id() -> u32 {
    capsule_id_from_env(std::env::var("PD_CAPSULE_APPEAR").ok().as_deref(), SFX_CAPSULE_APPEAR)
}

/// `SFX_CAPSULE_PICKUP`, or `PD_CAPSULE_PICKUP=<id>` when set.
pub fn capsule_pickup_id() -> u32 {
    capsule_id_from_env(std::env::var("PD_CAPSULE_PICKUP").ok().as_deref(), SFX_CAPSULE_PICKUP)
}

/// Every `common` bank id the original's sound emitter (`FUN_1401cbbc0`) plays,
/// recovered 2026-09-29 by scanning all **134** of its call sites in `.text`
/// and decoding each one's packed argument (`arg >> 20 & 0xfff`).
///
/// ⚠️ **This is the audition list.** The program's own paths are the only
/// *reliable* naming source and they do not cover the capsule sounds: none of
/// the 12 functions on the capsule-take path call the emitter. So the pickup /
/// aura-activate / green-capsule ids are identified **by ear**, from this list.
/// Each entry records how many call sites use it, which is a weak prior on how
/// common the sound is.
/// kept for the next audition: de-duplicate adjacent ids first (28/29, 30/31,
/// 32/33 and others share one WAVE clip — FORMATS-AUDIO.md "Audition pass 2").
#[allow(dead_code)]
pub const SOUND_EMITTER_IDS: &[(u32, &str, u32)] = &[
    (1, "6 sites", 6),
    (2, "1 site", 1),
    (3, "6 sites", 6),
    (6, "3 sites", 3),
    (8, "2 sites", 2),
    (9, "24 sites - the most used, likely a common impact/UI", 24),
    (11, "5 sites", 5),
    (13, "1 site", 13),
    (14, "7 sites", 7),
    (16, "2 sites", 16),
    (17, "4 sites", 17),
    (20, "6 sites", 20),
    (21, "3 sites", 21),
    (26, "17 sites", 26),
    (27, "8 sites", 27),
    (28, "5 sites", 28),
    (29, "3 sites", 29),
    (30, "23 sites - historical packed emitter id (regular playback is 35)", 30),
    (31, "2 sites", 31),
    (32, "1 site", 32),
    (33, "4 sites", 33),
];

/// `PD_LOCKON_DELAY` retune (seconds) for `LOCKON_CUE_DELAY_S`, read once
/// per process like the sim's own rate override.
fn lockon_delay() -> f32 {
    static DELAY: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *DELAY.get_or_init(|| {
        std::env::var("PD_LOCKON_DELAY").ok().and_then(|v| v.parse::<f32>().ok()).filter(|&d| d >= 0.0).unwrap_or(LOCKON_CUE_DELAY_S)
    })
}

/// Render-side lock-on cue state for `play_sfx` (one struct so the system
/// stays within Bevy's parameter limit): per-fighter toggle counts already
/// counted, plus ON-toggle cues waiting out `LOCKON_CUE_DELAY_S`.
#[derive(Default)]
struct LockSfx {
    seen: [u32; MAX_PLAYERS],
    pending: Vec<(f32, Cue)>,
    /// Each fighter's `cast_seq` already voiced/sounded (kept here, not as a
    /// separate `Local`, to stay within Bevy's parameter limit).
    cast: [u16; MAX_PLAYERS],
}

/// Sounds other render-side systems asked for this frame.
///
/// `sfx.rs`'s combat system owns playback (it has the `SfxLib` and the
/// per-frame budget), so anything that is not a `CombatEvent` — footsteps, for
/// one — pushes here instead of duplicating the playback path. Cleared by the
/// playback system every frame.
#[derive(Resource, Default)]
pub struct CueQueue(pub Vec<Cue>);

#[derive(Resource, Default)]
pub struct SfxLib {
    /// Sound id -> number of loop layers, for the `common` bank.
    common: HashMap<u32, usize>,
    /// Each player's voice bank folder and the ids it has.
    voices: [Option<(String, Vec<u32>)>; MAX_PLAYERS],
    skills: HashMap<i16, SkillSounds>,
    handles: HashMap<String, Handle<AudioSource>>,
    /// `cache/sfx/index.json`, kept to pick voice banks when the characters change.
    index: serde_json::Value,
    /// The characters `voices` were picked for.
    voice_chars: Vec<String>,
    /// `PD_SFX_LOG=1`: log every sound started (checking without ears).
    log: bool,
}

impl SfxLib {
    /// Whether the `common` bank has sound `id` (the ground / object sound systems skip ids it lacks).
    pub(crate) fn has_common(&self, id: u32) -> bool {
        self.common.contains_key(&id)
    }

    fn handle(&mut self, assets: &AssetServer, path: String) -> Handle<AudioSource> {
        self.handles.entry(path.clone()).or_insert_with(|| assets.load(path)).clone()
    }
}

/// The characters whose voices speak: the match's own player list (every
/// seat, `seats::MatchPlayers`) when there is one, else the two of the old
/// setup (`MatchSetup`, the startup defaults).
fn voice_characters(cfg: &Config, setup: Option<&crate::frontend::state::MatchSetup>, players: Option<&crate::seats::MatchPlayers>) -> Vec<String> {
    match (players.filter(|p| p.len() > 0), setup) {
        (Some(p), _) => p.chars(),
        (None, Some(s)) if !cfg.legacy => s.chars.to_vec(),
        _ => cfg.chars.to_vec(),
    }
}

fn load_sfx(mut commands: Commands, cfg: Res<Config>, world: Res<World>, assets: Res<AssetServer>, players: Option<Res<crate::seats::MatchPlayers>>) {
    let path = cfg.cache.join("sfx/index.json");
    let Some(index) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else {
        info!("sfx: no {} (run pd_convert sounds); playing without sound effects", path.display());
        return;
    };
    let ids = |bank: &serde_json::Value| -> Vec<(u32, usize)> {
        bank.as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.parse().ok()?, v["loops"].as_u64().unwrap_or(0) as usize))).collect()
    };
    let common: HashMap<u32, usize> = ids(&index["banks"]["common"]).into_iter().collect();
    let voice_chars = voice_characters(&cfg, None, players.as_deref());
    let voices = resolve_voices(&index, &voice_chars);
    let voice_groups = load_voice_groups(&cfg.cache.join("skills.json"));
    let with_effect = load_effect_skills(&cfg.cache.join("skills.json"));
    let skills = world
        .0
        .combat
        .skills
        .iter()
        .map(|d| {
            let voice = voice_groups.get(&d.id).copied().unwrap_or(0);
            let mut s = skill_sounds(&d.raw, world.0.combat.action_row(d.id, false).map_or(0, |r| r.cast_sound), voice, |id| common.contains_key(&id));
            s.has_effect = with_effect.contains(&d.id);
            (d.id, s)
        })
        .collect();
    let mut lib = SfxLib { common, voices, skills, handles: HashMap::new(), index, voice_chars, log: std::env::var_os("PD_SFX_LOG").is_some() };
    // Preload what the two arsenals can play, so the first cast isn't silent.
    let arsenal: HashSet<i16> = world.0.combat.arsenals.iter().flatten().copied().collect();
    for id in arsenal {
        let s = lib.skills.get(&id).cloned().unwrap_or_default();
        for n in s.cast.iter().chain(&s.impact).chain(&s.travel) {
            lib.handle(&assets, format!("sfx/common/{n}.wav"));
        }
    }
    // Footsteps play outside any skill's cast too, so they are never in that
    // set - preload or the first step of a round is silent.
    lib.handle(&assets, format!("sfx/common/{SFX_FOOTSTEP}.wav"));
    // Same for the lock-on blip: it fires on a toggle, never on a cast.
    lib.handle(&assets, format!("sfx/common/{SFX_LOCKON}.wav"));
    // Same for the capsule cues and the wet step: they fire on field/sim
    // state, never on a cast. The env overrides are read here so a retuned id
    // is preloaded too (a non-preloaded id still plays, one frame late, via
    // the per-cue `handle` in `play_sfx`).
    lib.handle(&assets, format!("sfx/common/{}.wav", capsule_appear_id()));
    lib.handle(&assets, format!("sfx/common/{}.wav", capsule_pickup_id()));
    // The events lane's layers fire on sim events, never on a cast: preload them too.
    for id in [SFX_AURA_TICK, SFX_HIT_CHARACTER, SFX_HIT_OBJECT, SFX_GUARD_BROKEN, SFX_GET_UP, SFX_KNOCK_LANDED, SFX_KNOCK_GROUND, SFX_ENV_SET, SFX_ENV_END, SFX_CRYSTAL_TAKEN, SFX_BUTTON_ERASED, SFX_CAPSULE_ERASED, SFX_EXPLOSION, SFX_DENIED] {
        lib.handle(&assets, format!("sfx/common/{id}.wav"));
    }
    for k in 0..lib.common.get(&SFX_ENV_HUM).copied().unwrap_or(0) {
        lib.handle(&assets, format!("sfx/common/{SFX_ENV_HUM}_loop{k}.wav"));
    }
    for id in crate::surface_ground::WET_IDS {
        lib.handle(&assets, format!("sfx/common/{id}.wav"));
    }
    info!("sfx: {} common sounds, voices {:?}", lib.common.len(), lib.voices.iter().take(lib.voice_chars.len()).map(|v| v.as_ref().map(|(f, _)| f.as_str())).collect::<Vec<_>>());
    commands.insert_resource(lib);
}

/// Each skill's ground (else mid-air) action row's voice group (w4), from
/// skills.json.
fn load_voice_groups(path: &std::path::Path) -> HashMap<i16, i16> {
    let Some(v) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return HashMap::new() };
    let group = |s: &serde_json::Value| ["ground", "air"].iter().find_map(|k| s["action"][*k]["voice_group"].as_i64().filter(|&g| g > 0));
    crate::first_skill_records(&v).filter_map(|s| Some((s["id"].as_i64()? as i16, group(s)? as i16))).collect()
}

/// The skills whose action row names an effect (w2 > 0, ground or mid-air).
fn load_effect_skills(path: &std::path::Path) -> HashSet<i16> {
    let Some(v) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return HashSet::new() };
    let has = |s: &serde_json::Value| ["ground", "air"].iter().any(|k| s["action"][*k]["effect"].as_i64().is_some_and(|e| e > 0));
    crate::first_skill_records(&v).filter(|s| has(s)).filter_map(|s| Some(s["id"].as_i64()? as i16)).collect()
}

/// Each player's battle-voice bank for `chars` (index.json "voices",
/// from the game's character table), with the clip ids it has.
fn resolve_voices(index: &serde_json::Value, chars: &[String]) -> [Option<(String, Vec<u32>)>; MAX_PLAYERS] {
    std::array::from_fn(|i| {
        // A seat with no character (a duel's slots 3 and 4) has no voice.
        let folder = index["voices"][chars.get(i)?.as_str()].as_str()?.to_string();
        // Silent placeholder banks (some enemy characters) are skipped.
        let list: Vec<u32> = index["banks"][&folder].as_object()?.iter().filter(|(_, v)| v["rms"].as_f64().unwrap_or(0.0) > 1e-3).filter_map(|(k, _)| k.parse().ok()).collect();
        (!list.is_empty()).then_some((folder, list))
    })
}

/// Render-side pseudo-random numbers for the voice pick (never the sim's stream:
/// which clip of a list speaks changes nothing in the match).
fn xorshift32(mut x: u32) -> u32 {
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

/// A cue that does not come from a player (footsteps, lock-on blips).
const NO_SOURCE: usize = usize::MAX;

/// The player a combat event's sounds belong to (its attacker for the impact
/// of a hit, a block or a burst).
fn event_source(ev: &CombatEvent) -> usize {
    match *ev {
        CombatEvent::Hit { attacker, .. } | CombatEvent::Blocked { attacker, .. } | CombatEvent::Burst { attacker, .. } => attacker,
        CombatEvent::Capsule { who, .. }
        | CombatEvent::Denied { who, .. }
        | CombatEvent::CapsuleAppeared { who, .. }
        | CombatEvent::CapsuleErased { who, .. }
        | CombatEvent::CrystalSet { who, .. }
        | CombatEvent::CrystalTaken { who, .. }
        | CombatEvent::AuraTick { who }
        | CombatEvent::KnockLanded { who }
        | CombatEvent::GotUp { who }
        | CombatEvent::SkillFired { who, .. }
        | CombatEvent::StatusEffect { who, .. }
        | CombatEvent::AuraParticleUsed { who }
        | CombatEvent::ChargeHeld { who, .. }
        | CombatEvent::ChargeReleased { who, .. }
        | CombatEvent::JumpTakeoff { who }
        | CombatEvent::SkillCaptured { who, .. }
        | CombatEvent::AddEffect { who, .. }
        | CombatEvent::Landed { who, .. }
        | CombatEvent::Knockout { who } => who,
        CombatEvent::BlastStarted { attacker, .. } | CombatEvent::ButtonErased { attacker, .. } | CombatEvent::RainDrop { attacker, .. } => attacker,
        CombatEvent::EnvironmentEnded { .. } | CombatEvent::ShotExpired { .. } => NO_SOURCE,
    }
}

/// Marks a one-shot sound (counted against `MAX_PLAYING`).
#[derive(Component)]
struct OneShot;

#[allow(clippy::too_many_arguments)]
fn play_sfx(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut lib: ResMut<SfxLib>,
    world: Res<World>,
    playing: Query<(), With<OneShot>>,
    mut seen_event: Local<u32>,
    mut lock_sfx: Local<LockSfx>,
    mut active_blasts: Local<Vec<(i16, bool, u64)>>,
    mut voice_rng: Local<u32>,
    mut travel: Local<HashMap<(usize, u32), Vec<Entity>>>,
    queue: Option<ResMut<CueQueue>>,
    state: Res<State<crate::frontend::state::AppState>>,
    cfg: Res<Config>,
    setup: Option<Res<crate::frontend::state::MatchSetup>>,
    players: Option<Res<crate::seats::MatchPlayers>>,
    time: Res<Time>,
) {
    let c = &world.0.combat;
    let in_match = *state.get() == crate::frontend::state::AppState::Match;
    // The characters picked for the match (every seat) speak, not the startup defaults.
    let chars = voice_characters(&cfg, setup.as_deref(), players.as_deref());
    if chars != lib.voice_chars {
        lib.voices = resolve_voices(&lib.index, &chars);
        lib.voice_chars = chars;
    }
    // A new match starts its event numbers again from 1.
    if c.events.last().map_or(0, |e| e.0) < *seen_event {
        *seen_event = 0;
        lock_sfx.pending.clear();
    }
    // Every cue with the player it comes from (`NO_SOURCE`: a system with no
    // player, e.g. footsteps), for the per-frame de-dup below.
    let mut cues: Vec<(Cue, usize)> = Vec::new();
    for i in c.roster.slots() {
        let f = &c.fighters[i];
        // Back to 0 = a new round or match reset the fighter, not a cast.
        if f.cast_seq != lock_sfx.cast[i] {
            lock_sfx.cast[i] = f.cast_seq;
            if f.cast_seq == 0 {
                continue;
            }
            // An automatic Shelter that put itself up (`Combat::auto_guard`) plays neither the row's
            // cast sound nor the voice: only `FUN_14025a3e0` (the press path) plays them. LIVE trace.
            if f.auto_cast_seq == f.cast_seq {
                continue;
            }
            if let Some(s) = lib.skills.get(&f.cast_skill) {
                cues.extend(s.cast.iter().map(|&n| (Cue::Sfx(n, VOL_SFX), i)));
                cues.extend(s.voice.map(|g| (Cue::Voice(i, g), i)));
            }
        }
    }
    let from_seq = *seen_event;
    let fresh: Vec<CombatEvent> = c.events.iter().filter(|(seq, _)| *seq > from_seq).map(|e| e.1).collect();
    // Blasts still alive from earlier frames (see `Batch::with_active`); a new match restarts the clock.
    let now_tick = world.0.tick;
    active_blasts.retain(|&(_, _, until)| until > now_tick && until <= now_tick + BLAST_LIFE_TICKS);
    let active: Vec<(i16, bool)> = active_blasts.iter().map(|&(skill, area, _)| (skill, area)).collect();
    for ev in &fresh {
        if let CombatEvent::BlastStarted { skill, area, .. } = *ev {
            active_blasts.push((skill, area, now_tick + BLAST_LIFE_TICKS));
        }
    }
    let batch = Batch::of(&fresh).with_active(&active);
    for &(seq, ev) in c.events.iter().filter(|(seq, _)| *seq > from_seq) {
        *seen_event = seq;
        if lib.log {
            info!("sfx: tick {} event #{seq} {ev:?}", world.0.tick);
        }
        let reaction = |p: usize| (world.0.combat.reaction_voice_group(p), world.0.combat.reaction_sound(p));
        let from = event_source(&ev);
        cues.extend(cues_in_batch(&ev, |id| lib.skills.get(&id).cloned().unwrap_or_default(), reaction, &batch).into_iter().map(|cue| (cue, if let Cue::Voice(p, _) = cue { p } else { from })));
    }
    // Lock-on blips: one per ON-toggle per fighter (O4). The sim counts
    // every toggle in `Player::lock_on_events`; `lock_on_cue` turns the
    // count difference into at most one cue and stays silent on unlocks,
    // held ticks and match restarts. The cue itself waits out the
    // original's 10-frame HUD countdown (`LOCKON_CUE_DELAY_S`) before
    // playing, like the program's marker updater does.
    for i in c.roster.slots() {
        let p = &world.0.players[i];
        let (next, cue) = lock_on_cue(lock_sfx.seen[i], p.lock_on_events, p.locked);
        lock_sfx.seen[i] = next;
        if let Some(c) = cue {
            lock_sfx.pending.push((time.elapsed_secs() + lockon_delay(), c));
        }
    }
    let now = time.elapsed_secs();
    lock_sfx.pending.retain(|&(at, c)| {
        if at <= now {
            cues.push((c, NO_SOURCE));
            false
        } else {
            true
        }
    });
    // Same sound twice in one frame (multi-shot skills) plays once. With
    // three or four players the same sound from two different players is two
    // sounds (each player's hit, cast or voice), so the player is part of the
    // key; a duel keeps its exact old rule.
    let crowd = c.roster.count() > 2;
    let mut started: HashSet<(String, usize)> = HashSet::new();
    let mut budget = MAX_PLAYING.saturating_sub(playing.iter().count());
    // Cues pushed by other render-side systems (footsteps). Drained here so
    // there is one playback path, one budget and one "already played this frame"
    // set. `footsteps` runs in PostUpdate and this in Update, so a step raised
    // after this system's pass waits for the next frame - at 60 Hz that is
    // under 17 ms, and footfalls are 19+ ticks apart, so it is not audible.
    if let Some(mut q) = queue {
        if !q.0.is_empty() {
            cues.extend(q.0.drain(..).map(|cue| (cue, NO_SOURCE)));
        }
    }
    for (cue, source) in cues {
        let (path, vol) = match cue {
            Cue::Sfx(n, v) => (format!("sfx/common/{n}.wav"), v),
            Cue::Voice(p, group) => {
                let Some((folder, ids)) = lib.voices.get(p).and_then(Option::as_ref) else { continue };
                // The program's pick: a random entry of the group's 10-entry list; a 0 entry (or
                // a clip this character lacks) is a silent cast / hit.
                *voice_rng = xorshift32(if *voice_rng == 0 { 0x2545_F491 } else { *voice_rng });
                let Some(id) = voice_pick(group, *voice_rng >> 8).filter(|id| ids.contains(id)) else { continue };
                (format!("sfx/{folder}/{id}.wav"), VOL_VOICE)
            }
        };
        if budget == 0 || !started.insert((path.clone(), if crowd { source } else { NO_SOURCE })) {
            continue;
        }
        budget -= 1;
        if lib.log {
            info!("sfx: play {path} at {vol}");
        }
        let h = lib.handle(&assets, path);
        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(vol)), crate::mixer::Mixed::effects(vol), OneShot));
    }

    // Travel hums: one set of loop layers per (owner, sound) while any of
    // that skill's projectiles are in the air.
    let mut flying: HashSet<(usize, u32)> = HashSet::new();
    // Outside a match (result screen, menus) nothing is flying: loops stop.
    for p in c.projectiles.iter().filter(|_| in_match && c.round_over.is_none()) {
        if let Some(t) = lib.skills.get(&p.skill).and_then(|s| s.travel) {
            flying.insert((p.owner, t));
        }
    }
    travel.retain(|key, ents| {
        let keep = flying.contains(key);
        if !keep {
            for e in ents.iter() {
                commands.entity(*e).despawn();
            }
        }
        keep
    });
    for key in flying {
        if travel.contains_key(&key) {
            continue;
        }
        let layers = lib.common.get(&key.1).copied().unwrap_or(0);
        if lib.log {
            info!("sfx: travel loop {} ({layers} layers) for player {}", key.1, key.0);
        }
        let ents = (0..layers)
            .map(|k| {
                let h = lib.handle(&assets, format!("sfx/common/{}_loop{k}.wav", key.1));
                commands.spawn((AudioPlayer::new(h), PlaybackSettings::LOOP.with_volume(Volume::Linear(VOL_TRAVEL)), crate::mixer::Mixed::effects(VOL_TRAVEL))).id()
            })
            .collect();
        travel.insert(key, ents);
    }
}

/// A shot hitting an arena object plays its skill's impact (table s21, the
/// "CollisionSFX": the sound of the shot meeting something) this loud; a
/// shot that breaks the object plays it at `VOL_SFX`.
const VOL_OBJECT_HIT: f32 = 0.35;
/// How close (units) a vanished shot must have been to the object.
const OBJECT_HIT_REACH: f32 = 40.0;

/// Which vanished shot hit an object: the one of last frame's shots
/// (id, position, skill) that no longer exists and was nearest the object's
/// box (within `OBJECT_HIT_REACH`).
pub fn shot_that_hit(prev: &[(u32, Vec3, i16)], alive: &HashSet<u32>, min: Vec3, max: Vec3) -> Option<i16> {
    prev.iter()
        .filter(|(id, _, _)| !alive.contains(id))
        .map(|&(_, p, skill)| ((p.clamp(min, max) - p).length(), skill))
        .filter(|&(d, _)| d <= OBJECT_HIT_REACH)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, s)| s)
}

/// Skills of the shots that ended this frame against terrain: gone from the field, not accounted
/// for by a `Hit` / `Blocked` / `Burst` event of the same skill (`explained`), and not within
/// `OBJECT_HIT_REACH` of an arena object that was hit or broke this frame (`object_boxes`, which
/// `object_sfx` already voices). READ: `FUN_14026cd70` cases 1, 2 and 3 all call `FUN_14026ed60`
/// with the skill's s21 (`0x26`), and only case 2 (a body hit) adds common/6 or 7 -- so a shot
/// that dies on a wall or the floor (case 1) plays its impact sound alone, as the original takes
/// show (`sound:0.509` with the impact effect and no hit). A shot whose life runs out is code 0
/// (`FUN_14026e720`; `FUN_14026cd70`'s default case: silent): the caller drops those first
/// (`CombatEvent::ShotExpired`).
pub fn terrain_impacts(prev: &[(u32, Vec3, i16)], alive: &HashSet<u32>, explained: &[i16], object_boxes: &[(Vec3, Vec3)]) -> Vec<i16> {
    terrain_ends(prev, alive, explained, object_boxes).into_iter().map(|(_, _, skill)| skill).collect()
}

/// The shots `terrain_impacts` counts, with their id and last position.
pub fn terrain_ends(prev: &[(u32, Vec3, i16)], alive: &HashSet<u32>, explained: &[i16], object_boxes: &[(Vec3, Vec3)]) -> Vec<(u32, Vec3, i16)> {
    prev.iter()
        .filter(|(id, _, skill)| !alive.contains(id) && !explained.contains(skill))
        .filter(|&&(_, p, _)| !object_boxes.iter().any(|&(min, max)| (p.clamp(min, max) - p).length() <= OBJECT_HIT_REACH))
        .copied()
        .collect()
}

/// Sounds of arena objects: a shot's impact when it hits or breaks one, and
/// the object type's own sound record (EF00 kind 11, e.g. st06 contaB's
/// crash, common 17, 52 frames after it breaks). No per-material hit sound
/// exists in the data (FORMATS-STAGES.md "Object effects").
#[allow(clippy::too_many_arguments)]
fn object_sfx(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut lib: ResMut<SfxLib>,
    world: Res<World>,
    cfg: Res<Config>,
    setup: Option<Res<crate::frontend::state::MatchSetup>>,
    time: Res<Time>,
    mut seen: Local<HashMap<u32, (crate::props::PropState, i32)>>,
    mut prev_shots: Local<Vec<(u32, Vec3, i16)>>,
    mut prev_free: Local<Vec<(u32, Vec3, i16)>>,
    mut prev_lead: Local<HashMap<u32, Vec3>>,
    mut seen_event: Local<u32>,
    mut type_sounds: Local<(String, HashMap<u32, Vec<(u32, f32)>>)>,
    mut pending: Local<Vec<(f32, u32)>>,
    mut ground: ResMut<crate::ground_sfx::StageSound>,
) {
    use crate::props::PropState;
    let c = &world.0.combat;
    // `PD_SHOT_PATH_LOG=1`: every shot's program position each sim tick (film checks of a path; reads the sim only).
    if std::env::var_os("PD_SHOT_PATH_LOG").is_some() {
        for p in &c.projectiles {
            let q = p.pos + p.lead();
            info!("shotpath tick {} id {} skill {} pos {:.2},{:.2},{:.2}", world.0.tick, p.id, p.skill, q.x, q.y, q.z);
        }
    }
    let stage = crate::ground_sfx::current_stage(&cfg, setup.as_deref());
    ground.ensure(&cfg.cache, &stage);
    if type_sounds.0 != stage {
        // Placement index -> the type's break sounds (id, delay s).
        let read = |p: std::path::PathBuf| -> serde_json::Value { std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default() };
        let fxj = read(cfg.cache.join(format!("fx/stage/{stage}.objfx.json")));
        let props = read(cfg.cache.join(format!("{stage}.props.json")));
        let mut map = HashMap::new();
        for o in props["objects"].as_array().into_iter().flatten() {
            let (Some(i), Some(ty)) = (o["index"].as_u64(), o["type"].as_u64()) else { continue };
            let list: Vec<(u32, f32)> = fxj["types"][ty as usize]["main"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["trigger"] == 2)
                .filter_map(|r| Some((r["sound"]["common_id"].as_u64()? as u32, r["sound"]["delay_frames"].as_f64().unwrap_or(0.0) as f32 / 60.0)))
                .collect();
            if !list.is_empty() {
                map.insert(i as u32, list);
            }
        }
        *type_sounds = (stage, map);
        seen.clear();
    }
    let alive: HashSet<u32> = c.projectiles.iter().map(|p| p.id).collect();
    let now = time.elapsed_secs();
    let mut cues: Vec<(u32, f32)> = Vec::new();
    // Skills whose shots a Hit / Blocked / Burst event of this frame accounts for.
    if c.events.last().map_or(0, |e| e.0) < *seen_event {
        *seen_event = 0;
    }
    let mut explained: Vec<i16> = Vec::new();
    // Shots whose life ran out this frame: silent (impact code 0, `CombatEvent::ShotExpired`).
    let mut expired: Vec<u32> = Vec::new();
    for &(seq, ev) in &c.events {
        if seq <= *seen_event {
            continue;
        }
        *seen_event = seq;
        if let CombatEvent::Hit { skill, .. } | CombatEvent::Blocked { skill, .. } | CombatEvent::Burst { skill, .. } = ev {
            explained.push(skill);
        }
        if let CombatEvent::ShotExpired { id, .. } = ev {
            expired.push(id);
        }
    }
    prev_free.retain(|(id, _, _)| !expired.contains(id));
    let mut object_boxes: Vec<(Vec3, Vec3)> = Vec::new();
    for prop in &c.props {
        // First sight (match start, `--props-state`): just remember it.
        let Some((was, wear)) = seen.insert(prop.index, (prop.state, prop.wear)) else { continue };
        let broke = matches!(prop.state, PropState::Broken | PropState::Gone) && was != prop.state && was != PropState::Broken;
        let hit = prop.wear > wear && !broke;
        if !(hit || broke) {
            continue;
        }
        object_boxes.push((prop.min, prop.max));
        let skill = shot_that_hit(&prev_shots, &alive, prop.min, prop.max);
        let vol = if broke { VOL_SFX } else { VOL_OBJECT_HIT };
        if let Some(s) = skill.and_then(|s| lib.skills.get(&s)).and_then(|s| s.impact) {
            cues.push((s, vol));
        }
        // A shot meeting something that is not a fighter: the impact (s21) and common/7
        // (`FUN_14026cd70` code 2, `0x14026d144`; a fighter's hit plays 6 instead).
        if skill.is_some() {
            cues.push((SFX_HIT_OBJECT, vol));
        }
        if broke {
            for &(id, delay) in type_sounds.1.get(&prop.index).into_iter().flatten() {
                pending.push((now + delay, id));
            }
            // The type's sound group (header-0xF8 table, slot 1): `FUN_1401fc750` queues it from the
            // handler that breaks the object, `FUN_140201d30` plays it (cooldown, spacing).
            ground.prop_broke(prop.index, (prop.min + prop.max) * 0.5);
        }
    }
    // A shot that ended on terrain plays its s21 impact (see `terrain_impacts`).
    if c.round_over.is_none() {
        // The surface-hit sound of a shot's stage contact (`ground_sfx::StageSound::shot_contact`); see
        // P0-SOUND.md section 12 for the evidence and what stays UNCONFIRMED.
        if lib.log {
            // Where each shot that ended without a hit was last seen (`pos`, and the half step it was
            // about to make, `lead`): the data the surface-hit analysis reads (`engine/re`).
            for &(id, pos, skill) in prev_free.iter().filter(|(id, _, skill)| !alive.contains(id) && !explained.contains(skill)) {
                let lead = prev_lead.get(&id).copied().unwrap_or(Vec3::ZERO);
                info!("sfx: shot ended skill {skill} id {id} pos {:.2},{:.2},{:.2} lead {:.2},{:.2},{:.2} tick {}", pos.x, pos.y, pos.z, lead.x, lead.y, lead.z, world.0.tick);
            }
        }
        for (id, pos, skill) in terrain_ends(&prev_free, &alive, &explained, &object_boxes) {
            if let Some(s) = lib.skills.get(&skill).and_then(|s| s.impact) {
                cues.push((s, VOL_SFX));
            }
            // The surface-hit sound of the face it ended on (see `ground_sfx`). `pos` trails the program
            // position by `lead`, and the step it died on is twice that.
            let lead = prev_lead.get(&id).copied().unwrap_or(Vec3::ZERO);
            // READ (P0-SOUND section 13): every step crossing a face is a sweep contact.
            if !crate::ground_sfx::switched_off("PD_SFX_NO_SHOT_SURFACE") {
                if let Some(sound) = ground.shot_contact(pos + lead, lead * 2.0, world.0.tick) {
                    cues.push((sound, VOL_SFX));
                }
            }
        }
    }
    for id in ground.flush_props(world.0.tick) {
        cues.push((id, VOL_SFX));
    }
    *prev_shots = c.projectiles.iter().map(|p| (p.id, p.pos, p.skill)).collect();
    *prev_free = c.projectiles.iter().filter(|p| p.prop.is_none()).map(|p| (p.id, p.pos, p.skill)).collect();
    *prev_lead = c.projectiles.iter().map(|p| (p.id, p.lead())).collect();
    pending.retain(|&(at, id)| {
        if at <= now {
            cues.push((id, VOL_SFX));
            false
        } else {
            true
        }
    });
    let mut started = HashSet::new();
    for (id, vol) in cues {
        if !lib.common.contains_key(&id) || !started.insert(id) {
            continue;
        }
        if lib.log {
            info!("sfx: object sound {id} at {vol}");
        }
        let h = lib.handle(&assets, format!("sfx/common/{id}.wav"));
        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(vol)), crate::mixer::Mixed::effects(vol), OneShot));
    }
}

/// The crystal id whose hum should be playing: the standing one, in a match whose round
/// is still on (the hum stops with the crystal -- taken, cancelled or timed out -- and
/// with the round).
pub fn hum_wanted(in_match: bool, round_over: bool, crystal: Option<u32>) -> Option<u32> {
    if in_match && !round_over { crystal } else { None }
}

/// The hum of a standing environment crystal: common/87, two loop layers
/// (`87_loop0` / `87_loop1`), started with the crystal (`FUN_14017b880`, the
/// two loop points `0x14017b966` it updates every tick) and stopped when it
/// ends. The set (93), taken (88) and end (94) one-shots are `CombatEvent`s.
/// A separate system like `object_sfx` (`play_sfx` is at Bevy's parameter limit).
#[allow(clippy::too_many_arguments)]
fn environment_hum(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut lib: ResMut<SfxLib>,
    world: Res<World>,
    state: Res<State<crate::frontend::state::AppState>>,
    mut hum: Local<Option<(u32, Vec<Entity>)>>,
) {
    let c = &world.0.combat;
    let want = hum_wanted(*state.get() == crate::frontend::state::AppState::Match, c.round_over.is_some(), c.crystal.as_ref().map(|cr| cr.id));
    if hum.as_ref().map(|h| h.0) == want {
        return;
    }
    if let Some((_, ents)) = hum.take() {
        for e in ents {
            commands.entity(e).despawn();
        }
    }
    let Some(id) = want else { return };
    let layers = lib.common.get(&SFX_ENV_HUM).copied().unwrap_or(0);
    if lib.log {
        info!("sfx: environment hum {SFX_ENV_HUM} ({layers} layers) start");
    }
    let ents = (0..layers)
        .map(|k| {
            let h = lib.handle(&assets, format!("sfx/common/{SFX_ENV_HUM}_loop{k}.wav"));
            commands.spawn((AudioPlayer::new(h), PlaybackSettings::LOOP.with_volume(Volume::Linear(VOL_TRAVEL)), crate::mixer::Mixed::effects(VOL_TRAVEL))).id()
        })
        .collect();
    *hum = Some((id, ents));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capturing_a_capsule_plays_the_pickup_cue() {
        // Read from the program (P0-SOUND.md): the capture call passes r9d = 1
        // to FUN_140261a00, which plays common/0x56. Laurence ruled out 92
        // (the erase sound) and 34 is not a sound at all.
        assert_eq!(SFX_CAPSULE_PICKUP, 86);
        let cues = cues_for_event(
            &CombatEvent::Capsule { who: 0, slot: 2, skill: 7 },
            |_| SkillSounds::default(),
            |_| (0, 0),
        );
        assert_eq!(cues, vec![Cue::Sfx(SFX_CAPSULE_PICKUP, VOL_SFX)]);
        // Both retired ids must never come back.
        assert_ne!(SFX_CAPSULE_PICKUP, SFX_CAPSULE_PICKUP_RETIRED); // 34
        assert_ne!(SFX_CAPSULE_PICKUP, 92);
    }

    #[test]
    fn capsule_ids_default_to_the_ear_result_and_accept_overrides() {
        // 85 = orb appearing (CONFIRMED by ear), 86 = pickup (read from the
        // program); 92 and 34 are both wrong for the pickup and retired.
        assert_eq!((SFX_CAPSULE_APPEAR, SFX_CAPSULE_PICKUP), (85, 86));
        assert_eq!(capsule_id_from_env(None, 85), 85);
        assert_eq!(capsule_id_from_env(Some("92"), 85), 92);
        // Unset-shaped values keep the default (this is what the game reads
        // when the env var is absent or garbage).
        for bad in [Some(""), Some("bogus"), Some("-1")] {
            assert_eq!(capsule_id_from_env(bad, 86), 86, "override {bad:?}");
        }
    }

    #[test]
    fn a_refused_press_plays_the_programs_common_2() {
        // FUN_140261e90 plays (0, 2) at 0x140261fa0 when a pressed skill
        // can't be used; our old synthesized buzz is gone.
        let cues = cues_for_event(&CombatEvent::Denied { who: 0, slot: 1 }, |_| SkillSounds::default(), |_| (0, 0));
        assert_eq!(cues, vec![Cue::Sfx(2, VOL_SFX)]);
        assert_eq!(SFX_DENIED, 2);
    }

    #[test]
    fn regular_footsteps_use_laurences_audition_id_35() {
        // Not a magic number: FUN_1401cbbc0's 4th argument is PACKED, and the
        // id is the top 12 bits. The footstep dispatch passes 0x1e0c350
        // (verified at byte level, 0x140230ece: `mov r9d, 0x1e0c350`), which
        // decodes to 30. If someone "simplifies" this to the raw constant, or
        // copies an id straight out of the decompiled source, the steps go
        // silent or play the wrong clip - which is exactly the bug that put a
        // wrong sound on the capsule pickup.
        assert_eq!(SFX_FOOTSTEP, 35);
        // Keep the historical decode separate from the user's playback choice.
        assert_eq!((0x1e0c350u32 >> 20) & 0xfff, 30);
    }

    #[test]
    fn a_shot_ending_on_terrain_plays_its_impact_unless_something_explains_it() {
        let prev = [(1, Vec3::new(0.0, 5.0, 0.0), 65), (2, Vec3::new(500.0, 5.0, 0.0), 11), (3, Vec3::new(900.0, 5.0, 0.0), 14), (4, Vec3::new(0.0, 5.0, 9.0), 7)];
        let none: HashSet<u32> = HashSet::new();
        let (bmin, bmax) = (Vec3::new(490.0, 0.0, -5.0), Vec3::new(510.0, 20.0, 5.0));
        // Everything vanished: the Burst (11) and the object hit (500, 5, 0) are accounted for elsewhere.
        assert_eq!(terrain_impacts(&prev, &none, &[11], &[(bmin, bmax)]), vec![65, 14, 7]);
        // A shot still flying is not an impact; a Hit of the same skill explains its shot.
        assert_eq!(terrain_impacts(&prev, &HashSet::from([1, 3, 4]), &[], &[]), vec![11]);
        assert_eq!(terrain_impacts(&prev, &none, &[65, 11, 14, 7], &[]), Vec::<i16>::new());
    }

    #[test]
    fn the_footstep_wav_is_in_the_cache_and_the_candidate_is_not_wired() {
        // The wav must exist, or steps are silent at runtime with no error.
        // `cargo test` runs with the CRATE directory as the cwd, not the
        // workspace root, so resolve the cache the way `fx.rs` does.
        let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/sfx/common");
        // A fresh clone has no converted cache (it is built from the player's own game files): nothing to check then.
        if !cache.exists() {
            return;
        }
        assert!(cache.join(format!("{SFX_FOOTSTEP}.wav")).exists(), "common/{SFX_FOOTSTEP}.wav (the footstep) must be present");
        // And the retired id must NOT be what a pickup plays. Kept as a
        // guard against someone restoring the old constant.
        assert_ne!(SFX_CAPSULE_PICKUP_RETIRED, SFX_FOOTSTEP);
        assert_ne!(SFX_CAPSULE_PICKUP, SFX_CAPSULE_PICKUP_RETIRED);
    }

    #[test]
    fn object_hit_sound_comes_from_the_vanished_shot_nearest_the_object() {
        let prev = [(1, Vec3::new(0.0, 5.0, 0.0), 65), (2, Vec3::new(12.0, 5.0, 0.0), 10), (3, Vec3::new(500.0, 0.0, 0.0), 26)];
        let (min, max) = (Vec3::new(10.0, 0.0, -5.0), Vec3::new(20.0, 10.0, 5.0));
        // Shot 2 was inside the box and is gone: it hit.
        assert_eq!(shot_that_hit(&prev, &HashSet::from([1, 3]), min, max), Some(10));
        // Shot 2 still flying: shot 1 (10 units away) is the one that vanished.
        assert_eq!(shot_that_hit(&prev, &HashSet::from([2, 3]), min, max), Some(65));
        // Only a far shot vanished: not this object's hit.
        assert_eq!(shot_that_hit(&prev, &HashSet::from([1, 2]), min, max), None);
    }

    fn raw(fields: &[(usize, i16)]) -> [i16; 70] {
        let mut r = [0i16; 70];
        for &(f, v) in fields {
            r[f] = v;
        }
        r
    }

    // Values from the skill table (FORMATS-SKILLS.md / FORMATS-AUDIO.md).
    // s13 (once misread as a 3xx cast sound: Laser 305, Bomb 336) is the
    // mid-air action row, not a sound -- the real cast sound is the action
    // table's own `cast_sound` (242 for Laser, confirmed by footage), passed
    // separately now.
    fn laser() -> [i16; 70] {
        raw(&[(20, 603), (21, 509)])
    }
    fn bomb() -> [i16; 70] {
        raw(&[(20, 601), (21, 521)])
    }
    fn lightning_bolt() -> [i16; 70] {
        raw(&[(8, 8), (19, 618), (21, 526)])
    }

    #[test]
    fn skill_fields_pick_cast_travel_and_impact() {
        let all = |_| true;
        assert_eq!(skill_sounds(&laser(), 242, 0, all), SkillSounds { cast: vec![242], fire: None, travel: Some(603), impact: Some(509), voice: None, no_reaction: false, has_effect: false, hit_body: Some(6) });
        assert_eq!(skill_sounds(&bomb(), 236, 0, all), SkillSounds { cast: vec![236], fire: None, travel: Some(601), impact: Some(521), voice: None, no_reaction: false, has_effect: false, hit_body: Some(6) });
        // s8 (8 here) is not a sound: the program never plays it. s19 (618, Lightning
        // Bolt) is the FIRE-tick sound, not part of the cast list.
        assert_eq!(skill_sounds(&lightning_bolt(), 0, 0, all), SkillSounds { cast: vec![], fire: Some(618), travel: None, impact: Some(526), voice: None, no_reaction: false, has_effect: false, hit_body: Some(6) });
        let diffused = raw(&[(18, 3012), (20, 10035), (21, 509)]);
        assert_eq!(skill_sounds(&diffused, 311, 0, all), SkillSounds { cast: vec![311], fire: None, travel: None, impact: Some(509), voice: None, no_reaction: false, has_effect: false, hit_body: Some(6) });
        // s19 is for attack-kind objects only (s43 = 0, motion != 4: `0x140262caa`).
        assert_eq!(skill_sounds(&raw(&[(19, 313), (43, 4)]), 0, 0, all).fire, None);
        assert_eq!(skill_sounds(&raw(&[(19, 313), (63, 4)]), 0, 0, all).fire, None);
        // Ids the bank lacks are skipped (the action row's cast sound too).
        assert_eq!(skill_sounds(&laser(), 242, 0, |id| id != 242).cast, Vec::<u32>::new());
    }

    /// The blades Laurence reported (2026-09-27): each plays only its action
    /// row's cast sound at the cast (Psycho Knife row 3 = 203, Mist Blade row
    /// 7 = 207, Excalibur row 4 = 204 -- not their s8 27 / 28 on top), the
    /// caster's voice group 1 / 3 (row w4), and its sword impact (s21) on a hit.
    #[test]
    fn blades_play_their_row_sound_and_voice_not_s8() {
        let all = |_| true;
        let psycho_knife = skill_sounds(&raw(&[(21, 501)]), 203, 1, all);
        assert_eq!(psycho_knife, SkillSounds { cast: vec![203], fire: None, travel: None, impact: Some(501), voice: Some(1), no_reaction: false, has_effect: false, hit_body: Some(6) });
        let mist_blade = skill_sounds(&raw(&[(8, 27), (21, 501)]), 207, 1, all);
        assert_eq!(mist_blade.cast, vec![207]);
        let excalibur = skill_sounds(&raw(&[(8, 28), (21, 501)]), 204, 3, all);
        assert_eq!((excalibur.cast, excalibur.impact, excalibur.voice), (vec![204], Some(501), Some(3)));
        assert_eq!(voice_of_group(0), None);
        assert_eq!(voice_of_group(7), None);
    }

    #[test]
    fn events_map_to_impacts_and_voices() {
        let sounds = |id: i16| match id {
            65 => skill_sounds(&laser(), 242, 0, |_| true),
            10 => skill_sounds(&bomb(), 236, 0, |_| true),
            _ => SkillSounds::default(),
        };
        // No row loaded for this hit (group 0): falls back to `VOICE_HURT`.
        let no_row = |_| (0, 0);
        // A hit = the skill's s21 + common/6 (a playable victim, `FUN_14026cd70`) + the victim's voice.
        let hit = |victim, skill| CombatEvent::Hit { attacker: 0, victim, skill };
        assert_eq!(cues_for_event(&hit(1, 65), sounds, no_row), vec![Cue::Sfx(509, VOL_SFX), Cue::Sfx(6, VOL_SFX), Cue::Voice(1, VOICE_HURT)]);
        // Knocked down: the heavy reaction rows' voice group (5).
        assert_eq!(cues_for_event(&hit(1, 65), sounds, |p| (if p == 1 { 5 } else { 4 }, 0)), vec![Cue::Sfx(509, VOL_SFX), Cue::Sfx(6, VOL_SFX), Cue::Voice(1, 5)]);
        // An ordinary hit: group 4.
        assert_eq!(cues_for_event(&hit(1, 65), sounds, |_| (4, 0)), vec![Cue::Sfx(509, VOL_SFX), Cue::Sfx(6, VOL_SFX), Cue::Voice(1, 4)]);
        // A block plays the impact at FULL level (the 0.3 was invented: the block
        // shares the hit's branch); a broken defence adds common/14 (`0x14026d527`).
        assert_eq!(cues_for_event(&CombatEvent::Blocked { attacker: 1, victim: 0, skill: 65, broken: false }, sounds, no_row), vec![Cue::Sfx(509, VOL_SFX)]);
        assert_eq!(cues_for_event(&CombatEvent::Blocked { attacker: 1, victim: 0, skill: 65, broken: true }, sounds, no_row), vec![Cue::Sfx(509, VOL_SFX), Cue::Sfx(14, VOL_SFX)]);
        assert_eq!(cues_for_event(&CombatEvent::Burst { attacker: 0, pos: Vec3::ZERO, skill: 10 }, sounds, no_row), vec![Cue::Sfx(521, VOL_SFX)]);
        // A skill with no impact sound and no travelling shot (a melee object) gets the hurt voice
        // only: common/6 / 7 belong to shots (LIVE, 341 takes; `hit_body_for`).
        assert_eq!(cues_for_event(&hit(0, 26), sounds, no_row), vec![Cue::Voice(0, VOICE_HURT)]);
        assert_eq!(hit_body_for(true, Some(0)), Some(6));
        assert_eq!(hit_body_for(true, Some(5)), Some(6));
        assert_eq!(hit_body_for(true, Some(6)), None);
        assert_eq!(hit_body_for(true, Some(2)), Some(7));
        assert_eq!(hit_body_for(true, Some(20)), Some(7));
        assert_eq!(hit_body_for(true, Some(13)), None);
        assert_eq!(hit_body_for(true, Some(12)), None);
        assert_eq!(hit_body_for(false, Some(0)), None);
    }

    /// Every new event -> its cue, with the program address the id comes from.
    #[test]
    fn the_events_lanes_events_map_to_the_programs_sounds() {
        let none = |_| SkillSounds::default();
        let g = |_| (0, 0);
        let one = |ev: CombatEvent| cues_for_event(&ev, none, g);
        let p = Vec3::ZERO;
        // 0x14014ed6f: appear 85 on a respawn only; the opening fill is silent.
        assert_eq!(one(CombatEvent::CapsuleAppeared { who: 0, pos: p, respawn: true }), vec![Cue::Sfx(85, VOL_SFX)]);
        assert!(one(CombatEvent::CapsuleAppeared { who: 0, pos: p, respawn: false }).is_empty());
        // 0x14014edc7 / 0x14014ff07: erase 92 (never the pickup).
        assert_eq!(one(CombatEvent::CapsuleErased { who: 1, pos: p, skill: 3 }), vec![Cue::Sfx(92, VOL_SFX)]);
        // 0x14017bdc9 set 93, 0x14017bf70 taken 88, 0x14017bb6d end 94 (the 87 hum is `environment_hum`).
        assert_eq!(one(CombatEvent::CrystalSet { who: 0, pos: p }), vec![Cue::Sfx(93, VOL_SFX)]);
        assert_eq!(one(CombatEvent::CrystalTaken { who: 0, pos: p }), vec![Cue::Sfx(88, VOL_SFX)]);
        assert_eq!(one(CombatEvent::EnvironmentEnded { pos: p }), vec![Cue::Sfx(94, VOL_SFX)]);
        // 0x14024df1d: Aura regen tick 5.
        assert_eq!(one(CombatEvent::AuraTick { who: 0 }), vec![Cue::Sfx(5, VOL_SFX)]);
        // 0x140254e3f landing 82, 0x14025c4ee get-up 81, 0x14026fb8e button erased 91, 0x14026dca4 explosion 99.
        assert_eq!(one(CombatEvent::KnockLanded { who: 0 }), vec![Cue::Sfx(82, VOL_SFX)], "70 is the floor material's (ground_sfx)");
        assert_eq!(one(CombatEvent::GotUp { who: 0 }), vec![Cue::Sfx(81, VOL_SFX)]);
        assert_eq!(one(CombatEvent::ButtonErased { attacker: 0, victim: 1, skill: 5 }), vec![Cue::Sfx(91, VOL_SFX)]);
        assert_eq!(one(CombatEvent::BlastStarted { attacker: 0, pos: p, skill: 10, radius: 30.0, area: false }), vec![Cue::Sfx(99, VOL_SFX)]);
        // The KO voice: group 6, at the fighter that fell.
        assert_eq!(one(CombatEvent::Knockout { who: 2 }), vec![Cue::Voice(2, 6)]);
        // 0x140262caa: s19 at the fire tick (not at the cast).
        let rain = |id: i16| if id == 9 { skill_sounds(&raw(&[(19, 313)]), 305, 0, |_| true) } else { SkillSounds::default() };
        assert_eq!(cues_for_event(&CombatEvent::SkillFired { who: 0, skill: 9 }, rain, g), vec![Cue::Sfx(313, VOL_SFX)]);
        assert_eq!(rain(9).cast, vec![305], "the cast list holds the row's sound only");
    }

    /// `FUN_14026cd70` code 2 vs the blast: a blast's victims are hurt by the blast, not by
    /// the shot's own hit (no s21, no common/6, only the reaction's voice), and a mine's
    /// or wave's blast has common/99 only, not the skill's s21 (code 8).
    #[test]
    fn a_blast_hurts_its_victims_without_the_shots_hit_sounds() {
        let sounds = |id: i16| if id == 10 { skill_sounds(&bomb(), 236, 0, |_| true) } else { SkillSounds::default() };
        let rx = |_| (4, 0);
        let at = Vec3::new(1.0, 2.0, 3.0);
        let shot = Batch::of(&[CombatEvent::BlastStarted { attacker: 0, pos: at, skill: 10, radius: 30.0, area: false }]);
        let hit = CombatEvent::Hit { attacker: 0, victim: 1, skill: 10 };
        assert_eq!(cues_in_batch(&hit, sounds, rx, &shot), vec![Cue::Voice(1, 4)]);
        // The shot's own explosion burst still plays its impact (s21), then 99.
        assert_eq!(cues_in_batch(&CombatEvent::Burst { attacker: 0, pos: at, skill: 10 }, sounds, rx, &shot), vec![Cue::Sfx(521, VOL_SFX)]);
        // A mine: 99 only.
        let mine = Batch::of(&[CombatEvent::BlastStarted { attacker: 0, pos: at, skill: 10, radius: 50.0, area: true }]);
        assert!(cues_in_batch(&CombatEvent::Burst { attacker: 0, pos: at, skill: 10 }, sounds, rx, &mine).is_empty());
        // Another skill's hit in the same frame is untouched.
        assert_eq!(cues_in_batch(&CombatEvent::Hit { attacker: 1, victim: 0, skill: 65 }, |_| SkillSounds { impact: Some(509), hit_body: Some(6), ..Default::default() }, rx, &shot), vec![Cue::Sfx(509, VOL_SFX), Cue::Sfx(6, VOL_SFX), Cue::Voice(0, 4)]);
    }

    /// A blast hurts everyone it reaches over its whole life (30 program ticks), not only on the
    /// frame it starts: a mine's victim, hit tens of ticks later, plays the reaction (voice, row
    /// sound) and neither the skill's s21 nor common/6 / 7 (LIVE: Timer Mine, Sensor Mine, Big Bomb).
    #[test]
    fn a_hit_from_a_blast_still_alive_is_a_blast_hit() {
        let sounds = |_| SkillSounds { impact: Some(521), hit_body: Some(7), ..Default::default() };
        let rx = |_| (4, 0);
        let hit = CombatEvent::Hit { attacker: 0, victim: 1, skill: 10 };
        let alive = Batch::default().with_active(&[(10, true)]);
        assert_eq!(cues_in_batch(&hit, sounds, rx, &alive), vec![Cue::Voice(1, 4)]);
        let shell = Batch::default().with_active(&[(10, false)]);
        assert_eq!(cues_in_batch(&hit, sounds, rx, &shell), vec![Cue::Voice(1, 4)]);
        // A skill with no blast on the field is untouched.
        let other = Batch::default().with_active(&[(11, true)]);
        assert_eq!(cues_in_batch(&hit, sounds, rx, &other), vec![Cue::Sfx(521, VOL_SFX), Cue::Sfx(7, VOL_SFX), Cue::Voice(1, 4)]);
    }

    /// A skill that is not an attack makes no reaction (no hurt voice); the row's own
    /// sound (100 on the bind rows) plays only with the attacker's effect (`FUN_140259310`).
    #[test]
    fn reaction_sounds_need_an_attack_and_an_effect() {
        let hit = CombatEvent::Hit { attacker: 0, victim: 1, skill: 5 };
        let bind_row = |_| (6, 100);
        let attack = |has_effect| move |_| SkillSounds { has_effect, hit_body: Some(6), ..Default::default() };
        assert_eq!(cues_for_event(&hit, attack(true), bind_row), vec![Cue::Sfx(6, VOL_SFX), Cue::Voice(1, 6), Cue::Sfx(100, VOL_SFX)]);
        assert_eq!(cues_for_event(&hit, attack(false), bind_row), vec![Cue::Sfx(6, VOL_SFX), Cue::Voice(1, 6)], "no effect, no row sound");
        // A status / erase skill landing (capsule kind 3 = erase here): the impact only.
        let erase = |_| skill_sounds(&raw(&[(9, 3), (21, 509)]), 0, 0, |_| true);
        assert_eq!(cues_for_event(&hit, erase, bind_row), vec![Cue::Sfx(509, VOL_SFX), Cue::Sfx(6, VOL_SFX)]);
    }

    /// `FUN_1402738d0` (end of `FUN_140272bc0`): what an Add effect plays on its victim. Every row is a
    /// LIVE take of the PC original (`takes_full`): s51 / s52 -> the common sound logged at the effect.
    #[test]
    fn add_effect_sounds_follow_the_programs_switch() {
        let live = [
            (2, 2, Some(90)),       // Interference
            (2, 1, Some(90)),       // Restrain
            (2, 99, Some(90)),      // Disintegrate
            (4, 2, Some(90)),       // Panic Gas, Quantum Decay, Bloody Ritual
            (4, -2, Some(89)),      // Level Berserk
            (0x2c, 2, Some(89)),    // Boost Mine, Quantum Amp
            (0x31, 2, Some(90)),    // Meltdown
            (0xb, 0, Some(90)),     // Song of Succubus, Succubus Tempt
            (0xd, 30, Some(95)),    // Alleviate
            (0xd, -20, Some(96)),   // Burden
            (0xd, -90, Some(96)),   // Gravity Mine
            (0xc, 1, Some(95)),     // Friendship
            (0xf, 20, Some(95)),    // Ethos Inspiration
            (0x11, 0, Some(98)),
            (0x33, 0, Some(97)),
            (0x32, 0, None),        // Annoying Gift: silent here (its sound is the captures')
            (1, 99, None),          // Fairy Ring
            (0x2a, 0, None),        // Trigger
        ];
        for (code, amount, want) in live {
            assert_eq!(add_effect_sound(code, amount), want, "s51 {code} s52 {amount}");
        }
    }

    /// The three events of this lane and what they play: a skill jump's take-off (`0x1402549e0`, 61), a
    /// skill capture (`0x140261bc6`, 86) and an Add effect (`add_effect_sound`).
    #[test]
    fn takeoff_capture_and_add_events_have_their_sounds() {
        let one = |ev| cues_for_event(&ev, |_| SkillSounds::default(), |_| (0, 0));
        assert!(one(CombatEvent::JumpTakeoff { who: 0 }).is_empty(), "61 is the floor material's (ground_sfx)");
        assert_eq!(one(CombatEvent::SkillCaptured { who: 0, slot: 1, skill: 5 }), vec![Cue::Sfx(86, VOL_SFX)]);
        assert_eq!(one(CombatEvent::AddEffect { who: 1, code: 4, amount: -2 }), vec![Cue::Sfx(89, VOL_SFX)]);
        assert!(one(CombatEvent::AddEffect { who: 1, code: 0x32, amount: 0 }).is_empty());
    }

    /// `FUN_140271590` (`0x14027161f`): what a status effect plays on the affected fighter.
    #[test]
    fn status_effects_play_the_programs_ids() {
        assert_eq!((status_effect_sound(1, 1), status_effect_sound(1, 0), status_effect_sound(1, -2)), (Some(89), Some(90), Some(90)));
        for kind in 4..=7 {
            assert_eq!((status_effect_sound(kind, 3), status_effect_sound(kind, -3)), (Some(95), Some(96)));
        }
        assert_eq!((status_effect_sound(8, 1), status_effect_sound(10, 1), status_effect_sound(0x12, 1), status_effect_sound(0x17, 1)), (Some(98), Some(89), Some(97), Some(97)));
        assert_eq!((status_effect_sound(2, 1), status_effect_sound(19, 1), status_effect_sound(0, 1)), (None, None, None));
        let cues = cues_for_event(&CombatEvent::StatusEffect { who: 0, kind: 1, amount: 1 }, |_| SkillSounds::default(), |_| (0, 0));
        assert_eq!(cues, vec![Cue::Sfx(89, VOL_SFX)], "the Aura Particle's Level +1");
        let cues = cues_for_event(&CombatEvent::AuraParticleUsed { who: 0 }, |_| SkillSounds::default(), |_| (0, 0));
        assert!(cues.is_empty(), "live trace on the PC original: only 89 plays at an Aura Particle use, no 90");
    }

    /// The voice lists are the program's (gsdata table, file offset 0x33194): a roll picks
    /// one entry of ten; 0 is a silent pick; cross-group ids occur.
    #[test]
    fn voices_are_picked_from_the_programs_ten_entry_lists() {
        assert_eq!(VOICE_LISTS[0], [10, 11, 10, 12, 11, 0, 20, 10, 11, 12]);
        assert_eq!(VOICE_LISTS[5], [61, 60, 51, 62, 60, 61, 60, 62, 60, 61]);
        assert_eq!(voice_pick(1, 5), None, "entry 5 of group 1 is silent");
        assert_eq!(voice_pick(1, 6), Some(20), "a group-1 cast can speak a group-2 clip");
        assert_eq!(voice_pick(4, 9), None);
        assert_eq!(voice_pick(6, 13), Some(voice_pick(6, 3).unwrap()), "rolls wrap");
        assert_eq!(voice_pick(0, 0), None);
        assert_eq!(voice_pick(7, 0), None);
        // Uniform over the entries: ten rolls = each entry once.
        assert_eq!((0..10).filter_map(|r| voice_pick(2, r)).count(), 9);
    }

    #[test]
    fn the_aura_particle_activation_is_silent() {
        // Row 343 (the Aura Particle's ground row) has w3 = w4 = 0 and the record has no
        // s19..s21: nothing at cast, nothing at fire (P0-SOUND.md section 2).
        let s = skill_sounds(&raw(&[]), 0, 0, |_| true);
        // (An all-zero record reads as a shell, so it carries a body-hit id; it never hits anything.)
        assert_eq!(s, SkillSounds { hit_body: Some(6), ..Default::default() });
    }

    /// O4, lockon lane: the lock-on blip fires once per ON-toggle -- not per
    /// tick, not on unlock, not on a match restart.
    #[test]
    fn lock_blip_fires_once_per_toggle_on() {
        assert_eq!(SFX_LOCKON, 4, "the footage-identified id (Lock-On Animation.mp4, 0.84 at 0.35 s; FUN_1401536e0)");
        let blip = Some(Cue::Sfx(SFX_LOCKON, VOL_SFX));
        // Toggle on: one cue, seen advances.
        assert_eq!(lock_on_cue(0, 1, true), (1, blip));
        // Same tick again (or any held tick): silence.
        assert_eq!(lock_on_cue(1, 1, true), (1, None));
        // Toggle off: counts, but stays silent.
        assert_eq!(lock_on_cue(1, 2, false), (2, None));
        assert_eq!(lock_on_cue(2, 2, false), (2, None));
        // Toggle on again: fires again.
        assert_eq!(lock_on_cue(2, 3, true), (3, blip));
        // A new match restarts the sim counters at 0: resync, no phantom cue
        // even though the fresh fighter starts locked.
        assert_eq!(lock_on_cue(3, 0, true), (0, None));
    }
    #[test]
    fn every_seat_gets_its_own_voice_bank_and_empty_seats_none() {
        let index = serde_json::json!({
            "voices": { "pc00a": "VoA", "pc01a": "VoB", "pc02a": "VoC", "pc03a": "VoD" },
            "banks": { "VoA": { "10": { "rms": 0.1 } }, "VoB": { "20": { "rms": 0.1 } }, "VoC": { "30": { "rms": 0.1 } }, "VoD": { "40": { "rms": 0.1 } } },
        });
        let chars: Vec<String> = ["pc00a", "pc01a", "pc02a", "pc03a"].iter().map(|s| s.to_string()).collect();
        let v = resolve_voices(&index, &chars);
        assert_eq!(v.iter().map(|b| b.as_ref().map(|(f, _)| f.as_str())).collect::<Vec<_>>(), [Some("VoA"), Some("VoB"), Some("VoC"), Some("VoD")]);
        // A duel: seats 3 and 4 have no character, so no voice (and no panic).
        let v = resolve_voices(&index, &chars[..2]);
        assert!(v[0].is_some() && v[1].is_some() && v[2].is_none() && v[3].is_none());
    }

    /// Event -> cue trace for four players, off a real sim run to a knock-out:
    /// no event (attacker or victim in seats 3/4 included) can index outside
    /// the player arrays, and the sources and voices belong to the right seats.
    #[test]
    fn four_player_events_trace_to_cues_through_a_knock_out() {
        use crate::combat::{Combat, Roster, AURA_PARTICLE};
        use crate::sim::{Input, Match};
        use pd_formats::collision::Collision;
        let spawns = [Vec3::new(-100.0, 0.0, -100.0), Vec3::new(100.0, 0.0, -100.0), Vec3::new(100.0, 0.0, 100.0), Vec3::new(-100.0, 0.0, 100.0)];
        let mut m = Match::with_spawns(&spawns, Combat::with_roster(Vec::new(), Roster::free_for_all(4), std::array::from_fn(|_| vec![AURA_PARTICLE; 30])));
        let mut col = Collision::new(Vec::new());
        for _ in 0..3 {
            m.step([Input::default(); 4], &mut col);
        }
        let roster = m.combat.roster;
        for victim in [3, 2, 1] {
            m.combat.change_health(victim, -999);
            for _ in 0..4 {
                m.step([Input::default(); 4], &mut col);
            }
        }
        let outcome = m.combat.round_outcome();
        assert_eq!(outcome, Some(crate::combat::Outcome::Winner { slot: 0, team: roster.team[0] }));
        // Each fighter spoke its KO voice the moment it fell (a Knockout event per seat,
        // mid-round for seats 4 and 3), the winner never.
        let ko: Vec<Cue> = m.combat.events.iter().flat_map(|(_, e)| cues_for_event(e, |_| SkillSounds::default(), |_| (0, 0))).filter(|c| matches!(c, Cue::Voice(..))).collect();
        assert_eq!(ko, vec![Cue::Voice(3, VOICE_KO), Cue::Voice(2, VOICE_KO), Cue::Voice(1, VOICE_KO)]);
        // Hits, blocks and bursts between every pair of seats, as the sim logs them.
        for attacker in 0..MAX_PLAYERS {
            for victim in (0..MAX_PLAYERS).filter(|&v| v != attacker) {
                let hit = CombatEvent::Hit { attacker, victim, skill: 65 };
                assert_eq!(event_source(&hit), attacker);
                let cues = cues_for_event(&hit, |_| SkillSounds { impact: Some(509), hit_body: Some(6), ..Default::default() }, |p| (m.combat.reaction_voice_group(p), 0));
                assert_eq!(cues[0], Cue::Sfx(509, VOL_SFX));
                assert_eq!(cues[1], Cue::Sfx(6, VOL_SFX));
                assert!(matches!(cues[2], Cue::Voice(p, _) if p == victim), "the hurt voice is the victim's");
                let block = CombatEvent::Blocked { attacker, victim, skill: 65, broken: false };
                assert_eq!(event_source(&block), attacker);
                assert_eq!(cues_for_event(&block, |_| SkillSounds::default(), |_| (0, 0)), Vec::<Cue>::new());
            }
            assert_eq!(event_source(&CombatEvent::Burst { attacker, pos: Vec3::ZERO, skill: 1 }), attacker);
        }
    }

    // ---- A scripted match, event by event, to cue ---------------------------------------

    /// The program address a `common` id is played from (for the trace).
    fn address_of(id: u32) -> &'static str {
        match id {
            2 => "0x140261fa0 FUN_140261e90 (can't use)",
            5 => "0x14024df1d FUN_14024de20 (Aura +1)",
            6 => "0x14026d144 FUN_14026cd70 (character hit)",
            14 => "0x14026d527 FUN_14026cd70 (guard broken)",
            81 => "0x14025c4ee FUN_14025c3d0 (get up)",
            82 => "0x140254e3f FUN_140254c80 (knock-down lands)",
            85 => "0x14014ed6f FUN_14014ed10 (orb appears)",
            86 => "0x140261bc6 FUN_140261a00 (capsule captured)",
            88 => "0x14017bf70 FUN_14017be00 (crystal taken)",
            89 | 90 | 95 | 96 | 97 | 98 => "0x14027161f FUN_140271590 (status effect)",
            91 => "0x14026fb8e FUN_14026f870 (button erased)",
            92 => "0x14014edc7 FUN_14014ed90 (capsule erased)",
            93 => "0x14017bdc9 FUN_14017bcc0 (environment set)",
            94 => "0x14017bb6d FUN_14017ba00 (environment ended)",
            99 => "0x14026dca4 FUN_14026dbd0 (explosion)",
            313 => "0x140262caa FUN_1402626d0 (s19 at the fire tick)",
            _ => "skill table s21 via FUN_14026ed60",
        }
    }

    /// One scenario's rig: a sim stepped a tick at a time, every new event turned into
    /// cues with `cues_in_batch` and written to a trace line.
    struct Rig {
        c: crate::combat::Combat,
        p: [crate::sim::Player; 2],
        col: pd_formats::collision::Collision,
        name: &'static str,
        seen: u32,
        hum: Option<u32>,
        lines: Vec<String>,
        cues: Vec<Cue>,
    }

    fn trace_sounds(id: i16) -> SkillSounds {
        match id {
            65 => skill_sounds(&raw(&[(19, 313), (21, 509)]), 242, 1, |_| true),
            70 => skill_sounds(&raw(&[(21, 510)]), 253, 3, |_| true),
            10 => skill_sounds(&raw(&[(21, 521)]), 236, 1, |_| true),
            _ => SkillSounds::default(),
        }
    }

    impl Rig {
        fn new(name: &'static str, skills: Vec<crate::combat::SkillDef>, dist: f32) -> Self {
            let (c, p, col) = crate::combat::testkit::setup(skills, dist);
            Rig { c, p, col, name, seen: 0, hum: None, lines: Vec::new(), cues: Vec::new() }
        }

        fn tick(&mut self, buttons: [u8; 2]) {
            self.c.step(&mut self.p, buttons, &self.col);
            let fresh: Vec<(u32, CombatEvent)> = self.c.events.iter().filter(|e| e.0 > self.seen).copied().collect();
            let batch = Batch::of(&fresh.iter().map(|e| e.1).collect::<Vec<_>>());
            for (seq, ev) in fresh {
                self.seen = seq;
                let reaction = |pl: usize| (self.c.reaction_voice_group(pl), self.c.reaction_sound(pl));
                let cues = cues_in_batch(&ev, trace_sounds, reaction, &batch);
                let tick = self.c.round_ticks;
                if cues.is_empty() {
                    self.lines.push(format!("{:<18} tick {tick:>5}  {ev:?}  ->  (silent)", self.name));
                }
                for cue in cues {
                    let what = match cue {
                        Cue::Sfx(n, _) => format!("play common/{n}  [{}]", address_of(n)),
                        Cue::Voice(pl, g) => format!("voice group {g} of p{}  [list pick, {:?}]", pl + 1, VOICE_LISTS[g as usize - 1]),
                    };
                    self.lines.push(format!("{:<18} tick {tick:>5}  {ev:?}  ->  {what}", self.name));
                    self.cues.push(cue);
                }
            }
            let want = hum_wanted(true, self.c.round_over.is_some(), self.c.crystal.as_ref().map(|cr| cr.id));
            if want != self.hum {
                let tick = self.c.round_ticks;
                self.lines.push(format!("{:<18} tick {tick:>5}  crystal {:?}  ->  {}  [0x14017b966 FUN_14017b880 (hum, 2 loop layers)]", self.name, want.or(self.hum), if want.is_some() { "common/87 loop START" } else { "common/87 loop STOP" }));
                self.hum = want;
            }
        }

        fn run(&mut self, n: usize) {
            for _ in 0..n {
                self.tick([0; 2]);
            }
        }

        fn press(&mut self, who: usize, slot: usize) {
            let mut b = [0u8; 2];
            b[who] = 1 << slot;
            self.tick(b);
            self.tick([0; 2]);
        }

        fn count(&self, id: u32) -> usize {
            self.cues.iter().filter(|c| matches!(c, Cue::Sfx(n, _) if *n == id)).count()
        }

        fn use_skill(&mut self, who: usize, skill: i16) {
            self.c.fighters[who].level = 8;
            self.c.fighters[who].aura_ticks = 8 * crate::combat::AURA_TICKS_PER_POINT;
            self.c.fighters[who].slots[0] = Some(skill);
            self.press(who, 0);
        }
    }

    /// The whole list of new cues on a scripted match, each firing once at the right moment
    /// (P0-SOUND ledger lanes A and B). Prints the trace with `--nocapture`; writes it to
    /// `PD_EVENTS_TRACE=<file>` when set.
    #[test]
    fn a_scripted_match_fires_each_new_cue_once_at_the_right_moment() {
        use crate::combat::testkit::*;
        use crate::combat::{Capsule, ReactionRow, AURA_PARTICLE, AURA_TICKS_PER_POINT, CAPSULE_APPEAR_TICKS};
        let mut all: Vec<String> = Vec::new();
        let mut done = |r: Rig| all.extend(r.lines);

        // 1. Capsules: the opening deal is silent; capture into a button = 86; the respawn
        //    plays the appear sound (85) once.
        let mut r = Rig::new("capsules", vec![def(65, Capsule::Attack, 1, &[(f::MOTION, 6)])], 300.0);
        let spots = vec![Vec3::new(0.0, 0.0, 50.0), Vec3::new(0.0, 0.0, 100.0), Vec3::new(0.0, 0.0, 150.0)];
        r.c.set_capsule_spots([spots.clone(), vec![Vec3::new(200.0, 0.0, 200.0)]]);
        r.c.fighters[0].arsenal = vec![65; 10];
        r.run(2 + CAPSULE_APPEAR_TICKS as usize);
        assert_eq!(r.count(85), 0, "the opening fill is silent");
        r.p[0].pos = spots[1];
        r.press(0, 1);
        assert_eq!(r.count(86), 1, "capture into a button");
        r.p[0].pos = Vec3::ZERO;
        r.run(2 * 420 + CAPSULE_APPEAR_TICKS as usize);
        assert_eq!((r.count(85), r.count(92)), (1, 0), "one respawn, never the erase sound");
        done(r);

        // 2. Using a skill: s19 at the fire tick (Laser: tick 26), the hit = s21 + 6 + voice.
        let mut r = Rig::new("use+hit", vec![def(65, Capsule::Attack, 1, &[(f::MOTION, 6), (f::MOTION_PARAMS, -80), (f::MOTION_PARAMS + 1, 140), (f::MOTION_PARAMS + 2, 8), (f::MOTION_PARAMS + 3, 20), (f::MOTION_PARAMS + 4, 20)])], 300.0);
        r.c.set_action_row(65, Some(ActionRow { anim_name: "ATK_LASER".into(), cast_sound: 242, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: 13, hit_window_end: 0 }), None);
        r.use_skill(0, 65);
        r.run(24);
        assert_eq!(r.count(313), 0, "not at the cast");
        r.run(1);
        assert_eq!(r.count(313), 1, "s19 at the fire tick");
        r.run(120);
        assert_eq!((r.count(509), r.count(6)), (1, 1));
        done(r);

        // 3. A broken defence: 14 once.
        let wall = def(26, Capsule::Defence, 1, &[(43, 2), (f::DEFENCE_KIND, 0), (f::DEFENCE, 3), (f::ANTI_PIERCE, 1)]);
        let blaster = def(70, Capsule::Attack, 1, &[(f::MOTION_PARAMS, -30), (f::MOTION_PARAMS + 1, 140), (f::MOTION_PARAMS + 4, 15)]);
        let mut blaster = blaster;
        blaster.strength = 4;
        let mut r = Rig::new("guard-break", vec![wall, blaster], 100.0);
        r.c.fighters[1].slots[0] = Some(26);
        r.press(1, 0);
        r.use_skill(0, 70);
        r.run(60);
        assert_eq!((r.count(14), r.count(510)), (1, 1), "the break plays 14, the impact at full level");
        done(r);

        // 4. An explosion: 99 once, with the shot's own impact and no common/6 for the blast.
        let mut bomb = def(10, Capsule::Attack, 1, &[(f::MOTION, 1), (f::MOTION_PARAMS, 90), (f::MOTION_PARAMS + 2, 4), (f::MOTION_PARAMS + 3, 10), (f::ADD_EFFECT, 5), (f::ADD_AMOUNT, 300)]);
        bomb.strength = 3;
        let mut r = Rig::new("explosion", vec![bomb], 100.0);
        r.use_skill(0, 10);
        r.run(160);
        assert_eq!((r.count(99), r.count(521), r.count(6)), (1, 1, 0));
        done(r);

        // 5. A knock-down: lands (82) once, then gets up (81) once.
        let rail = def(14, Capsule::Attack, 1, &[(f::MOTION, 6), (f::MOTION_PARAMS + 1, 200)]);
        let mut r = Rig::new("knock-down", vec![rail], 150.0);
        r.c.set_reaction_row(crate::combat::AIR_KNOCKDOWN_ROW, ReactionRow { kind: crate::combat::reaction_kind::KNOCKDOWN, launch_speed: 20, launch_angle: 50, voice_group: 5, w: [0, 45, 0, 0, 0, 48, 0, 0], ..Default::default() });
        r.p[1].grounded = false;
        r.use_skill(0, 14);
        r.run(60 + 70 + crate::combat::KNOCKDOWN_TICKS as usize);
        assert_eq!((r.count(82), r.count(81)), (1, 1));
        done(r);

        // 6. Environment: set 93 + hum start, taken 88 + hum stop; Starting Point cancels = 94.
        let env = def(400, Capsule::Environment, 1, &[(43, 5), (44, 0), (45, 450), (46, -1)]);
        let starting_point = def(159, Capsule::Erase, 1, &[(f::ADD_EFFECT, 38), (f::MOTION, 4), (f::MOTION_PARAMS, 2)]);
        let mut r = Rig::new("environment", vec![env, starting_point], 100.0);
        r.use_skill(0, 400);
        r.run(crate::combat::CAST_TICKS as usize + 4);
        assert_eq!(r.count(93), 1);
        r.use_skill(1, 159);
        r.run(crate::combat::CAST_TICKS as usize + 4);
        assert_eq!((r.count(94), r.count(88)), (1, 0), "a cancellation plays 94, no take sound");
        r.use_skill(0, 400);
        r.run(crate::combat::CAST_TICKS as usize + 4);
        r.p[1].pos = r.c.crystal.as_ref().unwrap().pos;
        r.c.crystal.as_mut().unwrap().age = crate::moves::effects::CRYSTAL_CAPTURE_AFTER + 1;
        r.c.fighters[1].slots = [None; 4];
        r.press(1, 1);
        r.run(crate::combat::CAST_TICKS as usize + 4);
        assert_eq!((r.count(93), r.count(88), r.count(94)), (2, 1, 1), "taken: 88 only, the hum just stops");
        done(r);

        // 7. Erase skills: 92 per capsule (Violent Change with two on the field), then a button
        //    erased by an attack plays 91 (Tiger-Slayer-like attack, s48 = attack colour).
        let violent = def(284, Capsule::Erase, 1, &[(43, 1), (44, 5), (45, 0)]);
        let mut r = Rig::new("erase", vec![violent], 40.0);
        r.c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: 65, pos: Vec3::new(500.0, 0.0, 500.0), appear: 0 });
        r.c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: AURA_PARTICLE, pos: Vec3::new(-500.0, 0.0, -500.0), appear: 0 });
        r.use_skill(0, 284);
        r.run(crate::combat::CAST_TICKS as usize + 4);
        assert_eq!(r.count(92), 2);
        done(r);
        let mut slayer = def(216, Capsule::Attack, 1, &[(f::MOTION_PARAMS, -30), (f::MOTION_PARAMS + 1, 140), (f::MOTION_PARAMS + 4, 15), (48, 1), (49, 2), (50, 1)]);
        slayer.strength = 2;
        let laser2 = def(65, Capsule::Attack, 1, &[(f::MOTION, 6)]);
        let mut r = Rig::new("button-erased", vec![slayer, laser2], 100.0);
        r.c.fighters[1].slots[2] = Some(65);
        r.use_skill(0, 216);
        r.run(80);
        assert_eq!(r.count(91), 1);
        done(r);

        // 8. Aura regenerates: one blip per whole point.
        let mut r = Rig::new("aura-regen", vec![], 300.0);
        r.c.fighters[0].level = 5;
        r.c.fighters[0].aura_ticks = 0;
        r.run(900);
        let gained = (r.c.fighters[0].aura_ticks / AURA_TICKS_PER_POINT) as usize;
        assert!(gained >= 2 && r.count(5) == gained, "one 5 per point: {gained} points, {} blips", r.count(5));
        done(r);

        // 9. A refused press (frozen button) = 2; the Aura Particle's Level +1 = 89.
        let mut r = Rig::new("denied+aura", vec![def(65, Capsule::Attack, 1, &[(f::MOTION, 6)])], 300.0);
        r.c.fighters[0].slots[0] = Some(65);
        r.c.fighters[0].frozen_ticks = 600;
        r.press(0, 0);
        assert_eq!(r.count(2), 1);
        r.c.fighters[0].frozen_ticks = 0;
        r.c.fighters[0].level = 3;
        r.c.fighters[0].slots[1] = Some(AURA_PARTICLE);
        r.press(0, 1);
        assert_eq!(r.count(89), 1);
        done(r);

        // 10. A knock-out: the fallen fighter's voice, once, the moment it falls.
        let mut r = Rig::new("knock-out", vec![], 300.0);
        r.c.change_health(1, -999);
        r.run(3);
        assert_eq!(r.cues.iter().filter(|c| matches!(c, Cue::Voice(1, 6))).count(), 1);
        done(r);

        let trace = all.join("\n");
        if let Some(path) = std::env::var_os("PD_EVENTS_TRACE") {
            std::fs::write(path, &trace).expect("write the trace");
        }
        println!("{trace}");
    }
}
