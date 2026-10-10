//! Controller vibration, played render-side from sim state (docs/RUMBLE.md).
//!
//! The original runs a small "feedback" system (`pdvkFBManager`, one `pdvkComFeedBack` timeline per pad):
//! an action-table row or a hit-reaction row names up to five (motor, strength, delay, length) pieces,
//! the timeline steps once per program tick (30 Hz), and every frame the sum is sent to the pad
//! (`Windows.Gaming.Input.Gamepad.Vibration`: left motor, right motor; the triggers stay 0).
//! This module rebuilds that timeline from the same table rows (`skills.json` carries each row's `raw`
//! words) and reads only sim state: `Fighter::cast_seq` (cast rumble), `CombatEvent::SkillFired`
//! (fire rumble), `Fighter::hit_seq` (the hit reaction row's rumble) and `CombatEvent::KnockLanded`
//! (the landing rows). The sim never reads anything here.
//!
//! Left motor = Bevy `strong_motor`, right motor = `weak_motor` (the Xbox pad's low / high frequency motors).

use crate::combat::{CombatEvent, MAX_PLAYERS};
use crate::{menu, seats, World};
use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

/// One 0x72-byte table row: 57 `i16`.
pub type Raw = [i16; 57];

/// The motors a piece drives: the program's kind 1 = both, 2 = left, 3 = right (`FUN_14025a8a0`).
const fn mask_of(kind: i16) -> u8 {
    match kind {
        1 => 3,
        2 => 1,
        3 => 2,
        _ => 0,
    }
}

/// One piece of a timeline (a 0x1c-byte slot of the program's `pdvkComFeedBack`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Piece {
    /// `false` = set the level once at `start`; `true` = add `strength` on every tick in `start..end`.
    add: bool,
    mask: u8,
    /// Table units (0..100 = percent of full).
    strength: f32,
    start: i32,
    end: i32,
    active: bool,
}

/// What `FUN_14022c780` step does to the level after the timeline is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Loop {
    #[default]
    Stop,
    /// Row word w5 = 2: step back one tick each time past the end.
    Back,
    /// Row word w5 = 1: restart.
    Restart,
}

/// One pad's timeline (the program's `pdvkComFeedBack`, `FUN_14022c780` per tick).
#[derive(Clone, Debug, Default)]
pub struct Timeline {
    pieces: Vec<Piece>,
    /// Ticks before the timeline starts (`+0x210`) and its end (`+0x214`).
    delay: i32,
    end: i32,
    /// The program's tick counter (`+0x218`); -1 = finished / idle.
    tick: i32,
    loop_mode: Loop,
    left: i32,
    right: i32,
}

impl Timeline {
    pub fn idle() -> Self {
        Timeline { tick: -1, ..Default::default() }
    }

    pub fn is_idle(&self) -> bool {
        self.tick == -1
    }

    /// Drops everything (`FUN_14025ace0`, and the program's stop).
    pub fn clear(&mut self) {
        *self = Timeline::idle();
    }

    /// A plain buzz of both motors at `strength` (table units, 0..100) for `ticks` program ticks (30 Hz), then off. Not
    /// a table row: Prop Hunt's "the seeker's controller rumbles once the countdown ends" (`prophunt.rs`).
    pub fn buzz(&mut self, strength: f32, ticks: i32) {
        *self = Timeline { pieces: vec![Piece { add: false, mask: 3, strength, start: 0, end: 1, active: true }], delay: 0, end: ticks.max(1) - 1, tick: 0, loop_mode: Loop::Stop, left: 0, right: 0 };
    }

