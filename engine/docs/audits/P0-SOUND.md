# P0 audit: SOUND (capsule lifecycle + all other audio) -- 2026-09-30

Read-only audit. Evidence: `decomp_all.c` + byte reads of `PDUWP.exe` (`engine/re/call_consts.py`,
`decomp_fn.py`, `pe.py`), `gsdata_en.dat` tables, `engine/cache/sfx/index.json`, `sfx.rs`,
`frontend/sound.rs`, `combat.rs`, `moves/erase.rs`. Nothing was run or changed.

## 0. How the program plays a sound (the key to everything below)

* **Every game SFX goes through `FUN_140276330(bank, id, id, 0x1000, 0x1000, &pos|0)`** (157 calls in 60-odd
  functions; `call_consts.py 140276330`). `bank` is the first arg (`xor ecx,ecx` at every site below),
  `id` is the second. The sound is the **plain id** in that bank, no packing (the packed `>>20` form is only the
  separate emitter `FUN_1401cbbc0`).
* **Bank 0 = our `common` bank, so id N = `cache/sfx/common/N.wav`.** Proof: lock-on `0,4` matched footage on
  common/4; footsteps `0,iVar` from the `DAT_1402f6270` table matched Laurence's ear (52/54); the orb appear
  `0,0x55` was confirmed by ear as common/85. Other banks: **1** = UI bank (ids 1, 2, 6 at `14021c210`,
  `1402214c0`, `140221950/70`, `140221f10`, `1402269b0`, `140234d10`, `140243250`, `1402440d0`; not yet mapped
  to a file), **3+player index** = that player's `BattleVo_*` bank (cast voice / hit voice, `FUN_14025a3e0`,
  `FUN_140258690`), **4** = also the enemy/boss bank (`FUN_14026ed60` sends ids > 10000 to bank 4 as id % 10000;
  the HUD/story calls `4,0x1e..0x34` at `1401372b0`...`14014ceb0` are out of scope).
* **Hex constants in the decompile ARE the common ids** (0x55 = 85 ...). The earlier "34 is not a sound" finding
  was about `FUN_14024d4d0(..,0x22)`, a status *query*, not a play call; `0,0x22` IS a real sound elsewhere (menu
  spinner tick at `14018eb40`, `140175840`; same WAVE clip as 35).
* **Gotcha that hid the pickup for a week:** Ghidra shows `FUN_140261a00(button, skill, 1)` with 3 args. The real
  call passes a 4th, `r9d` = "play sound". It is only visible in the bytes: `41 b9 01 00 00 00 / 45 8b c1` at
  `0x140251b8d` (r9 = r8 = 1). Always read r9/stack args from bytes (`call_consts.py 140261a00`).
* Sounds are positional in the original (the pos arg), volume arg is a constant 0x1000 (full). The listener /
  attenuation model (`FUN_140240a00`, `FUN_140241440`, `FUN_140241b10`, globals `1405336b8/c0`) is **not read**.
  Laurence's rule "both players hear everything" is what we follow (flat, non-positional).

## 1. Capsule lifecycle: what the program plays (the real call sites)

Capsule slot state (per player, 3 slots, `DAT_140585c9c` stride 0x48): 0 waiting (420 ticks), 1 appearing
(15 ticks), 2 present/pickable, 3 gone. `FUN_14014fd10(mgr, player, slot, mode)` = remove a capsule: **mode 0 =
taken** (effect `cap03`, **no sound**), **mode 1 = erased** (effect `cap04` + **sound 0x5c = 92**), mode 2 = silent
re-arm. The kind (colour) of the capsule is **never tested** in any of these paths: appear, pickup and erase are
the same sound for attack/defence/status/special/erase/environment/Aura.

| Event | Program path | Call site (byte-checked, `rcx=0`) | Sound |
|---|---|---|---|
| **Appear** (respawn after the 420-tick wait, state 0 -> 1, 15 ticks before it can be taken) | `FUN_14014ed10` per tick from `FUN_14014f320` | `0x14014ed6f` edx=0x55 | **common/85** (ear-confirmed) |
| **Opening fill of a round** | `FUN_14014ee70` writes state 2 directly (`puVar16[-9]=2`) | none | **silent** |
| **Capture into a button** (stand on capsule, press a skill button) | `FUN_1402517c0` mode `+0x17160 == 1` -> `FUN_140261a00(button, skill, 1, 1)` (`0x140251b8d`), then `FUN_14014fd10(..,0)` | `0x140261bc6` edx=0x56 inside `FUN_140261a00`, gated `param_4 != 0` and `(player+0x61a0)+0x10 == 0` (every playable player, not "local only") | **common/86**, at the capturing player's position |
| **Aura Particle capture** | same path; skill id 0 is an ordinary `FUN_140261a00` argument (no kind test) | same | **common/86** (= Laurence's "same as ordinary pickup") |
| **Erase by skill** (Abyss, Eliminate, Shatter, Evaporate, Violent Change, Change the World), one per capsule | `FUN_14026ff20` (s43=1, s45=0) -> `FUN_14014fd10(..,1)` (`0x140270106`, r9=1) -> `FUN_14014ed90` / second branch | `0x14014edc7` and `0x14014ff07`, edx=0x5c | **common/92** at the capsule's position |
| **Thieving Imp** (s45=1, capture onto the casting button) | `FUN_14026ff20`: `FUN_140261a00(..,0,1)` (`0x1402700e3`, r9=1) **then** `FUN_14014fd10(..,1)` | 0x56 then 0x5c | **86 + 92** |
| **Random capture** (`FUN_140263180`, r9=1 at `0x140263293`) | `FUN_14014fd10(..,0)` then `FUN_140261a00` | 0x56 | **86** |
| Particle Convert (s45=3), Change-the-World reroll (s45=2, `FUN_14014fa20`) | state/skill rewritten in place | none | silent |
| Capsule timed expiry (only when `DAT_1403e0304 % 10 == 1 && DAT_140530287 == 0` and the slot timer passed, `FUN_14014f320`) | `FUN_14014fd10(..,1)` | 0x5c | **92** (game-mode gated, see reads) |
| Round-start deal of the 4 hand skills (`FUN_14024dc60`, r9=0) | | | **silent** |

So Laurence's answers line up exactly: **92 is the ERASE sound, not the pickup** (the old docs had the
`ed10/ed90` attribution and the take path wrong), the pickup sound is carried by `FUN_140261a00`, Aura capture uses
it unchanged, and the opening capsules do not make the appear sound.

