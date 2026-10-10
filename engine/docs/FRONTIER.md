# FRONTIER — open threads and unbuilt findings (2026-09-28)

> **Archivist, 2026-10-03:** history (open-thread list as of 2026-09-28/30; many items since built). Current open items: `NEXT-SESSION.md` and the newest `WAVES.md` blocks.


## Current acceptance frontier — Laurence, 2026-09-30

**Dedicated Claude passes planned, not launched:** see CLAUDE-DEEP-PASSES.md.
Historical green labels below describe past implementations/research, not a
blanket fidelity approval. Current open milestones:

- P1: capsule **collection and use audio** are still incorrect (separate cases).
- P2: locked strafing + walk/run stick sensitivity/detection close but off.
- P3: **move effects and their animation/timing remain imperfect**.
- P4: directional/depth/atmosphere lighting needs a dedicated pass.
- P5: school aura requires a ground-up modern technique, previous attempts rejected.
- P6: actual four-player architecture/input/targets/views + readable setup/HUD.
- P7: arsenal builder flow/art fidelity to original + multiplayer assignment.
- P8: gamemode rules/design/build/full-match verification pass.

**Accepted:** regular steps common35; grain wall dissolve COMPLETE and approved.
Keep exceptions separate; mapped data, skill dispatch and test counts aren't
equivalent to verified or approved recreation behaviour.

**What this is:** every ❓ and every "we found it, we didn't build it" from the
2026-09-28 lighting + engine research, in one place, with the size of each and
what would close it. Nothing here is claimed as done.

**Rule:** a line only moves to ✅ when it is *built and verified with a
screenshot* (AGENTS.md rule 4) or *proven from the binary with an address*. A
lane that merely wrote code has not closed its line.

Status key: ✅ done · 🔨 in a lane tonight · ⏳ proven, not built · ❓ open question

---

## 🔨 Building tonight (four lanes, disjoint files)

> **⚠️ 00:15 — CONFIRMED DEAD: no lane work happened tonight. Do not retry tonight.**
>
> All four lanes failed with `HTTP 429: The usage limit has been reached` after
> ~11s each, before reading any file. A **single-lane probe** (`deleg_e04dd208`,
> the sky lane alone) then failed identically in **10.95s** — so this is **not a
> concurrency limit**, it is a fully spent quota. Retrying, rewording the briefs
> or re-scaling the wave will all fail the same way. **Zero code was written;
> zero files were touched by any lane.**
>
> **⚠️ Which quota — corrected 00:20.** The batch report shows
> `Model: gpt-5.6-luna`, and `config.yaml` confirms `delegation.model:
> gpt-5.6-luna` / `delegation.provider: openai-codex`. **So the lanes never ran
> on Space Bunny Free at all** — the `delegate_task` worker model is pinned in
> config and **I did not change it**, despite being asked to run the lanes "as
> Space Bunny Free models". The 429 is the **Codex subscription's** quota, not
> the free tier's. Two consequences: (1) `space-bunny-free` may well be perfectly
> healthy and was never exercised, so the free tier is still an untested option
> rather than a proven-dead one; (2) the fix is to **pin the delegation worker to
> `space-bunny-free` / `opencode-free` explicitly** (or wait for the Codex quota
> to reset), not to keep retrying blindly.
>
> The table below is therefore a *plan for a future session*, not a record of
> work. The file fences it specifies are still valid and can be reused verbatim.
> Gates when it is next attempted: (a) a working worker model — either the Codex
> quota resets, or `delegation.model`/`provider` is changed to the free tier and
> a **single** probe lane is run to prove it before any wave; (b) `pd_game.exe`
> closed so a real build + screenshot is possible — `cargo check` alone cannot
> satisfy AGENTS.md rule 4.

