# Camera audit: free camera, lock-on camera, framing, pitch, camera-relative movement (2026-09-30, cloud, read-only)

Document-and-code audit only, in the style of `MOVEMENT-AUDIT.md`. Nothing was run; there is no game data or Ghidra export in the cloud session. Every "program read" below is quoted from `PROGRAM-RE.md`, `GAME.md`, `FOOTAGE.md` or `VERIFY.md`; nothing is a new claim about the program. "Check on PC" items are for a Ghidra session. Code: `pd_game/src/sim.rs` (deterministic), `pd_game/src/main.rs` (`follow_cameras` ~1406, `layout_viewports` ~1499, `apply_fov` ~1527, input ~1020-1200). There is no separate camera module.

## 0. Headline findings

1. **The GAME.md contradiction is resolved: the code is right, GAME.md line 80 is stale.** `sim.rs` has no `FREE_CAM_FOLLOW` (grep over all `.rs` and `.md`: only GAME.md:80 and MOVEMENT-AUDIT.md mention it). Running never turns the free camera: `sim.rs:592-601` changes `cam_yaw` only from the right stick (`input.cam`) or the recentre click, and the test `running_never_turns_the_free_camera` (`sim.rs:765`) asserts it. GAME.md's own controls paragraph (line 156) says "moving never turns the free camera (Laurence 2026-09-27: like modern third-person games)". The 2.5/s follow was evidently an earlier build, removed on Laurence's request. **It is a deliberate departure chosen by Laurence, not a program fact**: the program's behaviour is not read (see section 3).
2. **Every lock-on camera rate in the program is unread.** PROGRAM-RE.md says the camera yaw and pitch are rate-limited toward the target and the pitch is clamped, from `configdata.dat`. Ours snaps: the sim sets `cam_yaw = yaw_of(toward(opponent))` instantly (`sim.rs:427`), and the render camera smooths by eye (`1 - exp(-8 dt)`). So our movement frame (stick-relative-to-camera) changes instantly; the program's would lag at a per-player rate.
3. **Most camera numbers are tuned by eye**: boom 60 / height 30 / look-ahead 30, pitch limits, smoothing rates 6/8/2 per s, occlusion margins, top/bottom split constants (section 5). None has a program source.
4. Two more doc-vs-code drifts: the lock button (GAME.md:80 says gamepad Y, GAME.md:156 and code say right trigger) and `sim.rs` module header (`sim.rs`/`main.rs:12`) versus GAME.md. See section 6.

## 1. What the program read says (all from PROGRAM-RE.md unless noted)

- Fighter object: camera yaw `+0x12e5c`, camera pitch `+0x12e60` / `+0x12e64`; "movement is camera-relative" (PROGRAM-RE.md "Fighter object", ~line 520).
- Facing turns toward (camera yaw + stick angle) at the character's turn rate, 20 deg/tick; speed times `max(0, 1 - angle_left/90)` ("Facing turns, position follows facing", ~529).
- Lock-on ("Lock-on", ~533-537; `FUN_140252a20`, `FUN_1402531d0`, `FUN_140252f70`, `FUN_1402543e0`, `FUN_14025b5e0`, `FUN_14025b270`): target choice (alive, not self, not team-mate, not status 8; auto-pick first valid if none; second button mask 0x400 cycles; lock button 0x800 toggle-or-hold by option `0x14052dfb7`); distance band 1/2/3 at 80/300 u; "while locked, the camera yaw turns toward the target and the pitch toward the target's chest, each limited to a per-player rate, and the pitch is clamped; rates/limits come from `setup/configdata.dat` (6 blocks of 13 floats, block 0 = 6, 3, 20, 10, 13, 55, 13, 55, 70, -70, -9, -9, 1; which float is which unknown; first two read as 6 deg/tick yaw and 3 deg/tick pitch)".
- Movement while locked: stick more than 45 deg from "the camera's forward" strafes; "standing still, the facing turns toward the camera yaw (the target) at the turn rate" (~538).
- Open item (PROGRAM-RE.md ~648): "Camera settings in `setup/configdata.dat`: which of the 13 floats is which." Status line ~493: "Left: ... the camera settings file."
- GAME.md quotes the original's tutorial text: "You cannot move the camera while you are locked onto an enemy" (code comment `sim.rs:592`); right stick is the camera control (GAME.md:156).
- FOOTAGE.md Round 2 "Lock-On" (~149-153): camera behaviour on lock **cannot be told** from the clip; framing looked static in a 2.2 s idle clip; no measurement. So there is **no footage evidence** for any camera rate, distance, height, free-camera follow or occlusion behaviour.
- VERIFY.md:11: camera changes need "two frames + Laurence plays (flicker does not show in stills)". Not a program fact; a method.
- Not read anywhere in the docs: free-camera behaviour of the program (follow, distance, height, FOV), split-screen framing, collision avoidance, recentre, free-camera pitch input, what happens to the camera on unlock.

