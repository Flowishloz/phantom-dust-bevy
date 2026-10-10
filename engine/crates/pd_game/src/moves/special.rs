//! Special skills (yellow capsules). Owned by the special lane: only this
//! file changes. Contract: docs/MOVES.md "Capsule colours" and moves/effects.rs.
//!
//! Guide: "Special skills can affect movement, restore status, affect button
//! presses, or capture."
//!
//! `apply` runs when one of these skills takes effect: `reach` says on whom
//! (`Reach::Caster` for "your ...", `Target(i)` when it landed on player i,
//! `Everyone`, `Field` for capsules). Change `c.fighters[..]` (health, level,
//! `aura_ticks`, `slots`, `arsenal`, `used`, `status`, `frozen_ticks`,
//! `poison_ticks`, `capsule_anchor`), `c.capsules`, `c.crystal`, and `players`
//! (positions) as the skill says. Return false for skills not built yet.
//! Deterministic: no clocks, no HashMap iteration; random picks use
//! `c.random(n)`.
//!
//! Field readings (special_skills.json, 45 records):
//! - s43 = 4 ("Self") or 3 (moves/teleports yourself): `Reach::Caster`, split
//!   below by `raw[43]` into "self status/capture" and "you move" (`raw[44]`,
//!   all eight values distinct: `move_kind`).
//! - Everything that arrives as `Reach::Target`/`Reach::Everyone` (self-status
//!   skills never do) carries an "Add:" code in s51 with its amount in s52,
//!   same scheme as the defence lane's own s51 table but with special-capsule
//!   meanings (`add` module below). A few enemy-only records (736, 737) reuse
//!   defence-style codes (6, 2) with their own note text, so they're matched
//!   by id instead.
//! - Orb (180) and Amulet (299) react to erase skills; that's the erase
//!   lane's file. Casting them here does nothing (returns true: handled,
//!   elsewhere).

use super::effects::{Reach, Stat, Status};
use super::table_ticks;
use crate::combat::{Combat, SkillDef, AURA_PARTICLE, AURA_TICKS_PER_POINT, SLOTS};
use crate::sim::Player;
use glam::Vec3;

/// Ids referred to by name below, either because their note's exact wording
/// (not a table code) is the only evidence ("Doubles ...", "???"), or because
/// they need different handling from other skills that share their s51 code
/// (Boost Mine) or the same code but no evidence at all (Frighten, Sword Slap,
/// Vacuum Bullet all read s51 = 0).
mod id {
    pub const HEALING_WATER: i16 = 179;
    pub const GRACEFUL_WOODS: i16 = 178;
    pub const REBIRTH: i16 = 297;
    pub const LEVEL_AMP: i16 = 117;
    pub const LEVEL_BOOST: i16 = 120;
    pub const RECALL: i16 = 119;
    pub const RELEARN: i16 = 118;
    pub const BRAVERY_DECISION: i16 = 55;
    pub const MEPHISTOS_PACT: i16 = 300;
    pub const REGENERATE: i16 = 238;
    pub const INVULNERABILITY: i16 = 298;
    pub const ORB: i16 = 180;
    pub const AMULET: i16 = 299;
    pub const PLAY_DEAD: i16 = 366;
    pub const DOWNCAST: i16 = 354;
    pub const PHANTOM_DUST: i16 = 374;
    #[allow(dead_code)] // the recipient is the tripper now (LIVE take 307); kept for the tests
    pub const BOOST_MINE: i16 = 307;
    pub const VACUUM_BULLET: i16 = 336;
    pub const LEVEL_BERSERK: i16 = 116;
    pub const FRIGHTEN: i16 = 52;
    pub const SWORD_SLAP: i16 = 367;
    pub const PARALYZE_BARRIER: i16 = 736;
    pub const INTERFERENCE: i16 = 737;
}

/// s44 on the eight "you move" specials (s43 = 3 -> `Reach::Caster`). Strong
/// evidence: all eight values are distinct except Surprise Attack and Charge,
/// which also share their note text verbatim ("Teleports you to target's
/// start point").
pub mod move_kind {
    pub const JUMP: i16 = 0;
    pub const RETREAT: i16 = 2;
    pub const CALL: i16 = 4;
    pub const LEVITATE: i16 = 5;
    pub const RETURN: i16 = 6;
    pub const TO_TARGET_SPAWN: i16 = 7; // Surprise Attack / Charge
    pub const DASH: i16 = 8;
}

/// s51 ("Add:" code) on specials that arrive as `Reach::Target` or
/// `Reach::Everyone`. Checked against the note of every skill using each code.
mod add {
    /// Blessed Water (+5 to the target) / Blessed Rain (+5 to everyone).
    pub const HEALTH: i16 = 43;
    /// Boost Mine (+2 to the caster, not whoever trips it) / Quantum Amp (+2
    /// everyone) / Level Baton (+2 to the target it hits).
    pub const LEVEL: i16 = 44;
    /// Teleportation / Teleportrap ("Teleports target to your start point") /
    /// Mobilize ("All players are teleported to your start points").
    pub const TELEPORT_TO_CASTER_SPAWN: i16 = 45;
    /// Fusion: "Your Health and target's are averaged."
    pub const FUSION: i16 = 46;
    /// Dazzle: "Scrambles target's skill buttons for 10 sec" (s52 = 300, table_ticks -> 600 sim ticks = 10 sec, confirmed unit).
    pub const DAZZLE: i16 = 41;
    /// Trigger: "Forces target to use equipped skill assigned to same button."
    pub const TRIGGER: i16 = 42;
    /// Annoying Gift: "Forces target to capture your equipped skills."
    pub const ANNOYING_GIFT: i16 = 50;
    /// Angel's Breath / Armaros Watch (target) / Armaros (everyone):
    /// "Reverses ... attributes (Speed +2 becomes Speed -2)."
    pub const REVERSE_ATTRIBUTES: i16 = 51;
    /// Level Berserk only: note says "Doubles target's Level" (s52 = -2 here,
    /// not a usable amount, so the note's wording is the source of truth).
    pub const LEVEL_BERSERK_DOUBLE: i16 = 4;
    /// Paralyze Barrier (736) only: reuses the defence lane's PARALYSE code.
    /// s52 = 90 table ticks, and FORMATS-SKILLS.md confirms the unit exactly
    /// from this note ("Target can't move for 3 sec" = 90).
    pub const PARALYZE: i16 = 6;
    /// Interference (737) only: reuses the defence lane's AURA_FROM_COST slot
    /// for a different, special-capsule meaning ("Reduces target's Level by 2", s52 = 2).
    pub const LEVEL_DRAIN: i16 = 2;
}

/// Sim ticks per program tick (the wind-up is the clip's length less two program ticks).
fn moves_ticks_per_table() -> u16 {
    super::SIM_TICKS_PER_TABLE_TICK
}

/// unconfirmed: Frighten ("Briefly stops target's movement") and Sword Slap
/// (identical note) give no numeric field for the bind's length. Paralyze
/// Barrier (736, s52 = 90 table ticks) confirms "3 sec" for the same kind of
/// bind, so "briefly" is picked well under that.
const BRIEF_BIND_TICKS: u16 = 30;

/// "Regenerates Health while immobile" (Regenerate) and "Health recovers while
/// immobile" (Play Dead): player state 21 counts program ticks and at 91
/// (`0x5b`) gives +1 Health and starts again (`FUN_14025b5e0`, state `0x15`:
/// `if (++counter >= 0x5b) { Health += 1; counter = 0 }`) = 182 sim ticks, not
/// the 1 point per second this used to guess.
const REGEN_TICKS: u16 = 91 * 2;

pub fn apply(c: &mut Combat, players: &mut [Player], caster: usize, reach: Reach, def: &SkillDef) -> bool {
    match reach {
        Reach::Caster => self_effect(c, players, caster, def),
        Reach::Target(target) => target_effect(c, players, caster, target, def),
        Reach::Everyone => everyone_effect(c, players, caster, def),
        // None of the 45 special skills read s43 = 1 ("Capsule"): nothing to do.
        Reach::Field => false,
    }
}

