//! In-game dev tools (keyboard only):
//!
//!   F1  overlay (PD_DEV_OVERLAY=1 starts with it on): live sim state for
//!       every active seat, CPUs included (position, motion, health, aura,
//!       slots, this tick's input), projectile/capsule counts and the latest
//!       combat events
//!   F2  pause / resume the match (sim only; rendering keeps running)
//!   F3  while paused: advance exactly one sim tick
//!   F8  bug report: takes a screenshot, pauses, and opens a note box.
//!       Type what went wrong, Enter saves, Esc cancels. Saved to
//!       `engine/bug_reports/<date>_<time>/`: note.txt, screenshot.png,
//!       state.txt (full dump of every active player and the match) and
//!       inputs.csv (the last 10 s of every seat's input, one row per tick).
//!       PD_DEV_TEST_REPORT=<tick> files one automatically (scripted tests).
//!
//! Reads sim state, never writes it: pausing works by `step_sim` asking
//! `DevTools::take_tick` whether to run, same as the settings menu's pause.
//! Bug reports contain screenshots of game assets: `engine/bug_reports/` is
//! git-ignored and must never be committed.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use std::collections::VecDeque;
use std::path::PathBuf;

use crate::combat::MAX_PLAYERS;
use crate::frontend::state::{AppState, MatchSetup};
use crate::sim::{Input, HZ};
use crate::{PendingInputs, World};

/// Ticks of input kept for a bug report (10 s at 60 Hz).
const INPUT_HISTORY_TICKS: usize = 600;
/// Frames between the bug-report screenshot and showing the note box, so the
/// box is never in the picture.
const NOTE_BOX_DELAY_FRAMES: u8 = 3;
/// Combat events listed in the overlay.
const OVERLAY_EVENTS: usize = 6;

#[derive(Resource, Default)]
pub struct DevTools {
    overlay: bool,
    paused: bool,
    steps: u32,
    report: Option<Report>,
    /// (tick, every seat's input), newest last.
    history: VecDeque<(u64, [Input; MAX_PLAYERS])>,
    /// Smoothed frame rate for the overlay.
    fps: f32,
}

struct Report {
    dir: PathBuf,
    state: String,
    inputs: String,
    note: String,
    /// Frames left before the note box shows (screenshot first).
    delay: u8,
    /// Frames the box has been visible (PD_DEV_TEST_REPORT auto-submit).
    open_frames: u32,
}

impl DevTools {
    /// Whether the sim will step this tick, without using up an F3 step:
    /// for systems that run just before `step_sim` (the CPU seats).
    pub fn sim_runs(&self) -> bool {
        self.report.is_none() && (!self.paused || self.steps > 0)
    }

    /// Whether the sim may step this tick (`main.rs::step_sim`). Paused by F2
    /// or by an open bug report; F3 lets single ticks through.
    pub fn take_tick(&mut self) -> bool {
        if !self.paused && self.report.is_none() {
            return true;
        }
        if self.report.is_none() && self.steps > 0 {
            self.steps -= 1;
            return true;
        }
        false
    }
}

/// Run condition: the bug-report note box is not taking keystrokes, so game
/// and menu input may run.
pub fn not_typing(dev: Res<DevTools>) -> bool {
    dev.report.is_none()
}

impl DevTools {
    /// The F8 note box is taking keystrokes (game input is off).
    pub fn typing(&self) -> bool {
        self.report.is_some()
    }
}

/// A bug report with no screenshot or typed note, filed by the game itself
/// (an online match that went out of sync, `online_match.rs`): note.txt,
/// state.txt and inputs.csv in a fresh `engine/bug_reports/<time>_auto/`.
pub(crate) fn write_auto_report(dev: &DevTools, cfg: &crate::Config, world: &World, pending: &PendingInputs, setup: &MatchSetup, note: &str) -> Option<PathBuf> {
    let root = cfg.cache.parent().map(|p| p.join("bug_reports")).unwrap_or_else(|| PathBuf::from("bug_reports"));
    let dir = root.join(format!("{}_auto", timestamp()));
    std::fs::create_dir_all(&dir).ok()?;
    let state = format!("checksum: {:016x}\n{}", world.0.checksum(), dump_state(world, pending, setup, cfg, AppState::Match, dev.paused));
    std::fs::write(dir.join("note.txt"), format!("{note}\n")).ok()?;
    std::fs::write(dir.join("state.txt"), state).ok()?;
    std::fs::write(dir.join("inputs.csv"), dump_inputs(&dev.history)).ok()?;
    Some(dir)
}

