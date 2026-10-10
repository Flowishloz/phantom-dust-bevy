//! Prop Hunt (Laurence, 2026-10-07): a local 2-4 player hide-and-seek mode played in rounds.
//!
//! One player is the SEEKER, everyone else HIDES. Each round the seeker is the next player in seat order. The rules
//! (Laurence's words in quotes, docs/DESIGN-PROPHUNT.md has the full brief and every choice made beyond it):
//!
//! - "The seeker gets the move psycho knife and 10 attempts to use it (infinite mana, 20 life, each knife use costs 2
//!   life)." The knife sits on button X (slot 0); its Aura is refilled every tick; each cast costs 2 Health; the 11th
//!   press does nothing.
//! - A knife hit on a hider catches them (one hit): they turn back into their fighter and play the knock-down, then lie
//!   there for the rest of the round. The seeker scores one kill per catch.
//! - "Hiders will be able to click X on a prop to become it, B to freeze themselves in place." X picks the nearest
//!   intact stage object in reach that is small enough to be a believable disguise (`props::nearest_disguise`); X again
//!   near another object switches. B toggles freeze: no movement, facing locked. Hiders cannot cast anything.
//! - "The seeker has a 12 second countdown before they can move." During it the seeker cannot move, turn, cast or see
//!   (the renderer covers their view). When it ends the seeker's pad rumbles and their view becomes the whole window;
//!   the hiders' views go black ("they can still move blind").
//! - "The round is time based": the hunt clock (2:00 by default, a setup option) starts when the seeker is released. A
//!   round ends when the clock runs out, every hider is caught, or the seeker has used all ten knives / has no Health.
//! - "First to get 5 kills wins." No round limit (Laurence 2026-10-08: "No round limit, play to 5"): the match ends at
//!   the end of the round in which someone reaches 5 kills. Only the round's seeker scores, so two players can never
//!   reach 5 in the same round and there is no draw.
//!
//! Part of the deterministic simulation: it lives in `sim::Match::prophunt` and is driven only by the players' `Input`s
//! and the sim's own state (`Match::step` calls [`PropHunt::filter_inputs`] before the tick and
//! [`PropHunt::after_step`] after it). It reads no time, no randomness and nothing of the renderer. A match without it
//! (`Match::prophunt == None`, every other mode) runs exactly as before: the only change to `Combat` is the
//! `rounds_by_mode` flag (default off) that hands round endings to this module.

use crate::combat::{Action, Combat, CombatEvent, AURA_TICKS_PER_POINT, COUNTDOWN_TICKS, MAX_PLAYERS, SLOTS};
use crate::sim::{Input, Player, HZ};

/// Psycho Knife (`skills.json` id 3, the same id `frontend::modes::skill::PSYCHO_KNIFE` names).
pub const PSYCHO_KNIFE: i16 = 3;
/// "10 attempts to use it".
pub const KNIFE_ATTEMPTS: u32 = 10;
/// "each knife use costs 2 life".
pub const KNIFE_HEALTH_COST: i32 = 2;
/// "20 life".
pub const SEEKER_HEALTH: i32 = 20;
/// The seeker's Level (Aura limit). "Infinite mana" is the Aura refilled to this every tick, so it only has to cover
/// the knife's cost; 10 is the highest Level of the Standard rule. UNCONFIRMED value (any cost-covering Level plays the same).
pub const SEEKER_LEVEL: i32 = 10;
/// The hiders' head start: 20 s (Laurence 2026-10-08, "change hider hiding time to 20 seconds"; was 12).
pub const SEEKER_COUNTDOWN_TICKS: u32 = 20 * HZ as u32;
/// How long a hider's line says "nothing in reach" after a disguise press found no object.
pub const NOTHING_IN_REACH_TICKS: u16 = 90;
/// "First to get 5 kills wins": the default of the setup's KILLS TO WIN row (Laurence 2026-10-08: "make the kills to win
/// settable in settings").
pub const KILLS_TO_WIN: u32 = 5;
/// The KILLS TO WIN row's choices (setup page 2). UNCONFIRMED list; 5 is Laurence's default.
pub const KILLS_OPTIONS: [u32; 8] = [1, 2, 3, 4, 5, 6, 8, 10];
/// "Each round is 2 minutes" by default.
pub const DEFAULT_ROUND_SECS: u32 = 120;
/// The ROUND TIME row's choices in seconds (setup page 2). UNCONFIRMED list; 2:00 is Laurence's default.
pub const ROUND_TIME_OPTIONS: [u32; 5] = [60, 90, 120, 180, 300];
/// How long the between-rounds card (standings, the next seeker) stays up before the next round starts. UNCONFIRMED.
pub const CARD_TICKS: u32 = 5 * HZ as u32;

/// What the setup chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// The hunt clock of each round, in sim ticks (starts when the seeker is released).
    pub hunt_ticks: u32,
    /// Kills that win the match (no round limit).
    pub kills_to_win: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config { hunt_ticks: DEFAULT_ROUND_SECS * HZ as u32, kills_to_win: KILLS_TO_WIN }
    }
}

/// Why a round ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundEnd {
    /// The hunt clock ran out.
    TimeUp,
    /// Every hider was caught.
    AllCaught,
    /// The seeker used all ten knives or ran out of Health.
    SeekerSpent,
}

