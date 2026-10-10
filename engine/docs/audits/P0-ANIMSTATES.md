# P0 audit: ANIMSTATES (fighter states outside ordinary/locked movement)

2026-09-30. Read-only audit, deep pass P3 "character side". Scope: states 2-7 and 9-20 of the fighter dispatcher `0x140256e80`, plus the casting/facing/charge/recovery/reaction/knock-down/get-up/landing/dodge/death paths that feed them. States 0, 1, 8, 21, 22, 29 (idle, walk/run, strafe pivot) are the approved locked-on lane (MOVEMENT-AUDIT.md "Locked-on lane") and were not reopened.

Method (METHOD-FIDELITY.md): every handler was read, not one function. Ghidra's export loses the float/stack arguments of the clip-start helpers (`FUN_140257310` / `FUN_1402573e0`), which is exactly where blend and start-frame live, so **every clip-start call below was re-read from the instructions** (capstone run over the bytes of `PDUWP.exe`, installed into a temp folder, nothing added to the repo; no Ghidra). Nothing was run or built.

Helper signatures (read from the callee bodies, `0x140257310` / `0x1402573e0`): clip start = (controller, clip, **loop**, **seek frame** [`0x1402573e0` only] or **blend frames** [`0x140257310` 4th arg], **root-motion flag**, hold-loop start `+0xd4`, hold-loop end `+0xd8`, and for `0x1402573e0` a stack arg **blend frames**). Blend <= 0 means full weight at once (no cross-fade). Clip ids = the 136-name table at `0x1402db400` (our `ANIM_NAMES`).

## 0. The five things to know

1. **Blend: the program cuts, we cross-fade.** A player's cast clip starts with blend **0** (the blend is a per-character float, charaset record `+0x16` -> in-memory `+0x2c`, read at `0x14025a692`; it is 0 for all 11 playable fighters, 5 for enemies). Every hit reaction clip (except the 1-frame flinch), landing, get-up, ground dodge and jump take-off also starts with blend <= 0. Ours blends **everything** 5 frames except the locked-on pivot hand-off (`main.rs:1839-1842`).
2. **Cast facing is a rate-limited turn toward an aim captured at the press.** Ours snaps to the opponent in the sim, three times (`combat.rs:1420`, `:1529`, `:1641`), ignores lock-on and the camera, then eases on screen.
3. **Casting is only allowed from certain states; ours allows it from any "Ready" action**, so a cast can cancel a dodge (`sim.rs:429-433`), start during a landing lock, or happen in the air with a ground-only skill. Mid-air casts **hover** in the program (state 11 has no gravity); ours falls.
4. **Reactions are state machines with clip-length timing; ours are a flat 0.5 s stagger** except the flinch and the knock-down. About 42 attack-skill slots (heavy drain / spike / raise hits) use the "clip, then lie, then get up" kind (~3.7 s helpless); ours frees the victim after 0.5 s.
5. **Missing whole feature sets** (grep-confirmed absent in `pd_game/src`): root motion, the wounded/fast/slow/confused clip sets, the paralysed struggle clip, bind clips, wall hits, FLOAT clips.

## 1a. The program's state table (states 2-20, plus KO), vs ours

Units: 1 program tick = 1/30 s = 2 sim ticks. "len" = clip length in frames (the per-character int at controller `+0x114+16*clip`; our `clip_ticks_from_glb` measures the same numbers: ATK_LASER 34, DMG_SHOT_FRONT 20, DMG_SPHERE_FRONT 30, BLOW_LAND_SF/MF/LF 22/40/56, DOWN_*_STAND 30, JUMP_LAND1/0 13/30, DASHEND_B 12). Timer semantics: handler exits when timer < 1, decrement after the test, so lock = len + 1 ticks press-to-free; ours `action_ticks = len x 2` is within one sim tick of that (fine, not a finding).

