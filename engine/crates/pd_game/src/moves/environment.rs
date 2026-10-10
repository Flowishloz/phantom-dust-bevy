//! Environment skills (aqua capsules): data-driven, as in the original.
//!
//! Casting one sets a crystal at the caster's feet (`Combat::fire`): one
//! crystal at a time, and a new one can be set once it has expired or been
//! captured. The record IS the effect (`FUN_14017bcc0`, `FUN_14017be00`,
//! `FUN_140271660`, read from `PDUWP.exe`, audits/P0-SKILLS.md appendix B):
//!
//! | field | meaning |
//! |---|---|
//! | s44 | the status id the crystal holds on everyone |
//! | s45 | capture lock: it cannot be captured until this many program ticks after being set (450 = 15 s on all of them) |
//! | s46 | the status amount (`FUN_140271660(id)` returns it for that id and 0 for every other) |
//! | s47 | lifetime in program ticks, 0 = until captured (1800 = 60 s on the seven bans) |
//!
//! Capturing a crystal (a skill-button press within 15 u, ground states only)
//! gives the capturer an Aura Particle on that button and removes the crystal
//! (`FUN_14017be00` -> `FUN_140261a00(slot, 0)`).
//!
//! Every reader of a status asks `status(env, id)`; the status ids are the
//! program's (0 attack, 1 speed, 2 homing, 3 Aura rate, 17 Health per skill
//! use, 18 Health per capture, 19 Health per Aura Particle captured, 20 Pierce
//! for all, 21/22 attack-skill STR bans, 25 erase ban, 29 refill only while
//! idle, 30 cost halved / doubled, 31 Necronomicon, 32 Memory Lapse, 34
//! capsule respawn wait, 35 stick turned 180 degrees, 38/39 cost bans,
//! 41-45 school bans). Defaults = no effect. Deterministic, no Bevy.

use super::effects::Stat;
use super::table_ticks;
use crate::combat::{Capsule, SkillDef, AURA_PARTICLE};
use pd_formats::skills::field;

/// The status id a crystal holds (s44).
pub const STATUS_ID: usize = field::DEFENCE_KIND;
/// Capture lock in program ticks (s45).
pub const CAPTURE_LOCK: usize = field::STRENGTH_RULE;
/// The status amount (s46).
pub const STATUS_AMOUNT: usize = field::STRENGTH;
/// Lifetime in program ticks, 0 = until captured (s47).
pub const LIFETIME: usize = field::DEFENCE;

/// What the crystal `env` gives status `id`: its amount (s46) if its status
/// id (s44) is `id`, else 0 (`FUN_140271660`).
pub fn status(env: &SkillDef, id: i16) -> i32 {
    if env.raw[43] == 5 && env.raw[STATUS_ID] == id {
        env.raw[STATUS_AMOUNT] as i32
    } else {
        0
    }
}

/// Sim ticks after being set before the crystal can be captured (s45; the
/// capture needs the age to be past it, `FUN_14017c5d0`).
pub fn capture_lock_ticks(env: &SkillDef) -> u32 {
    table_ticks(env.raw[CAPTURE_LOCK].max(0)) as u32
}

/// Sim ticks the crystal stands (s47), `None` = until captured or erased
/// (`FUN_14017bcc0`: expiry = now + s47 when s47 > 0).
pub fn lifetime_ticks(env: &SkillDef) -> Option<u32> {
    (env.raw[43] == 5 && env.raw[LIFETIME] > 0).then(|| table_ticks(env.raw[LIFETIME]) as u32)
}

/// Added to everyone's `stat` (attack in STR points, the rest in percent):
/// statuses 0 attack, 1 speed, 2 homing, 3 Aura rate.
pub fn stat(env: &SkillDef, stat: Stat) -> i32 {
    let id = match stat {
        Stat::Attack => 0,
        Stat::Speed => 1,
        Stat::Homing => 2,
        Stat::AuraRate => 3,
    };
    status(env, id)
}

/// Aura cost of `def` while `env` stands: status 30 halves it (-1, rounded
/// down: Optimization) or doubles it (+1: Irregular Rhythm) (`FUN_1402623b0`).
pub fn cost(env: &SkillDef, def: &SkillDef, cost: i32) -> i32 {
    let _ = def;
    match status(env, 30) {
        -1 => cost >> 1,
        1 => cost * 2,
        _ => cost,
    }
}

