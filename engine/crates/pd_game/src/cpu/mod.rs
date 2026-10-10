//! The computer opponent: the original's own quest AI (`com\ai\eneprog006.ssb`, a bytecode script) run on our VM
//! (`pd_formats::ssb_vm`) at 30 Hz against the real sim, steering on the stage's `com\stNN\comdata*.cad` nav graph and writing
//! the same `Input` a human pad produces. Byte-level spec: docs/FORMATS-CPU.md; what it does: docs/audits/P0-AI.md.
//!
//! Files: `host.rs` (the per-seat tick protocol), `natives.rs` (the 75 natives `eneprog006` calls), `nav.rs` (`.cad` + steering),
//! `threat.rs` (bullet threat predictor), `world.rs` (what the natives read of the sim).
//!
//! A CPU seat = `seats::SeatKind::Cpu { epa_row }`; `--cpu 2:<row>` (seat number, difficulty row) sets one for scripted runs.
//!
//! Data: `cache/com/ai/eneprog006.ssb`, `cache/com/param/enemyparam.epa`, `cache/com/stNN/comdata<v>.cad` (written by
//! `pd_convert cpu <game data dir> <cache>`); when the cache has none, the game's own `Assets/Data/com` next to the cache is read.

pub mod host;
pub mod nav;
pub mod natives;
pub mod threat;
pub mod world;

use crate::seats::{SeatKind, SeatKinds, MAX_SEATS};
use bevy::prelude::*;
use host::{CpuData, CpuHost};
use nav::Cad;
use std::path::{Path, PathBuf};

pub const SCRIPT: &str = "eneprog006.ssb";

/// The program's stage code of one of our arenas: `stNN` -> NN (the cache names the arenas by the program's stage numbers).
pub fn stage_code(stage: &str) -> i32 {
    stage.trim_start_matches("st").parse().unwrap_or(-1)
}

/// Directories to look for `com\...` in, in order: `<cache>/com`, `$PD_COM_DIR`, and the game's data next to the cache
/// (`<cache>/../../game_files/Assets/Data/com`, the layout of this repository).
fn com_dirs(cache: &Path) -> Vec<PathBuf> {
    let mut v = vec![cache.join("com")];
    if let Some(d) = std::env::var_os("PD_COM_DIR") {
        v.push(PathBuf::from(d));
    }
    if let Some(root) = cache.parent().and_then(|p| p.parent()) {
        v.push(root.join("game_files").join("Assets").join("Data").join("com"));
    }
    v
}

fn read_first(cache: &Path, rel: &str) -> Option<Vec<u8>> {
    com_dirs(cache).iter().find_map(|d| std::fs::read(d.join(rel)).ok())
}

/// The first existing `com\<rel>` (`param/enemyparam.epa`, `deck/DECK_PSYCHO`, ...) among the same places `load_data` looks.
/// Public for the front end (lane `cpuseat`: the CPU seat's grades and decks are read from the original's files).
pub fn com_file(cache: &Path, rel: &str) -> Option<Vec<u8>> {
    read_first(cache, rel)
}

/// Whether `stage` has a nav graph (`load_cad` would find a file): CPU seats stand still on a stage without one. Existence only
/// (no parse), for the setup screen's per-stage warning.
pub fn stage_has_nav(cache: &Path, stage: &str) -> bool {
    let code = stage_code(stage);
    code >= 0 && (0..3).any(|v| com_dirs(cache).iter().any(|d| d.join(format!("st{:02}/comdata{v}.cad", code % 10)).is_file()))
}

/// `eneprog006.ssb` and `enemyparam.epa`.
pub fn load_data(cache: &Path) -> Result<CpuData, String> {
    let ssb = pd_formats::ssb::Ssb::parse(&read_first(cache, &format!("ai/{SCRIPT}")).ok_or("CPU script ai/eneprog006.ssb not found (run `pd_convert cpu`)")?).map_err(|e| e.to_string())?;
    let epa = pd_formats::epa::EnemyParams::parse(&read_first(cache, "param/enemyparam.epa").ok_or("param/enemyparam.epa not found (run `pd_convert cpu`)")?).map_err(|e| e.to_string())?;
    Ok(CpuData { ssb, epa })
}

