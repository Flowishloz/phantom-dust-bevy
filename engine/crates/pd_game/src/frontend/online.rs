//! ONLINE: meet one friend over Steam or a LAN, then the lobby
//! (docs/ONLINE-PLAN.md, lane B + the Steam lane). Screens, all inside
//! `AppState::Online`:
//!
//! - Choose: Host Steam lobby / Find Steam lobbies / Host LAN game / Join
//!   LAN game (B goes back to the title; the BACK row went 2026-10-09). The
//!   Steam rows are greyed, with a plain "start Steam" line, when Steam isn't
//!   available (`net::steam::SteamNet::unavailable`).
//! - Steam host: creates a Steam lobby (App 480, friends-only unless switched
//!   to public), "Lobby open - invite a friend", Invite friend (Steam overlay),
//!   and waits; the friend joining moves both to the Lobby page.
//! - Steam find: the lobbies of this build that friends are in or that are
//!   public; Refresh; pick one to join (-> Connecting -> Lobby).
//! - Host: `net::open_lan_host`, "waiting for a player on port 7777", this
//!   PC's address to tell the friend, and the port-forwarding note.
//! - Join: type `ip` or `ip:port` (remembered in `userdata/online.json`).
//! - Connecting: waiting for the host's hello (LAN or Steam).
//! - Lobby: each side picks character, colour and arsenal; the host alone
//!   picks gamemode (STANDARD / CLOSE QUARTERS / every `modes::MODES` entry that can be played online: not Prop Hunt), stage, time limit and rounds; both press Ready; the host then
//!   sends `NetMsg::Start` and both sides start the match with identical data.
//! - Error: the session ended (`LobbyEvent::Closed`).
//!
//! An accepted Steam invite on any screen but a match jumps here and joins
//! ([`steam_invites`]).
//!
//! The `NetSession` resource lives from "Host"/"Connect" until the player
//! backs out (removed in [`cleanup`]). Starting a match keeps it alive for
//! lane E's lockstep: it is removed only by the next visit to this screen
//! (`enter`) or whoever ends the online match. A Steam session's transport
//! owns our place in the Steam lobby, so removing the session leaves it.
//!
//! The lobby logic ([`Lobby`], [`apply_start`]) is plain data over a
//! `NetSession`, tested against `NetSession::loopback_pair`.

use super::discovery::{CharacterEntry, StageEntry};
use super::input::MenuAxis;
use super::modes;
use super::setup::{ArsenalOption, COLOURS};
use super::state::{AppState, GameData, MatchSetup, SavedArsenals, SkillCatalog, BY_RULE, DEFAULT_ROUNDS_IDX, DEFAULT_TIME_IDX, ROUNDS_OPTIONS, TIME_OPTIONS};
use super::ui::{self, Glyph, MenuArt, PixelText, RowRefs};
use crate::net::steam::{LobbyListing, SteamEvent, SteamNet, Visibility};
use crate::net::{self, LobbyEvent, LobbyPick, LobbyRules, NetMsg, NetSession, Role, DEFAULT_PORT};
use crate::seats::{LocalViews, PlayerSetup, RequestedPlayers, SeatKind, SeatKinds};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use std::path::{Path, PathBuf};

// ---- the lobby: pure logic -------------------------------------------------

/// What the lobby chooses from.
pub struct LobbyCtx<'a> {
    pub chars: &'a [CharacterEntry],
    pub stages: &'a [StageEntry],
    pub arsenals: &'a [ArsenalOption],
}

/// The rows of the lobby screen, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbyRow {
    Character,
    Colour,
    Arsenal,
    /// The host's gamemode selector (read-only for the joiner).
    Gamemode,
    Stage,
    Time,
    Rounds,
    Ready,
    Leave,
}

pub const LOBBY_ROWS: [LobbyRow; 9] = [LobbyRow::Character, LobbyRow::Colour, LobbyRow::Arsenal, LobbyRow::Gamemode, LobbyRow::Stage, LobbyRow::Time, LobbyRow::Rounds, LobbyRow::Ready, LobbyRow::Leave];

impl LobbyRow {
    /// The host alone sets the rules.
    pub fn enabled(self, role: Role) -> bool {
        !matches!(self, LobbyRow::Gamemode | LobbyRow::Stage | LobbyRow::Time | LobbyRow::Rounds) || role == Role::Host
    }
}

/// What a pumped frame produced.
#[derive(Debug, Default, PartialEq)]
pub struct Pumped {
    pub connected: bool,
    /// Start the match now (`picks[slot]`).
    pub start: Option<(LobbyRules, [LobbyPick; 2])>,
    pub closed: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Lobby {
    pub char_idx: usize,
    pub colour: u8,
    pub arsenal_idx: usize,
    pub ready: bool,
    /// The host's own rule choices (indices into the stage list / option tables).
    /// The host's gamemode: `None` = STANDARD, else an index into `modes::MODES`.
    pub mode_idx: Option<usize>,
    pub stage_idx: usize,
    pub time_idx: usize,
    pub rounds_idx: usize,
    /// The other player's latest pick.
    pub peer: Option<LobbyPick>,
    /// The rules the host announced (what a joiner shows).
    pub remote_rules: Option<LobbyRules>,
    start_sent: bool,
}

impl Lobby {
    pub fn new() -> Self {
        Lobby { char_idx: 0, colour: 0, arsenal_idx: 0, ready: false, mode_idx: None, stage_idx: 0, time_idx: DEFAULT_TIME_IDX, rounds_idx: DEFAULT_ROUNDS_IDX, peer: None, remote_rules: None, start_sent: false }
    }

    pub fn my_pick(&self, ctx: &LobbyCtx) -> LobbyPick {
        LobbyPick {
            char: ctx.chars.get(self.char_idx).map(|c| c.id.clone()).unwrap_or_default(),
            colour: self.colour,
            arsenal: ctx.arsenals.get(self.arsenal_idx).map(|a| a.skills.clone()).unwrap_or_default(),
            ready: self.ready,
        }
    }

    /// The host's rules as chosen.
    pub fn host_rules(&self, ctx: &LobbyCtx) -> LobbyRules {
        let (mode, rules_hash) = modes::lobby_fields(self.mode_idx);
        LobbyRules {
            mode,
            rules_hash,
            stage: ctx.stages.get(self.stage_idx).map(|s| s.id.clone()).unwrap_or_default(),
            time_limit_ticks: TIME_OPTIONS[self.time_idx.min(TIME_OPTIONS.len() - 1)].1,
            rounds_to_win: ROUNDS_OPTIONS[self.rounds_idx.min(ROUNDS_OPTIONS.len() - 1)].1,
        }
    }

    /// The rules this screen shows: the host's own, or the ones announced.
    pub fn shown_rules(&self, role: Role, ctx: &LobbyCtx) -> LobbyRules {
        match role {
            Role::Host => self.host_rules(ctx),
            Role::Client => self.remote_rules.clone().unwrap_or_default(),
        }
    }

    /// The gamemode this screen shows: the host's own choice, or the one the host announced (`None` = STANDARD, and also
    /// for a mode this build doesn't know: [`modes::resolve_lobby`] refuses that at Start).
    pub fn shown_mode(&self, role: Role) -> Option<usize> {
        match role {
            Role::Host => self.mode_idx,
            Role::Client => self.remote_rules.as_ref().and_then(|r| modes::by_id(&r.mode)),
        }
    }

    fn send_pick(&self, session: &mut NetSession, ctx: &LobbyCtx) {
        session.send(NetMsg::Pick(self.my_pick(ctx)));
    }

    /// Tell the other side everything we know (once connected).
    pub fn announce(&self, session: &mut NetSession, ctx: &LobbyCtx) {
        self.send_pick(session, ctx);
        if session.role == Role::Host {
            session.send(NetMsg::Rules(self.host_rules(ctx)));
        }
    }

