//! An online match (docs/ONLINE-PLAN.md wave 2): ties the `net::NetSession`
//! the Online screens (or `--online-host` / `--online-join`) opened to the
//! fixed-step sim.
//!
//! Per sim tick (FixedUpdate, right before `step_sim`): this PC's input goes
//! to the session for `tick + INPUT_DELAY`; the sim steps tick `t` only when
//! both players' inputs for `t` are here (`OnlineGate::ready`), and then runs
//! on exactly those inputs on both PCs. Every `CHECKSUM_EVERY` ticks the two
//! compare `Match::checksum()`. A dropped connection or a checksum mismatch
//! ends the match with the reason on the Online screen (a mismatch also
//! writes a bug report, `devtools::write_auto_report`).
//!
//! Render-only: reads sim state, chooses which inputs a tick gets and
//! whether it runs; never writes sim state.

use bevy::prelude::*;

use crate::frontend::state::{AppState, MatchSetup};
use crate::net::{self, LobbyEvent, NetSession, Status, CHECKSUM_EVERY};
use crate::seats::{LocalViews, SeatKind, SeatKinds};
use crate::sim::Input;
use crate::{PendingInputs, World};

#[derive(Resource, Default)]
pub struct OnlineGate {
    /// A session drives this match.
    pub active: bool,
    /// Both inputs for the current tick are in: `step_sim` may run.
    pub ready: bool,
    /// This PC's input, captured after `gather_input` (before the lockstep
    /// overwrites `PendingInputs` with the agreed pair).
    pub local: Input,
    /// The tick the lockstep prepared (to tell whether `step_sim` ran).
    prepared: Option<u64>,
    /// Fixed steps spent waiting for the other PC since the last real tick.
    pub waiting: u32,
    /// The last match was online: restore local-play views/seats when its
    /// result screen closes.
    was_online: bool,
}

impl OnlineGate {
    /// The match just played (now on its result screen) was online.
    pub fn was_online(&self) -> bool {
        self.was_online
    }
}

/// Why the online match ended early, carried to the Online screen.
#[derive(Resource, Default)]
pub struct OnlineEnd(pub Option<String>);

pub fn plugin(app: &mut App) {
    app.init_resource::<OnlineGate>()
        .init_resource::<OnlineEnd>()
        .add_systems(Startup, cli_session)
        .add_systems(OnEnter(AppState::Match), begin)
        .add_systems(Update, (capture_local.after(crate::gather_input), watch).run_if(in_state(AppState::Match)))
        .add_systems(OnEnter(AppState::Result), finish)
        .add_systems(OnExit(AppState::Result), restore_local_play)
        .add_systems(OnEnter(AppState::Title), drop_stale_session)
        .add_systems(OnEnter(AppState::Online), restore_local_play);
}

/// `--online-host` / `--online-join <ip[:port]>` (test hook, skips the menus
/// like the other match options): open the session at startup and play the
/// command line's match (stage, `--p1`/`--p2`) with this PC on its slot.
/// Both PCs must use the same options. Waits for the other PC at tick 3.
fn cli_session(mut commands: Commands, cfg: Res<crate::Config>, mut views: ResMut<LocalViews>, mut kinds: ResMut<SeatKinds>) {
    let args: Vec<String> = std::env::args().collect();
    let opened = if args.iter().any(|a| a == "--online-host") {
        let port = args.iter().position(|a| a == "--port").and_then(|i| args.get(i + 1)).and_then(|p| p.parse().ok()).unwrap_or(net::DEFAULT_PORT);
        Some(net::open_lan_host(port, &cfg.cache))
    } else if let Some(addr) = args.iter().position(|a| a == "--online-join").and_then(|i| args.get(i + 1)) {
        Some(net::open_lan_join(addr, &cfg.cache))
    } else {
        None
    };
    match opened {
        Some(Ok(mut s)) => {
            // `--mode N|id` picks the gamemode on this hook's legacy match: fold it into the handshake so two copies
            // started with different modes refuse to connect instead of desyncing a second later.
            let mode = args.iter().position(|a| a == "--mode").and_then(|i| args.get(i + 1)).and_then(|k| crate::frontend::modes::find(k));
            let mode_idx = mode.and_then(|m| crate::frontend::modes::by_id(m.id));
            s.mix_setup(crate::frontend::modes::rules_fingerprint(mode_idx));
            let me = s.my_slot();
            info!("online: {:?} on slot {me}, waiting for the other PC", s.role);
            views.0 = Some(vec![me]);
            kinds.0[1 - me] = SeatKind::Remote;
            commands.insert_resource(s);
        }
        Some(Err(e)) => error!("online: could not open the session: {e}"),
        None => {}
    }
}

fn begin(session: Option<Res<NetSession>>, mut gate: ResMut<OnlineGate>) {
    *gate = OnlineGate { active: session.is_some(), was_online: session.is_some(), ..default() };
}