    /// The program's cast block of a row (`FUN_14025a8a0`, words 5..25). Nothing happens when the main kind (w6) is not 1..3.
    pub fn start_cast(&mut self, w: &Raw) {
        if !(1..=3).contains(&w[6]) {
            return;
        }
        self.delay = w[8] as i32;
        self.end = w[9] as i32 + w[8] as i32;
        self.tick = 0;
        self.pieces.clear();
        // The main piece: SET the level at the delay tick.
        self.pieces.push(Piece { add: false, mask: mask_of(w[6]), strength: w[7] as f32, start: w[8] as i32, end: w[8] as i32 + 1, active: true });
        for k in 0..4 {
            let (kind, s, d, l) = (w[10 + 4 * k], w[11 + 4 * k], w[12 + 4 * k], w[13 + 4 * k]);
            if (1..=3).contains(&kind) {
                self.pieces.push(Piece { add: true, mask: mask_of(kind), strength: s as f32, start: d as i32 + w[8] as i32, end: d as i32 + w[8] as i32 + l as i32, active: true });
            }
        }
        self.loop_mode = match w[5] {
            1 => Loop::Restart,
            2 => Loop::Back,
            _ => Loop::Stop,
        };
        self.left = 0;
        self.right = 0;
    }

    /// The program's fire block of a row (`FUN_14025aad0`, words 26..37): replaces the cast block's pieces and timing. READ quirk: the two
    /// extra pieces are only added where the CAST block's pieces 1 and 2 (w10, w14) have a motor kind (the program tests those words).
    pub fn start_fire(&mut self, w: &Raw) {
        if !(1..=3).contains(&w[26]) {
            return;
        }
        self.delay = w[28] as i32;
        self.end = w[29] as i32 + w[28] as i32;
        self.tick = 0;
        self.pieces.clear();
        self.pieces.push(Piece { add: false, mask: mask_of(w[26]), strength: w[27] as f32, start: w[28] as i32, end: w[28] as i32 + 1, active: true });
        for k in 0..2 {
            let gate = w[10 + 4 * k];
            let (kind, s, d, l) = (w[30 + 4 * k], w[31 + 4 * k], w[32 + 4 * k], w[33 + 4 * k]);
            if (1..=3).contains(&gate) {
                self.pieces.push(Piece { add: true, mask: mask_of(kind), strength: s as f32, start: d as i32 + w[28] as i32, end: d as i32 + w[28] as i32 + l as i32, active: true });
            }
        }
        self.left = 0;
        self.right = 0;
    }

    /// One program tick (30 Hz): the (left, right) level in 0..=65535 the pad gets this tick (`FUN_14022c780` then `FUN_14022c660`).
    pub fn step(&mut self) -> (i32, i32) {
        if self.tick == -1 {
            return (0, 0);
        }
        if self.delay > self.tick {
            self.tick += 1;
            return (0, 0);
        }
        if self.end < self.tick {
            match self.loop_mode {
                Loop::Stop => {
                    self.clear();
                    return (0, 0);
                }
                Loop::Back => self.tick -= 1,
                Loop::Restart => {
                    self.tick = 0;
                    self.right = 0;
                    for p in &mut self.pieces {
                        p.active = true;
                    }
                }
            }
        }
        let tick = self.tick;
        for p in &mut self.pieces {
            if !p.active || p.start > tick {
                continue;
            }
            if tick < p.end {
                // The program's float multiply: strength x 0.01 x 65535.
                let v = (p.strength * 0.01f32 * 65535.0f32) as i32;
                if p.add {
                    if p.mask & 1 != 0 {
                        self.left += v;
                    }
                    if p.mask & 2 != 0 {
                        self.right += v;
                    }
                    continue;
                }
                if p.mask & 1 != 0 {
                    self.left = v;
                }
                if p.mask & 2 != 0 {
                    self.right = v;
                }
            }
            p.active = false;
        }
        self.left = self.left.clamp(0, 0xffff);
        self.right = self.right.clamp(0, 0xffff);
        self.tick += 1;
        (self.left, self.right)
    }
}

/// The rows rumble reads, from `skills.json` (each row's `raw`).
#[derive(Resource, Default)]
pub struct RumbleTables {
    /// skill id -> (ground, air, chain rows, s40)
    pub actions: HashMap<i16, SkillRows>,
    /// hit-reaction table row id -> words
    pub reactions: HashMap<i16, Raw>,
}