/// Where the round is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The seeker waits (frozen, blind) for `left` more ticks; the hiders hide.
    Countdown { left: u32 },
    /// The seeker hunts; `left` ticks on the hunt clock.
    Hunt { left: u32 },
    /// The round is over: the standings card shows for `left` more ticks, then the next round (or the end).
    Card { left: u32, why: RoundEnd },
    /// The match is over (the front end moves to the result screen).
    Over { why: RoundEnd },
}

/// A hider's disguise: the stage object (`props::Prop::index`, the placement index the stage glTF names its nodes by)
/// and the hider's facing when they took it (the renderer turns the copy by how far the hider has turned since).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disguise {
    pub prop: u32,
    pub yaw: f32,
}

/// Where a frozen hider is held (x, z and facing; the height still follows the floor).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frozen {
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
}

/// One player's hider state this round (all default for the seeker).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hider {
    pub disguise: Option<Disguise>,
    pub frozen: Option<Frozen>,
    pub caught: bool,
    /// Ticks left of the "nothing in reach" note after a disguise press that found no object.
    pub nothing_in_reach: u16,
}

/// The whole mode's state. Everything here is sim state (hashed by `Match::checksum`).
#[derive(Clone, Debug, PartialEq)]
pub struct PropHunt {
    pub config: Config,
    /// 1-based round number.
    pub round: u32,
    pub seeker: usize,
    pub phase: Phase,
    pub knives_left: u32,
    /// Kills per slot (only ever scored as the seeker).
    pub kills: [u32; MAX_PLAYERS],
    pub hiders: [Hider; MAX_PLAYERS],
    /// How many times a seeker has been released (the renderer rumbles the seeker's pad on each new one).
    pub releases: u32,
    /// Which slots play (from the roster at the start).
    active: [bool; MAX_PLAYERS],
    /// Skill buttons held last tick (press edges: X and B for hiders).
    prev_buttons: [u8; MAX_PLAYERS],
    /// The last combat event number already looked at.
    seen_event: u32,
    /// The seeker's `cast_seq` already paid for.
    seen_cast: u16,
}

/// The seeker of round `round` (1-based): the players take turns in slot order.
pub fn seeker_for(round: u32, active: &[bool; MAX_PLAYERS]) -> usize {
    let slots: Vec<usize> = (0..MAX_PLAYERS).filter(|&i| active[i]).collect();
    if slots.is_empty() {
        return 0;
    }
    slots[(round.max(1) as usize - 1) % slots.len()]
}

const X_BUTTON: u8 = 1 << 0;
const B_BUTTON: u8 = 1 << 1;
/// A hider's "become an object" buttons: A, X or Y (every skill button but B, which freezes). Laurence
/// 2026-10-08: "Pressing A did not turn me into a prop" (it was X only).
const DISGUISE_BUTTONS: u8 = X_BUTTON | 1 << 2 | 1 << 3;

impl PropHunt {
    /// A new match: round 1 is set up on `combat` (the caller puts the players on their spawns).
    pub fn start(config: Config, combat: &mut Combat) -> Self {
        let mut ph = PropHunt {
            config,
            round: 0,
            seeker: 0,
            phase: Phase::Countdown { left: SEEKER_COUNTDOWN_TICKS },
            knives_left: KNIFE_ATTEMPTS,
            kills: [0; MAX_PLAYERS],
            hiders: [Hider::default(); MAX_PLAYERS],
            releases: 0,
            active: combat.roster.active,
            prev_buttons: [0; MAX_PLAYERS],
            seen_event: 0,
            seen_cast: 0,
        };
        combat.rounds_by_mode = true;
        // The sim's own time limit would end the round on its own: the hunt clock replaces it.
        combat.time_limit = 0;
        ph.begin_round(combat);
        ph
    }

    /// The next round: the next seeker, fresh fighters (the knife on the seeker's X, nothing for the hiders), no
    /// disguises. The sim's own 3-2-1 countdown is skipped: the seeker's 20 s countdown is this round's start.
    fn begin_round(&mut self, combat: &mut Combat) {
        self.round += 1;
        self.seeker = seeker_for(self.round, &self.active);
        self.phase = Phase::Countdown { left: SEEKER_COUNTDOWN_TICKS };
        self.knives_left = KNIFE_ATTEMPTS;
        self.hiders = [Hider::default(); MAX_PLAYERS];
        // A button held over the round change is not a press.
        self.prev_buttons = [u8::MAX; MAX_PLAYERS];
        combat.restart_round();
        combat.round_ticks = COUNTDOWN_TICKS;
        for i in combat.roster.slots().collect::<Vec<_>>() {
            let f = &mut combat.fighters[i];
            f.arsenal.clear();
            for k in 0..SLOTS {
                f.set_slot(k, None);
            }
            if i == self.seeker {
                f.set_slot(0, Some(PSYCHO_KNIFE));
                f.level = SEEKER_LEVEL;
                f.aura_ticks = SEEKER_LEVEL * AURA_TICKS_PER_POINT;
                f.health = SEEKER_HEALTH;
                f.max_health = SEEKER_HEALTH;
            } else {
                // No Aura at all: nothing to cast, and no Aura glow giving a disguise away.
                f.level = 0;
                f.aura_ticks = 0;
            }
        }
        self.seen_event = combat.events.last().map_or(0, |e| e.0);
        self.seen_cast = combat.fighters[self.seeker].cast_seq;
    }

    /// Whether `i` hides this round (plays and is not the seeker).
    pub fn is_hider(&self, i: usize) -> bool {
        i < MAX_PLAYERS && self.active[i] && i != self.seeker
    }