#[derive(Component)]
struct OverlayText;

#[derive(Component)]
struct NoteBox;

#[derive(Component)]
struct NoteText;

#[derive(Component)]
struct Toast(f32);

pub fn plugin(app: &mut App) {
    // PD_DEV_OVERLAY=1 starts with the overlay on (screenshot runs).
    app.insert_resource(DevTools { overlay: std::env::var_os("PD_DEV_OVERLAY").is_some(), ..default() })
        .add_systems(Startup, spawn)
        .add_systems(Update, (hotkeys, type_note, show_overlay, show_note, fade_toasts).chain())
        .add_systems(FixedUpdate, record_inputs.after(crate::step_sim).run_if(in_state(AppState::Match)));
}

fn spawn(mut commands: Commands) {
    // Own UI camera above the HUD (5) and settings menu (10).
    let cam = commands.spawn((Camera2d, Camera { order: 20, clear_color: ClearColorConfig::None, ..default() }, crate::SleepWhenEmpty)).id();
    commands.spawn((
        OverlayText,
        UiTargetCamera(cam),
        Visibility::Hidden,
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(Color::srgb(0.85, 1.0, 0.9)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.82)),
        // Below P1's health/aura panel in the top-left corner.
        Node { position_type: PositionType::Absolute, left: px(8), top: percent(20), padding: UiRect::all(px(8)), ..default() },
    ));
    let note = commands
        .spawn((
            NoteBox,
            GlobalZIndex(50),
            UiTargetCamera(cam),
            Visibility::Hidden,
            BackgroundColor(Color::srgba(0.02, 0.06, 0.06, 0.95)),
            BorderColor::all(Color::srgb(0.45, 0.85, 0.72)),
            Node {
                position_type: PositionType::Absolute,
                left: percent(20),
                top: percent(30),
                width: percent(60),
                min_height: px(160),
                padding: UiRect::all(px(16)),
                border: UiRect::all(px(2)),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
        ))
        .id();
    let title = commands
        .spawn((Text::new("BUG REPORT - describe what went wrong (Enter saves, Esc cancels)"), TextFont { font_size: FontSize::Px(18.0), ..default() }, TextColor(Color::srgb(0.45, 0.85, 0.72))))
        .id();
    let body = commands.spawn((NoteText, Text::new("_"), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::WHITE))).id();
    commands.entity(note).add_children(&[title, body]);
}

fn hotkeys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut dev: ResMut<DevTools>,
    world: Res<World>,
    pending: Res<PendingInputs>,
    setup: Res<MatchSetup>,
    state: Res<State<AppState>>,
    cfg: Res<crate::Config>,
    time: Res<Time<Real>>,
    mut auto_done: Local<bool>,
) {
    let dt = time.delta_secs();
    if dt > 0.0 {
        dev.fps = if dev.fps == 0.0 { 1.0 / dt } else { dev.fps * 0.95 + 0.05 / dt };
    }
    if dev.report.is_some() {
        return;
    }
    if keys.just_pressed(KeyCode::F1) {
        dev.overlay = !dev.overlay;
    }
    if keys.just_pressed(KeyCode::F2) {
        dev.paused = !dev.paused;
        dev.steps = 0;
    }
    if keys.just_pressed(KeyCode::F3) && dev.paused {
        dev.steps += 1;
    }
    // PD_DEV_TEST_REPORT=<tick>: file a report automatically at that match
    // tick (scripted test of the F8 path; the note is filled in for you).
    let auto = std::env::var("PD_DEV_TEST_REPORT").ok().and_then(|v| v.parse::<u64>().ok());
    let auto_now = auto.is_some_and(|t| *state.get() == AppState::Match && world.0.tick >= t && !*auto_done);
    if keys.just_pressed(KeyCode::F8) || auto_now {
        *auto_done = true;
        let root = cfg.cache.parent().map(|p| p.join("bug_reports")).unwrap_or_else(|| PathBuf::from("bug_reports"));
        let dir = root.join(timestamp());
        if let Err(e) = std::fs::create_dir_all(&dir) {
            toast(&mut commands, &format!("Bug report failed: {e}"));
            return;
        }
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(dir.join("screenshot.png")));
        let state_txt = dump_state(&world, &pending, &setup, &cfg, *state.get(), dev.paused);
        let inputs = dump_inputs(&dev.history);
        dev.report = Some(Report { dir, state: state_txt, inputs, note: String::new(), delay: NOTE_BOX_DELAY_FRAMES, open_frames: 0 });
    }
}

