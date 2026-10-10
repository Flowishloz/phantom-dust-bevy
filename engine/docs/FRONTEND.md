# Front end — title, versus setup, arsenal builder, result

Code: `pd_game/src/frontend/` (this lane's own module) plus the state machine
and 3D-scene glue in `pd_game/src/main.rs` (owned by this lane too).
`menu.rs` holds the Options overlay (title row OPTIONS MODE, or Esc/Start in a
match), three tabs since 2026-10-08 (VIDEO SETTINGS, SOUND, CONTROLS): see "Options tabs",
"Display options" and "Look options" below.

> **Reading order (archivist, 2026-10-03; checked 2026-10-07):** this file is layered by date, newest
> sections last. Where an early section and a later one disagree the later one
> wins. Current owners: setup and character select = "Four-player setup" and
> the lane sections after it; Options overlay = "Display options" + "Look
> options"; menu sounds = "Menu sound map"; modes = "Game modes". The early
> "Screens" text for Versus setup (two columns, p1/p2 cursors) is the 2-player
> history. The online lane (merged 2026-10-03) replaced the title's MODES row
> with ONLINE and moved modes into a GAMEMODES row of the local setup: "Game
> modes" below is current; online itself is ONLINE-PLAN.md.

## Why a state machine

Before this lane, `pd_game` always built one match at Startup from CLI flags
and ran it forever. Two people now need to pick characters, arsenals, a
stage, a time limit and a rounds-to-win target from the title screen, play,
see a result, and set up another match — all without the command line. That
needs Bevy `States` (`frontend::state::AppState`: `Title, Setup, Arsenal,
Match, Result, Online`; a `Modes` state existed 2026-10-02 to 2026-10-03) so the match-only systems (`step_sim`, HUD/fx, input
gathering, animation, camera follow) run only `in_state(Match)`, and a way to
(re)build the 3D scene each time a match starts.

## Two launch paths

**Legacy** (`Config.legacy`, computed in `main()`): if the command line has
*any* of the options that already existed before this lane — `--p1 --p2
--stage --demo --screenshot --shot-frame --loadout --arsenal --open-menu` —
the game skips the menus entirely and starts `AppState::Match` immediately,
built from those flags exactly as it always was (`setup`, `fx::spawn_fx`,
`hud::spawn_hud` run once at `Startup`, gated `.run_if(cfg.legacy)`).
Automated tests and other lanes' screenshot scripts depend on this staying
identical; nothing about it changed except that it now flows through the
same `AppState::Match`-gated systems as the interactive path (harmless: the
state simply never leaves `Match`).

**Interactive** (no such flags — "no options" per the brief): the game opens
on `AppState::Title`. The versus setup screen builds a `MatchSetup` resource
(chosen characters, arsenals, stage, time limit, rounds to win) and requests
`AppState::Match`; `main.rs::enter_match` (an `OnEnter(Match)` system) then:

1. Despawns the previous match's scene, if any (everything tagged
   `frontend::state::MatchScene` — the stage, both characters, both cameras,
   the light and the music; `main.rs::spawn_scene` is the one place that
   spawns them, shared by `setup` (legacy) and `enter_match`).
2. Rebuilds `World` (`sim::Match` + `combat::Combat::with_arsenals`) and
   `StageCollision` from what was picked.
3. Respawns the scene for the new stage/characters via `spawn_scene`, and
   resets `CharacterAnims` so `build_graphs` rebuilds the animation graph for
   the (possibly different) character models.

The HUD and fx cameras/assets (`hud::spawn_hud`, `fx::spawn_fx`) are spawned
once, the first time a match starts, and reused after that — `SceneReady`
tracks this. A match's own dynamic visuals (capsules, projectiles, barriers)
already get cleared and rebuilt by `fx.rs`'s own systems whenever
`combat.capsules`/`projectiles`/`areas` are reset, which is exactly what a
fresh `Combat` looks like to them (the same thing that already happens on an
ordinary round restart) — so `enter_match` doesn't need to touch `fx.rs`.

**Hiding the previous screen without touching cameras.** Every frontend
screen (Title/Setup/Arsenal/Result) draws through its own `Camera2d` at
`frontend::ui::FRONTEND_ORDER` (6) with an **opaque** clear color
(`ClearColorConfig::Custom`), above the match's `Camera3d`s (order 0/1) and
its HUD (order 5, `hud.rs`), below the settings overlay (order 10,
`menu.rs`) so Esc/Start still shows on top. Bevy renders cameras in `order`
each frame, so the frontend camera's own clear simply paints over whatever
the match/HUD cameras drew that frame — no need to deactivate them while
sitting in a menu. Each screen's whole UI tree (including its camera) is
tagged `DespawnOnExit(AppState::X)` (a Bevy built-in), so leaving a screen
tears it down automatically; nothing in this lane's code despawns them by
hand.

Ending a match: `main.rs::check_match_end` (`in_state(Match)`) watches
`combat.wins` against `MatchSetup.rounds_to_win`; once reached it requests
`AppState::Result`, which — since `step_sim` only runs `in_state(Match)` —
simply stops advancing the sim, freezing the last frame (fully hidden behind
the opaque result screen anyway). "Endless" (the default, and today's
existing behaviour) never does this; rounds just keep restarting.

## Screens

How the screens *look* (the original's menu art, pixel font, portraits,
stage previews) is in MENUS.md.

**Title** (`frontend/title.rs`): VERSUS MODE / ONLINE / ARSENAL / OPTIONS MODE /
EXIT GAME (ONLINE added by the online lane, 2026-10-03; the old MODES row is gone, modes live inside the two game-settings screens), one shared cursor (either player's
controls), a description line for the highlighted row. Options sets
`menu::MenuState.open = true`, opening the settings overlay.

**Versus setup, 2-player original** (superseded 2026-10-01 by the four-seat,
two-page setup in "Four-player setup" below; kept for the row semantics):
two columns that move *at the same
time* — each player has their own row cursor (`SetupState.p1_row`/`p2_row`)
over eight rows: **Character**, **Arsenal** (each player's own,
independently chosen), then the shared **Stage**, **Time limit**, and the
**Rules** block — **Rounds to win**, **Starting level**, **Starting skills** —
then **Start battle** (either player's cursor can sit on and change/confirm
the shared rows; up from Character wraps to Start). A shared row's dark tab
says whose cursor is on it (`1P`, `2P`, `1P2P`); the row turns red. The
description line explains the row of whoever moved last.
- **Character**: every `cache/chars/pc*.glb` (`frontend::discovery::list_characters`),
  labelled by id (`PC00A`...) — see "Character names" below.
- **Arsenal**: the built-ins (see below) plus any *legal* saved arsenal
  (`arsenal::check(..).is_empty()` — the brief's "only legal arsenals can be
  picked for a match"); illegal saved ones simply don't appear here (they're
  still there to keep editing in the builder).
- **Stage**: every `st*.pdcl` with a matching `.glb`/`.bounds.json`
  (`frontend::discovery::list_stages`); named and ordered from
  `cache/stages.json` when the stages lane has written one (confirmed
  working: `st06` showed as "Edgar's Dream" once that file existed), else the
  id in caps.
