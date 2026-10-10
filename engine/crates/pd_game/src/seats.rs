//! Who plays which slot: the match's player list and the controller seats.
//!
//! Four-player plumbing (docs/audits/P0-FOURPLAYER.md step S2, decisions
//! 2026-10-01 in DECISIONS.md):
//! - a match is a list of `PlayerSetup`s (character, costume colour, arsenal,
//!   team, seat); slot `i` of the sim is entry `i` (slots are always the first
//!   `n`, so `Roster::with_teams` fits);
//! - controllers take seats: pads join by pressing a button (or, until the
//!   front end asks for that, in the order they are found), keep their seat
//!   if they reconnect, and the match pauses while a seat's pad is missing;
//! - the keyboard plays seat 1 (slot 0) only.
//!
//! Render/input side only; nothing here is read by the simulation.

use crate::combat::{Roster, MAX_PLAYERS};
use bevy::prelude::*;

/// Seats = sim slots.
pub const MAX_SEATS: usize = MAX_PLAYERS;

/// One player of a match.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSetup {
    /// Character model id (`pc00a`).
    pub char: String,
    /// Costume colour 0..=3 (`cache/chars/<id>.c<colour>.png`).
    pub colour: u8,
    /// The 30-skill deck.
    pub arsenal: Vec<i16>,
    /// Team id (free-for-all: each player their own).
    pub team: u8,
    /// The controller seat that plays this slot (today the same as the slot).
    pub seat: usize,
}

/// Who plays a seat's fighter. `Cpu { epa_row }` is the computer opponent rebuilt from the original's quest AI
/// (`cpu/`, docs/FORMATS-CPU.md): the same fighter as a human seat (the program's seat constructor with the human
/// flag 0, an AI-info struct and a row of `enemyparam.epa`, 0..81: `epa_row` picks the personality and strength,
/// P0-AI.md 2.5). The character-select screen makes CPU seats (`frontend/cpuseat.rs`: five grades = rows 64..60) and writes this
/// before the match; `--cpu 2:<row>` does it for scripted runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SeatKind {
    #[default]
    Human,
    Cpu { epa_row: u8 },
    /// Played on another PC over the network (online versus,
    /// docs/ONLINE-PLAN.md): this machine's `gather_input` leaves the slot
    /// alone; `net::NetSession` fills its `PendingInputs` entry each tick.
    Remote,
}

/// Which sim slots get a view (camera + HUD) on THIS machine, in view order.
/// `None` = every active slot, i.e. local split-screen exactly as before.
/// An online match sets `Some(vec![my_slot])`: one full-window view of a
/// two-fighter match. `views::ViewLayout` is chosen from this count, not from
/// `roster.count()` (docs/ONLINE-PLAN.md, lane A).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalViews(pub Option<Vec<usize>>);

impl LocalViews {
    /// Local play: only human seats get a view (a CPU has no screen). 1 human + CPUs = one full-window
    /// view, 2 humans + CPUs = a 2-way split, and so on. All humans (or none) = `None`, the
    /// ordinary one-view-per-slot layout. Online sets its own list instead (`Remote` seats).
    pub fn for_kinds(kinds: &SeatKinds, players: usize) -> Self {
        let humans: Vec<usize> = (0..players.min(MAX_SEATS)).filter(|&s| kinds.0[s] == SeatKind::Human).collect();
        if humans.is_empty() || humans.len() == players {
            LocalViews(None)
        } else {
            LocalViews(Some(humans))
        }
    }

    /// The slots shown here, given the match's active slots.
    pub fn slots(&self, roster: &crate::combat::Roster) -> Vec<usize> {
        match &self.0 {
            Some(v) => v.iter().copied().filter(|&s| roster.is_active(s)).collect(),
            None => roster.slots().collect(),
        }
    }

