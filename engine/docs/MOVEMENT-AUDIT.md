# Movement audit: walk/run and strafe vs the original (2026-09-30, cloud, read-only)

Laurence reports that the walk-to-run change and the strafing "are guesswork and not accurate". This is a document-and-code audit only: nothing was run (no game data or Ghidra export in the cloud session). It lists where our code (`pd_game/src/sim.rs::step_player_at`, `main.rs::strafe_sequence` / `visual_faces_movement`) follows the program read in `PROGRAM-RE.md` "Ground movement" and "Lock-on", where it adds something the program does not do, and what to check in the program next. Nothing here is a finding about the program itself; the checks are for the PC.

## Reconciliation with `main` (added 2026-09-30, after merging a41037f)

This audit was first written against an older copy of `main`. Since then `main` (GAME.md, DECISIONS.md, 2026-09-29) has already changed much of what is listed below. Read the list as "what the audit found" and this box as "what is now stale":

- **H (the spasm):** largely addressed on `main`. The pivot now plays once per strafe engagement and mid-strafe direction or gait changes no longer replay it (DECISIONS 2026-09-29). The left/right and back-cone splits got the same +/-5 degree hysteresis (`LOCK_STRAFE_SIDE_BAND`, `Player::strafe_right`, `strafe_back`). Whether Laurence's "spasm towards the camera" is gone is not recorded; ask him.
- **D (the 5 degree band):** now a documented choice, extended to the side splits. Still unconfirmed against `FUN_14025b5e0`.
- **A (dead zone) and "run kicks in too late":** `main` lowered the run threshold from 0.7 to 0.6 (Laurence's feel call, DECISIONS.md). The program read says 0.7, so this is a known deviation, not a verified fact.
- **C (facing during strafe):** `main` has a large body of render-side work (baked hip-yaw cancellation, 0.4 s turn delay, visual turn rate 300 deg/s). Those numbers are tuned to Laurence's eye, not read from the program.
- **Current verdict (FRONTIER.md, CLAUDE-DEEP-PASSES.md P2):** "locked strafing and walk/run sensitivity are close but still off against the original". The golden-trace tools and the program reads are how to settle the remaining gap; the audit's open questions (graded speed in the lock-on path, direction choice, dead-zone compare) are still open.

## Program read results (PC, 2026-09-30, merged from main)

Verified on the real program by the PC lane. These override the hypotheses below where they conflict.

2026-09-30, PC step 2. Exactly ten finding lines:

1. `0x140256e80`: stick magnitude selects two gait states (`+0x18`): run above 0.7, walk below 0.4; no third tier in this dispatcher.
2. `0x14025b276–0x14025b2a8`: locked strafe speed = speed-status multiplier (`+0x24`) × selected gait constant, NOT stick magnitude (`+0x20`).
3. `0x1402f7d5c / 0x1402f7e8c`: those constants are 0.6944444 / 1.8518518 units per 30-Hz tick (20.8333 / 55.5556 units/s).
4. `0x14025b270`: gait also indexes the TURN clip pair and queued ordinary WALK/RUN; `+0xc8` playback scale receives the status multiplier, not stick strength.
5. `0x14025ad50`: ordinary movement also selects WALK/RUN by gait; its continuous speed variation comes from turning slowdown, not magnitude scaling.
6. `0x14024f95a–0x14024f9b1`: the turn helper returns `max(0, 1 − remaining facing error/90°)` in XMM0; Ghidra incorrectly calls it void.
7. `0x14025b5e0`: strafe direction (`+0x1c`) is chosen on entry from camera-relative stick angle; exactly-back input randomly chooses a back side.
8. `0x14025b270`: while in strafe state 8, direction stays stored; heading follows the live stick, and gait changes retime the pivot rather than reselecting its side.
9. `0x140256e80`: gait compares fighter `+0x14268`, input-block `+0x20`; `0x140256710` computes that magnitude from the dead-zone-processed axes.
10. `0x140256600`: each axis subtracts the signed 13,000 dead zone then divides by 19,767; comparison is rescaled, not raw pad strength; full A–D follow-up remains step 3.

### Evidence and limits

- Read the existing Ghidra export at
  `%USERPROFILE%\.pdtools\re-out\decomp_all.c` (three requested functions,
  plus only their immediate movement/input dependencies needed to answer the
  brief). Independently disassembled the local `game_files/PDUWP.exe` with
  MinGW objdump. Read float constants directly from the PE by VA.
- Local disassembly extracts: `engine/shots/gait-read/` (ignored, never commit).
  Reproduce with objdump `-d -M intel --start-address=<VA> --stop-address=<VA>`;
  ranges are named in findings above. No decompiled game code copied here.
- The ordinary-movement return path is absent in the decompile's void helper:
  its instructions establish the XMM0 factor, which the caller multiplies by
  the status multiplier before the selected gait constant. A decompile-only
  read would miss that distinction.
- Direction selection is not rerun each frame of state 8. This does not prove
  all possible interruptions/re-entry conditions; those belong to the next
  explicitly unticked step. Status effects, animation root motion and actual
  footage can still affect apparent speed. No live controller trace was taken.
- This is evidence for the original's requested paths, not permission to tune
  the recreation, remove its hysteresis, or change its gameplay.

### What this changes (cloud reading, to be confirmed by a live trace)

- **The original has no graded stick-to-speed blend.** Two gaits only (findings 1 to 5). Laurence's "medium to hard push is graded" is therefore not stick-strength scaling in the program. The graded feel in the original can only come from the turn slowdown (finding 6) and the two-speed switch itself. The earlier audit item 0 is answered: no.
- **The gait compares the dead-zone-rescaled stick (findings 9, 10).** Dead zone is 13,000 of 32,767 (0.397) per axis, then divides by 19,767. Run needs a rescaled 0.7, about a raw 0.82 push straight up; walk is held until rescaled 0.4, about raw 0.64. Because the rescale is per axis, a diagonal push needs more raw deflection still. Our sim compares the unrescaled stick against 0.6/0.4, so on paper ours runs **earlier** than the original. That does not match "run kicks in too late", so either the feel complaint has another cause (pad range, the later turn/pivot timing, locked-on clip timing) or something else differs. Do not change the threshold on this alone. Settle it with the golden trace (S02, S09) and a real-pad input log.
- **The strafe direction is chosen on entry and held (findings 7, 8).** That supports main's "pivot plays once per engagement" and stickiness work and is consistent with the spasm being ours. The exact re-entry rules are still open (step 3).

## Program read results, step 3 (PC, merged from main)

2026-09-30. Reads only; Ghidra export cross-checked against local executable
instructions. Twelve finding lines (including contradictions):

1. `0x140256600`: square/per-axis dead zone; each axis independently subtracts signed 13,000 then divides by 19,767; no radial test precedes this in `0x14028e710`.
2. `0x140256600 / 0x140256710`: two saturated positive axes give magnitude √2 ≈ 1.414214, NOT capped at 1; hypothetical int16 −32,768 on both axes gives ≈1.414285 (actual pad gate determines reachable diagonal values).
3. `0x140256710`: magnitude is `sqrt(x*x + y*y)` from the current processed axes, not max-axis; no smoothing/previous magnitude is mixed in (button history is separate in `0x140256580`).
4. `0x140256ef8–0x140256f22`: gait retains its previous state throughout [0.4, 0.7], including endpoints; switches only on >0.7 or <0.4, using processed magnitude.
5. `0x14025b703–0x14025b855`: strafe ENTRY requires active lock and body/camera alignment within ≈10°; direction split is plain ±45°/±135° relative to facing-adjusted stick angle, with no 40°/50° band.
6. `0x14025b2ad–0x14025b324`: zero processed magnitude ends state 8, copies heading `+0x12e6c` into facing `+0x12e68`, and selects idle/state 0 (movement call occurs before this test).
7. `0x14025b270`: swinging forward or losing lock does NOT directly exit an already-playing TURN pivot; this handler has no new 45° test or active-lock check.
8. `0x140257510 / 0x14025b270`: pivot completion switches to queued WALK/RUN, restores facing from heading, then the non-TURN branch changes state 8→1; stored strafe side is not reselected during the pivot, but heading turns toward live stick.
9. `0x140258180`: cast aim is skill/target-dependent, NOT universally either yaw: eligible locked casts calculate target yaw into state `+0x88`; another branch uses camera yaw/pitch.
10. `0x140258585–0x14025859e`: fallback casts from state 8 first copy heading into facing then cache facing; `0x14025bd40` turns facing toward cached aim, and `0x14025be40` melee/dash tracks targets and rotates/moves using facing, not a direct heading read.
11. `0x1401c85c0→0x1401c7630→0x1401d1250→0x14028ea10`, then `0x14024ec80→0x140256580`: game samples/processes stick once per 30-Hz logic tick (up to three catch-up ticks); no extra render-frame stick read in this traced path; OS device sampling itself is not measured.
12. Contradictions: original `0x140256600/710/e80` vs `sim.rs:139–142,517–522` (raw-ish/clamped vector, 0.6 run and radial movement cutoff); `0x14025b270/5e0` vs `sim.rs:544–595,618–619` (per-tick banded reclassification/instant strafe heading); `0x140258180` vs `combat.rs:1420` (unconditional target-facing cast); original 30-Hz path vs `sim.rs:12` 60 Hz.

Read receipts (local, ignored): `engine/shots/gait-read-step3/*.asm.txt`.
The constant values and signed-axis bound were computed locally; no binary,
decompiled function bodies or generated tables are committed. No recreation
behaviour was modified; these findings do not declare its feel corrected.

### Step 3: what it means (cloud reading, not yet confirmed by a live trace)

Arithmetic from findings 1 to 4 for a pad whose stick reaches the unit circle (straight push `r`, or a 45 degree diagonal where each axis is `0.707 r`):

| push | original processed magnitude | original gait | our sim (raw radial, run at 0.6) |
|---|---|---|---|
| straight, r = 0.5 | 0.17 | walk (full walk speed, no scaling) | walks |
| straight, r = 0.64 | 0.40 | walk (holds until below 0.4 after running) | runs |
| straight, r = 0.82 | 0.70 | run starts just above this | runs |
| diagonal, r = 0.8 | 0.39 | walk | runs |
| diagonal, r = 0.98 | 0.70 | run starts just above this | runs |
| diagonal, r = 1.0 | 0.73 | barely runs | runs |

- **The original is stingier than ours for running, especially on diagonals.** A diagonal push has to be near full deflection to run. That is the opposite of "run kicks in too late" in our build, so the cause of that complaint is probably not the gait threshold. Candidates: which input device the comparison used (a keyboard gives a full diagonal, a pad may not), the locked-on pivot timing (finding 8 in step 2 and 7 to 8 here), and our own 0.4 s turn delay.
- **Magnitude is not capped at 1.** A full diagonal on a square gate reaches 1.414. It only matters for the gait compare, since speed is not scaled.
- **Strafe rules in the original:** entry needs an active lock and the body within about 10 degrees of the camera; the direction split is a plain 45/135 degrees with no band; zero stick ends strafe; swinging forward mid-pivot does not cancel the pivot. Main's hysteresis bands are therefore additions, kept deliberately against the original. They can stay while the spasm is open, but they are deviations and should be labelled as such.
- **Aim while strafing depends on the skill and the target**, so "casts use the target yaw" in `combat.rs` is only right for some casts.
- **Timing:** the original samples the stick once per 30 Hz tick. Ours samples at 60 Hz.

**Recommendation:** do not change the thresholds or the input curve from these reads. Applying the exact dead-zone curve would make our run start later, which is the opposite of what Laurence reports. The decisive step is a live trace: the same scripted or real-pad stick pushes logged in the original (plug-in logger) and in ours (`PD_TRACE`), compared with `trace_compare.py`. Also ask which device Laurence compared with.

## Laurence's report (2026-09-30, answers to a direct question)

1. In the original, a **medium-to-hard stick push is variable**: slow to fast, walk to run, graded. Ours is not; "it isn't correct like the original is".
2. Ours lets him do a **fast walk that sometimes doesn't trigger the run animation**. **Mostly locked-on.**
3. "Run kicks in too late" and "turning feels wrong".
4. A **locked-on spasm when walking towards the camera** (stick pulled back towards the camera while locked on).
5. His local docs already describe these issues but are **not pushed to git yet**, so this audit was written against the pushed docs only. Please push them and the cloud session will reconcile.

### What this means for the audit

- Report 1 **contradicts the "walk or run, nothing in between" reading** in `PROGRAM-RE.md` "Ground movement" (which read `FUN_14025ad50` state 1 / `FUN_14024f8f0`). That reading may be right for free-camera movement but the complaint is lock-on, which goes through `FUN_14025b270` / `FUN_14025b5e0`. The first job is to read whether the locked-on path scales speed and/or the clip with stick magnitude. Until that read, `sim.rs` should not be tuned.
- Item A below (dead-zone rescale) would make run start **later**, not earlier, so it cannot explain "run kicks in too late"; it stays on the list as a check but is not the lead.
- The **spasm** has a likely mechanism in our own code (H below), and it is a documented shortcut, not a program fact.

## H. Likely cause of the spasm when pulling back while locked on (ours)

Two things combine, both in our code:

1. `sim.rs` picks BackRight or BackLeft by `on_right = stick_world.dot(right) >= 0.0` every tick. With the stick held roughly straight back, natural wobble flips the sign tick to tick. The hysteresis band (`LOCK_STRAFE_*`) only covers the forward/strafe boundary, not this left/right choice.
2. `main.rs::strafe_sequence` treats any change of (direction, gait) as "replay the pivot": it restarts the TURN_*_BR/BL clip from frame 0 each time. `PROGRAM-RE.md` says the program does **not** restart: `FUN_14025b270` retimes the new clip with `FUN_1402573e0` carrying over relative progress through a length ratio, and our note records "our engine just restarts the new pivot from frame 0, an accepted simplification" (the exact ratio use is marked unconfirmed).

So near dead-back the clip restarts from its first frame every few ticks: that is the spasm, and it is that "accepted simplification" showing. The program's random pick between BR and BL applies only "when the stick is exactly behind", so a stick held just off centre would keep one side in the original.

**Do not implement before the program read below.** Carrying the clip progress over alone would still swap the mirrored BR and BL clips every time the sign flips, and adding a side-stickiness band would be one more invented rule. The original must have something that stops this (for example re-evaluating the direction only when no pivot clip is playing, which matches the pending-transition flag at +0x3a); read `FUN_14025b5e0` for when the direction is (re)chosen. Candidate fix once that is known (render side, no sim change): when the direction or gait changes while a TURN_* clip is playing, start the new clip at the **same fraction of its length** as the old one had reached instead of frame 0, and keep the last left/right side while the stick is within a small angle of dead-back. The carry-over formula is a stated intention in the program read; the side stickiness is a choice to confirm against `FUN_14025b5e0`.

## Where we match what the program read says

| Rule | Program read (`PROGRAM-RE.md`) | Our code |
|---|---|---|
| Run / walk speed | 55.6 / 20.8 u/s, times the speed status | `RUN_SPEED`, `WALK_SPEED` |
| Turn rate | 20 degrees per 30 Hz tick = 600 degrees/s | `TURN_RATE_DEG_PER_S` |
| Speed while turning | times max(0, 1 - angle left / 90 degrees) | `turn_factor` |
| Gait thresholds | run above 0.7, walk below 0.4, nothing in between | `RUN_GAIT_ON`, `STICK_DEAD_ZONE` (but see A) |
| Strafe split | stick more than 45 degrees off the camera/target forward | `LOCK_FORWARD_CONE_COS` (but see D) |
| Pivot is a one-shot TURN_* clip, then the plain run clip, body facing the heading | read 2026-09-27 | `strafe_sequence`, `visual_faces_movement` (render side only) |

## Where our code differs from, or adds to, the program read

**A. No stick dead-zone rescale (a check, not the lead: it would make run start later, not earlier).** The program's input read (`FUN_140256580`/`FUN_140256600`) applies a **dead zone of 13,000/32,767 (about 40 %) and then a linear rescale to 1.0**. Our `Input::stick()` (`sim.rs`) only divides by 127 and caps at 1, and `main.rs` feeds the gamepad axes straight in (Bevy's own small default dead zone only). The 0.7 and 0.4 gait floats were recorded without saying whether the program compares them with the raw or the rescaled magnitude. If it is the rescaled one, the original's run starts at a raw push of about 0.82 (0.4 + 0.7 x 0.6) and returns to walk at about 0.64, so a much wider range of light-to-medium pushes walks in the original than in ours (ours walks only between raw 0.4 and 0.7). The 0.4 coincidence with the dead zone could point either way. **Check:** which value `FUN_14024f8f0` / `FUN_140256e80` compare against `0x1402f7d60` and `0x1402f7d30`.