## 2. Rule table

| # | Rule | Program read | Our code | Verdict | Check on PC |
|---|---|---|---|---|---|
| 1 | Free camera: only the right stick turns it, not while locked | Tutorial text quoted in GAME.md:156 / `sim.rs:592` ("You cannot move the camera while locked"); mechanism in program not read | `sim.rs:596-599` (`!p.locked && input.cam != 0`) | MATCH (to the tutorial text only) | Which function writes `+0x12e5c` when unlocked, and does any stick other than the right one write it? |
| 2 | Free camera swings behind a running player | Not stated by any doc (GAME.md:80 says ours does; stale) | None. `sim.rs:592-601`, test `sim.rs:765`; GAME.md:156 says never | ADDED BY US (absence chosen by Laurence 2026-09-27) | Does the program's free camera auto-follow the heading (a follow rate may be one of the 13 `configdata` floats)? Find writers of `+0x12e5c` other than the lock-on function and the right-stick reader. |
| 3 | Right-stick camera turn speed | "per-player rate" only for lock-on; free rate unread | `CAM_TURN_SPEED = 2.6` rad/s (`sim.rs:74`, about 149 deg/s), `sim.rs:598` | ADDED BY US (tuned by eye, no source) | Which function reads the right stick and the step it adds to `+0x12e5c` per tick; is it a `configdata` float (candidates: 6, 10, 13, 20)? |
| 4 | Right-stick dead zone / response curve | Input read `FUN_140256580`/`FUN_140256600`: dead zone 13,000/32,767, rescale (applies to which sticks is not stated) | `main.rs:1066-1069`: hard threshold 0.2, then raw `r.x` (no rescale) | DIFFERS (if the program applies its 40 % dead zone to the right stick) | Does `FUN_140256600` process the right stick the same way; is the camera turn proportional or fixed-rate? |
| 5 | Recentre (right stick click) | Not read | `sim.rs:599-600`: `cam_yaw = p.yaw` instantly | ADDED BY US (guessed) | Is there any recentre / "look behind" binding (button mask) in the pad reader `FUN_140256e80` (mask list)? |
| 6 | Lock-on: camera yaw turns to the target, rate limited | Yes, rate-limited per player, rate from `configdata` (first float read as 6 deg/tick) | Sim: instant `cam_yaw = yaw_of(toward(p.pos, opponent))` every tick (`sim.rs:425-428`). Render: eye smoothing `k = 1 - exp(-8 dt)` (`main.rs:1491`) | DIFFERS (program limits yaw rate; ours is instant in sim, exponential in render) | In `FUN_1402543e0` / `FUN_140252f70` find the per-tick yaw step and its source index in the `configdata` block; does it clamp the step or ease? |
| 7 | Lock-on: pitch turns toward target chest | Yes, limited per player, clamped; 3 deg/tick read | `main.rs:1433-1448`: aim `LOCK_AIM_HEIGHT = 12` above foe origin, exponential ease `1 - exp(-6 dt)`, clamp `[pitch_min, max_down]` | DIFFERS (rate model) / UNKNOWN (what "chest" height is) | Which float is the pitch step; what height offset is added to the target position (look for a constant near the target position read in `FUN_14025b270`/`FUN_1402543e0`)? |
| 8 | Pitch limits, locked | "the pitch is clamped" (limits unread; `configdata` has 70, -70 in block 0) | Locked: `clamp(-1.15, max_down)`, max_down eases 0.12 to 0.85 rad with distance (`main.rs:1382-1447`) = -66 to +49 deg (sim-free, render only) | DIFFERS / UNKNOWN | Are the limits the 70 / -70 floats (degrees)? Is sign = up or down? Are they per-distance? |
| 9 | Pitch limits, free (right stick Y look up/down) | Free-camera pitch unread. `+0x12e60/64` exist on the fighter | `CAM_PITCH_SPEED = 1.6` rad/s, clamp -1.15..+0.85 (`main.rs:1374-1383`), eye orbit clamped -0.6..+0.85 (`main.rs:1399-1400`), eases to level on lock | ADDED BY US (limits picked from an A-F sheet by Laurence, not from the program) | Does the program let the right stick change pitch at all when unlocked? If yes, what writes `+0x12e60`? |
| 10 | Point-blank lock-on pitch cap | Not read | `LOCK_PITCH_DOWN_NEAR = 0.12`, `LOCK_AIM_MIN_DIST = 25` (`main.rs:1376-1391`) | ADDED BY US (fix for a visual complaint) | Does the original have any distance term; does yaw/pitch freeze when the target is within some radius (possibly the 13/13 or 55/55 floats)? |
| 11 | Camera yaw feeds movement (stick relative to camera) | Camera yaw +0x12e5c; facing target = camera yaw + stick angle; turn 20 deg/tick; speed x max(0,1-angle/90) | `sim.rs:424-451`: `fwd` from `cam_yaw` (free) or toward opponent (locked); `stick_world = right*x + fwd*y`; `target_yaw = yaw_of(stick_world)` (free) | MATCH (rule), with the caveat of rows 6 (instant yaw) and 12 | Confirm the stick angle is added to the **camera** yaw (+0x12e5c) not to the facing (+0x12e68) in `FUN_14025ad50` / `FUN_14024f8f0`. |
| 12 | Locked: movement basis = direction to target | "Forward = camera's forward" while locked; camera yaw itself follows the target at a limited rate | `sim.rs:425-430`: basis is the exact opponent direction this tick | DIFFERS (program basis is camera yaw, which lags; ours is exact) | Is the strafe split computed against `+0x12e5c` (lagging camera) or directly against the target vector? Read the operands in `FUN_14025b5e0`. |
| 13 | Camera-relative stick mapping, handedness | Signed camera-relative angle in 32,768-per-turn units; 0 right, 1 left, 2 back-right, 3 back-left (`FUN_14025b5e0`) | `sim.rs:446-448` `right = (-fwd.z, 0, fwd.x)`; `on_right` via dot with right | MATCH (assumed), convention check in section 4 | Confirm the program angle sign (increases clockwise seen from above?) so "right" is the same side. Footage 2026-09-27 (strafe R clip names) already agrees for R/L. |
| 14 | Unlock: what `cam_yaw` does when unlocking | Not read | `cam_yaw` keeps the last locked value (toward the foe) (`sim.rs:427`, then only free branch edits it) | UNKNOWN | What does the program set `+0x12e5c` to when the lock releases (kept, reset to facing, or eased)? |
| 15 | Respawn: camera yaw | Not read | `Player::new(spawn, yaw)` sets `cam_yaw = yaw` (`sim.rs:250`), `locked` kept (`sim.rs:585`) | ADDED BY US | Where does the program initialise `+0x12e5c` on spawn / round restart? |
| 16 | Camera position (boom length, height, look-ahead) | Not read | 60 behind, eye 30-16 up from head = 14 above head+16; look 30 ahead (`main.rs:1465-1466,1488`) | ADDED BY US (by eye) | The camera distance/height are most likely among the `configdata` floats 13, 55, 70, 20, 10 (see section 3) or the CAMERA joint in the character (PROGRAM-RE.md ~620 names a `CAMERA` joint) - read how the view matrix is built (`0x1405a5ca0` is the view matrix). |
| 17 | Free camera orbit around the **player** | Not read | `head = pos + 16` orbit (`main.rs:1421`) | ADDED BY US | Where is the camera pivot (head joint, fighter origin + offset)? |
| 18 | Collision avoidance of scenery | Not read in docs at all | Raycast head to ideal eye (`col.0.raycast`), pull in instantly to `hit - 3` (min 4), ease out `1 - exp(-2 dt)` (`main.rs:1467-1480`) | ADDED BY US (GAME.md: "Occlusion... prevents flicker") | Does the original run any camera collision (look for a call to the stage collision query in the camera update), or does it clip through walls / fade near geometry? Note `FUN_140196c40` "camera-near fade" exists for capsules only (CAPSULE-VISUALS). |
| 19 | Split-screen layout | Not read (original versus screen layout not in docs) | Side-by-side halves or top/bottom halves (settings), `layout_viewports` `main.rs:1499-1521` | UNKNOWN | Which viewport rectangles does the original use for 2P (probably fixed; check the viewport set call near the render loop), and does it have both layouts? |
| 20 | Top/bottom FOV | Not read | `TB_MIN_VFOV_SHARE = 0.45`, `TB_CAM_HEIGHT_DROP = 14`, `TB_LOOK_DROP = 6` (`main.rs:39-42, 1404`) | ADDED BY US (A-F sheets, Laurence picked) | Program FOV value(s); is there a single fixed FOV constant for the projection matrix? |
| 21 | FOV 40-110, default 67 (settings) | Not read | `menu::Settings.fov_deg`, `apply_fov` (`main.rs:1527`) | ADDED BY US | Projection setup call; aspect handling for split views. |
| 22 | Camera framing of *both* players (shared frame) | Not read; original is per-player cameras (presumed; docs state "camera yaw" per fighter) | One `Camera3d` per player, each from own player's pose (`main.rs:823`) | MATCH to the fighter-object layout (camera yaw stored per fighter); otherwise UNKNOWN | Are there two views in the original or a single shared camera for 2P? |
| 23 | Lock-on target switch (left trigger / mask 0x400) | Yes: cycles to next target | Not built: one opponent only (GAME.md:156) | DIFFERS (needed for 2v2) | `FUN_140252a20` cycle order (nearest? next slot?). |
| 24 | Lock toggle vs hold | Option at `0x14052dfb7` toggles between toggle and hold | Toggle only (`sim.rs:420-423`) | DIFFERS | Default value of the option byte at `0x14052dfb7`; does `+0x13f24` (active flag) clear on target death? |
| 25 | Auto-lock when no target | First valid target is auto-picked (option `0x14052dfb8`) | Lock toggles on regardless (only one foe); does not require a valid target, works on invisible foe? `sim.rs:323` excludes Optical Camo | MATCH (partly) | Default of `0x14052dfb8`. |
| 26 | Distance band 1/2/3 effect on camera | Bands stored at lock struct +0xc (`0x1402f8040`, `0x1402f8138`); used for dash; camera use unread | Not used by camera | UNKNOWN | Does the camera pitch/yaw rate or framing depend on band? (Floats 13/55 twice may be per-band.) |