    /// The view index (layout cell) that shows `slot`, or `None` when this
    /// machine has no view of it.
    pub fn view_of(&self, slot: usize, roster: &crate::combat::Roster) -> Option<usize> {
        self.slots(roster).iter().position(|&s| s == slot)
    }

    /// The window layout for the views shown here: chosen from the number of
    /// local views, not the fighter count.
    pub fn layout(&self, roster: &crate::combat::Roster, split: crate::menu::Split) -> crate::views::ViewLayout {
        crate::views::ViewLayout::for_players(self.slots(roster).len(), split)
    }
}

/// The kind of each seat (= sim slot) of the next / current match. Defaults to all humans, which changes nothing.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SeatKinds(pub [SeatKind; MAX_SEATS]);

impl SeatKinds {
    pub fn any_cpu(&self) -> bool {
        self.0.iter().any(|k| matches!(k, SeatKind::Cpu { .. }))
    }
}

/// The players of the match being played (set when a match starts, by
/// `main.rs::enter_match` or the legacy CLI launch).
#[derive(Resource, Clone, Debug, Default)]
pub struct MatchPlayers(pub Vec<PlayerSetup>);

/// A player list the front end (or a test hook) wants the next match to use
/// instead of the old two-player `MatchSetup`; `enter_match` takes it.
#[derive(Resource, Default)]
pub struct RequestedPlayers(pub Option<Vec<PlayerSetup>>);

impl MatchPlayers {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn chars(&self) -> Vec<String> {
        self.0.iter().map(|p| p.char.clone()).collect()
    }

    pub fn colours(&self) -> Vec<u8> {
        self.0.iter().map(|p| p.colour).collect()
    }

    pub fn teams(&self) -> Vec<u8> {
        self.0.iter().map(|p| p.team).collect()
    }

    pub fn roster(&self) -> Roster {
        Roster::with_teams(&self.teams())
    }

    pub fn arsenals(&self) -> [Vec<i16>; MAX_SEATS] {
        std::array::from_fn(|i| self.0.get(i).map(|p| p.arsenal.clone()).unwrap_or_default())
    }

    /// Players for `chars` with the default costumes (`default_colours`),
    /// each on their own team, slot = seat.
    pub fn free_for_all(chars: &[String], arsenals: &[Vec<i16>]) -> Self {
        Self::with_teams(chars, arsenals, &(0..chars.len() as u8).collect::<Vec<_>>())
    }

    /// As `free_for_all` with explicit teams (`[0, 0, 1, 1]` is a 2v2).
    pub fn with_teams(chars: &[String], arsenals: &[Vec<i16>], teams: &[u8]) -> Self {
        let n = chars.len().min(MAX_SEATS);
        let colours = default_colours(&chars[..n]);
        MatchPlayers(
            (0..n)
                .map(|i| PlayerSetup {
                    char: chars[i].clone(),
                    colour: colours[i],
                    arsenal: arsenals.get(i).cloned().unwrap_or_default(),
                    team: teams.get(i).copied().unwrap_or(i as u8),
                    seat: i,
                })
                .collect(),
        )
    }
}

/// The default costume colour of each player: the first of a character keeps
/// colour 0, the next fighter with the same character gets 1, then 2, 3 (the
/// four colours every character has). With two players this is the old "P2
/// wears the alternate costume when both picked the same fighter".
pub fn default_colours(chars: &[String]) -> Vec<u8> {
    (0..chars.len()).map(|i| chars[..i].iter().filter(|c| **c == chars[i]).count().min(3) as u8).collect()
}

/// How an unseated pad gets a seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinMode {
    /// Pads take seats as they are found (the order Bevy lists them), like
    /// the two-player game always did.
    Auto,
    /// Seats are given out on the character-select screen (it does the
    /// joining itself, decided 2026-10-01); `Seats::update` -- which runs
    /// during the match -- only lets a pad that presses a button take back a
    /// seat that lost its pad. A spare pad can never take a new seat mid-match.
    Press,
}