/// "Self" (s43 = 4, self-status/capture) and "you move" (s43 = 3) specials:
/// both arrive as `Reach::Caster`.
fn self_effect(c: &mut Combat, players: &mut [Player], caster: usize, def: &SkillDef) -> bool {
    if def.raw[43] == 3 {
        return move_self(c, players, caster, def);
    }
    match def.id {
        // "Restores 5 Health." (s49 = 5, matches).
        id::HEALING_WATER => {
            let f = &mut c.fighters[caster];
            f.health = (f.health + def.raw[49] as i32).min(f.max_health);
            true
        }
        // "Restores Aura to maximum." Max aura is `level` points (Combat::step
        // caps recovery at `f.level * AURA_TICKS_PER_POINT`); s49 = 99 is a
        // "maximum" sentinel, not a literal amount.
        id::GRACEFUL_WOODS => {
            let f = &mut c.fighters[caster];
            f.aura_ticks = f.level * AURA_TICKS_PER_POINT;
            true
        }
        // "Restores 10 Health. Penalty: Reduces Level to 0." (s49 = 10,
        // matches the heal). The record's own level_penalty (s23 = -1) is
        // already applied generically in Combat::cast as `level -= penalty`,
        // which would ADD a level instead of zeroing it, so "reduces Level to
        // 0" has to be done explicitly here, overriding whatever that left.
        id::REBIRTH => {
            let f = &mut c.fighters[caster];
            f.health = (f.health + def.raw[49] as i32).min(f.max_health);
            f.level = 0;
            f.aura_ticks = 0; // max aura at level 0 is 0
            true
        }
        // "Increases your Level by 2." (s49 = 2, matches).
        id::LEVEL_AMP => {
            let f = &mut c.fighters[caster];
            f.level = (f.level + def.raw[49] as i32).min(f.level_max);
            true
        }
        // "Doubles your Level. Requires: Level >10" (the requirement is
        // already gated by Combat::requirement_met). s49 = 2 here is
        // misleading (it's not "+2"); the note's "Doubles" is the source of truth.
        id::LEVEL_BOOST => {
            let f = &mut c.fighters[caster];
            f.level = (f.level * 2).min(f.level_max);
            f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT);
            true
        }
        // "Restores 5 random consumed skills to your arsenal." (s49 = 5, matches).
        id::RECALL => recall(c, caster),
        // "Automatically captures last skill consumed": the last entry of the
        // consumed list (`FUN_140270b00` case 0x13: pop, then
        // `FUN_140261a00(own button, skill)`) lands on Relearn's OWN button,
        // replacing it. Relearn's use is not taken at the fire tick
        // (`FUN_1402626d0`), so it is not itself on the consumed list; with
        // nothing consumed yet it stays where it is.
        id::RELEARN => {
            let button = c.fighters[caster].last_slot;
            if let Some(skill) = c.fighters[caster].used.pop() {
                c.fighters[caster].set_slot(button, Some(skill));
                // `FUN_140270b00` case 0x13 -> `FUN_140261a00` with its sound: common/86 (LIVE, Relearn take).
                c.log(crate::combat::CombatEvent::SkillCaptured { who: caster, slot: button, skill });
            }
            true
        }
        // "Erases all Aura Particles from your arsenal."
        id::BRAVERY_DECISION => {
            c.fighters[caster].arsenal.retain(|&s| s != AURA_PARTICLE);
            true
        }
        // "Immediately captures an unconsumed skill from your arsenal.
        // Penalty: 3 Health." (health penalty already applied generically via
        // `health_cost`). Arsenal's next element is last (Fighter::arsenal
        // doc), so pop takes the one that would've come out next.
        id::MEPHISTOS_PACT => {
            if let Some(skill) = c.fighters[caster].arsenal.pop() {
                if let Some(slot) = c.fighters[caster].slots.iter().position(Option::is_none) {
                    c.fighters[caster].set_slot(slot, Some(skill));
                } else {
                    c.fighters[caster].arsenal.push(skill); // no free slot: leave it
                }
            }
            true
        }
        // "Regenerates Health while immobile." The sim (Combat::step) already
        // reads `status.regen_every` and adds +1 Health every that many ticks
        // while `still_ticks > 0`; the rate is the program's own (`REGEN_TICKS`).
        id::REGENERATE => {
            c.fighters[caster].status.regen_every = REGEN_TICKS;
            true
        }
        // "Immune to damage while immobile" -> Fighter::status_blocks already
        // reads `status.still_immune` together with `still_ticks`.
        id::INVULNERABILITY => {
            c.fighters[caster].status.still_immune = true;
            true
        }
        // "Health recovers while immobile. For 30 sec, movement speed is
        // reduced by 50%." s49 = -50 (Speed) and s53 = 900 table ticks (30 sec
        // exactly) both match the note directly.
        id::PLAY_DEAD => {
            let f = &mut c.fighters[caster];
            f.status.regen_every = REGEN_TICKS;
            f.status.add(Stat::Speed, def.raw[49] as i32, Some(table_ticks(def.raw[53])));
            true
        }
        // Orb / Amulet react to erase skills; that's the erase lane's file
        // (moves/erase.rs). Casting them here does nothing.
        id::ORB | id::AMULET => true,
        // "Penalty: 1 Level." is the whole note; no positive effect is described.
        id::DOWNCAST => false,
        // Note is literally "???"; no evidence to build against.
        id::PHANTOM_DUST => false,
        _ => false,
    }
}

/// "Restores 5 random consumed skills to your arsenal." `used` already has
/// this very Recall cast as its last entry (Combat::cast pushes it before
/// dispatch), so up to 5 of the entries *before* that come back, picked at
/// random with `c.random`.
fn recall(c: &mut Combat, caster: usize) -> bool {
    for _ in 0..5 {
        let eligible = c.fighters[caster].used.len().saturating_sub(1);
        if eligible == 0 {
            break;
        }
        let idx = c.random(eligible as u32) as usize;
        let skill = c.fighters[caster].used.remove(idx);
        c.fighters[caster].arsenal.push(skill);
    }
    true
}

/// The eight "you move" specials (s43 = 3), keyed by s44 (`move_kind`).
fn move_self(c: &mut Combat, players: &mut [Player], caster: usize, def: &SkillDef) -> bool {
    match def.raw[44] {
        // "Teleports you to your start point."
        move_kind::RETURN => {
            players[caster].pos = players[caster].spawn;
            true
        }
        // Surprise Attack / Charge: "Teleports you to target's start point."
        move_kind::TO_TARGET_SPAWN => {
            // "next to a target (random opponent in 4-player)", PROGRAM-RE.md "Warps"
            // (`FUN_140257e90`, not read line by line: UNCONFIRMED whether the pick is
            // random or the lock target). Against one enemy there is nothing to pick,
            // so no random number is drawn and a duel replays as it always did.
            let enemies: Vec<usize> = c.roster.enemies_of(caster).collect();
            let target = if enemies.len() > 1 { enemies[c.random(enemies.len() as u32) as usize] } else { c.aim_target(caster, players) };
            players[caster].pos = players[target].spawn;
            true
        }
        // "Sets capsule spawn point to current location for duration of
        // battle." -> Fighter::capsule_anchor (read by Combat::spawn_capsule).
        move_kind::CALL => {
            c.fighters[caster].capsule_anchor = Some(players[caster].pos);
            true
        }
        // "Teleports you to one of your capsule spawn points" (the hint: one
        // of your capsules currently on the field, via `c.random`); no-op if
        // none of yours are out.
        move_kind::RETREAT => {
            let mine: Vec<Vec3> = c.capsules.iter().filter(|cap| cap.owner == caster).map(|cap| cap.pos).collect();
            if !mine.is_empty() {
                let idx = c.random(mine.len() as u32) as usize;
                players[caster].pos = mine[idx];
            }
            true
        }
        // Levitate (s44 = 5): float status 11 for s46 ticks, then the s44 = 0 jump of height s45
        // that hangs at its top (`FUN_140270200` case 5, which falls into case 0). Skywalk is the
        // same record shape (s44 = 5) and goes through the same body (`float_and_jump`).
        move_kind::LEVITATE => float_and_jump(c, players, caster, def),
        // The program's Jump (skill 239, `FUN_140270200` s44 = 0 -> `FUN_140257b90`).
        move_kind::JUMP => {
            let (wind, launch) = jump_numbers(c, caster, def);
            players[caster].begin_jump(wind, launch);
            true
        }
        // The Dash skill (s44 = 8, `FUN_140270200` case 8): see `dash`.
        move_kind::DASH => dash(c, players, caster, def),
        _ => false,
    }
}