| St | Program: entry -> exit; clip / blend / loop / root; facing; timing | Ours | Verdict |
|---|---|---|---|
| 2 | Unused forward slide (`0x14025af60`, 2.59 u/tick) -> 9. No setter found. | none | n/a |
| 3 | **Ground dodge 751 / Dash skill 240**: edge press of the dodge button, only in states 0/1/8 (`0x1402517c0`; pivot snaps facing := heading first). `0x140270200` s44 = 8: clip row.w38 (DASH_A 33; Dash 77) via `0x1402573e0`: **loop 0, blend 0, seek frame row.w43 (0), root 1**. Timer = clip len - w43 (DASH_A is 40 frames for the five 751 fighters). Each tick (`0x14025afe0`) moves `s45*10/(w42-w41)` u along the facing matrix for w42-w41 = 21 ticks, plus the clip's own root motion. Facing is not turned. End -> idle clip, blend 5. | `Dodge::Tumble/Cartwheel`, different speeds/lengths/recovery (`sim.rs:103-107, 737-760`); direction = stick or camera-forward; clips DASHEND_A/DASH_A with 5-frame blend; no root motion | DIFFERS (known, AUDIT-JUMPS-DODGES 3; new: blend 0, direction = facing) |
| 4 | Jump wind-up (`0x140257b90` from `0x140270200` s44 0/9): start clip = kind row [0] (JUMP_START 37 / DASH_A 33), **loop 0, blend 5, root 1**; timer = len - 2; then `0x14025a0b0`. | none (velocity set on the press tick, `sim.rs:766-771`) | DIFFERS (small for the dodge jump: DASH_A is 2-3 frames) |
| 5 | Airborne. Take-off (`0x14025a0b0`): up clip [1] **blend 0, root 0**, stretched to v/g; at vy <= 0 (`0x14025a1c0`): down clip [2] **blend 5** (-1 if state 2), root 0, stretched; state timer v/g. Facing turns toward the stick at the turn rate, moves at run speed x turn factor (`0x14025b0f0`). Floating status -> FLOAT_IN (42, blend 5) -> state 7 (`0x14025a330`). | `Motion::Rise/Fall`, JUMP_UP/DOWN stretched (`main.rs:1867-1874`), both with the 5-frame blend; air control per AUDIT-JUMPS-DODGES | DIFFERS (blend at take-off 5 vs 0; rest = existing audit) |
| 6 | Landing (`0x140254c80`): fall height 40 / 80 u picks kind-row entry [3 / 4 / 5]; a clip -> **loop 0, blend 0, root 1**, timer len - 1; else straight to idle (clip 0 started with blend 5). **Jump kind 0 (before any landing) = never a landing clip; after the first landing the kind is reset to 6** (`0x140254c80` end, `+0x12f34 = 6`), so walk-off falls use the 40/80 rule. Exit -> idle, blend 5. | `land_timer` 12 / 29 of 30 Hz frames (`sim.rs:77-80`, `:830-837`), clip with 5-frame blend | MATCH (rule), DIFFERS (blend, root, dodge-jump landing, see AUDIT-JUMPS-DODGES 2) |
| 7 | Floating (Skywalk/Levitate): FLOAT_WAIT/MOVE (43/44), button toggles off (`0x14025b170`). | hover teleports to a height; no FLOAT clips | DIFFERS (known) |
| 9 | Short recovery (only reached from the unused state 2). | none | n/a |
| 10 / 11 | **Cast, ground / air** (`0x14025a3e0` via `0x140258180`, per tick `0x14025bd40`). Clip row.w38 (row w1 -> root 1) via `0x1402573e0`: **loop 0, seek frame = combo offset w48 (0), blend = per-character float (0 for players)**. State timer = clip len - w48, **frozen while a charge is held**. Fire = countdown w43 checked before the dispatch (`0x14025c720`). Mid-air (state 11): vy := 0 and **no gravity, no motion** until the state ends (`0x140254590`: state 0xb skips the vertical step). Facing: **turns toward the aim yaw `+0x88` at the character turn rate (20 deg/tick) on every tick of the state, including after fire**. Exit: timer < 1 -> state 7 (floating) or 0; idle clip starts when the cast clip ends (blend 5, `0x140257510`). | `Combat::cast` -> `Action::Cast`, `action_ticks = clip len`, fire timer from the row (tests `a_hit_before_the_fire_tick_cancels_the_cast`); yaw snapped; clip starts with 5-frame blend; gravity and AIR_CONTROL keep acting in the air | fire tick, lock length MATCH; blend, facing, hover DIFFER |
| 12 | **Short-range cast + dash** (`0x14025be40`): same clip start. Wind-up counts down `[0x26]` ticks turning toward the target's bearing (re-read each tick) at the char turn rate; dash ticks `[0x27]`: body turns toward the target by at most `+0xac` (= s66/10 deg: 1 deg/tick swords, 3 deg/tick punches) and moves along the **body facing**, stops 11 u short; afterwards turns toward the last aim at the char rate. | `lunge()` sets yaw = exact bearing every tick and steps straight at the target (`combat.rs:1641-1652`) | DIFFERS (dash homing is perfect; original steers slowly) |
| 13 | Automatic Shelter's own short state (`0x1402626d0`). | guard shown as a cast (`auto_guard`) | not audited |
| 14 | **Reaction hold** (`0x14025c1a0`): timer = clip len (kinds 0/2/3/10), or w42 ticks (kind 11, 5-tick flinch). Exit: queued second reaction (s60) -> `0x140258690` again; `0xc` -> state 18 with 30; else idle (blend 5) if grounded. | `Action::Hit`, `action_ticks = 30` sim ticks for every non-flinch row (`combat.rs:1929-1934`, `HIT_STUN_TICKS`) | DIFFERS (AUDIT-COMBAT-CORE 1.8): program 20 frames (DMG_SHOT) / 30 frames (SPHERE, SORD, DRAIN_LOW, SPIKE_LOW, RAISE_LOW) = 40 / 60 sim ticks |
| 15 | **Kind 1 (clip, then lie)**: rows 9, 11, 13, 25 (drain / spike / raise HIGH): clip w41 (**blend -1, loop 0**, root w1), queued clip w42 (DOWN_BACK/FRONT, loop 1); state 15 timer = clip len (DMG_DRAIN_HIGH = 50 frames); exit -> 18 with timer 30 -> get-up. While in 15/17/18 only s62 bit 2/4 attacks can touch (`0x14026f870`). | plain stagger of 30 sim ticks showing the first clip only | DIFFERS (big) |
| 16 | **Bind** (kind 8, rows 20, 22, 23; arrive as s60): clip w41 with hold-loop frames w43..w44, timer w42 + w43 + 1 -> 14 (clip plays out). | bound = `status.bound_ticks`, idle pose, no clip | DIFFERS (not built) |
| 17 | **Knocked flying / sliding** (`0x14025c260`). Start (`0x140258690` kinds 4/5/6): clip (BLOW0/1/2_FRONT 45-47 or BACK 48-49) **blend -1, loop 1, root 0**; **facing := bearing to attacker (front set) or bearing + 180 deg (back set), slide direction = away from attacker**; launch w39/10.8 u/tick at w40 deg; gravity 0.12; horizontal speed constant in flight. On landing (`0x140254c80`): dust by fall height, landing clip **BLOW_LAND_{S,M,L}{F,B} by fall height class (<40 / 40-80 / >80)**, blend 0, root 1, then the queued DOWN_FRONT/BACK loop (flags in the charaset: `[1,1,0|1,1,0]` for every fighter, facing +180 deg only for LB). Slide x0.85 per **program** tick; the state's counter runs only while grounded and ends the state at **60 ticks** (or 90 ticks stuck in the air). | `Action::Knocked`, physics flight, slide x0.85 per **sim** tick (`combat.rs:1198-1201`), no body turn, landing clip by **launch speed** (`lying_clip`, MF/MB for ordinary hits) | DIFFERS (3 items below) |
| 18 | **Lying** (`0x14025c3d0`): counter 0 (from 17) -> immediately: alive -> get-up clip (DOWN_FRONT_STAND 61 / BACK_STAND 63 by the current lying clip), **blend 0, loop 0, root 1**, protect/blink timer := clip len + 45, state 19 timer len - 1; dead -> state 27. | `Floored` 120 sim ticks then `GetUp` 60 (`combat.rs:66-68`) | MATCH (durations), DIFFERS (blend 5, landing clip) |
| 19 | Getting up: countdown -> idle (blend 5). | `Motion::GetUp` | MATCH |
| 20 | **Paralysed struggle** (status 4 + stick, from states 0/1/8): DMG_PARARISE (116) **blend -1, loop 0, root 1**, facing := camera + stick instantly, timer = clip len, reaction row 42 sound; repeats per push. | nothing shown | DIFFERS (not built) |
| 27 | KO: state 18 with the death flag (`FUN_14024e000` clears `+0x24`) -> 27 for 90 ticks. A fighter who is idle/recovering when dead plays reaction row 25 (DMG_DRAIN_HIGH then DOWN_BACK) from `0x14024ec80`. The killing blow otherwise plays its normal reaction (launch included). | `hurt()` -> `Action::Down` at once, no launch, pose `BLOW0_FRONT` (`main.rs:290`); round end 180 sim ticks | DIFFERS (presentation) |
| 30 / 31 | Warp out / in (`FUN_1402501d0`). | none | not audited |
| fall-out | `0x140251d80`: no reaction; moved to the start point, facing and camera := start yaw, vy 0, health -3 silently (`0x14024d7a0`, no damage accumulator), idle clip (blend 5), **90-tick protect/blink timer (`+0x6434 = 0x5a`)**. | `fall_out` -> `hurt(3)` -> a stagger with whatever `reaction_row` was last set, no protection, yaw kept (`combat.rs:1965`, `sim.rs:808`) | DIFFERS |

