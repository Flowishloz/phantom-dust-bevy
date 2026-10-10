//! Steam online play (docs/ONLINE-PLAN.md "Steam"): App ID 480 ("Spacewar",
//! Valve's public test app), lobbies to meet, Steam Networking Messages to
//! talk. Same `NetSession` / lockstep / lobby screens as LAN; only the pipe
//! differs.
//!
//! - [`SteamNet`] (Bevy resource, always present): the Steam client, or the
//!   reason Steam isn't available (not running, built without the `steam`
//!   feature). [`pump`] runs Steam's callbacks every frame.
//! - Lobbies: [`SteamNet::create_lobby`] (friends-only by default, max 2,
//!   tagged `game=pd_rebuild`, `build=<net::build_id()>`, `host=<persona>`),
//!   [`SteamNet::refresh_lobbies`] (public lobbies with our tags + lobbies
//!   friends are in), [`SteamNet::join_lobby`], overlay invites, and "join"
//!   from an accepted Steam invite / `+connect_lobby <id>` launch argument
//!   ([`SteamNet::take_invite`]). App 480 is shared with every Spacewar
//!   tester in the world: a lobby is shown only if its data carries our tags
//!   ([`listing_from`]).
//! - [`SteamTransport`]: `net::Transport` over `ISteamNetworkingMessages`
//!   (channel 0; `wire` encoding; the messages UDP sends reliably go
//!   reliable). The lobby owner is the host. A peer that leaves the lobby
//!   reads as `NetMsg::Bye` ("The other player left"); dropping the transport
//!   leaves the lobby.
//!
//! Nothing here is read by the sim (determinism rule).

#[cfg(feature = "steam")]
use super::wire;
use super::{build_id, data_hash, NetSession, Role};
use bevy::prelude::*;
use std::collections::BTreeMap;

/// Valve's public test app ("Spacewar").
pub const APP_ID: u32 = 480;
/// Lobby data keys and our tag value.
pub const KEY_GAME: &str = "game";
pub const KEY_BUILD: &str = "build";
pub const KEY_HOST: &str = "host";
pub const GAME_TAG: &str = "pd_rebuild";
/// 1-v-1: the host and one friend.
pub const MAX_MEMBERS: usize = 2;

/// Who can see a hosted lobby.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    /// Friends see it on Find (through "playing Spacewar") and by invite.
    #[default]
    FriendsOnly,
    /// Also in the public lobby list (anyone running this same build).
    Public,
}

impl Visibility {
    pub fn label(self) -> &'static str {
        match self {
            Visibility::FriendsOnly => "FRIENDS ONLY",
            Visibility::Public => "PUBLIC",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Visibility::FriendsOnly => Visibility::Public,
            Visibility::Public => Visibility::FriendsOnly,
        }
    }
}

/// One lobby on the Find page.
#[derive(Clone, Debug, PartialEq)]
pub struct LobbyListing {
    pub id: u64,
    /// The host's Steam name (from the lobby's `host` tag).
    pub host: String,
    pub members: usize,
    /// Found through a friend (rather than the public list).
    pub friend: bool,
    /// Hosted by this Steam account (a second copy of the game on this PC).
    pub mine: bool,
}

/// Is this lobby one of ours, from the same build? `data` reads a lobby
/// data key. Untagged lobbies (other Spacewar testers) never pass.
pub fn lobby_is_ours(data: impl Fn(&str) -> Option<String>, build: &str) -> bool {
    data(KEY_GAME).as_deref() == Some(GAME_TAG) && data(KEY_BUILD).as_deref() == Some(build)
}

/// The listing for lobby `id`, or `None` if it isn't ours / is full /
/// its data hasn't arrived.
pub fn listing_from(id: u64, data: impl Fn(&str) -> Option<String>, members: usize, friend: bool, owner_is_me: bool, build: &str) -> Option<LobbyListing> {
    if id == 0 || !lobby_is_ours(&data, build) || members >= MAX_MEMBERS {
        return None;
    }
    let host = data(KEY_HOST).filter(|h| !h.trim().is_empty()).unwrap_or_else(|| "(unnamed)".into());
    Some(LobbyListing { id, host, members, friend, mine: owner_is_me })
}

