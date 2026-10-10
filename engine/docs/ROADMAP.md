# Roadmap

> **Archivist, 2026-10-03:** history. The P0-P8 passes below were executed as waves 1-6 (2026-09-30 to 10-01) and the film-comparison waves after them. Current priorities and scorecard: `NEXT-SESSION.md`; every lane: `WAVES.md`. CPU opponents are in scope since 2026-10-01 (`DECISIONS.md`), so "no AI" below is out of date.


Priorities come from Laurence: **split-screen versus first**, online later; no cutscenes, no story mode, no AI.

## Current roadmap — 2026-09-30 (supersedes historical Next below)

**Playable two-player foundation; fidelity and four-player readiness unfinished.**
Mapped assets and passing tests are not completion percentages. Laurence's live
evaluation is the current acceptance bar. Detailed research/build gates and
Claude lane orchestration: **[CLAUDE-DEEP-PASSES.md](CLAUDE-DEEP-PASSES.md)**.
Planning only; the heavy execution wave has not been launched.

| Pass | Dedicated outcome | Current state |
|---|---|---|
| P0 | Audit actual code/mapping and establish original-vs-runtime reference battery | Required before new wave |
| P1 Sound | Correct capsule COLLECTION and USE sounds/triggers/timing separately | Open; steps are good, preserve common35 |
| P2 Movement | Match locked strafing and walk→run stick sensitivity/detection as one system | Close, still off |
| P3 Move VFX/animation | Match cast/charge/release/flight/swing/hit/block/recovery timing, pose and effects by family | Not perfect; needs dedicated pass |
| P4 Lighting | Directional shape/depth, stage mood, shadows/effect illumination and measured performance | Needs its own pass; existing light component is not proof of correct visual result |
| P5 School aura | Ground-up modern silhouette-aware glow, wisps/trails and consume pulse | Existing approaches rejected; redesign, not more shell tuning |
| P6 Four-player foundation/UI | Generalise simulation/targets/inputs/views/setup, then readable quarter-screen HUD | Architectural work, not just shrinking two-player UI |
| P7 Arsenal builder | Original-like art, navigation, editing and practical multi-player arsenal assignment | Functional but not faithful |
| P8 Gamemodes | Specify supported rules/mode matrix, implement and play complete match/rematch paths | Needs scope/design/build pass |

**Order:** P0 → P1/P2 with safe fences → P3 → P4 → P5 → P6 → P7/P8.
Four-player/mode design research may start in parallel earlier; shared
simulation/main/render/frontend implementation must be serialized by ownership.
Each pass requires an independent critic and Laurence's milestone approval.

**Closed:** near-wall grain dissolve is approved COMPLETE (2026-09-29).
Regular steps use his exact `27_id035` / common35. Do not reopen either by default.
Capsule view-isolation repair is implemented; it does not close capsule AUDIO.

**Separate remaining fidelity backlog:** skill exceptions, range/guard rules,
dodges/root motion/combos/aura rules and scripted stage destruction require
code-truth inventory (PLAN moves track has stale statuses). No blanket "all
skills done" from dispatch coverage. Online/replays/distribution remain later;
no unrequested Skate/Prop Hunt/modding expansion during these passes.

## Historical baseline (2026-09-26; not current acceptance)

- [x] Unlock game files (community dump tool; not documented here) → `game_files/`
- [x] ALR parser: stages, textures, vertex formats; all 831 files parse
- [x] Music STX → WAV
- [x] Skinned characters with bone palettes, 129 animations, multi-part grafting, confirmed rotation convention
- [x] Collision (floors, walls, raycast), spawn picking
- [ ] **Collision fidelity (Laurence 2026-10-01):** Lost Palace (st03) stairs can't be walked up, only jumped; other maps may share it. Audit: `docs/audits/P0-COLLISION.md`; plan in `docs/WAVES.md`.
- [x] Split-screen prototype: movement, jump, lock-on cameras, animation states, interpolation, occlusion

## Done since (2026-09-26, later)

