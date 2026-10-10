//! Online versus over LAN or Steam (docs/ONLINE-PLAN.md): two PCs, one fighter each,
//! delay-based lockstep.
//!
//! The contract the other lanes build against:
//! - `NetSession` (a Bevy resource while online): `host` / `join` open it,
//!   the Online screens (`frontend/online.rs`, lane B) drive the lobby with
//!   `send` + `poll`, and the match glue (lane E) feeds this machine's input
//!   with `push_local_input` and steps the sim only when `inputs_for(tick)`
//!   has both players' inputs.
//! - `Transport` moves whole `NetMsg`s. `Loopback` (in memory, tests and the
//!   menu lane) works today; `udp::Udp` (lane D) is the real LAN one and owns
//!   the byte encoding.
//!
//! Lockstep: every tick each side sends its input for `tick + INPUT_DELAY`,
//! repeating the last `RESEND` ticks in every packet so a lost packet costs
//! nothing. The sim steps tick `t` once both inputs for `t` are here; the
//! first `INPUT_DELAY` ticks use neutral input on both sides. Nothing is
//! predicted, so nothing is ever rolled back (decision D2). Both sides
//! exchange `Match::checksum()` every `CHECKSUM_EVERY` ticks (D5).
//!
//! Deterministic sim rule: nothing here is read by `sim.rs` / `combat.rs`;
//! the session only decides WHICH inputs a tick gets and WHETHER it runs.

#![allow(dead_code)] // parts of the contract are used by later lanes

pub mod steam;
pub mod udp;
pub mod wire;

use crate::sim::Input;
use bevy::prelude::Resource;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Default LAN port (host listens here; join dials `ip:7777`).
pub const DEFAULT_PORT: u16 = 7777;
/// Ticks between pressing a button and the sim seeing it, on both PCs
/// (3 ticks = 50 ms at 60 Hz). Covers a LAN / nearby internet round trip.
pub const INPUT_DELAY: u64 = 3;
/// Inputs repeated in every `Inputs` message (loss cover).
pub const RESEND: usize = 8;
/// Ticks between checksum exchanges (1 s).
pub const CHECKSUM_EVERY: u64 = 60;
/// Sim slot of each role: the host is P1 (slot 0), the joiner P2 (slot 1).
pub const HOST_SLOT: usize = 0;
pub const CLIENT_SLOT: usize = 1;

/// Liveness (wall clock: this is outside the sim). A connected session sends
/// `Ping` every `PING_EVERY`; `Hello` is repeated every `HELLO_EVERY` until
/// the peer has clearly heard it; silence for `LOST_AFTER` once connected
/// ends the session ("Connection lost"), and a joiner that hears nothing for
/// `JOIN_TIMEOUT` gives up ("Could not reach host"). The host waits for a
/// joiner forever.
pub const PING_EVERY: Duration = Duration::from_millis(250);
pub const HELLO_EVERY: Duration = Duration::from_millis(500);
pub const LOST_AFTER: Duration = Duration::from_secs(5);
pub const JOIN_TIMEOUT: Duration = Duration::from_secs(10);
/// While this side has match inputs, `poll` repeats the newest `RESEND` this
/// often, so a lost packet can't stall lockstep while nobody pushes.
pub const INPUTS_REPEAT: Duration = Duration::from_millis(50);
/// Lobby/match messages kept while still waiting for the peer's Hello.
const MAX_EARLY: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Client,
}

impl Role {
    pub fn slot(self) -> usize {
        match self {
            Role::Host => HOST_SLOT,
            Role::Client => CLIENT_SLOT,
        }
    }
}

/// One player's choices in the lobby.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LobbyPick {
    pub char: String,
    pub colour: u8,
    /// The 30-skill arsenal.
    pub arsenal: Vec<i16>,
    pub ready: bool,
}

/// What the host decides for the match.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LobbyRules {
    pub stage: String,
    /// 0 = no limit.
    pub time_limit_ticks: u32,
    pub rounds_to_win: Option<u32>,
    /// The gamemode (`frontend::modes::MODES` entry `id`; empty = the
    /// normal versus). The host picks it; the joiner shows it read-only.
    pub mode: String,
    /// `modes::rules_fingerprint` of the `Rules` that mode plays under on
    /// the host's PC. The joiner recomputes it from its own table and
    /// refuses to start if it differs (lockstep needs the same `Rules`).
    pub rules_hash: u64,
}