/// Listings sorted for the screen: friends first, then by host name; one
/// entry per lobby (a friend's lobby can also be in the public list).
pub fn sort_listings(mut v: Vec<LobbyListing>) -> Vec<LobbyListing> {
    v.sort_by(|a, b| b.friend.cmp(&a.friend).then_with(|| a.host.to_lowercase().cmp(&b.host.to_lowercase())).then(a.id.cmp(&b.id)));
    let mut seen = std::collections::BTreeSet::new();
    v.retain(|l| seen.insert(l.id));
    v
}

/// `+connect_lobby <id>` (what Steam passes when it launches a game to join
/// a lobby).
pub fn connect_lobby_arg(args: &[String]) -> Option<u64> {
    let i = args.iter().position(|a| a == "+connect_lobby")?;
    args.get(i + 1)?.trim().parse().ok().filter(|&id| id != 0)
}

/// What the screens hear back from Steam (`SteamNet::take_events`).
#[derive(Clone, Debug, PartialEq)]
pub enum SteamEvent {
    /// Our lobby exists (tagged and ready to invite into).
    Created(Result<u64, String>),
    /// We are in lobby `.0` (as the joiner).
    Joined(Result<u64, String>),
    /// The lobby search finished (`lobbies()` has the result).
    ListReady,
}

/// What we're waiting on from Steam. A result for anything else (the player
/// backed out meanwhile) is dropped and its lobby left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pending {
    #[default]
    Nothing,
    Create,
    Join(u64),
}

/// What to do with an async lobby result given what we're waiting on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accept {
    /// Hand it to the screen.
    Use,
    /// Stale: we got into a lobby nobody wants any more; leave it.
    Leave,
    /// Stale failure: ignore.
    Ignore,
}

pub fn accept_result(pending: Pending, created: bool, id: Option<u64>) -> Accept {
    let wanted = match pending {
        Pending::Create => created,
        Pending::Join(want) => !created && id.is_none_or(|i| i == want),
        Pending::Nothing => false,
    };
    match (wanted, id) {
        (true, _) => Accept::Use,
        (false, Some(_)) => Accept::Leave,
        (false, None) => Accept::Ignore,
    }
}

/// Shared between Steam's callbacks (run inside `run_callbacks`) and the game.
#[derive(Default)]
struct Shared {
    /// Async results: (created?, Ok(lobby) / Err(reason)).
    results: Vec<(bool, Result<u64, String>)>,
    list: Option<Result<Vec<u64>, String>>,
}

#[derive(Resource)]
pub struct SteamNet {
    #[cfg(feature = "steam")]
    client: Option<steamworks::Client>,
    #[cfg(feature = "steam")]
    shared: std::sync::Arc<std::sync::Mutex<Shared>>,
    /// Lobbies whose member left: (lobby, user). Read by `SteamTransport`.
    #[cfg(feature = "steam")]
    left: std::sync::Arc<std::sync::Mutex<std::collections::BTreeSet<(u64, u64)>>>,
    /// Why Steam can't be used (shown on the greyed Steam rows).
    pub unavailable: Option<String>,
    /// This account's Steam name.
    pub persona: String,
    pending: Pending,
    /// Lobby ids found by the last search, with "found through a friend".
    found: BTreeMap<u64, bool>,
    pub searching: bool,
    pub list_error: Option<String>,
    events: Vec<SteamEvent>,
    /// An accepted Steam invite / launch-argument lobby to join.
    invite: Option<u64>,
}

impl SteamNet {
    /// Steam that isn't there (`reason` shown to the player).
    pub fn unavailable(reason: &str) -> Self {
        SteamNet {
            #[cfg(feature = "steam")]
            client: None,
            #[cfg(feature = "steam")]
            shared: Default::default(),
            #[cfg(feature = "steam")]
            left: Default::default(),
            unavailable: Some(reason.into()),
            persona: String::new(),
            pending: Pending::Nothing,
            found: BTreeMap::new(),
            searching: false,
            list_error: None,
            events: Vec::new(),
            invite: None,
        }
    }