/// The wind-up (sim ticks) and the launch speed (u/s) of the s44 = 0 / 5 jump
/// (`FUN_140270200` cases 0 and 5 -> `FUN_140257b90(h = s45, kind, now = 0, g)`):
/// - height `h` = s45, launch speed `v = sqrt(20 * g * h)` per program tick, so the apex is
///   `v^2 / 2g = 10 h` u (Jump s45 = 10 -> 100 u, Levitate 10 -> 100 u, Skywalk 5 -> 50 u);
/// - gravity `g = (s47 + 100) * 0.12 * 0.01` u per program tick squared (`0x1402f7ce4`,
///   `0x1402f7c74`): s47 is 0 on all three records, so 0.12 = `sim::GRAVITY` (108 u/s^2 =
///   0.12 x 30 x 30). That is why this builds the launch from `sim::GRAVITY`;
/// - `now = 0`: the start clip (JUMP_START, kind rows 0-4 and 6 of the table at `0x1403fae20`) plays
///   first and the state lasts its length - 2 program ticks (13 for the 15-frame clip), 2 sim ticks each.
/// Program ticks are 30 Hz, sim ticks 60 Hz: the launch is converted as u/s (x 30), the wind-up
/// as x 2 (`SIM_TICKS_PER_TABLE_TICK`).
fn jump_numbers(c: &Combat, caster: usize, def: &SkillDef) -> (u16, f32) {
    debug_assert_eq!(def.raw[47], 0, "gravity (s47 + 100) x 0.0012 is the ordinary 0.12 only for s47 = 0");
    let apex = 10.0 * def.raw[45].max(1) as f32;
    let launch = (2.0 * crate::sim::GRAVITY * apex).sqrt();
    let clip = c.clip_hold_ticks(caster, "JUMP_START");
    let wind = clip.saturating_sub(2 * moves_ticks_per_table()).max(2);
    (wind, launch)
}

/// Levitate and Skywalk (`FUN_140270200` case 5): float status 11 for s46 program ticks
/// (`+0x6240 = 1`, `+0x6244 = s46`; s46 = 0 would be the permanent slot `+0x623c`), then the case-0
/// jump of height s45 (`jump_numbers`) -- and at the top of that rise, while status 11 holds, the
/// body stops (`0x140254590`, `Player::begin_float`). So the body takes 13 + ~41 program ticks to
/// reach `10 x s45` u and hangs there for the rest of the s46 ticks; when the status ends it falls.
/// Status 11 is cleared by a hit (`FUN_14026f870` tail), Levitate included (same status as Skywalk).
pub(super) fn float_and_jump(c: &mut Combat, players: &mut [Player], caster: usize, def: &SkillDef) -> bool {
    let (wind, launch) = jump_numbers(c, caster, def);
    let f = &mut c.fighters[caster];
    f.status.hover_ticks = if def.raw[46] > 0 { table_ticks(def.raw[46]) } else { u16::MAX };
    // Informational: where the rise ends. The float is a real jump now, not a pin to this height.
    f.status.hover_height = 10.0 * def.raw[45].max(1) as f32;
    f.status_hover = true;
    players[caster].begin_float(wind, launch);
    true
}

/// The Dash skill (`FUN_140270200` case 8, row = the skill's s12 = 294, SPE_DASH):
/// - length `len = w42 - w43` program ticks of the action row (`+0x54` - `+0x56`; the row has
///   dash 0..15 and fire 0, so 15; PROGRAM-RE.md writes it w42 - w41, the same number here);
/// - step per program tick `s45 * 10 / len` u, never more than 10 (`0x1402f7f7c`): 10 * 10 / 15 =
///   6.67 u, i.e. 100 u in all (`FUN_14025afe0` moves `facing x step` for `len` ticks, state 3);
/// - no steering, no Aura-rate change; the walls stop it like any move (the sim's wall push).
/// 30 Hz -> 60 Hz: `2 * len` sim ticks at `step / 2` u each = `step * 30` u/s.
fn dash(c: &Combat, players: &mut [Player], caster: usize, def: &SkillDef) -> bool {
    let Some(row) = c.action_row(def.id, false) else { return true };
    let len = row.dash_end as i32 - row.fire_tick as i32;
    if len < 1 {
        // The program stores speed 0 and moves nothing (`iVar10 < 1`).
        return true;
    }
    let step = (def.raw[45] as f32 * 10.0 / len as f32).min(10.0);
    let per_sim_tick = step / moves_ticks_per_table() as f32;
    players[caster].begin_dash(len as u16 * moves_ticks_per_table(), per_sim_tick / crate::sim::DT);
    true
}