| Lane | Item | Size | Files it owns | Status |
|---|---|---|---|---|
| lane-sky | Sky cloud layer UV scroll (`m2_d1`) | S | new `sky_scroll.rs`, `main.rs` mod+schedule only | ✅ **BUILT + VERIFIED 2026-09-29** (90×/297× A/B) |
| lane-capsule-audio | Wire the capsule sounds | S | `sfx.rs` only | 🟡 pickup id was WRONG (34 is a voice-intensity, not a sound) — pickup now silent on purpose; **sounds are in another bank, not `common`**. 79/85/92 open |
| lane-footsteps | Footstep VFX | M | new `footsteps.rs`, `main.rs` mod+schedule only | ✅ BUILT + VERIFIED 2026-09-29 — one surface only (no ground material in our data); 10-frame gap, strafe-aware, cycle-timed. GAME.md "Footsteps" |
| lane-dissolve | Arrival dissolve (proof-first) | M | new `dissolve.rs`, `main.rs` mod+schedule only | ⏳ not started |

`main.rs` is shared: **each lane adds exactly one `mod` line and one entry in
the `Update` tuple, and touches nothing else.** If two lanes collide there, the
later one wins and re-adds the other's line — this is the only allowed overlap.

**Build constraint (2026-09-28 night):** Laurence's `pd_game.exe` was still
running (PID 42200) when these lanes were dispatched. On Windows the linker
cannot replace a running exe, so **the lanes can only run `cargo check`, not
`cargo build`**. ✅ `cargo check` was confirmed working. **No lane has linked,
run, or screenshotted anything.** Every line below marked 🔨 is *written and
type-checked only*. The build + screenshot gate happens first thing next
session.

---

## 🟡 Open threads from the footstep thread (2026-09-29) — measured, not guessed

**Everything here is measured.** Full detail in FORMATS-AUDIO.md "Footstep sound".

1. **The wet footstep sound — SOLVED to a two-candidate shortlist (2026-09-29).**
   The `0x23xxxxx` ("third call = id 35") lead was **read and killed**: all
   three footstep-region emitter calls pass the identical `0x1e0c350` → id 30,
   and byte-level there are only **two** machine sites (one shared tail)
   — FORMATS-AUDIO.md "Third emitter constant" (lane-step). The real
   per-surface variation is a different play call, `FUN_140276330`, fed a plain
   id from the i32 material table `DAT_1402f6270`: water class `0xD` → **52 /
   54**, dust `{5,6,8,0xB,0x15}` → 24/26 …, sand `{3,9,0xC}` → 28/30 ….
   **id 35 is the `+1` variant of the 34 footstep** (`iVar4+1`), which is why
   Laurence heard it as "regular stepping". Ear-check set ready at
   `engine/audition/wetcheck/` (52 vs 54).
   **Laurence's ear verdict (2026-09-29): all four candidates (52–55) sound
   correct; DECISIONS.md records the choice — wet ground plays the water-class
   pair (52/54), cycled per wet step (rotating index, not an unseeded roll, so
   replays stay clean).**
   **WIRED (lane-sounds `2a012ba`):** new `surface_ground.rs` loads the
   `<stage>.surf.json` sidecar and answers `class_below(x, z, max_y)`;
   `footsteps.rs` looks up the class under the stepping player and cycles
   52/54 on water (per-player rotating index), dry 30 otherwise (stages with no
   sidecar fall back to dry everywhere — the honest choice). `PD_SURFACE_LOG=1`
   logs the per-step class/id. **Live proof still open:** the demo never
   crossed a puddle in the lane's run (only classes 32/None → 30 logged), so
   the wet path is currently proven by unit tests, not by a live step.
   **Do NOT ship the 20 % random fake** — the mechanism is known and randomness
   would play the wrong clip at the wrong time.
