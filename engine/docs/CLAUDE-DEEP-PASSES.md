# Recreation evaluation and dedicated Claude passes

> **Archivist, 2026-10-03:** executed. P0-P8 became waves 1-6 (`WAVES.md`); "planning only" lines below are history. Kept for the reference gates and file fences. Current state: `NEXT-SESSION.md`.


> 2026-09-30: every pass should follow `METHOD-FIDELITY.md` (how movement was fixed; P2 movement is done and approved).

**Date:** 2026-09-30. **Requested by:** Laurence. **Status:** planning only;
no agents launched, implementation or premium-model spending authorised by
this document alone. Start execution when Laurence asks to run the passes.

## Candid baseline — source of truth

We have a substantial two-player reconstruction, not a verified faithful
recreation or a four-player party game. Asset/table coverage is broad; runtime
fidelity and integration are uneven. Do not give an overall completion percentage.
Skill dispatch coverage is NOT proof of skill correctness. Likewise, no FAMILY
case is NOT proof a skill is missing: some skills are handled elsewhere.

Laurence's current evaluation takes precedence over older "done" lists:

| Area | Current verdict | Dedicated pass |
|---|---|---|
| Steps | Good; regular clip is `27_id035` / common35 | Protect as a regression |
| Capsule collection AND use audio | Not correct; separate events, not interchangeable | P1 |
| Locked strafing; walking-to-running sensitivity/detection | Close, still off against original | P2 |
| Move effects and their animation | Not perfect; converted effects aren't faithful by default | P3 |
| Lighting | Needs a dedicated directional/depth/atmosphere pass | P4 |
| School aura | Existing approaches rejected; build anew with modern techniques | P5 |
| Four-player split screen and UI | Not built as a complete system; two-player assumptions throughout | P6 |
| Arsenal builder | Functional, not matched to the original | P7 |
| Gamemodes | Needs its own scope/design/build/validation pass | P8 |
| Near-wall grain dissolve | COMPLETE, explicitly approved | Preserve; do not reopen without regression |

Last actual merged validation: 345 game / 42 formats / 7 converter / 10 viewer
tests passed, build and shader runtime smoke succeeded. These counts measure
tests, NOT recreation fidelity. Re-run the baseline at execution time.

## Who does what

- **Claude orchestrator, high effort:** owns the evidence ledger, lane briefs,
  worktrees, merge order, independent verification and short reports to Laurence.
- **Strongest available Claude reasoning model, high effort:** reverse
  engineering, movement/combat state machines, rendering architecture,
  four-player generalisation and difficult cross-system diagnosis. Max effort
  only for demonstrated blockers. Discover exact available model IDs first.
- **Claude implementation lanes:** bounded work after the mechanism/spec is
  established; do not save budget by assigning weak models unresolved architecture.
- **Independent Claude critic per wave:** different session/context from builders;
  seek false claims, untested lifecycle paths and integration regressions.
- No fixed worker count or cost promise. Quote available-model costs and a
  bounded wave budget before launching; do not assume earlier free-model routing.

## P0 — establish evidence before more changes

Read AGENTS, INDEX, current HANDOFF, this plan, relevant format/code docs. Audit
the actual code rather than trusting ROADMAP history. Build a per-feature ledger
using these distinct milestones: **mapped / implemented / reproduced / verified /
Laurence-approved**. Record exact evidence and uncertainty independently.

For skill fidelity, enumerate the actual usable roster and trace each skill's
full path (not only behaviour_of). Recheck pending PLAN M5/M6/M11–M14 against
current code. Scope remaining combat rules/root motion/destruction separately;
do not silently include all historical backlog work in presentation passes.

Create a small fixed reference battery: capture vs use, idle/walk/run/locked
strafe, an unlocked→locked turn, shell/laser/melee/guard/status/environment
examples, close camera, bulky and coat characters, same-character P2 costume,
second match, pause/unpause. Use original footage + tick-controlled runtime
captures, real controller input for sensitivity. Record camera/distance/pose.
No new implementation until the affected reference behaviour is reproduced.

## P1 — sound and the capsule lifecycle

**Investigate:** capture into a button vs activation from that button; Aura vs
status/heal vs ordinary skills; cast voice/effect sound; appear/refill/erase;
who hears it in which view. Current wired id92 is NOT confirmed faithful simply
because it plays or a decompiled path references it. Treat capsule-use audio as
an open trigger/routing question, not reuse the collection cue by assumption.

**Files:** sfx.rs, combat event sites by explicit sub-fence, fx.rs only if needed.
**Proof:** ear-labelled original clips with correlation where useful; runtime
event→cue trace with no duplicate/missing triggers; capture THEN use in a real
scripted match, both players, restart/refill. Laurence listens to final result.
**Close:** collection and use sound/timing separately approved. Steps35, wet
sounds, pause cue and existing attack audio regressions protected.

## P2 — movement fidelity, not further isolated threshold nudges