    /// Call every frame while a session exists: reads the network, keeps the
    /// peer's pick and the rules, and (host) starts the match when both are ready.
    pub fn pump(&mut self, session: &mut NetSession, ctx: &LobbyCtx) -> Pumped {
        let mut out = Pumped::default();
        for ev in session.poll() {
            match ev {
                LobbyEvent::Connected => {
                    out.connected = true;
                    self.announce(session, ctx);
                }
                LobbyEvent::PeerPick(p) => self.peer = Some(p),
                LobbyEvent::Rules(r) => {
                    // A joiner that can't run the host's gamemode (other build / table) leaves now, not at the first desync.
                    if let Err(why) = modes::resolve_lobby(&r.mode, r.rules_hash) {
                        session.leave();
                        out.closed = Some(why);
                    }
                    self.remote_rules = Some(r);
                }
                LobbyEvent::Start { rules, picks } => out.start = Some((rules, picks)),
                LobbyEvent::Closed(reason) => out.closed = Some(reason),
            }
        }
        if session.role == Role::Host && out.start.is_none() && out.closed.is_none() && !self.start_sent && self.ready && self.peer.as_ref().is_some_and(|p| p.ready) {
            let rules = self.host_rules(ctx);
            let picks = [self.my_pick(ctx), self.peer.clone().unwrap_or_default()];
            session.send(NetMsg::Start { rules: rules.clone(), picks: picks.clone() });
            self.start_sent = true;
            out.start = Some((rules, picks));
        }
        out
    }

    /// A row's current value as text.
    pub fn value(&self, row: LobbyRow, role: Role, ctx: &LobbyCtx) -> String {
        let rules = self.shown_rules(role, ctx);
        match row {
            LobbyRow::Character => ctx.chars.get(self.char_idx).map_or("-".into(), |c| c.label.to_uppercase()),
            LobbyRow::Colour => format!("{}", self.colour as usize + 1),
            LobbyRow::Arsenal if self.shown_mode(role).is_some() => "FIXED HAND".into(),
            LobbyRow::Arsenal => ctx.arsenals.get(self.arsenal_idx).map_or("-".into(), |a| a.name.to_uppercase()),
            LobbyRow::Gamemode => modes::label(self.shown_mode(role)),
            LobbyRow::Stage => stage_name(&rules.stage, ctx),
            LobbyRow::Time => time_label(rules.time_limit_ticks).to_uppercase(),
            LobbyRow::Rounds => rounds_label(rules.rounds_to_win).to_uppercase(),
            LobbyRow::Ready => (if self.ready { "READY" } else { "NOT READY" }).into(),
            LobbyRow::Leave => String::new(),
        }
    }

    /// Left/right on a row. Any change takes the ready mark back and tells
    /// the other side. Returns whether anything changed.
    pub fn adjust(&mut self, row: LobbyRow, dir: i32, session: &mut NetSession, ctx: &LobbyCtx) -> bool {
        if !row.enabled(session.role) {
            return false;
        }
        let step = |i: usize, n: usize| if n == 0 { 0 } else { (i as i64 + dir as i64).rem_euclid(n as i64) as usize };
        match row {
            LobbyRow::Character => self.char_idx = step(self.char_idx, ctx.chars.len()),
            LobbyRow::Colour => self.colour = step(self.colour as usize, COLOURS as usize) as u8,
            // A gamemode deals its own fixed hand: nothing to pick.
            LobbyRow::Arsenal if self.shown_mode(session.role).is_some() => return false,
            LobbyRow::Arsenal => self.arsenal_idx = step(self.arsenal_idx, ctx.arsenals.len()),
            // Only the modes that can be played online (Prop Hunt is local split screen only).
            LobbyRow::Gamemode => self.mode_idx = modes::step_online(self.mode_idx, dir >= 0),
            LobbyRow::Stage => self.stage_idx = step(self.stage_idx, ctx.stages.len()),
            LobbyRow::Time => self.time_idx = step(self.time_idx, TIME_OPTIONS.len()),
            LobbyRow::Rounds => self.rounds_idx = step(self.rounds_idx, ROUNDS_OPTIONS.len()),
            LobbyRow::Ready | LobbyRow::Leave => return false,
        }
        self.ready = false;
        self.send_pick(session, ctx);
        if matches!(row, LobbyRow::Gamemode | LobbyRow::Stage | LobbyRow::Time | LobbyRow::Rounds) {
            session.send(NetMsg::Rules(self.host_rules(ctx)));
        }
        true
    }

    /// Confirm on the Ready row.
    pub fn toggle_ready(&mut self, session: &mut NetSession, ctx: &LobbyCtx) {
        self.ready = !self.ready;
        self.send_pick(session, ctx);
    }
}

impl Default for Lobby {
    fn default() -> Self {
        Self::new()
    }
}

fn stage_name(id: &str, ctx: &LobbyCtx) -> String {
    ctx.stages.iter().find(|s| s.id == id).map_or_else(|| id.to_uppercase(), |s| s.name.to_uppercase())
}

fn time_label(ticks: u32) -> &'static str {
    TIME_OPTIONS.iter().find(|(_, t)| *t == ticks).map_or("?", |(n, _)| n)
}

fn rounds_label(r: Option<u32>) -> &'static str {
    ROUNDS_OPTIONS.iter().find(|(_, v)| *v == r).map_or("?", |(n, _)| n)
}

/// Starts a match from a `Start` message: the same data on both PCs, except
/// whose slot is Human (ours) and which is Remote, and our one local view.
/// Slot 0 is the host's pick, slot 1 the joiner's; teams 0 / 1, seat = slot.
///
/// The gamemode is resolved first, from the host's id and `Rules` fingerprint: a PC that can't run the same `Rules`
/// refuses (`Err`, nothing is changed) instead of starting a match that would desync. In a gamemode both fighters deal
/// the mode's fixed hand and play at the neutral Aura rate, exactly as the local setup does.
pub fn apply_start(rules: &LobbyRules, picks: &[LobbyPick; 2], my_slot: usize, requested: &mut RequestedPlayers, kinds: &mut SeatKinds, views: &mut LocalViews, setup: &mut MatchSetup) -> Result<(), String> {
    let mode = modes::resolve_lobby(&rules.mode, rules.rules_hash)?;
    let hand = mode.and_then(modes::get).map(|m| m.hand.to_vec());
    let arsenal_of = |p: &LobbyPick| hand.clone().unwrap_or_else(|| p.arsenal.clone());
    let players: Vec<PlayerSetup> = picks.iter().enumerate().map(|(slot, p)| PlayerSetup { char: p.char.clone(), colour: p.colour, arsenal: arsenal_of(p), team: slot as u8, seat: slot }).collect();
    let mut k = SeatKinds::default();
    for slot in 0..2 {
        k.0[slot] = if slot == my_slot { SeatKind::Human } else { SeatKind::Remote };
    }
    *kinds = k;
    *views = LocalViews(Some(vec![my_slot]));
    *setup = MatchSetup {
        chars: [picks[0].char.clone(), picks[1].char.clone()],
        arsenals: [arsenal_of(&picks[0]), arsenal_of(&picks[1])],
        stage: rules.stage.clone(),
        time_limit_ticks: if rules.time_limit_ticks == BY_RULE { crate::combat::TIME_LIMIT_TICKS } else { rules.time_limit_ticks },
        time_by_rule: rules.time_limit_ticks == BY_RULE,
        rounds_to_win: rules.rounds_to_win,
        mode,
        arsenal_kinds: if mode.is_some() { std::array::from_fn(|s| (s < 2).then_some(crate::arsenal::DEFAULT_KIND)) } else { [None; crate::combat::MAX_PLAYERS] },
        ..MatchSetup::default()
    };
    requested.0 = Some(players);
    Ok(())
}

// ---- remembered address ------------------------------------------------------

/// `<engine>/userdata/online.json` (next to `arsenals.json`).
pub fn store_path(cache: &Path) -> PathBuf {
    let base = cache.parent().map(Path::to_path_buf).unwrap_or_else(|| cache.to_path_buf());
    base.join("userdata").join("online.json")
}

pub fn load_address(path: &Path) -> String {
    let v: serde_json::Value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v["last_address"].as_str().unwrap_or_default().to_string()
}

pub fn save_address(path: &Path, addr: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "last_address": addr }).to_string());
}

/// Keys the address box accepts.
pub fn address_char_ok(c: char) -> bool {
    c.is_ascii_digit() || c == '.' || c == ':'
}

