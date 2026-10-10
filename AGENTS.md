# AGENTS.md — Phantom Dust rebuild

Source of truth for any coding agent (Claude Code, Hermes/OpenCode, others) working in this folder. Read this first, then `engine/docs/INDEX.md`. Keep it current: if you change a fact below, change it here in the same session.

## Mission

A fan runtime for **Phantom Dust** (Microsoft, 2004; free Windows 10 re-release 2017) written in **Rust + Bevy**, in the style of [iw4L](https://github.com/vladtrc/iw4L): the player points it at *their own* copy of the game; it converts that data into a local cache and plays it in our own engine.

- **In scope:** local split-screen versus (priority), then online versus between friends. Story-mode assets may be reused.
- **Out of scope:** cutscenes (`*.mp4`), story mode, patching/redistributing the original game. (CPU opponents ARE in scope since 2026-10-01: rebuilt from the original's own quest AI script, DECISIONS.md.)
- **Owner:** Laurence, a non-technical creative director. Reports to him must be plain English, outcome first, no code. Visual changes must be shown with screenshots.

## Hard rules

1. **Never commit, upload, or publish game data (this repo is public).** `game_files/`, `engine/cache/`, and anything decoded from them (PNG, glTF, WAV, screenshots containing assets are fine to show Laurence locally) stay on this machine. Only our source code is ours.
2. **Never copy code from Esper-RE** (`tools/Esper-RE`, GPLv3). Read it as reference; reimplement from the documented layouts. Copying would force the whole project to GPL.
3. **The simulation is deterministic.** `engine/crates/pd_game/src/sim.rs` (with `combat.rs`) takes quantised inputs and produces state; nothing in it may read time, unseeded randomness, the renderer, or Bevy types. Rendering reads sim state, never writes it. This is what online play will stand on.
4. **Verify visual work visually.** A change to rendering, animation, or camera is not done until a screenshot has been looked at. When correctness is ambiguous (e.g. rotation conventions), render labelled variants A–F and let Laurence pick. See `engine/docs/VERIFY.md`.
5. **Record discoveries.** Any new fact about a game file format goes into `engine/docs/FORMATS-*.md` in the same session, with how it was established (evidence, not guesses). Mark unconfirmed facts as such.

## Version control

- Public GitHub repo `Flowishloz/phantom-dust-bevy` (branch `main`). It holds source code and our own documentation only.
- `.gitignore` excludes game data, cache, screenshots, build output and third-party tools. Before any commit, check `git status` for anything derived from the game (`.alr .stx .png .glb .wav .pdcl`, dumper logs); none of it may be committed. See `LEGAL.md`.
- Commit when a unit of work is verified.

## Layout

```
(repo root)
  AGENTS.md              this file
  README.md  LEGAL.md  LICENSE-MIT  LICENSE-APACHE   what this is, what is and is not shared, licences
  PLAY.bat               a numbered launcher menu (play, quick test match, control log, ...);
                         it runs engine\play\pd_game.exe, your own build copy
  game_files/            YOUR copy of the game's data (you provide it). Never commit.
  tools/bats/            the helper launchers PLAY.bat calls (each also works on its own)
  engine/                Rust workspace - all our code
    crates/pd_formats/   file-format library: ALR, animation, textures, STX, collision, glTF export
    crates/pd_convert/   CLI: game files -> cache (+ research/probe subcommands)
    crates/pd_viewer/    asset viewer: stage + character + anim, screenshots
    crates/pd_game/      split-screen game (sim.rs + combat.rs + moves/ = rules; cpu/ = computer opponent;
                         frontend/ = menus; net/ + online_match.rs = online play; main.rs, hud.rs, fx.rs,
                         effects.rs = Bevy glue)
    vendor/bevy_gilrs/   patched bevy_gilrs (weak-motor rumble fix; `[patch.crates-io]` in engine/Cargo.toml)
    re/                  Python tools: film comparison, move recorder, live trace, skill audits (README-PC-TOOLS.md)
    cache/               converted assets (generated on your machine, never commit)
    shots/               screenshots (generated)
    env.ps1              dot-source first: `. .\env.ps1` (PATH + cd)
    shot.ps1             one-line viewer screenshot helper
    montage.ps1          labelled grid of screenshots (A-F sheets)
    docs/                per-area docs - start at docs/INDEX.md
  docs/WHAT-WE-FOUND.md  plain-English summary of what the project has found out
```

## Toolchain (Windows, no Visual Studio)

- Rust stable, **GNU target** (`x86_64-pc-windows-gnu`), installed via rustup.
- A portable MinGW-w64 (linker + `dlltool`) at `%USERPROFILE%\.pdtools\mingw64`, or wherever `$env:PD_MINGW` points. It lives outside the project because MinGW's linker breaks on spaces in the project path. `engine/.cargo/config.toml` just names `gcc` and `dlltool`; `env.ps1` puts them on PATH.
- Every shell needs: `. .\engine\env.ps1` (puts MinGW + cargo on PATH, cd's to `engine/`).
- Dev profile keeps debug info minimal: full debug info makes the Bevy exe exceed Windows' 2 GB executable limit ("not a valid Win32 application").
- `glam` must match Bevy's version (currently `0.32`), or `Vec3` types won't unify.
- Bevy is **0.19**: scenes spawn with `WorldAssetRoot(...)` (not `SceneRoot`), events are `MessageReader/MessageWriter`.

Details: `engine/docs/TOOLCHAIN.md`.

## Everyday commands (PowerShell, from `engine/`)

| Task | Command |
|---|---|
| Build converter | `cargo build --release -p pd_convert` |
| Build game | `cargo build -p pd_game` (fails "Access is denied" if the game is open — ask Laurence to close it; never kill his session) |
| Build Laurence's copy | `cargo build --profile play -p pd_game` -> `target\play\pd_game.exe`, copied to `engine\play\` (optimised; `pd-build-run` skill) |
| Convert everything | see `engine/docs/PIPELINE.md` |
| Scripted game test + screenshot | `.\target\debug\pd_game.exe "$PWD\cache" --demo --near --screenshot "$PWD\shots\x.png" --shot-frame 300` (`--near`: players start close; without it, at the original far-apart start points) |
| Viewer screenshot | `.\shot.ps1 <name> chars/pc00a --close 45 --angle 1.57 --anim WAIT0 --anim-time 0.3` |

## Current state (update when it changes)

- **2026-10-09 (read this first):** overnight performance round 3 on main (not pushed): 4 players 61.5 ms a frame -> a locked 60 fps (16.67 ms, render thread 12 ms) on the same deterministic fight (effect / wisp / particle per-instance rows, shared stage light and fighter materials, sleeping overlay cameras, CPU light clustering, single-threaded view schedules, 2 shadow cascades per light from three views: Laurence's pick). Laurence's copy is now built with the optimised `play` profile (`cargo build --profile play -p pd_game`, DECISIONS 2026-10-09). Tests 1222 game. Detail: `engine/docs/PERF.md` "Round 3", `engine/docs/NEXT-SESSION.md` top block.
- **2026-10-08:** the cloud branch is merged into main on the PC: Prop Hunt (local 2-4 player hide and seek, third GAMEMODES entry), Options tabs (VIDEO SETTINGS / SOUND / CONTROLS: volume mixer, camera invert + sensitivity, button mapping), player memory (`cache/players.json`), Palace stairs head line, effect speed-ups round 2. Windows tests: 1215 game + 98 formats + 6 convert + 43 viewer + 9, Python 279 OK. Measured: 4 players 77 -> 52 ms mean vs the previous build, pictures identical (`engine/docs/PERF.md`). `engine\play` refreshed. Main not pushed yet (ask Laurence). Detail: `engine/docs/NEXT-SESSION.md` top block.
- **2026-10-07 (read this first; older entries below are history, newest first):**
  - **Where we are:** one-to-four-player local versus with CPU opponents, online versus for two PCs (LAN / IP + Steam App 480), game modes (Close Quarters, in the setup's GAMEMODES row and the online lobby), an Options overlay (display mode / resolution / aspect / HUD size / FOV and camera-distance sliders / look presets / vibration), and the original's rules read from its program. Laurence's direction now: **gameplay accuracy first, no new features** unless he asks.
  - **Scorecard:** 266 of 341 skills match the original in the film comparison (`engine/re/film_compare.py`, run 2026-10-03; detail and residue classes in `engine/docs/NEXT-SESSION.md`). Tests: 1138 green on main, run twice (2026-10-03, `engine/docs/handoffs/WAVE-1003-STATE.md`).
  - **Wave 1003 merged (2026-10-03) and main pushed to origin per Laurence:** camfov (FOV = horizontal angle of a 16:9 view, per-camera vertical FOV, camera-distance slider), rumble (controller vibration from the original's tables, VIBRATION row, `engine/docs/RUMBLE.md`), split3 (player 3 gets the whole bottom half), infomusic (original skill card with range icon, music per the original's stage table with loop regions, menu and result music, no stacked preview models), online (Brett's `online-mp`: title VERSUS MODE / ONLINE / ARSENAL / OPTIONS MODE / EXIT GAME, GAMEMODES row in local setup, only human seats get a view, Steam DLL in `engine/vendor/steam`), plus fxfix and stairs part 2. Fixed 2026-10-07: the setup's rule line no longer says "Standard" while a gamemode is selected.
  - **In-game dev tools** (`pd_game/src/devtools.rs`, from the online merge): F1 state overlay, F2 pause, F3 single tick, F8 bug report with typed note -> `engine/bug_reports/` (git-ignored). Details: GAME.md "In-game dev tools".
  - **Work in progress, not merged:** perf lane (branch `lane/perf`, WIP 344892f): faster 3-4 player frames with identical pictures, but measured on battery; re-measure on mains power. Notes: `PERF.md` "where I stopped" on that branch.
  - **Next / waiting on Laurence:** Psycho Knife square (he will say when it appears), locked-on top/bottom framing A/B, side-by-side letterbox question, online between two real PCs, `display_log.txt` from his TV, a live session for the UNCONFIRMED gameplay residue. Full list: `engine/docs/NEXT-SESSION.md` top block.
  - **Docs:** who owns which topic and how to keep them current: `engine/docs/ARCHIVIST.md` (the archivist lane is re-run after each merge wave; pass 2 was 2026-10-07).

- 2026-10-03 (history): wave 1003 opened (camfov, split3, infomusic, online, rumble); landed that day before it: Lost Palace stairs QoL (`PD_STAIRS_ORIGINAL=1` restores the original blocks), settings menu overhaul (browse then A, sliders, 4K, first-run auto-detect), MSPARTICLE blend-1 alpha fix (Rapid Cannon smoke) and `PD_FX_OFF` bisect switch, program-exact collision with per-triangle class, camera-only occluders. Tests then 1007 game.
- 2026-10-02 (history): Brett built online versus and the dev tools on branch `online-mp` (handoff `ONLINE-MP-HANDOFF.md`, repo root); merged into main 2026-10-03.

- 2026-10-01 evening (end of wave 6; details `engine/docs/WAVES.md`, HANDOFF top):
  - **CPU opponents:** the original's own `eneprog006` AI script runs on a Rust port of its script VM (`pd_formats` ssb/ssb_vm/epa, `pd_game/src/cpu/`). Add a CPU with Y on an empty setup seat: five grades (`enemyparam.epa` rows 64..60, "nearest" target rule), the original's school decks or saved arsenals.
  - **Move recorder:** all 341 skills filmed and traced from the PC original (`engine/re/move_recorder.py`, MOVE-RECORDER.md; takes in `.pdtools/moverec/takes_full`). Live audit 1,875 values match, 0 crosses.
  - **Built this wave:**
    - combos (High-speed Punch / Rapid Cannon / Trench Mortar), bind/pin/lift reactions, root motion;
    - Levitate/Skywalk float, Dash;
    - guard rules (blasts, Absorb, auto Shelters), instants, built-in decks;
    - effects: heat haze, gamma-space subtract blends, MSPARTICLE sizes, capsule pickup/erase bursts;
    - HUD: lock-on distance readout, opponent panel;
    - stage aux-model fix.
  - **Laurence's original save:** all collectable skills unlocked at his request (`engine/re/mr_unlock_all.py`).
  - **Open:**
    - The school-aura HALO source is not found. Live elimination ruled out the wisps, effect 600 and the HUD aura object; `aura_glow` is off by default.
    - Orb/Amulet need a live take.
    - Small-landing slide differs on some skills.
    - Rain column slot-1 stop.
  - **Tests:** 910 game / 88 formats / 9 convert / 39 viewer + 259 Python.

- 2026-10-01: P0 audits (`engine/docs/audits/P0-*.md`) + waves 1-5 merged
  (see `engine/docs/WAVES.md`, HANDOFF top): four-player (sim, views, HUD,
  setup, rules), cast/reaction/guard/projectile rules from the program,
  aura as effect-600 wisps, SWBOS character + SBTS stage light, shadows,
  effect blend/timeline/pixel stage, collision ramps, events/sounds, arsenal
  builder like the original. Tests 723 game / 57 formats / 8 convert / 25
  viewer. Awaiting Laurence's review sheet. Not yet approved by him.

- 2026-10-01 stage light lane (L3, branch `lane/stagelight`, since merged): the light
  script's vectors are stored (z, y, x) and its colours directional / hemisphere
  down / up (engine/docs/STAGE-LIGHT.md), per-pixel bump lighting on the stage
  (`SBTS` port), effect lights on the stage, exact stage shadow volumes, fighter
  hull shadow casters. Needs the stage and character caches re-exported
  (STAGE-LIGHT.md §8; stages with the default collision, the program's own 0x94 mesh + object hulls + per-triangle class: COLLISION.md; `--collision dat+walls` was retired 2026-10-03). Merged.

- 2026-09-30 late: **locked-on movement fixed and approved by Laurence** on
  his pad (strafe entry, pivot, hand-off, walk-to-run, facing as the
  program's state machine; MOVEMENT-AUDIT.md "Locked-on lane"). Merged with
  the cloud research branch. Next: Xbox disc analysis,
  `engine/docs/handoffs/H6-xbox-iso.md`.

- 2026-09-30 evaluation (history; executed as waves 1-6): recreation was a substantial two-player foundation,
  NOT fully faithful or four-player ready. The plan was
  `engine/docs/CLAUDE-DEEP-PASSES.md` P0–P8: heavy deep Claude orchestration
  dedicated to capsule collection/use audio, movement sensitivity/strafe,
  move VFX/animation, lighting, new modern school aura, four-player
  architecture/UI, original-like arsenal builder and gamemodes. Planning only;
  no new execution wave launched. Steps good and wall dissolve approved complete.
  Mapped/implemented/tested/approved are distinct; no blanket completion percent.

- 2026-09-29 live-play remediation: regular footsteps use **common35**, the
  exact `audition/27_id035.wav` Laurence identified. Capsule own/opponent draw
  isolation is repaired (viewer layers on every mesh, H5). **Near-wall grainy
  dematerialisation is complete and approved by Laurence**: per-view grain
  discard preserving authored cutouts/depth writes, full exact recovery after
  retreat and repeated approach; `cam_fade.rs` + `dissolve.wgsl`. Latest merged
  validation: **345 game / 42 formats / 7 converter / 10 viewer**, build and
  runtime smoke passed. `PLAY.bat` refreshed. School aura and moves remain parked.

- Assets: all 831 ALR files parse; stages, characters (25/28 folders), textures, music, and the skill table (`cache/skills.json`: 399 skills with names, school, capsule type, cost, damage, requirements) convert. Skinned characters with all 129 animations, including extra parts (coat tails etc.).
- Game: 2-player split-screen on stage `st06`, keyboard + gamepad, run/jump/land/idle animations, floor + wall + player collision, lock-on cameras that avoid scenery, settings menu. Combat M1: health, level/aura, capsules, 4 skill slots, Psycho Wave / Laser / Crystal Wall / aura particles from the game's skill table, knock-outs and rounds, the original battle HUD rebuilt from the game's own art (engine/docs/HUD.md), 3-2-1 round countdown, camera framing fixed for top/bottom split (see `engine/docs/GAME.md` "Combat"). Skill behaviour is data-driven per family (`pd_game/src/moves/`, `engine/docs/MOVES.md`): every family is built — attacks (shells, lobs, curving/bouncing lasers, crawlers, meteors, holes, rain, mines, waves, dashes), all defence kinds with STR vs DEF / Pierce, knock-down, and the erase / status / special / environment capsules (status system + environment crystal); ~150 of 164 non-attack skills, a few stragglers listed in MOVES.md. `tools/bats/PLAY_ALL_SKILLS.bat` starts with one skill of every family. Versus front end: title → setup (characters, arsenals, stage, time, rounds) → match → result, and an arsenal builder saving to `engine/userdata/` (docs FRONTEND.md). All 30 stages convert (19 versus arenas named from the game's stage table, `cache/stages.json`); sound effects and battle voices decoded and playing, including the capsule pickup sound the original plays on a capture (FORMATS-AUDIO.md). Battle timer, arsenal rules, powerless drain. Stages are lit like the original from each stage's `.dat` light script (baked into vertex colours), with the sky/backdrop model drawn **and its tiled cloud layer drifting** (`sky_scroll.rs`; the rate is eyeballed — GAME.md "The sky drifts"); footstep puffs on each footfall, timed off the walk cycle's own clock (`footsteps.rs`; one ground surface only — GAME.md "Footsteps"); Palace/Strange City music matched (FORMATS-STAGES.md). The front end is rebuilt in the original's menu style from the game's own menu art and font (`pd_convert menus`, MENUS.md). Skills stay on their button until their uses (s26) run out; the match opens with the original's rules (Level 0, 4 skills dealt) as setup options; each character's dodge (Jump / Tumble / Cartwheel) comes from the character table (`pd_convert chars`). Skill effects: all 476 convert (`cache/fx`, engine/docs/FORMATS-EFFECTS.md) and play in the game on cast / projectile / hit / block (`pd_game/src/effects.rs`) and in the viewer (`--fx`). Online versus merged into `main` 2026-10-03. The 2-player wording above predates four-player setup (2026-10-01) and the online merge; see the 2026-10-07 block.

**Measuring an effect's pixels** (learned the hard way on footsteps, GAME.md "Footsteps"): `--demo` is wall-clock driven so two runs of the same frame differ everywhere; drive with `PD_SHOT_TICKS=<tick>` (one sim tick per rendered frame) plus `PD_TEST_STICK`/`PD_FREE_CAM=1` to make the sim deterministic and keep the fighter in frame. Even then, **two identical-config runs differ by ~19% of pixels** (the camera lerps on `delta_secs` and particle draw order is unstable), so a whole-frame mean-difference A/B measures the harness, not the effect. Prove a small effect by **localising** the diff: one tight cluster where you expect it, nothing anywhere else.
- 2026-09-27: the original's program (`game_files/PDUWP.exe`) was read with Ghidra for its real rules (engine/docs/PROGRAM-RE.md, checked against Laurence's recordings in FOOTAGE.md). Built from it: per-skill cast timing (action table in `cache/skills.json`), hit reactions + knock-downs (reaction table), the original movement (walk/run speeds, turning, gravity, landing, locked-on strafing), 30 fps clips from the colour-variant character files, all effects from the packed archive attached to bones with game-style additive glow, cloth for extra parts. Plus a 3D character select, P2 alternate costume, menu/HUD polish, collision fixes. Next steps: engine/docs/archive/HANDOFF-ORCHESTRATION.md and PLAN.md "Moves track".
- See `engine/docs/ROADMAP.md` for next steps and open questions.

## Model/lane guidance

- Since 2026-09-30 Claude Code (Opus) is the sole editor and orchestrator, running Sonnet lanes in `lanes/<name>` worktrees (brief: `engine/docs/WAVE6-BRIEF.md`, wave 2026-10-03: `engine/docs/handoffs/WAVE-1003-COMMON.md`). It verifies each lane's screenshots, then merges. Hermes is not used on this project.

- Reverse-engineering unknown formats, architecture, netcode design → Claude Code (Opus), effort `high`; `max` only when stuck.
- Bounded implementation once a format/behaviour is documented (importers, per-skill rebuilds, tuning, tests) → Hermes / GLM-5.2.
- Handoffs between lanes use the templates in Laurence's global CLAUDE.md; put the handoff in `engine/docs/HANDOFF.md`.
