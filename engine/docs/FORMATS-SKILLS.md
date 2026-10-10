# Skill data: `gsdata_<lang>.dat` and `deckdata_*`

Code: `crates/pd_formats/src/skills.rs`. Convert: `pd_convert skills <gsdata_en.dat> cache\skills.json --decks <deckdata_debug>` (also writes `cache\skills.txt`, a readable table).

Starting point was Esper-RE's `EsperGSDATA.bt` / `esper_gsdata.hexpat` (reference only). Everything marked **confirmed** below was checked against the file's own text (skill names and "Requires / Penalty" notes) on 2026-09-26.

## File: `Data/gstorage/gsdata_<lang>.dat`

278,532 bytes, all nine languages. Two data versions:

| Languages | header `[4]` (format) | header `[5]` |
|---|---|---|
| en, ja | 140 | 374 |
| de, es, fr, it, ko, pt, zh | 100 | 320 |

The skill records differ between the two groups (en == ja byte-for-byte). **Use `gsdata_en.dat`** — it has the newer data (skills with ids 301–340 exist there). What changed between 100 and 140 is not yet examined.

### Header (160 bytes, `u32`s)

| Index | Value (en) | Meaning |
|---|---|---|
| 0 | 278,532 | file size — **confirmed** |
| 1 | 108,452 | + 0x19860 = offset of the text index — **confirmed** (0x19860 is Esper-RE's constant; the result lands exactly on the index) |
| 2 | 753 | record slots — used as the loop bound; the table ends earlier at a text-id-9999 record |
| 3 | 108,452 | same as `[1]`, unknown |
| 4 | 140 | format version |
| 5 | 374 | "SkillLimiter" in Esper-RE; unknown |

Rest of the header is zero-ish; not examined.

### Skill records (144 bytes each, from offset 160)

A leading `u32` (looks like it's always 0 in this framing) followed by 70 `i16`. Below, **sN** = the N-th `i16` (record offset `4 + 2N`).

**That leading `u32` is not always 0** ✅ (PROGRAM-RE.md "Hidden fields s70-s73", 2026-09-27): the program's own record is 0x90 bytes starting 8 bytes earlier (file offset 0x18 vs. our s2), so its last 8 bytes are our leading `u32` + s0 + s1 — **of the next record**, not this one. Read as i16 they're **s70-s73**: s70/s71 = that `u32`'s low/high half, s72/s73 = the next record's s0/s1. 19 of 399 records use them (all zero otherwise): Double / Triple / Octolaser (s71 = per-shot tilt step), Ricochet Laser (s70-s73 = shot count, yaw step, roll step, bounces), Impact Burst (s73 = bounces), the nine motion-20 Rain skills, Power / Psycho Kinesis (s71 = turn cap), the three Diffused Laser rows. `cache/skills.json`'s `s70_73` array (added 2026-09-27) carries them; `pd_formats::skills::Skill::s70_73` / `engine/re/action_table.py`'s `hidden()`.

- Slots 0–750 hold data; slot 751 has text id **9999** = end marker. Slots whose skill id is 0 are unused.
- en: **399 records**, 397 distinct skill ids (257 and 294 appear twice; the second copies, records 749–750, are variants). **365** have a catalogue number (s5 ≠ 0) = collectable; the other 34 are enemy/boss/story skills.

| Field | Name in code | Status / evidence |
|---|---|---|
| s2 | `TEXT_ID` | **confirmed**: keys the text index; every name matches (Psycho Wave, Excalibur, Laser Blade…) |
| s3 | `SKILL_ID` | **confirmed**: the id arsenals store (the debug decks decode to sensible skills). Grouped by school: Psycho 1–60, Optical 61–120, Nature 121–180, Ki 181–240, Faith 241–300, then 301–340 extra, 341+ enemy |
| s4 | — | 10 on all but 10 records |
| s5 | `CATALOG_NO` | 1…n on collectable skills, 0 on enemy/boss ones (assumed meaning) |
| s6 | `RARITY` | 0–5; Esper-RE: stars = n + 1 (unconfirmed) |
| s9 | `CAPSULE` | **confirmed by names**: 1 attack, 2 defence (Crystal Wall, Reflect Barrier), 3 erase (Shatter, Exhaust), 4 status (Burden, Skywalk), 5 special (Blessed Water, Level Boost), 6 environment (Desertification, Backdraft) |
| s11 | `SCHOOL` | **confirmed**: 1 Psycho, 2 Optical, 3 Nature, 4 Ki, 5 Faith, 0 enemy-only |
| s12 | — | Esper-RE calls it AnimationIDGround. **Not an animation**: equals the text id on most records |
| s13 | — | 3xx on projectile skills (Bomb 336, Laser 305…) — probably a projectile/effect id |
| s19–s21 | — | 6xx / 5xx ids, consistent within a family (swords 501–504, punches 506, kicks 507, lasers 509, bombs 521). Esper-RE: sound effects. Unconfirmed |
| s22 | `COST` | aura cost; 0–8 (one 99). Matches Esper-RE; plausible (Throwing Stone 0, Laser 1, Psycho Burst 7). Not yet checked against the game UI |
| s23 | `LEVEL_PENALTY` | **confirmed**: equals "Penalty: N Level" in 8 of 9 notes (Downcast, an enemy skill, is the exception) |
| s24 | `COST_PER_USE` | **confirmed**: 1 on all 5 skills whose note says "Aura cost +1 per use" |
| s25 | `HEALTH_COST` | **confirmed**: equals "Penalty: N Health" in 23/23 notes |
| s26 | `USES` (wired 2026-09-27: `SkillDef::uses`) | ⚠️ **probable, strong evidence (2026-09-27): number of uses, 0 = unlimited.** Registered skills: 0 × 204, 1 × 157, 3 × 3 (Power, Knife Grind, Amulet), 5 × 1 (Jump — community SkillDB: "Jump 30 ft, 5 uses"). All 5 "Penalty: Aura cost +1 per use" skills are 0 (a per-use penalty needs repeat uses); Knife Grind "Aura cost −1 per use" is 3; every environment skill is 1; Finisher 1. Community SkillDB lists uses 1/2/3/5/∞. Also gives Lack its "unlimited-use" filter (s26 = 0). **Contradicts the current rule "every skill is used up when cast"** (DECISIONS.md) — see HANDOFF.md |
| s38 | `REQUIREMENT` | **confirmed kinds**: 1 health ≤ n, 2 health ≥ n, 5 skills left in arsenal ≤ n, 7 maximum aura, 8 level ≤ n, 9 level ≥ n |
| s39 | `REQUIREMENT_AMOUNT` | the n above — **confirmed** (all 6 checked notes match) |
| s40 | — | 2 on 56 skills incl. Snap, Ki Bullet, Bullet of Fire, Rock Shot, Psycho Spear — maybe "can be used in mid-air" (guide: "Some skills can be launched in mid-air"). Unconfirmed |
| s42 | `FAMILY` | **family / motion category** (strong evidence, constant within note tags): 10 melee, 11 wave, 13–17 shots (14/15 `[Arc]`, 16/17 `[Parabola]`), 18 `[Crawl]`/`[Move]`, 19 `[Object]`, 20 `[Fall]`, 21 `[Rain]`, 22/75 `[Hole]`, 80 `[Barrier]`, 81 `[Reflect]`, 82/89 `[Shelter]`, 83 `[Absorb]`, 84 `[Brush]`, 85/86 `[Course]`, 90 environment, 100 various. Candidate key for choosing the cast animation (see below) |
| s43 | — | target kind (strong pattern, 2026-09-26): 0 attacks, 2 every defence skill, 5 every environment skill, 4 self-targeted status/special, 3 capsule-targeted specials, 1 some erase skills, 6 Recollection/Copy. Matches the guide's "noted Self / Capsule / Mine / All". Unconfirmed code-by-code |
| s44 | `DEFENCE_KIND` / `RANGE_BAND` | defence skills: **confirmed** against every note tag — 0 `[Barrier]`, 1 `[Shelter]`, 2 `[Course]` (Gust/Bend/Change Course), 3 `[Brush]`, 4 `[Reflect]`, 5 `[Absorb]`, 6 About Face / Reverse, 7 automatic `[Shelter]` ("Automatically projects when attacked"). Better than s42: Erase Shell has s42 = 80 but s44 = 1 and a `[Shelter]` tag. **On aimed attacks (Arc, Parabola, Fall, both Rains, Hole) and motion-4 instants: the range band** ✅ (PROGRAM-RE.md, `FUN_140273d40`/`FUN_14026e410`, 2026-09-27) — 0 → aim clamped to 80-300 u, 1 → 300-600 u (every Optical laser, mortars: they can't hit anyone closer than 300 u), 2 → no limit (every motion-16 Hole too). A clamped aim also drops the target: no homing/steering, the move just goes to the clamped point. With no target it aims 190 u ahead (0) or 300 u (1) |
| s45 | `STRENGTH_RULE` | **Defence skills: guard size** ✅ (confirmed 2026-09-27, PROGRAM-RE.md `FUN_140272400`/placement code) — **s45 × 0.1 = the guard's radius in world units**: 130 → 13 u on most, 800 → 80 u on the three `[Course]` skills, 300 → 30 u on one. Front-facing kinds (Barrier, Brush, Reflect, Absorb) draw it as a disc 5 u in front of the caster; round kinds (Shelter, Course, About Face, automatic Shelter) as a sphere around the caster. Also used, doubled, as `[Course]`'s max turn rate. Attacks: non-zero when strength is computed — **tabulated 2026-09-27 from every attack's note** (each value's skills all carry the matching note): 1 (target's Health − yours) × s46, only when positive (Judge of Anubis); 2 (your Health − target's) × s46 (Joke of Anubis); 3 consumed attack skills (Scar of Battles, Memory of Battle); 4 consumed defensive skills (Compressor); 17 target's Level; 18 target's Aura; 20 half the target's Aura; 21 set when used — Aura spent charging (Gungnir, Ki Cannon, Ki Palm, Young White Dragon: cost 0, "Hold button to charge"), double your Level (Finisher), the object's size (Psycho Kinesis, Power); 23 double the target's empty buttons; 24 double your empty buttons (Sign of Saints); 26 STR of all your equipped Laser / Blaster / "Piece of" / short-range Ki skills (Bundled Laser, Bundled Blaster, Crystallization, Dance of Death — the kind only in the note, matched by name); 28 random 0, s46 or 2 × s46 (Debris Bullet, Jack Pot, Excaliber); 30 Aura Particles on the battlefield (Overheat). On other capsule colours the same field means other things (attribute kind, etc.) |
| s46 | `STRENGTH` | attacks: damage ("STR"); 1 placeholder when s45 computes it. Esper-RE: Damage. Strong evidence (Muramasa 10, Psycho Spear 8, Throwing Stone 0). **Defence skills: guard duration** ✅ (confirmed 2026-09-27, `FUN_140264020`/`140264340`) — the guard's countdown starts at `s46 - 2`, so it lasts **≈ s46 − 1 ticks** at 1/30 s each: Crystal Wall 23 → 22 ticks ≈ 0.73 s; Brush 18 → 0.57 s; automatic Shelters 10 → 0.3 s (the automatic Shelter with s44 = 7 doesn't count down on its own — it ends another way, unread) |
| s47 | `DEFENCE` | defence skills: **DEF** (strong): Crystal Wall 3, Psychic Wall 8, Cyclone 8, Shield of Ruin 6; the `[Course]` skills are 0 and the guide says "Orbit-Changing types do not have a DEF rating". On a few attacks 1800/3–8 — unknown |
| s48 | — | attacks: which colour of equipped skill an erase targets (1 attack, 2 defence, 8 other, 9 any — erase.rs). **Correction 2026-09-26:** this row used to say s48 = Anti-Pierce; that was a column slip — Anti-Pierce is s49 (audit, verified against the data) |
| s49 | `ANTI_PIERCE` (defence) | defence skills: **confirmed** — 1 on exactly the six "Anti-Pierce" notes (Crystal Wall, Wall, Psychic Wall, Power Shell, Psycho Shell, Rafflesia Fang). Attacks: how an erase picks (1 highest cost, 2 one at random / lowest, 3 all, 4 same button — erase.rs); 2 on every "Erases 1 equipped … skill from target" note |
| s50 | `SHORT_RANGE_ONLY` (defence) | defence skills: **confirmed** — 1 on exactly the six "Erases any **short-range** attack skill it blocks" notes, 0 on the six plain "Erases any attack skill it blocks". Attacks: erase count (1 / 4 = all) |
| s41 | `PROTECTED` | **confirmed** — 2 on exactly the three "Can't be erased or overwritten" skills (Orb, Seal of Death, Sign of Saints); 1 on exactly the three "Retain if you have enough Aura to use again" skills (Photon Barrier, Arrow of Artemis, Reincarnation) — the skill stays on its button after use while the caster's remaining Aura covers its cost |
| s51 / s52 | `ADD_EFFECT` / `ADD_AMOUNT` | the "Add:" effect and its amount. **Confirmed codes** (checked against notes): attacks — 1 aura to 0 (99), 5 explodes on impact (s52 = blast radius: 300 Bomb, 400 Big Bomb/Spore Schism), 7 freeze buttons (300 = "10 sec"), 8 poison buttons (300 = "10 sec"), 9 health += damage, 11 level drain, 18 auto-capture; defence — 1 health += damage blocked, 2 aura += cost blocked, 3 erases the attack it blocks, 4 paralyse (90 = "3 sec"), 5 freeze (300), 7 captures the attack. Codes mean different things per capsule type |
| s54 | `PIERCE` | attacks: **confirmed** — 1 on all seven "Add: Pierce effect" notes, 2 on the six "Penetrates defensive skills" notes |
| s55 | `OBSTACLES` | attacks: 3 on "Penetrates obstacles" notes (Scream of Evil, Thor's Hammer, Lucifer's Arrow…) — strong |
| s57–s59 | `REACTION_LIGHT` / `_MEDIUM` / `_HEAVY` | **confirmed 2026-09-27** (PROGRAM-RE.md "Hit reactions", `FUN_14024fd80`): rows of the **hit-reaction table** (`gsdata` offset 0x2B00C, same 0x72-byte-row layout as the action table; `tables::reaction_table`), picked by the damage the target took in the last 3 seconds **including this hit**: **< 3 → s57 (light), 3–4 → s58 (medium), ≥ 5 → s59 (heavy)**. Not "standing / ? / in the air" as previously read — that's `s61` (below), which is 0 on every skill and so always resolves to reaction row 27, a knock-down (the guide's "hits in mid-air knock you down"). Esper-RE's "homing range per hit" reading was wrong. `s60` and `s61` are the same table: `s60` plays a second reaction after the hit's Add effect lands (14 skill erased, 15 aura drain, 16/17 status down/up, 18 reset, 19 confusion, 20-23 binds); `s61` is the in-air reaction (always 0 → row 27). Fixed ids outside the skill table: 26 = brushed, 27 = any hit in the air. `s62` (`TARGET_STATES`) is a bitmask of which target states the attack can touch (1 normal, 2 knocked down + airborne, 4 lying down, 8 get-up invulnerability) |
| s63 | `MOTION` | how the attack travels (strong evidence): 0 shell (straight), 1 `[Parabola]`, 2 `[Fall]`, 5 `[Crawl]`, 6 `[Arc]`, 7/20 `[Rain]`, 8 mines, 9 explodes around self (Photon Wave), 10 `[Move]` (tornado), 11 `[Object]`, 12 sword, 13 punch/kick/wave, 14 pursuit (hits downed targets), 15/16 `[Hole]`, 17 multi-laser (Ricochet, Diffused) |
| s64–s69 | `MOTION_PARAMS` | six numbers whose meaning **depends on s63** — table below (2026-09-26); read by `pd_game/src/moves/` |

