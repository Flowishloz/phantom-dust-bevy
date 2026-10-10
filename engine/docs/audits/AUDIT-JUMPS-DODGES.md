# Audit: jumps, landing, dodges vs the original (2026-09-30, cloud, read-only)

Same method and caveats as `MOVEMENT-AUDIT.md`: documents and code only, nothing run, no game data, no Ghidra. "Program read" = what `PROGRAM-RE.md` says was read from `PDUWP.exe`; I add no facts of my own about the program. Line numbers are `engine/crates/pd_game/src/sim.rs` unless stated. Laurence says movement feels inaccurate; items likely to change feel are marked **[FEEL]**.

Verdicts: MATCH / ADDED BY US / DIFFERS / UNKNOWN (program not read).

## 0. Headline

1. **The ground dodge is still the pre-program-read guess.** PROGRAM-RE says tumble and cartwheel are one move (751): DASH_A 40 ticks, scripted 40 u over 21 ticks plus ~41-43 u clip drift (about 80 u), player locked 1.33 s, no recovery phase. Ours: two different moves, tumble 0.5 s at 130 u/s (65 u), cartwheel 0.62 s at 180 u/s (111 u) then a 1.03 s recovery (`sim.rs:75-85`). The numbers are the same ones PROGRAM-RE's comparison table lists as our old column (line 474). **[FEEL]**
2. **The jump dodge has no landing lock in ours.** Program: DASHEND_B always played, 11 ticks locked (about 0.37 s). Ours: apex 14 u is under the 40 u landing floor, so no lock at all (`sim.rs:609-616`). GAME.md "Landings" admits this. **[FEEL]**
3. **Air control differs in three ways** (walk/run gait instead of run speed, 0.6 velocity blend per 60 Hz tick, speed status applied). **[FEEL]**
4. **Take-off is instant in ours**; program has a wind-up (clip length - 2 ticks; 2-3 frames for jumpers, so small) and no tick-rate issue for the jump itself.
5. **Skywalk / Levitate / Jump skill (s43 = 3) are built from the skill notes text, not the program read** (hover teleports to a fixed height; program: a jump, then a float at the apex).

