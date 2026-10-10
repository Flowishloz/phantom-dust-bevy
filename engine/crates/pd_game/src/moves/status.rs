//! Status skills (green capsules). Owned by the status lane: only this
//! file changes. Contract: docs/MOVES.md "Capsule colours" and moves/effects.rs.
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
//! The guide's own summary of this capsule: "The four basic qualities that
//! can be changed are strength of attack, speed of movement, homing, and
//! aura recovery speed" (`Stat` in effects.rs). Two shapes of record, told
//! apart by `reach` (delivery is already decided in effects.rs before we're
//! called):
//! - **Targeted** (it landed on someone, or on everyone): the effect and its
//!   amount/time are s51/s52 (`field::ADD_EFFECT` / `ADD_AMOUNT`, the codes
//!   in `code` below, checked against every registered skill's own note).
//! - **Self** (`Reach::Caster`): family (s42) 70 = permanent (until reset),
//!   71 = timed for s53 table ticks; which stat(s) change and by how much is
//!   s45 (selector) and s49 (+ s50 for a second stat) — see `apply_self`.
//!   Three self skills don't fit that shape and are keyed by id: Skywalk /
//!   Demon's Wing (hover) and Doppelganger (copy the other fighter's mods).

use super::effects::{Reach, Stat};
use super::table_ticks;
use crate::combat::{Combat, Fighter, SkillDef};
use crate::sim::Player;
use pd_formats::skills::field;

/// "Add:" codes seen on status skills (s51), amount/time in s52. Checked
/// against the note of every registered skill carrying that code.
mod code {
    /// "Prevents target from moving [or using skills]" -> `bound_ticks`
    /// (+ `frozen_ticks` on the three that also block skills, + `hover_*` on
    /// the two that levitate the target). Paralyze, Pressure, Entangle, Pin
    /// Down, Lift Up, Antigravity Trap, Still Moment (`Everyone`), Binding
    /// Circle.
    pub const BIND: i16 = 6;
    /// "Freezes ... skill button(s) for N sec" -> `frozen_ticks`. Freeze Gas,
    /// Chill Breeze (`Everyone`).
    pub const FREEZE: i16 = 7;
    /// Attack +/- STR points on the target. Friendship (+1), Doze / Curse of
    /// Earth (-1).
    pub const ATTACK: i16 = 12;
    /// Movement speed +/- percent on the target. Alleviate (+30), Burden
    /// (-20), Gravity Mine (-90, timed by s53 = "5 seconds").
    pub const SPEED: i16 = 13;
    /// Homing accuracy +/- percent. Magnetic Field (-30).
    pub const HOMING: i16 = 14;
    /// Aura recovery rate +/- percent. Ethos Inspiration (+20).
    pub const AURA_RATE: i16 = 15;
    /// Frostbite / Freeze Shot: "prevents target from using skill assigned
    /// to same button" (the button the caster used).
    pub const SAME_BUTTON: i16 = 16;
    /// Inverted directional controls for s52 ticks. Brain Concussion, Vast
    /// Mayhem (`Everyone`).
    pub const INVERT: i16 = 37;
    /// Decoy: "Target can only lock onto you" — redirects a teammate's
    /// lock-on, meaningless 1-v-1.
    pub const DECOY: i16 = 40;
    /// "... button presses cause 1 damage" -> `poison_ticks` (same meaning as
    /// the attack table's code 8, `moves::attack_add::POISON`). Poison Blur,
    /// Withering Curse, Venom Drip (`Everyone`).
    pub const POISON: i16 = 8;
}

/// Self skills (`Reach::Caster`, family 70/71): which stat(s) s49 (+ s50)
/// change, picked by s45. unconfirmed: no single note names this field; the
/// mapping is consistent across every registered self-skill (Heat, Stimulate
/// = 4; Speed Up, Lightning Speed = 5; Acquisition = 6; Meditation = 7;
/// Optical Camo = 12; Void = 14).
mod self_stat {
    pub const ATTACK: i16 = 4;
    pub const SPEED: i16 = 5;
    pub const HOMING: i16 = 6;
    pub const AURA_RATE: i16 = 7;
    pub const INVISIBLE: i16 = 12;
    pub const IMMUNE: i16 = 14;
}

/// Skywalk: s43 = 3, s44 = 5 (a movement special that the capsule colour files under Status).
const SKYWALK: i16 = 51;

