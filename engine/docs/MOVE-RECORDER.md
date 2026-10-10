# Move recorder (lane `moverec`, 2026-10-01)

Controlled, repeatable reference footage of single skills from the **original PC game** (Store
version, `PDUWP.exe`), for the skill-by-skill accuracy pass (`audits/SKILL-BY-SKILL.md`).
Code: `engine/re/move_recorder.py` (+ `save_guard.py`, one small patch to `live_trace.py`).
One-click: `tools/bats/RECORD_ORIGINAL_MOVES.bat`. Outputs live in `%USERPROFILE%\.pdtools\moverec\`
(never in git).

**Status in one paragraph.** Built and proven live on the original: injected input (stick +
buttons), a versus match started from memory, any skill put in a slot (owned or not), scripted
takes with a per-tick trace, the Aura Particle hold answer, a fresh match per take, the save put
back by hash, **and (2026-10-01 afternoon, screen on) video**: one `gfxcapture` recording of the
game window per batch, cut into one clip per take, aligned to the trace to about a tick, and a
contact sheet (`mr_contact_sheet.py`). The pilot's 5 clips were looked at (see "Video").

## How it works (no OS input of any kind)

The game is driven by writing **its own memory** through the same Win32 debugger as
`live_trace.py` (`DebugSetProcessKillOnExit(False)` first, every patched byte restored on exit).
One breakpoint, once per program tick, is the "hook"; everything else happens inside it.

| What | Where (Ghidra address; runtime = base + (addr − 0x140000000)) | How it was established |
|---|---|---|
| **Input source** | The program reads **Windows.Gaming.Input.Gamepad** (WinRT, `Xb1GamepadManager`: `FUN_1400dd4a0` enumerate, `FUN_1400dd6a0` update, `FUN_1400dda60` state `GetCurrentReading` → buttons / triggers / sticks). **Not XInput.** A keyboard fallback exists (pad `DAT_1403e0c74`, bits `DAT_14054ca40/48`); not used. | decompile; the string `Windows.Gaming.Input.Gamepad` is passed at `0x1400debd4` |
| **Per-tick poll** | `FUN_1401d1250` (called every program tick from `FUN_1401c7630`, **30 per second**) → `FUN_14028ea10` → per pad `FUN_14028e710` fills pad object *n* at **`0x1405a5ce0 + 0x30·n`**: `+0x1c` button mask, `+0x20/+0x24` left stick x/y, `+0x28/+0x2c` right stick x/y (signed ±32767; positive y = forward). | decompile + live: writing here moved the fighter |
| **The hook** | **`0x1401d12b0`**, the instruction right after `call FUN_14028ea10`. We stop there, overwrite pad object 0 / 1 (20 bytes from `+0x1c`), and the rest of the tick sees it as a pad reading: `FUN_14028eac0(pad)` → held / newly-pressed / repeat tables at `0x14059b800 + 0x34·pad` (+0, +4, +8), then each fighter's input block (`FUN_140256580`, dead zone 13,000, `FUN_140256600`). | live: `0x14059b800` read back `0x1000` the tick after injecting START |
| **Button bits** (pad object `+0x1c`) | A `0x10`, B `0x20`, X `0x80`, Y `0x100`, RB `0x200`, **LT `0x400` (switch target)**, **RT `0x800` (lock on)**, START `0x1000`, BACK `0x2000`, D-pad up/down/left/right `0x1/0x2/0x4/0x8`, LB/L-thumb `0x1000000`, R-thumb `0x2000000`. | table at `0x1403dc710`; battle meanings read live |
| **Skill buttons** | slot 0 = **Y** `0x100`, slot 1 = **X** `0x80`, slot 2 = **B** `0x20`, slot 3 = **A** `0x10` (fighter `+0x14248 + 0x40`). | read live from both fighters |
| **Fighter** | `DAT_1404bc9d8[player]`; position `+0x12e40` (x, y up, z), facing `+0x12e68`, clip `+0x12f00`, state `+0x12f04` (0 idle, 1 run, 10 cast, 12 melee cast), pad index `+0x14244`, stats `+0x40` (health `+0x0c`, max `+0x10`, Level `+0x18`, Aura `+0x1c`), lock flag/band/target `+0x13f24/+0x13f1c/+0x13f20`, skill slot *s* record at `+0x388 + 0x50·s` (skill index at `+0x30`). | PROGRAM-RE + live |
| **Skill index space** | **The program's skill index is the skill-TABLE INDEX = `skills.json` `record` + 1; index 0 = the Aura Particle** (`FUN_140262670` reads `DAT_1404bdc50 + index * 0x90`; stock counts, deck entries and slot records all use it). It is not the catalogue `id` the guides use, and **not `text_id`** either: text_id equals record + 1 only up to id 300 (ids 301-340 have text_id 304-353), and ids 257 / 294 appear twice in `skills.json` (records 749 / 750 are the dodge moves; the skills are table indices 77 and 243). My first version keyed on text_id (a mistake, fixed 2026-10-01 evening). | live: the take of ids 294 / 257 / 301 / 340 fired indices 243 / 77 / 301 / 340 (`takes_check\summary.csv`, `index_ok`); the owned list reads as school-grouped runs |
| **Put a skill in a slot** | `FUN_140261a00(slot record, index, 1, 0)` called on the game's thread (the same routine a capsule capture runs). | live: slot record index field changed, button then cast |
| **Start a versus match from memory** | `FUN_1401d0630(kind, player, value)` sets the launch parameters (kind 1 character per player, 2 arsenal slot per player, 3 stage number, 4 menu id, 6 rule), then `FUN_1401d0750()` starts battle kind 6; from the title the menu manager (`DAT_140533c58`, state `DAT_140533744` 2 → needs 3) is sent to the blank menu with `FUN_140246130(mgr, 1, 0)`. Rule 2 = **Quick** (no profile arsenal read, program's own decks). The battle state `DAT_1404bc950` goes 1 → 2 → 5 (countdown) → **6 (fighting)**. A **second launch inside a running battle works** (about 3 s) and gives a clean match. | live, several times |
| **Remote calls** | At the hook the thread's volatile registers are saved, a return address pointing back at the hook is pushed, RIP set to the function; when it returns to the hook the registers are restored (`Rig.call`). Safe there: nothing live in volatile registers (checked in the disassembly). | live: 6 distinct functions |
| **Trace hooks** | sound `0x140276330`, button accepted `0x140258180`, skill fires `0x1402626d0`, Level add `0x14024d930`, **effect request `0x14018d2f0` (player, effect id)** (central: 67 call sites), capture `0x140261a00`. | live |
| **Owned skills** | profile manager `DAT_1404bec10` → `+0x3a38` → signed byte per skill index (0 = not owned, 99 cap); loads after the first START presses. | `FUN_14015ab90`, `FUN_14024c220` case 0x24; live |

### Things that behave differently from what you would assume

- **Capturing happens on a button press.** If a fighter stands on a capsule and a skill button is
  pressed, that press captures the capsule into the slot and the skill is **not** cast. A running
  take can brush a capsule. `capsule_touched` in the take's `.json` says so; the Aura Particle take
  repeats itself with the stick turned 90° (`attempts`).
- **Statuses outlive a take** (Speed Up's +20 % speed carried into the next take: 2.22 instead of
  1.85 u per tick). The runner therefore starts a **fresh match for every take** (`--reuse-battle`
  turns that off).
- **Start points are shuffled at every launch**, so each take re-measures where P0 stands and
  finds a direction in which P1 lands on the same floor at the requested distance (up to nine
  tries of 40 ticks; the result is recorded as `placement_verified`).
- **Both fighters are human slots** (pad 0 and pad 1, no AI). Pad 1 is simply left idle, which is
  "the CPU opponent held still"; its pad can also be scripted (`opponent_timeline`).
- **Launching the original rewrites the save container** (new `container.NN`, rewritten blobs,
  same sizes) the moment it starts, even before anything is pressed; closing it later did not
  change it again in any run here. See "Save safety".
- The game **simulates and logs fine with no window** (30 ticks/s); the rig needs none of the
  screen.

## Feasibility results (phase 1)

| | Result | Evidence (all under `%USERPROFILE%\.pdtools\moverec\`) |
|---|---|---|
| (a) injected stick walks the fighter | **PASS** (log). Forward stick: run at **1.85 u/tick** (program says 1.852), state 1; idle when released. | `probe7.csv` runs; every take's `*_trace.csv` (Aura take: 1.85 u/tick for 100 ticks) |
| (b) injected button casts slot X | **PASS** (log). Y press → state 10 (12 for the melee dash), `press` then `fire` events, cast sound, Aura spent, Blaster hit for 4. Press→fire ticks measured on the original: **Laser 13, Psycho Knife 18, Blaster 20, Speed Up 20, Aura Particle 0**. | `takes\*_trace.csv` |
| (c) menus to a versus match | **PASS without menus**: the match is set up from memory (above). The menu path itself (title → versus lobby → rule → stage → characters → arsenals) was **not** walked (no screen; START presses do get past the title and load the profile). | `pilot_run2.log` |
| (d) any skill in any slot | **PASS**, and **unowned skills work**: Speed Up (not in Laurence's stock) was placed, cast, fired and its sounds played with no crash; flagged `unowned (memory deck)` in the take's notes. | `takes\049_Speed_Up.json` |
| (e) game keeps running unfocused, capture works then | **Capture PASS, unfocused UNTESTED.** Windows Graphics Capture of the game window by **hwnd** works (see "Video"); the game also runs with no window at all. Whether it films while another window covers it was not tried (the game was in front). | `takes_v2\*.mp4` |
| (f) opponent held still / scripted | **PASS**: pad 1 idle = stands still (hp, position constant); an `opponent_timeline` and `opponent_skill` are built (untested live). | traces |

**Aura Particle while the stick is held (the open question in `CHAR-LIGHT.md` §8.2): the original
PC program does not hold the fighter.** Controlled take (`takes\000_Aura_Particle_held_stick_trace.csv`):
stick pushed forward from tick 15, the Aura Particle button pressed at tick 51 → `press`, `fire`,
Level 3 → 4, sound 89; the fighter's speed is 1.85 u/tick **before, at and after** the press, its
state stays 1 (run) the whole time, no pause. Standing still, the use also changes no state (0).
Only sound 89 goes through the sound routine at the use (no 90). Seen in four separate runs with
different start points (state 5 appears later in a take only where the fighter runs off a ledge).
This is the Windows program, measured with an injected pad. Why Laurence remembers a hold is
UNCONFIRMED (the 46-tick glow is the only visible thing; the Xbox build was not tested).

## Video (how the capture works, and what was wrong at first)

- **The window.** A UWP game is hosted: the visible top-level window is an `ApplicationFrameWindow`
  titled `Phantom Dust` owned by `ApplicationFrameHost.exe` (PID not the game's); the game's own
  `Windows.UI.Core.CoreWindow` is a child. `EnumWindows` does **not** list it on this machine
  (not even when it is in front); `FindWindowW("ApplicationFrameWindow", "Phantom Dust")` does
  (`find_game_hwnd`). It is 2880x1800 physical (1440x900 as the process sees it).
- **gfxcapture:** `window_exe=PDUWP.exe` and `window_title=...` fail ("Failed to find capture
  source" / matches the Explorer window "Phantom dust - File Explorer"); **`hwnd=<n>` works**, with
  `width=1440:height=900:resize_mode=scale_aspect` so the downscale happens on the GPU (software
  scaling of 2880x1800 dropped frames).
- **One capture per batch**, cut afterwards. Starting/stopping a capture per take was slow
  (ffmpeg needs 3-5 s to start; 'q' on a piped stdin is not read, so it had to be killed) and
  once wedged the game. The master is a fragmented mp4 (`-g 10 -movflags +frag_keyframe+empty_moov`)
  so it is valid even when ffmpeg is killed; clips are cut after the game is closed
  (`cut_takes`: lead 0.8 s, tail 1.2 s; `video_t0_s`, `video_dur_s`, `video_clip_start_s` in each json).
- **Alignment.** The master's time 0 is `ready_at + 0.35 s` (ready = the file passed 20 kB).
  Calibrated on three takes by the HUD Aura digit: it changes in the video exactly at the trace's
  Aura change (Level/Aura spent at `fire`) once that offset is used; before it was applied the
  video was 0.75 s early. The contact sheet's frames at `press` / `fire` show the HUD ring flash
  on those ticks. Resolution of the alignment is about one tick; the first clips of the pilot
  (`takes_v2`) were cut with the older -0.4 s epoch and need `--shift -0.75`.
- **Both views are in the picture.** The match is two human slots, so the game draws its
  **split-screen** (P1 left, P2 right), not the single third-person view of Laurence's clips
  (`Pictures\Phantom Dust - Moves*`, which are single-player vs a CPU). Same engine, same effects,
  same HUD ring, but each view is half the screen and P1's camera is not centred on P2. A
  one-player view needs a different launch (a CPU opponent / one human); not found yet.
- **Screen saver.** An ASUS OLED Care screen saver takes the desktop after about 20-30 minutes
  without keyboard/mouse activity and nothing in the rig can prevent it (`SetThreadExecutionState`
  does not). While it is on there is no window and no capture; a run that crosses that point stalls
  (battle state stays 1) until someone touches the PC. Short runs (the pilot is ~3 minutes) are fine.

## Script format

A script is a JSON file (no script files are in git yet; the pilot scripts are generated in code).
Times are seconds, converted to 30 Hz ticks; events start after `lead_in`.

    { "name": "Laser", "skill": 65, "slot": 0, "distance": 60, "lock": true,
      "level": 12, "lead_in": 1.0, "tail": 4.0,
      "timeline": [ {"t": 0.3, "do": "press", "buttons": "Y", "for": 0.1} ] }

- `skill` is the **catalogue id** (`skills.json` `id`, 0 = Aura Particle); the runner maps it to the
  game's index.
- `do`: `press` (default 0.1 s), `hold` (needs `for`), `release`, `stick` / `rstick` (`x`, `y` in
  −1..1, `for`), `mark` (a label for alignment). Buttons: `A B X Y RB RT LT START BACK LB ...`
  joined with `+`.
- `lock` holds RT for the whole take; `distance` is world units between the fighters; `level` sets
  Level and Aura of both; `opponent_skill` / `opponent_timeline` script pad 1; `attempts` repeats a
  take when a capsule was touched; `record` overrides the length (else last event + `tail`).
- A skill list (`--list`) is a CSV with an `id` column (optional `name`, `script`); skills without a
  script get an automatic one (attacks: lock on and cast; everything else: press once). Those
  defaults are a **starting point**, not verified per kind (defences need an attacker, environment
  skills need a look at what to film).

## Outputs per skill (`takes\`)

- `<id>_<name>.mp4` — a clip cut from the batch's master recording (`_master_*.mp4`, 1440x900, ~4.4 Mbit/s;
  delete it when you do not need the uncut footage). `video: null` in the json when no window could be filmed.
- `<id>_<name>_trace.csv` — **one row per program tick**: `rec_tick` (0 = the take's first tick),
  absolute `tick`, `ms` wall clock, then for each fighter x y z facing clip state health level aura
  lock band, the pad mask, and `events` (`sound:bank.id`, `press`, `fire`, `level`, `effect:player.id`,
  `capture`). Alignment with the clip: row 0 is at `video_t0_s - video_clip_start_s` seconds in the clip, row n
  at that plus `ms`/1000.
- `<id>_<name>.json` — the setup, `placement_verified`, `capsule_touched`, notes (`unowned (memory deck)`),
  file names.
- `%USERPROFILE%\.pdtools\moverec\owned_skills.csv` — id, game_index, name, count owned, capsule, school.
  Laurence's account (read from game memory, nothing changed): **126 owned, 271 not owned** of the 397
  distinct skills in `skills.json` (the two dodge duplicates dropped).

## Running it

- Double-click `tools/bats/RECORD_ORIGINAL_MOVES.bat` (pilot) or drop a skill-list CSV on it.
- Or `python engine\re\move_recorder.py --pilot | --list FILE | --script FILE | --dump-owned`,
  options `--no-video`, `--stage N`, `--unowned include|skip`, `--reuse-battle`, `--leave-game`,
  `--out DIR`; `--dry-run-pilot` prints the plan and touches nothing.
- The run: snapshot of the package folder → start the game if it is not running (this run then
  owns that process) → START presses until the profile loads → versus launch → takes → detach
  (patched bytes restored, the game is released) → if the run started the game, close **that**
  process, put any changed save file back from the snapshot, hash-check against the snapshot and
  against the 2026-10-01 backup. A game that was already running is never closed.
- While running it asks Windows not to start the screen saver or switch the display off
  (`SetThreadExecutionState`, the call video players use). That cannot undo a screen saver that is
  already on.

## Save safety

- Backup of the package at the start of the lane: `%USERPROFILE%\.pdtools\original-save-backup-20261001`.
- **Starting the original rewrites two blobs and the container index** (container 55 → 57 → 58 → 59 …;
  blobs are enciphered, same sizes). Seen at every launch, before any input. No later write was
  seen from the match, injected input, slot changes or the Aura use (no battle was ever finished, so
  no result / reward write was exercised).
- `save_guard.py` compares by SHA-256, copies back differing save files, **never deletes** (the
  orphan `container.NN` / blob files a run leaves are left in place; the index points at the
  restored container). After every session of this lane the folder was restored and verified:
  *0 save files differ from the backup*.
- Never run `--restore` while the game is open.

## The full unattended run (`tools/bats/RECORD_ALL_MOVES.bat`)

- **List:** `%USERPROFILE%\.pdtools\moverec\all_skills.csv` (341 rows: the 340 roster skills + the Aura Particle) made by
  `python engine\re\mr_batch.py --make-list` from the usage order of `audits/SKILL-BY-SKILL.md` and `skills.json`. Columns:
  rank, id, name, family, distance, lock, tail, level, health, opponent_skill, opponent, flag. The **family plan** (`mr_batch.family_plan`):
  melee (punch / sword / pursuit) 35 u locked; shells 60; arc / lob / fall 70 and a 5 s tail; rain / tornado 70 and 7 s; crawl / hole 50
  and 6 s; self / environment skills unlocked; **defences** get a scripted attacker (P2 holds RT and casts **Blaster** at t+0.5 s, 35 u
  apart: generic timing, flagged "unverified"); **mines** flagged "trigger not scripted"; **movement** skills flagged "no stick
  scripted". Level / health come from the skill's own requirement (kinds 1, 2, 8, 9; kind 5 "skills left" is flagged, not set up).
  Edit the CSV to change any row; a bare `id,name` list also works (generic script).
- **Run:** double-click `tools/bats/RECORD_ALL_MOVES.bat` (or `python engine\re\move_recorder.py --list all_skills.csv --out
  %USERPROFILE%\.pdtools\moverec\takes_full`). **Screen saver / blanking must be off** (it checks and waits up to 10 minutes).
  About 90 minutes: ~39 min of footage + ~9 s per take for the fresh match, plus relaunches.
- **Watchdog:** the session ends when no game tick arrives for 25 s, a take passes its wall-clock limit, a match will not start, or the
  game process dies. The game this run started is closed, the save is restored from the pre-run snapshot and hash-checked
  (**after every session**, and once more at the end), the game is started again and the run carries on with the takes not done.
  A take that takes the game down twice is skipped and written as status `crashed`. Four sessions in a row with no finished take
  = give up (resume later with the same command). Seen live: the game died once in six relaunches (during a take), the watchdog
  relaunched and the run finished with all takes.
- **Resume:** a take is finished when its `.json` says `finished` and its trace exists; those are never redone (`--no-resume` redoes).
  Clips are cut in the background from a master recording per 25 takes (masters are deleted after their clips are cut; one left behind by a
  crash is cut on the next start).
- **Outputs** in `takes_full\`: per take `<id>_<name>.mp4`, `_trace.csv`, `.json`; `summary.csv` (rank, id, name, family, **status**
  = ok / refused (the press was not accepted: requirement, cost) / no-fire / WRONG-SKILL (the fire event named another index) / crashed,
  press / fire / hit ticks, flags); `run.log`.
- Every take's `.json` has `index_ok`: the skill index the program fired is compared with the table index asked for.

## What the full run still does not do

1. **A one-player view** like Laurence's clips: the matches are two-human split-screen.
2. **Per-skill scripts beyond the family plan.** Defence timing, mine triggers, movement direction, erase targets and
   "skills left" requirements are generic or unset (flagged in the CSV); the summary shows `refused` where a requirement blocked the cast.
3. **Effects / projectile numbers** beyond effect ids and the health columns.
4. **Stage choice** (`--stage N`, unconfirmed mapping) and the real menu path.
5. **Screen saver:** while it is on there is no window; the runner refuses to start and waits when it comes back.

## Trace-only full pass and the live audit (2026-10-01 evening)

- **Trace-only run** (screen saver on, Laurence away): `python engine\re\move_recorder.py --list all_skills.csv --out
  %USERPROFILE%\.pdtools\moverec\takes_full_trace --no-video`. The game needs no window for any of this; the screen-saver wait
  is skipped when there is no video. Same watchdog / resume / per-session save restore as the video run. The watchdog fires
  in practice (the game thread froze once in the first 8 takes: `STALL (no game tick for 25 s)`, relaunch, carried on).
- **`full_trace_summary.csv`** (`mr_trace_summary.py`, written at the end of every run and runnable on any takes folder): one row per
  take: press / fire ticks and the gap, the cast state and how many ticks it lasts, the state sequence of both fighters,
  Aura / Level / Health of both fighters before the press, at the fire tick and at the end, the first hit on the opponent (tick,
  damage), the sounds (bank.id x count; the cast sounds are the ones at the press tick) and effects (player.id@tick) the original
  requested, the fire index check, status and notes.
- **`skill_audit_live.py`** (separate from the probes lane's `skill_audit*.py`; it only imports `skill_audit_rules` for the cell
  types): turns each summary row into live cells against the ledger's program values: `fire_tick` (action row w43, 0 for
  clip-0 rows), `cast_sound` and `cast_effect` at the press, `aura_cost` (the chk_pay rule incl. Level-penalty clamp and Retain x2),
  `level`, `health`, `skill_index`, `castable`. Refused presses, charge rows, copy skills and own-Aura-effect skills are holes
  (`...`), never findings. `python engine\re\skill_audit_live.py [--out live.md] [--json live.json]`;
  `live_section()` returns the Markdown (counts, mismatch classes, every cross) for the report to include.

### Result of the trace-only full pass (2026-10-01, 12:09-14:14)

- **341 takes, 341 finished** (`takes_full_trace\`, `full_trace_summary.csv`): **334 ok**, **7 refused** (all expected: Rail Gun, Excalibur,
  Cluster Bomb need "7 or fewer skills left in the arsenal" (not set up); Orb and Amulet are passive / reactive; Recollection and Copy
  need a previous attack to copy), 0 crashed, 0 wrong skill (all `fire_index` = the table index asked for). Save: 0 files differ from
  the pre-run snapshot and from the 2026-10-01 backup after the run.
- **The game's main thread hangs after 7-10 in-battle relaunches** (the watchdog's thread dump showed it inside the effect-manager
  update, `FUN_1401899b0`, called from `FUN_14018c7a0`; 5 of 5 early sessions). The runner therefore restarts the game every
  `SESSION_TAKES` = 6 takes (a planned end of session, ~45 s) instead of waiting for the 25 s stall. With that: about 2.7 takes
  per minute, one real stall and one crash in ~60 sessions, all recovered by the watchdog. A killed game can linger a few seconds;
  the runner closes a leftover game it started itself before launching again (never one older than the run).
- **Live audit** (`skill_audit_live.py` on the summary): 1870 checks match, **5 live crosses**, 47 unconfirmed (guards: the fire tick is
  reactive), 21 holes. The crosses: four guards (Mind's Eye, Guard of Water, Auto Shell, Guardian Angel) never play their row's cast sound in
  the take, and Dash fires 1 tick after the press where the row says 0. A guard has no accepted press of its own: it fires when the
  attack arrives (a few ticks after the attacker's fire).

## livefix lane (2026-10-01): the live crosses and the knock-down drift

- **Automatic guards (Mind's Eye, Guard of Water, Auto Shell, Guardian Angel) play no cast sound and no voice** (READ
  `FUN_14026f530` -> `FUN_1402626d0`: the kind-7 branch starts the clip with `FUN_140257310` directly; the row's w3 cast sound and
  w4 voice are played only by `FUN_14025a3e0`, the press path). LIVE: 4 of 43 guards, exactly those. Ours: `Combat::auto_guard` marks the cast
  (`Fighter::auto_cast_seq`) and `sfx.rs` skips the cast sound and voice for it; the cast effect and clip are unchanged (LIVE: effect
  `0.<skill index>` is requested at the fire tick).
- **Dash fires one program tick after the press** (LIVE `240_Dash`: press 39, fire 40, the first 6.7 u step on tick 40). READ `FUN_14025a3e0` sets
  the countdown `+0x64 = w43`; `FUN_14025c720` checks it in the NEXT tick's state update (`< 1` fires), so a real clip with w43 = 0 (Dash row 294,
  the only one) fires like w43 = 1: `cast_from` uses `fire_tick.max(1)`. Clip-0 rows (the other 11 zero-fire skills) still fire in the press.
- **Root motion is real and exact.** The program moves a knocked-down body by the character's root track (`FUN_140257510`, flag set by
  `FUN_140259990` for the landing clips and by the get-up). With P1 = `pc01a` (character index 1): get-up net drift 6.48 u (DOWN_FRONT_STAND, file 6.51)
  and 2.17 u (DOWN_BACK_STAND, file 2.21) in 14 takes; the large landing (Whirlwind / Twister / Tornado, 87 u fall) drifts 18.38 u along the facing,
  `pc01a` BLOW_LAND_LF is -18.22. P0 (`pc00a`) got up with 3.38 u in four takes (file 3.24 / 3.20). The game plays the **colour files** (`pcXXa0.alr`,
  never the base file), which is what `pd_convert character` exports: the -17 / -22 u of PROGRAM-RE were measured on a base file. So pc00a's BLOW_LAND_LF -60 /
  LB +72 stands as the colour file's number; **no take has a pc00a body in a large landing** (UNCONFIRMED live: one Whirlwind take with the characters swapped closes it).
- **Slide after a small landing** (`engine/re/knock_live.py` prints every take): two shapes, 6.34 u (Photon Burst, Psycho Sniper, Mist Blade, Vibration
  Blaster, Illusion of Death: the 0.85 per tick slide plus the front landing clip's root track; ours 6.3, tested) and 1.7 u (Photon Wave, Bomb, Big Bomb, Delay
  Bomb, Hyper Ki Bullet, Hyperpunch, Swift Kick, Fast Wave, Aura Backflow: speeds .90 .65 .39 .15 then only the SB root wobble). Same start point and direction
  (Hyper Ki Bullet vs Judge of Anubis), same flight (26.2 u in 22 ticks). Ours gives 5.4 u for the second shape: **UNCONFIRMED program reason**, not changed.

## Live values of the school-aura glow (2026-10-01 evening, `mr_auralog.py`) -- LIVE

Question from the auralook lane (FORMATS-EFFECTS.md "School aura glow"): why is our halo ~2x narrower than the footage?
Tool: `python engine\re\mr_auralog.py [--ticks 90] [--level 9] [--no-skip-take] [--out DIR]` (hooks: the aura object's update `0x14019b520`,
draw `0x14019b920`, silhouette `0x14019c3d0`, MFilter `0x140286c60`, GFilter2 `0x140287460`, weight table `0x1402864e0`, composite
`0x140289140`, field set `0x140292d40`; float arguments are read from xmm registers, new in `live_trace.py`: `xmm0`..`xmm15`).
Versus match, P0 at Level 9 (tier 3), P1 at Level 0, 90-120 program ticks logged, the game window filmed. Results in
`%USERPROFILE%\.pdtools\moverec\auralog*\` (`summary.json`, `calls_*.json`, the master video). Game code was never changed on disk; the one
experiment redirects the game thread at the call (a return from the draw) while the debugger is attached.

- **LIVE: the halo does not come from `FUN_14019b920`.** In one take the draw ran normally for 60 ticks, then **every call of `FUN_14019b920`
  returned at once for 60 ticks** (120 skipped calls; no silhouette / MFilter / GFilter2 / composite call happened in that half).
  The red school halo round P0 in the video **did not change**: red-excess pixel area round the fighter (frames at 20 fps, red - max(G,B)
  > 40) **1873 +- 61 px before vs 1911 +- 71 px after** the skip (33 / 49 frames). So whatever draws the visible halo, it is not (only) the
  HUD aura object's composite. It does depend on Level: the same measurement at **Level 0 gives ~170 px** (an 11x smaller area) with
  the draw running normally. Candidates the auralook lane should read next: effect 600's wisps (Level-dependent) together with the engine's
  generic glow pass (second render target, RESEARCH-LIGHTING.md), which the footage's wide halo may be mostly made of. Caveat: one fighter /
  one view, red excess also counts the wisps' own pixels; the two school colours seen (Faith purple, Psycho red) differ per launch (the dealt deck).
- **LIVE: the draw and its whole chain run for every Level** (also Level 0: 94 draws, 188 composites in 60 ticks), the per-fighter opacity byte
  is 255 for all four slots, flags `0x33` (P0) / `0x30`.
- **LIVE call counts per program tick (30 Hz)**, 120 ticks, all hooks on: update 1, `FUN_1402864e0` 4, draw **2**, silhouette 2 (per tick; 1 per
  slot pair), MFilter 1, GFilter2 **1** (arguments `(first = 0, count = 2)` and `(2, 2)` alternating, i.e. 2 draws = the two slot pairs), composite 2 per draw
  (4 slot tiles per tick): each fighter slot's blur / persistence chain runs **once per program tick = 30 Hz**, not 60 Hz. With the video
  capture running the game drew fewer than that (1.73 draws per tick over 90 ticks in one take); it never exceeded 2.0. UNCONFIRMED: whether
  an unhooked game could render more than once per tick.
- **LIVE weights (`FUN_1402864e0`)**: its float argument (xmm2) is **0.2 for the Level 9 fighter and 0.3333 for the Level 0 fighters**, i.e.
  c4 = 5.0 (f36 = 500, tier 3) and 3.0 (f36 = 300). The 8 weights of the Level 9 fighter, as stored: `0.285019 0.104852 0.017332 0.001287
  0.348122 0.191054 0.047113 0.005220` = `exp(-0.2 (T_k + 0.25))` normalised for T_k = 0 1 3 6 10 15 21 28 (the ledger's formula, confirmed);
  Level 0: `0.316607 0.059799 0.002977 0.000039 0.441860 0.162551 0.015763 0.000403`.
- **LIVE composite (`FUN_140289140`) arguments**: ecx = 254 (opacity 255 squared >> 8), x = 0.5 / 0.75, y = 0.5 / 0.75 (the four fighter slots:
  `x = (u & 1) * 0.25 + 0.5`, `y = (u >> 1) * 0.25 + 0.5`), w = 0.25, h = 0.25 (fractions of the 512 x 512 target: a **128 x 128 px tile**, the
  lower-right quadrant), a per-view matrix pointer, last argument 254.
- **LIVE field at composite time**: `DAT_14057f36c = 240`, `DAT_14057f370 = 426.5`, `DAT_14057f374 = 2.0`, `DAT_14057f388 = 16000`,
  **`DAT_14057f390 = 1.309` rad (75 degrees)**. `FUN_140292d40` returns nothing (it only sets those globals from its arguments:
  `(width, height, 2, 16000, field)`); the calls seen per tick: `(640,480)` x2, `(320,240)` (field 1.0472 or 1.309), `(426.5,240)`, `(512,512)` and
  `(256,256)` (the silhouette tile, field 1.309). With the constants read from the exe (`0x1402f7df4` = 1.309, `0x1402f7cd0` = 0.087266,
  tan angles `0x1402f7e78` = 0.6545 rad = 37.5 deg and `0x1402f7e80` = 0.6981 rad = 40 deg, 37.6): the composite's blend factor
  `(1.309 - 1.309) / 0.087266 = 0`, so **H = 37.6 tan(37.5 deg) = 28.85 u** -- the ledger's value, confirmed. The tile's world size is not 2x
  larger; the narrowness is not from the field.
- **LIVE persistence**: `DAT_14054c1f0` (the blend constant the V pass uses) = `[100, 100, 100, 100]` in the Level 9 take (f35 = 100 for tier 3, as in the
  script table; alpha 100/255 = 0.39 per frame).
- **LIVE feedback**: over 120 ticks the source texture `DAT_14054c208` and destination `DAT_14054c228` are the same two objects in every call: **no ping-pong**.
  The previous result stays in the destination and is added with the constant alpha (`0xd101` = ONE, CONSTANT_ALPHA); nothing seen rebinds it as a
  blur input. Whether the V pass reads the destination as a source is UNCONFIRMED (only the unchanged slot bindings were observed).
- **What this means for the width question** (stated as measurement, not as a fix): the halo's width in the footage is not explained by (a) the field
  value, (b) the call rate of the persistence loop (30 Hz, which would make the blur *wider* than our 60 Hz loop, not narrower), (c) a re-blur of the
  previous frame (not observed); and the visible halo is still there with the whole draw skipped. The next place to look is whatever else draws a
  Level-dependent glow around the fighter (effect 600 wisps + the generic glow pass).

### Elimination of the halo's source (2026-10-01 night, `mr_aurahalo.py`) -- LIVE

Tool: `python engine\re\mr_aurahalo.py --segments "label:names@ticks,..." [--pre names] [--census] [--skip-addr name=0xADDR] [--out DIR]`. Same setup as above
(versus match, P0 Level 9, P1 Level 0, window filmed). A segment skips the named functions (the debugger redirects the game thread at the entry to its return
address, rax = 0; nothing changed on disk), `--pre` keeps a skip on from the boot, `-name` in a segment lifts a pre-skip, `--census` counts the callers of every draw
primitive per segment. Seven live runs of 1 to 2 minutes each, the save restored by hash after every one (0 files differ from the pre-run snapshot; the game adds two
new files to the container folder, which the guard never deletes). The measure is the school-colour excess round the fighter, `(R + B) / 2 - G > 25` (red, purple and
green decks all work), plus a look at the frames (`%USERPROFILE%\.pdtools\moverec\aurahalo*\sheet*.png`). **Caveat that matters for every number below:** the halo
pulses by itself (strong for the first 2-4 s after the Level is raised, weaker later; the same unskipped take went 4190 -> 4350 -> ~900 px), so only
pictures with the skip on while the halo is visibly strong count as "still there".

1. **Skip the eft0600 wisp draw `0x14011b450`** (800 calls skipped, 40 ticks): halo **not gone** (663 +- 92 px before, 732 +- 170 px during; frames identical in look).
2. **Never let effect 600 exist** (`FUN_14018d2f0` returns -1 for effect ids 600-609 from the boot on; 190 requests refused, all of them id 600 from
   `FUN_14019e6c0`; no other effect id of that range was requested): the Level 9 fighter still has a big red halo round head and shoulders (picture
   `aurahalo2\f_nofx.png`), so the halo is **not effect 600's wisps and not anything effect 600 creates**. The black spheres with a white rim at the hands are
   there with or without effect 600 (not investigated; they are not the halo).
3. **Level-dependent draws on the fighter, by call census** (every DrawIndexed / Draw wrapper, 17 call sites hooked, 30 ticks at Level 0 against 30 ticks at
   Level 9, counts normalised by frames drawn: the scene pass `FUN_140299310` runs 20 times at Level 0 and 34 at Level 9, i.e. the renderer ran faster, not
   the content changed): **the only draw call site that exists at Level 9 and not at Level 0 is the wisp draw (`idx@14011b994` in `FUN_14011b450`, 3.4 per frame).**
   Every other site has the same count per frame at both Levels (model draw `FUN_140298a80` ~300 per frame, `FUN_140298b60` 18, `FUN_1402a7df0` 8.5 sprites ...).
   So the halo is made by a draw that also happens at Level 0 (there the halo is ~11x smaller) and takes its strength from a constant, not from an extra draw.
   Skipped one after the other with the halo strong and **still there**: the whole HUD aura object draw `FUN_14019b920` (360 and 1253 and 1226 calls; skipped from the
   boot too, so a frozen leftover texture is not the explanation: in the take where the fighter turned round while it was skipped, the halo followed the new
   silhouette), the composite `FUN_140289140` (240 calls), the status-object draw `FUN_14024d3e0` (240 calls), the fighter's attached object at fighter + 0x17180
   (nulled for 60 ticks; its vtable is `0x14048d488`, draw entry `0x140004660`, the same for both fighters), the scene glow/blur chain `FUN_140288b00` (120 calls: the
   sky bloom gets weaker, the fighter's halo does not) and `FUN_140287b10` (120 calls). Not removable by any of them. `FUN_1402886e0` (the other post chain) is
   never called in a versus match (`DAT_140530034 & 8` set).
4. **Where the aura object's 512 x 512 target is displayed: not found in the versus view.** Facts read: `DAT_14054c208` / `DAT_14054c228` are the two shared scratch
   targets of the whole renderer, not the aura object's own: `FUN_140289140` (composite), `FUN_140287460` (GFilter2) and `FUN_140286c60` (MFilter) use them for the aura,
   and so do the scene bloom chain (`FUN_140288b00`: `FUN_140286590` downsample, `FUN_140287080` x4 blur, `FUN_1402885d0` -> `FUN_140288100`, the only full-screen quad
   that samples `DAT_14054c208`, blend `0x1101`) and `FUN_140285c60` / `FUN_140287b10`. The aura object's draw only does its work when `DAT_14052fff4 & 0x100000` is set
   (all its calls run in a versus match, LIVE), writes silhouettes into 128 x 128 tiles of the 512 target and composites them into that same shared target, which
   the bloom chain then (reading, not traced) overwrites by its own downsample before it samples it. Skipping the whole object (item 3) changed neither the world view nor the HUD dial /
   bars (earlier mean-frame comparison: HUD diff 0.07 against 0.07 jitter). **So in a versus match the object's output does not reach the picture we filmed; where
   it is shown (single player / campaign HUD portrait, pause or status screen) is UNCONFIRMED and was not run.** Consequence for our `aura_glow`: it copies a
   pipeline whose visible output the original does not show in versus; the halo seen in the footage and in these takes comes from somewhere else.
5. **Still open, narrowed:** a Level / school-dependent constant in the generic model or character draw (`FUN_140298a80`, `FUN_140298b60` and their callers; the
   per-fighter draw is `FUN_14024f2f0`), or a full-screen pass whose input is the fighter's colour. Next live step: skip `FUN_14024f2f0`'s call into the model
   (`FUN_14024f5e0`) while keeping the rest to see whether the halo goes with the body, then bisect the shader-constant writers (`FUN_14001e540` calls) by caller.

### Halo bisection, round 2 (2026-10-01 night, `mr_aurahalo.py --sites`) -- LIVE / READ, source STILL NOT FOUND

- **LIVE, corrects the round above: the halo pulses on its own** (about 5 s on, then 1-2 s almost off, then it grows back over ~2 s). Two "the halo vanished with this skip" results
  (indexed-draw calls 700-820 of a pass; descriptor group 0.29-0.31) were this natural off phase: in the second case **no draw was skipped at all** (skip counter absent). Treat only skips
  made while the halo was steady and compared with an unskipped neighbour. Round 1 conclusions stand (they were "still there" results).
- **LIVE: the school colour is the halo colour**: P0's aura school index is read at manager `DAT_1404bbee0 + 0x60 * player + 0x44`, colour floats at `0x140599080 + 0x10 * index`
  (2 = yellow 0.88/0.75/0.06, 3 = green 0.06/0.91/0.16, 4 = blue 0.06/0.19/0.88, 5 = purple 0.87/0.03/0.88); the halo in the video had exactly that hue in 5 launches.
  READ: that table is consumed only by the effect-600 manager path (`FUN_14019e280/e430/e5d0/e6c0`, `FUN_1402501d0`, `FUN_140251d80`, `FUN_140251f80`, `FUN_1402526c0`) and the HUD aura object's
  own colour, both already ruled out as the drawer. So the colour reaches the picture some other way (UNCONFIRMED how).
- **LIVE draw-descriptor census** (every call of the indexed-draw wrapper `FUN_14001ede0`, key = its 4th argument, 585 descriptors, Level 0 against Level 9): all descriptors keep
  their per-frame ratio except the six wisp descriptors; no new mesh, no second pass of the fighter. The halo is therefore made by existing draws with a changed constant, or by a
  non-indexed path (sprite flush `FUN_1402a7df0`, quads `FUN_14028caa0`, `FUN_14022d840`: each skipped alone, halo unchanged).
- **LIVE group scans with the halo steady**: skipping descriptor groups (by order of first appearance) over ranks ~93-290 of ~620 (round 14, 16 groups) and 8 groups over the whole range
  (round 16, camera too far to score) removed nothing that could be told from the pulse; group 0.25-0.375 removes the fighter body (halo gone with it, orbs stay).
- **READ shaders**: `SWBOS_ps` has no emissive or rim term; its only per-draw outputs are the lit colour and `o1 = cb0[17]` (the dual-source blend input, FORMATS-EFFECTS.md "glow pass: re-read");
  the model has no Level-dependent constant in `FUN_140199e70` (lighting only) or `FUN_14024f2f0` (only the wall-dissolve alpha `0xed000021`).
- **Open, next**: judge halos only with the camera close (`PD_`-style placement: settle P0 within ~8 u of the camera, as in round 1 runs 2/6) and a steady phase; then bisect the
  non-indexed paths and the pass-level flushes (`FUN_140298e40`, `FUN_140298f70`, `FUN_1402990a0`, `FUN_1402991d0`) individually.
