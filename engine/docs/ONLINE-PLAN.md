# Online versus: plan (single-screen view, Online menu, LAN + Steam 480)

**Status:** MERGED INTO MAIN 2026-10-03 (lane `online`, see "Merged into main" at the end); the text below is the original plan plus each lane's "as built" notes. (Was: plan only, nothing built.) Written 2026-10-02 on branch
`devtools-bug-report` (base `main` 92cd08b). **Testing policy:** basic checks
only while building (build, unit tests, scripted screenshots, two copies of
the game on one PC). The **full review** (independent critics, see "Review
gate") runs only **after** a person has played it over a real network.

## Goal

Two people on two PCs play a 1-v-1 match: each sees **only their own
fighter's view, full-screen**. They meet through a new **Online** item on the
title menu, over either:

- **LAN**: direct UDP, host and join by IP address. No accounts, no extra
  software. Built first: it's the cheapest way to prove the netcode.
- **Steam**: App ID **480** ("Spacewar", Valve's public test app). Host makes
  a Steam lobby, friend joins through the Steam overlay invite, traffic goes
  through Steam's relays (works behind home routers). Same netcode, different
  pipe.

Out of scope for this plan: 3-4 player online, matchmaking with strangers,
spectators, a real Steam App ID, voice chat.

## What the code already gives us (checked 2026-10-02)

- **Deterministic sim**: `sim.rs` / `combat.rs` / `moves/` take quantised
  `Input` (stick i8×2, buttons, `switch_target`) at fixed 60 Hz and never read
  time, rendering, or unseeded randomness. This is what online needs.
- **`views.rs`** already has `Kind::Full` (one view fills the window), but the
  layout is chosen from **`roster.count()`** (how many fighters), so a
  2-fighter match always splits the screen. Online needs "how many views on
  THIS machine" separate from "how many fighters".
- **Seats** (`seats.rs`): `SeatKind::{Human, Cpu}` per sim slot. Online adds
  `Remote`: a slot whose inputs come from the network.
- **CPU seats** already write `PendingInputs` from a FixedUpdate system right
  before `step_sim`, which is exactly where network inputs will plug in.
- **Dev tools** (`devtools.rs`): F1 overlay, F2/F3 pause/step, F8 bug report
  with input history. A desync checksum line is a natural addition.
- **Not there yet**: no `Match::checksum()`, no input serialisation, no
  network code. There are about 35 `sin`/`cos`/`atan2` calls in the sim. They're
  fine between two copies of the same .exe on x86-64 PCs. They're a
  cross-machine risk only if the maths library differs, so pin them via `libm`.
- **Rollback is not attempted in this plan** (see decision D2).

## Decisions (recommended defaults; change before launch if you disagree)

| # | Question | Recommendation | Why |
|---|---|---|---|
| D1 | LAN or Steam first? | **LAN first, Steam second, both behind one `Transport` interface** | LAN has no linking risk; Steam's `steamworks` crate is unproven on this MinGW/GNU toolchain. A spike decides early. |
| D2 | Netcode model | **`ggrs` with prediction window 0 (= delay-based lockstep), input delay 3 ticks** | Lockstep never rewinds, so sounds/effects/animation need no rollback handling. `ggrs` is the rollback library the docs already chose. Its prediction window can be raised later for true rollback without rewriting. |
| D3 | Who decides the match? | **Host picks stage/rules; each player picks own character + arsenal; both "Ready"** | Mirrors the versus setup screen. |
| D4 | Same build + same data check | **Before the match, exchange: build string + hash of `skills.json`, stage `.pdcl`, both `chars/*.glb` clip tables** | A mismatch desyncs silently otherwise. Refuse with a clear message. |
| D5 | Desync handling | **Exchange `Match::checksum()` every 60 ticks; on mismatch, end the match with "Out of sync" + auto F8-style report on both PCs** | Bugs become reportable instead of "it felt weird". |

## Shared contracts (Wave 0: one agent, serial, small; everything else waits)

Done first, on `main` of the work branch, so the parallel lanes don't fight
over `main.rs`:

1. `views::LocalViews` resource: the list of sim slots viewed on this machine
   (default = all active slots, so local play is unchanged). Every
   `ViewLayout::for_players(roster.count(), ..)` call (main.rs ×3, hud.rs)
   switches to `local_views.len()`, and per-view loops map view index →
   slot. **No visible change in local play**: tests pin it.
2. `seats::SeatKind::Remote`: no behaviour yet.
3. `sim::Input::{to_bytes, from_bytes}` (fixed 8 bytes) + round-trip test.
4. `Match::checksum() -> u64`: FNV-1a over `to_bits()` of every float and
   every int/enum of `Match` + `Combat` state, in a fixed field order (design
   in `DESIGN-REPLAYS.md`). Test: two sims fed the same inputs agree every tick.
5. `AppState::Online` (empty screen) + `net` module skeleton:
   `trait Transport { send(peer, &[u8]); recv() -> Vec<(peer, Vec<u8>)>; }` and
   a `Loopback` transport for tests.
6. Update `LANE-BRIEF.md` (base commit, test count, file fences below).

Basic test: `cargo test -p pd_game` green; a local 2-player match screenshot is
unchanged.

## Wave 1: four parallel lanes (one git worktree each)

File fences: a lane edits only its files; anything else is a note to the
orchestrator.

| Lane | Model | Owns (may edit) | Must not touch |
|---|---|---|---|
| **A: Single screen** | Sonnet | `views.rs`, `hud.rs`, `hud_layout.rs`, `hud_marker.rs`, `hud_dial.rs`, the camera/viewport fns in `main.rs` (`layout_viewports`, `apply_fov`, `follow_cameras`, camera spawn) | sim, frontend, net |
| **B: Online menu** | Sonnet | `frontend/title.rs` (one row), new `frontend/online.rs`, `frontend/state.rs` (Online state data only) | sim, views, net internals |
| **C: Determinism hardening** | Opus | `sim.rs`, `combat.rs`, `combat/`, `moves/`, `props.rs` (math only), `Cargo.toml` (`libm`) | rendering, frontend |
| **D: Netcode + transports** | Opus | new `net/` module (session, `ggrs` glue, `udp.rs`, `steam.rs`), `Cargo.toml` (`ggrs`, `steamworks` behind a `steam` feature) | sim rules, UI |

### Lane A: good single-screen view for one local player

- A 2-fighter match with `LocalViews = [my_slot]`: one full-window camera for
  my fighter. The opponent's camera isn't spawned, or is disabled.
- HUD in full-screen mode: my health/aura/dial at the original's single-player
  positions (the original game IS single-screen per console; use its own
  layout from `BtMenuEn.alr`, `HUD.md`). The opponent appears only as the
  lock-on marker and the small opponent health panel.
- Use the full-screen lock-on marker set (`hud_marker::Set::Full`), FOV from
  settings without the split-screen squeeze, and the near-wall dissolve for
  one view.
- Test hook: `--view-seat N` (show only slot N full-screen in an otherwise
  normal local match; the other seat is driven by `--demo`/`--cpu`).
- **Basic test**: screenshots of `--view-seat 0` and `--view-seat 1` at 16:9
  and 21:9; local 2P split-screen screenshot unchanged vs before.

### Lane B: Online menu item

- Title menu gets **ONLINE** (between Versus and Arsenal), in the original's
  menu style (`MENUS.md`).
- Online screen: **Host LAN game** / **Join LAN game** (IP:port entry, last
  address remembered in `engine/userdata/`) / **Host Steam lobby** / **Join
  via Steam invite** (greyed with "Steam not running" when unavailable) /
  Back.
- Lobby screen: both players' character + arsenal pickers (reusing setup
  widgets), host-only stage/rules, Ready toggles, connection status/ping,
  "data mismatch" errors (D4). Driven through the `Transport` trait, so it
  works against `Loopback` with no network.
- **Basic test**: menu screenshots via `--menu-script`; full click path
  host→lobby→ready→match against `Loopback`.

### Lane C: determinism hardening

- Replace sim trig with `libm` (`libm::sinf/cosf/atan2f`) or unit-vector
  facing. Keep results within test tolerances (golden trace,
  `MOVEMENT-AUDIT.md` must not regress: **movement is Laurence-approved,
  protect it**).
- Tick-count timers where the sim still keeps seconds as f32 (e.g.
  `land_timer`), only where behaviour is provably identical.
- HashMap iteration audit (no order-dependent loops), `checksum()` covers every
  field added since (status, roster, CPU-visible state).
- **Basic test**: full `cargo test`; replay test: record a `--demo` match's
  inputs, replay them into a fresh `Match`, all checksums equal; golden-trace
  compare unchanged.

### Lane D: netcode + LAN + Steam 480

- `net::Session` wraps `ggrs::P2PSession` (2 players, prediction window 0,
  input delay 3, `Input` as ggrs input type via `to_bytes`). The plan
  sentence "ggrs" must be confirmed first: check the latest `ggrs` builds with
  our Rust and needs no Bevy plugin (`bevy_ggrs` likely lags Bevy 0.19; we
  don't need it).
- `udp.rs`: `std::net::UdpSocket`, non-blocking, implements ggrs'
  `NonBlockingSocket`. Host listens on a port (default 7777); join dials IP.
- `steam.rs` (feature `steam`): **step 1 is a spike**: does the `steamworks`
  crate link and run under the GNU target with `steam_api64.dll` next to the
  exe, `steam_appid.txt` = `480`? If not, record why and stop; LAN still ships.
  If yes: create/join lobby (tag lobbies with `pd_rebuild` + build string so
  other Spacewar testers aren't matched), friend invite via overlay,
  messages over Steam Networking Messages as the ggrs socket.
- Setup handshake (D3/D4) and checksum exchange (D5) live here.
- **Basic test**: unit tests over `Loopback`; two game processes on one PC over
  `127.0.0.1` with scripted inputs (`PD_TEST_*`), both log identical checksums
  for 3,000 ticks; Steam spike result written up.

### Lane D as built (LAN only; hand-rolled lockstep, no ggrs, no Steam)

Code: `crates/pd_game/src/net/` (`mod.rs` session, `wire.rs` bytes,
`udp.rs` socket, `lan_tests.rs` real-socket tests). Std only, no new crates.

**Transport.** UDP, default port **7777**, one message per datagram (largest
~470 bytes; asserted < 1200). Host binds `0.0.0.0:port` and adopts the first
address whose packet decodes as a `Hello`; all other addresses are ignored.
Join takes `ip`, `ip:port`, `[v6]:port` or a host name (IPv4 preferred,
port 7777 if none). `WouldBlock` and Windows' `WSAECONNRESET` (peer's port
closed) are treated as "nothing yet". `udp::lan_ip()` gives this PC's LAN
address for the host screen; `NetSession::local_port()` the bound port.

**Wire format** (little-endian; strings = `u8` len + UTF-8, max 64 bytes,
cut at a char boundary when longer; decoding returns `None`, never panics,
on bad magic/protocol/tag, truncation, over-cap lengths, non-0/1 bools,
bad UTF-8 or trailing bytes):

| Bytes | Field |
|---|---|
| 0-2 | `PDN` |
| 3 | `PROTOCOL` (1) |
| 4 | tag |

| Tag | Message | Fields after the tag | Delivery |
|---|---|---|---|
| 1 | Hello | build str, data_hash u64 | plain, session repeats |
| 2 | Pick | char str, colour u8, n u8 (≤64) × i16 arsenal, ready u8 | reliable |
| 3 | Rules | stage str, time_limit_ticks u32, has_rounds u8, [rounds u32] | reliable |
| 4 | Start | Rules, Pick (slot 0), Pick (slot 1) | reliable |
| 5 | Inputs | first_tick u64, n u8 (≤16) × 5-byte `Input` | plain, redundant |
| 6 | Checksum | tick u64, hash u64 | reliable |
| 7 / 8 | Ping / Pong | ms u32 | plain |
| 9 | Bye | (none) | plain, sent 3× |
| 0x20 | (reliable frame) | seq u32, then a message from tag 1-9 | resent every 150 ms until acked |
| 0x21 | (ack) | seq u32 | plain |

Reliable frames are handed up exactly once and in order (like `Loopback`), so
the lobby code can send a `Pick` once and trust it to arrive.

**Liveness / timeouts** (wall clock, outside the sim): `Ping` every 250 ms
once connected, RTT from the `Pong` (`ping_ms()`); `Hello` repeated every
500 ms until the peer has sent something else (so one lost Hello can't
strand either side); **5 s** of silence once connected → `Closed("Connection
lost")`; a joiner that never connects gives up after **10 s** → `Closed("Could
not reach host")`; a waiting host never times out. Build or data-hash
mismatch closes **both** sides with the reason (the side that notices sends its
Hello back first). Messages arriving before the peer's Hello are held and
replayed after it.

**Lockstep details.** `push_local_input` sends the newest 8 inputs; `poll`
repeats them every 50 ms while a match has inputs, so a lost packet can't
stall a side that is waiting. `forget_before` keeps the newest 8 local inputs
for that. Sending or receiving `Start` resets the match state (tick 0) on
each side at the right point of the message stream; peer inputs more than
~`2*INPUT_DELAY + RESEND` ticks past our newest are dropped as stale packets
from the previous match.

**Data hash (D4).** FNV-1a 64 (8 bytes per step; ~55 ms on the full cache) over, sorted by name: `skills.json`,
`chars.json`, `stages.json`, every root `*.pdcl` and `*.props.json`, every
`chars/*.rootmotion.json` (name + length + bytes; a missing fixed file
hashes as absent). About 70 MB on the full cache, no glb/wav.

**Tested** (`cargo test -p pd_game net::`): wire round trip of every
variant, cut/extended packets rejected, 20 000 random/mutated packets never
panic and re-encode identically; host adopts the first Hello only; reliable
delivery in order under ~1/3 loss; ConnectionReset on a closed port;
host+join connect over 127.0.0.1 and lobby Pick/Rules/Start arrive; 600-tick
lockstep gives identical input logs on both sides (and checksums agree),
also with ~1/3 of datagrams lost both ways plus a 120-tick rematch; data
mismatch closes both with the reason; `Bye` closes the peer; 5 s silence and
10 s unreachable host close; data hash covers exactly the listed files;
`open_lan_host` / `open_lan_join` connect.

### Steam as built (lane `lane/steam`, 2026-10-02)

Code: `crates/pd_game/src/net/steam.rs`, Steam pages in
`frontend/online.rs`, one plugin line in `main.rs`
(`frontend::online::steam_plugin`).

**Spike: works on the GNU/MinGW toolchain.** Crate `steamworks` 0.13.1
(+ `steamworks-sys` 0.13.0 for three raw calls), no Bevy dependency.
`steamworks-sys`' build script copies Valve's `steam_api64.dll` and
`steam_api64.lib` (SDK redistributables shipped inside the crate) into its
OUT_DIR and emits `rustc-link-lib=dylib=steam_api64`; MinGW `ld` links the
MSVC import library as-is. No build changes were needed to link.
Cargo feature **`steam`, on by default**; `--no-default-features` builds
without it (Steam rows then greyed "This build has no Steam support").

**What ships next to the exe:** `steam_api64.dll` (Valve redistributable,
~300 KB; the exe will not start without it when built with `steam`) and
`steam_appid.txt` containing `480`. `crates/pd_game/build.rs` copies both
into `target/<profile>/` on every build (git-ignored there). Copy both next
to `engine/play/pd_game.exe` when refreshing it. (`Client::init_app(480)`
also sets `SteamAppId`, so the txt is belt and braces.) `cargo test` finds the
dll through cargo's PATH.

**Init.** `SteamNet` resource, created at startup: `Client::init_app(480)`;
on failure (Steam not running, etc.) the game runs normally and the Steam
rows are greyed with the reason. `PD_NO_STEAM=1` forces that state (test
switch). Steam callbacks run every frame in `PreUpdate`
(`process_callbacks`), on every screen. Every Networking Messages session
request is accepted; the transport only listens to its lobby peer.
`init_relay_network_access()` is called at init.

**Lobbies.** Created friends-only by default (toggle to Public on the host
page), max 2 members, data `game=pd_rebuild`, `build=<net::build_id()>`,
`host=<persona>`. Find = (a) `RequestLobbyList` filtered on `game` and
`build`, worldwide, plus (b) lobbies that friends running App 480 are in
(`GetFriendGamePlayed` + `RequestLobbyData`), because Steam never returns
friends-only lobbies in the public list. Both go through `listing_from`,
which drops anything without our two tags (App 480 is shared with every
Spacewar tester) and full lobbies. Listings: friends first, then by name.
Your own lobby (same Steam account in a second copy) is marked YOU and
cannot be joined. Async results the player no longer wants (backed out while
waiting) are left at once (`accept_result`).

**Invites.** "Invite friend" opens the overlay invite dialog
(`ActivateGameOverlayInviteDialog`). An accepted invite
(`GameLobbyJoinRequested`) on any screen but a match switches to Online and
joins; during a match it waits until the match ends. `+connect_lobby <id>`
on the command line does the same at startup. Caveat: Steam launches
*Spacewar* (not us) for an invite accepted while our game is closed, so
invites only work while the game is already running.

**Transport.** `SteamTransport` implements `net::Transport` over
`ISteamNetworkingMessages`, channel 0, the `wire.rs` plain encoding (same
bytes as a UDP plain frame; no extra framing because Steam does
reliability). `Pick`/`Rules`/`Start`/`Checksum`/`Bye` go
`RELIABLE_NO_NAGLE`, the rest `UNRELIABLE_NO_NAGLE`, all with
`AUTO_RESTART_BROKEN_SESSION`. The lobby owner is `Role::Host` (adopts the
first `Hello` from a lobby member, like UDP adopts the first address); the
joiner is `Role::Client` talking to the owner. A `LobbyChatUpdate`
Left/Disconnected/Kicked for the peer is surfaced as `NetMsg::Bye` ("The
other player left"); dropping the transport (session removed) leaves the
lobby. `NetSession`'s own liveness (5 s silence, 10 s join timeout) is
unchanged. Everything downstream (lobby page, lockstep, checksums,
`online_match.rs`) is untouched.

**Tested on one PC / one Steam account:** init + persona name; lobby
created and tagged; visibility toggle; the overlay invite dialog opens
(logged "Steam overlay opened"); a second copy of the game on the same
account finds the lobby via Find (marked YOU) and refuses to join it;
`PD_NO_STEAM=1` greys the rows; LAN untouched. Unit tests: lobby tag
filtering, sorting/dedupe, `+connect_lobby` parsing, stale-result handling,
init-failure wording, default visibility.

**Needs two PCs / two Steam accounts (not verified):** joining a friend's
lobby, the P2P connection through Steam's relays, the full lobby -> match ->
result flow, "The other player left" on lobby leave / game close, accepting
an overlay invite, ping on the lobby page over relays.
Not done: ping/region on the Find list (not cheap with this API).

## Wave 2: integration (orchestrator + one Opus lane, after A–D merge)

Merge order **C → A → B → D**, full tests after each merge.

Lane **E: wire it together** (owns `main.rs` sim/input glue, `cpu/mod.rs`
guard, `devtools.rs` additions):

- Online match: `LocalViews = [my_slot]`, my slot `Human`, other `Remote`.
  `gather_input` fills only my slot. A FixedUpdate system before `step_sim`
  hands my input to `net::Session` and gets both confirmed inputs. `step_sim`
  runs only when the session says a tick is ready (lockstep), the same gate
  the menu pause and dev pause use.
- The pause menu does NOT pause an online match (it just opens over it), and
  dev F2/F3 are disabled online.
- Disconnect / timeout → result screen "Connection lost"; desync → "Out of
  sync" + bug report on both PCs (D5).
- F1 overlay gains: role (host/client), ping, input delay, last checksum,
  frames waiting on the network.
- **Basic test**: two processes on one PC over LAN loopback play a full
  `--demo` match to the result screen with matching checksums; screenshot of
  each window (single screen each).

## Basic testing done by the agents (before the user plays)

Pass bar for handing over:
1. `cargo build -p pd_game` and `cargo test -p pd_game -p pd_formats` green
   (cache-dependent tests run against a real cache).
2. Local split-screen 2P and 4P unchanged (before/after screenshots looked at).
3. Single-screen screenshots per seat.
4. Two copies on one PC: a full scripted match over `127.0.0.1` with no
   checksum mismatch.
5. Steam: spike result; if it links, one PC can create a 480 lobby and see it
   in the log.
6. `PLAY.bat` gets an "Online" note; `engine/play/pd_game.exe` refreshed.

## Testing in person (you, before the full review)

Checklist to send back, F8 for anything odd (reports include inputs and the
state dump):
- LAN: two PCs on the same network, Host on one, Join by IP on the other,
  three full matches, including rematch and quitting mid-match.
- Steam: both PCs logged into **different** Steam accounts that are friends;
  host a Steam lobby, invite, play two matches. Expect Steam to show you both
  as "playing Spacewar".
- Note: input feel (3-tick delay ≈ 50 ms), any stutter, any "Out of sync",
  any wrong sound/effect, how the single-screen HUD reads.

## Review gate (only after your in-person test)

Independent critics in separate contexts, not the builders:
- **Determinism audit**: any sim read of time, rendering, hash order or
  uninitialised data; checksum coverage of every sim field.
- **Netcode review**: handshake, timeouts, packet loss/reorder, port reuse,
  malformed packets can't crash or desync, Steam lobby filtering.
- **UI/UX review** against the original's menu style and your notes.
- **Code review** (`/code-review` high, or `/code-review ultra` for the cloud
  multi-agent review) of the whole branch.
Findings are fixed in one bounded wave, re-tested the same basic way, then
merged.

## Cost / size (rough)

| Wave | Agents | Size |
|---|---|---|
| 0 contracts | 1 Opus | S |
| 1 A / B | 2 Sonnet | M each |
| 1 C / D | 2 Opus | M / L |
| 2 integration | 1 Opus | M |
| Review gate | 3–4 critics | M |

## Risks

- **Steam on MinGW** may not link: LAN still works; fallback is an MSVC build
  of just the game, a toolchain change that needs Laurence's OK.
- **Hidden non-determinism** (an unseeded pick, HashMap order, float from the
  renderer) shows up as "Out of sync": that's why Lane C and the checksum go
  first.
- **The cache must match on both PCs**: D4's hash check turns it into a clear
  error; both players must convert the same game version.
- **Laurence's rules**: online was planned "after the local game is proven"
  (`PLAN.md` Phase 4). This branch is separate; merging to his `main` is
  his call.

## Merged into main (2026-10-03, lane online)

Brett's `online-mp` (17 commits on `92cd08b`: wave 0 contracts, LAN lockstep, Steam App 480, single-screen view, F1 / F2 / F3 / F8 dev tools) was merged
with `git merge --no-ff`, so his commits and authorship are kept. What the merge changed on purpose:

- **Steam files:** `steam_api64.dll` and `steam_appid.txt` now live in the tracked `engine/vendor/steam/` (private repo; Valve's redistributable). They are NOT
  in `engine/play` (Laurence's game copy, ignored); `build.rs` copies both from the vendor folder next to the built exe in `target/<profile>/`. Whoever
  refreshes `engine\play` must put both next to `pd_game.exe`, or the exe won't start (`steam` feature on by default).
- **Cache:** `regen_cache.sh` is kept; never run it against `engine\cache` (it writes `engine/cache.new`).
- **Camera / FOV / HUD:** main's behaviour kept. Brett's `LocalViews` (which fighters get a view on this PC) is in with a small footprint
  (`seats.rs`, the layout calls). His full-window 74.7 degree FOV and unboosted HUD apply ONLY when `LocalViews` is restricted (online / `--view-seat`);
  local one-player play is unchanged. They become redundant once the `camfov` lane (FOV setting = horizontal angle of a 16:9 view, per camera) lands:
  drop the two `local.0.is_some()` branches (`main.rs::apply_fov`, `hud.rs::layout_hud`) then.
  *(Archivist 2026-10-07: done. camfov landed first and the fix-up merge 19a8bb8 dropped both branches; on main `apply_fov` uses `views::fov_for` for every camera and no `full_scales` exists. A restricted `LocalViews` view therefore gets the same FOV setting and HUD fit as any full-window view. `hud.rs::layout_hud` still takes a `local` parameter it no longer reads.)*
- **Menus:** ONLINE is a main-menu row (the MODES row is gone); gamemodes are chosen in the local setup (GAMEMODES) and in the online lobby (GAMEMODE).

### Gamemodes online

- `LobbyRules` gained `mode` (the `MODES` entry's stable `id`, empty = STANDARD) and `rules_hash` (`modes::rules_fingerprint`: FNV-1a of the Debug text of the
  resolved sim `Rules`, fixed hand included). `PROTOCOL` is 2. The host picks the GAMEMODE row; it is sent with every `Rules` message and in `Start`.
- **Refusing, not desyncing:** a joiner resolves the id against its own table and recomputes the fingerprint (`modes::resolve_lobby`). Unknown id or different
  fingerprint = it leaves the lobby at once ("The ... rules differ between the two PCs: both must run the same build"), and `apply_start` re-checks on `Start`
  for both host and joiner (`Err`: no players queued, no match, Online error page). The whole mode table is also folded into `net::data_hash`
  (`modes::table_hash`), so two builds with different tables are refused in the Hello handshake. The `--online-host` / `--online-join` test hook mixes its
  `--mode` into the Hello (`NetSession::mix_setup`), so copies started with different modes refuse to connect.
- **Lockstep coverage:** `Combat::hash_state` (and so `Match::checksum`, exchanged every 60 ticks) already includes `rules`, `fighters` (hand slots) and `arsenals`,
  so a `Rules` difference that slipped past the handshake would still be caught as "Out of sync" within a second. Test `rules_are_part_of_the_checksum_the_lockstep_compares`.
- **In a mode** both fighters deal the mode's hand (`PlayerSetup.arsenal` = the hand on both PCs, neutral Aura kind), exactly as the local setup does;
  the ARSENAL row is locked (FIXED HAND). Tests: `frontend::online::tests` (host -> joiner travel, read-only joiner, same players / Rules on both PCs, refusals),
  `frontend::modes::tests` (selector steps, fingerprints, table hash, handshake with different modes).

