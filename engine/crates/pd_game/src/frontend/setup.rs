//! Versus setup for one to four players, in the original's menu style, on two
//! screens (pages of one `AppState::Setup`):
//!
//! 1. **Characters** -- four seat columns. A seat is empty ("PRESS ANY BUTTON
//!    TO JOIN") until a pad takes it by pressing any button (the keyboard takes
//!    seat 1 with a menu key), or a joined player makes it a **CPU** (Y, or LB / RB
//!    to a seat and A: see `frontend/cpuseat.rs` for the grades and decks). A joined seat has a tall preview window (a live
//!    3D copy of its fighter facing the viewer, `frontend/charselect.rs`) and
//!    picks its character, costume colour, arsenal and (in teams mode) team,
//!    then readies up with A. B un-readies, then walks the cursor to the top,
//!    then leaves. When every joined seat (two or more) is ready -- or seat 1
//!    presses Start -- the game moves on to page 2. Joining only happens here.
//! 2. **Game rules** -- gamemode, team play, stage, game rule, time limit, rounds, starting
//!    level and skills, friendly fire, Start battle. Any joined seat's cursor
//!    can change them. B (or Esc) goes back to page 1 with every pick kept.
//!    With PROP HUNT as the gamemode the time and rounds rows become ROUND TIME and ROUNDS (the mode's own options,
//!    `prophunt.rs`); team play, game rule, level, skills and friendly fire are fixed; CPUs can't be added and a setup
//!    with CPUs or one person says why it can't start (2-4 people only).
//!
//! The setup is validated live (`SetupState::problem`): an arsenal that
//! doesn't meet the game rule is greyed with the reason, and a setup that
//! can't be played (nobody joined, no arsenal meets the rule, a teams match
//! with one team) says so and refuses to start. Starting hands the match its
//! players through `seats::RequestedPlayers` and the rest through
//! `MatchSetup`, and packs the joined seats down to the sim's slots `0..n`
//! (`Seats::pad` follows).

use super::cpuseat::{self, CpuSeat};
use super::modes;
use super::state::{AppState, GameData, MatchSetup, SavedArsenals, SkillCatalog, BY_RULE, DEFAULT_HAND_IDX, DEFAULT_LEVEL_IDX, DEFAULT_ROUNDS_IDX, DEFAULT_TIME_IDX, GAME_RULES, HAND_OPTIONS, LEVEL_OPTIONS, ROUNDS_OPTIONS, TIME_OPTIONS};
use super::ui::{self, Glyph, MenuArt, PixelText, RowRefs};
use crate::arsenal;
use crate::combat::{FriendlyFire, SkillDef, MAX_PLAYERS, RULE_TABLE};
use crate::seats::{seat_colour, JoinMode, PlayerSetup, RequestedPlayers, SeatKind, SeatKinds, Seats};
use bevy::prelude::*;

pub const SEATS: usize = MAX_PLAYERS;
/// Costume colours every character has.
pub const COLOURS: u8 = 4;

/// The two screens of the versus setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
    /// Per seat: join, character, colour, arsenal, team, ready.
    #[default]
    Characters,
    /// Shared: gamemode, team play, stage, game rule, time, rounds, level, skills, friendly fire, start.
    Rules,
}

// Page 1: the rows a seat's cursor visits, then the ready banner (not a cursor row).
const ROW_CHARACTER: usize = 0;
#[cfg(test)]
const ROW_ARSENAL: usize = 2;
#[cfg(test)]
const ROW_TEAM: usize = 3;
/// Rows a seat's cursor walks on page 1 (Team only in teams mode).
const SEAT_CURSOR_ROWS: usize = 4;
/// Lines in a seat's column: a human's four rows and the ready banner, a CPU's five rows.
const SEAT_LINES: usize = 5;

/// What one line of a seat's column holds. A human's lines are Character, Colour, Arsenal, Team and the ready banner; a
/// CPU's are Character, Colour, Strength (its grade), Arsenal (its deck) and Team. A column's cursor is a line index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Character,
    Colour,
    Strength,
    Arsenal,
    Team,
    Ready,
}

fn row_kind(cpu: bool, line: usize) -> Row {
    match (cpu, line) {
        (_, 0) => Row::Character,
        (_, 1) => Row::Colour,
        (false, 2) => Row::Arsenal,
        (false, 3) => Row::Team,
        (false, _) => Row::Ready,
        (true, 2) => Row::Strength,
        (true, 3) => Row::Arsenal,
        (true, _) => Row::Team,
    }
}

/// Lines a column's cursor visits: a human's Character..Team (Team only in teams mode), a CPU's Character..Team likewise.
fn cursor_lines(cpu: bool, teams: bool) -> usize {
    match (cpu, teams) {
        (false, false) => SEAT_CURSOR_ROWS - 1,
        (false, true) => SEAT_CURSOR_ROWS,
        (true, false) => SEAT_LINES - 1,
        (true, true) => SEAT_LINES,
    }
}

// Page 2: the shared rows.
/// GAMEMODES: STANDARD, then every entry of `modes::MODES` (Close Quarters, ...).
const R_GAMEMODE: usize = 0;
/// Teams or free for all.
const R_MODE: usize = 1;
const R_STAGE: usize = 2;
const R_RULE: usize = 3;
const R_TIME: usize = 4;
const R_ROUNDS: usize = 5;
const R_LEVEL: usize = 6;
const R_HAND: usize = 7;
const R_FRIENDLY: usize = 8;
const R_START: usize = 9;
const R_COUNT: usize = 10;

// Design-frame layout (1280x720). Page 1:
const SEAT_X0: f32 = 30.0;
const SEAT_STEP: f32 = 312.0;
const SEAT_W: f32 = 300.0;
/// The preview window: portrait (taller than wide), the whole fighter head to toe.
pub const WINDOW: (f32, f32) = (172.0, 248.0);
const SEAT_ROWS_Y: f32 = 380.0;
/// Five lines a seat: a human's four rows and the ready banner, or a CPU's five rows (caption 12 + panel 20).
const SEAT_ROW_STEP: f32 = 32.0;
const SEAT_ROW_H: f32 = 20.0;
const SEAT_CAPTION_H: f32 = 12.0;
const NOTICE_Y: f32 = 552.0;
// Page 2:
const PREVIEW: (f32, f32, f32, f32) = (60.0, 120.0, 420.0, 185.0);
const RULE_X: f32 = 520.0;
const RULE_W: f32 = 700.0;
const RULE_Y: f32 = 118.0;
const RULE_STEP: f32 = 42.0;
const RULE_H: f32 = 33.0;
/// Row text scale (pixel font).
const ROW_SCALE: f32 = 0.8;
const RULE_SCALE: f32 = 0.9;

/// One arsenal a seat can pick: a built-in or a legal saved one.
#[derive(Clone, Debug, PartialEq)]
pub struct ArsenalOption {
    pub name: String,
    pub skills: Vec<i16>,
    /// The declared "Number of Schools", when it has one.
    pub kind: Option<u8>,
}

/// What the setup reads from the game's data (borrowed, so the logic is
/// testable without Bevy).
pub struct Ctx<'a> {
    pub chars: &'a [super::discovery::CharacterEntry],
    pub stages: &'a [super::discovery::StageEntry],
    pub skills: &'a [SkillDef],
}

/// Why `deck` doesn't qualify for game rule `rule`, in words (None = it does).
pub fn rule_problem(rule: usize, deck: &[i16], skills: &[SkillDef]) -> Option<String> {
    if arsenal::eligible(rule, deck, skills) {
        return None;
    }
    let has_skill = arsenal::rule_flags(deck, skills) & arsenal::flag::HAS_SKILL != 0;
    let why = if !has_skill {
        "it has no skills"
    } else {
        match rule {
            6 => "its skills are not all from one school",
            7 => "it has the same skill more than once",
            8 => "it has a skill that costs more than 3 (or a charge skill)",
            9 => "it has a skill that costs less than 3 (or a charge skill)",
            _ => "it does not qualify",
        }
    };
    Some(why.to_string())
}

/// The one-line summary of a game rule: Health, Level range, time.
pub fn rule_summary(rule: usize) -> String {
    let r = RULE_TABLE[rule.min(11)];
    let level = if r.level_max >= 99 { format!("Level {}+", r.level_min) } else { format!("Level {}-{}", r.level_min, r.level_max) };
    format!("{}: {} Health, {level}, {} min.", arsenal::RULE_NAMES[rule.min(11)], r.health, r.minutes)
}

/// The teal line under the stage picture: the game rule's summary, or in a gamemode the mode's name with what it
/// plays as (its rule's Health and time, its own starting Level), so the line never names a rule the player didn't pick.
pub fn rule_line(state: &SetupState) -> String {
    let Some(m) = state.mode() else { return rule_summary(state.rule()) };
    if state.prop_hunt() {
        let c = state.prop_hunt_config();
        let secs = c.hunt_ticks / 60;
        return format!("{}: rounds of {}:{:02}, the seeker has {} knives, first to {} kills wins.", m.name, secs / 60, secs % 60, crate::prophunt::KNIFE_ATTEMPTS, c.kills_to_win);
    }
    let r = RULE_TABLE[state.rule().min(11)];
    format!("{}: {} Health, Level {}, {} min.", m.name, r.health, m.rules.start_level.clamp(r.level_min, r.level_max), r.minutes)
}

/// Teams for `n` joined players, in seat order: the first half on team 0, the
/// rest on team 1 (`[0, 1]`, `[0, 0, 1]`, `[0, 0, 1, 1]`).
pub fn default_teams(n: usize) -> Vec<u8> {
    (0..n).map(|i| u8::from(i >= n.div_ceil(2))).collect()
}

/// What a valid setup becomes when started.
pub struct StartPlan {
    pub players: Vec<PlayerSetup>,
    /// Who plays each slot (humans, and the CPUs with their `enemyparam.epa` row): `seats::SeatKinds` for the match.
    pub kinds: SeatKinds,
    pub setup: MatchSetup,
    /// The seats that play, in slot order (slot `k` is `seats[k]`).
    pub seats: Vec<usize>,
}

#[derive(Resource)]
pub struct SetupState {
    /// Which of the two screens is showing.
    pub(crate) page: Page,
    pub(crate) joined: [bool; SEATS],
    /// Seats that pressed A on page 1 (cleared when the page changes).
    pub(crate) ready: [bool; SEATS],
    /// Page 1 cursor per seat (`ROW_CHARACTER..=ROW_TEAM`).
    cursor: [usize; SEATS],
    /// Page 2 cursor per seat (`R_MODE..=R_START`).
    rcursor: [usize; SEATS],
    char_idx: [usize; SEATS],
    colour: [u8; SEATS],
    arsenal: [usize; SEATS],
    team: [u8; SEATS],
    teams_mode: bool,
    stage: usize,
    rule_idx: usize,
    time_idx: usize,
    rounds_idx: usize,
    level_idx: usize,
    hand_idx: usize,
    friendly_fire: bool,
    /// Which seat each scripted pad (`p1:`..`p4:`) took.
    virt: [Option<usize>; SEATS],
    /// Which seat moved last: the description line explains its row.
    last_mover: usize,
    repeat: [super::input::Repeat; SEATS],
    options: Vec<ArsenalOption>,
    /// Set by the result screen so the setup it returns to keeps its seats.
    rematch: bool,
    /// Seats a joined human made a CPU (`joined` is true for them too): grade and deck.
    cpu: [Option<CpuSeat>; SEATS],
    /// Which seat's column each human's cursor is editing: its own, a CPU's, or an empty seat's (to make it a CPU).
    focus: [usize; SEATS],
    /// What a CPU can play: the original's enemy decks, then the player's saved arsenals.
    cpu_options: Vec<ArsenalOption>,
    /// The five grades' numbers from `enemyparam.epa` (None: the file isn't there, so no CPU can be added).
    grades: Option<[cpuseat::GradeInfo; cpuseat::GRADES]>,
    /// The CPU script and its data were found.
    cpu_data: bool,
    /// Stage ids a CPU can navigate (the original's `comdata*.cad` exists); None = not checked.
    nav_stages: Option<Vec<String>>,
    /// The game mode being set up (`modes::MODES` index), None = the normal versus. A mode fixes every fighter's hand,
    /// the game rule, the starting Level and skills: those rows are locked and the cursor skips them.
    mode: Option<usize>,
    /// The one "arsenal" every seat plays in a mode: the mode's hand (`set_mode`).
    mode_deck: Option<ArsenalOption>,
    /// Prop Hunt's ROUND TIME row (`prophunt::ROUND_TIME_OPTIONS` index).
    ph_time_idx: usize,
    /// Prop Hunt's KILLS TO WIN row (`prophunt::KILLS_OPTIONS` index).
    ph_kills_idx: usize,
    /// Where each seat's picks are remembered between runs (`player_memory`); `None` in screenshot / test runs.
    memory_path: Option<std::path::PathBuf>,
}

impl Default for SetupState {
    fn default() -> Self {
        SetupState {
            page: Page::Characters,
            joined: [false; SEATS],
            ready: [false; SEATS],
            cursor: [0; SEATS],
            rcursor: [R_START; SEATS],
            char_idx: [0, 1, 2, 3],
            colour: [0; SEATS],
            arsenal: [0; SEATS],
            team: [0, 0, 1, 1],
            teams_mode: false,
            stage: 0,
            rule_idx: 0,
            time_idx: DEFAULT_TIME_IDX,
            rounds_idx: DEFAULT_ROUNDS_IDX,
            level_idx: DEFAULT_LEVEL_IDX,
            hand_idx: DEFAULT_HAND_IDX,
            friendly_fire: false,
            virt: [None; SEATS],
            last_mover: 0,
            repeat: Default::default(),
            options: Vec::new(),
            rematch: false,
            cpu: [None; SEATS],
            focus: [0, 1, 2, 3],
            cpu_options: Vec::new(),
            grades: None,
            cpu_data: false,
            nav_stages: None,
            mode: None,
            mode_deck: None,
            ph_time_idx: PH_DEFAULT_TIME_IDX,
            ph_kills_idx: PH_DEFAULT_KILLS_IDX,
            memory_path: None,
        }
    }
}

/// Prop Hunt's default ROUND TIME choice: 2:00.
const PH_DEFAULT_TIME_IDX: usize = 2;
/// Prop Hunt's default KILLS TO WIN choice: 5.
const PH_DEFAULT_KILLS_IDX: usize = 4;

