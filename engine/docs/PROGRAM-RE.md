# Reading the game program (PDUWP.exe) — pilot, 2026-09-27

Skill behaviour is not in any data file, so we read the 2017 program itself (`game_files/PDUWP.exe`, x64, no symbols) with Ghidra. This pilot covered three moves (Psycho Knife, Slide Laser, Blaster, plus Laser as a cross-check). **Result: the method works.** The program gives exact numbers, and it showed that most of the timing lives in a second table inside `gsdata_en.dat` that we had not decoded (the "action table", below).

Rules kept: no decompiled or disassembled game code is copied here. Logic is described in our own words, with function addresses (`FUN_1400abcd0`) so it can be found again. Raw exports stay in `%USERPROFILE%\.pdtools\re-out\` (outside git).

Confidence marks: ✅ read directly from the code, with the numbers checked against the data · ⚠️ read from the code, but one link in the chain is inferred · ❓ open.

## Tooling (how to repeat)

- Ghidra 12.1.4 at `%USERPROFILE%\.pdtools\ghidra_12.1.4_PUBLIC`, Java 21 (`JAVA_HOME=%USERPROFILE%\.pdtools\jdk-21.0.12.1+1`).
- **Ghidra refuses project paths that contain a folder starting with `.`** (`.pdtools`). The project lives in `%USERPROFILE%\.pdtools\ghidra-proj\` and is opened through the junction `%USERPROFILE%\pd-ghidra-proj` (created with `New-Item -ItemType Junction`).
- Import and auto-analysis (done once, ~4 min): `analyzeHeadless.bat %USERPROFILE%\pd-ghidra-proj PDUWP -import <game_files\PDUWP.exe> -max-cpu 6`
- Decompile everything to text (~3 min, 19,208 functions, 0 failures): `analyzeHeadless.bat %USERPROFILE%\pd-ghidra-proj PDUWP -process PDUWP.exe -noanalysis -readOnly -scriptPath engine\re -postScript ExportDecomp.java %USERPROFILE%/.pdtools/re-out` → `decomp_all.c` (16 MB) + `functions.txt`.
- Our helpers in `engine/re/`: `ExportDecomp.java` (the export above), `decomp_fn.py` (print a function by address, or list the functions that contain some text), `action_table.py` (print a skill's action-table rows: animation, fire tick, sounds, dash window).
- Address arithmetic: image base 0x140000000; `.rdata` file offset 0x2D1A00 = RVA 0x2D3000; `.data` file offset 0x3D1E00 = RVA 0x3D4000.

## Where things are in the program

| What | Where | Status |
|---|---|---|
| Logic rate: **30 Hz fixed step** | `FUN_1400b1bc0` (PDUWPMain constructor) sets the step timer to fixed-step mode with a target of 333,333 × 100 ns = 1/30 s (double at `0x1402f7f70`). Dozens of timers also add 1/30 per update (float at `0x1402f7ca0`) | ✅ |
| Animation clock: 1 key frame per 30 Hz tick | `FUN_140257510` adds the clip's speed (1.0, set on every clip start) to the clip time once per player update `FUN_14024ec80`, which runs once per 30 Hz step (phase 2, "Animation playback rate") | ✅ **30 fps. Our viewer and game play clips at 24 fps** |
| Skill table in memory | loaded by `FUN_14017da90`/`FUN_140196ee0` (checks the size 0x44004); parsed by `FUN_14017dcb0`. Records are 0x90 bytes from file offset 0x18, so a record starts at our **s2** (in-memory offset `o` = our s(o/2 + 2)). Pointer `DAT_1404bdc50`; skill id → record index table `DAT_1404bdc60` | ✅ (checked: +0x52/+0x54 = s43/s44, and s43 = 2 & s44 = 7 is tested exactly where the automatic Shelter is handled) |
| Action table | file offset 0x1B00C, 574 rows × 0x72 bytes (57 i16), pointer `DAT_1404bcc98`; a skill's **s12** picks its row | ✅ |
| Animation names | a pointer table at `0x1402db400`, 136 entries, `bas01::WAIT0` … `atk45::ATK_LASER` … `bas86::EXMOT5`. Index = our `ANIM_NAMES` index (64 ATK_SORD, 65 ATK_SHOT, 68 ATK_SKYSHOT_LOW, 76 ATK_LASER) | ✅ |
| Angles | 32,768 units per full turn (rad × 5215.19, float at `0x1402f824c`) | ✅ |

### Dispatch map (how a skill picks its behaviour) ✅

1. **Button press** → `FUN_140258180`: stores the record index, picks the dash target (below), and chooses the cast type: **10** normal, **12 (0xC)** short-range (motion 3/12/13/14, `FUN_140167910`), **11 (0xB)** mid-air/alternative. 11 is used when s40 = 1 or 3, or when s40 = 2 and the caster is in a jump/fall state (5 or 7).
2. **Cast start** → `FUN_14025a3e0`: action row = s12 (s13 for type 11). It plays the row's effect (slot 0) and cast sound, picks a battle voice, starts the row's animation, and sets the fire countdown and the state timer. The state timer is the clip length.
3. **Every tick** → player state switch `FUN_140256e80`: type 10/11 → `FUN_14025bd40` (count down, keep turning to face the target at the character's turn rate); type 12 → `FUN_14025be40` (wind-up, then the dash). `FUN_14025c720` counts the fire countdown down and, at 0, **fires**.
4. **Fire** → `FUN_1402626d0` → `FUN_140263a00` creates an *attack object* (pool of 48 × 0x1F0 per player). Its handler pair (init, update) is chosen by **s63 (motion)** from the table at `0x1403fb240` (24 bytes per motion), **if s43 = 0**. Otherwise it is chosen by s43 from `0x1403fb190`.

| s63 | init / update | s63 | init / update |
|---|---|---|---|
| 0 shell | `140264570` / `140264710` | 11 [Object] | `140268ee0` / `1402694e0` (+ `140269a50` draws it) |
| 1 [Parabola] | `140264b00` / `140265080` | 12 sword | `140269c60` / `140269db0` |
| 2 [Fall] | `140265470` / `1402657f0` | 13 punch/kick/wave | `14026a020` / `14026a0d0` |
| 3 (short-range) | `140265d60` / `140265e00` | 14 pursuit | same as 3 (no skill uses 3 itself) |
| 4 | `140265ea0` / `140265f50` | 15, 16 [Hole] | `14026a510` / `14026a7f0` |
| 5 [Crawl] | `140266060` / `140266290` | 17 multi-laser | `14026af40` / `14026b240` |
| 6 [Arc] | `1402666b0` / `140266bf0` | 18, 19 | none (empty) |
| 7 [Rain] | `140266fb0` / `140267410` | 20 [Rain] | `14026b480` / `14026bbd0` |
| 8 mines | `140267b90` / `140267e90` | s43 ≠ 0 | `0x1403fb190` + 24 × s43 (1 → `140263fc0`, 2 → `140264020`/`140264340` (defence guard; not auto-created by Ghidra, decompile with `DecompAt.java`), 3–5 → `1402642a0`/`1402642d0`/`1402642f0`; 1, 3, 4, 5 act once at the fire tick) |
| 9 around self | `140268050` / `140268320` | | |
| 10 [Move] | `140268590` / `140268940` | | |

Shared per-tick code: `FUN_14026e720` (count the lifetime down; at 0 the object ends silently), `FUN_14026cd70` (end / impact: which effect slot and sound). Collision and hits: `FUN_14026c1c0` (swept sphere, projectiles), `FUN_14026c5b0` (sphere), `FUN_14026c730` (sword blade), `FUN_14026c340` (punch sphere), guards `FUN_14026dcb0` → `FUN_140271760`, hit `FUN_14026f870`, stage objects `FUN_140274b30` — read in phase 2 (below).

### Action table columns (w = i16 index in the 0x72-byte row)

| w | Meaning | Status |
|---|---|---|
| 0 | 1 = row in use | ✅ |
| 2 | effect number (eftNNNN), played as slot 0 at cast start | ✅ |
| 3 | **cast sound id** (`common` bank), played at cast start at the caster; for most rows = row + 200 (Laser 242, Slide Laser 243, Blaster 253, Psycho Knife 203) | ✅ (the call passes bank 0; that bank 0 = `common.bin` is ⚠️) |
| 4 | battle-voice group 1–6: the voice id is drawn at random from a 10-entry list per group (i32 table at gsdata offset 0x33004 + 4·(10·(group + 9) + n)). Group 1 gives mostly voices 10–12, 3 gives 30–32, 6 gives 60–62 | ✅ lists; what each group sounds like ❓ |
| 6–25, 26–37 | controller rumble at cast / at fire (kind, strength, delay, length pieces; `FUN_14025a8a0` / `FUN_14025aad0` → `pdvkFeedBack`) | ✅ (not gameplay) |
| 38 | **animation index** (motion-name table) | ✅ |
| 39 | charge cycle in ticks (hold-to-charge skills: 13) — one charge step per cycle while held | ⚠️ |
| 40 | animation start frame on charge release (15) | ✅ read in `FUN_14025c720` |
| 41, 42 | short-range: **dash starts / ends** (ticks after cast start) | ✅ |
| 43 | **fire tick** — ticks from cast start to the attack object being created | ✅ |
| 44 | short-range: hit window ends (the sword object lives w44 − w43 ticks) | ✅ |
| 48 | combo offset (subtracted from the fire tick on later hits of a combo) | ⚠️ |

Fire-tick spread over the rows of all 399 skills (every capsule colour): 0 (17), 2 (49), 4 (13), 13–15 (34), 18–20 (131), 25 (97), 30 (33), 40 (1). The most common values are 20 and 25 ticks (0.67 / 0.83 s), not our flat 0.4 s.

## Units, settled

| Quantity | Program | Our guess (moves/mod.rs) |
|---|---|---|
| Time | 1 table tick = 1 logic tick = 1/30 s ✅ | same ✅ |
| Distance | s × 0.1 u for reach, blast (s52), hole radius, mine radius ✅; **shell range s64 < 0 → −s64 × 10 u** ✅ | 0.1 u ✅; range not used |
| Speed (shell, arc, object) | s × 1/10.8 u per tick = **2.78 u/s per point** (float 0.0925926 at `0x1402f7cd8`) ✅ | 3 u/s (8 % fast) |
| Homing (shell s68, arc s66) | s × 0.1° per tick × (homing %/100) = **3°/s = 0.052 rad/s per point** ✅ | 0.1 rad/s (1.9× too strong) |
| Arc angles s67, s68 | degrees ✅ | degrees ✅ (s68 used differently, below) |
| World units | assumed the same as ours (ALR units, 21 per character height) — the 0.1-scaled values give the sizes we already tuned by feel | ⚠️ |

## The three moves

### Laser / Slide Laser (s63 = 6 [Arc]) — Laser id 65 row 42, Slide Laser id 66 row 43

- **Timeline** ✅: cast start → slot-0 effect (eft0042 / eft0043) + cast sound 242 / 243 + voice group 1 + clip ATK_LASER (34 frames). **Fires at tick 13 = 0.433 s.** The caster stays in the cast state for the whole clip, 34 ticks = 1.13 s ⚠️ (clip length taken as frames at 30/s). While casting it keeps turning to face the target at its normal turn rate. Mid-air (s40 = 2 → rows 305 / 306): clip ATK_SKYSHOT_LOW, fires at tick 20 = 0.667 s.
- **Flight** (`FUN_1402666b0` init, `FUN_140266bf0` update) ✅:
  - speed = s65 / 10.8 per tick: Laser 12.96 u/tick = **389 u/s**, Slide Laser 14.8 u/tick = **444 u/s**.
  - The shot leaves rotated **s68° about its own forward axis** (both 20°: the arc's plane is tilted) and then **s67° sideways** in that tilted frame (Laser 20°, Slide Laser 40°). So a Slide Laser is the same move with a wider, faster swing.
  - Radius R = distance to target / (2 sin s67); it turns at speed/R per tick for (2·s67)/(turn rate) ticks, which brings it back onto the target. Then it flies straight for **30 more ticks (1 s)** and ends. Lifetime = curve ticks + 30.
  - **Mild homing**: while curving, every tick it re-aims its circle at the target's *current* position, turning at most s66 × 0.1° per tick × homing %: Laser 0.8°/tick (24°/s), Slide Laser 1°/tick (30°/s).
  - s64 (−80) is not read by the arc code.
  - Hit radius: 5 u default, set when the object is created (`FUN_140263a00`) ⚠️.
- **Effects** ✅: slot 0 at the caster at cast start; slot 1 (beam) created with the attack object at fire and moving with it; slot 2 at the end point on impact (`FUN_14026cd70`, impact cases 1–3); slot 3 on the victim when hurt (`FUN_140259310`).
- **Sounds** ✅: tick 0 cast sound 242 / 243; at fire, s20 = 603 (the loop that travels with the shot) and s19 (0 here); on impact, s21 = 509 at the impact point.

### Blaster (s63 = 0 shell) — id 80, row 53

- **Timeline** ✅: effect eft0053 + sound 253 + voice group 3 + clip ATK_SHOT (45 frames). **Fires at tick 20 = 0.667 s**; locked in the cast for 45 ticks = 1.5 s ⚠️. No mid-air row (s13 = 0, s40 = 0).
- **Flight** (`FUN_140264570` / `FUN_140264710`) ✅:
  - speed s65 = 140 → **389 u/s**. If s67 ≠ 0 the speed ramps *linearly* from s65 to s66 over s67 ticks. That covers Psycho Sniper / Dragon Bullet: they speed up from 0, rather than waiting and then flying as we built them. Blaster has s67 = 0: constant speed.
  - **Range**: s64 = −30 → 300 u; lifetime = 300 / 12.96 = **23 ticks (0.77 s)**, then it ends (`FUN_1402636f0`). If s64 ≥ 0 it is the lifetime in ticks.
  - **Homing**: s68 = 15 → at most 1.5°/tick (**45°/s**), and **only while the target is within 60° of its heading** (dot > 0.5). Outside that cone it flies straight.
- Effects and sounds as for the laser (loop 604, impact 509).

### Psycho Knife (s63 = 12 sword) — id 3, row 3

- **Timeline** ✅ (ticks from cast start, 30 Hz): clip ATK_SORD (40 frames), effect eft0003, sound 203, voice group 1.
  - 0–11: wind-up, standing, turning to face the target (character turn rate).
  - 12–21: **dash** (w41..w42), only if a target is locked on (lock state 1; otherwise no dash) ⚠️ what state 1 means exactly.
  - 18: attack object created (w43); **hit window ≈ 18–22** (it lives w44 − w43 = 4 ticks) = 0.6–0.73 s.
  - 40: back to idle (clip end, 1.33 s) ⚠️.
- **Dash** (`FUN_14025be40`, numbers from `FUN_14018f050` / `FUN_14018f0f0`) ✅:
  - speed per tick = s65 × 10 / (dash ticks) = 8 × 10 / 10 = **8 u/tick = 240 u/s** (capped at 10 u/tick), so the most it can cover is 80 u.
  - It stops **11 u** from the target's centre (float at `0x1402f7f90`). When less than one step is left, the program shortens the last step to speed − (distance − 11), so it can end slightly inside 11 u (read as written; it looks like a quirk).
  - It turns toward the target by at most s66/10 = **1° per tick**.
  - s67 (0 here) sets a flag that is not used in this move ❓.
- **Hit** (`FUN_140269c60` / `FUN_140269db0`) ✅: a sphere of radius **s64 × 0.1 = 13 u** that follows a matrix owned by the caster (the weapon / hand) ⚠️ which one. It hits during the window whether or not the dash arrived.
- Effects: slot 0 at cast start; slot 1 (the blade trail) on the sword object from tick 18; slot 3 on the victim.

## Validation check: "lasers fire ~1.26 s after the cast"

**Refuted.** The program fires Laser and Slide Laser **13 ticks = 0.433 s** after the cast starts (20 ticks = 0.667 s in mid-air). Our sim's 0.40 s is only one tick early.

- The 1.26 s came from sound `common/305`, which we had read as "the laser launch sound" because the lasers' s13 is 305. In the program **s13 is not a sound**: it is the mid-air action row, and row 305 is the lasers' mid-air cast. Sound 305 is the cast sound of **row 105 = Fire Storm** (a 2.5 s rain skill with a late burst, which explains the shape we saw). The Laser's cast sound is `common/242` (0.97 s).
- The laser charge-effect rings ending at frame 37–42 fit the effect playing on (0.8–1.4 s) after the shot leaves. Or effect frames run faster than 30/s. ❓ Footage will tell.

## What we do vs what the program does

| | Ours (pd_game today) | Program |
|---|---|---|
| **Laser** fire time | 0.40 s (`CAST_TICKS` 24 @ 60 Hz) | 0.433 s (13 ticks); mid-air 0.667 s with ATK_SKYSHOT_LOW |
| Laser caster locked | until fire (0.4 s) | whole clip, ~1.13 s ⚠️ |
| Laser speed | 420 / 480 u/s (Laser / Slide) | 389 / 444 u/s |
| Arc shape | flat circle in the ground plane, bend s67 | same bend, but the plane is **tilted by s68 = 20°** |
| Arc homing | none (aims at where the target stood) | re-aims its circle at the target each tick, ≤ 24°/s (Laser) / 30°/s (Slide) |
| After the arc | straight for 300 u | straight for 30 ticks (≈ 390 / 440 u) |
| **Blaster** fire time | 0.40 s | 0.667 s (20 ticks); locked ~1.5 s ⚠️ |
| Blaster range | 900 u (engine constant) | **300 u** (s64 = −30), 0.77 s |
| Blaster homing | 1.5 rad/s (86°/s), always | 45°/s, only within ±60° of its heading |
| Sniper-type shells | wait s67, then fly at s66 | speed ramps from s65 to s66 over s67 ticks |
| **Psycho Knife** start | dashes at once, 260 u/s, up to 0.6 s (156 u) | stands 0.4 s turning, then dashes 0.33 s at 240 u/s (≤ 80 u), stops 11 u from the target |
| Knife strike | on arrival, at 13 u + body radius | fixed window 0.6–0.73 s, 13 u sphere on the weapon; whiffs if out of reach |
| Knife recovery | 0.2 s after the strike | clip end at 1.33 s ⚠️ |
| Dash without lock-on | always dashes | no dash |
| Cast animation | hand table (`main.rs::cast_clip`) | action row w38 (same three clips here: ATK_LASER, ATK_SHOT, ATK_SORD) |
| Cast sound | s8 + s13 + s19 | action row w3 at cast; s19 + s20 loop at fire; s21 at impact |
| Animation rate | 24 fps | 1 frame per 30 Hz tick ⚠️ |
| Shell / arc speed unit | 3 u/s per point | 2.78 u/s per point |
| Homing unit | 0.1 rad/s per point | 3°/s (0.052 rad/s) per point, scaled by homing % |

## What Laurence's footage should check

Record at 30 or 60 fps from the original; count frames from the button press.

1. **Laser**: beam leaves ~13 frames (at 30 fps) after the press; the whole cast pose lasts ~34 frames (if ~42, clips run at 24 fps and the ⚠️ above is wrong). Mid-air cast: ~20 frames, different pose.
2. **Laser arc**: which way it swings out (left or right of the line), and whether it also rises or dips a little (the 20° tilt). Against a strafing target, does the arc bend after them a bit?
3. **Slide Laser**: same timing, visibly wider swing than Laser.
4. **Blaster**: leaves ~20 frames after the press; against a far target (more than ~14 body heights away) it should **vanish before reaching them**. Does it turn after a target that runs sideways, but not after one behind it?
5. **Psycho Knife**: a clear pause (~12 frames) before the lunge; lunge about 4 body heights at most; the hit lands ~18–22 frames after the press, even if the lunge fell short. Without lock-on: no lunge.
6. **Sound**: the laser cast sound should match `cache/sfx/common/242.wav`, not 305.
7. **Charge glow vs shot**: does the Laser's charge glow keep going after the beam has left?

## Phase 2 (2026-09-27): every motion handler and the shared rules

Same rules as the pilot: logic described in our own words, addresses given, raw exports outside the repo. Numbers below were read from the code, then checked against `skills.json`, the action table and the new tables. "Tick" = 1/30 s. Speeds: 1 point = 1/10.8 u per tick = 2.78 u/s. Distances: 1 point = 0.1 u unless said otherwise. Turning: 1 point = 0.1° per tick, times the caster's homing % (status 2, normally 100 %).

### New tools (engine/re/)

- `DisasmRange.java` — print Ghidra's disassembly for address ranges (needed where the decompiler loses an argument or a jump table). Same headless call as the export, with `-postScript DisasmRange.java <out.txt> <hexaddr>:<count> …`. Run it from PowerShell (Git Bash mangles the `-scriptPath` backslashes).
- `DecompAt.java` — decompile functions Ghidra's auto-analysis never created (the defence handlers `140264020` / `140264340` are only reached through the handler table). Read-only; nothing is saved to the project.
- `action_table.py` — now has the program's full 136-name motion table and prints the hidden fields **s70–s73** (below) and s44, s57–s62.
- `state_table.py` — prints the **hit-reaction table** (gsdata offset 0x2B00C, below).

### Things that change how the whole table is read

- **Hidden fields s70–s73** ✅. The program's record is 0x90 bytes from file offset 0x18, so its last 8 bytes are the first 8 bytes of the *next* record in our framing (our "u32 always 0" + s0 + s1). That u32 is **not** always 0: 19 records use it. Read as i16: s70, s71 = low/high half of the next record's u32, s72 = next s0, s73 = next s1. Used by: Double / Triple / Octolaser (s71 = −150 / −80 / −32: per-shot tilt step), Ricochet Laser (4, 0, 90, 3), Impact Burst (…, 1), the nine motion-20 Rain skills (300, 15, 90, 0), Power / Psycho Kinesis (0, 100, 0, 0). `skills.json` does not carry them yet (`action_table.py` shows them).
- **Attack s44 = range band** ✅ (`FUN_140273d40`, used by the aim helper `FUN_14026e410`). Parabola, Fall, Arc, both Rains and Hole aim at the target's centre, but the aim point's distance from the spawn point is clamped: **s44 = 0 → 80–300 u, s44 = 1 → 300–600 u, s44 = 2 (and every motion-16 Hole) → no limit**. When the aim is clamped the target is dropped: no homing or steering, the move goes to the clamped point. With no target it aims 190 u ahead (s44 = 0) or 300 u (s44 = 1). This is the guide's "short / mid / long range" that no field was found for. Lasers (s44 = 1) therefore **cannot be aimed closer than 300 u**: against a target 150 u away a Laser's circle closes at 300 u and passes ~26 u to the side of the target at 150 u (sagitta of a 20° arc over a 300 u chord). Motion 4 (instant effects) uses the same field: s44 = 0 needs the target within 300 u (`FUN_140273c80`).
- **Multi-shot and spawn points come from the cast effect** ✅ (`FUN_14018a310`). At the fire tick the program scans the caster's slot-0 cast effect (eftNNNN) for items named **`SHOT01`, `SHOT02`, …** (names packed in base 40, the packed-name scheme FORMATS-EFFECTS already knows; the check is `(name & 0xFFFFFF00) == 0xB3D8C500`). One attack object is created per marker, spawned at the marker's position and orientation (`FUN_1402626d0`). So "how many shots and where from" lives in the effect files, not in the skill table. s69 (Arc) / s70 (Ricochet) only pick which per-shot angle step each shot gets.
- **Shot index k**: each shot's index is the caster's running shot counter (fighter +8, never reset) plus its marker index, taken modulo the shot count; the set of angles fired per cast is always the full set.
- **Owner immunity**: every handler clears the caster's bit in the object's hit mask at init — except mines (a Sensor Mine is set off by anyone, and every mine blast hurts the owner), blasts that start on a wall or floor (`FUN_14026dbd0` with no target sets all four bits: a Bomb that hits a wall hurts its thrower), and Tornado after its first 30 ticks.
- **One hit per object per player** ✅ (`FUN_14026ecc0` clears the victim's bit). The drops of one motion-20 Rain cast share it: once any drop hits a player, no other drop of that cast can (**Rain lands once per cast — our choice was right**).
- **Objects that pass through players** (flag 0x20: melee, Rain column, Rain cloud, Hole, Tornado) keep going after a hit; everything else ends on its first hit (impact code 2) or on a wall/floor (code 1).
- **Walls and line of sight** (`FUN_14026ca50`, `FUN_1401641b0`): every moving object tests the segment from last tick's position to this tick's against stage collision; a hit ends it at the contact point (slot-2 effect, s21 sound, blast if s51 = 5). **s55 = 3 ("Penetrates obstacles")**: no wall test and no line-of-sight test. s55 = 2: wall contacts are ignored ⚠️ (only Giga Ruin, Particle Mortar-type). s55 = 1 (Ricochet Laser): a different collision query ⚠️. Body hits also need a clear line from the object's origin to the target unless s55 = 2 or 3.
- **Online authority flags** (`FUN_14026e860`, jump tables read with DisasmRange at `14026ebd8`…`14026ec38`): in network modes (game mode 4/5) flags 0x800/0x1000/0x2000/0x4000/0x8000 decide which machine runs hits, sends packets (`FUN_14026da70`) and ends the object. Offline every object gets 0x200 | 0x400 | 0x800 | 0x8000: all hit tests run locally. Useful later for netcode; ignore for split-screen.

### Impact codes (`FUN_14026cd70`) ✅

| Code | Meaning | Effect slot / sound |
|---|---|---|
| 0 | lifetime over (silent) | — |
| 1 | hit a wall or the floor | slot 2 at the contact point; s21; s51 = 5 → blast that hurts everyone incl. the caster |
| 2 | hit a player | slot 2; hit sound 6 or 7; blast (s51 = 5) skips the player already hit |
| 3 | stopped by a guard | slot 2; blast skips the guard's owner |
| 5 / 6 | sent back by Reflect / About Face | object re-aimed at its owner, slot 1 re-created |
| 7 | brushed | melee attacker → reaction row 26; projectile → turned away, target dropped, 60 more ticks |
| 8 | mine goes off | blast (hurts everyone incl. the owner) |
| 9 | guard held (on the guard object) | slot 4; the guard lingers 5 more ticks, then ends |
| 10 | guard broken | slot 5; sound 14 |

**Blasts** ✅ (`FUN_14026dbd0` → update `FUN_14026ae10`): a separate object that lives 30 ticks; its sphere **grows linearly from 0 to the full radius over those 30 ticks**, tested every tick (sphere vs body radius, line of sight from the blast centre). Radius: s52 × 0.1 for "Explodes on impact" (Bomb 30 u, Big Bomb 40 u), s64 × 0.1 for motion 9, s67 × 0.1 for mines. Sound 99 at the blast.

### Per-handler reference, most-used first

Counts are skills using the handler (collectable and enemy, all capsule colours). "Row" = action-table row (s12).

#### Motion 13 — punch, kick, palm, wave (29: 24 attacks + 5 others) — `14026a020` / `14026a0d0`

- **Timeline** ✅ (action rows; clip lengths in frames = ticks):

| Clip (examples) | dash w41–w42 | hit window w43–w44 | clip end |
|---|---|---|---|
| ATK_SPHERE (Psycho Wave row 1, Aura Leak, Ki Palm) | 10–20 | 20–22 | 40 (1.33 s) |
| ATK_PUNCH (Empty Hand row 13, High-speed Punch) | 10–20 | 20–22 | 30 (1.0 s) |
| ATK_TUKI (Dragon Punch, Splashdown, Swift Blow) | 10–20 | 20–22 | 35 |
| ATK_KICK (Tiger Kick row 16, Leg Throw, Hyperkick, Swift Kick) | 11–23 | 20–22 | 35 |
| ATK_BODY (Hyperpunch, Dance of Death, Swift Punch, Vacuum Palm) | 12–22 | 20–25 | 45 |
| ATK_DRAIN (Dhampir Claw/Fang, Succubus ×2, Fusion, Annoying Gift) | 15–25 | 25–27 | 45 |
| ATK_SPHERE (Pursuit Stopper row 396) | 1–20 | 20–22 | 40 |

- **Dash**: as the pilot's sword dash (`FUN_14025be40`): s65 = 8 → at most 80 u over the dash window, needs a lock-on, stops 11 u from the target's centre; **s66 = 30 → turns up to 3° per tick** (swords 1°). Pursuit Stopper s66 = 300 (30°/tick).
- **Hit** ✅: a sphere of radius **s64 × 0.1 = 13 u** (every motion-13 skill) plus the target's body radius, centred on a point that follows the caster: the `SHOT01` marker of the caster's running cast effect (fighter +0x13b20), else a caster matrix (fighter +0x13db8) ⚠️ which bone. Re-read every tick of the window (2–5 ticks). Line of sight from the caster's chest (position + height offset).
- Effects: the slot-1 effect is drawn 20 u in front of the caster's chest (scaled by radius × 0.2) while no wall is in between ⚠️ (read as written; the hit test itself uses the attached point).
- Reactions: punches 4 / 5 / 5, kicks 60 / 61 / 61, waves 58 / 59 / 59, drains 8 / 9 / 9 (see "Hit reactions").

#### Motion 12 — sword (19) — `140269c60` / `140269db0`

- **Timeline** ✅: ATK_SORD rows (Psycho Knife 3, Laser Blade, Mist Blade, Lightning/Ice/Flame Sword, Knife Grind, Shield Breaker, Level Baton): dash 12–22, hit 18–22, clip 40. **ATK_WHAND_SORD rows (two-handed: Excalibur 4, Psycho Blade, Blazing Sword, Vacuum Slash, Muramasa, True Muramasa, Excaliber, Frostbite): dash 17–30, hit window 14–34 (20 ticks, starting before the lunge), clip 54 (1.8 s).**
- **Hit** ✅ (`FUN_14026c730`): not a sphere but a **blade**: a segment from the attached point along its forward axis, length **s64 × 0.1** (13 u; Laser Blade / Flame / Level Baton 15; Ice / Blazing / Frostbite 25; Excalibur / Mist Blade 30), tested against the target's centre with a **fixed 12 u radius**, for both last tick's and this tick's blade (so a fast swing can't skip through). Line of sight from the caster's chest.
- Dash s66 = 10 → 1°/tick (pilot).

#### Motion 1 — [Parabola] lob (13) — `140264b00` / `140265080`

- **Aim**: target's centre through the range band (all s44 = 0 → 80–300 u, except Throwing Stone / Trench Mortar / Particle Mortar s44 = 1 → **300–600 u: mortars can't hit anyone closer than 300 u**).
- **Speed**: horizontal **s64 / 10.8 per tick** (s65 if s64 = 0): Bomb 90 → 250 u/s, Big Bomb 40 → 111 u/s, Throwing Stone 140 → 389 u/s, Gungnir 80 → 222 u/s. Flight time T = horizontal distance / speed.
- **Arc height** ✅: **s66 × 10 u above the straight line** at mid-flight: gravity g = 8 · (s66 × 10) / T², launch vertical speed = g·T/2 + height difference / T. Throwing Stone 10 u, Psycho Spear 20 u, Bomb 40 u, Big Bomb / Trench Mortar 50 u, Particle Mortar 70 u, Delay Bomb 100 u. (FORMATS-SKILLS had s66 as steering and s67 as gravity: the other way round.)
- **Steering** ✅ (`FUN_14026bf50`): each tick it predicts where the lob will come down at the target's current height and slides sideways toward the target by at most **s67 / 10.8 × homing % per tick** (Bomb 10 → 28 u/s, Psycho Spear 20 → 56 u/s, Gungnir 30 → 83 u/s). Once it is falling below the target's height it keeps its last correction.
- Lifetime T + 30 ticks; ends on the floor/wall (code 1) or a hit. Bombs blast (s52).
- Timeline: ATK_THROW rows fire at tick 20 (clip 40); mid-air rows ATK_SKYSHOT_LOW tick 20; Trench Mortar ATK_LASER tick 13; Gungnir ATK_SPIKE tick 25 (charge skill, w39 = 13).

#### Motion 5 — [Crawl] (10: 8 attacks + Entangle, Curse of Earth) — `140266060` / `140266290`

- Starts at the spawn point, heading = the caster's facing flattened to the ground.
- **Speed** ramps s65 → s66 over s67 ticks (as shells): Crawlers / Venom Fang / Spore Schism 30 → 83 u/s; Root / Fang of Tree 60 → 167 u/s; Crawling Bullet 110 → 306 u/s; Ground Laser 0 → 80 over 90 ticks; Young White Dragon 15 → 42 u/s.
- **Range** s64 (−80 → 800 u; Crawling Bullet −30 → 300 u).
- **Steering** ✅: only while touching the floor. Turning *accelerates*: each tick the turn rate grows by **s68 × 0.01° × homing %** (500 → 5°/tick², 750 → 7.5, 800 → 8) up to **s69 × 0.1° per tick** (100 → 10°/tick = 300°/s), or snaps straight onto the target when it is closer than one step.
- **Off an edge it falls** (0.12 u/tick², re-snaps when it lands) rather than ending; it ends at a wall or when its range runs out ⚠️ (how the floor snap interacts with the wall test is not fully read).
- Hit: swept sphere 5 u + body radius, so a jumping target clears it.
- Timeline: ATK_GROUND rows fire at tick 25 (clip 45).

#### Motion 20 — [Rain] cloud + drops (9) — `14026b480` / `14026bbd0` (+ drop spawner `14026b8d0`)

All nine share s64–s73 = 30, 30, 6, 5, 0, 1000, 300, 15, 90, 0.
- **Cloud**: set **s69 × 0.1 = 100 u above the target's centre** (range band; Photon Shower s44 = 1 → 300–600 u) and then **follows the target at s71 / 10.8 = 1.39 u/tick (42 u/s)**, holding 100 u above them. Lives s64 + s65 = 60 ticks. Slot-5 effect on the cloud.
- **Drops**: first drop **s64 = 30 ticks (1 s)** after the cloud appears, then one every s67 + 1 = 6 ticks, **s66 = 6 drops** (the last may be cut by the cloud's 60-tick life ⚠️ off-by-one). Each drop starts at a random point within **s70 × 0.1 = 30 u** of the cloud, pointing down with a random lean of up to 10°, falls at **s72 / 10.8 = 8.3 u/tick (250 u/s)** for (100 + 30) / 8.3 ≈ 16 ticks; hit sphere 5 u. Slot-4 effect per drop. Uses C `rand()` — the sim needs its own seeded generator.
- Lands **once per cast** (shared hit mask, above). Walls/roofs stop drops (s55 = 3 on Lucifer's Arrow: through roofs).
- Timeline: ATK_SPIKE rows fire at tick 25 (clip 46), ATK_WHAND_SPIKE tick 30 (clip 53).

#### Motion 2 — [Fall] (6) — `140265470` / `1402657f0`

- **Not straight down** ✅: it appears **s64 × 10 u above and s65 × 10 u back toward the caster** from the aim point, and flies diagonally at the aim point: Meteor / Mini-Meteor / Demon's Venom 60 u up, 180 u back; Thor's Hammer 120 u up, 180 back; Arrow of Artemis / Sign of Saints 40 u up, 190 back.
- **Speed s66 / 10.8**: Meteor 80 → 222 u/s, Arrow of Artemis 100 → 278 u/s, Sign of Saints 60, Demon's Venom 40 (111 u/s).
- **Steering**: while the target is still ahead, slides sideways (perpendicular to its flight) toward the target by at most **s67 / 10.8 × homing %** per tick (15 → 42 u/s; Mini-Meteor 10).
- Lifetime: twice the flight distance (it carries on into the ground). Range band s44 = 0 (80–300 u). Thor's Hammer s55 = 3 ignores roofs.
- Timeline: ATK_WHAND_SPIKE rows fire at tick 30, ATK_SPIKE tick 25.

#### Motion 9 — explode around self (6) — `140268050` (update = the shared blast)

- Becomes a blast at the spawn point (the cast effect's marker / caster) at once: radius **s64 × 0.1** (Photon Wave / Fast Wave 40 u, Photon Burst / Poison Blur 50 u), **growing from 0 over 30 ticks**, caster excluded. Slot-2 effect.
- Timeline: **fires at tick 3** (SPE_POWER_UP rows, clip 40); Poison Blur ENV_SET tick 25.

#### Motion 17 — ricochet / multi-laser (Ricochet Laser, Impact Burst + 3 enemy) — `14026af40` / `14026b240`

- Straight flight, speed ramp s65 → s66 over s67 (Ricochet 80 → 222 u/s; Impact Burst 40 → 200 over 80 ticks = 111 → 556 u/s). Lifetime s64 (≥ 0 = ticks: Ricochet 90 = 3 s, Impact Burst 180 = 6 s).
- **Per-shot direction** ✅ (disassembled, the decompiler lost the argument): roll about the line of fire by **s69 + k·s72** degrees, then yaw by **s68 + k·s71** degrees, k = shot index mod s70. Ricochet (−45, −45, 4, 0, 90): four shots each 45° off the line, in an X (up-left, up-right, down-right, down-left). Count = SHOTnn markers.
- **Bounces** ✅ (`FUN_14026ca50`): on a wall it reflects off the wall normal; bounce counter s73 (Ricochet 3, Impact Burst 1): when the counter reaches 0 and the target is on the open side of the wall, it turns **straight at the target** instead of reflecting. After that it keeps reflecting until its lifetime ends. It never ends on a wall.

#### Motion 15 / 16 — [Hole] (7 attacks + Pin Down, Disintegrate, Calamity) — `14026a510` / `14026a7f0`

- Aim at the target (range band: 15 → s44 = 0, 80–300 u; **16 → unlimited, and one hole is made at every player's feet** — `FUN_1402626d0` loops over all four players (`FUN_14026ede0`); **the caster's own hole DOES hurt the caster: the loop has no owner test, and LIVE takes agree (Tremor 147, Earthquake 312, Jack Pot 319, Overheat 86: P0 and P1 lose the same Health on the same tick; Massive Quake 148 -6 each; Calamity 283 floors both). Corrected 2026-10-02 (skilleffects lane); motion 15 (one hole at the target) not probed**).
- Dropped to the floor under the aim point (no floor → fails silently).
- **Warning** s64 = 30 ticks: the marker grows from 0 to full size and **slides along the target's running direction at the speed they had when it was cast** (target's position minus last tick's), sticking to the floor.
- **Active** s65 = 10 ticks (0.33 s): sphere of **s66 × 0.1 = 30 u** plus body radius at the floor, tested every tick, **only against players standing on the ground** (player +0x12e74).
- Timeline: ATK_GROUND rows fire at tick 25.

#### Motion 7 — [Rain] column (Lightning Bolt, Thunder Storm, Killer Satellite) — `140266fb0` / `140267410`

- Aim at the target (Killer Satellite s44 = 1 → 300–600 u). Warning **s64 ticks** (30; Killer Satellite 45), marker sliding along the target's run like the Hole; then **active s65 = 10 ticks**: a vertical column from the aim point up to **s66 × 10 = 60 u**, radius **s68 × 0.1** (Lightning 30 u, Satellite 40 u), swept every tick. On its first active tick it checks the line from the ground point to the top: a roof ends it (unless s55 = 2/3). Slot-2 effect replaces the marker then.
- Barrier and Reflect never stop it (see guard rules).

#### Motion 8 — mines (13: 4 attacks + 9 other colours) — `140267b90` / `140267e90`

- Set at the spawn point, dropped onto the floor (10 u probe up/down). Timeline: ATK_SET_BOMB rows **fire at tick 4** (clip 22).
- **s64 = 0 Timer**: goes off after **s66** ticks (Timer Mine 150 = 5 s ✅ note; Fast Bomb 20).
- **s64 = 1 Sensor**: armed after s66 = 60 ticks (2 s), then goes off when **anyone** (owner included) has their centre within **s65 × 0.1 = 30 u** with a clear line (`FUN_14026e340`).
- **s64 = 2 Remote**: goes off on the owner's next press (`FUN_14026cd70(…, 8)` from the button code).
- Blast radius **s67 × 0.1 = 50 u** (Remote 30 u), growing over 30 ticks, **hurts everyone including the owner**.

#### Motion 10 — [Move] Tornado (Twister, Whirlwind, Tornado) — `140268590` / `140268940`

- Placed on the floor at the spawn point, **moves straight along the caster's facing — no homing at all** ✅.
- Speed ramps s67 → s68 over s69 ticks: Twister 100 → 0 over 60 (starts 278 u/s, stops after 2 s); Whirlwind 40 → 0 over 90; Tornado 0 → 100 over 90 (starts still, speeds up). Lifetime s64: 90 / 150 / 90 ticks.
- Column radius **s65 × 0.1** (10 / 20 / 10 u), height **s66 × 10 − 2** (98 / 118 / 98 u) or up to the ceiling. Hits everything in the column (swept), passes through players.
- Follows the floor within ±7.5 u; no floor → ends; a face crossing its column at the new spot puts it back where it stood (it stays alive). No sideways wall ray (READ 2026-10-09 round 4, below).
- **Can hit its own caster after its first 30 ticks** ✅.
- Reaction 45: a straight-up launch (see reactions).

#### Motion 11 — [Object] (Power, Psycho Kinesis) — `140268ee0` / `1402694e0` / `140269a50` (draws the object)

- Picks the nearest throwable stage object (`FUN_14012e160`; none → nothing happens). Hit radius = half the object's bounding size; the object's own value is **added to the STR** (object +0x60).
- Lifts **s68 × 0.1 = 10 u over s69 = 30 ticks**, then flies at s65 → s66 over s67 (80 → 222 u/s), range s64 (−30 → 300 u), turning toward the target only while it is within 60° of its heading (turn cap from s71 = 100 → 10°/tick ⚠️ argument not confirmed).

#### Motion 14 — Pursuit, Sky Pursuit — uses motion 3's handler `140265d60` / `140265e00`

- ATK_STAMP rows 21/22: dash ticks 3–21, **one hit test at tick 19**: a sphere of **s64 × 0.1 = 25 u** plus body radius around the caster's centre, against the locked target only, then it ends.
- Lands only on a target that is **knocked down and on the ground** (states 17/18, `FUN_14026f870`); reaction 24 (pinned).
- Motion 3 itself: no skill uses it.

#### Motion 4 — instant effects (22 non-attack skills) — `140265ea0` / `140265f50`

- No travel. s64 = 0: needs the target within 300 u (else nothing happens); s64 = 2: **everyone** (all four players, caster included — the effect code decides who is affected). Takes effect **s65 ticks after the fire tick** (25, or 5 for Frighten / Paralyze Barrier / Interference).

#### Motions 18, 19 — no handler

Only enemy rows use them (Body Attack, Egg Bomb, Dust Cannon…): as attacks they do nothing.

#### Arc (6) and shell (0) additions to the pilot

- **Arc multi-shot fan** ✅: shot k's tilt = **s68 + k·s71**, bend = s67 + k·s70. Double Lasers: tilts −15° and −165° (a mirror pair: one swings out left, one right). **Triple Lasers: −10°, −90°, −170° — left, over the top, right.** Octolaser: 22° − 32°·k for k = 0…7 (22 down to −202°: a fan of eight planes over 224°). Bend −25° for all three.
- **Arc range band** ✅: s44 = 1 on every Optical laser → aim clamped to 300–600 u; target dropped (no homing) when clamped. Sonic Boom, Arc of Fire, Piece of Wind (s44 = 0): 80–300 u.
- **Course deflection on shells** ✅: after passing a [Course] field (flag 0x100), a shell's homing turns it *away* from the target at its normal homing rate (while the target is still within its 60° cone). Lobs, falls and crawlers do the same with their own steering.

### Non-attack capsules (s43 ≠ 0)

| s43 | Handler | What happens at the fire tick |
|---|---|---|
| 1 (erase field) | `140263fc0` → `FUN_14026fcb0` | applied once, then the object ends |
| 2 (defence) | `140264020` / `140264340` | a guard object (below) |
| 3 (capsule-targeted special) | `1402642a0` → `FUN_140270200` | applied once |
| 4 (self status / special) | `1402642d0` → `FUN_140270890` | applied once |
| 5 (environment) | `1402642f0` → `FUN_14017bcc0` | sets the environment crystal at the caster's feet |
| 6 (Recollection / Copy) | none (resolved when the button is pressed) | — |

The effect code of 1, 3, 4, 5 was not read line by line; our builds follow the notes and the program gives no timing beyond "at the fire tick".

**Defence guard object** ✅ (`140264020` init, `140264340` update):
- **Duration ≈ s46 − 1 ticks** (its countdown starts at s46 − 2): Crystal Wall 23 → 22 ticks ≈ 0.73 s; Brush 18 → 0.57 s; automatic Shelters 10 → 0.3 s. The automatic Shelter (s44 = 7) doesn't count down on its own ⚠️ (ends another way).
- **Size = s45 × 0.1** (13 u; [Course] 800 → 80 u; one 300 → 30 u).
- **DEF = s47 + s48 × (times this slot has been used)** (`FUN_140272400`): Failing Wall 7 − uses, Dark Protection 3 + uses, Protecting Air 3 − uses.
- **Placement**, re-done every tick so it follows the caster: Barrier (0), Brush (3), Reflect (4), Absorb (5) sit **body radius + 5 u in front** of the caster, facing forward; Shelter (1), Course (2), About Face (6), automatic Shelter (7) are centred on the caster.
- **Contact** (`FUN_14026dcb0`): the front kinds are a **disc** of radius s45 × 0.1 facing forward — an attack must cross the disc's plane within disc radius + its own radius. Shelter, Course, Absorb, About Face are a **sphere around the defender's centre** of radius s45 × 0.1 + the attack's radius. (So Absorb stands in front for drawing, but its contact test is all-round ⚠️; the resolution below still limits what it takes.)
- Melee uses the caster-to-hit-point segment; blasts use the segment to the defender's centre; projectiles their last-tick-to-now segment. **Projectiles are checked against the guard before the body**, melee and blasts after the body test passes.
- The **automatic Shelter** (`FUN_14026f530`) fires itself from its button when an attack reaches your body (any attack except motion-4 instants and s54 = 2), then resolves STR vs DEF at once.
- Slots 0 at cast, 4 on a block, 5 when broken.

### Shared rules

#### Strength (STR) ✅ (`FUN_140272400`)
STR at the moment of firing = the s45 rule (table of functions at `0x1402dd0a0`; plain s46 when s45 = 0) **+ the caster's attack status (status 0, ±9 cap) + s47 × uses of that slot**, never below 0. Charge skills add their charge (below).

#### STR vs DEF (`FUN_140271760`) ✅

Order of checks, for an attack meeting a live guard:
1. **s54 = 2 ("Penetrates defensive skills")** passes straight through Barrier, Shelter, Absorb, Brush and Reflect, unless the attacker has status 31 (which cancels any Pierce).
2. Things a guard kind **does not stop at all** (the attack carries on):
   - Barrier, Reflect: Rain (motion 7, 20) and Tornado (10).
   - Absorb: blasts, **all short-range attacks** (motion 3/12/13/14), Rain, Tornado, mines, around-self.
   - Brush: blasts, Rain, Tornado, mines, around-self.
   - About Face (6): only shells (0), falls (2), crawlers (5), objects (11), lobs (1) and arcs (6) are affected — and not when s54 = 3 or for blasts; those are **sent back at their owner** (code 6). Everything else passes.
   - Course (2): nothing is stopped; shells, lobs, falls, crawlers get the "steer away" flag (above). No DEF, never broken.
3. Otherwise **DEF −= STR** on the guard object:
   - **STR > DEF (DEF went negative) → the guard breaks** (code 10). If the attacker has Pierce (s54 = 1 or status 20) **and the guard is not Anti-Pierce (s49 = 1)**, the attack continues with **STR = the difference** and hits the body. Otherwise the attack is stopped too (code 3; a short-range attack into a Brush → code 7).
   - **STR ≤ DEF → blocked**: Reflect sends shells and arcs back (code 5), stops the rest; Brush brushes (code 7: a melee attacker plays reaction 26 = DMG_SPHERE_FRONT/BACK, 30 frames = **1.0 s**; a projectile is turned aside and loses its target); others stop it (code 3). **Every successful block ends the guard 5 ticks later** (code 9), and the DEF it lost stays lost for those 5 ticks — several shots inside that window drain it (Octolaser's STR-1 shots break a DEF-3 wall on the 4th).
4. The guard's **Add effect** (`FUN_140271ed0`, s51/s52 of the guard) runs on a block or a break — **except that a guard with s50 = 1 applies it only to short-range attacks** ✅ (the "Erases any short-range attack skill it blocks" notes; our build could not tell these apart). Codes: 1 +Health = STR, 2 Aura, 3 erase the attacker's skill on that button, 4 and 6 paralyse (status 4) for s52 ticks, 5 freeze buttons (status 5) for s52, 7 capture.

#### Damage ✅ (`FUN_14026f870`)
Health −= STR (whatever is left after guards). No damage while the target has status 33 (Void ⚠️ name) or is in player state 22 ⚠️. Each hit also adds STR to a **damage accumulator** that empties when 90 ticks (3 s) pass without a hit (`FUN_14024d2f0`). Getting hit clears statuses 8, 11 and 12 ⚠️ which ones those are. The hit's Add effect (s51/s52) is applied by `FUN_140272bc0` (codes as FORMATS-SKILLS; status ids: 0 attack, 1 speed %, 2 homing %, 3 aura rate %, 4 can't move, 5 buttons frozen, 6 poison, 13 + button = that button locked).

#### Who can be hit (s62) ✅ (`FUN_14026f870`, `FUN_14026f500`)
s62 is a mask of target states the attack can touch (0 means 1): **1** normal, **2** knocked down and still in the air, **4** lying on the ground, **8** during the get-up invulnerability. Knocked-down players (states 15, 17, 18) can only be hit by s62 bits 2/4: Gravity Press and Geyser Impact (7 = normal + downed; "Effective against downed opponents"), Backdraft Bullet (6 = downed only). Pursuit/Sky Pursuit check the down state themselves. Nothing in the table has bit 8.

#### Hit reactions ✅ (`FUN_14024fd80` picks, `FUN_140258690` plays)

A hit chooses a row of the **reaction table** (gsdata offset **0x2B00C**, 0x72-byte rows like the action table; pointer `DAT_1404bdc58` set in `FUN_14017dcb0`; tool `state_table.py`):
- **In the air → s61, and s61 is 0 on every skill → row 27** = knock-down (BLOW1). (Guide: hits in mid-air knock you down ✅.)
- **On the ground, by the damage accumulator (this hit included): < 3 → s57, 3–4 → s58, ≥ 5 → s59.** So s57/s58/s59 are **light / medium / heavy**, not standing / ? / air.
- **s60** = a second reaction played after the first when the hit's Add effect took hold (14 DMG_ABILITY "skill erased", 15 DMG_AURA, 16/17 status down/up, 18 reset, 19 confusion, 20–23 binds). Non-attack skills have s57–s59 = 0 and use only s60.
- The reaction faces the direction the attack came from.

Rows used by attacks (clip lengths are frames = ticks at 30 fps):

| Row | Used as | Kind | What happens |
|---|---|---|---|
| 46 | light, shells / lasers / lobs / rain (s57 = 46) | 11 | **DMG_SHORT for 5 ticks** (a flinch) |
| 2 | medium, shells | 2 | DMG_SHOT_FRONT / RIGHT / BACK / LEFT by hit direction, **20 frames (0.67 s)** |
| 3 | heavy, shells (and every hit of Snap, Ki Bullet, Bombs…) | 5 | **knock-down**: launch 20 (1.85 u/tick) at 50°, BLOW0_FRONT / BLOW0_BACK |
| 27 | any hit in the air | 5 | knock-down, BLOW1_FRONT / BACK |
| 4, 5 | punches light / heavy | 3, 5 | DMG_SPHERE_FRONT/BACK 30 frames; knock-down BLOW1 |
| 6, 7 | swords light / heavy | 3, 5 | DMG_SORD_FRONT/BACK 30 frames; knock-down BLOW2 |
| 58–69 | per-weapon variants (punch 505, kick 507, swords 502–504, 527 sounds) | 3 / 5 | as 4–7 with their own hit sound |
| 8, 9 | drains | 0, 1 | DMG_DRAIN_LOW; DMG_DRAIN_HIGH then lie down (DOWN_BACK) |
| 10, 11 | falls / rain medium, heavy | 0, 1 | DMG_SPIKE_LOW; DMG_SPIKE_HIGH then lie down |
| 12, 13 | crawlers | 0, 1 | DMG_RAISE_LOW; DMG_RAISE_HIGH then lie down |
| 45 | Tornado (all three) | 5 | **launch 50 (4.6 u/tick) straight up (90°)**: ~89 u high, ~2.6 s in the air |
| 48 | mines, explosions | 5 | knock-down BLOW0 |
| 24 | Pursuit | 7 | pinned (DMG_STAMP_FACEUP) |
| 26 | brushed | 3 | DMG_SPHERE_FRONT/BACK (1.0 s) |

Kinds (`FUN_140258690`): 0 play one clip for its length; 1 clip then lie down 30 ticks; 2 four-way by direction; 3 front/back; 5 knock-down; 7 pinned; 8/9 binds; 11 clip for a fixed tick count.

**Knock-down physics** ✅ (`FUN_140258690` kind 5, `FUN_14025c260` state 17, `FUN_14025c3d0` state 18):
- Launch speed w39 / 10.8 u per tick at angle w40 along the hit direction; player gravity 0.12 u/tick² ⚠️ (the value re-set on landing). Row 3: 1.19 u/tick back, 1.42 u/tick up → ~24 ticks in the air, ~28 u, then a slide that loses 15 % per tick (~8 u more): **~36 u of knock-back** (about 1.7 body heights).
- **60 ticks (2 s) on the ground**, then the get-up clip (DOWN_FRONT_STAND / DOWN_BACK_STAND, 30 frames = 1 s).
- **Invulnerable from the launch until (get-up clip + 45 ticks) after starting to get up**: while down only s62-bit-2/4 attacks can touch you; after getting up a timer of clip length + 45 ticks (75 ticks = 2.5 s) keeps s62-bit-8-less attacks off (nothing has bit 8), makes you blink (`FUN_14024f2f0`) and — via `FUN_140250180` — marks you as "protected" (lock-on) ⚠️ exact use. `FUN_140251d80` / `FUN_140251f80` set the same timer to 90 ticks (probably the respawn after falling out ⚠️).
- Timeline of a ground knock-down: ~0.8 s flying + 2 s lying + 1 s getting up ≈ **3.8 s** before you can act; **~5.3 s** before ordinary attacks can hit you again.
- If the body gets stuck in the air (no height change for 90 ticks) it is forced down.

**Getting hit cancels your own move** ✅: any reaction that plays a clip clears the pending fire countdown (`+0x68 = 0` in `FUN_140258690`, so `FUN_14025c720` never fires) and ends your live short-range hit objects (`FUN_14026f430`). A cast interrupted before its fire tick never fires. (Whether the skill is still spent: not read ❓.)

### Animation playback rate — settled ✅

**One keyframe per 30 Hz tick = 30 fps.** Chain: the fixed step is 1/30 s (`FUN_1400b1bc0`); the player update `FUN_14024ec80` runs once per step — it also runs `FUN_14024d2f0`, which counts every status timer down by 1, and those timers are in 1/30 s by the game's own text (300 = "10 sec") — and in the same call `FUN_140257510` adds the clip speed (**1.0**, set by `FUN_140257310` / `FUN_1402573e0` on every clip start) to the clip time, which is compared with the clip length stored in frames (0x5 track header +0x08). So ATK_LASER (34 frames) lasts 1.13 s, not 1.42 s. **Our viewer and game (24 fps) play every clip 25 % too slowly.** Also found there:
- clip changes blend over **5 frames** (the 5.0 passed to `FUN_140257310`);
- **root motion**: when a clip is started with the root-motion flag (+0xcc), the root track's x/z movement each tick is added to the player's position. Our export strips root translation ("gameplay owns position"); which clips use it ❓ (the flag comes from the caller: idle 0; knock-down chains 1…).

### Player states (step 4, partial)

State switch `FUN_140256e80` (state = player +0x12f04): 0 / 21 / 22 / 29 free movement (`FUN_14025b5e0`); 10 / 11 cast; 12 short-range cast with dash; 14 damage reaction (hold, then chain / idle); 15 bound → lie down; 16 bind; **17 knocked down (flying / sliding)**; **18 lying**; 19 getting up; 25 / 26 stunned (brush); 27 knocked out (90 ticks); 28, 30, 31 timers. Not read: jump, dodge (Jump / Tumble / Cartwheel), dash, capsule pick-up.

- **Charge** ⚠️ (`FUN_14025c720`): hold-to-charge skills count down **w39** ticks (Gungnir / Ki Cannon / Dragon Bullet / Young White Dragon rows: 13 ticks) and then, while the button is still held, add 1 to the charge each cycle via `FUN_1402620e0` (spends Aura; stops when it can't); on release the row's clip restarts at frame **w40** (15) and the shot fires. Our sim uses 20 sim ticks (1/3 s) per point; the program's cycle looks like 13 ticks (0.43 s) ⚠️ re-arm not traced. How the charge becomes STR (`FUN_1402626d0` divides a value by 10) ❓.
- Kind 9 rows (binds that lift) use gravity 0.12 u/tick² and a rise of w45 × 10 u (row 21, Lift Up / Antigravity Trap: 50 u; rows 44 / 47: 150 u), held w42 + w43 ticks (75).
- Combos (w48): not read.

## Ours vs program — every family (phase 2)

Ours = `pd_game` as described in MOVES.md / the code on 2026-09-27. Program = the sections above. The pilot's table (above) still holds for Laser / Blaster / Psycho Knife.

| Family | Ours | Program | Fix size |
|---|---|---|---|
| **All casts** | fire at 0.4 s, caster free after; clip by hand table | fire at the action row's tick (w43: mostly 20 or 25 ticks), caster held for the whole clip (w38), clip from the row | M (plan step 1) |
| **Animation rate** | 24 fps | **30 fps**, 5-frame blends, some clips drive position (root motion) | S (rate) + ❓ root motion |
| **Getting hit** | stun / knock-down only by reaction id | hit cancels a cast that hasn't fired and your live melee strike | S |
| **Reaction choice** | s57 standing, s59 air; knock-down if id ∈ {3,13,45,48,61} | **by damage taken in the last 3 s: <3 s57, 3–4 s58, ≥5 s59; any hit in the air → row 27 knock-down**; s60 plays after an Add effect | M |
| **Light / medium hits** | flinch of fixed length | light = 5-tick flinch; medium = 4-way stagger (0.67 s shells, 1.0 s punch / sword) | S |
| **Knock-down** | ~4 s rooted; nothing hits except Pursuit etc. | launch at the row's speed/angle (~36 u back; Tornado ~89 u straight up), 2 s lying, 1 s get-up; hittable only by s62-bit-2/4 attacks while down, then **2.5 s more invulnerable (blinking)** after starting to get up | M |
| **Hit reactions table** | none | gsdata 0x2B00C, 0x72-byte rows (clips, launch, sounds) | S (export it) |
| **Punch / kick / wave (13)** | dash then strike at reach 13 u + body | fixed window (2–5 ticks) at the row's ticks, 13 u sphere on the hand marker; dash turns 3°/tick | M |
| **Sword (12)** | as punch | a 13–30 u **blade segment** from the hand, 12 u around the target's centre, swept tick to tick; two-handed swords' window 14–34 | M |
| **Pursuit (14)** | dash + strike on a downed target | single test at tick 19, 25 u sphere around the caster, downed targets only | S |
| **Shell (0)** | range 900 u, homing 1.5 rad/s always, speed 3 u/s/pt | range from s64 (300 / 800 u), homing s68 × 3°/s only within 60°, 2.78 u/s/pt; Course makes it steer away | S (pilot) |
| **Arc (6)** | flat circle to where the target stood; fan step = s68 | tilted plane; re-aims at the target; **aim clamped to 300–600 u (Optical, s44 = 1)** — close targets are safe; **multi-shot = planes rotated by s71 per shot** (Triple: left / over the top / right); shot count = SHOTnn markers | M |
| **Parabola (1)** | throw speed s64 × 1 u/s, fixed gravity 150, steering s66 | horizontal speed s64 / 10.8, **arc height s66 × 10 u**, steering s67 / 10.8 × homing %, aim clamped 80–300 u (mortars 300–600 u) | M |
| **Crawl (5)** | speed s65 × 3, homing from s68 capped 4 rad/s, ends at gaps | speed ramp s65 → s66 over s67, turn *accelerates* s68 × 0.01°/tick² up to s69 × 0.1°/tick, only on the floor; falls off edges | S |
| **Rain (20)** | marker, warning s64, radius s65 whole units, s66 drops, lands once | a **cloud 100 u over the target that follows them at 42 u/s**; 6 drops from 1 s, every 0.2 s, random within 30 u, 250 u/s, 5 u each; lands once ✅ | M |
| **Rain column (7)** | as Rain | warning s64 with the marker sliding along the target's run; 10-tick column 60 u tall, radius s68 × 0.1; a roof cancels it | S |
| **Fall (2)** | 150 u straight above where the target stood, speed s66 × 3 | **diagonal**: from s64 × 10 u up and s65 × 10 u back, speed s66 / 10.8, steers sideways s67 / 10.8, range band | S |
| **Hole (15 / 16)** | warning s64, radius 30 u, floor only | marker grows over s64 and **slides with the target's run**; 10-tick window; grounded players only; motion 16 = a hole under every player, no range limit | S |
| **Around self (9)** | blast at end of cast | **fires at tick 3**; blast grows 0 → radius over 1 s | S |
| **Blasts (bombs, mines)** | instant sphere; mines hurt only the opponent | sphere **grows over 30 ticks**; wall-impact blasts and **all mine blasts hurt the owner too**; a Sensor Mine is set off by anyone, the owner included | S |
| **Mines (8)** | kind / fuse / radius as read | same readings ✅; set at tick 4; Remote radius 30 u | — |
| **Tornado (10)** | speed s64 u/s, homing 1.5 | **no homing**, speed ramp s67 → s68 over s69 (Twister slows to a stop in 2 s; Tornado speeds up), radius s65 × 0.1, column ~100 u, **hits its own caster after 1 s**, launches straight up | S |
| **Object (11)** | table speed/homing, STR from size | lift 10 u over 1 s, then s65 speed, turns only within 60°; STR = table STR + the object's value | S |
| **Ricochet (17)** | 4 lasers ±45° fan, 3 bounces | 4 lasers in an **X** 45° off the line (up-left, up-right…); on the s73-th bounce turns straight at the target; keeps bouncing until 3 s | S |
| **Instant (4)** | at once | s65 ticks after the fire tick; s44 = 0 needs the target within 300 u | S |
| **Guards: timing / size** | s46 × 2 sim ticks; front test by angle | ~s46 − 1 ticks; front kinds are a **13 u disc 5 u + body radius in front**; round kinds a sphere of s45 × 0.1 (Course 80 u) | S |
| **Guards: resolution** | STR vs DEF, Pierce, Anti-Pierce | same idea, plus: **a block ends the guard 5 ticks later**, DEF lost stays lost in that window; what each kind ignores (table above); **s50 = 1 → Add effect only on short-range**; About Face returns only listed motions | S |
| **Brush stun** | 0.75 s | reaction 26: DMG_SPHERE_FRONT/BACK, 1.0 s | S |
| **STR** | s45 rule + status | + s47 × uses of that slot (defence: DEF = s47 + s48 × uses: Failing Wall, Dark Protection, Protecting Air) | S |
| **Charge** | 1 Aura per 20 sim ticks | ~~cycle of w39 ticks~~ **phase 3:** w39 (13) ticks of wind-up, then +1 charge per tick while held, **1 Aura per 10 ticks (0.33 s — same rate as ours ✅)**; fire countdown frozen while held, shot leaves w43 − w39 (12) ticks after release, clip resumes at frame w40 (15) | S |
| **Ground movement** (phase 3) | stick × 72 u/s in any direction at once, analog | **run 55.6 u/s, walk 20.8 u/s** (run above 70 % stick, back to walk below 40 %; nothing in between), × speed status; always moves along its **facing**, which turns toward the stick direction at **20°/tick (600°/s)**; speed × max(0, 1 − angle off / 90°) while turning | M |
| **Lock-on movement** (phase 3) | — | locked on: sideways / backward input **strafes** (TURN_* clips, body turned toward the target, full speed); idle turns to face the target | S–M |
| **Gravity / fall** (phase 3) | 300 u/s², no cap | **0.12 u/tick² = 108 u/s²**, fall speed capped at 7.5 u/tick (225 u/s); the dodge jump uses its own 0.288 u/tick² | S |
| **Dodge: Jump (750)** (phase 3) | 75 u/s up, ~9.4 u apex, 0.5 s air, 0.45 s landing | **apex 14 u** (85 u/s launch), **~20 ticks (0.66 s) in the air**, steer at run speed, **landing clip DASHEND_B = 11 ticks locked**; press → free ≈ 32 ticks ≈ 1.05 s (footage ~1.0 s ✅) | S |
| **Dodge: ground (751)** (phase 3) | tumble 0.5 s / 65 u; cartwheel 0.62 s / 111 u + 1.03 s recovery | **one move for both**: DASH_A (40 frames in the files the game loads) → **locked 40 ticks (1.33 s)**; scripted 40 u over the first 21 ticks + the clip's own root motion (~41–43 u) ≈ **80 u** ⚠️; **no Aura refill while it runs** | S |
| **Landing** (phase 3) | 0.45 s; heavy 1.1 s after 1 s airborne | by **fall height**: < 40 u no landing clip; 40–80 u JUMP_LAND1; > 80 u JUMP_LAND0 (+ dust effect) — locked for the clip | S |
| **Root motion** (phase 3) | stripped | **every cast clip** (action row w1 = 1) and most hit reactions, landings and the jump start move the player by the root's x/z (e.g. ATK_KICK +38 u, ATK_STAMP +42 u, ATK_TUKI +34 u, ATK_SHOT −11 u) | M |
| **Combos** (phase 3) | none | High-speed Punch, Rapid Cannon, Trench Mortar chain into themselves: press between clip frames w45–w46, next cast starts at frame w47 with rows s14/s15 (ATK_PUNCH_2/3, ATK_LASER_2/3) | S |
| **Melee dash condition** (phase 3) | always dashes | dash only when the lock-on target is **within 80 u** | S |
| **Aura refill** (phase 3) | 1 point/s flat; paused only when knocked down | **max(0.25, 1.25 − Aura/10) points/s** × (100 + aura-rate status)/100 × handicap (1.3 / 1.0 / 0.7); **paused while casting (whole cast), during the ground dodge / Dash-skill dash, the jump wind-up**, and when dead | S |
| **Capsules** (phase 3) | 15 s; random spot 25–70 u from the player | **14 s (420 ticks) + 0.5 s appearing**, at the **same CP00 stage spot**, holding the next card of a 30-card deck (none once the deck is used up) | S |
| **Effects for skills 301–340** (phase 3) | borrowed by family | the action row's **w2** names the effect: 301–320 use packed-only eft0302–0321, 321–340 reuse existing effects by number; the game reads **all** effects from the packed archive | S (converter) |

## Scaling up to every attack type (plan)

1. **Action table export — no more program reading needed** (Sonnet/GLM lane, ½ day): a `pd_convert` step writes, per skill, its ground and mid-air rows (animation, fire tick, dash window, hit window, cast sound, voice group) into `skills.json`. The sim replaces `CAST_TICKS` with the fire tick. The renderer replaces `cast_clip` with the row's clip and the cast sound with w3. This alone fixes the timing and pose of all 574 rows. Tool to copy from: `engine/re/action_table.py`.
2. **Motion handlers** (Opus lane, reading): 17 non-empty motion slots plus 5 s43 slots. Each is an init/update pair of 50–250 lines, about 1 hour each with the method above: [Parabola], [Fall], 3/4, [Crawl], [Rain] ×2, mines, around-self, [Move], [Object], punch/kick (13, incl. waves), [Hole], multi-laser (17), and the defence/other s43 handlers. **About 2–3 sessions.**
3. **Shared rules** (Opus lane): hit tests and hit radius (`FUN_140274b30`, `FUN_14026c5b0/c730`), guard resolution (`FUN_14026dcb0`), damage reactions and knock-down (s57–s59, `dmg*` clips), status effects, cancel rules (none were seen in the cast state; the input side is not read). **About 1–2 sessions.**
4. **Player states** (Opus, optional): jump/fall, dash, charge (s37, w39/w40), combos (w48), capsule pick-up. **About 1 session.**
5. **Implementation** (GLM lane) per family, with unit tests on tick counts and distances, then an A/B against Laurence's footage.

Rough total: **4–6 Opus sessions of reading** plus implementation lanes. The pilot's tooling (project, export, helpers) is reusable as is.

**Status after phase 3 (2026-09-27):** step 4 done (movement, jump, dodges, lock-on, root motion, combos, charge), the s43 bodies (capsule-field, movement, status, environment placement), the packed effect archive and the Aura / capsule rules — see "Phase 3" below. Left: level-up, warps, what each environment does, the camera settings file.

**Status after phase 2 (2026-09-27):** step 2 done for every non-empty motion slot and the defence guard; the other s43 effect bodies (erase / special / status / environment) only located. Step 3 done: hit tests, guards, STR vs DEF, damage, reactions, knock-down, invulnerability. Step 4 partial (state list, charge, lift binds). The reading that is left is small (jump / dodge / dash states, combos w48, effect bodies of s43 1/3/4/5, root-motion clips).

**Recommended implementation order** (GLM lanes; S ≈ ½ day, M ≈ 1–2 days):
1. **Export** (S): the action table rows, the reaction table (0x2B00C) and s70–s73 into `skills.json` / a new `reactions.json` (`action_table.py`, `state_table.py` show the layouts).
2. **30 fps playback** in viewer and game (S) — visual check with Laurence.
3. **Cast timing from the action row** (M): fire tick, whole-clip lock, row clip and cast sound; hits cancel unfired casts.
4. **Reactions and knock-down** (M): light / medium / heavy by the 3-second damage total, air hits → row 27, launch physics, 2 s down + get-up, s62 masks, 2.5 s post-get-up invulnerability with blinking.
5. **Range bands s44** (S): aim clamp for Arc, Parabola, Fall, both Rains, Hole; motion-4 range.
6. **Guards** (S–M): disc / sphere geometry and placement, ~s46 − 1 ticks, block ends the guard after 5 ticks, per-kind ignore lists, s50 short-range-only Adds, DEF per use, Brush stun 1 s.
7. **Per-family motion fixes** (each S unless noted): melee windows + sword blade (M), Parabola height / steering, Fall diagonal, Rain cloud + drops (M), Hole / column sliding lead, Tornado without homing + self-hit, Crawl turn acceleration, arc multi-shot planes, Ricochet X + last-bounce aim, growing blasts that hurt the owner, mines.
8. **A/B against Laurence's footage** (FOOTAGE.md) after 2–4.

## Phase 3 (2026-09-27): movement, dodges, root motion, combos, capsule bodies, packed effects, Aura

Same rules as before (no game code copied; addresses to re-find things; raw exports in `%USERPROFILE%\.pdtools\re-out\`). Units: tick = 1/30 s; angles in the program's 32,768-per-turn units are given in degrees; distances in world units (u).

### New tools (engine/re/)

- `exe_const.py` — print floats / ints / small tables from `PDUWP.exe` by virtual address (refuses runtime-only `.bss` addresses).
- `xref.py` — rough "who references this address" scan of `.text` (RIP-relative operands; confirm hits in the export).
- `effect_archive.py` — list the packed effect archive, or unpack one effect to `eftNNNN.alr` + `eftNNNN.eft` (writes game data: keep the output outside git).
- `DisasmRange.java` now starts at the next instruction when given a mid-instruction address.

### Where the player lives in the program

- Fighter object (`pdNormalPlayerClass`, vtable `0x14048e3b0`): position +0x12e40 (x, y, z; y up), vertical speed +0x12e84, **gravity +0x12e80 (0.12 u/tick², set in `FUN_14024e190`, reset on every landing)**, grounded flag +0x12e74, facing +0x12e68, camera yaw +0x12e5c (movement is camera-relative), camera pitch +0x12e60/+0x12e64. Stats block at +0x40: health +0x0c (max +0x10), **Level +0x18 (int)**, **Aura +0x1c (float)**, statuses from +0x61b8 (12 bytes each: value, amount, timer).
- Animation / state controller at fighter +0x12f00 (the "state" of phase 2 is its +4). Per-clip table at controller +0x108 + 16 × clip (pointer, length as float, length as int — the int is what state timers use).
- Input at fighter +0x14248 (`FUN_140256580`): sticks with a **dead zone of 13,000 / 32,767 (40 %)**, then linear to 1.0 (`FUN_140256600`); held / previous / newly-pressed bit masks at +0x14 / +0x18 / +0x1c.
- **Character table in memory** (`FUN_14017d300` loads `setup/charaset/charaset_en.dat`, 0x4C-byte records, into 0xA0-byte records at `DAT_1404c01e0`). Each u16 field is converted: record +0x0C ÷ 10 = **height** (Hero 18 u, Edgar/pH/Sammah 19, Meister/Cuff/JD 20) → fighter +0x12ea0; +0x0E ÷ 10 (10) → +0x12ea4; **+0x10 ÷ 10 = body radius 5 u** → +0x12e9c; **+0x12 ÷ 10 = centre height 9 u** → +0x12e98; +0x14 ÷ 100 (1.0); +0x24..+0x30 are degrees (× 91.02 → angle units) — **+0x30 = turn rate, 20°/tick for every playable character** (enemies 10–12); **+0x36 = the dodge move (750 / 751) = a skill record index**; +0x3A = colour count (4 for players: the game then loads `pcXXa<colour>.alr`, see "Animation files" below). ✅

### Body size: which charaset field each test reads ✅ (bodyradius lane, 2026-10-03; `crates/pd_game/src/body.rs`)

Loader `FUN_14017d300` (0x14017d3cf..0x14017d3fa: `+0x0C / 10`, `+0x0E / 10`, `+0x10 / 10`, `+0x12 / 10`) and the copy onto the fighter in `FUN_14024e190`:

| record | fighter | playable value | read by |
|---|---|---|---|
| `+0x0C / 10` | `+0x12ea0` height | 18 / 19 / 20 u (Hero, Freia, Chunky, Know, Tsubutaki 18; Edgar, pH, Sammah 19; Meister, Cuff Button, JD 20) | wall sweep, push-apart, `FUN_140254fa0` |
| `+0x0E / 10` | `+0x12ea4` **contact size** (10; enemy Germano 12) | 10 | **push-apart** `FUN_140250780`, **wall sweep** `FUN_140255020` -> `FUN_1401959c0`, `FUN_1402559f0` (arena rim) |
| `+0x10 / 10` | `+0x12e9c` hit radius (copied to `+0x130` by `FUN_1402513a0`) | 5 (enemies 6 - 7.5) | every hit test (`FUN_14026c1c0` ...), CPU `margin`, `FUN_140250cd0` (registers the body sphere) |
| `+0x12 / 10` | `+0x12e98` centre height | 9 (enemies 9 - 12) | aim point, hit-test centre |

`+0x12eac = size / height` and `+0x12ea8 = height / size` are stored too; the only reader is the wall sweep.

- **Fighter against fighter** (`FUN_140250780`, vtable `0x14048e3b0` slot `0x70`, the first call of the per-fighter mover `FUN_140254590`, so it runs before that fighter's wall / ground step): for every other live fighter (`+0x20`, `+0x24` set, `+0x10` clear), centres are `y + height * 0.5`; if `|centre_b.y - centre_a.y| < (h_a + h_b) * 0.5` and the **horizontal** distance (y dropped) is under `size_a * 0.5 + size_b * 0.5` (`DAT_1402f7d40` = 0.5; **10.0 for any two playable fighters**), the two are moved apart along the line between them, `(contact - distance) * 0.5` each (the other fighter's `+0x34` set, as it is in a match; with `+0x34` clear only the first moves, by all of it). Coincident bodies separate along +x. MEASURED: the originals stop 10.0 apart (Swift Kick, Psycho Knife takes; min over all 682 trace pairs on one deck 9.99).
- **Fighter against the stage** (`FUN_140255020`): the body is three spheres on a vertical line through the fighter (centre `y + +0x12e98`, offsets `0` and `+-(height * 0.5 - r)`), radius `r = (height * 0.5) / (height / size) = size * 0.5` = **5** (`FUN_1401959c0` is called with `param_5 = height * 0.5`, `param_9 = height / size`: `0x140255102 divss xmm14, [rax + 0x12ea4]`, `0x1402554a4..0x1402554c5` the stack arguments; inside, `param_9 = param_5 / param_9`). Ours is a cylinder of the same radius (above step height, up to the head); the three-sphere shape itself is not rebuilt (UNCONFIRMED whether it matters).
- `FUN_1402559f0` (a rim: contact size + distance from the origin against `DAT_1402f80a4` = 150, only `DAT_1403e0304 % 10 == 1`) is not built.
- The old values: `BODY_RADIUS` 4.5 (a first guess, each body 9.0 from the other) and `BODY_HEIGHT` 20 for everyone.

### Ground movement ✅ (`FUN_14025ad50` state 1, `FUN_14024f8f0`, `FUN_14024f9c0`, `FUN_140256e80`)

- **Walk or run, nothing in between**: the stick magnitude only picks the gait, with hysteresis — run once it passes **0.7**, back to walk below **0.4** (floats at `0x1402f7d60` / `0x1402f7d30`).
- **Speeds: run 20/10.8 = 1.852 u/tick = 55.6 u/s; walk 7.5/10.8 = 0.694 u/tick = 20.8 u/s** (`0x1402f7e8c`, `0x1402f7d5c`), × the speed status (1 + status 1 / 100: Speed Up +20 %, Lightning Speed +50 %).
- **Facing turns, position follows facing.** Each tick the facing turns toward (camera yaw + stick angle) by at most the character's turn rate (**20°/tick = 600°/s**), and the player moves along the facing at speed × **max(0, 1 − |angle still to turn| / 90°)** — so a 180° reversal costs 9 ticks, the first ~4 of them without moving.
- Status 35 (0x23) rotates the stick by 90°; status 4 ("can't move") turns any stick push into a struggle clip (DMG_PARARISE, state 20).
- The walk / run clip is picked from a per-"mood" set (`0x1403faed0`, 5 rows): normal, fast (speed status > +49), slow (< −49), confused (status 35), wounded (**health < 6** → WAIT_DAM / WALK_DAM / RUN_DAM). Cosmetic, but free to copy.

### Lock-on ✅ (`FUN_140252a20`, `FUN_1402531d0`, `FUN_140252f70`, `FUN_1402543e0`, `FUN_14025b5e0`, `FUN_14025b270`)

- **Targets**: any player (32 lockable slots) that is alive, not the caster, not a team-mate (unless an option allows it) and **not under status 8** (Optical Camo / Place of Invisibility hide you from lock-on). With no target the lock picks the first valid one automatically (option at `0x14052dfb8`); a second button (mask 0x400) cycles to the next target. The lock button (0x800) is toggle or hold depending on an option (`0x14052dfb7`).
- **Distance band** (lock struct +0xc = fighter +0x13f1c): **1 within 80 u, 2 from 80 to 300 u, 3 beyond 300 u** (`0x1402f8040`, `0x1402f8138`) — the same thresholds as the skills' range bands. **The melee dash (motions 3/12/13/14) only gets a target in band 1** (`FUN_140258180`): no lunge unless the locked target is within 80 u. (Resolves the pilot's "lock state 1" ❓; footage round 2 "no dash without lock-on" ✅.)
- **Camera**: while locked, the camera yaw turns toward the target and the pitch toward the target's chest, each limited to a per-player rate, and the pitch is clamped; the rates / limits come from `setup/configdata.dat` (6 blocks of 13 floats, e.g. block 0 = 6, 3, 20, 10, 13, 55, 13, 55, 70, −70, −9, −9, 1; which float is which ⚠️ — the first two read as 6°/tick yaw and 3°/tick pitch).
- **Movement while locked** (lock active flag +0x13f24): if the stick points more than 45° away from the camera's forward, the player **strafes**: right (45–135°) plays TURN_RUN_R when walking / TURN_WALK_R when running (the table lists each pair walk-first — the clip names are misleading; confirmed by Laurence's footage 2026-09-27) with the body turned 90° back toward the target, left likewise, and backward (≥ 135°) plays TURN_*_BR / BL facing the target. Strafing moves at full walk / run speed (no turn slowdown) along a heading that turns at the character's turn rate. Forward (within 45°) is the ordinary run. Standing still, the facing turns toward the camera yaw (the target) at the turn rate. The TURN_* clips are played **without** root motion.
- **The TURN_* clip is a one-shot pivot, not a strafe-loop** ✅ (read 2026-09-27 for the "does it loop" question; `FUN_14025b5e0` picks the direction — 0 right, 1 left, 2 back-right, 3 back-left, 4 = forward/not a strafe, from the signed camera-relative angle in the 32,768-per-turn units, with an outright random pick between back-right/back-left when the stick is exactly behind — and starts the row's TURN clip via `FUN_140257310` with its **loop argument off** (the same call for the forward case passes the loop argument **on**, which is why the ordinary WALK/RUN clip *does* loop). There is **no dedicated "strafe run" clip anywhere in the data** — `FUN_14025b270` runs every tick while the currently-playing clip is still one of the TURN_* rows (checked by row-id range): it keeps turning the *movement heading* (fighter +0x12e6c) toward the target at the turn rate, sets the body's *visible facing* (+0x12e68) to that heading plus a fixed per-direction offset chosen when the strafe started (∓90° side, 180° back, at +0x12e70 — this is the "body turned toward the target" pose), and — this is the key part — **queues the ordinary WALK/RUN clip as the "next clip"** (record field +0x37, with its own loop flag on, a pending-transition flag at +0x3a) without starting it yet. The actual switch happens in the shared per-tick clip-advance function `FUN_140257510` (already known from "Root motion"/jump time-stretch): once the *currently playing* clip's own timer reaches its length and a next clip is queued, it starts that queued clip — here, the plain WALK or RUN, looping — snapping the visible facing (+0x12e68) to plain the movement heading (+0x12e6c), **dropping the pivot's target-facing offset for good**: `FUN_14025b270`'s whole heading/offset/queue block is gated on the *currently playing clip* being one of the TURN_* rows, so once the clip has switched to WALK/RUN that block stops running (the surrounding `else` just calls `FUN_140259d10(param_1,1)`, handing off to the ordinary state-1 ground-movement handler — see "Ground movement"). **Corrected reading, 2026-09-27** (supersedes an earlier draft of this note that assumed the offset kept being reasserted): the body does **not** stay turned toward the target for the continuous part of a strafe — it faces wherever it's now heading, exactly like ordinary forward movement, which is what Laurence's follow-up report ("make the character face where the left analogue stick is pointing") independently confirmed by feel. **Net effect**: press a strafe direction → the TURN_* pivot plays once, turning the body from facing the target to facing the direction of travel → the pivot ends and the sim hands off to state 1, which runs facing that same direction for as long as the same input holds. Changing direction or gait while a TURN_* clip is still mid-playback restarts a turn clip for the new combination (`FUN_14025b270` retimes it with `FUN_1402573e0`, carrying over relative progress via a length-ratio scale — unconfirmed exactly how the ratio is used ⚠️; our engine just restarts the new pivot from frame 0, an accepted simplification). Reported by Laurence after the walk/run-swap fix: our engine was looping the TURN_* clip itself for as long as the stick was held, instead of pivoting once and settling into a run — fixed as a clip-sequencing state machine (`main.rs::strafe_sequence`). Reported again after that fix: the body kept facing the target through the continuous phase instead of the movement direction — fixed render-side only (`main.rs::sync_visuals`/`visual_faces_movement`), easing the *displayed* yaw toward the movement direction at the turn rate while the sim's own facing (used for aim/casting) stays on the target.
- **The TURN_* clips bake their own hip-joint yaw — they are not turns in place** ✅ (read 2026-09-28 after a third report). "Played without root motion" (established earlier) only means the root joint's *translation* track is empty for these clips — its *rotation* track is not: measured directly from the converted `pc00a.glb` (node 22 = the root/hip joint), e.g. TURN_WALK_R's own baked yaw swings roughly -35° to -73° over the clip, TURN_WALK_BR roughly +4° to -160° (all 8 clips checked; every plain WAIT0/WALK/RUN clip carries a similar small baked sway too, e.g. RUN ~20°, WALK ~-3°, WAIT0 ~-36°, apparently a stylistic weight-shift baked into every clip, not specific to strafing). This is exactly what the fixed per-direction offset two paragraphs up (∓90°/180° at +0x12e70) is *for*: it is chosen once per strafe so that heading + offset lands close to the target-facing pose the TURN_* clip's own baked rotation is choreographed around, so the two together read as "body turned toward the target" rather than fighting each other. Our engine's fix (`main.rs::pivot_model_yaw`) doesn't try to reproduce the program's fixed-offset arithmetic exactly (our converted clips' measured curves don't match a clean ∓90°/180° constant, likely a retargeting/authoring difference from the original), and instead cancels each clip's own measured curve directly so the *effective* on-screen facing (model yaw + the clip's current baked yaw) traces a smooth turn-rate-limited path regardless of the clip's own shape — verified continuous at the pivot's start, through the pivot, and at the hand-off into the continuous WALK/RUN clip (which needed the same treatment for its own small baked sway) for all 8 (direction, gait) pairs. 2026-09-29 correction: the hand-off correction itself fades from the pivot's end baked yaw to the gait loop's over the clip switch's own 5-frame blend (`main.rs::handoff_baked_yaw`) -- swapping it in one frame stepped the rendered facing by up to ~90 deg while the clip only cross-fades (Laurence: the strafe-to-run transition "snapped").

### Jumps ✅ (`FUN_140257b90` launch, `FUN_14025a0b0` take-off, `FUN_14025b0f0` airborne, `FUN_14025a1c0` fall, `FUN_140254590` physics, `FUN_140254c80` landing)

- **Launch** (`FUN_140257b90(controller, h, kind, now, gravity)`): sets the player's gravity g, launch speed v = √(20 · g · h) — i.e. **apex = 10 × h** — and the jump kind (row of the 7-entry table at `0x1403fae20`: start, up, down, —, light landing, heavy landing clips; kinds 0–4 and 6 = JUMP_START / JUMP_UP / JUMP_DOWN / — / JUMP_LAND1 / JUMP_LAND0, **kind 5 = DASH_A / DASHEND_A / DASH_B / DASHEND_B ×3**). Unless "now", it first plays the start clip (with root motion) and waits **clip length − 2 ticks** (state 4).
- **Rise** (state 5): the "up" clip is played **time-stretched to last exactly v / g ticks** (time to apex); at the apex the "down" clip starts, stretched to the same duration. **This is why the whole jump in the footage is far shorter than the clips end to end.** Floating statuses (0x0b) switch to FLOAT_IN / FLOAT_WAIT / FLOAT_MOVE (state 7) instead.
- **Air control**: while the stick is pushed the player turns at the turn rate and moves along the facing at **run speed × the turn factor** (no speed-status bonus). There is no air dodge (the dodge button only works in states 0, 1, 8).
- **Gravity** 0.12 u/tick² (108 u/s²) unless the jump set its own; falling speed capped at **7.5 u/tick** (225 u/s). Walking off a ledge starts the same fall (state 5).
- **Landing** by **fall height** (highest point − landing point): **< 40 u → the kind's entry 3 (none for ordinary jumps: straight back to idle); 40–80 u → JUMP_LAND1; > 80 u → JUMP_LAND0 + a dust effect** (`0x1402f7ff0` = 40, `0x1402f8040` = 80). The landing clip plays with root motion and **locks the player (state 6) for its length − 1 ticks**. Gravity is reset to 0.12.

### Dodges and movement skills ✅ (`FUN_1402517c0` button, `FUN_140270200` = the s43 = 3 body)

- **The dodge is a hidden skill.** Pressing the dodge button (mask 0x1000000) in states 0 / 1 / 8 creates an attack object for **skill record 750 or 751 — the charaset +0x36 value** (program record index; = our records 749 / 750, the "duplicate 257 / 294" rows). Its s43 = 3 sends it to `FUN_140270200`, which switches on **s44**:
- **750 = Jump** (Hero, Freia, Meister, Chunky, Know, Tsubutaki; s44 = 9): launch with h = **s45 × 0.1 = 1.4 → apex 14 u**, jump kind 5 (the DASH_* clips), gravity **(s47 + 100) × 0.12 × 0.01 = 0.288 u/tick²** (s47 = 140). Launch speed 2.84 u/tick (85 u/s), **9.9 ticks up + 9.9 down ≈ 20 ticks (0.66 s) airborne**. In the files the game loads, jumpers' DASH_A is 2–3 frames (so take-off is immediate), DASHEND_A 10–13 (stretched over the rise), DASH_B 9–12 (fall), DASHEND_B 12 (Freia / Tsubutaki 23) = the landing, always played, **11 (22) ticks locked**. Press → free ≈ 1 + 20 + 11 = **32 ticks ≈ 1.05 s** — footage round 2 measured ~1.0 s total and ~0.3–0.37 s of landing ✅ (its 0.43–0.5 s airtime reading is shorter than 0.66 s ⚠️ — take-off / touch-down were judged by eye).
- **751 = ground dodge** (Edgar, Cuff Button, pH, JD, Sammah; s44 = 8, action row 355): plays **DASH_A with root motion** and moves forward **s45 × 10 = 40 u spread over w42 − w41 = 21 ticks (1.9 u/tick)**; the state (3) lasts the clip's length. In the files the game loads, **DASH_A is 40 frames for all five** (their DASHEND_A / DASH_B / DASHEND_B are 1-frame stubs) and its root drifts ~41–43 u forward — so the dodge covers ≈ 80 u and **locks the player for 40 ticks (1.33 s)** ⚠️ (the tumble / cartwheel difference is only in the clip; the program treats them the same). No Aura refill during it (state 3, below) ✅ matches the fan guide.
- **Dash (skill 240)**, same s44 = 8 path, row 294 (SPE_DASH, w41 0 / w42 15): **100 u in 15 ticks (6.67 u/tick = 200 u/s)**, locked for the SPE_DASH clip (20 ticks); speed capped at 10 u/tick. Read again 2026-10-01 (moveskills lane, disassembly of `0x1402706xx`): the window is `[row+0x54] - [row+0x56]` = **w42 - w43** (this text's "w42 - w41" is the same number only because w41 = w43 = 0 on rows 294 / 355); the step is `s45 x 10 / window` (cap 10, `0x1402f7f7c`), stored at `+0x12fa8` with the counter `+0x12f9c`; state 3 (`FUN_14025afe0`) then moves `facing x step` (`FUN_14024f9c0`, skipped under status 4) on each of the first `window` ticks of the state, which lasts clip length - w43. Built: `Player::begin_dash` (sim.rs), `special::dash`; at 60 Hz that is 30 sim ticks of 3.33 u.
- **Jump (skill 239)**, s44 = 0: h = s45 = 10 → **apex 100 u** ("30 ft" ≈ 110 u ✅), gravity 0.12 → ~41 ticks up, ~41 down; JUMP_START first (13 ticks), lands with JUMP_LAND0 (fall > 80 u).
- **Skywalk / Levitate** (s44 = 5): float status 0x0b for **s46 ticks** (Skywalk 450 = 15 s, Levitate 90 = 3 s), then the s44 = 0 jump (apex 50 u / 100 u); at the apex the player floats (no gravity, moves at run speed × turn factor, FLOAT_* clips) until the status ends or the same button is pressed again. Read 2026-10-01 (moveskills): case 5 writes status 11's timed slot (`+0x6240 = 1`, `+0x6244 = s46`; s46 = 0 would be the permanent slot `+0x623c`) and falls into case 0, so it **also waits JUMP_START (now = 0: clip length - 2 = 13 ticks) before the launch**; `FUN_140254590` does `vy -= g` each tick and, **the first tick `vy <= 0` while status 11 holds, sets `vy = 0`** -- so the apex is the discrete top of the rise (v = sqrt(20 g h): Levitate h 10 -> 100 u after ~41 ticks, Skywalk h 5 -> 50 u after ~29) -- and `FUN_14025a330` switches state 5 -> 7 then (FLOAT_IN, then FLOAT_WAIT / FLOAT_MOVE). State 7 (`FUN_14025b170`) goes back to idle the tick status 11 is gone, and the body then falls (landing by fall height, so ~100 u = the heavy landing). Levitate and Skywalk are the SAME status 11, so a hit (`FUN_14026f870` tail) ends both. s47 = 0 on both, so the gravity is the ordinary 0.12. Built: `Player::begin_float` + `special::float_and_jump` (Skywalk through `status::apply`).
- **Warps**: s44 = 2 Retreat → to one of your own capsule spots; 4 Call → stores your position (+10 u up) as your capsule spot; 6 Return → to your start point; 7 Surprise Attack / Charge → next to a target (random opponent in 4-player). All go through `FUN_140257e90` (SPE_WARP clip; not read line by line).

### Root motion ✅ (`FUN_140257510`)

- Each tick the program samples the clip's root track x and z before and after advancing, rotates the difference by the facing and, **if the clip was started with the root-motion flag, adds it to the position**. A non-looping clip clamps at its end (no drift after it ends); a looping clip wraps.
- **Who sets the flag**: every cast (`FUN_14025a3e0` and the charge release pass **action-row w1**, which is 1 on every used row), every hit reaction whose **reaction-row w1** is 1 (all except the 5-tick flinch row 46, row 70 and the status rows 0, 1, 33–43), knock-down landings (BLOW_LAND_*), get-ups, jump starts, landings, dodges, emotes. Not: idle / walk / run, strafe (TURN_*), JUMP_UP / JUMP_DOWN, FLOAT_*.
- **How much** (pc00a, root x / z drift over the clip; the character faces −z, so negative z = forward): ATK_STAMP −42, ATK_KICK −38, ATK_TUKI −34, ATK_BODY −27, ATK_WHAND_SORD −26, ATK_SPHERE −23, ATK_SORD −18, ATK_FAITH_SHOT −15, ATK_WHAND_LASER −14, ATK_DRAIN −11, DEF_COURSE −8, ATK_PUNCH / ATK_SPIKE / ATK_SUPER_SHOT −7, JUMP_START −6 (−7 in pc00a0), ATK_SHOT **+11 (a step back)**, ATK_PUNCH_2/3 +8, DEF_SHELTER +5; reactions DMG_SPHERE_FRONT +18, DMG_SORD_FRONT +17, DMG_SHOT_FRONT +10, DMG_SHOT_LEFT / RIGHT ∓11 sideways, BLOW_LAND_LB −22 / LF +17, WALLHIT ±15. Melee lunges (phase 2) come **on top** of these. (Measured with `pd_convert keys` on the root joint; the drift of the in-game colour files can differ by a few units.)

### Animation files ✅ (`FUN_140250f70`)

The game loads a player's model and motions from **`Player/pcXXa/pcXXa<colour>.alr`** (colour 0–3), not `pcXXa.alr`, whenever charaset +0x3A is non-zero (4 on all playable characters). **Their motion sets differ from the base file we convert**: e.g. Edgar WALK 30 frames (base 24); every 751 character's DASH_A is 40 frames with DASHEND_A / DASH_B / DASHEND_B 1-frame stubs (base: 15 / 25 / 10 / 30 or 1 / 12 / 12 / 30), and their JUMP_* are ordinary jump clips (in the base file they copy the DASH clips); jumpers' DASHEND_A 13 (base 15). Attack clips (ATK_SORD 40, ATK_SHOT 45, ATK_LASER 34) match.

### Combos ✅ (`FUN_14024fb90` gate, `FUN_14025a3e0` cast start)

- A skill can be chained into skill X if **X's program record index is in the current skill's s27–s36** (10 slots; data: High-speed Punch, Rapid Cannon and Trench Mortar list themselves; Splash ×3 list Rapid Cannon — enemy).
- **Window**: the press is accepted (and buffered) while the current clip frame is between **w45 and w46** of the current action row; the next cast starts exactly at frame **w47**. The chain counter increments; a normal cast resets it.
- **Rows**: hit k ≥ 1 of a chain uses **s14..s17 cyclically** (the non-zero ones) instead of s12: High-speed Punch 14 → 301 (ATK_PUNCH_2) → 302 (ATK_PUNCH_3) → 301…; Rapid Cannon / Trench Mortar 48 / 52 → 303 (ATK_LASER_2) → 304 → 303…
- **w48** is subtracted from the fire tick (and the clip starts that many frames in) on chained hits — but **w48 is 0 on every row**, so it has no effect in this data.
- Timings: High-speed Punch hit 1 fires at 20, window 17–24, chains at 24; hits 2/3 fire at 7 / 6, windows 4–11 / 3–10, chain at 11 / 10. Rapid Cannon fires at 13, window 8–17, chain at 17; later shots fire at 7, chain at 11. Whether each chained hit pays Aura again is not read ❓.

### Charge, finished ✅ (`FUN_14025c720`, `FUN_1402620e0`, `FUN_1402626d0`)

**Which skills charge** (read 2026-10-01, moveskills lane): the cast start (`FUN_140258180`, `0x14025823e`) reads the skill record's field at `+0x46` (= our **s37**) and sets the cast mode `+0x68`: s37 = 2 -> 1 (hold-to-charge), s37 = 0 -> 2 (ordinary), anything else -> 0 (not castable by hand: Orb / Amulet, s37 = 1); `FUN_14015b830` (the arsenal rules) tests the same field. `FUN_14025c720` reads w39 as a wind-up countdown in every mode but only freezes the fire countdown in mode 1. So **Dragon Bullet (s37 = 0, row 389 with w39 = 13, w40 15, w43 25 like the charge rows) is an ordinary cast** that fires at w43 = 25 (the earlier audit's "program True" rested on w39 > 0, which is wrong); the charge skills are exactly s37 = 2: Gungnir, Ki Cannon, Ki Palm, Young White Dragon (`SkillDef::charged` = `raw[37] == 2`).

Charge rows (Gungnir, Ki Cannon, Young White Dragon, Dragon Bullet: w39 13, w40 15, w43 25; Ki Palm: 7 / 9 / 20): the cast counts **w39 ticks** of wind-up (the fire countdown runs alongside). At 0 it plays the row's charge cue; from then on, **while the button is held the fire countdown is frozen and the charge count grows by 1 per tick**; each tick the cost check is base cost + **count / 10** (halved / doubled by status 30) against the current Aura — when it can't be paid the count stops growing. On release the clip restarts at frame **w40** and the countdown resumes, so the shot leaves **w43 − w39 = 12 ticks (0.4 s) after release**. At fire the slot's cost for this use becomes base + count / 10 (= "Aura spent charging", the s45 = 21 STR rule). So **1 Aura per 10 ticks = 0.33 s — the same rate our sim uses**.

### Capsule effect bodies ✅ (read in phase 3)

- **s43 = 1 — capsules on the field** (`FUN_14026fcb0`, per capsule `FUN_14026ff20`, filter `FUN_140270150`). Targets are capsules lying on the battlefield (each player has 3), not equipped skills. **s44 = 0** the one you touch, any kind; **1** the one you touch, Aura Particles only; **2** every player's Aura Particles; **3** the one you touch, skill capsules only; **4** opponents' skill capsules (unused); **5** every player's skill capsules. **s45** picks what happens: **0 erase** (the slot starts its respawn timer), **1 capture** onto the button you cast from (Thieving Imp), **2 swap the capsule's skill for a random undrawn card of that player's deck** (Change the World; built 2026-10-01: `FUN_14014fa20` takes a random undrawn card `r`, puts the capsule's old skill in the deck at `r`, then calls `FUN_14014fd10` mode 0 whose respawn draw takes the deck's NEXT card (`cursor++`) before the spot is set ready again with the card taken from `r` -- so the owner's undrawn pool shrinks by one per swapped capsule; with no undrawn card nothing swaps; appearing spots are skipped), **3 turn it into an Aura Particle** (Particle Convert). Impact code 11 plays at the capsule. Our erase.rs matches (s45 = 2 included since 2026-10-01).
- **s43 = 3 — movement specials** (`FUN_140270200`; the phase-2 label "capsule-targeted special" was wrong): Jump, Dash, Skywalk, Levitate, Retreat, Call, Return, Charge, Surprise Attack and the two dodges — see "Dodges and movement skills".
- **s43 = 4 — status / self specials** (`FUN_140270890` → `FUN_140270a00` → `FUN_140270b00`): **s44 = 0 the caster, 1 the target (needs one), 2 every player**. Up to **four effects per skill: kind s45+k, amount s49+k, duration s53+k** (k = 0..3, skipped when the kind is 0). Kinds: **1 Level + amount** (Level Amp +2); **2 Aura + amount** (Graceful Woods +99 = full); **3 Health + amount** (Healing Water +5, Rebirth +10); **4–7 statuses 0–3 (attack, speed %, homing %, Aura rate %) + amount**, permanent (capped at ±9 / ±90 / ±90 / ±90) or, with a duration, for that many ticks (Tiger's Strength attack +2; Speed Up +20 %; Lightning Speed +50 %; Acquisition homing +20 %; Meditation Aura rate +20 %; Heat attack +1 & Aura rate −10 %); **9 Aura × amount**; **10 Level doubled** (Level Boost); **11 Regenerate** (a standing state that gives +1 Health every 91 ticks); **12 invisible** (status 8: can't be locked on; Optical Camo, Place of Invisibility); **13 invisible + float + jump** (Demon's Wing); **14 status 33 (no damage; Void)**; **15 Invulnerability** (state 22: no damage); **16** copies the caster's skills onto the target's buttons ⚠️; **17 status 37** (Mind Reading ❓ effect); **18 copy the target's four statuses** (Doppelganger); **19 take back the last used skill** (Relearn); **20** deck operation + state 24 (Mephisto's Pact ⚠️); **21 return `amount` used skills to the deck at random** (Recall); **22 fill the deck** (Bravery Decision ⚠️); **23 invert all four statuses** ⚠️. The same kinds are the "Add" codes of non-attack capsules.
- **s43 = 5 — environment** (`FUN_14017bcc0`): writes a crystal record (position = the caster's feet, skill, creation tick, **expiry = now + s47 when s47 > 0** — Traveling Thought, Commandments, Providence, Vanishing Light, Stagnant Air, Spiritual World, Scientism: 1,800 ticks = 60 s; the rest last until replaced) into a small table (`0x14058bd30`, 0x1C bytes per entry), sound 0x5d. What each environment does is applied by the systems that read that table (not traced one by one ❓).

### Packed effects ✅ (`FUN_14018b2f0`, `FUN_14018c410`, `FUN_14018c4e0`, `FUN_14018cd10`, `FUN_14018d0b0`)

- **The game reads every effect from the packed archive**, not from the loose `eftNNNN.alr` / `.eft` files: an effect request (`FUN_14018cd10`) always goes through `FUN_14018d0b0`, which looks the number up in `Effect/e.i` and reads `Effect/a<k>.e`. (A debug path can load loose files by name when a flag at +0xac is set; it is off in normal play ⚠️.)
- **`e.i`**: u16 archive count (8), u16 entry count (496), then 496 × {**u16 effect number, u16 archive index, u32 byte offset, u32 byte size**}, sorted by number (the program binary-searches it). Offsets and sizes are multiples of 0x800.
- **An entry** = the effect's ALR (padded to 0x800) followed by **one 0x800 block whose first 412 bytes are the `.eft` event table**; the program keeps a pointer to "entry end − 0x800" for it. Checked over all 496: where a loose `.eft` exists it equals this block in 330 of 433 cases; **103 differ in a few fields** — the packed ones are what the game plays. The packed ALR parses with our reader (`pd_convert anims` on unpacked eft0302).
- **Which effect a skill uses = action row w2**, not the text id: skills 301–320 → eft0321, eft0302 … eft0320 (packed only); 321–340 reuse existing effects by number (Excaliber → eft0004, Knife Grind → eft0003, Twist Laser → eft0043, Fast Bomb → eft0097, Erase Shell → eft0130 …). Packed-only numbers: 302–321 and 43 in the 3000s / 4000s (story / system effects). Tool: `engine/re/effect_archive.py`.

### Aura and capsules ✅

- **Refill** (`FUN_14024de20`, every tick from the status update `FUN_14024d2f0`): **+ (100 + Aura-rate status) / 100 × handicap × max(0.25, 1.25 − Aura / 10) / 30 per tick**, capped at Level. So **1.25 points/s at 0 Aura, 0.75/s at 5, 0.25/s from 10 up**. Handicap (per player, gsdata 0x33034): 1.3 / 1.0 / 0.7 (normal = 1.0).
- **What pauses it** (`FUN_1402523a0`, vtable slot 10): health 0; **states 3–4 (ground dodge / Dash-skill dash / movement specials, jump wind-up) and 10–13 (casting, the whole clip)**. Under status 29 only standing still (states 0, 21, 22, 24) refills. The dodge *jump*'s airtime does not pause it.
- **Paying** (`FUN_1402621f0`): Aura ≥ cost; Level ≥ s23 then Level −= s23; Health −= s25 (−1 = half your Health, rounded up).
- **Level-up**: not located in this phase ❓ (the guide's "Aura Particle: Increases Level by 1"). `FUN_14024d970` clamps Level to a per-game-mode min / max read from gsdata 0x33044 (0 / 99 in versus modes) — so the in-game cap must come from elsewhere ❓.
- **Capsules** (`FUN_14014ee70` setup, `FUN_14014fd10` take / remove, `FUN_14014ed10` per-tick, `FUN_14014f320`): each player has **3 capsule slots at fixed stage spots — the CP00 start record's three capsule positions** (+0x14, +0x20, +0x2c) ✅ (FORMATS-STAGES "⚠️ use" → used). When a capsule is taken or erased its slot waits **420 ticks (14 s; + status 34)**, then spends **15 ticks appearing** (sound 0x55) and is pickable — at the **same spot**, holding the **next card of the player's 30-card deck**; once the deck is used up the slot stays empty. At the start of a round the slots are filled at once.
- **Capsule visuals** (2026-09-28, details in CAPSULE-VISUALS.md "Program read 2026-09-28"): models from `Map/st00.alr` (24-entry container) + animations from `Map/st00.dat`, loaded into a {model, anim} table at `0x140530390` by `FUN_1401d9210` (names at `0x1403ee490`). Render objects `pdvkCapsule2` (vtable `0x14048cae8`, made by `FUN_14021b270`, matrix `FUN_14021b3d0`, idle-orb draw `FUN_14021b840`, camera-near fade `FUN_140196c40`), `pdvkCapEffect` (vtable `0x14048cb20`, birth / flare draw `FUN_14021ba60`, tick counter `FUN_14021ba20`), `pdvkEnvCapsule` (vtable `0x14048cb58`). Pickup / erase bursts `FUN_14021b4f0` (3 = pickup `cap03_death`, 4 = erased `cap04_braek`). **Capsule colour table `0x1402dca00`** (ARGB, by s9): white, red, blue, purple (erase), green (status), yellow, aqua (environment). Shaders `vkCAP02(M2)`, `vkCAPM2M/A`, `vk_cappsprite(V)`.
- **Capsule draw recipe** (2026-09-28, full write-up CAPSULE-VISUALS.md "Draw recipe (decoded 2026-09-28)"): **billboarding is per joint** — skeleton-chunk handler `FUN_140296190` replaces the rotation of any joint with flag 0x200 by the transposed view rotation (view matrix `0x1405a5ca0`); the attack core sphere is not billboarded, its rim cup is. **0x13 chunk → UV scroll**: its load handler `0x140295f90` parks the chunk in `0x140582420`, the next 0x2 stores a relative pointer at +0x64 (`FUN_140294e10`), the draw record carries it (`FUN_140294fa0` record[4]), and the shader's per-draw function (`FUN_14023c440` and siblings `14023d210/d630/cc80/c760`) uploads c90/c91 = T × floats(+0x18…) [+ floats(+0x38…)]; T = `DAT_14057f398`, +1 per update in `FUN_1401c7630`; c_SCALES.z = 8 (`0x1402f8750`). Sampler WRAP set in `FUN_14023c320`; blend from material u16 +0x14 via `FUN_1402a6010` → `FUN_14028e4e0` (tables `0x1402f77c0` / `0x1402f7798`, GL enums); specular = material bytes +0x34 × 2/255 × light (`FUN_1402a75e0`); near-camera fade = 32-level stipple via the draw state word (`FUN_1402a5f60`). Draw buckets: capsule flag `DAT_14057f380 | 0x40` → list `DAT_140577b10` (drawn in `FUN_1402990a0`); flag-0x10 materials → depth-sorted list `DAT_140577b50` (`FUN_140298dc0`, drawn in `FUN_1402991d0`).
- **Powerless drain** confirmed: with nothing left to use, Health −1 every 91 ticks (`FUN_14024df30`; ours 3 s ✅).

### Other states met on the way

State 1 walk / run; 2 unused forward slide (2.59 u/tick, `FUN_14025af60`, no setter found); 3 special move (dash skills, ground dodge); 4 jump wind-up; 5 airborne; 6 landing; 7 floating; 8 turning / strafing; 9 and 13 short recoveries; 20 paralysed struggle; 21 regenerating; 22 invulnerable; 24 Mephisto's Pact; 28 emote.

## Character parts / cloth (jacket lane, 2026-09-27)

Question: why does the Hero's (pc00a) jacket look long and flat in our build when the original's is short (X-harness down to a black band, short green straps)? Answer: the lower jacket is a **cloth**, simulated by the program every tick; we had it rigid (or on a home-made pendulum). Same rules as above: our own words, addresses to re-find things.

### How a character model is drawn

- **ALR chunks are executed as a display list.** `FUN_14012e6a0` walks a model's chunks and calls a handler per chunk id from a table at `0x140410608` (16-byte rows: 0x1 materials `0x140293d60`, 0x2 draw `0x140295420`, 0x3 skeleton `0x140296190`, …); characters use a second table at `0x140410778` whose skeleton handler (`0x140296910`) only points at the joint table, because the fighter's matrices were already computed by its animation controller (`FUN_14012fb80` draws the fighter).
- **Per-joint draw visibility** ✅: the draw handlers (`0x140295420`, `0x140295260`, `0x1402958a0`) skip a draw when its transform joint has **bit 0x800 in the u16 at joint +0x24** (the same word FORMATS-ALR calls "flags"). No joint has 0x800 in the files. At spawn (`FUN_1402510a0`) the program sets 0x800 on every joint with **0x400** — in pc00a0 those are the 16 helper joints carrying the position-only (format 0x0B) draws, which we already skip. `FUN_14019b920` swaps 0x04 ↔ 0x400 joints for a special 4-pass render (not read further). So **no costume layer is hidden by a data flag** — the jacket geometry we draw is what the game draws.
- **The colour file's extra draws / joints** (pc00a0 vs pc00a: +13 draws, +14 joints): joints 3–18 are unnamed group nodes under joint 2 (flags 0x04 / 0x14), and the extra draws are **head pieces** (palette = HEAD only: eyes, a mouth/eyelid piece in three depths, hair tufts, left/right pairs) re-split onto those group joints. Nothing on the jacket. Whether the program shows only some of them at a time (a blink / expression switch via 0x800) ❓ — we draw all of them.
- **Joint names are packed strings** ✅: the u32 at joint +0x2C is a base-40 number over " 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_" (charset at `0x1402f7670`), e.g. HEAD, BODY1, BODY2, HIP, LMOMO, RHAND, CAMERA, CLP00, CLC00, CL021, COL20, COLF00 (`pd_formats::alr::joint_name`). The program recognises special joints by numeric name ranges.

### The cloth system ✅ (`FUN_14025d380` set-up, `FUN_14025e520` per part, `FUN_14025ec30` per tick)

- **Every extra skinned model in a character file is a cloth part.** At spawn `FUN_14025d380` (fighter +0x13b80) builds up to 32 parts from the ALR offset-table entries **after the body** (entry 131 on, i.e. our models 1, 2, …). It stores the world-matrix pointers of body joints named **CLPnn**; a part's root joint named **CLCnn** then takes CLPnn's world matrix every tick, times its own local rotation and scale (its local position is ignored — `FUN_14025f610`). pc00a: body joint 77 "CLP00" (child of BODY2, the chest) ↔ part root "CLC00".
- **Grid** (`FUN_14025e520`): part joints with flag 0x100 and a name from CL000 to CL9FF are the nodes; name character 4 = **row** (a strip), character 5 = **column** (down the strip). pc00a: 5 strips × 4 joints (CL00x front-right, CL01x right side, CL02x centre back, CL03x left side, CL04x front-left). Per node, from joint +0x30: **velocity kept per tick** (pc00a 1.0 / 0.8 / 0.8 / 0.5 down each strip) and **angle limit in degrees** (+0x34: 10 / 10 / 15 / 10; × 0.0174533 at `0x1402f7c84`); if the three floats at +0x30 are all 0 the defaults are 0.9 and 60°. Rest lengths along strips and between neighbouring strips are measured once from the bind pose (`FUN_14025fc30`). (pc00a's CL030 has limit **0** — that strip's top joint cannot turn; kept as data.)
- **Per tick** (`FUN_14025ec30`, called from the fighter update `FUN_14024ec80` right after the animation): (1) the part's joints are posed from the animation (FK from CLPnn); column 0 is pinned there. (2) Every other node moves by its velocity plus **gravity 0.12 u/tick²** down (the set-up passes 1.0 × `0x1402f7ce4` = 0.12, the fighter's own gravity). (3) `FUN_14025f920`: down each strip (`FUN_140260550`), the node is put back at its rest distance from the node above, then the joint above is turned toward it **by at most that joint's angle limit, measured from its animated rest direction** (`FUN_140260130` → `FUN_1401950c0`), and the node is re-placed at the end of that turned joint; then neighbouring strips are pulled back to their rest spacing, half each, starting from the middle strip and working outward (`FUN_14025ff60` with 0.5). (4) velocity = movement this tick × the node's kept share, capped at **1 u/tick** (`FUN_14024ec80` passes 1.0). (5) Every joint is aimed straight at the next node (no limit, 2π at `0x1402f7f34`) and the part is drawn.
- **Collision** (`FUN_140260750`, shapes built in `FUN_14025df70`): body joints named **COLFgk** (groups of 3–4 joints = triangles; a node behind the triangle's plane and inside it is pushed out along the normal) and **COLPgk** (capsules between consecutive joints, radius = joint +0x30 × 5). The second float at +0x34 of these joints is a bit mask of which parts they apply to. pc00a has **no** COLF/COLP joints (its COL00–COL22 names are outside both ranges), so its jacket collides with nothing; pc03a–06a, pc08a, enm10a/11a have COLF. **Not reproduced yet** ❓.
- **Why ours looked wrong**: WAIT0 bends the chest (BODY2) ~45° forward (55–63° relative to the hips), and the lower jacket hangs from CLP00 on the chest, so a rigid jacket (and our old pendulum, which also lags and swings) juts out behind like a tail — seen from the camera behind and above, that is the "long flat green flap" (the jacket's inner lining). In the program, gravity turns each strip joint back toward hanging by up to 10° + 10° + 15°, the strips stay linked side by side, and the lower harness and band show again (`shots/jk_sheet_wait0.png`, column C).
- **Ours now** (`pd_game/src/dangle.rs` "The program's cloth grid", 30 Hz fixed steps; converter writes `cloth_row / cloth_col / cloth_keep / cloth_limit` node extras and grafts CLCnn onto CLPnn). Calibration switch `PD_CLOTH=game|pendulum|free|nocross|unlock`, viewer `--no-cloth` = rigid. Unconfirmed ⚠️: gravity 0.12 comes from reading the set-up arguments in the disassembly (register passing) — the look barely depends on it, since the strips settle at their angle limits either way.

## Object life: the arming tick, blasts, Reverse (shotclass lane, 2026-10-03)

Every attack object (shots, mines, holes, rain, the delayed instants) goes through one per-tick updater, `FUN_1402638d0`, and one creator, `FUN_140263a00`. Read:

- **A new motion object starts in state 1** (`FUN_140263a00`: `+0x10 = 1` when s43 = 0; the s43 objects start at 2). The first call of the updater only turns state 1 into 2 and **returns**: nothing moves, counts down or is tested. The object is made inside the caster's update and the pool is updated after the players in the same frame, so the arming tick is the fire tick itself and the first real update is **one program tick after the fire tick**. LIVE: every attack take's fire -> first effect is 1 tick longer than a sim that steps at once (all 16 punches / swords: fire 59, health change 60; Meteor 24 vs 23; Crawling Bullet 2 vs 1; Tremor / Earthquake 32 vs 31), the four delayed instants (Meltdown, Quantum Decay, Quantum Amp, fire 69: Level changes at 95 = fire + s65 25 + 1), and the Sensor Mine (fire 43, goes off 103 = 43 + s66 60: the counter's first decrement is the tick after the arming one).
- **The age counter (`+0x1c`) is counted up before the family's handler runs**, so a handler's speed ramp (`FUN_140264710`, crawl `FUN_140266290`, tornado `FUN_140268940`) sees 1 on its first real step. With both rules Psycho Sniper (a ramp from 0) is unchanged and every constant-speed shell arrives one tick later; the 15 shells with a free flight (Bomb Shot ... Illusion of Death) land within a recorder tick (+-1) of the take.
- **A blast is the same object turned into state 3** (`FUN_14026dbd0`: life 30, handler `FUN_14026ae10`, radius `(30 - life) x R / 30`): its first update is the tick after the explosion, so the sphere starts testing one program tick later (Big Bomb: landing fire + 24 LIVE, hit +14 after it).
- **Reverse / About Face is code 6 of `FUN_14026cd70`**: the object keeps flying (state 5 -> 2, init re-run, owner and target swapped, hit mask = everyone but the holder) **with `+0x20 = 0x14`: it waits 20 program ticks** (the updater counts `+0x20` down before it moves). Reflect (code 5) waits 0. LIVE `034_Reverse`: P1's Blaster fired at 65, hits P1 back at 88 (our shell turned and hit at once).
- **A mine sits on the floor under the fire matrix's point** (`FUN_140267b90`: probe from y + 10 to y - 10 at the matrix's x / z; the matrix is the cast effect's marker or the hand, not the caster's feet), and the sensor test (`FUN_140267e90`) runs on every collider with the mine's hit mask untouched (the owner's bit is only cleared by the shell init `FUN_140264570`), so the owner sets it off too. LIVE Sensor Mine: owner hit 4 ticks into the blast, foe at 50 u 28 ticks in.
- Movement specials (s43 = 3: Dash, Jump...) leave the cast state (10) for state 3 at the fire tick (LIVE `240_Dash`: 10 at 39, 3 from 40 to 59). Aura refill and controls pause in both states alike, so the sim keeps its Cast through the move and the film tool maps state 3 after a cast to "still casting".

## Target point: where a fighter "is" for every object test (residue lane, 2026-10-03)

READ: a fighter's collider entity (the `+0x80` block of the fighter, registered by `FUN_14024d440`) has its
`+0x60` / `+0x68` (= fighter `+0xe0` / `+0xe8`) filled each tick by `FUN_1402513a0` from the **world matrix of the
bone at fighter `+0x13dc0`: x / z of its translation row (`+0x30` / `+0x38`)**. The five-entry bone table at fighter
`+0x13db0` is built by `FUN_14025d380` from the name hashes at `0x1402dcdd8`: LHAND, RHAND (`+0x13db8`), **BODY1
(`+0x13dc0`)**, ALPHA, HEAD (`+0x13dd0`, the lock camera's aim origin). Everything that asks "where is that fighter" reads
that point, not `+0x12e40`: the melee dash (`FUN_14025be40`: bearing and distance to stop at 11), the press's aim capture
(`FUN_140258180`), every attack-object hit test (`FUN_14026c340` melee, `FUN_14026c1c0` shots) and the knock-back angle
(`FUN_14024fd80`: `atan2(attacker.x - target.x, attacker.z - target.z)`, the attacker taken at its own position).
- MEASURED offset of that point from the fighter position, in the fighter's frame (forward = (sin f, cos f), right = (cos f,
  -sin f), program space): a still fighter's lock-on facing points at it. pc01a (-2.76, -1.74): 265 takes, 0.58 deg RMS
  (median 0.11); pc00a (-1.96, -0.50): 43 takes, 0.10 deg RMS. Other characters not measured (zero in `body.rs`).
  UNCONFIRMED: the offset follows the BODY1 bone through the current clip (the idle pose's value is what is used; a
  fighter in a reaction keeps its own position in our sim); a Python FK of BODY1 + root track in WAIT0 does not reproduce
  the numbers (pc00a -0.4, pc01a -1.9 forward), so the pose / model matrix factor is not understood.
- Dash formula check (`FUN_14025be40` tail, disassembled): step = (10 / window) x s65 capped at 10; `dist <= 11` -> no move;
  `step > dist - 11` -> move `step - dist + 11`; `dist` is to this target point. With it the take's Swift Kick partial steps
  (1.06 and 3.53) are reproduced.
- Knock direction over 21 melee knock-downs: attacker position at the hit vs the victim's target point 2.8 deg median error,
  vs the victim's position 16 deg.
- Melee blows are objects: armed one program tick after the fire tick (`FUN_1402638d0`); LIVE fire -> health change 1 tick.
- Psycho Kinesis / Power with no object in reach still cast (LIVE takes 018 / 017: state 10 for the clip, fire at press +
  10, 2 Aura); the creator just finds nothing.

## Open questions

Settled in phase 3: lock state for the melee dash (band 1 = target within 80 u ✅), root-motion clips ✅, charge re-arm and STR ✅, player gravity and the jump ✅, what s43 = 1 / 3 / 4 do ✅.

Settled in phase 2: animation rate (30 fps ✅), the multi-shot fan (SHOTnn markers + s71 tilt step ✅), impact cases 5–11 ✅, the "always 0" bytes past s69 (they are s70–s73 and are used ✅).

- ❓ The effect clock: whether effect frames advance 1 per tick (probably, same update loop; not traced).
- ✅ Melee hit point (2026-10-01, lane shotpoints): the `SHOT01` marker of the running effect's **slot-1** models (none for any melee skill), else -- through `FUN_14018a310`'s fallback -- the owner's `RHAND` bone matrix at that frame (fighter +0x13db8 is only the fallback of the fallback). Correction to "Multi-shot and spawn points" above: the markers live in slot 1, not the cast effect's slot 0; the marker is a fixed offset in the caster's frame (no animation), and only the translation of the fire matrix is replaced. See FORMATS-EFFECTS.md "Shot markers".
- ❓ Whether a cast interrupted by a hit still spends the skill / its Aura; whether each chained combo hit pays Aura again.
- ❓ s55 = 1 and s55 = 2 wall handling (one and two skills).
- ❓ Crawler floor snap vs wall test (does a crawler end on steps?).
- ❓ Statuses 11, 12 (cleared on every hit; 11 = floating, so a hit ends Skywalk ⚠️), 20 (Pierce for all?), 31 (cancels Pierce). Settled in phase 3: 8 = invisible to lock-on, 33 = no damage (Void), state 22 = Invulnerability.
- ❓ The s45 rule functions at `0x1402dd0a0` (our notes-based table is probably right; not cross-checked).
- ❓ **Level-up from Aura Particles** and the real Level cap (not located; `FUN_14024d970` only clamps to 0–99 in versus).
- ❓ The per-clip int at controller +0x114 + 16 × clip is taken as the clip length in frames (it matches every timing checked); where it is filled was not found.
- ❓ Ground-dodge distance (~80 u = 40 u scripted + the clip's own drift) and whether JD / Cuff Button's "tumble" really locks 40 ticks like the cartwheel — **footage**: film a JD tumble and an Edgar cartwheel from the side.
- ❓ Dodge-jump airtime: the program gives ~20 ticks (0.66 s); footage round 2 read 13–15 frames by eye — re-measure take-off / touch-down.
- ❓ Camera settings in `setup/configdata.dat`: which of the 13 floats is which.
- ❓ Warps (`FUN_140257e90`) and what each environment crystal does once placed (`0x14058bd30` readers).
- ❓ Effect kinds 16, 20, 22, 23 of the status body (named from the skills that use them, not read in detail).
- ❓ Cloth collision shapes COLF / COLP (`FUN_14025df70`, `FUN_14025dab0`, `FUN_140260750`): read in outline only, not built.
- ❓ Which of pc00a0's head pieces (draws on group joints 3–18) the program shows at a time.

## Effect attachment (knife-effect lane, 2026-09-27)

- Each effect piece names an attach point (RHAND, RSWORD, BODY2, …) that is one of the target character's own bone names; the program looks the name up in the caster's (or victim's) skeleton and places the piece on that bone every frame (`FUN_140189420`; bone lookup `FUN_14018a7d0` / `FUN_14018aa50`; `FUN_14018aaf0`). Our converter writes the names and `fx/bones.json`.
- Unknown element kinds decoded: `4ca477d9…` = BLADE01/02 (sword swing ribbon, `pdEftBlade01`, vtable `0x140479bf0`), `650e9921…` = FLAIR01 (flat ray burst, `pdEftFlair1`, vtable `0x140479d70`). Details: FORMATS-EFFECTS.md.
- Correction: Psycho Knife's "slot 1 blade trail on the sword object" is wrong — it is a BLADE01 ribbon in slot 0 that records the knife hand's path.


## Stage contacts and impact marks (surface-sound lane, 2026-10-09)

Full write-up: `audits/P0-SOUND.md` section 13.

| Address | What it does |
|---|---|
| `1401631e0` | builds the "player bullet" probes: `DAT_1404bec68` (sweep, shape 5; shots, `1401633b0`), `DAT_1404becb8` (sphere, shape 1; landings + blasts, `140163b50`), `DAT_1404bec88` (`1401644b0`), `DAT_1404becb0` (`140250cd0`, `140130dd0`) |
| `1401d4070` | stage collision node list `DAT_1405322c0` from the `.dat` 0x94 chunks, file order |
| `1401d4ff0` / `1401d5530` | register stage nodes in the spatial tree / query it against a probe |
| `1401d5860` | one probe pass: bumps the pass counter `DAT_1405300dc`, tests the probe against objects and the stage |
| `1401d5130` | pair test via `DAT_1402dc2f0[a*8+b]`, prepends contact records (+1 class, +8 pass, +0x18 other shape, +0x28 point) to both lists |
| `1401d7fd0` | sphere vs face chunk (two-sided plane distance < r, inside or edge) |
| `1402021f0` | per-tick contact walker: first contact per sphere probe in list order, nearest per pass for a sweep; calls `14021d3f0` |
| `14021d3f0` / `1402422a0` | mark type by class / place the mark (`1401c9210`) and play the hit sound only if placed |
| `1401c9210` | mark ring (32): refuse inside a live mark's model radius |
| `1401c8c10` | mark type entry: model bound radius (+0x58), box (+0x40..0x54), animation (+0x68) |
| `1401c8e60` | mark ageing (0.5 per call, only with an animation) and camera-in-box removal |
| `1401d9210` | model slot table `DAT_140530390`: `Map\st00` (slots 0-23), then `Map\st%02deff` |

## Melee dash order, knock-back origin, melee line of fire (gameplay-accuracy lane, 2026-10-09)

Trigger: the 2026-10-09 film list had six "knock-down without a landing" skills. The take traces show no
second hit: in 061 / 188 / 322 the knocked body flies off a deck edge and **falls out** (y drops past 240,
Health -3, respawn on the start spot), and in 023 / 024 the knocked body is the CASTER, hit by its own mine.
Our bodies landed instead because they flew 13-96 deg off the original's direction. The reaction rules
themselves (rows, kinds, launch speed / angle) were not wrong: 1.19 u per program tick at the row's angle
in both.

| Address | What it does (read 2026-10-09) |
|---|---|
| `14024ec80` | player update, one per program tick: state handler `140256e80` (the melee dash `14025be40` is one) **before** the clip update `140257510` (clip time + root motion), then the physics `140254590` (body push-apart `140250780` first) |
| `14025be40` | melee dash: distance from the fighter position `+0x12e40` to the target's point (`+0x60` of its collider), turn, `<= 11` no move, else step `+0xa8` cut to `step - dist + 11` when closer than a step, `14024f9c0` moves it |
| `14024fd80` | reaction picker: knock-back angle `atan2(obj+0x140 - victim+0x60, obj+0x148 - victim+0x68)`; `obj` is the attack object; s43 = 0 with s63 = 4 uses the victim's own facing instead |
| `1402638d0` | object update: copies the matrix `+0xd0..+0x10c` into the previous-matrix block `+0x110..+0x14c` before the family handler runs |
| `14026a0d0` / `140269db0` / `140265e00` | punch / sword / pursuit handlers: every update write `+0x1a4..+0x1ac` **and** `+0x140..+0x148` = the owner's position with y + centre height (`+0x12e98`); the matrix `+0xd0` = SHOT01 marker or RHAND |
| `14026c340` / `14026c730` | punch sphere / blade tests: reach test, then `1401641b0(obj+0x1a4 -> victim point)` must be clear before the guard test and the damage |
| `1401641b0` | line of fire: probe the stage tree, any contact of class != 0x10 blocks; then each fighter in the mask gets its own ray test |
| `1402513a0` | the collider point `+0x60..+0x68` = BODY1 world x / y / z |

Consequences, all READ:
- **Knock-back origin of a melee blow = the caster's own position** (`+0x140` is rewritten from the owner by the
  handler, not the hand matrix). This confirms the residue lane's "attacker position" measurement.
- **The dash measures before the root motion**: per program tick the dash step is decided from the position
  the body had at the end of the last tick; the cast clip's root drift is added after it. LIVE `188` rec 53 -> 54:
  distance 17.9 to the target point, step 8 cut to 1.1, root ~2.0: 3.13 measured (we had measured after the
  root drift and stepped 4.4).
- **Bodies are pushed apart once per program tick, after the whole dash step.** LIVE `188` rec 55 -> 56: the
  attacker's 9.4 u step ends 2 u from the target's centre; the push (half the overlap each along the line
  between them) sends the target 4 u sideways and the attacker past it on the other side, so the punch lands
  from the east and the target flies west off the deck. Our 60 Hz sim walked the step in two halves with a
  push after each and shoved the target ahead of the dash instead (the knock then went 96 deg off).
- **A melee blow needs a clear line from the caster's centre to the target's point.** LIVE `242_Dhampir_Fang`
  (target stands inside the class-2 box x -9.8..19.6 / z 79..96, the recorder's anchor) and `317_True_Muramasa`
  (a 25 u class-2 wall between the two): the original never lands; ours did.

Not changed (UNCONFIRMED / set aside):
- `322_Knife_Grind`: positions now match to ~0.5 u, but our body hits guardrail object 85 (`st06_guardrailB_`,
  hull box x 179.9..220.1, y 305.5..322.4, z 290..292.3) and stays on the deck; the original's flyer passes
  through it. Over all 341 takes the original's bodies cross guardrail hulls only in states 17 and 14 (Psycho
  Wave, Sensor Mine, Vacuum Palm, Hyperkick, Knife Grind; never walking). Whether reacting bodies ignore object
  hulls, or the recorder arena's rails were already damaged, is not read (COLLISION.md "eight prop hulls").
- `023` / `024` (own mine knocks the caster): the mine lies within ~1 u of the caster's body point, so the
  knock direction is decided by sub-unit differences (where the hand put the mine, the BODY1 sway). Our
  direction differs (023: -62 deg vs -2; 024: -153 vs -107); no rule found to change.
- `064_Photon_Burst`: placement fallback (the take's spot is not on st06), set aside.

### Follow-up reads (same lane, 2026-10-09 evening)

**Arc and multi-laser handedness (BUILT).** `FUN_140290180(m, a)` mixes rows 0 / 1 (row 1 -> row 1 cos a - row 0 sin a: a roll about the
forward row) and `FUN_140290050(m, a)` mixes rows 2 / 0 (row 2 -> row 2 cos a + row 0 sin a: the heading turns toward row 0).
`FUN_14012e6f0` (look) builds row 0 = (f.z, 0, -f.x) normalised: in the program's left-handed space that is the object's physical
right. Our export mirrors z, so our `Frame::look` right (`Y x fwd`) is the program's LEFT (the target-point offset measured the
same sign flip, `body.rs`). The arc init `FUN_1402666b0` (fire matrix rows from the caster, roll by s68 + k s71, then turn by s67 + k
s70, R = |aim - spawn| / (2 sin bend), rate -(speed / R), curve ticks 2 x trunc(bend / rate), life + 30) and update
`FUN_140266bf0` (turn by the rate, re-aim along the tangent through the target, `FUN_1401950c0` capped at s66 x 0.1 deg x homing,
5 deg away from a Course field) were otherwise as built; the ricochet init `FUN_14026af40` calls the same two helpers. We had
every arc and multi-laser flying the mirror of its arc. Video: `309_Piece_of_Wind` (bend +15) leaves to the right of the target
on the caster's screen and comes back from the right. Fixed with `Frame::roll_program` / `yaw_program`.

**Fighter bodies and object hulls (read, nothing to build).** The fighter's body probe is registered by `FUN_140254500` as category
1 with mask 0x1900; an object's hull colliders (`FUN_1401fbbc0` -> `FUN_1401d3ef0`) take category `+0x138` = 0x100 (0x800 when the
model's flag 0x10 is set, `FUN_1401fba40` defaults) and mask 0x44. A pair is tested when either mask holds the other's category
(`FUN_1401d5860`, `FUN_1401d5530`), so fighters meet object hulls in every state; the mover `FUN_140255020` skips only class 0x1F
contacts and class 0x14 in states 5 / 7. No state-dependent mask was found. So Knife Grind's flyer stopping on guardrail 85 is
what the program would do with the rail intact: the recorder arena's rail was most likely damaged or gone (UNCONFIRMED, arena state).

**Fall aim (read, matches ours).** `FUN_140265470`: the aim point from `FUN_14026e410` (the target's collider point = BODY1 x / y / z,
pulled by `FUN_140273d40` along the 3-D line from the spawn point to 80-300 u), the start s64 x 10 above and s65 x 10 back along the
yaw-only matrix, speed s66 / 10.8, life 2 x distance / speed, steer s67 / 10.8 x homing. Ours is the same. Thor's Hammer 261 still
hits in ours and misses in the original; the original's take plays no surface or hit sound at all after the fire (no 0.7 / 0.526),
so its hammer touched neither the floor nor the target: reason not read (UNCONFIRMED).

## Fire-matrix pitch, ricochet contact normal, silent expiry (gameplay-accuracy lane round 2, 2026-10-09)

| Address | What it does (read 2026-10-09) |
|---|---|
| `140258180` | the press: with a locked target, an attack (s43 = 0) that is not motion 8 / 16 / (4, s64 = 2): bearing `+0x88` = atan2 to the target's collider point, and **pitch `+0x8c` = atan2(caster y + centre height `+0x12e98` - target point y, horizontal distance)** from the caster's last-tick position `+0x12e4c` (written at the end of every player update, `14024ec80`); otherwise pitch 0 (controller `+0x8c` = fighter `+0x12f8c`) |
| `1402514c0` | the fire matrix `+0x12dc0`, rebuilt every player update: identity, translation = position with y + centre height, yaw by the facing `+0x12e68` (`1402906e0`), then **pitch by `+0x12f8c` (`1402905c0`: row 2 -> row 2 cos a - row 1 sin a, row 1 -> row 1 cos a + row 2 sin a; a positive angle tips the heading down)** |
| `14025c720` | the fire countdown hands `fighter + 0x12dc0` to the fire `1402626d0` as its matrix (only the translation is replaced by a SHOTnn marker / the hand, `14018a310`) |
| `1402666b0` / `14026af40` / `140264570` | arc, multi-laser and shell inits copy that matrix's rows into the object's own matrix `+0xd0`: their heading starts along the pitched facing (the arc's `14026e410` builds a yaw-only matrix toward the aim point but the init overwrites it with the fire matrix rows; only the aim POINT is kept, for the circle's radius) |
| `1402513a0` | the collider point is BODY1's world x **and y** and z (8 bytes from bone `+0x30` = x, y; then `+0x38` = z): the target point has a height, used by the pitch above, every aim point (`14026e410`) and the body hit sphere (`14026c1c0`: segment `+0x140 -> +0x100` against a sphere at that point, radius object `+0x190` + body `+0xb0`) |
| `14026ca50` | object wall test: s55 = 1 (Ricochet Laser) or a call with `param_2 = 0` uses `140163680` (probe `DAT_1404beca8`: segment, no `pdStageColPtr_PlayerBullet` owner, category 1, mask 0x1900) instead of `1401633b0` (`DAT_1404bec68`, owner = the player-bullet pointer, category 4): no impact-mark / surface-sound path for its contacts. Motion 17: a contact whose heading . normal >= 0 is **ignored** (return 0: no turn, no count, position kept); else counter `+0x1c8` - 1; at exactly 0 with a target: aim at (target point - contact) if that is on the normal's side, else reflect; position = the contact point |
| `140163e80` | the contact a wall query returns: the nearest to the segment start of the face contacts (type 3: point `+0x28`, **normal = the face's stored normal `+0x38`**, not turned toward the segment) and body contacts (type 1: normal (0, 1, 0)), skipping classes 0x10 / 0x12 / 0x14 / 0x1F |
| `14026e720` | per object per tick: hit tests (`14026c1c0` per collider), then the wall test, then the life count; at life 0 `14026cd70(obj, 0)` |
| `14026cd70` | code 0 goes to the switch's default: state 0, no slot effect, no s21 sound (cases 1 / 2 / 3 play s21) |

Built (READ):
- **The fire matrix pitch** for arcs and multi-lasers (`moves::Launch::pitch`, `Frame::fire`; captured in `cast_from`). The target point's height is `Body::point_height` (MEASURED: BODY1's world y in WAIT0 by forward kinematics, pc01a 11.89, pc00a 9.45, ... `pd_formats` probe `body1_height_probe`). Shells take the same matrix (`140264570`) but are NOT changed yet (they fly the flat facing; not filmed).
- **Ricochet contact normal**: our stage triangles are wound to their stored normals, so the bounce takes the triangle's own normal; a face it moves along is ignored. Object hulls keep their raw winding (not wound to the OC00 stored normal), so there the normal is still turned toward the laser (UNCONFIRMED for hull faces).
- **A shot whose life runs out is silent** (`CombatEvent::ShotExpired`, sound layer only): `sfx.rs` voiced every non-hit end as a wall end (s21).

Film (pinned `pd_game_archand_1009` -> `pd_game_arcpitch2_1009`; `compare_arc_after` -> `compare_arc2_after`): Piece of Wind 2 -> 0 (clears the class-27 box and lands 23 program ticks after the fire on class 6 like the original), Ricochet Laser 3 -> 0 (no hit; our lasers had spent their three bounces on faces they were leaving -- the first step after a bounce starts on the face -- and then turned on the target), Laser 1 -> 0, Sonic Boom 1 -> 0, Slide Laser 1 -> 1 (now 0.509 + 0.7: it meets an object), Twist Laser 2 -> 2.

Twist Laser 326 (read, not a rule): its arc is clamped to 300 u (s44 = 1), so only the heading matters. Ours meets tank object 260 (st06_tankA_, 9 program ticks). With tanks 259 / 260 gone the program path flies on and lands on class 5 (0.8) 18 program ticks after the fire, exactly the original's tick and sound, for any heading 0.3-1.5 deg off ours; at our exact heading it grazes the corner of a class-2 box (x 174.1, z 193.9; no sound). Object 259 is one of the hulls the original's fighters stand inside in the recorder arena (COLLISION.md), so the take's tanks were most likely already broken: arena state, UNCONFIRMED.

Tool: `PD_SHOT_PATH_LOG=1` (render side, `sfx.rs`) logs every shot's program position each sim tick (`shotpath tick ... pos x,y,z`) into a film run's `out.txt`.

## Body-point film gate, crawler ride height, tornado column (gameplay-accuracy lane round 4, 2026-10-09 night)

| Address | What it does (read 2026-10-09 night) |
|---|---|
| `140266290` | crawler update: after the move it calls `140163cb0(pos, r)` with **r = the object's radius `+0x190`** (5.0 from `140263a00`; disassembled: `movss xmm2, [rbx+0x190]` before the call). No contact: vertical speed `+0x1c8` -= 0.12; contact: steer, vertical speed 0 |
| `140163cb0` | sphere against the stage: every face contact (type 3) of the query, skipping classes 0x10 / 0x12 / 0x14 / 0x1F (mask `0x80150000`), goes through `140195f30`; with no stage (`0x7777` mode) y = max(y, r) |
| `140195f30` | sphere / triangle push-out: when the centre is within r of the face's plane (and on its front side) and projects inside the triangle, it is moved to exactly r along the face normal; else the nearest edge point within r pushes it out to r. **So a crawler's centre rides r = 5 above the floor** (ours rides 1 u above it, `moves::crawl`) |
| `140268590` | tornado init: identity matrix moved to the object's translation (the marker), yaw `+0xc0`; floor probe from y + 20 down to y - 20 (none: code 0); column = s66 x 10 - 2 from floor + 1, shortened to (roof - floor - 1) - 1 when a face is in it |
| `140268940` | tornado update: move along row 2 by the ramped speed; floor probe y + 7.5 .. y - 7.5 (none: code 0); then **one vertical segment** from floor + 1 up by the column height (`140163680`): a hit puts x / z back to `+0x140 / +0x148` (where it stood), else y = floor + 1. **There is no sideways ray along the step**: an upright wall stops it only through the column (a top face, an overhang) or the floor test. The hit test (`14026c1c0`) is the column segment `+0x140` (top) -> `+0x100` |
| `14026e410` (param 0) | the lob's aim point is the target's collider point `+0x60..+0x68` (BODY1 x, y, z), through the range band `140273d40` (3-D scale to 80 / 300 or 300 / 600 u from the spawn): ours `Launch::aim` does the same |
| `14026ca50` | a non-Ricochet contact sets `+0x140` from the contact point and the contact normal (`140124740`) and ends the object with code 1 (the blast starts there; ours `at + n`) |

Built (READ): the tornado rule (commit "Tornado: only the column at the new spot stops it"). Film (pinned
`pd_game_head_ce67_1009` -> `pd_game_tornado_1009`, `compare_tornado_before` / `_after`, 311 re-run singly `*_rr`): Tornado 311
3 -> 0 (hit t91.5 vs the original's t91; ours had stopped ~11 u after the start on the sideways ray), Twister 0 -> 0, Whirlwind
0 -> 0, control set 0 -> 0.

Film gate of round 3's patch (`lane3_wip.patch`: shell fire-matrix pitch + every victim-side point at BODY1 height,
`Player::body_point()`), **failed, not committed**. Pinned `pd_game_shellbefore_1009` (fa724aa) vs `pd_game_body1_1009`,
247 ids (every family except self / instant / movement / environment / erase-field / copy, `compare_full_1003b`) + the control
set, back to back (`compare_body1_gate_before` / `_after`), every changed id re-run singly with both exes (`*_rr`, all 11
reproduced exactly): total 109 -> 114, clean 176 -> 176, control 0 -> 0.
- Better: Vacuum Slash 125, Muramasa Blade 245, Excaliber 321 2 -> 0 each (their blade hit 12 ticks late; now on the original's
  t58), Ki Lance 199 1 -> 0.
- Worse, explained: Fang of Tree 139 0 -> 3 and Crawling Bullet 333 0 -> 4 (now miss): our crawler rides 1 u above the floor,
  the program's 5 u (`140195f30` above): with the sphere at BODY1 (pc01a 11.89) ours passes 10.89 u under the centre (> 5 + 5),
  the program's 6.89. Trench Mortar 72 1 -> 2 (placement fallback take, catwalk). Celestial Ship 267 0 -> 2 (random rain drops:
  the drop columns start 2.9 u higher and two land on other surfaces; harness note on the take).
- Worse, NOT explained: Rock Shot 8 0 -> 1 (the lob aims 2.9 u higher, ends 7 u further on and loses the class-6 contact and its
  0.9 that the original and our old path both make) and Big Bomb 12 1 -> 2 (the bomb now lands 24 program ticks after the fire,
  the original's tick (was 23), but its growing blast reaches the body 17 ticks after landing, the original 14, ours before 15).
  Both say the effective target-point height is below 11.89 for these takes, where the swords and Piece of Wind (round 2) want it
  above 9. The BODY1 height is MEASURED by our FK, which does not reproduce the MEASURED x / z offset either: UNCONFIRMED.
- Cluster Bomb 11 (never-fired take): hit tick 94.5 -> 98.5, not counted.

Slide Laser 66 (path log, HEAD): the laser ends 18 program ticks after the fire on handrail object 137 (`st06_handrailA_`,
about (-261, 340, 151) in our space) with s21 0.509 + the rail's 0.7; the original's laser makes no sound at all. Same class as
Twist Laser 326 (tanks 259 / 260) and Knife Grind 322 (guardrail 85): a thin stage object the recorder arena may no longer have.
UNCONFIRMED, not changed.