/// Everything that crosses the wire. `Transport`s move these whole; the UDP
/// transport encodes them (lane D).
#[derive(Clone, Debug, PartialEq)]
pub enum NetMsg {
    /// First message each way: the build and a hash of the converted data
    /// both sims read (D4). A mismatch ends the session with a clear reason.
    Hello { build: String, data_hash: u64 },
    /// This side's current pick (sent whenever it changes).
    Pick(LobbyPick),
    /// Host only: the match rules (sent whenever they change).
    Rules(LobbyRules),
    /// Host only: both are ready, start now with these exact picks
    /// (`picks[slot]`) and rules.
    Start { rules: LobbyRules, picks: [LobbyPick; 2] },
    /// Inputs for consecutive ticks starting at `first_tick`.
    Inputs { first_tick: u64, inputs: Vec<[u8; Input::BYTES]> },
    Checksum { tick: u64, hash: u64 },
    /// Keepalive / round-trip probe and its answer (milliseconds since the
    /// sender's session start).
    Ping(u32),
    Pong(u32),
    /// Leaving the session (back to title, window closed).
    Bye,
}

/// A way to move `NetMsg`s to the one other peer.
pub trait Transport: Send + Sync {
    fn send(&mut self, msg: &NetMsg);
    /// Everything that arrived since the last call, oldest first.
    fn recv(&mut self) -> Vec<NetMsg>;
    /// The local port, for transports that have one (UDP).
    fn local_port(&self) -> Option<u16> {
        None
    }
}

/// In-memory transport pair: what one end sends, the other receives.
pub struct Loopback {
    out: Arc<Mutex<VecDeque<NetMsg>>>,
    inbox: Arc<Mutex<VecDeque<NetMsg>>>,
}

impl Loopback {
    pub fn pair() -> (Loopback, Loopback) {
        let a = Arc::new(Mutex::new(VecDeque::new()));
        let b = Arc::new(Mutex::new(VecDeque::new()));
        (Loopback { out: a.clone(), inbox: b.clone() }, Loopback { out: b, inbox: a })
    }
}

impl Transport for Loopback {
    fn send(&mut self, msg: &NetMsg) {
        self.out.lock().unwrap().push_back(msg.clone());
    }
    fn recv(&mut self) -> Vec<NetMsg> {
        self.inbox.lock().unwrap().drain(..).collect()
    }
}

/// Protocol/build string sent in `Hello`: two copies talk only if this
/// matches. Bump `PROTOCOL` whenever `NetMsg` or its encoding changes.
pub const PROTOCOL: u32 = 2;

pub fn build_id() -> String {
    format!("pd_game {} net{}", env!("CARGO_PKG_VERSION"), PROTOCOL)
}

/// Hash of the converted data both sims read (D4), so two PCs with
/// differently converted caches (or different gamemode tables) refuse to play instead of drifting apart.
///
/// FNV-1a 64 (8 bytes per step) over, in sorted name order: `skills.json`, `chars.json`,
/// `stages.json`, every `*.pdcl` and `*.props.json` in the cache root and
/// every `chars/*.rootmotion.json`, each as name + length + bytes (a missing
/// fixed file hashes as "absent"). Small files only (~70 MB, no glb/wav).
pub fn data_hash(cache: &std::path::Path) -> u64 {
    let mut names: Vec<String> = ["skills.json", "chars.json", "stages.json"].map(String::from).to_vec();
    let mut list = |dir: &str, ok: &dyn Fn(&str) -> bool| {
        let Ok(rd) = std::fs::read_dir(cache.join(dir)) else { return };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if ok(&n) && e.file_type().is_ok_and(|t| t.is_file()) {
                names.push(if dir.is_empty() { n } else { format!("{dir}/{n}") });
            }
        }
    };
    list("", &|n| n.ends_with(".pdcl") || n.ends_with(".props.json"));
    list("chars", &|n| n.ends_with(".rootmotion.json"));
    names.sort();
    names.dedup();
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    // The gamemode table is sim data too: the same mode id must mean the same Rules on both PCs.
    h.eat(&crate::frontend::modes::table_hash().to_le_bytes());
    for name in &names {
        h.eat(name.as_bytes());
        h.eat(&[0]);
        match std::fs::read(cache.join(name)) {
            Ok(bytes) => {
                h.eat(&(bytes.len() as u64).to_le_bytes());
                h.eat(&bytes);
            }
            Err(_) => h.eat(b"\xffabsent"),
        }
    }
    h.0
}