### Motion parameters s64–s69 by motion type

Units (see "Units" below): time in table ticks of 1/30 s, distance in 1/10 world unit, speed in 1/10 unit per table tick (= 3 u/s per point).

| Motion | s64 | s65 | s66 | s67 | s68 | s69 | Evidence / status |
|---|---|---|---|---|---|---|---|
| 0 shell | −30 on most, −80 on Optical lasers / Rail Gun / Rapid Cannon / crawlers, −120 Diffused Laser: **range band?** (guide: Optical = "mid- to long-range"). Unconfirmed | **speed** (Rail Gun 300, Blaster 140, Faith fires 30–40) | if s65 = 0: speed after waiting s67 (Psycho Sniper 380 after 60, Dragon Bullet 120 after 30) | wait, table ticks | **homing** (Faith 40–50, Laser 20, Blaster 15, Rail Gun 3, Psycho Sniper 4) | — | Speed + homing: strong — the guide says Optical has "fast shells" and Faith "Shell speed is slow, but homing on these is quite accurate", which is what these columns show. Scales unconfirmed |
| 1 `[Parabola]` | horizontal speed / 10.8 per tick (Throwing Stone 140, Bomb 90, Big Bomb 40) | 0 | **arc height × 10 u above the straight line at mid-flight** (Throwing Stone 1 → 10 u, Psycho Spear 2 → 20 u, Bomb 4 → 40 u, Particle Mortar 7 → 70 u, Delay Bomb 10 → 100 u) | **sideways steering, / 10.8 × homing % per tick** (Bomb 10 → 28 u/s, Psycho Spear 20 → 56 u/s, Gungnir 30 → 83 u/s) | — | — | ✅ **confirmed 2026-09-27** (PROGRAM-RE.md "Motion 1"), corrects the swap below: s66 was read as steering and s67 as gravity — it's the other way round: **s66 = arc height, s67 = steering** |
| 6 `[Arc]` | −30 / −80 as shells | speed (100–160) | 8–20 | bend? (Laser 20, Slide Laser 40, Twist −40 = other way) | 20 / spread | **shot count** (Double 2, Triple 3, Octolaser 8) | s69 strong (matches notes); rest unconfirmed |
| 12/13 short-range | **reach** 130 (13 u; Excalibur / Mist Blade 300, Laser Blade 150) | 8 | 10 swords, 30 punches | — | — | — | Reach 13 u matches the 14 u tuned by feel; the dash itself is not in the table. Unconfirmed |
| 9 explode around self | radius? 400 / 500 (Photon Burst bigger) | | | | | | Unconfirmed |
| 15/16 `[Hole]` | 30: warning time (1 s)? | 10 | 300 | | | | Guide: "show the area … then appear suddenly at the target's feet". Unconfirmed |
| 7/20 `[Rain]` | 30: warning time? | 30 / 10: radius in whole units? (Lightning Bolt narrowest) | 6: drops? | 5: interval? | 400/300 (7) | 1000: drop height? (20) | Guide: "assault … over a duration … area shown for an instant". Built on these readings. Unconfirmed |
| 2 `[Fall]` | 4–12 | 18/19 | 40–100: fall speed? | 10/15 | | | Built with s66 as fall speed. Unconfirmed |
| 8 mines | kind: 0 timer, 1 sensor, 2 remote (matches the three notes) | sensor radius (300) | **fuse**: Timer Mine 150 = "Explodes automatically 5 sec later" (confirmed); Sensor 60 (arming time?); Fast Bomb 20 | **blast radius** 500 / 300 | | | Kind + fuse strong; radii unconfirmed |
| 5 `[Crawl]` | −80 / −30 | speed 30–110 | | | 500–800: homing? | 100 | Unconfirmed |