    /// Start Steam as App 480. Never fails: on error the resource says why.
    pub fn init() -> Self {
        let mut s = Self::init_client();
        if s.available() {
            let args: Vec<String> = std::env::args().collect();
            s.invite = connect_lobby_arg(&args);
        }
        s
    }

    #[cfg(not(feature = "steam"))]
    fn init_client() -> Self {
        Self::unavailable("This build has no Steam support")
    }

    #[cfg(feature = "steam")]
    fn init_client() -> Self {
        // Test switch: behave as if Steam weren't there.
        if std::env::var_os("PD_NO_STEAM").is_some() {
            return Self::unavailable("Steam is turned off (PD_NO_STEAM)");
        }
        // Steam's DLL can crash on a broken install; keep the game alive.
        let client = match std::panic::catch_unwind(|| steamworks::Client::init_app(APP_ID)) {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                let why = e.to_string();
                info!("Steam not available: {why}");
                return Self::unavailable(&steam_reason(&why));
            }
            Err(_) => return Self::unavailable("Steam failed to start"),
        };
        // Accept every Networking Messages session; `SteamTransport` only
        // listens to its lobby peer.
        client.networking_messages().session_request_callback(|req| {
            req.accept();
        });
        client.networking_utils().init_relay_network_access();
        let persona = client.friends().name();
        info!("Steam ready: {persona} (App {APP_ID})");
        SteamNet { client: Some(client), unavailable: None, persona, ..Self::unavailable("") }
    }

    pub fn available(&self) -> bool {
        self.unavailable.is_none()
    }

    /// Screens: async results since the last call.
    pub fn take_events(&mut self) -> Vec<SteamEvent> {
        std::mem::take(&mut self.events)
    }

    /// An accepted invite (or `+connect_lobby`) waiting to be joined.
    pub fn take_invite(&mut self) -> Option<u64> {
        self.invite.take()
    }

    pub fn has_invite(&self) -> bool {
        self.invite.is_some()
    }

    /// Forget whatever lobby result we were waiting for (player backed out).
    pub fn cancel(&mut self) {
        self.pending = Pending::Nothing;
    }

    pub fn pending(&self) -> Pending {
        self.pending
    }

    /// Lobbies from the last search that are ours and not full.
    pub fn lobbies(&self) -> Vec<LobbyListing> {
        #[cfg(feature = "steam")]
        if let Some(c) = &self.client {
            let mm = c.matchmaking();
            let me = c.user().steam_id();
            let build = build_id();
            let v = self
                .found
                .iter()
                .filter_map(|(&id, &friend)| {
                    let lobby = steamworks::LobbyId::from_raw(id);
                    listing_from(id, |k| mm.lobby_data(lobby, k), mm.lobby_member_count(lobby), friend, mm.lobby_owner(lobby) == me, &build)
                })
                .collect();
            return sort_listings(v);
        }
        Vec::new()
    }
}

/// A shorter player-facing reason for an init failure.
pub fn steam_reason(err: &str) -> String {
    let e = err.to_lowercase();
    if e.contains("not running") || e.contains("no steam client") || e.contains("cannot create ipc pipe") {
        "Steam is not running".into()
    } else if e.contains("steam_appid") || e.contains("app id") || e.contains("appid") {
        "Steam could not tell which app this is (steam_appid.txt)".into()
    } else {
        format!("Steam: {}", err.trim())
    }
}

#[cfg(feature = "steam")]
mod imp {
    use super::*;
    use steamworks::{CallbackResult, ChatMemberStateChange, LobbyId, LobbyKey, LobbyType, SteamId, StringFilter, StringFilterKind};

    impl SteamNet {
        pub fn client(&self) -> Option<&steamworks::Client> {
            self.client.as_ref()
        }

        /// Create our lobby; `SteamEvent::Created` follows.
        pub fn create_lobby(&mut self, vis: Visibility) {
            let Some(c) = &self.client else { return };
            self.pending = Pending::Create;
            let shared = self.shared.clone();
            c.matchmaking().create_lobby(lobby_type(vis), MAX_MEMBERS as u32, move |r| {
                shared.lock().unwrap().results.push((true, r.map(|l| l.raw()).map_err(|e| format!("Could not create the lobby ({e})"))));
            });
        }