#[derive(Clone, Debug, Default)]
pub struct SkillRows {
    pub ground: Option<Raw>,
    pub air: Option<Raw>,
    pub chain: Vec<Raw>,
    pub s40: i16,
}

fn raw_of(v: &serde_json::Value) -> Option<Raw> {
    let a = v["raw"].as_array()?;
    let mut r = [0i16; 57];
    for (k, x) in a.iter().take(57).enumerate() {
        r[k] = x.as_i64().unwrap_or(0) as i16;
    }
    Some(r)
}

impl RumbleTables {
    pub fn load(path: &std::path::Path) -> Self {
        let Some(v) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return Self::default() };
        let mut t = RumbleTables::default();
        for s in v["skills"].as_array().into_iter().flatten() {
            let Some(id) = s["id"].as_i64().map(|i| i as i16) else { continue };
            t.actions.entry(id).or_insert_with(|| SkillRows {
                ground: raw_of(&s["action"]["ground"]),
                air: raw_of(&s["action"]["air"]),
                chain: s["action"]["chain"].as_array().into_iter().flatten().filter_map(raw_of).collect(),
                s40: s["raw"][40].as_i64().unwrap_or(0) as i16,
            });
        }
        for r in v["reactions"].as_array().into_iter().flatten() {
            if let (Some(row), Some(raw)) = (r["row"].as_i64(), raw_of(r)) {
                t.reactions.insert(row as i16, raw);
            }
        }
        t
    }

    /// The action row a cast of `skill` plays (`Combat::action_row` / `cast_row`: mid-air row when s40 is 1 or 3, or 2 while the cast
    /// began in the air; a chained cast k >= 1 uses `chain[(k-1) % n]`).
    pub fn cast_row(&self, skill: i16, air: bool, chain_count: u8) -> Option<&Raw> {
        let s = self.actions.get(&skill)?;
        let k = chain_count as usize;
        if k > 0 && !s.chain.is_empty() {
            return Some(&s.chain[(k - 1) % s.chain.len()]);
        }
        let want_air = s.s40 == 1 || s.s40 == 3 || (s.s40 == 2 && air);
        if want_air {
            s.air.as_ref().or(s.ground.as_ref())
        } else {
            s.ground.as_ref().or(s.air.as_ref())
        }
    }
}

/// The reaction-table rows of a knock-down landing by fall class (`FUN_140254c80`: fall < 40 -> row 34, up to 80 -> 35, above -> 36).
pub fn landing_row(class: u8) -> i16 {
    34 + class.min(2) as i16
}

/// What a frame of the driver decides for one pad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    Stop,
    Level { strong: f32, weak: f32 },
}

fn to_out(l: i32, r: i32) -> Out {
    if l == 0 && r == 0 {
        Out::Stop
    } else {
        Out::Level { strong: l as f32 / 65535.0, weak: r as f32 / 65535.0 }
    }
}

/// Prop Hunt's release buzz: both motors at this strength (table units = percent) ...
pub const RELEASE_BUZZ_STRENGTH: f32 = 80.0;
/// ... for this many program ticks (30 Hz: 0.6 s). UNCONFIRMED feel values (Laurence asked for "rumbles", no more).
pub const RELEASE_BUZZ_TICKS: i32 = 18;

/// The driver's memory: one timeline per seat and what was last seen of the sim.
#[derive(Default)]
pub struct Driver {
    lines: [Timeline; MAX_PLAYERS],
    /// Prop Hunt: seeker releases already buzzed for (`PropHunt::releases`).
    releases: u32,
    cast: [u16; MAX_PLAYERS],
    hit: [u16; MAX_PLAYERS],
    seen_event: u32,
    last_tick: u64,
    primed: bool,
    /// The last level sent per seat and the real-time seconds since it was (re)sent.
    sent: [Option<(Out, f32)>; MAX_PLAYERS],
}

impl Driver {
    pub fn stop_all(&mut self) {
        for l in &mut self.lines {
            l.clear();
        }
        self.primed = false;
    }
}