/// Specials that land on the other player (s51 = "Add:" code, s52 = amount).
fn target_effect(c: &mut Combat, players: &mut [Player], caster: usize, target: usize, def: &SkillDef) -> bool {
    match def.raw[51] {
        // Blessed Water: "Target gains 5 Health."
        add::HEALTH => {
            let f = &mut c.fighters[target];
            f.health = (f.health + def.raw[52] as i32).min(f.max_health);
            true
        }
        // Level Baton hits the target. LIVE (take 307, the recorder's film): Boost Mine's Level +2 goes to
        // whoever TRIPS the mine, not to its setter -- P0 walked onto its own mine (+2 to P0), then P1 did (+2 to
        // P1, P0 unchanged); the note's "your Level" is the tripper's. The hit target is the tripper.
        add::LEVEL => {
            let f = &mut c.fighters[target];
            f.level = (f.level + def.raw[52] as i32).min(f.level_max);
            true
        }
        // Teleportation / Teleportrap: "Teleports target to your start point."
        add::TELEPORT_TO_CASTER_SPAWN => {
            players[target].pos = players[caster].spawn;
            true
        }
        // Fusion: "Your Health and target's are averaged." (integer average;
        // the odd point, if any, is dropped).
        // LIVE (take 294: P0 20, P1 40 -> P1 30, P0 stays 20): only the TARGET is set to the average. The program
        // (`0x140272f48`, case 0x2e) sets the target, then changes the caster by (average - caster) only
        // when `DAT_1404bc9d4` is not 4 / 5 (a networked mode queues it instead); the offline take shows the caster
        // unchanged, so UNCONFIRMED which mode the PC build's local versus runs in, LIVE says no change.
        add::FUSION => {
            let avg = ((c.fighters[caster].health + c.fighters[target].health) / 2).max(1);
            c.fighters[target].health = avg;
            true
        }
        // "Scrambles target's skill buttons for 10 sec." (s52 = 300 table
        // ticks = 10 sec exactly). Fisher-Yates with `c.random`, same pattern
        // Combat::start_round uses for shuffling an arsenal.
        add::DAZZLE => {
            let mut scramble: [u8; SLOTS] = [0, 1, 2, 3];
            for k in (1..SLOTS).rev() {
                let j = c.random(k as u32 + 1) as usize;
                scramble.swap(k, j);
            }
            let f = &mut c.fighters[target];
            f.status.scrambled_ticks = table_ticks(def.raw[52]);
            f.status.scramble = scramble;
            true
        }
        // "Forces target to use equipped skill assigned to same button": the
        // button the caster used (`Fighter::last_slot`).
        add::TRIGGER => {
            // Only a player who is free to act can be made to act (not one
            // mid-cast, stunned, knocked down or knocked out).
            if c.fighters[target].action != crate::combat::Action::Ready {
                return true;
            }
            let slot = c.fighters[caster].last_slot;
            if c.fighters[target].slots[slot].is_some() {
                c.cast_forced(target, slot, players);
            }
            true
        }
        // "Forces target to capture your equipped skills": each of the
        // caster's OTHER skills lands on the target's matching button (replacing
        // what was there); the button the gift was pressed from is skipped
        // (`FUN_140272bc0` case 0x32: `if (attack->button != k && caster slot k is
        // occupied) capture(target slot k, caster slot k's skill)`, `param_2 + 0xc` is the
        // pressed button), so the target never receives Annoying Gift itself. The target's
        // other buttons and anything that "can't be erased or overwritten" stay (the
        // capture, `FUN_140261a00`, tests the slot's current skill for s41 = 2).
        add::ANNOYING_GIFT => {
            let mine = c.fighters[caster].slots;
            let pressed = c.fighters[caster].last_slot;
            for (k, gift) in mine.into_iter().enumerate() {
                if k == pressed {
                    continue;
                }
                let kept = c.fighters[target].slots[k].and_then(|id| c.skill(id)).is_some_and(SkillDef::protected);
                if let (Some(skill), false) = (gift, kept) {
                    c.fighters[target].set_slot(k, gift);
                    // One `FUN_140261a00` per skill, each with its common/86 (LIVE: Annoying Gift, 3 captures, 3 sounds).
                    c.log(crate::combat::CombatEvent::SkillCaptured { who: target, slot: k, skill });
                }
            }
            true
        }
        // Angel's Breath / Armaros Watch: "Reverses target's attributes."
        add::REVERSE_ATTRIBUTES => {
            reverse_attributes(&mut c.fighters[target].status);
            true
        }
        // Level Berserk only ("Doubles target's Level"; s52 = -2 is not a
        // usable amount here, the note's wording is the source of truth).
        add::LEVEL_BERSERK_DOUBLE if def.id == id::LEVEL_BERSERK => {
            let f = &mut c.fighters[target];
            f.level = (f.level * 2).min(f.level_max);
            f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT);
            true
        }
        // Paralyze Barrier (736) only: "Target can't move for 3 sec" (s52 =
        // 90 table ticks, the unit FORMATS-SKILLS.md confirms from this exact note).
        add::PARALYZE if def.id == id::PARALYZE_BARRIER => {
            c.fighters[target].status.bound_ticks = table_ticks(def.raw[52]);
            true
        }
        // Interference (737) only: "Reduces target's Level by 2" (s52 = 2, matches).
        add::LEVEL_DRAIN if def.id == id::INTERFERENCE => {
            let f = &mut c.fighters[target];
            f.level = (f.level - def.raw[52] as i32).max(0);
            f.aura_ticks = f.aura_ticks.min(f.level * AURA_TICKS_PER_POINT);
            true
        }
        // Frighten / Sword Slap: "Briefly stops target's movement." No s51
        // code at all (both read 0); length is unconfirmed (BRIEF_BIND_TICKS).
        0 if def.id == id::FRIGHTEN || def.id == id::SWORD_SLAP => {
            c.fighters[target].status.bound_ticks = BRIEF_BIND_TICKS;
            true
        }
        // Vacuum Bullet: "Penetrates defensive skills" is the whole note, and
        // that's already handled generically before this ever runs
        // (SkillDef::ignores_defence, read by barrier::resolve in
        // Combat::strike) -- nothing left to do on landing.
        0 if def.id == id::VACUUM_BULLET => true,
        _ => false,
    }
}

/// Specials that land on both players at once (motion 4, s64 = 2).
fn everyone_effect(c: &mut Combat, players: &mut [Player], caster: usize, def: &SkillDef) -> bool {
    match def.raw[51] {
        // Blessed Rain: "Everyone gains +5 Health."
        add::HEALTH => {
            for i in c.roster.slots() {
                let f = &mut c.fighters[i];
                f.health = (f.health + def.raw[52] as i32).min(f.max_health);
            }
            true
        }
        // Quantum Amp: "Increases everyone's Level by 2."
        add::LEVEL => {
            for i in c.roster.slots() {
                let f = &mut c.fighters[i];
                f.level = (f.level + def.raw[52] as i32).min(f.level_max);
            }
            true
        }
        // Armaros: "Reverses everyone's attributes."
        add::REVERSE_ATTRIBUTES => {
            for i in c.roster.slots() {
                reverse_attributes(&mut c.fighters[i].status);
            }
            true
        }
        // Mobilize (unregistered, enemy/debug): "All players are teleported
        // to your start points" -- by analogy with Teleportation/Teleportrap
        // (same code, singular "your start point"), everyone goes to the
        // caster's spawn. unconfirmed: the note's plural "points" could mean
        // something else.
        add::TELEPORT_TO_CASTER_SPAWN => {
            let spawn = players[caster].spawn;
            for i in c.roster.slots() {
                players[i].pos = spawn;
            }
            true
        }
        _ => false,
    }
}

