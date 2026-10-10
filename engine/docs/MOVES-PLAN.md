# Moves plan: every skill + its animation

Goal: all 363 collectable skills behave, animate and look like the original in split-screen versus. Written 2026-09-26 for the lane that takes this over. Read AGENTS.md, then GAME.md "Combat", FORMATS-SKILLS.md, FORMATS-EFFECTS.md, DECISIONS.md.

## Where things stand

| Area | State |
|---|---|
| Skill table | `cache/skills.json`: 399 records, 363 unique registered skills. **Phase 0 (2026-09-26) decoded** time units (1/30 s, confirmed), motion type s63, shell speed/homing (s65/s68), DEF s47, defence kind s44, Pierce s54, Anti-Pierce s48, add-effects s51/s52 — FORMATS-SKILLS.md. |
| Rules (`combat.rs` + `moves/`, deterministic) | **Phase 0 done:** behaviour is data-driven per family (`moves/` registry). Built on it: Shell, [Parabola] (+ blasts), Barrier / Reflect-as-barrier / Shelter with STR vs DEF + Pierce, short-range dash with table reach. [Arc] flies straight (stand-in); unbuilt attack families fire a generic homing shot. Status per family: **MOVES.md**. |
| Effects (`effects.rs`) | All 476 effects convert and play by event slot: 0 cast, 1 projectile (looping), 2 launch, 3 hit, 4 block. Trails (0x14 `b8de89ea`) drawn. Not drawn: lights and other 0x14 kinds, distortion (pixel format 8). Effect speed 30 fps (unconfirmed). |
| Animation | All 129 clips convert and play. Cast clip is a **hand table by family** (`main.rs::cast_clip`) — no data field names the animation. Hit = DMG_SHOT_FRONT, down = BLOW0_FRONT only. |

## Families (capsule type, s42) — the unit of work

Counts are unique registered skills. Tags are from the game's notes.

| Capsule / s42 | n | Tag | Examples | Built? |
|---|---|---|---|---|
| attack 12 | 41 | shell | Blaster, Rail Gun, Mist Blaster | generic projectile (placeholder) |
| attack 10 | 35 | short-range | Psycho Wave, Psycho Knife, Excalibur | **dash + strike** |
| attack 100 | 27 | mixed | Pursuit, Gungnir, Ki Cannon | generic projectile |
| attack 21 | 12 | [Rain] | Rain of Stones, Photon Shower | no |
| attack 18 | 9 | [Crawl]/[Move] | Root of Tree, Crawlers | no |
| attack 16/17 | 10 | [Parabola] | Bomb, Throwing Stone, Trench Mortar | no |
| attack 15/14 | 11 | [Arc] | Laser, Double Lasers, Sonic Boom | **straight projectile** (the original *arcs*) |
| attack 13 | 7 | sniper/rapid | Psycho Sniper, Rapid Cannon, Ricochet Laser | generic |
| attack 20 | 6 | [Fall] | Meteor, Thor's Hammer | no |
| attack 75/22/74 | 6 | [Hole] | Tremor, Gravity Press, Overheat | no |
| attack 11, 19, 5, 50, 6 | 10 | waves, [Object], mines, … | Photon Wave, Psycho Kinesis, Sensor Mine | no |
| defence 80/81 | 18 | [Barrier]/[Reflect] | Crystal Wall, Reflect Barrier | **barrier** (reflect not built) |
| defence 82/89 | 10 | [Shelter] | Psycho Shell, Guardian Angel | no (animated) |
| defence 83 | 5 | [Absorb] | Absorb Energy, Flash Hole | no |
| defence 84 | 5 | [Brush] | Block, Guard, Devil's Arm | no |
| defence 85/86 | 5 | [Course] | Bend, About Face | no |
| erase (many s42) | 42 | erase skills/aura/level | Evaporate, Dragon Slayer, Confusion Gas | no |
| status (many) | 35 | status up/down, bind | Paralyze, Speed Up, Optical Camo | no |
| special (many) | 38 | heal, teleport, level | Regenerate, Teleportation, Level Amp | no |
| environment 90 | 29 | arena-wide | Holy Ray, Reduce Gravity | no |

The game's own guide text describes every type: `game_files/Assets/Data/Menu/mb01_en.ssb … mb05_en.ssb` (Shell, Parabola, Absorb, Orbit-changing, Crawl, Hole, Rain, Arc, Tornado, Fall, Detonation, immobilise, penalty, absorb, STR vs DEF, knock-down, auto-capture, ranges). Extract with the printable-string method used in HANDOFF (2026-09-26) — **quote these as the spec**, don't invent behaviour.

## Unknowns to settle first (research, one lane)

1. **Behaviour numbers**: gsdata s63–s69 (projectile speed, range, homing, lifetime?) — score candidates against the notes and guide text (e.g. Laser must be long-range and fast, Parabola short arcs). Record in FORMATS-SKILLS.md.
2. **Range bands** short/medium/long (guide: "each skill has an optimal range"; HUD reticle colours yellow/green/orange in tutorials) — which field?
3. **STR vs DEF** resolution and knock-down ("A more powerful attack skill will destroy a defensive skill").
4. **Effect frame rate** and slot meanings 2/5 (FORMATS-EFFECTS.md open list).
5. **Animation per family**: extend `cast_clip` into a table in a doc (DMG_* reactions by hit type too: SORD / SPHERE / SPIKE / DRAIN / BIND…). Where ambiguous, an A–F sheet for Laurence.

## Recommended phases

- **Phase 0 — spec + framework (single lane, Opus, high). DONE 2026-09-26** (see MOVES.md, HANDOFF.md). Settle the unknowns above; turn `combat::Effect` into a data-driven behaviour per family (params from the table, not per-skill code); write `docs/MOVES.md` = one row per family: behaviour, parameters + source, cast/hit animations, effect slots used, test. Output: framework merged, 2–3 families rebuilt on it as the pattern.
- **Phase 1 — fan-out by family (parallel lanes).** Each lane owns one family group end-to-end: rules (pure functions + tests), animation mapping, effect slot wiring, `--loadout` screenshot burst. Families are independent once Phase 0 lands.
- **Phase 2 — Laurence's eye.** One A–F / filmstrip sheet per family; he picks; tune constants. This can't be parallelised — batch sheets so he reviews many at once.

## Parallel-lane rules (if more than one agent commits)

- Fence files: `combat.rs` behaviour dispatch and `effects.rs` are **Phase-0 owned**; family lanes add code in a new module per family (`crates/pd_game/src/moves/<family>.rs`) and one line in a registry. HUD/menu untouched.
- One worktree per lane; merge in order; after every merge: `cargo test -p pd_game`, `cargo build -p pd_game`, one `--demo --loadout <family ids>` screenshot burst.
- Sim stays deterministic: no floats from time, no HashMap iteration order in the sim, no Bevy types in `combat.rs`/`moves/*`. The `same_inputs_same_result` test must keep passing.
- Never commit game data or screenshots (AGENTS.md hard rule 1). Esper-RE is reference only (GPL).

## Acceptance per family

- Rules: unit tests for cost/damage/special effect; determinism test still green.
- Visual: `--demo --loadout` burst shows cast animation, the family's effect slots, hit reaction; montage saved to `engine/shots/moves_<family>.png`.
- Docs: MOVES.md row filled with evidence (guide text quote / data field).
- Laurence sign-off on the sheet (or a DECISIONS.md row saying what he chose).