/// This PC's LAN address as the OS would use it to reach the internet (no
/// packet is sent: a UDP "connect" only picks the route).
pub fn local_addresses() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(a) = s.local_addr() {
                if !a.ip().is_unspecified() {
                    out.push(a.ip().to_string());
                }
            }
        }
    }
    out
}

// ---- the screens -------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Choose,
    SteamHost,
    SteamFind,
    Host,
    Join,
    Connecting,
    Lobby,
    Error,
}

/// The Choose page's rows, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChooseRow {
    HostSteam,
    FindSteam,
    HostLan,
    JoinLan,
}

/// No BACK row: B goes back to the title (Laurence, 2026-10-09: de-clutter; UI-DECLUTTER-PROPOSALS.md item 8).
pub const CHOOSE_ROWS: [ChooseRow; 4] = [ChooseRow::HostSteam, ChooseRow::FindSteam, ChooseRow::HostLan, ChooseRow::JoinLan];

impl ChooseRow {
    pub fn label(self) -> &'static str {
        match self {
            ChooseRow::HostSteam => "HOST STEAM LOBBY",
            ChooseRow::FindSteam => "FIND STEAM LOBBIES",
            ChooseRow::HostLan => "HOST LAN GAME",
            ChooseRow::JoinLan => "JOIN LAN GAME",
        }
    }
    pub fn needs_steam(self) -> bool {
        matches!(self, ChooseRow::HostSteam | ChooseRow::FindSteam)
    }
}

/// The Steam host page's rows: Invite friend, Visibility, Back.
pub const STEAM_HOST_ROWS: usize = 3;
/// Most lobbies listed on the Find page (then Refresh, Back).
pub const MAX_LISTED: usize = 6;

/// A Steam name in the menu font: upper case, plain characters, short.
pub fn menu_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_graphic() || c == ' ' { c.to_ascii_uppercase() } else { '?' }).take(22).collect();
    if s.trim().is_empty() {
        "?".into()
    } else {
        s
    }
}

#[derive(Resource)]
pub struct OnlineState {
    pub page: Page,
    built: Option<(Page, usize)>,
    pub row: usize,
    /// Message shown on the current page in red (open failure, closed session).
    pub error: Option<String>,
    pub addr: String,
    pub ips: Vec<String>,
    pub lobby: Lobby,
    pub role: Role,
    options: Vec<ArsenalOption>,
    store: PathBuf,
    cache: PathBuf,
    /// Set when `Start` arrived: [`begin_match`] applies it.
    pub pending_start: Option<(LobbyRules, [LobbyPick; 2])>,
    /// The match is starting: keep the `NetSession` when leaving the screen.
    pub starting: bool,
    repeat: [super::input::Repeat; 2],
    /// `PD_ONLINE_DEMO=1`: a scripted second player on an in-memory session,
    /// so the lobby can be screenshotted on one PC.
    demo: Option<(NetSession, Lobby)>,
    /// Steam, as of this frame: `None` = usable, else why not.
    pub steam_off: Option<String>,
    pub persona: String,
    /// The Steam lobby we host (once created).
    pub steam_lobby: Option<u64>,
    pub steam_vis: Visibility,
    pub overlay: bool,
    /// Find page: the lobbies found, and whether a search is running.
    pub steam_list: Vec<LobbyListing>,
    pub searching: bool,
    /// Joining over Steam: the host's name (Connecting page text).
    pub joining: Option<String>,
}

impl Default for OnlineState {
    fn default() -> Self {
        OnlineState {
            page: Page::Choose,
            built: None,
            row: 0,
            error: None,
            addr: String::new(),
            ips: Vec::new(),
            lobby: Lobby::new(),
            role: Role::Host,
            options: Vec::new(),
            store: PathBuf::new(),
            cache: PathBuf::new(),
            pending_start: None,
            starting: false,
            repeat: Default::default(),
            demo: None,
            steam_off: Some("Steam is starting".into()),
            persona: String::new(),
            steam_lobby: None,
            steam_vis: Visibility::default(),
            overlay: false,
            steam_list: Vec::new(),
            searching: false,
            joining: None,
        }
    }
}

impl OnlineState {
    fn go(&mut self, page: Page) {
        self.page = page;
        self.row = 0;
        if !self.row_enabled(0) {
            self.step_row(1);
        }
    }

    fn ctx<'a>(&'a self, data: &'a GameData) -> LobbyCtx<'a> {
        LobbyCtx { chars: &data.characters, stages: &data.stages, arsenals: &self.options }
    }

    /// Lobbies shown on the Find page (the first `MAX_LISTED`).
    fn listed(&self) -> &[LobbyListing] {
        &self.steam_list[..self.steam_list.len().min(MAX_LISTED)]
    }

    fn row_count(&self) -> usize {
        match self.page {
            Page::Choose => CHOOSE_ROWS.len(),
            Page::SteamHost => STEAM_HOST_ROWS,
            Page::SteamFind => self.listed().len() + 2,
            Page::Host | Page::Connecting | Page::Error => 1,
            Page::Join => 3,
            Page::Lobby => LOBBY_ROWS.len(),
        }
    }

    fn row_enabled(&self, i: usize) -> bool {
        match self.page {
            Page::Lobby => LOBBY_ROWS.get(i).is_some_and(|r| r.enabled(self.role) && !(*r == LobbyRow::Arsenal && self.lobby.shown_mode(self.role).is_some())),
            Page::Choose => CHOOSE_ROWS.get(i).is_some_and(|r| !r.needs_steam() || self.steam_off.is_none()),
            // Invite / visibility once the lobby exists.
            Page::SteamHost => i == STEAM_HOST_ROWS - 1 || self.steam_lobby.is_some(),
            _ => true,
        }
    }

    fn step_row(&mut self, dir: i32) {
        let n = self.row_count();
        for _ in 0..n {
            self.row = (self.row as i32 + dir).rem_euclid(n as i32) as usize;
            if self.row_enabled(self.row) {
                break;
            }
        }
    }

    /// (label, value) of each row on the current page.
    fn rows(&self, ctx: &LobbyCtx) -> Vec<(String, String)> {
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        match self.page {
            Page::Choose => CHOOSE_ROWS.iter().map(|r| s(r.label(), "")).collect(),
            Page::SteamHost => vec![s("INVITE FRIEND", ""), s("VISIBILITY", &format!("< {} >", self.steam_vis.label())), s("BACK", "")],
            Page::SteamFind => {
                let mut v: Vec<(String, String)> = self.listed().iter().map(|l| (menu_name(&l.host), (if l.mine { "YOU" } else if l.friend { "FRIEND" } else { "PUBLIC" }).to_string())).collect();
                v.push(s("REFRESH", if self.searching { "SEARCHING" } else { "" }));
                v.push(s("BACK", ""));
                v
            }
            Page::Host | Page::Connecting | Page::Error => vec![s("BACK", "")],
            Page::Join => vec![s("ADDRESS", &format!("{}_", self.addr)), s("CONNECT", ""), s("BACK", "")],
            Page::Lobby => LOBBY_ROWS
                .iter()
                .map(|&r| {
                    let label = match r {
                        LobbyRow::Character => "CHARACTER",
                        LobbyRow::Colour => "COLOUR",
                        LobbyRow::Arsenal => "ARSENAL",
                        LobbyRow::Gamemode => "GAMEMODE",
                        LobbyRow::Stage => "STAGE",
                        LobbyRow::Time => "TIME LIMIT",
                        LobbyRow::Rounds => "ROUNDS TO WIN",
                        LobbyRow::Ready => "READY",
                        LobbyRow::Leave => "LEAVE",
                    };
                    (label.to_string(), self.lobby.value(r, self.role, ctx))
                })
                .collect(),
        }
    }