- **Time limit**: 5 / 10 / 15 (default, per the guide) / No limit.
- **Rounds to win**: **1** (default, like the original's single battle) /
  2 / 3 / Endless.
- **Starting level**: **0** (default — the original's Standard Battle: "All
  participants start at Level 0 with 20 Health", `Menu/vsmenu_en.ssb`) … 5 →
  `MatchSetup.start_level: i32`.
- **Starting skills**: 0 / **4** (default — the original deals 4 skills onto
  the buttons) → `MatchSetup.start_hand: usize`.
- `start_level` / `start_hand` are carried in `MatchSetup` but **not yet
  read by the match** (`main.rs`, not this lane's file). A time-out is decided
  by the healthier player (unchanged, no row).

**Arsenal builder** (`frontend/builder.rs` state machine, `builder_ui.rs`
drawing; rebuilt 2026-10-01 after the original's own editor, lane F1 —
`audits/P0-FRONTEND.md` section 1a). Two pages.

- **List**: your arsenals (16 at most, names 15 characters) plus "New
  Arsenal". A open, X new, Y options (Change Arsenal Name / Number of
  Schools / Randomize Skills / Disassemble Arsenal), Left/Right page the
  contents preview, B back. New = name (on-screen keyboard, default
  "Arsenal 01"), then **Number of Schools 1-5** — the arsenal exists
  straight away, as in the original (`FUN_14015afe0`).
- **Edit**: the **30 fixed slots**, an empty slot is an **Aura Particle**
  (new arsenals are all Aura), beside the **stock** list with the original's
  type tabs AURA / ATTACK / DEFENSE / ERASE / STATUS / SPECIAL /
  ENVIRONMENTAL (LB/RB). On a slot: A Replace (pick from the stock), X
  Replace with Aura, Y Add Same Skill. In the stock: A place, X favourite,
  Y options (Sort — three keys from None/School/Cost/Strength/Number of
  Uses/Range/ID/Rarity/Amount/Favorite; Mask School; Mark Illegal Skills —
  No Mark / Single school (+ which school) / Highlander / Puny only / Heavy
  only, shown as **NG** on the row). Start saves. B with unsaved edits asks
  the original's three questions: Finish editing? → Reflect changes? → Undo
  edits? (B at any question = keep editing). Edits are compacted after every
  change (skills first, Aura after: `FUN_14015a800`).
- **Pad-complete**: every action is on D-pad / A B X Y / LB RB / Start; the
  keyboard still works (WASD/arrows, Enter = A, Esc = B, X, Y, Q/E = LB/RB,
  Ctrl+S or Tab = Start; typing names).
- **Rules** are `arsenal.rs`, all read from the program: the school limit is
  the arsenal's declared Number of Schools and is checked when changes are
  reflected (the script's SYSMAX dialog "limited to %d schools"), the 3-copy
  cap at placement, the rule flags (`rule_flags`, `eligible`; `FUN_14015b830`,
  mask table at `0x1402dbc78`) drive the marks and the "Qualifies for" line,
  and the aura handicap is the multiplicative 1.3 / 1.0 / 0.7 by kind
  (`aura_handicap`; `gsdata+0x33034`). An arsenal with no skill is saved but
  qualifies for nothing ("All Aura Particles: add at least one skill").
  The stock leaves out the 8 school-less enemy/object skills (701-741).
- **Detail panel**: TYPE counts, SCHOOL counts, Aura Particle count, schools
  in use, kind and handicap (`FUN_14015a8d0` counts).
- **Saved format**: `userdata/arsenals.json`, each arsenal gains an optional
  `"kind"` (1-5); a file without it loads and the default is worked out from
  the contents (`arsenal::default_kind`: school count, 2 when it has no
  schooled skill). Favourites / sort / school mask live in
  `userdata/builder.json`. Unsaved rule mark is per visit.
```json
{"arsenals":[{"name":"My Deck","kind":1,"skills":[{"id":1,"name":"Psycho Wave"}, ...]}]}
```
Ids are authoritative on load; names are only for a human reading the file.
- **Scripted proof**: `--open-screen arsenal --menu-script <tokens>` with
  `up down left right a b x y lb rb start bs t:<text> wait shot:<name>`
  (`shot:` saves `f1_<name>.png` next to the `--screenshot` path). Run with
  the worktree's own cache path so `userdata/` stays out of the real one.
  Shots `engine/shots/f1_*.png`: list_empty, name, kind, edit_new, mask,
  stock_fav, aura_tab, edit_built, saved, markdlg, mark, dirty, discard1-3,
  after_discard, reopened, reload_list, reload_edit.

**Result** (`frontend/result.rs`): headline (winner, or DRAW) and both
players' win counts; either player's confirm or cancel returns to Setup.

## Built-in arsenals (`frontend::discovery::school_decks`)

`cache/skills.json` has a `"decks"` array — the game's own starter decks. At
the time this was written it named PSYCHO/OPTICAL/KI/NATURE (each padded to
30 with unused slots, `-1`, which become Aura Particles) but no Faith deck;
for any school without a named deck this builds one the same shape (that
school's cheapest registered skills, up to 3 copies each, topped up with
Aura Particles) so every school has a legal starter arsenal. Two more
built-ins are always added: **Test Set** (`main.rs::TEST_ARSENAL`, the sim's
long-standing default) and **Mixed (one of each)** (`--arsenal mixed`'s set).

## Character names

Not decoded — characters are shown by their cache id (`PC00A`...), which the
brief allows ("unconfirmed names are fine as ids"). The guide text
(`scratchpad/lanes/guide_text.txt`) has story character names (Edgar, Freia,
"Red Head"...) but no confirmed id→name mapping was found without deeper
reverse-engineering; not attempted here to stay in scope.

## Test switches

- `--open-screen <title|setup|arsenal|online|result>` (`modes` removed 2026-10-03): opens directly on that
  screen (skips Title), for a repeatable screenshot of it alone.
- `--menu-script <tokens>`: a comma-separated list of `up`/`down`/`left`/
  `right`/`confirm`/`cancel`, applied one at a time (every 12 frames) to
  P1's virtual input (`frontend/script.rs`) — a test double, not a feature.
  Used with the Setup screen's defaults (both players' Character/Arsenal at
  index 0), `confirm,up,confirm` clicks Versus then moves P1's cursor up
  (wrapping) to Start battle, so `--menu-script confirm,up,confirm
  --screenshot out.png` proves a match can be started through the menus with
  no human input. (The old `confirm,down×5,confirm` now stops on Starting
  level: there are two more rows.)
- Both compose normally with `--screenshot out.png --shot-frame N`
  (`main.rs::auto_screenshot`, unchanged, runs regardless of screen/state).

## Proof

`cargo test -p pd_game`: 169 passed (162 pre-existing + this lane's own
unit tests for stage-list parsing, built-in-deck construction including the
synthesized Faith deck, arsenal add/remove/copy-cap rules, and saved-arsenal
JSON round-tripping — `frontend::discovery::tests`, `frontend::arsenals::tests`).

Screenshots (`engine/shots/`, looked at, not just generated):
`frontend_title.png`, `frontend_setup.png`, `frontend_arsenal_empty.png`
(catalog + skill card + a live rule problem, "An arsenal holds 30 skills (0
now)."), `frontend_result.png`, and `frontend_menu_to_match.png` (a real
match — on a different stage than the legacy default, proving the dynamic
stage/character rebuild — started with zero human input via `--menu-script`);
montaged into `frontend_sheet.png`. `frontend_legacy_smoke.png` confirms the
pre-existing `--demo` path renders identically to before.

## Requests (outside this lane's files)

- `hud.rs`: `skill_card` (name/school/type/cost/STR-or-DEF/note formatting)
  and `SkillNotes`'s map are private, so the arsenal builder rebuilds the
  same formatting locally from `SkillDef` + its own small notes loader. A
  `pub` accessor (or making `skill_card` `pub(crate)`) would remove that
  duplication.
- `Menu/DckMenu.alr` (the original's deck-menu art) would need
  `pd_convert` changes to extract (not this lane's crate) — skipped per the
  brief; the builder uses plain HUD-font/orange-palette panels instead.
- ~~Colour-variant character textures~~ **built (altcolour lane, 2026-09-27)**:
  see PIPELINE.md "Costume-colour textures" and "Character select" below.


## Backdrop (2026-09-26)

(Stale since the menu art landed: with `cache/ui/menu` present every screen uses the StMenu teal clouds; the DckMenu sky below is only the fallback.) Every menu screen without that art draws the original arsenal menu's stormy-sky backdrop (`Menu/DckMenu.alr` texture 1, top-left 640x380, darkened for readability) when `cache/ui/dck/hud1.png` exists (`ui::screen_root`; convert with `pd_convert hudtex DckMenu.alr cache\ui\dck`, PIPELINE.md). The panel bars in the same texture are not used yet. `Menu/Mpt*.alr` are story mission photos, not skill cards.

## Audit fixes (2026-09-26)

- **Esc / Start:** during a match it opens the settings overlay, which now has **Quit to title**. In the menus Esc means "back" only (it closes the overlay if it's open; the title's SETTINGS item opens it). Menu screens ignore input while the overlay is open.
- The match ends after the final round's YOU WIN / YOU LOSE banner, not on the knock-out frame; a time-up draw never ends a first-to-N match (it used to crash).
- Second and later matches: cameras get their split-screen viewports, hit/block effects and sounds aren't skipped, voices follow the characters picked in setup, the stage and music are removed when a match is left, flight hums stop.
- Every animation clip is loaded (the cast, knock-down and get-up poses were missing from the short list and never played).
- Versus setup remembers the players' picks between visits; the arsenal builder no longer types a stray "r" when renaming and keeps its slot cursor in range.

## Menu restyle + match rules (2026-09-27, menus lane)

Every screen and the settings overlay now use the original's menu art and
font (MENUS.md). State machine, screens' actions and test switches are
unchanged, except: setup has three rule rows (above), rounds-to-win defaults
to 1, and `MatchSetup::default()` has the same defaults — which means the
legacy direct-to-match path (`--demo` etc.) now ends after one round win and
shows the result screen, unless `main.rs::check_match_end` skips legacy runs
(requested). The arsenal builder shows 9-row lists for catalog / arsenal /
saved instead of one line each; keys unchanged.

## Character select: live 3D preview (2026-09-27, uiselect lane)

Each player's portrait box on the setup screen (`frontend/setup.rs`'s
`Portrait`) now shows a live, idling, slowly turning copy of their
highlighted character instead of the flat portrait picture — code:
`frontend/charselect.rs`, wired into `AppState::Setup` from `main.rs`
alongside `frontend::setup`'s own systems. It's independent of the match's
own animation/camera code in `main.rs` (which only exists `in_state(Match)`):
its own small render-to-texture setup, alive only while the setup screen is
open (all its entities are `DespawnOnExit(AppState::Setup)`).

- **How it renders:** one small offscreen `Camera3d` per player, each
  pointed at its own `RenderTarget::Image` (`PreviewTextures`, 480x344 —
  the same aspect as the portrait box), whose output is shown through the
  existing `Portrait` `ImageNode`. The two characters are spawned 4000 world
  units apart with the camera's far plane capped at 140, so P1's camera
  physically cannot see P2's model or vice versa — simpler and more robust
  than `RenderLayers`, which doesn't auto-propagate from a spawned glTF
  scene's root down to the actual mesh entities it creates as children (so
  tagging only the root wouldn't have filtered anything). The flat 2D
  portrait (`MenuArt::portrait`) is kept as the fallback for the one frame
  before `PreviewTextures` exists.
- **Pose and clip:** `WAIT0` (idle), the same clip the match uses, looping.
  When a player's character changes, `WAIT_ACT0` (a short idle-action clip
  every converted character has) plays once as a "just picked this one"
  flourish, then blends back to `WAIT0` after 0.6s.
- **Turning:** slow constant auto-rotation (about one turn per 18s), not
  left/right input. The brief allows either; left/right on this exact row
  already cycles the character, so turning the model with the same keys
  would fight that — auto-rotate is the option that doesn't. Framing (camera
  distance/height) is computed per character from its own converted
  `chars/<id>.bounds.json` height, not hand-tuned per character.
- **P1/P2 costume colour** (replaced 2026-09-27, altcolour lane): no longer a
  runtime tint — when both slots show the same character, P2's preview swaps
  to the game's own colour-1 costume texture (`cache/chars/<id>.c1.png`,
  `pd_convert character`, PIPELINE.md "Costume-colour textures"). Newly
  spawned mesh materials under P2's model get their own cloned
  `StandardMaterial` with `base_color_texture` pointed at that PNG, rather
  than mutating the shared asset (P1 and P2 can pick the same character, and
  must not end up recoloured together if they do) or touching `base_color`/
  `emissive` at all. When the two picks differ, P2's material is left
  completely untouched (shared colour-0, same as P1) — DECISIONS.md
  2026-09-27, "no tint otherwise". Logic: `frontend::charselect::
  p2_alt_colour_id` (pure, unit-tested) decides the id; `apply_p2_colour`
  does the swap. `main.rs`'s real match uses the same two functions
  (`P2AltColour` resource + its own `apply_p2_colour`) so a real split-screen
  match with both players on the same fighter shows the same costume swap,
  not just the setup screen's preview.
- **Sound:** no confirm/back sound was added. The brief allows this ("if we
  have them") — `cache/sfx/common/*.wav` has 615 anonymous numbered clips
  with no name mapping (`sfx/index.json` only has per-clip loudness/loop
  stats, not what each one is), so nothing there is confidently a menu
  cursor/confirm/cancel sound; guessing one felt worse than nothing. See
  Requests.

**Fixed after review (2026-09-27):** the first version showed both models
frozen in the bind pose (T-pose, arms straight out) — `attach_players` tried
to insert the `AnimationGraphHandle`/`AnimationTransitions` components *and*
immediately query and `.play()` them in the same system call; a `Commands`
insert isn't visible to a query until the next sync point, so the query
never matched and `.play()` never ran, yet the entity was already marked
"attached" and so never retried. Fixed by splitting attach (just inserts the
components) from `drive` (runs every frame, compares what should be playing
against what actually is, and calls `.play()` when they differ) — the same
split `main.rs`'s real match animation already uses (`attach_anim_slots` /
`drive_animation`) for exactly this reason.

Re-verified after merging `main`'s jacket/cloth lane (`dangle.rs`'s cloth
grid, regenerated `cache/chars/*.glb`): `dangle::setup_dangles`/
`swing_dangles` aren't gated to `in_state(Match)`, so they pick up the
preview's characters too with no changes needed here — jackets/cloth parts
swing on the preview models the same as in a real match, and the idle pose
still isn't the bind pose.

Screenshots (`engine/shots/`, looked at): `sel_rotate_0.png`/`sel_rotate_3.png`
(same screen, 3 seconds apart — both players' models visibly turned, at
different rates/phases so they never look like mirrors of each other),
`sel_flow_to_match.png` (the pre-existing
`confirm,right,down,down,right,right,up,up,up,confirm` script from
MENUS.md, unchanged, still reaches a real split-screen match), montaged
into `sel_sheet.png`. Superseded by the altcolour lane's own proof below
(`alt_setup_samechar.png` replaces `sel_same_char_tint.png`).

### Requests (character select)

- Identifying the game's own menu cursor/confirm/cancel sound ids among
  `cache/sfx/common/` (or wherever they actually live) would let this add a
  real confirm/back sound; needs evidence (e.g. matching the exe's menu
  sound-trigger table), not a guess among 615 unnamed clips — a job for
  whichever lane next reverse-engineers the sound bank's naming.

## Alternate costume colour (2026-09-27, altcolour lane)

