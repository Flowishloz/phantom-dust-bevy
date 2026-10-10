//! Golden-trace output (`docs/GOLDEN-TRACE.md` section 5): P1's movement as
//! 30 Hz CSV rows, plus an optional per-tick input script, so our sim can be
//! compared number-for-number with the original's recorded trace
//! (`re/trace_compare.py`). Debug-only (`PD_TRACE*` env vars); reads sim
//! state and never writes it.

use crate::sim::{Match, Motion, StrafeDir, HZ};
use std::io::Write;

pub const HEADER: &str = "tick,scenario_id,stick_x,stick_y,buttons,pos_x,pos_y,pos_z,facing,heading,vertical_speed,state,clip_id,clip_time,lock_active,lock_band,speed_status,grounded";

/// One row of an input script: `tick,stick_x,stick_y,buttons`
/// (buttons: 1 jump, 2 lock press, 4 ground dodge; 1 and 4 both press our
/// single jump/dodge button, which one it does depends on the character).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptRow {
    pub tick: u64,
    pub x: f32,
    pub y: f32,
    pub buttons: u8,
}

pub fn parse_script(text: &str) -> Vec<ScriptRow> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            Some(ScriptRow { tick: f.first()?.parse().ok()?, x: f.get(1)?.parse().ok()?, y: f.get(2)?.parse().ok()?, buttons: f.get(3).and_then(|b| b.parse().ok()).unwrap_or(0) })
        })
        .collect()
}

/// The row for `tick`, if the script has one (the header line parses to
/// nothing and is skipped).
pub fn script_at(rows: &[ScriptRow], tick: u64) -> Option<ScriptRow> {
    rows.iter().find(|r| r.tick == tick).copied()
}

/// Our motion mapped to the original's controller-state ints: 0 idle,
/// 1 walk/run, 8 strafe, 4 anything else busy (air, landing, dodge, cast).
pub fn state_int(m: Motion) -> u8 {
    match m {
        Motion::Idle => 0,
        Motion::Walk | Motion::Run => 1,
        Motion::Strafe(_) => 8,
        _ => 4,
    }
}

/// The trace's lock_band: 0 none, 1 forward, 2/3 strafe right/left,
/// 4/5 back-right/back-left.
pub fn band_int(locked: bool, m: Motion) -> u8 {
    match m {
        _ if !locked => 0,
        Motion::Strafe(StrafeDir::Right) => 2,
        Motion::Strafe(StrafeDir::Left) => 3,
        Motion::Strafe(StrafeDir::BackRight) => 4,
        Motion::Strafe(StrafeDir::BackLeft) => 5,
        Motion::Walk | Motion::Run => 1,
        _ => 0,
    }
}

#[derive(Default)]
pub struct TraceState {
    started: bool,
    file: Option<std::io::BufWriter<std::fs::File>>,
    start_tick: u64,
    last_clip: String,
    clip_t: u64,
}

/// Append a row if this sim tick lands on the 30 Hz grid. Call after the
/// step, so the position is where the player ended the tick. `stick` is the
/// raw pre-quantise push (x right, y forward).
pub fn write_row(st: &mut TraceState, world: &Match, stick: [f32; 2], buttons: u8, clip: &str) {
    if !st.started {
        st.started = true;
        let Ok(path) = std::env::var("PD_TRACE") else { return };
        let Ok(f) = std::fs::File::create(&path) else {
            eprintln!("[trace] cannot create {path}");
            return;
        };
        let mut f = std::io::BufWriter::new(f);
        let _ = writeln!(f, "# format: golden-trace v1\n# source: pd_game sim\n# hz: 30\n# stick: raw axes before i8 quantise, +y forward, +x right\n# state: 0 idle, 1 walk/run, 8 strafe, 4 other busy\n# heading: direction of horizontal velocity, blank when still\n# speed_status: blank (not exposed to the trace)\n{HEADER}");
        st.file = Some(f);
        st.start_tick = world.tick;
    }
    let Some(f) = st.file.as_mut() else { return };
    let rel = world.tick.saturating_sub(st.start_tick);
    let every = (HZ / 30.0).round() as u64;
    if rel % every != 0 {
        return;
    }
    let p = &world.players[0];
    if clip != st.last_clip {
        st.last_clip = clip.to_string();
        st.clip_t = 0;
    } else {
        st.clip_t += 1;
    }
    let heading = if p.vel.x.hypot(p.vel.z) > 0.01 { format!("{:.2}", p.vel.x.atan2(p.vel.z).to_degrees()) } else { String::new() };
    let id = std::env::var("PD_TRACE_ID").unwrap_or_else(|_| "live".into());
    let _ = writeln!(
        f,
        "{},{},{:.4},{:.4},{},{:.4},{:.4},{:.4},{:.2},{},{:.3},{},{},{},{},{},,{}",
        rel / every,
        id,
        stick[0],
        stick[1],
        buttons,
        p.pos.x,
        p.pos.y,
        p.pos.z,
        p.yaw.to_degrees(),
        heading,
        p.vel.y,
        state_int(p.motion),
        clip,
        st.clip_t / every,
        p.locked as u8,
        band_int(p.locked, p.motion),
        p.grounded as u8
    );
    let _ = f.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_parses_and_skips_header() {
        let rows = parse_script("tick,stick_x,stick_y,buttons\n0,0,0,2\n15,0.0,0.5,0\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], ScriptRow { tick: 0, x: 0.0, y: 0.0, buttons: 2 });
        assert_eq!(script_at(&rows, 15).unwrap().y, 0.5);
        assert!(script_at(&rows, 7).is_none());
    }

    #[test]
    fn state_and_band_maps() {
        assert_eq!(state_int(Motion::Idle), 0);
        assert_eq!(state_int(Motion::Run), 1);
        assert_eq!(state_int(Motion::Strafe(StrafeDir::Left)), 8);
        assert_eq!(state_int(Motion::Rise), 4);
        assert_eq!(band_int(true, Motion::Strafe(StrafeDir::BackLeft)), 5);
        assert_eq!(band_int(false, Motion::Strafe(StrafeDir::Right)), 0);
        assert_eq!(band_int(true, Motion::Run), 1);
    }

    #[test]
    fn columns_match_header() {
        assert_eq!(HEADER.split(',').count(), 18);
    }
}