    /// The text lines of the current page, with their colours.
    fn lines(&self, ctx: &LobbyCtx) -> Vec<(String, Color)> {
        let mut v: Vec<(String, Color)> = Vec::new();
        let mut line = |t: &str, c: Color| v.push((t.to_string(), c));
        match self.page {
            Page::Choose => {
                // One line, and a plain Steam message (item 8).
                line("Steam: play a friend anywhere. LAN: play on your local network.", ui::WHITE);
                match &self.steam_off {
                    Some(why) if why.starts_with("Steam is starting") => line("Connecting to Steam...", ui::DIM),
                    Some(_) => line("Steam isn't available: start Steam, then restart the game.", ui::BAD),
                    None => line(&format!("Signed in to Steam as {}.", self.persona), ui::TEAL),
                }
            }
            Page::SteamHost => {
                match (self.steam_lobby, &self.error) {
                    (_, Some(e)) => line(e, ui::BAD),
                    (None, None) => line("Creating your Steam lobby...", ui::YELLOW),
                    (Some(_), None) => {
                        line("Lobby open - invite a friend", ui::TEAL);
                        line(&format!("Host: {}", self.persona), ui::WHITE);
                        line("Waiting for your friend to join...", ui::YELLOW);
                    }
                }
                line("Invite friend opens the Steam overlay's invite list.", ui::DIM);
                if !self.overlay {
                    line("(The Steam overlay is off here: your friend can use Find Steam lobbies.)", ui::DIM);
                }
                line(
                    match self.steam_vis {
                        Visibility::FriendsOnly => "Friends only: your Steam friends can also find it with Find Steam lobbies.",
                        Visibility::Public => "Public: anyone with this same build can find it.",
                    },
                    ui::DIM,
                );
            }
            Page::SteamFind => {
                if self.searching {
                    line("Searching Steam...", ui::YELLOW);
                } else if self.steam_list.is_empty() {
                    line("No lobbies found.", ui::YELLOW);
                    line("Ask your friend to host one,", ui::WHITE);
                    line("then Refresh.", ui::WHITE);
                } else {
                    let n = self.steam_list.len();
                    line(&format!("{n} lobb{} found.", if n == 1 { "y" } else { "ies" }), ui::YELLOW);
                    line("Pick one to join.", ui::WHITE);
                }
                if let Some(e) = &self.error {
                    line(e, ui::BAD);
                }
                line("Only this game build is listed.", ui::DIM);
                line("Friends-only lobbies: while online.", ui::DIM);
            }
            Page::Host => {
                if self.error.is_none() {
                    line(&format!("Waiting for a player on port {DEFAULT_PORT}..."), ui::YELLOW);
                }
                if let Some(e) = &self.error {
                    line(&format!("Could not host: {e}"), ui::BAD);
                }
                line("Tell your friend this address:", ui::WHITE);
                if self.ips.is_empty() {
                    line("  (this PC's address could not be found)", ui::DIM);
                }
                for ip in &self.ips {
                    line(&format!("  {ip}:{DEFAULT_PORT}"), ui::TEAL);
                }
                line("Playing over the internet needs UDP port 7777 forwarded to this PC.", ui::DIM);
            }
            Page::Join => {
                line("Type the host's address: digits, dots and a colon for the port.", ui::WHITE);
                line("Example: 192.168.1.20 or 192.168.1.20:7777", ui::DIM);
                if let Some(e) = &self.error {
                    line(e, ui::BAD);
                }
            }
            Page::Connecting => match &self.joining {
                Some(host) => {
                    line(&format!("Joining {host}'s Steam lobby..."), ui::YELLOW);
                    line("Waiting for the host to answer through Steam.", ui::WHITE);
                }
                None => {
                    line(&format!("Connecting to {}...", self.addr), ui::YELLOW);
                    line("Waiting for the host to answer.", ui::WHITE);
                }
            },
            Page::Error => {
                line("Connection ended", ui::YELLOW);
                line(self.error.as_deref().unwrap_or("Unknown error"), ui::BAD);
            }
            Page::Lobby => {
                line("OTHER PLAYER", ui::YELLOW);
                match &self.lobby.peer {
                    None => line("Choosing...", ui::DIM),
                    Some(p) => {
                        let name = ctx.chars.iter().find(|c| c.id == p.char).map_or_else(|| p.char.to_uppercase(), |c| c.label.clone());
                        line(&format!("Character: {name}"), ui::WHITE);
                        line(&format!("Colour: {}", p.colour as usize + 1), ui::WHITE);
                        if self.lobby.shown_mode(self.role).is_some() {
                            line("Arsenal: the mode's fixed hand", ui::WHITE);
                        } else {
                            line(&format!("Arsenal: {} skills", p.arsenal.len()), ui::WHITE);
                        }
                        line(if p.ready { "READY" } else { "Not ready" }, if p.ready { ui::TEAL } else { ui::DIM });
                    }
                }
                line(&format!("Gamemode: {}", modes::label(self.lobby.shown_mode(self.role))), ui::TEAL);
                if self.role == Role::Client {
                    line("The host picks gamemode, stage, time and rounds.", ui::DIM);
                }
                if self.lobby.ready {
                    line("You are ready. Waiting for the other player.", ui::TEAL);
                }
            }
        }
        v
    }

    /// Where the page's text lines go (design pixels) and how many rows hang below.
    fn line_origin(&self) -> (f32, f32, f32) {
        match self.page {
            Page::Lobby | Page::SteamFind => (780.0, ui::CONTENT_TOP + 8.0, 30.0),
            Page::Choose => (150.0, ui::CONTENT_TOP + 8.0 + CHOOSE_ROWS.len() as f32 * 54.0 + 20.0, 32.0),
            _ => (150.0, ui::CONTENT_TOP + 8.0, 34.0),
        }
    }

    fn row_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        match self.page {
            Page::Lobby => (110.0, ui::CONTENT_TOP + 8.0 + i as f32 * 46.0, 620.0, 40.0),
            Page::SteamFind => (110.0, ui::CONTENT_TOP + 8.0 + i as f32 * 50.0, 620.0, 42.0),
            Page::Choose => (150.0, ui::CONTENT_TOP + 8.0 + i as f32 * 54.0, 560.0, 42.0),
            Page::SteamHost => (150.0, 400.0 + i as f32 * 54.0, 560.0, 42.0),
            Page::Join => (150.0, 300.0 + i as f32 * 54.0, 760.0, 42.0),
            Page::Host | Page::Connecting | Page::Error => (150.0, 470.0, 448.0, 42.0),
        }
    }
}

#[derive(Component)]
pub(crate) struct OnlineFrame;
#[derive(Component)]
pub(crate) struct OnlineContent;
#[derive(Component)]
pub(crate) struct OnlineRow(usize);
#[derive(Component)]
pub(crate) struct OnlineLine(usize);

const MAX_LINES: usize = 9;

fn drop_session(commands: &mut Commands, session: &mut Option<ResMut<NetSession>>) {
    if let Some(s) = session.as_mut() {
        s.leave();
    }
    commands.remove_resource::<NetSession>();
}

#[allow(clippy::too_many_arguments)]
pub fn enter(
    mut commands: Commands,
    assets: Res<AssetServer>,
    cfg: Res<crate::Config>,
    data: Res<GameData>,
    saved: Res<SavedArsenals>,
    catalog: Res<SkillCatalog>,
    mut state: ResMut<OnlineState>,
    session: Option<ResMut<NetSession>>,
    windows: Query<&Window>,
    art: Option<Res<MenuArt>>,
    mut images: ResMut<Assets<Image>>,
    mut end: ResMut<crate::online_match::OnlineEnd>,
    mut steam: ResMut<SteamNet>,
) {
    // Back from an online match (or a stale session): start clean.
    if let Some(mut s) = session {
        s.leave();
        commands.remove_resource::<NetSession>();
    }
    let store = store_path(&cfg.cache);
    let addr = if state.addr.is_empty() { load_address(&store) } else { state.addr.clone() };
    let options: Vec<ArsenalOption> = data
        .builtins
        .iter()
        .map(|b| ArsenalOption { name: b.name.clone(), skills: b.skills.clone(), kind: b.kind })
        .chain(saved.0.iter().filter(|a| crate::arsenal::check_kind(&a.skills, &catalog.skills, a.kind_or_default(&catalog.skills)).is_empty()).map(|a| ArsenalOption { name: a.name.clone(), skills: a.skills.clone(), kind: Some(a.kind_or_default(&catalog.skills)) }))
        .collect();
    *state = OnlineState { addr, options, store, cache: cfg.cache.clone(), ..default() };
    steam.cancel();
    sync_steam(&mut state, &steam);
    state.go(Page::Choose);
    // An online match that ended early (connection lost, out of sync):
    // say why (`online_match::watch`).
    if let Some(reason) = end.0.take() {
        state.error = Some(reason);
        state.go(Page::Error);
    }
    if std::env::var_os("PD_ONLINE_DEMO").is_some() {
        let (host, client) = NetSession::loopback_pair(0);
        let mut peer = Lobby::new();
        peer.char_idx = 1.min(data.characters.len().saturating_sub(1));
        peer.colour = 2;
        peer.arsenal_idx = 1.min(state.options.len().saturating_sub(1));
        peer.ready = true;
        state.demo = Some((client, peer));
        state.go(Page::Host);
        commands.insert_resource(host);
    }

    let art = MenuArt::get(&mut commands, art, &cfg.cache, &assets, &mut images);
    let (root, frame) = ui::menu_screen(&mut commands, &windows, &art, "ONLINE", true);
    commands.entity(root).insert(DespawnOnExit(AppState::Online));
    commands.entity(frame).insert(OnlineFrame);
    ui::prompt_bar(&mut commands, &art, frame, &[(Glyph::UpDown, "Select")], &[(Glyph::A, "Confirm"), (Glyph::B, "Back")], "Keyboard: W S select, Enter confirm, Esc back. Typing an address: digits . :  Backspace deletes");
}