        /// Change a hosted lobby's visibility.
        pub fn set_visibility(&self, lobby: u64, vis: Visibility) {
            if self.client.is_none() {
                return;
            }
            let ty = match vis {
                Visibility::FriendsOnly => steamworks_sys::ELobbyType::k_ELobbyTypeFriendsOnly,
                Visibility::Public => steamworks_sys::ELobbyType::k_ELobbyTypePublic,
            };
            // SAFETY: Steam is initialised (we have a client); plain C call.
            unsafe {
                steamworks_sys::SteamAPI_ISteamMatchmaking_SetLobbyType(steamworks_sys::SteamAPI_SteamMatchmaking_v009(), lobby, ty);
            }
        }

        /// Join lobby `id`; `SteamEvent::Joined` follows.
        pub fn join_lobby(&mut self, id: u64) {
            let Some(c) = &self.client else { return };
            self.pending = Pending::Join(id);
            let shared = self.shared.clone();
            c.matchmaking().join_lobby(LobbyId::from_raw(id), move |r| {
                shared.lock().unwrap().results.push((false, r.map(|l| l.raw()).map_err(|_| "Could not join the lobby (it is full or closed)".to_string())));
            });
        }

        /// Search: public lobbies tagged as ours (same build), plus the
        /// lobbies friends playing App 480 are in (friends-only lobbies are
        /// never in the public list). `SteamEvent::ListReady` follows.
        pub fn refresh_lobbies(&mut self) {
            let Some(c) = &self.client else { return };
            let mm = c.matchmaking();
            let build = build_id();
            self.found.clear();
            self.searching = true;
            self.list_error = None;
            for f in c.friends().get_friends(steamworks::FriendFlags::IMMEDIATE) {
                if let Some(g) = f.game_played() {
                    if g.game.app_id() == steamworks::AppId(APP_ID) && g.lobby.raw() != 0 {
                        self.found.insert(g.lobby.raw(), true);
                        // SAFETY: Steam is initialised; plain C call. Data
                        // arrives with a LobbyDataUpdate, `lobbies()` reads it.
                        unsafe {
                            steamworks_sys::SteamAPI_ISteamMatchmaking_RequestLobbyData(steamworks_sys::SteamAPI_SteamMatchmaking_v009(), g.lobby.raw());
                        }
                    }
                }
            }
            mm.add_request_lobby_list_string_filter(StringFilter(LobbyKey::new(KEY_GAME), GAME_TAG, StringFilterKind::Equal));
            mm.add_request_lobby_list_string_filter(StringFilter(LobbyKey::new(KEY_BUILD), &build, StringFilterKind::Equal));
            mm.set_request_lobby_list_distance_filter(steamworks::DistanceFilter::Worldwide);
            mm.set_request_lobby_list_result_count_filter(50);
            let shared = self.shared.clone();
            mm.request_lobby_list(move |r| {
                shared.lock().unwrap().list = Some(r.map(|v| v.iter().map(|l| l.raw()).collect()).map_err(|e| format!("Lobby search failed ({e})")));
            });
        }

        /// Steam overlay: invite a friend into `lobby`.
        pub fn invite_dialog(&self, lobby: u64) {
            if let Some(c) = &self.client {
                c.friends().activate_invite_dialog(LobbyId::from_raw(lobby));
            }
        }

        /// Is the overlay usable (Steam may block it for non-Steam-launched exes)?
        pub fn overlay_enabled(&self) -> bool {
            self.client.as_ref().is_some_and(|c| c.utils().is_overlay_enabled())
        }

        /// Owner of a lobby we're in.
        pub fn lobby_owner(&self, lobby: u64) -> Option<u64> {
            let c = self.client.as_ref()?;
            Some(c.matchmaking().lobby_owner(LobbyId::from_raw(lobby)).raw()).filter(|&o| o != 0)
        }

        pub fn my_id(&self) -> Option<u64> {
            self.client.as_ref().map(|c| c.user().steam_id().raw())
        }