/// Which gamepad plays which seat, and which seats have lost theirs.
#[derive(Resource, Debug)]
pub struct Seats {
    pub mode: JoinMode,
    pub pad: [Option<Entity>; MAX_SEATS],
    /// The seat had a pad that went away and has not been refilled. While a
    /// seat inside the match is lost, the match pauses.
    pub lost: [bool; MAX_SEATS],
}

impl Default for Seats {
    fn default() -> Self {
        Seats { mode: JoinMode::Auto, pad: [None; MAX_SEATS], lost: [false; MAX_SEATS] }
    }
}

impl Seats {
    /// The seat `pad` plays, if any.
    pub fn seat_of(&self, pad: Entity) -> Option<usize> {
        self.pad.iter().position(|p| *p == Some(pad))
    }

    /// Frees every seat (a fresh setup screen, a new match with new people):
    /// the front end calls this when it opens the join flow.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.pad = [None; MAX_SEATS];
        self.lost = [false; MAX_SEATS];
    }

    /// Whether a match of `players` should be paused: a pad that held one of
    /// its seats is missing.
    pub fn paused(&self, players: usize) -> bool {
        self.lost.iter().take(players).any(|&l| l)
    }

    /// One frame of seat bookkeeping. `present` = the connected pads in the
    /// order they are found, each with whether a button went down this frame.
    /// 1. pads that went away free their seat and mark it lost;
    /// 2. unseated pads take a seat: a lost seat first (a reconnecting pad
    ///    keeps its place), else -- `Auto` only -- the lowest free one. In
    ///    `Press` mode a pad only ever takes back a lost seat, and only on a
    ///    button press: joining happens on the character-select screen.
    pub fn update(&mut self, present: &[(Entity, bool)]) {
        for s in 0..MAX_SEATS {
            if let Some(e) = self.pad[s] {
                if !present.iter().any(|(p, _)| *p == e) {
                    self.pad[s] = None;
                    self.lost[s] = true;
                }
            }
        }
        for &(e, pressed) in present {
            if self.seat_of(e).is_some() || (self.mode == JoinMode::Press && !pressed) {
                continue;
            }
            let free = |s: usize| self.pad[s].is_none();
            let mut seat = (0..MAX_SEATS).find(|&s| free(s) && self.lost[s]);
            if seat.is_none() && self.mode == JoinMode::Auto {
                seat = (0..MAX_SEATS).find(|&s| free(s));
            }
            if let Some(s) = seat {
                self.pad[s] = Some(e);
                self.lost[s] = false;
            }
        }
    }
}

/// Runs every frame: keeps `Seats` in step with the connected gamepads.
pub fn update_seats(mut seats: ResMut<Seats>, pads: Query<(Entity, &Gamepad)>) {
    let present: Vec<(Entity, bool)> = pads.iter().map(|(e, g)| (e, g.get_just_pressed().next().is_some())).collect();
    seats.update(&present);
}

/// Seat names and colours (the HUD, the map quarter and the front end read
/// these; one place so a seat looks the same everywhere).
pub fn seat_name(seat: usize) -> String {
    format!("PLAYER {}", seat + 1)
}