## 1b. Ledger

Legend: ✅ yes, ❌ no, ~ partial, ? not observed in this read-only audit ("Reproduced" is `?` wherever nothing was run by me; earlier docs/tests are named where they exist). "Approved" = Laurence, per DECISIONS/HANDOFF.

| Feature | Mapped | Implemented | Reproduced | Verified | Approved | Evidence |
|---|---|---|---|---|---|---|
| Dispatcher: states, handlers, clip-start args (this audit) | ✅ all of 2-20, 27; 21/22/29/25/26/28/30/31 by setter only | ~ (no state object; `Motion` x `Action` pair) | ? | ❌ | ❌ | `0x140256e80`; helper reads above |
| Cast clip = action row w38 | ✅ | ✅ `main.rs:308-316` | ✅ footage (FORMATS-ANIMATION "Cast clips") | ✅ test `a_cast_plays_its_action_rows_clip` | ❌ | `0x14025a3e0` |
| Cast clip start: blend 0 (players), seek w48, root w1 | ✅ (this audit) | ❌ 5-frame blend, no root | ? | ❌ | ❌ | `0x14025a692-0x14025a737`; charaset rec +0x16 = 0 for rows 0-10 |
| Fire tick from row w43; hit cancels unfired cast | ✅ | ✅ `combat.rs:1243-1255, 1417` | ? | ✅ `a_hit_before_the_fire_tick_cancels_the_cast` | ❌ | `0x14025c720`, `0x140258690` |
| Whole clip holds the caster (timer = len) | ✅ | ✅ `combat.rs:1461` | ? | ~ | ❌ | `0x14025bd40` |
| Cast permission by state / s40 | ✅ (this audit) | ❌ | n/a | ❌ | ❌ | `0x14024fce0` (bitmask `0x1a3`), `0x1402517c0` |
| Aim captured at press (lock target / camera / facing), turn 20 deg/tick | ✅ (this audit) | ❌ snaps to opponent | ? | ❌ | ❌ | `0x140258180`, `0x14025bd40` |
| Mid-air cast hovers | ✅ (this audit) | ❌ | ? | ❌ | ❌ | `0x140254590` state 0xb, `0x14025a3e0` (`+0x12e84 = 0`) |
| Charge: hold pose loops frames w39..w40; wind-up w39 ticks; release restarts at w40 | ✅ | ~ release seek only (`main.rs:1851-1853`); hold plays the whole clip | ? | ~ `holding_a_charge_skill_spends_aura_into_its_strength` | ❌ | `0x14025a6a4-0x14025a72a`, `0x140257510` hold branch, `0x14025c720` |
| Short-range dash (window, 80 u gate, 11 u stop) | ✅ | ✅ | ? | ~ | ❌ | `0x14025be40`, PROGRAM-RE "Motion 13" |
| Short-range dash steering 1-3 deg/tick along body facing | ✅ | ❌ | ? | ❌ | ❌ | `0x14025be40` (`+0xac`) |
| Reaction pick (accumulator < 3 / 3-4 / >= 5; air -> row 27) | ✅ | ✅ `combat.rs:1841-1855` | ? | ✅ | ❌ | `0x14024fd80` |
| Hit direction: 4-way 45 deg sectors; 2-way front = +-90 deg; body turn on knock-down | ✅ | ~ 4-way ok, 2-way uses Right = front / Left = back, no turn | ? | ~ `stagger_direction_matches_where_the_attack_came_from` | ❌ | `0x140258690` kinds 2/3/5 |
| Reaction clip starts: blend -1, loop 0 (flyer loops), root w1 | ✅ (this audit) | ❌ 5-frame blend | ? | ❌ | ❌ | `0x140258890-0x140258df0` |
| Stagger length = clip length | ✅ | ❌ flat 30 sim ticks | ? | ❌ | ❌ | `0x140258690` kinds 0/2/3 |
| Flinch kind 11 (5 ticks, blend 5, loop) | ✅ | ✅ | ? | ~ | ❌ | row 46 |
| Kind 1 clip-then-lie (rows 9/11/13/25) | ✅ | ❌ | ? | ❌ | ❌ | `0x140258690` kind 1, state 15 |
| Kinds 7/8/9 (pinned, bind, lift) | ✅ | ❌ (kind 7 ids only) | ? | ❌ | ❌ | `0x140258690`; MOVES.md "not built" |
| s60 second reaction chained after the first | ✅ | ~ replaces the first at once (`combat.rs:2071-2077`) | ? | ❌ | ❌ | `0x14025c1a0` |
| Knock-down launch vector and gravity | ✅ | ✅ | ? | ✅ `knock_down_timeline_flight_lying_getup_then_invulnerable` | ❌ | `0x140258690` kind 5 |
| Knock-down landing clip by fall-height class; DOWN_* loop after | ✅ (this audit) | ❌ by launch speed, no DOWN loop | ? | ❌ | ❌ | `0x140259990`, `0x140254c80` state 17 |
| Knock-down timeline 60 ticks grounded, then 30-frame get-up, then 75-tick protect | ✅ | ✅ | ? | ✅ same test | ❌ | `0x14025c260`, `0x14025c3d0` |
| Slide x0.85 per 30 Hz tick | ✅ | ~ x0.85 per 60 Hz tick (half the slide) | ? | ❌ | ❌ | `0x14025c260`; AUDIT-COMBAT-CORE 2.3 |
| Wall hit while flying (WALLHIT clips, halve speed, <= 3) | ✅ (this audit) | ❌ (flight has no wall test) | ? | ❌ | ❌ | `0x140259b30` |
| Get-up clip: blend 0, root 1; protect timer len + 45 | ✅ | ~ clip yes, blend no; window yes | ? | ✅ window | ❌ | `0x14025c4c0` |
| Landing: 40/80 u rule, lock len - 1 | ✅ | ✅ | ✅ (footage) | ✅ `landing_choice_by_fall_height` | ❌ | `0x140254c80` |
| Landing clip blend 0 + root motion | ✅ | ❌ | ? | ❌ | ❌ | `0x140254c80` |
| Ground dodge / jump dodge | ✅ | ~ guessed numbers | ? | ❌ | ❌ | AUDIT-JUMPS-DODGES |
| Death: normal reaction then state 27; fall-out respawn without reaction | ✅ (this audit) | ❌ | ? | ❌ | ❌ | `0x14024e000`, `0x140251d80` |
| Root motion from clips (casts, reactions, landings, dodges, get-up) | ✅ | ❌ data exported, unused | n/a | ❌ | ❌ | `0x140257510`; `cache/chars/*.rootmotion.json` |
| Mood clip sets (fast > +49, slow < -49, confused status 35, wounded health < 6) incl. TURN_*DAM | ✅ | ❌ | n/a | ❌ | ❌ | `0x14025c980`, table `0x1403faed0` |
| Protect blink | ~ (rate read, decrement site not) | ~ 6-tick on/off (5 Hz) | ? | ❌ | ❌ | `0x14024f2f0` |
| Locked-on movement (states 0, 1, 8) | ✅ | ✅ | ✅ | ✅ | ✅ | DECISIONS 2026-09-30 |