## 3. `setup/configdata.dat`: the 13 floats

Stated in PROGRAM-RE.md: 6 blocks of 13 floats; block 0 = `6, 3, 20, 10, 13, 55, 13, 55, 70, -70, -9, -9, 1`. Only blocks-of-13 and the first block are quoted; **the other 5 blocks are not in the docs** (what distinguishes the blocks is unknown: players / characters / camera modes / distance bands / stages). The table lists possibilities only; none is confirmed.

| Index | Value | Doc reading | Other candidates (guesses) | How to identify |
|---|---|---|---|---|
| 0 | 6 | "6 deg/tick yaw" (from PROGRAM-RE) | yaw step locked; free right-stick turn rate | Find the reader that takes this index in `FUN_1402543e0` / `FUN_140252f70`; see what it adds to `+0x12e5c` |
| 1 | 3 | "3 deg/tick pitch" | pitch step locked or free | Same; writes to `+0x12e60` |
| 2 | 20 | none | camera distance/step, ease divisor, pitch-up step, dead-zone gap | Look at where the 13-float block base is indexed; log which offsets are read in camera-update per tick |
| 3 | 10 | none | height offset, second rate, ease divisor | as above |
| 4 | 13 | none | min/near distance, lower camera height; pair with 6 | 4 and 6 equal (13, 13): maybe a min/max pair or per-side; 5 and 7 equal (55, 55) |
| 5 | 55 | none | max/far distance (55 near our 60 boom!), FOV-ish angle, yaw-limit | Compare with the program's actual eye-to-head distance in a live capture (Ghidra + debugger or the view matrix `0x1405a5ca0`) |
| 6 | 13 | none | see index 4 | |
| 7 | 55 | none | see index 5 | |
| 8 | 70 | none | max pitch deg (up) | Pair with index 9 (-70): symmetric clamp; check which sign means up |
| 9 | -70 | none | min pitch deg | |
| 10 | -9 | none | pitch offset / look-at lower bias; pitch trim by distance | Pair with 11 (-9) |
| 11 | -9 | none | see 10 | |
| 12 | 1 | none | flag, enable, smoothing factor, scale | Check for integer compare vs float use |

