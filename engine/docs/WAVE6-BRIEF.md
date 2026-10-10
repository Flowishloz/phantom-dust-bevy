# Wave 6 brief: shared rules for every lane (2026-10-01)

Read with `AGENTS.md`, `engine/docs/LANE-BRIEF.md` (build/run, screenshot
harness), `engine/docs/METHOD-FIDELITY.md` (the method) and, for sim work,
`engine/docs/SIM-LANE-BRIEF.md`.

## Laurence's standing rules

- **No guessed values.** Tag every value you ship:
  - READ: a program address, from `%USERPROFILE%\.pdtools\re-out\decomp_all.c` (image base 0x140000000; helpers in `engine/re/`; no Ghidra; capstone in a scratch venv is OK).
  - MEASURED: from footage, `%USERPROFILE%\Pictures\Phantom Dust - *`.
  - LIVE: from a trace of the PC original (`engine/re/live_trace.py`, `engine/re/move_recorder.py`; see `MOVE-RECORDER.md`).
  - Anything else is UNCONFIRMED: say so, and don't fill the gap with a pick.
- Scope is **versus only** (no story mode).
- Never commit game data, frames or screenshots. Never push. Never send OS input to any window. Never touch `engine/userdata`. Never run the original game. The orchestrator owns the recorder and the live game.

## Mechanics

- Your worktree is `<repo>\lanes\<name>` on branch `lane/<name>` (the orchestrator gives the base commit). `engine\cache` there is a junction to the main cache: never delete through it.
- Build in your own target folder: run `. .\engine\env.ps1`, then set `$env:CARGO_TARGET_DIR` to `<worktree>\target-lane`.
- Edit only inside your fence. Shared edits must be MINIMAL and ADDITIVE, and listed in your report.
- If you change the sim, re-record only the replay goldens that move, each with a written reason. The movement-only golden never changes.
- Runtime proof: scripted captures go to the MAIN checkout `<repo>\engine\shots\` with your lane prefix. Look at them yourself. Use the absolute cache path `..\..\cache`, because a relative path resolves from the exe folder.
- If you change a converter, regenerate into your own worktree only, and put the exact command in your report.
- Finish:
  1. `git merge main`.
  2. Run the full suite: `cargo test -p pd_game -p pd_formats -p pd_convert -p pd_viewer`, plus `python -m unittest` in `engine/re`.
  3. Commit named files only, with the message ending `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
  4. Final report ≤ 12 lines: commit, what's done, READ/MEASURED/LIVE/UNCONFIRMED per value, evidence paths, shared edits, requests.

## Who owns what (concurrent lanes)

| lane | fence |
|---|---|
| moveskills (finishing) | `moves/special.rs` LEVITATE/DASH/Skywalk, `moves/erase.rs` s45=2, `moves/mod.rs` `charged`, `sim.rs` float/dash state |
| moverec | `engine/re/move_recorder.py` and helpers, bats |
| fxopen | `effects.rs`, `fx.rs` (skill-effect parts, NOT capsule functions), `aura_fx.rs`, `pd_convert` effect export |
| capfx | `capsule_birth.rs`, `fx.rs` capsule functions only (`sync_combat_visuals` capsule arms, `pickup_burst`, orb fade-in) |
| reactions2 | `combat.rs` reaction functions (`strike`/reaction-row arms, new kinds 7/8/9), `combat/reactions*`, `main.rs` animation fns (`clip_for`, `cast_clip`, `drive_animation`) |
| hudbits | `hud*.rs`, `frontend/state.rs` + setup screen files, `views.rs` world marker |
| probes | `combat/skill_audit.rs`, `engine/re/skill_audit*.py` (tool only, no game behaviour) |
| aiaudit | read-only; writes `engine/docs/audits/P0-AI.md` only |