/// A "Cost X" skill: s37 = 2, "hold to charge" (Gungnir, Ki Cannon, Ki Palm;
/// `FUN_140262d80` reads `+0x46`).
fn is_cost_x(def: &SkillDef) -> bool {
    def.raw[37] == 2
}

/// The table's colour number (s9) of a capsule type: 1 attack ... 6 environment.
fn colour(capsule: Capsule) -> i16 {
    match capsule {
        Capsule::Attack => 1,
        Capsule::Defence => 2,
        Capsule::Erase => 3,
        Capsule::Status => 4,
        Capsule::Special => 5,
        Capsule::Environment => 6,
    }
}

/// May `def` be used at all (`FUN_140262d80`, the "can use now" test)?
pub fn allowed(env: &SkillDef, def: &SkillDef) -> bool {
    // The built-in Aura Particle is never a "skill" to prohibit.
    if def.id == AURA_PARTICLE {
        return true;
    }
    // The cost compared is the one the button shows (after status 30).
    let cost = cost(env, def, def.cost);
    // 38 / 39: a cost ban (cost >= amount / cost <= amount), and both ban
    // every "Cost X" skill.
    let (s38, s39) = (status(env, 38), status(env, 39));
    if (s38 != 0 || s39 != 0) && is_cost_x(def) {
        return false;
    }
    if (s38 != 0 && s38 <= cost) || (s39 != 0 && cost <= s39) {
        return false;
    }
    let colour = colour(def.capsule);
    if colour == 1 {
        // 21 / 22: attack skills banned by STR (>= amount / <= amount); also
        // Cost X skills, [Object] attacks, and anything with a STR rule other
        // than 31 or that is not an ordinary attack object (Recollection...).
        let (s21, s22) = (status(env, 21), status(env, 22));
        if s21 != 0 || s22 != 0 {
            if is_cost_x(def) || def.raw[43] != 0 || def.motion() == 11 || (def.strength_rule != 0 && def.strength_rule != 31) {
                return false;
            }
            if (s21 != 0 && s21 <= def.strength) || (s22 != 0 && def.strength <= s22) {
                return false;
            }
        }
    }
    // Colour bans: status 22 + colour (23 attack ... 28 environment); 25 is
    // Commandments' erase ban.
    if status(env, 22 + colour) != 0 {
        return false;
    }
    // School bans: status 40 + school (41-45).
    if def.school() > 0 && status(env, 40 + def.school()) != 0 {
        return false;
    }
    true
}

/// Health lost for each skill use (status 17: Backdraft).
pub fn health_on_use(env: &SkillDef) -> i32 {
    (status(env, 17) != 0) as i32
}

/// Health change for capturing a capsule (`aura`: it was an Aura Particle):
/// status 18 (Soul Cage) costs 1 Health per capture of anything, status 19
/// (Nature Blessing) gives 1 per Aura Particle (the capture path of
/// `FUN_1402517c0`).
pub fn health_on_capture(env: &SkillDef, aura: bool) -> i32 {
    let mut d = 0;
    if status(env, 18) != 0 {
        d -= 1;
    }
    if status(env, 19) != 0 && aura {
        d += 1;
    }
    d
}

/// Capsule spawn speed in percent (200 = twice as fast): a summary of
/// `capsule_wait_delta` for callers that want a rate.
#[allow(dead_code)]
pub fn capsule_rate_pct(env: &SkillDef) -> i32 {
    match status(env, 34) {
        a if a < 0 => 200,
        a if a > 0 => 50,
        _ => 100,
    }
}

/// Change to a spot's capsule respawn wait, in program ticks (1/30 s), on top
/// of the 420 hidden ticks: status 34 (`FUN_14014fd10`: timer = `0x1a4` +
/// status 34; Quiet Days +450, Accelerate -225; audits/P0-SKILLS.md
/// appendix B). The two are not exact doublings.
pub fn capsule_wait_delta(env: &SkillDef) -> i32 {
    status(env, 34)
}

/// Every attack skill gains Pierce (status 20: Berserk).
pub fn all_pierce(env: &SkillDef) -> bool {
    status(env, 20) != 0
}

/// Everyone's directional controls are inverted (status 35: the stick turned
/// 180 degrees, Confused Gravity).
pub fn inverted(env: &SkillDef) -> bool {
    status(env, 35) != 0
}

