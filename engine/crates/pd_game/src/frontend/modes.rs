//! Game modes: rules variants of the normal versus match (DESIGN-BRAWL.md), and Prop Hunt (DESIGN-PROPHUNT.md).
//!
//! A mode is a table entry in [`MODES`]: a name, a one-line description, a [`Kind`], a `combat::Rules` preset and a
//! fixed four-skill hand. A `Kind::Versus` mode is just those rules and that hand; a `Kind::PropHunt` mode also runs
//! `prophunt.rs` in the sim (its own rounds and roles), is local only (`online: false`: the online lobby never offers
//! it) and takes its round count and round time from the setup. Adding the next mode is adding one entry. Modes are chosen in two places, both over this one
//! table: the GAMEMODES row of the local versus setup's game-rules page (`frontend/setup.rs`, STANDARD plus every
//! entry), and the GAMEMODE row of the online host's lobby (`frontend/online.rs`; the host picks, the joiner sees
//! it read-only). A mode fixes the hand, game rule, starting Level and skills; the setup keeps its character / stage /
//! seat / CPU rows. Starting the match carries the mode's index in `MatchSetup::mode`, and
//! `MatchSetup::apply_to_combat` turns it into the sim's `Rules`: no parallel sim path.

use crate::combat::{Rules, SLOTS};

/// What a mode is beyond its `Rules`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The normal versus match under the mode's rules and fixed hand (Close Quarters).
    Versus,
    /// Prop Hunt (`prophunt.rs`): seeker and hiders, its own rounds. The hand is the seeker's (the knife on X).
    PropHunt,
}

/// One game mode.
#[derive(Clone, Copy, Debug)]
pub struct GameMode {
    /// Stable key (for scripts and saves; never shown).
    pub id: &'static str,
    /// The list row and the setup screen's header. Easy to rename: nothing else depends on it.
    pub name: &'static str,
    /// The one-line description shown under the list.
    pub blurb: &'static str,
    /// The skills (`skills.json` `id`s) on buttons 1-4 of every fighter, in order, all match.
    pub hand: [i16; SLOTS],
    /// Everything else the mode changes; `fixed_hand` is filled from `hand` by [`GameMode::rules`].
    pub rules: Rules,
    pub kind: Kind,
    /// Offered in the online host's lobby. Prop Hunt is local split screen only (its views and pads are per seat).
    pub online: bool,
}

impl GameMode {
    /// The sim's rules for this mode: the preset with the fixed hand in. Prop Hunt deals no fixed hand to everyone
    /// (`prophunt.rs` puts the knife on the seeker's X and leaves the hiders empty-handed).
    pub fn rules(&self) -> Rules {
        match self.kind {
            Kind::Versus => Rules { fixed_hand: Some(self.hand), ..self.rules },
            Kind::PropHunt => Rules { fixed_hand: None, ..self.rules },
        }
    }

    pub fn is_prop_hunt(&self) -> bool {
        self.kind == Kind::PropHunt
    }
}

/// Skill ids used by the table (`skills.json` `id`, the s3 id every part of the game keys by; the program's own
/// index is `record + 1`, which is NOT this).
pub mod skill {
    pub const PSYCHO_KNIFE: i16 = 3;
    pub const POWER_SHELL: i16 = 29;
    pub const DASH: i16 = 240;
    pub const RAPID_CANNON: i16 = 75;
}