/// A new rumble request goes out when the level changes, and again this often while it holds
/// (each request lasts `HOLD`, so a missed frame can never leave a pad buzzing).
const RESEND: f32 = 0.10;
const HOLD: f32 = 0.18;

/// One frame. `humans[i]`: seat `i` is a human with a pad. Returns, per seat, what to send (`None` = nothing to do).
#[allow(clippy::too_many_arguments)]
pub fn drive_frame(d: &mut Driver, world: &crate::sim::Match, tables: &RumbleTables, humans: [bool; MAX_PLAYERS], dt: f32) -> [Option<Out>; MAX_PLAYERS] {
    let c = &world.combat;
    // A new match or round starts its numbers again: re-sync without playing anything.
    if !d.primed || world.tick < d.last_tick || c.events.last().map_or(0, |e| e.0) < d.seen_event {
        d.primed = true;
        d.seen_event = c.events.last().map_or(0, |e| e.0);
        d.releases = world.prophunt.as_ref().map_or(0, |ph| ph.releases);
        for i in 0..MAX_PLAYERS {
            d.cast[i] = c.fighters[i].cast_seq;
            d.hit[i] = c.fighters[i].hit_seq;
            d.lines[i].clear();
        }
    }
    let new_ticks = world.tick.saturating_sub(d.last_tick);
    d.last_tick = world.tick;
    // Events first (the program starts a timeline from the call sites below), in order.
    let fresh: Vec<CombatEvent> = c.events.iter().filter(|(s, _)| *s > d.seen_event).map(|e| e.1).collect();
    d.seen_event = c.events.last().map_or(d.seen_event, |e| e.0);
    for i in c.roster.slots() {
        if !humans[i] {
            d.lines[i].clear();
            d.cast[i] = c.fighters[i].cast_seq;
            d.hit[i] = c.fighters[i].hit_seq;
            continue;
        }
        let f = &c.fighters[i];
        // A hit reaction: clear what played (`FUN_14025ace0`), then the reaction row's own rumble (`FUN_140258690`).
        if f.hit_seq != d.hit[i] {
            d.hit[i] = f.hit_seq;
            d.lines[i].clear();
            if let Some(row) = tables.reactions.get(&f.reaction_row) {
                d.lines[i].start_cast(row);
            }
        }
        // A new cast (`FUN_14025a3e0`; the automatic Shelter never goes through it).
        if f.cast_seq != d.cast[i] {
            d.cast[i] = f.cast_seq;
            if f.cast_seq != 0 && f.auto_cast_seq != f.cast_seq {
                if let Some(row) = tables.cast_row(f.cast_skill, f.air_cast, f.chain_count) {
                    d.lines[i].start_cast(row);
                }
            }
        }
        for ev in &fresh {
            match *ev {
                CombatEvent::SkillFired { who, skill } if who == i => {
                    if let Some(row) = tables.cast_row(skill, f.air_cast, f.chain_count) {
                        d.lines[i].start_fire(row);
                    }
                }
                CombatEvent::KnockLanded { who } if who == i => {
                    if let Some(row) = tables.reactions.get(&landing_row(f.land_class)) {
                        d.lines[i].start_cast(row);
                    }
                }
                _ => {}
            }
        }
    }
    // Prop Hunt: the seeker was just released ("Their controller rumbles once the countdown ends").
    if let Some(ph) = &world.prophunt {
        if ph.releases != d.releases {
            d.releases = ph.releases;
            if humans[ph.seeker] {
                d.lines[ph.seeker].buzz(RELEASE_BUZZ_STRENGTH, RELEASE_BUZZ_TICKS);
            }
        }
    }
    // The program's timeline steps once per program tick = every second sim tick (60 Hz sim).
    let steps = ((world.tick / 2) - (world.tick.saturating_sub(new_ticks) / 2)) as usize;
    let mut out = [None; MAX_PLAYERS];
    for i in 0..MAX_PLAYERS {
        if !humans[i] {
            if d.sent[i].take().is_some() {
                out[i] = Some(Out::Stop);
            }
            continue;
        }
        let mut level = d.lines[i].step_level_hint();
        for _ in 0..steps {
            let (l, r) = d.lines[i].step();
            level = (l, r);
        }
        let want = to_out(level.0, level.1);
        let age = d.sent[i].map_or(f32::MAX, |(_, a)| a + dt);
        match (d.sent[i], want) {
            (None, Out::Stop) => {}
            (Some((Out::Stop, _)), Out::Stop) => {}
            (Some((prev, _)), w) if prev == w && age < RESEND => d.sent[i] = Some((prev, age)),
            (_, Out::Stop) => {
                d.sent[i] = None;
                out[i] = Some(Out::Stop);
            }
            (_, w) => {
                d.sent[i] = Some((w, 0.0));
                out[i] = Some(w);
            }
        }
    }
    out
}