**Investigate as one state machine:** raw/quantised stick, deadzone, magnitude
curve, walk/run hysteresis, turn slowdown, forward/side/back classifications,
TURN pivot, baked animation yaw compensation, transition timing, recovery and
lock-on camera. Preserve distinctions between sim facing and rendered facing.

**Files:** sim.rs and main.rs animation/input/camera sections, any combat lock
interaction explicitly scoped. No unrelated move-family rewrites.
**Proof:** original + real-pad logs, held diagonals, tiny push, gradual ramp
up/down, tremor, release/reverse, moving lock-toggle, cast/dodge interruptions;
tick-controlled videos and deterministic replay tests. Tune joined behaviour,
not one screenshot. Keyboard checks supplement rather than replace pad proof.
**Close:** Laurence approves strafe sequencing AND walk/run sensitivity.

## P3 — move effects and animation fidelity

**Investigate complete timelines:** input→cast animation→charge→release→flight/
swing→hit/block→recovery, both attacker/opponent cameras; grounded/airborne and
locked/unlocked paths. Compare bones, orientation, scale, texture/UV motion,
blend, duration, particle density, trails, easing and sound alignment. Separate
cast-pose/root-motion problems from effect-reader/render problems and lighting.

**Files:** effects.rs, fx.rs, effect shaders, main.rs animation path by fence;
pd_formats/pd_convert only for evidenced decoding errors. Start from VFX-AUDIT,
FORMATS-EFFECTS, FOOTAGE, PROGRAM-RE, MOVES; those docs are leads, not approval.
**Priority sample:** Psycho Knife/Wave, Laser/Slide Laser/Blaster, Crystal Wall,
one projectile per distinct motion family plus status/environment skills.
Expand by family only after a representative is faithfully validated.
**Proof:** matched phase-by-phase sheets + video (stills cannot prove timing),
close/bulky views, no-effect controls and localized pixel comparisons. Pin
discovered parsing mistakes with tests that fail before the fix. Fresh conversion
must reproduce corrected assets; don't depend on undocumented cache patches.
**Close:** measured per-family verdicts and Laurence's approval; keep exceptions
explicitly open, never claim all effects good from a handful of samples.

## P4 — lighting as a coherent rendering pass

There IS a DirectionalLight in main.rs; shadows are disabled and much stage
lighting is baked into vertex colours. That implementation fact does not
contradict Laurence's observation that directional lighting is absent in the
result. Do not dismiss his visual complaint with a component search.

**Investigate:** stage baked lighting, unlit vs lit materials, character light
sets, directional key/fill, shadows, normal maps, effect illumination, glow,
exposure/fog/grade. RESEARCH-LIGHTING's "already faithful" claims are not accepted
look validation. Avoid double-lighting already baked surfaces.
**Files:** stage_look.rs, character/stage material setup, converter lighting
sections if necessary, effect lighting; schedule sequentially with P3/P5.
**Proof:** measured samples of indoor/outdoor/dark/night arenas, consistent
exposure, contact/shape readability, characters during attacks, mobile camera;
faithful vs enhanced A/B if still desired. Benchmark 2 views, then all 4 at
target TV resolution; establish a measured budget, not guessed FPS.
**Close:** Laurence approves visible direction/depth and atmosphere across the
sample stages, with no global brightening that washes out the original look.

## P5 — new school aura, modern silhouette-aware approach

Read H3-school-aura-handoff before proposing anything. Do NOT retune the rejected
inflated-mesh shell, hoop or body-sphere cloud into another "completed" version.
Use a researched modern silhouette-mask glow/wisp/trail approach, or justify an
alternative against the same close-camera bar before implementation.

**Spec:** animated pose-following soft glow, saturated inner colour, no exposed
triangular geometry or blob envelope, subtle animated wisps, movement trailing,
ground spill if retained. Establish school colour/tie/Level gating and the brief
consume saturation/opacity pulse separately from the presentation architecture.
**Files:** school_aura.rs/WGSL, render graph/mask integration in main.rs; effect
interface if needed. Sim only exposes already-existing state/events.
**Proof:** slim/bulky/coat fighters FIRST, moving close and far, every school,
consume pulse, behind scenery, wall dissolve, same-character costumes and both
viewports. Carry architecture through four-view prototype and profile it.
**Close:** one small prototype reviewed early, then implementation and final
Laurence approval. No three unreviewed tuning rounds on distant screenshots.

## P6 — four-player foundation and UI

**Architecture first:** sim/fighters/inputs, explicit current target (replace
1-i assumptions), teams/friendly-fire policy hooks, inactive slots, win/draw
resolution, captures and effects/audio ownership, cameras/render masks, setup,
controller joining and HUD. Four-player support is NOT a viewport-only change.
Read DESIGN-2V2; its revive rules remain unverified leads until specified.