- [x] Lock-on toggle, settings menu (split layout, FOV), landing variants
- [x] Cloth physics for extra parts + body-capsule collision; P1 coat tails
- [x] Lit characters with the game's normal maps
- [x] Skinned draws placed at their transform joint (pc01a badge)
- [~] Long-coat cloth (pc01a) — **parked**, hybrid approach chosen (HANDOFF.md)

- [x] Skill table decoded (`cache/skills.json`, FORMATS-SKILLS.md)
- [x] Moves M1: health, level/aura, capsules, 4 slots, Psycho Wave (dash), Laser (shot), Crystal Wall (barrier), aura particles, knock-out + rounds, text readout (`combat.rs`, `hud.rs`)
- [x] All 30 stage `.alr` files converted (model + collision), verified playable by screenshot; 19 are real versus arenas, named from the game's own `stageset_en.dat` table (`cache/stages.json`, `pd_convert stages`, PIPELINE.md); 12 of those have their BGM track identified and converted to `cache/music/<id>.wav`

## Historical Next (superseded; recheck code before using)

1. **Laurence plays M1** — confirm/tune the placeholder rules (GAME.md "Combat": aura refill, capsule rate, auto-pickup, skills used up, speeds) and the cast animations.
2. **M2** — ~~HUD~~ (done: original HUD, HUD.md); hit feedback, sound effects (STH2 `.bin`), capsule glow sprites.
3. **M3** — ✅ **essentially done 2026-09-29** (archive/AUDIT-2026-09-29.md): of 397 skills, **0** return `NotBuilt` and only **14** (3.5%) fall through to the generic `Shell::PLACEHOLDER` motion — ids `205, 206, 341, 701, 714, 720, 721, 722, 724, 730, …`. Every other family (parabola, arc, crawl, fall, ricochet, melee, area, guard, effect) is wired. Remaining: those 14 skills' real motion, and effect visuals (`.eft`).
4. ~~**Stage props** — **DONE 2026-09-29** (see archive/AUDIT-2026-09-29.md).~~ Phase A: every arena's objects are placed from `stNN.dat` and collide. Also since done: objects as sim entities (state intact → damaged → broken, model swapped from `props.glb`, hull per state), Psycho Kinesis / Power ("nearby object" = nearest non-broken prop in reach, lifted, hung, thrown with per-prop STR), and jump zones (class 4 kind 16). Still open:
   - Destructible levels: st04 `yuka` / st03 bridges / st08 blocks (kind 23 / 58) falling; st06 contaB collapse animation (OA01).
   - Per-prop STR is derived from size (`props.rs::throw_strength`), **not** decoded from the `.dat` — an assumption, not a measurement.
   - Decode: kind-byte meanings, OE00 effect blocks, OA01 animation layout, which PS list / CP00 set versus uses.
   - ⚠️ The candidate filter in ROADMAP used to read "exclude kinds 23/26/28/48/58"; the code's `STRUCTURAL_KINDS` is `[6, 23, 26, 28, 48]` and kind 58 is handled by a separate scripted-kind guard. Code is correct; this line is the stale part.