/// "2:00", "1:30" for a round time in seconds.
fn ph_time_label(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

impl SetupState {
    /// Sets up for game mode `mode` (None = the normal versus): everyone's arsenal is the mode's hand, the game rule goes
    /// back to Standard (a mode's own Level and hand replace the rest). Aura recovery uses the neutral kind
    /// (`arsenal::DEFAULT_KIND`: the hand has no declared "Number of Schools", and a school count would hand some fighters a
    /// slower refill than others).
    pub fn set_mode(&mut self, mode: Option<usize>) {
        self.mode = mode.filter(|&m| modes::get(m).is_some());
        self.mode_deck = self.mode().map(|m| match m.kind {
            modes::Kind::Versus => ArsenalOption { name: "FIXED HAND".into(), skills: m.hand.to_vec(), kind: Some(arsenal::DEFAULT_KIND) },
            // Prop Hunt hands out its own roles each round: the seeker's knife, nothing for the hiders.
            modes::Kind::PropHunt => ArsenalOption { name: "SEEKER KNIFE / HIDER NONE".into(), skills: vec![m.hand[0]], kind: Some(arsenal::DEFAULT_KIND) },
        });
        if self.mode.is_some() {
            self.rule_idx = 0;
        }
        if self.prop_hunt() {
            // Everyone for themselves: the roles are the teams.
            self.teams_mode = false;
        }
    }

    /// Prop Hunt is the mode being set up.
    pub fn prop_hunt(&self) -> bool {
        modes::is_prop_hunt(self.mode)
    }

    /// Prop Hunt's round time as chosen (there is no round limit: first to 5 kills).
    pub fn prop_hunt_config(&self) -> crate::prophunt::Config {
        use crate::prophunt::ROUND_TIME_OPTIONS;
        crate::prophunt::Config {
            hunt_ticks: ROUND_TIME_OPTIONS[self.ph_time_idx.min(ROUND_TIME_OPTIONS.len() - 1)] * crate::sim::HZ as u32,
            kills_to_win: crate::prophunt::KILLS_OPTIONS[self.ph_kills_idx.min(crate::prophunt::KILLS_OPTIONS.len() - 1)],
        }
    }

    /// CPUs can be added: their data is there and the mode takes them (Prop Hunt is people only).
    fn cpus_allowed(&self) -> bool {
        self.cpu_available() && !self.prop_hunt()
    }

    pub fn mode(&self) -> Option<&'static modes::GameMode> {
        self.mode.and_then(modes::get)
    }

    /// A page-2 row a mode fixes (game rule, starting Level, starting skills): not selectable, not changeable.
    fn shared_locked(&self, row: usize) -> bool {
        (self.mode.is_some() && matches!(row, R_RULE | R_LEVEL | R_HAND)) || (self.prop_hunt() && matches!(row, R_MODE | R_FRIENDLY))
    }

    /// A page-2 row that is not shown at all: Friendly fire outside teams (Laurence, 2026-10-09: de-clutter, item 5).
    fn shared_hidden(&self, row: usize) -> bool {
        row == R_FRIENDLY && !self.teams_mode
    }

    /// Where page-2 row `row` is drawn: the rows after a hidden one move up.
    fn shared_slot(&self, row: usize) -> usize {
        row - (0..row).filter(|&r| self.shared_hidden(r)).count()
    }

    /// A line of a seat's column that is not shown: Team outside teams (item 5; the cursor already skips it).
    fn seat_line_hidden(&self, seat: usize, line: usize) -> bool {
        row_kind(self.is_cpu(seat), line) == Row::Team && !self.teams_mode
    }

    /// Where line `line` of seat `seat` is drawn: a person's ready banner moves up into the hidden Team line's place.
    fn seat_slot(&self, seat: usize, line: usize) -> usize {
        if !self.teams_mode && row_kind(self.is_cpu(seat), line) == Row::Ready { line - 1 } else { line }
    }

    /// A line of a seat's column a mode fixes (the arsenal).
    fn seat_line_locked(&self, cpu: bool, line: usize) -> bool {
        self.mode.is_some() && row_kind(cpu, line) == Row::Arsenal
    }

    /// The cursor line after one step up or down from `from`, over `rows` lines, skipping what a mode fixes.
    fn step_line(&self, cpu: bool, from: usize, rows: usize, up: bool) -> usize {
        let mut c = from.min(rows - 1);
        for _ in 0..rows {
            c = if up { (c + rows - 1) % rows } else { (c + 1) % rows };
            if !self.seat_line_locked(cpu, c) {
                break;
            }
        }
        c
    }

    /// The result screen's "Continue": the next visit to the setup keeps the
    /// same seats, pads and picks for a rematch (and opens on page 1).
    pub fn mark_rematch(&mut self) {
        self.rematch = true;
    }

    /// Seats taken, people and CPUs.
    pub fn joined_count(&self) -> usize {
        self.joined.iter().filter(|&&j| j).count()
    }

    pub fn is_cpu(&self, seat: usize) -> bool {
        self.cpu[seat].is_some()
    }

    /// A joined seat a person plays.
    pub fn is_human(&self, seat: usize) -> bool {
        self.joined[seat] && self.cpu[seat].is_none()
    }

    pub fn human_count(&self) -> usize {
        (0..SEATS).filter(|&s| self.is_human(s)).count()
    }

    pub fn cpu_count(&self) -> usize {
        self.cpu.iter().flatten().count()
    }

    /// Whether a CPU can be added at all: the CPU script and `enemyparam.epa` were found, and there is a deck to give it.
    pub fn cpu_available(&self) -> bool {
        self.cpu_data && self.grades.is_some() && !self.cpu_options.is_empty()
    }

    /// "PLAYER 2" or "CPU 2".
    pub fn seat_name(&self, seat: usize) -> String {
        if self.is_cpu(seat) {
            format!("CPU {}", seat + 1)
        } else {
            crate::seats::seat_name(seat)
        }
    }

    /// The arsenal seat `seat` plays: a person's own pick, or a CPU's deck.
    pub fn seat_arsenal(&self, seat: usize) -> Option<&ArsenalOption> {
        if let Some(deck) = &self.mode_deck {
            return Some(deck);
        }
        match self.cpu[seat] {
            Some(c) => self.cpu_options.get(c.deck),
            None => self.options.get(self.arsenal[seat]),
        }
    }

    /// Whether the chosen stage has a nav graph for the CPU (true when that was not checked).
    pub fn stage_navigable(&self, ctx: &Ctx) -> bool {
        match (&self.nav_stages, ctx.stages.get(self.stage)) {
            (Some(list), Some(st)) => list.iter().any(|id| *id == st.id),
            _ => true,
        }
    }

    /// The lowest empty seat.
    pub fn free_seat(&self) -> Option<usize> {
        (0..SEATS).find(|&s| !self.joined[s])
    }

    /// The game rule id in force (`arsenal::RULE_NAMES` index).
    pub fn rule(&self) -> usize {
        GAME_RULES[self.rule_idx.min(GAME_RULES.len() - 1)]
    }

    /// Per joined seat, the character index and colour its preview shows.
    pub fn previews(&self) -> [Option<(usize, u8)>; SEATS] {
        std::array::from_fn(|s| self.joined[s].then_some((self.char_idx[s], self.colour[s])))
    }

    /// Seat `seat` takes its place: its colour is made distinct from the
    /// others wearing the same character, its arsenal is the first one that
    /// meets the rule, and in teams mode the teams are dealt again. It is not
    /// ready.
    pub fn join(&mut self, seat: usize, ctx: &Ctx) {
        self.joined[seat] = true;
        self.cpu[seat] = None;
        self.focus[seat] = seat;
        self.ready[seat] = false;
        self.cursor[seat] = 0;
        self.rcursor[seat] = R_START;
        self.fix_colour(seat);
        self.arsenal[seat] = self.first_eligible(self.arsenal[seat], ctx.skills);
        self.deal_teams();
    }

    /// A person gives up their seat (a CPU seat is just removed). The last person leaving takes every CPU with them.
    pub fn leave(&mut self, seat: usize) {
        if self.is_cpu(seat) {
            self.remove_cpu(seat);
            return;
        }
        self.joined[seat] = false;
        self.ready[seat] = false;
        self.focus[seat] = seat;
        for v in &mut self.virt {
            if *v == Some(seat) {
                *v = None;
            }
        }
        if self.human_count() == 0 {
            self.clear_cpus();
        }
        self.deal_teams();
    }

    /// Makes empty seat `seat` a CPU: a character nobody else plays, the middle grade, the next of the original's decks (the
    /// first that meets the game rule), a free colour; teams are dealt again. Refused (false) on seat 1 (the keyboard's),
    /// a taken seat, with nobody to play against, or without the CPU's data.
    pub fn add_cpu(&mut self, seat: usize, ctx: &Ctx) -> bool {
        if seat == 0 || seat >= SEATS || self.joined[seat] || self.human_count() == 0 || !self.cpus_allowed() {
            return false;
        }
        let ordinal = self.cpu_count();
        self.joined[seat] = true;
        self.ready[seat] = false;
        self.cursor[seat] = 0;
        self.rcursor[seat] = R_START;
        self.cpu[seat] = Some(CpuSeat { grade: cpuseat::DEFAULT_GRADE, deck: ordinal % self.cpu_options.len() });
        if let Some(c) = (0..ctx.chars.len()).find(|&c| (0..SEATS).all(|o| o == seat || !self.joined[o] || self.char_idx[o] != c)) {
            self.char_idx[seat] = c;
        }
        self.fix_colour(seat);
        let rule = self.rule();
        if let Some(c) = self.cpu[seat].as_mut() {
            c.deck = Self::first_eligible_in(&self.cpu_options, c.deck, rule, ctx.skills);
        }
        self.deal_teams();
        true
    }

    /// Takes the CPU out of `seat`; anyone editing its column goes back to their own.
    pub fn remove_cpu(&mut self, seat: usize) {
        if self.cpu[seat].take().is_none() {
            return;
        }
        self.joined[seat] = false;
        self.ready[seat] = false;
        for h in 0..SEATS {
            if self.focus[h] == seat {
                self.focus[h] = h;
            }
        }
        self.deal_teams();
    }

    fn clear_cpus(&mut self) {
        for s in 0..SEATS {
            self.remove_cpu(s);
        }
    }

    /// Whether every person is ready and there are at least two seats taken (a CPU is always ready; one person alone has to
    /// press Start: that is a practice match).
    pub fn all_ready(&self) -> bool {
        self.joined_count() >= 2 && self.human_count() >= 1 && (0..SEATS).filter(|&s| self.is_human(s)).all(|s| self.ready[s])
    }

    /// On to the game rules (page 2): ready marks are cleared, every pick stays.
    fn go_rules(&mut self) {
        self.page = Page::Rules;
        self.ready = [false; SEATS];
        self.rcursor = [R_START; SEATS];
        self.focus = [0, 1, 2, 3];
    }

    /// Back to the characters (page 1) with every pick kept; nobody is ready.
    fn go_characters(&mut self) {
        self.page = Page::Characters;
        self.ready = [false; SEATS];
    }

    /// Opening the setup from the title: a fresh join flow on page 1.
    fn reset_seats(&mut self) {
        self.page = Page::Characters;
        self.joined = [false; SEATS];
        self.cpu = [None; SEATS];
        self.focus = [0, 1, 2, 3];
        self.ready = [false; SEATS];
        self.virt = [None; SEATS];
        self.cursor = [0; SEATS];
        self.rcursor = [R_START; SEATS];
        self.repeat = Default::default();
    }

    /// In teams mode, a fresh even split over the joined seats.
    fn deal_teams(&mut self) {
        if !self.teams_mode {
            return;
        }
        let seats: Vec<usize> = (0..SEATS).filter(|&s| self.joined[s]).collect();
        for (&s, t) in seats.iter().zip(default_teams(seats.len())) {
            self.team[s] = t;
        }
    }

    /// Colours of `seat`'s character that another joined seat already wears.
    fn colours_taken(&self, seat: usize) -> [bool; COLOURS as usize] {
        let mut taken = [false; COLOURS as usize];
        for o in (0..SEATS).filter(|&o| o != seat && self.joined[o] && self.char_idx[o] == self.char_idx[seat]) {
            taken[self.colour[o] as usize % COLOURS as usize] = true;
        }
        taken
    }

    /// Keeps `seat`'s colour if free, else the lowest free one.
    fn fix_colour(&mut self, seat: usize) {
        let taken = self.colours_taken(seat);
        if taken[self.colour[seat] as usize % COLOURS as usize] {
            if let Some(c) = (0..COLOURS).find(|&c| !taken[c as usize]) {
                self.colour[seat] = c;
            }
        }
    }

    fn cycle_colour(&mut self, seat: usize, forward: bool) {
        let taken = self.colours_taken(seat);
        let n = COLOURS as usize;
        let mut c = self.colour[seat] as usize;
        for _ in 0..n {
            c = if forward { (c + 1) % n } else { (c + n - 1) % n };
            if !taken[c] {
                self.colour[seat] = c as u8;
                return;
            }
        }
    }

    fn change_character(&mut self, seat: usize, forward: bool, count: usize) {
        let count = count.max(1);
        self.char_idx[seat] = if forward { (self.char_idx[seat] + 1) % count } else { (self.char_idx[seat] + count - 1) % count };
        self.fix_colour(seat);
    }

    /// `current` if it meets the rule in force, else the first option that
    /// does (else `current`, left greyed).
    fn first_eligible(&self, current: usize, skills: &[SkillDef]) -> usize {
        Self::first_eligible_in(&self.options, current, self.rule(), skills)
    }

    fn first_eligible_in(options: &[ArsenalOption], current: usize, rule: usize, skills: &[SkillDef]) -> usize {
        let ok = |i: usize| options.get(i).is_some_and(|o| arsenal::eligible(rule, &o.skills, skills));
        if ok(current) {
            current
        } else {
            (0..options.len()).find(|&i| ok(i)).unwrap_or(current)
        }
    }

    /// Why seat `seat`'s arsenal (a person's pick, or a CPU's deck) doesn't meet the rule, if it doesn't.
    pub fn arsenal_problem(&self, seat: usize, skills: &[SkillDef]) -> Option<String> {
        if self.mode.is_some() {
            return None; // the mode's hand is dealt whatever the game rule says
        }
        let o = self.seat_arsenal(seat)?;
        rule_problem(self.rule(), &o.skills, skills)
    }

    /// Whether the rule in force can be played with the arsenals on offer.
    pub fn rule_available(&self, skills: &[SkillDef]) -> bool {
        self.mode.is_some() || arsenal::rule_available(self.rule(), self.options.iter().map(|o| o.skills.as_slice()), skills)
    }

    /// What stops this setup from starting, in words (None = it can start).
    pub fn problem(&self, ctx: &Ctx) -> Option<String> {
        if self.human_count() == 0 {
            return Some("Press any button on a pad (or Enter) to join.".into());
        }
        if ctx.chars.is_empty() || ctx.stages.is_empty() {
            return Some("No characters or stages found in the cache.".into());
        }
        if self.mode.is_none() && (self.options.is_empty() || !self.rule_available(ctx.skills)) {
            return Some("No arsenals meet the rules.".into());
        }
        if self.prop_hunt() {
            if self.cpu_count() > 0 {
                return Some("Prop Hunt is for 2-4 people, no CPUs: remove the CPU seats on the previous screen (B).".into());
            }
            if self.human_count() < 2 {
                return Some("Prop Hunt needs at least two people: a second player can join on the previous screen (B).".into());
            }
        }
        for s in (0..SEATS).filter(|&s| self.joined[s]) {
            if let Some(why) = self.arsenal_problem(s, ctx.skills) {
                let name = self.seat_arsenal(s).map_or("", |o| o.name.as_str());
                return Some(format!("{}: {} doesn't meet the rules ({why}).", self.seat_name(s), name));
            }
        }
        if let Some(p) = self.first_player_problem() {
            return Some(p);
        }
        if self.cpu_count() > 0 && !self.stage_navigable(ctx) {
            let name = ctx.stages.get(self.stage).map_or("this stage", |s| s.name.as_str());
            return Some(format!("A CPU can't play on {name}: the original has no navigation data for it. Pick another stage."));
        }
        if self.teams_mode {
            let teams: Vec<u8> = (0..SEATS).filter(|&s| self.joined[s]).map(|s| self.team[s]).collect();
            if teams.iter().all(|&t| t == teams[0]) {
                return Some("Teams need an opponent: put players on both teams.".into());
            }
        }
        None
    }

    /// The players and rules of a valid setup.
    pub fn plan(&self, ctx: &Ctx) -> Result<StartPlan, String> {
        if let Some(p) = self.problem(ctx) {
            return Err(p);
        }
        let seats: Vec<usize> = (0..SEATS).filter(|&s| self.joined[s]).collect();
        let stage = ctx.stages.get(self.stage).ok_or("No stage picked.")?;
        let mut players = Vec::new();
        let mut kinds = [None; MAX_PLAYERS];
        let mut seat_kinds = SeatKinds::default();
        for (slot, &s) in seats.iter().enumerate() {
            let c = ctx.chars.get(self.char_idx[s]).ok_or("No character picked.")?;
            let a = self.seat_arsenal(s).ok_or("No arsenal picked.")?;
            let team = if self.teams_mode { self.team[s] } else { slot as u8 };
            players.push(PlayerSetup { char: c.id.clone(), colour: self.colour[s], arsenal: a.skills.clone(), team, seat: slot });
            kinds[slot] = a.kind;
            if let Some(cpu) = self.cpu[s] {
                seat_kinds.0[slot] = SeatKind::Cpu { epa_row: cpuseat::grade_row(cpu.grade) };
            }
        }
        let first = |k: usize| players.get(k).map_or_else(|| MatchSetup::default().chars[k].clone(), |p| p.char.clone());
        let time = TIME_OPTIONS[self.time_idx.min(TIME_OPTIONS.len() - 1)].1;
        let setup = MatchSetup {
            chars: [first(0), first(1)],
            arsenals: [players.first().map(|p| p.arsenal.clone()).unwrap_or_default(), players.get(1).map(|p| p.arsenal.clone()).unwrap_or_default()],
            stage: stage.id.clone(),
            time_limit_ticks: if time == BY_RULE { crate::combat::TIME_LIMIT_TICKS } else { time },
            time_by_rule: time == BY_RULE,
            rounds_to_win: ROUNDS_OPTIONS[self.rounds_idx].1,
            start_level: LEVEL_OPTIONS[self.level_idx],
            start_hand: HAND_OPTIONS[self.hand_idx],
            rule: self.rule(),
            friendly_fire: if self.teams_mode && self.friendly_fire { FriendlyFire::On } else { FriendlyFire::Off },
            arsenal_kinds: kinds,
            mode: self.mode,
            prop_hunt: self.prop_hunt_config(),
        };
        Ok(StartPlan { players, kinds: seat_kinds, setup, seats })
    }

    /// What pressing `a` does for seat `seat`; returns what the screen should
    /// do about it. Page changes happen inside.
    fn act(&mut self, seat: usize, a: &super::input::MenuAxis, ctx: &Ctx) -> Act {
        self.last_mover = seat;
        match self.page {
            Page::Characters => self.act_characters(seat, a, ctx),
            Page::Rules => self.act_rules(seat, a, ctx),
        }
    }

    fn act_characters(&mut self, seat: usize, a: &super::input::MenuAxis, ctx: &Ctx) -> Act {
        // Seat 1's Start goes on to the rules whenever it likes, ready or not.
        if a.start && seat == 0 {
            self.go_rules();
            return Act::None;
        }
        if self.ready[seat] {
            // Ready: nothing changes until B takes the ready mark back.
            if a.cancel {
                self.ready[seat] = false;
            }
            return Act::None;
        }
        // CPU seats: LB / RB pick the column this cursor edits, Y adds a CPU, and a cursor on another column edits that one.
        self.fix_focus(seat);
        if a.lb || a.rb {
            self.cycle_focus(seat, a.rb);
            return Act::None;
        }
        if a.y {
            return self.add_cpu_from(seat, ctx);
        }
        if self.focus[seat] != seat {
            return self.act_other_column(seat, self.focus[seat], a, ctx);
        }
        if a.up || a.down {
            let rows = cursor_lines(false, self.teams_mode);
            self.cursor[seat] = self.step_line(false, self.cursor[seat], rows, a.up);
        }
        if a.left || a.right {
            self.change_seat(seat, self.cursor[seat], a.right, ctx);
        }
        if a.confirm {
            if self.arsenal_problem(seat, ctx.skills).is_some() {
                return Act::Refused;
            }
            self.ready[seat] = true;
            if self.all_ready() {
                self.go_rules();
            }
            return Act::None;
        }
        if a.cancel {
            // B walks the cursor to the top of the column; from the top it leaves.
            if self.cursor[seat] != ROW_CHARACTER {
                self.cursor[seat] = ROW_CHARACTER;
            } else {
                return Act::Leave;
            }
        }
        Act::None
    }

    /// The column `human`'s cursor is editing: their own unless it is on a CPU (or an empty seat that can still become one).
    pub fn edit_column(&self, human: usize) -> usize {
        if self.focus_targets(human).contains(&self.focus[human]) {
            self.focus[human]
        } else {
            human
        }
    }

    /// The people's seats' picks now, by name, over `old` (a CPU or empty seat keeps its older entry).
    pub fn remembered(&self, ctx: &Ctx, old: &super::player_memory::PlayerMemory) -> super::player_memory::PlayerMemory {
        let mut m = old.clone();
        for s in (0..SEATS).filter(|&s| self.is_human(s)) {
            let Some(c) = ctx.chars.get(self.char_idx[s]) else { continue };
            m.seats[s] = Some(super::player_memory::SeatPick {
                character: c.id.clone(),
                colour: self.colour[s],
                arsenal: self.options.get(self.arsenal[s]).map_or_else(String::new, |a| a.name.clone()),
                team: self.team[s],
            });
        }
        m
    }

    /// Fill each seat's picks from `mem` (an unknown character or arsenal leaves that pick as it is). Joining then
    /// checks them as usual (`join`: a colour clash moves the colour, an arsenal the rule refuses moves on).
    pub fn recall(&mut self, ctx: &Ctx, mem: &super::player_memory::PlayerMemory) {
        for (s, p) in mem.seats.iter().enumerate() {
            let Some(p) = p else { continue };
            if let Some(i) = ctx.chars.iter().position(|c| c.id == p.character) {
                self.char_idx[s] = i;
            }
            self.colour[s] = p.colour % COLOURS;
            if let Some(a) = self.options.iter().position(|a| a.name == p.arsenal) {
                self.arsenal[s] = a;
            }
            self.team[s] = p.team.min(1);
        }
    }

    /// Whether B from the top of `seat`'s column leaves the setup (back to the title) rather than only the seat:
    /// `seat` is player 1, the first person seated.
    pub fn leaving_goes_back(&self, seat: usize) -> bool {
        (0..SEATS).find(|&s| self.is_human(s)) == Some(seat)
    }

    /// Slot 0 is the keyboard's seat and a CPU has no controller: the first taken seat must be a person.
    pub fn first_player_problem(&self) -> Option<String> {
        let first = (0..SEATS).find(|&s| self.joined[s])?;
        self.is_cpu(first).then(|| "The first player must be a person: take an earlier seat (any button on a pad, or Enter).".to_string())
    }

    /// The seats `human`'s cursor can sit on, in seat order: their own, every CPU's, and every empty seat after the first (to make
    /// it a CPU, when there is a CPU to add).
    fn focus_targets(&self, human: usize) -> Vec<usize> {
        (0..SEATS).filter(|&s| s == human || self.is_cpu(s) || (!self.joined[s] && s > 0 && self.cpus_allowed())).collect()
    }

    /// Puts `human`'s cursor back on their own column if the one it was on is no longer there (a CPU removed, a seat taken).
    fn fix_focus(&mut self, human: usize) {
        if !self.focus_targets(human).contains(&self.focus[human]) {
            self.focus[human] = human;
        }
    }

    /// LB / RB: the next (RB) or previous column `human` can edit.
    fn cycle_focus(&mut self, human: usize, forward: bool) {
        let t = self.focus_targets(human);
        if t.len() < 2 {
            return;
        }
        let at = t.iter().position(|&s| s == self.focus[human]).unwrap_or(0);
        self.focus[human] = if forward { t[(at + 1) % t.len()] } else { t[(at + t.len() - 1) % t.len()] };
    }

    /// Y: a CPU in the empty seat `human`'s cursor is on, else the lowest empty seat; the cursor goes to it so it can be set up.
    fn add_cpu_from(&mut self, human: usize, ctx: &Ctx) -> Act {
        let target = if !self.joined[self.focus[human]] && self.focus[human] > 0 { Some(self.focus[human]) } else { (1..SEATS).find(|&s| !self.joined[s]) };
        match target {
            Some(t) if self.add_cpu(t, ctx) => {
                self.focus[human] = t;
                Act::None
            }
            _ => Act::Refused,
        }
    }

    /// A press while `human`'s cursor is on another seat's column `col`: an empty seat (A or Y makes it a CPU, B goes back) or a
    /// CPU (up / down / left / right edit its rows, X or B at the top removes it, A is done and goes back to the own column).
    fn act_other_column(&mut self, human: usize, col: usize, a: &super::input::MenuAxis, ctx: &Ctx) -> Act {
        if !self.is_cpu(col) {
            if a.confirm {
                return self.add_cpu_from(human, ctx);
            }
            if a.cancel {
                self.focus[human] = human;
            }
            return Act::None;
        }
        if a.x {
            self.remove_cpu(col);
            self.focus[human] = human;
            return Act::None;
        }
        if a.up || a.down {
            let rows = cursor_lines(true, self.teams_mode);
            self.cursor[col] = self.step_line(true, self.cursor[col], rows, a.up);
        }
        if a.left || a.right {
            self.change_seat(col, self.cursor[col], a.right, ctx);
        }
        if a.confirm {
            self.focus[human] = human;
        } else if a.cancel {
            // B walks the cursor to the top of the column; from the top it removes the CPU.
            if self.cursor[col] != 0 {
                self.cursor[col] = 0;
            } else {
                self.remove_cpu(col);
                self.focus[human] = human;
            }
        }
        Act::None
    }

    fn act_rules(&mut self, seat: usize, a: &super::input::MenuAxis, ctx: &Ctx) -> Act {
        // A mode's fixed rows are skipped by the cursor.
        for (pressed, step) in [(a.up, R_COUNT - 1), (a.down, 1)] {
            if pressed {
                let mut r = self.rcursor[seat];
                for _ in 0..R_COUNT {
                    r = (r + step) % R_COUNT;
                    if !self.shared_locked(r) && !self.shared_hidden(r) {
                        break;
                    }
                }
                self.rcursor[seat] = r;
            }
        }
        let row = self.rcursor[seat];
        if a.left || a.right {
            self.change_rule_row(row, a.right, ctx);
        }
        // A change (Team play back to free for all) can hide the row a cursor is on.
        if !self.teams_mode {
            for r in &mut self.rcursor {
                if *r == R_FRIENDLY {
                    *r = R_START;
                }
            }
        }
        if (a.confirm && row == R_START) || (a.start && seat == 0) {
            return Act::Start;
        }
        if a.cancel {
            self.go_characters();
        }
        Act::None
    }

    /// Left / right on one of a seat's own rows.
    fn change_seat(&mut self, seat: usize, line: usize, forward: bool, ctx: &Ctx) {
        let step = |cur: usize, len: usize| if forward { (cur + 1) % len.max(1) } else { (cur + len.max(1) - 1) % len.max(1) };
        if self.seat_line_locked(self.is_cpu(seat), line) {
            return;
        }
        match row_kind(self.is_cpu(seat), line) {
            Row::Character => self.change_character(seat, forward, ctx.chars.len()),
            Row::Colour => self.cycle_colour(seat, forward),
            Row::Arsenal => match self.cpu[seat].as_mut() {
                Some(c) => c.deck = step(c.deck, self.cpu_options.len()),
                None => self.arsenal[seat] = step(self.arsenal[seat], self.options.len()),
            },
            Row::Strength => {
                if let Some(c) = self.cpu[seat].as_mut() {
                    c.grade = step(c.grade as usize, cpuseat::GRADES) as u8;
                }
            }
            Row::Team if self.teams_mode => self.team[seat] ^= 1,
            _ => {}
        }
    }

    /// Left / right on a shared (page 2) row.
    fn change_rule_row(&mut self, row: usize, forward: bool, ctx: &Ctx) {
        let step = |cur: usize, len: usize| if forward { (cur + 1) % len.max(1) } else { (cur + len.max(1) - 1) % len.max(1) };
        if self.shared_locked(row) {
            return;
        }
        match row {
            R_GAMEMODE => self.set_mode(modes::step(self.mode, forward)),
            R_MODE => {
                self.teams_mode = !self.teams_mode;
                self.deal_teams();
            }
            R_STAGE => self.stage = step(self.stage, ctx.stages.len()),
            R_RULE => {
                self.rule_idx = step(self.rule_idx, GAME_RULES.len());
                // Everyone moves to an arsenal that meets the new rule, if there is one.
                let rule = self.rule();
                for s in 0..SEATS {
                    self.arsenal[s] = self.first_eligible(self.arsenal[s], ctx.skills);
                    if let Some(c) = self.cpu[s].as_mut() {
                        c.deck = Self::first_eligible_in(&self.cpu_options, c.deck, rule, ctx.skills);
                    }
                }
            }
            R_TIME if self.prop_hunt() => self.ph_time_idx = step(self.ph_time_idx, crate::prophunt::ROUND_TIME_OPTIONS.len()),
            R_ROUNDS if self.prop_hunt() => self.ph_kills_idx = step(self.ph_kills_idx, crate::prophunt::KILLS_OPTIONS.len()),
            R_TIME => self.time_idx = step(self.time_idx, TIME_OPTIONS.len()),
            R_ROUNDS => self.rounds_idx = step(self.rounds_idx, ROUNDS_OPTIONS.len()),
            R_LEVEL => self.level_idx = step(self.level_idx, LEVEL_OPTIONS.len()),
            R_HAND => self.hand_idx = step(self.hand_idx, HAND_OPTIONS.len()),
            R_FRIENDLY if self.teams_mode => self.friendly_fire = !self.friendly_fire,
            _ => {}
        }
    }
}