### Action table and hit-reaction table (added 2026-09-27, moves track M1)

Two more tables live in `gsdata_en.dat` alongside the skill records, found by reading the program (`docs/PROGRAM-RE.md`): the **action table** (file offset 0x1B00C, 574 rows × 0x72 bytes = 57 `i16`) and the **hit-reaction table** (offset 0x2B00C, same row layout, no separate row count documented — we read however many full rows the file holds past that offset). Both are `pd_formats::skills::tables::{action_table, reaction_table}` (`TableRow`), exported by `pd_convert skills` into `cache/skills.json`:

- Every skill's **s12** (`GROUND_ROW`) picks its row of the action table for a normal cast; **s13** (`AIR_ROW`) picks the row used for a mid-air cast (s40 = 1/3, or s40 = 2 while airborne). **s13 is a row index, not a sound** — see FORMATS-AUDIO.md's `s13` row for the mix-up this caused. `skills.json` carries both as `"action": {"ground": {...}, "air": {...}}` per skill (`null` when that row is 0, i.e. no mid-air cast). Fields: `effect`, `cast_sound` (`common` bank, at cast start), `voice_group`, `anim_id`/`anim_name` (see below), `charge_cycle`, `start_frame`, `dash_start`/`dash_end` (short-range dash window, ticks after cast start), `fire_tick` (ticks to the attack object being created), `hit_window_end`, `combo_offset`, plus the full 57-field `raw` row.
- Every skill's **s57/s58/s59** (light/medium/heavy), **s60** (after an Add effect), **s61** (hit in the air) pick a row of the reaction table; so do the fixed ids 26 (brushed) and 27 (any hit in the air). `skills.json`'s top-level `"reactions"` array holds every row with any non-zero field past `w0` (`row`, `kind`, `effect`, `sound`, `voice_group`, `launch_speed`, `launch_angle`, `w` = w41..w48, `raw`). Row meanings by `kind`: PROGRAM-RE.md "Hit reactions".
- **Animation ids → names**: the action table's `anim_id` (w38) and the reaction table's `kind`-adjacent animation slots (w41..w48) index the same 136-entry motion-name table the program uses (pointer at `0x1402db400`); `pd_formats::skills::ANIM_NAMES` (also in `engine/re/action_table.py`'s `ANIM` list, kept in sync by hand). Index 64 = `ATK_SORD`, 65 = `ATK_SHOT`, 68 = `ATK_SKYSHOT_LOW`, 76 = `ATK_LASER`.
- Golden values (checked against `python engine/re/action_table.py gsdata_en.dat cache/skills.json <id>` and as Rust unit tests in `pd_formats::skills::tests`): Laser (id 65) ground row 42, fire tick 13, `ATK_LASER`, cast sound `common/242`; Psycho Knife (id 3) row 3, fire tick 18, dash 12..22, hit window 18..22; Blaster (id 80) row 53, fire tick 20, `ATK_SHOT`; Psycho Wave (id 1) row 1, `ATK_SPHERE`, fire tick 20, dash 10..20, hit window 20..22.
- `skills.json` also carries each skill's **`s70_73`** (see the "Skill records" section above) as a 4-element array.
- All additive: every existing `skills.json` field is unchanged; `pd_game` reads the file as a generic `serde_json::Value`, so the new keys don't affect it (checked 2026-09-27: the game still starts and plays with the regenerated cache).

### Units

- **Time: table tick = 1/30 s — confirmed.** "Freezes target's skill buttons for 10 sec" = 300 (Ice Sword, Ice Lance, Glacial Wall…), "For 10 sec, target's button presses cause 1 damage" = 300, "Target can't move for 3 sec" = 90 (Paralyze Barrier). The sim runs at 60 Hz, so one table tick = 2 sim ticks.
- **Distance: 1/10 world unit — likely.** Blast radii 300/400 as whole units would cover the whole arena (st06 is ~1,300 u across); as 30/40 u (2.5–3.3 m) they are bomb-sized. Short-range reach 130 → 13 u matches the reach tuned by feel.
- **Speed: 1/10 u per table tick = 3 u/s per point — assumed** from the two above. Gives Laser 420 u/s, Blaster 420, Rail Gun 900, Faith fires 90–120 (a little faster than running at 72 — "slow").
- **Homing: 0.1 rad/s per point — the scale is a guess**; only the ordering (Faith sharp, Rail Gun straight) is evidenced.
- **Range bands** (short/medium/long, guide): no field found. s64's −30/−80 on shells is the only candidate.

### Rules from the game's guide (`Data/Menu/mb01..05_en.ssb`)

- STR vs DEF: "STR > DEF: Attack has no effect. Defense skill is destroyed. STR = DEF: Attack has no effect. STR < DEF: Attack has no effect."
- Pierce: "when an attack has Pierce, the difference between the attack and defense … is incurred by the target" (STR 8 vs DEF 2 → 6).
- Barrier types protect the front only; Shelter types 360°; Absorb is "ineffective against close-proximity attacks"; only Shelter stops Rain; Orbit-Changing ([Course]) has no DEF.
- Knock-down: "after receiving damage from a powerful attack or when attacked in mid-air … for the few seconds it takes to get up the Esper cannot be the target of Attack skills." Falling out of the arena: 3 damage.

Everything else is kept verbatim in `raw` in `skills.json`.

**Animation:** no record field holds an index into our 129 `ANIM_NAMES`. The cast animation is most likely chosen by family (s42, with s21 splitting melee into sword/punch/kick). The note tags match animation names directly: `[Brush]` ↔ `DEF_BRUSH`, `[Absorb]` ↔ `DEF_ABSORB`, `[Shelter]` ↔ `DEF_SHELTER`, `[Course]` ↔ `DEF_COURSE`. Plan: a small hand table family → animation, checked visually.

### Text index (at header `[1]` + 0x19860)

`u32 unk (25395)`, `u32 count (393)`, `u32 block size (4736)`, `u32 (140)`, `u32 (374)`, then `count` × `(u32 text id, u32 name offset, u32 note offset)`. Strings start right after the index (offsets are relative to that point), NUL-terminated, ASCII in English. **Confirmed**: every entry decodes to a name + note.

- Notes carry a family tag in brackets: `[Barrier]` 18, `[Parabola]` 13, `[Shelter]` 13, `[Rain]` 12, `[Arc]` 11, `[Hole]` 10, `[Crawl]` 9, `[Fall]` 6, `[Course]` 5, `[Absorb]` 5, `[Brush]` 5, `[Move]` 3, `[Object]` 2, `[Reflect]` 2.
- Text 301 = **Aura Particle** ("Increases Level by 1.") has **no record**: the aura particle is built into the game, not a table skill.
- Texts 392–393 are Shift-JIS placeholders ("new skill 39/40").
- 110,596–212,317 holds other non-zero tables; not examined.

## File: `Data/setup/deckdata_debug` (6,604 bytes)

4-byte header (`04 00 01 00`), then 66 × 100-byte decks laid out like Esper-RE's `deck.h`: `u32, char name[16], i16 skills[30], u16 school count, u16, u32 stats[4]`. **Confirmed**: the first four decks are named PSYCHO / OPTICAL / KI / NATURE and hold 16 skills each (pairs of the same id; −1 = empty slot); the other 62 are empty. So this is a developer test file, **not** a set of default arsenals.

## Rule facts the data implies (for the sim)

- **Level** (aura capacity) and **Aura** are separate: requirements test "Level ≥ 10", "Level ≤ 2", "Maximum Aura".
- Health thresholds used: ≤ 7, ≥ 15 — so max health is above 15. Starting/max health and max level are **not** in this file; ask Laurence.

## Cross-check with the cloud research branch (2026-09-27)

Branch `claude/phantom-dust-confidence-kd0s74` (cloud session, no game files) added a `pd_convert skillprobe` command using the community SkillEditor framing (records of 0x90 at 0x18, fields as byte offsets). Run on our files (`engine/shots/skillprobe.txt`, local only): the same table as ours, framing shifted — SkillEditor `+off` = our s((off + 8 − 4) / 2) (record 0 of theirs is a header; their `+0x7A` "projectile behaviour" = our s63 motion, `+0x0E` capsule = s9, `+0x12` school = s11), with identical value histograms. Nothing in their field map changes ours. Its SSB reader parsed all 785 `.ssb` files (0 failures): scripts live only in Event, Menu, MenuD, System, boss, com/ai and questdata — **no skill scripts**, confirming skill behaviour is in the exe (rebuilt by observation, MOVES.md).

## Program evidence (PDUWP.exe, 2026-09-27)

Read from the game program with Ghidra; method, addresses and details are in PROGRAM-RE.md. ✅ = read in the code and checked against the data.

- ✅ **Record framing in the program**: records are 0x90 bytes from file offset 0x18, so the program's record starts at our **s2**; in-memory offset `o` = our s(o/2 + 2). Our "u32 always 0 + s0 + s1" are the program's last 8 bytes of the *previous* record (read only by the multi-shot arc code, as 0). Record 0 of the program's framing is the tail of the header.
- ✅ **s43 = 0 → the behaviour handler is chosen by s63 (motion)** from a 21-entry table; s43 ≠ 0 → by s43 (5 entries). s63 values 18 and 19 have no handler.
- ✅ **s12 = row in the "action table"** at file offset 0x1B00C (574 rows × 0x72 bytes = 57 i16). The row holds the cast animation index (w38; motion-name table = our ANIM_NAMES), the **fire tick** (w43, 1/30 s), the short-range dash window (w41..w42) and hit-window end (w44), the effect number (w2), the **cast sound id** (w3), the battle-voice group (w4) and controller rumble (w6–w37). Tool: `engine/re/action_table.py`. Examples: Laser row 42 = ATK_LASER, fires at tick 13; Blaster row 53 = ATK_SHOT, tick 20; Psycho Knife row 3 = ATK_SORD, dash ticks 12–22, hit ticks 18–22.
- ✅ **s13 is not a sound id**: it is the action row for the mid-air / alternative cast (Laser → row 305 = ATK_SKYSHOT_LOW, fires at tick 20). This corrects the s13 row of FORMATS-AUDIO.md (`common/305` is the cast sound of row 105, Fire Storm).
- ✅ **s40 = which row to use**: 1 or 3 → always the s13 row; 2 → the s13 row when cast while jumping or falling; 0 → always s12. (Confirms "s40 = 2 on skills usable in mid-air".)
- ✅ **Motion parameters, motion 0 (shell)**: s64 < 0 → range = −s64 × 10 u, lifetime = range / speed (Blaster −30 → 300 u); s64 ≥ 0 → lifetime in ticks. Speed = s65 / 10.8 u per tick (2.78 u/s per point at 30 Hz). s67 ≠ 0 → speed ramps linearly from s65 to s66 over s67 ticks (Psycho Sniper: it does not wait and then fly). s68 = max turn of s68 × 0.1° per tick × homing %, applied only while the target is within 60° of the shot's heading.
- ✅ **Motion 6 ([Arc])**: s65 speed (as shells); **s66 = homing** (0.1° per tick per point); **s67 = bend** in degrees (yaw); **s68 = tilt of the arc's plane** in degrees (roll about the shot's forward axis), not a fan step; s69 shot count; s64 unused. Curve time = arc length / speed, then 30 ticks straight.
- ✅ **Motions 3/12/13/14 (short-range)**: s64 × 0.1 = hit-sphere radius; s65 = dash speed: s65 × 10 / dash ticks per tick, capped at 10 u/tick, so at most 10 × s65 u in total; s66 / 10 = the dash's max turn in degrees per tick; the dash stops 11 u from the target. Motion 8: s67 × 0.1 = blast radius. Motion 15/16 ([Hole]): s66 × 0.1 = radius; s64/s65 set its grow / active timing (`FUN_14026a7f0`, not written up yet ⚠️).
- ✅ **s51 = 5 → explodes, radius s52 × 0.1** (read in the impact code). s19 is played at launch and s20 loops with the projectile (at fire); s21 is played at the impact point.
- ✅ **Units**: logic tick = 1/30 s (fixed-step timer); distance 0.1 u; **speed 2.78 u/s per point** (not 3); **homing 3°/s per point** (not 0.1 rad/s), scaled by the homing status percentage.