/// Every mode, in list order. A new mode is a new entry here and nothing else.
pub const MODES: &[GameMode] = &[
    // Laurence 2026-10-02: a close-to-mid range brawl. Working title; rename it here.
    GameMode {
        id: "close_quarters",
        name: "Close Quarters",
        blurb: "Close-to-mid range brawl: the same four skills for everyone, never used up, 10 Aura, no capsules.",
        hand: [skill::PSYCHO_KNIFE, skill::POWER_SHELL, skill::DASH, skill::RAPID_CANNON],
        // Level 10 = 10 Aura to spend, and it refills up to that as normal. No capsules; the four skills never run out.
        rules: Rules { start_level: 10, start_hand: 0, capsules: false, infinite_uses: true, ..Rules::NORMAL },
        kind: Kind::Versus,
        online: true,
    },
    // Laurence 2026-10-07: hide and seek. The seeker has Psycho Knife on X (10 attempts, 2 Health each, Aura never runs
    // out); hiders become stage objects with X and freeze with B. Local 2-4 people, no CPUs (prophunt.rs).
    GameMode {
        id: "prop_hunt",
        name: "Prop Hunt",
        blurb: "Hide and seek, 2-4 people: hiders become objects (A or X) and freeze (B); the seeker has 10 Psycho Knives. Seeker rotates each round.",
        hand: [skill::PSYCHO_KNIFE; SLOTS],
        // Level 10 for the seeker's knife (prophunt.rs refills it every tick and gives the hiders none); no capsules,
        // the knife never uses up (the mode counts its 10 attempts itself).
        rules: Rules { start_level: crate::prophunt::SEEKER_LEVEL, start_hand: 0, capsules: false, infinite_uses: true, ..Rules::NORMAL },
        kind: Kind::PropHunt,
        online: false,
    },
];

/// The mode a command-line key names: its position in the table or its `id` (`--mode 0`, `--mode close_quarters`).
pub fn find(key: &str) -> Option<&'static GameMode> {
    key.parse::<usize>().ok().and_then(get).or_else(|| MODES.iter().find(|m| m.id == key))
}

/// The mode `index` names, if any.
pub fn get(index: usize) -> Option<&'static GameMode> {
    MODES.get(index)
}

/// Whether mode `index` is Prop Hunt.
pub fn is_prop_hunt(index: Option<usize>) -> bool {
    index.and_then(get).is_some_and(GameMode::is_prop_hunt)
}

/// The mode whose stable `id` is `id` (online: what the host sent). Unlike [`find`], an index is not a key here.
pub fn by_id(id: &str) -> Option<usize> {
    MODES.iter().position(|m| m.id == id)
}

/// A fingerprint of everything that decides the sim's `Rules` for `mode` in an online match (the mode's resolved `Rules`,
/// fixed hand included; `None` = the normal versus' defaults). Two PCs agree on it exactly when they run the same `Rules`.
pub fn rules_fingerprint(mode: Option<usize>) -> u64 {
    let setup = super::state::MatchSetup { mode, ..Default::default() };
    crate::net::fingerprint(&format!("{:?}", setup.resolved_rules()))
}

/// The online lobby's wire fields for a mode choice: its `id` ("" = STANDARD) and [`rules_fingerprint`].
pub fn lobby_fields(mode: Option<usize>) -> (String, u64) {
    (mode.and_then(get).map_or_else(String::new, |m| m.id.to_string()), rules_fingerprint(mode))
}

/// What the host announced -> our mode choice, or why this PC can't play it (unknown id: a different build; fingerprint
/// differs: the same id means other `Rules` here). Lockstep needs both PCs on the same `Rules`, so a mismatch refuses.
pub fn resolve_lobby(id: &str, rules_hash: u64) -> Result<Option<usize>, String> {
    let mode = if id.is_empty() { None } else { Some(by_id(id).ok_or_else(|| format!("The host's gamemode \"{id}\" is not in this game build: both PCs must run the same build."))?) };
    if mode.and_then(get).is_some_and(|m| !m.online) {
        return Err(format!("{} is a local split-screen mode: it can't be played online.", label(mode)));
    }
    if rules_fingerprint(mode) != rules_hash {
        return Err(format!("The {} rules differ between the two PCs: both must run the same build.", label(mode)));
    }
    Ok(mode)
}