    /// The hiders of this round, in slot order.
    pub fn hider_slots(&self) -> impl Iterator<Item = usize> + '_ {
        (0..MAX_PLAYERS).filter(|&i| self.is_hider(i))
    }

    /// Hiders not caught yet.
    pub fn hiders_left(&self) -> usize {
        self.hider_slots().filter(|&i| !self.hiders[i].caught).count()
    }

    /// The seeker is waiting out the countdown (their view is covered).
    pub fn seeker_blind(&self) -> bool {
        matches!(self.phase, Phase::Countdown { .. })
    }

    /// The seeker is hunting (their view is the whole window, the hiders see nothing).
    pub fn hunting(&self) -> bool {
        matches!(self.phase, Phase::Hunt { .. })
    }

    /// The match is over.
    pub fn match_over(&self) -> bool {
        matches!(self.phase, Phase::Over { .. })
    }

    /// The clock the HUD shows, in ticks: the countdown, then the hunt clock (None between rounds).
    pub fn clock_ticks(&self) -> Option<u32> {
        match self.phase {
            Phase::Countdown { left } | Phase::Hunt { left } => Some(left),
            _ => None,
        }
    }

    /// Slot `i`'s disguise, if they hide as an object now (a caught hider is a fighter again).
    pub fn disguise_of(&self, i: usize) -> Option<Disguise> {
        self.hiders.get(i).filter(|h| !h.caught && self.is_hider(i)).and_then(|h| h.disguise)
    }

    /// Whether the match has been decided: someone has reached the kill target (there is no round limit).
    pub fn decided(&self) -> bool {
        self.kills.iter().any(|&k| k >= self.config.kills_to_win.max(1))
    }

    /// The winner by kills: the one player with the most, if they have any. A tie at the top (or nobody scoring) is a
    /// draw (`None`).
    pub fn winner(&self) -> Option<usize> {
        let best = (0..MAX_PLAYERS).filter(|&i| self.active[i]).map(|i| self.kills[i]).max()?;
        let mut top = (0..MAX_PLAYERS).filter(|&i| self.active[i] && self.kills[i] == best);
        match (top.next(), top.next()) {
            (Some(w), None) if best > 0 => Some(w),
            _ => None,
        }
    }

    /// Finishing places by kills (1 = most; equal kills share a place; 0 = not playing).
    pub fn places(&self) -> [u8; MAX_PLAYERS] {
        std::array::from_fn(|i| if self.active[i] { 1 + (0..MAX_PLAYERS).filter(|&j| self.active[j] && self.kills[j] > self.kills[i]).count() as u8 } else { 0 })
    }

    /// Before the tick: what each player's input is allowed to do this tick, and the hiders' own buttons (X takes a
    /// disguise, B toggles freeze). Lock-on is off for everyone (it would point the seeker at a hidden player).
    pub fn filter_inputs(&mut self, inputs: &[Input], combat: &mut Combat, players: &mut [Player]) -> [Input; MAX_PLAYERS] {
        let raw: [Input; MAX_PLAYERS] = std::array::from_fn(|i| inputs.get(i).copied().unwrap_or_default());
        let mut out = raw;
        let live = matches!(self.phase, Phase::Countdown { .. } | Phase::Hunt { .. });
        for i in 0..MAX_PLAYERS {
            let pressed = raw[i].skills & !self.prev_buttons[i];
            self.prev_buttons[i] = raw[i].skills;
            if !self.active[i] {
                continue;
            }
            let inp = &mut out[i];
            inp.lock = false;
            inp.switch_target = false;
            players[i].locked = false;
            if !live {
                *inp = Input::default();
                continue;
            }
            if i == self.seeker {
                if self.seeker_blind() {
                    // Frozen, blind and unable to act until released.
                    *inp = Input::default();
                    continue;
                }
                // "Infinite mana": the knife's Aura is always there.
                let f = &mut combat.fighters[i];
                f.aura_ticks = f.level * AURA_TICKS_PER_POINT;
                // Only the knife, and only while there are attempts and Health left.
                let can_knife = self.knives_left > 0 && combat.fighters[i].health > 0;
                inp.skills &= if can_knife { X_BUTTON } else { 0 };
                continue;
            }
            // A hider: never casts (the buttons are theirs).
            inp.skills = 0;
            if self.hiders[i].caught {
                *inp = Input::default();
                continue;
            }
            self.hiders[i].nothing_in_reach = self.hiders[i].nothing_in_reach.saturating_sub(1);
            if pressed & DISGUISE_BUTTONS != 0 {
                let p = &players[i];
                if let Some(k) = crate::props::nearest_disguise(&combat.props, p.pos) {
                    let prop = combat.props[k].index;
                    self.hiders[i].nothing_in_reach = 0;
                    if self.hiders[i].disguise.map(|d| d.prop) != Some(prop) {
                        self.hiders[i].disguise = Some(Disguise { prop, yaw: p.yaw });
                    }
                } else {
                    self.hiders[i].nothing_in_reach = NOTHING_IN_REACH_TICKS;
                }
            }
            if pressed & B_BUTTON != 0 {
                let p = &players[i];
                self.hiders[i].frozen = match self.hiders[i].frozen {
                    Some(_) => None,
                    None => Some(Frozen { x: p.pos.x, z: p.pos.z, yaw: p.yaw }),
                };
            }
            if self.hiders[i].frozen.is_some() {
                inp.stick = [0, 0];
                inp.jump = false;
            }
        }
        out
    }

    /// After the tick: pay for knife casts, catch the hiders the knife hit, hold frozen hiders, keep the hiders'
    /// Health full (nothing but the knife takes them out) and run the round clock. Returns true when a new round has
    /// begun (the caller puts everyone back on their spawns).
    pub fn after_step(&mut self, combat: &mut Combat, players: &mut [Player]) -> bool {
        let s = self.seeker;
        // A knife cast started this tick: one attempt and 2 Health gone. Set directly, not as damage: the swing plays
        // out even when it spends the seeker's last Health.
        let f = &combat.fighters[s];
        if f.cast_seq != self.seen_cast {
            self.seen_cast = f.cast_seq;
            if f.cast_skill == PSYCHO_KNIFE && self.hunting() {
                self.knives_left = self.knives_left.saturating_sub(1);
                let f = &mut combat.fighters[s];
                f.health = (f.health - KNIFE_HEALTH_COST).max(0);
            }
        }
        // Knife hits on hiders: caught.
        let fresh: Vec<CombatEvent> = combat.events.iter().filter(|(n, _)| *n > self.seen_event).map(|e| e.1).collect();
        self.seen_event = combat.events.last().map_or(self.seen_event, |e| e.0);
        if self.hunting() {
            for e in fresh {
                if let CombatEvent::Hit { attacker, victim, skill } = e {
                    if attacker == s && skill == PSYCHO_KNIFE && self.is_hider(victim) && !self.hiders[victim].caught {
                        self.catch(victim, combat);
                    }
                }
            }
        }
        for i in self.hider_slots().collect::<Vec<_>>() {
            let h = self.hiders[i];
            if h.caught {
                continue;
            }
            // Frozen: held where they froze, facing the same way (pushes and slides undone; the height still follows
            // the floor).
            if let Some(fz) = h.frozen {
                let p = &mut players[i];
                p.pos.x = fz.x;
                p.pos.z = fz.z;
                p.vel.x = 0.0;
                p.vel.z = 0.0;
                p.yaw = fz.yaw;
            }
            let f = &mut combat.fighters[i];
            f.health = f.max_health;
        }
        self.tick_clock(combat)
    }

    /// Hider `i` is caught: back to a fighter, knocked down for the rest of the round (the sim's knock-out: the hit's
    /// reaction plays, then they lie), and the seeker scores.
    fn catch(&mut self, i: usize, combat: &mut Combat) {
        self.hiders[i] = Hider { caught: true, ..Hider::default() };
        self.kills[self.seeker] += 1;
        // `rounds_by_mode` keeps this from ending the sim's own round.
        combat.change_health(i, -i32::MAX / 2);
    }

    fn tick_clock(&mut self, combat: &mut Combat) -> bool {
        match self.phase {
            Phase::Countdown { left } => {
                self.phase = if left <= 1 {
                    self.releases += 1;
                    Phase::Hunt { left: self.config.hunt_ticks.max(1) }
                } else {
                    Phase::Countdown { left: left - 1 }
                };
            }
            Phase::Hunt { left } => {
                let f = &combat.fighters[self.seeker];
                let mid_knife = matches!(f.action, Action::Cast | Action::Lunge);
                let spent = (self.knives_left == 0 || f.health <= 0) && !mid_knife;
                let why = if self.hiders_left() == 0 {
                    Some(RoundEnd::AllCaught)
                } else if spent {
                    Some(RoundEnd::SeekerSpent)
                } else if left <= 1 {
                    Some(RoundEnd::TimeUp)
                } else {
                    None
                };
                self.phase = match why {
                    Some(why) => Phase::Card { left: CARD_TICKS, why },
                    None => Phase::Hunt { left: left - 1 },
                };
            }
            Phase::Card { left, why } => {
                if left > 1 {
                    self.phase = Phase::Card { left: left - 1, why };
                } else if self.decided() {
                    self.phase = Phase::Over { why };
                } else {
                    self.begin_round(combat);
                    return true;
                }
            }
            Phase::Over { .. } => {}
        }
        false
    }

    /// The seeker after this round (for the card); None when the match is decided.
    pub fn next_seeker(&self) -> Option<usize> {
        (!self.decided()).then(|| seeker_for(self.round + 1, &self.active))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::testkit as k;
    use crate::combat::{Capsule, Roster};
    use crate::props::Prop;
    use crate::sim::{Match, Motion};
    use glam::Vec3;

    /// A knife the test can cast (the real Psycho Knife's table is in the cache; any castable attack with its id does).
    fn knife() -> crate::combat::SkillDef {
        let mut d = k::def(PSYCHO_KNIFE, Capsule::Attack, 1, &[]);
        d.name = "Psycho Knife".into();
        d
    }

    /// A prop hunt match of `n` players on a flat floor, 80 units apart in a row, with `props`.
    fn game(n: usize, props: Vec<Prop>, config: Config) -> (Match, pd_formats::collision::Collision) {
        let mut c = Combat::with_roster(vec![knife()], Roster::free_for_all(n), std::array::from_fn(|_| vec![PSYCHO_KNIFE]));
        c.set_rules(crate::combat::Rules { start_level: SEEKER_LEVEL, start_hand: 0, capsules: false, infinite_uses: true, ..crate::combat::Rules::NORMAL });
        c.props = props;
        let spawns: Vec<Vec3> = (0..n).map(|i| Vec3::new(i as f32 * 80.0, 0.0, 0.0)).collect();
        let mut m = Match::with_spawns(&spawns, c);
        m.start_prop_hunt(config);
        (m, k::floor())
    }

    fn quick() -> Config {
        Config { hunt_ticks: 600, kills_to_win: KILLS_TO_WIN }
    }

    fn ph(m: &Match) -> &PropHunt {
        m.prophunt.as_ref().unwrap()
    }

    fn ph_mut(m: &mut Match) -> &mut PropHunt {
        m.prophunt.as_mut().unwrap()
    }

    fn step(m: &mut Match, col: &mut pd_formats::collision::Collision, inputs: [Input; MAX_PLAYERS]) {
        m.step(inputs, col);
    }

    fn idle(m: &mut Match, col: &mut pd_formats::collision::Collision, ticks: u32) {
        for _ in 0..ticks {
            step(m, col, [Input::default(); MAX_PLAYERS]);
        }
    }

    fn skip_countdown(m: &mut Match, col: &mut pd_formats::collision::Collision) {
        idle(m, col, SEEKER_COUNTDOWN_TICKS);
        assert!(ph(m).hunting(), "released after 20 s");
    }

    fn press(slot: usize, buttons: u8) -> [Input; MAX_PLAYERS] {
        let mut i = [Input::default(); MAX_PLAYERS];
        i[slot].skills = buttons;
        i
    }

    fn walk(slot: usize) -> [Input; MAX_PLAYERS] {
        let mut i = [Input::default(); MAX_PLAYERS];
        i[slot] = Input::from_axes(0.0, 1.0, false, false);
        i
    }

    /// A small object (a drum-sized box) with its base at `at`.
    fn drum(index: u32, at: Vec3) -> Prop {
        Prop::new(index, 2, 3, at - Vec3::new(5.0, 0.0, 5.0), at + Vec3::new(5.0, 20.0, 5.0), true, true)
    }

    #[test]
    fn the_seeker_rotates_through_the_seats_in_order() {
        let all = [true, true, true, false];
        assert_eq!((1..=7).map(|r| seeker_for(r, &all)).collect::<Vec<_>>(), [0, 1, 2, 0, 1, 2, 0]);
        let duo = [true, true, false, false];
        assert_eq!((1..=4).map(|r| seeker_for(r, &duo)).collect::<Vec<_>>(), [0, 1, 0, 1]);
        // Through the real round flow: the time runs out every round.
        let (mut m, mut col) = game(3, Vec::new(), Config { hunt_ticks: 10, kills_to_win: KILLS_TO_WIN });
        let mut seen = vec![ph(&m).seeker];
        for _ in 0..3 {
            idle(&mut m, &mut col, SEEKER_COUNTDOWN_TICKS + 10 + CARD_TICKS);
            seen.push(ph(&m).seeker);
        }
        assert_eq!(seen, [0, 1, 2, 0]);
        assert_eq!(ph(&m).round, 4);
    }

    #[test]
    fn the_seeker_starts_with_the_knife_twenty_health_and_the_hiders_with_nothing() {
        let (m, _) = game(3, Vec::new(), quick());
        let c = &m.combat;
        assert_eq!(c.fighters[0].slots, [Some(PSYCHO_KNIFE), None, None, None]);
        assert_eq!((c.fighters[0].health, c.fighters[0].max_health), (SEEKER_HEALTH, SEEKER_HEALTH));
        for h in [1, 2] {
            assert_eq!(c.fighters[h].slots, [None; SLOTS], "hider {h}: no hand");
            assert!(c.fighters[h].arsenal.is_empty(), "no capsules to draw");
            assert_eq!((c.fighters[h].level, c.fighters[h].aura_ticks), (0, 0));
        }
        assert_eq!(ph(&m).knives_left, KNIFE_ATTEMPTS);
        assert!(c.rounds_by_mode && c.time_limit == 0, "the mode owns the round clock");
        assert!(!c.counting_down(), "no 3-2-1: the seeker countdown is the start");
    }

    #[test]
    fn the_countdown_blocks_the_seekers_movement_and_casts_but_not_the_hiders() {
        let (mut m, mut col) = game(2, Vec::new(), quick());
        let (seeker0, hider0) = (m.players[0].pos, m.players[1].pos);
        let mut both = walk(0);
        // The hider backs away (forward would walk into the seeker).
        both[1] = Input::from_axes(0.0, -1.0, false, false);
        both[0].skills = X_BUTTON;
        for _ in 0..120 {
            step(&mut m, &mut col, both);
        }
        assert!(ph(&m).seeker_blind());
        assert_eq!(m.players[0].pos, seeker0, "the seeker can't move");
        assert_eq!(m.combat.fighters[0].cast_seq, 0, "nor cast");
        assert_eq!(ph(&m).knives_left, KNIFE_ATTEMPTS);
        assert!(m.players[1].pos.distance(hider0) > 20.0, "the hider walks off to hide");
        // Released: the seeker moves.
        idle(&mut m, &mut col, SEEKER_COUNTDOWN_TICKS - 120);
        assert!(ph(&m).hunting());
        assert_eq!(ph(&m).releases, 1, "one release (the renderer's rumble cue)");
        let at = m.players[0].pos;
        for _ in 0..60 {
            step(&mut m, &mut col, walk(0));
        }
        assert!(m.players[0].pos.distance(at) > 10.0, "free to hunt");
    }

    #[test]
    fn the_seeker_has_infinite_aura_ten_knives_and_each_costs_two_health() {
        let (mut m, mut col) = game(2, Vec::new(), Config { hunt_ticks: 60 * 60, kills_to_win: KILLS_TO_WIN });
        skip_countdown(&mut m, &mut col);
        // The hider far out of the knife's reach (a hit would end the round).
        m.players[1].pos = Vec3::new(900.0, 0.0, 900.0);
        for n in 1..=KNIFE_ATTEMPTS {
            let before = m.combat.fighters[0].cast_seq;
            // Drained Aura: the refill puts it back before the press.
            m.combat.fighters[0].aura_ticks = 0;
            step(&mut m, &mut col, press(0, X_BUTTON));
            assert_ne!(m.combat.fighters[0].cast_seq, before, "knife {n} was cast");
            assert_eq!(ph(&m).knives_left, KNIFE_ATTEMPTS - n);
            assert_eq!(m.combat.fighters[0].health, SEEKER_HEALTH - KNIFE_HEALTH_COST * n as i32);
            idle(&mut m, &mut col, 150);
            assert_eq!(m.combat.fighters[0].slots[0], Some(PSYCHO_KNIFE), "the knife never leaves the button");
        }
        assert_eq!(m.combat.fighters[0].health, 0, "ten knives = all twenty Health");
        assert!(matches!(ph(&m).phase, Phase::Card { why: RoundEnd::SeekerSpent, .. }), "spent: the round is over");
        // The 11th press does nothing: the filter takes X away once the attempts are gone (or the Health is).
        for (knives, health) in [(0, 20), (5, 0)] {
            let (mut m, _) = game(2, Vec::new(), quick());
            let p = m.prophunt.as_mut().unwrap();
            p.phase = Phase::Hunt { left: 100 };
            p.knives_left = knives;
            m.combat.fighters[0].health = health;
            let out = m.prophunt.as_mut().unwrap().filter_inputs(&press(0, X_BUTTON), &mut m.combat, &mut m.players);
            assert_eq!(out[0].skills, 0, "{knives} knives, {health} Health");
        }
        // With both left, X goes through (and only X).
        let (mut m, _) = game(2, Vec::new(), quick());
        m.prophunt.as_mut().unwrap().phase = Phase::Hunt { left: 100 };
        let out = m.prophunt.as_mut().unwrap().filter_inputs(&press(0, 0b1111), &mut m.combat, &mut m.players);
        assert_eq!(out[0].skills, X_BUTTON);
    }

    #[test]
    fn hiders_cannot_cast_and_x_takes_the_nearest_object_in_reach() {
        let props = vec![
            drum(10, Vec3::new(80.0, 0.0, 20.0)),                                                          // 20 from the hider
            drum(11, Vec3::new(80.0, 0.0, -12.0)),                                                         // 12: the nearest
            Prop::new(12, 2, 23, Vec3::new(70.0, -5.0, -40.0), Vec3::new(140.0, 0.0, 40.0), false, true), // a floor section under them
            drum(13, Vec3::new(80.0, 60.0, 0.0)),                                                          // on the floor above
        ];
        let (mut m, mut col) = game(2, props, quick());
        // A button already held when the round starts is not a press.
        step(&mut m, &mut col, press(1, X_BUTTON));
        assert_eq!(ph(&m).hiders[1].disguise, None, "held over the round start");
        idle(&mut m, &mut col, 1);
        step(&mut m, &mut col, press(1, X_BUTTON | 4 | 8));
        assert_eq!(m.combat.fighters[1].cast_seq, 0, "a hider never casts");
        assert_eq!(ph(&m).hiders[1].disguise.map(|d| d.prop), Some(11), "the nearest object on their level, not the floor or the one above");
        assert_eq!(ph(&m).disguise_of(1).map(|d| d.prop), Some(11));
        // Held: no second pick. Walk over to the other drum and press again: switched.
        idle(&mut m, &mut col, 1);
        m.players[1].pos = Vec3::new(80.0, 0.0, 30.0);
        step(&mut m, &mut col, press(1, X_BUTTON));
        assert_eq!(ph(&m).hiders[1].disguise.map(|d| d.prop), Some(10), "X near another object switches");
        // Out of reach of everything: X changes nothing.
        idle(&mut m, &mut col, 1);
        m.players[1].pos = Vec3::new(400.0, 0.0, 400.0);
        step(&mut m, &mut col, press(1, X_BUTTON));
        assert_eq!(ph(&m).hiders[1].disguise.map(|d| d.prop), Some(10));
        assert!(ph(&m).hiders[1].nothing_in_reach > 0, "the line says nothing is in reach");
        // The seeker's X is the knife, never a disguise.
        assert_eq!(ph(&m).hiders[0].disguise, None);
    }

    #[test]
    fn a_and_y_disguise_a_hider_too() {
        for button in [1 << 3, 1 << 2] {
            let (mut m, mut col) = game(2, vec![drum(11, Vec3::new(80.0, 0.0, -12.0))], quick());
            idle(&mut m, &mut col, 1);
            step(&mut m, &mut col, press(1, button));
            assert_eq!(ph(&m).hiders[1].disguise.map(|d| d.prop), Some(11), "button bit {button}");
            assert_eq!(ph(&m).hiders[1].nothing_in_reach, 0);
        }
    }

    #[test]
    fn b_freezes_a_hider_in_place_facing_the_same_way_and_b_again_lets_them_go() {
        let (mut m, mut col) = game(2, Vec::new(), quick());
        idle(&mut m, &mut col, 1);
        step(&mut m, &mut col, press(1, B_BUTTON));
        let (at, yaw) = (m.players[1].pos, m.players[1].yaw);
        assert!(ph(&m).hiders[1].frozen.is_some());
        let mut push = [Input::default(); MAX_PLAYERS];
        push[1] = Input::from_axes(1.0, 0.3, true, false);
        push[1].skills = B_BUTTON; // still held from the press: not a new press
        for _ in 0..90 {
            step(&mut m, &mut col, push);
        }
        assert!((m.players[1].pos.x - at.x).abs() < 1e-4 && (m.players[1].pos.z - at.z).abs() < 1e-4, "no movement while frozen");
        assert_eq!(m.players[1].yaw, yaw, "facing locked");
        // Release B, press it again: free.
        idle(&mut m, &mut col, 1);
        step(&mut m, &mut col, press(1, B_BUTTON));
        assert!(ph(&m).hiders[1].frozen.is_none());
        for _ in 0..60 {
            step(&mut m, &mut col, walk(1));
        }
        assert!(m.players[1].pos.distance(at) > 10.0, "moves again (even blind, during the hunt)");
    }

    #[test]
    fn a_knife_hit_catches_the_hider_scores_a_kill_and_knocks_them_down_for_good() {
        let (mut m, mut col) = game(3, vec![drum(5, Vec3::new(80.0, 0.0, 10.0))], quick());
        idle(&mut m, &mut col, 1);
        step(&mut m, &mut col, press(1, X_BUTTON));
        skip_countdown(&mut m, &mut col);
        assert!(ph(&m).disguise_of(1).is_some());
        // The knife lands on hider 1 (the hit as the sim logs it).
        m.combat.log(CombatEvent::Hit { attacker: 0, victim: 1, skill: PSYCHO_KNIFE });
        idle(&mut m, &mut col, 1);
        assert!(ph(&m).hiders[1].caught);
        assert_eq!(ph(&m).disguise_of(1), None, "turns back into a player");
        assert_eq!(ph(&m).kills[0], 1);
        assert_eq!(m.combat.fighters[1].action, Action::Down, "knocked down");
        // The catch is settled at the end of the tick; the body's motion follows on the next one.
        idle(&mut m, &mut col, 1);
        assert!(matches!(m.players[1].motion, Motion::Down | Motion::Floored | Motion::Hit | Motion::Blown), "the knock-down plays: {:?}", m.players[1].motion);
        assert!(ph(&m).hunting(), "one hider is still out there");
        assert!(m.combat.round_over.is_none(), "the sim's own round end stays out of it");
        // Caught for the rest of the round: no movement, no disguise, a second hit scores nothing.
        let at = m.players[1].pos;
        let mut try_move = walk(1);
        try_move[1].skills = X_BUTTON;
        for _ in 0..30 {
            step(&mut m, &mut col, try_move);
        }
        assert_eq!(m.players[1].pos, at);
        m.combat.log(CombatEvent::Hit { attacker: 0, victim: 1, skill: PSYCHO_KNIFE });
        idle(&mut m, &mut col, 1);
        assert_eq!(ph(&m).kills[0], 1);
        // Another skill's hit (or someone else's) catches nobody.
        m.combat.log(CombatEvent::Hit { attacker: 0, victim: 2, skill: 75 });
        m.combat.log(CombatEvent::Hit { attacker: 1, victim: 2, skill: PSYCHO_KNIFE });
        idle(&mut m, &mut col, 1);
        assert!(!ph(&m).hiders[2].caught);
    }

    #[test]
    fn hiders_are_never_knocked_out_by_anything_but_the_knife() {
        let (mut m, mut col) = game(2, Vec::new(), Config { hunt_ticks: 5000, kills_to_win: KILLS_TO_WIN });
        skip_countdown(&mut m, &mut col);
        // Empty-handed fighters drain Health in a normal match ("powerless"); a hider's stays full.
        idle(&mut m, &mut col, 2000);
        assert!(ph(&m).hunting() && ph(&m).seeker == 0);
        assert_eq!(m.combat.fighters[1].health, m.combat.fighters[1].max_health);
        assert_ne!(m.combat.fighters[1].action, Action::Down);
    }

    #[test]
    fn a_round_ends_when_time_runs_out() {
        let (mut m, mut col) = game(2, Vec::new(), Config { hunt_ticks: 300, kills_to_win: KILLS_TO_WIN });
        skip_countdown(&mut m, &mut col);
        assert_eq!(ph(&m).clock_ticks(), Some(300), "the hunt clock starts at the release");
        idle(&mut m, &mut col, 299);
        assert!(ph(&m).hunting());
        idle(&mut m, &mut col, 1);
        assert_eq!(ph(&m).phase, Phase::Card { left: CARD_TICKS, why: RoundEnd::TimeUp });
        assert_eq!(ph(&m).next_seeker(), Some(1));
        idle(&mut m, &mut col, CARD_TICKS);
        assert_eq!((ph(&m).round, ph(&m).seeker), (2, 1), "the next round, the next seeker");
        assert!(ph(&m).seeker_blind());
        assert_eq!(m.players[0].pos, m.players[0].spawn, "everyone back on their spawn");
    }

    #[test]
    fn a_round_ends_when_every_hider_is_caught() {
        let (mut m, mut col) = game(3, Vec::new(), quick());
        skip_countdown(&mut m, &mut col);
        m.combat.log(CombatEvent::Hit { attacker: 0, victim: 1, skill: PSYCHO_KNIFE });
        idle(&mut m, &mut col, 1);
        assert!(ph(&m).hunting());
        m.combat.log(CombatEvent::Hit { attacker: 0, victim: 2, skill: PSYCHO_KNIFE });
        idle(&mut m, &mut col, 1);
        assert!(matches!(ph(&m).phase, Phase::Card { why: RoundEnd::AllCaught, .. }));
        assert_eq!(ph(&m).kills[0], 2);
    }

    #[test]
    fn a_round_ends_when_the_seeker_is_out_of_knives_or_health() {
        // Out of knives (the last swing plays out first).
        let (mut m, mut col) = game(2, Vec::new(), quick());
        skip_countdown(&mut m, &mut col);
        ph_mut(&mut m).knives_left = 1;
        step(&mut m, &mut col, press(0, X_BUTTON));
        assert_eq!(ph(&m).knives_left, 0);
        assert!(ph(&m).hunting(), "not while the last knife is still swinging");
        idle(&mut m, &mut col, 200);
        assert!(matches!(ph(&m).phase, Phase::Card { why: RoundEnd::SeekerSpent, .. }));
        // Out of Health some other way (a fall).
        let (mut m, mut col) = game(2, Vec::new(), quick());
        skip_countdown(&mut m, &mut col);
        m.combat.fighters[0].health = 0;
        idle(&mut m, &mut col, 1);
        assert!(matches!(ph(&m).phase, Phase::Card { why: RoundEnd::SeekerSpent, .. }));
    }

    #[test]
    fn first_to_five_kills_wins_at_the_end_of_that_round() {
        let (mut m, mut col) = game(2, Vec::new(), Config { hunt_ticks: 600, kills_to_win: KILLS_TO_WIN });
        ph_mut(&mut m).kills = [4, 0, 0, 0];
        skip_countdown(&mut m, &mut col);
        m.combat.log(CombatEvent::Hit { attacker: 0, victim: 1, skill: PSYCHO_KNIFE });
        idle(&mut m, &mut col, 1);
        assert!(ph(&m).decided(), "five kills");
        assert_eq!(ph(&m).next_seeker(), None);
        idle(&mut m, &mut col, CARD_TICKS);
        assert!(ph(&m).match_over());
        assert_eq!(ph(&m).winner(), Some(0));
        assert_eq!(ph(&m).places()[..2], [1, 2]);
        assert_eq!(ph(&m).round, 1, "no further round");
        // It stays over.
        idle(&mut m, &mut col, 100);
        assert!(ph(&m).match_over());
    }

    #[test]
    fn there_is_no_round_limit_the_match_plays_on_until_someone_has_five_kills() {
        let (mut m, mut col) = game(3, Vec::new(), Config { hunt_ticks: 10, kills_to_win: KILLS_TO_WIN });
        ph_mut(&mut m).kills = [1, 3, 0, 0];
        // Twelve empty rounds: nobody reaches 5, so it never ends.
        for _ in 0..12 {
            idle(&mut m, &mut col, SEEKER_COUNTDOWN_TICKS + 10 + CARD_TICKS);
        }
        assert!(ph(&m).round >= 12, "round {}", ph(&m).round);
        assert!(!ph(&m).match_over(), "no round limit");
        // Five kills decides it, with one winner.
        ph_mut(&mut m).kills = [1, 5, 0, 0];
        assert!(ph(&m).decided());
        // The target is the setup's KILLS TO WIN: at 8, five kills is not enough yet.
        ph_mut(&mut m).config.kills_to_win = 8;
        assert!(!ph(&m).decided());
        ph_mut(&mut m).kills = [1, 8, 0, 0];
        assert!(ph(&m).decided());
        assert_eq!(ph(&m).winner(), Some(1));
        assert_eq!(ph(&m).places()[..3], [2, 1, 3]);
    }

    #[test]
    fn lock_on_is_off_for_everyone() {
        let (mut m, mut col) = game(2, Vec::new(), quick());
        skip_countdown(&mut m, &mut col);
        let mut lock = [Input::default(); MAX_PLAYERS];
        lock[0].lock = true;
        lock[1].lock = true;
        for _ in 0..3 {
            step(&mut m, &mut col, lock);
            assert!(!m.players[0].locked && !m.players[1].locked);
        }
    }

    #[test]
    fn the_same_inputs_give_the_same_match_and_the_state_is_in_the_checksum() {
        let run = || {
            let (mut m, mut col) = game(3, vec![drum(5, Vec3::new(80.0, 0.0, 10.0))], quick());
            let mut sums = Vec::new();
            for t in 0..(SEEKER_COUNTDOWN_TICKS + 200) {
                let mut inp = [Input::default(); MAX_PLAYERS];
                inp[1].skills = if t % 50 == 0 { X_BUTTON } else if t % 77 == 0 { B_BUTTON } else { 0 };
                inp[2] = Input::from_axes((t as f32 * 0.01).sin(), 0.5, false, false);
                inp[0] = Input::from_axes(0.0, 1.0, false, false);
                inp[0].skills = if t % 90 == 0 { X_BUTTON } else { 0 };
                m.step(inp, &mut col);
                sums.push(m.checksum());
            }
            (sums, m.prophunt.clone())
        };
        assert_eq!(run(), run());
        // Two matches that differ only in prop hunt state checksum differently.
        let (a, _) = game(2, Vec::new(), quick());
        let mut b = a.clone();
        b.prophunt.as_mut().unwrap().kills[1] = 1;
        assert_ne!(a.checksum(), b.checksum());
    }
}