/// FNV-1a 64 of a string (the gamemode fingerprints).
pub fn fingerprint(text: &str) -> u64 {
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    h.eat(text.as_bytes());
    h.0
}

struct Fnv(u64);

impl Fnv {
    /// FNV-1a, 8 bytes per step (much faster than per byte on ~70 MB; the
    /// xor-shift keeps high bits flowing down), then the tail per byte.
    fn eat(&mut self, bytes: &[u8]) {
        let mut words = bytes.chunks_exact(8);
        for w in &mut words {
            let x = (self.0 ^ u64::from_le_bytes(w.try_into().unwrap())).wrapping_mul(0x0000_0100_0000_01b3);
            self.0 = x ^ (x >> 29);
        }
        for &b in words.remainder() {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

/// Host a LAN game: listen on `port` (UDP, all interfaces) for one player.
pub fn open_lan_host(port: u16, cache: &std::path::Path) -> std::io::Result<NetSession> {
    let udp = udp::Udp::host(port).map_err(|e| std::io::Error::new(e.kind(), format!("Can't listen on port {port}: {e}")))?;
    Ok(NetSession::new(Role::Host, Box::new(udp), build_id(), data_hash(cache)))
}

/// Join a LAN game at `addr` (`192.168.1.20` or `192.168.1.20:7777`).
pub fn open_lan_join(addr: &str, cache: &std::path::Path) -> std::io::Result<NetSession> {
    let udp = udp::Udp::join(addr)?;
    Ok(NetSession::new(Role::Client, Box::new(udp), build_id(), data_hash(cache)))
}

/// Host a Steam game in lobby `lobby` (just created by `steam`; the
/// friend who joins it becomes the peer). `None` if Steam went away.
pub fn open_steam_host(steam: &steam::SteamNet, lobby: u64, cache: &std::path::Path) -> Option<NetSession> {
    steam::open_session(steam, lobby, Role::Host, cache)
}

/// Join the Steam game in lobby `lobby` (already entered): the lobby owner
/// is the host. `None` if the lobby has no other owner or Steam went away.
pub fn open_steam_join(steam: &steam::SteamNet, lobby: u64, cache: &std::path::Path) -> Option<NetSession> {
    steam::open_session(steam, lobby, Role::Client, cache)
}

/// Where the session is.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Host waiting for someone to join / joiner waiting for the host's Hello.
    Waiting,
    /// Both Hellos exchanged and matching: lobby or match.
    Connected,
    /// Ended; the reason is shown to the player.
    Closed(String),
}

/// Lobby happenings for the Online screens (`poll`).
#[derive(Clone, Debug, PartialEq)]
pub enum LobbyEvent {
    Connected,
    PeerPick(LobbyPick),
    Rules(LobbyRules),
    /// Start the match: `picks[slot]`.
    Start { rules: LobbyRules, picks: [LobbyPick; 2] },
    Closed(String),
}

#[derive(Resource)]
pub struct NetSession {
    pub role: Role,
    transport: Box<dyn Transport>,
    pub status: Status,
    build: String,
    data_hash: u64,
    /// Wall-clock liveness (never read by the sim).
    started: Instant,
    last_recv: Instant,
    last_hello: Option<Instant>,
    last_ping: Option<Instant>,
    last_inputs: Option<Instant>,
    ping_ms: Option<u32>,
    /// The peer has sent something other than Hello since we connected, so
    /// it has heard our Hello and we can stop repeating it.
    peer_confirmed: bool,
    /// Messages that arrived before the peer's Hello (UDP reorders).
    early: Vec<NetMsg>,
    /// Highest tick this side has an input for, and the first tick still
    /// kept (stale or far-future peer inputs are ignored).
    local_hi: Option<u64>,
    kept_from: u64,
    /// Inputs by tick: this machine's and the peer's.
    local: BTreeMap<u64, Input>,
    remote: BTreeMap<u64, Input>,
    /// Checksums by tick, to compare when the peer's arrives.
    local_sums: BTreeMap<u64, u64>,
    remote_sums: BTreeMap<u64, u64>,
    /// First tick whose checksums disagreed.
    pub desync_tick: Option<u64>,
    /// Lobby events not yet handed to the screens.
    events: Vec<LobbyEvent>,
}

impl NetSession {
    /// A session over any transport. `build` + `data_hash` go in our Hello.
    pub fn new(role: Role, transport: Box<dyn Transport>, build: String, data_hash: u64) -> Self {
        NetSession {
            role,
            transport,
            status: Status::Waiting,
            build,
            data_hash,
            started: Instant::now(),
            last_recv: Instant::now(),
            last_hello: None,
            last_ping: None,
            last_inputs: None,
            ping_ms: None,
            peer_confirmed: false,
            early: Vec::new(),
            local_hi: None,
            kept_from: 0,
            local: BTreeMap::new(),
            remote: BTreeMap::new(),
            local_sums: BTreeMap::new(),
            remote_sums: BTreeMap::new(),
            desync_tick: None,
            events: Vec::new(),
        }
    }

    /// Two sessions wired to each other in memory (tests, menu lane).
    pub fn loopback_pair(data_hash: u64) -> (NetSession, NetSession) {
        let (a, b) = Loopback::pair();
        (NetSession::new(Role::Host, Box::new(a), "test".into(), data_hash), NetSession::new(Role::Client, Box::new(b), "test".into(), data_hash))
    }

    pub fn my_slot(&self) -> usize {
        self.role.slot()
    }

    /// Fold one more setup fingerprint into what this side's Hello claims
    /// (call before the first `poll`): the `--online-host` / `--online-join`
    /// test hook mixes in its `--mode`, so two copies started with different
    /// gamemodes refuse at the handshake instead of desyncing.
    pub fn mix_setup(&mut self, extra: u64) {
        self.data_hash = (self.data_hash ^ extra).wrapping_mul(0x0000_0100_0000_01b3);
    }

    /// The UDP port this end is bound to (`None` for in-memory sessions).
    pub fn local_port(&self) -> Option<u16> {
        self.transport.local_port()
    }

    /// Send a lobby message. Sending `Start` also resets the match state
    /// (the receiver does the same when it arrives), so both sides begin
    /// tick 0 clean.
    pub fn send(&mut self, msg: NetMsg) {
        if matches!(msg, NetMsg::Start { .. }) {
            self.reset_match();
        }
        self.transport.send(&msg);
    }

    /// Last measured round trip in milliseconds (`None` before the first
    /// `Pong`).
    pub fn ping_ms(&self) -> Option<u32> {
        self.ping_ms
    }

    /// Call every frame: reads the network, answers pings, keeps the
    /// connection alive / times it out, and returns lobby events. Match
    /// inputs and checksums are kept for `inputs_for` / `desync_tick`.
    pub fn poll(&mut self) -> Vec<LobbyEvent> {
        self.poll_at(Instant::now())
    }

    /// `poll` with an explicit clock (tests fast-forward time with this).
    pub(crate) fn poll_at(&mut self, now: Instant) -> Vec<LobbyEvent> {
        if matches!(self.status, Status::Closed(_)) {
            self.transport.recv();
            return std::mem::take(&mut self.events);
        }
        if (self.status == Status::Waiting || !self.peer_confirmed) && self.last_hello.is_none_or(|t| now.saturating_duration_since(t) >= HELLO_EVERY) {
            self.send_hello(now);
        }
        for msg in self.transport.recv() {
            if matches!(self.status, Status::Closed(_)) {
                break;
            }
            self.last_recv = now;
            self.handle(msg, now);
        }
        match self.status {
            Status::Connected => {
                if self.last_ping.is_none_or(|t| now.saturating_duration_since(t) >= PING_EVERY) {
                    self.transport.send(&NetMsg::Ping(self.ms(now)));
                    self.last_ping = Some(now);
                }
                if !self.local.is_empty() && self.last_inputs.is_none_or(|t| now.saturating_duration_since(t) >= INPUTS_REPEAT) {
                    self.send_inputs(now);
                }
                if now.saturating_duration_since(self.last_recv) > LOST_AFTER {
                    self.close("Connection lost".into());
                }
            }
            Status::Waiting if self.role == Role::Client && now.saturating_duration_since(self.started) > JOIN_TIMEOUT => {
                self.close("Could not reach host".into());
            }
            _ => {}
        }
        std::mem::take(&mut self.events)
    }

    fn ms(&self, now: Instant) -> u32 {
        now.saturating_duration_since(self.started).as_millis() as u32
    }

    fn send_hello(&mut self, now: Instant) {
        self.transport.send(&NetMsg::Hello { build: self.build.clone(), data_hash: self.data_hash });
        self.last_hello = Some(now);
    }

    fn handle(&mut self, msg: NetMsg, now: Instant) {
        if let NetMsg::Hello { build, data_hash } = msg {
            if build != self.build {
                self.send_hello(now); // so the peer can say why too
                self.close(format!("Different game builds (theirs: {build}, ours: {})", self.build));
            } else if data_hash != self.data_hash {
                self.send_hello(now);
                self.close("Different converted game data or gamemodes: both players must convert the same game files and run the same build".into());
            } else if self.status == Status::Waiting {
                // The joiner may have said hello before we were listening for it.
                self.send_hello(now);
                self.status = Status::Connected;
                self.events.push(LobbyEvent::Connected);
                for m in std::mem::take(&mut self.early) {
                    self.handle(m, now);
                }
            }
            return;
        }
        if self.status == Status::Waiting {
            if msg == NetMsg::Bye {
                self.close("The other player left".into());
            } else if self.early.len() < MAX_EARLY {
                self.early.push(msg);
            }
            return;
        }
        self.peer_confirmed = true;
        match msg {
            NetMsg::Hello { .. } => {}
            NetMsg::Pick(p) => self.events.push(LobbyEvent::PeerPick(p)),
            NetMsg::Rules(r) => self.events.push(LobbyEvent::Rules(r)),
            NetMsg::Start { rules, picks } => {
                self.reset_match();
                self.events.push(LobbyEvent::Start { rules, picks });
            }
            NetMsg::Inputs { first_tick, inputs } => {
                // In lockstep the peer can't be more than about
                // 2 * INPUT_DELAY ticks past our newest input; anything
                // further is a stale packet from a previous match.
                let hi = self.local_hi.unwrap_or(0).max(2 * INPUT_DELAY) + 2 * INPUT_DELAY + RESEND as u64;
                for (k, b) in inputs.into_iter().enumerate() {
                    let t = first_tick.saturating_add(k as u64);
                    if t >= self.kept_from && t <= hi {
                        self.remote.entry(t).or_insert(Input::from_bytes(b));
                    }
                }
            }
            NetMsg::Checksum { tick, hash } => {
                self.remote_sums.insert(tick, hash);
                self.compare_sums();
            }
            NetMsg::Ping(t) => self.transport.send(&NetMsg::Pong(t)),
            NetMsg::Pong(t) => self.ping_ms = Some(self.ms(now).wrapping_sub(t)),
            NetMsg::Bye => self.close("The other player left".into()),
        }
    }

    fn close(&mut self, reason: String) {
        self.status = Status::Closed(reason.clone());
        self.events.push(LobbyEvent::Closed(reason));
    }

    /// Leave politely (back to title, window closing).
    pub fn leave(&mut self) {
        self.transport.send(&NetMsg::Bye);
        self.status = Status::Closed("Left".into());
    }

    /// Forget all match inputs/checksums (a new match starts at tick 0).
    pub fn reset_match(&mut self) {
        self.local.clear();
        self.remote.clear();
        self.local_sums.clear();
        self.remote_sums.clear();
        self.desync_tick = None;
        self.local_hi = None;
        self.kept_from = 0;
        self.last_inputs = None;
    }

    /// This machine's input, read during sim tick `tick`: it takes effect at
    /// `tick + INPUT_DELAY`, here and on the peer. Sends the last `RESEND`.
    pub fn push_local_input(&mut self, tick: u64, input: Input) {
        let at = tick + INPUT_DELAY;
        self.local.entry(at).or_insert(input);
        self.local_hi = Some(self.local_hi.map_or(at, |h| h.max(at)));
        self.send_inputs(Instant::now());
    }

    /// Send our newest inputs: up to `RESEND` consecutive ticks ending at
    /// the newest.
    fn send_inputs(&mut self, now: Instant) {
        let Some(at) = self.local_hi else { return };
        let mut inputs = Vec::with_capacity(RESEND);
        for t in (at.saturating_sub(RESEND as u64 - 1)..=at).rev() {
            match self.local.get(&t) {
                Some(i) => inputs.push(i.to_bytes()),
                None => break,
            }
        }
        if inputs.is_empty() {
            return;
        }
        inputs.reverse();
        let first_tick = at + 1 - inputs.len() as u64;
        self.transport.send(&NetMsg::Inputs { first_tick, inputs });
        self.last_inputs = Some(now);
    }

    /// Both players' inputs for `tick` (indexed by slot), or `None` while
    /// the peer's hasn't arrived -- the sim must wait. The first
    /// `INPUT_DELAY` ticks are neutral on both sides.
    pub fn inputs_for(&self, tick: u64) -> Option<[Input; 2]> {
        if tick < INPUT_DELAY {
            return Some([Input::default(); 2]);
        }
        let mine = *self.local.get(&tick)?;
        let theirs = *self.remote.get(&tick)?;
        let mut out = [Input::default(); 2];
        out[self.my_slot()] = mine;
        out[1 - self.my_slot()] = theirs;
        Some(out)
    }

    /// Drop inputs older than `tick` (already simulated).
    /// The newest `RESEND` local inputs are kept regardless, so `poll` can
    /// still repeat them to a peer that lost them.
    pub fn forget_before(&mut self, tick: u64) {
        let keep_local = self.local_hi.map_or(tick, |h| tick.min(h.saturating_sub(RESEND as u64 - 1)));
        self.local = self.local.split_off(&keep_local);
        self.remote = self.remote.split_off(&tick);
        self.kept_from = self.kept_from.max(tick);
    }

    /// After stepping a tick that is a multiple of `CHECKSUM_EVERY`: send
    /// and remember our `Match::checksum()`.
    pub fn report_checksum(&mut self, tick: u64, hash: u64) {
        self.local_sums.insert(tick, hash);
        self.transport.send(&NetMsg::Checksum { tick, hash });
        self.compare_sums();
    }

    fn compare_sums(&mut self) {
        let common: Vec<u64> = self.local_sums.keys().filter(|t| self.remote_sums.contains_key(t)).copied().collect();
        for t in common {
            if self.local_sums[&t] != self.remote_sums[&t] && self.desync_tick.is_none_or(|d| t < d) {
                self.desync_tick = Some(t);
            }
            self.local_sums.remove(&t);
            self.remote_sums.remove(&t);
        }
    }
}

#[cfg(test)]
mod lan_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_bytes_round_trip() {
        let i = Input { stick: [-127, 64], jump: true, lock: false, skills: 0b1010, cam: -5, cam_fast: true, recenter: true, switch_target: true };
        assert_eq!(Input::from_bytes(i.to_bytes()), i);
        assert_eq!(Input::from_bytes(Input::default().to_bytes()), Input::default());
    }

    #[test]
    fn hello_connects_matching_peers_and_rejects_different_data() {
        let (mut h, mut c) = NetSession::loopback_pair(42);
        h.poll();
        c.poll();
        assert_eq!(h.poll(), vec![LobbyEvent::Connected]);
        assert_eq!(c.status, Status::Connected);

        let (a, b) = Loopback::pair();
        let mut h = NetSession::new(Role::Host, Box::new(a), "test".into(), 1);
        let mut c = NetSession::new(Role::Client, Box::new(b), "test".into(), 2);
        h.poll();
        c.poll();
        assert!(matches!(c.status, Status::Closed(_)));
    }

    #[test]
    fn lockstep_waits_for_both_inputs_and_orders_by_slot() {
        let (mut h, mut c) = NetSession::loopback_pair(0);
        h.poll();
        c.poll();
        h.poll();
        let a = Input { skills: 1, ..Default::default() };
        let b = Input { skills: 2, ..Default::default() };
        // Ticks below the delay are neutral without any network.
        assert_eq!(h.inputs_for(0), Some([Input::default(); 2]));
        h.push_local_input(0, a);
        assert_eq!(h.inputs_for(INPUT_DELAY), None, "peer's input not here yet");
        c.push_local_input(0, b);
        h.poll();
        c.poll();
        assert_eq!(h.inputs_for(INPUT_DELAY), Some([a, b]));
        assert_eq!(c.inputs_for(INPUT_DELAY), Some([a, b]), "same order on both machines");
    }

    #[test]
    fn checksum_mismatch_is_flagged_at_its_tick() {
        let (mut h, mut c) = NetSession::loopback_pair(0);
        h.report_checksum(60, 7);
        c.report_checksum(60, 7);
        h.report_checksum(120, 8);
        c.report_checksum(120, 9);
        h.poll();
        c.poll();
        h.poll(); // h hears c's Hello, then the checksums that came early
        assert_eq!(h.desync_tick, Some(120));
        assert_eq!(c.desync_tick, Some(120));
    }
}