2. **The surface class byte is the `0x94` hull-chunk FLAGS byte — data we
   already ship (2026-09-29).** The programme's probe `FUN_140256390` returns
   the nearest face's node `+2`, which the loader copies from the **per-hull
   chunk's flags byte** inside `Map/stNN.dat`'s header-`0x94` whole-stage
   collision block — a field no parser ever read (COLLISION.md; probe
   `engine/re/wetzone_probe.py`, 0 mismatches on all 22 ST00 stages).
   `FUN_140230cd0`'s switch (verified in the raw decompile) is sand
   `{3,9,0xC}` / dust `{5,6,8,0xB,0x15}` / water `{0xD}` / 0x14 and 0x1F
   **silent** (the old "gated on 0x14/0x1f" framing was wrong — 0x1F faces are
   skipped by the probe). Lost Palace **st03** holds **32 water-class faces**
   in puddle-sized 50×50 / 30×30 patches at floor level (shared with the st13 /
   st23 recolours). So per-surface footsteps **and** per-surface footstep VFX
   are reachable from converted data. Remaining work is converter + sfx wiring
   (see the frontier's next-waves list), not formats.
3. **The capsule sounds play through `FUN_140276330` — "silent, another bank"
   was wrong (2026-09-29).** The capsule path never calls the *emitter*
   (`FUN_1401cbbc0`) — but it does play audio, through the general SFX call:
   `FUN_14014ed10` → `0x55` = **85**, `FUN_14014ed90` → `0x5C` = **92**,
   `FUN_14014fd10` (take) → `0x5C` = **92** (FORMATS-AUDIO.md "Third emitter
   constant", §D). So the pickup is very likely `common/92` and the orb
   appearing `common/85` — the old doc attributions look swapped. **Ear results
   (2026-09-29): `85` = the orb appearing — CONFIRMED by Laurence; `92` = the
   pickup — corroborated (the take path is the other `FUN_140276330` caller)
   but not yet heard by him.**
   **WIRED (lane-sounds `2a012ba`):** `sfx.rs` plays **92** on
   `CombatEvent::Capsule` (replacing the deliberate no-op), and a new
   `capsule_sfx` system plays **85** when a capsule slot appears (diffing
   `Combat::capsules` frame to frame). Overrides `PD_CAPSULE_PICKUP` /
   `PD_CAPSULE_APPEAR`. Live log: `sfx: capsule appear 85 (6 new orbs)`; the
   pickup itself is proven by unit test (the demo AI never captures — the
   consume lane later found `--demo --start-hand 0` does walk onto orbs, so a
   repeat run could log 92). `common/79` (jump-zone hum, a looping ambient) is
   still open.

---

## 🟡 Laurence's play observations of the original (2026-09-29 evening) — new threads

Reported from playing the installed original, no recording attached. Each line
is an observation to reproduce in our build (measure ours first — some may be
partly there), not a confirmed program reading.

| # | Observation | Ours today | Size | Closes when |
|---|---|---|---|---|
| O1 | **Consuming an aura makes the school aura briefly saturated + higher opacity** | school aura exists (Level ≥ 1, steady); no consume flash | S (render-side; the school-aura shell material + a `CombatEvent` to key the flash on) | screenshot A/B of the flash against his description; his sign-off |
| O2 | **Consuming aura / a green (status-heal) capsule halts movement while the consume animation plays** | **BUILT + COMPLETED 2026-09-29** — lock (`combat.rs` `consume_ticks`) + the real clip. lane-consumeanim `c3bf9cd`: `CONSUME_LOCK_TICKS` = **79** (SPE_POWER_UP/SPE_HEAL are 40 frames @30 fps; 60 Hz sim, length−1) and the character plays **SPE_POWER_UP** (aura) / **SPE_HEAL** (green status) for the whole lock via the existing clip path (`consume_clip_for`/`consume_clip_now`). Aura + green status captures only; hits clear it; no self-extension | S–M | ✅ 4 new tests + **live proof**: `--demo --start-hand 0` captures (the lane's correction to a wrong belief), anim log shows `SPE_POWER_UP` t361→t440 and `SPE_HEAL` t241→t320 / t366→t445, all exactly 79 ticks; frames `shots/consumeanim_*` looked at (distinct raised-arm poses) |
| O3 | **Near-wall character dematerialisation** | **COMPLETE — approved by Laurence, 2026-09-29.** `6694aa0` + `f293227`: `cam_fade.rs` + `dissolve.wgsl` use grain discard, authored Opaque/Mask depth writes and per-view coverage, replacing the live-rejected Blend fade. Accepted defaults: 9→4 units, minimum coverage 0.15, 1px grain. | M | ✅ two wall approach/recovery cycles inspected, exact 1.0 recovery pinned, real ECS conversion/disable/re-enable/reset tests pass; merged suite/build/runtime shader smoke passed. Laurence explicitly requested marking complete. No approval pending. |
| O4 | **Locking on plays a 'lock on' sound and smoothly spins camera & player to face the opponent** | **BUILT 2026-09-29** — lane-lockon `a6cc056`. Measured first: the sim's `cam_yaw` **snapped** ~100° in one tick (the render Transform's lerp hid it only partly), the player's own yaw already eased at 600°/s (no fix needed). Now the locked `cam_yaw` **slews at 180°/s** (the program's 6°/tick, `PD_LOCK_CAM_RATE` override); `Player::lock_on_events` counts toggles for the renderer; the cue is **common id 4** — identified twice (FFT cross-correlation of Laurence's `Lock-On Animation.mp4` = 0.84, and the program's HUD lock-marker updater `FUN_1401536e0` plays `FUN_140276330(0,4,…)` 10 frames after the marker changes), played **on lock-on only** (unlock silence is an unconfirmed reading) | S–M | ✅ code + 5 tests + tick-exact before/after shots (before: swung by t0402; after: 6° of 99°, settling ~t433). **Needs Laurence's ear** on `wetcheck/lockon_candidate_4.wav` (Q3 in LISTEN.txt); 180°/s and the 10-frame cue delay are unconfirmed constants |
| O5 | **id 1 = unpausing** (heard only when P2 unpauses) | **BUILT 2026-09-29** — `Cue::Unpause` (frontend/sound.rs, lane-unpause `b6d6886`) + the `menu.rs` hook: closing the in-match overlay (Esc/Start toggle, or pad East) plays `common/1.wav`; opening and menu-screen closes keep the old Back id 154 | S | **Laurence's ears** — the overlay close can't be driven headlessly, so the trace proof is a unit test (`unpause_plays_common_bank_id_1`); needs one live unpause |

O2 and O4 touch the sim and must respect determinism (tick counters, no wall
clock). O1/O3 are render-only. None of these are in the moves track
(M5–M14) — they slot into the visual/polish cycle.

---

## ⏳ Proven from the binary, deliberately not built

These are real, evidenced, and would be *enhancements* or *out of scope*. Listed
so they are not re-derived or lost.

| # | Item | Why not built | Where |
|---|---|---|---|
| F1 | **Per-material Fresnel screen-space reflection** (`SCREFC_ps`: 1−N·V over a scene fetch, ×4 gain) and the same on characters (`SBOSREFS_ps`, 3 textures) | The original had it; it is a *material* effect, not a global pass. Worth doing only if footage ever shows a stage that needs it. Enhanced tier. | FORMATS-STAGES.md "screen-space reflections" |
| F2 | **Billboard light shafts** (`vkRAYLIGHT`, 14 instrs, no ray loop — a stretched additive quad) | Mechanism fully understood, but *which lights get shafts* is unread, so we'd be guessing placement | FORMATS-STAGES.md, same section |
| F3 | **The 10-band storm schedule** (table `0x1402f5f10`, 10 columns × 10 bands) and the per-band strike durations at `0x1402f5f1c` | Lightning itself is decoded (colour `0xFFDCDCFF`, 4-segment tapering strip, 1-in-10 gate); the *band meanings* are not. Needs the band-select caller. | FORMATS-STAGES.md "Lightning" |
| F4 | **Ground-material table meanings** — now **decoded** (2026-09-29): classes sand `{3,9,0xC}`, dust `{5,6,8,0xB,0x15}`, water `{0xD}`, everything else silent; ids via `DAT_1402f6270` (water → 52/54) | Kept here only until the sfx lane wires it (frontier thread 1/2) | FORMATS-AUDIO.md "Third emitter constant", FORMATS-STAGES.md "Footsteps" |
| F5 | **Camera tracks** `st01cam.alr`…`st08cam.alr` (65 tracks; position ✅, FOV ✅ 48.5–100.4°, dolly-zoom ramp ✅; **keyframe stride unsolved**) | Cutscenes are out of scope per AGENTS.md. Key stride failed at 16/20/12 bytes in 0 of 65 chunks. | FORMATS-ALR.md "Other chunk ids" |
| F6 | **Stage light-record chunk `0x09`** (40-byte record: colour, type, slot, flags, pos, range) | The engine supports it, **no stage file ships one** (0 of 30). A dead feature — decoded for completeness only. | FORMATS-ALR.md, RESEARCH-LIGHTING.md |
| F7 | **Per-draw fixed-function blend state** (the `R`/`C`/`P`/`S` suffixes) | The material's `+0x14` u16 selects it, and the constant-cache writer has **171 call sites** — needs the specific per-draw path, not a text search | FORMATS-EFFECTS.md |
| F8 | **Glow mask surface resolution ratio**, and the `cb0[0..2]` blur weight *values* | Both set on the CPU each frame — **not in the binary at all**. Unreachable statically. | FORMATS-STAGES.md "glow pass" |

---

## ❓ Open questions, in the order I'd attack them

1. **MSPARTICLE's word meanings** — the one item where static RE is exhausted.
   Every class name exists (`MSParticleFunc00–03`, `MSEffectEI`) but the RTTI is
   orphaned (0 of 1,060 descriptors have a `TypeDescriptor` pointer), so name →
   vtable is not walkable and nothing stamps it by name. **Needs one live
   breakpoint on the packed-name compare.** Symptom: "Psycho Knife's hit puffs
   read as a big white cloud"; treat our current MSPARTICLE numbers as wrong.
2. **`MSEffectEI`** — a class name that appears in no doc. Obvious
   "EffectEmitter/EffectInfo" candidate; the cheapest next class to open if
   MSPARTICLE is wanted.
3. **Per-material blend state** (F7) — only refines dark-skill blending; the
   visual outcome is already close.
4. **Which materials select `SBOSREFS`/`P`/`C`** — needs the model data's
   material flag, not read.
5. **The dissolve's actual `cb0[16].x` driver** — per-frame or skill-driven?
6. **`pk` / red-glow colour source** — the school aura is built, but the
   *program's* colour source is still unconfirmed (the `0x1402dca00` table turned
   out to be the capsule colour table, not the school one — confirmed negative).
