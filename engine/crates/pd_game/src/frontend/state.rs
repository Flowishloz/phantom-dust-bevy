//! App-wide screen state and the setup a match starts from.

use crate::combat::{Combat, CombatEvent, FriendlyFire, Outcome, RuleSet, Rules, MAX_PLAYERS};
use bevy::prelude::*;

#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum AppState {
    #[default]
    Title,
    Setup,
    Arsenal,
    Match,
    Result,
    /// Online versus: host / join over LAN, then the lobby
    /// (`frontend/online.rs`, docs/ONLINE-PLAN.md).
    Online,
}

/// Everything the versus setup screen chose beyond the player list; read
/// once when a match starts (`main.rs::enter_match`). The players themselves
/// (character, costume colour, arsenal, team, seat) go through
/// `seats::RequestedPlayers`; `chars` / `arsenals` here are the first two of
/// them, kept for the code that still reads the old two-wide setup (the
/// voices in `sfx.rs`, the legacy launch).
#[derive(Resource, Clone, Debug)]
pub struct MatchSetup {
    pub chars: [String; 2],
    pub arsenals: [Vec<i16>; 2],
    pub stage: String,
    pub time_limit_ticks: u32,
    /// Take the time limit of the game rule in force instead
    /// (`Combat::use_rule_time_limit`); `time_limit_ticks` is then ignored.
    pub time_by_rule: bool,
    /// Wins needed to end the match; `None` = rounds restart forever.
    /// Default 1, like the original's single battle.
    pub rounds_to_win: Option<u32>,
    /// Level (aura limit) every player starts each round at, kept inside the
    /// rule's range by the sim. The original's Standard Battle starts at 0
    /// ("All participants start at Level 0 with 20 Health",
    /// `Menu/vsmenu_en.ssb`).
    pub start_level: i32,
    /// Skills dealt onto the four buttons at the start of a round (the
    /// original deals 4 at random); 0 = start with empty buttons.
    pub start_hand: usize,
    /// The original's game rule in force: an index into
    /// `arsenal::RULE_NAMES` (0 Standard, 3 Puny, 4 Heavy, 6 Single School,
    /// 7 Highlander, 8 Puny Only, 9 Heavy Only are offered).
    pub rule: usize,
    /// Whether attacks hurt the attacker's teammates (2v2 option; default
    /// off, DECISIONS 2026-10-01).
    pub friendly_fire: FriendlyFire,
    /// Each player's declared "Number of Schools" (their arsenal's, when it
    /// has one): sets the aura recovery multiplier.
    pub arsenal_kinds: [Option<u8>; MAX_PLAYERS],
    /// The game mode played (an index into `modes::MODES`), None = the normal versus. A mode brings its own
    /// rules and fixed hand: `rule`, `start_level` and `start_hand` above are then ignored.
    pub mode: Option<usize>,
    /// Prop Hunt's round limit and round time (the setup's ROUNDS / ROUND TIME rows when Prop Hunt is the mode);
    /// unused by every other mode.
    pub prop_hunt: crate::prophunt::Config,
}

impl Default for MatchSetup {
    fn default() -> Self {
        MatchSetup {
            chars: ["pc00a".into(), "pc01a".into()],
            arsenals: [Vec::new(), Vec::new()],
            stage: crate::DEFAULT_STAGE.into(),
            time_limit_ticks: crate::combat::TIME_LIMIT_TICKS,
            time_by_rule: false,
            rounds_to_win: ROUNDS_OPTIONS[DEFAULT_ROUNDS_IDX].1,
            start_level: LEVEL_OPTIONS[DEFAULT_LEVEL_IDX],
            start_hand: HAND_OPTIONS[DEFAULT_HAND_IDX],
            rule: 0,
            friendly_fire: FriendlyFire::Off,
            arsenal_kinds: [None; MAX_PLAYERS],
            mode: None,
            prop_hunt: crate::prophunt::Config::default(),
        }
    }
}