/// The stage's nav graph: `com\st(code % 10)\comdata<version>.cad`, version 0 first (FORMATS-CPU 5.1: only stage codes 2..8 have
/// one; any other stage leaves the CPU seats inert, as in the program). Mirrored into our space.
pub fn load_cad(cache: &Path, stage: &str) -> Option<Cad> {
    let code = stage_code(stage);
    if code < 0 {
        return None;
    }
    for v in 0..3 {
        if let Some(bytes) = read_first(cache, &format!("st{:02}/comdata{v}.cad", code % 10)) {
            return Cad::parse(&bytes, true).ok();
        }
    }
    None
}

/// `--cpu 2:20,3:5` -> seat 2 (slot 1) is a CPU on difficulty row 20, seat 3 on row 5.
pub fn parse_cli(spec: Option<&str>) -> SeatKinds {
    let mut kinds = SeatKinds::default();
    for part in spec.unwrap_or("").split(',').filter(|s| !s.is_empty()) {
        let mut it = part.splitn(2, ':');
        let (Some(seat), row) = (it.next().and_then(|s| s.trim().parse::<usize>().ok()), it.next().and_then(|r| r.trim().parse::<u8>().ok())) else { continue };
        if (1..=MAX_SEATS).contains(&seat) {
            kinds.0[seat - 1] = SeatKind::Cpu { epa_row: row.unwrap_or(0) };
        }
    }
    kinds
}

/// The running host and what it was built for.
#[derive(Resource, Default)]
pub struct CpuRuntime {
    host: Option<CpuHost>,
    log_file: Option<PathBuf>,
    announced: bool,
}

/// Builds a host for the seats of `kinds` on the match `m` (stage `stage`), or `None` when there is no CPU seat or its data is missing.
pub fn build_host(cache: &Path, stage: &str, kinds: &SeatKinds, m: &crate::sim::Match, col: &pd_formats::collision::Collision) -> Option<CpuHost> {
    if !kinds.any_cpu() {
        return None;
    }
    let data = match load_data(cache) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[cpu] {e}");
            return None;
        }
    };
    let cad = load_cad(cache, stage);
    if cad.is_none() {
        eprintln!("[cpu] no nav graph for stage {stage}: CPU seats stand still (FORMATS-CPU 5.1)");
    }
    let mut host = CpuHost::new(data, cad, m, col, stage_code(stage));
    for (slot, k) in kinds.0.iter().enumerate() {
        if let SeatKind::Cpu { epa_row } = *k {
            if m.combat.roster.is_active(slot) {
                host.add_seat(m, slot, epa_row);
            }
        }
    }
    host.log_lines = std::env::var_os("PD_CPU_LOG").is_some();
    Some(host)
}

/// FixedUpdate, right before `step_sim`: runs the CPU seats and writes their entries of `PendingInputs`.
#[allow(clippy::too_many_arguments)]
pub fn drive_system(
    mut rt: ResMut<CpuRuntime>,
    kinds: Res<SeatKinds>,
    cfg: Res<crate::Config>,
    setup: Res<crate::frontend::state::MatchSetup>,
    mut world: ResMut<crate::World>,
    col: Res<crate::StageCollision>,
    mut pending: ResMut<crate::PendingInputs>,
    menu: Res<crate::menu::MenuState>,
    seats: Res<crate::seats::Seats>,
    dev: Res<crate::devtools::DevTools>,
) {
    if !kinds.any_cpu() || menu.open || seats.paused(world.0.combat.roster.count()) || !dev.sim_runs() {
        return;
    }
    // A new match (tick 0): rebuild the host for it.
    if world.0.tick == 0 || (rt.host.is_none() && !rt.announced) {
        let stage = if cfg.legacy { cfg.stage.clone() } else { setup.stage.clone() };
        rt.host = build_host(&cfg.cache, &stage, &kinds, &world.0, &col.0);
        rt.announced = true;
        rt.log_file = std::env::var("PD_CPU_LOG").ok().filter(|v| v != "1").map(PathBuf::from);
        if let Some(f) = &rt.log_file {
            let _ = std::fs::write(f, "");
        }
    }
    let log_file = rt.log_file.clone();
    let Some(host) = rt.host.as_mut() else { return };
    host.drive(&mut world.0, &col.0, &mut pending.0);
    for line in host.lines.drain(..) {
        match &log_file {
            Some(f) => {
                use std::io::Write;
                if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(f) {
                    let _ = writeln!(fh, "{line}");
                }
            }
            None => eprintln!("{line}"),
        }
    }
}

#[cfg(test)]
mod tests;