7. **`.w10` voice/tutorial audio** — never examined.
8. **ALR chunk ids `0x12`/`0x13`/`0x14`** — the per-draw effect/shader bindings
   `0x14` is half-read; `0x12`/`0x13` are untouched.

---

## ⏳ Cheap items that would pay off (not frontier — just small)

- `vkFRESNEL`, `SBOSREFC*`, `SBOSHDR` shaders are read-pending (all in the
  `alsh_Shader*` inventory in FORMATS-STAGES.md). Each is ~15 min of disassembly.
- Capsules: the program's real colour table is **known** (attack `FF3333`,
  defence `3333FF`, erase `FF33FF`, status `33FF33`, special `FFFF33`,
  environment `33FFFF`, white for the "none" slot) and lands in
  `CAPSULE-VISUALS.md` whenever a lane next touches that file.

---

## ⏳ State after 2026-09-29 (supersedes the ranked list in archive/AUDIT-2026-09-29.md)

**Done since that audit:** camera close-in framing fixed (two regressions of mine found and fixed), camera height added as a 5-way settings row, camera pitch limits decoded from `configdata.dat`, hit-stop + KO slow-motion shipped (`PD_HITSTOP=1`), anti-aliasing settled at a fixed 4× MSAA with the toggle parked.

**Parked deliberately:** sound (needs a frontier model for the bank/variant resolve), orbs/capsule glow (the original has no glow-sprite system on the capsule path — see the H4 handoff).