/// Leaving the screen for anything but a match ends the session.
pub fn cleanup(mut commands: Commands, state: Res<OnlineState>, mut session: Option<ResMut<NetSession>>, mut steam: ResMut<SteamNet>) {
    if !state.starting {
        drop_session(&mut commands, &mut session);
        steam.cancel();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut script: Option<ResMut<super::script::MenuScript>>,
    mut state: ResMut<OnlineState>,
    data: Res<GameData>,
    mut session: Option<ResMut<NetSession>>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    menu: Res<crate::menu::MenuState>,
    mut sfx: ResMut<super::sound::MenuSfxQueue>,
    mut key_events: MessageReader<KeyboardInput>,
    mut steam: ResMut<SteamNet>,
) {
    let typed: Vec<KeyboardInput> = key_events.read().cloned().collect();
    if menu.open || menu.is_changed() {
        return;
    }
    let st = &mut *state;
    if let Some((peer_session, peer_lobby)) = st.demo.as_mut() {
        let ctx = LobbyCtx { chars: &data.characters, stages: &data.stages, arsenals: &st.options };
        peer_lobby.pump(peer_session, &ctx);
    }
    // Steam: an accepted invite, lobby created / joined, search results.
    steam_frame(st, &mut steam, &mut session, &mut commands);
    // The network first: connection, the other player's picks, start, close.
    if let Some(sess) = session.as_deref_mut() {
        st.role = sess.role;
        let ctx = LobbyCtx { chars: &data.characters, stages: &data.stages, arsenals: &st.options };
        let pumped = st.lobby.pump(sess, &ctx);
        if pumped.connected && matches!(st.page, Page::Host | Page::SteamHost | Page::Connecting) {
            st.go(Page::Lobby);
            st.error = None;
        }
        if let Some(start) = pumped.start {
            st.pending_start = Some(start);
        }
        if let Some(reason) = pumped.closed {
            drop_session(&mut commands, &mut session);
            st.error = Some(reason);
            st.go(Page::Error);
            st.lobby = Lobby::new();
            st.steam_lobby = None;
            st.joining = None;
            return;
        }
    }

    let scripted = script.as_deref_mut().and_then(super::script::tick);
    let [ra, rb] = &mut st.repeat;
    let a = super::input::read_repeating(0, &keys, &pads, scripted, ra);
    let b = super::input::read_repeating(1, &keys, &pads, None, rb);
    sfx.0.extend(super::sound::cues_for(&a));
    sfx.0.extend(super::sound::cues_for(&b));
    let m = |f: fn(&MenuAxis) -> bool| f(&a) || f(&b);
    let (up, down, left, right, confirm, cancel) = (m(|x| x.up), m(|x| x.down), m(|x| x.left), m(|x| x.right), m(|x| x.confirm), m(|x| x.cancel));

    // Address entry: the box takes digits, dots and colons; Backspace deletes.
    if st.page == Page::Join {
        for ev in &typed {
            if ev.state != bevy::input::ButtonState::Pressed {
                continue;
            }
            match &ev.logical_key {
                Key::Backspace => {
                    st.addr.pop();
                }
                _ => {
                    if let Some(t) = &ev.text {
                        for c in t.chars().filter(|c| address_char_ok(*c)) {
                            if st.addr.len() < 40 {
                                st.addr.push(c);
                            }
                        }
                    }
                }
            }
        }
    }

    if up {
        st.step_row(-1);
    }
    if down {
        st.step_row(1);
    }

    match st.page {
        Page::Choose => {
            if cancel {
                next.set(AppState::Title);
            } else if confirm && st.row_enabled(st.row) {
                match CHOOSE_ROWS[st.row.min(CHOOSE_ROWS.len() - 1)] {
                    ChooseRow::HostSteam => start_steam_host(st, &mut steam),
                    ChooseRow::FindSteam => {
                        st.error = None;
                        st.steam_list.clear();
                        st.go(Page::SteamFind);
                        steam.refresh_lobbies();
                        st.searching = true;
                    }
                    ChooseRow::HostLan => start_hosting(st, &mut commands),
                    ChooseRow::JoinLan => {
                        st.error = None;
                        st.go(Page::Join);
                    }
                }
            }
        }
        Page::SteamHost => {
            if cancel || (confirm && st.row == STEAM_HOST_ROWS - 1) {
                drop_session(&mut commands, &mut session);
                steam.cancel();
                st.steam_lobby = None;
                st.lobby = Lobby::new();
                st.error = None;
                st.go(Page::Choose);
            } else if let Some(id) = st.steam_lobby {
                if confirm && st.row == 0 {
                    steam.invite_dialog(id);
                } else if st.row == 1 && (confirm || left || right) {
                    st.steam_vis = st.steam_vis.toggled();
                    steam.set_visibility(id, st.steam_vis);
                }
            }
        }
        Page::SteamFind => {
            let n = st.listed().len();
            if cancel || (confirm && st.row == n + 1) {
                st.error = None;
                st.go(Page::Choose);
            } else if confirm && st.row == n {
                st.error = None;
                steam.refresh_lobbies();
                st.searching = true;
            } else if confirm && n > 0 {
                let l = st.listed()[st.row.min(n - 1)].clone();
                if l.mine {
                    st.error = Some("Your own lobby (same Steam account).".into());
                } else {
                    drop_session(&mut commands, &mut session);
                    start_steam_join(st, &mut steam, l.id, l.host);
                }
            }
        }
        Page::Host => {
            if cancel || confirm {
                drop_session(&mut commands, &mut session);
                st.lobby = Lobby::new();
                st.error = None;
                st.go(Page::Choose);
            }
        }
        Page::Join => {
            if cancel || (confirm && st.row == 2) {
                st.error = None;
                st.go(Page::Choose);
            } else if confirm {
                connect(st, &mut commands);
            }
        }
        Page::Connecting => {
            if cancel || confirm {
                drop_session(&mut commands, &mut session);
                steam.cancel();
                st.lobby = Lobby::new();
                if st.joining.take().is_some() {
                    st.go(Page::SteamFind);
                    steam.refresh_lobbies();
                    st.searching = true;
                } else {
                    st.go(Page::Join);
                }
            }
        }
        Page::Error => {
            if cancel || confirm {
                st.error = None;
                st.steam_lobby = None;
                st.joining = None;
                st.go(Page::Choose);
            }
        }
        Page::Lobby => {
            let Some(sess) = session.as_deref_mut() else { return };
            let ctx = LobbyCtx { chars: &data.characters, stages: &data.stages, arsenals: &st.options };
            let row = LOBBY_ROWS[st.row.min(LOBBY_ROWS.len() - 1)];
            if cancel || (confirm && row == LobbyRow::Leave) {
                drop_session(&mut commands, &mut session);
                st.lobby = Lobby::new();
                st.steam_lobby = None;
                st.joining = None;
                st.go(Page::Choose);
            } else if left || right {
                let dir = if right { 1 } else { -1 };
                st.lobby.adjust(row, dir, sess, &ctx);
            } else if confirm && row == LobbyRow::Ready {
                st.lobby.toggle_ready(sess, &ctx);
                // Host: both may now be ready.
                let pumped = st.lobby.pump(sess, &ctx);
                if let Some(start) = pumped.start {
                    st.pending_start = Some(start);
                }
            }
        }
    }
}

/// Copy what the screens show about Steam (availability, name, overlay).
fn sync_steam(st: &mut OnlineState, steam: &SteamNet) {
    st.steam_off = steam.unavailable.clone();
    st.persona = steam.persona.clone();
    st.overlay = steam.overlay_enabled();
}

/// Steam's side of a frame: take an accepted invite, and act on lobby
/// created / joined / search results.
fn steam_frame(st: &mut OnlineState, steam: &mut SteamNet, session: &mut Option<ResMut<NetSession>>, commands: &mut Commands) {
    sync_steam(st, steam);
    if let Some(id) = steam.take_invite() {
        drop_session(commands, session);
        start_steam_join(st, steam, id, "your friend".into());
    }
    for ev in steam.take_events() {
        match ev {
            SteamEvent::Created(Ok(id)) => {
                if st.page != Page::SteamHost {
                    steam.leave_lobby(id);
                } else if let Some(s) = net::open_steam_host(steam, id, &st.cache) {
                    info!("Steam lobby {id} created");
                    commands.insert_resource(s);
                    st.steam_lobby = Some(id);
                    st.error = None;
                    st.row = 0; // Invite friend
                } else {
                    steam.leave_lobby(id);
                    st.error = Some("Steam went away while the lobby was made.".into());
                }
            }
            SteamEvent::Created(Err(e)) => st.error = Some(e),
            SteamEvent::Joined(Ok(id)) => {
                if st.page != Page::Connecting || st.joining.is_none() {
                    steam.leave_lobby(id);
                    continue;
                }
                let own = steam.lobby_owner(id).is_some_and(|o| Some(o) == steam.my_id());
                match if own { None } else { net::open_steam_join(steam, id, &st.cache) } {
                    Some(s) => {
                        info!("Joined Steam lobby {id}");
                        commands.insert_resource(s);
                    }
                    None => {
                        steam.leave_lobby(id);
                        st.joining = None;
                        st.error = Some(if own { "Your own lobby (same Steam account).".into() } else { "That lobby has no host any more.".into() });
                        st.go(Page::Error);
                    }
                }
            }
            SteamEvent::Joined(Err(e)) => {
                st.joining = None;
                st.error = Some(e);
                st.go(Page::Error);
            }
            SteamEvent::ListReady => {}
        }
    }
    if st.page == Page::SteamFind {
        st.steam_list = steam.lobbies();
        st.searching = steam.searching;
        st.row = st.row.min(st.row_count() - 1);
    }
}

fn start_steam_host(st: &mut OnlineState, steam: &mut SteamNet) {
    st.lobby = Lobby::new();
    st.role = Role::Host;
    st.steam_lobby = None;
    st.error = None;
    st.go(Page::SteamHost);
    steam.create_lobby(st.steam_vis);
}

fn start_steam_join(st: &mut OnlineState, steam: &mut SteamNet, id: u64, host: String) {
    st.lobby = Lobby::new();
    st.role = Role::Client;
    st.steam_lobby = None;
    st.joining = Some(host);
    st.error = None;
    st.go(Page::Connecting);
    steam.join_lobby(id);
}

/// An accepted Steam invite on any screen but a match: go to Online, whose
/// `input` joins it. During a match it waits until the match ends.
pub fn steam_invites(steam: Res<SteamNet>, state: Res<State<AppState>>, mut next: ResMut<NextState<AppState>>) {
    if steam.has_invite() && !matches!(state.get(), AppState::Online | AppState::Match) {
        next.set(AppState::Online);
    }
}

/// Steam (`net::steam`) plus the invite watcher: the one line in `main.rs`.
pub fn steam_plugin(app: &mut App) {
    app.add_plugins(net::steam::plugin).add_systems(Update, steam_invites);
}

fn start_hosting(st: &mut OnlineState, commands: &mut Commands) {
    st.go(Page::Host);
    st.ips = local_addresses();
    st.lobby = Lobby::new();
    st.role = Role::Host;
    match net::open_lan_host(DEFAULT_PORT, &st.cache) {
        Ok(s) => {
            st.error = None;
            commands.insert_resource(s);
        }
        Err(e) => st.error = Some(e.to_string()),
    }
}

fn connect(st: &mut OnlineState, commands: &mut Commands) {
    let addr = st.addr.trim().to_string();
    if addr.is_empty() {
        st.error = Some("Type the host's IP address first.".into());
        return;
    }
    save_address(&st.store, &addr);
    st.lobby = Lobby::new();
    st.role = Role::Client;
    match net::open_lan_join(&addr, &st.cache) {
        Ok(s) => {
            st.error = None;
            commands.insert_resource(s);
            st.go(Page::Connecting);
        }
        Err(e) => st.error = Some(format!("Could not connect: {e}")),
    }
}

/// `Start` arrived (or the host sent it): put the match in the shared
/// resources and go. The `NetSession` stays alive for the lockstep.
#[allow(clippy::too_many_arguments)]
pub fn begin_match(
    mut state: ResMut<OnlineState>,
    session: Option<Res<NetSession>>,
    mut requested: ResMut<RequestedPlayers>,
    mut kinds: ResMut<SeatKinds>,
    mut views: ResMut<LocalViews>,
    mut setup: ResMut<MatchSetup>,
    mut next: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    let Some((rules, picks)) = state.pending_start.take() else { return };
    let Some(session) = session else { return };
    if let Err(why) = apply_start(&rules, &picks, session.my_slot(), &mut requested, &mut kinds, &mut views, &mut setup) {
        // The two PCs can't agree on the same Rules: leave, say why, and do not start.
        warn!("online: refusing to start: {why}");
        commands.queue(|world: &mut bevy::ecs::world::World| {
            if let Some(mut s) = world.get_resource_mut::<NetSession>() {
                s.leave();
            }
            world.remove_resource::<NetSession>();
        });
        state.error = Some(why);
        state.steam_lobby = None;
        state.joining = None;
        state.lobby = Lobby::new();
        state.go(Page::Error);
        return;
    }
    state.starting = true;
    next.set(AppState::Match);
}

#[allow(clippy::too_many_arguments)]
pub fn layout(
    mut commands: Commands,
    mut state: ResMut<OnlineState>,
    data: Res<GameData>,
    art: Option<Res<MenuArt>>,
    windows: Query<&Window>,
    mut frames: Query<&mut UiTransform, With<ui::DesignFrame>>,
    frame: Query<Entity, With<OnlineFrame>>,
    content: Query<Entity, With<OnlineContent>>,
    mut rows: Query<(&OnlineRow, &RowRefs, &mut ImageNode)>,
    mut texts: Query<&mut PixelText>,
    mut lines: Query<(&OnlineLine, &mut Text, &mut TextColor, &mut Node)>,
) {
    ui::fit_frames(&windows, &mut frames);
    let (Some(art), Ok(frame)) = (art, frame.single()) else { return };
    if state.built != Some((state.page, state.row_count())) {
        for e in &content {
            commands.entity(e).despawn();
        }
        for i in 0..state.row_count() {
            let (x, y, w, h) = state.row_rect(i);
            let (panel, _) = ui::spawn_row(&mut commands, &art, ui::abs(x, y, w, h), "", "", 1.5);
            commands.entity(panel).insert((OnlineRow(i), OnlineContent));
            commands.entity(frame).add_child(panel);
        }
        let (x, y, step) = state.line_origin();
        for i in 0..MAX_LINES {
            let e = commands.spawn((ui::body("", art.ttf.clone(), 22.0, ui::WHITE), Node { position_type: PositionType::Absolute, left: px(x), top: px(y + i as f32 * step), width: px(if state.page == Page::Lobby { 460.0 } else { 1000.0 }), ..default() }, OnlineLine(i), OnlineContent)).id();
            commands.entity(frame).add_child(e);
        }
        state.built = Some((state.page, state.row_count()));
        return;
    }
    let ctx = state.ctx(&data);
    let texts_now = state.rows(&ctx);
    let lines_now = state.lines(&ctx);
    for (row, refs, mut panel) in &mut rows {
        let i = row.0;
        let Some((label, value)) = texts_now.get(i) else { continue };
        let on = i == state.row;
        let enabled = state.row_enabled(i);
        let (tint, color) = if !enabled {
            (ui::PANEL_DIM, ui::DIM)
        } else if on {
            (ui::PANEL_SEL, ui::YELLOW)
        } else {
            (ui::PANEL, ui::ROW_TEXT)
        };
        ui::set_row(&mut commands, &art, *refs, &mut panel, &mut texts, tint, ("", color), (label, color), (value, color));
    }
    for (line, mut text, mut color, _) in &mut lines {
        let (t, c) = lines_now.get(line.0).cloned().unwrap_or_default();
        if text.0 != t {
            text.0 = t;
        }
        if color.0 != c && !text.0.is_empty() {
            color.0 = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars() -> Vec<CharacterEntry> {
        vec![CharacterEntry { id: "pc00a".into(), label: "Hero".into() }, CharacterEntry { id: "pc01a".into(), label: "Edgar".into() }]
    }
    fn stages() -> Vec<StageEntry> {
        vec![StageEntry { id: "st01".into(), name: "One".into() }, StageEntry { id: "st02".into(), name: "Two".into() }]
    }
    fn arsenals() -> Vec<ArsenalOption> {
        vec![ArsenalOption { name: "A".into(), skills: vec![1, 2, 3], kind: None }, ArsenalOption { name: "B".into(), skills: vec![4, 5], kind: None }]
    }

    fn pump_both(h: &mut Lobby, hs: &mut NetSession, c: &mut Lobby, cs: &mut NetSession, ctx: &LobbyCtx) -> (Pumped, Pumped) {
        let mut hp = Pumped::default();
        let mut cp = Pumped::default();
        for _ in 0..4 {
            let a = h.pump(hs, ctx);
            let b = c.pump(cs, ctx);
            hp.connected |= a.connected;
            cp.connected |= b.connected;
            hp.start = hp.start.or(a.start);
            cp.start = cp.start.or(b.start);
        }
        (hp, cp)
    }

    #[test]
    fn lobby_exchanges_picks_then_host_starts_both() {
        let (chars, stages, arsenals) = (chars(), stages(), arsenals());
        let ctx = LobbyCtx { chars: &chars, stages: &stages, arsenals: &arsenals };
        let (mut hs, mut cs) = NetSession::loopback_pair(7);
        let (mut h, mut c) = (Lobby::new(), Lobby::new());
        let (hp, cp) = pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        assert!(hp.connected && cp.connected);
        // The joiner sees the host's rules; the host sees the joiner's default pick.
        assert_eq!(c.remote_rules, Some(h.host_rules(&ctx)));
        assert!(h.peer.is_some() && c.peer.is_some());

        // Host changes stage and time; joiner picks another character.
        h.adjust(LobbyRow::Stage, 1, &mut hs, &ctx);
        h.adjust(LobbyRow::Time, 1, &mut hs, &ctx);
        assert!(!c.adjust(LobbyRow::Stage, 1, &mut cs, &ctx), "only the host sets rules");
        c.adjust(LobbyRow::Character, 1, &mut cs, &ctx);
        c.adjust(LobbyRow::Arsenal, 1, &mut cs, &ctx);
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        assert_eq!(h.peer.as_ref().unwrap().char, "pc01a");
        assert_eq!(h.peer.as_ref().unwrap().arsenal, vec![4, 5]);
        assert_eq!(c.remote_rules.as_ref().unwrap().stage, "st02");

        // Joiner ready alone: nothing starts. Host ready: both start.
        c.toggle_ready(&mut cs, &ctx);
        let (hp, cp) = pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        assert!(hp.start.is_none() && cp.start.is_none());
        h.toggle_ready(&mut hs, &ctx);
        let (hp, cp) = pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        let (hr, hpicks) = hp.start.expect("host starts");
        let (cr, cpicks) = cp.start.expect("joiner starts");
        assert_eq!((&hr, &hpicks), (&cr, &cpicks));

        let build = |rules: &LobbyRules, picks: &[LobbyPick; 2], slot: usize| {
            let (mut rp, mut sk, mut lv, mut ms) = (RequestedPlayers::default(), SeatKinds::default(), LocalViews::default(), MatchSetup::default());
            apply_start(rules, picks, slot, &mut rp, &mut sk, &mut lv, &mut ms).expect("same rules on both PCs");
            (rp.0.unwrap(), sk, lv, ms)
        };
        let (hpl, hk, hv, hms) = build(&hr, &hpicks, hs.my_slot());
        let (cpl, ck, cv, cms) = build(&cr, &cpicks, cs.my_slot());
        assert_eq!(hpl, cpl);
        assert_eq!(hpl[0].char, "pc00a");
        assert_eq!(hpl[1].char, "pc01a");
        assert_eq!((hpl[0].team, hpl[1].team, hpl[0].seat, hpl[1].seat), (0, 1, 0, 1));
        assert_eq!((hms.stage.as_str(), hms.time_limit_ticks, hms.chars.clone()), (cms.stage.as_str(), cms.time_limit_ticks, cms.chars.clone()));
        assert_eq!(hms.stage, "st02");
        assert_eq!(hk.0[0], SeatKind::Human);
        assert_eq!(hk.0[1], SeatKind::Remote);
        assert_eq!(ck.0[0], SeatKind::Remote);
        assert_eq!(ck.0[1], SeatKind::Human);
        assert_eq!(hv, LocalViews(Some(vec![0])));
        assert_eq!(cv, LocalViews(Some(vec![1])));
    }

    #[test]
    fn changing_a_pick_takes_the_ready_mark_back() {
        let (chars, stages, arsenals) = (chars(), stages(), arsenals());
        let ctx = LobbyCtx { chars: &chars, stages: &stages, arsenals: &arsenals };
        let (mut hs, mut cs) = NetSession::loopback_pair(0);
        let (mut h, mut c) = (Lobby::new(), Lobby::new());
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        c.toggle_ready(&mut cs, &ctx);
        c.adjust(LobbyRow::Colour, 1, &mut cs, &ctx);
        assert!(!c.ready);
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        assert!(!h.peer.as_ref().unwrap().ready);
        assert_eq!(h.peer.as_ref().unwrap().colour, 1);
    }

    #[test]
    fn a_leaving_player_closes_the_other_lobby() {
        let (chars, stages, arsenals) = (chars(), stages(), arsenals());
        let ctx = LobbyCtx { chars: &chars, stages: &stages, arsenals: &arsenals };
        let (mut hs, mut cs) = NetSession::loopback_pair(0);
        let (mut h, mut c) = (Lobby::new(), Lobby::new());
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        cs.leave();
        let p = h.pump(&mut hs, &ctx);
        assert_eq!(p.closed.as_deref(), Some("The other player left"));
    }

    #[test]
    fn address_is_remembered_and_filtered() {
        let dir = std::env::temp_dir().join(format!("pd_online_test_{}", std::process::id()));
        let path = dir.join("userdata").join("online.json");
        assert_eq!(load_address(&path), "");
        save_address(&path, "192.168.1.20:7777");
        assert_eq!(load_address(&path), "192.168.1.20:7777");
        let _ = std::fs::remove_dir_all(&dir);
        assert!("1.2.3.4:7777".chars().all(address_char_ok));
        assert!(!address_char_ok('a') && !address_char_ok(' '));
        assert!(store_path(Path::new("C:/x/engine/cache")).to_string_lossy().replace('\\', "/").ends_with("engine/userdata/online.json"));
    }

    /// Both lobbies connected and exchanged picks (the host with rules as announced).
    fn connected() -> (Lobby, NetSession, Lobby, NetSession) {
        let (chars, stages, arsenals) = (chars(), stages(), arsenals());
        let ctx = LobbyCtx { chars: &chars, stages: &stages, arsenals: &arsenals };
        let (mut hs, mut cs) = NetSession::loopback_pair(7);
        let (mut h, mut c) = (Lobby::new(), Lobby::new());
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        (h, hs, c, cs)
    }

    #[test]
    fn the_hosts_gamemode_reaches_the_joiner_read_only_and_both_start_on_the_same_rules() {
        let (chars, stages, arsenals) = (chars(), stages(), arsenals());
        let ctx = LobbyCtx { chars: &chars, stages: &stages, arsenals: &arsenals };
        let (mut h, mut hs, mut c, mut cs) = connected();
        // Both open on STANDARD.
        assert_eq!((h.shown_mode(Role::Host), c.shown_mode(Role::Client)), (None, None));
        assert_eq!(h.value(LobbyRow::Gamemode, Role::Host, &ctx), "STANDARD");
        // The host cycles to CLOSE QUARTERS; the joiner sees it, and cannot change it.
        assert!(h.adjust(LobbyRow::Gamemode, 1, &mut hs, &ctx));
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        assert_eq!(h.mode_idx, Some(0));
        assert_eq!(c.shown_mode(Role::Client), Some(0));
        assert_eq!(c.value(LobbyRow::Gamemode, Role::Client, &ctx), "CLOSE QUARTERS");
        assert!(!c.adjust(LobbyRow::Gamemode, 1, &mut cs, &ctx), "only the host picks the gamemode");
        assert_eq!(c.shown_mode(Role::Client), Some(0), "unchanged");
        assert!(!LobbyRow::Gamemode.enabled(Role::Client) && LobbyRow::Gamemode.enabled(Role::Host));
        // The announced rules carry the mode id and the Rules fingerprint.
        let announced = c.remote_rules.clone().unwrap();
        assert_eq!(announced.mode, "close_quarters");
        assert_eq!(announced.rules_hash, modes::rules_fingerprint(Some(0)));
        assert_ne!(announced.rules_hash, modes::rules_fingerprint(None));
        // A mode fixes the hand: neither side can pick an arsenal.
        assert!(!h.adjust(LobbyRow::Arsenal, 1, &mut hs, &ctx) && !c.adjust(LobbyRow::Arsenal, 1, &mut cs, &ctx));
        assert_eq!(h.value(LobbyRow::Arsenal, Role::Host, &ctx), "FIXED HAND");
        // Both ready: both start with the same rules.
        h.toggle_ready(&mut hs, &ctx);
        c.toggle_ready(&mut cs, &ctx);
        let (hp, cp) = pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        let (hr, hpicks) = hp.start.expect("host starts");
        let (cr, cpicks) = cp.start.expect("joiner starts");
        assert_eq!(hr, cr);
        assert_eq!(cr.mode, "close_quarters");
        let mut out = Vec::new();
        for (rules, picks, slot) in [(&hr, &hpicks, 0), (&cr, &cpicks, 1)] {
            let (mut rp, mut sk, mut lv, mut ms) = (RequestedPlayers::default(), SeatKinds::default(), LocalViews::default(), MatchSetup::default());
            apply_start(rules, picks, slot, &mut rp, &mut sk, &mut lv, &mut ms).expect("starts");
            let mut combat = crate::combat::Combat::with_roster(Vec::new(), crate::combat::Roster::duel(), std::array::from_fn(|_| Vec::new()));
            ms.apply_to_combat(&mut combat);
            out.push((rp.0.unwrap(), ms.mode, ms.arsenal_kinds, combat.rules.clone()));
        }
        assert_eq!(out[0], out[1], "the same players, mode and sim Rules on both PCs");
        assert_eq!(out[0].1, Some(0));
        assert!(out[0].0.iter().all(|p| p.arsenal == modes::MODES[0].hand), "the mode's hand is everyone's arsenal");
        assert_eq!(out[0].3, modes::MODES[0].rules(), "the sim runs the mode's Rules");
        assert_eq!(out[0].3.fixed_hand, Some(modes::MODES[0].hand));
    }

    #[test]
    fn standard_goes_back_to_the_normal_rules_and_the_arsenal_row() {
        let (chars, stages, arsenals) = (chars(), stages(), arsenals());
        let ctx = LobbyCtx { chars: &chars, stages: &stages, arsenals: &arsenals };
        let (mut h, mut hs, mut c, mut cs) = connected();
        for _ in 0..modes::online_choices() {
            h.adjust(LobbyRow::Gamemode, 1, &mut hs, &ctx); // through every mode and back to STANDARD
        }
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        assert_eq!(h.mode_idx, None);
        assert_eq!(c.shown_mode(Role::Client), None);
        assert_eq!(c.remote_rules.as_ref().unwrap().mode, "");
        assert!(c.adjust(LobbyRow::Arsenal, 1, &mut cs, &ctx), "the arsenal row works again");
        assert_eq!(c.value(LobbyRow::Gamemode, Role::Client, &ctx), "STANDARD");
    }

    #[test]
    fn peers_with_different_rules_refuse_to_start_instead_of_desyncing() {
        let (chars, stages, arsenals) = (chars(), stages(), arsenals());
        let ctx = LobbyCtx { chars: &chars, stages: &stages, arsenals: &arsenals };
        let picks = [LobbyPick { char: "pc00a".into(), ..Default::default() }, LobbyPick { char: "pc01a".into(), ..Default::default() }];
        let good = Lobby::new().host_rules(&ctx);
        let mut cq = good.clone();
        (cq.mode, cq.rules_hash) = modes::lobby_fields(Some(0));
        let run = |rules: &LobbyRules| {
            let (mut rp, mut sk, mut lv, mut ms) = (RequestedPlayers::default(), SeatKinds::default(), LocalViews::default(), MatchSetup::default());
            let r = apply_start(rules, &picks, 1, &mut rp, &mut sk, &mut lv, &mut ms);
            (r, rp.0.is_some(), lv)
        };
        assert!(run(&good).0.is_ok() && run(&cq).0.is_ok());
        // A host on another build: the same id means other Rules here (the fingerprint differs).
        let mut other = cq.clone();
        other.rules_hash ^= 1;
        let (r, planned, views) = run(&other);
        assert!(r.unwrap_err().contains("rules differ"), "refused with a reason");
        assert!(!planned && views == LocalViews::default(), "nothing was set up");
        // The host says STANDARD but fingerprints something else (e.g. a mode set up on one side only).
        let mut std_but = good.clone();
        std_but.rules_hash = modes::rules_fingerprint(Some(0));
        assert!(run(&std_but).0.is_err());
        // A mode this build has no entry for.
        let mut unknown = cq.clone();
        unknown.mode = "no_such_mode".into();
        assert!(run(&unknown).0.unwrap_err().contains("not in this game build"));

        // In the lobby the joiner leaves at once when the announced rules don't match its own.
        let (mut hs, mut cs) = NetSession::loopback_pair(7);
        let (mut h, mut c) = (Lobby::new(), Lobby::new());
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        hs.send(NetMsg::Rules(other.clone()));
        let p = c.pump(&mut cs, &ctx);
        assert!(p.closed.as_deref().is_some_and(|r| r.contains("rules differ")), "{:?}", p.closed);
        assert!(matches!(cs.status, crate::net::Status::Closed(_)), "the joiner's session is closed: no match starts");
    }

    #[test]
    fn a_mismatched_start_is_refused_by_the_joiner_and_host_alike() {
        let (chars, stages, arsenals) = (chars(), stages(), arsenals());
        let ctx = LobbyCtx { chars: &chars, stages: &stages, arsenals: &arsenals };
        let (mut h, mut hs, mut c, mut cs) = connected();
        h.adjust(LobbyRow::Gamemode, 1, &mut hs, &ctx);
        pump_both(&mut h, &mut hs, &mut c, &mut cs, &ctx);
        // A host build whose Close Quarters differs sends Start with its own fingerprint.
        let mut bad = h.host_rules(&ctx);
        bad.rules_hash = bad.rules_hash.wrapping_add(1);
        let picks = [h.my_pick(&ctx), c.my_pick(&ctx)];
        let (mut rp, mut sk, mut lv, mut ms) = (RequestedPlayers::default(), SeatKinds::default(), LocalViews::default(), MatchSetup::default());
        assert!(apply_start(&bad, &picks, 1, &mut rp, &mut sk, &mut lv, &mut ms).is_err());
        assert!(apply_start(&bad, &picks, 0, &mut rp, &mut sk, &mut lv, &mut ms).is_err(), "the host checks its own fingerprint too");
        assert!(rp.0.is_none(), "no players were queued for a match");
    }
}