Notes: our boom is 60 and our pitch range is -66..+49 deg; 55 and 70/-70 are close enough to be suspicious but **this is not evidence**. How to settle it without guessing: (a) in Ghidra, locate the loader of `setup/configdata.dat` (search strings "configdata"), find where the floats land in memory and all read-xrefs with their indices; (b) open all 6 blocks from the PC (`pd_convert` or a hex dump; the game data stays local) and list the values per block; if blocks differ in only a few indices, those indices are character- or mode-specific; (c) live capture: read the game's camera eye-to-head distance and angle on Laurence's PC and compare with index values; (d) change a value and run the original (only if allowed, game data is read-only for us).

## 4. Conventions to verify (cheap, high value)

- **Handedness and angle units**: program angle is in 32,768 per turn units (half-turn = 32768?) per PROGRAM-RE (~537: "32,768-per-turn"; as written the wording is ambiguous; confirm whether 32,768 is a full turn or a half turn). Ours uses radians in the sim, `yaw_of`.
- **Camera yaw versus facing**: program facing target = camera yaw + stick angle. Ours does exactly this in free mode. In lock mode the program's camera yaw lags the target; our `fwd` does not (row 12).
- **Camera yaw unit of the `+0x12e5c` field**: radians or the 32,768 scale? Not stated; matters for any test that copies the program's values.