impl MatchSetup {
    /// Puts every choice of this setup into a fresh match's rules: the game
    /// rule (Health, Level range), starting Level and hand, the time limit,
    /// the friendly-fire policy and each arsenal's declared kind. Everything
    /// is set explicitly, so nothing a previous setup chose can leak into the
    /// match (`main.rs::enter_match`, and the matrix tests below).
    /// The sim's `Rules` this setup plays under: the mode's preset, or the
    /// setup's own game rule / starting Level / hand.
    pub fn resolved_rules(&self) -> Rules {
        match self.mode.and_then(super::modes::get) {
            Some(mode) => mode.rules(),
            None => Rules { start_level: self.start_level, start_hand: self.start_hand, set: RuleSet::new(self.rule), ..Rules::default() },
        }
    }

    /// Starts what the mode adds on top of `Combat` in a freshly built match: Prop Hunt's rounds and roles. Every
    /// other mode adds nothing.
    pub fn start_mode(&self, m: &mut crate::sim::Match) {
        if super::modes::is_prop_hunt(self.mode) {
            m.start_prop_hunt(self.prop_hunt);
        }
    }

    pub fn apply_to_combat(&self, combat: &mut Combat) {
        combat.time_limit = self.time_limit_ticks;
        combat.set_rules(self.resolved_rules());
        if self.time_by_rule {
            combat.use_rule_time_limit();
        }
        combat.friendly_fire = self.friendly_fire;
        combat.set_arsenal_kinds(self.arsenal_kinds);
    }
}

/// "Time limit" row: the rule's own first (every offered rule is 10 minutes),
/// then 5 / 10 / 15 / no limit. The value is the limit in sim ticks; the
/// first entry is the "by rule" marker.
pub const BY_RULE: u32 = u32::MAX;
pub const TIME_OPTIONS: [(&str, u32); 5] = [("By rule", BY_RULE), ("5 min", 5 * 60 * 60), ("10 min", 10 * 60 * 60), ("15 min", 15 * 60 * 60), ("No limit", 0)];
pub const DEFAULT_TIME_IDX: usize = 0; // the original's rule table: 10 minutes for every rule offered (DECISIONS 2026-10-01)
pub const ROUNDS_OPTIONS: [(&str, Option<u32>); 4] = [("1", Some(1)), ("2", Some(2)), ("3", Some(3)), ("Endless", None)];
pub const DEFAULT_ROUNDS_IDX: usize = 0;
pub const LEVEL_OPTIONS: [i32; 6] = [0, 1, 2, 3, 4, 5];
pub const DEFAULT_LEVEL_IDX: usize = 0;
pub const HAND_OPTIONS: [usize; 2] = [0, 4];
pub const DEFAULT_HAND_IDX: usize = 1;

/// The game rules the setup offers: Standard, then Puny / Heavy / Single
/// School / Highlander / Puny Only / Heavy Only (DECISIONS 2026-10-01). The
/// value is the rule id (`arsenal::RULE_NAMES` index).
pub const GAME_RULES: [usize; 7] = [0, 3, 4, 6, 7, 8, 9];

/// Every registered skill (for the arsenal builder) plus their catalog text.
#[derive(Resource, Clone, Default)]
pub struct SkillCatalog {
    pub skills: Vec<crate::combat::SkillDef>,
    pub notes: std::collections::HashMap<i16, String>,
}

/// Characters, stages and starter arsenals found in the cache; computed once
/// at startup (`frontend::discovery`).
#[derive(Resource, Clone, Default)]
pub struct GameData {
    pub characters: Vec<super::discovery::CharacterEntry>,
    pub stages: Vec<super::discovery::StageEntry>,
    pub builtins: Vec<super::discovery::BuiltinArsenal>,
}