/// What a seat's press asks the screen to do (page changes are made by `act`).
#[derive(Debug, PartialEq, Eq)]
enum Act {
    None,
    /// Start the battle (page 2).
    Start,
    /// The seat gives up its place (B at the top of its column on page 1).
    Leave,
    /// The press was understood but can't be done (ready with a bad arsenal).
    Refused,
}

/// A whole screen's content; shown while the setup is on that page.
#[derive(Component)]
pub(crate) struct PageRoot(Page);

#[derive(Component)]
pub(crate) struct SetupLine {
    seat: Option<usize>,
    row: usize,
    /// Text room in design pixels (for cutting long values).
    room: f32,
}

#[derive(Component)]
pub(crate) struct Portrait(usize);

/// The teal frame round a seat's preview window (gold when the seat is ready).
#[derive(Component)]
pub(crate) struct WindowFrame(usize);

#[derive(Component)]
pub(crate) struct SeatHeader(usize);

#[derive(Component)]
pub(crate) struct SeatDevice(usize);

/// "PRESS ANY BUTTON TO JOIN" in an empty seat's window.
#[derive(Component)]
pub(crate) struct JoinPrompt(usize);

/// The small caption above a seat's line `.1` (`.0` = the seat).
#[derive(Component)]
pub(crate) struct SeatCaption(usize, usize);

/// "CPU  GRADE 3" across the bottom of a CPU seat's preview window.
#[derive(Component)]
pub(crate) struct CpuBadge(usize);

/// The line under an empty seat's join prompt that says how to make it a CPU.
#[derive(Component)]
pub(crate) struct CpuHint(usize);

/// One line on the rules page naming what a seat is playing.
#[derive(Component)]
pub(crate) struct SeatSummary(usize);

#[derive(Component)]
pub(crate) struct StagePreview;

#[derive(Component)]
pub(crate) struct NoPreview;

#[derive(Component)]
pub(crate) struct SetupNotice;

#[derive(Component)]
pub(crate) struct RuleLine;

#[derive(Component)]
pub(crate) struct SetupDesc;

/// The arsenals every seat can choose from: the built-ins and the legal saved
/// ones (a saved arsenal must pass its own declared school limit).
fn arsenal_options(data: &GameData, saved: &SavedArsenals, skills: &[SkillDef]) -> Vec<ArsenalOption> {
    data.builtins.iter().map(|b| ArsenalOption { name: b.name.clone(), skills: b.skills.clone(), kind: b.kind }).chain(saved_options(saved, skills)).collect()
}