## 2. Discrepancies (most important first)

**D1. Clip starts are instant cuts in the program for almost every combat state, cross-fades in ours. CONFIRMED.**
- Program: cast (players) blend 0 via per-character float (`0x14025a692` loads `[0x1404c020c + idx*0xa0]`; source = charaset record `+0x16` via the loader `0x14017d300` at `0x14017d5d3`; values: players 0, enemies 5). Reactions: blend -1 for kinds 0-10 (`0x140258897`, `0x1402588d6`, `0x140258946`, `0x1402589e1`, `0x140258aa9`, `0x140258b43`, `0x140258b95`, `0x140258c07`, `0x140258c3f`), kind 11 = blend w43 (5). Landing, get-up, ground dodge, jump take-off (up clip) blend 0. **Blend 5 only** where: jump start (`0x140257b90`), the fall (down) clip, every return-to-idle, the flinch.
- Ours: one rule, 5 frames, except the pivot/gait cases (`clip_switch`, `main.rs:166-173`, `:1839-1842`).
- Visible: the original snaps into the cast pose / recoil / lying pose on the first frame; ours smears 0.17 s of the previous pose in (the same mechanism that made the pivot spasm). Fix is data-driven (table of start args per state), no tuning.

**D2. Cast facing and aim. CONFIRMED (both sides read).**
- Program (`0x140258180`): aim yaw `+0x88` is captured once at the press. With a lock target (and, for short-range skills, target within 80 u = band 1): bearing to the target. Otherwise (unlocked, or short-range out of reach): **camera yaw** (+ camera pitch). Attack skills aimed this way; defence (s43 = 2): camera yaw, pitch 0; erase / special / status / environment (s43 = 1, 3, 4, 5), mines, Holes: **facing unchanged**. Each tick of state 10/11 the facing moves toward `+0x88` by at most the character turn rate (20 deg = 1820 units per program tick, read at `0x14025bd40`), so a 180 deg turn takes 9 ticks (0.3 s). The shot leaves along the facing of the fire tick (spawn matrix rebuilt each tick from `+0x12e68`, `0x1402514c0`).
- Ours: `cast()`, `fire()` and every `lunge()` tick set `me.yaw = yaw_of(toward(me.pos, opponent))` regardless of lock, camera or skill kind (`combat.rs:1420, 1529, 1641`); `Motion::Cast/Lunge/Hit/...` then ease on screen with `k = 1 - exp(-14 dt)` (`main.rs:1448-1463`) - the anti-pattern METHOD-FIDELITY names.
- Visible: unlocked casts in the original fire where the camera looks; ours always aims at the foe. Self-buffs/erase/status casts turn the body toward the foe in ours, not in the original. Shot direction matches the drawn body in the original (turn in progress) but not in ours.