/// A seat's colour (sRGB): orange, blue, green, magenta -- apart enough to
/// read as four dots on a map.
pub fn seat_colour(seat: usize) -> Color {
    const C: [(f32, f32, f32); MAX_SEATS] = [(1.0, 0.62, 0.15), (0.35, 0.65, 1.0), (0.45, 0.9, 0.4), (0.95, 0.4, 0.85)];
    let (r, g, b) = C[seat.min(MAX_SEATS - 1)];
    Color::srgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(n: u32) -> Entity {
        Entity::from_raw_u32(n).expect("valid entity index")
    }

    #[test]
    fn default_colours_give_duplicates_the_next_costume() {
        let c = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(default_colours(&c(&["pc00a", "pc01a"])), [0, 0], "different fighters keep colour 0");
        assert_eq!(default_colours(&c(&["pc00a", "pc00a"])), [0, 1], "the old P2 alternate costume rule");
        assert_eq!(default_colours(&c(&["pc00a", "pc01a", "pc00a", "pc00a"])), [0, 0, 1, 2]);
        assert_eq!(default_colours(&c(&["pc02a"; 4])), [0, 1, 2, 3]);
    }

    #[test]
    fn players_from_characters_fill_slot_team_and_colour() {
        let chars: Vec<String> = ["pc00a", "pc00a", "pc01a"].iter().map(|s| s.to_string()).collect();
        let p = MatchPlayers::with_teams(&chars, &[vec![1], vec![2], vec![3]], &[0, 1, 1]);
        assert_eq!(p.len(), 3);
        assert_eq!(p.colours(), [0, 1, 0]);
        assert_eq!(p.teams(), [0, 1, 1]);
        assert_eq!(p.roster(), Roster::with_teams(&[0, 1, 1]));
        assert_eq!(p.arsenals()[2], vec![3]);
        assert!(p.arsenals()[3].is_empty());
        assert!(p.0.iter().enumerate().all(|(i, s)| s.seat == i));
        // A two-player list is the old duel roster.
        assert_eq!(MatchPlayers::free_for_all(&chars[..2], &[]).roster(), Roster::duel());
    }

    #[test]
    fn auto_mode_seats_pads_in_the_order_found() {
        let mut s = Seats::default();
        s.update(&[(e(10), false), (e(11), false)]);
        assert_eq!(s.pad, [Some(e(10)), Some(e(11)), None, None]);
        // A third pad later takes the next seat; existing seats stay put.
        s.update(&[(e(11), false), (e(10), false), (e(12), false)]);
        assert_eq!(s.pad, [Some(e(10)), Some(e(11)), Some(e(12)), None]);
        assert!(!s.paused(4));
    }

    #[test]
    fn a_spare_pad_pressing_a_button_mid_match_never_takes_a_new_seat() {
        // Two players hold seats 1 and 2 when the match starts (the setup
        // screen did the joining); a third pad is plugged in and mashed.
        let mut s = Seats { mode: JoinMode::Press, pad: [Some(e(1)), Some(e(2)), None, None], ..Seats::default() };
        let present = [(e(1), false), (e(2), false), (e(3), true), (e(4), true)];
        s.update(&present);
        s.update(&present);
        assert_eq!(s.pad, [Some(e(1)), Some(e(2)), None, None], "no new seat for the spare pads");
        assert!(!s.paused(2));
        // Nor does a press take a seat while nobody has been seated at all.
        let mut empty = Seats { mode: JoinMode::Press, ..Seats::default() };
        empty.update(&[(e(5), true)]);
        assert_eq!(empty.pad, [None; MAX_SEATS]);
    }

    #[test]
    fn a_spare_pad_may_take_back_a_seat_that_lost_its_pad_but_only_on_a_press() {
        let mut s = Seats { mode: JoinMode::Press, pad: [Some(e(1)), Some(e(2)), None, None], ..Seats::default() };
        s.update(&[(e(1), false), (e(3), false)]);
        assert!(s.paused(2), "seat 2's pad went away");
        assert_eq!(s.pad[1], None);
        s.update(&[(e(1), false), (e(3), true)]);
        assert_eq!(s.pad, [Some(e(1)), Some(e(3)), None, None], "the lost seat is refilled, not seat 3");
        assert!(!s.paused(2));
    }

    #[test]
    fn a_dropped_pad_pauses_the_match_and_its_seat_is_kept_for_the_pad_that_comes_back() {
        let mut s = Seats::default();
        s.update(&[(e(1), false), (e(2), false), (e(3), false)]);
        // Pad 2 (seat 2) drops: the seat is lost, the match pauses.
        s.update(&[(e(1), false), (e(3), false)]);
        assert_eq!(s.pad, [Some(e(1)), None, Some(e(3)), None]);
        assert!(s.paused(3) && s.paused(2));
        assert!(!s.paused(1), "a one-player match does not care about seat 2");
        // It reconnects as a new entity and takes seat 2 back, not seat 4.
        s.update(&[(e(1), false), (e(3), false), (e(9), false)]);
        assert_eq!(s.pad, [Some(e(1)), Some(e(9)), Some(e(3)), None]);
        assert!(!s.paused(3));
    }

    #[test]
    fn in_press_mode_a_reconnecting_pad_needs_a_press_and_the_match_waits() {
        let mut s = Seats { mode: JoinMode::Press, pad: [Some(e(1)), Some(e(2)), None, None], ..Seats::default() };
        s.update(&[(e(1), false)]);
        assert!(s.paused(2));
        s.update(&[(e(1), false), (e(7), false)]);
        assert!(s.paused(2), "no button yet: still waiting");
        s.update(&[(e(1), false), (e(7), true)]);
        assert_eq!(s.pad[1], Some(e(7)));
        assert!(!s.paused(2));
    }

    #[test]
    fn seat_colours_are_distinct() {
        let c: Vec<_> = (0..MAX_SEATS).map(|i| format!("{:?}", seat_colour(i))).collect();
        assert!((0..MAX_SEATS).all(|i| (0..i).all(|j| c[i] != c[j])));
    }
}