fn type_note(mut commands: Commands, mut events: MessageReader<KeyboardInput>, mut dev: ResMut<DevTools>) {
    let Some(report) = dev.report.as_mut() else {
        events.clear();
        return;
    };
    if report.delay > 0 {
        // Keystrokes before the box is visible (the F8 press itself) are dropped.
        report.delay -= 1;
        events.clear();
        return;
    }
    let mut done = None;
    report.open_frames += 1;
    if std::env::var_os("PD_DEV_TEST_REPORT").is_some() && report.open_frames >= 240 {
        report.note = "automated test report (PD_DEV_TEST_REPORT)".into();
        done = Some(true);
    }
    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Enter => done = Some(true),
            Key::Escape => done = Some(false),
            Key::Backspace => {
                report.note.pop();
            }
            _ => {
                if let Some(t) = &ev.text {
                    report.note.extend(t.chars().filter(|c| !c.is_control()));
                }
            }
        }
    }
    match done {
        Some(true) => {
            let r = dev.report.take().unwrap();
            let note = if r.note.trim().is_empty() { "(no note)".to_string() } else { r.note.clone() };
            let ok = std::fs::write(r.dir.join("note.txt"), format!("{note}\n"))
                .and_then(|_| std::fs::write(r.dir.join("state.txt"), &r.state))
                .and_then(|_| std::fs::write(r.dir.join("inputs.csv"), &r.inputs));
            match ok {
                Ok(()) => toast(&mut commands, &format!("Bug report saved: {}", r.dir.display())),
                Err(e) => toast(&mut commands, &format!("Bug report failed: {e}")),
            }
        }
        Some(false) => {
            let r = dev.report.take().unwrap();
            let _ = std::fs::remove_dir_all(&r.dir);
            toast(&mut commands, "Bug report cancelled");
        }
        None => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn show_overlay(dev: Res<DevTools>, world: Res<World>, pending: Res<PendingInputs>, setup: Res<MatchSetup>, cfg: Res<crate::Config>, state: Res<State<AppState>>, session: Option<Res<crate::net::NetSession>>, gate: Res<crate::online_match::OnlineGate>, mut q: Query<(&mut Text, &mut Visibility), With<OverlayText>>) {
    let Ok((mut text, mut vis)) = q.single_mut() else { return };
    // Hidden behind an open note box (UI text would draw through it).
    let show = (dev.overlay || dev.paused) && dev.report.as_ref().is_none_or(|r| r.delay > 0);
    *vis = if show { Visibility::Visible } else { Visibility::Hidden };
    if !show {
        return;
    }
    let mut s = format!("DEV TOOLS   F1 overlay | F2 pause | F3 step | F8 bug report\nFPS {:.0}   screen {:?}", dev.fps, state.get());
    if dev.paused {
        s.push_str("   ** PAUSED (F3 = one tick) **");
    }
    if let Some(n) = &session {
        s.push_str(&format!(
            "\nONLINE {:?} slot {}  {:?}  ping {}  input delay {} ticks  waiting {}",
            n.role,
            n.my_slot() + 1,
            n.status,
            n.ping_ms().map_or("-".into(), |p| format!("{p} ms")),
            crate::net::INPUT_DELAY,
            gate.waiting
        ));
    }
    if *state.get() == AppState::Match {
        s.push('\n');
        let (stage, chars) = names(&cfg, &setup);
        s.push_str(&overlay_match(&world, &pending, stage, chars));
    }
    text.0 = s;
}

/// The stage and characters actually playing: the command line's in a
/// legacy (menu-skipping) run, else the versus setup's.
fn names<'a>(cfg: &'a crate::Config, setup: &'a MatchSetup) -> (&'a str, &'a [String; 2]) {
    if cfg.legacy { (&cfg.stage, &cfg.chars) } else { (&setup.stage, &setup.chars) }
}