**D3. What may start a cast, and what a cast may interrupt. CONFIRMED (code read; program `0x14024fce0`).**
- Program: a press starts a cast only in states {0, 1, 8}; skill field s40: **0 = ground states only, 1 = air states (5, 7) only, 2 = {0, 1, 5, 7, 8}, 3 = same or after a landing (`+0x12f34 == 6`)**. Chains from states 10-12 use the combo window. Not allowed in 3 (dodge), 4, 6 (landing), 9, 14-20. The button is not buffered (except combos, `0x14024fb90`).
- Ours: `tick_fighter` accepts a press whenever `action == Ready` (`combat.rs:1225-1270`); `combat.rs` never reads `players[i].motion`, `dodge_ticks`, `land_timer`, `recover_ticks` or s40 (only the row pick, `combat.rs:888-896`). Consequences: a press during a ground dodge starts a cast and then `rooted()` zeroes `dodge_ticks` (`sim.rs:429-433`) = dodge cancelled; casting is possible mid-landing-lock and cartwheel recovery; a ground-only skill can be cast in the air (and uses the ground row).
- Visible: input feel around dodges/landings; interrupt rules the original never allows.

**D4. Mid-air casts hover. CONFIRMED.**
- Program: state 11 sets vy = 0 at the start (`0x14025a3e0`: `+0x12e84 = r15d = 0`) and `0x140254590` skips the vertical step while the state is 0xb; the cast clip (ATK_SKYSHOT_LOW/HIGH) plays with the body suspended for its whole length, then the body falls (state 0 -> `0x140257c70` -> state 5 fall clip).
- Ours: gravity and `AIR_CONTROL` run in every tick (`sim.rs:772-778`), `rooted()` only zeroes the stick. The caster falls and lands mid-cast (clip forced by `Motion::Cast` over a descending body).