**B. Strafe heading is not turn-limited in ours.** PROGRAM-RE says a strafe moves "at full walk / run speed (no turn slowdown) along a heading that turns at the character's turn rate". Ours sets velocity to `stick_world * gait_speed` the same tick, so the direction changes instantly, where the program sweeps the heading at 20 degrees per tick (for example, reversing a strafe curves over about 9 ticks in the original). **Check:** `FUN_14025b270` heading update (+0x12e6c): its target angle (stick direction or target direction) and step.

**C. Sim facing versus render facing during a strafe.** After the pivot the original faces its heading (PROGRAM-RE, corrected 2026-09-27). Ours keeps `p.yaw` on the target in the sim (for aiming) and only eases the displayed yaw to the movement direction in `visual_faces_movement`. If the program's aim while strafing uses the heading rather than the target, casts and melee will differ. **Check:** which yaw `FUN_14025a3e0` and the dash code read (+0x12e68 or +0x12e6c).

**D. The plus or minus 5 degree forward/strafe band is ours.** `LOCK_STRAFE_ON_COS` (50 degrees) and `LOCK_STRAFE_OFF_COS` (40 degrees) were added to stop a real-pad flicker (GAME.md "Forward/strafe hysteresis"). The program has a plain 45 degree split. Because the original starts a one-shot pivot and then hands off, it may never show that flicker, which would make the band unnecessary and the real split 45 degrees. **Check:** `FUN_14025b5e0` for any dead band or "already strafing" state (the read mentions a random pick only when exactly behind).