**Ranked remainder, in order of what a player notices:**

1. **Skill family 100** — **52 skills** carry `FAMILY == 100` and `moves/mod.rs` has no case for it. Not 10 skills as the audit's placeholder count implied: 18 of them are *movement/state* skills (Dash, Jump, Levitate, Rebirth…), a different subsystem from `behaviour_of`. **Scope first** — measure how many of the 52 are reachable in a real arsenal before building.
2. **Verify the Setup→Match transition.** Two bugs reported from real play (Setup screen over the countdown; crosshair/nametag stamping) with `Msaa` identified as the cause. The toggle is parked so the bad state is unreachable, but **nobody has seen it fixed**, and the headless harness structurally cannot reach that path.
3. **Destructible stages** — st04 `yuka`, st06 collapse, st08 blocks. Needs the OA01/OE00 decode.
4. **Capsule glow sprites** — reclassified: not a missing feature, the original doesn't have one. Only worth revisiting as rim opacity on the existing decoded shader.
5. **Per-prop throw strength** — currently derived from object size, an assumption in a shipped behaviour.

Full detail, including the three traps that cost the most time today, is in `archive/handoffs/H4-post-audit-camera-aa.md`.

---

## Provider state (2026-09-29 00:20) — why no lane work happened

**Two independent provider faults, both verified by direct command, not inferred:**