**D5. Charge pose and timing. CONFIRMED (pose), SUSPECTED (Aura timing).**
- Program: a charge skill's clip starts with hold-loop frames `+0xd4 = w39` (13) and `+0xd8 = w40` (15): it plays to frame 13 and then oscillates 13..15 (`0x140257510` hold branch) for as long as the button is held; release restarts the clip at frame w40. The state timer is frozen meanwhile (`0x14025bd40` tail). The charge counter starts after the w39-tick wind-up (`0x14025c720`), +1 per tick, cost check base + count/10.
- Ours: the clip plays from frame 0 to its end and then holds the last frame for the whole hold (`main.rs:1848-1853`: only WAIT0/WALK/RUN/DASH_A repeat); Aura is drained into the charge from the press, one point every 20 sim ticks (`combat.rs:1212-1219`, first point 26 sim ticks earlier than the program's first point at wind-up + 10 ticks; the payment point in the program, at release, is SUSPECTED from PROGRAM-RE "Charge, finished", `FUN_1402620e0` not read).
- Visible: Gungnir / Ki Cannon / Young White Dragon / Ki Palm holds show the whole wind-up animation instead of a held pose.

**D6. Knock-down presentation. CONFIRMED.**
- (a) Body turn: at launch the program sets facing := bearing to the attacker (front set) or bearing + 180 deg (back set) and the slide direction away from it (`0x140258dfd`-`0x140258f2d`); ours never touches yaw in `start_knockdown` (`combat.rs:1941-1962`), so a side or rear hit flies the body sideways in a pose meant for a front hit. The 2-way split is the front half (+-90 deg, test `iVar4 + 0x1fff < 0x3fff` at `0x140258a60`); ours maps Right to the front pose and Left to the back pose (`combat.rs:921`).
- (b) Landing clip: class by **fall height** (<40, 40-80, >80) = BLOW_LAND_S/M/L then the DOWN_FRONT/BACK loop (`0x140259990`); an ordinary hit (apex ~8 u) is **S** (22 frames). Ours picks by launch speed (`combat.rs:950`) = **M** (40 frames) for every ordinary hit, L only for launch >= 40 (Tornado, where the program also gives L: apex ~88 u).
- (c) Slide 0.85 per 60 Hz tick vs per 30 Hz tick (half the slide, ~3.4 u vs ~6.7 u; AUDIT-COMBAT-CORE 2.3).
- (d) No wall contact handling; the original bounces the flyer off walls up to 3 times with WALLHIT clips (`0x140259b30`); also the 90-tick stuck-in-air rule is absent (AUDIT-COMBAT-CORE 2.10).

**D7. Kind-1 reactions (clip, then lie, then get up) are flattened. CONFIRMED (both sides read).**
- Program: 42 attack-skill reaction slots use kind 1 (counted in `skills.json`: s57 x 1, s58 x 7, s59 x 34; rows 9 / 11 / 13): clip (DMG_DRAIN_HIGH 50 frames, DMG_SPIKE_HIGH, DMG_RAISE_HIGH) -> 30 ticks lying -> get-up 30 frames = ~110 ticks (3.7 s) helpless, untouchable by ordinary attacks the whole time (states 15/18/19).
- Ours: `Action::Hit` for 30 sim ticks (`combat.rs:1916-1922`, `reaction_stun_ticks`), first clip only, victim immediately hittable. Also s60 second reactions replace the first at once instead of chaining (`combat.rs:2071-2077`), and stagger lengths ignore clip length (D in AUDIT-COMBAT-CORE 1.8).

**D8. Root motion absent. CONFIRMED (grep: no `rootmotion` use in `pd_game/src`).** Every cast (row w1), reaction (row w1 except the flinch), landing, get-up and dodge moves the player by the clip's root x/z each tick (`0x140257510`; flag set by the clip start's 5th arg). Drift examples (pc00a): ATK_STAMP 42 u, ATK_KICK 38, ATK_TUKI 19-34, ATK_SORD 18, ATK_SHOT -11 (steps back), DMG_SPHERE_FRONT +18, BLOW_LAND_* +-17..22. `cache/chars/*.rootmotion.json` exists and is unused. Affects spacing after every move and the dash/lunge totals.

