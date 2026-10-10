# PC lane brief (the session running on Laurence's computer)

Updated 2026-09-30. This file is the single source of instructions for the PC lane.
Cloud reference: `origin/claude/trusting-cori-if07av`; main remains source of truth.
Laurence can say "read engine/docs/PC-LANE-BRIEF.md and do the next unticked step".

## Who does what

- Cloud session (no game data/Ghidra): docs, audits and tools, synthetic tests,
  merges and this brief.
- PC lane (real files/Ghidra): local tool runs, program reads, footage/traces,
  verified findings and contradictions.

## Ground rules

1. Main is source of truth. Read AGENTS, FRONTIER, CLAUDE-DEEP-PASSES first.
   Audit/tool branch is reference; merge wholesale only when Laurence agrees.
2. Cloud claims are hypotheses until verified on real data.
3. Never commit game-derived audio/dumps/screenshots/tables; keep outputs in
   ignored engine/shots. Document findings in words with evidence addresses.
4. No gameplay changes (sim and friends) without Laurence's permission.
5. Never copy community code (GPL or unlicensed).
6. Short, outcome-first reports. Write evidence into docs and push to main.
7. Budget tight: only the next unticked step, then stop and report. No plug-in,
   ISO work or gameplay code as a side task.

## Steps (tick when done; stop after each and report)

- [x] **1. Four tools on real data.** Done 2026-09-30. Fixes in `043fba3`;
  all four run, eight tests pass. Outputs `engine/shots/tool-check/`, ignored.
- [x] **2. Locked-on gait read in Ghidra.** Done 2026-09-30: ten finding lines
  with addresses under `MOVEMENT-AUDIT.md` → "Program read results".
  `FUN_14025b270`, `FUN_14025b5e0`, `FUN_14025ad50`, with machine-instruction
  cross-check: two gait speeds/clips, no continuous stick-speed blend; direction
  selected on entry, held through strafe; thresholds use rescaled input.
- [x] **3. Dead-zone and strafe reads.** Done 2026-09-30, reads only: twelve
  addressed findings under `MOVEMENT-AUDIT.md` → "Program read results, step 3".
  Square dead zone, unclamped hypot magnitude, stateful gait band, pivot-lifecycle
  exits, skill-dependent cast aim and traced 30-Hz input path verified; sim/main
  contradictions recorded without changes.
- [x] **3b. Exact strafe state machine.** Done 2026-09-30, reads only: twelve
  addressed lines under `MOVEMENT-AUDIT.md` → "Program read results, step 3b".
  Entry body/camera comparison, held-stick ordinary continuation, release/reset,
  heading initialisation/update, full-speed pivot translation and inverted
  WALK/RUN clip-name pairs checked against the executable. Stop here; step 4
  needs the next go. No gameplay changes authorised or made.
- [x] **3c. Pivot clip lengths.** Done 2026-09-30, reads only: eight Hero
  colour-0 lengths and clip-driven completion timing under `MOVEMENT-AUDIT.md`
  → "Program read results, step 3c". Walking uses TURN_RUN, running TURN_WALK;
  strict greater-than completion, then state handoff next movement update.
  Stop here; no gameplay changes made or authorised.
- [x] **3d. What starts a strafe while moving** (reads only). Done 2026-09-30 by the locked-on lane: MOVEMENT-AUDIT.md "Locked-on lane" reads 1-5 (states 21/22 are skill poses, 29 a timed wait; state 1 never calls entry; the only caller is the dispatcher; a moving player re-enters through a zero-stick tick or an action ending). Original brief: Play shows the original starts a strafe while already running, which step 3b finding 2 (entry dispatched for states 0/21/22/29, not state 1) does not explain. Answer in 10 lines or fewer, with addresses: (1) what are states 21, 22 and 29 (are any of them moving states, e.g. a run or turn state)? (2) does the ordinary-movement handler (state 1, `0x14025ad50`) call `0x14025b5e0` itself, or hand the state back to one of those states, when locked on and the stick moves off-axis? (3) is there any other caller of `0x14025b5e0`? (4) while running toward the target and then pushing sideways, what exact sequence of states results? Append under "Program read results, step 3d" in `MOVEMENT-AUDIT.md`. No gameplay changes.
- [ ] **4. Sound call-site census:** from `FUN_14025a3e0` identify the sound-id
  player, run `engine/re/call_consts.py`; exclude already-labelled ids.
- [ ] **5. Listening session** (Laurence, using the sheet).
- [ ] **6. Plug-in logger:** needs PDPL downloads/sideloading and explicit go;
  see audit branch `engine/probe/README.md`, TODO-LAURENCE section 3.
- [ ] **7. Golden traces:** original logger vs recreation PD_TRACE, then compare;
  see audit branch GOLDEN-TRACE and trace_compare tools. Not authorised here.
- [ ] **8. Xbox disc (optional):** extract then cheap identifier/string/inventory
  checks only. Xbox Ghidra/xemu work needs Laurence's go-ahead.