/// Family (s42) on self skills: 70 permanent, 71 timed (s53 table ticks).
const FAMILY_PERMANENT: i16 = 70;
const FAMILY_TIMED: i16 = 71;
/// Doppelganger: "Sets your attributes to match target's."
const FAMILY_COPY_STATS: i16 = 76;

/// 1 ft in world units (docs/MOVES.md, engine/AGENTS.md: 1 character ~= 21 u
/// ~= 1.75 m).
const FOOT: f32 = 3.66;

/// s53 as sim ticks, or `None` ("until reset") when it's 0 (every permanent
/// self-buff and every un-timed targeted debuff carries s53 = 0).
fn timed(def: &SkillDef) -> Option<u16> {
    let t = def.raw[53];
    (t > 0).then(|| table_ticks(t))
}

pub fn apply(c: &mut Combat, players: &mut [Player], caster: usize, reach: Reach, def: &SkillDef) -> bool {
    let button = c.fighters[caster].last_slot as u8;
    match reach {
        // Skywalk's record is a movement special (s43 = 3, s44 = 5), the same body as Levitate:
        // float status 11 for s46 ticks and the jump of height s45 that hangs at its top
        // (`FUN_140270200` case 5; `special::float_and_jump`).
        Reach::Caster if def.id == SKYWALK => super::special::float_and_jump(c, players, caster, def),
        Reach::Caster => {
            let target = c.aim_target(caster, players);
            apply_self(c, caster, target, def)
        }
        // Decoy (add code 40): the target can lock onto the caster only, for s52
        // program ticks (status 9 = caster + 1, `FUN_140272bc0` case 0x28,
        // read by the lock-on validity test `0x1402543e0`).
        Reach::Target(t) if def.raw[field::ADD_EFFECT] == code::DECOY => {
            let ticks = table_ticks(def.raw[field::ADD_AMOUNT]);
            c.fighters[t].forced_lock = (ticks > 0).then_some((caster, ticks));
            true
        }
        Reach::Target(t) => apply_targeted(&mut c.fighters[t], def, button),
        Reach::Everyone => {
            let mut any = false;
            for i in c.roster.slots().collect::<Vec<_>>() {
                any |= apply_targeted(&mut c.fighters[i], def, button);
            }
            any
        }
        // No status skill in the data targets the field (s43 = 1).
        Reach::Field => false,
    }
}

/// A targeted or `Everyone` skill landing on `f` (s51/s52, `code` above).
/// `button`: the skill button (slot) the caster used ("same button" skills).
fn apply_targeted(f: &mut Fighter, def: &SkillDef, button: u8) -> bool {
    let code = def.raw[field::ADD_EFFECT];
    let amount = def.raw[field::ADD_AMOUNT] as i32;
    // For bind/freeze/invert/poison the amount itself *is* the time (matches
    // every note: 90 = "3 sec", 300 = "10 sec", 450 = "15 sec").
    let as_ticks = table_ticks(def.raw[field::ADD_AMOUNT]);
    match code {
        code::BIND => {
            f.status.bound_ticks = as_ticks;
            if binds_skills_too(def) {
                f.frozen_ticks = as_ticks;
            }
            if let Some(h) = hover_amount(def) {
                f.status.hover_ticks = as_ticks;
                f.status.hover_height = h;
            }
            true
        }
        code::FREEZE => {
            f.frozen_ticks = as_ticks;
            true
        }
        code::ATTACK => {
            f.status.add(Stat::Attack, amount, timed(def));
            true
        }
        code::SPEED => {
            f.status.add(Stat::Speed, amount, timed(def));
            true
        }
        code::HOMING => {
            f.status.add(Stat::Homing, amount, timed(def));
            true
        }
        code::AURA_RATE => {
            f.status.add(Stat::AuraRate, amount, timed(def));
            true
        }
        code::INVERT => {
            f.status.inverted_ticks = as_ticks;
            true
        }
        code::POISON => {
            f.poison_ticks = as_ticks;
            true
        }
        // Decoy is handled in `apply` (it needs the caster's index).
        code::DECOY => true,
        // Frostbite / Freeze Shot: lock the target's button matching the one
        // the caster used (`Fighter::last_slot`). Time: s52 (Freeze Shot 300 =
        // 10 s). Frostbite has 0 there, which the program reads as a permanent
        // value (`FUN_14024d5c0` with duration 0 adds to the permanent slot):
        // the button stays locked until the status is reset.
        code::SAME_BUTTON => {
            let ticks = if def.raw[52] > 0 { table_ticks(def.raw[52]) } else { u16::MAX };
            (f.status.locked_slot, f.status.locked_ticks) = (button, ticks);
            true
        }
        _ => false,
    }
}