## 2. Activation (using the skill on a button): what the program plays

Pressing a button with nothing under you: `FUN_1402517c0` -> `FUN_140261e90` (check) -> `FUN_140258180` -> cast
start `FUN_14025a3e0` -> fire tick `FUN_1402626d0` -> object `FUN_140263a00`.

| Moment | Sound | Address | Applies to |
|---|---|---|---|
| Cast start | **action-table row w3** (`row+6`) in bank 0 at the caster (handle kept; stopped at the next cast) | `0x14025a506` | every kind |
| Cast start | caster voice, bank `3 + player`, id = random pick from the 10-entry list of voice group w4 (`gsdata 0x33004`) | `0x14025a5c6` | every kind that has w4 > 0 |
| Fire / object creation | s19 (`+0x22`) | `0x140262caa`, `FUN_140263a00` | attack kind only (s43=0, motion != 4) |
| Object in flight | s20 loop (`+0x24`), handle stopped on the object's end event | `FUN_140263a00` | attack, a few erase/special/status skills |
| Impact events 1/2/3 | s21 (`+0x26`) via `FUN_14026ed60`, **plus common/6** when the victim is a playable character (else **7**) on event 2 | `0x14026d144` (id 7 const; 6 via `uVar16`) | attack |
| Attack with "Explodes on impact" (s51 = 5) | **common/99** at detonation start | `0x14026dca4` (`FUN_14026dbd0`) | bombs etc. |
| Guard destroyed (defence object, event 10) | **common/14** | `0x14026d527` | defence |
| Attack erases a button on hit (`FUN_140272510` true) | **common/91** at the object | `0x14026fb8e` | attack with "Add: Erases" |
| Environment skill created | **common/93** (`FUN_14017bcc0`, `0x14017bdc9`), then **common/87 loop** (2 loop points, updated each tick, `0x14017b966`), ends with **common/94** (`FUN_14017ba00`, `0x14017bb6d`); taking the crystal plays **common/88** (`FUN_14017be00`, `0x14017bf70/0x14017c198`, `FUN_14017c280`) | see left | environment |
| Hit reaction row | row w3 sound at the victim (501-527 etc., bind rows 20-23 = **common/100**) + victim voice (groups 4/5) | `FUN_140258690` -> `FUN_14025cb00` | any hit |
| Knock-down flight ends (state 0x11) | **common/82** | `0x140254e3f` | any launch |
| Getting up (DOWN_FRONT/BACK_STAND) | **common/81** | `0x14025c4ee` | any knock-down |
| Can't use the button (can't pay, or status 5 / 0xc / 13+button) | **common/2**, non-positional, only when `(player+0x61a0)+0x30 != 0` | `0x140251b32`, `0x140261fa0`, `0x1402581da/0x140258277` | all kinds |
| Aura regen: every time the whole-number part of Aura rises | **common/5**, non-positional, same `+0x30` gate | `0x14024df1d` (`FUN_14024de20`) | regen |