**D9. Wounded / speed / confused clip sets missing. CONFIRMED (absent in `pd_game/src`).** `0x14025c980` picks a mood every tick: status 1 > 49 -> fast (WALK_FAST 10 / RUN_FAST 15), < -49 -> slow (11 / 16), status 35 -> confused (WAIT_CON 5 / WALK_CON 8 / RUN_CON 13, no strafe), else Health < 6 -> wounded (WAIT_DAM 6 / WALK_DAM 9 / RUN_DAM 14 and TURN_WDAM_* / TURN_RDAM_* 25-32). Table `0x1403faed0`. Ours always plays WAIT0/WALK/RUN (`main.rs:261-292`, `strafe_clip`). Health threshold interacts with our START_HEALTH 20 (program initial health unknown): needs Laurence's footage (Q4).

**D10. Short-range dash steering. CONFIRMED.** Program turns the body toward the target at most s66/10 deg per tick during the dash (1 deg swords, 3 deg punches/kicks, 30 deg Pursuit Stopper) and moves along the body facing; ours re-aims perfectly every tick (`combat.rs:1641-1652`). Against a moving target the original lunge curves slowly and can miss.

**D11. Death and fall-out. CONFIRMED.** KO: ours `Action::Down` immediately, no launch, `BLOW0_FRONT` pose (`main.rs:290`); original plays the killing blow's normal reaction (or row 25 DRAIN_HIGH + DOWN_BACK), lies, never gets up, state 27 for 90 ticks. Fall-out: see table row; ours shows a stagger from a stale `reaction_row`, no protection, yaw kept.

**D12. Smaller. Mostly CONFIRMED.**
- Bound (status 4) players show idle; the original plays DMG_PARARISE per push and turns to the stick instantly (`0x14025b5e0`, `0x14025ad50`).
- Protect blink: program draws on even timer values only (`0x14024f2f0`: `(timer & 1) == 0` draw), i.e. flickers every 30 Hz tick (15 Hz); ours 6 sim ticks on / 6 off (`main.rs:1437, 1473`). SUSPECTED until the timer's decrement (`FUN_14024d2f0`) is read.
- Brush stun 45 sim ticks vs 30 frames = 60 (`moves/barrier.rs:59`, PROGRAM-RE row 26).
- `main.rs:1822` keeps a finished cast's clip "following through" while the player stands still: not in the program (state timer = clip length already covers it); harmless, but a render-side compensation.
- Dodge direction: program moves along the body facing (`FUN_14024f9c0` uses the facing matrix `+0x12e20`, rebuilt at the end of each tick by `0x1402512c0`, so also one tick stale); ours along the stick / camera forward. SUSPECTED to matter only in the first ticks after a stick change.

## 3. Reads still needed

1. `FUN_14024d2f0` and the writers of `+0x6434` (protect/blink timer): decrement rate, exact draw rule (`0x14024f2f0`), whether the lock-on "protected" flag (`0x140250180`, vtable slot 11) also hides the fighter from targeting.
2. `FUN_1402620e0` / `FUN_1402621f0`: when a charge pays (per tick vs at release), refund on interrupt. Decides the Aura part of D5.
3. Left/right sign of the 4-way sectors in `0x140258690` kinds 2/4: index 1 = +90 deg from facing. Confirm against DMG_SHOT_LEFT/RIGHT root-drift signs in `cache/chars/pc00a.rootmotion.json` and our `hit_direction` right vector (`combat.rs:255-269`) before mirroring anything.
4. Root-motion sampling at clip start (`0x140257510`, deltas `+0xf8..0x100`): is there a root impulse on the first tick, is the delta rotated by the pre- or post-turn facing.
5. Setters of states 25/26 (`0x14025c5e0`, camera clamp), 28 (emote, `0x1402526c0`), 30/31 (`0x1402501d0`), 13 (`0x1402626d0` auto-Shelter) - only located.
6. `FUN_14025ace0`, `FUN_1402598d0`, `FUN_140259200` (side effects of every state change: voices, effects, rumble).
7. Kind 9 (lift) and kind 8 (bind) handlers in full (`0x140258c95`, `0x140258b87`), for rows 20-23, 44, 47.
8. `+0x88`/`+0x8c` (aim pitch) into `FUN_1402905c0` and the projectile launch: whether pitch follows the camera for unlocked casts.
9. Health at round start in versus (gsdata `0x33044` block?) to decide the wounded threshold (Q4).
10. Whether state 1's move uses the previous tick's facing matrix (observed: `0x1402512c0` runs after the state handler) - belongs to the closed movement lane; noted only so nobody is surprised.

## 4. Proposed build lanes (ordered)