1. **The Codex quota is spent.** `delegation.model: gpt-5.6-luna` /
   `provider: openai-codex` (`AppData/Local/hermes/config.yaml`). All **six**
   dispatch attempts (4 lanes + 1 unpinned probe + 1 probe with a per-task
   `delegation: {model: space-bunny-free, provider: opencode-free}` pin) failed
   `HTTP 429` on the **first API call**, ~11–12s each, every one reporting
   `Model: gpt-5.6-luna`. ⚠️ **The per-task pin is ignored** — the worker model is
   taken from config, so there is no way to redirect a lane off Codex from the
   call site. Direct probe: `hermes -p researcher chat -q "Reply with exactly OK"`
   → same 429. **The free tier was never exercised, so it is untested, not proven
   dead** — but it is also not reachable through `delegate_task` as configured.
2. **The default profile's model 404s.** `hermes chat -q ...` (default profile)
   → `HTTP 404: Model not found, inaccessible, and/or not deployed`, for
   `model.default: accounts/fireworks/models/kimi-k2p6`. ⚠️ **Any session not
   explicitly pinned to a working provider will fail on startup.** Needs fixing
   before the next lane wave. (The working session on 2026-09-28/29 was on
   `fireworks/kimi-k2p6` via a *different* path, which is why the coordinator kept
   working all night while every child died.)

**Bottom line for the next session:** the four builds below are ready to go and
are NOT blocked on anything in this project. They need (a) a working worker
provider, and (b) `pd_game.exe` closed so a real build + screenshot can satisfy
AGENTS.md rule 4. Fixing (a) may mean pointing `delegation.provider` at
`opencode-free`/`space-bunny-free` in config — **in the config file, not per
call.**

---

## Overnight log

- **2026-09-28 ~23:00** — four build lanes dispatched, files fenced as in the
  table above. ⚠️ **The log line originally said "as Space Bunny Free
  (`space-bunny-free` / `opencode-free`)" — that was WRONG** and is corrected
  above: they ran on `gpt-5.6-luna` / `openai-codex` because the delegation
  worker model is pinned in config and I did not change it before dispatching.
  State captured: `36e0f60` ("Sidestep hand-off keeps the facing continuous").
  Build gate deferred because `pd_game.exe` was still running.
- **2026-09-28 ~23:30** — RE continued in parallel (no conflict with the lanes).
  Closed **ALR chunk `0x12`**: a per-stage **cull volume** — float3 position,
  a radius (117–4133 in st08, 1.45–112 in st09), and a u16 **object index** into
  the global transform table; handler `FUN_140295a70`, and the test it calls
  (`FUN_140293140`) is a multi-plane frustum reject. 250 records, in `st08` and
  `st09` only. Written up in FORMATS-STAGES.md.
- **New item for this list, from the same session:** ALR chunk **`0x13`** — 1,789
  instances, fixed **136-byte** body, sits between a model's vertex-buffer table
  and its index buffers, handler `FUN_140295d90`. The handler reads a **u16 ×
  0x4C index into the material table** and does a 256-entry class lookup, so the
  block almost certainly carries a **per-model material assignment** — but that
  is inference from the stride, not read. `alr.rs` has no `ID_*` constant for it,
  which is why nobody has ever looked. **This is now the best next RE target**:
  one handler address, one fixed record size, and a plausible reason it matters
  (it is the only block between geometry and indices). Written up in
  FORMATS-ALR.md "0x13".
