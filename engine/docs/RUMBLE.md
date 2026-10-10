# Controller vibration (rumble)

Laurence 2026-10-03: "We need rumble support. The original vibrated when taking damage etc."
Lane `rumble`. Code: `engine/crates/pd_game/src/rumble.rs`. Setting: **VIBRATION ON / OFF** on the pause / options menu's CONTROLS tab (moved there 2026-10-08) (`Settings::rumble`, json `"rumble"`, default on).

Tags: **READ** = read from the program (`PDUWP.exe`, Ghidra export) or the data (`gsdata_en.dat`); **UNCONFIRMED** = not read or not implemented.
No game code is copied here; logic is described in our words with addresses.

## What the original does (READ)

**Output path.** The program uses WinRT `Windows.Gaming.Input.Gamepad` (not XInput). `FUN_1400de3f0(pad, {left, right})` is the only place that sets vibration: it divides two u16 levels by 65535 and calls `IGamepad::put_Vibration` (vtable slot 7, offset 0x38) with LeftMotor = level0, RightMotor = level1, **both triggers 0**. `FUN_1400dd9d0` is the same call with explicit doubles (used with 0, 0 to stop: `FUN_1400de2f0` on disconnect). So: **no trigger motors**. Left motor = Bevy `strong_motor`, right motor = `weak_motor`.

**Per-frame flush** `FUN_14022c480` (the pdvkFBManager update, vtable `0x14048d158`): only pads 0 and 1 (two pads), each frame sends the summed level and clears it. It zeroes the level when any of these hold:
- the per-pad vibration option nibble is 0 (`DAT_14052dfb2`, low nibble pad 0, high nibble pad 1) -- **the option flag that turns it off**; its default value is UNCONFIRMED (ours: on);
- `DAT_140530034 & 0x20001` (pause-type state flags; READ, meaning inferred = paused / not live);
- `DAT_140533744 != 3` (not the in-battle state);
- the game mode `DAT_1404bc950` is one of 1, 2, 3, 4, 7, 8, 10, 11, 12, 13 (menus / result screens).

**Timelines.** Each pad owns a `pdvkComFeedBack` (0x228 bytes; created in `FUN_14022c120`): up to 16 pieces of 0x1c bytes, a start delay, an end tick and a loop mode, stepped once per **program tick (30 Hz)** by `FUN_14022c780`. Piece types: SET (the level becomes strength once, at its start tick) and ADD (strength is added on every tick of `start..end`). Strength unit: table value x 0.01 x 65535 (so 30 = 30 % of full), clamped to 0..65535 per motor. Motor kind in the tables: 1 = both, 2 = left, 3 = right. Loop mode (row w5): 0 stop at the end, 1 restart, 2 step back one tick (rows 57, 74, 389 and two others only).