/// Saved arsenals loaded from `userdata/arsenals.json`, refreshed whenever
/// the builder saves or deletes one.
#[derive(Resource, Clone, Default)]
pub struct SavedArsenals(pub Vec<super::arsenals::SavedArsenal>);

/// Marks the entities `main.rs::spawn_match_scene` created (stage, the
/// characters, the cameras, lights, music), so a new match can despawn the
/// old scene before spawning the next one.
#[derive(Component)]
pub struct MatchScene;

/// Set once the 3D scene, HUD and fx have been built for the first time;
/// they're reused after that (only the stage/character models are swapped).
#[derive(Resource, Default)]
pub struct SceneReady(pub bool);

/// Whether the match is over: the round's winning team has reached
/// `target` round wins, or the round was drawn and the target is a single
/// round (the original's single battle ends in a draw result). Only after
/// the round-end banner has run its course. A drawn round of a longer match
/// is simply replayed. Free-for-all players are each their own team.
pub fn match_ended(combat: &Combat, target: u32) -> bool {
    match (combat.round_over, combat.round_outcome()) {
        (Some((_, left)), Some(Outcome::Winner { team, .. })) => left <= 2 && combat.team_wins(team) >= target,
        (Some((_, left)), Some(Outcome::Draw)) => left <= 2 && target == 1,
        _ => false,
    }
}

/// "Fun stats" for the result screen (goal 5 of the UI polish brief):
/// counted render-side from sim state polled once a frame, no sim changes.
/// Accumulates across every round of a match (`main.rs` resets it in
/// `OnEnter(AppState::Match)`, once per match/rematch, not per round). Four
/// wide: damage and knock-downs are credited to the attacker named by the
/// combat events (`CombatEvent::Hit`), not guessed from "the other player".
#[derive(Resource, Clone, Debug, Default)]
pub struct MatchStats {
    /// Health the victims lost to this player's hits (damage from poison,
    /// self-cost skills and the powerless drain has no attacker and counts
    /// for nobody).
    pub damage_dealt: [i32; MAX_PLAYERS],
    pub skills_used: [u32; MAX_PLAYERS],
    /// Times this player's hit was the last before someone went down.
    pub knockdowns: [u32; MAX_PLAYERS],
    pub longest_combo: [u32; MAX_PLAYERS],
    skill_counts: [std::collections::HashMap<i16, u32>; MAX_PLAYERS],
    /// What the previous frame looked like, so the next one can see what
    /// changed. Part of the stats so one reset (a new match) clears it all.
    track: Tracker,
}

impl MatchStats {
    /// The skill id this player cast the most, if they cast anything.
    pub fn favourite(&self, player: usize) -> Option<i16> {
        self.skill_counts[player].iter().max_by_key(|&(&id, &n)| (n, std::cmp::Reverse(id))).map(|(&id, _)| id)
    }
}

#[derive(Clone, Debug, Default)]
struct Tracker {
    prev: [PlayerSnapshot; MAX_PLAYERS],
    /// Who hit each player last (cleared when their health is refilled).
    last_attacker: [Option<usize>; MAX_PLAYERS],
    /// Each attacker's last hit: the round tick and the victim.
    last_hit: [Option<(u32, usize)>; MAX_PLAYERS],
    combo: [u32; MAX_PLAYERS],
    /// Highest combat event number already counted.
    seen_event: u32,
}

/// One player's polled fields, snapshotted each frame so [`observe`] can see
/// what changed since the last one. `Default` matches a fresh `Fighter`
/// (health 0 reads as "just reset", not "took full damage" -- see `observe`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerSnapshot {
    pub health: i32,
    pub down: bool,
    pub cast_seq: u16,
    pub cast_skill: i16,
}

/// Ticks between two hits on the same victim that still count as one combo
/// (2 s at the sim's 60 Hz -- long enough for a juggle's travel time, short
/// enough that two unrelated pokes a round apart don't chain).
const COMBO_WINDOW_TICKS: u32 = 120;

