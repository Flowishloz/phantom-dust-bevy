# Orchestration handoff (2026-09-27, night) — start a new chat here

**2026-09-30 current planning:** use **CLAUDE-DEEP-PASSES.md**, ROADMAP's current
P0–P8 table and latest HANDOFF. Laurence requested a heavy deep Claude-led
orchestration pass, dedicated by area, including move VFX/animation. Capsule
audio, movement fidelity, lighting, new school aura, four-player architecture/UI,
arsenal builder and modes are open. Plan only so far; no wave launched.
Separate mapped/implemented/verified/approved; strongest Claude high effort on
architecture/RE, bounded build lanes, independent critic, real-play evidence.

**Current sign-off (2026-09-29): wall dissolve COMPLETE and approved by
Laurence.** Start with the latest `HANDOFF.md` and `NEXT-SESSION.md`, not the
historical work queue below. Grain shader + exact recovery merged; validation
345 + 42 + 7 + 10 passed, `PLAY.bat` refreshed. Regular stepping is common35;
capsule draw-layer isolation repaired. School aura and moves stay parked.

**Laurence: start the new chat with:** "Read `engine/docs/HANDOFF-ORCHESTRATION.md` and run it."

You (the new chat) are the **orchestrator**. You don't write the fixes yourself: you set up agent lanes, check their work yourself (screenshots you look at, tests, reading the diff), merge, and report to Laurence. Read first: `AGENTS.md` (hard rules), `engine/docs/INDEX.md`, the top entry of `engine/docs/HANDOFF.md`, `engine/docs/PLAN.md` ("Moves track"), then this file.

## Laurence and how to report

Non-technical creative director. Plain English, outcome first, short bullets, **no code in replies**, screenshots for anything visual (`SendUserFile`), decisions as click-panels (`AskUserQuestion`) with a "(Recommended)" option. Failures first, with evidence. Budget ~£60/month across tools: Sonnet for bounded lanes, Opus only for program reading / hard investigation; say roughly what a round costs.