/// Pressure / Entangle / Pin Down / Lift Up: "Prevents target from moving
/// *or using skills*" (Paralyze, Antigravity Trap and the undocumented
/// Binding Circle only stop movement).
fn binds_skills_too(def: &SkillDef) -> bool {
    matches!(def.id, 42 | 173 | 230 | 43)
}

/// Lift Up / Antigravity Trap: "Levitates target 10 ft in air" -- but NOT as a held height. LIVE (`043_Lift_Up` P1, `303_Antigravity_Trap`
/// P0): the body rises 3.35 u on the first tick and then slows to a stop at +48.3 u over about 28 program ticks (a launch under
/// the program's own gravity: the reaction row's `sqrt(0.12 x 20 x w45)` = apex 10 x 5 = 50 u, `0x140258cbc`), hangs there until
/// w42 + w43 = 75 program ticks have run, then falls in state 17. That is `combat/reactions.rs` kind 9 (row 21) alone; a second,
/// pinned hover of 10 ft under it (what this used to add) stacked the two, to 85.7 u. So no held hover from the status.
fn hover_amount(_def: &SkillDef) -> Option<f32> {
    None
}

/// A self skill (`Reach::Caster`).
fn apply_self(c: &mut Combat, caster: usize, target: usize, def: &SkillDef) -> bool {
    match def.family {
        FAMILY_COPY_STATS => {
            let mods = c.fighters[target].status.mods.clone();
            c.fighters[caster].status.mods = mods;
            true
        }
        // Skywalk / Demon's Wing (hover) and Mind Reading share family 100
        // with the travelling Binding Circle / Bind Shot; s45 doesn't tell
        // them apart here, so they're kept by id (MOVES.md Phase 0: "fall
        // back to skill id ... and say so in a comment").
        100 => hover_or_voice(&mut c.fighters[caster], def),
        FAMILY_PERMANENT | FAMILY_TIMED => apply_self_stat(&mut c.fighters[caster], def),
        // Place of Invisibility (353) and Martial Arts Scroll (365) are
        // unregistered (boss-only) skills whose FAMILY field reads 0 (a data
        // quirk shared by every unregistered record here), but their s45
        // code and amounts match their own note text exactly, so they're
        // let through by id.
        _ if matches!(def.id, 353 | 365) => apply_self_stat(&mut c.fighters[caster], def),
        _ => false,
    }
}

/// s45 selects the stat, s49 (+ s50 for a second stat) the amount(s).
fn apply_self_stat(f: &mut Fighter, def: &SkillDef) -> bool {
    let ticks = timed(def);
    let amt1 = def.raw[49] as i32;
    let amt2 = def.raw[50] as i32;
    match def.raw[field::STRENGTH_RULE] {
        self_stat::ATTACK => {
            // Heat, Stimulate, Tiger's Strength, Trance: "+N damage[, -M%
            // Aura recovery]".
            f.status.add(Stat::Attack, amt1, ticks);
            if amt2 != 0 {
                f.status.add(Stat::AuraRate, amt2, ticks);
            }
            true
        }
        self_stat::SPEED => {
            f.status.add(Stat::Speed, amt1, ticks);
            true
        }
        self_stat::HOMING => {
            f.status.add(Stat::Homing, amt1, ticks);
            true
        }
        self_stat::AURA_RATE => {
            // Meditation: aura rate only. Martial Arts Scroll (unregistered)
            // pairs it with +Attack in s50, exactly matching its note ("+3
            // damage. Aura recovery speed +90%") even though the two amounts
            // sit in the opposite slots to the ATTACK case above —
            // unconfirmed: read off this one skill's own numbers, no second
            // example to check the field order against.
            f.status.add(Stat::AuraRate, amt1, ticks);
            if amt2 != 0 {
                f.status.add(Stat::Attack, amt2, ticks);
            }
            true
        }
        self_stat::INVISIBLE => {
            f.status.invisible_ticks = ticks.unwrap_or(u16::MAX);
            true
        }
        self_stat::IMMUNE => {
            f.status.immune_ticks = ticks.unwrap_or(u16::MAX);
            true
        }
        _ => false,
    }
}