        /// Tag our new lobby so Find (and only Find for this build) sees it.
        fn tag_lobby(&self, id: u64) {
            let Some(c) = &self.client else { return };
            let mm = c.matchmaking();
            let lobby = LobbyId::from_raw(id);
            mm.set_lobby_data(lobby, KEY_GAME, GAME_TAG);
            mm.set_lobby_data(lobby, KEY_BUILD, &build_id());
            mm.set_lobby_data(lobby, KEY_HOST, &self.persona);
            mm.set_lobby_joinable(lobby, true);
        }

        /// Leave a lobby we are in (but no session owns).
        pub fn leave_lobby(&self, id: u64) {
            if let Some(c) = &self.client {
                c.matchmaking().leave_lobby(LobbyId::from_raw(id));
            }
        }

        /// The transport for a session in `lobby` (we're already in it).
        pub fn transport(&self, lobby: u64, role: Role) -> Option<SteamTransport> {
            let c = self.client.clone()?;
            let me = c.user().steam_id();
            let peer = match role {
                Role::Host => None,
                Role::Client => Some(c.matchmaking().lobby_owner(LobbyId::from_raw(lobby))).filter(|o| o.raw() != 0 && *o != me),
            };
            if role == Role::Client && peer.is_none() {
                return None;
            }
            // Drop anything left over from an earlier session.
            while !c.networking_messages().receive_messages_on_channel(0, 64).is_empty() {}
            Some(SteamTransport { client: c, lobby: LobbyId::from_raw(lobby), me, peer, left: self.left.clone() })
        }

        pub(super) fn run(&mut self) {
            let Some(c) = self.client.clone() else { return };
            let mut invite = None;
            let left = self.left.clone();
            c.process_callbacks(|cb| match cb {
                CallbackResult::GameLobbyJoinRequested(j) => invite = Some(j.lobby_steam_id.raw()),
                CallbackResult::GameOverlayActivated(o) => info!("Steam overlay {}", if o.active { "opened" } else { "closed" }),
                CallbackResult::LobbyChatUpdate(u) if matches!(u.member_state_change, ChatMemberStateChange::Entered) => info!("Steam lobby {}: member {} joined", u.lobby.raw(), u.user_changed.raw()),
                CallbackResult::LobbyChatUpdate(u) => {
                    if !matches!(u.member_state_change, ChatMemberStateChange::Entered) {
                        left.lock().unwrap().insert((u.lobby.raw(), u.user_changed.raw()));
                    }
                }
                _ => {}
            });
            if let Some(id) = invite {
                info!("Steam invite accepted: lobby {id}");
                self.invite = Some(id);
            }
            let (results, list) = {
                let mut s = self.shared.lock().unwrap();
                (std::mem::take(&mut s.results), s.list.take())
            };
            for (created, r) in results {
                match accept_result(self.pending, created, r.as_ref().ok().copied()) {
                    Accept::Use => {
                        self.pending = Pending::Nothing;
                        if let (true, Ok(id)) = (created, &r) {
                            self.tag_lobby(*id);
                        }
                        self.events.push(if created { SteamEvent::Created(r) } else { SteamEvent::Joined(r) });
                    }
                    Accept::Leave => {
                        if let Ok(id) = r {
                            self.leave_lobby(id);
                        }
                    }
                    Accept::Ignore => {}
                }
            }
            if let Some(list) = list {
                self.searching = false;
                match list {
                    Ok(ids) => {
                        for id in ids {
                            self.found.entry(id).or_insert(false);
                        }
                    }
                    Err(e) => self.list_error = Some(e),
                }
                self.events.push(SteamEvent::ListReady);
            }
        }
    }

    fn lobby_type(vis: Visibility) -> LobbyType {
        match vis {
            Visibility::FriendsOnly => LobbyType::FriendsOnly,
            Visibility::Public => LobbyType::Public,
        }
    }

    /// `net::Transport` over Steam Networking Messages, to the one other
    /// member of a lobby. Owns our place in the lobby: dropping it leaves.
    pub struct SteamTransport {
        client: steamworks::Client,
        lobby: LobbyId,
        me: SteamId,
        /// The host learns its peer from the first Hello sent by a lobby member.
        peer: Option<SteamId>,
        left: std::sync::Arc<std::sync::Mutex<std::collections::BTreeSet<(u64, u64)>>>,
    }