**Files:** sim/combat and moves interfaces, main, menu, HUD, frontend setup/state;
cross-cutting migration led by Claude with explicit per-file/function ownership.
Rendering may need per-view mask/dissolve identity instead of the current
two-view viewport-origin heuristic. Preserve approved dissolve appearance.
**Proof:** 1/2-player regressions; 3/4-controller routing, simultaneous actions,
each player sees correct capsules/reticle/effects, target cycling, downs/draws,
second match, disconnect handling, deterministic four-player replay. HUD text,
skill dial, cost/uses, health/aura/status readable at quarter-screen TV size.
**Close:** actual four-human-input match + usable setup/HUD approved, not a
static four-camera screenshot. Ask only unresolved controller/layout choices.

## P7 — arsenal builder fidelity

Separate pass from quarter-screen combat HUD. Compare the original's actual
builder flow and DckMenu art: browsing, selection, card detail, filtering,
duplicates, aura particles, legality, save/load/edit/delete, focus/back behaviour.
Determine whether the builder is shared/sequential or per-player during setup
before imposing four miniature builders. Keep no unlocks/progression.

**Files:** frontend/builder.rs, arsenals.rs, menu art/font layout components;
arsenal.rs only for verified legality errors. No overlapping frontend lane with P6.
**Proof:** pad-only original-vs-recreation journey, build valid/invalid decks,
save/reload/replace, assign distinct arsenals to all players; screenshots and
input trace, legibility and no focus traps.
**Close:** fidelity and practical couch usability both approved by Laurence.

## P8 — gamemode pass

First inventory implemented modes vs settings vs design-only ideas. Establish
a named rules contract: eligible players, teams, skill source, aura/capsules,
timer, victory/draw, rounds/restart and results. Reference original standard
versus separately from house modes; don't assume a fan-guide variant is exact.

Review DESIGN-BRAWL (fixed loadout/infinite uses/aura policy) and DESIGN-2V2
(targeting/revival) with Laurence. Decide the initial supported set before
building. Prop Hunt/Skate/modding/rotation are backlog, not automatic scope.

**Files:** Rules/combat/sim, frontend mode/setup/result, target/teams helpers.
**Proof:** full start→play→finish→rematch for each approved mode, every supported
player count, invalid setups rejected, draw/time-up/double KO/team survival and
revive cases if selected; no leaking rules into the next mode. Automated
deterministic rules tests + a couch play session.
**Close:** approved mode matrix implemented and verified, not extra menu entries.

## Execution order and safe parallelism

1. **P0** baseline/reference ledger and design/scoping only.
2. **P1 + P2** may run together only after event/animation file fences are proven;
   serialize shared combat/main work. Stop for integrated capsule/movement review.
3. **P3** timeline/render fidelity. In parallel P6/P7/P8 may do read-only research
   and specs, not overlapping implementation.
4. **P4**, then **P5** (lighting pipeline/glow contract established first). A
   bounded aura prototype can precede full lighting only with explicit interface.
5. **P6** four-player migration, then **P7** builder and **P8** selected modes.
   Mode design starts earlier; implementation depends on relevant player/target
   foundation. P7/P8 only parallelize genuinely disjoint files.
6. Final four-player combined acceptance/performance wave plus independent critic.

Each pass is an outcome milestone, not one huge autonomous agent job. Break it
into bounded research→spec→implementation→verification units with checkpoint
receipts; allow review between passes. Do not launch all passes at once.

## Gates and lane contract (mandatory)

- One owned worktree and build directory per lane. Record base hash, exact
  allowed files/functions and migration number/API ownership where applicable.
  Shared main.rs is NOT a free-for-all: one integrator owns it per wave.
- Never reset/restore/stash/force-remove other work. Inspect clean/dirty state
  before cleanup, preserve unfamiliar changes. Commit named source/docs only.
- Protect approved dissolve and regular steps. Preserve capture ownership,
  P2 alternate costumes, transitions/rematches, deterministic simulation.
- Tests run foreground in the lane. Orchestrator reruns on lane and merged tree,
  builds, inspects images and exercises the actual interactive production path.
  Exit codes must propagate; no filter pipeline hiding failure or fake greens.
- Use tick-controlled inputs for simulation; real-pad/real-menu lifecycle proof
  when harness doesn't exercise that path. Do not confuse --near with menu entry.
- Every wave has an independent critic: CLOSED vs STILL OPEN, with reproducible
  evidence. A test for the proposed constant doesn't validate its assumption.
- Laurence approves feel/visual/audio milestones. Store local proof artifacts
  with stable paths; derived game assets/footage never committed/uploaded.
- After each merge, update HANDOFF, FRONTIER/pass ledger, actual test counts and
  next blocker. No pushes without Laurence asking.

## Suggested next Claude session prompt

"Read AGENTS.md, engine/docs/INDEX.md, the latest HANDOFF and
engine/docs/CLAUDE-DEEP-PASSES.md. Orchestrate a high-effort Claude recreation
pass. Start with P0 and propose a bounded P1/P2 wave with evidence, ownership
fences, available-model cost and acceptance gates. Do not trust historical
completion percentages; collection and use are separate audio cases, move VFX
remain imperfect, school aura needs a new approach, and four players requires
architecture. Preserve the approved dissolve and common35 footsteps. Verify
builders independently and obtain Laurence's approval between major passes."