**What worked with him today (keep doing it):**
- When he reports a visual/feel bug, **reproduce first** (tick-exact screenshots, logs, his footage) before fixing. Two of today's "fixes" missed because they were built on a simulated reproduction that didn't match what he saw; asking him for a debug recording (`tools/bats/PLAY_DEBUG_CONTROLS.bat`) or a short video (Win+Alt+R) found the real causes.
- For position/size tweaks, **measure against the original** (his footage) with overlays — rounds of nudging by eye failed three times. When taste is involved, give **numbered option sheets (he picks a number)**.
- He records the installed original (Appx `Microsoft.MSEsper`) on request; recordings live in `%USERPROFILE%\Pictures\Phantom Dust - Moves\`, `... - Moves 2\`, `Phantom Dust Walk\` (never commit).
- The game window: lanes only run it with `--screenshot` (window opens off-screen). If a build fails with "failed to remove pd_game.exe", **he has the game open — ask him to close it, never kill it.**

## Update 2026-09-28 (visual cycle) — read this first

- `main` has the HUD wheel, real orbs, VFX fixes, lock-on marker and school aura (HANDOFF.md top entry). Tests 7 + 36 + 276.
- Hermes (OpenCode Go models) takes small bounded jobs from `engine/docs/handoffs/H*.md`. Check its results in the real game yourself (its first proof shots were black) and confirm it worked in its own worktree.
- Laurence's reference clips: `Pictures\Phantom Dust - Orbs\`, `Pictures\Phantom Dust - Moves 2\Lock-On Animation.mp4`, plus the earlier folders. Extracted frames: `engine/shots/orb/footage/`, `engine/shots/lockon/`.
- Laurence's colour rules: lock-on marker orange = close, yellow = medium, green = far. School aura Psycho red, Optical yellow, Nature green, Ki blue, Faith purple (ties in that order), shown from Level 1.
- Next: ask Laurence (panel) whether to resume the moves track or continue visual polish.

## State at hand-off (2026-09-27)

- `main` at `be49c3c` or later (local git, **never push**). Tests: `cargo test -p pd_game -p pd_formats -p pd_convert` → **272 + 36 + 7 green**. Strafing: after the pivot the body faces the stick direction (render-only; aim stays on the target). The debug build is current (`PLAY.bat`).
- The cache is current: stages (collision fixes), chars (colour-variant motions, cloth grid, single-sided, colour textures `c0..c3.png`, root-motion tables), skills.json (action + reaction tables), fx (packed archive, bones).
- No worktrees or lane build copies left. Tools for program reading are installed (see HANDOFF top entry / PROGRAM-RE.md "Method").

## What happened today (details: HANDOFF.md top entry)

- **Program reading** of `PDUWP.exe` (Ghidra) gave the original's real rules: every attack handler, guards, hit reactions, knock-down, movement, dodges, Aura, capsules, combos, effects attachment, animation rate (30 fps). Docs: **PROGRAM-RE.md** (+ FOOTAGE.md checks).
- **Built from it:** per-skill cast timing, hit reactions + knock-downs, movement (walk/run, turning, gravity, landing, strafing), colour-variant motions, packed effects, 30 fps clips, blade stride/sounds, effects on bones + game-style additive glow, Hero's jacket cloth + single-sided characters.
- **Fixed:** Palace/Lane/Highway/Panorama-Boss collision; camera (look-up range, top & bottom height, no tilt when close); HUD dial (cost digits, uses count, rims); locked-on strafing (walk/run clips swapped per the program table; the turn plays once then the WALK/RUN loop).
- **UI:** 3D character select, P2 alternate costume, menu feel, HUD pixel font, result stats, Randomize Arsenal, non-versus arenas hidden.
- **Design docs** with Laurence's decisions: DESIGN-BRAWL, DESIGN-2V2, DESIGN-REPLAYS.

## Waiting on Laurence

- Play-test the latest build: strafing (left/right, full/half push), the HUD rims, knife/effects brightness, hit reactions, movement feel.
- Menu sound ids are guessed (`frontend/sound.rs`) — needs his ear.

## Next work (suggested order — confirm with a panel first)

1. **Moves track remainder** (PLAN.md): M5 range bands + guard rules; M11 dodges + movement skills (jump-dodge DASHEND_B landing + wind-up; 751 is one move); M12 root motion in the sim (`cache/chars/<id>.rootmotion.json`); M13 Aura refill curve/pauses + 14 s capsule respawn at CP00 + 30-card deck; M14 combos, charge release, Change the World; M6 per-family motion fixes. Sim lanes share `combat.rs`/`sim.rs`: at most two at once, split by function.
2. **Brawl mode** build (DESIGN-BRAWL.md, decisions recorded).
3. **Effects leftovers:** strike pose swings sideways vs the original's thrust; the second render target (glow buffer, likely bloom); MSPARTICLE reading. (Knife ray size, LIGHT02 and the red glow round the attacker were done 2026-09-28.)
4. **HUD layout:** the original's four knobs sit further out on the ring, with a small tab at the bottom (hud4 overlay) — ask before changing.
5. Cloth collision shapes (COLF/COLP); head pieces one-at-a-time?; 2v2 after the moves track; online prep last (PLAN.md Phase 4).

## How to run lanes (proven)

- One git worktree per lane outside the project folder: `git worktree add -b lane-<name> ..\pd-lane-<name> main`; one build copy per lane: `robocopy engine\target %USERPROFILE%\.pdtools\target-<name> /E /MT:16` (robocopy exit code 1 = success). Update `engine/docs/LANE-BRIEF.md` (hash + test counts) first and give each lane the brief + a prompt naming owned files.
- Launch with the Agent tool, `run_in_background: true`. A finished lane can be resumed with SendMessage (it keeps its context) — give it a new worktree.
- When main moves while a lane runs, tell the lane to `git merge main` before its final commit.
- On report: look at its sheets, check the diff (sim stays deterministic, no game data committed), merge `--no-ff`, run the tests, rebuild, spot-check in game, clean up (worktree, branch, build copy).
- Test switches: `--stage`, `--near`, `--demo`, `--loadout a,b,c,d`, `--p1/--p2 pcNNa`, `PD_TEST_PRESS=<tick>,<slot>[,<walk>]`, **`PD_SHOT_TICKS=a,b,pN`** (tick-exact screenshots), `PD_TEST_STICK=<mag>,<deg>[,<tick>]`, `PD_TEST_SPLIT=tb|sbs`, `PD_MOVE_LOG=1`, `PD_ANIM_LOG=1`, `PD_FX_LOG=1`, `PD_SFX_LOG=1`, `PD_INPUT_LOG=1`, `PD_TONEMAP=none`, `PD_FX_GAMMA=1`, `PD_DIAL_OFF` / `PD_BEZEL_OFF`, `PD_SHOW_WINDOW=1`. Full list: GAME.md "Test switches".

## Before stopping

Update the top entry of `engine/docs/HANDOFF.md`, `engine/docs/NEXT-SESSION.md` and this file; report to Laurence with screenshots and the next choices as a panel.
