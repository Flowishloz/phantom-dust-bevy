# Plan of action

> **Archivist, 2026-10-03:** history. "Planning only, not launched" below is out of date: P0-P8 ran as waves 1-6 and later waves. Current: `NEXT-SESSION.md`, `WAVES.md`. The online / replay / mode sketches (5A-5C) remain useful design input; online is being merged (wave 2026-10-03).


## Active planning brief — 2026-09-30

Laurence requested **dedicated heavy, deep orchestrated Claude passes** after
evaluating the recreation. Use **CLAUDE-DEEP-PASSES.md P0–P8** as the current
execution specification; ROADMAP's current table is its summary. This is a
plan, not an instruction to launch premium work without the next go-ahead.

Open passes: capsule collection/use sound, joined movement sensitivity/strafe,
move effects and animation fidelity, directional lighting, ground-up modern
school aura, four-player architecture/UI, original-like arsenal builder, and
gamemode design/build. Most have implementations, not user-approved fidelity.
P0 reconciles actual code and measured references; then bounded Claude lanes,
high-effort architecture/research, isolated worktrees and independent critics.
Do not fan out overlapping main/combat/effects/frontend work simultaneously.
Preserve approved dissolve and common35 steps. School aura and the old moves
track remain parked until explicitly entered under this plan.

The old waves below are evidence/backlog references. They contain superseded
timing reads and already-built tasks: **do not execute them wholesale**.

Turns `ROADMAP.md` (esp. "Future features (agreed 2026-09-27)"), the `HANDOFF.md` backlog, and `DECISIONS.md`
into ordered, non-overlapping lanes. Written read-only by the planning lane (2026-09-27); does not change code.

**Not replanned here** (in progress as of this writing): Lost Palace collision fix (`pd_formats/src/collision.rs`,
collision parts of `gltf.rs`, stage-collision in `pd_convert/src/main.rs`, `docs/COLLISION.md`) and melee/projectile
effect visuals (`pd_game/src/effects.rs`, `fx.rs`, `sfx.rs`, clip selection in `pd_game/src/main.rs`, `docs/MOVES.md`,
`docs/FORMATS-EFFECTS.md`). Wave 1 assumes both are merged.

Standing constraints applied throughout: **no unlocks/progression of any kind**; `sim.rs` / `combat.rs` / `moves/*` /
`props.rs` stay deterministic (no time-of-day reads, no unseeded RNG, no trig-heavy facing math added — see
"Online prep" below); anything cosmetic renders off sim state and never writes it; **online work does not start**
until the items under "Phase 4 — online prep" are done and Laurence has played a proven local build.

## Moves track — rebuild from the game's program (added 2026-09-27; runs before the waves below, supersedes Wave 1B)

The original's move rules were read from its program (`docs/PROGRAM-RE.md`, checked against Laurence's recordings in `docs/FOOTAGE.md`). Laurence chose this route; new hand-tuned move work is paused. Build in this order (each lane proves itself against FOOTAGE.md and the table numbers):

| Step | Outcome | Depends on | Files | Size | Model |
|---|---|---|---|---|---|
| M1 | Action table, reaction table and s70–s73 exported into `skills.json`; converter also writes emitter textures (`a07ac685*`, `90e7fd3e*` kinds) so a fresh convert keeps projectile sprites | none | `pd_formats/src/skills.rs`, `pd_convert/src/main.rs` (skills + effects commands), `docs/FORMATS-SKILLS.md` | S | Sonnet |
| M2 | Animations play at 30 fps (one keyframe per 30 Hz tick), not 24; 5-frame blends | none | `pd_formats/src/character.rs` (`FPS`), clip drive in `pd_game/src/main.rs`, `docs/FORMATS-ANIMATION.md` | S | Sonnet + Laurence's eye |
| M3 | Cast timing from the action row: per-skill fire tick, caster held for the whole clip, a hit before the fire tick cancels the cast; melee dash/hit windows from the row (replaces `CAST_TICKS`, `fx::STRIKE_LEAD_FRAMES`) | M1 | `pd_game/src/combat.rs`, `moves/melee.rs`, `fx.rs`, `docs/MOVES.md` | M | Sonnet |
| M4 | Hit reactions light/medium/heavy by recent damage, air hits knock down, knock-down launch/lie/get-up and 2.5 s blinking invulnerability | M1 | `combat.rs` (reactions), `pd_game/src/main.rs` (reaction clips) | M | Sonnet |
| M5 | Range bands (s44), guard shapes/durations/STR-vs-DEF/Pierce rules | M3 | `moves/mod.rs`, `moves/barrier.rs`, `combat.rs` (guard) | S–M | Sonnet |
| M6 | Per-family motion fixes (lob s66/s67 swap, diagonal Fall, Tornado no homing, Rain cloud, crawl turning, mines hurt owner, Ricochet X, multi-laser SHOTnn markers, homing ×0.52, speed 2.78 u/s per point) | M3 | `moves/*.rs` one family per lane | S each (melee, Rain: M) | Sonnet |
| M7 | One more Opus reading session: jump/dodge/dash states, combos, capsule effect bodies, root motion, packed effects 300–321 | none | `docs/PROGRAM-RE.md` only | M | Opus |