/// Update `stats` from how `cur` differs from the last frame's snapshot, and
/// from the combat `events` since then (`(number, event)` as `Combat::events`
/// holds them). `active` marks the players in the match; `round_ticks` times
/// the combo window (a value lower than the last hit's -- a new round
/// starting -- can only end a combo, never extend one, because the
/// subtraction saturates). Pure and unit-tested below; the only Bevy-facing
/// wrapper is `track_stats`.
pub fn observe(stats: &mut MatchStats, active: [bool; MAX_PLAYERS], round_ticks: u32, events: &[(u32, CombatEvent)], cur: [PlayerSnapshot; MAX_PLAYERS]) {
    let t = &mut stats.track;
    // A new match numbers its events from 1 again.
    if events.last().is_some_and(|e| e.0 < t.seen_event) {
        t.seen_event = 0;
    }
    let fresh: Vec<CombatEvent> = events.iter().filter(|(n, _)| *n > t.seen_event).map(|(_, e)| *e).collect();
    if let Some(last) = events.last() {
        t.seen_event = t.seen_event.max(last.0);
    }
    // Hits since the last frame: the attacker of a victim's latest hit gets
    // the credit for what that victim lost. Hitting yourself credits nobody.
    let mut hit_by: [Option<usize>; MAX_PLAYERS] = [None; MAX_PLAYERS];
    for e in &fresh {
        if let CombatEvent::Hit { attacker, victim, .. } = *e {
            if attacker < MAX_PLAYERS && victim < MAX_PLAYERS && attacker != victim {
                hit_by[victim] = Some(attacker);
                t.last_attacker[victim] = Some(attacker);
                let chained = t.last_hit[attacker].is_some_and(|(at, v)| v == victim && round_ticks.saturating_sub(at) <= COMBO_WINDOW_TICKS);
                t.combo[attacker] = if chained { t.combo[attacker] + 1 } else { 1 };
                stats.longest_combo[attacker] = stats.longest_combo[attacker].max(t.combo[attacker]);
                t.last_hit[attacker] = Some((round_ticks, victim));
            }
        }
    }
    for i in (0..MAX_PLAYERS).filter(|&i| active[i]) {
        let prev = t.prev[i];
        // A cast: `cast_seq` changed and isn't the "just reset" 0.
        if cur[i].cast_seq != prev.cast_seq && cur[i].cast_seq != 0 {
            stats.skills_used[i] += 1;
            *stats.skill_counts[i].entry(cur[i].cast_skill).or_insert(0) += 1;
        }
        // Health dropped (and isn't "just reset" from 0): the hit's owner dealt it.
        if prev.health > 0 && cur[i].health < prev.health {
            if let Some(a) = hit_by[i] {
                stats.damage_dealt[a] += prev.health - cur[i].health;
            }
        }
        // A fresh knock-down (not still down from last frame): the last
        // attacker scores it.
        if cur[i].down && !prev.down {
            if let Some(a) = t.last_attacker[i] {
                stats.knockdowns[a] += 1;
            }
        }
        // Health refilled (a new round): forget who hit this player.
        if cur[i].health > prev.health && prev.health <= 0 {
            t.last_attacker[i] = None;
        }
    }
    t.prev = cur;
}