/// Seats 3-4 have no name here (`Config`/`MatchSetup` only hold two).
fn char_name(chars: &[String; 2], i: usize) -> &str {
    chars.get(i).map_or("", |c| c.as_str())
}

fn overlay_match(world: &World, pending: &PendingInputs, stage: &str, chars: &[String; 2]) -> String {
    let m = &world.0;
    let c = &m.combat;
    let mut s = format!(
        "stage {}   tick {} ({:.1}s)   round {}  wins {:?}   round tick {}   players {}",
        stage,
        m.tick,
        m.tick as f64 / HZ,
        c.round,
        c.roster.slots().map(|i| c.wins[i]).collect::<Vec<_>>(),
        c.round_ticks,
        c.roster.count()
    );
    for i in c.roster.slots() {
        let p = &m.players[i];
        let f = &c.fighters[i];
        let inp = pending.0[i];
        let slots: Vec<String> = f.slots.iter().map(|s| s.map_or("-".into(), |id| id.to_string())).collect();
        s.push_str(&format!(
            "\n\nP{} {} team {}  pos ({:.1}, {:.1}, {:.1})  vel ({:.1}, {:.1}, {:.1})  yaw {:.0} deg\n   {:?} {:?}  grounded {}  locked {}\n   health {}  level {}  aura {}  action {:?} ({})  slots [{}]\n   input stick {:?} jump {} lock {} skills {:04b}",
            i + 1,
            char_name(chars, i),
            c.roster.team[i],
            p.pos.x,
            p.pos.y,
            p.pos.z,
            p.vel.x,
            p.vel.y,
            p.vel.z,
            p.yaw.to_degrees(),
            p.motion,
            p.gait,
            p.grounded,
            p.locked,
            f.health,
            f.level,
            f.aura(),
            f.action,
            f.action_ticks,
            slots.join(" "),
            inp.stick,
            inp.jump,
            inp.lock,
            inp.skills
        ));
    }
    s.push_str(&format!("\n\ncapsules {}  projectiles {}  areas {}  props {}", c.capsules.len(), c.projectiles.len(), c.areas.len(), c.props.len()));
    for (seq, ev) in c.events.iter().rev().take(OVERLAY_EVENTS) {
        s.push_str(&format!("\n  #{seq} {ev:?}"));
    }
    s
}

fn show_note(dev: Res<DevTools>, mut boxes: Query<&mut Visibility, With<NoteBox>>, mut texts: Query<&mut Text, With<NoteText>>) {
    let Ok(mut vis) = boxes.single_mut() else { return };
    let open = dev.report.as_ref().filter(|r| r.delay == 0);
    *vis = if open.is_some() { Visibility::Visible } else { Visibility::Hidden };
    if let (Some(r), Ok(mut t)) = (open, texts.single_mut()) {
        t.0 = format!("{}_", r.note);
    }
}

fn record_inputs(mut dev: ResMut<DevTools>, world: Res<World>, pending: Res<PendingInputs>) {
    let tick = world.0.tick;
    if dev.history.back().is_some_and(|(t, _)| *t == tick) {
        return;
    }
    dev.history.push_back((tick, pending.0));
    while dev.history.len() > INPUT_HISTORY_TICKS {
        dev.history.pop_front();
    }
}