## 1. Jump (dodge 750: Hero, Freia, Meister, Chunky, Know, Tsubutaki)

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Button works only in states 0 / 1 / 8 (idle, walk/run, turn/strafe); no air dodge | PROGRAM-RE "Air control", `FUN_1402517c0` | requires `p.grounded && jump_pressed && !busy` (`:525,545`); `rooted()` (casting, stunned, down) zeroes `input.jump` (`:314-317`) | MATCH in spirit | Are states 9 / 13 (short recoveries) and 14 (hit) excluded by ours? `rooted()` (combat.rs:506) covers cast/hit/down; confirm it covers 9/13 equivalents. `FUN_1402517c0` |
| Edge trigger (press, not hold) | not stated | `jump_pressed = input.jump && !p.prev_jump` (`:519`) | UNKNOWN | Does `FUN_1402517c0` test the held bit or a press edge? Is a press buffered during a landing lock? |
| Launch: h = s45 x 0.1 = 1.4, g = (s47+100) x 0.0012 = 0.288 u/tick2, v = sqrt(20 g h) = 2.84 u/tick | `FUN_140257b90`, `FUN_140270200` | `JUMP_VELOCITY` 85, `DODGE_JUMP_GRAVITY` 259.2 (`:54,58`), hardcoded, not read from records 749 | MATCH (values) / ADDED (hardcoded for one record) | none for values; if the records ever change, derive from s45 / s47 |
| Apex 14 u, ~20 ticks (0.66 s) airborne | same, "Dodge: Jump" | apex 85^2 / (2 x 259.2) = 13.9 u; 0.66 s up+down | MATCH | none |
| Integration at 30 Hz discrete vs ours 60 Hz | program steps v then position per 30 Hz tick | semi-implicit Euler at 60 Hz (`:556-559`) | DIFFERS slightly (sub-unit apex / airtime) | Is gravity applied before or after the position move in `FUN_140254590`? Affects apex by about 0.15 u, airtime by under 1 tick |
| Wind-up: plays the start clip and waits clip length - 2 ticks (state 4) | `FUN_140257b90`; for jumpers DASH_A is 2-3 frames | none, velocity set the same tick (`:545-550`) | DIFFERS (about 0-1 ticks for the dodge; 13 ticks for skill 239 Jump) | Confirm DASH_A really is 2-3 frames in the colour files each jumper loads (pcXXa0-3.alr) |
| Air control: turns at the turn rate, moves along facing at **run speed x turn factor, no speed-status bonus** | PROGRAM-RE "Air control" | wish uses `gait_speed` = **walk OR run by the stick gait**, **times `speed` (status)** (`:449,516`); in the air the locked strafe branch also applies (`:513`); then velocity blended at `AIR_CONTROL = 0.6` per 60 Hz tick (`:44,541-543`) | DIFFERS x3 (walk gait in air, speed status, invented blend); `AIR_CONTROL` ADDED BY US | In `FUN_14025b0f0`: is the air speed fixed at run speed regardless of stick magnitude? Is facing turned toward the stick or (locked) the target? Is there any horizontal velocity blend or is it set directly? **[FEEL]** |
| No stick in the air | not stated | velocity decays by 0.6 per tick toward 0: momentum dies in about 5 ticks (about 0.08 s) | ADDED BY US (implied) | `FUN_14025b0f0`: with no stick, is horizontal motion zero or carried? **[FEEL]** |
| Stick dead zone at take-off (0.4 of stick) | `FUN_140256580/600` dead zone 13,000/32,767 | `has_stick = mag >= 0.4` (`:445`); but `main.rs:1056` ignores pad axes under **0.2** before `Input` | DIFFERS (two different dead zones, plus no rescale, see MOVEMENT-AUDIT A) | as MOVEMENT-AUDIT A |
| Clips: rise = JUMP_UP stretched to v/g ticks, fall = JUMP_DOWN stretched to the same; dodge uses **kind 5 = DASH_A / DASHEND_A / DASH_B / DASHEND_B** | "Jumps" `FUN_14025a0b0`, "Dodge: Jump" | `Motion::Rise -> JUMP_UP`, `Fall -> JUMP_DOWN`, both stretched by `rise_seconds` (`main.rs:250-251,1680-1688`); clip lengths hardcoded 25/30 s and 10/30 s (`main.rs:125-126`) | DIFFERS (wrong clips for the dodge; PROGRAM-RE says the colour files' JUMP_* are ordinary jump clips but the dodge uses the DASH set) | Confirm clip names per kind (`0x1403fae20` table). Render side only |
| Walk-off-ledge starts the same fall (state 5) | "Gravity" | falls under `GRAVITY` (`:555`) once `grounded` drops | MATCH | none |
| Coyote time: 6 ticks (0.1 s) grounded over gaps | not in program read | `COYOTE_TICKS = 6` (`:72,582`); jump allowed during it | ADDED BY US | How does the program decide grounded off an edge? (`FUN_140254590`) Is there any grace? Also whether a jump is possible in that window |
| Ledge step-up / step-down 6 u | not read | `STEP_HEIGHT = 6.0` (`:60,573`) | ADDED BY US | Floor-follow rule in `FUN_140254590` / collision (step height, snap-down distance). Affects when a walk-off counts as a fall and the fall-height figure |
| Fall cap 7.5 u/tick (225 u/s), gravity 0.12 | "Gravity" | `GRAVITY` 108, `FALL_SPEED_CAP` 225 (`:46,48`) | MATCH | none |

## 2. Landing

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Choice by fall height (highest point minus landing point): < 40 none (kind entry 3), 40-80 JUMP_LAND1, > 80 JUMP_LAND0 + dust | `FUN_140254c80`; floats `0x1402f7ff0` = 40, `0x1402f8040` = 80 | `air_peak_y - pos.y` with `>` tests (`:609-615`); test `landing_choice_by_fall_height` | MATCH (boundary `>` vs `>=` unknown) | `FUN_140254c80`: strict or non-strict compare at exactly 40 / 80? Is "highest point" tracked from the launch or from the last grounded tick? |
| Walk-off falls of under 40 u: no lock, straight back | "Landing" | no lock (`:609-616`) | MATCH | none |
| **Jump dodge always lands with DASHEND_B, 11 (22 for Freia / Tsubutaki) ticks locked, regardless of height** | "Dodge: Jump" (11 ticks; clip 12, Freia / Tsubutaki 23) | not built: under 40 u -> no lock, no landing clip (GAME.md "Landings", HANDOFF "still missing -> M11") | DIFFERS | `FUN_140254c80` with jump kind 5: where is the "always play the kind's landing" branch and does it skip the 40 u test? Per-character clip length (12 vs 23). **[FEEL]** press-to-free is about 32 ticks (1.05 s) in the original; ours about 21 ticks (0.7 s) |
| Lock length = clip length - 1 ticks; light 12 ticks, heavy 29 | "Landing" (clip lengths from the colour files) | `LAND_TIME 12/30`, `HEAVY_LAND_TIME 29/30` hardcoded from **pc00a** clip lengths (`:66-69`) | MATCH for pc00a; ADDED (one length for all 25 characters) | Landing clip lengths per character in the files the game actually loads (colour variants); PROGRAM-RE line 568 says motion sets differ from the base file |
| Timer quantisation: lock set then decremented the same tick | n/a | `land_timer = X` then `max(DT)` subtract at `:625` | DIFFERS by 1 of our ticks (16 ms) | none, just fix when editing |
| Landing plays **with root motion** (moves the player by the clip's root x/z) and sets gravity back to 0.12 | "Root motion" (landings, jump start, dodges) | no root motion in the sim (grep finds none); velocity goes to 0 the moment the lock starts (`:511-517`, control 1.0) | DIFFERS (PLAN M12 not built) | Root drift of JUMP_LAND0/1 and DASHEND_B (`pd_convert keys`) |
| Jump start (JUMP_START) root motion -6 u forward | "Root motion" | none | DIFFERS (skill 239 Jump only) | as above |
| Dust effect on the heavy landing | `> 80` | "not built here -- fx lane" (GAME.md) | DIFFERS (not built) | none in program; fx lane |
| Landing dodge-jump "dust/impact flash" seen in footage (Jump.mp4, ~2.87 s) | FOOTAGE "Jump" | none | UNKNOWN | Is the flash the heavy-landing dust, a DASHEND_B effect, or a clip-attached effect? Nothing in PROGRAM-RE says |

## 3. Ground dodge (751: Edgar, Cuff Button, pH, JD, Sammah)

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| **One move for tumble and cartwheel** (the difference is only the clip) | "Dodges" `FUN_140270200`, s44 = 8, action row 355 | `Dodge::{Tumble, Cartwheel}` with different speed, time and recovery (`:81-85,525-535`) | DIFFERS | Does any branch in `FUN_140270200` / `FUN_1402517c0` test the character (or clip length) to change behaviour? PROGRAM-RE flags "does JD / Cuff Button's tumble really lock 40 ticks like the cartwheel" as open (line 646) |
| Plays DASH_A (40 frames in the colour files), state 3 lasts the clip length = **40 ticks, 1.33 s** | same | tumble 30 ticks = 0.5 s; cartwheel 37 ticks = 0.62 s + 62 recover = 1.65 s (`:81-85`); recovery only for cartwheel | DIFFERS (tumble 0.83 s too short, cartwheel 0.32 s too long) **[FEEL]** | Confirm the state-3 timer uses the clip length int (+0x108 table) not w42 - w41 |
| Distance: 40 u scripted over w42 - w41 = 21 ticks (1.9 u/tick = 57 u/s) **plus** the clip's root drift (~41-43 u) = about 80 u | same, marked warning | tumble 65 u, cartwheel 111 u at flat speed (`:82,84`), ramp-less | DIFFERS; speeds "guesses, unconfirmed" per the comment (`:80`) | Confirm the scripted 40 u is added to root motion and not replacing it; the real speed profile (scripted 57 u/s for 21 ticks then clip drift only). Footage side view per PROGRAM-RE 646 **[FEEL]** |
| Flat speed, instant start | not stated | constant speed `v`, velocity set the same tick (control 1.0) | ADDED BY US (shape) | Speed profile over time: scripted term is linear, root drift follows the clip curve |
| Direction | "moves forward"; heading = facing | along the **stick** (or, no stick, toward the target / camera forward) (`:526`) | UNKNOWN / ADDED BY US | Which heading does the scripted move use (+0x12e68 facing or +0x12e6c heading)? Does the stick change it? With no stick and free camera, ours goes camera-forward, not character-facing |
| Turning / steering during the dodge | not read | `p.yaw` keeps turning toward stick/target at 600 deg/s; the dodge vector is frozen at press (`dodge_dir`) | UNKNOWN | Whether the body keeps turning in state 3 |
| Speed status (Speed Up, Lightning Speed) | not stated | scales the dodge speed, not its duration (`:531`) | ADDED BY US | Does the scripted dash use the speed status? (The air move explicitly does not.) |
| No Aura refill during the dodge | "Aura refill" (states 3-4 pause it) | aura refill in `tick_fighter` has no dodge check and runs at a flat 1 point/s (`combat.rs:1142-1153`) | DIFFERS (PLAN M13, phase-3 rule not built) | none; the rule is read (`FUN_1402523a0`) |
| Dodge ends on hit | not stated | `rooted()` zeroes `dodge_ticks` (`:317`) | UNKNOWN | State change 3 -> 14 and whether a hit during state 3 is possible; any invulnerability during the dodge (not stated anywhere in the docs) |
| Dodge from state 0 / 1 / 8 only, and not during landing | "Dodges" | `!busy` (dodge, recover, landing) (`:524`) | MATCH | none |
| No stamina / aura cost | not stated | none | UNKNOWN | Does record 750 / 751 carry a cost (s-field)? Docs say no cost; not checked |
| Cartwheel recovery 1.03 s | "DASHEND_A 1.04 s" is from the **base** file | base-file clip lengths (`:78-79`), which PROGRAM-RE line 568 says the game does not load (colour files: DASH_A 40, DASHEND_A 1-frame stub) | DIFFERS (derived from the wrong file) **[FEEL]** | Colour-file clip lengths (already in PROGRAM-RE) |
| Clips: Tumble `DASHEND_A`, Cartwheel `DASH_A` then `DASHEND_A` | "DASH_A with root motion" | `main.rs:258-262` ("Unconfirmed pairing") | DIFFERS (in the colour files DASHEND_A is a 1-frame stub, so tumble shows one pose) | Use DASH_A for all five |

## 4. Float / Skywalk / Levitate / Jump skill / jump zones

| Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|
| Skill 239 Jump: s44 = 0, h = s45 = 10, apex 100 u, g 0.12, JUMP_START 13 ticks first, JUMP_LAND0 on landing | "Jump (skill 239)" | `sqrt(2 x GRAVITY x 30 ft x 3.66 = 109.8)` = apex **109.8 u**, instant, no start clip (`moves/special.rs:312-319`) | DIFFERS (apex 10 % high, no wind-up; "30 ft" text used instead of s45) | `FUN_140270200` s44 = 0: confirm h = s45 (x1.0) gives 10 x h = 100 u |
| Skywalk / Levitate (s44 = 5): float status for s46 ticks, then jump (apex 50 u / 100 u), **floats at the apex** (no gravity, run speed x turn factor, FLOAT_* clips) until the status ends or the button is pressed again | "Skywalk / Levitate" | `hover_ticks` / `hover_height` (Skywalk 15 ft = 54.9 u, Levitate 30 ft = 109.8 u, from note text); the sim **sets y = floor + height instantly** every tick, no rise, no button to end (`sim.rs:336-344`, `special.rs:304-308`, `status.rs:264-281`) | DIFFERS (hover is relative to the floor below, so it follows terrain; program floats at the apex height and moves freely) | How is the float height/apex derived? Which s-field gives Skywalk's 50 u? Can the jump button end it; does a hit end it (status 11, unread, PROGRAM-RE 642)? **[FEEL]** for anyone who plays those skills |
| Move while floating | run speed x turn factor | normal walk/run rules while hover holds y | DIFFERS | as above |
| Jump zones launch: speed = param | "unconfirmed unit" (GAME.md) | `vel.y = param` under ordinary gravity 108 (`:329-335`) | ADDED BY US (unit guess) | Stage jump-zone handler: which function launches, what gravity / height. Not in PROGRAM-RE |
| Dash skill (s44 = 8) 100 u over 15 ticks, stops at walls | "Dodges and movement skills" | instant teleport 30 ft = 109.8 u, no wall check (`special.rs:322-327`) | DIFFERS | none, read already |

## 5. Numbers tuned by eye or guessed (in the code or only in docs)

| Constant | Where | Note |
|---|---|---|
| `AIR_CONTROL = 0.6` | `:44` | no comment, no source; per 60 Hz tick |
| `COYOTE_TICKS = 6` | `:72` | invented |
| `STEP_HEIGHT = 6.0` | `:60` | invented, also used in walls / ground checks |
| `FALL_RESPAWN_BELOW = 150` | `:70` | invented (game rule unread) |
| `TUMBLE_SPEED 130`, `CARTWHEEL_SPEED 180`, `TUMBLE_TICKS 30`, `CARTWHEEL_TICKS 37`, `CARTWHEEL_RECOVER_TICKS 62` | `:81-85` | comment says "speeds are guesses, unconfirmed"; times from base-file clips |
| `LAND_TIME`, `HEAVY_LAND_TIME` | `:66-69` | pc00a clip lengths, applied to everyone |
| `JUMP_UP_CLIP_SECONDS 25/30`, `JUMP_DOWN 10/30` | `main.rs:125-126` | pc00a clip lengths; "approximation" |
| `BODY_RADIUS 4.5` vs program 5 u (PROGRAM-RE line 523); `BODY_HEIGHT 20` vs character height 18-20 | `:86-87` | DIFFERS slightly; affects how walls clip a dodge and whether a jump clears a projectile |
| `CAM_TURN_SPEED 2.6` | `:74` | not a jump rule, part of feel |
| `FEET_TO_UNITS 3.66` | `special.rs:116` | used for Jump / Dash / hover heights (30 ft = 109.8 u) though the program read says 100 u / 100 u |
| `main.rs` test dodge timings (`tick + 50`, `40`, `30`) | `main.rs:1095-1107` | demo script only |
| NEXT-SESSION / HANDOFF "jump height (apex ~0.75 m today) with a calibration choice" | docs | old open question; the program read (apex 14 u) closes it, never updated there |

## 6. Units: 30 Hz program tick vs our 60 Hz

- All jump / dodge physics are carried over as u/s and u/s2 (`:17-23`), which is correct. Discrete effects differ: apex and airtime (sub-unit, sub-tick), 1-tick landing-timer slip, and `AIR_CONTROL` which is applied per **60 Hz** tick (a 0.6 blend each tick is about 0.84 per 30 Hz tick, so the decay is not the same in either unit; it has no program source anyway).
- Locked timers: dodge state 3 = 40 program ticks = 80 of ours; landing 11 program ticks = 22 of ours. Ours store landing in seconds and dodge in (60 Hz) ticks; check the program ticks are doubled everywhere (`TUMBLE_TICKS 30` is 0.5 s, not 30 program ticks).
- Program step list says the scripted dodge moves 1.9 u per 30 Hz tick for 21 ticks; ours has no 21-tick concept.
- Cast / aura timers elsewhere count 60 Hz ticks; check any dodge-related number pulled from the tables (w41 / w42, clip length ints) is x2.

## 7. Contradictions between docs and code (or between docs)

1. **FOOTAGE.md "Jump" cites `JUMP_VELOCITY` 75 u/s, apex about 9 u, 0.5 s** as "confirmed"; the code is 85 / 259.2 (apex 14 u, 0.66 s) and PROGRAM-RE line 553 says the footage airtime (0.43-0.5 s) is shorter than the 0.66 s read, "judged by eye" (and round-2 in PROGRAM-RE line 647: 13-15 frames). Either re-measure or accept the program read; both appear as confirmed in different docs.
2. **HANDOFF "Jump is a per-character dodge" / "tumble has no recovery, cartwheel faster with recovery"** (fan guide wording) vs PROGRAM-RE (one move for both, 40 ticks). The code follows the fan guide. PROGRAM-RE itself notes the program treats them the same.
3. **GAME.md "Known gap" says the landing is M11's**; PROGRAM-RE's status line (line 493) says step 4 (movement, jump, dodges) is "done". It means read, not built. Do not read "done" in PROGRAM-RE as code.
4. **`sim.rs:49-53`** says the jump dodge is "not the ordinary jump -- no such skill is built yet"; `special.rs` does build skill 239 Jump (with different numbers).
5. **Jump-start clip**: GAME.md says `JUMP_UP`/`JUMP_DOWN` are used for the dodge; PROGRAM-RE says the dodge uses DASH_A/DASHEND_A/DASH_B/DASHEND_B.
6. Dodge recovery times come from the base `pc00a` file (`sim.rs:78-79`); PROGRAM-RE line 568 (2026-09-27) says the colour files differ for all five ground dodgers (DASH_A 40, stubs). The comment in `sim.rs` predates it.

## 8. Suggested reading order on the PC

1. `FUN_140270200` s44 = 8 and 9 (dodge bodies): direction, speed profile, state timer, character dependence, any cost or invulnerability.
2. `FUN_140254c80` (landing) with jump kind 5: DASHEND_B always, tick count, boundary compares.
3. `FUN_14025b0f0` (airborne): air speed, heading, no-stick behaviour, velocity blend.
4. `FUN_140254590` (physics) and collision floor-follow: step height, grounded rule, gravity order.
5. `FUN_1402517c0` (button): press vs hold, buffering, state list.
6. Footage to settle what Ghidra cannot: Laurence films a tumble (JD) and cartwheel (Edgar) from the side with something of known size in frame; a jump with no stick and with full stick (air carry).

## 9. Likely "feels wrong" list for Laurence (ranked)

1. Ground dodges: wrong duration (tumble too short, cartwheel gets a recovery the program does not have), wrong distance and speed shape.
2. Jump dodge: no end lag (11 ticks), so it returns control about 0.35 s early.
3. Air steering: walk speed when the stick is light, blended velocity, momentum dying when the stick is released.
4. Take-off / landing have no root-motion drift.
5. Coyote time lets you jump slightly after leaving a ledge; original probably falls at once (unread).
6. Pad dead zone (0.2 in `main.rs`) vs program 0.4 plus rescale, changes when a dodge picks a direction and when the air gait is walk.