    impl super::super::Transport for SteamTransport {
        fn send(&mut self, msg: &super::super::NetMsg) {
            let Some(peer) = self.peer else { return };
            let reliable = super::super::udp::is_reliable(msg) || *msg == super::super::NetMsg::Bye;
            let flags = if reliable { steamworks::networking_types::SendFlags::RELIABLE_NO_NAGLE } else { steamworks::networking_types::SendFlags::UNRELIABLE_NO_NAGLE } | steamworks::networking_types::SendFlags::AUTO_RESTART_BROKEN_SESSION;
            let bytes = wire::encode(msg);
            // Errors (no route yet, session broken) lose this message, like a
            // dropped datagram; the session repeats what matters.
            let _ = self.client.networking_messages().send_message_to_user(steamworks::networking_types::NetworkingIdentity::new_steam_id(peer), flags, &bytes, 0);
        }

        fn recv(&mut self) -> Vec<super::super::NetMsg> {
            let mut out = Vec::new();
            let nm = self.client.networking_messages();
            for _ in 0..64 {
                let batch = nm.receive_messages_on_channel(0, 64);
                if batch.is_empty() {
                    break;
                }
                for m in batch {
                    let Some(from) = m.identity_peer().steam_id() else { continue };
                    let Some(msg) = wire::decode(m.data()) else { continue };
                    match self.peer {
                        Some(p) if p == from => out.push(msg),
                        Some(_) => {}
                        None => {
                            let member = from != self.me && self.client.matchmaking().lobby_members(self.lobby).contains(&from);
                            if member && matches!(msg, super::super::NetMsg::Hello { .. }) {
                                self.peer = Some(from);
                                out.push(msg);
                            }
                        }
                    }
                }
            }
            if let Some(p) = self.peer {
                if self.left.lock().unwrap().contains(&(self.lobby.raw(), p.raw())) {
                    out.push(super::super::NetMsg::Bye);
                }
            }
            out
        }
    }

    impl Drop for SteamTransport {
        fn drop(&mut self) {
            self.client.matchmaking().leave_lobby(self.lobby);
        }
    }
}

#[cfg(feature = "steam")]
#[allow(unused_imports)]
pub use imp::SteamTransport;

#[cfg(not(feature = "steam"))]
impl SteamNet {
    pub fn create_lobby(&mut self, _vis: Visibility) {}
    pub fn set_visibility(&self, _lobby: u64, _vis: Visibility) {}
    pub fn join_lobby(&mut self, _id: u64) {}
    pub fn leave_lobby(&self, _id: u64) {}
    pub fn refresh_lobbies(&mut self) {}
    pub fn invite_dialog(&self, _lobby: u64) {}
    pub fn overlay_enabled(&self) -> bool {
        false
    }
    pub fn lobby_owner(&self, _lobby: u64) -> Option<u64> {
        None
    }
    pub fn my_id(&self) -> Option<u64> {
        None
    }
    fn run(&mut self) {}
}

/// A session in Steam lobby `lobby` that we have created (host) or joined
/// (client). `None` if Steam is gone or the lobby has no other owner.
pub fn open_session(steam: &SteamNet, lobby: u64, role: Role, cache: &std::path::Path) -> Option<NetSession> {
    #[cfg(feature = "steam")]
    {
        let t = steam.transport(lobby, role)?;
        Some(NetSession::new(role, Box::new(t), build_id(), data_hash(cache)))
    }
    #[cfg(not(feature = "steam"))]
    {
        let _ = (steam, lobby, role, cache, build_id, data_hash);
        None
    }
}

/// Every frame, before the screens: Steam's callbacks and async results.
pub fn pump(mut steam: ResMut<SteamNet>) {
    if steam.available() {
        steam.run();
    }
}