impl Timeline {
    /// The level the last step produced (held between program ticks: the sim runs two ticks per program tick).
    fn step_level_hint(&self) -> (i32, i32) {
        if self.tick == -1 {
            (0, 0)
        } else {
            (self.left, self.right)
        }
    }
}

/// The Bevy system: sim state in, `GamepadRumbleRequest`s out.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    world: Res<World>,
    tables: Res<RumbleTables>,
    settings: Res<menu::Settings>,
    menu: Res<menu::MenuState>,
    seats: Res<seats::Seats>,
    kinds: Option<Res<seats::SeatKinds>>,
    state: Res<State<crate::frontend::state::AppState>>,
    time: Res<Time<Real>>,
    mut driver: Local<Driver>,
    mut out: MessageWriter<GamepadRumbleRequest>,
    mut was_live: Local<[Option<Entity>; MAX_PLAYERS]>,
) {
    let players = world.0.combat.roster.count();
    let in_match = *state.get() == crate::frontend::state::AppState::Match;
    let live = in_match && settings.rumble && !menu.open && !seats.paused(players);
    if !live {
        // Paused, setting off, or out of the match: every pad that was buzzing stops, nothing is queued.
        driver.stop_all();
        for slot in was_live.iter_mut() {
            if let Some(g) = slot.take() {
                out.write(GamepadRumbleRequest::Stop { gamepad: g });
            }
        }
        driver.sent = [None; MAX_PLAYERS];
        return;
    }
    let mut humans = [false; MAX_PLAYERS];
    for (i, h) in humans.iter_mut().enumerate().take(players) {
        let cpu = kinds.as_ref().is_some_and(|k| matches!(k.0[i], seats::SeatKind::Cpu { .. }));
        *h = !cpu && seats.pad[i].is_some();
    }
    let acts = drive_frame(&mut driver, &world.0, &tables, humans, time.delta_secs());
    for (i, act) in acts.iter().enumerate() {
        let (Some(act), Some(pad)) = (act, seats.pad[i]) else { continue };
        match *act {
            Out::Stop => {
                out.write(GamepadRumbleRequest::Stop { gamepad: pad });
                was_live[i] = None;
            }
            Out::Level { strong, weak } => {
                // Replace, never stack: the pad's level is exactly what the timeline says.
                out.write(GamepadRumbleRequest::Stop { gamepad: pad });
                out.write(GamepadRumbleRequest::Add { duration: Duration::from_secs_f32(HOLD), intensity: GamepadRumbleIntensity { strong_motor: strong, weak_motor: weak }, gamepad: pad });
                was_live[i] = Some(pad);
            }
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_message::<GamepadRumbleRequest>().init_resource::<RumbleTables>().add_systems(Update, drive);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row with the shape of a common cast: right motor 10 % for 25 ticks, then an additive right 1 % piece.
    fn cast_row_raw() -> Raw {
        let mut w = [0i16; 57];
        w[6] = 3;
        w[7] = 10;
        w[9] = 25;
        w[10] = 3;
        w[11] = 1;
        w[13] = 20;
        w
    }

    /// A hit-reaction row of the table (row 3, heavy shell): left motor 50 % for 20 ticks.
    fn heavy_hit_raw() -> Raw {
        let mut w = [0i16; 57];
        w[6] = 2;
        w[7] = 50;
        w[9] = 20;
        w
    }

    fn level(raw: i32) -> f32 {
        raw as f32 / 655.35
    }

    #[test]
    fn a_hit_row_buzzes_the_left_motor_for_its_length_then_stops() {
        let mut t = Timeline::idle();
        t.start_cast(&heavy_hit_raw());
        let mut n = 0;
        let mut last = (0, 0);
        for _ in 0..40 {
            let (l, r) = t.step();
            if l > 0 {
                n += 1;
                last = (l, r);
            }
            assert_eq!(r, 0, "a kind-2 row never touches the right motor");
        }
        assert!((level(last.0) - 50.0).abs() < 0.1);
        assert!((20..=21).contains(&n), "20 program ticks (0.67 s), got {n}");
        assert!(t.is_idle(), "a non-looping row ends");
    }

    #[test]
    fn a_cast_row_drives_the_right_motor_with_its_additive_piece() {
        let mut t = Timeline::idle();
        t.start_cast(&cast_row_raw());
        let (l, r) = t.step();
        assert_eq!(l, 0);
        assert!((level(r) - 11.0).abs() < 0.2, "10 % main + 1 % piece on the first tick: {}", level(r));
    }

    #[test]
    fn kinds_map_to_the_motors() {
        assert_eq!((mask_of(1), mask_of(2), mask_of(3), mask_of(0)), (3, 1, 2, 0));
    }

    #[test]
    fn the_fire_block_replaces_the_cast_block_and_has_the_program_quirk() {
        let mut w = cast_row_raw();
        w[26] = 3;
        w[27] = 40;
        w[29] = 5;
        w[30] = 3;
        w[31] = 10;
        w[33] = 3;
        let mut t = Timeline::idle();
        t.start_cast(&w);
        t.start_fire(&w);
        let (_, r) = t.step();
        assert!((level(r) - 50.0).abs() < 0.2, "40 % + the piece gated by the cast block's kind: {}", level(r));
        let mut quirk = w;
        quirk[10] = 0;
        let mut t = Timeline::idle();
        t.start_fire(&quirk);
        assert!((level(t.step().1) - 40.0).abs() < 0.2, "no cast piece 1 kind: the fire piece is skipped");
    }

    #[test]
    fn the_landing_rows_follow_the_fall_class() {
        assert_eq!((landing_row(0), landing_row(1), landing_row(2), landing_row(9)), (34, 35, 36, 36));
    }

    #[test]
    fn the_cast_row_choice_follows_s40_chain_and_air() {
        let mut t = RumbleTables::default();
        let (mut g, mut a, mut c) = (cast_row_raw(), cast_row_raw(), cast_row_raw());
        g[7] = 1;
        a[7] = 2;
        c[7] = 3;
        t.actions.insert(5, SkillRows { ground: Some(g), air: Some(a), chain: vec![c], s40: 2 });
        assert_eq!(t.cast_row(5, false, 0).map(|r| r[7]), Some(1));
        assert_eq!(t.cast_row(5, true, 0).map(|r| r[7]), Some(2));
        assert_eq!(t.cast_row(5, false, 1).map(|r| r[7]), Some(3));
        assert!(t.cast_row(99, false, 0).is_none());
    }

    // ---- the driver system, with mocked pads (no OS input, no real controller) ----

    use crate::frontend::state::AppState;
    use bevy::ecs::message::Messages;

    struct Rig {
        app: App,
        pads: [Entity; 2],
    }

    impl Rig {
        fn new() -> Self {
            let mut app = App::new();
            let combat = crate::combat::Combat::new(Vec::new(), vec![crate::combat::AURA_PARTICLE; 30]);
            let pads = [app.world_mut().spawn_empty().id(), app.world_mut().spawn_empty().id()];
            let mut seats = seats::Seats::default();
            seats.pad[0] = Some(pads[0]);
            seats.pad[1] = Some(pads[1]);
            let mut tables = RumbleTables::default();
            tables.reactions.insert(3, heavy_hit_raw());
            let path = std::env::temp_dir().join(format!("pd_rumble_test_{}.json", std::process::id()));
            let _ = std::fs::remove_file(&path);
            let mut settings = menu::Settings::load(path);
            settings.disable_saving();
            app.init_resource::<Time<Real>>()
                .insert_resource(World(crate::sim::Match::new([Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0)], combat)))
                .insert_resource(tables)
                .insert_resource(settings)
                .insert_resource(menu::MenuState::opened(false))
                .insert_resource(seats)
                .insert_resource(seats::SeatKinds::default())
                .insert_resource(State::new(AppState::Match))
                .add_message::<GamepadRumbleRequest>()
                .add_systems(Update, drive);
            app.update(); // primes the driver
            Rig { app, pads }
        }

        /// Seat `s` is hit by reaction row 3: the sim bumps its hit counter and picks the row.
        fn hit(&mut self, s: usize) {
            let mut w = self.app.world_mut().resource_mut::<World>();
            let f = &mut w.0.combat.fighters[s];
            f.reaction_row = 3;
            f.hit_seq = f.hit_seq.wrapping_add(1);
        }

        /// Two sim ticks = one program tick, then a frame; returns this frame's requests as (pad, is_stop, strong, weak).
        fn frame(&mut self) -> Vec<(Entity, bool, f32, f32)> {
            self.app.world_mut().resource_mut::<World>().0.tick += 2;
            self.app.update();
            let mut msgs = self.app.world_mut().resource_mut::<Messages<GamepadRumbleRequest>>();
            msgs.drain()
                .map(|m| match m {
                    GamepadRumbleRequest::Stop { gamepad } => (gamepad, true, 0.0, 0.0),
                    GamepadRumbleRequest::Add { intensity, gamepad, .. } => (gamepad, false, intensity.strong_motor, intensity.weak_motor),
                })
                .collect()
        }
    }

    #[test]
    fn only_the_hit_seats_pad_gets_a_request() {
        let mut r = Rig::new();
        r.hit(1);
        let sent = r.frame();
        let adds: Vec<_> = sent.iter().filter(|m| !m.1).collect();
        assert_eq!(adds.len(), 1, "{sent:?}");
        assert_eq!(adds[0].0, r.pads[1], "seat 2's pad, not seat 1's");
        assert!((adds[0].2 - 0.5).abs() < 0.01 && adds[0].3 == 0.0, "left (strong) motor at 50 %: {adds:?}");
        assert!(sent.iter().all(|m| m.0 == r.pads[1]), "nothing at all for the other pad");
    }

    #[test]
    fn prop_hunt_buzzes_only_the_seekers_pad_when_the_countdown_ends_then_stops() {
        let mut r = Rig::new();
        {
            let mut w = r.app.world_mut().resource_mut::<World>();
            w.0.combat.set_rules(crate::frontend::modes::MODES[1].rules());
            w.0.start_prop_hunt(crate::prophunt::Config::default());
        }
        // Countdown: nothing.
        assert!(r.frame().is_empty());
        // Round 1's seeker (seat 1) is released.
        r.app.world_mut().resource_mut::<World>().0.prophunt.as_mut().unwrap().releases += 1;
        let sent = r.frame();
        let adds: Vec<_> = sent.iter().filter(|m| !m.1).collect();
        assert_eq!(adds.len(), 1, "{sent:?}");
        assert_eq!(adds[0].0, r.pads[0], "the seeker's pad only");
        let want = RELEASE_BUZZ_STRENGTH / 100.0;
        assert!((adds[0].2 - want).abs() < 0.01 && (adds[0].3 - want).abs() < 0.01, "both motors: {adds:?}");
        // It ends by itself after its length, and the hider's pad never buzzed.
        let mut stopped = false;
        for _ in 0..RELEASE_BUZZ_TICKS + 4 {
            let sent = r.frame();
            assert!(sent.iter().all(|m| m.0 == r.pads[0]), "nothing for the hider: {sent:?}");
            stopped |= sent.iter().any(|m| m.1);
        }
        assert!(stopped, "the buzz stops");
        // No release, no buzz.
        for _ in 0..5 {
            assert!(r.frame().iter().all(|m| m.1), "quiet");
        }
    }

    #[test]
    fn a_cpu_seat_gets_nothing() {
        let mut r = Rig::new();
        r.app.world_mut().resource_mut::<seats::SeatKinds>().0[1] = seats::SeatKind::Cpu { epa_row: 60 };
        r.hit(1);
        for _ in 0..5 {
            assert!(r.frame().is_empty());
        }
    }

    #[test]
    fn pausing_stops_the_buzz_once_and_stays_quiet() {
        let mut r = Rig::new();
        r.hit(0);
        assert!(r.frame().iter().any(|m| !m.1));
        r.app.world_mut().resource_mut::<menu::MenuState>().open = true;
        let stop = r.frame();
        assert_eq!(stop.len(), 1);
        assert!(stop[0].1 && stop[0].0 == r.pads[0], "one Stop for the pad that was buzzing: {stop:?}");
        r.hit(0);
        assert!(r.frame().is_empty(), "paused: no requests");
    }

    #[test]
    fn leaving_the_match_stops_it() {
        let mut r = Rig::new();
        r.hit(0);
        r.frame();
        *r.app.world_mut().resource_mut::<State<AppState>>() = State::new(AppState::Result);
        let sent = r.frame();
        assert!(sent.len() == 1 && sent[0].1, "{sent:?}");
    }

    #[test]
    fn the_setting_off_means_no_requests() {
        let mut r = Rig::new();
        r.app.world_mut().resource_mut::<menu::Settings>().rumble = false;
        r.hit(0);
        r.hit(1);
        for _ in 0..5 {
            assert!(r.frame().is_empty());
        }
    }

    #[test]
    fn a_hit_buzz_ends_by_itself_after_its_length() {
        let mut r = Rig::new();
        r.hit(0);
        let mut buzzing_frames = 0;
        let mut last_stop = false;
        for _ in 0..40 {
            let f = r.frame();
            if f.iter().any(|m| !m.1) {
                buzzing_frames += 1;
            }
            last_stop |= f.iter().any(|m| m.1);
        }
        assert!(buzzing_frames >= 1 && last_stop, "it starts, and a Stop follows when the 20 ticks are over");
        assert!(r.frame().is_empty(), "then silence");
    }

    #[test]
    fn the_settings_default_is_on_and_round_trips() {
        let path = std::env::temp_dir().join(format!("pd_rumble_settings_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut s = menu::Settings::load(path.clone());
        assert!(s.rumble, "default on");
        s.rumble = false;
        s.save();
        assert!(!menu::Settings::load(path.clone()).rumble);
        let _ = std::fs::remove_file(&path);
    }

    /// Against the real cache (skipped when it is absent): the rows the driver reads are the ones in the file.
    #[test]
    fn the_real_skills_json_gives_the_rows_the_program_reads() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cache/skills.json");
        let t = RumbleTables::load(&path);
        if t.reactions.is_empty() {
            eprintln!("skipped: cache/skills.json not found");
            return;
        }
        // Reaction row 3 (heavy shell hit): kind 2 (left), 50 %, 20 ticks. Rows 34-36 (knock-down landings): left 40 %, 6 / 10 / 15 ticks.
        let r3 = t.reactions[&3];
        assert_eq!((r3[6], r3[7], r3[9]), (2, 50, 20));
        assert_eq!([34, 35, 36].map(|k| (t.reactions[&k][6], t.reactions[&k][7], t.reactions[&k][9])), [(2, 40, 6), (2, 40, 10), (2, 40, 15)]);
        // A cast row exists for most skills and drives the right motor.
        let with_cast = t.actions.values().filter_map(|s| s.ground.as_ref()).filter(|r| r[6] == 3).count();
        assert!(with_cast > 100, "{with_cast}");
    }
}