/// The player's own legal saved arsenals.
fn saved_options(saved: &SavedArsenals, skills: &[SkillDef]) -> Vec<ArsenalOption> {
    saved.0.iter().filter(|a| arsenal::check_kind(&a.skills, skills, a.kind_or_default(skills)).is_empty()).map(|a| ArsenalOption { name: a.name.clone(), skills: a.skills.clone(), kind: Some(a.kind_or_default(skills)) }).collect()
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    mut commands: Commands,
    assets: Res<AssetServer>,
    cfg: Res<crate::Config>,
    data: Res<GameData>,
    saved: Res<SavedArsenals>,
    catalog: Res<SkillCatalog>,
    mut state: ResMut<SetupState>,
    mut seats: ResMut<Seats>,
    windows: Query<&Window>,
    art: Option<Res<MenuArt>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Pads join by pressing a button on page 1 (decided 2026-10-01); during
    // the match `Seats::update` only lets a pad take back a seat it lost.
    // Coming from the title everyone joins afresh; coming back from a result
    // keeps the seats for a rematch (on page 1 again, nobody ready). The picks
    // themselves are remembered between visits.
    seats.mode = JoinMode::Press;
    let first_visit = state.options.is_empty();
    if first_visit {
        // Laurence: Lost Palace is the default arena.
        state.stage = data.stages.iter().position(|s| s.id == crate::DEFAULT_STAGE).unwrap_or(0);
    }
    if state.rematch {
        state.rematch = false;
        state.go_characters();
    } else {
        seats.clear();
        state.reset_seats();
    }
    state.options = arsenal_options(&data, &saved, &catalog.skills);
    let n = state.options.len().max(1);
    for s in 0..SEATS {
        state.arsenal[s] %= n;
    }
    // Player memory: the first visit after the game starts opens each seat on its picks from last time.
    if first_visit {
        state.memory_path = super::player_memory::PlayerMemory::path(&cfg.cache, cfg.screenshot.is_some());
        if let Some(path) = state.memory_path.clone() {
            let ctx = Ctx { chars: &data.characters, stages: &data.stages, skills: &catalog.skills };
            state.recall(&ctx, &super::player_memory::PlayerMemory::load(&path));
        }
    }
    // CPU seats: the five grades from the original's `enemyparam.epa`, the decks they can play (the original's enemy decks, then
    // the player's saved arsenals) and which stages have the nav graph a CPU walks on.
    state.cpu_data = crate::cpu::com_file(&cfg.cache, &format!("ai/{}", crate::cpu::SCRIPT)).is_some();
    state.grades = cpuseat::load_grades(&cfg.cache);
    state.cpu_options = cpuseat::original_decks(&cfg.cache).into_iter().chain(saved_options(&saved, &catalog.skills)).collect();
    state.nav_stages = Some(data.stages.iter().filter(|s| crate::cpu::stage_has_nav(&cfg.cache, &s.id)).map(|s| s.id.clone()).collect());
    let m = state.cpu_options.len().max(1);
    for c in state.cpu.iter_mut().flatten() {
        c.deck %= m;
    }
    state.stage = state.stage.min(data.stages.len().saturating_sub(1));

    let art = MenuArt::get(&mut commands, art, &cfg.cache, &assets, &mut images);
    let (root, frame) = ui::menu_screen(&mut commands, &windows, &art, "VERSUS SETUP", false);
    commands.entity(root).insert(DespawnOnExit(AppState::Setup));
    let top = ui::CONTENT_TOP;
    let teal_frame = Color::srgba(0.45, 0.85, 0.72, 0.55);
    // One full-frame container per page: hidden together (`layout_shared`).
    let page_node = || ui::abs(0.0, 0.0, ui::DESIGN.x, ui::DESIGN.y);
    let page1 = commands.spawn((page_node(), PageRoot(Page::Characters))).id();
    let page2 = commands.spawn((page_node(), PageRoot(Page::Rules))).id();
    let mut kids = Vec::new();

    // Page 1: four seat columns -- header, tall preview window (or the join
    // prompt), four rows each with a caption, and the ready banner.
    let (ww, wh) = WINDOW;
    for s in 0..SEATS {
        let x = SEAT_X0 + s as f32 * SEAT_STEP;
        let head = ui::spawn_pixel(&mut commands, &art, &crate::seats::seat_name(s), seat_colour(s), 1.0);
        commands.entity(head).insert((Node { position_type: PositionType::Absolute, left: px(x + 2.0), top: px(top + 3.0), height: px(16.0), ..default() }, SeatHeader(s)));
        kids.push(head);
        kids.push(commands.spawn((ui::body("", art.ttf.clone(), 13.0, ui::DIM), ui::abs(x + 140.0, top + 5.0, 150.0, 16.0), SeatDevice(s))).id());
        let wx = x + (SEAT_W - ww) / 2.0;
        let wy = top + 22.0;
        kids.push(commands.spawn((ui::abs(wx - 3.0, wy - 3.0, ww + 6.0, wh + 6.0), BackgroundColor(teal_frame), WindowFrame(s))).id());
        kids.push(commands.spawn((ui::abs(wx, wy, ww, wh), BackgroundColor(Color::srgb(0.02, 0.05, 0.05)))).id());
        kids.push(commands.spawn((ui::abs(wx, wy, ww, wh), ImageNode::default(), Portrait(s), Visibility::Hidden)).id());
        // The empty seat's invitation, in the menu font, centred in its window.
        let prompt = commands.spawn((Node { position_type: PositionType::Absolute, left: px(wx), top: px(wy), width: px(ww), height: px(wh), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, row_gap: px(10.0), ..default() }, JoinPrompt(s))).id();
        let mut lines = vec![];
        for line in ["PRESS ANY", "BUTTON", "TO JOIN"] {
            lines.push(ui::spawn_pixel(&mut commands, &art, line, ui::DIM, 0.8));
        }
        if s == 0 {
            lines.push(commands.spawn((ui::body("or Enter on the keyboard", art.ttf.clone(), 13.0, ui::DIM), Node { margin: UiRect::top(px(8.0)), ..default() })).id());
        } else {
            lines.push(commands.spawn((ui::body("", art.ttf.clone(), 13.0, ui::DIM), Node { margin: UiRect::top(px(10.0)), ..default() }, CpuHint(s))).id());
        }
        commands.entity(prompt).add_children(&lines);
        kids.push(prompt);
        // A CPU's window carries its badge ("CPU  GRADE 3") along the bottom.
        let badge = commands
            .spawn((
                Node { position_type: PositionType::Absolute, left: px(wx), top: px(wy + wh - 24.0), width: px(ww), height: px(24.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(Color::srgba(0.20, 0.09, 0.38, 0.9)),
                Text::new(""),
                TextFont { font: FontSource::Handle(art.ttf.clone()), font_size: FontSize::Px(14.0), weight: FontWeight(700), ..default() },
                TextColor(ui::WHITE),
                TextLayout { justify: Justify::Center, ..default() },
                CpuBadge(s),
                Visibility::Hidden,
            ))
            .id();
        kids.push(badge);
        for k in 0..SEAT_LINES {
            let y = SEAT_ROWS_Y + k as f32 * SEAT_ROW_STEP;
            kids.push(commands.spawn((ui::body("", art.ttf.clone(), 12.0, ui::TEAL), ui::abs(x + 4.0, y, SEAT_W, SEAT_CAPTION_H), SeatCaption(s, k), Visibility::Hidden)).id());
            let (panel, _) = ui::spawn_row(&mut commands, &art, ui::abs(x, y + SEAT_CAPTION_H, SEAT_W, SEAT_ROW_H), "", "", ROW_SCALE);
            commands.entity(panel).insert((SetupLine { seat: Some(s), row: k, room: SEAT_W * (1.0 - ui::TAB_FRAC) - 30.0 }, Visibility::Hidden));
            kids.push(panel);
        }
    }
    commands.entity(page1).add_children(&kids);
    let bar1 = ui::prompt_bar_parts(&mut commands, &art, &[(Glyph::UpDown, "Select"), (Glyph::LeftRight, "Change")], &[(Glyph::A, "Ready"), (Glyph::B, "Back / Leave")], ui::KEYS_SEATS);
    commands.entity(page1).add_children(&bar1);

    // Page 2: the stage picture and who is playing on the left, the shared rows on the right.
    let mut kids = Vec::new();
    let (px0, py0, pw, ph) = PREVIEW;
    kids.push(commands.spawn((ui::abs(px0 - 2.0, py0 - 2.0, pw + 4.0, ph + 4.0), BackgroundColor(teal_frame))).id());
    kids.push(commands.spawn((ui::abs(px0, py0, pw, ph), BackgroundColor(Color::srgb(0.02, 0.05, 0.05)))).id());
    kids.push(commands.spawn((ui::body("No preview picture", art.ttf.clone(), 14.0, ui::DIM), ui::abs(px0 + 10.0, py0 + ph / 2.0 - 8.0, pw - 20.0, 18.0), NoPreview, Visibility::Hidden)).id());
    kids.push(commands.spawn((ui::abs(px0, py0, pw, ph), ImageNode::default(), StagePreview)).id());
    kids.push(commands.spawn((ui::body("", art.ttf.clone(), 15.0, ui::TEAL), ui::abs(px0, py0 + ph + 12.0, pw + 40.0, 20.0), RuleLine)).id());
    for s in 0..SEATS {
        kids.push(commands.spawn((ui::body("", art.ttf.clone(), 16.0, ui::DIM), ui::abs(px0, py0 + ph + 52.0 + s as f32 * 26.0, pw + 40.0, 22.0), SeatSummary(s))).id());
    }
    for row in R_GAMEMODE..=R_START {
        let node = ui::abs(RULE_X, RULE_Y + row as f32 * RULE_STEP, RULE_W, RULE_H);
        let (panel, _) = ui::spawn_row(&mut commands, &art, node, "", "", if row == R_START { 1.0 } else { RULE_SCALE });
        commands.entity(panel).insert(SetupLine { seat: None, row, room: RULE_W * (1.0 - ui::TAB_FRAC) - 30.0 });
        kids.push(panel);
    }
    commands.entity(page2).add_children(&kids);
    let bar2 = ui::prompt_bar_parts(&mut commands, &art, &[(Glyph::UpDown, "Select"), (Glyph::LeftRight, "Change")], &[(Glyph::A, "Start battle"), (Glyph::B, "Back to characters")], ui::KEYS_RULES);
    commands.entity(page2).add_children(&bar2);

    // Under both pages: the live problem / status line and the description line.
    let shared = [
        commands.spawn((ui::body("", art.ttf.clone(), 15.0, ui::BAD), ui::abs(36.0, NOTICE_Y, 1210.0, 20.0), SetupNotice)).id(),
        commands.spawn((ui::body("", art.ttf.clone(), 18.0, ui::WHITE), ui::abs(36.0, ui::CONTENT_BOTTOM - 30.0, 1210.0, 24.0), SetupDesc)).id(),
    ];
    commands.entity(frame).add_children(&[page1, page2]).add_children(&shared);
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<(Entity, &Gamepad)>,
    mut script: Option<ResMut<super::script::MenuScript>>,
    mut state: ResMut<SetupState>,
    data: Res<GameData>,
    catalog: Res<SkillCatalog>,
    mut seats: ResMut<Seats>,
    mut requested: ResMut<RequestedPlayers>,
    mut kinds: ResMut<SeatKinds>,
    mut views: ResMut<crate::seats::LocalViews>,
    mut next: ResMut<NextState<AppState>>,
    mut commands: Commands,
    menu: Res<crate::menu::MenuState>,
    mut sfx: ResMut<super::sound::MenuSfxQueue>,
) {
    // The settings overlay has the keys while it's open (and on the frame it toggles).
    if menu.open || menu.is_changed() {
        return;
    }
    let ctx = Ctx { chars: &data.characters, stages: &data.stages, skills: &catalog.skills };
    // A pad that went away gives up its seat.
    for s in 0..SEATS {
        if let Some(e) = seats.pad[s] {
            if pads.get(e).is_err() {
                seats.pad[s] = None;
                state.leave(s);
            }
        }
    }
    // Nobody left to set the rules (CPUs don't count): back to the join screen.
    if state.page == Page::Rules && state.human_count() == 0 {
        state.clear_cpus();
        state.go_characters();
    }
    if keys.just_pressed(KeyCode::Escape) {
        sfx.0.push(super::sound::Cue::Back);
        if state.page == Page::Rules {
            state.go_characters();
        } else {
            next.set(AppState::Title);
        }
        return;
    }
    // Joining (page 1 only -- never on the rules screen, never in a match): a
    // button on an unseated pad takes the lowest empty seat, a menu key takes
    // seat 1 (the keyboard is seat 1), and so does a scripted pad's first
    // press. The press that joins does nothing else.
    let joining = state.page == Page::Characters;
    let mut just_joined = [false; SEATS];
    if joining {
        for (e, pad) in &pads {
            if seats.seat_of(e).is_none() && pad.get_just_pressed().next().is_some() {
                if let Some(s) = state.free_seat() {
                    seats.pad[s] = Some(e);
                    state.join(s, &ctx);
                    just_joined[s] = true;
                }
            }
        }
        if super::input::keyboard_joins(&keys) && !state.joined[0] {
            state.join(0, &ctx);
            just_joined[0] = true;
        }
    }
    let mut scripted: [Option<super::input::MenuAxis>; SEATS] = [None; SEATS];
    if let Some((vpad, axis)) = script.as_deref_mut().and_then(super::script::tick_pad) {
        match state.virt[vpad].filter(|&s| state.joined[s]) {
            Some(s) => scripted[s] = Some(axis),
            None if joining => {
                let take = if vpad == 0 && !state.joined[0] { Some(0) } else { state.free_seat() };
                if let Some(s) = take {
                    state.virt[vpad] = Some(s);
                    state.join(s, &ctx);
                    just_joined[s] = true;
                }
            }
            None => {}
        }
    }
    if just_joined.iter().any(|&j| j) {
        sfx.0.push(super::sound::Cue::Confirm);
    }
    for s in 0..SEATS {
        if !state.is_human(s) || just_joined[s] {
            continue;
        }
        let pad = seats.pad[s].and_then(|e| pads.get(e).ok()).map(|(_, g)| g);
        let mut repeat = state.repeat[s];
        let a = super::input::read_seat(s, &keys, pad, scripted[s], &mut repeat);
        state.repeat[s] = repeat;
        if !a.any() {
            continue;
        }
        let page_before = state.page;
        sfx.0.extend(super::sound::cues_for(&a));
        match state.act(s, &a, &ctx) {
            Act::None => {}
            Act::Refused => sfx.0.push(super::sound::Cue::Back),
            Act::Leave => {
                // Player 1 (the first person seated) backs out of the whole screen, whoever else has joined
                // (Laurence 2026-10-08); anyone else only gives up their seat.
                if state.human_count() <= 1 || state.leaving_goes_back(s) {
                    next.set(AppState::Title);
                    return;
                }
                // The pad stays plugged in: a button press joins again.
                seats.pad[s] = None;
                state.leave(s);
            }
            Act::Start => match state.plan(&ctx) {
                Ok(plan) => {
                    // Player memory: these picks open the setup next time the game starts.
                    if let Some(path) = state.memory_path.clone() {
                        let old = super::player_memory::PlayerMemory::load(&path);
                        state.remembered(&ctx, &old).save(&path);
                    }
                    // Pack the joined seats' pads down to slots 0..n.
                    let old = seats.pad;
                    seats.pad = [None; SEATS];
                    seats.lost = [false; SEATS];
                    for (slot, &seat) in plan.seats.iter().enumerate() {
                        seats.pad[slot] = old[seat];
                    }
                    *views = crate::seats::LocalViews::for_kinds(&plan.kinds, plan.players.len());
                    requested.0 = Some(plan.players);
                    // Who is a CPU, and on which `enemyparam.epa` row (`cpu::drive_system` builds the host from this at tick 0).
                    *kinds = plan.kinds;
                    commands.insert_resource(plan.setup);
                    next.set(AppState::Match);
                    return;
                }
                Err(_) => sfx.0.push(super::sound::Cue::Back),
            },
        }
        // One press moves one page at most: the rest of the seats' presses this
        // frame belong to the new page and are dropped (they were read before).
        if state.page != page_before {
            break;
        }
    }
}

/// How to work the CPU seats, for the description line.
fn cpu_keys() -> &'static str {
    "Y adds a CPU; LB / RB (keyboard Q / E) choose a seat to edit."
}

/// The tail of a person's description line while someone is still not ready: how to go on without waiting
/// (it used to be its own status line above; Laurence, 2026-10-09: de-clutter, item 4).
fn start_hint(state: &SetupState) -> &'static str {
    let waiting = (0..SEATS).any(|s| state.is_human(s) && !state.ready[s]);
    if state.is_human(0) && (waiting || state.human_count() == 1) { " PLAYER 1: Start goes on at once." } else { "" }
}

/// One line explaining the row seat `seat`'s cursor is on (the description line).
fn describe(state: &SetupState, ctx: &Ctx, seat: usize) -> String {
    match state.page {
        Page::Characters => {
            if state.ready[seat] {
                return format!("Ready. Waiting for the others - B takes it back.{}", start_hint(state));
            }
            let col = state.edit_column(seat);
            if col != seat {
                if !state.is_cpu(col) {
                    return format!("An empty seat. A (or Y) makes it a CPU, B goes back to your own seat. {}", cpu_keys());
                }
                return match row_kind(true, state.cursor[col]) {
                    Row::Character => "Choose who the CPU plays as.".into(),
                    Row::Colour => "The CPU's costume colour.".into(),
                    Row::Strength => match (state.cpu[col], state.grades) {
                        (Some(c), Some(g)) => cpuseat::describe_grade(c.grade, &g[c.grade as usize % cpuseat::GRADES]),
                        _ => "How well the CPU plays.".into(),
                    },
                    Row::Arsenal => match state.arsenal_problem(col, ctx.skills) {
                        Some(why) => format!("Doesn't meet {}: {why}. Pick another deck.", arsenal::RULE_NAMES[state.rule()]),
                        None => "The CPU's deck: the original's enemy decks, then your saved arsenals. A when done, X removes the CPU.".into(),
                    },
                    _ if state.teams_mode => "Which team the CPU fights for.".into(),
                    _ => "Teams are off: everyone fights for themselves. Turn on Teams in TEAM PLAY on the next screen.".into(),
                };
            }
            match row_kind(false, state.cursor[seat]) {
                Row::Character => format!("Choose who you play as.{}", start_hint(state)),
                Row::Colour => format!("The costume colour. Two players on one character can't wear the same one.{}", start_hint(state)),
                Row::Arsenal => match state.arsenal_problem(seat, ctx.skills) {
                    Some(why) => format!("Doesn't meet {}: {why}. Pick another arsenal.", arsenal::RULE_NAMES[state.rule()]),
                    None => format!("The 30 skills your capsules come from. Build your own under ARSENAL.{}", start_hint(state)),
                },
                _ if state.teams_mode => "Which team you fight for. A team wins when the other has nobody left standing.".into(),
                _ => "Teams are off: everyone fights for themselves. Turn on Teams in TEAM PLAY on the next screen.".into(),
            }
        }
        Page::Rules if state.prop_hunt() && state.rcursor[seat] == R_TIME => "How long the seeker has to hunt each round (the clock starts when the seeker is released, after 20 seconds).".into(),
        Page::Rules if state.prop_hunt() && state.rcursor[seat] == R_ROUNDS => "Kills that win the match. Only the seeker scores; the seeker changes every round, with no round limit.".into(),
        Page::Rules => match state.rcursor[seat] {
            R_GAMEMODE => modes::blurb(state.mode).into(),
            R_MODE => "Free for all, or two teams (2v2 with four players).".into(),
            R_STAGE => "Where the battle takes place.".into(),
            R_RULE => "The original's game rule: it sets Health and the Level range, and which arsenals qualify.".into(),
            R_TIME => "When time runs out, the healthiest fighter wins the round; a tie is a draw.".into(),
            R_ROUNDS => "Round wins needed to take the match. Endless keeps going until you quit.".into(),
            R_LEVEL => "The Level (aura limit) everyone starts at. The game rule can raise or cap it.".into(),
            R_HAND => "Skills dealt onto the four buttons when a round starts. The original deals 4.".into(),
            R_FRIENDLY if state.teams_mode => "On: attacks can hurt your own teammate. Off: they pass through.".into(),
            R_FRIENDLY => "Friendly fire only matters in teams.".into(),
            // Start battle: the A prompt says it already (item 10).
            _ => String::new(),
        },
    }
}

/// The status line under page 1 (teal) or the problem line under page 2 (red).
fn notice(state: &SetupState, ctx: &Ctx) -> (String, Color) {
    match state.page {
        Page::Characters => {
            let n = state.human_count();
            // The ready count and "Start goes on" moved into the description line (item 4); this line keeps the join
            // prompt for an empty screen and the problems.
            let msg = if n == 0 { "Press any button on a pad (or Enter) to join.".to_string() } else { String::new() };
            let bad = (0..SEATS).filter(|&s| state.joined[s]).find_map(|s| state.arsenal_problem(s, ctx.skills).map(|why| format!("{}: that arsenal doesn't meet the game rule ({why}).", state.seat_name(s)))).or_else(|| state.first_player_problem());
            match bad {
                Some(b) => (b, ui::BAD),
                None => (msg, ui::TEAL),
            }
        }
        Page::Rules => (state.problem(ctx).unwrap_or_default(), ui::BAD),
    }
}