**E. Discrete 30 Hz versus our 60 Hz.** The program's per-tick rules (20 degrees, 1 - angle/90, gait checks) run at 30 Hz. We run 60 Hz and halve the per-tick step, so the speed-while-turning curve is sampled twice as finely. Small (distance over a reversal differs by a few percent) but it is a difference that accumulates over a long fight and affects determinism expectations.

**F. Camera.** Walking is camera-relative in the program (camera yaw +0x12e5c). The free camera behaviour (how fast it swings behind a running player) is the `configdata.dat` float block that PROGRAM-RE marks "which float is which ⚠️", and GAME.md's "swings behind a running player at `FREE_CAM_FOLLOW`" disagrees with the `sim.rs` comment ("running with the left stick never turns [the camera]", and there is no `FREE_CAM_FOLLOW` constant in `sim.rs`). A camera that behaves differently changes how every stick push feels even if the movement rules are exact.

**G. Status and mood variants not built.** Status 35 rotates the stick 90 degrees; wounded (health under 6) uses WAIT_DAM / WALK_DAM / RUN_DAM; fast/slow speed-status clip sets (PROGRAM-RE line "mood set `0x1403faed0`"). Cosmetic, listed for completeness.

## Suggested order

0. **Lock-on gait read** (new, first): in `FUN_14025b270` / `FUN_14025b5e0` / `FUN_14025ad50`, is the movement speed or the walk/run clip chosen or scaled by stick magnitude while locked on? Is there more than one gait tier (for example a blend of walk and run)? This decides whether "walk or run, nothing in between" is wrong.
1. **A** (program read, about an hour with the existing tools): the dead-zone question. `python engine/re/decomp_fn.py` on `FUN_14024f8f0`, `FUN_140256600`, `FUN_140256e80`; look only for what is compared to the two floats.
2. **B, C, D** (one read of `FUN_14025b270` and `FUN_14025b5e0`).
3. Only then change `sim.rs`, with unit tests copied from the verified rules (for example: a half-deflection stick walks; a 180 degree reversal needs 9 program ticks; a strafe reversal curves at 20 degrees per tick). Changing the code before the reads would be another guess.
4. **Footage that settles it without Ghidra** (Laurence's own clips, same method as `FOOTAGE.md`): (a) hold the stick at a steady half push and a steady three-quarter push, from the side, free camera, for 2 seconds each (walk or run at each); (b) locked on, hold right then snap to left, from above or behind; (c) locked on, back-and-forth sideways pushes. Measure the pace (u/s) from the frames.
## Program read results, step 3b

2026-09-30. Exact strafe-state follow-up; reads only, twelve addressed lines:

1. `0x14025b5e0 / 0x14025b703–0x14025b72e`: entry compares wrapped camera yaw `+0x12e5c` minus body facing `+0x12e68`, allowing ±0x38d (≈9.99°); NOT target bearing.
2. `0x140256e80 / 0x14025ad50`: this alignment/strafe-entry handler is dispatched for states 0/21/22/29, NOT ordinary movement state 1; active lock alone does not run the entry test every moving tick.
3. `0x140257510→0x14025b270`: completed TURN switches to queued WALK/RUN and copies heading into facing; on its next update, state 8 sees a non-TURN clip and hands off to state 1.
4. `0x14025ad50`: continuously held off-axis stick then keeps ordinary WALK/RUN toward camera-relative stick heading; it does NOT re-enter the pivot merely because body/camera alignment later returns inside ±0x38d.
5. `0x14025ad50 / 0x14025b2ad–0x14025b324`: release to zero processed magnitude returns movement to state 0; subsequent movement can re-enter through `0x14025b5e0`; other actions can also return to an entry state.
6. `0x140252a20 / 0x1402531d0 / 0x14025ad50`: traced lock toggling/target loss changes lock/camera state but does NOT itself reset moving state 1 or rerun strafe entry; state 8 also has no direct lock-loss exit test.
7. `0x14025b773–0x14025b797`: entry INITIALISES heading `+0x12e6c` directly to camera yaw + current processed stick angle; it does not start from the body's prior facing and ease into that first heading.
8. `0x14025b453–0x14025b4ec`: during TURN, heading pursues live camera-relative stick direction at the character-table turn limit (nominal 20°/tick for playable characters), shortest-way, with no speed-reduction factor.
9. `0x14025b276–0x14025b2a8`: each state-8 update moves along its STORED heading at full selected gait speed × speed-status multiplier, BEFORE updating that heading; full speed applies during the pivot, not only after it.
10. `0x14025b5e0 / 0x14025b8cd`: entry itself sets heading/facing, starts TURN and sets state 8 but contains no translation call; full strafe translation begins on the next state-8 tick, not after clip completion.
11. `0x14025b79d–0x14025b855 / 0x1403faedc`: entry angle θ = wrapped(camera yaw − body facing + stick angle): +45°<θ<+135°→R; −135°<θ<−45°→L; remaining backward sectors→BR/BL (exact −180° random); |θ|≤45°→ordinary movement.
12. `0x1403faedc / 0x14025b50b–0x14025b57c`: normal-mood walking tier uses TURN_RUN_R/L/BR/BL, running tier TURN_WALK_R/L/BR/BL; side chosen at entry only, gait changes within TURN may switch its paired clip with proportional progress preserved.

Evidence: existing local Ghidra export, direct executable disassembly and
normal-mood clip table read by address; receipts in ignored
`engine/shots/gait-read-step3b/`. Findings cover the ordinary active-player
path, not every status/action interruption. No generated data or game code is
included here; no gameplay changes were made.

## Program read results, step 3c

2026-09-30. Reads only. Default player = Hero (`pc00a`), colour 0;
read the actual playable motion file `Player/pc00a/pc00a0.alr`, NOT
the base `pc00a.alr`. These are authored lengths in **30-Hz ticks**, not
60-Hz recreation ticks or measured footage durations. Each clip's 27 tracks
agreed on its length.

| Clip | Motion slot | Length (30-Hz ticks) | Normal-mood gait using it |
|---|---:|---:|---|
| TURN_WALK_R | 17 | 24 | Running, right |
| TURN_WALK_L | 18 | 33 | Running, left |
| TURN_WALK_BR | 19 | 26 | Running, backward-right |
| TURN_WALK_BL | 20 | 35 | Running, backward-left |
| TURN_RUN_R | 21 | 35 | Walking, right |
| TURN_RUN_L | 22 | 49 | Walking, left |
| TURN_RUN_BR | 23 | 28 | Walking, backward-right |
| TURN_RUN_BL | 24 | 42 | Walking, backward-left |

**Pair selection:** walking uses the `TURN_RUN_R/L` pair (or `BR/BL`
for backward entry); running uses `TURN_WALK_R/L` (or `BR/BL`). The names
really are inverted relative to gait. Direct PE read of the normal-mood
table at `0x1403faedc`, indexed by side then gait, confirms step 3b;
`0x14025b50b–0x14025b57c` uses the same pairing during the pivot.

**Does the pivot end exactly at the clip end? No, not at equality.**
`0x1402575ca–0x1402575e0` transitions only when proposed clip time
(`current time + playback speed`) is **strictly greater than** the stored
length; equality keeps TURN. At normal speed 1.0, the length-th clock update
still keeps TURN (its non-looping sample clamps to length minus 0.01), and
the following clock update replaces it with queued WALK/RUN and restores
body facing from heading (`0x140257694–0x140257708`). Thus a fresh,
uninterrupted clip takes length + 1 animation-clock calls, counting the
entry tick's call, to switch clips. Movement dispatch precedes animation
update (`FUN_14024ec80` → `FUN_140256e80` → `FUN_140257510`), so state 8
hands off to state 1 on the **next movement update**, when
`FUN_14025b270` sees a non-TURN clip. This is clip-driven, not a separate
fixed pivot timer: entry's length-minus-one counter is not tested/decremented
by the state-8 dispatcher. Release/actions can interrupt it; status speed
and phase-preserving gait changes alter elapsed ticks.

Evidence: read ALR header slot offsets at `0x20 + 4*slot`, then each
type-5 track's f32 length at chunk +8; `FUN_140256a20` loads that same
field into the program's clip-length table. Base-file cross-check exposed
different lengths for slots 19/22/24 (25/48/40); those are NOT the playable
colour-0 values above. Existing Ghidra export cross-checked with local
executable disassembly; direct PE constant read confirms clamp 0.01 and
default speed 1.0. Local instruction receipts:
`engine/shots/gait-read-step3c/` (ignored). The table above is a bounded
research finding, not an exported asset/dump; no game files, generated
data, decompiled bodies or gameplay code are included. No live trace taken.

## Locked-on lane (PC, 2026-09-30, branch `lane/locked-on`)

**Verdict (Laurence, 2026-09-30, on his pad): "Movement is fixed."** Both
the pull-back spasm and the walk-to-run hang are gone in this build; an
earlier "same issues persist" was played on the old `PLAY.bat` build. Pad
log kept locally (ignored): `engine/shots/locked-on/pad_log.txt`.

Goal: our sim plus animation layer reproduce the original's locked-on
movement state machine (strafe entry, pivot, hand-off to running,
walk-to-run, facing) with no facing jump or clip mismatch. Method: reads of
the existing Ghidra export cross-checked against the executable's
instructions and PE constants; a measurement of our converted clips; unit
tests built from the reads; scripted frame-exact runs of the old and new
builds with `PD_FACE_LOG` / `PD_ANIM_LOG` / `PD_TRACE`. Every earlier
finding was treated as a hypothesis; contradictions are listed below.
Receipts (ignored, never commit): `engine/shots/locked-on/`.

### Program reads (addresses)

1. **Dispatch** `0x140256e80`: the entry handler `0x14025b5e0` is called
   for movement states 0, 21, 22 and 29 only; state 1 goes to `0x14025ad50`,
   state 8 to `0x14025b270`. This call is the only reference to
   `0x14025b5e0` in the export.
2. **States 21/22/29 are not moving states.** 21 and 22 are set only by
   the skill-effect interpreter `0x140270b00` (its effect cases 0xb and
   0xf, which also start a skill-linked pose from the reaction table): they
   are skill-held poses the stick can walk out of. With the stick at zero,
   21 counts 91 ticks then clears the speed-status input channel, 22 does
   nothing, 29 counts its timer down and returns to 0. (No direct setter of
   29 was found; it is entered through a computed state value.)
3. **State 1 never runs the entry test.** `0x14025ad50` has no lock test and
   no call to `0x14025b5e0`: zero processed magnitude sets state 0 without
   moving; otherwise it keeps WALK/RUN by gait (a gait change swaps clips
   with `FUN_1402573e0`, new time = old time / old length x new length,
   blend 0), turns the facing (`0x14024f8f0`) and moves.
4. **What returns a moving player to an entry state:** a zero stick in
   state 1 or 8, or the end of any action -- `0x14025afe0`, `0x14025b080`,
   `0x14025b170`, `0x14025bd40` (via 7), `0x14025be40`, `0x14025c1a0`,
   `0x14025c560` all finish with state 0.
5. **So a strafe "while running" in the original needs one of:** (a) one
   sampled tick with both stick axes inside the square dead zone (about 40%
   per axis, step 3 finding 1 -- a flick through the centre does it,
   rolling the stick round the rim does not); (b) a cast, dodge, hit or
   landing ending with the stick held; plus, in both cases, the body within
   ~10 deg of the camera (running at the foe it is; after a cast it faces
   the aim). Rolling from forward to sideways without passing the dead zone
   is an ordinary run turn. Item 14 rules out a hidden one-tick stick blank.
6. **Idle while locked** (`0x14025b5e0`, zero-stick branch): the facing
   turns toward the CAMERA yaw at the character's turn limit.
7. **Entry tick** (`0x14025b5e0`): no translation in any branch. Heading =
   camera yaw + processed stick angle. Forward sector (within 45 deg of the
   body): WALK/RUN clip, state 1. Otherwise side from plain 45/135 deg
   sectors (exact back: `rand`), pivot clip started with the 5-frame blend,
   state 8, and **facing = heading + a fixed offset** stored at `+0x12e70`:
   -0x2000 for right, +0x2000 for left, 0x4000 for either back side, in the
   program's units (0x8000 = 180 deg) -- a quarter or half turn.
8. **Pivot tick** (`0x14025b270`): move along the stored heading at full
   gait speed x speed status FIRST (also on the tick the stick is released);
   then zero stick = facing <- heading, state 0, idle clip (5-frame blend);
   otherwise the heading chases camera + stick at the turn limit and
   **facing = heading + offset again, every tick**; a gait change swaps to
   the paired clip at proportional time with blend 0.
9. **Clip end** (`FUN_140257510`, `0x140257694`): when the proposed clip
   time passes the length, facing <- heading and the queued WALK/RUN starts
   through `FUN_140257310` with blend argument `0x1402f82e0` = **-1.0, i.e.
   no blend**; `FUN_140257310`/`FUN_140297d80` treat a blend <= 0 as full
   weight at once. Ordinary clip starts pass `0x1402f7f18` = 5.0 (5 frames).
   The clip's playback scale is the speed status (`param_1[0x32]`).
10. **The model is drawn at the facing, unsmoothed**: `FUN_14024ec80` builds
    the model matrix from `+0x12e68` (plus a constant quarter turn for the
    axis convention) after movement and animation, every logic tick.
11. **Turn slowdown uses the angle left AFTER the turn**
    (`0x14024f95f`-`0x14024f9b1`): after `FUN_140193b50` turns the facing,
    factor = max(0, (8192 - |facing - target|) / 8192) (8192.0 at
    `0x1402f8258`, 1/8192 at `0x1402f7c1c`).
12. **Camera frame**: movement, the entry alignment test and the idle turn
    all read the camera yaw `+0x12e5c`, never the opponent's bearing; while
    locked the camera slews toward the target (`0x1402531d0`, per-player
    rate from the camera config block).
13. **Speed-status and wounded clip sets** (`0x14025c980` picks a "mood"
    each tick: speed status up / down, a status that forbids the pivot, and
    health under 6) switch WALK/RUN and, for the wounded set, the pivot
    clips too. Not built here (audit item G).
14. **No hidden one-tick stick blank in battle.** The fighter tick
    (`FUN_14024ec80`, vtable slot +0x20 of the player class at `0x14048e3b0`)
    is called only from `FUN_14012bbb0` (`0x14012bd69`); in a live fight its
    input mask is the global at `0x1404bca00`, set to all-on at battle setup
    (`0x1401286d5`) and pause resume, cleared only by pause, event/dialogue
    code and two script opcodes (`0x1402116c0`, `0x140211c60`). Each of the
    up-to-3 catch-up ticks per frame is a full tick with a fresh pad poll
    (`FUN_14028e710`). The per-player flag that forces the movement stick to
    zero (`0x1404bbd80`, read at `0x140256eb7`) is written only by init loops
    and .ssb script commands (`0x1402125a0`, `0x140211b90`, `0x140212580`,
    `0x140211b70`). `0x140533744 == 3` means no menu is open. So a moving
    player re-enters idle only through the dead zone or an action (read 5).
    (Helper read; notes in ignored `engine/shots/locked-on/tickmask/`.)

### Our converted clips (measured, not from the program)

A helper script measured, for all 11 playable characters, the model-space
yaw of the hip (the skeleton's body root) at the last frame of each TURN_*
clip, added the program's fixed offset, and compared it with the first
frame of the clip that takes over (TURN_WALK_* -> RUN, TURN_RUN_* -> WALK).
**Worst difference 7.4 deg (pc01a TURN_WALK_L); most clips within 2 deg;
pc00a within 4.5 deg.** The pivot clips are authored so that end pose +
offset = the loop pose facing the heading: the program's unblended switch
is seamless by construction. The clips' FIRST frames differ from WAIT0 by
up to 75 deg, which the entry's 5-frame blend covers (in the program too).
Script and full per-clip results: `engine/shots/locked-on/pivot_pose_check.*`.

### Contradictions with earlier findings

- **"Strafe entry allowed while moving (aligned)"** (DECISIONS 2026-09-30,
  commit 7270b51): contradicted by reads 1-4. Reverted; the scripted runs
  show why it mattered little in practice -- rolling the stick turns the
  body off the camera before 45 deg is reached, so the alignment test
  failed anyway; only a one-tick jump from forward to sideways differed.
- **"Our converted clips' baked curves don't match a clean 90/180 deg
  constant"** (DECISIONS 2026-09-28, the reason for `pivot_model_yaw`):
  contradicted -- they match once the loop clip's own hip yaw (RUN about
  +20 deg, WALK about -3 deg at frame 0) is counted.
- **"5.0 as the blend length on every clip start"** (`main.rs`
  `CLIP_BLEND_SECONDS` comment, PROGRAM-RE "Animation clock"): not every
  start -- the pivot's queued switch is -1 (none), gait retimes are 0.
- **Pivot end timing** on the branch was `2 * (length + 1)` sim ticks after
  entry; the clip switches `2 * length` sim ticks after the entry tick
  (step 3c's length + 1 clock calls count the entry tick's). Body facing
  flips on that same tick.
- **Speed-while-turning** used the angle before the turn step (sim.rs);
  the program uses the angle after (read 11).
- **Locked movement used the opponent's bearing**; the program uses the
  camera yaw (read 12).
- Step 3b findings 2 and 4 **stand**; step 3 finding 7 (lock loss does not
  end a pivot) **stands**.

### Answers to the brief's questions

- **What starts a strafe while moving:** reads 1-5 and 14.
- **Transition table:** 0 --stick--> entry (1 or 8, no movement that
  tick); 1 --zero stick--> 0; 1 never --> 8; 8 --zero stick--> 0 (facing <-
  heading, moved once more); 8 --clip end--> facing <- heading, WALK/RUN,
  then 1 on the next update; any action --end--> 0; 21/22 (skill poses) and
  29 (timed wait) behave as 0 for entry.
- **The facing offset that "cancels" the pivot's baked turn:** it does not
  cancel it. The body is held at heading + (+/-90 or 180 deg) and the
  clip's own turn carries the rendered body round to the heading; at the end
  the facing jumps to the heading while the clip switches instantly to the
  loop, and the two cancel (measured above).
- **Hand-off facing vs `main.rs`:** main/branch cancelled the clip's turn
  (`pivot_model_yaw`), added a 0.4 s hold and a 300 deg/s visual turn, then
  faded a correction over the 5-frame clip blend at the hand-off
  (`handoff_baked_yaw`, `strafe_return_step`). None of that exists in the
  program; all removed.
- **Pull-back spasm (strafe to run):** the old renderer cross-faded
  TURN_*_B* -> RUN over 5 frames. At that switch the hip's own rotation
  differs by about 180 deg between the two poses (about -160 vs +20 deg),
  and a pose blend between rotations half a turn apart has no stable path,
  so the body flipped (old build, frames 294 -> 296 -> 300: faces the
  camera, whips to profile, faces the camera again). The program switches
  that clip instantly. Side pivots differ by about 90 deg, which blends
  without flipping -- why only the pull-back spasmed.
- **Light-then-full push plays the walk pivot at run pace:** on main,
  `strafe_sequence` kept the entry gait's TURN clip playing after the sim
  switched to run speed (the cloud branch patched it on the renderer's own
  clock, with a 5-frame blend, ending two ticks before the sim did). The program swaps to the run pair at the same
  relative progress, instantly. Now the sim owns the pivot clock and the
  renderer plays exactly that clip at exactly that time.
- **Found in addition:** entry ticks never move (also from idle into an
  ordinary walk); WALK <-> RUN swaps are instant and keep the foot phase;
  the turn slowdown uses the angle left after the turn; releasing mid-pivot
  moves once more then snaps the body to the heading (the program's own
  snap); the pivot's clip clock follows the speed status; locked movement
  is camera-relative.

### What changed on `lane/locked-on`

- `sim.rs`: the program's state machine (entry only after idle or an
  action; camera-yaw frame; body = heading + side offset during a pivot;
  pivot clock in 30 Hz frames with strictly-greater completion; gait swap at
  proportional time; release moves once more; after-turn slowdown; entry
  tick moves nothing; instant facing changes are not blended between ticks).
  `LOCK_FORWARD_CONE_COS` removed. `Pivot`, `pivot_offset`, `Player::pivot`
  exposed for the renderer. 20 strafe tests rebuilt from the reads.