### Program evidence, phase 2 (PDUWP.exe, 2026-09-27)

Details and addresses in PROGRAM-RE.md "Phase 2". These notes correct or complete rows above (the rows themselves are left as they were).

- ✅ **The "u32 always 0" before s0 is not always 0** — it and s0, s1 are the program's last 8 bytes of the *previous* record: **s70–s73**. 19 records use them: Double / Triple / Octolaser s71 = −150 / −80 / −32 (per-shot tilt step of the arc plane), Ricochet Laser s70–s73 = 4, 0, 90, 3 (shot count, yaw step, roll step, bounces), Impact Burst s73 = 1 (bounces), the nine motion-20 Rains 300, 15, 90, 0 (drop scatter radius 30 u, cloud follow speed, drop fall speed), Power / Psycho Kinesis s71 = 100 (turn cap). `engine/re/action_table.py` prints them.
- ✅ **s44 on attacks = range band** for aimed moves (Parabola, Fall, Arc, Rain, Hole) and instant effects (motion 4): 0 → aim clamped to 80–300 u, 1 → 300–600 u, 2 → no limit (all motion-16 Holes too). A clamped aim also drops the target (no homing). Lasers are s44 = 1.
- ✅ **s57 / s58 / s59 = light / medium / heavy hit reaction**, chosen by the damage the target took in the last 3 s (including this hit): < 3, 3–4, ≥ 5. **s61** = reaction when hit in the air (0 everywhere → reaction 27, a knock-down). **s60** = reaction played after the hit's Add effect lands (14 skill erased, 15 aura, 16 / 17 status down / up, 18 reset, 19 confusion, 20–23 binds). The numbers index a **reaction table at gsdata offset 0x2B00C** (0x72-byte rows like the action table; `engine/re/state_table.py`): e.g. 46 = DMG_SHORT flinch 5 ticks, 2 = DMG_SHOT_* four-way stagger, 3 = knock-down (launch 20 at 50°), 27 = air knock-down, 45 = straight-up launch (Tornado), 48 = blast knock-down. This replaces the "standing / ? / in the air" reading and the list of knock-down ids.
- ✅ **s62 = which target states the attack can touch** (0 = 1): 1 normal, 2 knocked down and airborne, 4 lying on the ground, 8 during the post-get-up invulnerability. Gravity Press / Geyser Impact 7 ("Effective against downed opponents"), Backdraft Bullet 6 ("Effective only against knocked down targets").
- ✅ **Defence s45 = guard size** (s45 × 0.1 u: 13 u, [Course] 80 u) and **s46 = guard time** (about s46 − 1 ticks) — both readings above confirmed. **DEF = s47 + s48 × (uses of that slot)** (Failing Wall s48 = −1, Dark Protection +1, Protecting Air −1). **s50 = 1 → the guard's Add effect applies only to short-range attacks** (the six "Erases any short-range attack skill it blocks" notes). s49 = Anti-Pierce confirmed in the code.
- ✅ **Attack STR** = s45 rule (s46 when s45 = 0) + caster's attack status + **s47 × uses of that slot**, minimum 0. No current attack has s47 ≠ 0.
- ✅ **s54**: 1 = Pierce (difference goes through a broken guard unless s49 = 1), 2 = passes through Barrier / Shelter / Absorb / Brush / Reflect entirely. Status 31 cancels both.
- ✅ **s55**: 3 = no wall and no line-of-sight tests ("Penetrates obstacles"); 2 = walls ignored ⚠️; 1 = other collision query ⚠️.
- ✅ **Motion parameters (s64–s69, s70–s73) per motion** — full list in PROGRAM-RE.md. Corrections to the table above: [Parabola] s64 = horizontal speed (/10.8 per tick), **s66 = arc height in 10-u steps** (not steering), **s67 = sideways steering speed** (not gravity); [Fall] s64 / s65 = start height / distance back toward the caster in 10-u steps, s66 = speed, s67 = steering; [Crawl] s65 → s66 over s67 = speed ramp, s68 = turn acceleration (0.01°/tick² per point), s69 = max turn (0.1°/tick per point); [Rain] motion 20: s64 delay before the first drop, s65 + s64 cloud life, s66 drops, s67 gap (ticks, +1), s69 cloud height (0.1 u); motion 7: s64 warning, s65 active ticks, s66 column height (10-u steps), s68 radius (0.1 u); [Hole] s64 warning, s65 active ticks, s66 radius; mines as read (s65 sensor radius, s66 fuse / arming, s67 blast radius); motion 9 s64 blast radius; [Move] s64 life, s65 radius, s66 column height (10-u steps), s67 → s68 over s69 speed ramp; motion 17: s68 yaw, s69 roll (+ s71 / s72 per shot, s70 count, s73 bounces); motion 4: s64 0 = target within range / 2 = everyone, s65 delay.
- ✅ **Short-range s64** = hit-sphere radius (punches, 13 u) or **blade length** (swords, 13–30 u, tested 12 u around the target).
- ✅ Multi-shot counts (Double / Triple / Octo / Ricochet) come from `SHOT01…` markers in the cast effect, not from s69.