/// A hash of a whole mode table (id, name, hand and every rule field). It is part of the online handshake's data hash,
/// so two builds whose tables differ refuse to connect.
pub fn table_hash_of(table: &[GameMode]) -> u64 {
    let mut text = String::new();
    for m in table {
        text.push_str(&format!("{}|{}|{:?}|{:?}|{:?}|{};", m.id, m.name, m.hand, m.rules(), m.kind, m.online));
    }
    crate::net::fingerprint(&text)
}

/// [`table_hash_of`] for [`MODES`].
pub fn table_hash() -> u64 {
    table_hash_of(MODES)
}

/// The label a mode shows in a GAMEMODES / GAMEMODE selector: `None` is the normal versus.
pub fn label(mode: Option<usize>) -> String {
    mode.and_then(get).map_or_else(|| "STANDARD".to_string(), |m| m.name.to_uppercase())
}

/// The selector's one-line help for a mode (or for the normal versus).
pub fn blurb(mode: Option<usize>) -> &'static str {
    mode.and_then(get).map_or("The normal versus match: your own arsenals, the game rule, starting Level and skills you choose.", |m| m.blurb)
}

/// Step a selector one place (`forward` or back) over STANDARD plus every table entry, wrapping (the local setup's
/// GAMEMODES row).
pub fn step(current: Option<usize>, forward: bool) -> Option<usize> {
    step_over(current, forward, |_| true)
}

/// The online lobby's GAMEMODE row: as [`step`], over STANDARD and the modes that can be played online only.
pub fn step_online(current: Option<usize>, forward: bool) -> Option<usize> {
    step_over(current, forward, |m| m.online)
}

/// How many choices the online lobby's GAMEMODE row has (STANDARD plus every online mode).
#[allow(dead_code)] // the lobby and modes tests
pub fn online_choices() -> usize {
    1 + MODES.iter().filter(|m| m.online).count()
}

fn step_over(current: Option<usize>, forward: bool, offered: impl Fn(&GameMode) -> bool) -> Option<usize> {
    // STANDARD, then the offered entries by table index.
    let list: Vec<Option<usize>> = std::iter::once(None).chain((0..MODES.len()).filter(|&i| offered(&MODES[i])).map(Some)).collect();
    let n = list.len();
    let cur = list.iter().position(|&m| m == current).unwrap_or(0);
    list[if forward { (cur + 1) % n } else { (cur + n - 1) % n }]
}