- `main.rs`: the model faces the sim facing exactly in movement states
  (casts/hits keep their ease); the pivot clip plays at the sim's clock; the
  pivot's queued switch and gait swaps are instant, WALK <-> RUN keeps phase,
  everything else blends 5 frames (`clip_switch`). Removed
  `strafe_sequence`, `PivotFacing`, `pivot_model_yaw`, `handoff_baked_yaw`,
  `strafe_return_step`, `visual_faces_movement`, the 0.4 s delay and the
  300 deg/s visual rate. `PD_FACE_LOG` prints heading too; `PD_ANIM_LOG`
  prints how each clip switched.
- `combat.rs`: one test now allows for the entry tick after a capsule lock.
- Tests: 349 pass (`cargo test -p pd_game`).

### Scripted runs (old branch build vs lane build, frame-exact)

Scripts `engine/shots/locked-on/make_scripts.py`, runner `run_all.ps1`,
summary `analyze_face.py`, sheets `A_strip.png` (sidestep), `B_strip.png`
(pull-back), `C_strip.png` (light then full).

- Pull-back held: old build flips the body at the hand-off (above); lane:
  largest drawn-facing step 9.4 deg/frame, which is the pivot clip's own
  turning speed mid-clip, nothing at the switch.
- Sidestep from rest: hand-off step about 3 deg (pc00a residual).
- Light then full: lane swaps TURN_RUN_R -> TURN_WALK_R instantly on the
  tick the push reaches full and hands off at the retimed end.
