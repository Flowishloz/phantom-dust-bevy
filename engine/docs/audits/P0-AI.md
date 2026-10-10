# P0 audit: the original's computer opponent (lane `aiaudit`, 2026-10-01)

Read-only. Evidence: `%USERPROFILE%\.pdtools\re-out\decomp_all.c` (image base `0x140000000`), capstone
disassembly of the exe for functions Ghidra left undefined (a scratch venv, nothing committed), the
shipped data under `game_files\Assets\Data\com\` and `...\questdata\`, and our code at main `fc28225`.
**No live trace of the CPU exists** and nothing was measured from footage, so every value below is
READ (program address or data file given) or UNCONFIRMED. Tags: READ / MEASURED / LIVE / UNCONFIRMED.

**Update 2026-10-01 (lane `cpureads`, reads A of section 6):** every read of section 5 is closed or reduced to a named UNCONFIRMED
item; the byte-level results are in `engine/docs/FORMATS-CPU.md` (script VM, native table, `.cad`, `.epa`, steering, threat predictor,
seat structs) with tools `engine/re/ssb_dis.py`, `ssb_run.py`, `cad_dump.py`, `threat_predict.py`. Where this file's older text is
superseded it is corrected in place and marked "(cpureads)".

## 0. The answer in ten lines

1. **The CPU is a bytecode script, not engine code.** Every CPU fighter owns a small VM that runs
   `Data/com/ai/<name>.ssb` once per program tick (30 Hz, budget 50,000 instructions): `FUN_14017b220`
   (tick), `FUN_1402aaa30` (the VM), `FUN_14017b350` (loads `com\ai\<script>` and `com\deck\<deck>`).
2. **One script is the real opponent: `eneprog006.ssb`** (28 KB, 86 functions). All 331 regular quest
   enemies and all 30 partners name it; `eneprog000-005` (+ `_nodef`/`_noNdef` copies) are never referenced
   by any data file or program string. Bosses (`bossprog00` x5, `eneprog_freya` x1) are story-only.
3. **Difficulty = one row of `Data/com/param/enemyparam.epa`** (82 rows x 10 bytes, header u32 = 82),
   chosen per enemy by the quest record. Nine numbers per row become script registers v30..v38:
   how it picks a target, its think interval (30 ticks), and five probabilities (attack / react to a
   bullet / react to melee / dodge / wander). There is no difficulty menu.
4. **The script is a mode machine** (register v0): 0 init, 1 decide, 2 attack, 3 environment/mine,
   4 erase, 5 bullet-defence, 6 melee-defence, 7 status/ally, 8 collect own capsules, 9 collect other
   capsules. Movement is a second, independent "style" register (v1), 12 styles. Skill choice and spacing
   come from a per-skill **AI type = skill field s42** plus range bands (tables in appendix B).
5. **It writes the human input block**: pressed-button mask `P+0x14264`, stick magnitude `P+0x14268`
   (always 1.0 or 0) and stick angle `P+0x14270` (yaw units, absolute: camera yaw is forced to 0). It never strafes
   (CONFIRMED, cpureads: the movement code's lock branches need `P+0x30 != 0`), always pushes the stick fully (so it always
   **runs**: the walk gait needs magnitude <= 0.7), and steers on a **nav graph file per stage**
   (`com\stNN\comdata*.cad`, 674,312 bytes, format in FORMATS-CPU 5).
6. **Lock-on**: the AI is permanently "locked" on the target it picks (`ai+0xc`); the engine copies the
   target and the 80 / 300 u distance band into the same lock struct humans use (`P+0x13f1c/0x13f20`).
7. **RNG**: MSVC `rand()` (15-bit, time-seeded, shared with all other game systems). Not reproducible.
8. **The shipped versus menus have no CPU seat** ("Start a battle using split screen", PLAYER 1 / PLAYER 2).
   CPUs exist only in quests/Lab. Our `--demo` is a test script and the scope decision (AGENTS.md:10,
   DECISIONS.md:98) says "no AI". The brain nevertheless plays the same rules as versus (Level, Aura, capsules,
   erase, status, environment), so it is the right opponent if the scope is reopened.
9. **Former unknowns, now read (cpureads):** nav-graph internals and the `.cad` format (FORMATS-CPU 5), the bullet-threat
   predictor (FORMATS-CPU 4.4, executable model `engine/re/threat_predict.py`), how the movement state consumes the stick
   angle (FORMATS-CPU 5.9). What remains is a short list of UNCONFIRMED details (FORMATS-CPU 7), each with the live trace it needs.
10. Everything is **data-driven from files we already have** (script, epa, decks, cad, quest records);
    a faithful port runs the original script on our own VM instead of re-writing its logic.

## 1. Ledger

Legend: ✅ done, ~ partial, ❌ none. "Mapped" = we know what the program does (address / file field).

| Feature | Mapped | Implemented | Reproduced | Verified | Approved | Evidence |
|---|---|---|---|---|---|---|
| CPU seat in versus (shipped game) | ~ no seat in menu text; creation path unread for versus | ❌ | ❌ | ❌ | ❌ (scope: "no AI", DECISIONS 2026-09-26) | `Menu/vsmenu_en.ssb`; `FUN_14024e790` (ctor with AI info `param_3+0x20`), `FUN_140129260` (quest setup), `FUN_140214770` (debug 1v1 vs `eneprog006`, no static caller) |
| AI host + per-tick entry | ✅ | ❌ | ❌ | ❌ | ❌ | `FUN_14024ec80` -> `FUN_1401775b0` -> `FUN_14017b220` -> VM `FUN_1402aaa30` |
| Script format + VM opcodes | ✅ (32 ops; format, relocation, hash, tick protocol: FORMATS-CPU 1-3) | ✅ Python reference `ssb_run.py` (test oracle, not game code) | ✅ synthetic op tests + a fake-world run of `eneprog006` | ❌ | ❌ | `FUN_1402aaa30`, `FUN_1402aa620`, `FUN_1402ab5d0`; `ssb_dis.py` decodes all 16 `com/ai` files by control flow |
| Native API (group 4, ~115 distinct calls in all scripts) | ✅ all 75 used by `eneprog006` read (cpureads), semantics in FORMATS-CPU 4 | ❌ | ❌ | ❌ | ❌ | table `0x1402ddd00`+4 (`0x14020d460..0x140211ef0`), FORMATS-CPU 4, appendix A |
| Difficulty rows (`enemyparam.epa`) | ✅ meaning of v30..v38 from script use | ❌ | ❌ | ❌ | ❌ | `FUN_14017b020/b100` (copy 9 bytes -> `ai+0x20ec..0x210c`), section 2.5 |
| Quest record -> row, deck, char, HP | ✅ (fields +0x30..+0x38) | ❌ | ❌ | ❌ | ❌ | `FUN_140176d40`, `FUN_14024e950`; `questdata.qdt` 337 records |
| Decision state machine (10 modes) | ✅ | ❌ | ❌ | ❌ | ❌ | `eneprog006.ssb` main @0x6b8c, mode functions in 2.3 |
| Target choice (7 rules) + 3-4 player teams | ✅ | ~ our lock picks first valid | ~ lock only (human) | ❌ | ✅ lock rules for humans | script fn 0x03ac; `combat/lock.rs:83 next_target` |
| Lock block fill for CPU | ✅ | ❌ | ❌ | ❌ | ❌ | `FUN_14017b220` tail (`+0x13f1c` band, `+0x13f20` target) |
| Skill choice (types 0, 5-40, 49-50, 60-62, 70-79, 90) | ✅ | ❌ | ❌ | ❌ | ❌ | script 0x554c / 0x5ddc / 0x4dec / 0x5348; appendix B |
| Range keeping (approach / hold / flee) | ✅ | ❌ | ❌ | ❌ | ❌ | script 0x517c / 0x2d78 |
| Bullet and melee defence, guard choice, dodge | ✅ rules; threat predictor ✅ (cpureads) | ❌ | ❌ | ❌ | ❌ | script 0x1a74..0x1d20, 0x2204, 0x249c, 0x2300; `FUN_14020f9a0`, FORMATS-CPU 4.4, `threat_predict.py` |
| Capsule collection (own / other / erase) | ~ rules read, ownership semantics unread | ❌ | ❌ | ❌ | ❌ | script 0x66b0 / 0x6850 / 0x6594, 0x0c84 |
| Steering: nav graph, stuck recovery | ✅ format + queries + steering (cpureads); hazards' objects, ring-planner tie-breaks UNCONFIRMED | ❌ | ❌ | ❌ | ❌ | FORMATS-CPU 5; `FUN_1401776c0`, `140177f60`, `140178470`, `140178ad0`, `cad_dump.py` |
| Pad output (buttons, magnitude, angle) | ✅ | ~ our `Input` has the same slots | ❌ | ❌ | ❌ | `FUN_140178ad0`, `FUN_14020deb0`; human twin `FUN_140256580/600/710` |
| Perception latency (pair table, 4-tick stagger) | ✅ | ❌ | ❌ | ❌ | ❌ | `FUN_14012a900` (1 pair/tick), script 0x1b30 |
| RNG | ✅ | n/a | n/a | n/a | n/a | `FUN_14020dcf0`, `FUN_140291f00` (`srand(GetTickCount64)`) |
| Our `--demo` | ✅ read | ✅ test script | ✅ | n/a | n/a | `main.rs:1347-1430`, `GAME.md:222` ("a test script, not an AI") |
| Per-stage nav data for our 19 versus arenas | ✅ file coverage | ❌ | ❌ | ❌ | ❌ | `FUN_140176420` (`com\st%02d`, stage mod 10) |

## 2. What the program does (mapped)

### 2.1 Files and how they are chosen

- **Script**: `Data/com/ai/<name>.ssb`, loaded by `FUN_14017b350` into a 0x8000-byte slice of the
  player's own arena (`P+0x12c50`). Name = `param_3+0x46` (setup struct, `FUN_14017b020`) or
  `<name>.ssb` from the quest record at `+0x1a` (`FUN_14017b100`). A script is only loaded if the player
  entry carries an "AI info" struct (`param_3+0x20`, else `*COM = 0` and the seat is inert).
- **Deck**: `Data/com/deck/<name>` (405 files, most 100 bytes with 30 u16 skill ids and `ffff` = empty; the
  layout is not decoded here); loaded only when the name string is non-empty. Names seen in quests: `DECK_NATURE` x40, `DECK_PSYCHO` x22,
  `DECK_KI` x19, `DECK_OPTICAL` x8, `DECK_RELIGION`, `DECK_<CHARACTER>_<level>` and numbered `DECKnnn_k`.
- **Parameters**: `Data/com/param/enemyparam.epa` -> `DAT_1404c0188`. Row `id` at `base+4+id*10`:
  byte 0 = starting Health (20 for every row 0-81, used when the CPU seat is built through
  `FUN_14024e790`), bytes 1-9 = the nine AI parameters (copied to `ai+0x20ec..0x210c` by
  `FUN_14017b020/b100`, which are script registers **v30..v38**: register `i` lives at `ai+0x2074+4*i`).
- **Quest record** (58 bytes, `FUN_140176d40`/`FUN_14024e950`; 337 in `questdata.qdt`): +0 character
  (u16 + u16), +4 deck name, +0x1a script name, +0x30 team (0/1), +0x32 start-point index (0-11,
  `P+0x18`), **+0x34 parameter row**, +0x36 starting Health (5/10/15/20/30), +0x38 flag. Rows used: 0..81;
  most common 0 (33), 68 (13), 70 (12), 58 (11), 3 (11), 9 (11), 4 (12).
  Partners (`questpartner.qdt`, `FUN_14017b020` layout, row at +0x18) all use **row 0** and `eneprog006`.
- **Navigation graph**: `Data/com/st<NN>/comdata<V>.cad`, where NN = **stage code mod 10** and V = a variant
  number (`DAT_1404c51f0`); also `camdata.cam`. 674,312 bytes each (a fixed memory image) except `st01`
  (6,252, named `comdata.cad`, which `"comdata%d.cad"` never produces: story only). Folders `st02`-`st08` exist
  (variants 0/1 are byte-identical; only `st04/comdata2.cad` differs; an `st24` folder holds just `comdata2.cad`
  and is not reachable by the `% 10` rule). All 19 versus arenas have stage code mod 10 in 2..8, so all have nav
  data. A stage code mapping to a missing folder (9, 10, 19-21, ...) loads none and the AI tick is skipped
  (`FUN_14024ec80`: `DAT_1404c0180 != 0`).
- **Not used by shipped data**: `chkprog000`, `motchk`, `rootchk` (developer motion/ability checkers,
  referenced only by `com/single2.csv` debug rows; they read pad bits through g4.f225/f226).

### 2.2 The per-tick path (READ)

```
FUN_14024ec80 (player update, 30 Hz)             gate: pad slot active, P+0x20/+0x24 set, DAT_140533744 == 3 (battle running)
  P+0x30 != 0  -> human: FUN_140256580(P+0x14248)  pad -> block, dead zone 13,000 of 32,767 (scale 19,767), stick mag + angle (FUN_140256600/710)
  P+0x30 == 0  -> CPU:   P+0x12e5c = 0 (camera yaw base), P+0x14310 (dodge request) = 0,
                         if nav graph loaded: FUN_1401775b0(COM = P+0x142d0)