**Per capsule kind** (from `gsdata` action rows, every skill's ground row w3/w4 counted):
attack: per-skill cast sound (200-330ish, voice 1/3) + s19/s20/s21; defence: cast 246 / 315-354 (voice 1), nothing
else (s19/s20/s21 all 0); erase: cast 355-386 etc. (voice 1/2/3), a few s20/s21; status (green): cast 383-438
(voice 1/2/3), nothing else; special: cast 290-500 (voice 1/2/3); environment: cast 387-405 (voice 2) + 93/87/94/88.
**Aura Particle has no table record: the program's record 0 is the header tail (text id 301, s43 = 4, s45 = 1 Level +1,
USES 1) and its ground row is 343, whose w3 = 0 and w4 = 0 -> activation is SILENT (no cast sound, no voice), and the
Level +1 effect is silent too (`FUN_140270b00` has no sound call).** A green (status) capsule therefore sounds
different from Aura simply because it has a row sound + voice and Aura has neither (matches Laurence's answer 4).
Collection (86) is identical for both. No capsule-kind-specific sound exists on collect.

## 3. Ledger

Milestones: Mapped / Implemented / Reproduced (runtime seen) / Verified (against the program's rule) /
Laurence-approved. No runtime was run in this audit, so Reproduced is from existing logs only.

| Feature | Mapped | Implemented | Reproduced | Verified | Approved | Evidence |
|---|---|---|---|---|---|---|
| Capsule appear 85 at respawn | ✅ `14014ed10` | ~ `sfx.rs::capsule_sfx` (also fires on the opening fill) | ✅ log `sfx: capsule appear 85 (6 new orbs)` (FRONTIER.md) | ~ id yes, trigger no | ✅ id by ear (FORMATS-AUDIO pass 2), DECISIONS (5) | `0x14014ed6f`; `FUN_14014ee70` silent fill |
| Opening fill silent | ✅ | ❌ | ❌ | ❌ | ❌ | `FUN_14014ee70` state=2, no sound call; our diff plays for all new ids |
| Capture pickup = 86 | ✅ (candidate) | ❌ (0 = silent, env `PD_CAPSULE_PICKUP`) | ❌ | ❌ | ❌ (92 rejected, DECISIONS (1)) | `140261a00` tail `0x140261bc6`; r9=1 at `0x140251b8d` |
| Aura capture = same as pickup | ✅ | ✅ same `Capsule` cue (silent now) | ❌ | ~ | ✅ DECISIONS (3) | no kind test in `140261a00` |
| Capsule erase = 92 (skill erase, per capsule) | ✅ | ❌ no event, no cue; `PD_CAPSULE_ERASE` in DECISIONS is not in the code | ❌ | ❌ | ❌ ("unsure") | `14014edc7`, `14014ff07`; erase sites `moves/erase.rs:388-430` are silent |
| Thieving Imp = 86 + 92 | ✅ | ❌ | ❌ | ❌ | ❌ | `0x1402700e3`, `0x140270106` |
| Capsule timed expiry = 92 | ~ (mode gate unread) | ❌ (sim has no expiry) | ❌ | ❌ | ❌ | `FUN_14014f320` |
| Activation cast sound (row w3) + voice (w4) | ✅ | ✅ `sfx.rs play_sfx` cast_seq | ✅ existing logs | ~ timing/lists differ (see D5,D6) | ❌ ("not correct") | `14025a506`, `14025a5c6` |
| Aura Particle activation silent | ✅ row 343 w3=w4=0 | ✅ (by omission; id-0 def has no row) | ❌ | ✅ by read | n/a | gsdata header record 0, row 343 |
| Green/status activation = row sound + voice, no effect sound | ✅ | ✅ by the same cast path | ❌ | ~ | ❌ | rows above, `FUN_140270b00` has no sound call |
| Attack s19 at fire, s20 loop, s21 impact | ✅ | ~ s19 at cast (early by fire_tick), s20/s21 ✅ | ✅ | ~ | ~ | `1402626d0`, `140263a00`, `14026cd70` |
| Environment 93 / 87 loop / 94 / 88 | ✅ | ❌ | ❌ | ❌ | ❌ | `14017bdc9/14017b966/14017bb6d/14017bf70` |
| Denied press = common/2 | ✅ | ❌ own synth buzz (`denied_wav`) | ✅ (buzz) | ❌ | ✅ buzz accepted 09-27 while id "unidentified" | `0x140251b32`, `0x140261fa0` |
| Aura regen tick = common/5 | ✅ | ❌ | ❌ | ❌ | ❌ | `0x14024df1d` |
| Hit: s21 + common/6 + victim voice | ✅ | ~ s21 + voice, no 6 | ✅ | ~ | ~ | `14026cd70` case 2 |
| Block: s21 at full level | ✅ | ~ we play it at 0.3 (invented) | ✅ | ❌ | ❌ | event 3 shares the s21 branch |
| Guard broken = common/14 | ✅ | ❌ | ❌ | ❌ | ❌ | `0x14026d527` |
| Explosion adds common/99 | ✅ | ❌ | ❌ | ❌ | ❌ | `0x14026dca4` |
| Erased-by-attack = common/91 | ✅ | ❌ | ❌ | ❌ | ❌ | `0x14026fb8e` |
| Knock-down land 82 / get-up 81 | ✅ (function roles) | ❌ | ❌ | ❌ | ❌ | `0x140254e3f`, `0x14025c4ee`, anim 0x3c..0x3f = DOWN_* |
| Footsteps: dry common/35, wet 52/54 cycle | ✅ | ✅ `footsteps.rs`, `SFX_FOOTSTEP` | ✅ | ✅ | ✅ DECISIONS 09-29 | `DAT_1402f6270` table; PROTECT, do not touch |
| Lock-on cue common/4 (ON only, 1/6 s late) | ✅ footage 0.84 + `1401536e0` (`0x140153843`) | ✅ `lock_on_cue` | ✅ | ✅ | ~ no explicit ear sign-off logged | sfx.rs docs, FOOTAGE |
| Pause exit = common/1 | ~ (ear; menu code plays `0,1` for cancel) | ✅ `Cue::Unpause` | ✅ | ~ | ✅ ear (P2's unpause only) | FRONTEND/FRONTIER O5 |
| Menu move/confirm/back | ~ (program menus: **3 move, 0 select, 1 back/cancel**, 4 list switch, 34 spinner) | ❌ survey guesses 60/116/154 | ✅ | ❌ | ❌ (never ear-checked) | `14016e180` (3 = every direction input, 0/1/3 picks via `uVar5`), `140161fa0` (4), `140175840`/`14018eb40` (34) |
| Voice pick per group | ✅ lists read | ~ uniform 10..12 | ✅ | ❌ | ❌ | see D6 |
| KO voice group 6 | ~ rows 14-19 (w4 = 6) | ~ guess 60 | ✅ | ❌ | ❌ | state table |

## 4. Discrepancies (most important first)

**D1 (CONFIRMED). Capsule pickup is silent; the program plays common/86.** Program: `FUN_140261a00` tail plays
id 0x56 at the capturer when the 4th arg is set (capture passes 1: bytes `41 b9 01 00 00 00` at `0x140251b8d`).
Ours: `SFX_CAPSULE_PICKUP = 0` (`sfx.rs:218`), cue wired at `sfx.rs:135`. Also Thieving Imp and random capture.
**Not yet ear-approved; 86 is the single candidate (92 is erase).**

**D2 (CONFIRMED). The appear sound plays for the opening fill; the program is silent there.** Program: round-start
slots go straight to state 2 (`FUN_14014ee70`); only `FUN_14014ed10` (after the respawn wait) plays 85. Ours:
`capsule_appeared` (`sfx.rs:254`) reports every new id incl. frame 1, and its test even asserts "opening deal: every
orb is new" (`sfx.rs:799`). Visible effect: a burst of appear sounds at every round start that the original does not
have. Fix needs the sim to distinguish respawn from opening fill (new event or `spawn_timer` state). Also the program
sounds 15 ticks BEFORE the orb is pickable (state 1); ours sounds when it already exists (SUSPECTED ~0.5 s late).

**D3 (CONFIRMED). No erase sound at all, and no sim event to hang it on.** Program: 92 per erased capsule
(`14014edc7`, `14014ff07`, from `FUN_14026ff20` via `FUN_14014fd10` mode 1; Thieving Imp adds it after 86).
Ours: `moves/erase.rs:388-430` (`retain`/`remove`/`clear`) and `combat.rs:1320`(capture) log nothing for erase;
`DECISIONS.md` says an env override `PD_CAPSULE_ERASE` exists -- **it does not (grep: no hit in code)**. Render-side
diffing cannot tell erase from capture from a restart, so it needs a sim event.

**D4 (CONFIRMED). Denied press uses our synthesized buzz; the original plays common/2.** `FUN_140261e90`
(can't pay / not usable) plays `0,2` at `0x140261fa0`; `FUN_1402517c0` plays it for button-disabling statuses
(`0x140251b32`). Ours: `denied_wav()` (`sfx.rs:346`), only for "not enough Aura"; Frostbite/locked/frozen
presses are silent (`combat.rs:~1290`). common/2 is 0.19 s, stereo.

**D5 (CONFIRMED). Cast sound timing/layers.** Program: s19 is played at object creation/fire tick (20-ish ticks
after the row's w3 cast sound; `1402626d0`), attack kind only. Ours plays s19 together with w3 at cast
(`sfx.rs::skill_sounds` cast list, known comment). Environment, bind and explosion layers are missing
(see ledger: 93/87/94/88, 100, 99, 14, 91, 6).

**D6 (CONFIRMED). Voice selection.** Program picks from a 10-entry list per group (`gsdata 0x33004`):
g1 `10 11 10 12 11 0 20 10 11 12`, g2 `20 0 21 22 21 20 30 22 20 21`, g3 `31 30 22 32 31 30 32 31 40 30`,
g4 `42 41 50 40 42 41 40 40 41 0`, g5 `50 52 51 60 40 50 51 50 52 51`, g6 `61 60 51 62 60 61 60 62 60 61`
(0 = silent, cross-group ids occur). Ours picks uniformly from `group..group+3` (`sfx.rs:573`). Audible effect: small
(variant mix, ~10% silent casts in g1/g2/g4 in the original); environment skills (all group 2) sound different.

**D7 (SUSPECTED). Missing "body" layers on hits/knock-downs:** common/6 on a character hit, 82 landing, 81
get-up, 100 on bind rows 20-23, 91 when a button is erased, 14 guard broken, 99 explosion. Each is a real call
site but none is ear-checked, so each needs Laurence's yes before wiring.

**D8 (SUSPECTED). Block volume.** Our `VOL_BLOCK = 0.3` has no program basis: a blocked attack plays s21
through the same branch as a hit (`14026cd70` cases 1-3) at volume 0x1000.

**D9 (SUSPECTED). Menu cues are survey guesses (60/116/154).** The program's menu code plays 3 for any direction
input, 0 for selecting, 1 for cancel/backspace (`14016e180`, `0x14016e6c2`), 4 for list switching (`140161fa0`),
34 for spinners. Only Unpause = 1 is ear-backed. Laurence's pass-1 label "3 = menu traversal" agrees with the read.

**D10 (SUSPECTED, other lane's file but blocks honest audio). `CONSUME_LOCK_TICKS`/consume clip is triggered on
CAPTURE (`combat.rs:1313-1326`).** In the program nothing consumes at capture: the only forced use of a button is
the demo screen `FUN_140275210`; Aura/green effects run at activation (press on the filled button) with the
row-343 / status row animation. If Laurence saw the consume pose at pickup in the original this is wrong
timing for the lock, not for the audio; flag for the movement/combat audit.

**D11 (docs). Stale claims to fix when the lane lands:** `sfx.rs:127-135` still says 92 = pickup;
`combat.rs:615-618` says pickup id 34; FORMATS-AUDIO.md "What is not wired up" and "Audition pass 2" still list
92 = pickup and "ed10/ed90 swapped". The tests `capsule_ids_default_to_the_ear_result...` only assert the wired ids
(the anti-pattern called out in FORMATS-AUDIO).

## 5. Ear-check sheet: only real candidates (each is a program play call, bank 0)

Duplicates removed (34 = 35; 101 = 102 share a clip). Clip lengths from `cache/sfx/index.json`.

| Sheet | id | Program role (address) | len | Ask |
|---|---|---|---|---|
| **A1 pickup** | **86** | capture into button (`0x140261bc6`) | 0.98 s | primary |
| A2 | 88 | environment crystal taken (`0x14017bf70`) | 0.98 s | decoy |
| A3 | 81 | getting up (`0x14025c4ee`) | 0.38 s | decoy |
| A4 | 91 | button erased by attack (`0x14026fb8e`) | 0.84 s | decoy |
| **B1 erase** | **92** | capsule erased (`0x14014edc7`) | 0.87 s | primary |
| B2 | 94 | environment ends (`0x14017bb6d`) | 0.99 s | decoy |
| B3 | 93 | environment created (`0x14017bdc9`) | 1.50 s | decoy |
| C denied | 2 | can't use (`0x140261fa0`) | 0.19 s | vs our buzz |
| D aura tick | 5 | Aura +1 (`0x14024df1d`) | 0.18 s | new |
| E env | 93, 87 (loop), 94, 88 | environment set / hum / end / crystal | | one play each |
| F body | 6 (char hit), 7 (non-char hit), 14 (guard broken), 99 (explosion), 82 (landing), 81 (get-up), 100 (bind hit) | | | sanity |
| G menu | 3 (move), 0 (select), 1 (back), 4 (list), 34 (spinner) | | | vs 60/116/154 |
| Already known | 85 appear (ear), 4 lock-on, 35 step, 52/54 wet, 1 unpause | | | do not reopen |
Unlabelled in the same band (read, no decision needed): 83 (loop, `FUN_140231660`), 84 (effect helper
`FUN_14022bbd0`), 95-98 (no call site found). Suggested tooling: `engine/re/sound_sheet.py`.

## 6. Reads still needed

1. Which `DAT_1403e0304 % 10 == 1` mode enables timed capsule expiry (`FUN_14014f320`, `0x14014f3e5/0x14014f419`;
   slot field `piVar1[-2]` vs clock `DAT_1405315f4`): if versus uses it, the sim also needs capsule lifetimes.
2. Bank slot table: what file each slot (0, 1, 3-6) holds (`FUN_140240320` / `FUN_140241a90`, callers
   `14012ffc0`, `1401381f0`, `14013c440`, `14013fda0`, `140148050`, `140219860`, `14024f6c0`); which file bank 1 is.
3. Listener / attenuation / per-player hearing in split screen (`FUN_140240870`, `140240a00`, `140241440`,
   `140241b10`, `DAT_1405336b8`, `DAT_1405336c0`).
4. Meaning of player-controller flags `+0x10`, `+0x30`, `+0x34` at `(player+0x61a0)`: +0x10 = playable (gates
   86/85/92 scheduling), +0x30 = plays UI sounds (gates 2 and 5). Confirm both versus players pass.
5. Status ids 5 / 0xc / 0xd+button that make a press "denied" (`FUN_1402517c0`), to map to our frozen/locked/
   scramble states.
6. What 83 (`FUN_140231660` loop, stopped with `FUN_1402a9560`) and 84 (`FUN_14022ba20` caller) are.
7. `FUN_14026f870` contact rules and `FUN_140272510` return value (Ghidra dropped it) to confirm 91 = erased button.
8. Who plays the KO: state 27 handler `FUN_140256e80` -> KO voice group and any KO sound; rows 14-19 (w4 = 6).
9. Menu confirm/back ids per screen beyond the text-entry screen (`14016a180`, `14016fee0`, `140171390`, `1401585f0`).

## 7. Proposed build lanes (ordered)

**Lane 0 (no code): ear-check sheet** from section 5 through `sound_sheet.py`; Laurence answers Q1-Q4. Nothing
ships before this.

**Lane A: capsule lifecycle audio.** Owns `pd_game/src/sfx.rs` (`cues_for_event`, `capsule_sfx`,
`capsule_appeared`, constants, tests), `combat.rs` only at the capture arm (`:1308-1330`) and `spawn_capsule`
(`:2330`) to add `CombatEvent::CapsuleAppeared { respawn: bool }` and `CapsuleErased { pos }`, `moves/erase.rs`
(`field_effect`, `erase_touched`, `capture_touched`: log one erase event per removed capsule; Thieving Imp also logs
a `Capsule`), `fx.rs` (exhaustive no-op arms). Tests (from the reads, not from current behaviour): opening fill emits
no appear cue, a respawn emits exactly one 85; capture emits 86 (once Laurence confirms) for Aura and skill
capsules alike, never 92; erase emits 92 per capsule incl. mass erase, Particle Convert emits nothing; Thieving
Imp = 86 then 92; both players' events reach the queue. Runtime proof: scripted `PD_SHOT_TICKS` capture match with
`PD_SFX_LOG=1` (log lines `capsule appear/pickup/erase`), a real capture-then-use run for both players, restart/refill.
Cite `0x14014ed6f`, `0x140261bc6`, `0x14014edc7`, `0x14014ee70`.

**Lane B: activation + combat layers.** Owns `sfx.rs` (`skill_sounds`, `play_sfx` cast/hit cues, `denied_wav`
removal) and the event hooks for denied statuses. Only after the sheet: denied = common/2 (also Frostbite/locked
presses), s19 at `fire_tick`, voice lists D6, block at full volume, plus whichever of 6/14/99/82/81/91/100/5 and the
environment quartet Laurence approves, each behind its own cue so one "no" drops one line. Tests cite `14026cd70`,
`1402626d0`, `14025a3e0`, `1402517c0`. Regression guard: steps 35, wet 52/54, lock-on 4, unpause 1, existing
attack audio unchanged (`sfx.rs` tests + `footsteps.rs`).

**Lane C: menu cues.** Owns `frontend/sound.rs`, `menu.rs:224`. Move 3 / Confirm 0 / Back 1 (+ 4 / 34 for lists and
spinners) after the ear check; keep Unpause 1. Smallest lane, independent of A/B.

## 8. Questions only Laurence can answer (ear sheet)

1. **Capsule pickup (capture into a button):** which clip is it? A) 86, B) 88, C) 81, D) 91, E) none of these.
   (Recommended to test first: 86, the only program pickup call.)
2. **Capsule erased (by a skill, or Thieving Imp after the pickup):** is 92 right? A) yes, B) 94, C) 93, D) 91, E) none.
3. **"Can't use" press:** replace our synthesized buzz with the original common/2? A) yes, B) keep the buzz.
4. **Aura regen blip (common/5, each +1 Aura, only you hear it):** A) add it, B) leave silent, C) let me hear it first.
5. **Environment skills (a cast burst, a hum while active, an end sound, a blip when you take the crystal = 93/87/94/88):**
   A) add all four, B) add only the hum, C) add none, D) let me hear them first.