/// Skywalk ("Levitate 15 ft ... for 15 sec"), Demon's Wing ("Levitate 7 ft
/// ... Add: Invisibility ... for 15 sec" — both effects together, kept as
/// one hover+invisible skill since the table has no separate slot for the
/// Add:), Mind Reading (a 2-v-2 voice feature: nothing for the sim to do).
fn hover_or_voice(f: &mut Fighter, def: &SkillDef) -> bool {
    match def.id {
        // Skywalk (51) is handled before this in `apply` (it is a movement special).
        292 => {
            let ticks = table_ticks(def.raw[53]);
            f.status.hover_ticks = ticks;
            f.status.hover_height = 7.0 * FOOT;
            f.status.invisible_ticks = ticks;
            f.status_hover = true;
            true
        }
        231 => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::testkit::*;

    // ---- Paralyze / Pressure / Entangle / Pin Down / Lift Up (bind) --------

    #[test]
    fn paralyze_binds_but_leaves_skills_free() {
        // "Prevents target from moving for 3 sec." (no "or using skills").
        let paralyze = def(112, Capsule::Status, 4, &[(f::FAMILY, 13), (f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 90)]);
        let (mut c, mut players, _col) = setup(vec![paralyze.clone()], 50.0);
        assert!(apply(&mut c, &mut players, 0, Reach::Target(1), &paralyze));
        assert_eq!(c.fighters[1].status.bound_ticks, 180, "90 table ticks = 3 sec = 180 sim ticks");
        assert_eq!(c.fighters[1].frozen_ticks, 0);
    }

    #[test]
    fn pressure_entangle_pin_down_also_freeze_skill_buttons() {
        for (id, family) in [(42, 13), (173, 18), (230, 22)] {
            let skill = def(id, Capsule::Status, 4, &[(f::FAMILY, family), (f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 90)]);
            let (mut c, mut players, _col) = setup(vec![skill.clone()], 50.0);
            apply(&mut c, &mut players, 0, Reach::Target(1), &skill);
            assert_eq!(c.fighters[1].status.bound_ticks, 180, "id {id}");
            assert_eq!(c.fighters[1].frozen_ticks, 180, "id {id}: \"or using skills\"");
        }
    }

    #[test]
    fn lift_up_binds_freezes_and_hovers_the_target() {
        let lift_up = def(43, Capsule::Status, 2, &[(f::FAMILY, 13), (f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 90)]);
        let (mut c, mut players, _col) = setup(vec![lift_up.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Target(1), &lift_up);
        let victim = &c.fighters[1];
        assert_eq!(victim.status.bound_ticks, 180);
        assert_eq!(victim.frozen_ticks, 180);
        assert_eq!(victim.status.hover_ticks, 0, "the lift is the reaction row's launch (kind 9), not a held hover (LIVE 043_Lift_Up)");
    }

    #[test]
    fn antigravity_trap_hovers_without_freezing_skills() {
        let trap = def(303, Capsule::Status, 5, &[(f::FAMILY, 50), (f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 90)]);
        let (mut c, mut players, _col) = setup(vec![trap.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Target(1), &trap);
        let victim = &c.fighters[1];
        assert_eq!(victim.status.bound_ticks, 180);
        assert_eq!(victim.frozen_ticks, 0, "note doesn't say \"or using skills\"");
        assert_eq!(victim.status.hover_ticks, 0, "the lift is the reaction row's launch (kind 9), not a held hover (LIVE 303_Antigravity_Trap)");
    }

    #[test]
    fn binding_circle_binds_with_no_note_to_go_on() {
        let skill = def(710, Capsule::Status, 0, &[(f::FAMILY, 100), (f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 90)]);
        let (mut c, mut players, _col) = setup(vec![skill.clone()], 50.0);
        assert!(apply(&mut c, &mut players, 0, Reach::Target(1), &skill));
        assert_eq!(c.fighters[1].status.bound_ticks, 180);
        assert_eq!(c.fighters[1].frozen_ticks, 0);
    }

    // ---- Freeze buttons -----------------------------------------------------

    #[test]
    fn freeze_gas_freezes_only_the_one_it_hits() {
        let gas = def(48, Capsule::Status, 4, &[(f::FAMILY, 50), (f::ADD_EFFECT, 7), (f::ADD_AMOUNT, 300)]);
        let (mut c, mut players, _col) = setup(vec![gas.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Target(1), &gas);
        assert_eq!(c.fighters[1].frozen_ticks, 600, "300 table ticks = 10 sec");
        assert_eq!(c.fighters[0].frozen_ticks, 0);
    }

    #[test]
    fn chill_breeze_freezes_everyone_including_the_caster() {
        let breeze = def(330, Capsule::Status, 4, &[(f::FAMILY, 79), (f::ADD_EFFECT, 7), (f::ADD_AMOUNT, 300)]);
        let (mut c, mut players, _col) = setup(vec![breeze.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Everyone, &breeze);
        assert_eq!(c.fighters[0].frozen_ticks, 600);
        assert_eq!(c.fighters[1].frozen_ticks, 600);
    }

    // ---- Target's attack / speed / homing / aura rate ------------------------

    #[test]
    fn friendship_and_doze_change_the_targets_attack() {
        let friendship = def(171, Capsule::Status, 2, &[(f::FAMILY, 49), (f::ADD_EFFECT, 12), (f::ADD_AMOUNT, 1)]);
        let doze = def(113, Capsule::Status, 5, &[(f::FAMILY, 13), (f::ADD_EFFECT, 12), (f::ADD_AMOUNT, -1)]);
        let (mut c, mut players, _col) = setup(vec![friendship.clone(), doze.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Target(1), &friendship);
        assert_eq!(c.stat(1, Stat::Attack), 1);
        apply(&mut c, &mut players, 0, Reach::Target(1), &doze);
        assert_eq!(c.stat(1, Stat::Attack), 0, "+1 then -1");
    }

    #[test]
    fn alleviate_burden_and_gravity_mine_change_speed() {
        let alleviate = def(44, Capsule::Status, 2, &[(f::FAMILY, 49), (f::ADD_EFFECT, 13), (f::ADD_AMOUNT, 30)]);
        let burden = def(47, Capsule::Status, 4, &[(f::FAMILY, 13), (f::ADD_EFFECT, 13), (f::ADD_AMOUNT, -20)]);
        let gravity_mine = def(114, Capsule::Status, 5, &[(f::FAMILY, 50), (f::ADD_EFFECT, 13), (f::ADD_AMOUNT, -90), (53, 150)]);
        let (mut c, mut players, _col) = setup(vec![alleviate.clone(), burden.clone(), gravity_mine.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Target(1), &alleviate);
        apply(&mut c, &mut players, 0, Reach::Target(1), &burden);
        assert_eq!(c.stat(1, Stat::Speed), 10, "+30 then -20, no expiry (s53 = 0)");
        apply(&mut c, &mut players, 0, Reach::Target(1), &gravity_mine);
        assert_eq!(c.stat(1, Stat::Speed), -80, "10 - 90");
        assert_eq!(c.fighters[1].status.mods.last().unwrap().ticks, Some(300), "s53 150 table ticks = 5 sec");
    }

    #[test]
    fn magnetic_field_and_ethos_inspiration() {
        let magnetic = def(46, Capsule::Status, 3, &[(f::FAMILY, 13), (f::ADD_EFFECT, 14), (f::ADD_AMOUNT, -30)]);
        let ethos = def(335, Capsule::Status, 2, &[(f::FAMILY, 49), (f::ADD_EFFECT, 15), (f::ADD_AMOUNT, 20)]);
        let (mut c, mut players, _col) = setup(vec![magnetic.clone(), ethos.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Target(1), &magnetic);
        apply(&mut c, &mut players, 0, Reach::Target(1), &ethos);
        assert_eq!(c.stat(1, Stat::Homing), -30);
        assert_eq!(c.stat(1, Stat::AuraRate), 20);
    }

    // ---- Invert / Decoy / Frostbite -----------------------------------------

    #[test]
    fn brain_concussion_and_vast_mayhem_invert_controls() {
        let concussion = def(45, Capsule::Status, 3, &[(f::FAMILY, 13), (f::ADD_EFFECT, 37), (f::ADD_AMOUNT, 450)]);
        let (mut c, mut players, _col) = setup(vec![concussion.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Target(1), &concussion);
        assert_eq!(c.fighters[1].status.inverted_ticks, 900, "450 table ticks = 15 sec");

        let mayhem = def(342, Capsule::Status, 4, &[(f::FAMILY, 0), (f::ADD_EFFECT, 37), (f::ADD_AMOUNT, 450)]);
        let (mut c, mut players, _col) = setup(vec![mayhem.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Everyone, &mayhem);
        assert_eq!(c.fighters[0].status.inverted_ticks, 900);
        assert_eq!(c.fighters[1].status.inverted_ticks, 900);
    }

    #[test]
    fn decoy_forces_the_targets_lock_onto_the_caster_for_s52_ticks() {
        // Add code 40 writes status 9 = caster + 1 for s52 program ticks
        // (`FUN_140272bc0` case 0x28); the lock-on validity test `0x1402543e0`
        // then accepts no other slot (`FUN_14024d4d0(.., 9) != slot + 1`).
        let decoy = def(172, Capsule::Status, 2, &[(f::FAMILY, 40), (f::ADD_EFFECT, 40), (f::ADD_AMOUNT, 450)]);
        let mut c = Combat::with_roster(vec![decoy.clone()], crate::combat::Roster::with_teams(&[0, 1, 1]), std::array::from_fn(|_| vec![crate::combat::AURA_PARTICLE; 30]));
        let mut players = [Player::new(glam::Vec3::ZERO, 0.0), Player::new(glam::Vec3::X * 50.0, 0.0), Player::new(glam::Vec3::Z * 50.0, 0.0)];
        // From player 0's view players 1 and 2 are both enemies.
        assert!(c.valid_target(0, 1) && c.valid_target(0, 2));
        assert!(apply(&mut c, &mut players, 2, Reach::Target(0), &decoy), "paid for and cast");
        assert_eq!(c.fighters[0].forced_lock, Some((2, 900)), "450 program ticks");
        assert!(!c.valid_target(0, 1), "the other enemy can no longer be locked");
        assert!(c.valid_target(0, 2), "only the decoy");
        // It runs out with its timer.
        c.fighters[0].forced_lock = Some((2, 1));
        c.round_ticks = crate::combat::COUNTDOWN_TICKS;
        c.step(&mut players, [0u8; 3], &floor());
        assert_eq!(c.fighters[0].forced_lock, None);
        assert!(c.valid_target(0, 1));
    }

    #[test]
    fn neutralization_style_resets_end_a_decoy() {
        let mut c = Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
        c.fighters[1].forced_lock = Some((0, 100));
        c.fighters[1].status_hover = true;
        crate::moves::erase::reset_status_for_tests(&mut c.fighters[1]);
        assert_eq!((c.fighters[1].forced_lock, c.fighters[1].status_hover), (None, false));
    }

    #[test]
    fn frostbite_locks_the_targets_matching_button() {
        let frostbite = def(170, Capsule::Status, 6, &[(f::FAMILY, 10), (f::ADD_EFFECT, 16), (f::ADD_AMOUNT, 0)]);
        let freeze_shot = def(344, Capsule::Status, 3, &[(f::FAMILY, 0), (f::ADD_EFFECT, 16), (f::ADD_AMOUNT, 300)]);
        let (mut c, mut players, _col) = setup(vec![frostbite.clone(), freeze_shot.clone()], 50.0);
        c.fighters[0].last_slot = 2;
        assert!(apply(&mut c, &mut players, 0, Reach::Target(1), &frostbite));
        // Frostbite's s52 is 0: a permanent value in the program (`FUN_14024d5c0`
        // with duration 0), so the button stays locked until a reset.
        assert_eq!((c.fighters[1].status.locked_slot, c.fighters[1].status.locked_ticks), (2, u16::MAX));
        assert!(apply(&mut c, &mut players, 0, Reach::Target(1), &freeze_shot));
        assert_eq!(c.fighters[1].status.locked_ticks, 600, "Freeze Shot: 300 program ticks = 10 s");
    }

    // ---- Poison --------------------------------------------------------------

    #[test]
    fn poison_blur_and_withering_curse_poison_the_target() {
        for id in [361, 372] {
            let skill = def(id, Capsule::Status, 4, &[(f::FAMILY, 0), (f::ADD_EFFECT, 8), (f::ADD_AMOUNT, 300)]);
            let (mut c, mut players, _col) = setup(vec![skill.clone()], 50.0);
            apply(&mut c, &mut players, 0, Reach::Target(1), &skill);
            assert_eq!(c.fighters[1].poison_ticks, 600, "id {id}");
        }
    }

    #[test]
    fn venom_drip_poisons_everyone() {
        let venom = def(373, Capsule::Status, 4, &[(f::FAMILY, 0), (f::ADD_EFFECT, 8), (f::ADD_AMOUNT, 300)]);
        let (mut c, mut players, _col) = setup(vec![venom.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Everyone, &venom);
        assert_eq!(c.fighters[0].poison_ticks, 600);
        assert_eq!(c.fighters[1].poison_ticks, 600);
    }

    #[test]
    fn bind_shot_and_vapor_cloud_have_no_add_effect_to_act_on() {
        let bind_shot = def(739, Capsule::Status, 0, &[(f::FAMILY, 100)]);
        let vapor_cloud = def(343, Capsule::Status, 3, &[(f::FAMILY, 0)]);
        let (mut c, mut players, _col) = setup(vec![bind_shot.clone(), vapor_cloud.clone()], 50.0);
        assert!(!apply(&mut c, &mut players, 0, Reach::Target(1), &bind_shot));
        assert!(!apply(&mut c, &mut players, 0, Reach::Target(1), &vapor_cloud));
    }

    // ---- Still Moment (Everyone bind) -----------------------------------------

    #[test]
    fn still_moment_immobilizes_everyone() {
        let still = def(352, Capsule::Status, 6, &[(f::FAMILY, 0), (f::ADD_EFFECT, 6), (f::ADD_AMOUNT, 150)]);
        let (mut c, mut players, _col) = setup(vec![still.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Everyone, &still);
        assert_eq!(c.fighters[0].status.bound_ticks, 300, "150 table ticks = 5 sec");
        assert_eq!(c.fighters[1].status.bound_ticks, 300);
        assert_eq!(c.fighters[0].frozen_ticks, 0, "note only says \"immobilized\"");
    }

    // ---- Self buffs (Reach::Caster) ------------------------------------------

    #[test]
    fn heat_and_stimulate_raise_attack_and_lower_aura_rate_permanently() {
        let heat = def(175, Capsule::Status, 4, &[(f::FAMILY, 70), (f::STRENGTH_RULE, 4), (49, 1), (50, -10)]);
        let (mut c, mut players, _col) = setup(vec![heat.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &heat);
        assert_eq!(c.stat(0, Stat::Attack), 1);
        assert_eq!(c.stat(0, Stat::AuraRate), -10);
        assert_eq!(c.fighters[0].status.mods[0].ticks, None, "family 70: until reset");
    }

    #[test]
    fn tigers_strength_and_trance_are_timed_attack_buffs() {
        let tiger = def(232, Capsule::Status, 2, &[(f::FAMILY, 71), (f::STRENGTH_RULE, 4), (49, 2), (53, 450)]);
        let (mut c, mut players, _col) = setup(vec![tiger.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &tiger);
        assert_eq!(c.stat(0, Stat::Attack), 2);
        assert_eq!(c.fighters[0].status.mods[0].ticks, Some(900), "450 table ticks = 15 sec");
    }

    #[test]
    fn speed_up_and_acquisition() {
        let speed_up = def(49, Capsule::Status, 2, &[(f::FAMILY, 70), (f::STRENGTH_RULE, 5), (49, 20)]);
        let acquisition = def(50, Capsule::Status, 3, &[(f::FAMILY, 70), (f::STRENGTH_RULE, 6), (49, 20)]);
        let (mut c, mut players, _col) = setup(vec![speed_up.clone(), acquisition.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &speed_up);
        assert_eq!(c.stat(0, Stat::Speed), 20);
        apply(&mut c, &mut players, 1, Reach::Caster, &acquisition);
        assert_eq!(c.stat(1, Stat::Homing), 20);
    }

    #[test]
    fn meditation_raises_aura_rate_only() {
        let meditation = def(234, Capsule::Status, 4, &[(f::FAMILY, 70), (f::STRENGTH_RULE, 7), (49, 20)]);
        let (mut c, mut players, _col) = setup(vec![meditation.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &meditation);
        assert_eq!(c.stat(0, Stat::AuraRate), 20);
        assert_eq!(c.stat(0, Stat::Attack), 0);
    }

    #[test]
    fn martial_arts_scroll_pairs_aura_rate_and_attack() {
        // Unregistered; s49/s50 sit in the opposite order to Heat's (see
        // `apply_self_stat`), but they match this skill's own note exactly.
        let scroll = def(365, Capsule::Status, 4, &[(f::FAMILY, 0), (f::STRENGTH_RULE, 7), (49, 90), (50, 3), (53, 150)]);
        let (mut c, mut players, _col) = setup(vec![scroll.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &scroll);
        assert_eq!(c.stat(0, Stat::AuraRate), 90);
        assert_eq!(c.stat(0, Stat::Attack), 3);
    }

    #[test]
    fn optical_camo_and_void_are_timed_self_buffs() {
        let camo = def(115, Capsule::Status, 4, &[(f::FAMILY, 71), (f::STRENGTH_RULE, 12), (53, 450)]);
        let (mut c, mut players, _col) = setup(vec![camo.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &camo);
        assert_eq!(c.fighters[0].status.invisible_ticks, 900);

        let void = def(235, Capsule::Status, 4, &[(f::FAMILY, 71), (f::STRENGTH_RULE, 14), (53, 450)]);
        let (mut c, mut players, _col) = setup(vec![void.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &void);
        assert_eq!(c.fighters[0].status.immune_ticks, 900);
    }

    #[test]
    fn place_of_invisibility_is_let_through_despite_family_zero() {
        let hidden = def(353, Capsule::Status, 4, &[(f::FAMILY, 0), (f::STRENGTH_RULE, 12), (53, 450)]);
        let (mut c, mut players, _col) = setup(vec![hidden.clone()], 50.0);
        assert!(apply(&mut c, &mut players, 0, Reach::Caster, &hidden));
        assert_eq!(c.fighters[0].status.invisible_ticks, 900);
    }

    #[test]
    fn heat_devil_is_not_built() {
        // Unregistered, family 0, s45 = 14 (the same code as Void's immunity)
        // but its note describes invisibility + speed, not immunity — the
        // field doesn't match the note here, so it's left unbuilt rather than
        // guessed at.
        let heat_devil = def(360, Capsule::Status, 7, &[(f::FAMILY, 0), (f::STRENGTH_RULE, 14), (53, 150)]);
        let (mut c, mut players, _col) = setup(vec![heat_devil.clone()], 50.0);
        assert!(!apply(&mut c, &mut players, 0, Reach::Caster, &heat_devil));
    }

    #[test]
    fn doppelganger_copies_the_others_mods() {
        let doppel = def(291, Capsule::Status, 4, &[(f::FAMILY, 76), (f::STRENGTH_RULE, 18)]);
        let (mut c, mut players, _col) = setup(vec![doppel.clone()], 50.0);
        c.fighters[1].status.add(Stat::Attack, 3, None);
        c.fighters[1].status.add(Stat::Speed, -20, Some(100));
        apply(&mut c, &mut players, 0, Reach::Caster, &doppel);
        assert_eq!(c.fighters[0].status.mods, c.fighters[1].status.mods);
    }

    #[test]
    fn skywalk_and_demons_wing_hover() {
        // Skywalk is a movement special (s43 = 3, s44 = 5, s45 = 5, s46 = 450): status 11 for s46
        // program ticks and a jump of apex 10 x s45 = 50 u that hangs at its top
        // (`special::float_and_jump`; the rise over time is in special.rs's tests).
        let skywalk = def(51, Capsule::Status, 2, &[(f::FAMILY, 100), (43, 3), (44, 5), (45, 5), (f::STRENGTH, 450)]);
        let (mut c, mut players, _col) = setup(vec![skywalk.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &skywalk);
        assert_eq!(c.fighters[0].status.hover_ticks, 900, "\"15 sec\", read from s46");
        assert!(c.fighters[0].status_hover, "status 11");
        assert!((c.fighters[0].status.hover_height - 50.0).abs() < 0.01, "where the rise ends: 10 x s45");
        assert_eq!(players[0].jump_windup(), 26, "the jump's start clip first (13 program ticks)");

        let demons_wing = def(292, Capsule::Status, 5, &[(f::FAMILY, 100), (53, 450)]);
        let (mut c, mut players, _col) = setup(vec![demons_wing.clone()], 50.0);
        apply(&mut c, &mut players, 0, Reach::Caster, &demons_wing);
        assert_eq!(c.fighters[0].status.hover_ticks, 900);
        assert!((c.fighters[0].status.hover_height - 25.62).abs() < 0.01, "7 ft");
        assert_eq!(c.fighters[0].status.invisible_ticks, 900, "\"Add: Invisibility\"");
    }

    #[test]
    fn mind_reading_is_a_no_op() {
        let mind_reading = def(231, Capsule::Status, 2, &[(f::FAMILY, 100)]);
        let (mut c, mut players, _col) = setup(vec![mind_reading.clone()], 50.0);
        let before = c.fighters[0].status.clone();
        assert!(apply(&mut c, &mut players, 0, Reach::Caster, &mind_reading));
        assert_eq!(c.fighters[0].status, before);
    }
}