FUN_1401775b0: clear P+0x1424c..0x1427b (pad words), set P+0x1427c/80/84 = 1,2,3 (button remap), ai+0x38 = 1.0
  FUN_14017b220(ai = COM+0x138)   VM: jump to function "main" (name hash at 0x1402f0d0c), run <= 50,000 instructions
  while COM+0x10 == 0: FUN_1401776c0 (steer-target selection on the nav graph), FUN_140177f60 (local hazards, 472 lines, unread)
        (COM+0x10 := 1 when the body is airborne (P+0x12e74 == 0) or in state 4; := 0 when the state is 0, 1 or 6)
  FUN_140178ad0   steer target -> stick, or dodge button
```

- `ai = COM+0x138`; script registers `ai+0x2074`, vectors `ai+0x2144`; VM at `ai+0x20`; script pointer `ai+0x2230`.
  In `P` terms: register k = `P+0x1647c+4k` (v0 = `P+0x1647c`, v30 = `P+0x164f4`), target index `ai+0xc` = `P+0x14414`.
- **The script is re-entered every tick**; there is no persistent program counter. All memory of the AI is the
  48 integer registers and 20 vectors (zeroed by `FUN_14017afb0` at creation).
- `FUN_14017b220` end: if the target `ai+0xc` (< 4) is alive, `P+0x13f20 = target_stats+0x60` and
  `P+0x13f1c` = **1 if pair distance <= 80 u, 3 if > 300 u, else 2** (`0x1402f8040/0x1402f8138`; same band as
  humans, `PROGRAM-RE.md` "Lock-on"). The cast code (`FUN_140258180`) reads these exactly as for a locked human.
- **Output to the pad block** (`FUN_140178ad0`, `FUN_14020deb0`):
  - `PRESS(slot)` (g4.f48) ORs the slot's button bit into `P+0x14264` (the "pressed this tick" mask;
    human twin: block `[7]`). While status 0x24 is active the slot is remapped through the 4-entry table at
    `P+0x14278`. One press = one tick; the script presses again later if it wants more.
  - Stick: if the steer target `ai+0x50` is farther than **5.0 u** (`0x1402f7f18`) and the actor is
    steerable: `P+0x14268 = 1.0` and `P+0x14270 = atan2(dx, dz)` in 32,768-per-turn yaw units, **absolute**
    (camera yaw `P+0x12e5c` is 0 for CPUs). Closer: magnitude 0. So the CPU always pushes the stick fully (how a full push is
    split between walk and run for a CPU is unread). While airborne (`P+0x12e74 == 0`) it keeps pushing only if a ballistic estimate says it will reach the
    target's height in time, else lets go.
  - Dodge: `path49(deg)` sets `COM+0x40=1`, `COM+0x44 = deg x 91.02`; at the end of the tick, if the actor is in
    state 0/1/8, `FUN_140178ad0` sets button `0x1000000` (the dodge, charaset record 750/751) in `P+0x1425c`,
    clears it in `P+0x14260`, sets facing and angle to `COM+0x44`, magnitude 1.0.
- The CPU never sets the lock-active flag `P+0x13f24` (only `FUN_14017b220` writes lock words for CPUs), so it
  is expected to move in the free-camera branch (turn toward heading and run), never the TURN_* strafe clips.
  CONFIRMED (cpureads): the consumers `FUN_14025b5e0` / `FUN_14025b270` / `FUN_14024f8f0` take the free branch for `P+0x30 == 0`
  (FORMATS-CPU 5.9); the slot presses and the dodge are handled by `FUN_1402517c0`, the same code for CPU and human, and act only in states 0, 1, 8.

### 2.3 The VM and the script

Instruction = one 32-bit word, opcode in the low byte, `b1` = bits 8-15, `s` = signed 16 bits 16-31.
All values are 64-bit integers; stack grows down; **arguments are pushed right-to-left, so a native's `a0`
is the top of stack**; callee pops its own `args+locals` (`RET pop=N`). Ops (READ `FUN_1402aaa30`):
`01` native call (`group = word>>28`, `fn = (word>>16)&0xfff`, `b1` = argument words to drop; result in a
register read by `10` PUSHRES; table `0x1402ddd00`), `02` (`b1=0` WAIT `s` ticks, `1` fault, `2` restart),
`03` call local, `04` call imported, `05` return, `06` alloc, `07` branch (`b1` 0 always / 1 if nonzero /
2 if zero / 3 if zero-and-skip), `08` case (compare `b1`), `09` push local, `0a` store local (= += -=),
`0d` pop, `0e` push imm16, `0f` push imm32, `11` push string, `12-16` + - * / %, `17` compare (EQ NE GT LT GE LE
bit-test), `18-1d` and or xor not neg lnot, `1e` shifts, `1f` push data address (word offset, signed 24-bit).
File header: word0 = 0x20 (code start), word1 = function-table offset, word2/3 = table end / string pool.
Function table entries are `(name hash, 0, word offset)`; the name hash is the game's base-40 6-character
packing (table `0x1402f76a0`; `main` = `0x8e225400`). `eneprog006` `main` is at file offset 0x6b8c.
Group 0 = VM library (g0.f5 `rand`, g0.f16 table lookup); group 4 = the battle API.

**Script registers** (names are ours, read from use): v0 mode, v1 movement style, v2/v3/v4 last three nav
nodes, v5 chosen capsule, v6 range class (1 near <= 8, 2 mid, 4 far >= 30 script units), v7 distance to target,
v8 slot to press, v9 its skill, v10 line-of-sight flag, v11 "no valid target", v12-v13 chosen defence kind /
score, v14 melee-threat target, v15-v16 incoming skill / its motion kind s63, v17 melee-defence latch, v18
time-to-impact counter, v19 capsule-hunt armed, v20 think timer, v21 bullet-check cooldown, v22 melee-check
cooldown, v23 wander timer, v24 last applied style, v25 wander sub-timer, v26 target re-pick cooldown, v27
random slot start, v28 capture timeout, v29 wandering, **v30-v38 parameters**, v39 never written (0), v41
frustration counter, v42 stage class, v43 stand-here node, v44 last node. Vectors: 0 self, 2 own capsule,
3 target, 4 incoming attack, 6 own start point, 7 other capsule, 8 target's start point, 9 player 0's
position (follow target when no enemy is left), 10 erase target, 11 mine point.

**Per tick `main`** (script 0x6b8c): init if v0==0 (0x620c: v0=1, v1=0) -> cooldowns v20-v22 count down (0x0904)
-> expire the melee latch (0x09a4) -> pick a target unless in mode 7 (0x03ac) -> update world model (0x01a4:
v0/v3 positions, v7 distance, v6 range class, v10 LOS; **if stuck() > 900 ticks or f234: f235 + v0=1**) -> if no
valid target: face player 0 and stop -> if own cond==2 (reaction) do nothing; if cond==3 v0=1 -> **incoming-attack
pre-check** (0x1de8, only when v0 not 5/6) -> run mode function -> wander timer (0x2a4c) -> movement
(`func_2bd8` while wandering else `func_2d78` by style v1) -> remember positions.

| v0 | Mode (debug name from `eneprog003`'s string pool; which name is which number is our inference) | What it does (script fn) |
|---|---|---|
| 0 | init | 0x620c |
| 1 | NormalMode | 0x69b4, the decision list below |
| 2 | ATTACK_Mode | 0x6430: skill chosen; fire when in band, else position (0x5348); give up after v41 > 150 |
| 3 | ENV_MODE | 0x6514: walk to v11 = the foe's position (its start point if the foe is off the graph), press the type-50 skill within 10 u |
| 4 | ERASE_Mode | 0x6594: walk to the foe's capsule v10 (type 60-62 skills), press within 10 u |
| 5 | BUL_DEFENSE_Mode | 0x6294: guard / dodge an incoming projectile |
| 6 | MOT_DEFENSE_Mode | 0x6364: guard / dodge the target's melee cast |
| 7 | Status_Mode | 0x6608: type-49 skill on an ally (picks a team-mate) |
| 8 | CAPTURE_MODE (own) | 0x66b0: walk to own capsule, stand on it, press a slot |
| 9 | CAPTURE_MODE (other) | 0x6850: same for a capsule from the shared list, timeout 900 ticks |

**Mode 1 decision order** (each tick, first hit wins; "ready" = `slot_ready`, "affordable" = Health cost
+1 if status 17 +1 if status 6 is below current Health, script 0x0048): (1) any ready type-0 skill ->
`func_00e0`: if standing on a capsule step off, else **press every 5th tick**; (2) a capsule worth grabbing
(0x6770) -> mode 9; (3) a type 70-79 skill whose own condition holds (0x4dec) -> press every 5th tick;
(4) a type-90 environment skill and no environment capsule pending -> press; (5) capsule hunt (0x15dc with
v38) -> mode 8; (6) a type 60-62 erase skill with a capsule to hit (0x2624) -> mode 4; (7) a type-49 skill and an ally
exists -> mode 7; (8) think timer: when v20 == 0 set v20 = v31 (30) and continue only with probability v32 %;
(9) pick an attack (0x5f84: scan the 4 slots from random start v27, type <= 40, ready, affordable, situation
test 0x5ddc) -> v41 = 0, mode 2; else a type-50 skill -> mode 3. Idle movement style v1 = 8 throughout.

### 2.4 Movement (READ, script 0x2d78 / 0x517c / 0x2bd8; `FUN_14020d660/d6b0`, `FUN_140178ad0`)

Style v1 (names from `eneprog_freya.ssb`'s string pool): 0 STOP (hold), 1 STARTPOS (go to own start point),
2 ENEMYPOS (to the target), 3 BULLET (turn toward the incoming attack), 4 CAPSULE (own capsule), 5 other
capsule, 6 BUCK (back to the previous node v4), 7 AWAY (the farthest of 10 random nodes from the target),
8 idle-by-v37, 10 erase capsule, 11 environment point, 12 stand on the per-stage preferred node (v43).
**Idle (v37)**: 0 = avoid (if the target is within 200 u of my start point or I am within 30 u of it, go AWAY, else
go home when `path51(target, home) == 0`), 1 = roam random nodes, 2 = camp at the target's start point (stop within 10 u). 1/3 of ticks
use planner style 2 (`COM+0x48`), else 0 (meaning of `COM+0x48` unread).
**Spacing** (`func_517c(min, max, navmode)`, distances in script units = world x 0.1): if navmode and a
per-stage table has a node for the target's node, style 12; else if min <= d <= max and LOS hold, if d < min
style 7 (away), if d > max style 2 (approach). **Wander**: every 90-179 ticks (v23) with probability v36 % set
v29 and `nav_step(random node)`; combat modes keep running, only movement is replaced.
**Strafe / circle**: none. The only sideways motion is the dodge (below). **Jump**: only as the dodge;
ledge jumps / jump zones on the nav graph are unread (`FUN_140177f60`).
**Stuck**: counter `COM+0x120` (< ~7 u moved since the reference point); script resets at > 900 ticks (30 s).
Per-stage "stand here" tables: `func_3004` (0x3004) maps the stage code to a class v42 -- stage 3/13/23 -> 0 (76
nodes), 2/12/22 -> 1 (63), 4/14 -> 2 (85), 24 -> 3 (23), 5/15/25 -> 4 (57), 6/16/26 -> 5 (120), 7/17/27 -> 6 (126),
8/18/28 -> 8 (121; the script also writes 7 first and then overwrites it with 8, so class 7 is unreachable: a script
defect), anything else 9 = none. Tables at 0x31b8-0x4a80, 2 words per node (navmode 1 and 2, 500 = none), indexed by
the node the target stands on. (cpureads) Checked against the cad files: every non-500 entry of every table is a valid, unblocked node index of
the matching cad, so the index is the cad's node index. Table lengths (2 words per node) vs cad node counts: equal for stage codes 3 (76), 4 (85),
24 (23, the 23-node `st04/comdata2.cad`), 5 (57), 7 (127, not 126); shorter for 2 (63 vs 72), 6 (120 vs 122), 8 (121 vs 123). The script bounds-checks its own tables (`func_004a80`: node index > table length - 1 gives 500 = none), so the extra cad nodes of
stages 2, 6 and 8 simply have no override. Most entries are 500 (none): the tables are sparse overrides (1 / 4 / 1 / 29 / 0 / 6 non-none entries per navmode for stages 3, 4, 5, 6, 7, 8; stages 2 and 24 have none).

### 2.5 Difficulty parameters (READ: 82 rows, `enemyparam.epa`; use sites in `eneprog006.ssb`)

| reg | col | name (ours) | use (script offset) |
|---|---|---|---|
| v30 | e0 | target rule | 0x03ac: 0 lowest slot index, 1 highest slot index, 2 lowest Health, 3 highest Health (re-checked every tick); 4 nearest, 5 farthest, 6 random (re-picked every 90 ticks, cooldown v26). Valid = other team, cond != 0 and != 5. Status 9 (value = player + 1) overrides |
| v31 | e1 | think interval | 0x6170/0x69b4: v20 reload; **30 in every row** (30 Hz ticks = 1.0 s) |
| v32 | e2 | attack probability % | 0x6170/0x69b4: `v32 > rand(100)` at each think |
| v33 | e3 | react to bullet % | 0x1b30: roll when a projectile is detected (v21 cooldown 10) |
| v34 | e4 | react to melee % | 0x1d20: roll when the target is mid melee cast and range class is near |
| v35 | e5 | dodge % | 0x242c: `v35 >= rand(100)` when the threat is dodgeable |
| v36 | e6 | wander % | 0x2a4c: every 90-179 ticks |
| v37 | e7 | idle style | 0x2d78 case 8: 0 avoid, 1 roam, 2 camp enemy start |
| v38 | e8 | capsule-hunt strategy 0-3 | 0x15dc argument (details of 1 / 2 / 3 unread) |

Rows (e0 / e7 / e8 fixed within a family; e2..e5 / e6 form a 5-step ladder):
rows 0-19: target rule 0 / 1 / 2 / 3 per block of 5, e7=0, e8=0, ladder (88,88,88,88 | e6 3), (77x4 | 6), (66x4 | 9),
(55x4 | 12), (44x4 | 15). rows 20-39 same with e8=2 (rules 0,1,2,6). rows 40-54: e7=2, e8=1, rules 0/1/2, e2 88..66
and e3-e5 55,44,33,22,0 (heavy attacker, weak defence). rows 55-59: rule 6, e8=3, e2 44,33,22,11,5, e3-e5 88..66
(evasive). rows 60-64: rule 4, e7=2, (88,55,88,55) down to (66,11,66,11). rows 65-69: rule 5, e7=1, e8=2,
(88,88,55,88) down to (77,77,11,77). rows 70-74: rule 0, e7=1, e8=2, e2 = 0,33,22,11,5 and e6 = 11,18,21,**80,80**
(row 70 never attacks; 73/74 wander 80 %). rows 75-81 mixed (rules 1,3,5). Rows 82-99 are zero (unused).
Partners = row 0. Starting Health (byte 0) = 20 in all used rows.

### 2.6 Skills: choice, timing, costs (READ)

- **AI type = skill field s42** (program record offset +0x50; g4.f175): our cache already holds it
  (`skills.json` raw[42]); values seen on registered skills: 5,6,10-29,31-33,40-42,49-51,60-62,70-86,89,90,99,100.
  100 = never used by the AI (52 skills), 70-79 = utility/self skills with conditions (appendix B), 90 =
  environment, 80-89/99 = defence-like (never chosen by type; the defence modes pick by capsule type 2).
- **Affordability**: the script itself only checks Health cost (s25, g4.f169) against Health. Aura and Level
  affordability is not computed in the script: it asks the program's per-slot state (`slot+0x24`: 0 = ready, and
  2 / 4 are also accepted by the capture / defence checks; meanings unread).
  Slot capture swaps (0x0c84) replace, in order: a type-100 skill, a skill whose Health cost >= Health, a duplicate
  skill, a duplicate colour, a skill whose Aura cost (slot+0x18) > max Aura (stats+0x18), else a random slot; ids
  **85 (Killer Satellite), 282 (Judgment), 298 (Invulnerability)** are never overwritten. Free slot first (0x1124).
- **Timing**: an attack is chosen at most once per 30 ticks and only with probability e2. Utility / Aura
  skills (types 0, 70-79) are re-pressed every 5th tick while their condition holds (so 6 presses/s).
  No press while standing on a capsule (a press would capture it): attacks return false (`0x554c`), mode 1 steps off.
- **Range**: appendix B (fire bands) and the matching positioning bands (func_5348).

### 2.7 Defence and dodge (READ rules; threat predictor ~)

- **Bullets** (0x1a74, mode 5): every 4th tick (`TICK & 3 == player index`) when v21 == 0, scan all hostile attack
  objects (`g4.f176/f177`: s43 == 0, motion not 3/4/12/13/14); `obj186` returns predicted ticks to impact
  (`FUN_14020f9a0`, per-motion formulas, assembly, unread beyond the straight-shot and ballistic branches).
  First hit sets v15/v16, v21 = 10, then `rand(100) < v33` -> mode 5.
- **Melee** (0x1c7c / 0x1d20, mode 6): target is in a cast state (cond 2), its current skill is an attack, my
  range class is near, v22 == 0 (cooldown 10), roll `v34`; v18 = `T@1bcc[s63]` (+ distance when > 10 ticks).
- **Guard choice** (0x2204): over my ready defence-capsule (type 2) slots pick the best of
  `T@1f1c[s63 of incoming x 8 + s44 of defence]` (0-3, 3 best; table 21 x 8 in the script); press when
  `v18 < T@1e54[s44]` = {10,10,15,10,10,10,15,0} ticks (> 2) after turning to face the threat (< 45 off).
  Not while status 5 (buttons frozen).
- **Dodge**: only if the character's dodge record is **not 750** (`dodge_skill != 750`, i.e. the second dodge
  kind); `T@23a4[s63]` = {13,15,10,0,0,10,10,-1,-1,-1,10,10,20,20,0,-1,-1,13,10,0,-1}: 0 = no dodge, -1 = style 6
  (run back), n > 0 = dodge when time-to-impact < n with probability e5 -> `path49(heading to the threat +- 90)`,
  other side if `f53` says there is no floor. UNCONFIRMED why 750 is excluded (read literally).

### 2.8 Targeting and lock-on, including 3-4 players (READ)

- Players 0..3 (`DAT_1404bc9d8[4]`); `team_of` is `P+0x1C` (a per-slot table in modes 4/5). Teams are plain
  numbers from the quest record (0/1) or the setup; allies are same-team players, used by type-49 skills.
- The script re-picks the target each tick for rules 0-3 and every 90 ticks for 4-6, then `set_target` and
  `v0 = 1` if it changed. No enemy left: walk toward player 0 until facing it within 45 degrees, then stop.
- The program also keeps a table of per-pair line of sight + distance, updated **one pair per tick**
  (`FUN_14012a900`, 6 unordered pairs, round-robin, symmetric copy): `pairflag` (v10) and `pairdist` come from it,
  so a CPU's LOS knowledge is up to 6 ticks (0.2 s) old; the target-position vector `dist(0,3)` is current.

### 2.9 Latency and randomness (READ)

Everything runs at the 30 Hz logic tick. Built-in delays: think interval 30 ticks; bullet check every 4th tick
then 10-tick cooldown; melee check cooldown 10; pair table up to 6 ticks; capture timeout 900; wander
every 90-179; stuck 900; target re-pick 90 (rules 4-6). RNG: `FUN_14020dcf0` = `(rand() & 0x7fff) * n >> 15`
(uniform, no bias fix) with the MSVC CRT `rand()` seeded by `srand(GetTickCount64())` (`FUN_140291f00`), the same
stream effects and sound use (141 call sites). Per-run non-deterministic; a lockstep port needs its own seeded stream.

### 2.10 What is versus-relevant

| Item | Relevant to a versus CPU? |
|---|---|
| `eneprog006.ssb` + epa rows 0-81 + `com/deck` + `comdata*.cad` | **Yes**: the generic fighter, same rules as versus (Level, Aura, capsules, erase, status, environment, 2v2 allies) |
| `eneprog000-005`, `_nodef`, `_noNdef` | No: unreferenced (older variants: no defence / no normal defence) |
| `bossprog00`, `eneprog_freya`, `boss/*` scripts, quest records with bosses | No (story) |
| `chkprog000`, `motchk`, `rootchk`, `single2.csv` | No (developer checkers) |
| Quest-only fields: start-point index, per-enemy Health 5-30 | Only as hints (a versus CPU would take the rule's Health) |

## 3. What we have

- `--demo` (`main.rs:1347-1430`, state `DemoWalker` `main.rs:1561-1570`): a test script. Wall-clock driven
  (`time.elapsed_secs()` for tap timing, so runs are not reproducible; `GAME.md:296`), walks to the nearest own
  capsule while a slot is free, otherwise closes in until < 60 u then circles at +-0.7 stick, taps a filled slot
  twice a second, jumps and side-steps when stuck. No target rule, no spacing, no defence, no dodge, no
  nav graph. `PD_TEST_PRESS`, `PD_TEST_STICK[2-4]`, `PD_TRACE_SCRIPT`, `PD_TEST_LOCK/SWITCH` drive pads for tests.
- The sim's input surface is `sim::Input` (`sim.rs:114`): `stick: [i8; 2]` (x right, y forward toward the
  opponent), `jump`, `lock` (toggle), `skills` bitmask (slots 0-3), `cam`, `recenter`, `switch_target`.
  A CPU seat is just another producer of this struct; the stick is lock-relative, the program's CPU stick is a
  world angle, so a port converts (bearing of the heading relative to the bearing to the target).
- Lock for humans: `combat/lock.rs:83 next_target` (first valid slot, `switch_target` cycles), `aim_at`
  (`lock.rs:106`). No CPU "always locked for casts, free for movement" mode.
- Match rules, Fighter state, slots, statuses, capsules exist (`combat.rs:671`, `moves/status.rs`); the AI's
  observables map onto them except: per-pair LOS/distance table, projectile time-to-impact, capsule-contact state,
  the 4-slot "ready" flag, status ids 5/6/8/9/12/13+k/17, `cond` codes.

## 4. Discrepancies (our code vs the program's rule)

| # | Program | Ours | Effect | Status |
|---|---|---|---|---|
| D1 | CPU seats exist only in quests; versus menu has none; brain = script on a 30 Hz tick | no CPU; `--demo` test script | nobody can fight a computer in versus | CONFIRMED (menu text, code) / scope = Laurence's |
| D2 | an attack is decided at most every 30 ticks with probability e2; utility / Aura skills re-pressed every 5th tick while their condition holds | demo taps a filled slot 2/s on the wall clock | cast rate and mix far from the original | CONFIRMED |
| D3 | target rule from row (7 rules), re-evaluated per tick, status 9 override, no-target walks to player 0 | circles the lock target; nothing for 3-4 players | wrong in FFA / 2v2 | CONFIRMED (read both) |
| D4 | absolute-angle stick at magnitude 1 or 0, camera yaw 0, lock-active flag never set (so no TURN_* strafes) | demo uses a lock-relative strafing stick | wrong gait / animation | **CONFIRMED** (cpureads: FORMATS-CPU 5.9; gait = run whenever the magnitude is 1.0) |
| D5 | CPU always has `P+0x13f1c/0x13f20` set (aims casts at its target, homing) while not "locked" for movement | one `locked` flag does both | CPU shots would not home / dash if only the flag is used | **CONFIRMED** (cpureads: `FUN_140258180` reads only band + target; for CPUs the aimed branch is taken whenever a target exists; FORMATS-CPU 5.9) |
| D6 | LOS/distance for decisions come from a 6-pair round-robin (0.2 s old) | none | a free sim-side accuracy; add the same lag | CONFIRMED rule |
| D7 | RNG = CRT `rand()` | n/a | document; use own seeded RNG | CONFIRMED |
| D8 | an attack is gated by LOS + a distance band per skill type; spacing by the band, flee under min | none | CPU would fire at wrong ranges | CONFIRMED |

## 5. Reads (closed by lane `cpureads`, 2026-10-01)

Each former item, with its result. READ = a program address in FORMATS-CPU; UNCONFIRMED = named with the live trace it needs.

1. **Nav graph: READ.** Layout, record fields, five route tables, hub classes, route pool, runtime flags: FORMATS-CPU 5.2-5.3 (`cad_dump.py`
   reads and checks all 12 shipped files). Queries `FUN_140176f90/e90` (node at a point, height-plane, 20.0 tolerance, loose fallback),
   `FUN_140179f80` (LOS along portals), `FUN_14017a1a0/a750` (`path51`, hub routing), `FUN_14020d100`, `FUN_1401791f0/92b0`, `FUN_140178d00`,
   `FUN_1401796b0`, `FUN_140179420`: 5.4-5.7. File choice: `com\st(code % 10)\comdata(version).cad`, only stage codes 2-8 have data (5.1).
   UNCONFIRMED: the semantic definition of tables T1/T3 and the greedy shortcut tie-breaks (trace `COM+0x68..0xec`); the region `0xb0ec..0xb3bf`
   and node `+0x34..0x53` are never read by the program.
2. **Local hazards / jumps: READ (structure).** `FUN_140177f60` = sphere avoidance against stage objects within 50 u (5.8); jumps = jump-link ids in
   the node lists + jump points (5.5); `FUN_140178470` off-graph recovery (5.8). UNCONFIRMED: which stage objects answer `vtable+0x98(4)`/`+0x1f0`
   (trace the ids/kinds/spheres near a fighter per arena); a first port may skip hazards (deviation to list).
3. **Threat predictor `FUN_14020f9a0`: READ.** Per-motion formulas for s63 0/11, 1, 2, 5, 6, 7, 9, 10, 15/16, state-3 straight shot: FORMATS-CPU 4.4;
   executable and unit-tested in `engine/re/threat_predict.py`. `P+0x44` = `S+4` = the player index (inferred: Decoy / status 9 and the pair table use it as one); UNCONFIRMED: which sim fields feed `O+0x1c0..0x1d4`.
4. **Natives not read: READ.** f53 (dodge landing probe), f60 (path_step 0/1/2, arrived = same node and < 7.0 u), f49 (dodge request), f51 (`path51`), f123
   (`arr_b - arr_a + 30` in versus; meaning of the pair UNCONFIRMED), f125 (cast skill), f135 (**nearest pickable stage object**, not an enemy: Psycho Kinesis),
   f229 (**Health += x**; the script's `-100` kills the CPU when no hand slot can be freed), f234/f235 (hopeless flag; teleport to start point), f176-f200 (object scan,
   shared capsule list of 4, per-player list of 3), f80-f85 (`COM+0x48` planner style: T2 / T4 table choice). FORMATS-CPU 4.2-4.3, 5.
5. **Pad consumers for CPU: READ.** `FUN_14024f8f0`, `FUN_14025b5e0`, `FUN_14025b270`, `FUN_14025ad50`, `FUN_140256e80`, `FUN_1402569b0`: the CPU stick is an absolute yaw
   (camera 0), magnitude 1 -> run gait (threshold 0.7), turn rate = the character's, no strafe ever (FORMATS-CPU 5.9). Dodge: `FUN_1402517c0` handles the button edge
   (held and not previous) for CPU and human alike, in states 0, 1, 8; slot presses likewise. D4 and D5 CONFIRMED.
6. **Status 9 / 17 and the versus CPU seat: READ.** Status 9 = Decoy (add code 40, value = attacker + 1); status 17 is written only by add code 19, which no
   registered skill carries (never active in shipped data). The versus launcher `FUN_1401d0750` writes human seats with no AI info; the seat loop `FUN_140128cc0`
   builds fighters through vtable slot 0 (`FUN_14024e790`), so **a CPU seat is the same call with the seat's human flag 0 and an AI info struct** (FORMATS-CPU 6): no
   shipped path does it for versus, but nothing else is needed.
7. **Capsule ownership: READ.** The 4-entry global list (`0x14058bd40`) has no owner field; the 3-per-player list holds the capsules deployed for that player ("own capsules"); the AI applies no
   extra rule (FORMATS-CPU 4.3).
8. **`COM+0x40` reset: READ.** `FUN_14024ec80` clears `P+0x14310` (and so the request path) at the start of every CPU tick; a dodge request made while the state is not
   0/1/8 is dropped when `FUN_140178ad0` runs (FORMATS-CPU 5.9).
9. **`eneprog006` details: READ** (with the new goto-style decompiler `engine/re/ssb_dec.py`, used on `func_0015dc`, `func_004dec`, `func_004a80`):
   - **Capsule hunt `func_0015dc(v38)`** (mode 1 step 5): 0 when no own capsule is out (`func_12a0()`, the count of own placed capsules, 0..3) or `f123(me) <= 1`;
     if `f123(me) <= 3` it returns `func_151c()` (latch `v19` set when all three own capsules are still out; with max Aura <= 5 it never hunts, because it looks for a
     capsule of type `s9 == 0`, which no skill has; otherwise it hunts when an own **attack-type** (`s9 == 1`) capsule is out, else when any own capsule is out). With
     `f123 > 3`: strategies 0, 1 and 3 prefer the nearest own attack-type capsule (`func_130c(1)` sets `v5` = its index and `v2` = its position lowered by 9.0 u),
     else the nearest own capsule of any type (`func_11b8`); strategy 2 takes the nearest of any type only. All of them return 1 whenever an own capsule is out. The
     extra "prefer a defence capsule when the foe holds attack skills and I have no usable defence skill" branch of strategies 0 and 3 is unreachable, and the test
     `func_000b48(12) == 0` ("no usable skill of capsule type 12") is always true (no skill has `s9 == 12`).
   - **`func_004dec` (AI types 70-79)**: exactly as appendix B: 70, 75, 79 always; 71 a usable attack skill in hand, Aura full, 1/30; 72 own status 0-3 < 0; 73 max Aura <= foe's + 4;
     74 no own capsule out; 76 own status 0-3 < 0 or foe's > 0; 77 foe's status 0-3 > 0; 78 never.
   - **Per-stage tables** (`func_004a80`): `tbl(&table, node*2 + navmode-1)` with the node index > table length - 1 giving 500 (none); the index is the cad node index.
   - `func_0048` (affordable) = Health cost (s25) + 1 (status 17) + 1 (status 6) < Health; `func_00a70` / `func_00b48` (identical) = "a usable skill of capsule type t in my hand" (ready, or slot state 2 or 4);
     `func_000c20(t)` = the target holds a skill of capsule type t.

## 6. Proposed build lanes (ordered; disjoint fences)

Prerequisite: Laurence reopens the "no AI" scope (question 1).

**Lane A `cpureads` (RE only, docs + tools): DONE 2026-10-01.** Delivered `engine/docs/FORMATS-CPU.md` (instead of `CPU-AI.md`), `engine/re/ssb_dis.py`,
`ssb_run.py`, `cad_dump.py`, `threat_predict.py`, each with a test; no decompiler (`ssb_dec.py`) and no `cpu_trace.py`. The text below is the original brief.
Original goal: close section 5 items 1-5. No game code. Proof: decompile all 16 scripts, assembly notes for the
predictor with a unit-testable formula, nav-graph layout table. LIVE needs (recorder, see tracing below):
(a) memory dump of `DAT_1404c0180` for stages 2-8 (header counts, first 3 nodes, first edge) to check the
decoded layout; (b) per-tick `COM+0x50` steer target, `COM+0x68..` path ring for a scripted walk; (c) for the
predictor, log `g4.f186` arguments and return for each tick a projectile is in flight (return = ticks to impact,
compare with the real flight time from the trace).

**Lane B `ssbvm` (formats + tests, no game behaviour).** Goal: `pd_formats/src/ssb.rs` (parser, base-40 names,
function table) and a VM (`pd_formats/src/ssb_vm.rs`) with a `Host` trait for group 0 / group 4 calls; plus
`pd_formats/src/cad.rs` once Lane A has the layout. Fence: only `pd_formats/src/{ssb,ssb_vm,cad}.rs`, a
`pd_convert` `ssb` / `cad` subcommand, tests. Proof: tests from reads (each opcode vs `FUN_1402aaa30`; all 16 files parse;
the `main` entry offsets 0x6b8c / 0x23c0 / 0x4a0; function counts; per-stage table lengths 76 / 63 / 85 / 23 / 57 / 120 / 127 / 121 for stage codes
3 / 2 / 4 / 24 / 5 / 6 / 7 / 8 (cpureads: they equal the cad node counts only for 3, 4, 24, 5 and 7; for 2, 6, 8 the cads have 72, 122, 123 nodes), and the
cad parser against `cad_dump.py`'s checks). LIVE needs: a log of **every native call of one
CPU for 600 ticks** (breakpoints at the VM's dispatch `call [rax+rdx*8]` at `0x1402aac34`: `rdx` = function number, callee
= `[rax+rdx*8]` (identifies group + function), `rcx` = the argument block (dump 4 qwords: `a0` is the first), and at
`0x1402aac37` right after it: `rax` = the result) plus the `rand` outputs (return of `FUN_14020dcf0`); our VM fed the recorded results
must reproduce the same call sequence (strongest test of the VM + script).

**Lane C `cpuhost` (game).** Goal: a CPU seat in versus that runs `eneprog006` on the lane-B VM at 30 Hz.
Fence: new `pd_game/src/cpu/{mod,host,natives,nav,threat}.rs`; minimal additive edits to `main.rs` input gather
(a seat is CPU -> `cpu::tick` fills `Input`), `sim.rs` (CPU lock mode: target + band without the lock flag, only if D5
is confirmed), `combat/lock.rs` (accessor). Dependencies: A and B; coordinate with `moveskills` (sim.rs float/dash).
Proof: scripted headless runs (`PD_SHOT_TICKS`) of a CPU vs a held-still pad and vs `PD_TRACE_SCRIPT`, a
mode-transition log checked against the live log; unit tests per native from its asm; a replay golden for a fixed
seed (own RNG). LIVE needs (the same CPU battle in the original and in ours, same stage / arsenals / row):
mode v0 per tick, v1, v20, v21, v22, v23, v41, the press ticks and slot, the stick magnitude / angle per tick,
distance to the target; distributions to compare: think interval (30), time from projectile spawn to defence
press, dodge rate vs e5, cast-per-second, time at each range band.

**Lane D `cpuseat` (frontend, optional, needs Laurence).** Goal: choose a CPU seat, its row (family x 5 grades),
and arsenal (school decks) in the setup screens. Fence: `frontend/state.rs` + setup files (hudbits owns them
now; wait for that lane to land). Proof: screenshots A-F sheet of the picker.

**Tracing the original.** The recorder already starts versus matches and writes pad input. A CPU opponent can be
created by the program's own debug launcher `FUN_140214770` (it fills an AI-info struct at `0x1405b18b0` with
`eneprog006.ssb`, row **20**, Health 20, an empty deck name, and starts a battle; it has no static caller, so the
rig would remote-call it: UNCONFIRMED that this works) or by setting `param_3+0x20` / `+0x10` before
`FUN_14024e790`. Per-tick log (read memory at the tick hook `0x1401d12b0`): `P+0x1647c + 4k` (k 0..49),
`P+0x14320` (steer target), `P+0x14264/68/70`, `P+0x14310`, `P+0x13f1c/20`, `P+0x12e40` (all four), `P+0x12f04`,
stats `+0x0c/+0x18/+0x1c`, hand slots `+0x348+0x50k`, plus the native-call bracket above. The orchestrator owns the
rig; this lane only specifies the log.

## 7. Questions only Laurence can answer (max 5)

1. **Reopen "no AI"?** a) yes, build a CPU opponent from the original's own script (recommended), b) keep it
   out, c) only after four-player versus is fully verified.
2. **How should the player choose CPU strength?** a) five grades per personality (the file's own ladder, 88%
   ... 44%), b) one fixed strength (the original's partner row 0), c) random row per match.
3. **Which CPU arsenals?** a) the original's enemy decks (school decks such as DECK_NATURE / PSYCHO / KI /
   OPTICAL / RELIGION and per-character decks), b) the player's own arsenals, c) both selectable.
4. **Do you have footage of a computer enemy dodging or guarding a shot** (your single-player clips)? If yes name
   the file so latency can be MEASURED; a) yes, b) no (we trace the original live instead).
5. **Personality names**: expose the original's seven target rules (first / last / weakest / strongest /
   nearest / farthest / random) as "CPU style", or hide them? a) expose, b) hide, c) expose in a debug screen.

## Appendix A: natives used by `eneprog006` (group 4, address table `0x1402ddd00` + 4 x 8)

Names are ours (the exe has none); "by use" = the meaning comes from how the script uses it. **Superseded in detail by FORMATS-CPU section 4**
(cpureads read every native below from its assembly: "READ call / internals UNREAD" entries are now read, see 5.4 for the nav ones; f18/f31 = loose/strict
`node_at`, f24 = portal LOS, f51 = `path51(avoid, goal)` with 1 = success, f53 = dodge-landing probe, f186 = FORMATS-CPU 4.4).

| g4.fN | name(args) | meaning | status | program address |
|---|---|---|---|---|
| f0 | `setvar(i,v)` | script register i := v (int regs at AI+0x2074, 48 of them) | READ | 0x14020d460 |
| f1 | `getvar(i)` | read register i | READ | 0x14020d480 |
| f2 | `vcopy(dst,src)` | copy 3-float vector register (20 of them, AI+0x2144, 12 bytes each) | READ | 0x14020d490 |
| f16 | `dist(a,b)` | distance between vectors a,b x 0.1, int ("script units": 8 = 80 u, 30 = 300 u) | READ | 0x14020d4d0 |
| f18 | `navnode_at(v)` | nav-graph node containing point v (-1 none) via FUN_140176f90 | READ call / internals UNREAD | 0x14020d600 |
| f31 | `navnode2(v)` | second node lookup (FUN_140176e90) | READ call / internals UNREAD | 0x14020d630 |
| f19 | `MOVE_TO(v)` | steer target := vector v (ignored while airborne/COM+0x10) | READ | 0x14020d660 |
| f20 | `nav_step(node)` | steer to centre of node if node flag&0xC==0 (FUN_14020d100) | READ | 0x14020d6b0 |
| f21 | `vadd(v,dx,dy,dz)` | v += (dx,dy,dz) x 10.0, the arguments being raw **float bits** (the script's `-1083808154` is -0.9f: own capsule point lowered 9.0 u) (cpureads) | READ | 0x14020d6f0 |
| f23 | `heading(a,b)` | atan2(b.x-a.x, b.z-a.z) in degrees (via 32768-unit yaw) | READ | 0x14020d7b0 |
| f24 | `ground_ok(a,b)` | FUN_140179f80 walkability/LOS between points | READ call / internals UNREAD | 0x14020d830 |
| f28 | `vx(v,k) / f29 vy / f30 vz` | component x scale 0.1 x k | READ | 0x14020d900 |
| f32 | `rand(n)` | (CRT rand()&0x7fff)*n>>15 = uniform 0..n-1 | READ | 0x14020dcf0 |
| f33 | `angdiff(a,b)` | `d = a-b; if d < -180: d += 360` (only the low side is wrapped: range -180..360) (cpureads) | READ | 0x14020dd10 |
| f37 | `nodedist(a,b)` | distance between two nav nodes (centres) x 0.1 | READ | 0x14020daf0 |
| f48 | `PRESS(slot)` | ORs the slot button bit into the pad block "pressed" mask P+0x14264 (remapped via +0x14278 table while status 0x24 is on) | READ | 0x14020deb0 |
| f49 | `path49(deg)` | DODGE request: COM+0x40=1, COM+0x44 = deg x 91.02 (yaw units); consumed at end of tick by FUN_140178ad0, cleared at start of next (P+0x14310) | READ | 0x14020df20 |
| f51 | `path51(a,b)` | FUN_14017a1a0 route/visibility test between two points on the nav graph (returns int) | READ call / internals UNREAD | 0x14020df90 |
| f53 | `f53(deg)` | probe point 10x(angle) in front of body, FUN_140179f80 walkability test (1 = ok) | READ call / internals UNREAD | 0x14020e020 |
| f56 | `stuck()` | COM+0x120: ticks the body has moved <~7 u from its reference point | READ | 0x14020e300 |
| f60 | `path_step()` | 0 = no goal node or no route, 1 = following, 2 = arrived (same node as the goal and < 7.0 u; the 5.5 u branch is dead code) (cpureads, FORMATS-CPU 5.4) | READ | 0x14020e380 |
| f82 | `style0() / f85 style2()` | COM+0x48 := 0 / 2 (path-planner style, read by FUN_140178d00/1792b0, UNREAD) | READ set / meaning UNREAD | 0x14020e5f0 |
| f96 | `set_target(p) / f97 get_target()` | AI+0xc target player index (feeds lock block) | READ | 0x14020e650 |
| f112 | `self()` | own player index (P+0x14) | READ | 0x14020e670 |
| f113 | `vpos_of(p,v)` | v := position of player p (only if p alive and p+0x10==0) | READ | 0x14020e690 |
| f114 | `vtargetpos_of(p,v)` | v := START POINT of player p (P+0x18 index into the start-point table) | READ | 0x14020e700 |
| f115 | `yaw_of(p)` | facing of p in degrees | READ | 0x14020e760 |
| f117 | `pairflag(a,b)` | LOS flag of the pair table (updated one pair per tick, see 2.12) | READ | 0x14020e7f0 |
| f118 | `hp(p)` | stats+0x0C Health | READ | 0x14020e810 |
| f119 | `maxaura(p)` | stats+0x18 (Aura capacity) | READ (name by use) | 0x14020e830 |
| f120 | `aura(p)` | stats+0x1C current Aura | READ (name by use) | 0x14020e850 |
| f121 | `has_status(p,id)` | FUN_14024d4d0(stats, id): signed amount or 0 | READ | 0x14020e880 |
| f122 | `team_of(p)` | P+0x1C team (team table in modes 4/5) | READ | 0x14020e8c0 |
| f123 | `f123(p)` | P+0x17158 / respawn timer variant; script treats <=1 as "too low to hunt capsules" | UNCONFIRMED meaning | 0x14020e920 |
| f124 | `cond(p)` | 0 absent, 1 idle, 2 states 10-12 (cast / melee cast), 3 state 0x11, 4 acting, 5 out, 6 not entered | READ | 0x14020e980 |
| f125 | `f125(p)` | P+0x12F5C while state 10-12: skill being cast | READ (name by use) | 0x14020ea50 |
| f134 | `dodge_skill(p)` | charaset dodge record (750 / 751) of p | READ | 0x14020ed30 |
| f135 | `f135(p)` | `FUN_14012e160`: id of the nearest pickable **stage object** within 100 u in a 45 degree cone or within 25 u; -1 none (cpureads, FORMATS-CPU 4.3) | READ | 0x14020ed70 |
| f144 | `slot_skill(p,k)` | skill index in hand slot k (-1 empty) | READ | 0x14020ee00 |
| f146 | `slot_ctype(p,k)` | capsule type s9 of that skill (1 attack .. 6 env) | READ | 0x14020ee70 |
| f147 | `slot_f360(p,k)` | slot+0x18 = Aura cost | READ (name by use) | 0x14020eec0 |
| f148 | `slot_ready(p,k)` | slot has skill, state(+0x24)==0, +0x40==-1 | READ | 0x14020ef00 |
| f150 | `slot_state(p,k,x)` | slot state == x | READ | 0x14020ef90 |
| f161 | `sk_ctype(s) / f162 sk_sel / f166 sk_motion / f168 sk_s44 / f169 sk_hpcost / f172 / f173 / f175 sk_aitype` | skill-table fields: s9 / s43 / s63 (when s43==0) / s44 (when s43==2) / s25 / s48 / s51 / s42 | READ | 0x14020f2a0 |
| f165 | `sk_class(s)` | FUN_140167970 delivery class 0..5 (2 vs 4 split by s44 and s63) | READ | 0x14020f330 |
| f171 | `sk_ok(s)` | FUN_140167a10 (used only by check scripts) | READ | 0x14020f420 |
| f176 | `scan_reset() / f177 scan_next()` | iterate the 4x48 world-object table (0x1F0 each at P+0x4E0): next hostile attack object (s43==0, motion not 3/4/12/13/14); -1 end | READ | 0x14020f550 |
| f178 | `obj_skill(o) / f179 obj_pos(o,v)` | skill id of object / position (+0x100) | READ | 0x14020f6a0 |
| f186 | `obj186(o)` | FUN_14020f9a0: predicted ticks until object o reaches me (0 = harmless). Per-motion formulas in assembly | PARTLY READ | 0x1402105a0 |
| f192 | `tbl192(p,k) / f193 tbl193(p,k,v)` | per-player list of 3 capsule records (table 0x140584F80, stride 0x48 per (player*3+k)): id / position | READ call / meaning by use | 0x140210a50 |
| f194 | `f194() / f195()` | P+0x17160 / +0x17164: capsule-contact state (script: 1 own capsule, 3 foreign) and its index | READ (name by use) | 0x140210b10 |
| f197 | `cap_skill() / f198 cap_due / f199 cap_pos / f200 cap_index` | global capsule list at 0x14058BD30 (stride 0x1C) | READ summary | 0x140210b70 |
| f208 | `TICK()` | global tick counter DAT_1404BC63C (30 Hz) | READ | 0x140210ce0 |
| f210 | `nodecount()` | nav node count (cad +0x2EE4) | READ | 0x140210d00 |
| f229 | `f229(p,x)` | `FUN_14024d7a0(stats,x)`: Health += x (dies at <= 0); the script's -100 kills the CPU when no slot can be freed (cpureads) | READ | 0x140210e80 |
| f234 | `f234()` | COM+0x110 = "hopeless" flag set by the recovery scan (cpureads, FORMATS-CPU 5.8) | READ | 0x14020e340 |
| f235 | `f235(p)` | `COM+0x120 = 0`, `FUN_1402501d0(P, start point record of P+0x18, P+0x12e58)`: put the player back on its start point (cpureads) | READ | 0x140210f70 |
| f425 | `f425() / f426 stage_div10()` | stage code DAT_1404C01D8 (0x7777 -> -1) and stage/10 | READ | 0x140211ed0 |

## Appendix B: AI type (s42) -> fire band and positioning band (script units = world u / 10)

Fire test `func_554c` (needs: not on a capsule, target reachable `cond` 1/2, target not under status 8/12;
`func_54d4(min, max, needLOS)`). Positioning `func_5348 -> func_517c(min, max, navmode)`.

| s42 | skills (examples) | fire band (LOS) | extra gate | positioning band (navmode) |
|---|---|---|---|---|
| 0 | enemy-only | always when ready | - | - |
| 5 | Scream of Evil, shots | 5-30 | - | 5-25 (1) |
| 6 | Charged Particle | 5-50 | - | 5-45 (2) |
| 10 | melee | 0-5 (LOS) | foe height within 15 u unless a nav node | 1-3 (0) |
| 11 | Photon Wave/Burst | 0-3 (LOS) | same | 0-2 (0) |
| 12 / 13 / 14 | mid / long straight / arc | 5-30 / 5-50 / 10-30 (LOS) | - | 5-25 (1) / 5-45 (2) / 10-25 (1) |
| 15 | Laser | 30-50 | 1/60 per tick (attack mode) else 1/90 | 30-45 (2) |
| 16 / 17 | parabola / throw | 10-30 / 30-50 (LOS) | - | 10-25 (1) / 30-45 (2) |
| 18 | crawlers / twister | class 2: 2-30, class 4: 2-50 (LOS) | floor between us, or no node + 1/60 | 2-5 (1 or 2) |
| 19 | Psycho Kinesis | class 2: 10-30, class 4: 30-50 (LOS) | `f135(self) != -1` | by class: 10-25 (1) / 30-45 (2) |
| 20 / 21 / 22 | arrow, rain, gravity press | class 2: 10-30, class 4: 30-50 (20: LOS, 21-22: no LOS) | - | by class |
| 23-28, 31, 40 | Restrain / Exhaust, Tiger / Dragon / Giant Slayer, Wind Pressure, Hungry Demon, Neutralization, Decoy | 5-50 (LOS) | situation test 0x5ddc (23 target has Aura, 24 target holds attack skills, 25 defence, 26 other, 27 any, 28 same slot full, 31 target has buffs, 32 own Health < target, 40 own Aura full) | 5-45 (2) |
| 29, 32, 33 | Disrupt, Fusion, Angel's Breath | 0-5 (LOS) | 0x5ddc | 1-3 (0) |
| 49 | friendly targeting | ally in range, mode 7 | - | - |
| 50 | landmines / gas | mode 3 (walk to the foe's node, press within 10 u) | - | - |
| 60-62 | Abyss, Eliminate, Shatter | mode 4 (foe capsule) | `func_1810` picks the nearest capsule of the class | - |
| 70 | Meditation, Heat, Speed Up, Healing Water, Recall, Level Amp... | always | `func_4dec` | - |
| 71 | Optical Camo, Tiger's Strength, Trance, Void | a ready attack skill in hand, Aura full, 1 in 30 per tick | `func_4dec` | - |
| 72 | Refresh, Angel Tears, Blessed Rain | own attribute status below 0 (status 0-3 < 0) | `func_4dec` | - |
| 73 | Disintegrate, Vicious Balance, Quantum Decay | foe's max Aura + 4 >= mine | `func_4dec` | - |
| 74 | Evaporate, Violent Change, Particle Convert, Overheat, Meltdown | none of my own capsules is out | `func_4dec` | - |
| 75 / 79 | Massive Quake, Tremor / Turbulence, Calamity | always | `func_4dec` | - |
| 76 / 77 | Purify, Doppelganger, Armaros / Return to Nature | own attribute status < 0 or foe's > 0 / foe's > 0 | `func_4dec` | - |
| 78 | Starting Point, Judgment, Crush | never auto-used | `func_4dec` | - |
| 41, 42, 51, 80-89, 99 | Frighten, Annoying Gift, Boost Mine, defence-like, Orb / Amulet | not chosen by any attack, utility or capsule path | - | - |
| 90 | environment | when no environment capsule is pending (`cap_skill() == -1`) | - | - |
| 100 | never used | - | - | - |

Class 2 vs 4 (`FUN_140167970`, g4.f152/f165): for attack skills (s43 = 0) 1 = motions 3/12/13/14, 5 = motions 8/9,
0 = motions 4/16, otherwise 2 when s44 != 1, else 3 (motions 0, 5, 10, 11) or 4 (others); 3 is mapped to 4.

## Appendix C: tools used (own code, not committed)

Disassembler / decompiler for `.ssb` (from the VM at `0x1402aaa30`), a native-table dumper (`0x1402ddd00`),
capstone for undecompiled functions (`FUN_14020d460` ... `FUN_14020f9a0` tail). Lane A committed `engine/re/ssb_dis.py`, `ssb_run.py`, `cad_dump.py`,
`threat_predict.py`; the capstone assembly reads (scratch venv, nothing committed) are cited by address in FORMATS-CPU.