Built the converter/game halves of the "Colour-variant character textures"
request above (DECISIONS.md 2026-09-27: "Player 2 is told apart by the
game's own alternate colour costume ... when both pick the same fighter; no
tint otherwise").

- **Converter**: `pd_convert character` now also writes
  `cache/chars/<id>.c0..c3.png` (PIPELINE.md "Costume-colour textures") next
  to the glb -- the one texture that differs between `pcXXa0..3.alr`, found
  by which `texture_idx` covers the most drawn vertex indices
  (`pd_formats::character::base_colour_texture_index`, the same method as
  `pd_convert texprobe`'s "used-indices"), not assumed to be index 0 (it is,
  on every character checked, but a normal map at another index also exists
  and must not be swapped). A new `pd_convert charcolours <Player dir> <cache/chars
  dir>` writes just these PNGs for every character with colour siblings,
  without touching the glb/bounds/rootmotion a full `character` re-export
  would also rewrite -- what regenerated the main cache this lane used.
  `--no-colour-textures` on `character` skips the extra PNGs.
- **Confirmed on all 11 playable characters** (`pc00a`..`pc10a`, all have
  colour 0-3 siblings): `pd_convert texprobe` on each colour shows only this
  one texture's mean colour changing (same format/size/resource pointer);
  the decoded PNGs (`shots/alt_colour_sheet.png`, looked at) show real,
  distinct costume colours per character -- not just a hue shift, e.g.
  pc00a's jacket goes green/maroon/black/silver, pc08a's outfit
  purple/yellow/orange/teal.
  Colour 0 is also exported (`.c0.png`) even though it's already baked into
  the glb -- cheap, and gives every colour the same kind of file.
- **Game**: both the setup screen's live preview
  (`frontend::charselect::apply_p2_colour`) and a real match
  (`main.rs::apply_p2_colour`, gated by a `P2AltColour` resource `spawn_scene`
  sets from whichever launch path is active) clone P2's mesh materials and
  swap `base_color_texture` to the picked id's `.c1.png` when
  `frontend::charselect::p2_alt_colour_id` says both players picked the same
  character; otherwise P2's material is left alone. This replaces the
  earlier runtime tint entirely (no `base_color`/`emissive` change survives).
- **Proof** (`engine/shots/`, looked at): `alt_colour_sheet.png` (all 11
  characters x colours 0-3, labelled), `alt_setup_samechar.png` (both players
  moved to Edgar on the setup screen -- P2's preview shows Edgar's pale/red
  colour-1 costume, P1 stays on the default black/red colour-0),
  `alt_match_samechar.png` (a real split-screen match, both players on
  pc00a -- P2 wears the maroon colour-1 jacket, P1 stays in the default black
  colour-0, no glow/tint), `alt_match_diffchar.png` (pc00a vs pc01a -- both
  stay on their own colour-0, confirming "no tint otherwise").
- Test: `frontend::charselect::tests::p2_alt_colour_only_when_both_picks_are_the_same_character`
  (the same-character rule, pure and unit-tested; both `apply_p2_colour`
  systems just call it).

## UI polish: menu feel, stats, randomize (2026-09-27, UI polish lane)

Screens, state machine and test switches (`--open-screen`, `--menu-script`)
are unchanged; this pass is feel and two new pieces of content.

**Navigation feel** (`frontend/input.rs`): holding a direction now repeats
(`Repeat`/`read_repeating` -- first pulse immediately, then a 0.35s delay,
then every 0.1s; confirm/cancel stay one-press), used by every screen with a
list to move through (title, setup, builder, the settings overlay). Wrap-
around was already there on every screen (`(x + len - 1) % len`). Row
highlight colour now eases toward its target instead of snapping
(`ui::set_row`/`ease_toward`), and every `menu_screen` (title/setup/
arsenal/result) fades in from black over 0.16s (`ui::FadeIn`/`update_fade`,
a sibling overlay spawned after the screen's content, self-despawning).

**Menu sounds** (`frontend/sound.rs`, new): a move/confirm/back cue from the
game's own `common` sound bank on every navigation input, across all four
screens and the settings overlay. Which exact ids are right is
**unconfirmed** -- nobody has listened to any decoded sound yet
(FORMATS-AUDIO.md) -- picked by waveform shape from the ids the skill table
never references ("system sounds"); see the module doc comment to remap.

**Fun stats on the result screen** (`frontend/state.rs`
`MatchStats`/`observe`/`track_stats`, `result.rs`): damage dealt, skills
used, knock-downs, longest combo and favourite skill, polled render-side
from `World`'s sim state once a frame and reset once per match
(`main.rs::reset_match_stats`, `OnEnter(Match)` -- rounds restarting within
a match keep accumulating). No sim changes; `observe` is a pure function
(unit-tested: cast counting, damage/knockdown attribution to the *other*
player, the combo time-window, and that a fresh match's reset values -- 0
health, `cast_seq` 0 -- don't get misread as damage/a cast). Damage
attribution is an approximation: a skill that costs the caster their own
health would misattribute that loss as the opponent's damage; rare enough
to accept on a cosmetic panel.

**Randomize Arsenal** (builder list, Y > Randomize Skills; was key G): a
seeded arsenal shaped like the original's own Quick decks (16 skills + 14
Aura Particles, at most 2 copies) from up to *kind* schools; opened unsaved
(Start keeps it). Unit-tested legal for each kind across 30 seeds.

Proof: `engine/shots/ui_title.png`, `ui_setup.png`, `ui_arsenal.png`,
`ui_result.png` (all four screens after this pass), `ui_hud_font.png` /
`ui_hud_font3.png` (a real match's HUD -- names/skill names/timer in the
pixel font, HUD.md), `ui_result_stats.png` (the fun-stats panel; zeros here
since `--open-screen result` doesn't play a match -- the counting logic
itself is unit-tested, not just eyeballed), `ui_stage_preview_sheet.png` +
family zooms (MENUS.md). `cargo test -p pd_game -p pd_formats -p
pd_convert`: 237 + 34 + 7 passed (was 225 + 34 + 7 at the start of this
lane), no new warnings.

## Four-player setup, game rules, result (2026-10-01, lane setup4)

Replaces the two-column setup and result screens; the old two-wide `MatchSetup` stays (`chars`/`arsenals` = the first two players, for the voices and the legacy launch) and gained `rule`, `friendly_fire`, `arsenal_kinds`, `time_by_rule`.

- **Seats.** Setup opens with `Seats.mode = Press` and `Seats::clear()` (from the title; coming back from a result keeps the seats for a rematch). A pad takes the lowest empty seat with any button, a menu key takes seat 1 (the keyboard is seat 1; it shares the seat with a pad that took it first). B leaves a seat (the last player's B, or Esc, goes back to the title). A pad that unplugs frees its seat. Start packs the joined seats into sim slots `0..n` and packs `Seats::pad` the same way. `RequestedPlayers(Some(Vec<PlayerSetup>))` carries character, costume colour, arsenal, team.
- **Per seat:** character, costume colour (colours of the same character worn by another seat are skipped; a joiner gets the lowest free one), arsenal, team (in teams mode). **Shared rows:** mode (free for all / teams, teams dealt 1+1, 2+1, 2+2), stage, game rule (Standard, Puny, Heavy, Single School, Highlander, Puny Only, Heavy Only), time limit (by rule = 10 min, 5/10/15/none), rounds, starting Level (shows the rule's clamp), starting skills, friendly fire (teams only, default OFF, set explicitly on the sim). Every joined seat's cursor walks all 13 rows.
- **Rules.** `MatchSetup::apply_to_combat` sets `Rules.set = RuleSet::new(n)`, the time limit (`use_rule_time_limit` for "by rule"), `friendly_fire`, `set_arsenal_kinds`. An arsenal that fails `arsenal::eligible` for the rule is greyed with the reason (`rule_problem`); a rule change moves everyone to the first eligible arsenal. Invalid setups say so in red and refuse Start: nobody joined, "No arsenals meet the rules." (`arsenal::rule_available`), a seat's arsenal not eligible, teams with one team. Saved arsenals are filtered with `check_kind(.., kind_or_default)`. The eight Quick decks (`skills.json` `quick_decks`) are built-ins "Quick Q000".. with their stored kind 2.
- **Result.** `result::results(&Combat)`: free for all names the winning player and ranks 1st-4th (round wins, then last-round placement); teams name the winning team (both members 1st); a drawn single battle shows DRAW. **Behaviour change:** a drawn round ends the match as a draw when Rounds to win is 1 (the original's single battle); with more rounds it is replayed (`state::match_ended`, was "a draw never ends a match"). Stats are four wide; damage and knock-downs go to the attacker of the `CombatEvent::Hit` (no more `1 - i`), self-damage and poison credit nobody.
- **Scripted proof.** `--menu-script` tokens: `up down left right confirm|a cancel|b`, `pN:token` for virtual pad N (a pad's first press joins the next seat), `wait[:frames]`, `shot:name` (writes `<prefix>name.png` beside `--screenshot`, prefix = up to the first `_`), `mshot@tick:name` (during the match), `quit`. `PD_TEST_KO=<tick>:<slots>[;..]` knocks 1-based slots out in a match. Shots: `engine/shots/s4p_*.png` (title, empty, four_seats, teams, heavy_ff, match4, result_2v2, three_seats, ffa_rule, match3, result_ffa, rematch_setup, rejected_rule, rejected_still_setup, rejected_one_team, result_draw).
- **main.rs touched:** `main()` (Setup/Match system registration, `BuiltinArsenal.kind`), `enter_match` (calls `apply_to_combat`), `check_match_end` (uses `state::match_ended`; `match_won` and its test moved to `frontend/state.rs`). Also `arsenal.rs`: `#[allow(dead_code)]` on `check` (setup now uses `check_kind`).
- **Not done / open:** real-pad join not tried on hardware (scripted virtual pads only); the settings "split screen" row is not forced for 3-4 players (fourview's layouts ignore it); ~~a spare pad pressing a button mid-match can still take a free seat~~ fixed by lane charselect (below).

## Character select and game rules as two screens (2026-10-01, lane charselect)

The versus setup is now two pages of the one `AppState::Setup` (`setup::Page`, `SetupState.page`; no new `AppState`, `main.rs` untouched). Each page is a full-frame container (`PageRoot`) shown or hidden together.

- **Screen 1, characters** (opens here from the title and after a result, seats kept): four seat columns. An empty seat shows **PRESS ANY BUTTON TO JOIN** in the menu font inside its (empty) preview window (seat 1 adds "or Enter on the keyboard"). A pad's first button press takes the lowest empty seat, a menu key takes seat 1, as before; joining happens **only here**. A joined seat has a **portrait preview window** (`setup::WINDOW`, 172x248 design px, 344x496 render target, four fit across) with a live 3D fighter, then Character, Costume colour, Arsenal, Team (greyed "NO TEAMS" until the mode is Teams on screen 2) and a **ready banner**. Cursor rows: Character, Colour, Arsenal, Team (Team only in teams mode). **A** (any row) = ready (refused, with the reason, if the seat's arsenal breaks the game rule in force); a ready seat is frozen, its window frame turns gold, **B** takes the ready mark back. **B** with the cursor below the top row jumps to the top; **B at the top leaves the seat** (the pad stays plugged in and can rejoin; the last player's B, or Esc, goes back to the title). When **every joined seat is ready (two or more)** the setup moves to screen 2; **seat 1's Start** (pad Start, keyboard Tab) moves on at once with any number of players, ready or not, so one player alone can start a practice match (same minimum as before: one joined seat).
- **Screen 2, game rules**: Mode, Stage (with its picture), Game rule, Time limit, Rounds to win, Starting level, Starting skills, Friendly fire, **Start battle**; a line per seat names who plays what. Any joined seat's cursor (each has its own, opening on Start battle) edits the shared rows, the tab on the row says whose cursors are there. **B or Esc goes back to screen 1** with every pick and seat kept (ready marks cleared, so nothing advances on its own). Start battle on a setup with a problem (`SetupState::problem`) still says why in red and refuses.
- **Rematch**: the result screen's Continue returns to **screen 1** with the seats, pads and picks kept, nobody ready (`mark_rematch`).
- **Preview facing**: the characters' own forward is +Z (the sim's facing is `(sin yaw, cos yaw)`), so each seat's preview camera sits on the +Z side (`charselect::FACE_ANGLE`) and the fighter now faces the viewer, swaying +-0.3 rad (`SWAY`) rather than turning round the whole circle. Framing is head to toe (`FRAME_FRACTION` 0.80 of the window height, eye at mid-body). A latent bug showed up once the view was front-on: the character glb also carries the authored `shadowhull_dN` shadow-hull meshes (no material); a match keeps them off the cameras (`shadows::tag_fighter_hulls`) but the previews drew them as white lumps over the body until a first match had run. They are hidden now (`charselect::apply_colours`).
- **Spare pad mid-match** (`seats.rs`): `JoinMode::Press` (set by the setup) now only lets a pad that presses a button take back a seat that **lost its pad**; it can never take a new seat. `Seats::update` runs during the match, so a spare pad mashed mid-match is ignored (tests `a_spare_pad_pressing_a_button_mid_match_never_takes_a_new_seat`, `a_spare_pad_may_take_back_a_seat_that_lost_its_pad_but_only_on_a_press`). `Auto` (the legacy launch) is unchanged.
- **Scripted proof**: `--menu-script` gained `start` (`pN:start`); everything else as before (a virtual pad's first press joins). Note the script's `wait`/`shot` tokens are only consumed by the menu screens: after the press that starts the match the next token must be an `mshot@tick:name` (or `quit`). Run with a scratch cache folder (a directory junction to the real cache whose parent is a scratch folder) so `userdata/` stays out of the real one. `PD_TEST_KO=200:2,3` ends the match. Shots `engine/shots/cs_*.png`: alone (seats 2-4 prompts), two, four, three_left (seat 3 left, its prompt back), p1_down, two_ready, rules, rules_teams, back_teams (screen 1 again with Team rows), rules_ffa, match, result, rematch (screen 1 again, seats 1, 2, 4 kept, seat 3 prompt).
- **Tests**: `frontend::setup::tests` (no seat joined at first, one then a second joins the lowest empty seat, a leaving seat is the next to be taken, ready freezes a seat and B un-readies, B walks to the top then leaves, everyone-ready advances and Back keeps picks, lone player needs Start and only seat 1's Start counts, ready refused with a rule-breaking arsenal, rules rows / Start / Back, rematch and fresh visit open on screen 1, the window is portrait and four fit), `charselect::tests` (camera always in front, whole fighter in frame, portrait target), `seats::tests`.

## Rules screen: no seat list on Start battle (2026-10-01, lane hudbits)

The dark tab on the Start battle row showed the joined seats ("124") because every cursor rests there. The program's rules menu has no such tab (`vsmenu_en.ssb` holds no seat strings for it), so that row's tab is empty now; the other shared rows still show whose cursor is on them once someone moves. Proof: `engine/shots/hb_rules.png` (`--open-screen setup --menu-script p1:a,p2:a,p3:a,wait:20,p1:a,p2:a,p3:a,wait:40,shot:rules,quit`, scratch cache with a junction to the real one).

## CPU seats on the character-select screen (2026-10-01, lane cpuseat)

A joined person can make an empty seat a CPU (`SeatKind::Cpu { epa_row }`, `cpu/`). Code: `frontend/cpuseat.rs` (grades, decks, pure), `frontend/setup.rs` (seat state, input, drawing), `MenuAxis` gained X / Y / LB / RB (`input.rs`, `script.rs` tokens `x y lb rb`).

- **Making and removing.** **Y** (keyboard Y) makes the lowest empty seat after seat 1 a CPU; or **LB / RB** (keyboard Q / E) walk the person's cursor over their own column, every CPU's and every empty seat (never another person's), and **A** on an empty seat makes it a CPU. The cursor then sits on the new CPU, so its rows can be set at once; **A** hands the cursor back to the person's own column (it does not ready them). **X** removes the CPU being edited; **B** walks its cursor to the top, then removes it (like a person leaving). The last person leaving takes every CPU with them; a rematch keeps them. A CPU can never be seat 1 (the keyboard's seat) and a setup whose first taken seat is a CPU says so and refuses to start. A ready person is frozen (no Y, no LB / RB) until B takes the ready mark back.
- **A CPU's column.** Header "CPU 2" in the seat colour, the grade beside it, a violet window frame (the colour of whoever's cursor is on it when one is) and a "CPU   GRADE 3" strip across the bottom of the live preview. Five lines: Character, Costume colour, **Strength** (GRADE 1..5), **Arsenal** (its deck), Team (teams mode). A CPU is always ready: with two or more seats taken, everyone *person* ready goes on to the rules. Teams are dealt over people and CPUs together (1+1; 1+3 = 2v2 by seat order). A new CPU starts on a character nobody plays, a free costume, grade 3, the next of the original's decks (the first that meets the game rule). Empty seats show "Y: ADD A CPU" / "LB / RB, then A" under the join prompt.
- **Five grades = `enemyparam.epa` rows 64, 63, 62, 61, 60** (grade 1 weakest .. grade 5 strongest, grade 3 = row 62 default). Laurence picked the "nearest" ladder. READ from the file: rows 60-64 share target rule 4 (the nearest opponent), idle style 2 (camp at the target's start point), capsule strategy 0, think 30; across the block attack % and react-to-melee % go 66, 77, 77, 77, 88 (never rising) and react-to-bullet % and dodge % go 11, 22, 33, 44, 55 (strictly rising), wander % falls 15, 12, 9, 6, 3, so row 64 is the weakest and row 60 the strongest. There is **no separate targeting option**: each grade carries its row's rule, 4 for all five, which in a free-for-all makes CPUs fight each other as well as the person. The description line prints the row's numbers. (The first pick, rows 4..0, the equal-percentage ladder with target rule 0 = lowest slot, is superseded.)
- **Decks.** The quests pair no deck with a row (rows 0-4 appear with 40+ different decks) and the school decks go with the story's enemy models (quest character ids 11-26 = `enm00a`..), so the default is a school deck the quests use most: `DECK_PSYCHO`, `DECK_KI`, `DECK_NATURE`, `DECK_OPTICAL`, `DECK_RELIGION` (shown ENEMY PSYCHO / KI / NATURE / OPTICAL / FAITH): 15 skills, three of each of five skills, the rest Aura Particles; the declared kind at +0x50 is 5 (read from the file, so Aura recovery x1.0), loaded from `Data/com/deck` (beside the cache, `PD_COM_DIR`, or the game's own data) with the same ids kept in `ORIGINAL_DECKS` as the fallback (`pd_convert cpu` does not copy decks). The player's legal saved arsenals follow them in the CPU's Arsenal row. The game rule greys a deck that breaks it (Highlander greys the originals: three copies) and moves the CPU to one that qualifies. Not offered: the per-character decks (`DECK_EDGAR_0`, `DECK_JD_0` ...; quest character id = the `pcNNa` number, so Edgar = `pc01a`).
- **Character.** The quest records pair a row with many characters (and the story's enemy models), never one: not READ, so the person picks the CPU's character on its seat like their own.
- **Start.** `StartPlan.kinds` (`SeatKinds`, slot order after packing) is written to the `SeatKinds` resource, the CPU's deck is that slot's arsenal and its declared kind goes into `arsenal_kinds`; `cpu::drive_system` builds the host at tick 0 as before. A CPU stands still on a stage without a nav graph (`cpu::stage_has_nav`: only stage codes 2-8 mod 10), so Start refuses with the reason and the Stage row greys. `cpu::com_file` / `cpu::stage_has_nav` are the two public helpers added to `cpu/mod.rs`. No `main.rs` change.
- **Scripted proof**: `--open-screen setup --menu-script p1:a,wait:60,p1:y,...` (tokens `y x lb rb` added); shots `engine/shots/cs_cpu_*.png`: hint, added, grade, deck, focus_empty, four, rules, 1v1_seats, 1v1_rules, match_1v1(+_later), teams_rules, match_teams(+_later), match4_early / match4_later (1 person + 3 CPUs). Tests: `frontend::setup::tests` (CPU add / refuse / focus / edit / remove / ready / plan kinds + decks / packing / teams / first-seat / nav / rule / rematch), `frontend::cpuseat::tests` (the five grades against the shipped file, the five decks against their files).

## Menu sound map and overlay timing (lane menus, 2026-10-02) -- READ from the program

Menu scripts (`Menu/*.ssb`) call **no sound native** (checked: no group 1/2/3/5/6 native reaches `FUN_140276330` within two calls); the sounds come from the C++ menu controls (`FUN_140246130` manager, control handlers in the table at `0x1403fa340`..`0x1403fae00`, pad bits A `0x1010`, B `0x2020`, up `0x10001`, down `0x20002`, left `0x40004`, right `0x80008`). All play `FUN_140276330(0, id, id, ...)` = common bank.

| Action | common id | Address |
|---|---|---|
| Cursor move (up/down, only when the index actually changes; wrap lists play, a clamped end is silent) | 3 | generic list `FUN_14024b000` (`0x14024b442`), `FUN_14024a5b0`, scrolling lists `FUN_14015c310`/`FUN_14015fad0`/`FUN_1401585f0` (only when the list has > 1 entry) |
| Confirm (A) | 0 | `FUN_14024b000` (`0x14024b442`), `FUN_140248ef0`, `FUN_14024a100` (stock pick; silent when the pick is refused), press-start `FUN_1401c9d00` |
| Back (B) | 1 | `FUN_14024b000` (suppressed when item flag bit 13), `FUN_140249850`, `FUN_1402489f0`, `FUN_14024a100`, `FUN_140248b50` |
| Invalid / disabled item | 2 | `FUN_14024b000` (item flag bit 16 with mode bit 12), `FUN_140248b50` (state 10) |
| Tab / page switch (left/right across the 3 builder pages, each step) | 4 | `FUN_140161fa0`; preview-page timer `FUN_1401536e0` |
| Text entry: cursor / character step | 34 (0x22) | `FUN_14018eb40`, `FUN_140175840` |
| Text entry: delete | 11 (0xb) | `FUN_14018eb40` |
| Text-entry keyboard (on-screen) | 3 move, 0 pick, 1 back/delete | `FUN_14016e180` |
| Left / right on a list row (option value, seat, level) | none | `FUN_14024b000` just posts event 0x13/0x14 |
| LB/RB, X, Y on a list | none (events 10/8/9; Y in `FUN_14024a100` plays 0) | `FUN_14024b000` |
| Add / remove seat or CPU, builder place / remove / save, start match | no extra sound: they are list confirms (0) handled by the script after the control event | no call in the script natives |

Wired: `sound.rs` (`Cue::Move/Confirm/Back/List/Spinner(=text 34)/Delete(11)/Denied(2)`); left/right on list rows are now silent (was common/3), builder LB/RB/left/right play 4, name entry plays 34 / 11. Not wired: Denied has no caller yet (our screens have no disabled-item press). **UNCONFIRMED**: LIVE ear/trace check of the above; no menu footage or sound trace exists. Recorder steps for the orchestrator: start the original, load `live_trace.py` sound hook (`0x140276330`), at the main menu press down x3, up, A, B, then in Multiplayer > Split Screen press left/right, then in the arsenal editor press LB, RB, left, right, A on a stock skill, B; read the (id) column of the csv.

**Overlay timing (READ `FUN_1401c6450`, 30 Hz):** screen alpha ramps +0x14 per tick to 0xff = 13 ticks (0.433 s) in and out (ours was 0.16 s: now 13/30 s); the header ("MAIN MENU_") types on at `counter >> 1` characters (15 chars/s) and the trailing `_` blinks 8 ticks on / 8 off (`FUN_1401c6630`). Frame geometry (640x480 design): top bar 0..54, bottom bar from y 423, side pillars x -108..1 and 639+, header text at (48,35) teal `0x5cac95`, sub-line yellow `0xffff00`.

**Cloud backdrop motion: UNCONFIRMED, not changed.** `StMenu.alr` holds a 3D model (two tilted layers, 22 and 967 vertices, root at +1190/-250 Y, tilt -12 and -22 degrees) plus a sprite sheet (chunk 0x10: `desktop_00.tga`, `window_mainmenu*.tga`, `title_logo_mini.tga`); the menu scripts set lights/fog (`g2.f0..f5`) and load it via `g2.f80("STMENU")`, but the code that moves it was not found (not in the control handlers, overlay tick, or the 2D draw list `FUN_140198be0`). No menu footage exists. Next step: LIVE capture of two menu frames 1 s apart, or a read of the model draw's matrix source from `FUN_1402a7df0` callers.

**LIVE 2026-10-02 (lane moverec, `engine/re/mr_menu.py`; original running, pads written in memory, sound hook `0x140276330`, video of the main menu):**

- **Menu sounds, confirmed (bank, id):** main menu down x3 = `3` each, up = `3`, A on a row = `0`, B = `1` (Options > B back to main menu = `1`); Multiplayer pop-up: down `3`, A `0`; Xbox Live list: down `3`, A `0`, left / right = **no sound**, B `1` (twice, back out of the list and the pop-up). Press START on the title plays `0` (three times in the first seconds, bank 0). So the table above holds for every row tested (cursor 3, confirm 0, back 1, left/right silent). **Not reached**: Multiplayer > Split Screen's own left / right (the screen waits for "Player 2, select your controller / NEED CONTROLLER", a second real pad) and the arsenal editor (Scenario Mode > A goes straight into the story's first mission, "READY / TIPS / Press ENTER", not into an editor; the arsenal editor is somewhere else and was not found by pad). Scenario > A also plays the loader's **bank 1** ids `2, 7, 7, 2, 7..., 32 x2, 31 x20, 1, 0, 3, 6 ...` (many, then `6` repeating about 3-5 times a second while the mission loads) -- not a menu sound.
- **Main menu picture (video `%USERPROFILE%\.pdtools\moverec\menu1\_master.mp4`):** rows SCENARIO / MULTIPLAYER / OPTIONS / HOW TO PLAY / HELP & SUPPORT / EXIT GAME, header "MAIN MENU_", "Sign-in: <gamertag>", "Skill Update 1.40 applied" bottom right, faint "Phantom Dust" logo under the list, hint bar "Select / Confirm / Back". Options opens a pop-up list Controller / Details / Sound / Display / Mouse / Keyboard over a different backdrop (a teal-lit scene photo, not the clouds); Multiplayer opens Xbox Live / Split Screen / Online Store; the screens behind the Split Screen entry and the Xbox Live list show the same photo-style backdrops. The static scan-line overlay (CRT stripes) and vignette sit on top of all of them and do not move.
- **Cloud backdrop: what animates it (LIVE, 24 consecutive frames x 3 windows, every vertex / pixel constant upload and draw logged).** The StMenu model is drawn by **two indexed draws per frame** from the 3D list flush (`FUN_140298b60`, call site `0x140298bfe`): **60 indices** (the 22-vertex layer) and **5520 indices** (the 967-vertex layer). Across the 24 frames **only one constant changes: vertex-shader constant slot `0x5a` (90), 2 x float4, set just before the 5520-index draw** by the material set-up `FUN_1402a07b0` (call return `0x1402a0a6c`): its six floats are the material record's six int16 fields at `+0x3c, +0x3e, +0x40, +0x42, +0x44, +0x46` times the global frame counter `DAT_14057f398` (an int that `FUN_1401c7630` advances once per program tick) times `1e-5` (`DAT_1402f7c08`). Every other constant (matrices, fog, lights, texture slots) and the 60-index draw are identical frame to frame, so there is **no rotation, no colour and no alpha pulse** in the clouds; the model's tilt (-12 / -22 degrees) is fixed.
- **LIVE scroll values** (the six floats grow linearly; read at program ticks 453..662, 30 Hz): per tick `(+1.0e-4, -2.0e-3, 0, -3.5e-3, 0, -2.8e-3)` = per second **(+0.003, -0.060, 0, -0.105, 0, -0.084)** in UV units. The floats come in a texture-offset pair per layer, so the three pairs read (u, v) = **(+0.003, -0.060)**, **(0, -0.105)**, **(0, -0.084)** UV/s (the pair-to-layer assignment is UNCONFIRMED; the mesh draws that show it is the 967-vertex layer): the clouds scroll **almost purely vertically** (V decreasing, the texture slides in one direction, three speeds ~ 1 : 1.75 : 1.4), with a tiny +u drift on the first pair only. At live tick 640 the six values were exactly (0.1, -2.0, 0, -3.5, 0, -2.8) (counter x int16: int16 fields (10, -200, 0, -350, 0, -280) if the counter step is 1). The counter wraps (`FUN_1401c7630` subtracts `_DAT_1402f83d4` at `DAT_1402f829c`), at which point the offsets jump back; the wrap value was not read.
- **What is not measured:** the sprite sheet (`desktop_00` / `window_mainmenu*`) is drawn by the 2D batch (`FUN_14028caa0` quads, 8 x 24 frames, identical constants every frame): static. The motion could not be recovered from the video by image correlation (the fixed scan-line overlay dominates the correlation); the constants above are the authoritative LIVE numbers. The pad-to-sound log and the constant logs are in `%USERPROFILE%\.pdtools\moverec\menu1..9`.

**Menu cloud scroll (lane camcloud, 2026-10-02): built.** READ `SBTSCLOUD2_vs` / `SBTSCLOUD3_vs` (the only shaders that use all six floats: c90.xyzw + c91.xy; the exact variant StMenu uses is inferred from its 3-texture material, flags 0x43d): three UV sets from the one vertex UV, `uv0 = uv*c12.z + c90.xy`, `uv1 = ... + c90.zw`, `uv2 = ... + c91.xy`, sampling the material's textures 0, 1, 2 = `st3` (cloud colour), `st4`, `st5` (two normal maps; their alpha masks the layers, `SBTSCLOUD2_ps`). So the pairs are: (+0.003, -0.060) UV/s on the colour layer, (0, -0.105) on `st4`, (0, -0.084) on `st5`. READ from `StMenu.alr` model 0 material 0: the fields are (10, -200, 0, -350, 0, -280) = the LIVE numbers (x 1e-5 x 30 Hz). Built in `frontend/cloud.rs` + `cloud.wgsl` (a UI material on the backdrop; colour layer scrolls, bump shading off by default). UNCONFIRMED: the original's light constants (so `PD_CLOUD_BUMP` is A/B only), the vertex alpha, and how many repeats of the 4.6 x 2.7 UV range show on screen (`PD_CLOUD_TILES`, default 1 = old look). `PD_CLOUD_FRAME_DT=<s>` freezes the clock for proof. Proof `shots/cc_clouds.png`.

## Game modes (2026-10-02, lane modes; moved into the settings screens 2026-10-03, lane online)

Menu structure (Laurence 2026-10-03): the title is VERSUS MODE / **ONLINE** / ARSENAL / OPTIONS MODE / EXIT GAME. There is no MODES row and no
`AppState::Modes` screen any more; `frontend/modes.rs` keeps the single `MODES` table ("add a mode = add one entry") and two selectors read it:

- **Local split screen** (VERSUS MODE, setup page 2 "game rules"): the first row is **GAMEMODES**: STANDARD (the normal versus), then every `MODES`
  entry (CLOSE QUARTERS, ...). Left / right cycles and wraps. Choosing a mode runs `SetupState::set_mode`: everyone's arsenal is the mode's
  fixed hand ("FIXED HAND", no arrows, cursor skips it, also on CPU columns), and GAME RULE / STARTING LEVEL / STARTING SKILLS show their fixed
  values dimmed and are skipped. STANDARD restores the normal rows. Character, colour, team, stage, time, rounds, friendly fire and CPUs work in
  every mode. The old "MODE" row (teams / free for all) is now **TEAM PLAY**. The choice is kept across rematches like the other setup picks;
  the header is always "VERSUS SETUP". B / Esc on page 1 goes back to the title.
- **Online** (ONLINE, host's lobby): a **GAMEMODE** row between ARSENAL and STAGE over the same table. The host picks it; the joiner sees it
  read-only (greyed row). It travels in `LobbyRules` (`mode` id + `rules_hash`). In a mode the ARSENAL row reads FIXED HAND on both PCs and is
  locked. Details and the lockstep guarantees: ONLINE-PLAN.md "Gamemodes online".

**Rule line (fixed 2026-10-07):** the teal line under the stage picture used to read the Standard rule's summary ("Standard: ...") while CLOSE QUARTERS was selected. In a gamemode it now names the mode with what it plays as (its rule's Health and time, its own starting Level), `setup::rule_line`; with STANDARD it is the game rule's own summary as before.

**Player memory** (2026-10-08, Laurence: "remember what player 1, 2 etc had selected previously"): each seat's last character, costume colour, arsenal and team are saved in `cache/players.json` when a match starts from the setup (people's seats only; CPU and empty seats keep their older entry), and the first setup visit after the game starts opens every seat on them (`frontend/player_memory.rs`, `SetupState::remembered` / `recall`). Joining still checks them (a colour clash moves the colour, an arsenal the rule refuses moves on); an unknown character or a deleted arsenal falls back to the default. Screenshot runs and `PD_SETTINGS` test runs neither read nor write the file. Within one run the setup already kept its picks between visits.

**Prop Hunt** (2026-10-07, lane prophunt; full design, rules and the choices made: DESIGN-PROPHUNT.md). The third
GAMEMODES entry (STANDARD, CLOSE QUARTERS, PROP HUNT), local only (`GameMode::online = false`: the online GAMEMODE row
uses `modes::step_online` and skips it). Choosing it turns TIME LIMIT into **ROUND TIME** (1:00 / 1:30 / 2:00 / 3:00 /
5:00, default 2:00) and ROUNDS TO WIN into **KILLS TO WIN** (1 / 2 / 3 / 4 / 5 / 6 / 8 / 10, default 5; no round limit, Laurence 2026-10-08), and fixes TEAM PLAY
(SEEKER AND HIDERS), GAME RULE, STARTING LEVEL (N/A), STARTING SKILLS (SEEKER: KNIFE) and FRIENDLY FIRE (N/A), dimmed
and skipped like a Versus mode's rows. The seat arsenal reads SEEKER KNIFE / HIDER NONE. Y adds no CPU in this mode;
`SetupState::problem` refuses a setup with a CPU or with one person, with the reason on the red line. The choice and
the two numbers reach the match in `MatchSetup::mode` / `MatchSetup::prop_hunt`; `MatchSetup::start_mode` turns the
built `Match` into Prop Hunt in `enter_match`. In the match `prophunt_view.rs` swaps `seats::LocalViews` (seeker alone
while hunting), draws the overlay (seeker cover, hider lines, between-rounds card) and the disguises; `check_match_end`
goes to the result screen when the sim says the match is over, and the result screen ranks by kills (`KILLS n`).
`--mode prop_hunt [--ph-time <secs>] [--ph-kills N]` starts it from the command line.

Hidden switches: `--mode <index|id>` (direct to a match, legacy launch). `--open-screen modes` is gone. Design and sim flags: DESIGN-BRAWL.md
"Game modes (built)". Scripted proof of the setup row: `--menu-script wait:40,confirm,p1:a,p1:start,down,left,...` with `--shot-frame 200000`
(the default 300 ends the run before a long script finishes).

## Display options (2026-10-02, lane display; rewritten 2026-10-03, lane settingsux)

Laurence plays on a 4K 55-inch TV and a PC monitor. The Options overlay (Esc / Start in a match, OPTIONS MODE on the title; one overlay, so the title and the in-match screens share all of this) has, on its VIDEO SETTINGS tab (since 2026-10-08, "Options tabs" below), ten rows: **split screen, field of view (slider), camera distance (slider), camera height, display mode, resolution, aspect ratio, HUD and text size, lighting and look**, and Quit to title in a match. Code: `display.rs` (everything testable), `menu.rs` (rows, sliders, browsing).

**How a row changes (2026-10-03, Laurence: "let them switch between the options and select the one they want with A. Not a 'confirm changes' button"):**

- **Display mode, resolution, aspect ratio, HUD size browse.** Left / Right shows the next choice on the row, dimmed, with an `A: APPLY` badge in the row's tab and a help line saying it is not applied yet. **A applies it at once** (and saves). Moving to another row, B, Start or Esc drops the choice and nothing changes. There is no Apply row and no keep-these-settings prompt or countdown (`KeepPrompt`, `KEEP_SECS`, `keep_prompt_tick`, `PD_TEST_KEEP_PROMPT` are gone).
- **Everything else applies as you move**: split, camera height, both sliders and every Lighting and look row. All of it works from the pause menu in a match (the sim is paused, the cameras, layout and HUD are not: `layout_viewports`, `apply_fov`, `follow_cameras`, `layout_hud`, `look::apply_look` run every frame and read the settings).
- **Sliders** (field of view, camera distance): a bar with a filled part and a knob between the label and the value (FOV in whole degrees, distance in %). Left / Right moves it; a tap is one unit, holding repeats and speeds up (1, then 2, 4, 8 per repeat after about 0.7, 1.3 and 2.3 s held: `menu::slider_step`). The value applies as it moves and is written to `settings.json` when the direction is let go (or the cursor leaves the row). Ranges are `menu::FOV_MIN..FOV_MAX` and `CAM_DIST_MIN..CAM_DIST_MAX` (the camera lane owns what they mean).

**What each display row offers** (all sizes are PHYSICAL pixels; `display::resolution_options`, from Bevy's `Monitor`):

- **Display mode:** Windowed / Borderless (a screen-sized window, always the desktop's native size) / Exclusive (takes the monitor over; switches its video mode).
- **Screen (row "SCREEN"; 2026-10-10, Laurence: "make the game open on my 4K TV by default, and a way to pick which screen it opens on"):** `AUTO (BIGGEST SCREEN)` / `SCREEN 1`, `SCREEN 2` ... (`display::Screen`, json `screen`: `"auto"` or `"2"`). It browses like the other display rows (Left / Right shows it, A applies and saves), and applying it **moves the window now**.
  - Auto = the connected screen that can show the most pixels, by its biggest video mode (a TV left at 1080p on the desktop still counts as 4K), the main screen on a tie. On Laurence's rig (laptop panel 2880x1800 + the 4K TV) that is the TV, with no setting saved.
  - Numbers count the connected screens left to right in Windows' display layout; the row shows each one's current size so the TV is easy to tell from the laptop. A number whose screen is not connected (TV off) behaves as Auto and reads `NOT FOUND`.
  - Mechanism: `display::track_screens` keeps the `Screens` list; `apply_display` runs `screen_step` first. Once per change of choice (and once at startup) it moves the window (windowed: centred on that monitor; borderless / exclusive: the fullscreen mode is re-targeted at it), waits for Bevy's `OnMonitor` to report arrival (gives up after 90 frames), then applies the mode and size from the NEW monitor's info. A window the player drags to another screen by hand is left alone.
  - Known blemish: the window is created before Bevy lists the monitors, so on a launch it appears for a moment on the main screen and then jumps to the chosen one. Not yet verified on the real two-screen rig (UNCONFIRMED until Laurence runs it).
  - The VIDEO SETTINGS tab now has 10 rows (11 in a match), so `ROW_PITCH` went from 42 to 41 to keep Quit inside the panel.
- **Resolution:** Native plus
  - *Exclusive:* **every video mode the monitor reports** (distinct sizes of at least 640x480, biggest first), so whatever the TV says it can do is listed, 3840x2160 included; A switches to that size's highest refresh rate;
  - *Windowed:* the usual sizes (4K, 1440p, 1080p, 900p, 720p, the 16:10 sizes, ultrawides, 4:3) **up to the monitor's biggest video mode**, not only its current desktop size, plus the desktop's own size if it is not a usual one;
  - *Borderless:* Native only, shown as `NATIVE (3840 X 2160)`.
  Native means: exclusive = the mode the monitor is in; windowed = the window as it was (shrunk to 90 % of the desktop if it was bigger, see below); borderless = the desktop's size. A saved size the monitor no longer offers falls back to Native when the mode changes. A window size survives a visit to borderless.
- **Aspect ratio:** Auto (fill the window; default), 16:9, 16:10, 21:9, 4:3. A fixed ratio carves a **game area** out of the window (`display::letterbox`, centred, integer maths, black bars; a window already within 3% of the ratio counts as that ratio, so "21:9" fills a 3440x1440 monitor). Everything that used to ask for the window now asks `display::GameArea`: `layout_viewports` (`ViewLayout::rect_in`, all three layouts: side by side, top/bottom, 2x2), `apply_fov`, the HUD (`layout_hud`, the 3-player map quarter), the menu screens and overlay (`ui::fit_frames`), and every non-player camera (HUD, menus, character preview: `display::fit_cameras` sets their viewport to the area). A camera below everything clears the whole window black while bars exist (`BarsCamera`; inactive in Auto, so Auto touches no camera).
  - FOV (reworked 2026-10-03, lane camfov): the setting is the horizontal FOV of a 16:9 view (default 75, range 50-110, json `fov_h16`); each camera's vertical angle comes from its own viewport aspect (`views::fov_for`, `apply_fov`), so a 4:3 or 21:9 area, a top/bottom strip and a side-by-side half all show at least the original's 16:9 picture. Camera distance (`cam_dist_pct`, 70-160) scales the whole boom. See GAME.md "FOV and camera distance settings".
  - Quadrant trap (fixed): `dissolve.wgsl` and `fx_gamma.wgsl` pick a view's per-quadrant value from `view.viewport` origin > 0. With bars every view's origin is > 0 and a pillarboxed side-by-side pair would both count as quadrant 1. The CPU (`views::quadrant_in`) and both shaders now subtract the area's corner (`Dissolve::grain.yz`, `FxGamma::params.zw`, written from `display::area_origin()`).
- **HUD and text size:** 75 / 100 (default) / 125 / 150%. Multiplies the HUD's own fit-to-view scale (`hud_layout::Scales::times`, used in `layout_hud`): panels, names, numbers, the dial and its text, the lock-on readout and the map quarter. Menu screens and the overlay are a fixed 1280x720 design frame already scaled to fill the game area (3x at 4K), so the option does not change them (UNCONFIRMED whether Laurence wants an extra menu zoom).

**Detecting the monitor (first run).** With no saved display choice the game starts **borderless fullscreen on the monitor the window opens on, at that monitor's native physical size, aspect Auto** (`DisplaySettings::first_run`, `startup_mode`, then `apply_display` with `MonitorSelection::Current`). A "first run" is a `settings.json` without `display_chosen: true` whose display keys are absent or all at the old defaults (older builds wrote every key on any save), so an old file with nothing chosen also gets it; any other display value, or the marker, counts as the player's choice (the marker is written the first time A applies a display row). Capture runs stay windowed (`--screenshot`) and never write the file.

**Physical vs logical (audited 2026-10-03).** Everything in `display.rs` is physical pixels: `MonitorInfo.size` (Bevy `Monitor::physical_size`, winit's `MonitorHandle::size`, which is the desktop in physical pixels because winit makes the process per-monitor DPI aware), the video modes, `WindowPlan.size`, `GameArea`. The scale factor is kept separately (`MonitorInfo.scale`) and only `GameArea::logical()` and the UI's design frame divide by it. The one mix-up found was the default window: `Window.resolution = film_window()` (1600x900) is built with `WindowResolution::new`, which takes physical pixels, but bevy_winit's `create_window` hands `window.width()` (physical / scale 1.0) to winit as LOGICAL, so the window opens 1600x900 *logical*: 4800x2700 physical at 300 % scaling, bigger than a 4K screen (3200x1800 at 200 %, which fits a 4K screen but is wider than a 2880x1800 laptop panel: MEASURED on this PC, 2880x1800 at scale 2.0). A windowed run now fits it: `display::fit_window` shrinks a "native" window to 90 % of the desktop (`plan`), and `default_window_size` gives the 1600x900-logical default as physical for the monitor's scale. A size the player picked is never shrunk.

**The monitor follows the window.** `track_monitor` re-reads the monitor under the window every frame. It used to wait for `Added<OnMonitor>`, but Bevy re-inserts `OnMonitor` when the window is dragged to another screen (a change, not an addition), so the lists stayed on the first monitor (BUG-GRAVEYARD "Resolution list stuck on the first monitor").

**What his TV reports.** Once the monitor is known the game logs one line to stderr and writes it to `display_log.txt` next to `settings.json`: `display: monitor 3840x2160 scale 3.0 (window scale 3.0) largest mode 3840x2160@120 modes [3840x2160@120/60 ...]` (monitor desktop in physical pixels, its scale factor, the biggest video mode, every size with its refresh rates). One line per monitor the window is on; capture runs write no file. (`PLAY.bat` starts the game without a console, so read the file.)

**Persistence:** `settings.json` keys `display_mode` (windowed|borderless|exclusive), `resolution` (native|WxH), `aspect` (auto|16:9|16:10|21:9|4:3), `hud_scale` (75|100|125|150), `display_chosen`, `fov_deg`, `cam_dist_pct`; missing or junk values load as defaults. A display choice is written the moment A applies it; sliders when let go. `PD_SETTINGS=<file>` points a run at another settings file (the lanes' captures use it; a `--screenshot` run never writes one).

**Capture / test switches:** `PD_TEST_ASPECT=auto|16:9|16:10|21:9|4:3`, `PD_TEST_HUD=75|100|125|150` (never saved); `--open-menu` opens the overlay at start; `--menu-script` now also drives the overlay (`start` opens it in a match, `up down left right confirm cancel` move it, `shot:<name>` takes a picture; tokens are taken by the overlay while it is open or a match is on). A 4K capture is `PD_WINDOW=1920x1080` on a 200%-scaled monitor (physical 3840x2160) or `3840x2160` where the scale is 100%.

**Sim untouched:** nothing under `sim.rs` / `combat*` / `moves/` changed; the default area is the window and no camera is touched.

**Not verifiable offscreen:** real borderless / exclusive switches and the OS video-mode change (only the mapping to Bevy's `WindowMode`, the option lists and the tests with a mocked 4K TV are covered), and what Laurence's TV reports (see the log line above). Laurence should try each mode once on the TV and send `display_log.txt`.

**Tests:** `display::tests` (lists for a 4K TV whose desktop is 4K or 1080p at scale 1, 2 and 3; exclusive switches to the 120 Hz 4K mode; `Monitor` to `MonitorInfo` keeps physical pixels; the fitted default window; first-run defaults and the settings-file rules; the monitor follows a window dragged to another screen; the log line), `menu::tests` (sliders, browse steps, apply rules), `pause_menu_input_tests` in `main.rs` (browse then A, browse then leave, every display row, sliders tap / hold / save-on-release, every main row and every Look row in a match, split / FOV / tone reach the cameras while paused, the menu script drives the overlay; the P1 d-pad and stick tests are unchanged).

**Measured on this PC (2026-10-03):** the startup line read `display: monitor 2880x1800 scale 2.0 (window scale 2.0) largest mode 2880x1800@120 modes [...]`: the monitor size is the physical 2880x1800 (not the logical 1440x900 PowerShell reports), so the size is not DPI-scaled.

## Look options: the Lighting and look page (2026-10-03, lane look)

Optional modern lighting, on a second page of the Options overlay (the main page's tenth row **LIGHTING AND LOOK**, whose value shows the current preset; confirm opens the page). Code: `look.rs` (everything testable), `menu.rs` (page, rows, help line), `glow.rs` (`GlowParams`), `shadows.rs` (`original_filter`, `map_size` made public), `char_light.rs` / `.wgsl` (extra effect lights). **Every default is the original's look**: a settings file without the new keys, or no file, plays as before (a default capture against the pre-change build differs by 0.6 % of pixels, the harness's own run-to-run noise of 0.9 %).

| Row | Choices (default first) | What it switches | Cost (help-line note) |
|---|---|---|---|
| LOOK PRESET | ORIGINAL / ENHANCED / MAX (reads CUSTOM when the rows below match none) | sets the rows below at once | sum of its rows |
| BLOOM | OFF / LOW / HIGH | `glow.rs` per-view bright-pass + two blur layers added back; HIGH = the glow lane's tuned values, LOW = half the halo strength | about +0.5 to +1.5 ms with 4 views at 1080p, not measurable with 2 (HIGH); LOW about half |
| SHADOW QUALITY | ORIGINAL / HIGH | ORIGINAL: hardware 2x2 filter, 2048 px maps (`PD_SHADOW_FILTER` / `PD_SHADOW_MAP` still apply). HIGH: Gaussian filter + 4096 px maps. The original's darkening (colour, N.L weight) is the same either way and stays on | about +0.3 to +1 ms with 4 views, not measurable with 2 |
| TONE CURVE | ORIGINAL / FILMIC | Bevy's default curve (or `PD_TONEMAP=none`) / AgX on every player camera | negligible (+0 to +0.7 ms, inside the noise) |
| EFFECT LIGHTS | ORIGINAL (1) / 4 PER FIGHTER | the program's character shader has ONE point-light slot (the strongest playing LIGHT02 decal, `char_light::gather_effect_lights`); the option sums the 4 strongest into the same maths (`char_light.wgsl::point_light`, `CharLightUniform::x_col/x_pos`). The stage keeps its one light | negligible (+0 to +1.2 ms, inside the noise; one more loop of 3 cheap lights in the fighter shader) |

- **Presets:** ORIGINAL = all defaults. ENHANCED = bloom LOW, shadows HIGH, effect lights 4, tone ORIGINAL. MAX = bloom HIGH, shadows HIGH, effect lights 4, tone ORIGINAL (Laurence 2026-10-03: FILMIC greys the dark stages, so it stays a row of its own and is in no preset).
- **Rows apply at once and save** (settings.json keys `look_bloom`, `look_shadows`, `look_tone`, `look_lights`; unknown or missing values load as the default per key). No keep prompt: nothing here can strand the player. The same keys as the other rows: Up/Down select (move sound), Left/Right change (silent, like every spinner row), A / Enter on BACK or B / Backspace returns to the main page on the LIGHTING AND LOOK row, Esc / Start close the whole overlay. The bottom line of the panel shows what the highlighted choice does and what it costs (`look::help_text`).
- **Anti-aliasing is deliberately not an option** (Laurence, 2026-10-03): the game keeps the 4x MSAA it has always shipped, and nothing here touches a camera's `Msaa`. No FXAA / SMAA / TAA.
- **Not offered, and why** (read from Bevy 0.19.1): SSAO only darkens the ambient term of Bevy's PBR lighting, which the unlit stage and the SWBOS fighters never use (and it needs MSAA off + prepasses); contact shadows, volumetric fog and clustered lights are PBR-light features of the same kind.
- **Tone curve choice:** a six-way sheet on Lost Palace (ORIGINAL, ACES-fitted, AgX, Blender Filmic, SomewhatBoringDisplayTransform, ReinhardLuminance; mean absolute pixel change from the original 24.9 / 8.5 / 3.1 / 3.0 / 3.2 of 255). ACES darkens this game's already-clamped picture too hard, the last three barely change it, AgX is the visible-but-usable one. **It still darkens and greys the dark stages** (`engine/shots/look_dark_st03.png`, MAX row): that is why ENHANCED leaves the tone curve alone. Laurence chose (2026-10-03) to leave FILMIC out of MAX.
- **Capture switches** (never saved): `PD_TEST_LOOK=original|enhanced|max`, `PD_TEST_BLOOM=off|low|high`, `PD_TEST_SHADOWS=original|high`, `PD_TEST_TONE=original|filmic`, `PD_TEST_LIGHTS=original|more`, `PD_TEST_LOOK_PAGE=<row>` (overlay open on the page); `PD_GLOW=0|1` still wins over the bloom row, `PD_SHADOWS=0` removes the shadow lane (the row then does nothing), `PD_LOOK_LOG=1` logs what is applied.
- **Proof** (local, not committed): `engine/shots/look_dark_st03.png` (Lost Palace, ORIGINAL / ENHANCED / MAX), `look_bright_st22.png` (Sunlight Highway, same), `look_effect_moment.png` (both fighters cast Big Bomb, ORIGINAL vs MAX at two ticks), `look_menu_page.png` (the page, ENHANCED, help line under the rows), `look_four_views_max.png` (MAX in 4 views, 1920x1080 window; all views draw, HUD intact). The overlay freezes the sim, so a menu capture needs `--shot-frame N`, not `PD_SHOT_TICKS`. Checked with 2 views at 1600x900 logical (3200x1800 physical); 4 views with MAX at 1920x1080 checked by eye; the 4K cost runs (`--players 4`, window 1920x1080 on a 200%-scaled display = 3840x2160 physical) ran every option with no render error.
- **Cost** (`PD_FRAME_STATS`, st03, `--demo --near`, debug-optimised build, this PC's GPU shared with other lanes' runs, so treat as order of magnitude): 2 views: every option and MAX measured 16.6 to 16.7 ms median, the same as ORIGINAL, at both 1080p and 4K physical: the capture window's 60 Hz pacing is the limit there, so the cost is hidden under it (no headroom number from this harness). 4 views at 1080p (quiet rounds, median frame time, ORIGINAL 17.6 to 18.2 ms): bloom HIGH +0.5 to +1.4, shadows HIGH +0.3 to +1.0, tone FILMIC 0 to +0.7, effect lights 0 to +1.2, MAX +1.0 to +2.5 ms. 4 views at 4K: all within the run-to-run noise (17 to 19 ms medians in the quiet rounds, no option visibly above ORIGINAL), i.e. this PC's GPU is not the limit in these scenes. Other lanes' film runs were using the GPU for most of the session: rounds taken while they ran gave medians from 40 to 800 ms and were thrown away, only the quiet rounds are quoted. The help-line notes quote these ranges. **UNCONFIRMED on Laurence's own GPU and TV**
- **Tests:** `look::tests` (defaults = original, option lists, preset mapping both ways, row stepping, JSON round trip and junk, a real settings-file round trip, help text), `pause_menu_input_tests` (open the page, rows wrap on the page's own length, B / BACK / Start, left/right change the look and leave the main rows alone), `char_light::tests` (extra lights add, the default count ignores them, ranking).
- **Not verified:** the look on a real 4K TV and with a pad (the pad path is the same code as the keyboard and is covered by the Rig tests); the real frame cost on Laurence's GPU.


### Settings row: VIBRATION (lane rumble, 2026-10-03)

Pause / options menu, row after LIGHTING AND LOOK and before QUIT TO TITLE: `VIBRATION < ON | OFF >`, left / right toggles at once and saves (`Settings::rumble`, settings.json key `"rumble"`, default on). Off = no rumble requests at all; see docs/RUMBLE.md. Row pitch is now 42 px (11 rows). **Moved 2026-10-08 to the CONTROLS tab's first row** (same behaviour; "Options tabs" below).

## Options tabs: VIDEO SETTINGS / SOUND / CONTROLS (2026-10-08, lane settingstabs)

Laurence: "Put the current video settings under 'Video Settings' tab, then add a 'Sound' and 'Controls' settings tab too", then "add button remapping to the controls tab too". The same overlay as before (title OPTIONS MODE, or Esc / Start in a match); code `menu.rs` (tabs, rows, pages), `mixer.rs` (volume), `bindings.rs` (mapping), `gather_input` in `main.rs` (camera settings and mapping applied).

**Tab strip** under the OPTIONS_ header: `LB  [VIDEO SETTINGS] [SOUND] [CONTROLS]  RB`; the showing tab is lit in the selected-row red with yellow text, the others dark teal. **LB / RB on any pad, Q / E on the keyboard** (`--menu-script` tokens `lb` / `rb`) switch left / right and wrap at both ends; the new tab opens on its first row, a browsed display choice is dropped and a slider not yet written is saved; the program's switch-list sound plays (common/4, `Cue::List`, `FUN_140161fa0`). A switch from the Lighting and look or button-mapping page lands on the other tab's own rows. The overlay remembers its tab when it closes. **Quit to title is the last row of every tab** in a match (A on it, as before).

| Tab | Rows (default) | Notes |
|---|---|---|
| VIDEO SETTINGS | split screen, field of view (slider), camera distance (slider), camera height, display mode, resolution, aspect ratio, HUD and text size, lighting and look `>` | exactly the old main page without VIBRATION: same order (so every old row index is unchanged), same browse-then-A display rows, sliders, first-run auto-detect, Look page |
| SOUND | MASTER VOLUME, MUSIC VOLUME, EFFECTS VOLUME: sliders 0-100 (100) | applied live (see below) |
| CONTROLS | VIBRATION (moved here, same behaviour), INVERT CAMERA Y `< OFF >`, CAM SENSITIVITY slider 50-200 % (100), BUTTON MAPPING `DEFAULT / CUSTOM  >` | the label is CAM SENSITIVITY because "CAMERA SENSITIVITY" in the pixel font is wider than the space left of the slider bar; the help line spells it out |

**Volume** (`mixer.rs`): every sound the game starts carries `mixer::Mixed { channel, base }` (music: `music.rs`, base 1.0; effects: `sfx.rs` one-shots, travel and environment hums, voices, and `frontend::sound` menu cues). One system (`mixer::mix`, PostUpdate before transform propagation, so before Bevy's audio start) sets a new sound's `PlaybackSettings` volume to `base x gain` and, when a slider moves, every playing sound's `AudioSink` to its new level (times `GlobalVolume`, so `PD_MUTE` still mutes). `gain` = `slider_gain(master) x slider_gain(channel)`, `slider_gain(p) = (p / 100)^2` (an audio taper: 50 sounds about half as loud); at 100 / 100 it is exactly 1.0 and nothing is touched, so the defaults are the old loudness bit for bit. EFFECTS covers fight sounds, voices and menu cues.

**Camera** (`Settings::shape_look`, called in `gather_input` on the players' own keyboard and pad look input, before the scripted `PD_TEST_*` switches and before quantising): the turn (right stick x, Q / E) is multiplied by sensitivity and held to -1..1, then quantised into `sim::Input::cam` as before, so online play sends the shaped number and both PCs step on it (settings are per PC; only the quantised `Input` travels). The sim's top turn speed (`CAM_TURN_SPEED`) is the original's and lives in the sim, so above 100 % the camera reaches that top speed with less stick but never turns faster. Look up / down (right stick y, R / F) is render-only (`CamPitchInput`): multiplied in full (200 % = twice as fast) and flipped by INVERT CAMERA Y. The locked-on camera reads neither (unchanged). 100 % and off return the input unchanged (tested bit for bit).

**Button mapping** (CONTROLS > BUTTON MAPPING, a page like Lighting and look): ten rows, the eight remappable actions with their `PAD  /  KEY`, then RESET TO DEFAULTS and BACK.

| Action | Pad (default) | Keyboard P1 (default) |
|---|---|---|
| DODGE | L3 (click left stick) | SPACE |
| DODGE (2ND) | LB | - |
| SKILL LEFT / RIGHT / TOP / BOTTOM (also take a capsule) | X / B / Y / A | J / K / U / I |
| LOCK ON | RT | Y |
| SWITCH TARGET | LT | T |

- **A on an action row listens** (row badge `LISTEN`, value `PRESS A BUTTON OR KEY`): the next pad button (any pad) or key becomes that action's. A button / key another action had **swaps** with it, so no action is ever without a pad button. A key for an action that has none (DODGE (2ND)) that another action uses is refused (it would leave that action keyless) with the help line saying so. Fixed buttons and keys are refused with the denied sound (common/2) and listening goes on: Start, the D-pad, the sticks' axes, W A S D, Q E R F, the arrows, Esc, Enter, Backspace, Tab, Right Shift, the numpad (keyboard player 2) and F1-F12 (dev tools). **Esc or Start cancels; after five seconds (300 frames) it stops by itself.** While listening, every press goes to the mapping (A, B, LB, RB can be bound like any other button). The help line names the result ("DODGE is now X on the pad.").
- **One mapping for every pad** (no per-player profiles) plus keyboard player 1's keys. RESET TO DEFAULTS puts the old layout back. B / Backspace (or BACK) returns to the CONTROLS tab on the BUTTON MAPPING row.
- **Not remappable:** moving and the camera (sticks and axes), clicking the right stick (recentre), the D-pad / arrows (skill cards and every menu), Start / Esc (pause: nobody can lock themselves out), the keyboard's second player (Enter, Right Shift, numpad 1 2 4 5; a test fallback). **The menus never read the mapping:** A / B / Start / D-pad and Enter / Space / Backspace / Esc / arrows navigate every screen and this overlay whatever is bound.
- **Where it applies:** `gather_input` reads jump, lock, switch target and the skill bits through `Settings::bindings` (pressed / just-pressed exactly as the old fixed code did) before anything is quantised into `sim::Input`; the sim, replays, the dev-tools input log and the online lockstep only see that `Input`. The default mapping gives a byte-identical `Input` for every button and key (tested).

**Persistence:** new `settings.json` keys `vol_master`, `vol_music`, `vol_effects` (0-100), `invert_cam_y` (bool), `cam_sens_pct` (50-200), `bindings` (`{"dodge": {"pad": "LeftThumb", "key": "Space"}, ...}`, Bevy's button / key names, `"key": null` = none). Missing keys (every file written before this lane) load the defaults = the old behaviour; junk or out-of-range numbers load the default or the nearest end; a `bindings` entry with an unknown name, a fixed key or a doubled button loads the whole default mapping (never a broken one). Toggles and bindings save at once, sliders when let go.

**Capture switches:** `PD_TEST_OPTIONS_TAB=video|sound|controls|buttons` opens the overlay on that tab (buttons = the mapping page); `--menu-script` `lb` / `rb` switch tabs, and while the mapping page listens `a b x y lb rb` are bound as those pad buttons (`start` cancels).

**Layout** (MENUS.md "Options overlay rows"): the tab strip is at panel top + 58 (32 high), rows start at + 100 (was + 66) at the same 42 px pitch; at most ten rows (VIDEO SETTINGS in a match: nine + quit; the mapping page: ten).

**Tests:** `menu::tests` (Video rows unchanged in order, rows per tab, quit only in a match, tab stepping and wrap, new settings default + round trip incl. an old file and junk, `shape_look` identity at 100 % / off and its scaling / clamp / invert, new rows change their settings), `mixer::tests` (defaults bit-identical levels, channel and master scaling, the real `mix` system levels new and waiting sounds), `bindings::tests` (defaults = old buttons, swap rules, fixed buttons / keys refused, json round trip and bad files), `pause_menu_input_tests` (LB / RB / Q / E / player 2's pad switch both ways and wrap, quit on every tab, vibration moved to CONTROLS, sound sliders save on release, byte-identical `Input` for every pad button and key with the defaults, sensitivity / invert through the real `gather_input`, listen / bind / swap / refuse / cancel / timeout, reset and back, a remapped jump and skills drive the `Input` while menus keep A / B / Start, the menu script switches tabs and binds). Every old Video-row test runs unchanged.

**UNCONFIRMED (no screen, no audio device here):** the look of each tab (tab strip text fits, the slider bars on the Sound and Controls rows, the mapping page values), the live volume change on playing sounds (the sink path; the spawn path is tested), whether the squared volume curve feels right. Needs a PC screenshot of each tab: `PD_TEST_OPTIONS_TAB=video|sound|controls|buttons pd_game <cache> --demo --near --screenshot shots/tab_<name>.png --shot-frame 120` (the overlay pauses the sim, so use `--shot-frame`), or `--menu-script start,rb,shot:sound,rb,shot:controls,down,down,down,confirm,shot:buttons`.

## Three-player layout (2026-10-03, lane split3)

Laurence: "3 player split screen must give player 3 the entire bottom half of the screen."
- **Layout** (`views::ViewLayout`, `Kind::Trio`): P1 top-left, P2 top-right, P3 the whole bottom half edge to edge (a view that spans every column takes the whole width, so it reaches both edges of an odd-sized window). Four players stay 2x2; one and two players are the old numbers (`two_player_layouts_equal_the_old_numbers`). The 2x2 grid with the map and scoreboard quarter (2026-10-01) is gone: no map, no scoreboard, no quarter code (it was a UI overlay on the HUD camera; there was no separate overview camera).
- **Views are not all the same size any more.** `rect` / `rect_f` / `rect_in` take the view index; `aspect_of(i, window)` is the per-view aspect (`aspect(window)` is view 0's); `is_grid()` = Quad or Trio (the HUD's small-cell scales). P3 is 32:9 at a 16:9 window. (Resolved by the camfov merge, 2026-10-03: `apply_fov` now takes each camera's own viewport aspect, `views::viewport_aspect` + `views::fov_for`, so P3's wide view gets Hor+ like any strip; GAME.md "FOV and camera distance settings".)
- **Quadrant ids.** The shaders (`dissolve.wgsl`, `fx_gamma.wgsl`) only read where a viewport STARTS: P1 quadrant 0, P2 quadrant 1, P3 (left edge of the bottom half) quadrant 2, quadrant 3 unused; with bars round the game area the origin is taken relative to it. Tests `a_views_quadrant_is_its_viewport_origin`, `the_wide_view_sits_in_the_bottom_half_of_the_game_area`. The near-wall dissolve and the lock-on markers need nothing else (dissolve is world-distance based; markers anchor with `world_to_viewport` in the camera's own viewport).
- **HUD** (`hud_layout::hud_view`, `hud::HudScale::of(player)`): each player's HUD is fitted to ITS OWN view (before: one scale for all). The scale follows the view height (the HUD is drawn for 16:9, a wide view is height-limited), so P3's HUD is as big as P1's and P2's: panels at the left, dial at the bottom right of the wide view (and of the window), capture hint centred in the free width left of the dial's ribbons (as in a top/bottom strip). Lock-on marker sprites size by their own view's height.
- **Frame cost, 3-4 views** (`perf_probe.rs`, `PD_FRAME_STATS` / `PD_GPU_TIMING`): see WAVES.md "split3". `PD_SHADOW_CASCADES_TRIM=1` (off by default, look changes) drops the two shadow lights from 4 + 3 cascades to 2 + 2 from three players (`shadows::cascades_for`).

## De-clutter wave (2026-10-09, UI lane; Laurence's "modern comfort" + "de-clutter the space")

Items 1-13 of `UI-DECLUTTER-PROPOSALS.md`, approved by Laurence (DECISIONS 2026-10-09). The character bios went first (same day, MENUS.md).

- **Keyboard keys line** (`ui::KeyLine`, `ui::plugin`, `ui::KeyboardInUse`): the small grey line under every prompt bar (title, setup pages, result, online, arsenal builder) is hidden until a keyboard key is pressed and hidden again by any gamepad button. The title and result have their own lines (`KEYS_TITLE`, `KEYS_RESULT`) instead of the setup's.
- **Title**: one short line per row, no "Local versus" caption, no B prompt (B does nothing there).
- **Character select**: one hint line. The status line above it shows only problems (red) and, on an empty screen, the join prompt; the description line carries " PLAYER 1: Start goes on at once." while someone is not ready (`setup::start_hint`). The Team line (and its caption) is hidden outside teams and a person's ready banner moves up into its place (`seat_line_hidden`, `seat_slot`). A CPU's grade shows only on its Strength line (no grade beside the header; the preview strip says just "CPU"), described in words (`cpuseat::describe_grade`: "Grade 3 of 5: average. Goes for the nearest opponent."). The add-CPU hint shows only on the seat Y fills ("Y: ADD A CPU"; "A: ADD A CPU" on a seat your cursor is on).
- **Game rules**: Friendly fire is hidden in free-for-all and Start battle moves up (`shared_hidden`, `shared_slot`; the cursor skips it and a cursor on it moves to Start when teams go off). The by-rule time reads "10 MIN (RULE)"; the seat recap drops the colour; no description on Start battle (the A prompt says it).
- **Result**: the character names head the stats columns (no line under the cards; TEAM n stays in teams), no "-" tab on a draw, one "A Continue" prompt (B still continues).
- **Online (Choose page)**: one explanation line, a plain "Steam isn't available: start Steam, then restart the game." ("Connecting to Steam..." while it starts), no BACK row (B goes back), no Change prompt; the Steam host page's VISIBILITY value shows its arrows.
- **Arsenal list / builder**: the list page drops its cursor counter and "(Left/Right: page)"; the edit page's detail panel no longer repeats the name and kind line (the header has them); TYPE and SCHOOL list only non-zero counts (`builder::detail_body`, `count_lines`).
- **Options overlay** (`menu.rs`): one short help line for the highlighted row (`tab_help`; no key list, no "saved automatically"); prompts per page (`Prompts`: Video Select/Change/Apply/Close, Sound and Controls Select/Change/Close, Lighting and look Select/Change/Back, Button mapping Select/A Change/B Back); the button-mapping page has no BACK row (`BUTTON_ROWS` 9); the panel ends under the page's last row with the prompts and help under it (`PanelPart`, `panel_height`).
- **Proof**: before / after pairs `engine/shots/declutter/built/ba_*.png` (local, game art); raw shots `shots/uiaud_*` (before) and `shots/uia2_*` / `uia3_*` (after). Tests: `ui::key_line_tests`, `builder::tests::detail_panel_leaves_out_zero_counts...`, `cpuseat::tests::a_grade_describes_itself_in_plain_words`, the menu / setup tests updated for the removed rows.
