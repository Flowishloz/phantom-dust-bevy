# Guess inventory and doc contradictions

Read-only audit, 2026-09-30. Sources swept: AGENTS.md, docs/research/DECODING_RESEARCH.md, every file in engine/docs/ (handoffs/ not swept line by line), and `grep` over engine/crates/*/src for guess/assumed/unconfirmed/by eye/tuned/stand-in/placeholder/approximation comments plus the `const` lists of sim.rs, combat.rs, props.rs, arsenal.rs, sfx.rs, moves/*, effects.rs, fx.rs, school_aura.rs, stage_look.rs, main.rs (camera block). Nothing was run, built or edited. Line numbers are as of this date and will drift.

Completeness caveat: this is a sweep by marker words, not a full read of every document. Rows come from the marker lines and the code constants listed above. A guess that is written without any marker word will be missing. Roughly 230 rows follow.

Confidence column, in the project's own terms:
- `?` = the doc's unknown marker (question-mark glyph) or "not decoded / not read".
- `~` = the doc's warning-sign glyph: assumed, partly verified or inferred at one link.
- `U` = code or doc says "unconfirmed" or "guess" in words.
- `T` = tuned or matched by eye, no data behind the number.
- `P` = a placeholder value the project itself says to tune.
- `ok` = listed only because a doc shows it as still partly open; the core is confirmed.

"Settle" column abbreviations: PR = read the program (PDUWP.exe in Ghidra; address given where the docs give one), FOOT = film or measure Laurence's footage, EAR = listening test, WEB = public community source (wiki, guides, Esper-RE notes: facts only, never code), SHEET = A-F calibration sheet for Laurence to choose.

---

# Section A. Inventory of guesses

## A1. Movement (sim.rs, main.rs, FORMATS-ANIMATION.md, GAME.md)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| MOV-01 | Free camera never auto-turns behind a running player (design choice "like modern third-person games"), not the original's behaviour | sim.rs:592-598; MOVEMENT-AUDIT.md:53 | U | camera, every stick push in free mode | PR: camera follow code using `configdata.dat` floats (see CAM-01); FOOT of free-camera run |
| MOV-02 | `AIR_CONTROL` = 0.6 (air steering strength), no comment, no source | sim.rs:44 | U | jumps, knock-back steering, fall feel | PR: player update state 5 (airborne) near `FUN_14025ad50`; FOOT |
| MOV-03 | Tumble speed 130 u/s and tick length 30 | sim.rs:80-82 | U | ground dodge (JD, Cuff Button, pH roll) | PR already says one move for all 751 dodgers (PROGRAM-RE.md:474, 554); needs code change, then FOOT side view of a tumble (PROGRAM-RE.md:646) |
| MOV-04 | Cartwheel speed 180 u/s, 37 ticks, recovery 62 ticks | sim.rs:83-85; DECISIONS.md:38 | U | Edgar / Sammah dodge distance and lock time | Same as MOV-03. PROGRAM-RE.md:554 says 40 u scripted + root motion ~80 u total, locked 40 ticks |
| MOV-05 | Ground dodge distance: ~80 u = 40 u scripted + clip's own root-motion drift (~41-43 u) | PROGRAM-RE.md:474, 646 | ~ | dodge | FOOT: film Edgar cartwheel from the side |
| MOV-06 | Whether JD / Cuff Button "tumble" really locks 40 ticks like the cartwheel | PROGRAM-RE.md:646 | ? | dodge, Aura refill pause | FOOT, PR (DASHEND_A path) |
| MOV-07 | Which clip is the tumble's roll (DASHEND_A is played) | FORMATS-ANIMATION.md:83; main.rs:260 "Unconfirmed pairing" | ? | dodge animation | FOOT; PR clip selection for state 3 |
| MOV-08 | Dodge-jump airtime: program gives ~20 ticks (0.66 s), footage round 2 read 13-15 frames by eye | PROGRAM-RE.md:647; FOOTAGE.md:147 | U | Jump dodge timing | FOOT re-measure take-off and touch-down at frame level |
| MOV-09 | JUMP_UP suspect; looks-right-in-game unconfirmed | FORMATS-ANIMATION.md:34; ROADMAP.md:81 | ~ | jump animation | SHEET/FOOT |
| MOV-10 | Root motion: which clips use it (program flag at +0xcc from caller; idle 0, knock-down chains 1); our export strips root translation | PROGRAM-RE.md:424, 441; FORMATS-ANIMATION.md:52 | ? | ATK_SORD stride, dodge, knock-down slides | PR: callers of clip start `FUN_1402573e0` |
| MOV-11 | Walk/run hysteresis 0.4 / 0.7 and "walk or run, nothing in between" read from the free-camera path; Laurence reports lock-on differs | sim.rs:30-33; MOVEMENT-AUDIT.md:15 | ~ | lock-on movement feel | PR: `FUN_14025b270`, `FUN_14025b5e0` |
| MOV-12 | Strafe hysteresis 50/40 degrees around the 45 degree forward/strafe boundary (invented to avoid flicker) | sim.rs:38-43 | U | locked-on strafe | PR `FUN_14025b5e0`; program picks direction outright |
| MOV-13 | Strafe pivot restarts TURN_* clip from frame 0 on any (direction, gait) change; program retimes by length ratio and picks BR/BL randomly only when stick exactly behind ("accepted simplification", cause of the spasm) | MOVEMENT-AUDIT.md:24-26; PROGRAM-RE.md:539 | U | locked-on back-pedal animation | PR already read; needs code. Verify with FOOT |
| MOV-14 | Visual-yaw compensation for baked hip yaw: per-character measurement (pc00a node 22) | main.rs:179-187; DECISIONS.md:7 | T | strafe facing, all characters | SHEET per character; PR `FUN_14025b5e0` offset table |
| MOV-15 | Landing thresholds 40 / 80 u fall height, LAND_TIME 12/30, HEAVY 29/30 | sim.rs:63-69 | ~ | landing animation / lock | PR landing state 6; FOOT (footage recovery ~300-370 ms vs 433 ms, FOOTAGE.md:145-157) |
| MOV-16 | `COYOTE_TICKS` 6 (grace to cross floor gaps) and `STEP_HEIGHT` 6 | sim.rs:60, 72 | U | stair/ledge feel, edge falls | PR state-1 floor snap; not a program value (engine convenience) |
| MOV-17 | `FALL_RESPAWN_BELOW` 150 (fall-out y) | sim.rs:70 | U | falling out damage | PR fall-out rule (guide says damage 3); stage kill plane may be in `.dat` 0x94 block |
| MOV-18 | ~~Body radius 4.5 and height 20~~ **resolved 2026-10-03 (bodyradius lane): radius 5 and height 18-20 per character, READ** (`body.rs`) | sim.rs | R | collision, push-apart | PROGRAM-RE.md "Body size" |
| MOV-19 | `JUMP_UP`/`JUMP_DOWN` clip stretch to airtime (25/30 and 10/30 s constants, pc00a only) | main.rs:120-126 | T | jump animation, other characters | per-character clip length table |
| MOV-20 | Dodge recovery/lock "no Aura refill while it runs" not built | DECISIONS.md:38; PROGRAM-RE.md:479, 599 | ok | aura | code only |
| MOV-21 | Jump zone launch strength unit (u/s?) | sim.rs:329; GAME.md:139 | U | st05-st08 jump zones | PR jump-zone kind 16 trigger (FORMATS-STAGES kind byte 16); FOOT |
| MOV-22 | Forward slide state 2 (2.59 u/tick) has no setter found | PROGRAM-RE.md:609 | ? | unknown move | PR xrefs |
| MOV-23 | Warps (`FUN_140257e90`) | PROGRAM-RE.md:649 | ? | Retreat/Call/warp skills | PR |
| MOV-24 | Invulnerability blink period 6 ticks ("rate unconfirmed") | main.rs:1242 | U | post-getup look | PR `FUN_14024f2f0`; FOOT |
| MOV-25 | Sim runs 60 Hz while program ticks at 30 Hz; all per-tick values halved/quartered by hand | sim.rs:12, 15-24 | ok | determinism, everything | design choice; risk only if a rule is per-tick-integer (e.g. table_ticks x2) |
| MOV-26 | Floats deterministic only same binary / same CPU architecture; fixed-point "decide later" | GAME.md:22 | ~ | online play | design decision |

## A2. Camera (main.rs, GAME.md "Cameras")

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| CAM-01 | `setup/configdata.dat`: which of 13 floats in each of 6 blocks is which (first two read as 6 and 3 deg/tick yaw/pitch); our camera uses none of them | PROGRAM-RE.md:537, 648; MOVEMENT-AUDIT.md:53 | ? | all lock-on and free camera behaviour | PR: reader of configdata (camera update near lock struct +0xc); cross-check with FOOT |
| CAM-02 | Camera sits 60 behind / 30 above, looks past player | GAME.md:94 | T | both cameras | PR camera rig; FOOT overlay of lock-on clips |
| CAM-03 | `CAM_TURN_SPEED` 2.6 rad/s, `CAM_PITCH_SPEED` 1.6 rad/s | sim.rs:74; main.rs:1374 | U | free camera | PR configdata block; FOOT |
| CAM-04 | Pitch limits -1.15..+0.85 rad (Laurence's request, not program) | main.rs:1382-1383; GAME.md:98 | T | free camera | PR configdata (block has 70, -70 degree-looking floats) |
| CAM-05 | `LOCK_AIM_HEIGHT` 12, `LOCK_AIM_MIN_DIST` 25, `LOCK_PITCH_DOWN_NEAR` 0.12 (point-blank pitch easing) | main.rs:1376-1391 | T | lock-on camera | PR configdata; SHEET |
| CAM-06 | Top/bottom split: vertical FOV share `TB_MIN_VFOV_SHARE` 45 %, `TB_LOOK_DROP` 6; Laurence picked variant A | GAME.md:94 | T | split-screen framing | SHEET (done once); original split layout itself unknown (ROADMAP.md:85 open question) |
| CAM-07 | Original split-screen layout (side-by-side vs top/bottom) | ROADMAP.md:85 | ? | layout | WEB / FOOT of 2-player footage |
| CAM-08 | Camera occlusion/pull-in rules vs scenery, tight corridors (st03/st04/st13) | NEXT-SESSION.md:43 | ~ | camera | FOOT; PR collision-aware camera |
| CAM-09 | Camera enhanced by per-stage `.dat` 0x7 camera animation chunk never decoded | FORMATS-ALR.md:113 | ? | intros, maybe stage cameras | PR / decode chunk 0x7 |

## A3. Combat rules (combat.rs, GAME.md "Combat", PROGRAM-RE.md)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| CMB-01 | Aura refill flat 1 point/s (`AURA_TICKS_PER_POINT`=60, "Placeholders, to tune in play"). Program: max(0.25, 1.25 - Aura/10) points/s x (100+rate)/100 x handicap, pauses while casting / dodging | combat.rs:32-34; GAME.md:136; PROGRAM-RE.md:479, 598-599 | P (code) vs read (doc) | every match, arsenal school aura rate | Code change from PR (already read, `FUN_14024de20`) |
| CMB-02 | Aura recovery change by school count (`aura_rate_pct`: 20 etc.) "placeholders" | arsenal.rs:22-28 | P | arsenal rules | PR: handicap factor 1.3/1.0/0.7 already read (PROGRAM-RE.md:479) - map school count to it; WEB |
| CMB-03 | Capsule respawn 900 sim ticks (15 s) "from two fan guides"; program reads 420 program ticks = 14 s wait | combat.rs:29-30; DECISIONS.md:39; PROGRAM-RE.md:602 | U vs read | capsule pacing | Code change from PR |
| CMB-04 | No-layout capsule spawn: every 300 ticks, max 3, 25-70 u from player | combat.rs:36-42; GAME.md:138 ("every 2 s") | P | tests, Call-anchor fallback | design |
| CMB-05 | `PICKUP_RADIUS` 8 | combat.rs:43 | P | capsule capture | PR `FUN_14014fd10` |
| CMB-06 | `MAX_LEVEL` 16 and how Level rises (Aura Particle +1; damage-based +dmg at combat.rs:2007) | combat.rs:31, 1413, 2007; PROGRAM-RE.md:601, 644 | ? | level cap, aura particle, Succubus-like drain | PR: level-up code not located; `FUN_14024d970` only clamps 0-99 |
| CMB-07 | `START_HEALTH` 20 (Laurence's pick) | combat.rs:21 | P | damage balance | PR health init; WEB |
| CMB-08 | `CAST_TICKS` 24 flat (only for untimed/test skills) | combat.rs:44, 709 | U | fallback | none once all rows exist |
| CMB-09 | `AURA_ABSORB_TICKS` 12, `DENIED_FLASH_TICKS` 24 | combat.rs:45, 594 | P | feedback | design |
| CMB-10 | `HIT_STUN_TICKS` 30 "flat guess" for reaction kinds 0/1/2/3/7 where clip length unknown | combat.rs:46-50; PROGRAM-RE "Hit reactions" | U | light hit stun | PR already says "play one clip for its length": use character clip length |
| CMB-11 | Fallback knock-down/pinned row ids (pre-RE guess) | combat.rs:82-86 | U | tests only | none |
| CMB-12 | Knock-down lying 120 / get-up 60 / invuln 150 sim ticks from program ticks x2 | combat.rs:51-66 | ~ | knock-down | confirmed by PR (60, 30, 75 ticks) - check conversion is exact, FOOT |
| CMB-13 | Damage accumulator window 3 s for light/medium/heavy; which accumulator thresholds | combat.rs:78-80; PROGRAM-RE.md:377 | ok | hit reactions | PR `FUN_14024d2f0` |
| CMB-14 | Knock-down gravity 0.12 "value re-set on landing" | PROGRAM-RE.md:412 | ~ | knock-down arcs | PR |
| CMB-15 | Time-out result: healthier wins (Laurence's call) vs "probably a draw" in fan notes | combat.rs:86-89; HANDOFF.md:107; PLAN.md:222 | U | match end | WEB / PR match-end code |
| CMB-16 | `TIME_LIMIT_TICKS` 15 min (guide) | combat.rs:89 | ok | match | guide quoted; PR mode setting |
| CMB-17 | Powerless drain: one Health per 3 s ("rate unconfirmed") | combat.rs:90-93 | U | endgame | PR / WEB |
| CMB-18 | `FALL_OUT_DAMAGE` 3 (guide) | combat.rs:95 | ok | falling | guide |
| CMB-19 | `ROUND_END_TICKS` 180, rounds default 1 (original: one battle) | combat.rs:96; HANDOFF.md:107 | P | match structure | WEB / PR |
| CMB-20 | `LUNGE_SPEED` 240 u/s (docs also say 260) and `LUNGE_MAX_TICKS` 36; program gives dash window from action row and speed s65 x 10 / ticks, cap 10 u/tick | combat.rs:103-111; GAME.md:144; FORMATS-SKILLS.md:157 | P | melee dash | Code change from PR (already read) |
| CMB-21 | `LUNGE_STOP_SHORT` 11 u "a quirk" | combat.rs:106; PROGRAM-RE.md "Motion 13" | ok | melee | PR |
| CMB-22 | Melee hit point: `SHOT01` marker of the running cast effect else fighter +0x13db8, which bone | PROGRAM-RE.md:222, 638 | ? | melee hit position | PR: bone that matrix follows |
| CMB-23 | `PROJECTILE_RADIUS` 2.5 (program default hit radius 5 u set at creation, `FUN_140263a00`) | combat.rs:112; PROGRAM-RE.md:93 | U vs ~ | all projectile hits | Code change from PR |
| CMB-24 | `CHEST` 12 (aim/fire height) | combat.rs:114 | U | every aim | PR `SHOT01`/attach matrices |
| CMB-25 | Face-down + lie (w43/w44) from behind pairing | combat.rs:902 | U | pinned knock-downs | PR |
| CMB-26 | "Every round starts with the arena as placed" | combat.rs:974 | U | round reset, props | WEB / FOOT |
| CMB-27 | Whether a cast interrupted before fire still spends the skill / Aura; whether each chained combo hit pays Aura again | PROGRAM-RE.md:418, 576, 639 | ? | skill use rules | PR `FUN_14025c720`, `FUN_140258690` |
| CMB-28 | Charge: `CHARGE_TICKS_PER_POINT` 20 (program: cycle w39 = 13 ticks, restart at w40 15) | combat.rs:663-668; PROGRAM-RE.md:430 | U vs ~ | Gungnir, Ki Cannon, Dragon Bullet | Code change from PR |
| CMB-29 | Statuses 11, 12 (cleared on each hit), 20, 31 meanings; "Void" name for 33; state 22 | PROGRAM-RE.md:377, 642 | ~ / ? | status system | PR |
| CMB-30 | `s45` damage-formula functions at `0x1402dd0a0` not cross-checked with our notes-based table | PROGRAM-RE.md:643 | ? | STR rules (Level, Aura, half Aura) | PR |
| CMB-31 | Octolaser 8 shots x STR 1 "plausible, unconfirmed" | MOVES.md:117 | U | balance | PR / WEB |
| CMB-32 | Skill (s26) uses: 0 = unlimited, 1 once, 3, 5 | FORMATS-SKILLS.md:56 | ~ "probable, strong" | skill consumption | PR use counter / WEB |
| CMB-33 | Extra drop damage after an air hit (guide says amount unknown) | MOVES.md:109 | ? | air combos | WEB / PR |
| CMB-34 | 2v2 / partner immunity for own Hole ("caster's own hole can't hurt caster") | PROGRAM-RE.md:280; DESIGN-2V2.md:12 | ~ | 2v2 | PR |
| CMB-35 | Resuscitation rule (single web source, unverified) | DESIGN-2V2.md:17 | ~ | 2v2 | WEB / design |
| CMB-36 | Brawl mode fixed sets (design option A "by eye") | DESIGN-BRAWL.md:86 | T | Brawl mode | design |

## A4. Defence and status (moves/barrier.rs, status.rs, special.rs, erase.rs, environment.rs)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| DEF-01 | Barrier time = s46 x 2 sim ticks (Crystal Wall 23 -> 46 ticks = 0.77 s). Program: countdown starts at s46 - 2, lasts about s46 - 1 program ticks = 22 (0.73 s) | barrier.rs:75-78; MOVES.md:43,47; PROGRAM-RE.md:347 | U vs read | all defence skills | Code change from PR |
| DEF-02 | Default guard time 60 ticks when s46 = 0 | barrier.rs:57 | U | fallback | none |
| DEF-03 | Automatic Shelter (s44 = 7) "doesn't count down on its own (ends another way)" | PROGRAM-RE.md:347 | ~ | Shelter | PR |
| DEF-04 | Brush stun 45 sim ticks (MOVES.md says 0.75 s, HANDOFF owed "1.0 s") | barrier.rs:59; MOVES.md:50; HANDOFF.md:51 | U | Brush | PR |
| DEF-05 | Course / About Face field: s45 800 -> 80 u, 130 on rest (program: sphere radius s45 x 0.1 + attack radius, disc for front kinds) | barrier.rs:79-82; PROGRAM-RE.md:351 | U vs read | Course, Reverse, Absorb | Code change from PR |
| DEF-06 | Barrier rooted while up; Reflect "returns weaker shots" as barrier | MOVES.md:45 | U | defence feel | PR defence kinds |
| DEF-07 | Shield STR vs DEF, Pierce gets STR - DEF through | MOVES.md:46; PROGRAM-RE.md:377 | ok | defence | PR confirmed for core |
| DEF-08 | Status tags as text, not icons (icons not in files) | MOVES.md:116; HUD.md:138 | ~ | status HUD | WEB / PR icon draw |
| DEF-09 | Status kind meanings: 4-7 statuses 0-3, 9-14; "kinds 16, 20, 22, 23 of the status body named from skills that use them" | FORMATS-SKILLS.md:185; PROGRAM-RE.md:650 | ~ | status skills | PR `FUN_140270b00` |
| DEF-10 | Status duration field picked by s45; "no single note names it" | status.rs:71 | U | timed status | PR |
| DEF-11 | Frostbite duration 10 s same as freezes (field 0) | status.rs:170 | U | Frostbite | PR / WEB |
| DEF-12 | Regenerate "+1 Health / 91 ticks standing" in doc vs `REGEN_TICKS_UNCONFIRMED` = 60 (aura rate) in code | special.rs:124-127, 221-237; FORMATS-SKILLS.md:185 | U | Regenerate, Health+ | Code change from PR (doc reading 91) |
| DEF-12b | Skywalk duration reads from s46 (450 = 15 s) | status.rs:270 | U | Skywalk | PR |
| DEF-13 | One status read off a single skill's numbers, no second witness | status.rs:244 | U | one status skill | WEB |
| DEF-14 | Frighten / Sword Slap "briefly stops target" = `BRIEF_BIND_TICKS` 30 | special.rs:118-122, 432 | U | two skills | PR |
| DEF-15 | Jump-dash specials: feet to units 3.66 (`FEET_TO_UNITS`, `FOOT`) for "Dash 30 ft", "Jump 30 ft" notes; no collision in dash | special.rs:116, 312-320; status.rs:92 | U | Dash, Jump skills | PR `FUN_140257e90` / special state 3 |
| DEF-16 | "Points" plural in a spawn-related special note | special.rs:473 | U | one special | WEB |
| DEF-17 | Erase skills: (filter, pick, count) triple from s48-s50; random pick for Tiger Slayer etc. | erase.rs:46, 166, 290 | U | erase skills | PR erase capsule handler |
| DEF-18 | Environment capsule "cost is variable" sentinel | environment.rs:65, 96 | U | environment | PR `FUN_14017bcc0` |
| DEF-19 | What each environment crystal does once placed (readers of `0x14058bd30`) | PROGRAM-RE.md:587, 649 | ? | environment skills | PR |
| DEF-20 | Environment crystal duration: 1800 ticks for 7 skills, `CRYSTAL_CAPTURE_AFTER` 15 s for capture | moves/effects.rs:155; PROGRAM-RE.md:587 | ~ | environment | PR |
| DEF-21 | School IDs 1..5 mapping to Psycho..Faith | environment.rs:47-51; school_aura.rs:97-101 | ~ | arsenal, aura colour | WEB / PR school table |
| DEF-22 | Status 8 invisible, 33 void, state 22 invuln settled in phase 3; "which ones are 8, 11, 12" cleared on hit | PROGRAM-RE.md:377, 642 | ~ | hit clears | PR |

## A5. Attacks (moves/*, FORMATS-SKILLS.md unit tables)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| ATK-01 | Table distance unit 0.1 world unit (blast 300 -> 30 u, reach 130 -> 13 u) | moves/mod.rs:34-36; FORMATS-SKILLS.md:108 | U ("likely"); s64 x 0.1 hit radius and s67 x 0.1 blast radius ✅ in PROGRAM-RE | every distance | Largely settled by PR (MELEE radius, blast); confirm range-band fields |
| ATK-02 | Table speed unit: 3 u/s per point (`TABLE_SPEED`); program reads /10.8 per tick = 2.78 u/s (s65 in the melee case) | moves/mod.rs:37-39; FORMATS-SKILLS.md:109; combat.rs:70-74 | U vs read | every projectile speed | Code change from PR; mixed unit in code (combat.rs LAUNCH_SPEED_UNIT 30/10.8 vs mod.rs 3.0) |
| ATK-03 | Homing 0.1 rad/s per point ("the scale is a guess", only ordering evidenced) | moves/mod.rs:40-43; FORMATS-SKILLS.md:110 | U | shell homing, Faith, Crawl | PR projectile update `FUN_140263a00` family; FOOT |
| ATK-04 | Shell range `MAX_RANGE` 900; range band s44 (-30 / -80 / -120) meaning "range?" | shell.rs:12; FORMATS-SKILLS.md:83 | U | shells | PR: bands 1/2/3 at 80/300 u read for lock-on; attack use not |
| ATK-05 | Shell generic stand-in (170 u/s, homing 1.5) for unmapped motions | shell.rs:31; mod.rs:209, 215 | P | unmapped skills | PR |
| ATK-06 | [Arc] bend = s67 degrees, swing back on target's launch position, `OVERSHOOT` 300, `MAX_BEND_DEG` 80 | arc.rs:12-21; FORMATS-SKILLS.md:85 | U | Laser family arcs | PR motion 6 |
| ATK-07 | Fan step s68/s69, shot count s69 (Double 2, Triple 3, Octolaser 8) | FORMATS-SKILLS.md:85 | ~ (s69 strong) | multi-shot | PR |
| ATK-08 | Short-range reach `DEFAULT_REACH` 14 tuned by feel; table reach 130 -> 13 u | melee.rs:12-17; FORMATS-SKILLS.md:86 | U but hit radius s64 x 0.1 ✅ in PROGRAM-RE.md:118 | melee | settled for radius; reach layering is ours |
| ATK-09 | [Parabola]: speed = s64 x 1 u/s, flight 0.5-1.6 s, gravity 150 (half jump gravity, stale reason), steering s66 x 0.1; s67 might be gravity | parabola.rs:9-23; MOVES.md:43,121 | U/T | bombs, lobs, Gungnir | PR motion 1 (`FUN_14026a...` not cited); HANDOFF.md:51 flags s66/s67 swap |
| ATK-10 | Mines: fuse Timer 150 = 5 s confirmed, Sensor 60 arming?, blast radius 500/300 | FORMATS-SKILLS.md:91; area.rs:21 (`MINE_LIFE_TICKS` 1800) | U partly ✅ | mines | PR mine object; HANDOFF.md:51 says "mines hurt owner" doc correction owed |
| ATK-11 | [Hole]: warning s64 (30 = 1 s), radius s66 x 0.1; grow/active timing s64/s65 in `FUN_14026a7f0` not written up | FORMATS-SKILLS.md:88,157; MOVES.md:54; PROGRAM-RE.md:280 | U / ~ | Hole skills | PR `FUN_14026a7f0` |
| ATK-12 | [Rain]: warning, radius whole units (Lightning Bolt 10), drops s66=6, gap s67+1, `RAIN_HEIGHT` 150 vs program drop height / speed | area.rs:23; FORMATS-SKILLS.md:89; PROGRAM-RE.md:255 | U vs ✅ read | Rain skills | Code change from PR (already read) |
| ATK-13 | Rain "lands once per cast" (MOVES) vs program: drops 6 with cloud life 60 ticks off-by-one | MOVES.md:55; PROGRAM-RE.md:255 | ~ | Rain | PR |
| ATK-14 | [Fall]: height 150 constant; fall speed = s66 x 3; "Fall diagonal" correction owed | fall.rs:9-16; MOVES.md:59; HANDOFF.md:51 | U | Meteor, Thor's Hammer | PR motion 2 |
| ATK-15 | [Crawl]: speed s65 x 3 u/s, homing s68 capped 4 rad/s, range 700; "homing from 500-800?" | crawl.rs:13-23; FORMATS-SKILLS.md:92 | U | crawlers | PR crawl object; floor snap vs wall test ❓ (PROGRAM-RE.md:641) |
| ATK-16 | Tornado speed = s64 u/s "guess", homing 1.5 (doc owed "Tornado no homing") | crawl.rs:23; MOVES.md:58; HANDOFF.md:51 | U | Tornado | PR motion 10 |
| ATK-17 | Ricochet: 4 lasers ±45 degrees, 3 bounces, range 900; bounce mirrors leading axis since no wall normals | ricochet.rs:6-17; MOVES.md:60 | U | Ricochet Laser | PR motion 17 |
| ATK-18 | [Object] Psycho Kinesis / Power: nearest throwable prop, STR per object | mod.rs:209; props.rs:18-44; GAME.md:178 | U (all "unconfirmed" in GAME.md:178) | two skills, props | PR / FORMATS-STAGES object weight not in `.dat`; WEB |
| ATK-19 | Explode-around-self radius 400/500 | FORMATS-SKILLS.md:87 | U | Photon Burst | PR motion 9 |
| ATK-20 | s55 = 1 and 2 wall handling (one and two skills); s55 = 3 confirmed | FORMATS-SKILLS.md:172; PROGRAM-RE.md:184, 640 | ~ | few skills | PR |
| ATK-21 | s40 = 2 "usable in mid-air" now ✅ by action row choice | FORMATS-SKILLS.md:59 (old) vs 154 | stale | aerial casts | see B |
| ATK-22 | Off-edge fall 0.12 u/tick2 for crawlers; end at wall or range | PROGRAM-RE.md:247 | ~ | crawlers | PR |
| ATK-23 | Homing turns only within 60 degrees of heading (turn cap from s71 argument unconfirmed) | PROGRAM-RE.md:311 | ~ | homing lasers | PR |
| ATK-24 | Laser caster locked for whole clip 1.13 s, Blaster 1.5 s (clip length = frames at 30/s) | PROGRAM-RE.md:86, 99, 133-144 | ~ | cast lock | FOOT (Blaster footage gave ~1.33 s, FOOTAGE.md:56) |
| ATK-25 | Blaster fire time 20 ticks; footage delta 533 ms vs 667 ms "roughly confirmed / can't tell" | FOOTAGE.md:68 | ~ | Blaster timing | FOOT cleaner clip |
| ATK-26 | Psycho Knife total 1.33 s clip end | FOOTAGE.md:80; PROGRAM-RE.md:112 | ~ | knife | FOOT |
| ATK-27 | Extra hit s67 flag "not used in this move" | PROGRAM-RE.md:117 | ? | motion 13 | PR |
| ATK-28 | Combo offset w48, charge cycle w39 | PROGRAM-RE.md:62, 67 | ~ | combos, charge | PR |
| ATK-29 | Which bone the melee hit sphere follows | PROGRAM-RE.md:118, 222 | ? | melee | PR |
| ATK-30 | "Slot-1 effect drawn 20 u in front of chest scaled by radius x 0.2" vs hit test uses attached point | PROGRAM-RE.md:223 | ~ | melee visuals | PR |
| ATK-31 | Default hit window end (`w44`) and dash cap, Pursuit / Sky Pursuit dash-at-downed-foe, Recollection / Copy, Necronomicon, Diffused Laser, Lack's filter not built | HANDOFF.md:225; NEXT-SESSION.md:44 | ? | stragglers | PR |
| ATK-32 | Launch angle only 50 and 90 degrees; sin/cos precomputed | MOVES.md:104; combat.rs:266 | ok | knock-back | none |
| ATK-33 | Pinned kind's face-down half (w43/w44), clip-then-lie lying half (w42) not rendered | MOVES.md:109 | ? | pinned states | PR / animation |
| ATK-34 | Attack Add effects: 1 target aura -> 0, 7 freeze buttons, 8 poison cost 1 Health per press for 10 s, 9, 11, 18 Auto-Capture; `ADD_EXPLODES` 5 | MOVES.md:53; mod.rs:69 | ~ | hit effects | PR `s51/s52` switch |
| ATK-35 | Skill field s40 = 2 on 56 skills, s43 target kinds code-by-code, s47 defence 1800/3-8 on a few attacks, s57-s59, s61 (0 everywhere), s44 range band | FORMATS-SKILLS.md:59-65 | ~ / ? | many skills | PR / Esper-RE facts (WEB) |
| ATK-36 | Fields s3/s5/s6/s7/s8/s10/s36/s37: catalog no (assumed), rarity stars (Esper-RE unconfirmed), unknown | FORMATS-SKILLS.md:25-27, 45-46; SKILL-FIELD-CROSSWALK.md:16-19 | ~ / ? | catalogue UI | WEB (community SkillDB); run `engine/re/skill_field_audit.py` |
| ATK-37 | Texts 392-393 are Shift-JIS placeholders | FORMATS-SKILLS.md:130 | ok | two dummy skills | none |
| ATK-38 | s54-s56 / s62 meanings beyond s55 = 3 | SKILL-FIELD-CROSSWALK.md:55; PROGRAM-RE | ~ | projectile properties | PR |
| ATK-39 | Which animation does each cast use: hand table by family `main.rs::cast_clip` (superseded by action row w38 for timed skills) | MOVES-PLAN.md:12; FORMATS-SKILLS.md:122 | stale-ish | casts | PR already read; check untimed skills |
| ATK-40 | Status/erase/special "Add effect" names (Swarm of Moths poison etc.) reconstructed from guide notes | MOVES.md:53 | ~ | status skills | PR |

## A6. Effects and visuals (effects.rs, fx.rs, FORMATS-EFFECTS.md, VFX-AUDIT.md)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| FX-01 | Effect frame rate 30 fps; program's effect clock not traced; particles/rays/blades age per drawn frame (taken as 60/s) | effects.rs:19-20, 138-142; FORMATS-EFFECTS.md:68, 141; PROGRAM-RE.md:637 | ~ | all effect speed | PR effect update; FOOT tick-matched sheets |
| FX-02 | `STEPS_PER_FRAME` 2 inferred from Psycho Knife ray burst showing on two footage frames | effects.rs:138-142 | ~ | particles, flares, blades | FOOT (one data point) |
| FX-03 | Slot meanings: 0 cast, 1 projectile body, 2 ground impact, 3 hit, 4 block (renders only) | FORMATS-EFFECTS.md:33; effects.rs:23-30 | ~ | all effects | PR effect request `FUN_14018cd10` call sites per event |
| FX-04 | Event-table entry word +4 = 0x1D or 0x1B unknown | FORMATS-EFFECTS.md:28 | ? | effects | PR / decode |
| FX-05 | Emitter piece words: 7 spin units, 14-15 second colour pair, 19 blend/billboard mode, 28-31 unknown, 24-27 flipbook range (~), 16-18 lifetime scale (~), 4-6 move (~), 8-11 easing selectors (~ but read) | FORMATS-EFFECTS.md:84-94; effect.rs:65-69 | ? / ~ | every piece | PR per piece class; docs/research/DECODING_RESEARCH.md:71 differential analysis |
| FX-06 | Flipbook grid large values (200-3000) on 0x14 chunks | FORMATS-EFFECTS.md:84 | ? | shader pieces | PR |
| FX-07 | Trail (`b8de89ea`) width 3 x grow.x, length 5 x grow.z; `TRAIL_GROW_FRAMES` 3; `PROJECTILE_PIECE_REACH` 8 | effects.rs:32-41; FORMATS-EFFECTS.md:72 | U | projectile trails | PR trail class; FOOT |
| FX-08 | LIGHT02 radius scale `RANGE_SCALE` 0.02 (by-eye, real program uses 32-light hand-rolled falloff LUT) | effects.rs:42-47, 269; FORMATS-EFFECTS.md:73; VFX-AUDIT.md:59; RESEARCH-LIGHTING.md | T / U | floor light decals | PR: light table falloff |
| FX-09 | LIGHT02 drawn as floor decal not a real light because stages are baked unlit | VFX-AUDIT.md:55 | ok | look | design |
| FX-10 | Sprite emitter (`a07ac685`) / puff emitter (`90e7fd3e`) semantics; sprite size = words 30/31 x `SPRITE_SIZE` 1.0; `SPRITE_EVERY` 2; `SPRITE_MIN_LIFE` 10 | effects.rs:118-134; FORMATS-EFFECTS.md:74-75 | U / ~ | Moths, Hell's Fire, crawlers, casts | PR `MSParticleFunc` (effects.rs:1540 "unconfirmed reading") |
| FX-11 | What `90e7fd3e` "spawner" emitters spawn (hit slots of Laser, Blaster, Bomb) | FORMATS-EFFECTS.md:188; docs/research/DECODING_RESEARCH.md:110 | ? | hits | PR; runtime log |
| FX-12 | 43 effect numbers loose-only (500s + others) absent from `e.i`; reached by what path | FORMATS-EFFECTS.md:189; CAPSULE-VISUALS.md:10 | ? | shared/status effects | PR `FUN_14018d0b0` callers |
| FX-13 | Shared hit effects 503/504 exist nowhere | FORMATS-EFFECTS.md:188 | ? | hits | PR |
| FX-14 | Pixel format 8 (distortion / heat haze) not decoded; pieces hidden | FORMATS-EFFECTS.md:98; effects.rs:1215 | ~ | ice, heat effects | PR / graphics capture |
| FX-15 | Blend mode per draw: additive for all but distortion; `BB01R` / `BB01C` / `MSICE11` / `PDMTEX01` state unread; `PDEFTBB01R` maybe reverse-subtract (dark skills) | FORMATS-EFFECTS.md:116, 187; VFX-AUDIT (ice pieces white) | ? | dark skills, ice | PR blend state per class (C++ per class) |
| FX-16 | PARTICLE01 point size `POINT_SIZE` 0.0024 (clip-space offsets, FOV 60 assumed) | effects.rs:144-147; FORMATS-EFFECTS.md:67 | ~ | particle size | PR camera FOV; FOOT |
| FX-17 | PARTICLE01 word 28 = 1/2 spawn functions (`FUN_1401a2bc0`, `FUN_1401a34a0`) not read | FORMATS-EFFECTS.md:67 | ? | some particles | PR |
| FX-18 | BLADE01 ribbon look (`~` for look); sample spline between samples exact form unconfirmed (`FUN_14017e120`) | FORMATS-EFFECTS.md:61; effects.rs:869 | ~ | sword arcs | PR; FOOT |
| FX-19 | FLAIR01 ray-burst look, random angles | FORMATS-EFFECTS.md:62 | ~ | strike starburst | FOOT |
| FX-20 | `CAST_REACH` 14 / `CAST_PIVOT_Y` 12: slot-0 root-anchored pieces farther than 14 u from chest are pulled in and shrunk ("by-eye calibration", program shows no such factor) | effects.rs:92-102; FORMATS-EFFECTS.md:151, 178; VFX-AUDIT.md:35 | T | Laser charge, spiral casts | PR: real scale chain; FOOT overlays |
| FX-21 | Laser charge rings ending 37-42 frames fit "effect plays on after shot" or effect frames faster than 30/s | PROGRAM-RE.md:126 | ? | charge timing | FOOT |
| FX-22 | Green crescent in Slide Laser footage: which piece draws it | FORMATS-EFFECTS.md:159 | ? | Slide Laser | FOOT + piece comparison |
| FX-23 | Psycho Wave "stacked rings" vs original | FORMATS-EFFECTS.md:193 | ~ | Psycho Wave | FOOT (no footage) |
| FX-24 | Crawlers / Root of Tree / Venom Fang visible look ("what original draws is not known") | FORMATS-EFFECTS.md:185; docs/research/DECODING_RESEARCH.md:109 | ? | crawler family | FOOT |
| FX-25 | Stand-in ball drawn when a flight slot draws nothing (Bomb Shot, Spore Schism, Copy / Recollection) | fx.rs:1410-1450; MOVES.md:113 | P | 6 skills | PR / decode |
| FX-26 | Stage-effect "on shared gamma/tone-map": Bevy TonyMcMapface vs original straight-out additive; fixed by `FxGamma` | FORMATS-EFFECTS.md:139 | ~ | all additive glow | SHEET |
| FX-27 | The game's second render target (glow buffer, `blend_colour`) what it adds | HANDOFF-ORCHESTRATION.md:48; CAPSULE-VISUALS.md:394, 458; RESEARCH-LIGHTING.md | ? | halo/bloom look | PR / graphics capture |
| FX-28 | Dark blob: alpha-kill reproduction guess reverted; solid pieces | FORMATS-EFFECTS.md:153 | ok | puffs | none |
| FX-29 | Per-piece easing/flipbook applied to plain pieces same as timed emitter class | FORMATS-EFFECTS.md:86 | ~ | pieces | PR |
| FX-30 | MSPARTICLE reading | HANDOFF-ORCHESTRATION.md:48 | ? | particles | PR |
| FX-31 | Status/shared effects 501-511 (crystal appear/break, life cost, etc.) | CAPSULE-VISUALS.md:47 | ~ | system effects | PR |
| FX-32 | Stage object effects: `OE00`/`OE01`/`OE02` blocks `EF00` kinds 3, 7, 8, 9, 11, 16 etc.: smoke colours and blends, camera shake amplitude, debris size p1 x 100 (`DEBRIS_SIZE_SCALE`), fragment speed 45, gravity 200 | FORMATS-STAGES.md:170-191; fx.rs:190-196 | ~ / U | destructible stages | PR `EF00` interpreter; FOOT |
| FX-33 | 5 unknown EF00 record kinds (5, 7, 13, 18, 31), kind 8 | FORMATS-STAGES.md:187-191; PLAN.md:66, 164 | ? | lamps, blocks, bridges | PR / decode |
| FX-34 | `MAX_LIGHT_DECALS` 12, `LIGHT_DECAL_Y` 0.03 (engine conveniences) | effects.rs:50-53 | P | | none |

## A7. Capsule / orb / school aura visuals (fx.rs, orb_*, school_aura.*, CAPSULE-VISUALS.md)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| ORB-01 | Capsule colours by type: code now = program table (erase purple, status green, environment aqua) but HUD.md:25 and builder.rs:139 comments still say assumed; fx.rs `COLOUR_TO_ORB_SLOT` bridging | fx.rs:535-552; hud.rs:315-327; HUD.md:25,136; CAPSULE-VISUALS.md:155 | ~ in HUD.md, ✅ in CAPSULE-VISUALS | HUD, orb | doc fix only (see B) |
| ORB-02 | Core light: fixed key light/hemisphere (stand-in), bump gain, centre/edge balance tuned by eye | orb_sphere_lit.wgsl:43-58; orb_material.rs:375-379; CAPSULE-VISUALS.md:454, 492, 496 | T | idle orb look | PR: light set pick `FUN_1402a7a00`; SHEET |
| ORB-03 | Cup facing sign (−Z toward camera) not seen on screen | CAPSULE-VISUALS.md:456; orb_material.rs:354 | ~ | orb | one A/B render |
| ORB-04 | T per 30 Hz update vs per frame (UV scroll rates) | CAPSULE-VISUALS.md:455 | ~ | orb animation | PR `FUN_1401c7630` |
| ORB-05 | Orb motes (drifting orange points): procedural stand-in, default off; real source maybe slot 0 sparkle during respawn wait | fx.rs:982-1004; CAPSULE-VISUALS.md:398, 420, 477 | U | pickups | FOOT / PR |
| ORB-06 | Fade: original 32-level screen-door dither; ours scales down | CAPSULE-VISUALS.md:419, 478, 494; fx.rs:819, 859 | ok | orb | code only |
| ORB-07 | Materialise burst: real 429-frame animation stretched over 14 s wait; we approximate | fx.rs:870, 1287; CAPSULE-VISUALS.md:136 | U | capsule respawn | PR / FOOT |
| ORB-08 | Layers 1-3 of Status/Environment's 4-texture unlit blend reuse layer 0's UV; ring boost/soften constants | CAPSULE-VISUALS.md:521, 539 | ~ | two orb kinds | data exists; code |
| ORB-09 | Disc format 0x15 colour offset assumed by analogy (+16 BGRA) | alr.rs:587-606; orb_rim.wgsl:132; CAPSULE-VISUALS.md:495 | ~ | orb disc | hex dump |
| ORB-10 | Footage's smoke/skull Aura Particle is slot 11 (opponent view) or slot 4 lit dark; whose capsule was filmed | CAPSULE-VISUALS.md:179 | ~ | aura orb | FOOT |
| ORB-11 | `0x13` chunk layout: scroll speeds vs other (360.0 = rotation period?); `0x5` animations in `st00.dat` for slots 0-3 | CAPSULE-VISUALS.md:187-188 | ? | orb anim | PR / decode |
| ORB-12 | `FUN_14022e8a0` pickup-on-player effect, colour top byte meaning | CAPSULE-VISUALS.md:138-139, 189 | ? | pickup | PR |
| ORB-13 | Aura Particle effect 600 = on-character aura wisps (tiers m0-m2); 601-606 empty placeholders | CAPSULE-VISUALS.md:39-43 | ~ | aura visuals | PR |
| ORB-14 | hud1 glow sprites "aura/capsule glows, likely in-world art" vs "not used by this draw path" | HUD.md:12; CAPSULE-VISUALS.md:191 | contradiction (B) | | see B |
| ORB-15 | School aura (glow outline): width 1.4, falloff 1.8, gain 1.1, head bias, arm fade 2.0; colours per school pink-red etc. "own guess", tie-break order from community guide; program has no rim/fresnel class by name | school_aura.rs:72-112, 129-134; school_aura.wgsl:37; VFX-AUDIT.md:79-93 | T / U | every player's glow | PR: glow/outline class (`pdvkGlow*`); FOOT; SHEET |
| ORB-16 | Glow only visible while Level >= 1 | GAME.md:166; VFX-AUDIT.md:79 | ~ | aura display | FOOT |
| ORB-17 | Lock-on marker ring shades: orange <300, yellow 300-600 (visual match), green >600 matches footage; procedural ring sheet frames | hud.rs:98-110; VFX-AUDIT.md:73 | U | lock-on HUD | PR marker class; FOOT |
| ORB-18 | Lock-on ring art missing in atlas; procedural stand-in | hud.rs:98-103 | P | lock-on | decode |

## A8. Sound (FORMATS-AUDIO.md, sfx.rs, frontend/sound.rs)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| SND-01 | Nobody has listened to any decoded sound yet; every check is by numbers and waveform | FORMATS-AUDIO.md:5; FRONTEND.md:387; frontend/sound.rs:5 | P (whole system) | all sound | EAR: one listening session |
| SND-02 | Sound-event volume at REAT/TRAT +0x10 (0-127) | FORMATS-AUDIO.md:36-37 | ~ | mix levels | EAR / PR sound player |
| SND-03 | Event play length 0x1C (0 = clip length / loop) | FORMATS-AUDIO.md:46 | ~ | cut/loop | EAR; PR |
| SND-04 | Event kinds 2/3/5/6 (pan/pitch sweeps, echo tails, Rain whistle octave rise) not played | FORMATS-AUDIO.md:47, 52-53; docs/research/DECODING_RESEARCH.md:108 | ~ | echoes, Rain | EAR; FOOT audio |
| SND-05 | Pan -100..100 (+0x58), pitch semitones (+0x5C) | FORMATS-AUDIO.md:52-53; sound.rs:47-51 | ~ / U | panning, pitch | EAR |
| SND-06 | Speaker-level pairs: keys 0-5 = FL, FR, C, LFE, RL, RR; keys 11, 12 unknown; centre/LFE always 0 | FORMATS-AUDIO.md:55 | ~ / ? | surround downmix | EAR |
| SND-07 | 0x60 always 12, 0xF4 running number, 0x10/0x48/0x64 unknowns | FORMATS-AUDIO.md:54, 57-58 | ? | minor | none |
| SND-08 | Tracks play together as layers; empty (-1) slots may carry meaning | FORMATS-AUDIO.md:60 | ~ / ? | layering | PR sound player |
| SND-09 | `sound::render` (resample 22,050 Hz stereo, 5 ms fade, mixdown) | FORMATS-AUDIO.md:62-64 | ~ | all SFX | EAR |
| SND-10 | s20 "travel hum" loops (length 0 + loop points) | FORMATS-AUDIO.md:75 | ~ | projectile flight | EAR |
| SND-11 | s19 played at cast vs program fires it at launch tick (and renamed "shot sound" by community) | FORMATS-AUDIO.md:77, 93; FORMATS-SKILLS.md:158; docs/research/DECODING_RESEARCH.md:41; SKILL-FIELD-CROSSWALK.md:26 | contradiction | launch sound timing | PR ✅ already (`FUN_1402626d0`): move play to fire tick |
| SND-12 | s18 sounds (3012, 4031, 5001-5017) enemy only | FORMATS-AUDIO.md:79 | ? | none | skip |
| SND-13 | `common` ids 44-300 and 355-500 system sounds (UI, capsule, aura, guard) unlabelled | FORMATS-AUDIO.md:81; docs/research/DECODING_RESEARCH.md:106 | ? | UI/capsule/aura sound | PR `engine/re/call_consts.py` on the sound player; EAR |
| SND-14 | Menu cue ids (move / confirm / back) picked by waveform length (60, etc.) | frontend/sound.rs:5-12; HANDOFF-ORCHESTRATION.md:42; FRONTEND.md:282 | U | menus | EAR / PR menu sound calls |
| SND-15 | `VOICE_HURT` 40 and `VOICE_KO` 60 ("❓ no row seen using it"), hurt 10-12 / KO 60-62 meanings | sfx.rs:30-42; HANDOFF.md:184, 211 | U | voices | EAR; PR (reaction row w4 already used) |
| SND-16 | Voice group meanings (what each group 1-6 sounds like) | PROGRAM-RE.md:59 | ? | voice | EAR |
| SND-17 | `common` bank = bank 0 (call passes 0) | PROGRAM-RE.md:58 | ~ | cast sounds | PR |
| SND-18 | Volumes `VOL_SFX` 0.55, `VOL_BLOCK` 0.3, `VOL_TRAVEL` 0.25, `VOL_VOICE` 0.7, `MAX_PLAYING` 24, `VOL_OBJECT_HIT` 0.35, reach 40 | sfx.rs:48-53, 379-381 | P | mix | EAR |
| SND-19 | No per-material object hit sound in the data | FORMATS-AUDIO.md:99 | ? | prop sounds | PR / EAR |
| SND-20 | EF00 kind 11 sounds: delay frames, looped byte; jump-zone hum 79 not played | FORMATS-AUDIO.md:101; FORMATS-STAGES.md:189 | ~ | stage ambience | EAR; PR |
| SND-21 | `DSP_*.bin` = reverb/DSP per stage (25,836 bytes) | FORMATS-AUDIO.md:17 | ? | reverb | PR / decode |
| SND-22 | Voice / tutorial `.w10` files | FORMATS-AUDIO.md:104 | ? | tutorial voice | decode |
| SND-23 | 7 names x 2 languages are silent 1-clip placeholder banks | FORMATS-AUDIO.md:22 | ok | enemies | none |
| SND-24 | Which audio API the exe imports (XAudio2 assumed) | docs/research/DECODING_RESEARCH.md:86 | ~ | none for us | PR imports |
| SND-25 | Sword layer combination (502-504 = 501 + element) | FORMATS-AUDIO.md:60 | ~ | sword hits | EAR |
| SND-26 | Music: Palace and Strange City BGM identities (recorded matched in AGENTS.md; ROADMAP still asks) | ROADMAP.md:88; PIPELINE.md:85 | ~ | music | EAR / FOOT of stage |
| SND-27 | STX loop points/transition, other music not claimed (`13_JuryokuHanten` group) | ROADMAP.md:88; PIPELINE.md:85 | ~ | music | EAR |

## A9. Stages (FORMATS-STAGES.md, COLLISION.md, PIPELINE.md, props.rs, stage_look.rs)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| STG-01 | `.dat` object class byte (1 fixture, 2 prop, 3 door, 4 trigger) "from which names carry which value" | FORMATS-STAGES.md:48; stagedat.rs:374 | ~ | props | PR / decode |
| STG-02 | Kind byte (behaviour code): 1, 2, 3, 4, 6, 8, 16, 17, 18, 23, 26, 28, 48, 53, 54, 58, 65-74 meanings | FORMATS-STAGES.md:49, 112-124 | ? | prop behaviour | PR object handlers |
| STG-03 | Param: jump launch strength ✅ varies; collapsing block order (st08 0-75), st01 tower sector 0-5 | FORMATS-STAGES.md:50, 120, 138 | ~ | st08, st01 | PR |
| STG-04 | Which placement list is versus: `PS0/` used, fallback first list (st24 has only PS02); evidence = guide text for Panorama | FORMATS-STAGES.md:57 | ~ | prop layout | PR: list selection |
| STG-05 | Which of six start sets a 1v1 uses (we use set 0 starts 0 and 1); Highway starts 0/2 not connected to 1/3 | FORMATS-STAGES.md:96-110; COLLISION.md:70 | ~ / ? | spawns | PR `CP00` consumer |
| STG-06 | `CP00` offsets 0x10, 0x38 unknown; 0xB0 has one start with capsules spread over map; 0xB8 duplicates 0xA4 | FORMATS-STAGES.md:104-110 | ? | spawn sets | decode |
| STG-07 | OB00 broken/damaged model slots (1, 2), collapse animation offset (4), hull offset (7), effect offset (9), slot 10, slots 3/5/8/11 unused | FORMATS-STAGES.md:72-79 | ~ / ? | destructibles | PR |
| STG-08 | contaB collapse animation (`OA01`) and collapsed hull (`OC01`) not used | NEXT-SESSION.md:44; ROADMAP.md:54 | ? | st06 | decode; PR |
| STG-09 | Hit points / weight / throwable flag not in `.dat`; wear constants all "unconfirmed named constants" (`SIZE_PER_TOUGHNESS` 12, `THROW_*`) | FORMATS-STAGES.md:124; GAME.md:178; props.rs:14-44 | U | prop breaking and throwing | PR exe/SSB; FOOT |
| STG-10 | Structural kinds list `[6,23,26,28,48]`; scripted kinds (58, >=64) left intact | props.rs:39-50 | U | arena floors | PR |
| STG-11 | Header 0x94 untagged ~150 KB hull block = whole-stage collision (we use ALR geometry); 0xF4/0xF8 animation chunk, 0x98 EF00, 0x9C `SP00`, 0x70-0x78 OE blocks | FORMATS-STAGES.md:31-40; COLLISION.md:68 | ? | stage borders, Highway floor | decode; PR |
| STG-12 | Arena edges open to the void; real bounds likely in 0x94 block; Highway and st24 huge invisible walkable floor (fmt 0x10 / flags 0x0003) | COLLISION.md:68; HANDOFF.md:53 | ~ | bounds | decode 0x94 |
| STG-13 | Hull `attr` (0x77FC, 52, 0x34, 0x0B) meaning; face flags 1/6/0x10 | FORMATS-STAGES.md:94, 197 | ? | surface type | PR |
| STG-14 | Embedded SSB script start/end 0xD0/0xFC = light script (-1 in st12-15, st22-25, found by content) | FORMATS-STAGES.md:39 | ~ | lighting | PR |
| STG-15 | Light script call roles: `07` point lights, `02` clear/backdrop colour and factor; sets 0 stage / 1 characters / 2 sky / 3 default "roles read from numbers" | FORMATS-STAGES.md:144-157 | ~ / ? | stage + character look | PR SSB interpreter |
| STG-16 | `cS0_VEC = 0.5 x axis, w = 0.5` hemisphere assumption | FORMATS-ALR.md:130 | ~ | stage vertex bake | PR vertex shader constants set |
| STG-17 | `CHAR_LIGHT_REF` 0.5 and LIGHT_SUN / LIGHT_AMBIENT luma mapping "tuned by eye" | stage_look.rs:13-16; main.rs:35; GAME.md:172; FORMATS-STAGES.md:157 | T | character lighting per stage | SHEET |
| STG-18 | Fog read but drawn only with `PD_STAGE_FOG=1`; sets 2/3, point lights unused | FORMATS-STAGES.md:157 | ok | stage look | SHEET |
| STG-19 | Stage variants (night / weather grades) may not exist in data | PLAN.md:166; ROADMAP.md future | ? | variants | PR / data scan |
| STG-20 | `st09` hub and `st09_01..08` area files: no placed objects, script + light data not decoded | FORMATS-STAGES.md:17; PIPELINE.md:81 | ? | hub | none needed (out of scope) |
| STG-21 | Stage name mapping: Surface Log entries vs `st01`/`st09`; "World Beyond", "Tower of Sein" | PIPELINE.md:83 | ? | labels | WEB |
| STG-22 | Is `st01` / `st24` offered in versus | ROADMAP.md:89; HANDOFF.md:211 | ? | stage list | WEB / FOOT |
| STG-23 | Stage preview picture mapping (`STAGE_PREVIEWS` for 17 cells) partly guessed, later "matched to codes" | ui.rs:244-248; MENUS.md:87-120; HANDOFF.md:72 | U / ok | stage select | SHEET (MENUS.md says verified) |
| STG-24 | Character portraits: pc00a..pc10a in order, "matched by eye" | ui.rs:205-210; MENUS.md:63 | ok (confirmed by eye) | char select | none |
| STG-25 | Stage model slots 2nd/3rd = background/skybox; inner flag-0x10 shell blend unknown | FORMATS-ALR.md:132, 146 | ~ / ? | sky look | PR |
| STG-26 | Shadow volumes (material flag 0x2000, formats 0x10/0x07): stencil volumes identified; not drawn | FORMATS-STAGES.md:142; FORMATS-ANIMATION.md:74 | ~ (char version "probably") | shadows | PR |
| STG-27 | Material flags (0x01, 0x04, 0x08, 0x10, 0x20) meanings | FORMATS-ALR.md:133 | ~ | materials | PR |
| STG-28 | Collision: Highway/st09 connectivity, whether original closes edges with its own collision | COLLISION.md:68-70 | ? | bounds | decode 0x94 |
| STG-29 | st08 collapsing blocks and st01 tower pieces scripted | props.rs:45-50; FORMATS-STAGES.md:138 | ? | st08, st01 | PR |

## A10. UI, HUD, menus, front end (HUD.md, MENUS.md, FRONTEND.md, hud.rs, frontend/*)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| UI-01 | HUD layout is not stored with the art ("probably in the exe or .ssb scripts; not examined") | HUD.md:7 | ? | HUD | PR / SSB read |
| UI-02 | hud1 glow sprites purpose; hud0 coloured orbs vs capsule types | HUD.md:9-25, 136-138 | ~ | HUD art | settled for capsules by CAPSULE-VISUALS (see B) |
| UI-03 | HUD font: original `dbfont.ddx` now decoded; Bahnschrift stand-in remains for some text | HUD.md:67; hud.rs:486, 526; DECISIONS.md:31 | ok | HUD text | none |
| UI-04 | Cost text centring and dial offsets hand-tuned; `PD_DIAL_OFF` from A-J sheet | HUD.md:149-188; hud.rs:50-84 | T | HUD | SHEET (done once) |
| UI-05 | Rim/circle design picked by Laurence from sheet | hud.rs:66 | ok | HUD | none |
| UI-06 | Status icons not found in files (text tags stand in) | HUD.md:138; MOVES.md:116; NEXT-SESSION.md:(done list) | ~ | status display | PR icon draw; WEB |
| UI-07 | Opponent-damage stat "an approximation" | frontend/state.rs:101 | U | result screen | design |
| UI-08 | Arsenal rules: 30 skills, max 3 copies except Aura Particles, max 3 schools; guide quotes | arsenal.rs:18-21 | ok (guide) | builder | WEB / PR |
| UI-09 | Capsule colours in builder "assumed mapping" | builder.rs:139 | stale (see B) | builder | doc/comment fix |
| UI-10 | Menu layout from game's own art, fonts; unconfirmed names "fine as ids" | FRONTEND.md:156, 261, 318; MENUS.md:23-36 | ~ | menus | SHEET |
| UI-11 | Sound-trigger table for clips not a guess among 615 clips (future) | FRONTEND.md:318 | ? | menu anim | PR |
| UI-12 | Controls: gamepad layout from tutorial text; P2 keyboard test fallback | GAME.md:156 | ok | input | FOOT / WEB |
| UI-13 | `INVULN_BLINK_TICKS`, `LOCK_MARKER_FRAMES` 12 and frame 128 | main.rs:1242; hud.rs:103-105 | U | polish | FOOT |

## A11. Characters, animation, cloth (FORMATS-ANIMATION.md, FORMATS-ALR.md, dangle.rs, character.rs)

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| CHR-01 | Animation blending rules and events (when projectile spawns during ATK_SHOT); 5-frame blends | FORMATS-ANIMATION.md:89; main.rs:120 (`CLIP_BLEND_SECONDS` 5/30); PROGRAM-RE.md:441 | ~ | clip transitions | PR clip start routines |
| CHR-02 | `exmot0..5.ala` header differs, offset table counts (166, 2, 91) | FORMATS-ANIMATION.md:88 | ? | intro/outro | decode |
| CHR-03 | Camera animation chunk 0x7, dynamic lights 0x9, effect/shader bindings 0x12-0x14 | FORMATS-ALR.md:113 | ? | intros, effects | decode |
| CHR-04 | Joint flags +0x24 low byte (0x13/0x33 deforming, 0x10/0x30 leaves) | FORMATS-ALR.md:46 | ? | rig | PR |
| CHR-05 | Cloth parameters (velocity kept 0.9, limit 60, gravity 0.12 "unconfirmed"), COLF/COLP capsule radius x 5 (~), collision shapes "not built" | FORMATS-ALR.md:50; PROGRAM-RE.md:627-629, 651; dangle.rs:263-356 | ~ / ? | coat tails | PR `FUN_14025df70`, `FUN_140260750` |
| CHR-06 | Which of pc00a0's head pieces (draws on groups 3-18) the program shows at a time | PROGRAM-RE.md:619, 652 | ? | alt costumes | PR |
| CHR-07 | Character lighting: object-space normal map approximated via bind-space tangent frame; ramp `t3` "likely per stage" not in char file | FORMATS-ANIMATION.md:91-95; FORMATS-ALR.md | ~ | character look | PR ramp source |
| CHR-08 | Normal maps = textures with mean (127,127,255); effect-sampled blue-purple textures | FORMATS-ALR.md:109 | ~ / ? | materials | PR |
| CHR-09 | Vertex tangent/binormal at +24/+28 from shader signature, not decoded | FORMATS-ALR.md:82 | ~ | char lighting | decode |
| CHR-10 | Skinned palette: identity placeholder matrices ignored | FORMATS-ANIMATION.md:62; alr.rs:223 | ok | none | none |
| CHR-11 | `charaset.dat` +0x0C u16 120-285 meaning, voice bank +0x3C | FORMATS-ANIMATION.md:78 | ? | roster | WEB / PR |
| CHR-12 | P2 alternate costume chosen; 4 colour variants per character = colour variants (now used) | ROADMAP.md:64,86 vs AGENTS.md | stale (see B) | roster | none |
| CHR-13 | Helper-joint draws (b36 flag 0x04) skipped "probably shadow volumes" | FORMATS-ANIMATION.md:74 | ~ | shadows | PR |
| CHR-14 | Clip length int at controller +0x114 + 16 x clip, where filled not found | PROGRAM-RE.md:645 | ? | clip lengths | PR |
| CHR-15 | `CLIP` stale doc on `anim::Track::length` | FORMATS-ANIMATION.md:113 | fixed | | none |

## A12. Formats and pipeline

| id | what is guessed | where | conf | affects | settle |
|---|---|---|---|---|---|
| FMT-01 | Skill table unknown fields: header indices 3 (108,452) and 5 (374 "SkillLimiter") | FORMATS-SKILLS.md:25-27 | ? | none | WEB |
| FMT-02 | Stage `.dat` header blocks, SP00 sprite/UV tables | FORMATS-STAGES.md:31-40 | ? | stages | decode |
| FMT-03 | Texture pixel format 8 shared 32x32, DXT variants; "texture atlas 0x10" use | FORMATS-ALR.md:98, 113; FORMATS-EFFECTS.md:98 | ~ | UI, distortion | PR |
| FMT-04 | Esper-RE's 495-entry `eft.h` index vs our 476 effects / 496 packed entries | docs/research/DECODING_RESEARCH.md:45; FORMATS-EFFECTS.md:14 | ? | effects | WEB |
| FMT-05 | Composer credits (unconfirmed search summary) | docs/research/DECODING_RESEARCH.md:89 | U | credits only | WEB |
| FMT-06 | Shaders `SBTS` tangents, SD lighting constants | FORMATS-ALR.md:82,130 | ~ | lighting | PR |
| FMT-07 | `Data/Menu/BtMenuEn.alr` layout stored elsewhere | HUD.md:7 | ? | HUD | PR |
| FMT-08 | Effect archive entry word meaning; per-piece `FxDef` values from 0x13/0x14 chunks | FORMATS-EFFECTS.md:14, 84 | ~ | effects | PR |
| FMT-09 | Skill action-table rows: w39, w40, w48 etc. semantic readings marked ~ | PROGRAM-RE.md:58-67 | ~ | casts | PR |
| FMT-10 | Skill table `s12 == text id` explanation (see B) | FORMATS-SKILLS.md:49 | stale | | see B |
| FMT-11 | Effect-name list Japanese `name.lst` reliability; effect numbers 607-609 do not exist | CAPSULE-VISUALS.md:40 | ok | | none |
| FMT-12 | Level cap source; `gsdata 0x33044` min/max | PROGRAM-RE.md:601 | ? | see CMB-06 | PR |
| FMT-13 | Stage BGM identifications: 12 of 19 tracks identified; others guess | ROADMAP.md (done list); PIPELINE.md:85 | ~ | music | EAR |

---

# Section B. Contradictions and stale statements

"Right side" = which side is probably correct, judged from dates, evidence cited and whether code matches. All are stated with both locations.

| id | side 1 | side 2 | probably right |
|---|---|---|---|
| B-01 | GAME.md:80 "cam_yaw swings behind a running player at `FREE_CAM_FOLLOW` (2.5/s)" | sim.rs:592-598 "Running with the left stick never turns it"; no `FREE_CAM_FOLLOW` anywhere in crates (grep confirms none); MOVEMENT-AUDIT.md:53 already flagged | Code. GAME.md is stale. Whether the original follows is a separate open question (MOV-01, CAM-01) |
| B-02 | FORMATS-SKILLS.md:49 "s12 ... Not an animation: equals the text id on most records" | FORMATS-SKILLS.md:98 and 152 "s12 = GROUND_ROW, row in the action table" | Lines 98/152 (program read). Line 49 stale |
| B-03 | FORMATS-SKILLS.md:50 "s13 probably a projectile/effect id" | FORMATS-SKILLS.md:98, 153; FORMATS-AUDIO.md:76 "not a sound, the mid-air action row" | Program-read side. Line 50 stale |
| B-04 | FORMATS-SKILLS.md:51 "s19-s21 ... Esper-RE: sound effects. Unconfirmed" | FORMATS-SKILLS.md:158 and FORMATS-AUDIO.md:75-77 (s19 fire-tick, s20 loop, s21 impact, program-confirmed) | Later lines; line 51 mild stale |
| B-05 | FORMATS-SKILLS.md:59 s40 = 2 "maybe can be used in mid-air, Unconfirmed" | FORMATS-SKILLS.md:154 "s40 = which row to use ... confirmed" | Line 154 |
| B-06 | FORMATS-SKILLS.md:83-92 family table says range band / reach / speed "Unconfirmed" for many motions | FORMATS-SKILLS.md:157 and PROGRAM-RE.md:222, 255, 280, 311 give program readings for motions 3/12/13/14, 8, 15/16, 7/20, 17 | Program readings; unconfirmed cells are partly stale |
| B-07 | FORMATS-EFFECTS.md:143 "every skill with a launch sound (s13)" | FORMATS-EFFECTS.md:145 correction paragraph, FORMATS-AUDIO.md:76 | Correction. Line 143 not edited (kept as history) |
| B-08 | PIPELINE.md:20 "eftNNNN.alr (NNNN = skill text id)"; HANDOFF.md:314 "Effect number = skill text id (proved)" | FORMATS-EFFECTS.md:19 "effect number = action-table row's effect column (w2), not the text id" | FORMATS-EFFECTS.md:19 |
| B-09 | HANDOFF.md:208 "s21 impact, s20 flight loop, s13 launch, s19 fall whistle, s8 activation" | FORMATS-AUDIO.md:76-78 (s13, s8 not sounds) | FORMATS-AUDIO.md |
| B-10 | FORMATS-AUDIO.md:95 "Known bug: playing s13 as sound at cast time (sfx.rs)" | sfx.rs:75-88, 509 already state s13 is not used (only `FIELD_CAST_B` = 19, 300..=699 filter) | Code. The bug note is stale; but see B-11 |
| B-11 | FORMATS-AUDIO.md:77, 93 / sfx.rs:88 s19 "played at cast" | FORMATS-SKILLS.md:158 "s19 is played at launch (fire tick)", SKILL-FIELD-CROSSWALK.md:26 "rename: shot sound" | FORMATS-SKILLS.md (program `FUN_1402626d0`). Code still plays at cast: a real behavioural gap |
| B-12 | FORMATS-AUDIO.md:5 "Nobody has listened"; FRONTEND.md:387 | HANDOFF.md:211 "Needs Laurence: listen" | All agree, still true; not a contradiction but a global open item |
| B-13 | FOOTAGE.md:33, 40-41 "our converter `const FPS: f32 = 24.0`" / "24 fps our converter currently assumes" | character.rs:17-22 `const FPS: f32 = 30.0`; FORMATS-ANIMATION.md:38, INDEX.md:41 | Code (changed 2026-09-27). FOOTAGE.md top part is historical |
| B-14 | FOOTAGE.md:40 "FORMATS-ANIMATION.md:18 assumes 24 fps" | FORMATS-ANIMATION.md:18 now says 30 fps | FORMATS-ANIMATION.md |
| B-15 | HANDOFF.md:51 "Doc corrections still owed: FORMATS-ANIMATION 24 fps; INDEX 24 fps line" | FORMATS-ANIMATION.md:38, INDEX.md:41 already corrected | Docs corrected; HANDOFF line stale |
| B-16 | MOVES.md:43, 47 "Barrier: time = s46 x 2 sim ticks (Crystal Wall 23 -> 0.77 s)"; GAME.md:145 "Crystal Wall 0.77 s"; barrier.rs:75-78 | PROGRAM-RE.md:347 and FORMATS-SKILLS.md:64: s46 - 1 program ticks = 22 (0.73 s); HANDOFF.md:51 lists "barrier ~ s46-1 ticks" as owed | PROGRAM-RE.md. Code and MOVES.md still use the old reading |
| B-17 | MOVES.md:50 "Brush stun 0.75 s" / barrier.rs:59 45 ticks | HANDOFF.md:51 "Brush stun 1.0 s" owed | HANDOFF flags later reading; no PROGRAM-RE line checked in this sweep |
| B-18 | MOVES.md:58 "Tornado homing 1.5"; crawl.rs:23 `TORNADO_TURN` 1.5 | HANDOFF.md:51 "Tornado no homing" | HANDOFF (reading lane), doc and code still carry the guess |
| B-19 | MOVES.md:43 Parabola s66/s67 | HANDOFF.md:51 "Parabola s66/s67 swap" | HANDOFF correction pending |
| B-20 | MOVES.md (mines) / GAME.md "mines hurt only the opponent" (HANDOFF.md:258) | HANDOFF.md:51 "mines hurt owner" | HANDOFF (program read), doc and code not updated |
| B-21 | GAME.md:136 "Aura refills 1 point/s ... refill rate is a placeholder"; combat.rs:32-34 | PROGRAM-RE.md:479, 598 exact curve max(0.25, 1.25 - Aura/10) | PROGRAM-RE.md |
| B-22 | GAME.md:138 "one of yours every 2 s" capsules; combat.rs:36-38 comment "5 s: 3 spots x 15 s" and value 300 ticks | combat.rs:30 15 s respawn per spot; PROGRAM-RE.md:602 14 s (420 ticks) per spot | PROGRAM-RE.md. GAME.md line and CAPSULE_EVERY_TICKS comment disagree with each other |
| B-23 | DECISIONS.md:39, HANDOFF.md:65 "15 s unconfirmed timing" | PROGRAM-RE.md:602 "420 ticks (14 s)" | PROGRAM-RE.md |
| B-24 | GAME.md:144 "dash at the target if within 130 u ... dash speed 260 placeholders" | MOVES.md:41 "only when locked on and target within 80 u (distance band 1)"; combat.rs:103-111 (80, LUNGE 240); PROGRAM-RE.md:536 | PROGRAM-RE.md, combat.rs. GAME.md stale |
| B-25 | DECISIONS.md:34 "Short-range always dash at the target (up to 0.6 s at 260 u/s)" | MOVES.md:41, combat.rs:110 "dash only with target in band 1 (80 u)" | PROGRAM-RE.md / code |
| B-26 | HANDOFF.md:106 "Match start: Level starts at 0 (ours START_LEVEL = 1)" | combat.rs:24 `START_LEVEL: 0` | Code (done) |
| B-27 | HANDOFF.md:104 "Skill uses (s26): ... `main` cu[rrently]..." | AGENTS.md "Skills stay on their button until uses (s26) run out" | AGENTS.md (done) |
| B-28 | HUD.md:25 "capsule colours erase cyan, status magenta, environment green ... assumed" and HUD.md:136 "hud.rs `capsule_color()` currently synthesises" | CAPSULE-VISUALS.md:155 "StrategyWiki right, HUD.md wrong: erase purple, status green, environment aqua"; hud.rs:321-326 uses the program table | CAPSULE-VISUALS.md + code. HUD.md:25 and builder.rs:139 comment stale |
| B-29 | HUD.md:12 hud1 glow sprites "likely in-world capsule art" | CAPSULE-VISUALS.md:191 "not used by this draw path; probably HUD-only" | CAPSULE-VISUALS.md (later) |
| B-30 | GAME.md:136 "Aura | capsule ... rate placeholder" vs AGENTS.md "capsule spots full at the start and refilled after" | DECISIONS.md:39 | Minor: doc split across three places |
| B-31 | MOVES-PLAN.md:10 "[Arc] flies straight (stand-in)" and :20 "attack 12 shell generic projectile (placeholder)", :11 "Not drawn: lights ... slot 2 launch", "Effect speed 30 fps (unconfirmed)" | MOVES.md:47 [Arc] built with bend; FORMATS-EFFECTS.md:33 slot 2 = ground impact; effects drawn incl. LIGHT02 (FORMATS-EFFECTS.md:66) | MOVES.md / FORMATS-EFFECTS.md. MOVES-PLAN.md is an early-planning snapshot |
| B-32 | HANDOFF.md:325 "slots 0 + 2 on the caster" | FORMATS-EFFECTS.md:33 slot 2 is ground impact where shots end | FORMATS-EFFECTS.md |
| B-33 | ROADMAP.md "Next" item 1 (Laurence plays M1, placeholder rules), 2 (HUD done, sound effects, capsule glows), 3 (generalise skills by family), 4 (stage props phase A and sim, PK, jump zones, start points), 6 (character select) | AGENTS.md current state lists all as done: HUD, sounds, all families, props, jump zones, start points, 3D character select | AGENTS.md. ROADMAP.md "Next" is stale (only item 5 sky partly done, 7 first-run converter, 8 online remain) |
| B-34 | ROADMAP.md:82 "Do coat-tail parts swing with physics? (Assumed yes; currently rigid.)" | ROADMAP.md done list "Cloth physics ..."; GAME.md:110 cloth section; PROGRAM-RE.md:627-629 | Cloth built; open question stale |
| B-35 | ROADMAP.md:64 "Convert the 4 variant models per character (likely colour variants)" and :87 `pc00a0..3` question | PROGRAM-RE.md:619 colour-variant files and AGENTS.md "30 fps clips from the colour-variant character files", "P2 alternate costume" | Done in part; question stale |
| B-36 | ROADMAP.md:85 "Which original split layout" | GAME.md:94 top/bottom built; still unanswered about the original | Open (kept) |
| B-37 | ROADMAP.md:87 "Which animation does Psycho Wave use ... ATK_SORD by family" | FORMATS-SKILLS.md:152 and PROGRAM-RE: action-row clip (w38) | Program read. Stale |
| B-38 | ROADMAP.md:88 "Which BGM is Palace/Strange City ... plausible guess" | AGENTS.md "Palace/Strange City music matched (FORMATS-STAGES.md)" | AGENTS.md. Check FORMATS-STAGES.md for the evidence; ROADMAP stale |
| B-39 | ROADMAP.md:88 "Are skills picked up automatically?" | GAME.md:140 capture-by-pressing rule from tutorial text, PROGRAM-RE.md:602 | GAME.md. Stale |
| B-40 | ROADMAP.md:85 "Where does skill behaviour live ... probably s64-s69" | FORMATS-SKILLS.md:157; PROGRAM-RE.md | Answered: exe + gsdata |
| B-41 | NEXT-SESSION.md:3-4 "`git log -1` should be bd932a9; tests 197 + 16" | NEXT-SESSION.md:19 "tests 7 + 36 + 276" | Later line; top paragraph stale |
| B-42 | NEXT-SESSION.md:~36-43 "Do next, in order": 1 Controls report, 2 Arena objects leftovers, 3 tight corridors, 4 MOVES stragglers, 5 builder art / design docs | HANDOFF.md:51 "backlog item 4 is superseded"; DESIGN-2V2/BRAWL/REPLAYS exist (2-v-2 design doc done); builder and menus rebuilt (MENUS.md) | Items 1, 5 stale; 2 and 4 partly done |
| B-43 | NEXT-SESSION.md:(item 1) "jump apex ~ 0.75 m today" | sim.rs:54-58 jump apex 14 u (~1.2 m) from program | Code/PROGRAM-RE.md |
| B-44 | FORMATS-STAGES.md:96 heading "Start points: layout ✅, use ⚠️" | PROGRAM-RE.md:602 "✅ used" and GAME.md:138-139 | PROGRAM-RE.md; heading stale |
| B-45 | FORMATS-STAGES.md:35 header 0x98 "EF00 stage effects ❓" / :77 "contents ❓" | FORMATS-STAGES.md:170-191 decodes EF00 records (kinds 3, 9, 11, 16) | Later sections; the early table is stale |
| B-46 | FORMATS-ALR.md:113 "0x7 camera animation ❓, 0x9 dynamic lights ❓" | FORMATS-EFFECTS.md:66-73 LIGHT02 dynamic light now decoded as an effect-side emitter | Distinct items; not a contradiction, but 0x9 note is vague |
| B-47 | FORMATS-ALR.md:46 flags "low byte ❓" vs FORMATS-ANIMATION.md:74 "b36 flag 0x04 helper ... probably shadow volumes" | FORMATS-ALR.md:46 "0x04 = helper ... ✅" program-read | FORMATS-ALR.md (later); ANIMATION line still says "probably" |
| B-48 | FORMATS-ANIMATION.md:52 "Still different: root motion not applied (ATK_SORD's 18 u forward stride plays in place)" | HANDOFF.md:37 "ATK_SORD's knee-up stride + slash matches the footage"; PROGRAM-RE.md:424 root motion flag | Unclear; both partly true |
| B-49 | FORMATS-ANIMATION.md:83-84 "Distances, speeds and recovery length not in the table (sim.rs constants, unconfirmed)" | PROGRAM-RE.md:553-554 gives 40 u over 21 ticks, locked = clip length | PROGRAM-RE.md. Constants in sim.rs:80-85 not updated |
| B-50 | PROGRAM-RE.md:474 "one move for both dodges (DASH_A, 40 ticks)"; DECISIONS.md:38 "Tumble 0.5 s / Cartwheel 0.62 s + 1.04 s recovery" | sim.rs:81-85 tumble 30 ticks, cartwheel 37 + 62 recovery | PROGRAM-RE.md. Sim still has the old two-way split |
| B-51 | sim.rs:66-69 "LAND_TIME ... JUMP_LAND1 13 frames @ 30 fps" (light) vs PROGRAM-RE.md / FOOTAGE.md:157 measured ~300-370 ms | FOOTAGE says closer to 433 ms than 542 ms but shorter still | Open; code matches program reading |
| B-52 | combat.rs:68-70 comment "sim::GRAVITY ... is tuned for ordinary jumps" | sim.rs:46 GRAVITY is now the program's 0.12 u/tick2 (108 u/s2) | sim.rs. Comment stale ("tuned") |
| B-53 | parabola.rs:20-23, MOVES.md:43 "gravity 150 = half the players' (at 300 ...)" | sim.rs:46 player gravity is 108, not 300; KNOCKDOWN_GRAVITY 108 | Code. Parabola comment's premise is stale |
| B-54 | MOVES.md:39-41 knock-down kinds and "s57-s59" | HANDOFF.md:51 "s57-s59 are light/medium/heavy" owed | HANDOFF |
| B-55 | FORMATS-SKILLS.md:108-110 "distance 1/10 likely; speed 3 u/s per point assumed" | PROGRAM-RE.md:80 (speed /10.8 = 2.78) and combat.rs:70-74 uses 30/10.8 for launch, moves/mod.rs:39 uses 3.0 | PROGRAM-RE.md. Two speed units coexist in code |
| B-56 | FORMATS-SKILLS.md:64 "s46 placeholder 1 when s45 computes" (old) | FORMATS-SKILLS.md:64 also defence guard duration ✅ | Same line, both true for different families |
| B-57 | GAME.md:136 "Start points: start 0 and 1" vs FORMATS-STAGES.md:110 "which set the game uses ❓" | PROGRAM-RE.md:602 | Open in FORMATS-STAGES, used in code |
| B-58 | DESIGN-REPLAYS.md / GAME.md:22 float determinism vs AGENTS.md hard rule 3 "deterministic" | sim runs at 60 Hz fixed while program 30 Hz (sim.rs:12) | Consistent; note: not bit-exact cross-platform |
| B-59 | AGENTS.md "Bevy 0.19, glam 0.32" vs TOOLCHAIN.md (not checked) | not verified | not checked |
| B-60 | AGENTS.md "`.claude/skills/ (build-run, convert, verify, reverse-engineer)`" | CLAUDE.md names pd-build-run, pd-asset-pipeline, pd-visual-verify, pd-reverse-engineer | CLAUDE.md. AGENTS layout line uses the old names |
| B-61 | HANDOFF.md:53 "Highway and st24 huge invisible floor ... new issues" | COLLISION.md:68, AGENTS.md "collision fixes" | Possibly fixed; COLLISION.md:70 says two Highway starts still unconnected |
| B-62 | CAPSULE-VISUALS.md:3 "placeholder capsules (fx.rs 2.5 u sphere)" | CAPSULE-VISUALS.md:203-248 replaced; fx.rs:591 | Later sections |
| B-63 | CAPSULE-VISUALS.md:10 "500s unreachable ❓" vs :47 decoded names of loose-only numbers | same file | Later |
| B-64 | VFX-AUDIT.md:47-55 LIGHT02 "Not drawn" then "Fixed 2026-09-28" | FORMATS-EFFECTS.md:73 "Drawn" | Fixed |
| B-65 | FORMATS-EFFECTS.md:15 `ecopy.pl`/`x.bat` "placeholders 520-550" | FORMATS-EFFECTS.md:14 archive authoritative | Historical note |
| B-66 | docs/research/DECODING_RESEARCH.md:41 "FORMATS-AUDIO.md calls s19 PostHitSFX (marked uncertain, 'played at cast')" and "behaviour matches" | FORMATS-SKILLS.md:158 and program fire-tick read: s19 at launch | FORMATS-SKILLS.md. "Behaviour matches" is not quite: cast vs fire tick |
| B-67 | docs/research/DECODING_RESEARCH.md:13 "Many skill visual effects are guessed" vs :102 "well ahead" | same file section 11 | Section 11 (dated 2026-09-30) is the current position |
| B-68 | FORMATS-SKILLS.md:56 s26 "⚠️ probable" vs AGENTS.md "Skills stay on their button until uses (s26) run out" | HANDOFF.md:104 | Code acts on a probable reading |
| B-69 | HUD.md:47 "remain unconfirmed as program colour constants" for ring shades | hud.rs:103-110 same | Consistent |

---

# Counts and top five

Section A rows by system: Movement 26, Camera 9, Combat 36, Defence/status 23, Attacks 40, Effects 34, Orb/school/lock-on visuals 18, Sound 27, Stages 29, UI/HUD/menus 13, Characters/animation 15, Formats 13. Total 283 rows. Section B: 69 rows (about 10 of them are "still true open item, no conflict").

Top five by impact (what changes the feel of every match most):
1. Table units: speed 3 u/s per point and distance 0.1 (and a second unit 30/10.8 already in combat.rs); homing scale 0.1 rad/s. Affects every projectile (ATK-01 to ATK-03).
2. Camera: `configdata.dat` 13-float blocks unread, free camera does not follow, lock-on rates, framing all by eye (CAM-01 to CAM-05, MOV-01).
3. Aura system: flat 1 pt/s refill, placeholder school rate %, and the 15 s versus 14 s capsule respawn, while the program formula is already read but not applied (CMB-01 to CMB-03).
4. Sound: nobody has ever listened to a decoded sound; system sound ids, menu cue ids, voice groups, event volume/pan/pitch all unheard (SND-01 to SND-16).
5. Effect blend/words and clock: per-class blend state (ice pieces white, dark skills), effect frame clock, spawner emitters, packed-only numbers, glow buffer (FX-01 to FX-15, FX-27).