## 5. Numbers tuned by eye or guessed (camera code)

| Constant | Value | Where | Basis |
|---|---|---|---|
| `CAM_TURN_SPEED` | 2.6 rad/s | `sim.rs:74` | none (sim state! affects movement basis) |
| `CAM_PITCH_SPEED` | 1.6 rad/s | `main.rs:1374` | none |
| `CAM_PITCH_MIN/MAX` | -1.15 / +0.85 rad | `main.rs:1382-1383` | Laurence pick from A-F sheet; max from older "proven-safe" value |
| `FREE_EYE_PITCH_MIN/MAX` | -0.6 / +0.85 | `main.rs:1399-1400` | empirical (camera went under floor at lower values) |
| Boom, height, look-ahead | 60, 30, 30 (head = +16, lookat +14) | `main.rs:1465-1488` (`PD_CAM_DIST/UP/AHEAD`) | by eye |
| `LOCK_AIM_HEIGHT` | 12 | `main.rs:1376` | "their chest", guessed |
| `LOCK_AIM_MIN_DIST` / `LOCK_PITCH_DOWN_NEAR` | 25 / 0.12 rad | `main.rs:1378,1391` | complaint-driven fix |
| Camera smoothing | `1-exp(-8dt)` position and rotation, `1-exp(-6dt)` pitch | `main.rs:1448, 1491` | by eye; frame-rate dependent only via dt (render side) |
| Occlusion | margin 3, min 4, ease out rate 2/s | `main.rs:1474-1479` | by eye |
| Stick look threshold | 0.2 | `main.rs:1067-1072` | guess |
| `TB_MIN_VFOV_SHARE`, `TB_CAM_HEIGHT_DROP`, `TB_LOOK_DROP` | 0.45, 14, 6 | `main.rs:39,42,1404` | A-F sheets |
| Default FOV | 67 deg (settings) | `menu` / `apply_fov` | unknown origin |
| `LOCK_STRAFE_ON/OFF_COS` | cos 50 / cos 40 | `sim.rs:42-43` | ours (see MOVEMENT-AUDIT D) |

Determinism note: `CAM_TURN_SPEED` and the locked yaw are sim state (deterministic, correct for online); pitch, boom, smoothing are render-only (correct per AGENTS.md rule 3). If the program's yaw rate limit is adopted it belongs in `sim.rs` because it feeds the movement basis.

## 6. Doc-vs-code contradictions