fn toast(commands: &mut Commands, msg: &str) {
    info!("{msg}");
    commands.spawn((
        Toast(4.0),
        Text::new(msg.to_string()),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(Color::srgb(1.0, 0.95, 0.6)),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
        GlobalZIndex(100),
        Node { position_type: PositionType::Absolute, right: px(12), bottom: px(12), padding: UiRect::all(px(8)), ..default() },
    ));
}

fn fade_toasts(mut commands: Commands, time: Res<Time<Real>>, mut q: Query<(Entity, &mut Toast)>) {
    for (e, mut t) in &mut q {
        t.0 -= time.delta_secs();
        if t.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

fn dump_state(world: &World, pending: &PendingInputs, setup: &MatchSetup, cfg: &crate::Config, state: AppState, paused: bool) -> String {
    let m = &world.0;
    let mut s = format!("Phantom Dust rebuild bug report\nbuild: pd_game {}\nscreen: {state:?}  dev-paused: {paused}\n\n", env!("CARGO_PKG_VERSION"));
    let (stage, chars) = names(cfg, setup);
    s.push_str(&format!("stage {stage}  characters {chars:?}  menus skipped (command line): {}\n\nmatch setup:\n{setup:#?}\n\n", cfg.legacy));
    if state == AppState::Match {
        s.push_str(&format!("tick {}  round {}  wins {:?}  round_ticks {}  round_over {:?}  time_up {}\n", m.tick, m.combat.round, m.combat.wins, m.combat.round_ticks, m.combat.round_over, m.combat.time_up));
        s.push_str(&format!("inputs this tick: {:?}\n\n", pending.0));
        s.push_str(&format!("roster: {:?}\n\n", m.combat.roster));
        for i in m.combat.roster.slots() {
            s.push_str(&format!("=== P{} player ===\n{:#?}\n\n=== P{} fighter ===\n{:#?}\n\n", i + 1, m.players[i], i + 1, m.combat.fighters[i]));
        }
        s.push_str(&format!("=== capsules ===\n{:#?}\n\n=== projectiles ===\n{:#?}\n\n=== areas ===\n{:#?}\n\n", m.combat.capsules, m.combat.projectiles, m.combat.areas));
        s.push_str(&format!("=== crystal ===\n{:#?}\n\n=== recent events ===\n{:#?}\n", m.combat.crystal, m.combat.events));
    }
    s
}

fn dump_inputs(history: &VecDeque<(u64, [Input; MAX_PLAYERS])>) -> String {
    let mut s = String::from("tick");
    for n in 1..=MAX_PLAYERS {
        s.push_str(&format!(",p{n}_stick_x,p{n}_stick_y,p{n}_jump,p{n}_lock,p{n}_skills,p{n}_cam,p{n}_switch_target"));
    }
    s.push('\n');
    for (t, ins) in history {
        s.push_str(&t.to_string());
        for i in ins {
            s.push_str(&format!(",{},{},{},{},{},{},{}", i.stick[0], i.stick[1], i.jump as u8, i.lock as u8, i.skills, i.cam, i.switch_target as u8));
        }
        s.push('\n');
    }
    s
}

/// Local-free UTC `YYYY-MM-DD_HH-MM-SS` (no date crate in the workspace).
fn timestamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (mo <= 2) as i64;
    format!("{y:04}-{mo:02}-{d:02}_{:02}-{:02}-{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_steps_one_tick_at_a_time() {
        let mut dev = DevTools::default();
        assert!(dev.take_tick());
        dev.paused = true;
        assert!(!dev.sim_runs());
        assert!(!dev.take_tick());
        dev.steps = 2;
        // The CPU seats ask first without using the step up.
        assert!(dev.sim_runs() && dev.sim_runs());
        assert!(dev.take_tick());
        assert!(dev.take_tick());
        assert!(!dev.sim_runs());
        assert!(!dev.take_tick());
    }

    #[test]
    fn timestamp_shape() {
        let t = timestamp();
        assert_eq!(t.len(), 19, "{t}");
        assert_eq!(&t[10..11], "_");
    }
}