## 9. Events lane results (2026-10-01) -- what was built, and what this audit got wrong

Built (`sfx.rs`, `fx.rs`, new `CombatEvent` variants; trace `engine/shots/events_trace.txt`, ear sheet `engine/shots/ear_check.html`, both local only): see DECISIONS 2026-10-01 "Lane EVENTS". Each cue is pinned by `sfx::tests` with its program address and fired once at the right moment in `a_scripted_match_fires_each_new_cue_once_at_the_right_moment`.

Corrections from the re-read (bytes + decompile):
- **Thieving Imp plays 86 only**: `FUN_14014fd10` gets mode 0 (`xor r9` at `0x1402700f3`); only s45 = 0 erases with mode 1 (`0x1402700fb`). Change the World and Particle Convert are silent.
- **Erase 92 per capsule** is right; **94 is only the forced cancellation** of an environment (add effects 38 / 39, `FUN_14017ba00`); a time-out plays nothing and a take plays 88 (and no 86: `FUN_14017be00` passes the play-sound flag 0).
- **6 vs 7** is a character-table flag (`charaset` record +0x34): 6 for every playable character, 7 for non-fighters. 6 / 7 play on code 2 (body hit) only, never on wall / guard impacts or for a blast's victims.
- **99** also covers mines (code 8, which plays no s21) and motion-9 skills (Photon Wave / Burst); Rain drops never blast.
- **91 plays at the victim**, not the attack object.
- **s19**: also plays for every object `FUN_140263a00` creates (each Rain drop), so a normal attack plays it twice in the original; we play it once, at the fire tick.
- **Aura Particle activation is NOT silent**: `FUN_140270b00` always ends with `FUN_140271590(fighter, kind, amount)` -- Level +1 plays common/89 (90 for a drop); stat kinds 4-7 play 95 / 96; 8 -> 98; 10 -> 89; 0x12 / 0x17 -> 97. (Runtime-unverified that the Aura record takes this path.)
- **Reaction row sounds** (w3: 501-527, bind rows 20-23 = 100) play only together with the attacker's effect; every row except 42 has w2 = 0.
- **No KO sound or voice exists** in the program (states 0x12 / 0x1b have no sound call); rows 14-19 (w4 = 6) are ordinary reaction rows. The KO voice stays only because Laurence asked for it.
- **Denied** (`common/2`) is gated by `(player+0x61a0)+0x30` = "driven by a pad on this machine": every local human hears it. The doc missed `0x14025189d` (capture / crystal take blocked by status 5 / 0xc / 0xd).
- **Aura tick**: same gate, so every human hears their own tick; it fires when the whole number of Aura rises.
- **Landing dust 520 + sound 38** are the jump-pad launch (floor attribute 6), not a landing; the "76" and "67" are floor-type landing / take-off tables; 36 is a story pose; 97 / 98 are status-effect sounds.
- Menu cues: 3 on every cursor step (not at a clamped edge in lists), 0 select, 1 back (a Yes / No dialog's B only moves to No), 2 on a disabled item, 4 tab switch, 34 number spinners; the pause menu closes with 1 = Back.


## 10. Soundfill lane (2026-10-02) -- film comparison sound classes

Full film comparison (341 skills, `film_compare.py run --real`): 161 skills missing / 74 extra before.
- **Wall / floor / range-end impact plays s21** (READ `FUN_14026cd70` cases 1-3 all call `FUN_14026ed60` with s21; 6/7 only on case 2). Built in `sfx.rs::terrain_impacts` (render side, reads the sim only). Fixes `0.509`-class misses (31 -> 10 skills).
- **0.26 / 0.27 and 0.30 / 0.31 are footsteps** on two floor classes of the recorder's arena (READ `FUN_1402311e0`: `DAT_1402f6270[class*8 + col] + foot index`; class 4/6 -> 24/26, class 2/3/5 -> 28/30). Protected (footsteps approved), so the compare tool sets them aside like 0.30/0.31.
- Our sounds after the take's last tick are not comparable (Aura regen tick, get-up): the tool ignores them.
- **Landing ground sound is by floor class** (READ `FUN_140254b30`, called at the end of `FUN_140254c80`): class 2/3/5 -> 70, 4/6 -> 71, 7/8/9 -> 72, 10 -> 75, 11 -> 78, 12 -> 73, 13/14 -> 77, 15/32 -> 76, 21..30 -> 151..160. We always play 70 (the 5 takes with 71 are on class 4/6). Take-off is the sibling table `FUN_1402549e0` (61, 62, 63, ...).
- **0.8 / 0.9 / 0.10 (and 11-13, 16) were first read as stage-object sounds (CORRECTED in section 11: they are surface-hit sounds by surface class; the object groups are 17-29)**: `FUN_1401fc750` (a virtual method of the prop classes) looks up a sound group in the stage `.dat` (header `+0xf8` table, 8 shorts per object type, slot = event), then `FUN_140201ce0` queues it; `FUN_140201d30` plays `DAT_1402dc640[group*2 + (rand>>14)]` (group 1 -> 8, 2 -> 9, 3 -> 10, 4 -> 11, 5 -> 12, 6 -> 13, 8 -> 14, 9 -> 16), 30-tick cooldown, up to 5 positions 4+ units apart per burst. NOT built: needs the stage `.dat` `+0xf8` table exported and the prop-event triggers (player lands on a prop, shot meets a prop) wired. About 80 skill-takes (0.8 x42, 0.9 x24, 0.10 x6).


## 11. Propsound lane (2026-10-02) -- common 8-16 are surface-hit sounds, not stage-object sounds; floor-material landing / take-off

**Correction to section 10.** The film's `0.8` / `0.9` / `0.10` are NOT the stage-object sound groups. Evidence:

- The only caller of the group queue is `FUN_1401fc750` (a virtual method, slot 0x88 of the prop classes; 16 call sites in `0x1401e8xxx-0x1401f5xxx`, all passing event 0 or 1) plus the cable class `FUN_1401f9900`. The stage `.dat` header-`0xF8` table (`pd_convert` now exports it as `types[].sound_slots`) holds **groups 17-29 (slot 1) and 36-38 (slot 2) only**, in every stage that has it; groups 1-9 (common 8-16) are in no stage file. st06: gas bottle / container C / fan / mac / oilcan B 17, container A 22, container B and heli 25, oilcan A / tank 23, every fence and rail 29. None of 17, 172, 22, 177, 23, 178, 181, 182, 189, 190 appears in any of the 341 takes (no take breaks a prop), while 8/9/10 appear in 72.
- The ids 8-16 come from a different table, `DAT_1402f5da0` (indexed by **surface class - 2**, read by `FUN_14021d740` when `param_4 == 1`): class 4, 5, 7 -> 8; 6, 8 -> 9; 9, 10, 11 -> 10; 12 -> 13; 13 -> 16; 14 -> 11; 15, 16 -> 15; 23..32 and 34 -> 10; the rest -1. `FUN_14021d740` is the stage-contact effect: `FUN_1402021f0` (once per tick, from `FUN_140202710`) walks the contacts of the "player bullet" probe (`pdStageColPtr_PlayerBullet`, `DAT_1404bec60`) against the stage collision, and `FUN_14021d3f0(class)` spawns an impact decal (`FUN_1401c9210`, up to 32 live, none within an active one's radius) and the sound only when the decal was really spawned. Which classes spawn a decal is its switch: type 0 = classes 2, 3, 4, 5, 6, 21, 25, 27; type 1 = 7, 8, 9, 10, 13, 24, 26, 28, 29, 30, 32 (Sein, stage number % 10 == 8: no 4 and no 24); the Lane stages (% 10 == 7) play common/199 for every contact. `DAT_1403e0304` is the **stage number** (`FUN_1401d1950`, `DAT_140509bb8` table), not a frame counter.
- What feeds the probe: a **landing** (`FUN_140254c80`: knock-down state 0x11 with radius 10 / 15 / 20 by fall class, `DAT_1402f7f7c / 7fb0 / 7fc4`; the airborne state 5 only for a fall over 80, `DAT_1402f8040`), a **shot's terrain hit** (`FUN_14026ca50` -> `FUN_1401633b0` sweeps the shot with the probe, then `FUN_140245350` and the impact event `FUN_14026cd70(.., 1)`), and a **blast** (`FUN_14026dbd0` -> `FUN_140163b50` with the blast radius).
- LIVE check against the takes (class from `st06.surf.json`): every knock-down landing on class 5 plays 70 + 8, on class 6 plays 71 + 9 (140, 187, 194, 260, 311), the Jump skill's landing (fall ~97 > 80) plays 70 + 8 and no 82 (239).

Built (`pd_game/src/ground_sfx.rs`, render side only; the sim only gained the transient `Player::landed` -> `CombatEvent::Landed`):

- **Landing sound by floor class** (`FUN_140254b30`): 2, 3, 5 -> 70; 4, 6 -> 71; 7, 8, 9 -> 72; 10 -> 75; 11 -> 78; 12 -> 73; 13, 14 -> 77; 15, 32 -> 76; 21..30 -> 151..160; other classes silent. Plays on a knock-down landing (with 82) and on every airborne landing (`Landed`, not played before). Where the surface sidecar has no triangle under the body the old 70 is kept (UNCONFIRMED class).
- **Take-off sound by floor class** (`FUN_1402549e0`): 61, 62, 63, 66, 69, 64, 68, 67, 141..150 by the same classes; 61 as the fallback.
- **Surface-hit sound** by class (above) at a knock-down landing, a heavy airborne landing (fall over 80) and a blast (nearest triangle within the blast radius, UNCONFIRMED contact rule). A shot that ends on terrain does NOT play it: the shot's sweep contact was tried (first face crossed) and in 9 takes it matched the original's id in none (7 of them have no 8-16 at all), so which shot contacts spawn a decal stays UNCONFIRMED.
- **Stage-object sound groups** on a prop break (event 1 = the break handlers): `DAT_1402dc640[group * 2 + bit]`, the 30-tick cooldown, 500-unit spacing and five-per-burst rules of `FUN_140201d30`, the random bit from the audio side's own xorshift.

UNCONFIRMED: the decal radius and lifetime (the program refuses a new decal inside an active one, so repeated landings on the same spot can be silent: takes 124, 125, 191, 245, 303, 323 play 82 + 70 and no 8-10; we only stop one impact sounding twice in a frame, radius 30); which decal types a stage loads (`DAT_140530390[(type + 0x40) * 2]`, per stage; type 1 = classes 7-10, 13, 26... assumed present); the multi-contact count of a blast / heavy landing (we play one); whether the airborne landing's tiny falls (< 40) always sound (the program calls `FUN_140254b30` for every state-5 landing); the debris group (slot 2) and cable class (`FUN_1401f9900`) are not played; the object-group sound has no live take (no film take breaks a prop).

Film comparison (`film_compare.py run --real`, all 341 takes, PD_COMPARE_OUT `.pdtools/compare_propsound`; before = `.pdtools/compare_soundfill`): see the lane report in `engine/docs/WAVES.md` / HANDOFF for the counts. Per source, our 8-16 sounds vs the original's: landings 35 takes (16 same id, 13 where the original has no 8-16), blasts 15 takes (11 same id, 3 none). The Jump skill (239) falls ~50 units in our sim against ~97 in the original (y 427 -> 330), so it misses the heavy-landing contact: a Jump height difference, not a sound one.


## 12. Soundfill2 lane (2026-10-03) -- sound classes of the first trustworthy full comparison

Baseline `.pdtools/compare_full_1003` (244 of 341 clean; 63 skills with a missing sound, 50 with an extra). After: `.pdtools/compare_soundfill2` (merged main incl. bodyradius; 82 mismatch skills + 25 sound-sensitive clean skills + 40 random clean controls = 147 runs, `--real`), A/B of the shot / burst surface sounds in `compare_soundfill2_noshot`.

**Tool artefacts (film_compare.py `set_aside_sound`, notes on each sheet):** takes the recorder never fired (5, 11, 14, 180, 205, 206, 299: every sound of the take), `capsule_touched` takes (118, 181, 188, 196, 295: common/86), erase-on-hit skills with a colour filter (s48 1-5, 7) whose victim's DEALT hand we cannot see (common/91: 122, 135, 215, 216, 217, 315 have it, 128, 134, 187, 309, 324 do not). Re-diffing the old logs with these alone: 244 -> 252 clean.

**READ rules built (general):**
- Blast victims over the blast's whole life (30 program ticks, `Blast`): a mine's victim or a shell's later victim is hurt by the blast, so no s21 / common/6 / 7 (`FUN_14026cd70` code 8). Before, only the start frame counted (Timer / Sensor Mine played 521 + 522, Big Bomb a 0.7).
- Add-effect sound only when the effect TOOK HOLD (`FUN_140272bc0` returns before `FUN_1402738d0`): 0x11 / 0x27 (Purify, Return to Nature) need one of the 47 status slots non-empty (`FUN_14024d690`), 0x2f (Vicious Balance) a victim Level above the lowest present, 0x33 (Angel's Breath, Armaros) a non-zero attack / speed / homing / Aura-rate status, 0x31 (Meltdown) an Aura Particle on the victim's spots. LIVE takes 160 / 162 / 280 / 293 / 296 log none of 97 / 98 / 90. Class "add/status effect extra": 6 -> 0 skills.
- Knock-down landing probe (`FUN_140254c80` state 0x11): radius 10 / 15 / 20 for a fall under 40 / up to 80 / over 80 (`DAT_1402f7f7c / 7fb0 / 7fc4`), centred on the body centre 9 u above the feet (charaset +0x12 / 10; the sign of the axis is not read, the floor is 9 u from the centre either way). Checked against 40 original landings: with the floor class under the feet (class 5 -> 8, class 6 -> 9) the original's id matches in about 22 of 40, and the floor lies exactly 9.0 from the centre in nearly all.

**Built, UNCONFIRMED rule, kept because the film says so:** a shot that ends on the stage plays the surface-hit sound of the face its last step crosses (`ground_sfx::StageSound::shot_contact`), and a Burst (rain drop, shell) the nearest face within 5 u. A/B on the 85 skills that touch sounds 8-16: clean 34 -> 37, missing-sound skills 36 -> 28, extra-sound skills 25 -> 30 (new extras 173, 174 0.9; 22, 126, 319 0.8). Switches: `PD_SFX_NO_SHOT_SURFACE`, `PD_SFX_NO_BURST_SURFACE`, `PD_SFX_OLD_LANDING`.

**Read but NOT built (UNCONFIRMED), with the evidence:**
- Which contact spawns a decal. `FUN_1401c9210` refuses a new decal inside an active one's radius (type table +0x58, lifetime from the decal animation, and `FUN_1401c8e60` also retires a decal by a test in CAMERA space), so repeated landings can be silent. Original pairs: Dragon Punch flight 80 then landing 99 (11 u, 19 ticks) both sound, Vacuum Slash / Muramasa flight 74 then landing 80 is silent, Blazing Sword 54 (P0) then landing 80 (P1, 35 u away) is silent: no single radius / lifetime fits. Class "surface-hit landing extra" stays at 13-14 skills.
- Which face of a mixed neighbourhood wins (Psycho Burst / Aura Leak play 9 although a class-5 face is nearer; Agonies of Death plays 8 although class 6 is nearer): list order of `FUN_1401d5130`, not nearest, not triangle index.
- A knocked body's flight contact 2-6 u above the floor (Dragon Punch 80, Aura Backflow 79, Vacuum Slash 74, Timer Mine 217, Fast Bomb 87) and melee-dash / cast-time contacts (Frostbite, Excaliber 55, Blazing Sword 54, Dazzle 62, Armaros Watch 63): the class under the actor always matches the id, the trigger tick does not follow the body probe (`FUN_140255020`: radius h/2 + 7.5 + step).
- common/127 and 128 (9 events, 8 skills): 39-72 ticks after a hit or landing, no program call site found (data-driven `FUN_1402506c0` sounds); reaction row 100 (Pressure, Lift Up, Antigravity Trap) needs the unbuilt bind reaction kinds; knock-down / get-up / landing-class differences (81, 82, 70 vs 71) follow the sim's knock reactions and slide distance, not the sound rules.

Counts (147-skill set, new tool, same skills): clean 65 -> 77 (62 with the old tool), skills with a missing sound 50 -> 42, with an extra 45 -> 39; the 25 sound-sensitive and 40 control skills stay clean except 173 and 174 (extra 0.9).


## 13. Surface-sound lane (2026-10-09) -- which face a sphere contact sounds, and the impact-mark refusal

**READ: the sphere probe sounds the FIRST chunk in stage-file order, not the nearest face.**
- The stage collision list `DAT_1405322c0` is one 0x50-byte node per `.dat` 0x94 chunk with flag bit 1 (every st06 chunk has it), in file order (`FUN_1401d4070`); class = chunk byte 0, shape = byte 1 (3 = face chunk, 8 = the vertex pool).
- Landings and blasts all use ONE sphere probe, `DAT_1404becb8` (shape type 1, built in `FUN_1401631e0`), through `FUN_140163b50(centre, radius, mode)`; shots sweep `DAT_1404bec68` (type 5) in `FUN_1401633b0`. Pair tests: `FUN_1401d5860` -> stage tree `FUN_1401d5530` -> `FUN_1401d5130` -> table `DAT_1402dc2f0[a*8+b]` (sphere vs face chunk = `FUN_1401d83f0` -> `FUN_1401d7fd0`: chunk bound sphere, then per face a two-sided plane distance below the radius plus an inside-or-edge test; contact point = centre projected on the plane). No class is skipped (the probe mask 0x1900 meets every stage node's 0x40021000).
- The per-tick walker `FUN_1402021f0` visits stage nodes in list order; for a type-1 (sphere) probe it keeps ONE contact per probe shape, the first met; for a type-5 (sweep) probe, per pass the contact nearest the sweep start. So every landing and blast of a tick together play at most one surface-hit sound, the class of the first chunk any of them touched. A silent class (e.g. 0x1F, 0x12) that comes first means no sound.
- Built: `SurfaceGround::first_touching`, `StageSound::sphere_probe`, one sphere contact per tick in `ground_cues`. `PD_SFX_NEAREST_CONTACT=1` = the old nearest-face rule.
- Film A/B on 52 sound-mismatch skills (`.pdtools/compare_sphere_new` vs `compare_sphere_old`, same exe build, `--real`): 5 skills better (Psycho Burst, Aura Leak, Swift Punch: missing 0.9 / extra 0.8 gone; Swift Kick, Leg Throw: extra 0.10 gone), 0 worse; sound_missing 36 -> 33, sound_extra 25 -> 20.

**READ: the hit sound needs the impact mark.** `FUN_14021d3f0` -> `FUN_1402422a0` (disassembled: the sound flag passed to `FUN_14021d6b0` -> `FUN_14021d740` is `FUN_1401c9210`'s return). `FUN_1401c9210` refuses a mark whose position is inside a live mark's radius (32-entry ring, oldest overwritten). The radius is the mark model's bound (`FUN_1401c8c10`: max over its meshes of |sphere centre| + radius, from the index chunk floats): the mark slots 0x40-0x42 are MD00 40-42 `mdl_hit_iron / stone / soil` (24 st00 slots, then the stage's aux list from the `st%02deff` pass of `FUN_1401d9210`), **24.57 for all three on every stage**. Mark position = contact point + face normal. A mark ages by 0.5 per call (`DAT_1402f7d40`) only when its model has an animation (`FUN_1401c8e60`), and is also dropped when the camera is inside its model box.
- Built opt-in (`PD_SFX_MARKS=1`, `MARK_RADIUS`, `MAX_MARKS`): UNCONFIRMED whether the hit models have an animation (if not, a mark lives until 32 newer ones overwrite it, or the camera enters it).
- Film A/B of `PD_SFX_MARKS=1` on the same 52 skills (`.pdtools/compare_sphere_marks` vs `compare_sphere_new`): no sound difference in any skill (the one-shot takes rarely put two contacts within 24.57 u). Stays opt-in.

**READ: shot steps.** `FUN_1401633b0` (called by the object movers `140269db0`, `14026a0d0`, `14026ca50`) sets the sweep probe `DAT_1404bec68` to the SEGMENT from last tick's position to this tick's (shape +0xc start, +0x18 end) and runs one pass; the walker keeps, per pass, the contact nearest the segment start. So every step of a moving object that crosses a stage face can place a mark and sound, not only the step it dies on. The contact is silenced only when the owner probe's vtable +8 (`FUN_1401631c0`, `pdStageColPtr_PlayerBullet` vtable `14047ba90`) returns 1, which shifts the class by 0x21 out of `FUN_14021d3f0`'s range: it returns the probe's +0xc word (0 at creation, set by vtable +0 `FUN_1401631a0`) unless the mode byte (`FUN_1401633b0` param 7, `140163b50` param 4) is above 0xFE. Who sets +0xc is not read.
- Built (now default): the shot step contact (`StageSound::shot_contact`, only a step that moves: a shot that ends standing still touches nothing), the rain-drop / shell burst contact, and the impact marks (radius 24.57, ring of 32). Switches OFF: `PD_SFX_NO_SHOT_SURFACE`, `PD_SFX_NO_BURST_SURFACE`, `PD_SFX_NO_MARKS`. Film on the 52 sound-mismatch skills (`.pdtools/compare_defaults_1009` vs `compare_sphere_new`): clean 5 -> 10, sound_missing 33 -> 27, sound_extra 20 -> 21; better Rock Shot, Dazzle, Octolaser, Blazing Storm, Celestial Ship, Armaros Watch; worse Piece of Wind only (it now plays 0.10 where the original plays 0.9: a neighbouring face, missing before). Steps: marks alone fixed Celestial Ship's extra 0.9; dropping the standing-still fallback fixed Entangle / Curse of Earth (crawlers that expire).
- Left (52-skill set): missing 0.9 / 0.8 mostly from rain and shell skills whose drops our sim lands elsewhere, and the knock-down sets (0.70 + 0.8 + 0.82 extra: we knock down where the original does not) -- gameplay, not sound rules.