- Rolling the stick mid-run: no pivot in either build. Flicking through the
  centre mid-run: pivot in both.
- Walk to run: lane swaps WALK -> RUN instantly at the same foot phase.
- `trace_compare.py` runs on these real traces (old vs new pull-back: 9 of
  9 metrics pass -- the sim's path barely changed; the spasm was drawing).
  Note for the golden trace: the original's facing also jumps by 90 or 180
  deg on the tick a pivot ends, so `max_turn_rate` will read that, not 20.

### Not determined

- Whether Laurence's "the original starts a strafe while running" is the
  dead-zone flick / action-end path (read 5); read 14 rules out a hidden
  stick blank, but no logger in the original, so no live trace. The shipped
  .ssb scripts were not scanned for the input-lock opcodes (read 14).
- Facing when a pivot is cut by a jump or dodge (we face the heading, as a
  cast from a pivot does -- unconfirmed for jumps).
- Our sim reads the stick at 60 Hz, the program at 30 Hz, so ours catches
  slightly shorter flicks through the dead zone.
- The lock camera's slew rate (6 deg/tick is inferred, not read).
- Speed-status and wounded clip sets; the program's cast facing turn
  (`0x14025bd40`, ours snaps in the sim and eases on screen).
- What the original draws between its 30 Hz ticks.