**Status 2026-09-27 night:** M1 (tables in `skills.json`), M2 (30 fps, Laurence approved) and M7 (phase 3 reading) are done and merged. Phase 3 added these build lanes (PROGRAM-RE.md "Phase 3"; GLM/Sonnet-sized):

| Step | Outcome | Files | Size |
|---|---|---|---|
| M8 | Converter loads motions from the colour-variant `pcXXa<colour>.alr` files the game uses, plus a per-clip root-drift table; `chars.rs` dodge split uses /30 | `pd_convert` (`main.rs` character command, `chars.rs`), `pd_formats/src/character.rs`, `anim.rs` comment, `PIPELINE.md` | S–M (Laurence's eye) |
| M9 | All 496 packed effects exported from `Effect/e.i` + `a*.e`; skill→effect from action row w2 (drop family borrowing) | new `pd_formats` reader, `pd_convert` effects command, `pd_game/src/effects.rs` + `fx.rs` (`SkillFx::load`) | S |
| M10 | Movement model: walk/run two speeds with stick thresholds, 20°/tick turn + turn slowdown, gravity 0.12 u/tick² + fall cap, landing by fall height, locked strafing | `sim.rs`, clip choice in `pd_game/src/main.rs`, `GAME.md` | M |
| M11 | Dodges and movement skills as the hidden skills 750/751 (+ Jump, Dash, Skywalk, Levitate) | `sim.rs` dodge constants, `moves/` specials, `combat.rs` | S–M |
| M12 | Root motion in the sim for cast and reaction states | `combat.rs`, `sim.rs` (after M8) | M |
| M13 | Aura refill curve + pauses, 14 s capsule respawn at fixed CP00 spots, 30-card deck | `combat.rs` | S |
| M14 | Combos (w45–w47), charge release 12 ticks, melee 80 u lock gate, Change the World swap, status kind table | `combat.rs`, `moves/erase.rs`, `moves/` specials | S |

**Done 2026-09-27:** M1, M2, M3, M4, M7, M8, M9, M10 merged (HANDOFF top entry); **remaining: M5, M6, M11, M12, M13, M14.** Sim lanes (M3, M4, M5, M10–M14) mostly share `combat.rs` / `sim.rs`: run at most two at once and split by function, or run them in sequence. Suggested order: M8 + M9 (converters, parallel) with M3; then M4 + M10; then M5, M11, M12; then M13, M14, M6.

M1 + M2 + M7 can run in parallel (no shared files). M3/M4 touch `combat.rs` — run them one after the other, or split by function with care.

## Waves

Each wave = up to 3 lanes, no file overlap within the wave. Later waves may depend on earlier ones (noted).

### Wave 1 — small faithfulness fixes (ready once collision + effects lanes merge)

| Lane | Outcome | Depends on | Files | Size | Model | Proof |
|---|---|---|---|---|---|---|
| 1A | Non-versus arenas (`st01` "Edgar's Dream", `st24` "Panorama Boss") no longer offered on the setup screen | none | `pd_convert/src/main.rs` (stages command, `versus` flag ~L780-912), `docs/FORMATS-STAGES.md` | S | Sonnet | regenerate `cache/stages.json`, screenshot of setup screen stage list missing both |
| 1B | Laser-family skills fire ~1.26 s after cast instead of 0.4 s (sound evidence vs. `CAST_TICKS`); other families reviewed against the same guide-text timing | none | `pd_game/src/combat.rs` (`CAST_TICKS` and per-family timing), `pd_game/src/moves/erase.rs`, `moves/special.rs` (laser call sites), `docs/MOVES.md` | S/M | Sonnet | unit tests on cast-to-fire tick count; before/after screenshot burst of a laser cast |
| 1C | "Cartwheeling halts Aura regeneration" rule built; camera shake plays from stage break records (`stNN.objfx.json`) as a render-only offset | none (shake needs effects lane merged for `objfx.json` shape) | `pd_game/src/sim.rs` (cartwheel/aura interaction), `pd_game/src/main.rs` (`follow_cameras`), `docs/DECISIONS.md`, `docs/MOVES.md` | S | Sonnet | test for aura regen pause during cartwheel; screenshot/video-frame burst of camera shake on an object break |

Cost: 3 Sonnet lanes ≈ low.

### Wave 2 — arena/effects leftovers + Laurence's controls report

| Lane | Outcome | Depends on | Files | Size | Model | Proof |
|---|---|---|---|---|---|---|
| 2A | Controls diagnosed and fixed from `engine/shots/input_log.txt` (jump / right-stick camera reports) — **blocked until Laurence runs `tools/bats/PLAY_DEBUG_CONTROLS.bat`** | Laurence's recording | `pd_game/src/sim.rs`, input handling in `pd_game/src/main.rs` | S | Sonnet (Opus only if the log shows something structural) | before/after input log + screenshot |
| 2B | contaB collapse animation + hull (`OA01`/`OC01`); melee hits register on objects; rubble falls on players near a break | Wave 1 shake lane merged (shares `props.rs`/objfx read path) | `pd_game/src/props.rs`, `props_view.rs`, `pd_formats/src/stagedat.rs` (OA01/OC01 parsing), `docs/FORMATS-STAGES.md`, `docs/GAME.md` | M | Opus (new binary record layout to decode) | screenshot burst of a contaB break; test for melee-vs-object damage |
| 2C | Remaining stage-effect gaps: per-piece blend modes (ice pieces currently solid white), Aura Particle effect (600–609), crater decals (`mdl_hit_*`), ambient jump-zone hum, the 5 unknown `EF00` record kinds | Effects lane merged | `pd_formats/src/effect.rs`, `pd_formats/src/stage_effects.rs`, `pd_game/src/effects.rs`, `pd_game/src/fx.rs`, `docs/FORMATS-EFFECTS.md` | M | Opus (unlabelled binary records) | before/after screenshot sheet per fixed effect |

Cost: 1 Sonnet + 2 Opus lanes ≈ medium-high (2C and 2B are both reverse-engineering).

### Wave 3 — MOVES.md stragglers + camera polish (bounded, cheap)

| Lane | Outcome | Depends on | Files | Size | Model | Proof |
|---|---|---|---|---|---|---|
| 3A | Necronomicon, Diffused Laser, extra fall damage after an air hit, knock-back distance, Photon Barrier's Retain built | none | `pd_game/src/moves/special.rs`, `moves/erase.rs`, `moves/status.rs`, `docs/MOVES.md` | M | Sonnet | unit tests per skill |
| 3B | Camera no longer pulls in too close in tight corridors (st03/st04/st13) | none | `pd_game/src/main.rs` (camera occlusion), `docs/GAME.md` | S | Sonnet | before/after screenshots, all three stages |
| 3C | Status icons rendered from the menu pixel font instead of text tags where an icon glyph exists; arsenal builder panel-bar art matched to `DckMenu.alr` | none | `pd_game/src/hud.rs`, `pd_game/src/frontend/builder.rs`, `docs/HUD.md` | S | Sonnet | screenshot of HUD status row + builder screen |

Cost: 3 Sonnet lanes ≈ low.

### Wave 4 — research prerequisites (do before the roster/costume features below)

| Lane | Outcome | Depends on | Files | Size | Model | Proof |
|---|---|---|---|---|---|---|
| 4A | `pcNNa0–3.alr` costume/colour variant files decoded: confirm whether they are separate costumes or the per-draw visibility flag needed to fix the Hero's short jacket (HANDOFF backlog #2) | none | `pd_formats/src/alr.rs`, `pd_formats/src/character.rs` (read-only probe first), new findings in `docs/FORMATS-ALR.md` | M | Opus (reverse-engineering) | probe output + A–F calibration sheet of jacket candidates |
| 4B | `enm03a`, `enm12a`, `evpc00a` decoded enough to convert; character id → display-name mapping confirmed for a real roster list | none | `pd_convert/src/chars.rs`, `pd_formats/src/character.rs`, `pd_formats/src/charaset.rs`, `docs/FORMATS-ANIMATION.md` | M | Opus | converted models render in viewer; roster names screenshot |

Cost: 2 Opus lanes ≈ high. **This wave gates Wave 5's roster items.**

### Wave 5 — party/couch features (all render/frontend/setup only, no sim rule changes beyond named constants)

| Lane | Outcome | Depends on | Files | Size | Model | Proof |
|---|---|---|---|---|---|---|
| 5A | Randomize Arsenal button (seeded, honours 30-slot/3-copy/school legality + aura-rate curve) | none | `pd_game/src/arsenal.rs`, `pd_game/src/frontend/builder.rs` | S | Sonnet | test: 100 seeded rolls all pass the legality checker; screenshot |
| 5B | Brawl mode: 4 fixed infinite-use skills, fixed aura with no refill, no capsule pickups — a `combat::Rules` variant, not a new game mode | none | `pd_game/src/combat.rs` (`Rules`), `pd_game/src/frontend/setup.rs` | S/M | Sonnet | test: capsules never spawn, skills never run out, aura never refills in Brawl; screenshot of a full match |
| 5C | Replays: record the deterministic input stream per match, play it back through the same sim | none (sim is already deterministic; this only adds a recorder/player around it) | `pd_game/src/main.rs` (match loop), new `pd_game/src/replay.rs`, `docs/GAME.md` | M | Sonnet (mechanical once sim is deterministic) | test: replay of a recorded match reproduces the identical final state; screenshot |

Cost: 3 Sonnet lanes ≈ low-medium. 5C is worth doing early — it doubles as an online-readiness check (if a replay ever desyncs, the sim has a hidden non-determinism to fix before online starts).

**Design docs ready (2026-09-27):** 5B → `docs/DESIGN-BRAWL.md` (skill selection, aura/capsule `Rules` flags, UI entry, decisions for Laurence). 5C → `docs/DESIGN-REPLAYS.md` (file format, `Match::checksum()`, the sim's actual trig call sites that risk cross-platform determinism, playback UI). Both lanes can start from these without further research.

### Wave 6 — roster & front-end depth (needs Wave 4)

| Lane | Outcome | Depends on | Files | Size | Model | Proof |
|---|---|---|---|---|---|---|
| 6A | 3D character selector: live animated model in the picker, rotate left/right (reuses viewer's spawn/anim code) | Wave 4B | `pd_game/src/frontend/discovery.rs`, `frontend/ui.rs`, `pd_viewer/src/main.rs` (shared spawn helper) | M | Sonnet | screenshot/video of picker rotating |
| 6B | P1/P2 tint so both players can pick the same fighter; stage-music picker + custom music from `userdata/` | none (music picker) / Wave 4A helps tint if jacket fix changes material setup | `pd_game/src/main.rs` (material tint), `pd_game/src/frontend/setup.rs`, `pd_formats/src/sound.rs` | S/M | Sonnet | screenshot of same-character match with tint; music picker screenshot |
| 6C | Announcer calls from the decoded voice system | none | `pd_game/src/sfx.rs`, `pd_formats/src/sound.rs` | S | Sonnet | audio clip list + in-match screenshot with `PD_SFX_LOG=1` |

Cost: 3 Sonnet lanes ≈ low-medium.

### Wave 7 — feel & presentation pass (render-only; can run anytime after Wave 1, listed here for budget grouping)

| Lane | Outcome | Depends on | Files | Size | Model | Proof |
|---|---|---|---|---|---|---|
| 7A | Modern lighting (bloom on capsules/effects, soft shadows, per-stage mood grade) — render-side only | none | `pd_game/src/stage_look.rs`, `pd_game/src/fx.rs`, `docs/FORMATS-STAGES.md` | M | Sonnet (Opus if Bevy 0.19's PBR/bloom API fights back) | A–F calibration sheet per stage mood |
| 7B | Modern particle layer (dust, sparks, ambient stage life) layered on top of the game's own authored effects | none | `pd_game/src/fx.rs`, `pd_game/src/effects.rs` | M | Sonnet | screenshot/video comparison |
| 7C | Hit-stop + KO slow-mo (render-side time-scale only; sim ticks stay real-time and deterministic — freeze the *view*, not the *sim*, so online isn't affected); win poses/entrances from existing clips; post-match stats screen (fun stats only) | none | `pd_game/src/fx.rs`, `pd_game/src/main.rs` (render time-scale), `pd_game/src/frontend/result.rs` | M | Sonnet | screenshot/video of a KO with slow-mo; stats screen screenshot |

Cost: 3 Sonnet lanes ≈ medium (7A may need Opus if the API is awkward — flag if it stalls).

**Deferred pending a decision (see questions below), not yet waved:** IK (foot planting/lean/head tracking),
mantling/vaulting + coyote time/jump buffering (these touch `sim.rs` — deterministic, so they need an Opus design
pass even though the feature itself is small), stage variants (night/weather), winner-stays-on rotation, 2v2 +
resuscitation, Skate mode, Prop Hunt, general mod support, stage editor-lite. Sized below under "Large/uncertain
items" once Laurence has picked which of these he actually wants next — building all of them without prioritisation
would blow the budget.

## Large/uncertain items (need a design doc before waving)

| Item | Why it's not waved yet | Rough size | Model |
|---|---|---|---|
| Mantling/vaulting, coyote time, jump buffering | Touches deterministic `sim.rs`; "automatic, no new button" still changes movement resolution — needs an Opus pass to get the state machine right before Sonnet can tune it | M design + S/M implementation | Opus then Sonnet |
| IK (foot planting, lean, head tracking) | Cosmetic/render-only in principle, but needs a real rig/animation-blend design in Bevy 0.19; easy to get janky | M | Opus for the approach, Sonnet to implement once decided |
| Stage variants (night/weather grades) | Needs research into whether the light-script format supports alternate grades or if it's a hand authored recolour | M research + M build | Opus (research) then Sonnet |
| 2v2 with resuscitation | New views, teams, lock-on switching (left trigger); touches `combat.rs`, `sim.rs`, camera, HUD, front end all at once | L | Opus, design doc first |

**Design doc ready (2026-09-27):** `docs/DESIGN-2V2.md` — split layout options, teams, lock-on cycling (the original's own team-mate exclusion and target-cycle button, read from the program), the revive proposal, the concrete list of `[Player; 2]`/`[Fighter; 2]` sites that need generalising, phases, risks, decisions for Laurence. Still Opus-sized once a lane starts (the generalisation touches nearly all of `combat.rs`).
| Winner-stays-on rotation (2 pads, 3-4 players) | Not yet confirmed as wanted; changes match flow and front end | M | Sonnet once confirmed |
| Skate mode | New sandbox physics mode; explicitly must never touch the versus game — best kept in its own crate/module | L | Opus, design doc first |
| Prop Hunt | New gamemode reusing the existing PK throw; needs hide/seek state machine in the sim | L | Opus, design doc first |
| General mod support (`mods/` folder, custom gamemodes/skills, arsenal legality checker validation, custom rigged fighters) | Large surface area, security/stability implications (loading external data) | L | Opus, design doc first |
| Stage editor-lite | Needs a real in-game editing UI over the existing prop library | L | Opus for the approach, Sonnet for grunt UI work |

## Phase 4 — online prep (after the local game is proven; do not start building netcode before this)

From `NETCODE-RESEARCH.md` (referenced in `HANDOFF.md`) and the "Online" line in ROADMAP.md:
1. Replace `atan2`/`sin`/`cos` facing math in `sim.rs`, `combat.rs`, `moves/*` with unit-vector facing (no trig) —
   required for cross-platform determinism.
2. Move any `f32` seconds-based timers (e.g. `land_timer`) to tick counts.
3. Add `Match::checksum()` and `serde` on `Input` for desync detection and replay/rollback.
4. Then: lockstep vs rollback (`ggrs` 0.13 candidate), transport (iw4L-style UDP p2p + QUIC introducer).

This is Opus work, large, and explicitly **after** Laurence has played a proven local build (roadmap order).
Wave 5C's replay feature is a good early signal: if a replay ever desyncs, that's a determinism bug to fix
before any of this phase starts.

## Research-first flags

- **`pcNNa0–3.alr` costume variants** (Wave 4A): unknown whether they are alternate costumes/colours or hold the
  per-draw visibility flag that would fix the Hero's jacket (currently shows long flat flaps instead of the
  original's short jacket + lower harness — HANDOFF backlog #2, `docs/HANDOFF.md`). Research this before building
  the 3D character selector or "convert the 4 variant models" roadmap line, since the jacket fix may ride along.
- **`enm03a`, `enm12a`, `evpc00a`** (Wave 4B): need decoding before they can go in any roster list.
- **contaB collapse (`OA01`/`OC01`)** (Wave 2B) and the 5 unknown `EF00` record kinds (Wave 2C): unlabelled binary
  formats, evidence-first probes required (see `.claude/skills/pd-reverse-engineer`).
- **Stage light-script variants** (deferred item): unknown if night/weather grades exist in the data at all —
  needs a probe before committing to a build size.

## Rough cost per wave (budget ~£60/month total; an Opus lane is the expensive unit, Sonnet cheap)

| Wave | Composition | Relative cost |
|---|---|---|
| 1 | 3 Sonnet | Low |
| 2 | 1 Sonnet + 2 Opus | Medium-high |
| 3 | 3 Sonnet | Low |
| 4 | 2 Opus | High |
| 5 | 3 Sonnet | Low-medium |
| 6 | 3 Sonnet | Low-medium |
| 7 | 3 Sonnet (1 may need Opus) | Medium |
| Large items | mostly Opus design docs first | High if all pursued — pick 1-2, not all |
| Online prep | Opus, large | High (not yet, by design) |

Suggested pacing: Waves 1-3 back to back (cheap, high-value cleanup), then Wave 4 (needed research, unlocks
roster features), then Waves 5-7 interleaved as budget allows, picking at most one "Large/uncertain" item at a
time with Laurence's sign-off before spending an Opus design-doc lane on it.

## Questions for Laurence

1. **Which large feature next, after Waves 1-6?**
   - A: 2v2 with resuscitation — the co-op/competitive depth people ask for most in couch games. (Recommended)
   - B: Skate mode — a fun toggleable sandbox, fully separate from versus, lower risk of breaking anything.
   - C: Prop Hunt — reuses systems you already have (Psycho Kinesis throw), quick novelty payoff.
   - D: Mod support — biggest long-term value (custom fighters, gamemodes) but the largest, least certain scope.

2. **Winner-stays-on rotation (3-4 players, 2 pads) — build it or drop it?**
   - A: Build it — couch nights often have more than 2 people wanting a turn. (Recommended)
   - B: Drop it for now — focus stays on 1v1/2v2; revisit after 2v2 ships.
   - C: Not sure — hold as a backlog idea, no lane yet.

3. **Stage variants (night/weather grades) — worth a research pass?**
   - A: Yes, research first (Wave "research-first" item) — cheap to find out if the data even supports it.
   - B: No — the 19 arenas' base look is enough for now.
   - C: Only for the two Palace variants (a smaller, cheaper first step). (Recommended)

4. **Hit-stop / KO slow-mo (Wave 7C) — how strong?**
   - A: Subtle (a few frames of hitstop, brief slow-mo on KO only) — closest to the original's feel. (Recommended)
   - B: Punchy (longer hitstop on every hit, dramatic KO slow-mo) — more "modern fighting game" feel.
   - C: Skip slow-mo for now, keep hitstop only.

5. **IK and mantling (Wave "deferred") — worth the Opus design cost now, or later?**
   - A: Later — ship the cheaper party features (Waves 5-6) first, revisit once those land. (Recommended)
   - B: Now — these are the biggest "feel" upgrades and worth doing before more content features.

## Contradictions found between the docs

- **Rounds vs. single battle.** `DECISIONS.md` (2026-09-27, "Match rules default to the original's") sets rounds
  to default to **1** (a single battle) per `MATCH-RULES.md` fan-guide evidence, and ROADMAP's "Done since" section
  confirms rounds exist as a setting. But `HANDOFF.md`'s 2026-09-26 entry lists "rounds / best-of not found —
  ours has rounds; keep as an option, default to 1?" as still an open question at that time — it reads as unresolved
  there even though the later DECISIONS.md entry closes it. Not a real conflict (the later decision supersedes),
  but worth flagging since a lane skimming only HANDOFF.md's older entries could reopen it.
- **Time-out result.** `HANDOFF.md`'s "Handoff for a fresh chat" entry (older) says "time-out probably a **draw**"
  per `MATCH-RULES.md`, while `DECISIONS.md`'s 2026-09-27 "Match rules default to the original's" entry says
  "Time-out keeps 'healthier player wins'" citing Laurence directly. These disagree; DECISIONS.md is newer and
  cites Laurence by name, so it should win, but the two docs still contradict each other in plain reading — worth
  a one-line fix in HANDOFF.md's older entry (historical record, not urgent) or at least awareness for any lane
  that reads the older entry first.
- **st01/st24 versus status.** ROADMAP's "Open questions" still asks whether `st01` and `st24` are versus-selectable,
  while `HANDOFF.md` backlog item 5 already states flatly "st01/st24 are flagged not versus-selectable in
  `stageset_en.dat`" and just needs the code flag flipped (Wave 1A above). ROADMAP.md's open question is stale and
  should be removed once Wave 1A lands.