/// Aura recovers only while idle (status 29: Fatigue; the program refills in
/// player states 0, 21, 22, 24 -- `FUN_1402523a0`).
pub fn aura_stops_while_moving(env: &SkillDef) -> bool {
    status(env, 29) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::testkit::{self as k, f};

    /// A crystal's record as the data holds it (s44 status id, s45 capture
    /// lock, s46 amount, s47 lifetime; audits/P0-SKILLS.md appendix B, read
    /// from `cache/skills.json`).
    fn env(id: i16, status_id: i16, amount: i16, life: i16) -> SkillDef {
        k::def(id, Capsule::Environment, 2, &[(43, 5), (STATUS_ID, status_id), (CAPTURE_LOCK, 450), (STATUS_AMOUNT, amount), (LIFETIME, life)])
    }

    /// Appendix B, by skill id: (status id, amount, lifetime).
    const TABLE: [(i16, i16, i16, i16); 29] = [
        (163, 3, -30, 0), // Desertification
        (105, 0, -1, 0),  // Holy Ray
        (286, 0, 1, 0),   // Lunar Force
        (38, 1, 20, 0),   // Reduce Gravity
        (39, 1, -20, 0),  // Abnormal Gravity
        (166, 2, -30, 0), // Dense Fog
        (108, 17, 1, 0),  // Backdraft
        (109, 30, -1, 0), // Optimization
        (225, 30, 1, 0),  // Irregular Rhythm
        (164, 19, 1, 0),  // Nature Blessing
        (167, 20, 1, 0),  // Berserk
        (165, 21, 4, 0),  // Forest Sanctuary
        (288, 22, 2, 0),  // Athena's Command
        (168, 25, 1, 1800), // Commandments
        (224, 29, 1, 0),  // Fatigue
        (227, 18, 1, 0),  // Soul Cage
        (226, 32, 1, 0),  // Memory Lapse
        (287, 31, 1, 0),  // Necronomicon
        (106, 38, 3, 0),  // Reduce Entropy
        (228, 39, 2, 0),  // Chaos
        (111, 34, 450, 0), // Quiet Days
        (107, 34, -225, 0), // Accelerate
        (40, 35, 1, 0),   // Confused Gravity
        (169, 41, 1, 1800), // Providence
        (41, 42, 1, 1800), // Vanishing Light
        (229, 43, 1, 1800), // Stagnant Air
        (289, 44, 1, 1800), // Spiritual World
        (110, 45, 1, 1800), // Scientism
        (332, 0, 0, 0),   // Unison (empty record)
    ];

    fn rec(id: i16) -> SkillDef {
        let (_, s, a, l) = TABLE.iter().find(|r| r.0 == id).copied().expect("an environment skill id");
        env(id, s, a, l)
    }

    #[test]
    fn the_status_a_crystal_holds_comes_from_its_own_record_not_its_id() {
        // Same numbers under another id: same effect (the old code keyed on id).
        let sanctuary = env(900, 21, 4, 0);
        assert_eq!((status(&sanctuary, 21), status(&sanctuary, 22), status(&sanctuary, 0)), (4, 0, 0), "FUN_140271660 returns the amount for its own status id only");
        let fan = k::def(901, Capsule::Attack, 1, &[(STATUS_ID, 21), (STATUS_AMOUNT, 4)]);
        assert_eq!(status(&fan, 21), 0, "not an environment record (s43 != 5)");
    }

    #[test]
    fn stat_hooks_match_the_table() {
        assert_eq!(stat(&rec(163), Stat::AuraRate), -30, "Desertification");
        assert_eq!(stat(&rec(105), Stat::Attack), -1, "Holy Ray");
        assert_eq!(stat(&rec(286), Stat::Attack), 1, "Lunar Force");
        assert_eq!(stat(&rec(38), Stat::Speed), 20, "Reduce Gravity");
        assert_eq!(stat(&rec(39), Stat::Speed), -20, "Abnormal Gravity");
        assert_eq!(stat(&rec(166), Stat::Homing), -30, "Dense Fog");
        assert_eq!(stat(&rec(163), Stat::Attack), 0, "wrong stat: no effect");
        assert_eq!(stat(&rec(332), Stat::Attack), 0, "Unison's record is empty");
    }

    #[test]
    fn capture_lock_and_lifetime_are_read_from_the_record() {
        assert_eq!(capture_lock_ticks(&rec(163)), 900, "450 program ticks = 15 s = 900 sim ticks");
        assert_eq!(lifetime_ticks(&rec(163)), None, "no lifetime: stands until captured");
        assert_eq!(lifetime_ticks(&rec(168)), Some(3600), "Commandments: 1800 program ticks = 60 s");
        assert_eq!(lifetime_ticks(&rec(110)), Some(3600), "Scientism");
    }

    #[test]
    fn cost_status_halves_or_doubles() {
        let atk = k::def(1, Capsule::Attack, 4, &[]);
        assert_eq!(cost(&rec(109), &atk, 4), 2, "Optimization: -50%");
        assert_eq!(cost(&rec(109), &atk, 5), 2, "rounds down");
        assert_eq!(cost(&rec(225), &atk, 3), 6, "Irregular Rhythm: doubled");
        assert_eq!(cost(&rec(163), &atk, 3), 3, "unrelated crystal: unchanged");
    }

    #[test]
    fn str_bans_follow_status_21_and_22() {
        let sanctuary = rec(165);
        let (three, four) = (k::def(1, Capsule::Attack, 1, &[(f::STRENGTH, 3)]), k::def(2, Capsule::Attack, 1, &[(f::STRENGTH, 4)]));
        let computed = k::def(3, Capsule::Attack, 1, &[(f::STRENGTH_RULE, 17)]); // STR = target's Level
        let ruled31 = k::def(4, Capsule::Attack, 1, &[(f::STRENGTH_RULE, 31), (f::STRENGTH, 2)]);
        let object = k::def(5, Capsule::Attack, 1, &[(f::MOTION, 11), (f::STRENGTH, 1)]);
        let cost_x = k::def(6, Capsule::Attack, 0, &[(37, 2), (f::STRENGTH, 1)]);
        let status_skill = k::def(7, Capsule::Status, 1, &[(f::STRENGTH, 9)]);
        assert!(allowed(&sanctuary, &three), "STR 3: not >= 4");
        assert!(!allowed(&sanctuary, &four), "STR >= 4: banned");
        assert!(!allowed(&sanctuary, &computed), "STR = X (rule 17): banned");
        assert!(allowed(&sanctuary, &ruled31), "rule 31 is the one exception");
        assert!(!allowed(&sanctuary, &object), "[Object] attacks are banned too");
        assert!(!allowed(&sanctuary, &cost_x), "Cost X skills are banned too (s37 = 2)");
        assert!(allowed(&sanctuary, &status_skill), "not an attack skill: unaffected");

        let athena = rec(288);
        let two = k::def(8, Capsule::Attack, 1, &[(f::STRENGTH, 2)]);
        assert!(!allowed(&athena, &two), "STR <= 2: banned");
        assert!(allowed(&athena, &three), "STR 3: allowed");
        assert!(!allowed(&athena, &computed), "STR = X: banned");
    }

    #[test]
    fn cost_bans_follow_status_38_and_39_and_catch_cost_x() {
        let entropy = rec(106);
        let (cheap, costly) = (k::def(1, Capsule::Attack, 2, &[]), k::def(2, Capsule::Attack, 3, &[]));
        assert!(allowed(&entropy, &cheap), "cost 2: below the ban");
        assert!(!allowed(&entropy, &costly), "cost 3 or more: banned");
        let chaos = rec(228);
        assert!(!allowed(&chaos, &cheap), "cost 2 or lower: banned");
        assert!(allowed(&chaos, &costly), "cost 3: above the ban");
        // "Cost X" is s37 = 2 (Gungnir, Ki Cannon, Ki Palm), not "cost >= 90".
        let charge = k::def(3, Capsule::Attack, 0, &[(37, 2)]);
        let pricey = k::def(4, Capsule::Attack, 99, &[]);
        assert!(!allowed(&entropy, &charge) && !allowed(&chaos, &charge), "both bans catch a hold-to-charge skill");
        assert!(!allowed(&entropy, &pricey), "cost 99 is simply expensive");
        assert!(allowed(&chaos, &pricey), "...and not a cost-X skill");
    }

    #[test]
    fn the_ban_compares_the_cost_the_button_shows() {
        // Reduce Entropy bans cost >= 3 against the effective cost; no crystal
        // holds two statuses, so this is the cost status 30 itself would give.
        let optimisation = rec(109);
        let four = k::def(1, Capsule::Attack, 4, &[]);
        assert!(allowed(&optimisation, &four), "status 30 is not a ban");
        assert_eq!(cost(&optimisation, &four, four.cost), 2);
    }

    #[test]
    fn colour_and_school_bans() {
        let cmd = rec(168);
        let (erase, attack) = (k::def(1, Capsule::Erase, 1, &[]), k::def(2, Capsule::Attack, 1, &[]));
        assert!(!allowed(&cmd, &erase), "status 25 bans erase skills");
        assert!(allowed(&cmd, &attack), "only erase skills");
        let (faith, psycho) = (k::def(3, Capsule::Attack, 1, &[(f::SCHOOL, 5)]), k::def(4, Capsule::Attack, 1, &[(f::SCHOOL, 1)]));
        assert!(!allowed(&rec(110), &faith), "status 45 bans Faith");
        assert!(allowed(&rec(110), &psycho));
        for (id, school) in [(169, 1), (41, 2), (229, 3), (289, 4), (110, 5)] {
            let skill = k::def(10, Capsule::Attack, 1, &[(f::SCHOOL, school)]);
            assert!(!allowed(&rec(id), &skill), "crystal {id} bans school {school}");
        }
    }

    #[test]
    fn aura_particle_is_never_prohibited() {
        // Chaos bans cost <= 2, which would otherwise catch the particle's 0 cost.
        assert!(allowed(&rec(228), &SkillDef::aura_particle()));
    }

    #[test]
    fn health_hooks_match_the_table() {
        assert_eq!(health_on_use(&rec(108)), 1, "Backdraft: status 17");
        assert_eq!(health_on_use(&rec(163)), 0);
        assert_eq!(health_on_capture(&rec(227), false), -1, "Soul Cage: any capsule");
        assert_eq!(health_on_capture(&rec(227), true), -1, "including Aura Particles");
        assert_eq!(health_on_capture(&rec(164), true), 1, "Nature Blessing: Aura Particle");
        assert_eq!(health_on_capture(&rec(164), false), 0, "not an ordinary capsule");
    }

    #[test]
    fn flag_statuses() {
        assert_eq!((capsule_wait_delta(&rec(111)), capsule_wait_delta(&rec(107)), capsule_wait_delta(&rec(163))), (450, -225, 0), "status 34: +450 / -225 program ticks");
        assert_eq!((capsule_rate_pct(&rec(107)), capsule_rate_pct(&rec(111)), capsule_rate_pct(&rec(163))), (200, 50, 100));
        assert!(all_pierce(&rec(167)) && !all_pierce(&rec(163)), "Berserk: status 20");
        assert!(inverted(&rec(40)) && !inverted(&rec(163)), "Confused Gravity: status 35");
        assert!(aura_stops_while_moving(&rec(224)) && !aura_stops_while_moving(&rec(163)), "Fatigue: status 29");
    }

    // ---- end-to-end through the sim (combat.rs calls these hooks itself) ----

    #[test]
    fn chaos_crystal_blocks_casting_a_cheap_skill_end_to_end() {
        let cheap = k::def(50, Capsule::Attack, 2, &[]);
        let (mut c, mut p, col) = k::setup(vec![rec(228), cheap], 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 228); // player 0 sets the Chaos crystal
        assert!(c.crystal.is_some());
        c.fighters[1].level = 8;
        c.fighters[1].aura_ticks = 8 * crate::combat::AURA_TICKS_PER_POINT;
        c.fighters[1].slots[0] = Some(50);
        k::press(&mut c, &mut p, &col, 1, 0);
        assert_eq!(c.fighters[1].slots[0], Some(50), "cost-2 skill banned: never leaves the slot");
        assert_eq!(c.fighters[1].aura(), 8, "no aura spent: it never cast");
    }

    #[test]
    fn optimization_crystal_halves_cost_end_to_end() {
        let atk = k::def(60, Capsule::Attack, 4, &[(pd_formats::skills::field::USES, 1)]);
        let (mut c, mut p, col) = k::setup(vec![rec(109), atk], 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 109); // player 0 sets the Optimization crystal
        c.fighters[1].level = 8;
        c.fighters[1].aura_ticks = 8 * crate::combat::AURA_TICKS_PER_POINT;
        c.fighters[1].slots[0] = Some(60);
        k::press(&mut c, &mut p, &col, 1, 0);
        k::run(&mut c, &mut p, &col, crate::combat::CAST_TICKS as usize + 2); // paid when it leaves
        assert_eq!(c.fighters[1].slots[0], None, "cast: 4 halved to 2 is affordable");
        assert_eq!(c.fighters[1].aura(), 6, "8 - 2 (halved cost), not 8 - 4");
    }

    #[test]
    fn a_crystal_with_a_lifetime_runs_out_and_a_new_one_can_be_set() {
        // `FUN_14017bcc0`: expiry = now + s47. Commandments stands 60 s, then the field is free.
        let (mut c, mut p, col) = k::setup(vec![rec(168), rec(163)], 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 168);
        assert_eq!(c.crystal.as_ref().map(|cr| cr.skill), Some(168));
        k::run(&mut c, &mut p, &col, 3500);
        assert!(c.crystal.is_some(), "still up inside its minute");
        k::run(&mut c, &mut p, &col, 200);
        assert!(c.crystal.is_none(), "gone after 3600 sim ticks");
        c.fighters[0].arsenal.clear();
        k::use_skill(&mut c, &mut p, &col, 1, 163);
        assert_eq!(c.crystal.as_ref().map(|cr| cr.skill), Some(163), "a new one can be set after the expiry");
    }

    #[test]
    fn a_crystal_without_a_lifetime_never_expires() {
        let (mut c, mut p, col) = k::setup(vec![rec(163)], 100.0);
        k::use_skill(&mut c, &mut p, &col, 0, 163);
        k::run(&mut c, &mut p, &col, 5000);
        assert!(c.crystal.is_some());
    }

    #[test]
    fn capturing_a_crystal_gives_the_capturer_an_aura_particle_on_the_pressed_button() {
        // `FUN_14017be00`: past the 450-tick lock, a skill-button press within 15 u
        // assigns record 0 (the Aura Particle) to that button and removes the crystal.
        let (mut c, mut p, col) = k::setup(vec![rec(163), k::def(7, Capsule::Attack, 1, &[])], 100.0);
        c.fighters[0].arsenal.clear();
        c.fighters[1].arsenal.clear();
        k::use_skill(&mut c, &mut p, &col, 0, 163);
        p[1].pos = p[0].pos + glam::Vec3::new(8.0, 0.0, 0.0); // within 15 u of the crystal
        // (`press` steps twice and the crystal ages first: 898 -> 899 at the press.)
        c.crystal.as_mut().unwrap().age = 898;
        k::press(&mut c, &mut p, &col, 1, 1);
        assert_eq!(c.crystal.as_ref().map(|cr| cr.skill), Some(163), "age 899 is not past the lock (needs now > created + s45 = 900)");
        c.fighters[1].slots[1] = Some(7);
        c.crystal.as_mut().unwrap().age = 901;
        k::press(&mut c, &mut p, &col, 1, 1);
        assert!(c.crystal.is_none(), "captured");
        assert_eq!(c.fighters[1].slots[1], Some(crate::combat::AURA_PARTICLE), "the Aura Particle replaces what was on the pressed button");
        assert_eq!(c.fighters[1].slots[0], None, "no other button touched");
    }

    #[test]
    fn a_crystal_cannot_be_captured_from_far_away_or_before_its_lock() {
        let (mut c, mut p, col) = k::setup(vec![rec(163)], 100.0);
        c.fighters[0].arsenal.clear();
        k::use_skill(&mut c, &mut p, &col, 0, 163);
        c.crystal.as_mut().unwrap().age = 5000;
        c.fighters[1].slots[2] = Some(crate::combat::AURA_PARTICLE);
        c.fighters[1].slots[3] = None;
        // Player 1 stands 100 u away: pressing a button casts, it does not capture.
        k::press(&mut c, &mut p, &col, 1, 3);
        assert!(c.crystal.is_some(), "out of reach: not captured");
    }

    #[test]
    fn necronomicon_and_memory_lapse_statuses_are_readable() {
        let (mut c, mut p, col) = k::setup(vec![rec(287), rec(226)], 100.0);
        c.fighters[0].arsenal.clear();
        assert_eq!((c.env_status(31), c.env_status(32)), (0, 0), "no crystal: nothing");
        k::use_skill(&mut c, &mut p, &col, 0, 287);
        assert_eq!((c.env_status(31), c.env_status(32)), (1, 0), "Necronomicon is status 31");
    }
}