**Who starts a timeline** (all READ; the pad is the player's own: `pad = player index` in versus mode `DAT_1404bc9d4 == 6`, else pad 0; and only when the player is a human, `player+0x30 != 0`, so **CPU seats never rumble**):

| # | Game event | Program | Table row | Motors, strength, length | Status |
|---|---|---|---|---|---|
| 1 | **Cast starts** (any skill press that starts a cast) | `FUN_14025a3e0` -> `FUN_14025a8a0` | the cast's action row, words w5..w25 (385 of 574 rows have it) | most common: right motor 10 % for 25 ticks (102 rows), 30 ticks (79), 35 (34), 20 (26); 30 % right for 30 ticks (30 rows), 30 % for 5 ticks (20 rows, with a delayed extra piece on 15). Plus up to four extra ADD pieces (typically right +1 % for ~20 ticks) | READ (rows from the file) |
| 2 | **Skill fires** (the fire tick) | `FUN_14025c720` / `FUN_14025a3e0` -> `FUN_14025aad0` | the action row's w26..w37 | only rows 57, 74, 389: right motor 40 % for 5 ticks. It *replaces* the cast timeline. READ quirk: its two extra pieces are only added when the cast block's pieces w10 / w14 have a motor | READ |
| 3 | **Taking a hit** (any hit reaction: stagger, knock-down launch, collapse, KO fall) | `FUN_140258690` (clears the pad's timeline first, `FUN_14025ace0`, then `FUN_14025a8a0`) | the hit-reaction row (gsdata 0x2B00C, the same row the reaction clip uses) | **left (strong) motor only**: rows 1-33, 37-39, 40-43, 45-70: 30 % (light/medium staggers), 40 % (rows 14-24 status/bind etc.), **50 % for heavy / knock-down rows (3, 5, 7, 9, 11, 13, 32, 39, 45, 48 ...)**, each for **20 ticks = 0.67 s** | READ (rows from the file) |
| 4 | **Knock-down lands** (touch-down after the flight) | `FUN_140254c80` -> `FUN_140258fb0(row)` | reaction rows 34 / 35 / 36 by fall height (< 40 u / up to 80 u / above) | left motor 40 % for 6 / 10 / 15 ticks | READ |
| 5 | Skill-jump / landing from a 0x38-0x3b state | `FUN_140259990` | reaction rows 37 / 38 / 39 (+ fall class) | left motor 30 % / 40 % / 50 % x 20 ticks | READ the call; **UNCONFIRMED** which of our events matches (not implemented) |
| 6 | Button press while a status holds the buttons (status 6 / 0x12), bound-struggle push (status 4), `FUN_140261e90` | `FUN_140258fb0` rows 40 / 41 / 42 / 43 | reaction rows 40-43 | left motor 30 % x 20 ticks | READ the call; not implemented (UNCONFIRMED event match) |
| 7 | **Stage-object screen shake** (`pdvkEbScrShk`, `FUN_140220f10`) | accumulates straight into the pad level (`FUN_14022c660(0, rand x shake x 65535)`) every program tick inside its window, scaled by distance to the player | stage EF00 records (camera-shake type 16) | right motor, random 0..shake amplitude | READ the structure; **UNCONFIRMED / not implemented** (needs the per-player distance rule and the shake record units) |
| 8 | **Capsule / environment object effect** (`FUN_14021c540`) | accumulates `rand(10000..35000)` into the right motor every second tick, and raises the camera-shake floor | -- | right motor, random 15-53 % | READ the structure; **UNCONFIRMED** which object it is (vtable shared by pdvkCapEffect / pdvkEnvCapsule / pdvkBattle); not implemented |
| 9 | Spatial feedback emitter (`pdvkFeedBack`, effect event 0x40 / 0x41, `FUN_1401b0790` -> `FUN_14022bdf0`, update `FUN_14022bf80`) | distance falloff `(1 - d/r)^2` times a per-tick pattern from a 10-sample table | -- | both motors alternating | READ the structure; **UNCONFIRMED** which effect scripts emit it; not implemented |

Which pad is "the affected player's": the timeline index is the player the call site belongs to (`+0xbd0 -> +0x14`), so a hit makes only the victim's pad buzz and a cast only the caster's.

## What we implemented

Presentation only (`rumble.rs`), reading sim state; the sim never reads it:
- **Timeline model** (`Timeline`): the program's SET / ADD pieces, delay, end, loop, 30 Hz stepping (every second 60 Hz sim tick), strength x 0.01 x 65535, clamp. Rows come from `skills.json` (`action.ground/air/chain[].raw`, `reactions[].raw`), read at startup.
- **Events**, none new in the sim: `Fighter::cast_seq` changes (cast; not the automatic Shelter, as in `sfx.rs`), `CombatEvent::SkillFired`, `Fighter::hit_seq` changes (hit reaction row = `Fighter::reaction_row`, clears the old timeline first), `CombatEvent::KnockLanded` (rows 34-36 by `Fighter::land_class`).
- **Pad choice**: `Seats::pad[seat]` -- the same seat mapping `gather_input` uses (never "the first pad Bevy lists"). Seats without a pad, and `SeatKind::Cpu` seats, get nothing.
- **Delivery**: `GamepadRumbleRequest` (Bevy 0.19, gilrs force feedback). Each change of level sends `Stop` then `Add { duration 0.18 s, strong = left, weak = right }`; a held level is re-sent every 0.10 s, so a lost frame can never leave a pad buzzing. Never stacks.
- **Stops**: paused (pause menu open, or a seat's pad missing), setting off, left the match (title / result), round or match restart (timelines cleared, nothing replayed).
- **Vendored fix**: upstream `bevy_gilrs 0.19.1` sends the *weak* request to the *strong* (left) motor (its `rumble.rs` builds both as `BaseEffectType::Strong`, and the weak one has no replay length). `engine/vendor/bevy_gilrs` is a copy with that one effect fixed (`Weak`, same duration), wired with `[patch.crates-io]` in `engine/Cargo.toml`. The copy also logs, when a pad is found or connects, `gamepad found: <name> (force feedback: yes|NO)`.
- Not implemented (the table above, UNCONFIRMED): rows 5-9. Triggers: the original never uses them.

## Tests

`cargo test -p pd_game rumble` (mocked pads, no OS input): timeline arithmetic (hit row = left motor 50 % for 20 ticks then ends; cast row = right motor; fire block replaces the cast and has the program quirk; cast-row choice by s40 / air / chain; landing rows by fall class), and the driver with two mocked seats: only the hit seat's pad gets a request, a CPU seat gets none, pause sends one Stop then nothing, leaving the match stops it, setting off sends nothing, a hit buzz ends by itself, setting default on and round-trips.

## What to feel (Laurence)

Take a normal hit: the **left (heavy) motor** buzzes at 30-50 % for about two-thirds of a second. A knock-down launch is the 50 % one, then a short thump when you land (0.2-0.5 s). Casting any skill gives a very light right-motor (high-frequency) tick: 10 % for about 1 s on most skills, 30 % for a short beat on the punchy ones. Only the pad of the player involved moves. Pause or open the menu: it stops. VIBRATION OFF in the menu: silent. Not felt yet: stage-object shake and capsule effects (rows 7-9 above).