/// This PC's own controls: they land in slot 0 (keyboard P1 / first pad).
/// `--demo` writes each slot's own scripted input, so take ours from there.
fn capture_local(session: Option<Res<NetSession>>, cfg: Res<crate::Config>, dev: Res<crate::devtools::DevTools>, pending: Res<PendingInputs>, mut gate: ResMut<OnlineGate>) {
    if let Some(s) = session {
        // While the F8 note box is open `gather_input` doesn't run, so
        // `PendingInputs` still holds the last agreed pair: send neutral.
        gate.local = if dev.typing() { Input::default() } else { pending.0[if cfg.demo { s.my_slot() } else { 0 }] };
    }
}

/// FixedUpdate, before `step_sim`: send ours, and if both inputs for this
/// tick are here, put them in `PendingInputs` and open the gate.
pub fn lockstep(session: Option<ResMut<NetSession>>, world: Res<World>, mut pending: ResMut<PendingInputs>, mut gate: ResMut<OnlineGate>) {
    let Some(mut s) = session else {
        gate.active = false;
        gate.ready = true;
        return;
    };
    gate.active = true;
    if matches!(s.status, Status::Closed(_)) {
        gate.ready = false;
        return;
    }
    // Read what arrived since the frame started (a lobby event here can only
    // be a close, which `watch` sees through `status`).
    s.poll();
    let t = world.0.tick;
    let local = gate.local;
    s.push_local_input(t, local);
    match s.inputs_for(t) {
        Some(both) => {
            pending.0 = [Input::default(); crate::combat::MAX_PLAYERS];
            pending.0[..2].copy_from_slice(&both);
            gate.ready = true;
            gate.prepared = Some(t);
        }
        None => {
            gate.ready = false;
            gate.waiting += 1;
        }
    }
}

/// FixedUpdate, after `step_sim`: if the prepared tick ran, drop old inputs
/// and exchange the checksum on its beat.
pub fn after_step(session: Option<ResMut<NetSession>>, world: Res<World>, mut gate: ResMut<OnlineGate>) {
    let Some(mut s) = session else { return };
    let Some(t) = gate.prepared else { return };
    if world.0.tick <= t {
        return; // paused (dev tools / pad drop): the same tick runs again
    }
    gate.prepared = None;
    gate.ready = false;
    gate.waiting = 0;
    s.forget_before(t);
    let now = world.0.tick;
    if now % CHECKSUM_EVERY == 0 {
        s.report_checksum(now, world.0.checksum());
    }
}

/// Every frame: keep the connection alive and end the match on a close or
/// an out-of-sync checksum.
#[allow(clippy::too_many_arguments)]
fn watch(
    mut session: Option<ResMut<NetSession>>,
    mut end: ResMut<OnlineEnd>,
    mut next: ResMut<NextState<AppState>>,
    dev: Res<crate::devtools::DevTools>,
    cfg: Res<crate::Config>,
    world: Res<World>,
    pending: Res<PendingInputs>,
    setup: Res<MatchSetup>,
) {
    let Some(s) = session.as_mut() else { return };
    let mut reason = None;
    for ev in s.poll() {
        if let LobbyEvent::Closed(r) = ev {
            reason = Some(r);
        }
    }
    if let (None, Status::Closed(r)) = (&reason, &s.status) {
        reason = Some(r.clone());
    }
    if let Some(t) = s.desync_tick {
        let note = format!("Online match went out of sync at tick {t} (checksum mismatch). Role {:?}. Send this report and the other player's.", s.role);
        let saved = crate::devtools::write_auto_report(&dev, &cfg, &world, &pending, &setup, &note);
        reason = Some(match saved {
            Some(dir) => format!("Out of sync at tick {t}. Bug report saved: {}", dir.display()),
            None => format!("Out of sync at tick {t}."),
        });
        s.leave();
    }
    if let Some(r) = reason {
        warn!("online: match ended: {r}");
        end.0 = Some(r);
        next.set(AppState::Online);
    }
}

/// The match reached its result: the session's job is done.
fn finish(mut commands: Commands, session: Option<ResMut<NetSession>>) {
    if let Some(mut s) = session {
        s.leave();
        commands.remove_resource::<NetSession>();
    }
}

/// After an online match, local play gets every view and human seats back.
fn restore_local_play(mut gate: ResMut<OnlineGate>, mut views: ResMut<LocalViews>, mut kinds: ResMut<SeatKinds>) {
    if gate.was_online {
        gate.was_online = false;
        views.0 = None;
        for k in kinds.0.iter_mut() {
            if *k == SeatKind::Remote {
                *k = SeatKind::Human;
            }
        }
    }
}

/// Quit to title from an online match (pause menu): leave the session and
/// give local play its views and seats back.
fn drop_stale_session(mut commands: Commands, session: Option<ResMut<NetSession>>, gate: ResMut<OnlineGate>, views: ResMut<LocalViews>, kinds: ResMut<SeatKinds>) {
    if let Some(mut s) = session {
        s.leave();
        commands.remove_resource::<NetSession>();
    }
    restore_local_play(gate, views, kinds);
}