/// Bevy wrapper for [`observe`]: polls `World`'s sim state once a frame
/// (`in_state(Match)`, registered in `main.rs`). A fresh match's `Fighter`s
/// start at full health/`Action::Ready`/`cast_seq` 0, which reads as "just
/// reset" against any stale previous snapshot -- see `observe` -- and
/// `MatchStats` itself (previous frame included) is reset by `main.rs` on
/// `OnEnter(Match)`.
pub fn track_stats(world: Res<crate::World>, mut stats: ResMut<MatchStats>) {
    let c = &world.0.combat;
    let cur = std::array::from_fn(|i| PlayerSnapshot { health: c.fighters[i].health, down: c.fighters[i].action == crate::combat::Action::Down, cast_seq: c.fighters[i].cast_seq, cast_skill: c.fighters[i].cast_skill });
    observe(&mut stats, c.roster.active, c.round_ticks, &c.events, cur);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Roster;

    #[test]
    fn match_rules_default_to_the_originals() {
        let m = MatchSetup::default();
        assert_eq!(m.rounds_to_win, Some(1));
        assert_eq!(m.start_level, 0);
        assert_eq!(m.start_hand, 4);
        assert_eq!(m.rule, 0);
        assert_eq!(m.friendly_fire, FriendlyFire::Off, "friendly fire is a setup option that defaults off");
        assert_eq!(TIME_OPTIONS[DEFAULT_TIME_IDX].1, BY_RULE);
        assert_eq!(RuleSet::STANDARD.time_limit_ticks(), 10 * 3600);
    }

    /// A harness for [`observe`] so each test only states what changed.
    struct Watch {
        stats: MatchStats,
        active: [bool; MAX_PLAYERS],
        cur: [PlayerSnapshot; MAX_PLAYERS],
        events: Vec<(u32, CombatEvent)>,
        tick: u32,
    }

    impl Watch {
        fn new(players: usize) -> Self {
            let cur = [PlayerSnapshot { health: 20, ..default() }; MAX_PLAYERS];
            let mut stats = MatchStats::default();
            stats.track.prev = cur;
            Watch { stats, active: std::array::from_fn(|i| i < players), cur, events: Vec::new(), tick: 0 }
        }

        /// Advance `ticks` and apply one frame (everything unchanged except
        /// what the closure sets).
        fn tick(&mut self, ticks: u32, f: impl FnOnce(&mut [PlayerSnapshot; MAX_PLAYERS])) {
            self.tick += ticks;
            f(&mut self.cur);
            observe(&mut self.stats, self.active, self.tick, &self.events, self.cur);
        }

        fn hit(&mut self, attacker: usize, victim: usize) {
            let n = self.events.last().map_or(0, |e| e.0) + 1;
            self.events.push((n, CombatEvent::Hit { attacker, victim, skill: 1 }));
        }
    }

    #[test]
    fn a_cast_counts_for_the_caster_and_tallies_its_skill() {
        let mut w = Watch::new(2);
        w.tick(1, |c| {
            c[0].cast_seq = 1;
            c[0].cast_skill = 7;
        });
        w.tick(1, |c| {
            c[0].cast_seq = 2;
            c[0].cast_skill = 7;
        });
        assert_eq!(w.stats.skills_used[..2], [2, 0]);
        assert_eq!(w.stats.favourite(0), Some(7));
        assert_eq!(w.stats.favourite(1), None);
    }

    #[test]
    fn a_fresh_match_cast_seq_reset_to_zero_is_not_counted() {
        let mut w = Watch::new(2);
        w.stats.track.prev[0].cast_seq = 9; // as if a previous match left off mid-cast
        w.tick(1, |c| c[0].cast_seq = 0); // the new Combat's fighter starts at 0
        assert_eq!(w.stats.skills_used, [0; MAX_PLAYERS]);
    }

    #[test]
    fn damage_is_credited_to_the_attacker_the_event_names() {
        let mut w = Watch::new(4);
        // Player 3 hits player 1 for 5 while player 0 is also on the field:
        // the old "the other player" guess would have credited slot 0.
        w.hit(3, 1);
        w.tick(1, |c| c[1].health = 15);
        assert_eq!(w.stats.damage_dealt, [0, 0, 0, 5]);
        w.hit(0, 2);
        w.tick(1, |c| c[2].health = 12);
        assert_eq!(w.stats.damage_dealt, [8, 0, 0, 5]);
    }

    #[test]
    fn health_lost_with_no_hit_credits_nobody() {
        let mut w = Watch::new(2);
        w.tick(1, |c| c[0].health = 18); // poison, a health-cost skill or the powerless drain
        assert_eq!(w.stats.damage_dealt, [0; MAX_PLAYERS]);
        // And a hit on yourself (a mine in your own face) is not damage dealt either.
        w.hit(0, 0);
        w.tick(1, |c| c[0].health = 16);
        assert_eq!(w.stats.damage_dealt, [0; MAX_PLAYERS]);
    }

    #[test]
    fn a_ko_reset_to_full_health_is_not_counted_as_damage() {
        let mut w = Watch::new(2);
        w.stats.track.prev[1].health = 0; // just knocked out
        w.tick(1, |c| c[1].health = 20); // the new round's fresh fighter
        assert_eq!(w.stats.damage_dealt, [0; MAX_PLAYERS]);
    }

    #[test]
    fn knockdowns_go_to_the_last_attacker_once_per_fall() {
        let mut w = Watch::new(3);
        w.hit(2, 1);
        w.tick(1, |c| {
            c[1].health = 10;
            c[1].down = true;
        });
        w.tick(1, |_| {}); // still down: not counted again
        w.tick(1, |c| c[1].down = false); // gets up
        w.tick(1, |c| c[1].down = true); // knocked down a second time (same last attacker)
        assert_eq!(w.stats.knockdowns[..3], [0, 0, 2]);
    }

    #[test]
    fn a_fall_with_no_attacker_is_nobodys_knockdown() {
        let mut w = Watch::new(2);
        w.tick(1, |c| c[0].down = true); // fell off the stage
        assert_eq!(w.stats.knockdowns, [0; MAX_PLAYERS]);
    }

    #[test]
    fn a_new_round_forgets_who_hit_whom() {
        let mut w = Watch::new(2);
        w.hit(0, 1);
        w.tick(1, |c| {
            c[1].health = 0;
            c[1].down = true;
        });
        assert_eq!(w.stats.knockdowns[0], 1);
        w.tick(1, |c| {
            c[1].health = 20; // next round
            c[1].down = false;
        });
        w.tick(1, |c| c[1].down = true); // falls with nobody to thank
        assert_eq!(w.stats.knockdowns[0], 1);
    }

    #[test]
    fn consecutive_hits_on_one_victim_within_the_window_extend_the_combo() {
        let mut w = Watch::new(3);
        w.hit(0, 1);
        w.tick(1, |c| c[1].health = 18); // hit 1: combo of 1
        w.hit(0, 1);
        w.tick(COMBO_WINDOW_TICKS, |c| c[1].health = 16); // hit 2, right at the edge of the window
        w.hit(0, 1);
        w.tick(1, |c| c[1].health = 14); // hit 3
        assert_eq!(w.stats.longest_combo[..3], [3, 0, 0]);
    }

    #[test]
    fn a_gap_or_a_different_victim_starts_a_new_combo() {
        let mut w = Watch::new(3);
        w.hit(0, 1);
        w.tick(1, |c| c[1].health = 18);
        w.hit(0, 1);
        w.tick(1, |c| c[1].health = 16); // combo of 2
        w.hit(0, 2);
        w.tick(1, |c| c[2].health = 18); // another victim: a fresh combo of 1
        w.hit(0, 1);
        w.tick(COMBO_WINDOW_TICKS + 1, |c| c[1].health = 14); // too late: a fresh combo of 1
        assert_eq!(w.stats.longest_combo[0], 2, "the earlier combo of 2 is still the longest seen");
    }

    #[test]
    fn events_are_counted_once_and_a_new_matchs_numbering_restarts() {
        let mut w = Watch::new(2);
        w.hit(0, 1);
        w.tick(1, |c| c[1].health = 18);
        w.tick(1, |c| c[1].health = 16); // no new event: the second drop has no owner
        assert_eq!(w.stats.damage_dealt[0], 2, "the same event is not credited twice");
        // A new match: events start from 1 again (and the stats were reset).
        w.stats = MatchStats::default();
        w.stats.track.prev = [PlayerSnapshot { health: 20, ..default() }; MAX_PLAYERS];
        w.events = vec![(1, CombatEvent::Hit { attacker: 1, victim: 0, skill: 1 })];
        w.cur = [PlayerSnapshot { health: 20, ..default() }; MAX_PLAYERS];
        w.tick(1, |c| c[0].health = 15);
        assert_eq!(w.stats.damage_dealt[1], 5);
    }

    // --- the setup -> combat plumbing and the matrix of rules ------------

    fn game(roster: Roster) -> Combat {
        Combat::with_roster(Vec::new(), roster, std::array::from_fn(|_| vec![crate::combat::AURA_PARTICLE; 30]))
    }

    fn setup_for(rule: usize) -> MatchSetup {
        MatchSetup { rule, time_by_rule: true, ..MatchSetup::default() }
    }

    /// Every offered rule reaches the sim as that rule's Health, Level range
    /// and time limit, and a setup applied to a used Combat overrides what
    /// the last one set (nothing leaks into the next match).
    #[test]
    fn each_offered_rule_reaches_the_sim_and_nothing_leaks_between_setups() {
        for &rule in &GAME_RULES {
            let mut c = game(Roster::with_teams(&[0, 0, 1, 1]));
            setup_for(rule).apply_to_combat(&mut c);
            let row = RuleSet::new(rule).row(0);
            assert_eq!(c.rules.set, RuleSet::new(rule), "rule {rule}");
            assert_eq!(c.time_limit, RuleSet::new(rule).time_limit_ticks(), "rule {rule}: time by rule");
            for i in 0..4 {
                assert_eq!(c.fighters[i].health, row.health, "rule {rule} slot {i}");
                assert_eq!(c.fighters[i].max_health, row.health);
                assert!((row.level_min..=row.level_max).contains(&c.fighters[i].level), "rule {rule}: level inside {}..={}", row.level_min, row.level_max);
            }
            // The same Combat re-set up for Standard with different options is
            // exactly a Standard match again: no Puny/Heavy Health or Level left.
            let std = MatchSetup { rule: 0, time_by_rule: false, time_limit_ticks: 5 * 3600, start_level: 2, friendly_fire: FriendlyFire::On, arsenal_kinds: [Some(1), None, None, None], ..MatchSetup::default() };
            std.apply_to_combat(&mut c);
            assert_eq!(c.time_limit, 5 * 3600);
            assert_eq!(c.friendly_fire, FriendlyFire::On);
            for i in 0..4 {
                assert_eq!((c.fighters[i].health, c.fighters[i].level), (20, 2), "rule {rule} left nothing behind");
            }
            assert_eq!(c.fighters[0].aura_handicap, crate::arsenal::aura_handicap(1), "a declared kind sets the aura handicap");
            assert_eq!(c.fighters[1].aura_handicap, 1.0);
        }
        // Puny caps the starting Level at 5, Heavy starts at 5 or more.
        let mut puny = game(Roster::duel());
        MatchSetup { rule: 3, start_level: 5, ..MatchSetup::default() }.apply_to_combat(&mut puny);
        assert_eq!((puny.fighters[0].health, puny.fighters[0].level), (10, 5));
        let mut heavy = game(Roster::duel());
        MatchSetup { rule: 4, start_level: 0, ..MatchSetup::default() }.apply_to_combat(&mut heavy);
        assert_eq!((heavy.fighters[0].health, heavy.fighters[0].level), (40, 5), "Heavy lifts the starting Level to its minimum");
    }

    #[test]
    fn friendly_fire_is_set_from_the_setup_not_left_to_the_sim_default() {
        let mut c = game(Roster::with_teams(&[0, 0, 1, 1]));
        assert_eq!(c.friendly_fire, FriendlyFire::On, "the sim's own default");
        MatchSetup::default().apply_to_combat(&mut c);
        assert_eq!(c.friendly_fire, FriendlyFire::Off, "the setup's default is off");
    }

    /// Start -> finish -> rematch for every offered rule: a 2v2 ends when a
    /// team has the only players standing, a rematch from the same setup
    /// starts clean, and the result reads placements and team wins.
    #[test]
    fn a_two_v_two_finishes_and_a_rematch_starts_clean_for_every_rule() {
        for &rule in &GAME_RULES {
            let setup = setup_for(rule);
            let mut c = game(Roster::with_teams(&[0, 0, 1, 1]));
            setup.apply_to_combat(&mut c);
            assert!(!match_ended(&c, 1));
            c.change_health(2, -999);
            assert!(c.round_over.is_none(), "one of team 1 still stands");
            c.round_ticks += 30;
            c.change_health(3, -999);
            assert_eq!(c.round_outcome(), Some(Outcome::Winner { slot: 0, team: 0 }), "rule {rule}");
            assert_eq!(c.team_wins(0), 1);
            assert_eq!(c.team_wins(1), 0);
            c.round_over = c.round_over.map(|(w, _)| (w, 1));
            assert!(match_ended(&c, 1), "rule {rule}: the winning team has its round");
            let p = c.placements();
            assert_eq!((p[0], p[1]), (1, 1), "the standing team shares first");
            assert!(p[2] >= 3 && p[3] >= 3, "the knocked-out team places after");
            // Rematch: a fresh Combat from the same setup is a clean start.
            let mut again = game(Roster::with_teams(&[0, 0, 1, 1]));
            setup.apply_to_combat(&mut again);
            assert_eq!(again.round_outcome(), None);
            assert_eq!(again.wins, [0; MAX_PLAYERS]);
            assert!(!match_ended(&again, 1));
            assert_eq!(again.fighters[2].health, RuleSet::new(rule).row(2).health);
        }
    }

    #[test]
    fn a_free_for_all_of_three_finishes_with_ranked_placements() {
        let mut c = game(Roster::free_for_all(3));
        setup_for(0).apply_to_combat(&mut c);
        c.round_ticks = 400;
        c.change_health(1, -999);
        c.round_ticks = 900;
        c.change_health(2, -999);
        assert_eq!(c.round_outcome(), Some(Outcome::Winner { slot: 0, team: 0 }));
        assert_eq!(c.placements(), [1, 3, 2, 0], "the survivor first, then the later knock-out; the fourth slot is not playing");
    }

    #[test]
    fn a_match_ends_on_team_wins_not_slot_wins() {
        let mut c = game(Roster::with_teams(&[0, 0, 1, 1]));
        c.wins = [1, 0, 1, 1];
        c.round_over = Some((2, 1)); // slot 2 (team 1) survived and its partner (slot 3) already had a win: team 1 has 2
        assert!(match_ended(&c, 2), "team 1's score is the sum over its members");
        assert!(!match_ended(&c, 3));
        c.round_over = Some((2, 5));
        assert!(!match_ended(&c, 2), "the banner has not finished");
        c.round_over = None;
        assert!(!match_ended(&c, 1));
        // Free-for-all: each player is their own team (slot 3 here).
        let mut f = game(Roster::free_for_all(4));
        f.wins = [0, 0, 0, 2];
        f.round_over = Some((3, 1));
        assert!(match_ended(&f, 2) && !match_ended(&f, 3));
    }

    #[test]
    fn a_drawn_single_battle_ends_as_a_draw_but_a_longer_match_replays_the_round() {
        let mut c = game(Roster::duel());
        c.round_over = Some((crate::combat::DRAW, 1));
        assert!(match_ended(&c, 1), "the original's single battle ends in a draw result");
        assert!(!match_ended(&c, 2), "first to two: the drawn round is replayed");
        c.round_over = Some((crate::combat::DRAW, 50));
        assert!(!match_ended(&c, 1), "not before the banner has run");
    }
}
