# Handoff

> **Archivist, 2026-10-03:** the "current" block below is end of wave 6 (2026-10-01). Newer state: `NEXT-SESSION.md` top, `WAVES.md`, `handoffs/WAVE-1003-STATE.md` (main checkout, untracked). Hermes is no longer used (AGENTS.md).


The latest handoff between lanes goes at the top. Keep old ones below a divider for history.

## Wave 6 merged + live recording complete + folder tidy — 2026-10-01 night (current)

**Merged on main**, each lane screenshot-checked first:
- loaders, decks, instants;
- moverec (video + full pass), moveskills;
- hudbits, reactions2, capfx, aiaudit, probes;
- guardfix, fxopen, fxfollow;
- ssbvm, cpureads, cpuhost, cpuseat;
- livefix, auralook (its glow is now off by default; the live trace showed the original doesn't draw it in versus);
- combos, stagefix.

Main cache regenerated: `fx` (bump maps, wisp skins), `skills.json` (`action.chain`), stages (`--collision dat+walls`), `com/`. Tests: 910 game / 88 formats / 9 convert / 39 viewer + 259 Python.

**Live recording:**
- The original PC game is fully recorded: 341 takes in `.pdtools/moverec/takes_full`, 334 ok and 7 refused, all of them expected.
- The live audit shows 0 crosses.
- Laurence asked directly for every skill unlocked in his save; done with `mr_unlock_all.py` (the game's own save `FUN_1401cff20`).

**Tidy:**
- `PLAY.bat` is one menu; its helpers are in `tools/bats/`.
- Research notes are in `docs/research/`.
- The 56 finished lane worktrees were parked in `_archive/old-lanes/` (deleting them was refused as irreversible; Laurence may delete the folder).
- An unmerged early aura experiment is saved as `.pdtools/archive/lane-h3-uncommitted.patch`.
- The repo moved to `GODOT\Phantom Dust Rewrite`.

## Waves 1-5 merged (autonomous, Laurence asleep) — 2026-10-01 (current)

Claude (Opus) orchestrated ~20 Sonnet lanes from the P0 audits
(`audits/P0-*.md`); state and open items per lane in **`WAVES.md`**; every
default taken for Laurence is a DECISIONS row (2026-10-01). Merged on main:
aura (effect-600 wisps), arsenal builder + rules, animation start args,
character light (SWBOS), four-player sim/views/HUD/setup, cast state + rule
sets, collision (0x94 ramps, rails kept), effects blend/billboard + timeline +
per-shader pixel stage, shadows + stage SBTS light (light vectors were read
backwards before), glow (original has none; optional, off), reactions,
guards/erase/status/environment, projectile families, shot markers, events
(sounds/effects), live tracer for the Store original. Tests 723 + 57 + 8 + 25.
Caches regenerated on main (chars, stages `--collision dat+walls`, fx,
skills, shotpoints); backups in `~/.pdtools/*-backup-20261001`.

**For Laurence:** `engine/shots/REVIEW-2026-10-01.html` (review sheet) +
`engine/shots/ear_check.html` + `tools/bats/PLAY_ORIGINAL_TRACED.bat`. Critic report:
`audits/CRITIC-2026-10-01.md`.

## Xbox ISO step 1 done — 2026-09-30

- Disc archive verified and unpacked to `%USERPROFILE%\.pdtools\xbox\`
  (outside the repo). It is an **xiso** (game partition only), 4.35 GB,
  2,062 files incl. `default.xbe`; details in `handoffs/H6-xbox-iso.md`
  "Step 1 result". No download was needed; `extract-xiso` is unnecessary.
- Next: H6 step 2 (extract with our own reader) -> step 3 inventory diff vs
  the 2017 PC files. Waiting on Laurence's go.

## Locked-on movement FIXED and merged; Xbox ISO next — 2026-09-30

- **Movement: approved by Laurence on his pad ("Movement is fixed")** after
  playing the lane build (his earlier "same issues persist" was on the old
  `PLAY.bat` exe). `lane/locked-on` merged into main (with the cloud branch
  `claude/trusting-cori-if07av` it was built on: trace tools, GOLDEN-TRACE,
  audits, xbe/inventory/strings tools, TODO-LAURENCE). Evidence:
  MOVEMENT-AUDIT.md "Locked-on lane". Protect it: do not re-add render-side
  facing compensations; the model faces the sim facing and clips play as
  authored (unblended pivot->run switch, phase-kept WALK<->RUN).
- The bullets below were written before his verdict (history).

### Earlier the same evening


- **Movement lane `lane/locked-on` (pushed, NOT merged):** strafe/pivot/
  hand-off/walk-to-run/facing rebuilt exactly from PC program reads
  (branch `engine/docs/MOVEMENT-AUDIT.md` "Locked-on lane", 349 tests).
  **Laurence on his pad: "the same issues persist."** Parked by his choice.
  Next movement attempt needs live evidence (his side-by-side video, pad
  log, or an xemu trace) before any more code; no reading-only pass.
- **Next: Xbox disc analysis** -- full handoff in
  `engine/docs/handoffs/H6-xbox-iso.md` (image in Downloads, tools on the
  cloud branch, steps, what needs Laurence's yes).
- Unrelated uncommitted edits sit in the main working tree
  (`pd_game/src/sfx.rs`, `docs/DECISIONS.md`) -- not from this session;
  leave them to their owner.

## Recreation evaluation and next heavy Claude planning — 2026-09-30

Laurence: steps good; capsule use/collection sound not correct; locked strafing
and walk→run sensitivity close but off; lighting needs its own pass; school aura
needs a modern ground-up rebuild; four-player split-screen UI and original-like
arsenal builder need work; gamemodes need their own pass. He also explicitly
added **move effects/animations are not perfect**. These are open fidelity
milestones, not mere polish on a finished recreation.

Updated ROADMAP/PLAN/FRONTIER. **CLAUDE-DEEP-PASSES.md** specifies P0 baseline
and P1–P8 dedicated passes, ownership/dependencies, reference cases, acceptance
gates, strong Claude/high effort orchestration and independent critique.
**No new agents or premium execution launched for this request.** Read that
brief before starting the next wave; ask only genuinely unsettled decisions.

Preserve **approved complete grain dissolve** and exact regular-step common35.
Latest actual merged validation is 345 + 42 + 7 + 10 with build/runtime smoke
passed; do not treat counts/dispatch coverage as fidelity. Assets/tables mapped
broadly, behaviours and presentation only partly verified. No overall percent.

## Sign-off — 2026-09-29 (current)

**Wall dissolve COMPLETE — Laurence explicitly approved:** "mark dissolve as
complete. Update necessary docs, sign off." Grain-discard implementation and
full recovery replace the rejected Blend fade; defaults 9→4 units, coverage
floor 0.15, 1px grain are accepted. No further dissolve approval is pending.
Merged code: `6694aa0` + `f293227`; latest validation **345 + 42 + 7 + 10**
passed, build and runtime shader smoke succeeded. `PLAY.bat` is current.

Regular steps use the exact identified **27_id035/common35**. Capsule
view-isolation remediation is merged (H5); no separate user sign-off is
inferred for that item. School aura and moves remain parked. Historical
provisional/rejected states below are superseded by this sign-off.

## Reopened by Laurence's live play — 2026-09-29 (history)

The earlier "verified" visual claims below did not survive live play. Active
remediation is limited to these three reports; school aura and moves remain parked.

- Regular footsteps must play **`audition/27_id035.wav`** (common id **35**).
  SHA256 matches the cache's `common/35.wav`. Do not override his ear label
  with the historical packed-emitter id 30.
- Wall dematerialisation hides model pieces and fails to recover normally.
  The desired effect is **grainy and smooth**, not ordinary alpha blending.
  Reproduce entry AND exit through a real wall-camera pull-in. The old
  smoothing deadband is 0.01, but restoration requires alpha >= 0.9999:
  those conditions are incompatible, so recovery can remain stuck forever.
- Own/opponent capsule appearances are **reversed** in live play. Investigate
  rendered owner/view/model mapping and cache output before swapping anything;
  capture both players' views and preserve capture ownership.

No claim of completion until the reported lifecycle is tested visually.

**Capsule remediation merged:** `1132134` + `ecdce45` (H5). Both models were
superimposed in both views because only the orb root carried RenderLayers;
draw children defaulted to shared layer 0. Each actual mesh now has its
viewer's layer. Mapping/capture ownership unchanged. The regression test
drives real spawn_orb, checks every mesh descendant for both viewers, and
fails on the old root-only implementation. Orchestrator inspected the
before/after sheet and independently ran the ECS test. Main rebuild underway;
still needs Laurence's visual approval. Wall dissolve was a separate active lane
at this checkpoint (now complete; see sign-off above).

**Wall dissolve merged and approved:** `6694aa0` + `f293227`. Replaces ordinary
Blend alpha with an ExtendedMaterial PBR fragment shader using per-pixel grain
discard while retaining authored Opaque/Mask passes. Per-view coverage selects
by split viewport origin (both layouts; hypothetical third fullscreen view is
not covered). Rest coverage snaps to exactly 1.0; original handles restore on
disable; re-enable and fresh match roots are tested. Review caught two material
components coexisting on a mesh and a spent-Added-trigger re-enable bug; both
fixed with a failed-before production ECS lifecycle test. Orchestrator ran all
9 cam_fade tests and inspected native grain crops + two-cycle wall sheet.
**Laurence approved completion:** 9/4 distance window, 0.15 floor and 1px grain
are accepted. No shader or
simulation changes to the parked school aura or moves.

Merged validation completed: **345 game + 42 formats + 7 converter + 10 viewer
tests passed**, build succeeded. Fresh main-binary screenshot run exited 0,
shader compiled without a shader error and both fighters rendered normally at
rest (`shots/dissolve_merged_smoke_review.png`, inspected). `PLAY.bat` refreshed.
Laurence subsequently approved the grain look and marked dissolve complete.

## Handoff from the orchestration chat: 2026-09-29 evening (Laurence's original-game discoveries → lanes) — START HERE

**Scope:** Laurence reported what he saw and heard in the **installed original** (not our build): a wet-step sound tied to map patches, id 1 = unpausing, aura consumption flashing the school aura, movement halting during consume, the model dematerialising near the camera, lock-on playing a sound and spinning camera+player to the opponent. All recorded in `FRONTIER.md` as threads O1–O5. **School aura work is skipped** (Laurence: it needs a full overhaul first), so O1 is parked with it.

**State:** `main` green at **324 + 36 + 7 + 10** (the orchestrator ran the merged tree, not just the lanes). `PLAY.bat`'s debug build is current. All lanes below were verified by the orchestrator (tests re-run in the lane worktree AND on the merged tree, diffs read, screenshots looked at, decompile claims spot-checked with grep/`git show`).

**Merged this session:**
- **lane-unpause** (`b6d6886`): `Cue::Unpause` = common id 1; the orchestrator added the `menu.rs` hook — closing the in-match overlay (Esc/Start toggle or pad East) plays id 1, everything else keeps Back (154). **Needs one live unpause by Laurence's ear.**
- **lane-step** (`9f3b1fe`, docs): every footstep-region emitter call passes packed `0x1e0c350` → **id 30** (there is no third constant; the `0x23xxxxx`/id-35 hypothesis is dead — byte-level there are two machine sites sharing one tail). The real per-surface variation is `FUN_140276330` fed by the material table `DAT_1402f6270` (water class `0xD` → **52/54**, sand `{3,9,0xC}` → 28/30…). **id 35 = the `+1` variant of 34.** The duplicate `cache/sfx/common` pairs are **legitimate shared clips** (verified by probe: same WAVE clip index), not a converter bug. Capsule sounds **are** played — through `FUN_140276330` (`0x55` = 85, `0x5C` = 92), so the "capsules are silent / another bank" verdict was wrong.
- **lane-wetzone** (`13712a2`, docs): the surface class byte the footstep dispatcher reads is the **flags byte of each hull-face chunk in `Map/stNN.dat`'s header-`0x94` block** — a field no parser ever read (`engine/re/wetzone_probe.py`, 0 mismatches over the 22 ST00 stages). **st03 (Lost Palace) has 32 water-class faces** in puddle-sized patches at floor level. Per-surface footsteps **and** VFX are reachable from shipped data; the old "unreachable" verdict is overturned. The dispatcher switch was confirmed in the raw decompile: sand `{3,9,0xC}` / dust `{5,6,8,0xB,0x15}` / water `{0xD}`, 0x14/0x1F **silent** (the old "gated on 0x14/0x1f" framing was wrong).
- **lane-fade** (`c81392f`): O3 camera-proximity model fade — new `cam_fade.rs`, clones each fighter's own `StandardMaterial` pieces once with `alpha_mode: Blend` and eases alpha over eye-to-head distance (9.0→4.0, floor 0.15, hysteresis). 2 `main.rs` lines. `PD_CAMFADE=0` disables; `PD_FADE_START/END/MIN`. A/B screenshots looked at (models ghost, coat tails fade with the body, distant opponent stays solid; a fade-off control run is pixel-identical to fade-on at normal boom length, so it does not misfire).
- **lane-consume** (`c9f71c3`): O2 consume lock — `CONSUME_LOCK_TICKS = 30` (0.5 s at the 60 Hz sim) roots the player on capturing an **Aura** or **green Status** capsule (movement, jump, dodge, cast, further captures all blocked); `hurt()`/`stun()` clear it; attack/defence/erase/special/environment captures do not lock. Step-1 measurement proved the gap first (a player kept running 26.9 u during a capture).

**Waiting on Laurence (cheap, unblocks features):**
1. **Ear-check Q3/Q4** in `engine/audition/wetcheck/LISTEN.txt`: **Q3** = is `common/4` the lock-on sound? (already wired ON-only; the id was identified twice — 0.84 correlation with his own `Lock-On Animation.mp4` + the program's HUD lock-marker path). **Q4** = is `common/92` the capsule pickup? (already wired, `PD_CAPSULE_PICKUP` override). Wet footsteps (52/54) and the orb-appear (85) are done.
2. One **live unpause** to hear id 1 (the overlay close can't be driven headlessly).
3. **A live wet step**: walk onto the Lost Palace puddles (st03). The demo's own pathing never crosses them (measured: every logged step is class 32 → dry 30), so the wet path is unit-tested but not yet heard in play. `PD_SURFACE_LOG=1` prints the class/id per step if he wants a log while playing.
4. His eye on the **camera fade** window (9/4/0.15) once he plays.

**Open, low:** `main.rs::matte_characters` misses never-cloned character materials, so `PD_CAMFADE=0`-from-launch renders glossy (debug path only; the default path is correct); the consume lock blocks remote-mine detonation unlike the frozen rule (unverified against the original); unlock silence for the lock-on cue (id 4 shipped ON-only); jump-zone hum (common/79, a looping ambient) unwired.**In flight (wave 2, free models, `muse-spark-1.3-contributor-free`):** `lane-lockon` (O4 — find the lock-on sound in the program, smooth the camera/player spin) and `lane-surface` (export the `0x94` class per face through `pd_formats`/`pd_convert` into a new sidecar so a later lane can wire per-surface footsteps). A **sceptical verification gate** agent audits wave 1's claims. Next after those: wire the wet footstep (sound + VFX) and the capsule 92/85 cue once the ear-check lands.

**Waves 2 + 3 — all merged (orchestrator verified each; 342 + 42 + 7 + 10 green):**
- **lane-lockon** (`a6cc056`): measured that the sim's `cam_yaw` **snapped** ~100° in one tick; locked `cam_yaw` now **slews at 180°/s** (the program's 6°/tick; `PD_LOCK_CAM_RATE`). The lock-on cue is **common id 4**, identified twice — FFT cross-correlation of Laurence's `Lock-On Animation.mp4` (**0.84**) and the program's HUD lock-marker updater (`FUN_1401536e0` → `FUN_140276330(0,4,…)` 10 frames later). ON-only. New test switches `PD_TEST_LOCK`, `PD_TEST_CAM`. **Needs Laurence's ear (Q3).**
- **lane-surface** (`cfc8784`): `0x94` hull flags exported as `cache/<stage>.surf.json` (`surface-v1`), parity-checked against `wetzone_probe.py` (21/21 stages, 0 fails, positions ≤1.5 mm). New `pd_formats/src/surface.rs` (`class_kind`, `class_below`). `pd_game` untouched.
- **Gate (wave 1)** found one real defect: the fade pinned every character material to `Blend` forever (no depth write → coat tails could mis-sort at full opacity). **lane-fadefix** (`268c611`) fixed it — materials keep their authored mode at rest (`MASK` on 18/27 pieces, so "restore Opaque" would have been wrong), switch to `Blend` only while fading; measured A/B (old vs fixed = mean-abs 2.31 on the fighters, concentrated on them). The gate's other notes were documentation-hygiene (one unreproducible integer, corrected; the wet-patch probe now derives orientation itself). It also flagged a **low, debug-only** issue: `matte_characters` misses never-cloned materials, so `PD_CAMFADE=0`-from-launch looks glossy.
- **lane-sounds** (`2a012ba`): wet footsteps — new `surface_ground.rs` loads the sidecar and answers `class_below`; `footsteps.rs` cycles **52/54** on water per player (Laurence's DECISIONS call), dry 30 unchanged, `PD_SURFACE_LOG=1`. Capsules — **92** on the pickup (was a deliberate no-op; `PD_CAPSULE_PICKUP`) and **85** on a slot appearing (new `capsule_sfx`; `PD_CAPSULE_APPEAR`); live log shows `sfx: capsule appear 85 (6 new orbs)`. Its committed tree needed one `main.rs` line (`mod surface_ground;`) — applied by the orchestrator.
- **lane-consumeanim** (`c3bf9cd`): `CONSUME_LOCK_TICKS` 30 → **79** (SPE_POWER_UP/SPE_HEAL measure 40 frames @30 fps) and the character now plays **SPE_POWER_UP** (aura) / **SPE_HEAL** (green status) for the whole lock through the existing clip path. **Live proof:** `--demo --start-hand 0` *does* capture orbs (correcting an earlier belief); anim log shows the clips running exactly 79 ticks (aura t361→440; heal t241→320, t366→445); frames looked at.

**Process note that paid off:** reproduce-first killed the "third emitter call = id 35" hypothesis on a direct byte read **before** anything was built on it — and the same read then found the real mechanism. Keep measuring before building.

---

## Handoff from Claude Code: where to continue (2026-09-28, visual cycle: HUD dial, orbs, VFX audit + fixes, school aura) — START HERE

**Scope:** Laurence chose "visuals first" for this cycle; the moves track (M5, M6, M11–M14) is paused until he says otherwise.

**Done and merged on `main` (tests 7 + 36 + 276 green):**
- HUD move wheel: ring size measured by overlay on Laurence's frame; knobs on the ring art's own sockets; empty slots show the bare socket; rim centred on its fill; aura count size/position; capsule colours from the program's table (erase purple, status green, environment aqua).
- Capsule orbs: real st00 models, decoded draw recipe (core sphere + camera-facing cup, wrap sampling, scroll speeds), per-kind shaders, Status/Environment 4-layer blend, Aura halo fix. Parked at ring variant A with Laurence's notes (CAPSULE-VISUALS.md "Parked").
- Effects: skinned/palette pieces exported (0x20); dark-blob / milky double-sided blend bug fixed; LIGHT02 drawn as floor light decals; charge size envelope (`CAST_REACH`) + Slide Laser cast family; FLAIR01 random-size words 24/25 (52 effects had invisible rays; knife burst was oversized). Effect units are 1:1 with world units (FORMATS-EFFECTS.md "Effect units").
- Lock-on marker: animated reticle (Y icon, arcs, ring, crescent), range colours orange/yellow/green, thick strokes (Hermes lanes H1/H1b).
- School aura: skinned shell glow per fighter, dominant-school colour, Level ≥ 1 (VFX-AUDIT item 6, GAME.md).

**Open (small jobs handed to Hermes in `engine/docs/handoffs/H3-polish-batch.md`):** school aura polish; knife streaks thin + ~4 ticks late; Slide Laser crescent timing; lock-on scythe crescent; orb notes (uneven glow, redder attack, deeper-blue Defence, per-kind ring).

**Process lessons:** measure against Laurence's frames/footage with overlays (nudging by eye failed repeatedly); check agents' sheets yourself (several "done" reports weren't); lanes must use their own worktree — Hermes twice switched the shared checkout's branch, and the orchestrator once ran `git reset --hard` in the shared checkout by mistake. Never do either while a lane works there.

---

## Handoff from Claude Code: where to continue (2026-09-27 night, after the collision / moves / program-reading lanes)

Read AGENTS.md → docs/INDEX.md → this entry → docs/PLAN.md ("Moves track"). Everything is merged and committed on `main` (local, not pushed). Tests: `cargo test -p pd_game -p pd_formats -p pd_convert` → **268 + 36 + 7 green** (end of session).

**Done this session:**
- **Lost Palace barrier** (merged): every stage's invisible shadow-volume mesh (vertex format 0x07/0x10 + material flag 0x2000) was solid collision; now left out. Fixes the Palace spawn alcove (all 3 variants) and Lane (st07/st17). Dawn City roof gaps are now open (jump pads cross them). New `pd_convert walkprobe` (walks a body out of every start point, maps the reachable area; `--map`, `--compare`). COLLISION.md, FORMATS-STAGES.md "Stage shadow volumes". Proof: `shots/col_*_sheet.png`, `shots/orch_col_check_sheet.png`.
- **Move visuals** (merged): melee swing (slot 0) now plays at the strike, strike clip seeked + held; first-swing preload; emitter-only flight effects drawn (Moths, Hell's Fire); stand-in when a flight slot draws nothing; `PDEFTOPT06`/`YKREFRECT00` are reflective solids, not distortion (Muramasa/Rail Gun blades, mine shells back); skills 301–340 borrow a matching skill's effect; sweep test `every_attack_shows_something_in_flight_or_swing`; `PD_FX_LOG=1`. The emitter textures in `cache/fx/eftNNNN_tN.png` were written by a one-off tool — **a fresh convert loses them until PLAN M1 lands**.
- **Program reading** (Laurence's choice — his original is installed as Appx `Microsoft.MSEsper`): Ghidra 12.1.4 + JDK 21 in `%USERPROFILE%\.pdtools\`, project opened via the junction `%USERPROFILE%\pd-ghidra-proj`, exports in `.pdtools\re-out\`; our scripts in `engine/re/`. Findings in **PROGRAM-RE.md**: the action table (gsdata 0x1B00C: per-skill cast clip, fire tick, dash/hit windows, sounds), reaction table (0x2B00C), hidden s70–s73, every attack motion handler, guards, STR/DEF/Pierce, knock-down, **animations play at 30 fps** (ours 24), logic 30 Hz, unit fixes (speed 2.78 u/s per point, homing 3°/s per point). The old "lasers fire at 1.26 s" was wrong (0.433 s; sound 305 is Fire Storm's).
- **Footage** (Laurence recorded the original): FOOTAGE.md rounds 1–2 confirm Laser/Slide Laser fire timing, cast sounds, arc shape; Psycho Knife: no dash without lock-on, hit at ~0.69 s, whole move ~1.33 s; three clips lean 30 fps. Videos in `%USERPROFILE%\Pictures\Phantom Dust - Moves\` and `... - Moves 2\` (never commit).
- Screenshot runs open their window off-screen/behind everything (`PD_SHOW_WINDOW=1` to show) — Laurence asked for no pop-ups.

**Later the same night (all merged; tests 216 + 31 + 6 green):** M1 tables in `skills.json` (`action` ground/air rows, `reactions`, `s70_73`; converter writes emitter textures); M2 clips at 30 fps + 5-frame blends (Laurence approved from `shots/anim_24_vs_30.mp4`; all `cache/chars/*.glb` regenerated); M7 phase-3 reading (movement, dodges = hidden skills 750/751, root motion, combos, capsule effect bodies, packed effect archive `Effect/e.i` + `a*.e` with `engine/re/effect_archive.py`, Aura refill curve + pauses, 14 s capsule respawn); Highway far floor (fmt 0x10/flags 0x0003 draw > 1,000 u footprint) and Panorama Boss sky dome (fmt 0x0B) no longer solid, all 30 `.pdcl` regenerated (`shots/orch_hw_sheet.png`). Next build lanes: PLAN.md "Moves track" M3–M6 and M8–M14. Viewer note: `--close 45` renders black on st06 right now; use `--close 30`.

**Build round 1 (merged; tests 223 + 34 + 7 green):** M3 casts follow the action table (per-skill fire tick, caster held for the whole clip from each character's own clip lengths, early hit cancels, melee dash/hit windows, lunge only when locked on within 80 u, 240 u/s, stops 11 u short; cast sound from the row) — checked in game: Laser holds 68 ticks, fires ~26 ticks in; M8 characters converted from the colour-variant `pcXXa0.alr` files + `cache/chars/<id>.rootmotion.json` (Edgar's dodge now a real cartwheel; jacket unchanged, but colour files have 13 extra draws — lead for the jacket); M9 all effects from the packed archive, skill effect chosen by action row w2 (no borrowing; sweep 143 flights / 50 swings / 11 story-only with none). Open from these lanes: guard time should be s46 − 1 ticks (M5); charge release not screenshotted/tested (M14); 43 loose-only effects (500s) not in `e.i` — why unknown; ATK_TUKI root drift 19.5 u in colour file vs 34 in base (footage check); 751 ground dodge is one move (M11). Next: M4 + M10, then M5, M11, M12, then M13, M14, M6 (PLAN.md).

**Laurence's play-test fixes (merged; tests 225 + 34 + 7 green):** blades — every cast now plays its action row's clip from the cast start (ATK_SORD's knee-up stride + slash matches the footage; two-handed swords use ATK_WHAND_SORD), s8 is never a sound (extra sounds on Mist Blade/Excalibur/Muramasa removed), attacker voice at cast, victim voice 40 / 50 on knock-down (unconfirmed by ear); proof `shots/bl_pk_before_after_footage.png`. Camera — free look up to ~65° (eye orbit decoupled from aim), locked camera 14 u lower in top & bottom, no downward tilt when locked on and close (A–F sheets `shots/cam_calib_*`; D applied). Test hooks `PD_TEST_SPLIT=sbs|tb`, `PD_CAM_DEBUG=1`. **Open from these:** our blade swing effect is far smaller/dimmer than the original's big magenta blade and sits near the feet; the original plays slot 0 at cast start (see footage sheet); fx.rs comments + FORMATS-EFFECTS line ~37 still mention the removed `strike_lead`; a dash over a jump zone launches the caster mid-cast (st05/st06 `--near`); sim has no hit-reaction rows yet (M4); s19 sound plays at cast, program plays it at the fire tick.

**UI + research round (merged; tests 246 + 36 + 7 green):** blade effects start at the cast, sprite/puff emitters shown on cast/hit slots (Psycho Knife's slash still smaller/lower than the original — needs a tick-exact screenshot tool + piece scale/attach check); live 3D character previews on the setup screen (`frontend/charselect.rs`), P2 alternate-colour costume decided (converter lane queued after jacket2); menu feel (fades, eased highlights, hold-to-repeat, menu sounds — ids guessed by waveform, `frontend/sound.rs` consts), st01/st24 hidden (17 versus arenas), HUD names/skills/timer in the menu pixel font, result fun stats, Randomize Arsenal (G in the builder); Hero's jacket = the program's cloth grid (`dangle.rs`, PROGRAM-RE "Character parts / cloth"; Laurence picked C) — untextured side panels being fixed in `lane-jacket2`; design docs DESIGN-BRAWL/2V2/REPLAYS with Laurence's decisions. Open: cloth collision shapes (COLF/COLP); head pieces shown one at a time?; menu sound ids need Laurence's ear.

**Later (merged; tests 245 + 36 + 7 green):** character materials single-sided — the jacket lining no longer shows (Laurence confirmed option B, `shots/jk2_sheet_sides.png`); P2 wears the game's colour-1 costume when both pick the same fighter, setup preview + match (`cache/chars/<id>.c0..c3.png`, `pd_convert charcolours`; `shots/alt_*`). To check at next play: Psycho Wave's rings now show as many stacked sprite copies (`shots/alt_match_samechar.png`) since sprites were enabled on cast slots — compare with the original. Next: moves round 2 (M4 reactions + M10 movement), Brawl mode build, Psycho Knife effect size (needs a tick-exact screenshot switch).

**Moves round 2 + polish (merged; tests 261 + 36 + 7 green):** M10 movement (walk/run 20.8/55.6 u/s with hysteresis, 600°/s turning + slowdown, gravity 108 u/s² + 225 u/s cap, landing by fall height, locked strafing, jump clips stretched; jump-dodge 85 u/s / 259.2 u/s²; its DASHEND_B landing + wind-up still missing → M11); M4 hit reactions (reaction table in the sim, light/medium/heavy by damage in the last 3 s, air hit → knock-down, launch physics, lie 2 s / get up 1 s / 2.5 s blinking invulnerable, directional hurt clips, row voices); effects attach to the character's named bones (`fx/bones.json`), BLADE01 ribbons + FLAIR ray bursts drawn, PARTICLE01 read the program's way, additive effects add colour like the game (`fx_gamma.wgsl`, `PD_FX_GAMMA=1` = old), piece easing curves; tick-exact screenshots `PD_SHOT_TICKS`; HUD cost digits centred from measured glyph size, uses count "xN" beside the name. Laurence kept the default tone-mapping (A) — `PD_TONEMAP=none` exists. Open: knife rays still larger/denser than footage (camera distance?), strike pose swings sideways vs footage's thrust, LIGHT02 point lights not drawn, second render target unknown, MSPARTICLE reading, red glow round the attacker in footage (not the cast). Next (PLAN.md): M5 range bands + guards, M11 dodges/movement skills, M12 root motion in the sim, M13 Aura/capsules, M14 combos/charge, M6 per-family motion fixes; Brawl mode build.

**End of session (merged; tests 268 + 36 + 7 green; build current):** HUD rims — main ring offset −1.5,−1.5 (Laurence's pick from a numbered sheet), knob bezels drawn at the disc's scale (art ratio; `shots/hud4_overlay.png`), overridable with `PD_DIAL_OFF`/`PD_BEZEL_OFF`; locked-on strafing — the program's walk/run table is walk-first, so walking strafe = TURN_RUN_*, running = TURN_WALK_* (Laurence's footage `Pictures/Phantom Dust Walk/`), the TURN clip is a one-shot pivot then the looping WALK/RUN with the body kept toward the target (`strafe_sequence`, `PD_ANIM_LOG=1`), plus ±5° hysteresis at the 45° split; `tools/bats/PLAY_DEBUG_CONTROLS.bat` now also logs gait/motion/clip. **Next chat: run `engine/docs/archive/HANDOFF-ORCHESTRATION.md`.**

**Decided (Laurence):** rebuild moves from the program + footage; **hand-tuned move work paused** until then; scale-up = table lane + reading lanes together.

**Next:** PLAN.md "Moves track" M1 + M2 + M7 in parallel, then M3 → M4 → M5 → M6. Then the PLAN waves (1A st01/st24, 1C cartwheel/shake; 1B is superseded).

**Doc corrections still owed** (flagged by the reading lanes; do them with the matching M lane): FORMATS-ANIMATION "24 fps" → 30 + root-motion note; INDEX "24 fps" line; MOVES.md knock-down (s57–s59 are light/medium/heavy), Parabola s66/s67 swap, Fall diagonal, Tornado no homing, mines hurt owner, Brush stun 1.0 s, barrier ≈ s46−1 ticks, s50 short-range; FORMATS-SKILLS main rows ("u32 always 0" = s70–s73 of the previous record, s44 range band, s57–s59, defence s45/s46); FORMATS-EFFECTS `SHOTnn` markers + the 1.26 s note; FORMATS-AUDIO s13 row; backlog item 4 in the entry below is superseded.

**New issues:** Highway (st02/12/22) and st24 have a huge invisible walkable floor (likely the fmt 0x10 / flags 0x0003 draw) — walkprobe shows millions of u²; real arena bounds are probably in the undecoded `.dat` block at header 0x94. Crawler flight effect barely visible. Ki Palm has no impact sound (s21 = 0). `PD_TEST_PRESS` press window is 3 ticks (can be skipped on slow screenshot frames; widen to ~10).

**Lane requests not yet done:** `pd_convert effects` writes emitter textures (in M1); export packed effects 300–321 from `e.i`/`a*.e` (M7 research, then a converter lane).

---

## Handoff from Claude Code: where to continue (2026-09-27 evening, after the menus / palace / effects lanes)

Read AGENTS.md → docs/INDEX.md → docs/NEXT-SESSION.md → this entry. Everything is merged and committed on `main` (local, not pushed). Tests: `cargo test -p pd_game -p pd_formats -p pd_convert` → **211 + 22 + 2 green**. The cache is fully regenerated (all stages relit, `stagefx`, `menus`, `chars`).

**Done today (this session):**
- **Skill uses (s26)**: skills stay on their button until their uses run out (0 = unlimited, ~half of all skills); per-use Aura cost (s24) per button; consumed = used up; Lack = unlimited-use filter; HUD "xN" on limited skills.
- **Match rules as settings** (`combat::Rules`, setup screen rows): Level 0, 4 skills dealt, capsule spots full at start + refilled 15 s after pickup (unconfirmed timing), rounds default 1; time-out = healthier wins (Laurence). `--start-level` / `--start-hand` for test runs; `--loadout` deals no hand.
- **Per-character dodge** from `charaset.dat` +0x36 (750 Jump / 751 ground) + clip length (tumble vs cartwheel): Jump, Tumble, Cartwheel (+ recovery); left bumper also dodges (`sim.rs`, `pd_convert chars`, FORMATS-ANIMATION.md "Character table").
- **Menus lane** (merged): front end rebuilt in the original's style from the game's menu art and pixel font (`System/dbfont.ddx`), portraits, bios, stage previews (MENUS.md, FRONTEND.md). The versus-setup menu script is now `confirm,up,confirm`.
- **Palace lane** (merged): stages lit from each `.dat`'s light script (the SD shader's formula baked into vertex colours), sky + backdrop drawn, characters lit by the script's character set (`stage_look.rs`), Palace + Strange City music, `--near` floor fix, glb size fix (FORMATS-STAGES.md "Lighting script", FORMATS-ALR.md). All 19 arenas checked: `shots/palace_all_stages.png`.
- **Effects lane** (merged): stage EF00 object-effect records decoded (debris, fragments, smoke, sounds, camera shake) and played on hit/break; shots hitting objects play their impact sound; `PDEFTOPT` distortion pieces hidden (they were the cyan striped swirls); effect slot 2 = ground impact (FORMATS-STAGES.md "Object effects", FORMATS-EFFECTS.md).
- Reference: `Reference Images/` (Laurence's screenshots of the original, gitignored). The community modding wiki (phantomdust.miraheze.org) and dustguide.neocities.org were the key leads. YouTube does not play in the in-app browser here.

**Needs Laurence:** the controls recording (`tools/bats/PLAY_DEBUG_CONTROLS.bat`); dodge feel (cartwheel speed/recovery; is the tumble clip right?); stage preview pictures on the setup screen (mapping partly guessed — an A–F sheet would settle it); st04/st14 very dark views (data says so); tone mapping (palace calibration sheet C/D).

**Backlog (next, in order):**
1. Controls log when it exists.
2. **Jacket (pc00a) — needs a reverse-engineering pass** (Laurence 2026-09-27: "none are correct"). Findings so far: the original jacket is SHORT (ends at the waist: X-harness down to a black band, short green straps — `Reference Images/Jacket Back Texture.png`); ours shows long flat green flaps to mid-thigh and no lower harness. The extra part (model 1, 278 verts, UVs cover the lower harness + strap strip of texture 0) is grafted at body joint 63 under the chest (p0j10, which the idle pose tilts ~46°), so it juts out behind (`shots/jb_sheet.png`). Re-grafting it to the hips/lower spine (`PD_PART_HOST=8|9`) puts band + straps at the hips but the long flaps and missing harness remain (`shots/jacket_calib.png`, `jcal_close.png`). Normal maps are not the cause. Likely lead: the body ALR holds overlapping costume geometry (a long-coat layer) that the original hides per costume — compare with `Player/pc00a/pc00a0..3.alr` (see ROADMAP "Convert the 4 variant models") and find the per-draw visibility/costume flag. Tools: `PD_SKIP_DRAWS=a-b` (converter), `PD_PART_HOST`, `PD_CLOTH_HANG` (viewer/game).
3. **Camera shake** from break records (values in `cache/fx/stage/stNN.objfx.json`; render-only offset in `follow_cameras`).
4. **Laser timing**: sound evidence says the original fires lasers ~1.26 s after the cast; the sim fires at 0.4 s (`CAST_TICKS`). Decide per family (MOVES.md).
5. st01 / st24 are flagged not versus-selectable in `stageset_en.dat` (MENUS.md) — set `"versus": false` in the stages converter and FORMATS-STAGES.md.
6. Effects: per-piece blend modes (ice pieces show as white chunks), Aura Particle effect (600–609 show nothing), crater decals (`mdl_hit_*`), ambient records (jump-zone hum), 5 unknown record kinds.
7. "Cartwheeling halts Aura regeneration" (one fan guide) — small combat rule.
8. Older backlog: arena leftovers (contaB collapse, rubble on players, melee on objects), camera in tight corridors, MOVES.md stragglers, HUD could use the menu font, 2-v-2 and online design docs, first-run converter.

**Lanes this round:** three Opus lanes in worktrees `..\pd-lane-*` with their own build copies `%USERPROFILE%\.pdtools\target-*` (all removed after merge). `..\pd-probe` (cloud-research branch checkout) is still there — remove with `git worktree remove ..\pd-probe` when not needed.

---

## Handoff for a fresh chat (2026-09-27, after the cloud-research probe) 

**Who / what.** Laurence (non-technical creative director; plain English, outcome first, no code in replies, screenshots for visual work, decisions as click-panels with a recommended option) is rebuilding **Phantom Dust** (2004/2017) as a Rust + Bevy 0.19 fan runtime for **local split-screen versus**, later online. Hard rules, toolchain and commands: `AGENTS.md` (root). Docs map: `engine/docs/INDEX.md`. Project skills: `.claude/skills/` (pd-build-run, pd-asset-pipeline, pd-visual-verify, pd-reverse-engineer). Never commit/push game data; push only when Laurence asks; sim (`combat.rs`, `sim.rs`, `moves/`, `props.rs`) stays deterministic.

**State of `main`** (local, 90+ commits ahead of `origin/main`, not pushed): every versus system exists — all skill families (MOVES.md), defences, knock-down, arsenal rules + builder, title/setup/result menus, 19 named arenas with their objects (breakable, throwable — GAME.md "Arena objects"), music, sound effects and voices, original HUD, skill effects. `cargo test -p pd_game -p pd_formats` → **197 + 16 green**. Launch: `PLAY.bat` (menus), `tools/bats/PLAY_ALL_SKILLS.bat`, `tools/bats/PLAY_DEBUG_CONTROLS.bat` (records input to `engine/shots/input_log.txt`).

### The cloud-research branch — what happened

A cloud session (no game files, no Windows toolchain) wrote research on branch `origin/claude/phantom-dust-confidence-kd0s74` (5 commits on top of the *initial* checkpoint `ff11975` — it never saw today's `main`). Its handoff asked the PC lane to build and run `pd_convert skillprobe`, then write this handoff. Done:
- Built on Windows/GNU in a side worktree (`%USERPROFILE%\Documents\GODOT\pd-probe`, branch checkout; delete with `git worktree remove ..\pd-probe` when no longer needed). Its `ssb` tests pass (2).
- Probe output: `engine/shots/skillprobe.txt` (local only — contains game-derived data).
- **Confirmed:** the skill table is `gstorage/gsdata_*.dat`, exactly the table `main` already decodes (FORMATS-SKILLS.md) — the community "SkillEditor" framing is ours shifted (their `+0x7A` behaviour = our s63 motion, `+0x0E` capsule = s9, `+0x12` school = s11; identical histograms). Its **SSB reader parses all 785 scripts, 0 failures**; scripts exist only for events, menus, bosses, enemy AI, quests — **none for skills**, so skill behaviour is in the exe and must be rebuilt by observation (which is what `main` did).
- **Superseded by `main`:** the branch's "next steps" (netcode refactor *before* building combat, first kind = straight shot) assumed nothing was built. Its SKILL-KINDS.md is covered by MOVES.md. **Do not merge the branch wholesale** — its FORMATS-SKILLS/HANDOFF/INDEX/ROADMAP/PIPELINE conflict with `main`. Worth cherry-picking (ask Laurence first; he hasn't asked to merge): `pd_formats/src/ssb.rs` + the `skillprobe` command (working tools), `docs/NETCODE-RESEARCH.md` and `docs/MATCH-RULES.md` (read them straight from the branch: `git show origin/claude/phantom-dust-confidence-kd0s74:engine/docs/MATCH-RULES.md`).

### New findings that change the game (do these first)

1. **Skill uses (s26) — probable, strong evidence** (FORMATS-SKILLS.md s26): 0 = unlimited, 1 = single use, 3 (Power, Knife Grind, Amulet), 5 (Jump). About 204 of 365 skills are **unlimited** and should stay on their button; `main` currently uses up *every* skill on cast (DECISIONS.md 2026-09-26 "Skills are used up when cast" — made without this evidence). Build: per-slot remaining-uses counter (captured skill → s26 uses; 0 = ∞), decrement on use, empty at 0; "Aura cost +1 per use" (s24) and Knife Grind's −1 then accumulate per slot; Lack = erase equipped skills with s26 = 0; Retain (s41 = 1) keeps its meaning for single-use skills; HUD shows remaining uses. Re-check tests that assume single use. Tell Laurence in one line (it changes play a lot).
2. **Jump is a per-character dodge** (MATCH-RULES §5, community ✅): click left stick = each character's fixed move — **Jump, Tumble (roll, no recovery) or Cartwheel (faster, has recovery)**; Xbox One/PC also on left bumper. Ties into Laurence's report that "jump doesn't work": our jump is a small hop for everyone (apex ≈ 0.75 m). Find which character has which (look for per-character data in `game_files` / the character table; animation names DODGE*/ROLL*/CART* in `ANIM_NAMES`), then build the three moves. Add left bumper as a second dodge button.
3. **Match start** (MATCH-RULES §1–2): Level starts at **0** (ours `START_LEVEL = 1`); **4 random skills already on the buttons** at the start (ours: empty); a picked-up capsule **respawns at the same spot after ~15 s** (ours: every 2 s, placeholder); draw order random (ours ✓). Using a skill or Dash may pause Aura refill (⚠️ one source).
4. **Match structure** (MATCH-RULES §4): one battle, last player standing (rounds / best-of not found — ours has rounds; keep as an option, default to 1?); time-out probably a **draw** (ours: healthier wins); falling off = −3 health ✓; rule variants (Standard / Puny / Heavy / Single School / Highlander / Ante / Quick…) for the setup screen later.
5. **Online prep** (NETCODE-RESEARCH.md): plain `ggrs` 0.13 rollback; the sim must have **no `atan2`/`sin`/`cos`** (ours uses `yaw` angles and trig in `sim.rs`, `combat.rs`, `moves/` — replace facing angles with unit vectors or pinned `libm`), tick-count timers (e.g. `land_timer` is f32 seconds), a `Match::checksum()`, and serde on `Input`. Not urgent until online starts, but every new sim line should follow these rules now.

### Still open from before

- **Done just before this handoff:** free camera (not locked on) — the left stick no longer swings it; the right stick turns it and now looks up/down (keyboard R/F). `shots/pitch_sheet.png`. Laurence's other controls report (jump / camera "not working") still awaits the input log below.

- **Controls report (Laurence):** jump and right-stick camera "don't work", keyboard and pad. Sim verified fine on all 19 arenas at the real start points; layout matches the official one (click left stick jump, RT lock on/off, right stick camera only when unlocked, click to recentre). Waiting for `engine/shots/input_log.txt` from `tools/bats/PLAY_DEBUG_CONTROLS.bat` — lines `[input] … P1 in Input {…} | P1 y … grounded … locked … cam_yaw …`. OS-level key injection into the game window proved unreliable here — don't rely on it.
- Arena objects leftovers: contaB collapse (`OA01`/`OC01`), the game's break effects (`OE00`) and hit sounds (`mdl_hit_*`), melee hitting objects, rubble on players (GAME.md "Arena objects").
- Camera in tight corridors (st03/st04/st13).
- MOVES.md stragglers: Necronomicon, Diffused Laser, extra damage from the drop after an air hit, knock-back distance (Lack becomes easy with s26).
- Status icons: not in the game's art files (HUD.md) — text tags stay.
- Later: builder art polish, 2-v-2 (revive halves max health — MATCH-RULES §4), online, first-run converter.

### Suggested order for the fresh chat
1. Read AGENTS.md → INDEX.md → this entry → NEXT-SESSION.md.
2. If `engine/shots/input_log.txt` exists, diagnose the controls; otherwise remind Laurence to run `tools/bats/PLAY_DEBUG_CONTROLS.bat`.
3. **Skill uses (s26)** — biggest faithfulness gain; tests + a short in-game check.
4. **Match start values** (Level 0, 4 skills on buttons, 15 s capsule respawn at the same spot) — small, all named constants; record in DECISIONS.md.
5. **Per-character dodge** (Jump / Tumble / Cartwheel) — needs a data search first (pd-reverse-engineer), then animation + sim; screenshots/A–F sheet for Laurence.
6. Ask Laurence (one panel): merge the cloud branch's tools/docs? rounds vs single battle? time-out draw vs health? jump height for Jump characters?

### Confidence picture (plain English, for Laurence)
- Versus rules and systems: all present; faithfulness now limited by (a) numbers the files don't hold (speeds, refill rate, toughness) and (b) the three findings above — fixing uses + match start + dodge moves is the biggest step toward "feels like the original".
- Skills: table fully decoded and cross-checked by an independent method; behaviour rebuilt from the game's own guide text, most rules evidence-based, the rest named "unconfirmed" constants.
- Online: design researched; the sim needs a trig-free facing refactor first.

**Risks:** the 2017 build was server-patched, so on-disk numbers may not be final balance; community sources (MATCH-RULES) are mostly search snippets (⚠️); Esper-RE / SkillEditor are GPLv3 — facts only, never code; budget ~£60/month — use Sonnet lanes for bounded work, verify every agent's output before merging.

**Commands this session ran:** `git fetch origin claude/phantom-dust-confidence-kd0s74`; `git worktree add ..\pd-probe origin/claude/phantom-dust-confidence-kd0s74`; in it `cargo build --release -p pd_convert`, `cargo test -p pd_formats ssb`; `pd_convert.exe skillprobe ..\game_files > engine\shots\skillprobe.txt`.

---

## Handoff from Claude Code: where to continue (2026-09-27, arena objects B + audits + STR rules)

Entry point for the next session or lane. Read AGENTS.md → docs/INDEX.md → docs/NEXT-SESSION.md → this entry. Everything is committed on `main` (local; not pushed). Tests: `cargo test -p pd_game -p pd_formats` → 197 + 16 green.

**Done this session:**
- **Arena objects phase B** (GAME.md "Arena objects", `props.rs`, `props_view.rs`): objects wear down from shots, blasts and areas; damaged/broken models from `stNN.props.glb` replace the placed ones; broken objects lose their collision (the `.pdcl` now tags each object's triangles — `OBJT` trailer, COLLISION.md; **regenerate stages with `pd_convert stages`** on a fresh cache); floors/bridges drop players; Psycho Kinesis / Power throw the nearest throwable object (lift, hang, fly, shatter; STR by size); objects reset every round. All sizes/limits are unconfirmed named constants. Proof: `shots/propsB_st06_sheet.png`, `shots/pkw_sheet.png`.
- **Rules:** Retain skills (s41 = 1) stay while Aura covers them; same-button effects keep the cast-time button; Brush brushes shells aside and they destroy objects; Auto-Capture refills the button from the arsenal; attacks that erase on hit (s48–s50); **attack STR rules** (s45 fully tabulated in FORMATS-SKILLS.md) incl. hold-to-charge and Finisher.
- **Audits (2 read-only Sonnet passes) — findings fixed:** phantom cast sound/effect every round start (`sfx.rs`/`fx.rs`); world visuals (capsules, shots, area markers, crystal, bursts) leaked into the next match and could reuse the wrong colour (`fx.rs`, now `MatchScene`-tagged). Everything else audited clean (stagedat/sound/stageset parsers, converter props pipeline, jump zones, determinism, HUD, dangle, menu, animation driving).
- **Status icons:** searched every menu/battle art file — the original icons aren't there (HUD.md "Status icon search"); text tags stay.
- **Controls:** Laurence reported jump and right-stick camera not working (keyboard and pad). The sim works (checked on all 19 arenas at the real start points); layout matches the official one (click left stick = jump, RT = lock on/off, right stick = camera only when unlocked). Quick taps are no longer lost between slow frames. **Waiting on `tools/bats/PLAY_DEBUG_CONTROLS.bat`'s recording** (`engine/shots/input_log.txt`, `PD_INPUT_LOG=1`) to see what his devices send. Jump height (apex ≈ 9 u ≈ 0.75 m) may be too small to notice — no height in the data; a tuning question for him.
- **Test tools:** `--near` (players start close), demo side-steps when stuck, `--props-state damaged|broken`, `PD_PROP_LOG`, `PD_SHOT_ON_THROW`, `PD_DEMO_LOG`, `PD_INPUT_LOG` (GAME.md "Test switches").

**Needs Laurence:** the controls recording; jump height; everything in the previous entry's row 1 (feel, sounds, voices).

**Backlog (next, in order):** (1) read `input_log.txt` when it exists and fix what it shows; (2) arena objects leftovers: contaB collapse animation (`OA01`) + collapsed hull, the game's break effects (`OE00`) and hit sounds (`mdl_hit_*`), melee hitting objects, rubble falling on players; (3) camera in tight corridors; (4) MOVES.md stragglers (Lack filter, Necronomicon, Diffused Laser, drop damage after an air hit, knock-back distance); (5) builder art polish; (6) 2-v-2 and online design docs; (7) first-run converter.

---

## Handoff from Claude Code: where to continue (2026-09-26, after the arena-objects lane + audit)

This is the entry point for the next session or lane (Claude Code or Hermes). Read AGENTS.md → docs/INDEX.md → this entry. Everything below is committed on `main` (local; not pushed).

**Goal:** a faithful local split-screen versus Phantom Dust. Every system of versus now exists (skills of every colour, defences, knock-down, arsenal rules and builder, menus, 19 named arenas, music, sound). What remains is (a) Laurence's judgement on feel/looks/sound, (b) arena objects (phase A in progress — see STATUS below), (c) bigger new systems (2-v-2, online), (d) polish.

**Key decisions (newest in DECISIONS.md; per-family evidence in MOVES.md):**
- Behaviour comes from the skill table (`cache/skills.json` `raw`), one module per family / capsule colour in `pd_game/src/moves/`; the game's guide text (`Menu/mb0*_en.ssb`) is the spec. Guesses are named constants marked "unconfirmed".
- Table units: time 1/30 s (confirmed); distance 0.1 u, speed 3 u/s per point, homing 0.1 rad/s per point (unit guesses — one constant each in `moves/mod.rs`).
- The sim (`combat.rs`, `sim.rs`, `moves/`) is deterministic; render/audio (`fx.rs`, `hud.rs`, `sfx.rs`, `frontend/`) only read it.
- Parallel lanes: one worktree per lane, each owning named files; shared build dir `%USERPROFILE%\.pdtools\lane-target` (copy of `engine/target`); lane brief template: **docs/LANE-BRIEF.md**. Check `git log -1` in a new worktree — agent worktrees have started from stale commits before.

**STATUS (2026-09-26, all landed and merged; 180 pd_game + 14 pd_formats tests green):**
- **Arena objects phase A (Opus lane) — done.** `Map/stNN.dat` decoded (FORMATS-STAGES.md): object types (intact / broken / damaged model + collision hull), placed objects (type, class/kind/param, position, rotation), six start-point sets (4 starts each, facing, 3 capsule spots per start). Every stage glTF shows its objects; `.pdcl` includes their hulls; `stNN.props.json` + `stNN.props.glb` (model library for swapping broken/damaged) for the sim lane. Stage bounds now ignore >1500-u draws (st24 no longer starts in a void).
- **In play (orchestrator):** matches start at the original start points (start set 0, starts 0 and 1), capsules appear at each player's three fixed spots (Call's anchor overrides; Retreat now makes sense), jump zones (class 4, kind 16) launch players with speed = param (unconfirmed unit). Falls back to the survey when a stage has no layout file.
- **Audit (Sonnet, read-only) — done; all findings fixed and tested:** rules (Anti-Pierce is s49 — a column slip in the original decode; short-range-only erasers s50; "can't be erased or overwritten" s41; passive Orb/Amulet; automatic Shelter penalties + environment; Trigger same button; Annoying Gift; level penalty −1 = to 0; resets clear freeze/poison; Paralyze Barrier binds; same-tick double KO = draw; same-tick button presses not lost), front end (crash on a time-up draw in first-to-N; 2nd-match viewports / effects / sounds / voices; **all animation clips now loaded — most cast and knock-down poses had never played in game**; Esc = back in menus, Quit to title in the match overlay; music stops when leaving a match; setup keeps picks; builder fixes), robustness (spawn picking, title without st06).
- Still open from the audit (low): Hungry Demon reads the caster's button when it lands, not when cast (a later press in flight changes it); Photon Barrier's "Retain" isn't built.

**Files that matter:** `pd_game/src/{combat,sim,arsenal}.rs` + `moves/*` (rules), `frontend/*` + `main.rs` (menus, match lifecycle), `hud.rs`, `fx.rs`, `effects.rs`, `sfx.rs` (presentation), `pd_formats` (file formats: alr, skills, sound, stageset…), `pd_convert` (cache builder). Docs: MOVES.md, FRONTEND.md, FORMATS-*.md, PIPELINE.md, HUD.md, GAME.md.

**Tests:** `cargo test -p pd_game -p pd_formats` (173 + 10 at the time of writing); visual proof via `--demo --screenshot` bursts, `--open-screen <title|setup|arsenal|result>`, `--menu-script ...`, `PD_SFX_LOG=1`.

**Backlog, in suggested order (lane size; who):**

| # | Task | Size / who | Acceptance |
|---|---|---|---|
| 1 | **Laurence's review pass**: play PLAY.bat (menus) and tools/bats/PLAY_ALL_SKILLS.bat; listen; review `shots/poses_phase1.png`; answer: shot speeds, lob height, knock-down length, barrier times, voice clips (hurt 10–12 / KO 60–62 guess in `sfx.rs`), Edgar's Dream + Panorama Boss in versus? Palace/City music? | needs Laurence | DECISIONS.md rows; constants tuned |
| 2 | **Arena objects phase B (sim)**: objects as sim entities with intact → damaged → broken state (swap models from `stNN.props.glb`, hide the `propNNN_*` node); Psycho Kinesis / Power throw the nearest non-broken class-2 object that has a broken/damaged model (not floor/building kinds 23, 26, 28, 48, 58), STR by object size (not in the data); destructible floors/bridges (st04 `yuka`, st03 bridges kind 23, st08 blocks kind 58, st06 contaB) + rubble damage; shots hitting objects. FORMATS-STAGES.md "What a sim lane needs" | Claude (touches combat.rs + a new props module) | tests + burst screenshots per behaviour |
| 3 | **Status icons**: the original shows icons beside the Health bar; not found in `BtMenuEn.alr` — search other menu `.alr` atlases (`pd_convert hudtex` + `sprites`), then swap the text tags in `hud.rs` | Hermes-sized | screenshot of each icon in play |
| 4 | Arsenal builder polish: the original's `DckMenu.alr` panel bars, skill-card art if any exists, gamepad-only navigation check | Hermes-sized | screenshots |
| 5 | Stragglers listed as not built in MOVES.md (Lack's "unlimited-use" filter, Necronomicon, Diffused Laser, extra fall damage after an air hit, knock-back distance, `environment::stat` needs `age` for timed buffs) | Hermes-sized, one PR each | unit tests |
| 6 | Camera in tight corridors (st03/st04/st13 pull in very close) | Claude or Hermes | before/after screenshots |
| 7 | 2-v-2 + resuscitation ("approach and capture him or her just as you would a capsule") — needs 4 views, teams, lock-on switching (left trigger) | Claude, big | design doc first |
| 8 | Online versus (deterministic sim is ready: lockstep vs rollback, transport) | Claude, big | design doc first |
| 9 | First-run converter inside the game (user picks their game folder), iw4L-style | Hermes-sized | fresh cache from nothing |

**Risks:** many rules are readings of unlabelled table fields — plausible, tested, but unconfirmed until someone who knows the game plays; several agents wrote code quickly (the audit exists for this); `cache/` must be regenerated by the PIPELINE commands on another machine; never commit game data; Laurence's budget (~£60/month) — prefer Hermes for rows marked Hermes-sized.

**Commands Claude Code ran this session (all green at the end):** `cargo test -p pd_game -p pd_formats`, `cargo build -p pd_game`, `cargo build --release -p pd_convert`, `pd_convert stages / sounds / hudtex DckMenu.alr cache\ui\dck`, demo + menu bursts in `engine/shots/`.

---

## Handoff from Claude Code: all versus systems (2026-09-26, second orchestrated run)

**Goal:** Laurence: "replicate all systems, even the deck builder" (he's away; no play-testing).

**Done (173 pd_game + 10 pd_formats tests; demo and menu-started matches run clean):**
- **Match rules (orchestrator):** per-player arsenals (`Combat::with_arsenals`), arsenal rules `arsenal.rs` from the guide (30 skills, ≤ 3 copies except Aura Particles, ≤ 3 schools; 1 school = faster aura, 3 = slower — amounts unconfirmed), 15-min battle timer (guide) shown lower right, time-up → healthier player wins / the game's own DRAW banner, "powerless" Health drain when no skills are left anywhere, capsules in the guide's seven colours.
- **Front end (Sonnet lane):** title menu → versus setup (both players at once: character, arsenal; stage, time limit, rounds to win) → match → result; **arsenal builder** (catalogue with school/type filters, skill card, live rule problems, save/load/delete to `engine/userdata/arsenals.json`). All old CLI switches still go straight into a match. docs/FRONTEND.md.
- **Stages (Sonnet lane):** all 30 stage files convert (`pd_convert stages`); the game's own stage-select table (`setup/stageset/stageset_en.dat`) gives **19 versus arenas with their names** (Spiral Highway, Lost Palace, Panorama Earthquake, Dawn City, Storm Refinery, Twilight Lane, Sein, Edgar's Dream, + recolours); `cache/stages.json`; 12 arenas matched to their music. PIPELINE.md.
- **Sound (Opus lane):** all 73 STH2 banks decoded (plain PCM clips with layered events — not ADPCM; Esper-RE's exporter is wrong for them), 1,291 WAVs + index (`pd_convert sounds`); skill fields → sounds (s21 impact, s20 flight loop, s13 launch, s19 fall whistle, s8 activation); `sfx.rs` plays cast/flight/impact/block sounds and hurt/KO voices. FORMATS-AUDIO.md. **Nobody has listened yet.**
- **Integration (orchestrator):** sound switched on; menu shows character names from the game's character table (Hero, Edgar, Freia, Meister, Chunky, Cuff Button, pH, Know, Tsubutaki, JD, Sammah); stage list limited to versus arenas; each arena plays its own music when known.

**Needs Laurence:** listen (voice clip meanings 10–12 vs 60–62 are guesses: `VOICE_HURT`/`VOICE_KO` in sfx.rs), play through the menus, judge the arsenal builder, confirm whether Edgar's Dream / Panorama Boss belong in versus, Palace/City music.

**Not built:** the original's deck-menu art (`Menu/DckMenu.alr`, needs a pd_convert extractor), status icons (not found in the HUD atlas), skill-card pictures (`Menu/Mpt*.alr`?), 2-v-2 / resuscitation, online.

---

## Handoff from Claude Code: every skill family built (2026-09-26, orchestrated run)

**Goal:** Laurence: "don't stop until you have done everything you can do without me. Orchestrate whatever models you need."

**Done (156 pd_game tests; long `--demo --arsenal mixed` run with no crash):**
- Knock-down from the table's hit reactions (s57 standing / s59 air; ids 3, 13, 45, 48, 61): blow → land → get up ≈ 4 s, untouchable except by Pursuit-type skills and Gravity Press; falling out of the arena costs 3.
- Groundwork for the non-attack capsules (`moves/effects.rs`): delivery from s43/motion, a status system wired into the sim, the environment crystal, hooks, test kit.
- **Four Sonnet lanes in parallel worktrees**, one file each (`moves/erase.rs`, `status.rs`, `special.rs`, `environment.rs`), each with its own tests; merged in order and re-verified; follow-up fixes by the orchestrator (Dash stops at walls, Frostbite/Freeze Shot button lock, Hungry Demon uses the caster's button, Calamity reaches everyone). Totals: erase 42/44, status 41/45, special 43/45, environment 26/30 — details in MOVES.md.
- Leftover attacks: Ricochet Laser (4 bouncing lasers), Pursuit/Sky Pursuit (dash at a downed foe), Recollection/Copy, Psycho Kinesis stand-in.
- HUD: status tags beside the health panels, environment banner, crystal in the world; cast poses for the new colours.
- `tools/bats/PLAY_ALL_SKILLS.bat` (`--arsenal mixed`): one skill of every family + 10 aura particles. PLAY.bat keeps Laurence's test set.

**Lessons for the next orchestrated run:** agent worktrees can start from an old commit — each lane fast-forwarded itself to main; brief them to check `git log -1` first. `git add -A` in the main checkout picks up `.claude/worktrees/` (now in .gitignore). A shared pre-built `CARGO_TARGET_DIR` (copy of `engine/target`) saved each lane a full Bevy build.

**Not built (see MOVES.md):** Lack's "unlimited-use" filter, a few enemy-only / note-less skills, Necronomicon, Diffused Laser, stage objects for Psycho Kinesis, extra fall damage after an air hit, knock-back distance, the original's status icons (text tags stand in), sound effects.

**Next concrete step (needs Laurence):** play `tools/bats/PLAY_ALL_SKILLS.bat`; review `shots/poses_phase1.png` (cast poses) and judge speeds, arcs, knock-down timing, barrier times. Every guess is one named constant. Bounded follow-ups (Hermes-sized): status icons from the HUD atlas, demo script that raises Level before casting, the not-built stragglers.

---

## Handoff from Claude Code: defences + Add: effects (2026-09-26)

**Goal:** every defence kind and the skills' "Add:" side effects (Laurence: "you decide"; single lane because it all runs through `Combat::strike` / `barrier::resolve`).

**Done (46 pd_game tests pass):** Reflect (weaker shots go back), Course (bends shots aside inside an 80 u field), About Face / Reverse (sends them back), Absorb (front, not vs short-range; heal / aura / erase / capture), Brush (stuns short-range attackers), automatic Shelter (triggers itself if you can pay; can't be cast by hand); defence Add: effects (paralyse 3 s, freeze 10 s, erase, capture, +health, +aura); attack Add: effects (freeze buttons, poison presses, aura → 0, heal = damage, level drain). `strike` now takes how the attack arrives (`Arrival::Melee/Shot/Area`). Cast poses matched by clip name for the new families (sheet `shots/poses_phase1.png`, unconfirmed).

**Not built:** erase/status/special/environment capsule skills; knock-down (reaction ids s57–s59); erase-on-hit attacks; Brush's destroying deflected shells; [Object], Ricochet/Diffused lasers, Pursuit, Copy; HUD icons for frozen/poisoned.

**Next concrete step:** Laurence reviews `shots/poses_phase1.png` + plays when free. Then status / erase / special skills (s43 target kinds + s51 codes; mostly bounded — a good Hermes brief) and knock-down.

---

## Handoff from Claude Code: moves Phase 1 — attack families (2026-09-26)

**Goal:** build the remaining attack families on the Phase 0 framework (Laurence chose "start Phase 1 here").

**Done (32 pd_game tests pass; in-game bursts `shots/p1_area_sheet.png`, `p1_shot_sheet.png`):**
- New shared sim piece: **set areas** (`combat::Area`, `moves/area.rs`) — warning countdown, pulses, timer / sensor / remote triggers, open-sky (Rain) and floor-only (Hole) checks; attacks with no direction (`strike(from = None)`) are covered only by a Shelter.
- Families: **[Hole]**, **[Rain]** (lands once per cast), **mines** (timer / sensor / remote — Timer Mine fuse 150 = "5 sec", confirmed), **Photon Wave** (blast around the caster), **[Arc]** (circle out and back onto where the target stood; Double/Triple/Octo fans), **[Crawl]** + Tornado (along the floor, miss airborne targets, stop at walls/drops), **[Fall]** (drops from 150 u above; roofs stop it unless "Penetrates obstacles").
- Renderer: each area plays its skill's slot-1 effect while set; a faint disc marks Hole/Rain landing spots during the warning (not mines — "very difficult to see"); go-offs play slot 3 at the spot.

**Key decisions / guesses (all in MOVES.md):** Rain hits once per cast; Hole/Rain only blocked by Shelter; mines hurt only the opponent; Remote Mine detonates on the owner's next skill press (press not spent); Tornado speed from s64 in u/s; crawl homing capped 4 rad/s; Arc has no homing.

**Not built:** [Object] (Psycho Kinesis), Ricochet/Diffused lasers, Pursuit, Recollection/Copy (they fire the generic shot); defence kinds Brush / Absorb / Course / automatic Shelter / Reflect's reflection; knock-down; erase/status/special/environment; cast animations for the new families (all ATK_SHOT).

**Next concrete step:** Laurence plays a few loadouts (e.g. `--loadout 147,20,23,63` areas, `65,71,138,19` lasers/crawler/meteor) and reacts; then the defence kinds (Brush, Absorb, Course, auto-Shelter, Reflect) — they touch `strike`/`barrier::resolve` in combat.rs, so one lane.

---

## Handoff from Claude Code: moves Phase 0 done — data-driven families (2026-09-26)

**Goal:** Phase 0 of MOVES-PLAN.md — settle the behaviour numbers, make skill behaviour data-driven by family, rebuild Shell / [Parabola] / [Barrier] on it, write MOVES.md.

**Key decisions:**
- Table units: time = 1/30 s (**confirmed** by "10 sec" = 300, "3 sec" = 90); distance = 0.1 u and speed = 3 u/s per point (likely / assumed); homing 0.1 rad/s per point (guess). One constant each in `moves/mod.rs`.
- Decoded (FORMATS-SKILLS.md, with evidence): s63 motion type, s65/s68 shell speed/homing (matches the guide's "Optical fast shells", "Faith slow but accurate homing"), s47 DEF (Crystal Wall 3 — the HUD card used to say 23, fixed), s44 defence kind (all tags), s48 Anti-Pierce, s54 Pierce / penetrates defence, s55 penetrates obstacles, s51/s52 add-effect codes, s69 multi-laser count.
- Rules from the guide text: STR > DEF destroys the defence with no damage; Pierce gets STR − DEF through; Barrier front-only, Shelter all round.
- Parabola lob gravity 150 (at the players' 300 the bomb peaked ~4 body heights up, out of view). Unconfirmed, tune by eye.
- Short-range: s64 read as strike **reach** (13 u), not dash range; dash range stays 130 u (engine constant) — play is unchanged for Psycho Wave.

**Files touched:** new `pd_game/src/moves/{mod,melee,shell,parabola,barrier}.rs`; `combat.rs` (dispatch, `Shot`-based projectiles with gravity/delay/blast/sub-stepped hits, `ActiveGuard`, `Blocked { broken }`, `Burst`), `fx.rs` (Burst effect, guard bubble), `hud.rs` (card shows real DEF), `main.rs` (loads DEF + raw record), `pd_formats/src/skills.rs` (field names). Docs: new MOVES.md; FORMATS-SKILLS, GAME, MOVES-PLAN, INDEX, AGENTS.

**Tests:** `cargo test -p pd_game` 20 pass (10 new: shell speed, homing, sniper wait, parabola lob + blast, near-miss blast, barrier front-only, shelter all round, STR > DEF breaks, Pierce vs Anti-Pierce, barrier time) + pd_formats 3. Visual: `--demo --loadout 80,10,26,30` and `10,10,10,10` bursts (`shots/moves_p0_sheet.png`, `moves_p1_sheet.png`, `moves_bomb_sheet.png`); an in-game log confirmed every bomb lands and explodes on the target.

**Risks:** speed/homing/lob scales are unit guesses — Laurence's eye (Phase 2) decides; one constant moves every shell together. [Arc] still flies straight. Barrier time from s46 is a reading. Knock-down not built. The `--demo` script taps by wall-clock, so burst frames aren't repeatable frame-for-frame.

**Next concrete step:** Laurence plays via PLAY.bat with shells/bombs/barriers (try `--loadout 80,10,26,30`) and says if shots feel too fast/slow and lobs too high/low. Then Phase 1: fan out families per MOVES.md "Adding a family" — suggested order [Arc] (curve + multi-shot, s67/s69), [Rain] + [Hole] (need a delayed-area sim list in combat.rs — do that one first, single lane), [Fall], [Crawl], Photon Wave, then defence kinds (Brush, Absorb, Course, automatic Shelter).

**Commands Claude Code already ran:** `cargo test -p pd_game -p pd_formats`, `cargo build -p pd_game`, `cargo check -p pd_convert -p pd_viewer`, demo bursts + montages above.

---

## Handoff from Claude Code: complete all moves + animation (new lane, 2026-09-26)

**Goal:** every collectable skill (363) behaves, animates and looks like the original in split-screen versus. Full plan, family table and lane rules: **docs/MOVES-PLAN.md**.

**Key decisions:** DECISIONS.md (newest first). Recent: capture capsules with a skill button; opponent's capsules are faint ghosts in your view; projectiles leave at the end of the cast; original pad layout (X B Y A skills, click-left-stick jump, RT lock-on, right-stick camera, pad 1 = P1); D-pad (P1 arrows) shows the slot's skill card.

**Files touched or planned:** built so far — `pd_game/src/combat.rs` (rules + `CombatEvent`s), `effects.rs` (effect player, trails), `fx.rs` (skill → effect wiring, capsules), `hud.rs` (capture prompt, skill card), `sim.rs` (camera input); `pd_formats/src/effect.rs`; `pd_convert effects`. Planned: data-driven behaviour per family (Phase 0), then `pd_game/src/moves/<family>.rs` per family (Phase 1), `docs/MOVES.md`.

**Tests added or planned:** 10 pd_game tests (incl. capture-by-button, hit/block events, right-stick camera, determinism) + 3 pd_formats. Planned: per-family rule tests + a `--demo --loadout` screenshot burst per family.

**Risks identified:** behaviour numbers (s63–s69) unconfirmed — guessing them in parallel gives inconsistent feel; the game's guide text (`Menu/mb01..05_en.ssb`) is the spec to quote. Laurence's review is the bottleneck: batch A–F sheets. Shared files (`combat.rs`, `effects.rs`) conflict if several lanes edit them — fence them (MOVES-PLAN.md). Budget: a big multi-agent run is expensive; phase it.

**Next concrete step:** Phase 0 in MOVES-PLAN.md — decode s63–s69 against the notes/guide text, make `combat::Effect` data-driven by family, rebuild Shell (12), [Parabola] (16/17) and [Barrier] on it as the pattern, write `docs/MOVES.md`.

**Commands Claude Code already ran:** `cargo test -p pd_game` (10 pass), `cargo build -p pd_game -p pd_viewer`, `cargo build --release -p pd_convert`, `pd_convert effects ..\game_files\Assets\Data\Effect cache\fx` (476 effects, 70 trail textures), demo bursts in `engine/shots/` (las_*, cap_*, info*.png).

**Useful sources found this session:** the game's tutorial/guide text is plain ASCII inside `Data/Event/en/*.ssb`, `Data/Menu/*_en.ssb`, `Data/questdata/en/questdata.qdt` — extract printable runs (see the python one-liner pattern: `re.finditer(rb'[\x20-\x7e\n%]{8,}', data)`). It settled capture, pad layout and D-pad behaviour; use it for skill types too.

---

## Handoff from Claude Code: skill effects decoded + viewer playback (2026-09-26)

**Goal:** M2 plan steps 1–3 (find the skill → effect link, decode enough to place/time an effect, render it).

**Done (verified with viewer filmstrips — Laser charge, beam, swirl; Psycho Wave swirl + hit dome; Crystal Wall wall + shards):**
- Effect number = skill **text id** (proved via the Japanese `name.lst`). `.eft` = 6 event slots → effect models (holds for all 476 files). Per-piece 0x13 block: flipbook grid, start/end frame, start/end ARGB colour, growth — all in **FORMATS-EFFECTS.md** with evidence and ✅/⚠️/❓.
- `pd_formats::effect` (+1 test), `pd_convert effects` → `cache/fx/eftNNNN.glb` + `.fx.json` (476 effects, 14,739 timed pieces). glTF draw nodes are now named per model (`m{model}_d{draw}`) — nothing else relied on the old names.
- Viewer: `--fx eftNNNN --fx-slot N --fx-frame F` plays an effect with additive glow; format-8 (distortion placeholder) pieces hidden.

**Open / risks:** effect frame rate (30 fps assumed), slot meanings 1/2 partly guessed, where each slot attaches (hand, projectile, target) and beam stretching are not in the data we've read (likely game code), spin/velocity/blend words unknown, some crystal pieces may want normal transparency rather than glow.

**Laurence confirmed** the Laser charge-up looks like the original, and chose "effects into the game" next.

**Also done (same session, verified with an in-game `--demo --loadout 65,65,26,65` burst, `shots/fxgame_sheet.png`):**
- Shared effect player `pd_game/src/effects.rs` (the viewer includes it via `#[path]`): `play(name, slot, transform, looping)`; pieces are linked to their effect once their material has loaded; the effect stays hidden until then (no raw-material flash); one-shots despawn when their last piece ends.
- Sim logs `CombatEvent::Hit / Blocked` in `Combat::events` (numbered, last 16) — deterministic, tested (9 sim tests pass).
- Game (`fx.rs`): on cast → slots 0 + 2 on the caster (children of the player, so they follow); projectiles → slot 1, looping, facing their travel; hit → slot 3 on the victim; block → slot 4 of the victim's barrier. Stand-in balls/bubble remain only where a skill has no effect in that slot. Skill → effect name from `skills.json` text ids (`fx::SkillFx`).

**Next concrete step:** Laurence plays via PLAY.bat and judges speed (`PD_FX_FPS`, default 30) and look. Known gaps: Laser beam (slot 1) is a fixed-size quad, not stretched to the target; Laser has no hit pieces (its slot 3 is emitter-only 0x14 chunks); spin/velocity words not applied; format-8 distortion pieces hidden. Cast-animation A–F sheet (Psycho Wave) still to do.

**Commands Claude Code ran:** `cargo test -p pd_formats effect` (1 pass), `cargo build --release -p pd_convert`, `pd_convert effects ..\game_files\Assets\Data\Effect cache\fx`, `cargo build -p pd_viewer`, viewer screenshots + montages in `engine/shots/`.

---

## Start here: moves M2 — skill effects + cast animations (new Claude Code session, 2026-09-26)

**Goal:** skills look like the original when cast — the right body animation and the skill's own effect (laser beam, wave, wall, glow).

**What exists:** body animations all convert and play (cast clip chosen by family in `main.rs::cast_clip`, a guess). Combat visuals are placeholder balls/bubbles (`pd_game/src/fx.rs`). HUD done (HUD.md).

**Effect data (not decoded):** `Data/Effect/` has 476 `.alr` (effect models/textures — same container we already parse) + 476 `.eft` (412 bytes each; Esper-RE `formats/eft.h` gives only the shape: 12 × 20-byte "high" entries, a u32, 6 × 28-byte "low" entries of i16, fields all unknown) + `e.i` (495 × 12-byte index entries) + `ecopy.pl`/`.lst`/`.bat` (build scripts — may name effects!). Link from skill → effect probably via gsdata s13 (3xx on projectile skills) or s12 — see FORMATS-SKILLS.md.

**Plan:**
1. Read `Effect/*.lst`, `ecopy.pl`, `e.bat` for names; `pd_convert info`/`dumptex` a few effect `.alr` (find the Laser's); render one in the viewer. Screenshot.
2. Find the skill → effect link (s12/s13 vs e.i / file numbers). Record in new `docs/FORMATS-EFFECTS.md`.
3. Decode `.eft` enough to place/scale/time an effect on a cast (compare files of similar skills; evidence first, pd-reverse-engineer skill).
4. A–F sheet of cast animations for ambiguous skills (Psycho Wave first) for Laurence.
5. Then hand the per-skill rollout to Hermes (bounded).

**Rules:** evidence before guesses; never commit game data; visual changes need screenshots; sim stays deterministic (effects are render-only unless they change hit shapes).

---

## Handoff from Claude Code: original HUD + top/bottom camera (2026-09-26)

**Done (verified with screenshots in both split layouts, 9 sim tests pass):**
- **HUD** (`hud.rs`, doc HUD.md): the game's own battle-HUD art (`Menu/BtMenuEn.alr` → `cache/ui/hud0..2.png` via `pd_convert hudtex`), laid out after 2017 screenshots: health panels with tick bars, opponent aura/level + arsenal, arsenal counter with next-capsule colour, skill dial (aura over level, cost discs coloured by capsule type, fading name ribbons), lock-on ring + name, 3-2-1-START and YOU WIN / YOU LOSE banners. Font: Bahnschrift stand-in (`cache/ui/hud.ttf`).
- **Countdown** added to the sim (`COUNTDOWN_TICKS`, players frozen), with a test.
- **Top/bottom camera:** vertical view ≥ 45% of the FOV setting and aim 6 lower (Laurence picked A, closer); the old rule gave a ~19° strip that cut the player off.
- World combat visuals moved to `fx.rs`. New research tool `pd_convert sprites` (atlas piece boxes).

**Open:** Laurence to confirm the top/bottom framing (sheet) and the HUD look. HUD gaps listed in HUD.md (aura meter segments, timer, original font, which orb colour = which capsule type).

**Next concrete step:** Laurence plays both layouts via PLAY.bat; tune from his notes. Then M2 remainder: hit feedback, sound effects (STH2 `.bin`), capsules drawn with the hud1 glow sprites instead of plain balls.

---

## Handoff from Claude Code: moves M1 combat rules built (2026-09-26)

**Goal:** first real fights in split-screen — done for M1; next is Laurence playing it and tuning.

**Done (verified: 8 sim tests + screenshots of capsules, laser hits, barrier bubbles, dash attacks, KO banner, round restart):**
- `pd_game/src/combat.rs` — health 20, level 1–16, aura refill, 30-skill arsenal → capsules → auto-pickup into 4 slots, skills used up on cast, table-driven cost/strength/requirements/penalties, Psycho Wave dash, Laser projectile, Crystal Wall barrier, aura particle, hit stun, KO + round reset. Deterministic (seeded xorshift in state).
- `hud.rs` — capsule/projectile/barrier visuals, text readout per player, round banner.
- `main.rs` — loads `cache/skills.json`, skill buttons (P1 J K U I, P2 numpad 1 2 4 5, pad X B LB RB), cast animations by family, `--loadout` and `PD_SHOT_ON_KO` test switches, smarter `--demo`.
- All rules + placeholders tabled in GAME.md "Combat".

**Key decisions:** DECISIONS.md (health/level/test set by Laurence; skills used up, auto-pickup, aura particle from a slot — assumptions to confirm).

**Risks:** placeholders may feel wrong to someone who knows the game (aura refill 1/s, capsule every 2 s, projectile speeds, barrier 1 s). Psycho Wave's animation (ATK_SORD) is a guess. Cameras pull in very tight when a fight hugs a wall (pre-existing occlusion behaviour, more visible now that players fight up close).

**Next concrete step:** Laurence plays via PLAY.bat (build is current). Collect his notes on: pickup style, aura refill speed, capsule rate, Psycho Wave look. Offer an A–F sheet of candidate Psycho Wave animations (ATK_SORD / ATK_SHOT / ATK_KI_SHOT / ATK_PUNCH / ATK_WHAND_SORD / ATK_SPHERE) via the viewer. Then M2 (HUD, hit feedback, sound).

**Commands Claude Code already ran:** `cargo test -p pd_game` (8 pass), `cargo build -p pd_game`, demo screenshots in `engine/shots/combat*.png`.

---

## Handoff from Claude Code: moves M1 — skill data done, rules next (2026-09-26)

**Goal:** M1 step 2 — combat rules in `sim.rs` (health, level/aura, 4 skill slots from capsules, cost, KO + round reset), using `cache/skills.json`.

**Done this session (M1 step 1, verified):**
- `pd_formats::skills` reads the skill table (`gsdata_en.dat`) and debug arsenals (`deckdata_debug`); 2 unit tests (synthetic data).
- `pd_convert skills … cache\skills.json --decks …` → 399 skills (365 collectable), names + notes from the game's own text, `cache\skills.txt` readable table. Added to the PIPELINE regenerate list.
- Field meanings in `docs/FORMATS-SKILLS.md`. Confirmed against every "Requires / Penalty" note: school, capsule type, cost, level/health penalties, cost-per-use, requirement kind+amount, strength (damage). Arsenal ids = field s3 (not Esper-RE's `skill_names.txt` order — that list is by text id, so Esper-RE's deck_reader mislabels most skills).
- Aura Particle has **no record** (built into the game). Debug decks are developer test decks (16 skills), not default arsenals.

**Key decisions:** use `gsdata_en.dat` (format 140; the other languages carry older format-100 data). Unknown fields stay in `raw`; only evidenced fields get names.

**Open (not blocking step 2):** no field indexes our 129 animations. Cast animation should come from a hand table keyed on family field s42 (+ s21 for sword/punch/kick); note tags `[Brush]/[Absorb]/[Shelter]/[Course]` match `DEF_*` animation names. Projectile behaviour type = s63. Speed/range fields s64–s69 unconfirmed.

**Laurence decided (DECISIONS.md):** 20 health; level starts 1, cap 16, aura particle +1; both players carry the same test set (Psycho Wave, Laser, a barrier, aura particles). Still open: capsule spawn timing (use 4 slots per the plan; ask him once it's playable).

**Files touched:** `crates/pd_formats/src/skills.rs` (new), `pd_formats/src/lib.rs`, `pd_convert/src/main.rs` (`skills` subcommand), docs FORMATS-SKILLS (new), INDEX, PIPELINE, HANDOFF.

**Tests:** `cargo test -p pd_formats skills` (2 pass). Real-file check: `pd_convert skills` → 399 / 365 / 393 texts / 66 decks.

**Risks:** cost (s22) not yet checked against the game's own UI; strength-rule codes (s45) not tabulated — skills whose damage is computed show 1.

**Next concrete step:** in `pd_game`, load `cache/skills.json` at startup (Bevy side) and pass a plain skill list into the sim; add deterministic `health`, `level`, `aura`, `slots: [Option<skill_id>; 4]` to the player state with pure-function tests (cost paid, damage applied, KO resets round). Start with Psycho Wave (id 1), Laser (65), a Barrier-family defence, and the built-in aura particle.

**Commands Claude Code already ran:** `cargo test -p pd_formats skills`, `cargo build --release -p pd_convert`, `pd_convert skills` (cache/skills.json exists).

---

## Handoff from Claude Code: moves (skills) milestone M1 (2026-09-26)

**Goal:** Give the split-screen prototype its first real combat: health, aura, skill capsules and a handful of skills driven by the game's own skill table, so two players can actually fight.

**State at handoff (all verified with screenshots unless noted):**
- Play feel accepted by Laurence: run 72 u/s, jump 75/300, camera 60 back / 30 up, lock-on toggle (Y / pad Y), settings menu (split layout, FOV), flicker fixed.
- Characters lit with the game's normal maps; floating badge on pc01a fixed (skinned draws placed at their transform joint).
- Cloth: extra-part chains swing (dangle.rs) with body-capsule collision; P1 coat tails OK. P2 long coat **parked** (see below).
- Every task left a debug tool behind; see "Debug tools" below.

**Key decisions:** DECISIONS.md. Sim must stay deterministic (sim.rs): quantised input in, state out, no clocks/randomness/Bevy types.

**Data available for skills (not yet parsed by us):**
- `game_files/Assets/Data/gstorage/gsdata_<lang>.dat` (278,532 bytes each, 9 languages): per-skill records. Layout (partly) in `tools/Esper-RE/src/formats/templates/EsperGSDATA.bt`: SkillTextID, SkillID, rarity, sound IDs, **CapsuleType** (aura/attack/defence/...), **SchoolID**, **AnimationIDGround / AnimationIDAir** (ids into our 129 `ANIM_NAMES`), multi-press, **Cost**, extra/health cost, **SkillUses**, status effect, requirements, ground/air/both, **Damage**, effect duration, hit effects. Many fields still `unk`.
- Skill names: `tools/Esper-RE/src/deck_reader/skill_names.txt` (395 entries; reference only).
- Decks/arsenals: 30 skill ids each (`deck.h`; wiki "Decks"); `setup/deckdata_debug` (6,604 bytes) likely holds default arsenals.
- Effects: `Data/Effect/*.eft|*.alr` (visuals) — not decoded.
- Skill *behaviour* (projectile speed, homing, hitboxes) is probably partly in unknown gsdata fields and partly in the exe. SSB scripts are mostly menus/text/AI (only a few boss scripts), so behaviour is not script-driven.

**Proposed plan (M1 → M3):**
1. **M1 data:** `pd_formats::skills` parses gsdata into typed records; `pd_convert skills` writes `cache/skills.json` + a readable table. Verify: names line up with skill_names.txt; damage/cost/animation ids plausible (e.g. Psycho Wave = cheap ranged, animation ATK_SHOT).
2. **M1 rules (sim.rs):** health, aura meter (collect aura particles; cap), 4 skill slots filled from capsules that spawn from your 30-skill arsenal, skill cost/uses, KO + round reset. Values the data doesn't give → ask Laurence (he knows the game) via an A–F style choice, don't invent silently.
3. **M1 first skills (one per family):** ranged projectile (Psycho Wave), melee (Psycho Blade/Laser Blade), defence (a shield, DEF_SHIELD), aura particle. Animations by the data's AnimationID; hit reactions DMG_*/BLOW_*.
4. **M2:** HUD (health, aura, 4 slots) per player viewport; hit feedback; sound effects (needs STH2 `.bin` decoding).
5. **M3:** generalise by family across all 395 skills; effect visuals (.eft).

**Files touched or planned:** new `crates/pd_formats/src/skills.rs`, `pd_convert` subcommand, `pd_game/src/sim.rs` (combat state, deterministic), `pd_game/src/main.rs` (HUD, capsule/projectile visuals). Docs: new `docs/FORMATS-SKILLS.md`, update GAME.md, ROADMAP.md.

**Tests added or planned:** unit tests for gsdata parsing (known skill → known values); sim tests for damage/aura/uses (pure functions, no Bevy). Visual: `--demo` extended with scripted attacks + screenshots.

**Risks identified:** unknown gsdata fields may hide behaviour; wrong guesses feel wrong to someone who knows the game — confirm with Laurence. Keep the sim deterministic (projectiles/hitboxes as sim state, not Bevy physics). Build fails while the game is open.

**gsdata findings so far (2026-09-26, verified by byte dumps):**
- Header is **160 bytes**; ints: `[0]` file size (278,532), `[1]` 108,452, `[2]` 753 (record count?), `[3]` 108,452, `[4]` 140, `[5]` 374 ("SkillLimiter" in the template).
- Records are **144 bytes**, starting at offset **160** (confirmed: the 3rd short after the leading `int` counts up 1, 2, 3, 4… across consecutive records). 144 = exactly the field list in `EsperGSDATA.bt`, which also matches Esper-RE's 144-byte "skill pack" entries.
- First records (shorts after the leading int): `0 0 1 1 10 1 3 1 0 1 0 1 1 …` → per template: unk, unk, **SkillTextID**, unk, **RegisterID**, **SkillID**, **Rarity**, unk, SoundFileID, **CapsuleType**, unk, **SchoolID**, **AnimationIDGround** … Field 22 is 505/501 in early records (likely a sound id).
- The region at 108,452 is zeros; the names/descriptions text lives somewhere after it — not found yet. Record count 751–753 to settle (template says 751).

**Next concrete step:** implement `crates/pd_formats/src/skills.rs` reading 144-byte records from offset 160 using the template's field order; add `pd_convert skills <gsdata> <out.json>`; print the first 40 records with SkillID next to `tools/Esper-RE/src/deck_reader/skill_names.txt` and check that names line up (e.g. record for "Psycho Wave" should have a cheap cost and AnimationIDGround = 65 ATK_SHOT or similar). Then locate the text table (names) in the file's tail. Put findings in a new `docs/FORMATS-SKILLS.md`.

**Commands Claude Code already ran:** full cache regeneration; `cargo build -p pd_game` (current build: lit characters, P2 coat parked, badge fix).

### Debug tools added this session

| Tool | Use |
|---|---|
| `PD_WALK=<speed>` (viewer) | walks the character along +X so cloth sees real motion; camera tracks |
| `PD_SHOT_COUNT` / `PD_SHOT_EVERY` (viewer & game) | burst of numbered screenshots |
| `PD_CAM_DIST` / `PD_CAM_UP` / `PD_CAM_AHEAD` (game) | close-up camera for inspecting a player |
| `PD_SUN` / `PD_AMBIENT` | character light levels |
| `--no-cloth` (viewer), `--look-y` (viewer) | disable cloth; camera look height |
| `PD_SKIP_DRAWS=a-b` (convert) | leave out body draws — bisect a bad piece |
| `PD_DEBUG_PIN`, `PD_DEBUG_SKIRT` (convert) | print unskinnable vertices / coat detector view |
| `pd_convert uvmap` / `drawinfo` / `joints` / `clothprobe` / `texprobe` / `dumptex` | see PIPELINE.md |

---

## Parked: long-coat cloth for pc01a (2026-09-26)

**Status:** disabled (`skirt::COAT_RIGS` is empty). pc01a's coat moves with his legs exactly as the game data does (it angles back when running — that *is* the original behaviour).

**Laurence's chosen direction when resumed: "Hybrid"** — keep the original leg skinning and add a *gentle extra sway on the hem only*.

**What was learned (don't repeat):**
- Coat vertices are found by **texture area** (UV rect) + skinned to a leg bone **or the hips** (`skirt.rs`, known-good pc01a rect `[0.40, 0.55, 1.0, 1.0]`). Distance-from-bone rules failed (flared trousers are as far out as the coat); height rules failed (hem strips overlap trousers).
- Leg bones = downward segments toward the child with the most subtree skin, owning their childless helper joints' skin. The farthest-child rule picked the spine → collar rigged → "broken neck".
- Fully moving the coat onto free chains from the waist made the whole coat fling out behind when running.

**Hybrid sketch:** keep each coat vertex's original leg/hip weights at, say, 70–100% near the waist tapering to ~60% at the hem; give the remaining weight to short chains that only exist over the lower third of the coat, with heavy damping and no capsule push-out beyond attachment depth. Verify with `PD_WALK=72` side-on bursts and `--no-cloth` comparisons before showing Laurence.

---

## Handoff from Claude Code: split-screen prototype → play-feel tuning (2026-09-26)

**Goal:** Tune the prototype's feel with Laurence's feedback (run speed, jump height/airtime, camera distance/height), no new systems.
**Key decisions:** see DECISIONS.md. Sim constants live in `crates/pd_game/src/sim.rs`; camera numbers in `follow_cameras` in `crates/pd_game/src/main.rs` (60 back, 30 up, look 30 ahead).
**Files touched or planned:** `sim.rs` constants, `main.rs::follow_cameras`. Nothing in `pd_formats`.
**Tests added or planned:** none automated yet. Proof = `pd_game --demo --screenshot` at frames 200 and 600 + Laurence plays via PLAY.bat.
**Risks identified:** building fails while the game is open (ask Laurence to close it). Keep sim.rs free of rendering/time/randomness.
**Next concrete step:** ask Laurence which of speed / jump / camera feels off, change one constant at a time, rebuild `cargo build -p pd_game`, have him replay.
**Commands Claude Code already ran:** full cache regeneration (PIPELINE.md), `cargo build -p pd_game` (current build = jump 75/300, grafted coat tails).
