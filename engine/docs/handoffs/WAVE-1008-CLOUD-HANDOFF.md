# Handoff: finish the cloud sessions' work on the PC (2026-10-08)

**Who this is for:** a Claude session on Laurence's Windows PC (the main checkout with `game_files/`, `engine/cache/`, the GPU and the MinGW toolchain). Read `AGENTS.md`, `engine/docs/INDEX.md` and the `pd-build-run`, `pd-visual-verify`, `pd-film-compare` and `pd-laurence-bug` skills first. Then do the steps below **in order**. Report to Laurence the way AGENTS.md says: plain English, outcome first, pictures for anything visual.

**Why:** two cloud sessions (2026-10-07 and 2026-10-08) built everything below on Linux with no game data and no GPU. All of it compiles and passes the tests that can run there. **None of it has been seen, heard, timed or played.** Laurence can't see any of it yet: PLAY.bat runs `engine\play\pd_game.exe`, which only this PC can rebuild. He has asked twice where Prop Hunt is.

## What is on the branch

Branch **`claude/laughing-fermi-91wzrp`** on origin. It is based on main `e730169` and has not been merged.

| Area | What | Where it is described |
|---|---|---|
| Setup | The rule line under the stage picture names the gamemode (it used to say "Standard" in Close Quarters). | FRONTEND.md "Game modes" |
| Setup | Player 1 (the first person seated) backs out to the title with B even when others have joined. | DECISIONS 2026-10-08 |
| Setup | **Player memory:** each seat's character, colour, arsenal and team are saved in `cache/players.json` when a match starts and recalled on the first setup visit of a run. | FRONTEND.md "Player memory", `frontend/player_memory.rs` |
| Stairs | The Palace pocket's cross wall stands one wall radius past the head line. Before, a fighter stopped with about 5 u of air over his head. | COLLISION.md "Head line", BUG-GRAVEYARD |
| Performance | Round 1 = `lane/perf`, merged: FighterBlock, PieceWrite, FxUniform. Round 2 = lane fxperf: effect preload, material cache and pool, meshes rewritten in place, pipeline warm-up, env switches read once, `[fx-churn]` stats. | PERF.md, BUG-GRAVEYARD "Frame drops when casting" |
| Prop Hunt | A new local 2-4 player mode, the third GAMEMODES entry. ROUND TIME (default 2:00, started at the seeker's release, so a round is 2:12) and KILLS TO WIN (default 5) rows. No round limit. | DESIGN-PROPHUNT.md, FRONTEND.md, DECISIONS |
| Options | Tabs VIDEO SETTINGS / SOUND / CONTROLS. A volume mixer. Camera invert and sensitivity. A button mapping page. | FRONTEND.md "Options tabs", GAME.md "Settings menu" |
| Docs | Archivist pass 2; every new lane has a WAVES / DECISIONS / BUG-GRAVEYARD entry. | ARCHIVIST.md |

Every new behaviour has an off switch, so you can bisect:

| Switch | What it turns off |
|---|---|
| `PD_FX_PRELOAD=0` | effect preload |
| `PD_FX_MATCACHE=0` | material cache and pool |
| `PD_FX_MESHREUSE=0` | meshes rewritten in place |
| `PD_FX_WARMUP=0` | pipeline warm-up |
| `PD_STAIRS_ORIGINAL=1` | the opened Palace stairs (restores the original collision) |

Other things to know:
- **Cloud test state** (Linux, last run): `cargo test -p pd_game --bin pd_game` gives 1211 passed and 4 failed. The 4 failures are known and Linux-only (TOOLCHAIN.md "Building on Linux"):
  - `combat::tests::replay::{one_v_one_replay_hash_is_unchanged, one_v_one_movement_only_hash_is_unchanged, one_v_one_the_target_button_changes_nothing}`. The goldens are Windows hashes. The Linux hashes stayed byte-identical through every merge on the branch, so **the Windows goldens should still pass**.
  - `sfx::tests::the_footstep_wav_is_in_the_cache...` (needs the cache).
- **Tests that skipped in the cloud** because they need the cache: `stairs_under` (real st03 file), anything that loads `cache/`. They run for real here.
- **Cargo.lock** records `bevy_gilrs` as the vendored path source (86b6000). If Windows cargo rewrites it back, commit whatever cargo writes. Don't hand-edit it.

## Step 1: merge (no pictures yet)

1. `git status` in the main checkout. Leave untracked lane files and `handoffs/WAVE-1003-STATE.md` edits alone. If main has local commits not on origin, say so before merging.
2. Run `git fetch origin claude/laughing-fermi-91wzrp`, then `git merge --no-ff origin/claude/laughing-fermi-91wzrp` into **main**. Main should still be `e730169` plus whatever this PC added. Resolve conflicts by hand, keeping both sides; docs are append-style.
3. Build: `cargo build -j 2 -p pd_game` and `-p pd_viewer`. The viewer compiles `effects.rs` too.
4. Run **every** suite: `cargo test -p pd_game -p pd_formats -p pd_convert -p pd_viewer` plus the Python tests under `engine/re` the way VERIFY.md says. **All must pass, including the three replay goldens and the footstep test.**
   - If a replay golden fails on Windows: it is NOT expected (the Linux hashes never moved). Bisect over the first-parent commits listed in `git log --first-parent e730169..origin/claude/laughing-fermi-91wzrp`. Suspects in order:
     1. fd031ed (prophunt: `Combat::rounds_by_mode`, `Match::step` input filter; both are meant to be no-ops outside Prop Hunt);
     2. c33be69 (env reads once);
     3. the settingstabs merge (`gather_input` bindings, sensitivity).
   - Fix the cause; don't re-record the goldens unless the change is meant to alter play. None of these are meant to.
5. Don't push yet.

## Step 2: performance (Laurence: "frame drops when casting moves")

Follow PERF.md "Round 2" exactly. Use mains power, close other lanes, and read the PERF.md notes about battery noise.

- Run a 4P and a 2P demo fight with `PD_FRAME_STATS=1800,200`, A/B:
  - (a) all of `PD_FX_PRELOAD=0 PD_FX_MATCACHE=0 PD_FX_MESHREUSE=0 PD_FX_WARMUP=0`;
  - (b) none of them;
  - (c) one switch off at a time.
- Then run `600,0` for the first-cast hitch. Compare mean, p99, max and the `[fx-churn]` line.
- Run `lane/perf`'s round-1 table again on mains power. It was only ever measured on battery.
- **Picture identity:**
  - Fixed-tick captures, 2P and 4P at ticks 300 / 900 / 1500, old main exe vs the merged exe.
  - A `film_compare.py` before/after run on a dozen effect-heavy skills: Rapid Cannon, Photon Burst, Bomb, a blade skill, a ribbon skill, an aura.
  - Effect randomness gives 0.2-5 % pixel noise between reruns (PERF.md). Anything clearly beyond that is a regression: bisect with the switches, set that switch's default to off, and write it in BUG-GRAVEYARD.
- Write the measured table into PERF.md (replace "UNCONFIRMED" with numbers and the date).

## Step 3: screenshots for Laurence before he plays

Use `pd-visual-verify`. Look at every picture yourself before sending it.

1. **Prop Hunt** (DESIGN-PROPHUNT.md "Needs a PC check"). Run `--mode prop_hunt --p1 pc00a --p2 pc01a [--ph-time 60] [--ph-kills 2]`, shots before and after frame 720 (the end of the 12 s countdown). Check:
   - the black seeker cover with its countdown;
   - the hider lines ([DISGUISED] / [FROZEN]);
   - the seeker's view going full window and the hiders' views going black after release;
   - the between-rounds card, the result screen KILLS;
   - a disguised hider showing as the right prop model (it depends on `propNNN_*` node names; UNCONFIRMED), with the fighter, shadow and aura hidden;
   - a knife catch showing the hider turning back into a fighter and the knockdown;
   - Psycho Knife aimed without lock-on reaching and logging a hit (skill 3);
   - overlay text size at 4K (it uses Bevy's default font, not the menu pixel font: say so to Laurence).
2. **Options tabs:** `PD_TEST_OPTIONS_TAB=video|sound|controls|buttons --demo --near --screenshot shots/tab_<name>.png --shot-frame 120`. Check that the tab strip text fits, the slider bars, the "CAM SENSITIVITY" label, and the button mapping page values.
3. **Stairs:** P1 under a big Palace flight, walked in from the hall end until it stops: head just under the soffit, no gap. Make a before/after sheet using `PD_STAIRS_ORIGINAL=1` for "before".
4. **Setup:** player memory reopens last run's picks; player 1's B leaves to the title with others joined; the Prop Hunt rows (GAMEMODES = PROP HUNT, ROUND TIME, KILLS TO WIN) and the rule line text.

## Step 4: refresh Laurence's copy and hand over

1. Refresh `engine\play` per `pd-build-run`:
   - build, then copy `pd_game.exe`;
   - make sure `steam_api64.dll` and `steam_appid.txt` from `engine\vendor\steam\` are next to it.
2. Prove it by filming one take with `PD_GAME_EXE` set to that copy.
3. Tell Laurence what to try, one line each:
   - PLAY.bat → 1 → VERSUS MODE → page 2 GAMEMODES → PROP HUNT;
   - Options tabs (LB/RB) and button mapping;
   - walking under the Palace stairs;
   - casting moves to judge the frame drops.
   - Ask him to check by ear: the volume curve (squared; 50 should sound half as loud) and a live music volume change.
   - Ask him to try on his pad: remapping by press and the 5 s timeout.
4. **Push:** Laurence's push permission on 2026-10-03 was for that wave. Ask him before pushing main to origin.

## Still open (not done in the cloud: do them, or put them in front of Laurence)

- **Effect batching:** per-instance effect data (MeshTag index into a storage buffer), so effect pieces batch. This is the remaining `render(encode)` cost in PERF.md. It needs a GPU to verify the `fx_gamma.wgsl` colour and UV path. Aura wisp materials are not pooled yet (a one-frame stale-colour risk in `PostUpdate`).
- **Camera sensitivity above 100 %:** it doesn't raise the top turning speed. That limit is in the sim. A small sim change would allow it: ask Laurence whether he wants it.
- **Prop Hunt polish:**
  - the overlay uses plain text, not the original menu art;
  - the disguise copy is one frame late;
  - a caught hider's knockdown starts one tick late;
  - the "START" banner still flashes at round start;
  - if nobody ever catches anyone, the match never ends (by design, Laurence's "no round limit"; players quit from the pause menu).
- **Small cleanups:**
  - `hud.rs::layout_hud` has an unused `local` parameter (`players` warning);
  - `turn_clip_seconds` and `hand_line` are dead code;
  - ARCHIVIST.md "Pass 2 findings" lists the rest.
- **Archivist pass 3** after this merge: NEXT-SESSION top block, AGENTS "Current state", INDEX lines for DESIGN-PROPHUNT.md / PERF.md / this file, the test count from your Windows run.

## Waiting on Laurence (carried over)

- Psycho Knife square.
- Top/bottom split-screen framing A/B.
- The side-by-side letterbox question.
- Online between two PCs.
- `display_log.txt` from the TV.
- A live session with the original for the UNCONFIRMED gameplay residue (`pd-live-original`).
- The school-aura halo stays parked.
