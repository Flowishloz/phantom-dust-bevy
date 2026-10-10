# P0 audit: SKILLS (gameplay fidelity), 2026-09-30

Read-only audit. Scope: what each skill family **does** in the match (rules, costs, uses, STR/DEF, pierce, statuses, environment, erase, capsules, match timing). Not visuals (effects auditor) and not sound (P1). Nothing was run or built.

**Evidence tags.** `[R]` = I read it in the decompile (`decomp_all.c`) this pass, address given, and compared with our code. `[PR]` = `PROGRAM-RE.md` (earlier read; I did not re-read it, but I read our side). `[D]` = computed from `cache/skills.json` / `gsdata_en.dat` this pass. `[C]` = our code read (`pd_game/src`, file:line as of HEAD `acd8b59`; combat.rs/moves/* are unchanged since the cloud audits of the same day, whose line numbers mostly still hold). Older "done" claims were treated as leads.

## 0. Headline

- Skill **coverage** is complete (every player skill has *a* behaviour) but **fidelity** is not: the big gaps are in the shared rules, not in the per-skill families. Top three, all read in the program: **(1) the original pays Aura/Level/Health/uses at the fire tick, we pay at the button press** (a hit during wind-up costs nothing in the original); **(2) a defence broken by a stronger attack is erased from its button, and each block drains the guard, we keep the skill and let a guard block forever**; **(3) Aura refill is a curve (1.25 pts/s at 0 Aura falling to 0.25 pts/s from 10) with a 1.3/1.0/0.7 deck handicap and pauses while casting, we refill flat 1 pt/s.**
- **The environment capsules are fully data-driven in the original** (s44 = status id, s46 = amount, s45 = 450 = the 15 s capture lock, s47 = lifetime). `FORMATS-SKILLS`/`environment.rs` say "the table holds no numbers"; that is wrong. Capturing a crystal also hands the capturer an **Aura Particle**; we do not.
- The projectile families still run the 2026-09-26 guesses (speed 3 u/s per point, homing 0.1 rad/s, no range bands, gravity 150, straight Fall...). `PLAN` M5/M6/M11-M14 are **not done** (section 5).
- The **versus catalogue is wrong by definition**: it is "catalogue number != 0", which includes 23 enemy-only records (Body Attack, Dust Cannon, Splash, Diffused Laser, "( debris)" ...). Real player roster = ids 1-340 (340 skills).

## 1. The usable roster [D][C]

| Set | Records | Note |
|---|---|---|
| `skills.json` records | 399 (397 distinct ids; 257, 294 appear twice) | |
| "registered" (catalogue no. != 0) = what `main.rs:435 load_skills(..,true)` hands the builder | 365 records = 363 ids | includes the 2 duplicate dodge records and **23 enemy records (ids 701-741)**; 11 of them have no handler (motion 18: Body Attack x3, Dust Cannon, Egg Bomb 724, "( debris)", "( trash)", Diffused Particle) or are placeholder-shots (Diffused Laser 720-722) |
| **Player roster (ids 1-340)** | **340**: 156 attack, 43 defence, 42 erase, 37 special, 33 status, 29 environment | every id 1-340 is registered (catalogue numbers have quirks: no #50, Crawling Bullet 333 carries 70) |
| "Unregistered" extras ids 341-374 | 34 | boss-flavoured (Impact Burst, Giga Ruin, Geyser Impact, Young White Dragon, Protecting Air, Failing Wall, Dark Protection...). Their availability in the original is not read |

The original program's own collectable rule was **not found** (see section 6, R1); `s4` (register id 10 = usable) is 10 on 355 of the 365 and on all 34 extras, so it does not separate them either. Non-attack count in `AGENTS.md`/`MOVES.md` ("164") is stale: it is **184** for ids 1-340.

The Aura Particle is **program record 0** (the tail of the gsdata header, our framing hides it): text 301, s12 = 343 (an empty action row: WAIT0, fire tick 0), s26 = 1, s43 = 4, s45 = 1 (kind "Level +"), s49 = 1 [D]. So it is a normal s43 = 4 status-body skill (`FUN_140270b00` kind 1), Level clamped 0..99 by `FUN_14024d970` [R].

## 2. Ledger

Milestones: Mapped = program/table rule known; Impl = code exists; Repro = seen in our runtime (docs/shots, not re-checked); Verified = tested against the program's rule (tests that only pin our old reading do not count); Appr = Laurence said so. ✅ yes, ~ partial, ❌ no. Counts are player roster (1-340). **Appr is ❌ for every row** unless stated: DECISIONS holds no approval of any move family; the P0 baseline says move effects/animation "not perfect".

### 2a. Shared rules

| Feature | Mapped | Impl | Repro | Verified | Appr | Evidence |
|---|---|---|---|---|---|---|
| Cast timing (action row w38/w43 fire tick, caster held for the clip, hit before fire cancels) | ✅ | ✅ | ✅ | ~ | ❌ | `combat.rs:1417,1461`; Laser/Blaster/Knife vs footage FOOTAGE claims 1-18 (3 moves only). Defence rows all fire at tick 2 [D] (ours tick 0, trivial) |
| **Payment at fire** (Aura >= cost, Level >= s23, Level -= s23, Health -= s25; uses -1; s24 per-use; poison/Backdraft hit at fire) | ✅ [R] | ❌ pays at press | - | ❌ | ❌ | `FUN_1402626d0` calls `FUN_1402621f0` (pay) then `FUN_140263410` (uses); ours `combat.rs:1423-1441` |
| Skill uses s26 (0 = unlimited), cost per use s24, slot cost cached at capture | ✅ [R] | ✅ | ~ | ~ | ❌ | `FUN_140262670` (uses = s26, 0 -> unlimited), `FUN_1402623b0` (cost = s22 + s24*uses, status 30 halves/doubles, cap 99) vs `combat.rs:1375`; mines and Relearn do not consume at fire (mine consumes when it goes off) `[R 1402626d0, 1402635b0]`, ours consumes at press |
| Retain s41 = 1 / lock s41 = 2 | ✅ [R] | ~ | - | ❌ | ❌ | `FUN_140262fc0/140263460`: when a use-up / erase would clear a Retain skill and Aura covers it, the program **pays the cost again now** and keeps it; status 31 disables. Ours keeps it without re-paying (`combat.rs:1441`). Only 3 Retain skills; Photon Barrier s26 = 0 so moot |
| Requirements s38/s39 | ✅ [R] `FUN_140262470` | ✅ | - | ~ | ❌ | codes 1,2,5,7,8,9 built; 3,4,6 unused in data. "Skills left" counts capsules lying/pending too, ours counts only the undealt deck (`combat.rs:1357`) |
| STR rules (s45, s47 x uses) | ~ (table at `0x1402dd0a0` unread) | ✅ | - | ~ | ❌ | notes-based; STR is fixed **at fire** in the original (`1402626d0` top), at hit in ours (`combat.rs:1741`), differs only if Level/Aura change in flight |
| STR vs DEF, Pierce, Anti-Pierce (arithmetic) | ✅ [R 140271760] | ✅ | ~ | ~ | ❌ | `barrier.rs:145-165` |
| Guard lifecycle (drain, 5-tick linger, break erases skill, per-kind ignore lists, geometry) | ✅ [R]+[PR] | ❌ | - | ❌ | ❌ | section 3 items 2, 14 |
| Hit reactions, knock-down, get-up invulnerability | ✅ | ✅ | ✅ | ✅ (tests from reads) | ❌ | `combat.rs:1841-1965`, MOVES "Hit reactions"; slide decay is per 60 Hz tick (item 6) |
| Air-hit drop damage | ✅ [R] | ❌ | - | - | - | `FUN_140254c80`: 0 / 1 / 2 by fall height (<40 / 40-80 / >80 u) in battle phase |
| Status system (slots: permanent value + one timed amount + timer, caps, on-hit clears) | ~ [R] | ~ (Vec of stacking mods) | - | ❌ | ❌ | `FUN_14024d5c0/650/2f0/4d0` vs `effects.rs:86-130` |
| Aura refill | ✅ [R] | ❌ flat | - | ❌ | ❌ | item 3 |
| Level (start, cap), Aura Particle +1 | ✅ [R] | ~ cap 16 | ✅ | ~ | ~ start 0 from a fan guide + Laurence "granular settings" | item 8 |
| Start health 20 | ✅ [R] gsdata `0x33040` mode rows (modes 0-2: health 20, Level 0..99, time 10 min) | ✅ | ✅ | ✅ now | "Laurence's pick" | was an unconfirmed pick; the table agrees |
| Capsule timing/pickup (3 spots per player, 420 + 15 ticks, same spot, next card of the 30-card deck) | ✅ [R] | ~ (900 sim ticks, instant) | ✅ | ❌ | ❌ | item 7 |
| Environment crystal | ✅ [R] | ~ (by skill id) | ~ | ❌ | ❌ | item 4 |
| Match timer / countdown / rounds | ~ [R] | ✅ | ✅ | ~ | Laurence: 1 round default, healthier wins | section 4 |
| Fall-out (-3 Health, then respawn) | ✅ [R] `FUN_14024ec80 -> 140251d80(-3)` | ~ | ✅ | ✅ damage | ❌ | no 90-tick respawn invulnerability (item 12) |
| Combos (w45-w47, s27-s36 chain list) | ✅ [PR] | ❌ | - | - | - | High-speed Punch 13, Rapid Cannon 14, Trench Mortar (3 player skills) |

### 2b. Attack families (156)

| Family (count; ids in appendix) | Mapped | Impl | Repro | Verified | Evidence (what differs) |
|---|---|---|---|---|---|
| Shell, motion 0 (51) | ✅ [PR] | ~ | ✅ (Blaster) | ❌ | speed 3 vs 2.78 u/s per pt; homing 0.1 rad/s vs 3 deg/s; range 900 vs s64 (Blaster 300); no 60 deg cone; Psycho Sniper ramps, does not wait (`shell.rs:34`); s44 band absent; fire origin chest, not SHOT01 marker |
| Melee: sword 12 (14), punch/kick/palm/wave 13 (22), Pursuit 14 (2) | ✅ [PR] | ~ | ✅ (Knife) | ~ timing ✅ geometry ❌ | dash/hit windows from row ✅; hit = flat reach test from feet (`combat.rs:1655-1658`) vs 13 u sphere / blade segment; dash 240 u/s flat vs s65-based with turn cap; Pursuit single test tick 19 |
| Arc, motion 6 (11) | ✅ [PR] | ~ | ✅ (Laser) | ❌ | flat circle aimed where target stood vs tilted plane (s68) re-aiming at s66; fan by yaw vs roll s71; counts from s69 vs SHOTnn markers; **no range band: Optical lasers cannot hit closer than 300 u in the original** |
| Parabola, motion 1 (11) | ✅ [PR] | ~ | ~ | ❌ | `parabola.rs:37` reads s66 as steering (it is arc height); gravity 150 fixed; 0.5-1.6 s flight clamp; no band |
| Fall, motion 2 (6) | ✅ [PR] | ~ | ~ | ❌ | straight down 150 u vs diagonal s64 up / s65 back; speed 3 vs 2.78; no steering |
| Crawl 5 (6), Tornado 10 (3) | ✅ [PR] | ~ | ~ | ❌ | Tornado folded into Crawl with homing 1.5 (program: none, ramp s67->s68, hits caster after 30 ticks, launches up) |
| Rain: cloud motion 20 (9), column motion 7 (3) | ✅ [PR] | ~ | ~ | ❌ | instant disc vs 100 u cloud following at 42 u/s with 6 random drops; column fields misread (s65 radius = active ticks) |
| Hole 15/16 (6) | ✅ [PR] | ~ | ~ | ❌ | no sliding marker, no 10-tick window, motion 16 "hole under every player" |
| Mines 8 (4) | ✅ [PR] | ~ | ~ | ~ fuse/radius ✅ | mine blast and Sensor hurt the **owner** in the original; ours opponent only |
| Around-self 9 (3: Photon Wave 63, Photon Burst 64, Fast Wave 325) | ✅ [PR] | ~ | ~ | ~ | fires tick 3, blast grows over 30 ticks; ours instant sphere at row tick |
| Blasts / "Explodes on impact" s51 = 5 (7) | ✅ [PR] | ~ | ✅ | ❌ | sphere grows 0 -> r over 30 ticks, wall-impact blast hurts the thrower; ours instant |
| [Object] 11 (Power, Psycho Kinesis) | ~ | ~ | ~ | ❌ | `props.rs` constants own guesses; table lift/ramp/turn cap read in PR |
| Ricochet 17 (Ricochet Laser) | ✅ [PR] | ~ | ~ | ❌ | yaw fan +-45 vs X pattern by roll/yaw (s68/s69/s71/s72), bounce reflects a wall normal, turns at target on bounce s73 |
| Charge: Gungnir 73, Ki Cannon 204, Ki Palm 195 | ✅ [PR] | ~ | ~ | ~ rate ✅ | no 13-tick (Ki Palm 7) wind-up before counting; chosen by heuristic (s45 = 21 and cost 0) not s37 = 2 / w39. **Dragon Bullet 334 has w39 = 13** (charge row) and is not treated as charge |
| Attack Add effects (s51: 1,5,6-11,12-16,18) | ✅ [R 140272bc0] | ✅ | ~ | ~ | code table read; ours matches for the codes present in the roster except permanence (item 10) |
| Erase-on-hit / erase-equipped s48-s50 (12 attacks + 14 erase skills) | ✅ [R 140272510] | ~ | - | ❌ | item 9 |
| Recollection 206 / Copy 205 (s43 = 6) | ✅ | ✅ | - | ❌ | resolved at press as the copied skill; the registry marks them PLACEHOLDER but `cast` intercepts |
| Enemy-only records in the catalogue (11 dead + 3 Diffused Laser) | - | placeholder shot / nothing | - | - | should be out of the roster (section 1) |

### 2c. Defence (43)

| Family | Mapped | Impl | Repro | Verified | Evidence |
|---|---|---|---|---|---|
| Barrier 0 (15), Brush 3 (5), Reflect 4 (2), Absorb 5 (5) | ✅ [R]+[PR] | ~ | ✅ | ~ STR arithmetic | guard = 13 u disc 5 u in front in the original, half-plane in ours (`barrier.rs:138`); time s46-1 vs s46 x 2 (`barrier.rs:96`); Absorb/Brush "do not stop" lists missing except Absorb vs melee (`barrier.rs:156`); Brush stun 45 vs 60 sim ticks (`barrier.rs:59`) |
| Shelter 1 (7), Auto Shelter 7 (4) | ✅ | ~ | ✅ | ~ | auto fires through the normal fire path incl. payment (`FUN_14026f530` [R]) ok in shape; auto does not trigger for motion-4 instants / s54 = 2 (ours triggers for any strike `combat.rs:1758`) |
| Course 2 (3), About Face/Reverse 6 (2) | ✅ [PR] | ~ | ✅ | ❌ | Course = steer-away flag, stops nothing; ours bends 90 deg and makes the shot harmless (`combat.rs:2134-2151`); About Face only listed motions |
| Guard Add effects (s51/s52, s50 short-range only) | ✅ [R] | ~ | - | ~ | s50 filter applied to erase only; code 6 missing |

### 2d. Erase (42), Status (33), Special (37), Environment (29)

| Family | Mapped | Impl | Repro | Verified | Evidence |
|---|---|---|---|---|---|
| Erase, capsule field s43 = 1 (9) | ✅ [PR] | ✅ | - | ~ | Change the World (s45 = 2 swap) erases instead |
| Erase, equipped skills s43 = 0 (14 + 12 attacks) | ✅ [R] | ~ | - | ❌ | item 9 |
| Erase, attribute s51 codes (Level, Aura, reset, crystal) | ~ | ✅ | - | ❌ | s51 1,2,4,10,17,38,39,47,49 are the program's Add codes (`FUN_140272bc0`) and ours agree by note; Level cap 16 |
| Status, self s43 = 4 (14) | ✅ [R 140270b00] | ✅ by s42/s45 | - | ~ | program: up to 4 effects `kind s45+k, amount s49+k, duration s53+k`; ours one selector; durations match notes |
| Status, targeted s51 (19) | ✅ [R] | ~ | - | ~ | permanence/stacking (item 10) |
| Special, self s43 = 4 (21) | ✅ [R] | ✅ by skill id | - | ~ | Regenerate +1 Health / 91 ticks vs ours 60 sim ticks; Bravery Decision (kind 22 "fill deck" per PR vs note "erases Aura Particles") and Mephisto's Pact (kind 20) unsettled |
| Special, movement s43 = 3 (Jump, Dash, Skywalk, Levitate, Retreat, Call, Return, Charge, Surprise Attack) | ✅ [PR] | ~ | - | ❌ | Dash is a teleport in ours (program 100 u over 15 ticks, locked 20); Jump uses apex 100 u |
| Special, targeted s51 41-51 (Dazzle, Trigger, Fusion...) | ~ [R codes 41-45] | ✅ by note | - | ❌ | Dazzle = status 36 + input scramble `[R]`; Trigger stores button `[R 0x2a]` |
| Environment (29) | ✅ [R] | ~ by id, 26 of 29 | - | ❌ | item 4; **Memory Lapse, Necronomicon, Unison not built** (Memory Lapse is NOT "already true": 204 of 340 skills have s26 = 0) |

## 3. Discrepancies (most important first)

CONFIRMED = I read the program side and our side. SUSPECTED = one side only or semantic uncertainty. Effects are what a player sees.

1. **Payment at press vs at fire. CONFIRMED.** Program: `FUN_140258180` starts the cast (no payment); `FUN_1402626d0` (the fire function, runs at w43) begins with `FUN_1402621f0` = pay (Aura >= cost, Level >= s23 then -= s23, Health -= s25) and returns without creating the object if it cannot pay; uses decrement and poison/Backdraft health loss also happen there (`FUN_140263410`, status 6 and 17 at the top). A hit before the fire tick clears the countdown (`PR 140258690`), so the cast never pays. Ours: `combat.rs:1423-1441` charges Aura, Level, Health and the use at the press; the test at `combat.rs:3120` only asserts "no shot". Effect: interrupted wind-ups (0.67-0.83 s for most moves) lose the skill, Aura and use in ours, nothing in the original. Closes PR open question "does an interrupted cast spend the skill".
2. **Guard break and drain. CONFIRMED (program side also `[PR]`).** `FUN_140271760`: each block does `guard.STR -= attack.STR`; below 0 the guard breaks (impact 10), its Add runs, and **`FUN_140262fc0` erases the defence skill from the holder's button** (Retain/lock rules apply); a successful block ends the guard 5 ticks later (impact 9) and the DEF it lost stays lost in that window. Ours: `barrier::resolve` never mutates the guard; a block does nothing to it; a break only sets `guard = None` and stuns half a hit (`combat.rs:1774-1790`); any damage also drops the guard (`combat.rs:1904`, invented). Effect: a Barrier blocks unlimited shots for its full time and survives being broken.
3. **Aura refill. CONFIRMED.** `FUN_14024de20`: per 30 Hz tick `+ (100 + status3)/100 x handicap x max(0.25, 1.25 - Aura/10) / 30`, capped at Level (constants `0x1402f7ddc` 1.25, `0x1402f7f7c` 10, `0x1402f7d0c` 0.25, `0x1402f7fd8` 30) = 1.25 pts/s at 0 Aura, 0.75 at 5, 0.25 from 10. Handicap = gsdata `0x33034` [1.3, 1.0, 0.7] indexed by (the deck record's u16 at +0x50) - 1 (`FUN_140214770`, `FUN_140187710`); `FUN_14015bbf0` treats +0x50 = 1 as a single-school deck, so this is very likely the school count (1 school 1.3, 2 schools 1.0, 3 schools 0.7; SUSPECTED on the exact meaning of +0x50). Paused at health 0 and in player states 3-4 (dodge/dash/jump wind-up) and 10-13 (**the whole cast clip**); under status 29 only idle states refill (`FUN_1402523a0` [R]). It also plays sound id 5 (`FUN_140276330(0,5,..)`) on each whole point gained by a local player. Ours: flat 1 pt/s (`combat.rs:34,1183`), school rate +20/0/-20 percent "placeholders" (`arsenal.rs:26`) instead of +30/0/-30 percent, no pauses. Effect: early game is slower/faster than the original depending on Aura, and casting never stalls the refill.
4. **Environment crystals are data, and capture grants an Aura Particle. CONFIRMED.** `FUN_140271660` adds `s46` to status `s44` while the active crystal stands; `FUN_14017bcc0` records `expiry = now + s47` when s47 > 0 (seven skills, 1800 ticks); `FUN_14017be00` allows capture only when `now > created + s45` (450 ticks = 15 s) and **assigns record 0 (Aura Particle) to the pressed button** then removes the crystal; `FUN_14017c4c0` = the newest non-expired crystal, so a new one can be set after expiry. Ours: skills keyed by id (`environment.rs:20-189`), the crystal never expires (ban windows end but the crystal keeps blocking new environment skills, `combat.rs:1586`), capture just clears it (`combat.rs:1286`). Full table in appendix B. Also from `FUN_140262d80` (the ban evaluator): "cost X" = **s37 == 2** (Gungnir, Ki Cannon, Ki Palm), not cost >= 90 (`environment.rs:62`), so Reduce Entropy lets those three through in ours; Forest Sanctuary/Athena also ban Power/Psycho Kinesis (object motion) and any attack whose s45 is non-zero; Fatigue (status 29) is state-based (states 0, 21, 22, 24 refill) not position-based; Quiet Days/Accelerate add +450/-225 ticks to the 420-tick respawn (not exactly x2 / x0.5). Status 31 (Necronomicon) consumers for the unbuilt skill: Retain off (`140262fc0/263460`), auto-Shelter trigger (`14026f530`), attack Add effects skipped and s54 zeroed (`14026f870`, `1402723b0`, `140271760`). Memory Lapse = status 32 sets uses to 0 on use unless s41 = 2 (`140263410`).
5. **Costs can kill; Level gate missing. CONFIRMED.** `FUN_14024d7a0` (used for every Health change) sets health 0 and kills at < 1, including Health cost s25 (5 skills with 3), poison (-1 on capture and at fire), Backdraft/Soul Cage. Ours clamps to 1 (`combat.rs:1303,1429,1435,1992`). Payment also requires **Level >= s23** (`1402621f0`): Mist Blade/Laser/Blaster, Charged Particle, Restrain, Level Baton (Level penalty 1) cannot be cast at Level 0; ours casts them (`combat.rs:1433` just floors at 0). No record uses s25 = -1 or s22 < 0 [D], so those program branches are moot.
6. **Air-hit drop damage and knock-back slide. CONFIRMED.** `FUN_140254c80`: a knocked-down player landing from state 17 takes **0 / 1 / 2 damage for a fall of < 40 / 40-80 / > 80 u** (`0x1402f7ff0` = 40, `0x1402f8040` = 80), battle phase only; ordinary jump landings take none. Ordinary ground knock-downs peak ~8 u (no damage); Tornado (~89 u) and high air hits do. Ours: none (MOVES "amount unknown"). The post-landing slide multiplies speed by 0.85 **per 30 Hz tick** (`0x1402f7d8c`, cutoff 0.02, `FUN_14025c260`); ours multiplies per 60 Hz sim tick (`combat.rs:1199`) so the slide is about half as long (~4 u instead of ~8 u, from arithmetic, not measured).
7. **Capsule respawn and pickup. CONFIRMED.** `FUN_14014fd10` on take/erase: slot state 0, timer **420 + status 34** ticks (`0x1a4`), draws the next card of the 30-card deck immediately; `FUN_14014ed10`: at timer 0 state 1 for **15 ticks** (appear sound 0x55) then state 2 (pickable) at the same spot. Total 435 ticks = **14.5 s** (870 sim ticks). Ours 900 sim ticks, pickable at once (`combat.rs:30,2346`). Reach (`FUN_14014f630`, constants `0x1402f7fb0` = 15, `0x1402f7f64` = 9): 3-D distance from the capsule's ground point (y - 9) to the player **<= 15 u**; ours 12.5 u flat with a height gate (`combat.rs:43,1346`). The press-to-capture design is right: `FUN_140251550` sets "capsule in reach" (mode 1) / crystal (3), `FUN_1402517c0` captures on a skill-button press in states 0/1/8 only; ours allows any Ready state incl. airborne. (So `CAPSULE-VISUALS` "walks into the orb" is wrong for capture.) Capsule sounds seen on the way: appear 0x55, capture `FUN_140261a00` tail 0x56 (when its 4th argument is non-zero), erase 0x5c, crystal set 0x5d / capture 0x58, hit 0x5b, land 0x52 (for P1).
8. **Level cap and school handicap. CONFIRMED.** Level is clamped to `0..gsdata[0x33044 + mode*0x10 .. 0x33048]` = **0..99** for modes 0-2, 5-9, 11 (`FUN_14024d970`); no record has 16. Ours `MAX_LEVEL = 16` (`combat.rs:31,1468`, Laurence's early guess). Handicap amounts in item 3.
9. **Erase-equipped selection. CONFIRMED.** `FUN_140272510` (+ `140273a80/b80`, `1402739c0`): s48 picks the slots (1-5 by capsule colour, 6 Aura Particles, 7 non-Aura, 8 colours >= erase, 9 any); s49 orders them: **0 random, 1 highest cost first, 2 lowest cost first (ties random), 3 shuffle then keep only s26 = 0 (Lack), 4 only the slot of the pressed button**; s50 = how many of the list (from the front) are erased (`FUN_140262fc0`). Ours treats s49 = 2 as "random" except Disremember and s49 = 1 as "highest" only for s48 = 9 (`erase.rs:44-50,282-316`). Affected (11): Tiger Slayer 216, Dragon Slayer 217, Lightning Sword 122, Lightning Blade 135, Dragon Punch 187, Tiger Kick 184, Storm Blade 134, Lightning Bolt 142, Piece of Thunder 128, Piece of Wind 309 (all s49 = 2, s50 = 1: the **lowest-cost** skill of that colour) and Shield Breaker 324 (s49 = 1: the highest-cost defence). The "erase all" ones (s50 = 4) are unaffected by ordering.
10. **Status slots. CONFIRMED.** `FUN_14024d5c0/650`: a status has a permanent value (added, clamped to +-9 attack, +-90 speed/homing/aura rate) and **one** timed amount + timer (a new timed application **replaces** it; duration 0 = permanent until reset). Ours pushes every application onto a list (`effects.rs:117`), no caps, so Tiger's Strength twice = +4 (program +2) and permanent buffs/debuffs never cap. Frostbite (s52 = 0) is permanent in the original, 10 s in ours (`status.rs:172`). Getting hit clears statuses 8 (invisible), 11 (float) and 12 (buttons locked) (`FUN_14026f870` tail), so Optical Camo and Skywalk end on a hit; ours survive (`combat.rs:1901`).
11. **Post-respawn invulnerability. CONFIRMED (85 %).** `FUN_140251d80(player, -3)` (fall-out) sets the invulnerability counter to 0x5a = **90 ticks (3 s)** (same field as get-up invulnerability); ours none (`combat.rs:1965`).
12. **Regenerate / powerless drain. CONFIRMED.** Regenerate (state 21) +1 Health every 91 ticks (`FUN_14025b5e0`, `0x15`) = 182 sim ticks; ours 60 (`special.rs:127`). Powerless drain: -1 Health per 91 ticks while "skills left" (deck + capsule slots still pending) is 0 (`FUN_14024df30`, `14014ff80`); it does **not** look at the hand, ours requires empty slots too (`combat.rs:1107`) and counts 180 sim ticks. SUSPECTED that the hand really is ignored.
13. **Guard details (program `[PR]`, our side `[C]`).** Time s46 - 1 ticks, caster held by the cast clip not the guard time (`combat.rs:1419,1474`); disc geometry and sphere kinds; per-kind "does not stop" lists (Barrier/Reflect: Rain, Tornado; Absorb: blasts, all short-range, Rain, Tornado, mines, around-self; Brush: blasts, Rain, Tornado, mines, around-self; About Face only motions 0,1,2,5,6,11); Brush stun 1.0 s (reaction 26); DEF = s47 + s48 x uses (Failing Wall, Dark Protection, Protecting Air are s43 = 2 skills in ids 341-374, so moot for the roster); s50 short-range filter on all Adds; Add code 6 missing.
14. **Projectile and area families still on 2026-09-26 readings (`[PR]` vs `[C]`).** Section 2b. Units alone (`moves/mod.rs:39,43`) move every shell. The range band (s44 0 = 80-300 u, 1 = 300-600 u, 2 = free; `FUN_140273d40`) is implemented nowhere [C: no attack reads s44]. Blast growth, owner damage, Rain/Hole timelines, Arc/Ricochet geometry, SHOT01 spawn points are as the cloud audit `AUDIT-ATTACKS.md` lists; I re-read the family modules (`shell/parabola/arc/crawl/fall/ricochet/area/melee.rs`) and `move_projectiles`/`tick_areas`/`lunge`, and its "ours" column is still true. Its program column is `[PR]`, not re-read by me.
15. **Roster/catalogue. CONFIRMED.** Section 1. Builder and randomiser can put enemy-only records into a deck (`main.rs:442`, `frontend/discovery.rs:114`).
16. **Smaller, read both sides.** STR fixed at fire (above); Retain re-pays; mines/Relearn do not consume at fire; capture only in states 0/1/8; Aura Particle use has no cast animation in its row (WAIT0, fire 0): our 12-tick hold + 79-tick consume lock is an observation (Laurence), not from the row; Dragon Bullet not a charge skill in ours; `is_cost_x`; Forest Sanctuary STR uses the stored slot STR (includes statuses) not raw s46.

## 4. Match rules (round / timer / level / aura / capsules)

| Rule | Program | Ours | Verdict |
|---|---|---|---|
| Start health | 20 (gsdata `0x33040`, modes 0-2,5-9: 20; mode 3: 10, mode 4: 40, mode 10: 5, mode 11: 10) `[R]` | 20 (`combat.rs:21`) | ✅ matches; was "Laurence's pick" |
| Level | clamp 0..99 (`FUN_14024d970`); start Level is passed in from match setup (default unread) | start 0 (setup option, fan guide), cap 16 | cap differs (item 8) |
| Aura | item 3; Aura starts from the setup struct (`FUN_14024e790` copies setup aura and Level) | starts full at Level | curve/handicap/pauses differ |
| Capsule respawn | 420 + 15 ticks, +status 34, same spot, next deck card, none once the deck is out | 15.0 s flat, instant, same spot | "15 s unconfirmed" is now **read: 14.5 s** (item 7) |
| Timer | `minutes x 0x708` (1800 ticks/min) from the setup struct `+0x356`; <= 0 = no limit (`FUN_140129260`); gsdata mode default 10 min | 5/10/15/none, default 15 (guide) | menu values are the original's setup menu (unread); default differs |
| Countdown / round gap | phase 5 counts 90 ticks (`FUN_14012ade0` = 0x5a); between rounds phase 7 also 90 ticks (`FUN_14012c510`) | 180 / 180 sim ticks | ✅ equal (3 s) |
| Rounds | count from setup (`DAT_1404bc9b0`), decrement per finished round | 1/2/3/endless | ✅ shape; default 1 from fan guide |
| Time-up winner | not read (`FUN_14012c510 -> 14012c790/14012c440`) | healthier wins (Laurence) | open (R2) |
| Fall-out | -3 Health, respawn, 90-tick invulnerability | -3, none | item 11 |
| Drop damage | item 6 | none | differs |
| Game modes | `DAT_1404bc5dc` mode table (12 rows); mode 10 = "BattleRoyal" strings, mode 5 disables environments, mode 1 has a special end check | Brawl/2v2 not built | for P8: start from gsdata `0x33040` rows |

## 5. PLAN M5 / M6 / M11-M14 against current code `[C]`

| Step | Status | Notes |
|---|---|---|
| M5 range bands, guard shapes/durations/STR-DEF | **not done** | bands absent; guard geometry/time/lifecycle absent (items 2, 13, 14). Only the STR-vs-DEF arithmetic is in |
| M6 per-family motion fixes | **not done** | none of lob s66/s67, diagonal Fall, Tornado no-homing, Rain cloud, crawl turn accel, mines hurt owner, Ricochet X, SHOTnn, homing x0.52, speed 2.78 is in; `mv/*` comments still say "unconfirmed" |
| M11 dodges + movement skills | partial | dodges live in `sim.rs` (locked-on lane, movement audit); Jump/Dash/Skywalk/Levitate built by note (Dash teleports) |
| M12 root motion in cast/reaction | not done | `sim.rs`/`combat.rs` do not add clip root motion (PR lists ATK_KICK +38 u etc.) |
| M13 aura refill curve + pauses, 14 s capsule respawn, 30-card deck | **not done** except the 30-card deck and stage spots | items 3, 7 |
| M14 combos, charge release 12 ticks, melee 80 u gate, Change the World swap, status kind table | partial | charge release 12 ticks, 80 u gate built; combos, swap, status kind table not |

## 6. MOVES.md stragglers

| Item | State now |
|---|---|
| Lack's unlimited-use filter | **Built and program-correct**: s49 = 3 is literally "shuffle, keep s26 = 0" (`FUN_140272510`), s50 = 4 |
| Necronomicon | not built; rule now known (item 4: status 31 disables Retain, auto-Shelter trigger, attack Add effects, s54) |
| Memory Lapse | not built; MOVES says "already true" (false). Rule: status 32 -> uses = 0 on use unless s41 = 2 |
| Unison | not built; record is empty (s44 = 0, s46 = 0): probably a 2-v-2 feature |
| Traveling Thought (371, id > 340) | out of roster |
| Diffused Laser (720-722) | enemy-only, catalogue bug not a move bug; data: motion 17, s64 -120, ramp s65->s66 over s67 = 120, bounces 3 |
| Air-hit drop damage | **answered**: 1 (40-80 u) / 2 (> 80 u) (item 6) |
| Knock-back distance | flight physics right; slide decays per 60 Hz tick instead of 30 Hz (item 6) |
| Downcast 354, Phantom Dust 374, Bind Shot 739, Vapor Cloud 343 | ids > 340 or enemy; out of roster |
| Heat Devil 360, Decoy 172, Mind Reading 231 | Heat Devil out of roster; Decoy s51 40 = status 9 on the target (2-v-2 only); Mind Reading kind 17 = status 37 consumed at `FUN_140187180/14027abe0` (unread) |

## 7. Guess vs read, by family (what a change must not break)

- **Read from program/table and implemented right**: action-row timing, melee dash gate (80 u, lock), hit-reaction pick (3 s damage window), knock-down physics and timeline, STR vs DEF arithmetic, Lack/erase-on-hit encoding (except item 9), attack Add codes 1,5-16,18, STR rule notes, s26 uses rule, requirements codes, start health, countdown/round gap, fall-out damage, crystal 15 s lock numerically, movement skills apart from Dash, mines fuse/radius.
- **Read from the program but not implemented (or implemented on the old reading)**: items 1-14 above.
- **Pure guess in ours** (no program value behind it): `PICKUP_RADIUS` 8, `AURA_ABSORB_TICKS` 12 (program: row 343 is empty), `CONSUME_LOCK_TICKS` (Laurence observation), `MAX_LEVEL` 16, `school_rate` +-20, `REGEN` 60, `POWERLESS` 180, `BRUSH_STUN` 45, `BRIEF_BIND_TICKS` 30 (Frighten/Sword Slap: their real s65-delay and state are unread), guard `HIT_STUN_TICKS/2` on break, Dash/Jump "30 ft" = 109.8 u teleport, `LUNGE_SPEED`, parabola flight clamp/gravity, `MINE_LIFE_TICKS`, Rain height, `OVERSHOOT`, `MAX_BEND`, brushed-shell 300 u aside, reflected shell homing 1.5, hit-stun 30 ticks for reaction kinds 0-3,7, `is_cost_x`.
- **Implemented by skill id / note text where the program is data-driven**: all 29 environment skills, ~21 self specials, Orb/Amulet, Skywalk/Demon's Wing/Doppelganger, Trigger/Annoying Gift/Fusion/Teleportation, Frighten/Sword Slap. Correct in effect for most; they need re-keying to (status id, amount) / (kind, amount, duration) so the next skill cannot be wrong by name.

## 8. Doc corrections this audit implies (not done; I edit nothing else)

`FORMATS-SKILLS.md`: environment rows s44 = status id, s45 = capture lock ticks, s46 = amount, s47 = lifetime; s37 = 0 normal / 1 auto-only (Orb, Amulet) / 2 hold-to-charge (cost X; Gungnir, Ki Cannon, Ki Palm); s48-s50 erase ordering (item 9); s51 Add codes 17-37 = "apply status id (code - 1..)" with caps; s23 is also a Level gate. `MOVES.md`: Memory Lapse, "table holds no numbers", Regenerate 1 HP/s, ~150/164. `PROGRAM-RE.md`: status 35 adds 0x4000 of 0x8000 units = **180 degrees** (inversion, not 90); the Aura Particle is record 0. `CAPSULE-VISUALS.md`: pickup is a button press. `AGENTS.md`/`GAME.md`: "all defence kinds" built, Level cap 16, capsule every 2 s.

## 8b. Leads for other auditors (seen on the way, not my area)

- Sound (P1): the refusal/"cannot use" cue is `FUN_140276330(0,2,..)` (bank 0 id 2) at `FUN_140258180`, `140261e90`, `1402517c0` (ours is a synthesised buzz); Aura gain tick = id 5 (`FUN_14024de20`); capsule appear 0x55 (`14014ed10`), capture 0x56 (`140261a00` tail, needs its 4th argument non-zero: check the capture call site in `1402517c0`, which shows three arguments), erase 0x5c, crystal set 0x5d / capture 0x58 (`14017bcc0`, `14017be00`), hit 0x5b (`14026f870`), knock-down landing 0x52 / lying 0x51 (`140254c80`, `14025c3d0`), blast 99.
- Aura (P5): the school-glow pulse when a skill is paid is `FUN_1400b9580(ctrl, school)` called from `FUN_1402621f0` (school = s11 > 0), i.e. it fires **at fire**, not at the press.

## 9. Reads still needed

| # | Question | Start from |
|---|---|---|
| R1 | What defines the collectable list / what a deck builder may offer (ids 341-374? 700s?) | the deck/collection UI (strings `DckMenu`); record fields s4, s5, s6; `DAT_1404bdc50` scans over 365 rows |
| R2 | Time-up winner, draw rule, per-round score, what mode index versus uses (1? 2?) | `FUN_14012c510 -> 14012c790, 14012c440, 14012b300`; `DAT_1404bc5dc` writers `140187710`, `1401d0750` |
| R3 | Default start Level / start Aura and the 4-card opening hand | `FUN_140187710` args into `FUN_14024e790` (param_3 +0x18, +4); `FUN_14014ee70` |
| R4 | Guard contact test and linger (verify PR) | `FUN_14026dcb0`, `140271760` tail, impact `14026cd70` codes 9/10 |
| R5 | s45 STR rule functions | table `0x1402dd0a0`, `FUN_140272400` |
| R6 | Status 37 (Mind Reading), 11/12 clear, 23-28 bans unused?, 24 | `FUN_140187180`, `14027abe0`, `1402569b0` |
| R7 | Which skills can be cast airborne (s40 = 0 in the air) | `FUN_140261e90` vtable `+0x40` ("can use now"), `FUN_14025b0f0` |
| R8 | What the Aura Particle use shows (row 343 is WAIT0) and where SPE_POWER_UP / SPE_HEAL are chosen | `FUN_140270b00` kinds 1, 3; `FUN_14025a8a0` |
| R9 | Mines/blast owner damage, blast growth, hole/rain timelines (verify PR before building) | `FUN_14026dbd0`, `14026ae10`, `14026a7f0`, `14026b8d0` |
| R10 | Combo chain cost (does each chained hit pay) | `FUN_14024fb90`, `FUN_14025a3e0` |
| R11 | Whether the hand counts toward "powerless" | `FUN_14014ff80` callers; program behaviour vs footage |

## 10. Proposed build lanes (ordered)

**Lane S1, rules core** (`combat.rs` functions `cast`, `fire`, `tick_fighter` (refill, knock slide), `step` (powerless, timer), `spawn_capsule`, `capsule_in_reach`, `hurt`, `fall_out`, `start_round`, constants; `arsenal.rs::aura_rate_pct`; `main.rs::load_skills` roster filter). Goal: items 1, 3, 5, 6, 7, 8, 11, 12, 15. Tests cite addresses: payment happens only at tick `fire` and not when a hit cancels (`1402626d0`); refill per tick matches the formula at Aura 0/5/10 with handicap 1.3/1.0/0.7 and pauses in cast/dodge (`14024de20`, `1402523a0`); capsule timeline 420 then 15 ticks then pickable, reach 15 u (`14014fd10`, `14014ed10`, `14014f630`); Health cost kills; Level gate; drop damage 0/1/2 at 40/80 u (`140254c80`); slide factor per 30 Hz tick; fall-out 90-tick invulnerability; catalogue has exactly ids 1-340. Runtime proof: scripted match with `PD_SHOT_TICKS`: Laser interrupted at tick 10 leaves Aura/slot intact; log of Aura vs time from a fresh start matching 1.25/s; capsule respawn log 435 ticks. Do first; everything else builds on it.

**Lane S2, guards, erase, status, environment** (`moves/barrier.rs`, `moves/environment.rs`, `moves/erase.rs`, `moves/status.rs`, `moves/special.rs` (Regenerate), `moves/effects.rs` (Status/Crystal model); in `combat.rs` only `strike`, `auto_guard`, `guard_extra`, `brush_away`, `send_back`). Goal: items 2, 4, 9, 10, 13, guard lifecycle, crystal table (status id/amount/expiry/capture -> Aura Particle), Memory Lapse, Necronomicon, status slot model with caps, on-hit clears. Tests: guard drains and breaks and erases the skill (`140271760`, `140262fc0`); environment table appendix B as data tests; erase ordering for the 11 skills in item 9; Frostbite permanent; Tiger's Strength twice = +2. Runs in parallel with S3 after S1 merges (function-disjoint in `combat.rs`).

**Lane S3, projectile and area families to the read numbers** (`moves/mod.rs` units, `shell.rs`, `parabola.rs`, `arc.rs`, `crawl.rs`, `fall.rs`, `ricochet.rs`, `area.rs`, `melee.rs`; in `combat.rs` only `move_projectiles`, `tick_areas`, `lunge`, the launch half of `fire`). Goal: item 14 in PLAN M5 (bands) and M6 order: units + shell range/cone/ramp + bands first, then lob/fall/crawl/tornado, then Rain/Hole/mines/blasts, then Arc/Ricochet (needs SHOTnn markers from the effect files, `effects.rs` read-only). Tests: Laser at 150 u closes at 300 u and misses by ~26 u; Blaster life = 300 u / speed; Bomb peak 40 u from s66 (`140264b00`). Verify each family against `PR` first for the parts I did not re-read (R9).

## 11. Questions only Laurence can answer

1. **Roster for versus.** A) Ids 1-340 only (recommended). B) 1-340 plus the 34 boss skills (Impact Burst, Geyser Impact, Young White Dragon...). C) Everything in the file.
2. **Mines, bombs against a wall, Tornado hurting the person who set them** are real in the original (Sensor Mines are set off by anyone). A) Faithful, it hurts you too (recommended for fidelity). B) Keep "only the opponent is hurt" as a house rule.
3. **From the original, if you can check on the footage you have:** when you are hit during a skill's wind-up (before the shot leaves) is the skill and its Aura still there afterwards? A) yes (the program says so). B) no. C) can't tell.
4. **From the original:** when a barrier is broken by a stronger attack, does the barrier skill vanish from its button? A) yes (the program says so). B) no, it stays. C) can't tell.

---

## Appendix A. Player roster by behaviour family (ids) [D]

Attack 156: shell 14 15 16 74 75 76 78 79 80 81 82 83 127-137 198-204 246-257 305 306 310 314 318 320 334 337 338; melee 1 2 3 4 5 61 62 121-125 181-197 241-245 313 317 321 322; arc 65-71 126 309 326 329; parabola 6-13 72 73 301; fall 19 261-264 302; crawl 138 139 258 259 260 333; tornado 140 141 311; rain 20 21 84 85 142-146 265-267; hole 22 86 147 148 312 319; mines 23-25 323; around-self 63 64 325; object 17 18; ricochet 77; copy 205 206.
Defence 43: barrier 26-28 87-91 149-153 268 269; shelter 29 30 94 155 207 271 327; course 31 32 156; brush 209-211 272 273; reflect 92 93; absorb 212-214 274 275; about-face 33 34; auto-shelter 95 154 208 270.
Erase 42: self 223 285; everyone 37 102 103 159-162 280-282 339; capsule field 36 99-101 158 237 278 279 284; hole 104 283; mine 35 98; melee 215 324; shot/crawl 96 97 157 216-222 276 277 315 316.
Special 37 and Status 33 and Environment 29 (38-41 105-111 163-169 224-229 286-289 332): by delivery, see `moves/effects.rs::delivery`.

## Appendix B. Environment table decoded from the data (status id, amount, lifetime; capture lock s45 = 450 ticks on all) [D]

Status id -> program meaning `[R]` where consumed: 0 attack, 1 speed, 2 homing, 3 aura rate (caps +-9 / 90 / 90 / 90), 17 lose Health per skill use, 18 lose Health per capture, 19 +1 Health per Aura Particle capture, 20 Pierce for all, 21/22 attack bans by STR (amount 4: STR >= 4 banned; amount 2: STR <= 2 banned; "STR = X" banned, object motion banned), 25 bans erase colour, 29 refill only when idle, 30 cost halve (-1) / double (+1), 31 Necronomicon, 32 Memory Lapse, 34 capsule respawn wait (+ticks), 35 stick turned 180 degrees, 38 bans cost >= amount and X skills, 39 bans cost <= amount and X skills, 41-45 bans school Psycho/Optical/Nature/Ki/Faith.

| id | skill | status | amount | life |
|---|---|---|---|---|
| 163 | Desertification | 3 | -30 | |
| 105 / 286 | Holy Ray / Lunar Force | 0 | -1 / +1 | |
| 38 / 39 | Reduce / Abnormal Gravity | 1 | +20 / -20 | |
| 166 | Dense Fog | 2 | -30 | |
| 108 | Backdraft | 17 | 1 | |
| 109 / 225 | Optimization / Irregular Rhythm | 30 | -1 / +1 | |
| 164 | Nature Blessing | 19 | 1 | |
| 167 | Berserk | 20 | 1 | |
| 165 / 288 | Forest Sanctuary / Athena's Command | 21 / 22 | 4 / 2 | |
| 168 | Commandments (erase ban) | 25 | 1 | 1800 |
| 224 | Fatigue | 29 | 1 | |
| 227 | Soul Cage | 18 | 1 | |
| 226 | Memory Lapse | 32 | 1 | |
| 287 | Necronomicon | 31 | 1 | |
| 106 / 228 | Reduce Entropy / Chaos | 38 / 39 | 3 / 2 | |
| 111 / 107 | Quiet Days / Accelerate | 34 | +450 / -225 | |
| 40 | Confused Gravity | 35 | 1 | |
| 169 / 41 / 229 / 289 / 110 | Providence / Vanishing Light / Stagnant Air / Spiritual World / Scientism | 41 / 42 / 43 / 44 / 45 | 1 | 1800 |
| 332 | Unison | 0 | 0 | empty record |

Erase-equipped encoding: s48 filter (1-5 colour, 6 Aura Particles, 7 non-Aura, 8 colours >= erase, 9 any), s49 order (0 random, 1 highest cost, 2 lowest cost, 3 unlimited-use only, 4 pressed button), s50 count. Self s43 = 4 kinds seen in the roster: 1 Level+, 2 Aura+, 3 Health+, 4-7 attack/speed/homing/aura-rate, 8 reset statuses (Refresh, Angel Tears; not in the PR table), 10 Level x2, 11 Regenerate, 12 invisible, 13 invisible+float, 14 Void, 15 Invulnerability, 17 status 37, 18 copy statuses, 19 Relearn, 20 Pact, 21 Recall, 22 Bravery Decision.