/// "Reverses ... attributes (Speed +2 becomes Speed -2)": negate every
/// currently active status modifier's amount.
fn reverse_attributes(status: &mut Status) {
    for m in &mut status.mods {
        m.amount = -m.amount;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::testkit::*;

    // ---- Self: status / capture (s43 = 4) ----------------------------------

    #[test]
    fn healing_water_restores_five_health() {
        let (mut c, mut p, col) = setup(vec![def(id::HEALING_WATER, Capsule::Special, 4, &[(43, 4), (49, 5)])], 100.0);
        c.fighters[0].health = 10;
        use_skill(&mut c, &mut p, &col, 0, id::HEALING_WATER);
        assert_eq!(c.fighters[0].health, 15);
    }

    #[test]
    fn graceful_woods_restores_aura_to_the_level_cap() {
        let (mut c, mut p, col) = setup(vec![def(id::GRACEFUL_WOODS, Capsule::Special, 1, &[(43, 4), (49, 99)])], 100.0);
        c.fighters[0].level = 8;
        c.fighters[0].aura_ticks = 0;
        use_skill(&mut c, &mut p, &col, 0, id::GRACEFUL_WOODS);
        assert_eq!(c.fighters[0].aura(), 8);
    }

    #[test]
    fn rebirth_heals_and_zeroes_level_despite_the_generic_penalty() {
        let (mut c, mut p, col) = setup(vec![def(id::REBIRTH, Capsule::Special, 0, &[(43, 4), (23, -1), (49, 10)])], 100.0);
        c.fighters[0].health = 5;
        c.fighters[0].level = 6;
        use_skill(&mut c, &mut p, &col, 0, id::REBIRTH);
        assert_eq!((c.fighters[0].health, c.fighters[0].level, c.fighters[0].aura()), (15, 0, 0));
    }

    #[test]
    fn level_amp_and_boost_change_your_own_level() {
        // Direct `apply` calls (allowed by the lane brief): `use_skill` pins
        // level/aura to 8 before casting, which would hide the +2 vs doubled distinction.
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.fighters[0].level = 4;
        let amp = def(id::LEVEL_AMP, Capsule::Special, 3, &[(43, 4), (49, 2)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Caster, &amp));
        assert_eq!(c.fighters[0].level, 6, "+2");
        let boost = def(id::LEVEL_BOOST, Capsule::Special, 7, &[(43, 4), (49, 2)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Caster, &boost));
        assert_eq!(c.fighters[0].level, 12, "doubled, not +2");
    }

    #[test]
    fn recall_restores_five_random_used_skills_excluding_itself() {
        let (mut c, mut p, col) = setup(vec![def(id::RECALL, Capsule::Special, 5, &[(43, 4), (49, 5), (26, 1)])], 100.0);
        c.fighters[0].arsenal.clear(); // setup()'s shared 30-skill loadout would otherwise dilute the count below
        c.fighters[0].used = vec![10, 11, 12, 13, 14, 15, 16];
        use_skill(&mut c, &mut p, &col, 0, id::RECALL);
        // 7 prior + the Recall cast itself pushed by Combat::cast = 8; 5 come back.
        assert_eq!(c.fighters[0].used.len(), 3, "5 of the 7 prior uses came back, Recall itself stayed");
        assert_eq!(c.fighters[0].arsenal.len(), 5);
        assert!(c.fighters[0].arsenal.iter().all(|s| *s != id::RECALL), "never restores itself");
    }

    #[test]
    fn relearn_puts_the_last_consumed_skill_on_its_own_button() {
        let (mut c, mut p, col) = setup(vec![def(id::RELEARN, Capsule::Special, 3, &[(43, 4), (45, 19), (26, 1)])], 100.0);
        c.fighters[0].used = vec![41, 42];
        use_skill(&mut c, &mut p, &col, 0, id::RELEARN);
        assert_eq!(c.fighters[0].slots[0], Some(42), "the last consumed skill replaces Relearn on its button");
        assert_eq!(c.fighters[0].used, vec![41], "popped off the list; Relearn did not consume itself");
    }

    #[test]
    fn relearn_with_nothing_consumed_stays_on_its_button() {
        // Its use is not taken at the fire tick: nothing to capture, nothing lost.
        let (mut c, mut p, col) = setup(vec![def(id::RELEARN, Capsule::Special, 3, &[(43, 4), (45, 19), (26, 1)])], 100.0);
        use_skill(&mut c, &mut p, &col, 0, id::RELEARN);
        assert_eq!(c.fighters[0].slots[0], Some(id::RELEARN));
        assert!(c.fighters[0].used.is_empty());
    }

    #[test]
    fn bravery_decision_erases_aura_particles_only() {
        let (mut c, mut p, col) = setup(vec![def(id::BRAVERY_DECISION, Capsule::Special, 0, &[(43, 4)])], 100.0);
        c.fighters[0].arsenal = vec![AURA_PARTICLE, 7, AURA_PARTICLE, 8];
        use_skill(&mut c, &mut p, &col, 0, id::BRAVERY_DECISION);
        assert_eq!(c.fighters[0].arsenal, vec![7, 8]);
    }

    #[test]
    fn mephistos_pact_captures_the_next_arsenal_skill() {
        let (mut c, mut p, col) = setup(vec![def(id::MEPHISTOS_PACT, Capsule::Special, 0, &[(43, 4), (25, 3)])], 100.0);
        c.fighters[0].arsenal = vec![7, 8, 9];
        use_skill(&mut c, &mut p, &col, 0, id::MEPHISTOS_PACT);
        assert_eq!(c.fighters[0].arsenal, vec![7, 8]);
        assert!(c.fighters[0].slots.contains(&Some(9)));
    }

    #[test]
    fn regenerate_and_invulnerability_set_their_status_flags() {
        let (mut c, mut p, col) = setup(vec![def(id::REGENERATE, Capsule::Special, 6, &[(43, 4)]), def(id::INVULNERABILITY, Capsule::Special, 4, &[(43, 4)])], 100.0);
        use_skill(&mut c, &mut p, &col, 0, id::REGENERATE);
        assert!(c.fighters[0].status.regen_every > 0);
        use_skill(&mut c, &mut p, &col, 0, id::INVULNERABILITY);
        assert!(c.fighters[0].status.still_immune);
    }

    #[test]
    fn play_dead_slows_for_thirty_seconds_and_regenerates() {
        let (mut c, mut p, col) = setup(vec![def(id::PLAY_DEAD, Capsule::Special, 5, &[(43, 4), (49, -50), (53, 900)])], 100.0);
        use_skill(&mut c, &mut p, &col, 0, id::PLAY_DEAD);
        assert_eq!(c.stat(0, Stat::Speed), -50);
        assert!(c.fighters[0].status.regen_every > 0);
    }

    #[test]
    fn downcast_and_phantom_dust_are_not_built() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        let downcast = def(id::DOWNCAST, Capsule::Special, 1, &[(43, 4)]);
        let phantom = def(id::PHANTOM_DUST, Capsule::Special, 99, &[(43, 4)]);
        assert!(!apply(&mut c, &mut p, 0, Reach::Caster, &downcast));
        assert!(!apply(&mut c, &mut p, 0, Reach::Caster, &phantom));
    }

    #[test]
    fn orb_and_amulet_are_handled_elsewhere() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        let orb = def(id::ORB, Capsule::Special, 2, &[(43, 4)]);
        let amulet = def(id::AMULET, Capsule::Special, 1, &[(43, 4)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Caster, &orb));
        assert!(apply(&mut c, &mut p, 0, Reach::Caster, &amulet));
    }

    // ---- "You move" (s43 = 3) ----------------------------------------------

    #[test]
    fn return_teleports_you_to_your_spawn() {
        let (mut c, mut p, col) = setup(vec![def(57, Capsule::Special, 2, &[(43, 3), (44, move_kind::RETURN)])], 100.0);
        p[0].pos = Vec3::new(40.0, 0.0, 40.0);
        use_skill(&mut c, &mut p, &col, 0, 57);
        assert_eq!(p[0].pos, p[0].spawn);
    }

    #[test]
    fn surprise_attack_teleports_you_to_the_targets_spawn() {
        let (mut c, mut p, col) = setup(vec![def(58, Capsule::Special, 3, &[(43, 3), (44, move_kind::TO_TARGET_SPAWN)])], 100.0);
        let target_spawn = p[1].spawn;
        use_skill(&mut c, &mut p, &col, 0, 58);
        assert_eq!(p[0].pos, target_spawn);
    }

    #[test]
    fn retreat_teleports_to_one_of_your_own_capsules() {
        let (mut c, mut p, col) = setup(vec![def(60, Capsule::Special, 5, &[(43, 3), (44, move_kind::RETREAT)])], 100.0);
        let spot = Vec3::new(15.0, 0.0, 15.0);
        c.capsules.push(CapsuleItem { id: 1, owner: 0, skill: 7, pos: spot, appear: 0 });
        c.capsules.push(CapsuleItem { id: 2, owner: 1, skill: 7, pos: Vec3::new(-15.0, 0.0, -15.0), appear: 0 });
        use_skill(&mut c, &mut p, &col, 0, 60);
        assert_eq!(p[0].pos, spot, "the only capsule that's actually theirs");
    }

    #[test]
    fn call_sets_the_capsule_anchor_here() {
        let (mut c, mut p, col) = setup(vec![def(59, Capsule::Special, 3, &[(43, 3), (44, move_kind::CALL)])], 100.0);
        p[0].pos = Vec3::new(20.0, 0.0, -30.0);
        use_skill(&mut c, &mut p, &col, 0, 59);
        assert_eq!(c.fighters[0].capsule_anchor, Some(Vec3::new(20.0, 0.0, -30.0)));
    }

    /// Casts `skill` from button 0 in a real match on a flat floor and records the caster's height and
    /// position every sim tick for `ticks` ticks (the body is stepped by `Match::step`, the sim's physics).
    fn track(skill: SkillDef, row: Option<crate::combat::ActionRow>, ticks: usize) -> (crate::sim::Match, Vec<(f32, Vec3)>) {
        use crate::sim::{Input, Match};
        let mut col = crate::combat::testkit::floor();
        let id = skill.id;
        let mut combat = crate::combat::Combat::new(vec![skill], vec![crate::combat::AURA_PARTICLE; 30]);
        combat.set_rules(crate::combat::testkit::TEST_RULES);
        combat.round_ticks = crate::combat::COUNTDOWN_TICKS;
        if let Some(row) = row {
            combat.set_action_row(id, Some(row), None);
        }
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 400.0)], combat);
        m.combat.fighters[0].level = 8;
        m.combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        m.combat.fighters[0].slots[0] = Some(id);
        let mut log = Vec::new();
        for t in 0..ticks {
            let skills = if t == 0 { 1 } else { 0 };
            m.step([Input { skills, ..Default::default() }, Input::default()], &mut col);
            log.push((m.players[0].pos.y, m.players[0].pos));
        }
        (m, log)
    }

    /// Levitate's and Skywalk's own row (254 / 229): WAIT0, fire 0 -- the effect takes hold at the press.
    fn instant_row() -> crate::combat::ActionRow {
        crate::combat::ActionRow { anim_name: "WAIT0".into(), cast_sound: 454, charge_cycle: 0, dash_start: 0, dash_end: 0, fire_tick: 0, hit_window_end: 0 }
    }

    /// Levitate (s44 = 5, s45 = 10, s46 = 90, s47 = 0): `FUN_140270200` case 5 sets float status 11 for
    /// s46 program ticks and makes the case-0 jump of height s45: a 13-program-tick wind-up, then a
    /// rise under gravity 0.12 that stops at 10 x s45 = 100 u (`vy` zeroed at the first `vy <= 0`
    /// with status 11, `0x140254590`), then it hangs there until the status ends, then falls.
    #[test]
    fn levitate_rises_over_time_to_a_hundred_units_and_hangs_there_for_the_status() {
        let levitate = def(56, Capsule::Special, 2, &[(43, 3), (44, move_kind::LEVITATE), (45, 10), (46, 90)]);
        let (m, log) = track(levitate, Some(instant_row()), 420);
        let ys: Vec<f32> = log.iter().map(|l| l.0).collect();
        let apex = ys.iter().cloned().fold(0.0f32, f32::max);
        // Not a snap: the first rise is the 26-sim-tick wind-up, then a ballistic climb.
        let first_up = ys.iter().position(|&y| y > 0.5).unwrap();
        assert!(first_up >= 26, "rooted in the start clip (13 program ticks = 26 sim ticks): rose at {first_up}");
        let top_at = ys.iter().position(|&y| y >= apex - 0.01).unwrap();
        // ~41 program ticks (sqrt(2 * 100 / 0.12) = 40.8) = ~82 sim ticks of rise.
        assert!(top_at - first_up >= 78 && top_at - first_up <= 90, "rise took {} sim ticks", top_at - first_up);
        assert!((apex - 100.0).abs() < 3.0, "apex 10 x s45 = 100 u: {apex}");
        // Floats: the height does not move while the status runs. Status 11 lasts s46 = 90 program
        // ticks = 180 sim ticks from the cast (tick 0), so it hangs from `top_at` to about tick 180.
        assert!(ys[top_at..170].iter().all(|&y| (y - apex).abs() < 0.01), "hangs at the top");
        // ...and when it ends, it falls under the ordinary gravity and lands.
        assert!(ys[200] < apex - 1.0, "falling after the status ended: {}", ys[200]);
        assert!(m.players[0].grounded && m.players[0].pos.y.abs() < 0.01, "back on the floor");
        assert_eq!(m.combat.fighters[0].status.hover_ticks, 0, "status over");
        assert!(m.combat.fighters[0].status_hover, "status 11, so a hit ends it (as for Skywalk)");
    }

    /// Skywalk (51: s43 = 3, s44 = 5, s45 = 5, s46 = 450) is the same body with half the height: 50 u,
    /// and the float lasts 15 s.
    #[test]
    fn skywalk_rises_to_fifty_units_and_floats_fifteen_seconds() {
        let skywalk = def(51, Capsule::Status, 3, &[(f::FAMILY, 100), (43, 3), (44, move_kind::LEVITATE), (45, 5), (46, 450)]);
        let (m, log) = track(skywalk, Some(instant_row()), 700);
        let ys: Vec<f32> = log.iter().map(|l| l.0).collect();
        let apex = ys.iter().cloned().fold(0.0f32, f32::max);
        assert!((apex - 50.0).abs() < 3.0, "apex 10 x s45 = 50 u: {apex}");
        let left = m.combat.fighters[0].status.hover_ticks as i32;
        assert!((left - (900 - 699)).abs() <= 3, "900 sim ticks = 450 table ticks = 15 s from the cast, {left} left after 700");
        assert!((ys[699] - apex).abs() < 0.01, "still floating at tick 700");
        assert!(!m.players[0].grounded);
    }

    #[test]
    fn a_hit_ends_the_float_and_the_body_falls() {
        let levitate = def(56, Capsule::Special, 2, &[(43, 3), (44, move_kind::LEVITATE), (45, 10), (46, 90)]);
        let (mut m, log) = track(levitate, Some(instant_row()), 150);
        let y = log[149].0;
        assert!(y > 90.0, "floating near the top at tick 150: {y}");
        // What reaches the body clears status 11 (`FUN_14026f870` tail: `Combat::strike` does it).
        m.combat.fighters[0].status.hover_ticks = 0;
        let mut col = crate::combat::testkit::floor();
        for _ in 0..8 {
            m.step([crate::sim::Input::default(), crate::sim::Input::default()], &mut col);
        }
        assert!(m.players[0].pos.y < y - 1.0, "falling: {} from {y}", m.players[0].pos.y);
    }

    #[test]
    fn jump_gives_you_upward_velocity() {
        let (mut c, mut p, col) = setup(vec![def(239, Capsule::Special, 1, &[(43, 3), (44, move_kind::JUMP)])], 100.0);
        use_skill(&mut c, &mut p, &col, 0, 239);
        // The Jump skill starts with its JUMP_START clip (state 4, `0x140257b90`):
        // the take-off waits for the clip's length less two program ticks
        // (15 - 2 = 13 program ticks = 26 sim ticks for the 15-frame clip).
        assert_eq!(p[0].jump_windup(), 26, "13 program ticks (the body isn't stepped in this harness)");
        assert_eq!(p[0].body_state(), crate::sim::BodyState::Busy, "no cast, no capture while winding up");
    }

    #[test]
    fn jump_takes_off_after_its_wind_up_to_an_apex_of_100_units() {
        use crate::sim::{Input, Match};
        let mut col = crate::combat::testkit::floor();
        let mut combat = crate::combat::Combat::new(vec![def(239, Capsule::Special, 1, &[(43, 3), (44, move_kind::JUMP), (45, 10)])], vec![crate::combat::AURA_PARTICLE; 30]);
        combat.set_rules(crate::combat::testkit::TEST_RULES);
        combat.round_ticks = crate::combat::COUNTDOWN_TICKS;
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat);
        m.combat.fighters[0].level = 8;
        m.combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        m.combat.fighters[0].slots[0] = Some(239);
        m.step([Input { skills: 1, ..Default::default() }, Input::default()], &mut col);
        let (mut apex, mut left_at, mut windup_seen) = (0.0f32, None, 0u16);
        for t in 0..400 {
            m.step([Input::default(), Input::default()], &mut col);
            windup_seen = windup_seen.max(m.players[0].jump_windup());
            if left_at.is_none() && !m.players[0].grounded {
                left_at = Some(t);
            }
            apex = apex.max(m.players[0].pos.y);
        }
        assert!(windup_seen >= 20, "a wind-up ran: {windup_seen}");
        assert!(left_at.unwrap() >= 20, "rooted in the start clip first: left the ground at tick {left_at:?}");
        // apex = 10 x h = 100 u (h = s45 = 10), `FUN_140257b90` v = sqrt(20 g h).
        assert!((apex - 100.0).abs() < 4.0, "apex {apex}");
    }

    /// `FUN_14025a0b0` (state 4 -> 5, the take-off) calls `FUN_1402549e0`, which plays common/61 on
    /// materials 2 / 3 / 5 (`0x1402549e0` switch; LIVE: the Jump, Levitate, Skywalk and Demon's Wing
    /// takes log `sound:0.61` the tick the state becomes 5, 14 program ticks after the press): one
    /// `JumpTakeoff` event, at the tick the body leaves the ground, never at the press.
    #[test]
    fn jump_logs_one_takeoff_event_when_the_body_leaves_the_ground() {
        use crate::combat::CombatEvent;
        use crate::sim::{Input, Match};
        let mut col = crate::combat::testkit::floor();
        let mut combat = crate::combat::Combat::new(vec![def(239, Capsule::Special, 1, &[(43, 3), (44, move_kind::JUMP), (45, 10)])], vec![crate::combat::AURA_PARTICLE; 30]);
        combat.set_rules(crate::combat::testkit::TEST_RULES);
        combat.round_ticks = crate::combat::COUNTDOWN_TICKS;
        let mut m = Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat);
        m.combat.fighters[0].level = 8;
        m.combat.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
        m.combat.fighters[0].slots[0] = Some(239);
        m.step([Input { skills: 1, ..Default::default() }, Input::default()], &mut col);
        let takeoffs = |m: &Match| m.combat.events.iter().filter(|(_, e)| matches!(e, CombatEvent::JumpTakeoff { who: 0 })).count();
        assert_eq!(takeoffs(&m), 0, "not at the press");
        let mut left_at = None;
        for t in 0..60 {
            m.step([Input::default(), Input::default()], &mut col);
            if left_at.is_none() && !m.players[0].grounded {
                left_at = Some(t);
                assert_eq!(takeoffs(&m), 1, "logged on the tick the body leaves the ground");
            }
        }
        assert!(left_at.is_some());
        assert_eq!(takeoffs(&m), 1, "once");
    }

    /// `FUN_140270b00` case 0x13 (Relearn) and case 0x32 (Annoying Gift) both end in `FUN_140261a00` with
    /// its sound flag: common/86 per captured skill (LIVE: Relearn one `capture:` + 0.86, Annoying Gift
    /// three `capture:` + three 0.86 at one tick).
    #[test]
    fn relearn_and_annoying_gift_log_a_capture_per_skill_they_place() {
        use crate::combat::CombatEvent;
        let captures = |c: &Combat| c.events.iter().filter_map(|(_, e)| if let CombatEvent::SkillCaptured { who, slot, skill } = e { Some((*who, *slot, *skill)) } else { None }).collect::<Vec<_>>();
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.fighters[0].slots = [Some(295), Some(2), None, Some(4)];
        c.fighters[0].last_slot = 0;
        c.fighters[1].slots = [Some(9); 4];
        let gift = def(295, Capsule::Special, 3, &[(51, add::ANNOYING_GIFT)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &gift));
        assert_eq!(captures(&c), vec![(1, 1, 2), (1, 3, 4)], "one capture per placed skill, on the target");
        let relearn = def(id::RELEARN, Capsule::Special, 3, &[(43, 4), (45, 19), (26, 1)]);
        let (mut c, mut p, _col) = setup(vec![relearn.clone()], 100.0);
        c.fighters[0].slots[0] = Some(id::RELEARN);
        c.fighters[0].last_slot = 0;
        c.fighters[0].used.push(7);
        assert!(apply(&mut c, &mut p, 0, Reach::Caster, &relearn));
        assert_eq!(captures(&c), vec![(0, 0, 7)]);
    }

    /// `FUN_140272bc0` ends in `FUN_1402738d0(victim, s51, s52)` (the Add effect's sound): LIVE Interference
    /// (s51 2, s52 2) 0.90, Level Berserk (4, -2) 0.89, Boost Mine (0x2c, 2) 0.89 on the caster-side targets;
    /// the Level codes 2 / 4 return before it when the victim is at Level 0.
    #[test]
    fn an_add_effect_logs_one_add_event_per_victim() {
        use crate::combat::CombatEvent;
        let adds = |c: &Combat| c.events.iter().filter_map(|(_, e)| if let CombatEvent::AddEffect { who, code, amount } = e { Some((*who, *code, *amount)) } else { None }).collect::<Vec<_>>();
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.fighters[1].level = 5;
        let interference = def(id::INTERFERENCE, Capsule::Special, 0, &[(51, add::LEVEL_DRAIN), (52, 2)]);
        assert!(crate::moves::apply_effect(&mut c, &mut p, 0, Reach::Target(1), &interference));
        assert_eq!(adds(&c), vec![(1, 2, 2)]);
        let berserk = def(id::LEVEL_BERSERK, Capsule::Special, 2, &[(51, add::LEVEL_BERSERK_DOUBLE), (52, -2)]);
        assert!(crate::moves::apply_effect(&mut c, &mut p, 0, Reach::Target(1), &berserk));
        assert_eq!(adds(&c), vec![(1, 2, 2), (1, 4, -2)]);
        c.fighters[1].level = 0;
        assert!(crate::moves::apply_effect(&mut c, &mut p, 0, Reach::Target(1), &interference));
        assert_eq!(adds(&c).len(), 2, "Level 0: nothing to reduce, silent");
        let amp = def(308, Capsule::Special, 3, &[(51, add::LEVEL), (52, 2)]);
        assert!(crate::moves::apply_effect(&mut c, &mut p, 0, Reach::Everyone, &amp));
        assert_eq!(&adds(&c)[2..], &[(0, 0x2c, 2), (1, 0x2c, 2)], "Quantum Amp: both players (LIVE: two 0.89)");
    }

    /// Dash (240: s43 = 3, s44 = 8, s45 = 10; row 294: SPE_DASH, w42 = 15, w43 = 0): `FUN_140270200` case 8
    /// moves `s45 x 10 / (w42 - w43)` = 6.67 u a program tick for 15 program ticks (`FUN_14025afe0`,
    /// `FUN_14024f9c0`) = 100 u in all -- here 3.33 u a sim tick for 30 sim ticks, not an instant jump.
    #[test]
    fn dash_covers_a_hundred_units_over_fifteen_program_ticks() {
        let dash = def(240, Capsule::Special, 2, &[(43, 3), (44, move_kind::DASH), (45, 10)]);
        let row = crate::combat::ActionRow { anim_name: "SPE_DASH".into(), cast_sound: 494, charge_cycle: 0, dash_start: 0, dash_end: 15, fire_tick: 0, hit_window_end: 0 };
        let (m, log) = track(dash, Some(row), 80);
        let d: Vec<f32> = log.iter().map(|l| l.1.distance(Vec3::ZERO)).collect();
        let total = d[79];
        assert!((total - 100.0).abs() < 1.5, "100 u in all: {total}");
        let moving: Vec<usize> = (1..80).filter(|&t| d[t] - d[t - 1] > 0.01).collect();
        assert_eq!(moving.len(), 30, "15 program ticks = 30 sim ticks of travel: {moving:?}");
        assert!(moving.windows(2).all(|w| w[1] == w[0] + 1), "one unbroken run");
        for &t in &moving {
            assert!(((d[t] - d[t - 1]) - 10.0 * 10.0 / 15.0 / 2.0).abs() < 0.01, "6.67 / 2 = 3.33 u a sim tick at {t}");
        }
        // It runs along the facing (the caster faces the +z opponent), and the step cap is 10 a program tick.
        assert!(m.players[0].pos.z > 99.0 && m.players[0].pos.x.abs() < 0.5);
        let fast = def(240, Capsule::Special, 2, &[(43, 3), (44, move_kind::DASH), (45, 40)]);
        let row = crate::combat::ActionRow { anim_name: "SPE_DASH".into(), cast_sound: 494, charge_cycle: 0, dash_start: 0, dash_end: 15, fire_tick: 0, hit_window_end: 0 };
        let (_, log) = track(fast, Some(row), 80);
        let z = log[79].1.z;
        assert!((z - 150.0).abs() < 2.0, "s45 40 would be 26.7 u a program tick; capped at 10 (`0x1402f7f7c`) x 15 ticks = 150 u: {z}");
    }

    /// No action row (or a window that is not positive): the program stores speed 0 and moves nothing.
    #[test]
    fn dash_without_a_window_goes_nowhere() {
        let dash = def(240, Capsule::Special, 2, &[(43, 3), (44, move_kind::DASH), (45, 10)]);
        let (m, _) = track(dash, None, 40);
        assert!(m.players[0].pos.distance(Vec3::ZERO) < 0.01);
    }

    // ---- Targeted (Reach::Target / Reach::Everyone) ------------------------

    #[test]
    fn blessed_water_and_rain_add_health() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.fighters[0].health = 10;
        c.fighters[1].health = 10;
        let water = def(177, Capsule::Special, 3, &[(51, add::HEALTH), (52, 5)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &water));
        assert_eq!(c.fighters[1].health, 15, "target only");
        let rain = def(331, Capsule::Special, 3, &[(51, add::HEALTH), (52, 5)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Everyone, &rain));
        assert_eq!((c.fighters[0].health, c.fighters[1].health), (15, 20), "everyone (capped at START_HEALTH)");
    }

    #[test]
    fn level_skills_go_to_the_right_recipient() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        let baton = def(328, Capsule::Special, 0, &[(51, add::LEVEL), (52, 2)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &baton));
        assert_eq!(c.fighters[1].level, 3, "Level Baton hits the target");
        let boost_mine = def(id::BOOST_MINE, Capsule::Special, 3, &[(51, add::LEVEL), (52, 2)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &boost_mine));
        assert_eq!((c.fighters[0].level, c.fighters[1].level), (1, 5), "Boost Mine benefits whoever tripped it (LIVE take 307), here target 1");
        let amp = def(308, Capsule::Special, 3, &[(51, add::LEVEL), (52, 2)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Everyone, &amp));
        assert_eq!((c.fighters[0].level, c.fighters[1].level), (3, 7), "Quantum Amp hits everyone");
    }

    #[test]
    fn teleportation_and_mobilize_send_players_to_the_casters_spawn() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        let caster_spawn = p[0].spawn;
        let teleport = def(53, Capsule::Special, 3, &[(51, add::TELEPORT_TO_CASTER_SPAWN)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &teleport));
        assert_eq!(p[1].pos, caster_spawn);
        p[0].pos = Vec3::new(5.0, 0.0, 5.0); // caster has since walked away from their spawn
        p[1].pos = Vec3::new(-5.0, 0.0, -5.0);
        let mobilize = def(345, Capsule::Special, 6, &[(51, add::TELEPORT_TO_CASTER_SPAWN)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Everyone, &mobilize));
        // Both go to the caster's *spawn* point (fixed), not their current position.
        assert_eq!((p[0].pos, p[1].pos), (caster_spawn, caster_spawn));
    }

    #[test]
    fn fusion_sets_the_targets_health_to_the_average_and_leaves_the_casters_alone() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.fighters[0].health = 20;
        c.fighters[1].health = 6;
        let fusion = def(294, Capsule::Special, 3, &[(51, add::FUSION)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &fusion));
        // LIVE take 294 (P0 20, P1 40 -> P1 30, P0 still 20 at the end); program `0x140272f48` case 0x2e.
        assert_eq!((c.fighters[0].health, c.fighters[1].health), (20, 13));
    }

    #[test]
    fn dazzle_scrambles_buttons_for_ten_seconds() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        let dazzle = def(54, Capsule::Special, 4, &[(51, add::DAZZLE), (52, 300)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &dazzle));
        assert_eq!(c.fighters[1].status.scrambled_ticks, table_ticks(300), "300 table ticks = 10 sec");
    }

    #[test]
    fn trigger_forces_the_targets_skill_on_the_same_button() {
        let (mut c, mut p, col) = setup(vec![def(7, Capsule::Attack, 0, &[(f::MOTION, 0), (P + 1, 999), (26, 1)])], 100.0);
        c.fighters[1].level = 8;
        c.fighters[1].aura_ticks = 8 * crate::combat::AURA_TICKS_PER_POINT;
        c.fighters[1].slots[2] = Some(7);
        c.fighters[0].last_slot = 2; // the caster used button 2: the same button fires
        let trigger = def(236, Capsule::Special, 4, &[(51, add::TRIGGER)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &trigger));
        assert_eq!(c.fighters[1].cast_skill, 7, "made to cast it");
        run(&mut c, &mut p, &col, CAST_TICKS as usize + 2);
        assert_eq!(c.fighters[1].slots[2], None, "used it up (paid at the fire tick), same as pressing it themselves");
    }

    #[test]
    fn annoying_gift_mirrors_the_casters_other_slots_onto_the_target() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        // The gift (295) was pressed from button 0 (`last_slot`): `FUN_140272bc0` case 0x32 skips that button.
        c.fighters[0].slots = [Some(295), Some(2), None, Some(4)];
        c.fighters[0].last_slot = 0;
        c.fighters[1].slots = [Some(9), Some(9), Some(9), Some(9)];
        let gift = def(295, Capsule::Special, 3, &[(51, add::ANNOYING_GIFT)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &gift));
        assert_eq!(c.fighters[1].slots, [Some(9), Some(2), Some(9), Some(4)], "not the gift itself, and the target keeps what the caster had no skill for");
        // Pressed from button 3 instead: button 3 is the one skipped.
        c.fighters[0].slots = [Some(1), Some(2), None, Some(295)];
        c.fighters[0].last_slot = 3;
        c.fighters[1].slots = [Some(9), Some(9), Some(9), Some(9)];
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &gift));
        assert_eq!(c.fighters[1].slots, [Some(1), Some(2), Some(9), Some(9)]);
    }

    #[test]
    fn angels_breath_and_armaros_reverse_attributes() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.fighters[1].status.add(Stat::Speed, 2, None);
        let breath = def(293, Capsule::Special, 1, &[(51, add::REVERSE_ATTRIBUTES)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &breath));
        assert_eq!(c.stat(1, Stat::Speed), -2);
        c.fighters[0].status.add(Stat::Speed, 4, None);
        let armaros = def(296, Capsule::Special, 3, &[(51, add::REVERSE_ATTRIBUTES)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Everyone, &armaros));
        assert_eq!((c.stat(0, Stat::Speed), c.stat(1, Stat::Speed)), (-4, 2), "everyone's active mods flip again");
    }

    #[test]
    fn level_berserk_doubles_not_adds_two() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.fighters[1].level = 5;
        let berserk = def(id::LEVEL_BERSERK, Capsule::Special, 2, &[(51, add::LEVEL_BERSERK_DOUBLE), (52, -2)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &berserk));
        assert_eq!(c.fighters[1].level, 10);
    }

    #[test]
    fn paralyze_barrier_and_interference_enemy_copies() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        c.fighters[1].level = 5;
        let paralyze = def(id::PARALYZE_BARRIER, Capsule::Special, 0, &[(51, add::PARALYZE), (52, 90)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &paralyze));
        assert_eq!(c.fighters[1].status.bound_ticks, table_ticks(90), "90 table ticks = 3 sec, confirmed by the note");
        let interference = def(id::INTERFERENCE, Capsule::Special, 0, &[(51, add::LEVEL_DRAIN), (52, 2)]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &interference));
        assert_eq!(c.fighters[1].level, 3);
    }

    #[test]
    fn frighten_and_sword_slap_briefly_bind_the_target() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        let frighten = def(id::FRIGHTEN, Capsule::Special, 4, &[]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &frighten));
        assert_eq!(c.fighters[1].status.bound_ticks, BRIEF_BIND_TICKS);
        let sword_slap = def(id::SWORD_SLAP, Capsule::Special, 3, &[]);
        assert!(apply(&mut c, &mut p, 0, Reach::Target(0), &sword_slap));
        assert_eq!(c.fighters[0].status.bound_ticks, BRIEF_BIND_TICKS);
    }

    #[test]
    fn vacuum_bullet_landing_is_a_no_op_pierce_is_generic() {
        let (mut c, mut p, _col) = setup(vec![], 100.0);
        let bullet = def(id::VACUUM_BULLET, Capsule::Special, 2, &[(54, 2)]);
        assert!(bullet.ignores_defence());
        assert!(apply(&mut c, &mut p, 0, Reach::Target(1), &bullet), "landing itself does nothing extra");
    }
}