**Lane 1 - CAST-STATE (sim rules). Goal: D2, D3, D4, D5 (timing), D10.**
Owns: `combat.rs` `Combat::cast` (gating table from `0x14024fce0`, aim capture from `0x140258180`, no yaw snap), `tick_fighter` cast arm (turn toward the aim at 20 deg/tick, timer pause during hold), `fire`, `lunge` (dash steering), `release_charge`, `CHARGE_TICKS_PER_POINT`; `sim.rs` `Match::step` rooted block (dodge not cancelled by a cast; casts refused in dodge/landing/recovery) and the air-cast gravity hold in `step_player_at`; small new `Player`/`Fighter` fields (aim yaw, air-cast flag). Does NOT touch `main.rs`.
Tests from reads: gate truth table for s40 0-3 x states (mask `0x1a3`); facing advances <= 1820 units per program tick, 180 deg takes 9 ticks; aim source = lock bearing / camera yaw / unchanged by s43; air cast keeps y constant for clip-length ticks; charge: hold timer frozen, first count after w39 ticks; dash turn <= s66/10 deg per tick.
Runtime proof: `PD_TRACE_SCRIPT` / `PD_FACE_LOG`: (a) unlocked cast with the camera turned 90 deg, face log shows 5 ticks of turn; (b) cast pressed during a dodge is refused; (c) mid-air Sky shot y trace flat; before/after strips of P1's view.

**Lane 2 - ANIM-ARGS (presentation). Goal: D1, D5 (pose), D9, D12 (blink, follow-through), and the on-screen ease of D2.**
Owns: `main.rs` `drive_animation`, `clip_switch` / `ClipSwitch`, `clip_for`, `faces_sim_exactly`, `model_rotation`, `sync_visuals` (blink), plus a new table module (e.g. `anim_start.rs`) holding per-state `{blend, loop, root, seek, hold-loop}` exactly as in section 1a, and the mood selector (reads health / speed status from `Combat`, read-only). After Lane 1 lands, the model draws the sim facing exactly in combat states (delete the exponential ease). Does NOT touch `combat.rs`/`sim.rs` (reads `Fighter`/`Player` only).
Tests from reads: `clip_switch` table (cast/reaction/landing/get-up/dodge/take-off = Instant; fall, idle return, jump start = 5 frames; flinch = 5), charge hold loop 13..15 then seek 15, mood table rows of `0x1403faed0`.
Runtime proof: `PD_ANIM_LOG` first-frame strips (cast start, hit start, land start) old vs new; wounded clip sheet (low Health via test loadout).

**Lane 3 - REACTIONS (sim + loader). Goal: D6, D7, D11, D12 (bind/struggle, brush stun) and the hit-direction rules.**
Owns: `combat.rs` `hit_direction` + `reaction_clip` (2-way = +-90 deg), `hurt`, `reaction_stun_ticks` (clip-length holds from `clip_hold_ticks`), a reaction sub-state for kinds 1/7/8/9 (state 15/16 equivalents: clip, lie 30, get-up), `start_knockdown` (body turn, slide per 30 Hz tick, fall-height class recorded during flight), `step_knockdown_flight` (wall contact + 90-tick stuck rule when scoped), `lying_clip` / `getup_clip`, `fall_out` and the KO path in `hurt`, `attack_extra` s60 chaining; `sim.rs` fall-respawn block (`:806-811`, yaw := start yaw); `main.rs` `load_reaction_rows` only if a column is missing. Must land after Lane 1 (same file, separate functions) and before Lane 2's final table pass for reaction clips.
Tests from reads: kind-1 timeline = clip len + 30 + get-up len; front/back split at +-90 deg; body facing after launch = attacker bearing (+180 back); BLOW_LAND class from fall height 8 / 60 / 90 u; fall-out leaves no reaction, protect 180 sim ticks, yaw = start yaw; KO plays the reaction then stays down 180 sim ticks.
Runtime proof: frame-exact side view of a Blaster hit and a Fire Storm/Crawler heavy hit (PD_SHOT_TICKS), victim timeline log (`action`, clip, y) old vs new, compared with the row numbers above.

Deferred (needs Laurence, Q3/Q5): **root motion** as a Lane 4 after 1-3 (sim must consume per-clip root curves deterministically; clip-start flag table from Lane 2).

## 5. Questions only Laurence can answer

1. Casting with the free camera (not locked on): where should your fighter turn and aim?
   A. Toward where the camera looks (the original; recommended). B. Always toward the opponent (today).
2. Heavy hits from Fall / Rain / Crawler / Drain skills: in the original the victim collapses and is helpless about 3.7 seconds (down, then getting up), and cannot be hit by normal attacks meanwhile.
   A. Copy the original (recommended). B. Shorter, about 1.5 s. C. Keep today's half second.
3. Casting in the air (Sky Shot style): in the original the caster hangs in the air for the whole cast, then falls.
   A. Copy the original (recommended). B. Keep falling while casting.
4. From your recordings: does a fighter visibly limp or droop (low-health idle/walk/run) when nearly knocked out, and at roughly what Health bar level does it start?
   A. Yes, at about 5 or less. B. Yes, at a different level (tell me). C. No / cannot tell. (The program switches clip sets at Health below 6; our starting Health of 20 is our own choice.)
5. Clips that move the body (kicks lunge ~38 units forward, the Blaster steps back ~11, recoils and get-ups drift): adopt them so spacing matches the original?
   A. Yes, after the state work above (recommended). B. Not now.