### Program evidence, phase 3 (PDUWP.exe, 2026-09-27)

Details and addresses in PROGRAM-RE.md "Phase 3". ✅ = read in the code and checked against the data.

- ✅ **Program record index = our record + 1** (the program's record k starts at our record k − 1's s2). So a record index stored *inside* the data (charaset dodge 750 / 751, combo lists s27–s36) names our record 749 / 750, 13 (High-speed Punch), 47 (Rapid Cannon)… Skill ids (s3) are unaffected.
- ✅ **Records 749 / 750 (the "second copies" of ids 257 and 294) are the characters' dodges**: charaset +0x36 = 750 → our 749 = the jump dodge (s43 3, s44 9, s45 14 → apex 14 u, s47 140 → gravity (140 + 100) × 0.0012 = 0.288 u/tick²); 751 → our 750 = the ground dodge (s44 8, s45 4 → 40 u, action row 355 DASH_A). They are not arsenal skills.
- ✅ **s43 = 3 is "movement special", not "capsule-targeted"**: s44 0 jump (apex = 10 × s45 u, gravity (s47 + 100) × 0.0012), 2 Retreat (warp to own capsule spot), 4 Call (move own capsule spot to you), 5 float + jump (status 11 for s46 ticks: Skywalk 450, Levitate 90), 6 Return (warp to start), 7 warp to a target (Charge, Surprise Attack), 8 dash (s45 × 10 u over action-row w42 − w41 ticks: Dash 100 u / 15 ticks), 9 jump with the DASH_* clips (the jump dodge).
- ✅ **s43 = 1 acts on capsules lying on the field**: s44 = which (0 touched any, 1 touched Aura Particle, 2 all Aura Particles, 3 touched skill capsule, 4 opponents' skill capsules, 5 all skill capsules); **s45 = what** (0 erase, 1 capture to the cast button, **2 swap for a random undrawn card of the owner's deck**, 3 turn into an Aura Particle).
- ✅ **s43 = 4 (status / self) carries up to four effects: kind s45–s48, amount s49–s52, duration s53–s56** (ticks; 0 = permanent). s44 = 0 caster, 1 target, 2 everyone. Kinds: 1 Level +, 2 Aura +, 3 Health +, 4–7 statuses 0–3 (attack, speed %, homing %, Aura rate %), 9 Aura ×, 10 Level ×2, 11 Regenerate (+1 Health / 91 ticks standing), 12 invisible (status 8), 13 invisible + float + jump, 14 status 33 (no damage), 15 invulnerable (state 22), 17 status 37, 18 copy target's statuses, 19 take back last used skill, 21 return used skills to the deck, others ⚠️ (PROGRAM-RE.md).
- ✅ **s43 = 5 environment: s47 = lifetime in ticks when > 0** (1,800 = 60 s on seven skills), otherwise until replaced.
- ✅ **s14–s17 = combo action rows** (hit k ≥ 1 of a chain uses them in turn); **s27–s36 = program record indices of skills this one can chain into** (only High-speed Punch, Rapid Cannon, Trench Mortar and three enemy Splash records use them).
- ✅ **s22 / s23 / s25 at payment**: Aura ≥ s22 (+ charge / 10); Level ≥ s23, then Level −= s23; Health −= s25, **s25 = −1 means half your Health, rounded up**.
- ✅ **Action table w1 = root-motion flag** for the cast clip (1 on every used row); **w2 = the effect number (eftNNNN)** — for the extra skills 301–340 it points at packed-only eft0302–0321 (301–320) or at existing effects (321–340), so no family-based borrowing is needed; **w45 / w46 = combo input window (clip frames), w47 = chain frame**, w48 = combo start offset (0 everywhere).
- ✅ **Reaction table w1 = root-motion flag** for the reaction clip (0 only on rows 0, 1, 33–43, 46, 70).