5. ~~**Sky & materials**~~ — ✅ **done 2026-09-29**: sky geometry found and the drift scroll shipped (`41d7516` + `be69005`). Material/shader work continues in RESEARCH-LIGHTING.md.
6. ~~**Character select**~~ — ✅ **done 2026-09-29**: `frontend/charselect.rs` spawns the live rotating 3D preview (`setup.rs:359`). The `enm03a`/`enm12a`/`evpc00a` naming and the `pcNNa0–3` variant models are still open (see Future features).
7. **First-run converter** inside the game (user picks their game folder), iw4L-style.
8. **Online**: deterministic sim is in place; choose lockstep vs rollback, then transport (see iw4L's UDP p2p + QUIC master design).

## Future features (agreed 2026-09-27)

Aim: the ultimate party / couch game, not a game about progression.
**Explicitly not wanted:** unlocks, progression, earnables (cosmetic or otherwise).

Feel & presentation:
- Modern lighting (bloom on capsules/effects, soft shadows, per-stage mood grade) — render-side only. **Decided 2026-09-28:** faithful as the default, enhanced as a settings toggle; the enhanced tier targets a 55-inch 4K TV, so anti-aliasing is settled first. Research, tiers and lanes in RESEARCH-LIGHTING.md.
- Modern particle layer (dust, sparks, trails, ambient stage life) on top of the game's own authored effects.
- IK for weight and fluidity (foot planting, lean, head tracking the lock-on target) — cosmetic, render-side.
- Mantling/vaulting (automatic, no new button — keeps the netcode input contract unchanged), coyote time, jump buffering.
- Hit-stop and KO slow-mo; win poses and entrances from existing clips.
- Post-match stats screen (fun stats, not progression).

Party & couch:
- **Brawl mode**: all players spawn with 4 set, infinite-use skills they can't change; a fixed amount of aura with no refill; no capsule pickups. Pure aura management + the moves you're given. Zero menu time — made for pass-the-pad.
- 2v2 with resuscitation (an original feature, not built yet).
- Randomize Arsenal button (seeded; honours the 30-slot / 3-copy / school rules and the aura-rate curve).
- Replays (a replay is just the recorded input list fed back into the deterministic sim; doubles as a debugging tool).
- Winner-stays-on rotation for 3-4 players with 2 pads (candidate, not yet confirmed).

Roster & front end:
- **3D character selector**: live animated model in the picker (reuses the viewer's spawn/anim machinery), rotating with left/right.
- Convert the 4 variant models per character (`pcNNa0–3.alr`) — likely costume/colour variants; settles an open question and could multiply the roster ~5x.
- Convert `enm03a`, `enm12a`, `evpc00a`; character id→name mapping for a proper roster.
- P1/P2 tint so both players can pick the same fighter.
- Stage variants (night/weather grades), stage-music picker, custom music from `userdata/`.
- Announcer calls from the decoded voice system.

Novelty & modding:
- Skate mode (toggleable sandbox, board physics, grinding on prop railings; never touches the versus game).
- Prop Hunt gamemode (hider disguised as a stage prop; seeker uses the Psycho Kinesis throw that already exists).
- General mod support: `mods/` folder with custom gamemodes and retuned skills, validated by the arsenal legality checker; custom fighters as glTF rigged to the skeleton.
- Stage editor-lite: place/drag the existing prop library, save as custom stage.

Online (after the local game is proven): replays before online; p2p UDP with a QUIC introducer master in the iw4L style; fixed-point math before any cross-platform play.

## Open questions

- Stage objects: which placement list (`PS0/` vs `PS01`) and which of the six start-point sets does 1v1 versus use? Where do object hit points / weight (PK damage "depends on object") live — exe or SSB? (FORMATS-STAGES.md)
- Does JUMP_UP look right in-game now? (Laurence unsure.)
- Do coat-tail parts swing with physics in the original? (Assumed yes; currently rigid.)
- `pc00a0..3.alr` — costume/colour variants?
- Which original split-screen layout: side-by-side (current) or top/bottom?
- Where does skill behaviour live — SSB scripts, data tables, or the exe? (Partly answered: family, costs, damage, requirements are in gsdata; speeds/ranges probably s64–s69, unconfirmed.)
- Which animation does Psycho Wave use in the original? (Currently ATK_SORD by family; candidates sheet for Laurence.)
- Are skills picked up automatically or by pressing a button over the capsule? Are aura particles used from a slot (current) or absorbed on pickup?
- Which BGM track is "Palace" and which is "Strange City"? `stageset_en.dat` names both families but no BGM track's romanized name obviously matches either (see PIPELINE.md); `13_JuryokuHanten`/`14`/`15` ("Gravity Reversal") is the only unclaimed group of three and is a plausible guess for one of them, unconfirmed.
- Is `st01` ("Edgar's Dream", story-only intro/dream stage per its name) actually offered in versus mode, or only reachable from the story? Is `st24` ("Panorama Boss") a normal versus pick or boss-only? Both are non-dummy rows in `stageset_en.dat` so `stages.json` marks them `"versus": true`, but neither has recolour siblings the way the other 5 families do, which is what a boss/story one-off would look like.
