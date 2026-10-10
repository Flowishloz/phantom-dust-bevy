# Engine docs index

> **Public repository note.** Older history docs mention a few files that are kept private and are not in this repository (agent skill files, personal handoffs, third-party research notes, notes on the Xbox disc build). Those links are dead on purpose. Start at the repository [README](../../README.md), [LEGAL.md](../../LEGAL.md) and [docs/WHAT-WE-FOUND.md](../../docs/WHAT-WE-FOUND.md).

Map of every doc. **Status:** *current* = kept up to date, trust it; *reference* = stable facts, change only on new evidence; *log* = append-only, newest first; *history* = dated, read for background only (a banner at its top points to the current doc). One owner per topic: see [ARCHIVIST.md](ARCHIVIST.md). Start a session with `AGENTS.md`, then [NEXT-SESSION.md](NEXT-SESSION.md).

## Where we are and what is next

| File | Covers | Status |
|---|---|---|
| [NEXT-SESSION.md](NEXT-SESSION.md) | top block = state, scorecard, open wave, next steps; dated blocks below it are history | current (top) |
| [WAVES.md](WAVES.md) | every lane: cause, files, tests, UNCONFIRMED residue | log (lanes own it) |
| [BUG-GRAVEYARD.md](BUG-GRAVEYARD.md) | every Laurence bug report: cause, fix, pinning test | log (lanes own it) |
| [DECISIONS.md](DECISIONS.md) | why things are the way they are; who decided | log |
| [ARCHIVIST.md](ARCHIVIST.md) | doc ownership, post-merge checklist, conventions | current |
| [handoffs/](handoffs/) | `WAVE-1003-COMMON.md` (lane rules, wave 2026-10-03), `WAVE-1003-STATE.md` (wave 2026-10-03 state and its to-do list; tracked since 2026-10-03, wave merged), H3 school aura, H5 orb ownership, H6 Xbox ISO | current / reference |
| [handoffs/WAVE-1008-CLOUD-HANDOFF.md](handoffs/WAVE-1008-CLOUD-HANDOFF.md) | what the 2026-10-07/08 cloud sessions built on branch `claude/laughing-fermi-91wzrp` and the PC steps to merge, test, measure, screenshot and hand it to Laurence | done (merged 2026-10-08) |
| [WAVE6-BRIEF.md](WAVE6-BRIEF.md), [LANE-BRIEF.md](LANE-BRIEF.md), [SIM-LANE-BRIEF.md](SIM-LANE-BRIEF.md) | shared lane rules (tags READ/MEASURED/LIVE/UNCONFIRMED, fences, report format) | reference (WAVE6-BRIEF current; the other two are templates from waves 1-4) |
| [PC-LANE-BRIEF.md](PC-LANE-BRIEF.md) | the session on Laurence's PC (Xbox ISO / live recording) | reference |
| [HANDOFF.md](HANDOFF.md) | lane-to-lane handoffs, top = end of wave 6 | history |
| [ROADMAP.md](ROADMAP.md), [PLAN.md](PLAN.md), [FRONTIER.md](FRONTIER.md), [CLAUDE-DEEP-PASSES.md](CLAUDE-DEEP-PASSES.md), [MOVES-PLAN.md](MOVES-PLAN.md) | 2026-09 plans and open-thread lists (P0-P8 ran as waves 1-6) | history |
| [archive/](archive/) | `HANDOFF-ORCHESTRATION.md` (2026-09-27), `AUDIT-2026-09-29.md`, `handoffs/` H1, H1b, H2, H3-polish, H4 | history |

## Build, convert, verify

