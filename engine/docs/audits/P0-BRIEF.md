# P0 audit wave — brief for every auditor (2026-09-30)

You are one of several parallel **read-only auditors** on the Phantom Dust
rebuild (Rust + Bevy fan runtime of the 2004 game). Laurence (owner, a
non-technical creative director) has asked us to bring every remaining area
up to the original. Before anyone writes code, each area gets an honest
evidence ledger. You write ONE markdown file; you change nothing else.

## Read first

1. `AGENTS.md` (hard rules), `engine/docs/INDEX.md`.
2. `engine/docs/CLAUDE-DEEP-PASSES.md` — the P0–P8 plan; your section of it.
3. `engine/docs/METHOD-FIDELITY.md` — **the method that finally fixed
   movement after a dozen failed rounds. Apply it.** Read the program's
   whole state machine / call sites and their arguments before judging;
   measure our data against the program's rule; no tuning by eye; tests
   from reads, not from old behaviour.
4. `engine/docs/DECISIONS.md` (Laurence's calls — do not reopen without new
   facts) and the docs your area names.

## Evidence sources (all local)

- Our code: `engine/crates/*/src`. Tests: `cargo test` is NOT yours to run
  (parallel auditors would fight over the build folder). Read only.
- The original PC program, decompiled: `%USERPROFILE%\.pdtools\re-out\decomp_all.c`
  (16 MB, Ghidra C of every function in `game_files/PDUWP.exe`),
  `functions.txt` (addresses), `strings_all.txt`. Helpers in `engine/re/`
  (`decomp_fn.py`, `xref.py`, `call_consts.py`, `exe_const.py`,
  `state_table.py`, `action_table.py`). **Do not launch Ghidra** (one shared
  project, it locks). If you need a disassembly Ghidra can't give, list it
  under "Reads still needed".
- Existing findings: `engine/docs/PROGRAM-RE.md`, `FORMATS-*.md`,
  `FOOTAGE.md` (Laurence's recordings of the original), `audits/*.md`,
  `VFX-AUDIT.md`, `MOVES.md`, `FRONTIER.md`, `ROADMAP.md`, `PLAN.md`.
  Treat older "done" claims as leads, not proof.
- Game data (read-only, never copy into git): `game_files/`, converted
  cache `engine/cache/`.

## What to produce

Write **`engine/docs/audits/P0-<AREA>.md`** (your area name is in your
prompt) with these sections:

1. **Ledger** — a table, one row per feature/behaviour in your area:
   `Feature | Mapped | Implemented | Reproduced | Verified | Laurence-approved | Evidence`.
   Milestones are distinct: *mapped* = we know what the original does (with
   program address / file field); *implemented* = code exists (file:fn);
   *reproduced* = our runtime was seen doing it; *verified* = tested against
   the original's rule; *approved* = Laurence said so (cite DECISIONS/HANDOFF).
   Use ✅ / ❌ / ~ (partial) and say which.
2. **Discrepancies** — every place our code differs from the program's
   rule, most important first: what the original does (address + the
   decisive line/constant), what we do (file:line), expected visible effect.
   Mark each CONFIRMED (you read both sides) or SUSPECTED.
3. **Reads still needed** — program questions you could not settle, with
   the addresses to start from.
4. **Proposed build lanes** — 1–3 bounded lanes to fix the discrepancies:
   goal, exact files/functions owned (so lanes don't overlap), the tests
   that would prove it (citing addresses), and the runtime proof
   (scripted `PD_SHOT_TICKS` captures, logs). Order them.
5. **Questions only Laurence can answer** — only things needing his eyes/ears
   (e.g. "which of these two clips is the capsule pickup"), max 5, each
   phrased as a multiple choice.

Be concrete and brief; addresses and file:line beat prose. Do not pad. If
something is already right, say so in one row and move on. Honesty over
completeness: "SUSPECTED, not read" is a fine answer.

## Rules

- **Edit nothing except your own `P0-<AREA>.md`.** No code, no other docs,
  no git commands that change state (no add/commit/checkout/stash).
- Never copy code from `tools/Esper-RE` (GPL). Never put game data in git.
- Don't launch the game, the viewer or Ghidra.
- Final reply to the orchestrator: 5–10 lines — the file path, the top 3
  discrepancies, and the lanes you propose.