## Rule table, Aura handicaps and Quick decks (2026-10-01, lane castrules F2)

Three more blocks of `gsdata_en.dat`, read by `pd_formats::skills::{rule_table, aura_handicaps, quick_decks}` and written by `pd_convert skills` into `skills.json` as `rules`, `aura_handicap` and `quick_decks` (evidence: audits/P0-FRONTEND.md appendix B-D, re-read from the file; `combat::RULE_TABLE` is a built-in copy pinned to the file by a data-dependent test):

- **Rule table**, offset `0x33040`, 13 rows of four `i32` (health, Level min, Level max, minutes), index = rule id (0 Standard, 1 Ante, 2 Quick, 3 Puny 10 HP Level 0-5, 4 Heavy 40 HP Level 5-99, 5 Special Environment, 6 Single School, 7 Highlander, 8 Puny Only, 9 Heavy Only, 10 KO Battle 5 HP 3 min, 11 Annihilate the Odd One 10 HP, 12 the Odd One himself 40 HP, no limit). Time limit = minutes x `0x708` program ticks (`FUN_140128cc0`) = 3600 sim ticks a minute. A fighter starts with the row's Health at the row's Level minimum, Aura full (`FUN_14024e790` -> `FUN_14024d020`).
- **Aura handicap**, offset `0x33034`: three floats 1.3 / 1.0 / 0.7 for 1 / 2 / 3 schools (declared at deck `+0x50`); other kinds 1.0.
- **Quick decks**, offset `0x33804`, 8 x 0x80 bytes: name at +4, 30 `i16` skills at +0x14, kind (`u16`) at +0x50 (2 for all eight).

Regenerate: `pd_convert skills <Data>\gstorage\gsdata_en.dat <cache>\skills.json --decks <Data>\setup\deckdata_debug`.