/// A shared row's (label, value, dimmed) for the current setup.
fn shared_text(state: &SetupState, ctx: &Ctx, row: usize) -> (&'static str, String, bool) {
    let rule = state.rule();
    if state.prop_hunt() {
        use crate::prophunt::ROUND_TIME_OPTIONS;
        match row {
            R_MODE => return ("TEAM PLAY", "SEEKER AND HIDERS".into(), true),
            R_TIME => return ("ROUND TIME", ph_time_label(ROUND_TIME_OPTIONS[state.ph_time_idx.min(ROUND_TIME_OPTIONS.len() - 1)]), false),
            R_ROUNDS => return ("KILLS TO WIN", state.prop_hunt_config().kills_to_win.to_string(), false),
            R_LEVEL => return ("STARTING LEVEL", "N/A".into(), true),
            R_HAND => return ("STARTING SKILLS", "SEEKER: KNIFE".into(), true),
            R_FRIENDLY => return ("FRIENDLY FIRE", "N/A".into(), true),
            _ => {}
        }
    }
    if let Some(m) = state.mode() {
        match row {
            R_RULE => return ("GAME RULE", arsenal::RULE_NAMES[0].to_uppercase(), true),
            R_LEVEL => return ("STARTING LEVEL", m.rules.start_level.to_string(), true),
            R_HAND => return ("STARTING SKILLS", "FIXED".into(), true),
            _ => {}
        }
    }
    let level = {
        let r = RULE_TABLE[rule];
        let chosen = LEVEL_OPTIONS[state.level_idx];
        let eff = chosen.clamp(r.level_min, r.level_max);
        if eff == chosen {
            chosen.to_string()
        } else {
            format!("{eff} (RULE)")
        }
    };
    match row {
        R_GAMEMODE => ("GAMEMODES", modes::label(state.mode), false),
        R_MODE => ("TEAM PLAY", if state.teams_mode { "TEAMS" } else { "FREE FOR ALL" }.into(), false),
        R_STAGE => ("STAGE", ctx.stages.get(state.stage).map_or_else(|| "-".to_string(), |s| s.name.to_uppercase()), state.cpu_count() > 0 && !state.stage_navigable(ctx)),
        R_RULE => ("GAME RULE", arsenal::RULE_NAMES[rule].to_uppercase(), !state.rule_available(ctx.skills)),
        R_TIME => {
            let (name, ticks) = TIME_OPTIONS[state.time_idx];
            ("TIME LIMIT", if ticks == BY_RULE { format!("{} MIN (RULE)", RULE_TABLE[rule].minutes) } else { name.to_uppercase() }, false)
        }
        R_ROUNDS => ("ROUNDS TO WIN", ROUNDS_OPTIONS[state.rounds_idx].0.to_uppercase(), false),
        R_LEVEL => ("STARTING LEVEL", level, false),
        R_HAND => ("STARTING SKILLS", HAND_OPTIONS[state.hand_idx].to_string(), false),
        R_FRIENDLY => ("FRIENDLY FIRE", if !state.teams_mode { "N/A" } else if state.friendly_fire { "ON" } else { "OFF" }.into(), !state.teams_mode),
        _ => ("START BATTLE", String::new(), state.problem(ctx).is_some()),
    }
}

/// The seat rows' (value, dimmed) text.
fn seat_text(state: &SetupState, ctx: &Ctx, seat: usize, line: usize) -> (String, bool) {
    match row_kind(state.is_cpu(seat), line) {
        Row::Character => (ctx.chars.get(state.char_idx[seat]).map_or_else(|| "-".to_string(), |c| c.label.to_uppercase()), false),
        Row::Colour => (format!("COLOUR {}", state.colour[seat] + 1), false),
        Row::Strength => (state.cpu[seat].map_or_else(String::new, |c| cpuseat::grade_name(c.grade)), false),
        Row::Arsenal => (state.seat_arsenal(seat).map_or_else(|| "-".to_string(), |a| a.name.to_uppercase()), state.arsenal_problem(seat, ctx.skills).is_some()),
        Row::Team => {
            if state.teams_mode {
                (format!("TEAM {}", state.team[seat] + 1), false)
            } else {
                ("NO TEAMS".to_string(), true)
            }
        }
        Row::Ready => (if state.ready[seat] { "READY!" } else { "PRESS A WHEN READY" }.to_string(), !state.ready[seat]),
    }
}

/// The small caption above a line of a seat's column.
fn caption_text(cpu: bool, line: usize) -> &'static str {
    match row_kind(cpu, line) {
        Row::Character => "Character",
        Row::Colour => "Costume colour",
        Row::Strength => "Strength",
        Row::Arsenal => "Arsenal",
        Row::Team => "Team",
        Row::Ready => "",
    }
}

