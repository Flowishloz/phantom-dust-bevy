# Next session: start here (updated 2026-10-09)

## Summary as of 2026-10-09 morning (overnight perf, PC; the 2026-10-08 block below is history)

**Performance round 3 is on main (13 commits since the wave 1008 merge, NOT pushed: ask Laurence).** Detail and numbers: `PERF.md` "Round 3"; handoff `handoffs/OVERNIGHT-PERF-1009.md`; bug entry BUG-GRAVEYARD "Four players well under 60 fps".
- **4 players, same deterministic fight:** 61.5 ms (16 fps) at the start of the night -> 19.0 ms in the dev build with the OLED screensaver running; 17.8 ms in the optimised `play` build, median frame 16.7 ms (= the 60 fps cap), render thread 15.0 ms of the 16.7 budget. 3 players: render thread 11.4 ms. The frames over 16.7 ms coincide with the screensaver's periodic spikes. 3 players were at the 60 fps cap already at 10-09 07:45 (16.7 ms); 2 players locked 60.
- **Laurence's copy (`engine\play`) is now built with the optimised `play` profile** (DECISIONS 2026-10-09; `pd-build-run` skill updated). Refreshed 10-09 10:02 (previous copy kept as `engine/shots/n1009/play_backup_1008.exe`); proven with a 6-skill film compare (same results as the previous copy).
- **Tests:** pd_game 1222 passed (2 ignored); pd_viewer checks. Other suites untouched by this work.
- **Pictures:** every step checked with fixed-tick captures old vs new (within rerun noise, many byte-identical); forced aura byte-identical; footstep puffs within 2/255 (accepted, DECISIONS).
- **Later the same morning (Laurence back):** he picked B on the shadow A/B sheet (2 cascades per light from three views, 5163d4c) and asked to push (done for the first 11 commits). With the screensaver off, the final PLAY copy holds **4 players at a locked 60 fps** (16.67 ms mean, p99 17.4, render thread 12 ms of 16.7); 3 players locked, render 8.8 ms.
- **Waiting on Laurence:** play a 3-4 player match and say if it feels smooth, especially casting.
- **Next (perf):** re-measure `PD_RENDER_ST=1` on an idle machine (no screensaver); the remaining cost is Bevy's per-view work x 32 views (4 cameras + 28 shadow cascades): candidates are a vendored Bevy patch that caches per-view preprocess bind groups / merges a camera's command buffers. 4K at 4 players is not measured yet. Thin LTO measured: no gain (17.7 vs 17.8 ms), profile removed.

## Summary as of 2026-10-08 evening (PC session; the 2026-10-07 block below is history)