#[cfg(test)]
mod local_views_tests {
    use super::*;
    use crate::combat::Roster;
    use crate::views::Kind;

    fn kinds(k: &[SeatKind]) -> SeatKinds {
        let mut s = SeatKinds::default();
        s.0[..k.len()].copy_from_slice(k);
        s
    }

    #[test]
    fn one_human_against_a_cpu_gets_one_full_window_view() {
        let cpu = SeatKind::Cpu { epa_row: 64 };
        let roster = Roster::duel();
        let lv = LocalViews::for_kinds(&kinds(&[SeatKind::Human, cpu]), 2);
        assert_eq!(lv, LocalViews(Some(vec![0])));
        assert_eq!(lv.layout(&roster, crate::menu::Split::SideBySide).kind, Kind::Full);
        assert_eq!(lv.view_of(0, &roster), Some(0));
        assert_eq!(lv.view_of(1, &roster), None, "the CPU has no view");
        // The CPU in slot 0 and the human in slot 1: the view is the human's.
        assert_eq!(LocalViews::for_kinds(&kinds(&[cpu, SeatKind::Human]), 2), LocalViews(Some(vec![1])));
    }

    #[test]
    fn humans_alone_keep_the_ordinary_layout_and_cpus_do_not_add_views() {
        let cpu = SeatKind::Cpu { epa_row: 64 };
        assert_eq!(LocalViews::for_kinds(&kinds(&[SeatKind::Human, SeatKind::Human]), 2), LocalViews(None));
        // 2 humans + 2 CPUs: a 2-way split of the humans' views.
        let lv = LocalViews::for_kinds(&kinds(&[SeatKind::Human, SeatKind::Human, cpu, cpu]), 4);
        assert_eq!(lv, LocalViews(Some(vec![0, 1])));
        let roster = Roster::free_for_all(4);
        assert_eq!(lv.slots(&roster).len(), 2);
        assert_ne!(lv.layout(&roster, crate::menu::Split::SideBySide).kind, Kind::Full);
        // 3 humans + 1 CPU: three views.
        assert_eq!(LocalViews::for_kinds(&kinds(&[SeatKind::Human, SeatKind::Human, SeatKind::Human, cpu]), 4).slots(&roster).len(), 3);
        // Nobody human (a CPU-only test match): every slot keeps its view.
        assert_eq!(LocalViews::for_kinds(&kinds(&[cpu, cpu]), 2), LocalViews(None));
    }
}