| File | Covers | Status |
|---|---|---|
| [TOOLCHAIN.md](TOOLCHAIN.md) | Rust/MinGW setup, build traps, Bevy 0.19 API notes | current |
| [PIPELINE.md](PIPELINE.md) | `pd_convert` commands, source data map, regenerating `cache/` | current |
| [VERIFY.md](VERIFY.md) | proof per change type (screenshots, montages, A-F sheets) | current |
| [MOVE-RECORDER.md](MOVE-RECORDER.md) | filming/tracing the ORIGINAL (memory-injected pad, 341 takes, live audits, save safety) | current |
| [GOLDEN-TRACE.md](GOLDEN-TRACE.md) | numeric original-vs-ours movement comparison, `PD_TRACE` | reference |
| [METHOD-FIDELITY.md](METHOD-FIDELITY.md) | the READ/MEASURED method that fixed locked-on movement | reference |

## File formats and the original's program (reference)

| File | Covers |
|---|---|
| [FORMATS-ALR.md](FORMATS-ALR.md) | models, textures, stage vertex/index data |
| [FORMATS-ANIMATION.md](FORMATS-ANIMATION.md) | skeletons, skinning, tracks, rotation conventions |
| [FORMATS-SKILLS.md](FORMATS-SKILLS.md), [SKILL-FIELD-CROSSWALK.md](SKILL-FIELD-CROSSWALK.md) | skill table, arsenals, costs, damage, families; community field labels |
| [FORMATS-EFFECTS.md](FORMATS-EFFECTS.md) | skill effects: link, event slots, timing, colour, blends, school aura glow |
| [FORMATS-AUDIO.md](FORMATS-AUDIO.md) | music (`.stx`), sound effects, sound-event mapping |
| [FORMATS-STAGES.md](FORMATS-STAGES.md) | stage objects, `stNN.dat`, jump zones, start points, `props.json`, sky |
| [FORMATS-CPU.md](FORMATS-CPU.md) | computer opponent: `.ssb` VM, `.cad` graph, `.epa` rows, steering |
| [PROGRAM-RE.md](PROGRAM-RE.md) | reading `PDUWP.exe` with Ghidra: dispatch map, action table, exact numbers per function |
| [FOOTAGE.md](FOOTAGE.md) | Laurence's recordings measured (24 vs 30 fps verdict etc.) |
| [MOVEMENT-AUDIT.md](MOVEMENT-AUDIT.md) | walk/run and strafe vs the program read, locked-on lane |

## The game (`pd_game`), by area (current)

| File | Covers |
|---|---|
| [GAME.md](GAME.md) | simulation, input, cameras, animation states, sound, stage look, props, footsteps |
| [MOVES.md](MOVES.md) | per-family skill rules, parameters, evidence, how to add a family |
| [COLLISION.md](COLLISION.md) | floors, walls, classes, spawns, camera occluders, stairs QoL |
| [FRONTEND.md](FRONTEND.md) | title, setup, character select, builder, result, modes, Options overlay (VIDEO SETTINGS / SOUND / CONTROLS tabs: display, look, volume, camera, button mapping); layered by date |
| [MENUS.md](MENUS.md) | how the menus look: art, font, portraits, previews, overlay panel |
| [UI-DECLUTTER-PROPOSALS.md](UI-DECLUTTER-PROPOSALS.md) | ranked de-clutter / modern-comfort proposals per screen (2026-10-09), awaiting Laurence's picks; sheets in `engine/shots/declutter/` |
| [HUD.md](HUD.md) | battle HUD: atlas pieces, layout, scaling |
| [CHAR-LIGHT.md](CHAR-LIGHT.md), [STAGE-LIGHT.md](STAGE-LIGHT.md), [SHADOWS.md](SHADOWS.md) | fighter shader, stage light script and per-pixel port, shadow volumes |
| [VFX-AUDIT.md](VFX-AUDIT.md) | effect fidelity gaps vs footage, with the 2026-10-03 sweep (lead inventory) |
| [CAPSULE-VISUALS.md](CAPSULE-VISUALS.md) | capsule / Aura Particle art and draw recipe (orb shader section superseded) |
| [RUMBLE.md](RUMBLE.md) | controller vibration read from the original (hit, knock-down landing, cast, fire), the VIBRATION row, the vendored `bevy_gilrs` fix (2026-10-03, lane rumble) |