1. **GAME.md:80** "cam_yaw swings behind a running player at FREE_CAM_FOLLOW (2.5/s)" vs code/tests/GAME.md:156: no follow. Code is right. Fix wording in GAME.md (not done: read-only task).
2. **GAME.md:80** lock button "any gamepad Y/North" vs GAME.md:156 and `main.rs:1062` (`RightTrigger2`). North button is a skill slot in code (`main.rs:1074`). Doc:80 is stale.
3. **GAME.md:156 "Not built: ... camera up/down"** vs GAME.md:98 and `main.rs:1374` (look up/down is built). Stale sentence.
4. **GAME.md:156 "Q/E turn the free camera"**: code `main.rs:1036` has `E - Q` (E = right). Consistent.
5. **GAME.md:92 (Cameras)** "camera sits 60 behind and 30 above": code puts the eye at head + (30-16) = 14 above the head (head is +16 above feet), i.e. 30 above the feet. Consistent only if "player" = feet; confirm wording. In top/bottom the drop is 14.
6. **PROGRAM-RE.md:537 vs our lock-on**: program rate-limits the camera (6 deg/tick yaw read); our sim snaps; GAME.md "Cameras" says nothing about the rate, so the difference is undocumented.
7. **MOVEMENT-AUDIT F** says "free-camera behaviour... is the configdata block": there is no documented link between the float block and free-camera follow; PROGRAM-RE.md ties the floats only to lock-on. Unverified assumption.
8. **PROGRAM-RE.md ~493 status line** ("Left: ... the camera settings file") vs AGENTS.md "lock-on cameras that avoid scenery" listed as done: our camera was built before the program read and never reconciled.
9. **FOOTAGE.md** found no camera evidence; GAME.md cites none for any camera number, only Laurence's preferences.

## 7. Check-on-PC shopping list (in priority order)

1. **`configdata.dat` loader and reads**: xrefs of the block base, per-index readers (camera update = probably inside `FUN_1402543e0` / `FUN_140252f70`), also the 5 other blocks and what selects a block.
2. **Who writes `+0x12e5c` / `+0x12e60` / `+0x12e64`** (all writers): lock-on function, right-stick reader, spawn init, unlock. This answers rows 1-3, 5, 14, 15 in one pass.
3. **`FUN_14025b5e0` operands**: is the strafe angle relative to camera yaw (lagging) or to the target vector (rows 12-13).
4. **Right stick path** (`FUN_140256580` / `FUN_140256600` / `FUN_140256e80`): dead zone, rescale, turn proportional or fixed (rows 4, 5, 9).
5. **View matrix build** (`0x1405a5ca0` writers): eye position, pivot, distance/height, FOV, near/far (rows 16-17, 20-21).
6. **Camera/stage collision**: any stage-collision call from the camera code (row 18).
7. **Viewport setup for 2 players** (row 19, 22).
8. **Lock cycling and auto-pick** defaults (`0x14052dfb7/8`, rows 23-25).
9. Footage that needs no Ghidra (same method as FOOTAGE.md): (a) locked on, circle the foe fast and film from the screen: how many degrees per second the view lags; (b) unlock after a hard circle: does the view stay, reset, or drift; (c) free camera running 5 s in a straight line then a U-turn: does the view ever move by itself; (d) walk the camera into a wall: clip, pull-in or fade; (e) 2P screen layout screenshot.

## 8. Suggested order

- Do **nothing** to `sim.rs` camera logic until items 1-3 of section 7 are read; changing now would be another guess (same rule as MOVEMENT-AUDIT).
- Cheap doc fixes now (separate session): GAME.md:80 (remove `FREE_CAM_FOLLOW` and the Y lock-on claim), GAME.md:156 ("camera up/down not built").
- Once the lock-on rate is known, implement it in `sim.rs` as a per-tick yaw step toward the target (deterministic), with a test copied from the program (for example: 180 deg target swing takes N ticks at the step read), then base the strafe basis on the lagging yaw.

## Summary (5 lines)
1. GAME.md:80's `FREE_CAM_FOLLOW` is stale: code and tests (`sim.rs:592-601,765`) and GAME.md:156 say running never turns the free camera (Laurence's choice, not a program fact).
2. Program rate-limits lock-on camera yaw/pitch from `configdata.dat`; ours snaps yaw in the sim and eases by eye in render, so the movement frame differs.
3. The 13 floats are unidentified: 6/3 read as yaw/pitch deg per tick; 70/-70 look like pitch limits; 13/55 pairs and -9/-9 unknown; five other blocks are undocumented.
4. Free-camera follow, boom distance, height, pitch limits, occlusion, recentre, split framing and FOV are all ours by eye (about a dozen constants listed), none read from the program.
5. Next: one Ghidra pass on all writers of `+0x12e5c/60/64` plus the `configdata` readers settles most rows.