/// The hand line for a mode, from the skill names in the catalog.
pub fn hand_line(mode: &GameMode, skills: &[crate::combat::SkillDef]) -> String {
    let name = |id: i16| skills.iter().find(|s| s.id == id).map_or_else(|| format!("#{id}"), |s| s.name.clone());
    if mode.is_prop_hunt() {
        return format!("Seeker: {} on X, 10 attempts. Hiders: no skills", name(mode.hand[0]));
    }
    let names: Vec<String> = mode.hand.iter().map(|&id| skills.iter().find(|s| s.id == id).map_or_else(|| format!("#{id}"), |s| s.name.clone())).collect();
    format!("Hand: {}", names.join("  /  "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec3;
    use crate::combat::testkit as k;
    use crate::combat::{Combat, Roster, AURA_PARTICLE, AURA_TICKS_PER_POINT, CAPSULE_RESPAWN_TICKS, COUNTDOWN_TICKS, CAPSULE_EVERY_TICKS};
    use crate::sim::Player;

    fn cache_skills() -> Option<Vec<crate::combat::SkillDef>> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/skills.json");
        p.exists().then(|| crate::load_skills(&p, false))
    }

    /// A duel under `rules`, countdown over, the real skill table when the cache has it (else the tests that need
    /// real skills are skipped by the caller).
    fn duel(rules: Rules, skills: Vec<crate::combat::SkillDef>) -> (Combat, [Player; 2], pd_formats::collision::Collision) {
        duel_with(rules, skills, AURA_PARTICLE)
    }

    /// As `duel`, every fighter's arsenal 30 of `deck_skill`.
    fn duel_with(rules: Rules, skills: Vec<crate::combat::SkillDef>, deck_skill: i16) -> (Combat, [Player; 2], pd_formats::collision::Collision) {
        let mut c = Combat::with_roster(skills, Roster::duel(), std::array::from_fn(|_| vec![deck_skill; 30]));
        c.set_rules(rules);
        c.round_ticks = COUNTDOWN_TICKS;
        let players = [Player::new(Vec3::ZERO, 0.0), Player::new(Vec3::new(0.0, 0.0, 60.0), std::f32::consts::PI)];
        (c, players, k::floor())
    }

    #[test]
    fn the_table_entry_gives_the_right_rules_and_hand() {
        let m = get(0).expect("the first mode");
        assert_eq!((m.id, m.name), ("close_quarters", "Close Quarters"));
        assert_eq!(m.hand, [3, 29, 240, 75], "Psycho Knife, Power Shell, Dash, Rapid Cannon (skills.json ids)");
        let r = m.rules();
        assert_eq!(r.fixed_hand, Some([3, 29, 240, 75]));
        assert_eq!(r.start_level, 10, "10 Aura to spend");
        assert!(!r.capsules && r.infinite_uses);
        // Everything the mode does not change is the normal match's.
        let n = Rules::default();
        assert_eq!((r.set, r.capsule_respawn_ticks, r.start_hand), (n.set, n.capsule_respawn_ticks, 0));
        assert_eq!(Rules::default(), Rules::NORMAL);
        assert_eq!((n.capsules, n.infinite_uses, n.fixed_hand), (true, false, None), "the normal match is unchanged");
        // Every mode's ids are distinct names and keys.
        for (i, a) in MODES.iter().enumerate() {
            assert!(MODES.iter().skip(i + 1).all(|b| b.id != a.id && b.name != a.name));
        }
        assert_eq!(m.kind, Kind::Versus);
        assert!(m.online);
    }

    #[test]
    fn prop_hunt_is_in_the_table_after_close_quarters_local_only_with_no_fixed_hand() {
        let i = by_id("prop_hunt").expect("prop hunt is in the table");
        assert_eq!(i, 1, "after STANDARD and CLOSE QUARTERS");
        let m = get(i).unwrap();
        assert!(m.is_prop_hunt() && !m.online && is_prop_hunt(Some(i)));
        assert!(!is_prop_hunt(Some(0)) && !is_prop_hunt(None));
        assert_eq!(label(Some(i)), "PROP HUNT");
        let r = m.rules();
        assert_eq!(r.fixed_hand, None, "prophunt.rs deals the seeker's knife itself");
        assert!(!r.capsules && r.infinite_uses);
        assert_eq!(m.hand[0], skill::PSYCHO_KNIFE, "the knife is on X (slot 0)");
        assert_eq!(skill::PSYCHO_KNIFE, crate::prophunt::PSYCHO_KNIFE);
        assert_eq!(find("prop_hunt").map(|m| m.id), Some("prop_hunt"), "--mode prop_hunt");
        // The local GAMEMODES row: STANDARD, CLOSE QUARTERS, PROP HUNT. The online row skips it.
        assert_eq!(step(Some(0), true), Some(i));
        assert_eq!(step_online(Some(0), true), None, "online: Close Quarters wraps back to STANDARD");
        assert_eq!(step_online(None, false), Some(0));
        assert_eq!(online_choices(), MODES.len(), "every mode but Prop Hunt");
        // A lobby announcing it (another build, a hand-made packet) is refused with a reason.
        let (id, hash) = lobby_fields(Some(i));
        assert!(resolve_lobby(&id, hash).unwrap_err().contains("local"));
    }

    #[test]
    fn a_mode_reaches_the_match_through_the_setup() {
        use crate::frontend::state::MatchSetup;
        let mut c = Combat::with_roster(Vec::new(), Roster::duel(), std::array::from_fn(|_| Vec::new()));
        MatchSetup { mode: Some(0), ..MatchSetup::default() }.apply_to_combat(&mut c);
        assert_eq!(c.rules, MODES[0].rules());
        // No mode: exactly what the setup chose, as before.
        MatchSetup::default().apply_to_combat(&mut c);
        assert_eq!(c.rules, Rules { start_level: 0, start_hand: 4, ..Rules::NORMAL });
    }

    #[test]
    fn every_fighter_starts_with_the_hand_on_the_buttons_and_ten_aura() {
        let Some(skills) = cache_skills() else { return };
        for who in 0..2 {
            let (c, _, _) = duel(MODES[0].rules(), skills.clone());
            let f = &c.fighters[who];
            assert_eq!(f.slots, [Some(3), Some(29), Some(240), Some(75)]);
            assert_eq!((f.level, f.aura(), f.aura_ticks), (10, 10, 10 * AURA_TICKS_PER_POINT));
            assert!(f.arsenal.is_empty(), "nothing left to draw");
        }
        // A new round puts the same hand back.
        let (mut c, _, _) = duel(MODES[0].rules(), skills);
        c.fighters[0].set_slot(1, None);
        c.set_rules(MODES[0].rules());
        assert_eq!(c.fighters[0].slots, [Some(3), Some(29), Some(240), Some(75)]);
    }

    #[test]
    fn no_capsule_ever_appears() {
        let Some(skills) = cache_skills() else { return };
        // Both capsule paths: the stage's own spots, and the timer-driven random placement.
        for spots in [false, true] {
            let (mut c, mut p, col) = duel(MODES[0].rules(), skills.clone());
            if spots {
                c.set_capsule_spots([vec![Vec3::new(30.0, 0.0, 0.0), Vec3::new(-30.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -30.0)], vec![Vec3::new(30.0, 0.0, 90.0)]]);
            }
            for _ in 0..(CAPSULE_RESPAWN_TICKS as usize + CAPSULE_EVERY_TICKS as usize) * 2 {
                c.step(&mut p, [0; 2], &col);
                c.fighters[0].health = c.fighters[0].max_health;
                c.fighters[1].health = c.fighters[1].max_health;
                assert!(c.capsules.is_empty(), "a capsule appeared (spots: {spots})");
                assert!(c.pending_capsules().is_empty());
            }
        }
        // The same field with capsules on does fill up (so the check above is not vacuous).
        let (mut c, mut p, col) = duel_with(Rules { start_hand: 0, ..Rules::default() }, skills, 3);
        for _ in 0..CAPSULE_EVERY_TICKS as usize * 2 {
            c.step(&mut p, [0; 2], &col);
        }
        assert!(!c.capsules.is_empty(), "with capsules on, the field fills");
    }

    #[test]
    fn the_four_skills_stay_in_their_slots_after_many_casts() {
        let Some(skills) = cache_skills() else { return };
        let (mut c, mut p, col) = duel(MODES[0].rules(), skills.clone());
        c.set_action_rows(crate::load_action_rows(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/skills.json")));
        let hand = [Some(3), Some(29), Some(240), Some(75)];
        let mut cast = [0u32; 4];
        for round in 0..30 {
            for slot in 0..4 {
                let before = c.fighters[0].aura_ticks;
                c.fighters[0].aura_ticks = 10 * AURA_TICKS_PER_POINT;
                c.fighters[1].health = c.fighters[1].max_health;
                k::press(&mut c, &mut p, &col, 0, slot);
                k::run(&mut c, &mut p, &col, 150);
                if c.fighters[0].aura_ticks < 10 * AURA_TICKS_PER_POINT || before != c.fighters[0].aura_ticks {
                    cast[slot] += 1;
                }
                assert_eq!(c.fighters[0].slots, hand, "after cast {round} of button {slot}");
                assert_eq!(c.fighters[0].spent, [0; 4], "no use is ever counted");
                c.fighters[1].health = c.fighters[1].max_health;
            }
        }
        assert!(cast.iter().all(|&n| n >= 25), "every button really cast: {cast:?}");
        assert_eq!(c.round, 1, "no round ended (the slots were not put back by a restart)");
    }

    #[test]
    fn infinite_uses_keeps_even_a_skill_with_a_use_count() {
        // A skill with 1 use would be gone after one cast in a normal match; the flag keeps it.
        let mut zap = k::def(7, crate::combat::Capsule::Attack, 1, &[(k::f::USES, 1)]);
        zap.name = "One-shot".into();
        for infinite in [false, true] {
            let (mut c, mut p, col) = k::setup(vec![zap.clone()], 80.0);
            c.set_rules(Rules { infinite_uses: infinite, ..k::TEST_RULES });
            c.round_ticks = COUNTDOWN_TICKS;
            c.fighters[0].level = 8;
            c.fighters[0].set_slot(0, Some(7));
            for _ in 0..3 {
                c.fighters[0].aura_ticks = 8 * AURA_TICKS_PER_POINT;
                k::press(&mut c, &mut p, &col, 0, 0);
                k::run(&mut c, &mut p, &col, 100);
            }
            assert_eq!(c.fighters[0].slots[0].is_some(), infinite, "infinite_uses = {infinite}");
        }
    }

    #[test]
    fn aura_refills_as_normal_up_to_ten() {
        let Some(skills) = cache_skills() else { return };
        let (mut m, mut mp, col) = duel(MODES[0].rules(), skills.clone());
        // The same fighters under the normal rules at Level 10: only the mode's own flags differ.
        let (mut n, mut np, _) = duel(Rules { start_level: 10, start_hand: 0, ..Rules::default() }, skills);
        for c in [&mut m, &mut n] {
            c.set_arsenal_kinds([Some(2); crate::combat::MAX_PLAYERS]);
            c.round_ticks = COUNTDOWN_TICKS;
            c.fighters[0].aura_ticks = 3 * AURA_TICKS_PER_POINT;
        }
        let mut prev = m.fighters[0].aura_ticks;
        for t in 0..600 {
            m.step(&mut mp, [0; 2], &col);
            n.step(&mut np, [0; 2], &col);
            assert_eq!(m.fighters[0].aura_ticks, n.fighters[0].aura_ticks, "tick {t}: same refill as a normal match");
            assert!(m.fighters[0].aura_ticks >= prev && m.fighters[0].aura_ticks <= 10 * AURA_TICKS_PER_POINT);
            prev = m.fighters[0].aura_ticks;
        }
        assert!(prev > 3 * AURA_TICKS_PER_POINT + AURA_TICKS_PER_POINT, "it did refill: {prev}");
        // ... and stops at the Level 10 cap.
        for _ in 0..6000 {
            m.step(&mut mp, [0; 2], &col);
        }
        assert_eq!(m.fighters[0].aura_ticks, 10 * AURA_TICKS_PER_POINT);
    }

    #[test]
    fn the_list_text_names_the_hand() {
        let Some(skills) = cache_skills() else { return };
        assert_eq!(hand_line(&MODES[0], &skills), "Hand: Psycho Knife  /  Power Shell  /  Dash  /  Rapid Cannon");
    }

    #[test]
    fn the_selector_steps_standard_then_every_mode_and_wraps() {
        assert_eq!(step(None, true), Some(0));
        assert_eq!(step(Some(0), false), None);
        assert_eq!(step(None, false), Some(MODES.len() - 1));
        assert_eq!(step(Some(MODES.len() - 1), true), None, "wraps to STANDARD");
        // A walk through the whole table visits every entry once and comes home.
        let mut m = None;
        let mut seen = Vec::new();
        for _ in 0..=MODES.len() {
            m = step(m, true);
            seen.push(m);
        }
        assert_eq!(seen.last(), Some(&None));
        assert_eq!(seen.iter().flatten().count(), MODES.len());
        // An out-of-range value is treated as STANDARD, never a panic.
        assert_eq!(step(Some(MODES.len() + 5), true), Some(0));
        assert_eq!(label(None), "STANDARD");
        assert_eq!(label(Some(0)), "CLOSE QUARTERS");
        assert_eq!(blurb(Some(0)), MODES[0].blurb);
    }

    #[test]
    fn lobby_fields_round_trip_and_catch_a_different_table() {
        for mode in std::iter::once(None).chain((0..MODES.len()).filter(|&i| MODES[i].online).map(Some)) {
            let (id, hash) = lobby_fields(mode);
            assert_eq!(resolve_lobby(&id, hash), Ok(mode), "{mode:?}");
        }
        // Different modes are different Rules: their fingerprints differ.
        assert_ne!(rules_fingerprint(None), rules_fingerprint(Some(0)));
        // A wrong hash, an unknown id, and STANDARD with a mode's hash are all refused.
        let (id, hash) = lobby_fields(Some(0));
        assert!(resolve_lobby(&id, hash ^ 1).is_err());
        assert!(resolve_lobby("nope", hash).is_err());
        assert!(resolve_lobby("", hash).is_err());
        // The handshake's table hash moves with any entry change (name, hand or a rule), so builds with other tables don't connect.
        let base = table_hash_of(MODES);
        assert_eq!(base, table_hash());
        let mut t = MODES.to_vec();
        t[0].hand[0] += 1;
        assert_ne!(table_hash_of(&t), base, "the hand");
        let mut t = MODES.to_vec();
        t[0].rules.start_level += 1;
        assert_ne!(table_hash_of(&t), base, "a rule");
        let mut t = MODES.to_vec();
        t.push(MODES[0]);
        assert_ne!(table_hash_of(&t), base, "an extra mode");
        assert_eq!(by_id("close_quarters"), Some(0));
        assert_eq!(by_id("0"), None, "an index is not an id");
    }

    #[test]
    fn two_copies_started_with_different_modes_refuse_at_the_handshake() {
        use crate::net::{NetSession, Status};
        let (mut h, mut c) = NetSession::loopback_pair(5);
        h.mix_setup(rules_fingerprint(Some(0)));
        c.mix_setup(rules_fingerprint(None));
        for _ in 0..4 {
            h.poll();
            c.poll();
        }
        assert!(matches!(h.status, Status::Closed(_)) || matches!(c.status, Status::Closed(_)), "different mode: no connection");
        // The same mode on both: they connect.
        let (mut h, mut c) = NetSession::loopback_pair(5);
        h.mix_setup(rules_fingerprint(Some(0)));
        c.mix_setup(rules_fingerprint(Some(0)));
        for _ in 0..4 {
            h.poll();
            c.poll();
        }
        assert_eq!((h.status.clone(), c.status.clone()), (Status::Connected, Status::Connected));
    }

    #[test]
    fn rules_are_part_of_the_checksum_the_lockstep_compares() {
        // Two sims identical in everything but the mode's Rules hash differently (Combat::hash_state includes `rules`).
        let mk = |rules: Rules| {
            let mut c = Combat::with_roster(Vec::new(), Roster::duel(), std::array::from_fn(|_| vec![3; 30]));
            c.set_rules(rules);
            let m = crate::sim::Match::with_spawns(&[Vec3::ZERO, Vec3::new(0.0, 0.0, 60.0)], c);
            m.checksum()
        };
        let cq = MODES[0].rules();
        assert_eq!(mk(cq), mk(cq), "same Rules, same checksum");
        assert_ne!(mk(cq), mk(Rules::default()), "a different mode is caught by the per-second checksum at the latest");
    }
}
