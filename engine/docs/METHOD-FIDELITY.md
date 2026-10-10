# How locked-on movement was cracked -- and how to reuse the method

2026-09-30. Locked-on movement (strafe, pivot, hand-off, walk-to-run,
facing) had been "close but off" through about a dozen rounds of fixes.
It was approved by Laurence on his pad after one pass using the method
below. Evidence for movement itself: `MOVEMENT-AUDIT.md` "Locked-on lane".

## What went wrong before (the anti-pattern)

1. **Tuning the symptom on screen.** Each report ("spasm", "turn too
   late", "snaps") got a render-side patch: cancel the clip's baked turn,
   add a 0.4 s delay, a 300 deg/s visual turn, a fade at the hand-off, a
   paced return. Each patch was tuned by eye and created the next symptom.
   None of them existed in the original.
2. **A plausible-but-unchecked premise.** "Our clips' baked turn doesn't
   match a clean 90/180 deg offset" was measured without counting the
   next clip's own pose, so the program's simple rule was thrown away.
3. **Relaxing a read to match a report.** "Strafe can start while moving"
   was added because play seemed to contradict the read, without reading
   what makes it happen in the program.
4. **Judging the wrong build.** The first "same issues persist" verdict
   was played on the old `PLAY.bat` exe; the lane build had never run
   (no log file existed). Always confirm which exe was played.

## The method (in order)

1. **Read the whole state machine, not one function.** Start at the
   dispatcher (which handler runs in which state), then every handler,
   every writer of the state field, and every caller of the function in
   question. List the transitions explicitly. Most "mysteries" are a
   transition you haven't read (here: state 1 never calls the entry test;
   only a zero-stick tick or an action ending returns to idle).
2. **Read how the animation layer is driven, including blend arguments.**
   The decisive fact was a single constant: the clip start at the pivot's
   end passes blend -1.0 (none) where ordinary starts pass 5.0. Check the
   blend/start-time arguments of every clip start involved
   (`FUN_140257310` 4th arg = blend frames, `FUN_1402573e0` = seek + blend)
   before touching any visual layer.
3. **Read what the renderer draws from.** The model matrix is built from
   the stored facing every tick, unsmoothed. If ours smooths or pursues,
   that is a difference, not a feature.
4. **Read constants from the PE, and disassemble where Ghidra is vague.**
   The turn-slowdown uses the angle left AFTER the turn -- only visible in
   the instructions after the call (Ghidra typed the helper as void).
5. **Measure our data against the program's rule.** A helper script
   checked every playable character's clip poses against the program's
   fixed offsets (end pose + offset = next clip's first pose, within
   7.5 deg). That turned "the offset is a hack for their clips" into "the
   clips are authored for the offset" -- so the faithful rule works for
   every fighter at once.
6. **Rebuild the rule exactly; delete compensations.** Sim follows the
   read state machine; the renderer shows the sim facing and plays clips as
   authored. Removing ~700 lines of tuned compensation fixed more than any
   of it did.
7. **Tests from the reads, not from the old behaviour.** Each test cites an
   address and asserts the program's timing/sector/offset (e.g. pivot ends
   `2 x length` sim ticks after entry; body = heading + offset every tick).
8. **Scripted, frame-exact before/after runs.** Same stick script on the
   old and new builds (`PD_TRACE_SCRIPT`, `PD_SHOT_TICKS`), with
   `PD_FACE_LOG` / `PD_ANIM_LOG`, a small log analyser, and cropped
   before/after strips of Player 1's own view. Look at the frames yourself.
9. **Laurence plays the lane build with logging on** (`PLAY_LOCKED_ON.bat`
   pattern: separate exe, logs to `engine/shots/...`). Check the log exists
   before accepting any verdict.
10. **Keep receipts out of git; write findings in words with addresses.**

## Cost notes

One orchestrator (Opus) did the reads and the code; two Sonnet helpers did
bounded, self-contained jobs in parallel (the per-character clip-pose
measurement; one narrow tick-mask read). Separate worktree and build folder
(`~/.pdtools/target-<lane>`), plus an `engine/cache` junction so tests find
the cache.

## Where to apply it next

- **Move effects / animation (deep pass P3):** check every cast clip
  start's blend and start-frame arguments and what the effect attaches to,
  before any timing tweak. Our renderer blends every clip change 5 frames;
  the program does not always.
- **Cast facing:** the program turns toward the aim at a rate
  (`0x14025bd40`); ours snaps in the sim and eases on screen -- same
  anti-pattern, same fix route.
- **Hit reactions, knock-downs, get-ups, landings, dodges:** read their
  state handlers and exits the same way (states 2-7, 9-20 of
  `0x140256e80`); check what facing and clip each sets and whether blended.
- **Speed-status and wounded clip sets** (`0x14025c980`): read, then build.
- **Camera:** the lock camera's slew rate and free-camera config floats --
  read the writer of `+0x12e5c`, not tuned by eye.
- **Sound:** same discipline -- read the call site and its arguments before
  choosing ids by ear, and confirm which build Laurence heard.
- **Any "close but off" report:** stop patching, run steps 1-9.