**The cloud branch `claude/laughing-fermi-91wzrp` is merged into main** (Prop Hunt, Options tabs VIDEO / SOUND / CONTROLS with volume mixer, camera invert + sensitivity and button mapping, player memory, Palace stairs head line, effect speed-ups round 2). Main is NOT pushed yet: ask Laurence.
- **Tests (Windows):** pd_game 1215 passed (2 ignored), pd_formats 98, pd_convert 6, pd_viewer 43, plus 9; `python -m unittest` in `engine/re` 279 OK (4 skipped). The 3 replay goldens and the footstep test pass; the real-file stairs test ran.
- **Performance** (PERF.md "PC measurement 2026-10-08"): 4 players 77 -> 52 ms mean, worst frame 198 -> 129 ms vs the old PLAY copy; 3 players 32 -> 27 ms. Round 2's switches individually are inside the noise; all stay ON. Pictures identical. Still ~20 fps at 4 players: next is per-instance effect data (batching).
- **Seen on screen:** Prop Hunt (cover, hider lines, disguise as a lamp post, full-window seeker, round card, setup rows), all four Options pages, stairs before/after, setup. NOT seen: a real knife catch + knock-down, result screen KILLS, rumble, 4K text size (DESIGN-PROPHUNT.md "PC check 2026-10-08").
- **Camera sensitivity above 100 % now really turns faster** (Laurence's choice, DECISIONS 2026-10-08; `Input::cam_fast`).
- **`engine\play` refreshed** (proven with a film take). Laurence to try: Prop Hunt, Options tabs + button mapping, walking under the Palace stairs, casting for frame drops, volume curve by ear, remapping by press + 5 s timeout on his pad.
- **Open:** effect batching; Prop Hunt polish list (handoff "Still open"); small cleanups (unused `players` in `hud.rs::layout_hud`, dead `turn_clip_seconds`, `hand_line`).

## Summary as of 2026-10-07 (archivist pass 2; the dated blocks below are the detail and the history)

**On branch `claude/laughing-fermi-91wzrp` (cloud sessions 2026-10-07/08), NOT on main yet. Full handoff for the PC session: `handoffs/WAVE-1008-CLOUD-HANDOFF.md` (read it first; the short checklist below is its summary):**
1. Merge the branch into main; run every suite on Windows (the cloud could not check the 3 replay goldens or anything needing the cache; the Linux replay hashes were unchanged by every merge, so the Windows goldens should still pass).
2. Performance (PERF.md "Round 1" = lane/perf, merged; "Round 2" = fxperf: material cache + pool, in-place meshes, pipeline warm-up, env reads once; plus the effect preload): run the PERF.md Round 2 recipe (`PD_FRAME_STATS=1800,200` with all of `PD_FX_PRELOAD=0 PD_FX_MATCACHE=0 PD_FX_MESHREUSE=0 PD_FX_WARMUP=0` vs none, then `600,0` for the first-cast hitch) and the picture check (fixed-tick captures + film_compare before/after). Any picture difference: bisect with the switches, turn that one off by default, tell Laurence.
3. Prop Hunt (DESIGN-PROPHUNT.md "Needs a PC check"): `--mode prop_hunt`, shots before/after frame 720 (seeker cover, hider lines, full-window seeker, black hider views), disguise models, knockdown on a catch, rumble on a real pad, text size on the TV. Send Laurence the pictures before he plays.
3b. Options tabs (FRONTEND.md "Options tabs"): a screenshot of each tab (`PD_TEST_OPTIONS_TAB=video|sound|controls|buttons --demo --near --screenshot shots/tab_<name>.png --shot-frame 120`): tab strip text fits, slider bars, CAM SENSITIVITY label, the button-mapping page. By ear: the volume curve and a live change on playing music. On a real pad: binding by press, the 5 s timeout. Setup: player memory (`cache/players.json`) reopens last run's picks; player 1's B leaves to the title with others joined; Prop Hunt KILLS TO WIN row.
4. Palace stairs head line: a picture under a big flight from the hall end (pd-visual-verify), then Laurence walks it.
5. Refresh `engine\play` and tell Laurence what to try. (Prop Hunt choices answered 2026-10-08: hunt clock starts at release; no round limit, first to 5 kills.)

**Scorecard: 266 of 341 skills match the original** (full run `.pdtools\compare_full_1003b`, main 60f227f, 2026-10-03; no full run since). 128 mismatches remain: sound_missing 49, sound_extra 32, gameplay in 13 skills plus 3 never-fired takes. Most gameplay residue is UNCONFIRMED and needs a LIVE trace from Laurence's PC (list in the `pd-live-original` skill and the 2026-10-03 block below).

**Tests:** 1138 green, run twice on main after the wave 1003 merges (2026-10-03, `handoffs/WAVE-1003-STATE.md` update 16:45; the note does not split it per suite. The online lane alone saw 1113 game / 95 formats / 6 convert / 43 viewer).

**What the game is now:** one-to-four-player local versus with CPU opponents (original `eneprog006` script, five grades), online versus for two PCs (LAN / IP and Steam App 480, Brett's work), the arsenal builder, game modes (Close Quarters, chosen in the setup's GAMEMODES row or the online lobby's GAMEMODE row), an Options overlay with display / FOV / camera distance / look / vibration options, all original rules read from the program. Direction from Laurence: **gameplay accuracy only, no new features** unless he asks.

**Wave 1003 is merged and main is pushed to origin** (Laurence: "once all lanes are completed push to github"):
- camfov: the FOV setting is the horizontal angle of a 16:9 view (default 75, LIVE x scale cot 37.5); each camera works out its own vertical FOV from its viewport (Hor+ for strips, Vert+ for narrow halves); camera-distance slider scales boom and height (GAME.md "FOV and camera distance settings");
- rumble: controller vibration rebuilt from the original's tables, VIBRATION row in Options, vendored `bevy_gilrs` weak-motor fix (`RUMBLE.md`);
- split3: three players = P1 top-left, P2 top-right, P3 the whole bottom half; the map quarter is gone (FRONTEND.md "Three-player layout", DECISIONS 2026-10-03);
- infomusic: the D-pad skill card is the original's card with its range icon (HUD.md "Skill card"); music per the original's stage table with its loop region, menu and result music (PIPELINE.md "Music"; needs `pd_convert musictracks`, done on Laurence's cache); character-select preview models no longer stack;
- online: Brett's `online-mp` merged; title is VERSUS MODE / ONLINE / ARSENAL / OPTIONS MODE / EXIT GAME (no MODES row), GAMEMODES row in the local setup, host-picked GAMEMODE in the online lobby, only human seats get a view (1 human vs CPUs = full window), Steam DLL tracked in `engine/vendor/steam` (`ONLINE-PLAN.md` "Merged into main");
- also: fxfix (Rapid Cannon black smoke), stairs part 2 (Panorama needs nothing), archivist pass 1.
- `engine\play` refreshed with the Steam DLLs (2026-10-03).

**Fixed 2026-10-07:** the setup's teal rule line said "Standard" while CLOSE QUARTERS was selected; it now names the mode (`setup::rule_line`).

**Work in progress, not merged:** perf lane (branch `lane/perf`, WIP 344892f, pushed as a branch). 4 players 1080p 60 -> 49 ms, 3 players 33 -> 29 ms, pictures identical, but measured on battery: re-measure on mains power before trusting it. Next step there: shared effect materials + per-instance data. Its notes are `PERF.md` "where I stopped", on that branch only (not on main). Target stays 4K 60 fps, 1-4 players, no look change.

**To do next:**
- Psycho Knife square: Laurence saw no square in our tick-45 picture; waiting for him to say when it appears in-game.
- Locked-on top/bottom framing: an A/B sheet for Laurence.
- Side-by-side letterbox: open question for Laurence.
- Online between two real PCs (only tested as two copies on one PC; Steam not tested).
- Then: a live session for the UNCONFIRMED gameplay residue, the remaining sound classes (`audits/P0-SOUND.md` section 12). Parked by Laurence: the school-aura halo (ask first).

**Waiting on Laurence:** the live session for the UNCONFIRMED gameplay residue; `display_log.txt` (written next to `settings.json`) after he runs on the TV; the Psycho Knife square, the top/bottom framing A/B and the letterbox question above; a second PC (or friend) for the online test.

**Docs:** `INDEX.md` maps every doc; `ARCHIVIST.md` says who owns which topic and what to update after a merge. Older dated blocks below may name features that have since changed (for example the MODES title row, the display keep prompt, "no networking on main"): the current doc for each topic is in INDEX.

---

## Detail (2026-10-03, later) — scorecard 266 of 341 skills match

Merged since: bodyradius (contact 10 / wall 5, READ charaset), residue (target point = BODY1 bone, READ FUN_1402513a0; melee arming tick; Psycho Kinesis/Power cast without object), soundfill2 (blast-life victims, Add-effect sound gate, landing probe by fall class; shot/burst surface sounds opt-in only, UNCONFIRMED). Full run `.pdtools\compare_full_1003b` (exe `pd_game_base_1003b.exe`, main 60f227f; run with --jobs 2 can GPU-OOM a take — rerun singly): 266 clean, 128 mismatches: sound_missing 49, sound_extra 32, gameplay in 13 skills (7 Stone Shot, 70 Triple Lasers, 77 Ricochet Laser, 146, 182/183/189 post-hit, 242, 261, 265, 311, 317, 260) + 3 never-fired takes (5/11/14). Most gameplay residue is UNCONFIRMED and needs a LIVE trace (hit-test inputs, knocked-pose offset, BODY1 offset per character/clip). Next: ask Laurence for a live session (list in pd-live-original skill + these), else remaining sound classes (P0-SOUND §12).

## Earlier (2026-10-03) — first trustworthy scorecard: 244 of 341 skills match

Laurence's direction: **gameplay accuracy only, no new features** (memory). Merged since 2026-10-02 night: film harness mirror fix (collide), field capsules + charge shots (hitdiff), program-exact collision with per-triangle class + `dat+walls` retired (collclass), camera-only occluders `stNN.camcol` keeping his approved camera (camwalls), arming tick for attack objects + Reverse/mines (shotclass), plus display options, Close Quarters mode and lighting-look settings he asked for. Suites green (1007 game). `engine\play` refreshed.

**Full run on main 6b3b76c** (`.pdtools\compare_full_1003`, exe `.pdtools\pd_game_base_1003.exe`): 244 clean, 164 mismatches in 97 skills: sound_missing 63, sound_extra 50, hit_tick 12, p1_health 12, original_no_fire 7 (takes 5/11/14), fire_tick 5, cast_end 4, aura 5, effect 5. Gameplay residue: 18 Psycho Kinesis, 17 Power, 146, 181, 7, 70, 189, 242, 261, 265, 311, 317, 328, 182, 183, 260. Running: bodyradius (fighters 10 u apart, READ charaset 5), soundfill2 (sound classes).

**On branch `online-mp`?** Read `ONLINE-MP-HANDOFF.md` (repo root) first:
online versus (LAN + Steam), how to rebuild the cache and `engine/play`,
and the two-PC test that comes next.
(Archivist 2026-10-07: `online-mp` was merged into main on 2026-10-03; the two-PC test is still open. See the top block.)

## History (2026-10-02, later) — castpose, fxdiscs, soundfill, superblast, propsound merged
## Earlier (2026-10-02, later) — castpose, fxdiscs, soundfill, superblast, propsound merged

Suites green: 945 game / 89 formats / 6 convert / 41 viewer / 9. Merged today: cast hold = clip+1 tick; Regenerate/Invulnerability/Mephisto state switch at fire; our stand-in red Hole disc removed; terrain impact sounds; world-wide blasts hit everyone incl. caster; guard block recoil; landing/take-off by floor class, surface-hit 8–16, prop-break sounds. fxsheets found no READ cause (needs a live trace of eft0117 dome / Bomb shells / eft0110 rays — ask Laurence). Details per lane in WAVES.md.

**Disk:** C: had ~5 GB free; `_archive/old-lanes` (76 lanes, ~5 GB each with build output) is the cause. Laurence will delete it himself. Do not start lanes until there is space; consider sharing one CARGO_TARGET_DIR across lanes.

**Next:** effects too bright (~24 skills); Photon Burst ray spin (READ word 20, FUN_1401a1620); Heat/Level Boost floor glow (bisect effect 218/271 vs LIVE light); prop-sound residue (16 new extra 0.8 from landing gate, UNCONFIRMED decal rules); Jump height (ours falls ~50 vs ~97); Spore Schism crawl self-blast; reaction root drift return; live session list for Laurence (effect draw timing, wall camera).

## Earlier (2026-10-02) — film-comparison + visual waves merged; no lanes running

Main is clean and all suites are green (933 game / 88 formats / 6 convert / 41 viewer + 274 Python); PLAY.bat is current.

**The 341-skill film comparison is the work engine now:**
- Tool: `engine/re/film_compare.py` (`run --real`, top/bottom split default).
- Outputs:
  - `.pdtools/compare2` is the full numeric run: 124 clean skills, 366 mismatches.
  - `.pdtools/compare3` holds 60 fair-framed visual sheets.

**Next, from WAVES.md (top block, "2026-10-02 visual wave"):**
1. Effects bigger/brighter than the original on about 24 skills. Measure luminance on the compare3 sheets; change only with a READ cause. The bombhand lane's global gamma-add was NOT merged (it gave Splashdown a black blob).
2. Shields and rings drawn as flat sheets (28, 149, 126, 64, 39); this includes the Bomb hand shells Laurence reported (his pick: panel A).
3. The cast pose held after the fire (102, 19, 120, 21).
4. Hard red floor discs (22, 86, 147, 148, 175, 312, 319, 120).
5. Remaining sound mismatches (161 missing / 74 extra in compare2; footsteps are protected).
6. Camera swing and wall pull-in, which are UNCONFIRMED (they need a live trace near a wall).

**Parked by Laurence:** the school aura. Note the evidence: the original shows big school-coloured ribbons rising off fighters (`engine/shots/fl_123.png`), so ask him before touching it.

**Live sessions:**
- Ask Laurence first every time; the move-recorder lane (`engine/re/move_recorder.py`) runs them.
- His original save is unlocked; the recorder restores to each run's own snapshot.

**Orchestration:**
- Lanes go in `lanes/<name>` with a cache junction; remove junctions with `cmd /c rmdir` before ever deleting a lane.
- Deleting worktrees is refused by the permission system, so park finished ones in `_archive/old-lanes`.
- The shared brief is WAVE6-BRIEF.md.

## Earlier (2026-10-01 night) — wave 6 merged; folder is now "Phantom Dust Rewrite"

The project moved to `<repo>` (the old
`Phantom dust` folder holds only an empty, locked `.git` stub; delete it once no Claude
session is open in it). Open new Claude sessions in the new folder. Read AGENTS (current
state) → INDEX → HANDOFF top → **WAVES.md** (every lane and open item). Laurence plays
with the one `PLAY.bat` menu; give him that, not commands.

Next candidates, all listed in WAVES.md:
- the school-aura halo, parked by Laurence: the next step is a close camera in the steady phase, then re-run the "no effect 600" test, because the colour table's only users were ruled out;
- an Orb/Amulet live take;
- the small-landing slide check;
- the Rain column slot-1 trace;
- the LIVE combo take;
- the Psycho Wave blend pick and the glow on/off choice, still with Laurence.

## Earlier (2026-10-01) — waves 1-5 merged; review sheet waiting

Read AGENTS → INDEX → HANDOFF top → **WAVES.md** (lane-by-lane state, open
items, what's next) → `audits/CRITIC-2026-10-01.md`. Laurence's answers to
`engine/shots/REVIEW-2026-10-01.html` decide the next wave; open items with
no decision needed are listed in WAVES.md. Mandate (memory): Claude is sole
editor/orchestrator with Sonnet lanes; budget no issue; original's
behaviour by default.

## Earlier (2026-09-30, latest) — Xbox ISO step 1 DONE

Image verified + unpacked (xiso, 2,062 files; H6 "Step 1 result"). Next is
H6 step 2/3: extract with our own XDVDFS reader (no extract-xiso download),
then `file_inventory.py --diff` against `game_files/`. Laurence's go first.

## Earlier (2026-09-30, late) — Xbox ISO analysis

Read AGENTS → INDEX → top of HANDOFF.md → **`handoffs/H6-xbox-iso.md`** and
do its step 1 (verify + unpack the disc image, report), then stop. Locked-on
movement is FIXED and merged (Laurence approved on his pad; HANDOFF top).
The deep-pass plan below still stands for everything else.

## Earlier — deep Claude passes PLANNED, not launched

Read AGENTS → INDEX → latest HANDOFF → **CLAUDE-DEEP-PASSES.md**. Laurence
requested separate heavy deep orchestration passes for sound (capsule collect
AND use), movement, move VFX/animation, lighting, modern school aura, four-player
foundation/UI, arsenal builder and gamemodes. ROADMAP/PLAN point to P0–P8.
Start with actual-code audit + reference battery; propose a bounded wave with
cost and safe file ownership before execution. Do not run historical waves
blindly or call implemented/mapped features faithful without approval.
Steps common35 and near-wall dissolve are accepted; protect them. Latest
actual tests: 345 + 42 + 7 + 10. School aura/moves remain parked until their
dedicated pass is authorised. No new pass has run from this planning request.

Laurence is away and wants work to continue autonomously. Read `AGENTS.md`, `engine/docs/INDEX.md`, then the top entry of `engine/docs/HANDOFF.md` ("where to continue"), then this file. Everything is committed on `main` (local, not pushed; `git log -1` should be `bd932a9` or later). Tests at hand-off: `cargo test -p pd_game -p pd_formats` → 197 + 16 green.

## Standing rules (don't ask Laurence; he's away)

- Work autonomously; decide and record choices in `engine/docs/DECISIONS.md`. Only things that truly need his eyes/ears go on the "needs Laurence" list in HANDOFF.md.
- Never commit game data (`game_files/`, `engine/cache/`, screenshots, WAV/PNG/GLB). Never push unless Laurence asks (he asked on 2026-10-03: push main to origin once every lane in the wave is merged and tests are green; lanes still never push). Never copy Esper-RE code.
- Sim (`combat.rs`, `sim.rs`, `moves/`) stays deterministic; renderer/audio/menus only read it.
- Visual changes need screenshots you look at; guesses are named constants marked "unconfirmed" with evidence in docs.
- Budget matters (~£60/month across tools): parallel agent lanes only where files don't overlap; Sonnet for bounded lanes, Opus for reverse-engineering; always verify an agent's work yourself before merging (the last audit found real bugs in agent code AND in the orchestrator's own decoding).
- Parallel lanes: `engine/docs/LANE-BRIEF.md` (update its commit hash / test count), one worktree per lane, shared build dir = a copy of `engine/target` in `%USERPROFILE%\.pdtools\lane-target` (delete after), check each worktree's `git log -1` (they have started stale), `git add` only named files (`.claude/worktrees/` is ignored now).
- Reports to Laurence: plain English, outcome first, short, no code; send screenshots for visual work.

## Earlier (2026-09-29 sign-off)

**Wall dissolve is COMPLETE, explicitly approved by Laurence.** Do not queue
another approval or rebuild the rejected alpha-blend approach. See the top
sign-off in `HANDOFF.md`; implementation `cam_fade.rs` + `dissolve.wgsl`.
Latest merged validation: **345 + 42 + 7 + 10**, build/runtime smoke passed.
`PLAY.bat` current. Regular step corrected to **27_id035/common35**; capsule
mesh-layer isolation corrected (H5). School aura and moves remain parked;
this sign-off does not expand scope or approve unrelated outstanding items.

## Earlier (2026-09-29 night)

Read the top entry of `engine/docs/HANDOFF.md`, then `engine/docs/FRONTIER.md` ("Laurence's play observations" O1–O5). `main` green at **342 + 42 + 7 + 10**; `PLAY.bat` current with everything merged. Three waves ran on **free models** (`muse-spark-1.3-contributor-free`) with an independent verification gate: unpause cue, footstep research, wet-patch surface class + export, camera fade (+ the gate's defect fixed), consume lock (now 79 ticks with the real `SPE_POWER_UP`/`SPE_HEAL` animations), lock-on camera slew + id-4 cue, wet footsteps 52/54, capsule sounds 85/92. **Cheapest unblock: Laurence's ear on Q3/Q4 in `engine/audition/wetcheck/LISTEN.txt` and one live walk onto Lost Palace's puddles (st03, he plays, `PD_SURFACE_LOG=1` logs it) — the demo's pathing never crosses them.** School aura is skipped (needs a full overhaul). Worktrees clean; `%USERPROFILE%\.pdtools\target-*` build copies can be deleted.

## Earlier (2026-09-29 evening, after Laurence's original-game discoveries)

Read the top entry of `engine/docs/HANDOFF.md` (2026-09-29 evening), then `engine/docs/FRONTIER.md` ("Laurence's play observations" O1–O5). `main` green at **324 + 36 + 7 + 10**; `PLAY.bat` current. Five lanes ran on **free models** (`muse-spark-1.3-contributor-free`) and are merged (unpause cue, footstep research, wet-patch surface class, camera fade, consume lock). Wave 2 in flight: `lane-lockon` (O4), `lane-surface` (0x94 class export), plus a verification gate. **Cheapest unblock: Laurence's ear-check in `engine/audition/wetcheck/LISTEN.txt`** (wet step 52/54; capsule 85/92) — then wire the wet footstep sound and the capsule pickup. School aura is skipped (needs a full overhaul).

## Earlier (2026-09-28, end of visual cycle)

Read the top entry of `engine/docs/HANDOFF.md` (2026-09-28), then `engine/docs/archive/HANDOFF-ORCHESTRATION.md`. `main` at the school-aura merge or later; tests 7 + 36 + 276. Small polish jobs are handed to Hermes in `engine/docs/handoffs/H3-polish-batch.md`. First question for Laurence (panel): resume the moves track (M5, M11–M14, M6) or keep polishing visuals.

## Earlier (2026-09-27, end of session)

**Read `engine/docs/archive/HANDOFF-ORCHESTRATION.md` and run it.** It has the state, what's waiting on Laurence, the next work in order, and how to run lanes. Tests 272 + 36 + 7; `main` at the strafe-facing merge or later.

## Earlier (2026-09-27, night) — done

Top entry of HANDOFF.md ("2026-09-27 night … START HERE") is current. Next: PLAN.md "Moves track" — M1 (tables into skills.json + emitter textures), M2 (30 fps clips, show Laurence), M7 (last Opus reading pass) in parallel; then M3 → M6. Hand-tuned move changes are paused until they come from PROGRAM-RE.md.

## Earlier (2026-09-27, late) — done

**Read `engine/docs/archive/HANDOFF-ORCHESTRATION.md` and run it**: three agent lanes (Lost Palace invisible barrier, melee/projectile visuals, roadmap → PLAN.md). The rest of this file is background.

## Earlier (2026-09-27, evening)

The top entry of HANDOFF.md ("where to continue (2026-09-27 evening …) — START HERE") is current and supersedes everything below: skill uses, match rules, dodges, and the menus / palace / effects lanes are all done and merged. Tests at hand-off: `cargo test -p pd_game -p pd_formats -p pd_convert` → 211 + 22 + 2 green. Follow its backlog order.

## Older notes (superseded)

## Earlier (2026-09-27, later)

The top entry of HANDOFF.md ("Handoff for a fresh chat … START HERE") supersedes the list below where they differ: it adds skill uses (s26), match-start values, the per-character dodge, and the cloud-research branch results. Order there: controls log → skill uses → match start → dodge moves → questions for Laurence.

## Earlier: do-next list (2026-09-27)

1. **Controls report from Laurence** (jump and right-stick camera "don't work", keyboard and pad): if `engine/shots/input_log.txt` exists, read it (lines `[input] … P1 in Input {…} | P1 y … grounded … locked … cam_yaw …`) and fix what it shows; if not, remind him to run `tools/bats/PLAY_DEBUG_CONTROLS.bat` (it records what the game receives). Ask him about jump height (apex ≈ 0.75 m today) with a calibration choice.
2. **Arena objects leftovers:** contaB's collapse animation (`OA01`) and collapsed hull (`OC01`); the game's own break effects (`OE00` "EF00" blocks, format unknown — pd-reverse-engineer) and hit sounds (`mdl_hit_iron/stone/soil`); melee hits on objects; rubble falling on players.
3. **Camera in tight corridors** (st03/st04/st13 pull in very close) — before/after screenshots.
4. **MOVES.md stragglers:** Lack's "unlimited-use" filter, Necronomicon, Diffused Laser, extra damage from the drop after an air hit, knock-back distance.
5. Then HANDOFF.md backlog: builder art polish, 2-v-2 design doc, online design doc, first-run converter.

Done 2026-09-27 (see HANDOFF.md top entry): audits of the never-audited code (all findings fixed), arena objects phase B, Hungry Demon button + Retain, status icon search (not in the files), demo script / `--near`, attack STR rules + charge, Brush, Auto-Capture, erase-on-hit.

Update the top entry of HANDOFF.md and this file before stopping.

**PLAY.bat runs engine\play\pd_game.exe** (gitignored copy). Lanes share engine\target, so after every merge: build main (touch pd_game main.rs to force), then copy target\debug\pd_game.exe to engine\play. Laurence's lock-on complaint 2026-10-02 was a lane build overwriting his exe.