/// Starts Steam (once) and pumps it every frame, on every screen.
pub fn plugin(app: &mut App) {
    app.insert_resource(SteamNet::init()).add_systems(PreUpdate, pump);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data<'a>(kv: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| kv.iter().find(|(a, _)| *a == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn only_tagged_lobbies_of_this_build_are_listed() {
        let b = "pd_game 0.1.0 net1";
        let ours = [(KEY_GAME, GAME_TAG), (KEY_BUILD, b), (KEY_HOST, "Ann")];
        let l = listing_from(5, data(&ours), 1, false, false, b).unwrap();
        assert_eq!((l.id, l.host.as_str(), l.members, l.friend, l.mine), (5, "Ann", 1, false, false));
        // Spacewar testers' lobbies: no tags, wrong game, other build.
        assert!(listing_from(5, data(&[]), 1, false, false, b).is_none());
        assert!(listing_from(5, data(&[(KEY_GAME, "spacewar"), (KEY_BUILD, b)]), 1, false, false, b).is_none());
        assert!(listing_from(5, data(&[(KEY_GAME, GAME_TAG), (KEY_BUILD, "pd_game 0.0.9 net1")]), 1, false, false, b).is_none());
        assert!(listing_from(5, data(&[(KEY_GAME, GAME_TAG)]), 1, false, false, b).is_none(), "no build tag");
        // Full, or no id.
        assert!(listing_from(5, data(&ours), MAX_MEMBERS, false, false, b).is_none());
        assert!(listing_from(0, data(&ours), 1, false, false, b).is_none());
        // A missing host name still lists.
        assert_eq!(listing_from(6, data(&ours[..2]), 1, true, true, b).unwrap().host, "(unnamed)");
    }

    #[test]
    fn listings_sort_friends_first_and_dedupe() {
        let l = |id, host: &str, friend| LobbyListing { id, host: host.into(), members: 1, friend, mine: false };
        let v = sort_listings(vec![l(1, "zed", false), l(2, "Bob", false), l(3, "amy", true), l(2, "Bob", true)]);
        assert_eq!(v.iter().map(|x| (x.id, x.friend)).collect::<Vec<_>>(), vec![(3, true), (2, true), (1, false)]);
    }

    #[test]
    fn connect_lobby_launch_argument() {
        let a = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(connect_lobby_arg(&a(&["pd_game.exe", "cache", "+connect_lobby", "109775241000000001"])), Some(109775241000000001));
        assert_eq!(connect_lobby_arg(&a(&["pd_game.exe", "+connect_lobby"])), None);
        assert_eq!(connect_lobby_arg(&a(&["pd_game.exe", "+connect_lobby", "x"])), None);
        assert_eq!(connect_lobby_arg(&a(&["pd_game.exe", "+connect_lobby", "0"])), None);
        assert_eq!(connect_lobby_arg(&a(&["pd_game.exe", "cache"])), None);
    }

    #[test]
    fn stale_lobby_results_are_left_not_shown() {
        // Waiting to create: our creation is used; a late join result is left.
        assert_eq!(accept_result(Pending::Create, true, Some(9)), Accept::Use);
        assert_eq!(accept_result(Pending::Create, true, None), Accept::Use, "the failure is shown");
        assert_eq!(accept_result(Pending::Create, false, Some(9)), Accept::Leave);
        // Joining 7: only 7 (or its failure) counts.
        assert_eq!(accept_result(Pending::Join(7), false, Some(7)), Accept::Use);
        assert_eq!(accept_result(Pending::Join(7), false, None), Accept::Use);
        assert_eq!(accept_result(Pending::Join(7), false, Some(8)), Accept::Leave);
        assert_eq!(accept_result(Pending::Join(7), true, Some(8)), Accept::Leave);
        // Backed out: a lobby that still arrives is left, a failure ignored.
        assert_eq!(accept_result(Pending::Nothing, true, Some(3)), Accept::Leave);
        assert_eq!(accept_result(Pending::Nothing, false, None), Accept::Ignore);
    }

    #[test]
    fn init_failures_read_plainly() {
        assert_eq!(steam_reason("Steam is not running"), "Steam is not running");
        assert!(steam_reason("Could not determine the app ID (steam_appid.txt)").contains("steam_appid.txt"));
        assert_eq!(steam_reason("weird"), "Steam: weird");
        let s = SteamNet::unavailable("Steam is not running");
        assert!(!s.available());
        assert!(s.lobbies().is_empty());
    }

    #[test]
    fn visibility_defaults_to_friends_only() {
        assert_eq!(Visibility::default(), Visibility::FriendsOnly);
        assert_eq!(Visibility::default().toggled(), Visibility::Public);
    }
}