## Designs not (fully) built

| File | Covers | Status |
|---|---|---|
| [DESIGN-BRAWL.md](DESIGN-BRAWL.md) | game modes framework (Close Quarters built), Rules flags | current |
| [DESIGN-PROPHUNT.md](DESIGN-PROPHUNT.md) | Prop Hunt: Laurence's brief, rules, controls, code map, choices made, what needs a PC check (built 2026-10-07; seen on the PC 2026-10-08) | current |
| [PERF.md](PERF.md) | frame-time work: what each speed-up does, its off switch, and the measured tables (round 1 + round 2, measured on the PC on mains 2026-10-08) | current |
| [ONLINE-PLAN.md](ONLINE-PLAN.md) | online versus: single-screen view, Online menu, LAN + Steam 480, gamemodes online; merged into main 2026-10-03 ("Merged into main" at its end is current) | current (built; two-PC test open) |
| [DESIGN-2V2.md](DESIGN-2V2.md) | teams exist; resuscitation (revive) not built | design |
| [DESIGN-REPLAYS.md](DESIGN-REPLAYS.md) | replay format, `Match::checksum()`, determinism | design |
| [RESEARCH-LIGHTING.md](RESEARCH-LIGHTING.md) | lighting research; what shipped is in FRONTEND "Look options" | history |

## Audits and research ([audits/](audits/))

| File | Covers | Status |
|---|---|---|
| `P0-BRIEF.md`, `P0-AI`, `P0-ANIMSTATES`, `P0-AURA`, `P0-COLLISION`, `P0-EFFECTS`, `P0-FOURPLAYER`, `P0-FRONTEND`, `P0-LIGHTING`, `P0-SKILLS`, `P0-SOUND` | 2026-09-30 evidence ledgers per area (P0-SOUND section 12 is the live sound-class list) | reference, partly superseded by built lanes |
| `AUDIT-ATTACKS`, `-CAMERA`, `-COMBAT-CORE`, `-DEFENCE-STATUS-CAPSULES`, `-EFFECTS-SOUND`, `-JUMPS-DODGES`, `SKILL-BY-SKILL`, `GUESS-INVENTORY` | cloud audits and the 283-row guess inventory | history (hypotheses until checked) |
| `CRITIC-2026-10-01.md` | critic pass on the wave 1-5 build | history |
| `RESEARCH-BUILDS-AND-STRINGS`, `-ENGINE-LINEAGE`, `-REBELLIOUS-BULLET`, `-STAGES-PUBLIC` | public research on builds, engine, prototypes, stages | reference |

Outside `engine/docs/`: `AGENTS.md` (rules, layout, state), `TODO-LAURENCE.md` (only things he must do or decide), `.claude/skills/pd-*` (8 project skills: build-run, asset-pipeline, visual-verify, reverse-engineer, film-compare, live-original, lane-wave, laurence-bug), `docs/research/` (2026-09 decoding research, history), `engine/re/README-PC-TOOLS.md` (Python tools).

## Coordinate conventions (read once, applies everywhere)

- Game data is Direct3D, **left-handed**, Y up. Characters face **game −Z** (verified: rendered face points that way).
- Everything we export is glTF, **right-handed**. Conversion = mirror Z: positions `(x, y, −z)`; matrices `S·M·S` with `S = diag(1,1,−1)` (`gltf::to_rh`); triangle winding flipped `(a, c, b)`.
- 1 character ≈ 21 units tall ≈ 1.75 m → about 12 units per metre.
- Animations run at **30 fps** (frame numbers in keys are 30 Hz logic-tick frames, one keyframe per tick — confirmed by reading `PDUWP.exe`'s clip-time update, `PROGRAM-RE.md` "Animation clock", and Laurence's own footage, `FOOTAGE.md` "24-vs-30 fps verdict"; our converter assumed 24 fps until 2026-09-27).
