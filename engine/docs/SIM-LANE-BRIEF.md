# Sim lane brief (2026-10-01) — for the parallel combat lanes

Three lanes edit the simulation at once (`reactions`, `guards`,
`projectiles`). Each owns named FUNCTIONS, not whole files. Read this with
`LANE-BRIEF.md` (build/run, hard rules) and `METHOD-FIDELITY.md` (the
method). OVERRIDE of LANE-BRIEF rule 3: you may edit `sim.rs`, `combat.rs`,
`combat/*`, `moves/*` — but ONLY inside your fence below.

## Shared rules

- Base commit `c0657da`. Tests at start: 490 game + 51 formats + 7 convert +
  15 viewer. Deterministic sim: no time, no unseeded randomness, no Bevy.
- **Fences.** Edit only the functions/files your prompt names. If you need a
  change elsewhere (a new field on `Fighter`, a new `CombatEvent` variant, a
  helper in someone else's fence), make it MINIMAL and ADDITIVE (append new
  fields/variants at the end, never rename/reorder), and list it in your
  report under "Shared edits". New `CombatEvent` variants: add them, but do
  not wire sound/effects for them — an events lane does that afterwards.
- **Replay goldens** (`combat::tests::replay`): your rule changes will change
  some hashes. Re-record ONLY the scenarios your change affects, with a one
  line written reason each next to the table. The movement-only 1v1 golden
  must never change.
- Every rule you build cites the program address it was read from, in a test.
  Verify the audit's reads in `%USERPROFILE%\.pdtools\re-out\decomp_all.c`
  first (capstone in a scratch venv for dropped args; no Ghidra) and report
  any audit claim you find wrong.
- Runtime proof: scripted captures (`PD_SHOT_TICKS`, `--loadout`, `--near`,
  `PD_TEST_SPAWN`, `PD_ANIM_LOG`) to the main checkout's `engine/shots/` with
  your lane prefix. Look at them yourself.
- Before your final commit: `git merge main` into your branch (other lanes
  may have merged), resolve, re-run the full suite.
- Commit on your branch (named files only; message ends
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`). No push, no
  merge to main. Final report ≤ 20 lines: commits, done/not done, evidence,
  audit claims wrong, shared edits, requests (sound/effects/HUD/anim).

## Defaults already decided (DECISIONS.md 2026-10-01)

Original behaviour wherever it is read: heavy-hit collapse ~3.7 s helpless;
mines/bombs/Tornado can hurt their own setter; a hit during wind-up keeps the
skill and its Aura (payment is at fire since `castrules`); a broken barrier
leaves its button; status slots replace timed amounts with caps.