#[allow(clippy::too_many_arguments)]
pub fn layout_rows(
    mut commands: Commands,
    art: Option<Res<MenuArt>>,
    windows: Query<&Window>,
    mut frames: Query<&mut UiTransform, With<ui::DesignFrame>>,
    state: Res<SetupState>,
    data: Res<GameData>,
    catalog: Res<SkillCatalog>,
    mut rows: Query<(&SetupLine, &RowRefs, &mut ImageNode, &mut Visibility, &mut Node)>,
    mut texts: Query<&mut PixelText>,
) {
    ui::fit_frames(&windows, &mut frames);
    let Some(art) = art else { return };
    let ctx = Ctx { chars: &data.characters, stages: &data.stages, skills: &catalog.skills };
    for (line, refs, mut panel, mut vis, mut node) in &mut rows {
        // Page 1 rows are a seat's, page 2 rows are shared; a page's whole
        // container is hidden when it isn't showing (`layout_shared`).
        let (visible, on): (bool, Vec<usize>) = match line.seat {
            // A person's own column shows their cursor while it is there; a CPU's shows whoever is editing it.
            Some(s) if state.is_cpu(s) => (true, if state.cursor[s] == line.row { (0..SEATS).filter(|&h| state.is_human(h) && !state.ready[h] && state.edit_column(h) == s).collect() } else { vec![] }),
            Some(s) => (state.joined[s], if state.joined[s] && !state.ready[s] && state.edit_column(s) == s && state.cursor[s] == line.row { vec![s] } else { vec![] }),
            None => (true, (0..SEATS).filter(|&s| state.is_human(s) && state.rcursor[s] == line.row).collect()),
        };
        // Rows that do nothing in this mode are hidden and the ones under them move up (item 5).
        let (visible, top) = match line.seat {
            Some(s) => (visible && !state.seat_line_hidden(s, line.row), SEAT_ROWS_Y + state.seat_slot(s, line.row) as f32 * SEAT_ROW_STEP + SEAT_CAPTION_H),
            None => (visible && !state.shared_hidden(line.row), RULE_Y + state.shared_slot(line.row) as f32 * RULE_STEP),
        };
        if node.top != Val::Px(top) {
            node.top = Val::Px(top);
        }
        let want = if visible { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if !visible {
            continue;
        }
        let (label, value, dim) = match line.seat {
            Some(s) => {
                let (v, d) = seat_text(&state, &ctx, s, line.row);
                ("", v, d)
            }
            None => shared_text(&state, &ctx, line.row),
        };
        let ready_row = line.seat.is_some_and(|s| row_kind(state.is_cpu(s), line.row) == Row::Ready);
        let ready_on = ready_row && line.seat.is_some_and(|s| state.ready[s]);
        let selected = !on.is_empty() || ready_on;
        let (tint, color) = match (selected, dim) {
            (true, true) => (ui::PANEL_SEL, ui::DIM),
            (true, false) => (ui::PANEL_SEL, ui::YELLOW),
            (false, true) => (ui::PANEL_DIM, ui::DIM),
            (false, false) => (ui::PANEL, ui::ROW_TEXT),
        };
        // The dark tab says whose cursor is here (seat numbers).
        let digits: String = if ready_on { line.seat.map(|s| (s + 1).to_string()).unwrap_or_default() } else { on.iter().map(|s| (s + 1).to_string()).collect() };
        // The Start battle row is everybody's resting place; a seat list ("124")
        // there is our invention (the program's rules menu has no such tab: its
        // script `vsmenu_en.ssb` carries no seat strings for it), so it stays empty.
        let digits = if line.seat.is_none() && line.row == R_START { String::new() } else { digits };
        let tab_color = if let (true, Some(s)) = (ready_on, line.seat) {
            seat_colour(s)
        } else if on.len() == 1 {
            seat_colour(on[0])
        } else {
            ui::WHITE
        };
        let start_row = line.seat.is_none() && line.row == R_START;
        let (left, right) = if ready_row {
            (art.fit(&value, line.room, ROW_SCALE), String::new())
        } else if let Some(s) = line.seat {
            // A row a game mode fixes has nothing to change: no arrows.
            let text = if state.seat_line_locked(state.is_cpu(s), line.row) { value.clone() } else { format!("< {value} >") };
            (art.fit(&text, line.room, ROW_SCALE), String::new())
        } else if start_row {
            (label.to_string(), String::new())
        } else {
            let room = line.room - art.text_width(label, RULE_SCALE) - 12.0;
            let text = if state.shared_locked(line.row) { value.clone() } else { format!("< {value} >") };
            (label.to_string(), art.fit(&text, room, RULE_SCALE))
        };
        ui::set_row(&mut commands, &art, *refs, &mut panel, &mut texts, tint, (&digits, tab_color), (&left, color), (&right, color));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn layout_seats(
    mut commands: Commands,
    art: Option<Res<MenuArt>>,
    state: Res<SetupState>,
    data: Res<GameData>,
    seats: Res<Seats>,
    char_preview: Option<Res<super::charselect::PreviewTextures>>,
    mut portraits: Query<(&Portrait, &mut ImageNode, &mut Visibility), (Without<JoinPrompt>, Without<SeatCaption>, Without<CpuBadge>)>,
    mut prompts: Query<(&JoinPrompt, &mut Visibility), (Without<Portrait>, Without<SeatCaption>, Without<CpuBadge>)>,
    mut captions: Query<(&SeatCaption, &mut Visibility, &mut Text), (Without<Portrait>, Without<JoinPrompt>, Without<CpuBadge>, Without<SeatDevice>, Without<CpuHint>)>,
    mut frames: Query<(&WindowFrame, &mut BackgroundColor)>,
    mut headers: Query<(Entity, &SeatHeader, &mut PixelText)>,
    mut devices: Query<(&SeatDevice, &mut Text), (Without<SeatCaption>, Without<CpuBadge>, Without<CpuHint>)>,
    mut badges: Query<(&CpuBadge, &mut Visibility, &mut Text), (Without<Portrait>, Without<JoinPrompt>, Without<SeatCaption>, Without<SeatDevice>, Without<CpuHint>)>,
    mut hints: Query<(&CpuHint, &mut Text, &mut TextColor), (Without<SeatCaption>, Without<SeatDevice>, Without<CpuBadge>)>,
) {
    let Some(art) = art else { return };
    let set = |vis: &mut Visibility, on: bool| {
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    };
    let previews = state.previews();
    for (p, mut img, mut vis) in &mut portraits {
        set(&mut vis, state.joined[p.0]);
        // The live 3D preview (frontend/charselect.rs) once it exists; the flat
        // portrait picture is the fallback for the frame before that.
        let id = previews[p.0].and_then(|(c, _)| data.characters.get(c)).map_or("", |c| c.id.as_str());
        let want = char_preview.as_ref().map(|pv| ImageNode { image: pv.0[p.0].clone(), color: Color::WHITE, ..default() }).or_else(|| art.portrait(id)).unwrap_or_else(|| ImageNode { color: Color::NONE, ..default() });
        if img.rect != want.rect || img.image != want.image {
            *img = want;
        }
    }
    for (j, mut vis) in &mut prompts {
        set(&mut vis, !state.joined[j.0]);
    }
    for (c, mut vis, mut text) in &mut captions {
        let cpu = state.is_cpu(c.0);
        let label = if state.seat_line_locked(cpu, c.1) { "Hand (fixed by the mode)" } else { caption_text(cpu, c.1) };
        set(&mut vis, state.joined[c.0] && !label.is_empty() && !state.seat_line_hidden(c.0, c.1));
        if text.0 != label {
            text.0 = label.to_string();
        }
    }
    // The frame: gold when a person is ready, violet for a CPU, and the colour of whoever's cursor is on the seat when one is.
    for (f, mut bg) in &mut frames {
        let s = f.0;
        let editor = (0..SEATS).find(|&h| h != s && state.is_human(h) && !state.ready[h] && state.edit_column(h) == s);
        let want = match editor {
            Some(h) => seat_colour(h).with_alpha(0.95),
            None if state.is_cpu(s) => Color::srgba(0.70, 0.55, 1.0, 0.85),
            None if state.joined[s] && state.ready[s] => Color::srgba(1.0, 0.86, 0.22, 0.9),
            None => Color::srgba(0.45, 0.85, 0.72, 0.55),
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (e, h, mut pt) in &mut headers {
        let color = if state.joined[h.0] { seat_colour(h.0) } else { ui::DIM };
        let text = state.seat_name(h.0);
        let scale = pt.scale;
        ui::set_pixel(&mut commands, &art, e, &mut pt, &text, color, scale);
    }
    for (d, mut t) in &mut devices {
        let label = match (state.joined[d.0], state.cpu[d.0], seats.pad[d.0].is_some(), d.0) {
            (false, _, _, _) => String::new(),
            // The grade is on the Strength line; once is enough (item 6).
            (true, Some(_), _, _) => String::new(),
            (true, None, true, 0) => "PAD + KEYBOARD".to_string(),
            (true, None, true, _) => "PAD".to_string(),
            (true, None, false, 0) => "KEYBOARD".to_string(),
            (true, None, false, _) => "SCRIPT".to_string(),
        };
        if t.0 != label {
            t.0 = label;
        }
    }
    for (b, mut vis, mut text) in &mut badges {
        // Just "CPU" on the preview: the grade is on the Strength line (item 6).
        set(&mut vis, state.is_cpu(b.0));
        let label = if state.is_cpu(b.0) { "CPU".to_string() } else { String::new() };
        if text.0 != label {
            text.0 = label;
        }
    }
    // Under an empty seat's join prompt: how to make it a CPU (the lowest empty seat takes Y; a cursor on one says A).
    let lowest_empty = (1..SEATS).find(|&s| !state.joined[s]);
    for (h, mut text, mut color) in &mut hints {
        let s = h.0;
        let focused = (0..SEATS).any(|p| state.is_human(p) && !state.ready[p] && p != s && state.edit_column(p) == s);
        let (label, want) = if !state.cpu_available() || state.human_count() == 0 || state.joined[s] {
            (String::new(), ui::DIM)
        } else if focused {
            ("A: ADD A CPU".to_string(), ui::YELLOW)
        } else if Some(s) == lowest_empty {
            ("Y: ADD A CPU".to_string(), ui::DIM)
        } else {
            // One hint, on the seat Y fills (item 9); a blank line keeps the join prompt where the other seats have it.
            (" ".to_string(), ui::DIM)
        };
        if text.0 != label {
            text.0 = label;
        }
        if color.0 != want {
            color.0 = want;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn layout_shared(
    art: Option<Res<MenuArt>>,
    state: Res<SetupState>,
    data: Res<GameData>,
    catalog: Res<SkillCatalog>,
    mut roots: Query<(&PageRoot, &mut Visibility), Without<NoPreview>>,
    mut preview: Query<&mut ImageNode, (With<StagePreview>, Without<Portrait>)>,
    mut no_preview: Query<&mut Visibility, (With<NoPreview>, Without<PageRoot>)>,
    mut notice_q: Query<(&mut Text, &mut TextColor), (With<SetupNotice>, Without<RuleLine>, Without<SetupDesc>, Without<SeatSummary>)>,
    mut rule_lines: Query<&mut Text, (With<RuleLine>, Without<SetupNotice>, Without<SetupDesc>, Without<SeatSummary>)>,
    mut desc: Query<&mut Text, (With<SetupDesc>, Without<SetupNotice>, Without<RuleLine>, Without<SeatSummary>)>,
    mut summaries: Query<(&SeatSummary, &mut Text, &mut TextColor), (Without<SetupNotice>, Without<RuleLine>, Without<SetupDesc>)>,
) {
    let Some(art) = art else { return };
    let ctx = Ctx { chars: &data.characters, stages: &data.stages, skills: &catalog.skills };
    for (root, mut vis) in &mut roots {
        let want = if root.0 == state.page { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    let stage_id = data.stages.get(state.stage).map_or("", |s| s.id.as_str());
    let want = art.stage_preview(stage_id);
    for mut img in &mut preview {
        let w = want.clone().unwrap_or_else(|| ImageNode { color: Color::NONE, ..default() });
        if img.rect != w.rect || img.color != w.color {
            *img = w;
        }
    }
    for mut v in &mut no_preview {
        let hide = if want.is_some() { Visibility::Hidden } else { Visibility::Inherited };
        if *v != hide {
            *v = hide;
        }
    }
    let set = |t: &mut Text, text: String| {
        if t.0 != text {
            t.0 = text;
        }
    };
    for (mut t, mut c) in &mut notice_q {
        let (text, color) = notice(&state, &ctx);
        set(&mut t, text);
        if c.0 != color {
            c.0 = color;
        }
    }
    for mut t in &mut rule_lines {
        set(&mut t, rule_line(&state));
    }
    for (s, mut t, mut c) in &mut summaries {
        let i = s.0;
        let (text, color) = if state.joined[i] {
            let who = ctx.chars.get(state.char_idx[i]).map_or("-".to_string(), |c| c.label.to_uppercase());
            let a = state.seat_arsenal(i).map_or("-", |a| a.name.as_str());
            let team = if state.teams_mode { format!(", team {}", state.team[i] + 1) } else { String::new() };
            let grade = state.cpu[i].map_or_else(String::new, |c| format!(", {}", cpuseat::grade_name(c.grade)));
            (format!("{}: {who}{grade}, {a}{team}", state.seat_name(i)), seat_colour(i))
        } else {
            (String::new(), ui::DIM)
        };
        set(&mut t, text);
        if c.0 != color {
            c.0 = color;
        }
    }
    let who = if state.is_human(state.last_mover) { state.last_mover } else { (0..SEATS).find(|&s| state.is_human(s)).unwrap_or(0) };
    // No character bio here any more (Laurence, 2026-10-09: de-clutter; DECISIONS.md).
    for mut t in &mut desc {
        set(&mut t, describe(&state, &ctx, who));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Capsule;
    use crate::frontend::discovery::{CharacterEntry, StageEntry};
    use crate::frontend::input::MenuAxis;

    fn skill(id: i16, school: i16, cost: i32) -> SkillDef {
        let mut raw = [0i16; 70];
        raw[pd_formats::skills::field::SCHOOL] = school;
        SkillDef { id, name: format!("S{id}"), capsule: Capsule::Attack, family: 0, cost, strength: 1, strength_rule: 0, requirement: 0, requirement_amount: 0, level_penalty: 0, health_cost: 0, defence: 0, raw, s70_73: [0; 4] }
    }

    fn deck(ids: &[i16]) -> Vec<i16> {
        let mut d = ids.to_vec();
        d.resize(arsenal::ARSENAL_SIZE, crate::combat::AURA_PARTICLE);
        d
    }

    struct Data {
        chars: Vec<CharacterEntry>,
        stages: Vec<StageEntry>,
        skills: Vec<SkillDef>,
    }

    impl Data {
        fn new() -> Self {
            Data {
                chars: (0..5).map(|i| CharacterEntry { id: format!("pc0{i}a"), label: format!("Char {i}") }).collect(),
                stages: vec![StageEntry { id: "st03".into(), name: "Lost Palace".into() }, StageEntry { id: "st06".into(), name: "Storm Refinery".into() }],
                // 1, 2: school 1 cost 2 and 4; 65: school 2 cost 3.
                skills: vec![skill(1, 1, 2), skill(2, 1, 4), skill(65, 2, 3)],
            }
        }
        fn ctx(&self) -> Ctx<'_> {
            Ctx { chars: &self.chars, stages: &self.stages, skills: &self.skills }
        }
    }

    fn state_with(options: Vec<ArsenalOption>) -> SetupState {
        SetupState { options, ..SetupState::default() }
    }

    fn opt(name: &str, ids: &[i16]) -> ArsenalOption {
        ArsenalOption { name: name.into(), skills: deck(ids), kind: None }
    }

    fn press(f: impl FnOnce(&mut MenuAxis)) -> MenuAxis {
        let mut a = MenuAxis::default();
        f(&mut a);
        a
    }

    fn confirm() -> MenuAxis {
        press(|a| a.confirm = true)
    }

    fn cancel() -> MenuAxis {
        press(|a| a.cancel = true)
    }

    /// A state with `n` seats joined, on `page`.
    fn joined(n: usize, page: Page) -> SetupState {
        let d = Data::new();
        let mut s = state_with(vec![opt("A", &[1, 2])]);
        for seat in 0..n {
            s.join(seat, &d.ctx());
        }
        s.page = page;
        s
    }

    #[test]
    fn a_mode_fixes_the_hand_and_skips_its_rows_but_still_plans_a_normal_match() {
        let d = Data::new();
        let ctx = d.ctx();
        // No saved arsenal at all: a mode still starts (its hand is the arsenal).
        let mut s = state_with(Vec::new());
        s.set_mode(Some(0));
        assert_eq!(s.mode().map(|m| m.id), Some("close_quarters"));
        s.join(0, &ctx);
        s.join(1, &ctx);
        assert!(s.problem(&ctx).is_none(), "{:?}", s.problem(&ctx));
        // Page 1: the cursor walks Character, Colour, (Arsenal skipped), and wraps without ever resting on it.
        let mut seen = vec![s.cursor[0]];
        for _ in 0..4 {
            s.act(0, &press(|a| a.down = true), &ctx);
            seen.push(s.cursor[0]);
        }
        assert!(!seen.contains(&ROW_ARSENAL), "{seen:?}");
        assert!(seen.contains(&1) && seen.contains(&0), "{seen:?}");
        // Left / right on a locked row changes nothing.
        s.cursor[0] = ROW_ARSENAL;
        s.act(0, &press(|a| a.right = true), &ctx);
        assert_eq!(s.arsenal[0], 0);
        // A CPU column skips its deck row too.
        s.cpu[2] = Some(CpuSeat { grade: 2, deck: 0 });
        s.joined[2] = true;
        s.cursor[2] = 2;
        s.focus[0] = 2;
        s.cpu_options = vec![opt("Deck", &[1])];
        s.grades = None;
        s.act(0, &press(|a| a.down = true), &ctx);
        assert_eq!(s.cursor[2], 0, "Strength (2): the deck row (3) is skipped, and without teams there is no row after it, so it wraps");
        s.teams_mode = true;
        s.cursor[2] = 2;
        s.act(0, &press(|a| a.down = true), &ctx);
        assert_eq!(s.cursor[2], 4, "with teams, Strength goes straight to Team");
        s.teams_mode = false;
        s.remove_cpu(2);
        // Page 2: Game rule, Starting level and Starting skills are skipped by the cursor and cannot change.
        s.page = Page::Rules;
        s.rcursor[0] = R_STAGE;
        s.act(0, &press(|a| a.down = true), &ctx);
        assert_eq!(s.rcursor[0], R_TIME, "the game rule row is skipped");
        s.rcursor[0] = R_ROUNDS;
        s.act(0, &press(|a| a.down = true), &ctx);
        assert_eq!(s.rcursor[0], R_START, "starting level and skills are skipped, and Friendly fire is hidden outside teams");
        s.rcursor[0] = R_LEVEL;
        s.act(0, &press(|a| a.right = true), &ctx);
        s.rcursor[0] = R_RULE;
        s.act(0, &press(|a| a.right = true), &ctx);
        assert_eq!((s.level_idx, s.rule_idx), (DEFAULT_LEVEL_IDX, 0));
        // What is shown for them.
        assert_eq!(shared_text(&s, &ctx, R_LEVEL), ("STARTING LEVEL", "10".to_string(), true));
        assert_eq!(shared_text(&s, &ctx, R_HAND).1, "FIXED");
        assert_eq!(seat_text(&s, &ctx, 0, 2).0, "FIXED HAND");
        // The plan: everyone plays the mode's hand, the neutral Aura rate, and the match is told the mode.
        let plan = s.plan(&ctx).expect("valid");
        assert_eq!(plan.setup.mode, Some(0));
        assert_eq!(plan.setup.rule, 0);
        assert!(plan.players.iter().all(|p| p.arsenal == [3, 29, 240, 75]));
        assert_eq!(plan.setup.arsenal_kinds[..2], [Some(arsenal::DEFAULT_KIND); 2]);
        // Leaving the mode (VERSUS MODE on the title) gives the arsenal rows back.
        s.set_mode(None);
        assert!(s.plan(&ctx).is_err(), "no arsenals: the normal versus needs one");
        assert!(s.mode().is_none());
        assert!(!s.shared_locked(R_LEVEL));
    }

    #[test]
    fn the_gamemodes_row_cycles_standard_then_every_mode_and_locks_what_the_mode_fixes() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![opt("Mixed", &[1, 65])]);
        s.join(0, &ctx);
        s.page = Page::Rules;
        // The row is the first on the page and starts on STANDARD with nothing locked.
        assert_eq!(R_GAMEMODE, 0);
        assert!(s.mode().is_none());
        assert_eq!(shared_text(&s, &ctx, R_GAMEMODE), ("GAMEMODES", "STANDARD".to_string(), false));
        for r in 0..R_COUNT {
            assert!(!s.shared_locked(r), "row {r} is free in STANDARD");
        }
        // Right: STANDARD -> CLOSE QUARTERS -> (every later MODES entry) -> STANDARD again.
        s.rcursor[0] = R_GAMEMODE;
        let mut seen = vec![shared_text(&s, &ctx, R_GAMEMODE).1];
        for _ in 0..modes::MODES.len() {
            s.act(0, &press(|a| a.right = true), &ctx);
            seen.push(shared_text(&s, &ctx, R_GAMEMODE).1);
        }
        assert_eq!(seen[1], "CLOSE QUARTERS");
        assert_eq!(seen.len(), modes::MODES.len() + 1);
        assert_eq!(s.mode, Some(modes::MODES.len() - 1), "the last entry");
        s.act(0, &press(|a| a.right = true), &ctx);
        assert!(s.mode.is_none(), "wraps back to STANDARD");
        // Left from STANDARD goes to the last mode.
        s.act(0, &press(|a| a.left = true), &ctx);
        assert_eq!(s.mode, Some(modes::MODES.len() - 1));
        // In CLOSE QUARTERS: game rule, starting level and skills are locked and the arsenal line is fixed; the rest is free.
        s.set_mode(Some(0));
        s.level_idx = 3;
        for r in [R_RULE, R_LEVEL, R_HAND] {
            assert!(s.shared_locked(r), "row {r} is fixed by the mode");
        }
        for r in [R_GAMEMODE, R_MODE, R_STAGE, R_TIME, R_ROUNDS, R_FRIENDLY, R_START] {
            assert!(!s.shared_locked(r), "row {r} stays free");
        }
        assert!(s.seat_line_locked(false, 2) && s.seat_line_locked(true, 3), "the arsenal line of a person and of a CPU");
        assert_eq!(shared_text(&s, &ctx, R_GAMEMODE).1, "CLOSE QUARTERS");
        assert_eq!(shared_text(&s, &ctx, R_HAND), ("STARTING SKILLS", "FIXED".to_string(), true));
        // The cursor wraps from START to the GAMEMODES row, and skips the locked rows.
        s.rcursor[0] = R_START;
        s.act(0, &press(|a| a.down = true), &ctx);
        assert_eq!(s.rcursor[0], R_GAMEMODE);
        // Back to STANDARD (left from CLOSE QUARTERS, the first entry): the normal rows work again and the arsenal
        // line is unlocked.
        s.rcursor[0] = R_GAMEMODE;
        s.act(0, &press(|a| a.left = true), &ctx);
        assert!(s.mode.is_none());
        for r in 0..R_COUNT {
            assert!(!s.shared_locked(r), "row {r} unlocks again");
        }
        assert!(!s.seat_line_locked(false, 2));
        s.rcursor[0] = R_LEVEL;
        s.act(0, &press(|a| a.right = true), &ctx);
        assert_eq!(s.level_idx, 4, "the starting Level moves again");
        // The plan carries the choice into the match.
        s.set_mode(Some(0));
        s.join(1, &ctx);
        assert_eq!(s.plan(&ctx).expect("valid").setup.mode, Some(0));
        s.set_mode(None);
        assert_eq!(s.plan(&ctx).expect("valid").setup.mode, None);
        // The help line names the choice.
        s.rcursor[0] = R_GAMEMODE;
        assert!(describe(&s, &ctx, 0).starts_with("The normal versus"));
        s.set_mode(Some(0));
        assert_eq!(describe(&s, &ctx, 0), modes::MODES[0].blurb);
        // The rule line under the stage picture names the mode, not the Standard rule it runs on.
        assert_eq!(rule_line(&s), format!("{}: 20 Health, Level 10, 10 min.", modes::MODES[0].name));
        s.set_mode(None);
        assert!(rule_line(&s).starts_with(arsenal::RULE_NAMES[s.rule()]), "no mode: the game rule's own summary");
    }

    fn prop_hunt_index() -> usize {
        modes::by_id("prop_hunt").unwrap()
    }

    #[test]
    fn prop_hunt_is_offered_after_close_quarters_and_turns_time_and_rounds_into_its_own_rows() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(2, Page::Rules);
        s.rcursor[0] = R_GAMEMODE;
        s.act(0, &press(|a| a.right = true), &ctx);
        s.act(0, &press(|a| a.right = true), &ctx);
        assert_eq!(s.mode, Some(prop_hunt_index()), "STANDARD, CLOSE QUARTERS, PROP HUNT");
        assert_eq!(shared_text(&s, &ctx, R_GAMEMODE).1, "PROP HUNT");
        // Its rows: kills to win 5, round time 2:00 (no round limit).
        assert_eq!(shared_text(&s, &ctx, R_ROUNDS), ("KILLS TO WIN", "5".to_string(), false));
        assert_eq!(shared_text(&s, &ctx, R_TIME), ("ROUND TIME", "2:00".to_string(), false));
        assert_eq!(s.prop_hunt_config(), crate::prophunt::Config { hunt_ticks: 120 * 60, kills_to_win: 5 });
        assert_eq!(rule_line(&s), "Prop Hunt: rounds of 2:00, the seeker has 10 knives, first to 5 kills wins.");
        // The rows change: one step right each.
        s.change_rule_row(R_ROUNDS, true, &ctx);
        s.change_rule_row(R_TIME, true, &ctx);
        assert_eq!(shared_text(&s, &ctx, R_ROUNDS).1, "6");
        assert_eq!(shared_text(&s, &ctx, R_TIME).1, "3:00");
        s.change_rule_row(R_TIME, false, &ctx);
        s.change_rule_row(R_TIME, false, &ctx);
        assert_eq!(shared_text(&s, &ctx, R_TIME).1, "1:30");
        // ... and the normal versus' time and rounds are left as they were.
        assert_eq!((s.time_idx, s.rounds_idx), (DEFAULT_TIME_IDX, DEFAULT_ROUNDS_IDX));
        // Fixed rows are dimmed and skipped by the cursor: team play, rule, level, skills, friendly fire.
        for r in [R_MODE, R_RULE, R_LEVEL, R_HAND, R_FRIENDLY] {
            assert!(s.shared_locked(r), "row {r}");
            assert!(shared_text(&s, &ctx, r).2, "row {r} dimmed");
        }
        for r in [R_GAMEMODE, R_STAGE, R_TIME, R_ROUNDS, R_START] {
            assert!(!s.shared_locked(r), "row {r} works");
        }
        s.rcursor[0] = R_GAMEMODE;
        s.act(0, &press(|a| a.down = true), &ctx);
        assert_eq!(s.rcursor[0], R_STAGE, "team play is skipped");
        // The plan carries the mode and its two numbers to the match.
        let plan = s.plan(&ctx).expect("two people can play");
        assert_eq!(plan.setup.mode, Some(prop_hunt_index()));
        assert_eq!(plan.setup.prop_hunt, crate::prophunt::Config { hunt_ticks: 90 * 60, kills_to_win: 6 });
        assert!(plan.players.iter().all(|p| p.arsenal == vec![modes::skill::PSYCHO_KNIFE]));
        assert_eq!(plan.players.iter().map(|p| p.team).collect::<Vec<_>>(), [0, 1], "free for all");
        // Leaving the mode gives the versus rows back.
        s.set_mode(None);
        assert_eq!(shared_text(&s, &ctx, R_ROUNDS).0, "ROUNDS TO WIN");
        assert_eq!(shared_text(&s, &ctx, R_TIME).0, "TIME LIMIT");
    }

    #[test]
    fn prop_hunt_turns_teams_off_and_refuses_cpus_and_a_lone_player() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(2, Page::Rules);
        s.teams_mode = true;
        s.set_mode(Some(prop_hunt_index()));
        assert!(!s.teams_mode, "the roles are the teams");
        // One person: can't start.
        let mut lone = joined(1, Page::Rules);
        lone.set_mode(Some(prop_hunt_index()));
        assert!(lone.problem(&ctx).unwrap().contains("at least two people"));
        assert!(lone.plan(&ctx).is_err());
        // A CPU seat (made before the mode was picked): can't start, with the reason.
        let mut s = joined(2, Page::Rules);
        s.cpu_data = true;
        s.grades = Some(grades());
        s.cpu_options = vec![opt("Deck", &[1, 2, 3])];
        assert!(s.add_cpu(2, &ctx), "fine in a versus");
        s.set_mode(Some(prop_hunt_index()));
        assert!(s.problem(&ctx).unwrap().contains("no CPUs"), "{:?}", s.problem(&ctx));
        assert_eq!(s.act(0, &press(|a| a.start = true), &ctx), Act::Start, "Start is asked for ...");
        assert!(s.plan(&ctx).is_err(), "... and refused by the plan");
        // While Prop Hunt is the mode, no new CPU can be added.
        s.remove_cpu(2);
        assert!(!s.add_cpu(2, &ctx));
        assert!(!s.focus_targets(0).contains(&3), "an empty seat is not offered for a CPU");
        assert!(s.problem(&ctx).is_none());
        // Back to the versus: CPUs again.
        s.set_mode(None);
        assert!(s.add_cpu(2, &ctx));
    }

    #[test]
    fn each_seat_reopens_on_its_remembered_picks() {
        use crate::frontend::player_memory::{PlayerMemory, SeatPick};
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(2, Page::Characters);
        s.char_idx = [1, 0, 0, 0];
        s.colour = [2, 1, 0, 0];
        let mem = s.remembered(&ctx, &PlayerMemory::default());
        let p1 = mem.seats[0].clone().expect("seat 1 remembered");
        assert_eq!((p1.character.as_str(), p1.colour), (ctx.chars[1].id.as_str(), 2));
        assert_eq!(p1.arsenal, s.options[s.arsenal[0]].name);
        assert!(mem.seats[1].is_some() && mem.seats[2].is_none(), "only seated people are remembered");
        // A fresh setup recalls them; an unknown character or arsenal leaves the pick as it is.
        let mut fresh = state_with(vec![opt("A", &[1, 2])]);
        fresh.recall(&ctx, &mem);
        assert_eq!((fresh.char_idx[0], fresh.colour[0]), (1, 2));
        let mut odd = mem.clone();
        odd.seats[0] = Some(SeatPick { character: "nobody".into(), colour: 1, arsenal: "gone".into(), team: 0 });
        let mut other = state_with(vec![opt("A", &[1, 2])]);
        other.recall(&ctx, &odd);
        assert_eq!((other.char_idx[0], other.colour[0], other.arsenal[0]), (0, 1, 0));
        // An older entry for a seat that is now empty is kept.
        let kept = s.remembered(&ctx, &PlayerMemory { seats: [None, None, Some(SeatPick { character: "pc05a".into(), ..Default::default() }), None] });
        assert_eq!(kept.seats[2].as_ref().map(|p| p.character.as_str()), Some("pc05a"));
    }

    #[test]
    fn player_one_backs_out_of_the_setup_even_when_others_have_joined() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(3, Page::Characters);
        // Player 1 at the top of their column: B leaves, and the input system takes that back to the title.
        s.cursor[0] = ROW_CHARACTER;
        assert_eq!(s.act(0, &press(|a| a.cancel = true), &ctx), Act::Leave);
        assert!(s.leaving_goes_back(0), "player 1 backs out of the screen");
        // Anyone else only gives up their own seat.
        assert!(!s.leaving_goes_back(1) && !s.leaving_goes_back(2));
        // With seat 1 gone, the first person seated is player 1.
        s.leave(0);
        assert!(s.leaving_goes_back(1));
        let _ = ctx;
    }

    #[test]
    fn four_seats_join_in_order_and_each_gets_its_own_costume() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![opt("Mixed", &[1, 65])]);
        // Everyone lands on the same character: colours 0..3, none shared.
        s.char_idx = [0; 4];
        for seat in 0..4 {
            assert_eq!(s.free_seat(), Some(seat));
            s.join(seat, &ctx);
        }
        assert_eq!(s.free_seat(), None);
        assert_eq!(s.joined_count(), 4);
        let mut c = s.colour;
        c.sort_unstable();
        assert_eq!(c, [0, 1, 2, 3]);
        // Leaving frees the lowest-numbered gap for the next to join.
        s.leave(1);
        assert_eq!(s.free_seat(), Some(1));
    }

    #[test]
    fn a_new_setup_has_nobody_in_any_seat_and_opens_on_the_characters() {
        let s = SetupState::default();
        assert_eq!(s.joined_count(), 0, "seats 1-4 are all empty until someone presses a button");
        assert_eq!(s.page, Page::Characters);
        assert_eq!(s.free_seat(), Some(0));
        assert!(s.problem(&Data::new().ctx()).unwrap().contains("join"));
        assert!(!s.all_ready(), "nobody joined is not 'everyone ready'");
    }

    #[test]
    fn one_player_alone_then_a_second_joins_the_lowest_empty_seat() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![opt("A", &[1])]);
        s.join(0, &ctx);
        assert_eq!((s.joined, s.free_seat()), ([true, false, false, false], Some(1)), "seats 2-4 show the join prompt");
        // A press on a spare pad joins seat 2, then 3; seat 4 stays empty.
        let seat = s.free_seat().unwrap();
        s.join(seat, &ctx);
        s.join(s.free_seat().unwrap(), &ctx);
        assert_eq!(s.joined, [true, true, true, false]);
        // Seat 2 leaves: it is empty again and is the next to be taken.
        s.leave(1);
        assert_eq!(s.joined, [true, false, true, false]);
        assert_eq!(s.free_seat(), Some(1));
        assert!(!s.ready[1]);
    }

    #[test]
    fn colour_cycling_skips_colours_a_same_character_seat_wears() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![opt("A", &[1])]);
        s.char_idx = [0, 0, 1, 1];
        s.join(0, &ctx);
        s.join(1, &ctx);
        assert_eq!((s.colour[0], s.colour[1]), (0, 1), "the second one of a character wears colour 2");
        s.cycle_colour(0, true);
        assert_eq!(s.colour[0], 2, "seat 1 steps past colour 1, which seat 2 wears");
        s.cycle_colour(0, false);
        assert_eq!(s.colour[0], 0);
        // Changing character gives the colour back to the one that is free there.
        s.change_character(1, true, 5);
        assert_eq!(s.char_idx[1], 1);
        assert!(s.colours_taken(1).iter().all(|&t| !t));
    }

    #[test]
    fn teams_deal_evenly_over_the_joined_seats() {
        assert_eq!(default_teams(1), [0]);
        assert_eq!(default_teams(2), [0, 1]);
        assert_eq!(default_teams(3), [0, 0, 1]);
        assert_eq!(default_teams(4), [0, 0, 1, 1]);
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![opt("A", &[1])]);
        s.join(0, &ctx);
        s.join(2, &ctx);
        s.join(3, &ctx);
        s.teams_mode = true;
        s.deal_teams();
        assert_eq!((s.team[0], s.team[2], s.team[3]), (0, 0, 1));
    }

    #[test]
    fn a_four_seat_teams_setup_plans_a_two_v_two_with_the_rule_and_friendly_fire() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![ArsenalOption { name: "Psycho".into(), skills: deck(&[1, 1, 2]), kind: Some(1) }]);
        for seat in 0..4 {
            s.join(seat, &ctx);
        }
        s.page = Page::Rules;
        s.rcursor[0] = R_MODE;
        assert_eq!(s.act(0, &press(|a| a.right = true), &ctx), Act::None);
        assert!(s.teams_mode);
        s.rcursor[0] = R_RULE;
        s.act(0, &press(|a| a.right = true), &ctx);
        assert_eq!(s.rule(), 3, "Standard, then Puny");
        s.rcursor[0] = R_FRIENDLY;
        s.act(0, &press(|a| a.right = true), &ctx);
        let plan = s.plan(&ctx).expect("a valid 2v2");
        assert_eq!(plan.players.len(), 4);
        assert_eq!(plan.players.iter().map(|p| p.team).collect::<Vec<_>>(), [0, 0, 1, 1]);
        assert_eq!(plan.players.iter().map(|p| p.seat).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!(plan.setup.rule, 3);
        assert_eq!(plan.setup.friendly_fire, FriendlyFire::On, "the option was switched on");
        assert_eq!(plan.setup.arsenal_kinds, [Some(1); 4], "the arsenal's declared kind travels with the player");
        assert!(plan.setup.time_by_rule);
        // Free for all ignores the teams and the friendly-fire switch.
        s.rcursor[0] = R_MODE;
        s.act(0, &press(|a| a.left = true), &ctx);
        let ffa = s.plan(&ctx).unwrap();
        assert_eq!(ffa.players.iter().map(|p| p.team).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!(ffa.setup.friendly_fire, FriendlyFire::Off);
    }

    #[test]
    fn friendly_fire_defaults_off_and_only_changes_in_teams_mode() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(2, Page::Rules);
        s.rcursor[0] = R_FRIENDLY;
        s.act(0, &press(|a| a.right = true), &ctx);
        assert!(!s.friendly_fire, "free for all: the row does nothing");
        assert_eq!(s.plan(&ctx).unwrap().setup.friendly_fire, FriendlyFire::Off);
    }

    #[test]
    fn a_rule_moves_everyone_to_an_arsenal_that_meets_it_and_greys_the_rest() {
        let d = Data::new();
        let ctx = d.ctx();
        // "Mixed" has two schools; "Psycho" one. Single School is rule 6, the fourth in the list.
        let mut s = state_with(vec![opt("Mixed", &[1, 65]), opt("Psycho", &[1, 2])]);
        s.join(0, &ctx);
        s.join(1, &ctx);
        s.page = Page::Rules;
        assert_eq!(s.arsenal, [0, 0, 0, 0]);
        s.rcursor[0] = R_RULE;
        for _ in 0..3 {
            s.act(0, &press(|a| a.right = true), &ctx);
        }
        assert_eq!(s.rule(), 6);
        assert_eq!((s.arsenal[0], s.arsenal[1]), (1, 1), "moved off the two-school arsenal");
        assert!(s.problem(&ctx).is_none());
        // Browsing back to the ineligible arsenal greys it with the reason and blocks Start.
        s.arsenal[1] = 0;
        let why = s.arsenal_problem(1, ctx.skills).expect("greyed");
        assert!(why.contains("one school"), "{why}");
        let p = s.problem(&ctx).unwrap();
        assert!(p.contains("PLAYER 2") && p.contains("Mixed") && p.contains("doesn't meet"), "{p}");
        assert!(s.plan(&ctx).is_err());
    }

    #[test]
    fn a_rule_no_arsenal_meets_is_rejected_with_the_original_words() {
        let d = Data::new();
        let ctx = d.ctx();
        // Heavy Only needs every skill to cost 3 or more: skill 1 costs 2.
        let mut s = state_with(vec![opt("Cheap", &[1])]);
        s.join(0, &ctx);
        s.rule_idx = GAME_RULES.iter().position(|&r| r == 9).unwrap();
        assert!(!s.rule_available(ctx.skills));
        assert_eq!(s.problem(&ctx).as_deref(), Some("No arsenals meet the rules."));
        assert!(s.plan(&ctx).is_err());
        // A deck of cost >= 3 skills qualifies.
        s.options.push(opt("Heavy", &[2, 65]));
        assert!(s.rule_available(ctx.skills));
        assert_eq!(s.first_eligible(0, ctx.skills), 1);
    }

    #[test]
    fn teams_with_one_team_and_empty_setups_are_rejected() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![opt("A", &[1])]);
        assert!(s.problem(&ctx).unwrap().contains("join"), "nobody has joined");
        s.join(0, &ctx);
        s.join(1, &ctx);
        s.teams_mode = true;
        s.team = [0, 1, 1, 1];
        assert!(s.problem(&ctx).is_none());
        s.team[1] = 0;
        assert!(s.problem(&ctx).unwrap().contains("opponent"));
        // One player alone is a practice match and is allowed (free for all).
        s.teams_mode = false;
        s.leave(1);
        assert!(s.problem(&ctx).is_none());
    }

    #[test]
    fn starting_packs_gappy_seats_into_consecutive_slots() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![opt("A", &[1])]);
        for seat in 0..4 {
            s.join(seat, &ctx);
        }
        s.leave(1);
        let plan = s.plan(&ctx).unwrap();
        assert_eq!(plan.seats, [0, 2, 3]);
        assert_eq!(plan.players.iter().map(|p| p.seat).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(plan.setup.chars[1], plan.players[1].char, "the old two-wide setup follows the first two players");
    }

    #[test]
    fn a_ready_seat_is_frozen_until_b_takes_the_ready_mark_back() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(3, Page::Characters);
        assert_eq!(s.act(1, &confirm(), &ctx), Act::None);
        assert!(s.ready[1]);
        let (c, col) = (s.char_idx[1], s.colour[1]);
        s.act(1, &press(|a| a.right = true), &ctx);
        s.act(1, &press(|a| a.down = true), &ctx);
        assert_eq!((s.char_idx[1], s.colour[1], s.cursor[1]), (c, col, 0), "a ready seat's picks don't move");
        // B un-readies; it does not leave.
        assert_eq!(s.act(1, &cancel(), &ctx), Act::None);
        assert!(!s.ready[1] && s.joined[1]);
    }

    #[test]
    fn b_walks_to_the_top_of_the_column_then_leaves() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(2, Page::Characters);
        s.act(1, &press(|a| a.down = true), &ctx);
        s.act(1, &press(|a| a.down = true), &ctx);
        assert_eq!(s.cursor[1], ROW_ARSENAL);
        assert_eq!(s.act(1, &cancel(), &ctx), Act::None, "first B goes to the top row");
        assert_eq!(s.cursor[1], ROW_CHARACTER);
        assert_eq!(s.act(1, &cancel(), &ctx), Act::Leave, "B at the top leaves");
    }

    #[test]
    fn the_team_row_is_only_on_the_cursor_in_teams_mode() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(2, Page::Characters);
        for _ in 0..5 {
            s.act(0, &press(|a| a.down = true), &ctx);
        }
        assert!(s.cursor[0] < ROW_TEAM, "free for all: Character, Colour, Arsenal only");
        s.teams_mode = true;
        s.cursor[0] = ROW_ARSENAL;
        s.act(0, &press(|a| a.down = true), &ctx);
        assert_eq!(s.cursor[0], ROW_TEAM);
        let t = s.team[0];
        s.act(0, &press(|a| a.right = true), &ctx);
        assert_ne!(s.team[0], t);
    }

    #[test]
    fn everyone_ready_goes_on_to_the_rules_and_back_keeps_the_picks() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(3, Page::Characters);
        // Seat 2 changes character and colour first.
        s.act(1, &press(|a| a.right = true), &ctx);
        s.act(1, &press(|a| a.down = true), &ctx);
        s.act(1, &press(|a| a.right = true), &ctx);
        let picks = (s.char_idx, s.colour, s.arsenal);
        s.act(0, &confirm(), &ctx);
        s.act(1, &confirm(), &ctx);
        assert_eq!(s.page, Page::Characters, "seat 3 is not ready yet");
        assert!(!s.all_ready());
        s.act(2, &confirm(), &ctx);
        assert_eq!(s.page, Page::Rules, "the last ready press moves to the rules");
        assert_eq!(s.ready, [false; 4], "ready marks are cleared on the new page");
        // Back from the rules: page 1 again, picks and seats untouched, nobody ready.
        s.act(1, &cancel(), &ctx);
        assert_eq!(s.page, Page::Characters);
        assert_eq!((s.char_idx, s.colour, s.arsenal), picks);
        assert_eq!(s.joined, [true, true, true, false]);
        assert_eq!(s.ready, [false; 4]);
    }

    #[test]
    fn a_lone_player_does_not_auto_advance_but_start_goes_on_for_seat_one_only() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(1, Page::Characters);
        s.act(0, &confirm(), &ctx);
        assert!(s.ready[0]);
        assert_eq!(s.page, Page::Characters, "one ready player is not 'everyone': a friend may still join");
        s.act(0, &press(|a| a.start = true), &ctx);
        assert_eq!(s.page, Page::Rules, "seat 1's Start goes on, ready or not");
        // Seat 2's Start does nothing.
        let mut s = joined(2, Page::Characters);
        s.act(1, &press(|a| a.start = true), &ctx);
        assert_eq!(s.page, Page::Characters);
        s.act(0, &press(|a| a.start = true), &ctx);
        assert_eq!(s.page, Page::Rules, "...and seat 1's Start works with others not ready");
        assert!(s.plan(&ctx).is_ok());
    }

    #[test]
    fn ready_is_refused_with_an_arsenal_that_breaks_the_game_rule() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = state_with(vec![opt("Mixed", &[1, 65]), opt("One", &[1, 2])]);
        s.join(0, &ctx);
        s.join(1, &ctx);
        s.rule_idx = GAME_RULES.iter().position(|&r| r == 6).unwrap();
        s.arsenal[1] = 0;
        assert_eq!(s.act(1, &confirm(), &ctx), Act::Refused);
        assert!(!s.ready[1]);
    }

    #[test]
    fn rules_rows_are_shared_start_needs_the_start_row_and_back_leaves_the_page() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = joined(2, Page::Rules);
        assert_eq!(s.rcursor, [R_START; 4], "the cursor opens on Start battle");
        assert_eq!(s.act(0, &confirm(), &ctx), Act::Start);
        s.rcursor[0] = R_STAGE;
        assert_eq!(s.act(0, &confirm(), &ctx), Act::None, "confirm elsewhere does nothing");
        s.act(1, &press(|a| a.down = true), &ctx);
        assert_eq!(s.rcursor[1], R_GAMEMODE, "down from Start wraps to the top");
        s.rcursor[0] = R_STAGE;
        s.act(0, &press(|a| a.right = true), &ctx);
        assert_eq!(s.stage, 1);
        // Up from the first row wraps to Start.
        s.rcursor[0] = R_GAMEMODE;
        s.act(0, &press(|a| a.up = true), &ctx);
        assert_eq!(s.rcursor[0], R_START);
        // B on the rules is "back to characters", for any seat.
        assert_eq!(s.act(1, &cancel(), &ctx), Act::None);
        assert_eq!(s.page, Page::Characters);
        assert_eq!(s.stage, 1, "the rules picks survive too");
    }

    #[test]
    fn a_rematch_or_a_fresh_visit_opens_on_the_characters() {
        let mut s = joined(3, Page::Rules);
        s.ready = [true; 4];
        s.go_characters();
        assert_eq!((s.page, s.joined_count(), s.ready), (Page::Characters, 3, [false; 4]), "rematch: seats kept");
        s.page = Page::Rules;
        s.reset_seats();
        assert_eq!((s.page, s.joined_count()), (Page::Characters, 0), "from the title: everyone joins afresh");
    }

    #[test]
    fn rule_problems_name_the_reason_and_the_summary_reads_the_table() {
        let skills = vec![skill(1, 1, 2), skill(2, 1, 4), skill(65, 2, 3)];
        assert_eq!(rule_problem(0, &deck(&[1]), &skills), None);
        assert!(rule_problem(0, &deck(&[]), &skills).unwrap().contains("no skills"));
        assert!(rule_problem(6, &deck(&[1, 65]), &skills).unwrap().contains("one school"));
        assert!(rule_problem(7, &deck(&[1, 1]), &skills).unwrap().contains("more than once"));
        assert!(rule_problem(8, &deck(&[2]), &skills).unwrap().contains("more than 3"));
        assert!(rule_problem(9, &deck(&[1]), &skills).unwrap().contains("less than 3"));
        assert_eq!(rule_summary(4), "Heavy: 40 Health, Level 5+, 10 min.");
        assert_eq!(rule_summary(3), "Puny: 10 Health, Level 0-5, 10 min.");
    }

    #[test]
    fn the_preview_window_is_portrait_and_four_fit_across() {
        assert!(WINDOW.1 > WINDOW.0 * 1.25, "taller than wide");
        assert!(WINDOW.0 <= SEAT_W && SEAT_X0 + 3.0 * SEAT_STEP + SEAT_W <= ui::DESIGN.x);
        assert!(ui::CONTENT_TOP + 22.0 + WINDOW.1 < SEAT_ROWS_Y, "the window ends above the first row");
        let last_line_bottom = SEAT_ROWS_Y + (SEAT_LINES - 1) as f32 * SEAT_ROW_STEP + SEAT_CAPTION_H + SEAT_ROW_H;
        assert!(SEAT_CAPTION_H + SEAT_ROW_H <= SEAT_ROW_STEP && last_line_bottom <= NOTICE_Y && NOTICE_Y + 20.0 <= ui::CONTENT_BOTTOM - 30.0, "five lines, notice and description don't overlap");
    }

    // ---- CPU seats (lane cpuseat) ------------------------------------------------------------------------------------

    fn grades() -> [cpuseat::GradeInfo; cpuseat::GRADES] {
        std::array::from_fn(|g| {
            let row = cpuseat::GRADE_ROWS[g];
            let pct = 44 + 11 * g as i8;
            cpuseat::GradeInfo { row, target_rule: 4, attack: pct, bullet: pct, melee: pct, dodge: pct, wander: 15 - 3 * g as i8 }
        })
    }

    /// A setup that can make CPUs: two original-style decks (3 copies each) and one single-copy saved arsenal after them.
    fn cpu_state() -> SetupState {
        let mut s = state_with(vec![opt("Mine", &[1, 2])]);
        s.cpu_data = true;
        s.grades = Some(grades());
        s.cpu_options = vec![
            ArsenalOption { name: "ENEMY PSYCHO".into(), skills: deck(&[1, 1, 1, 2, 2, 2]), kind: Some(5) },
            ArsenalOption { name: "ENEMY OPTICAL".into(), skills: deck(&[65, 65, 65]), kind: Some(5) },
            ArsenalOption { name: "Saved".into(), skills: deck(&[1, 2, 65]), kind: Some(2) },
        ];
        s
    }

    fn press_all(s: &mut SetupState, seat: usize, ctx: &Ctx, f: impl FnOnce(&mut MenuAxis)) -> Act {
        let mut a = MenuAxis::default();
        f(&mut a);
        s.act(seat, &a, ctx)
    }

    #[test]
    fn y_makes_the_lowest_empty_seat_a_cpu_and_the_cursor_goes_to_it() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        assert_eq!(press_all(&mut s, 0, &ctx, |a| a.y = true), Act::None);
        assert!(s.is_cpu(1) && s.joined[1] && !s.is_human(1));
        assert_eq!((s.cpu_count(), s.human_count(), s.joined_count()), (1, 1, 2));
        assert_eq!(s.edit_column(0), 1, "the cursor is on the new CPU, so it can be set up straight away");
        assert_eq!(s.seat_name(1), "CPU 2");
        // Y again adds the next one, up to the last seat; a full table refuses.
        assert_eq!(press_all(&mut s, 0, &ctx, |a| a.y = true), Act::None);
        assert_eq!(press_all(&mut s, 0, &ctx, |a| a.y = true), Act::None);
        assert_eq!((s.cpu_count(), s.free_seat()), (3, None));
        assert_eq!(press_all(&mut s, 0, &ctx, |a| a.y = true), Act::Refused);
    }

    #[test]
    fn a_cpu_never_takes_seat_one_needs_a_person_and_needs_its_data() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        assert!(!s.add_cpu(1, &ctx), "nobody to play against");
        s.join(1, &ctx);
        assert!(!s.add_cpu(0, &ctx), "seat 1 is the keyboard's");
        assert!(!s.add_cpu(1, &ctx), "a taken seat");
        assert!(s.add_cpu(2, &ctx));
        let mut none = state_with(vec![opt("A", &[1])]);
        none.join(0, &ctx);
        assert!(!none.cpu_available() && !none.add_cpu(1, &ctx), "no CPU script / epa / deck found: no CPU");
        assert_eq!(press_all(&mut none, 0, &ctx, |a| a.y = true), Act::Refused);
    }

    #[test]
    fn a_new_cpu_gets_a_fresh_character_a_free_colour_the_middle_grade_and_the_next_original_deck() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.char_idx = [0, 0, 0, 0];
        s.join(0, &ctx);
        s.add_cpu(1, &ctx);
        assert_eq!(s.char_idx[1], 1, "the first character nobody plays");
        s.add_cpu(2, &ctx);
        assert_eq!(s.char_idx[2], 2);
        assert_eq!(s.cpu[1], Some(CpuSeat { grade: cpuseat::DEFAULT_GRADE, deck: 0 }));
        assert_eq!(s.cpu[2].map(|c| c.deck), Some(1), "each CPU starts on the next of the original's decks");
        assert_eq!(s.seat_arsenal(1).unwrap().name, "ENEMY PSYCHO");
        assert_eq!(cpuseat::grade_row(s.cpu[1].unwrap().grade), 62, "grade 3 is epa row 62");
    }

    #[test]
    fn lb_and_rb_walk_the_cursor_over_the_own_cpu_and_empty_seats_and_a_makes_an_empty_one_a_cpu() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.join(1, &ctx);
        // Seat 2 sees its own column, then the empty seats 3 and 4 (never another person's).
        assert_eq!(s.focus_targets(1), [1, 2, 3]);
        press_all(&mut s, 1, &ctx, |a| a.rb = true);
        assert_eq!(s.edit_column(1), 2, "on the empty seat 3");
        press_all(&mut s, 1, &ctx, |a| a.rb = true);
        assert_eq!(s.edit_column(1), 3);
        press_all(&mut s, 1, &ctx, |a| a.rb = true);
        assert_eq!(s.edit_column(1), 1, "wraps to the own column");
        press_all(&mut s, 1, &ctx, |a| a.lb = true);
        assert_eq!(s.edit_column(1), 3, "LB goes back");
        // A on the empty seat 4 makes that one a CPU (seat 3 stays empty for a person to join).
        press_all(&mut s, 1, &ctx, |a| a.confirm = true);
        assert!(s.is_cpu(3) && !s.joined[2]);
        assert_eq!(s.edit_column(1), 3);
        // B on an empty seat gives the cursor back.
        s.focus[1] = 2;
        press_all(&mut s, 1, &ctx, |a| a.cancel = true);
        assert_eq!(s.edit_column(1), 1);
    }

    #[test]
    fn a_humans_cursor_edits_the_cpus_character_grade_and_deck_and_a_gives_it_back() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.add_cpu(1, &ctx);
        s.focus[0] = 1;
        let ch = s.char_idx[1];
        press_all(&mut s, 0, &ctx, |a| a.right = true);
        assert_eq!(s.char_idx[1], (ch + 1) % 5, "line 1 = Character");
        assert_eq!(s.char_idx[0], 0, "the person's own pick didn't move");
        press_all(&mut s, 0, &ctx, |a| a.down = true);
        press_all(&mut s, 0, &ctx, |a| a.down = true);
        assert_eq!(row_kind(true, s.cursor[1]), Row::Strength);
        press_all(&mut s, 0, &ctx, |a| a.right = true);
        assert_eq!(s.cpu[1].unwrap().grade, 3);
        assert_eq!(cpuseat::grade_row(3), 61);
        for _ in 0..2 {
            press_all(&mut s, 0, &ctx, |a| a.right = true);
        }
        assert_eq!(s.cpu[1].unwrap().grade, 0, "wraps from grade 5 to grade 1");
        press_all(&mut s, 0, &ctx, |a| a.down = true);
        assert_eq!(row_kind(true, s.cursor[1]), Row::Arsenal);
        press_all(&mut s, 0, &ctx, |a| a.right = true);
        assert_eq!(s.seat_arsenal(1).unwrap().name, "ENEMY OPTICAL");
        press_all(&mut s, 0, &ctx, |a| a.right = true);
        press_all(&mut s, 0, &ctx, |a| a.right = true);
        assert_eq!(s.seat_arsenal(1).unwrap().name, "ENEMY PSYCHO", "the deck list wraps: the originals, then the saved ones");
        // A is "done": the cursor is the person's own again, and they are not ready yet.
        press_all(&mut s, 0, &ctx, |a| a.confirm = true);
        assert_eq!(s.edit_column(0), 0);
        assert!(!s.ready[0]);
    }

    #[test]
    fn the_cpus_team_row_only_exists_in_teams_mode() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.add_cpu(1, &ctx);
        s.focus[0] = 1;
        for _ in 0..6 {
            press_all(&mut s, 0, &ctx, |a| a.down = true);
        }
        assert!(s.cursor[1] < 4, "free for all: Character, Colour, Strength, Arsenal");
        s.teams_mode = true;
        s.cursor[1] = 3;
        press_all(&mut s, 0, &ctx, |a| a.down = true);
        assert_eq!(row_kind(true, s.cursor[1]), Row::Team);
        let t = s.team[1];
        press_all(&mut s, 0, &ctx, |a| a.right = true);
        assert_ne!(s.team[1], t);
    }

    #[test]
    fn x_or_b_at_the_top_removes_the_cpu_and_the_last_person_leaving_takes_them_all() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.add_cpu(1, &ctx);
        s.add_cpu(2, &ctx);
        s.focus[0] = 1;
        press_all(&mut s, 0, &ctx, |a| a.x = true);
        assert!(!s.joined[1] && s.cpu[1].is_none() && s.is_cpu(2));
        assert_eq!(s.edit_column(0), 0);
        // B: first walks the CPU's cursor to the top, then removes it.
        s.focus[0] = 2;
        s.cursor[2] = 2;
        press_all(&mut s, 0, &ctx, |a| a.cancel = true);
        assert!(s.is_cpu(2) && s.cursor[2] == 0);
        press_all(&mut s, 0, &ctx, |a| a.cancel = true);
        assert!(!s.is_cpu(2) && !s.joined[2]);
        assert_eq!(s.free_seat(), Some(1), "the seat is empty again for a person or another CPU");
        // The only person leaving takes the CPUs with them.
        s.add_cpu(1, &ctx);
        s.add_cpu(2, &ctx);
        s.leave(0);
        assert_eq!((s.joined_count(), s.cpu_count()), (0, 0));
    }

    #[test]
    fn a_cpu_is_always_ready_so_one_person_plus_cpus_advance_when_that_person_is_ready() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        assert!(!s.all_ready());
        s.add_cpu(1, &ctx);
        s.add_cpu(2, &ctx);
        assert!(!s.all_ready(), "the person has not pressed A");
        s.focus[0] = 0;
        assert_eq!(press_all(&mut s, 0, &ctx, |a| a.confirm = true), Act::None);
        assert!(s.page == Page::Rules, "2+ seats and every person ready: on to the game rules");
        assert!(s.plan(&ctx).is_ok());
        // Two people and a CPU: both people must be ready.
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.join(1, &ctx);
        s.add_cpu(2, &ctx);
        s.focus[0] = 0;
        s.act(0, &confirm(), &ctx);
        assert_eq!(s.page, Page::Characters);
        s.act(1, &confirm(), &ctx);
        assert_eq!(s.page, Page::Rules);
    }

    #[test]
    fn a_ready_person_cannot_edit_a_cpu_until_b_takes_it_back() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.join(1, &ctx);
        s.add_cpu(2, &ctx);
        s.act(1, &confirm(), &ctx);
        assert!(s.ready[1]);
        press_all(&mut s, 1, &ctx, |a| a.y = true);
        press_all(&mut s, 1, &ctx, |a| a.rb = true);
        assert_eq!((s.cpu_count(), s.edit_column(1)), (1, 1), "ready: nothing changes");
    }

    #[test]
    fn starting_writes_the_seat_kinds_with_the_cpus_rows_and_hands_the_match_the_cpus_deck() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        for seat in 1..4 {
            s.add_cpu(seat, &ctx);
        }
        // Grades 1, 3 and 5 on the three CPUs: epa rows 64, 62 and 60.
        s.cpu[1].as_mut().unwrap().grade = 0;
        s.cpu[3].as_mut().unwrap().grade = 4;
        s.cpu[2].as_mut().unwrap().deck = 2;
        let plan = s.plan(&ctx).expect("1 person + 3 CPUs is a valid setup");
        assert_eq!(plan.kinds.0, [SeatKind::Human, SeatKind::Cpu { epa_row: 64 }, SeatKind::Cpu { epa_row: 62 }, SeatKind::Cpu { epa_row: 60 }]);
        assert!(plan.kinds.any_cpu());
        assert_eq!(plan.players.len(), 4);
        assert_eq!(plan.players[0].arsenal, deck(&[1, 2]), "the person's own arsenal");
        assert_eq!(plan.players[1].arsenal, deck(&[1, 1, 1, 2, 2, 2]), "CPU 2 plays ENEMY PSYCHO");
        assert_eq!(plan.players[2].arsenal, deck(&[1, 2, 65]), "CPU 3 plays the saved arsenal it was given");
        assert_eq!(plan.setup.arsenal_kinds[1], Some(5), "the deck's declared kind (the file's +0x50) travels with it");
        assert_eq!(plan.players.iter().map(|p| p.seat).collect::<Vec<_>>(), [0, 1, 2, 3]);
        // No CPU: all humans, the plan says so (the resource is overwritten every start, so a CLI --cpu cannot leak in).
        let mut h = cpu_state();
        h.join(0, &ctx);
        h.join(1, &ctx);
        assert!(!h.plan(&ctx).unwrap().kinds.any_cpu());
    }

    #[test]
    fn slots_pack_over_the_cpus_too_and_teams_deal_over_people_and_cpus() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.join(2, &ctx);
        s.add_cpu(3, &ctx);
        let plan = s.plan(&ctx).unwrap();
        assert_eq!(plan.seats, [0, 2, 3]);
        assert_eq!(plan.kinds.0, [SeatKind::Human, SeatKind::Human, SeatKind::Cpu { epa_row: 62 }, SeatKind::Human], "the CPU is slot 2 after packing");
        // 1 person + 1 CPU in teams: one each. 1 person + 3 CPUs: 2v2 by seat order.
        let mut t = cpu_state();
        t.teams_mode = true;
        t.join(0, &ctx);
        t.add_cpu(1, &ctx);
        assert_eq!(t.plan(&ctx).unwrap().players.iter().map(|p| p.team).collect::<Vec<_>>(), [0, 1]);
        t.add_cpu(2, &ctx);
        t.add_cpu(3, &ctx);
        assert_eq!(t.plan(&ctx).unwrap().players.iter().map(|p| p.team).collect::<Vec<_>>(), [0, 0, 1, 1]);
        // Removing one re-deals.
        t.remove_cpu(3);
        assert_eq!(t.plan(&ctx).unwrap().players.iter().map(|p| p.team).collect::<Vec<_>>(), [0, 0, 1]);
    }

    #[test]
    fn a_cpu_cannot_be_the_first_player_and_a_stage_without_nav_data_refuses_to_start() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.join(2, &ctx);
        s.add_cpu(1, &ctx);
        s.leave(0);
        assert!(s.is_cpu(1) && s.is_human(2));
        assert!(s.problem(&ctx).unwrap().contains("first player must be a person"));
        assert!(s.plan(&ctx).is_err());
        s.join(0, &ctx);
        assert!(s.problem(&ctx).is_none());
        // Only st03 has a nav graph in this fake world; st06 doesn't.
        s.nav_stages = Some(vec!["st03".into()]);
        assert!(s.problem(&ctx).is_none());
        s.stage = 1;
        let why = s.problem(&ctx).unwrap();
        assert!(why.contains("Storm Refinery") && why.contains("navigation"), "{why}");
        assert!(s.plan(&ctx).is_err());
        // A match with people only is unaffected by it.
        s.clear_cpus();
        assert!(s.problem(&ctx).is_none());
    }

    #[test]
    fn a_game_rule_moves_cpu_decks_off_the_ones_that_break_it_and_greys_a_pick_that_does() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.options = vec![opt("Mine", &[1, 2, 65])];
        s.join(0, &ctx);
        s.add_cpu(1, &ctx);
        assert_eq!(s.cpu[1].unwrap().deck, 0);
        // Highlander (rule 7, the fifth in the list): the original decks hold three copies of a skill, the saved one does not.
        s.page = Page::Rules;
        s.rcursor[0] = R_RULE;
        for _ in 0..4 {
            s.act(0, &press(|a| a.right = true), &ctx);
        }
        assert_eq!(s.rule(), 7);
        assert_eq!(s.cpu[1].unwrap().deck, 2, "moved to the saved arsenal, the first that qualifies");
        s.cpu[1].as_mut().unwrap().deck = 0;
        let why = s.arsenal_problem(1, ctx.skills).unwrap();
        assert!(why.contains("more than once"), "{why}");
        let p = s.problem(&ctx).unwrap();
        assert!(p.contains("CPU 2") && p.contains("ENEMY PSYCHO"), "{p}");
    }

    #[test]
    fn the_cpu_columns_lines_are_character_colour_strength_arsenal_team_and_a_humans_end_in_the_ready_banner() {
        let human: Vec<Row> = (0..SEAT_LINES).map(|k| row_kind(false, k)).collect();
        let cpu: Vec<Row> = (0..SEAT_LINES).map(|k| row_kind(true, k)).collect();
        assert_eq!(human, [Row::Character, Row::Colour, Row::Arsenal, Row::Team, Row::Ready]);
        assert_eq!(cpu, [Row::Character, Row::Colour, Row::Strength, Row::Arsenal, Row::Team]);
        assert_eq!((cursor_lines(false, false), cursor_lines(false, true), cursor_lines(true, false), cursor_lines(true, true)), (3, 4, 4, 5));
        assert_eq!(caption_text(true, 2), "Strength");
        assert_eq!(caption_text(false, 4), "", "the ready banner has no caption");
    }

    #[test]
    fn a_rematch_keeps_the_cpus_but_a_fresh_visit_clears_them() {
        let d = Data::new();
        let ctx = d.ctx();
        let mut s = cpu_state();
        s.join(0, &ctx);
        s.add_cpu(1, &ctx);
        s.cpu[1].as_mut().unwrap().grade = 4;
        s.go_rules();
        s.go_characters();
        assert_eq!(s.cpu[1].map(|c| c.grade), Some(4), "rematch: seats, grades and decks kept");
        s.reset_seats();
        assert_eq!((s.joined_count(), s.cpu_count()), (0, 0));
    }
}
