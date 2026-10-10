# pd_game — split-screen prototype

**Status (2026-10-07):** a one-to-four-player local versus runtime with CPU opponents,
online versus for two PCs (ONLINE-PLAN.md), modes, an Options overlay and the original's rules built from its program. Fidelity is
measured per skill against the original (scorecard and open items: `NEXT-SESSION.md`;
lane log: `WAVES.md`). Online play was merged on 2026-10-03 (lane online). The 2026-09-30
plan `CLAUDE-DEEP-PASSES.md` P0-P8 was executed as waves 1-6 and is history now.
Sections below are dated; where one contradicts a newer one, the newer wins.

Code: `crates/pd_game/src/sim.rs` (movement rules), `combat.rs` (combat rules), `main.rs` (Bevy glue), `fx.rs` (capsules, projectiles, barriers in the world), `hud.rs` (the original battle HUD, see HUD.md). Launcher: `PLAY.bat`.

## Live-play corrections (2026-09-29)

- Ordinary footsteps use **common/35**, identical to Laurence's
  `audition/27_id035.wav`. Historical id-30 emitter research is not the
  approved playback choice. Wet selection stays separate.
- Capsule view masks now live on **every draw mesh**, not only the root.
  Own/opponent styles no longer superimpose across both cameras; capture
  ownership is unchanged. See `handoffs/H5-orb-ownership.md`.
- Camera-proximity dematerialisation is **grain discard**, not transparent
  Blend: `cam_fade.rs` + `dissolve.wgsl` keep normal PBR lighting/cutouts and
  depth writes. Coverage is per viewport and restores exactly to 1.0 after
  retreat, including a second approach/recovery. Defaults (9→4 units,
  minimum coverage 0.15, 1px grain) are **approved by Laurence, complete**
  (2026-09-29 sign-off).
  `PD_CAMFADE=0` restores original material handles; `PD_FADE_LOG=1` logs
  per-view coverage. `PD_DISSOLVE_GRAIN` sets grain size. Capture-only
  `PD_TEST_STICK` now accepts semicolon-separated motion windows to test
  wall approach, retreat and repeat through the normal camera raycast.

## Architecture

```
input devices ──gather_input (Update)──► PendingInputs [Input; 4]
                                              │
                         FixedUpdate 60 Hz    ▼
                         step_sim ─► Match::step(inputs, &Collision)   ← deterministic
                                              │ (state only)
            Update ◄──────────────────────────┘
  sync_visuals (interpolated) · follow_cameras · drive_animation · layout_viewports
```

### Determinism contract (sim.rs)

- Input is quantised: `Input { stick: [i8; 2], jump: bool, lock: bool, skills: u8 }` (skills = bit per slot). This is exactly what will go over the network.
- `Match::step` depends only on its state, the inputs, and the collision data. No clocks, randomness, Bevy types, floats from rendering, or iteration over hash maps in the sim.
- `prev_pos` / `prev_yaw` are recorded at the start of each tick so rendering can interpolate (`render_pose`, using `Time<Fixed>::overstep_fraction`). Rendering must never write sim state.
- ⚠️ Floats are deterministic only for the same binary on the same CPU architecture. Cross-platform lockstep or rollback may need fixed-point later; decide when building netcode.

### Tuning constants (sim.rs)

Ground speeds, turning, gravity and landing are the original program's own
numbers, read from `PDUWP.exe` and checked against Laurence's footage
(`engine/docs/PROGRAM-RE.md` "Ground movement" / "Jumps" / "Landing by fall
height"; moves lane M10, 2026-09-27) -- replacing the earlier hand-tuned
guesses in this table.

| Constant | Value | Meaning |
|---|---|---|
| `RUN_SPEED` | 55.6 u/s | run, fixed (not analog) |
| `WALK_SPEED` | 20.8 u/s | walk, fixed |
| `STICK_AXIS_DEAD_ZONE` / `STICK_DEAD_ZONE` / `RUN_GAIT_ON` | 13,000/32,767 per axis / 0.4 / 0.7 | the program's input model (MOVEMENT-AUDIT.md steps 2-3): per-axis square dead zone + rescale, uncapped magnitude; run above processed 0.7, walk below 0.4, previous gait kept between. Replaced the earlier raw 0.6 feel threshold on Laurence's request to match the original (2026-09-30) |
| `TURN_RATE_DEG_PER_S` | 600°/s | facing turn rate (20°/program tick) |
| `GRAVITY` | 108 u/s² | ordinary fall (walked off a ledge, etc.) |
| `FALL_SPEED_CAP` | 225 u/s | |
| `JUMP_VELOCITY` / `DODGE_JUMP_GRAVITY` | 85 u/s / 259.2 u/s² | the Jump dodge's (hidden skill 750) own numbers -- apex 14 u, ~0.66 s air; kept off the shared `GRAVITY` above so lowering it doesn't make the dodge floaty (M11 still owns rebuilding the dodge itself) |
| `AIR_CONTROL` | 0.6 | |
| `STEP_HEIGHT` | 6 | auto step-up/down |
| `COYOTE_TICKS` | 6 | grace ticks over floor seams (fixed run-animation flicker) |
| `LAND_FALL_MIN` / `LAND_FALL_HEAVY` | 40 u / 80 u | landing pick by fall height (below "Landings") |
| `Player::body` (`body.rs`) | radius 5 (charaset `+0x0E` / 10 / 2), height 18-20 per character | walls and player-vs-player (fighters stop 10.0 apart); READ `FUN_140250780`, `FUN_140255020` (PROGRAM-RE.md "Body size"). `BODY_HEIGHT` 20 remains only for `props::nearest_throwable` (UNCONFIRMED) |
| `FALL_RESPAWN_BELOW` | 150 | respawn after falling this far below spawn |

### Movement model

Walk or run, nothing in between (stick magnitude only picks the gait, with
hysteresis): the player always moves along its own facing, which turns
toward the stick direction at the turn rate; while turning, speed is cut by
how much angle is left (a big reversal costs a few ticks standing still
before it moves). Lock-on like the original: within 45° of the opponent is
the ordinary run (facing turns to face them); further off is a strafe --
body kept turned on the opponent, feet moving sideways/back at full speed
(no turn slowdown), playing the TURN_WALK_*/TURN_RUN_* clips
(`sim::Motion::Strafe`, `main.rs::strafe_clip`). Idle and locked: facing
turns to face the opponent at the same turn rate. Facing follows the stick/
target (not velocity — velocity jitters against walls).

**Forward/strafe hysteresis** (gait lane M10b, 2026-09-27): the 45° split
above has a ±5° band (`LOCK_STRAFE_ON_COS`/`LOCK_STRAFE_OFF_COS`, `Player::strafing`),
the same idea as the walk/run split's own hysteresis. Without it, Laurence
hit a real bug: locked on, a full stick push settled hard on one side of the
boundary and stuck there (looked like "only walking" even at full push,
because it had locked onto a strafe clip and stayed there), while a light
push — much less mechanically steady near partial deflection than a push
held against the gate — kept crossing the boundary from ordinary hand
tremor, flipping `Motion` between `Run` and `Strafe` every tick and so
alternating between the run clip and a `TURN_RUN_*` "quick step" clip.
Reproduced deterministically in `sim.rs` tests (a steady 43°/47° wobble,
comfortably either side of 45°, used to flip on every tick) rather than
needing a real pad. Test switches: `PD_TEST_STICK=<mag>,<angle_deg>[,<start_tick>]`
holds P1's stick at that magnitude/angle (0° = forward, camera- or
opponent-relative same as any push) from the given tick; `PD_MOVE_LOG=1`
prints P1's gait/motion/clip every tick. Proof `shots/gait_walk_run_sheet.png`
(locked full/light, free full/light).

**Strafe left/right hysteresis** (2026-09-29): the strafe's own left/right
split had no band -- it compared dead-centre every tick -- so a thumb
pulled straight back sat exactly on the BackRight/BackLeft boundary and
hand tremor flipped it every few ticks. Each flip replays the TURN_*
pivot in the renderer, so a settled backward run flicked back around and
back to normal rapidly (Laurence). Same +/-5 deg cure as the forward/strafe
split (`sim.rs::LOCK_STRAFE_SIDE_BAND`, persisted `Player::strafe_right`):
only a decisive crossing flips the side. Deterministic (kept state, not a
random pick), so the sim stays netcode-clean. Reproduced in the
`sim.rs` test `locked_strafe_side_split_has_hysteresis_against_stick_jitter`
(a steady 177°/183° wobble must never flip sides; 150°/210° still switch
decisively). Known adjacent knife-edge, not yet fixed: the back-cone
boundary itself (Right/BackRight at the diagonals) -- flagged before
changing behaviour beyond what was reported.

**Strafe back-cone hysteresis** (2026-09-29, same day): the report above
kept the words "firm diagonal, both sides" -- and the diagonal boundary
was the same disease, worse. A firm 135°/225° hold sits *exactly* on the
back-cone line (the quantised diagonal normalises to f32 -0.70710683, one
ulp past -√2/2), so per-tick rounding dust from the rotating foe direction
flipped Right/BackRight nearly every tick: a scripted static 135° run
alternated every 1-4 ticks for 80+ ticks, replaying the TURN_* pivot each
time with ~0.6 rad facing jumps. Same cure, mirrored onto the cone
(persisted `Player::strafe_back`: become back past 140°, revert within
130° -- reuses `LOCK_STRAFE_OFF_COS`/`LOCK_STRAFE_ON_COS`, no new
constants), deterministic like the rest. Regression test
`locked_strafe_back_cone_boundary_has_hysteresis_against_float_dust`
(steady 135°/225° against a far foe must never flip; verified it still
flaps with the boundary-less comparison).

**Lock-on toggle** (P1 keyboard `Y`, any gamepad `Y`/North; P2 keyboard `Right Shift` is a test fallback only; real play is one controller per player). Unlocked: movement is relative to the player's own camera direction `cam_yaw`, which swings behind a running player at `FREE_CAM_FOLLOW` (2.5/s). `locked` and `cam_yaw` are sim state (free movement depends on them) and survive respawn.

### Motion states → clips

`Idle→WAIT0, Walk→WALK, Run→RUN, Rise→JUMP_UP, Fall→JUMP_DOWN, Land→JUMP_LAND1, HeavyLand→JUMP_LAND0`, 120 ms crossfades via `AnimationTransitions`. Clips are looked up by name in each character's glTF (`named_animations`). Each character's `AnimationPlayer` is linked to its player slot by walking `ChildOf` parents to the `PlayerVisual` entity. JUMP_UP/JUMP_DOWN are time-stretched to the jump's real airtime (`Player::rise_seconds`, `main.rs::drive_animation`) -- PROGRAM-RE.md "Jumps".

> **2026-09-30, branch `lane/locked-on`:** the render-layer strafe work described below (baked-yaw cancellation, turn delay, visual turn rate, hand-off fade, return pacing) is removed there: the model faces the sim facing and the clips play as authored, as in the program (MOVEMENT-AUDIT.md "Locked-on lane"). The paragraphs below are history.
>
> **2026-09-30 update:** the locked-on strafe logic described in the paragraphs below (per-tick classification, +/-5 deg hysteresis bands, sim facing kept on the target) was replaced by the program's state machine; see DECISIONS.md 2026-09-30 and MOVEMENT-AUDIT.md steps 3b/3c. The render-layer notes (pivot yaw, turn delay, baked-yaw cancellation) still describe `main.rs` but were tuned to the old model.

**Strafe(dir) is sequenced, not a single clip** (gait lane M10c, 2026-09-27; PROGRAM-RE.md "Lock-on"): the program never loops a TURN_* clip -- it plays once as a pivot, then falls through to the ordinary WALK/RUN clip (looping) for as long as the same direction and gait hold (no dedicated strafe-loop clip exists in the data). `main.rs::strafe_sequence` (pure, unit-tested) is the state machine: `AnimSlot::strafe` remembers the key whose pivot may still be playing; entering a strafe plays that direction's TURN_WALK_*/TURN_RUN_* pivot (`strafe_clip`, by gait and direction) via `drive_animation`, checked against the actual `AnimationPlayer` via `is_finished()`. A direction or gait change mid-strafe does NOT replay the pivot: the playing pivot plays out and the gait loop takes over at its end (PROGRAM-RE.md: the per-tick pivot function never restarts the TURN clip; the facing offset was chosen when the strafe started). Replaying on every crossing is what made a trembling thumb spasm (2026-09-29): each replay restarted the sidestep, the hold and the turn. The facing side (`sync_visuals`) keys its pivot clock and clip to the strafe-entry direction for the same reason, while the intended facing itself is carried (`PivotFacing::facing`) and retargets smoothly to the live stick direction -- a mid-pivot flip turns from where the body is instead of jumping between mirror-image pursuits. Fixes the reported looping TURN_* clip the same as before. Debug switch `PD_ANIM_LOG=1` (P1's real clip changes, from `drive_animation`, distinct from `PD_MOVE_LOG`'s sim-side `clip_for` reading which doesn't see this sequencing). Proof `shots/strafe_sequence_sheet.png` (tick-exact via `PD_SHOT_TICKS`, full/half push, right/left).

**Continuous strafing faces the movement direction, not the target** (gait lane M10d, 2026-09-27; PROGRAM-RE.md "Lock-on"): once the TURN_* pivot hands off to the ordinary WALK/RUN clip, the program's own state machine no longer re-aims the body at the target -- it moves like ordinary ground movement, facing where it's going. Laurence noticed the earlier build kept the body facing the target throughout ("make the character face where the left analogue stick is pointing"). Cosmetic only: the *sim's* own facing (`Player::yaw`, used for casting/aim) is untouched, still toward the target while locked -- only the rendered `Transform` differs. `main.rs::sync_visuals` eases the rendered yaw toward `yaw_of(Player::vel)` (`visual_faces_movement` decides when: `Motion::Strafe` with real velocity) at the strafe's own visual rate (180 deg/s, `STRAFE_VISUAL_TURN_RATE_DEG_PER_S`; the sim's 600 deg/s turn rate is untouched) instead of the usual exponential smoothing, so the pivot's own turn hands off with nothing left to snap; stopping, casting, dashing in or moving forward all fall back to the sim's own target-facing yaw (with the pre-existing smoothing). Debug switch `PD_FACE_LOG=1` (P1's sim vs visual yaw and velocity, every tick); `PD_TEST_STICK` takes an optional end tick and an optional tremor wobble (`<mag>,<angle>[,<start>[,<end>[,<wobble_deg>[,<wobble_period>[,<mag_wobble>]]]]]`) to test releasing the stick or a shaking thumb. Proof `shots/strafe2_facing_sheet.png` + the `PD_FACE_LOG` trace (sim yaw stays put; visual yaw eases to the movement direction, settles, and returns to the sim yaw on stopping or casting).

**The TURN_* pivot itself needed the same treatment** (gait lane M10e, 2026-09-28): the fix above rotated the whole model toward the movement direction *during the pivot too*, but the TURN_* clips are not root-motion-free turns in place -- they **bake their own hip-joint yaw** (measured from `pc00a.glb`, node 22: e.g. TURN_WALK_R swings roughly -35 to -73 deg, TURN_WALK_BR roughly +4 to -160 deg -- `main.rs::turn_clip_keyframes`, ~8-point downsampled curves per clip). Rotating the parent *on top of* that double-counted the turn -- Laurence: "the character sidesteps facing away from them [the target]... spins the opposite direction when the sidestep animation is triggered, then flicks back around once the regular run initiates." PROGRAM-RE.md "Lock-on" was corrected accordingly: the program's own fixed per-direction offset exists to cancel this same baked rotation, tuned for its own clips. `main.rs::pivot_model_yaw` does the same by construction instead of guessing a fixed offset (exact for our clips regardless of their measured timing): model yaw = (a turn-rate-limited pursuit from the facing the model had when the pivot began, to the movement direction) minus (the clip's own baked yaw at that instant) -- so effective facing (model + clip) always equals that smooth pursuit curve, continuous at the pivot's start and landing exactly on the movement direction once the pursuit completes (well inside every clip's own length). The continuous WALK/RUN clip that follows also carries a small, roughly-constant baked yaw (`main.rs::gait_clip_yaw`: RUN ~20 deg, WALK ~-3 deg) which gets the same cancellation, so the pivot -> run hand-off (both under one held strafe) has no residual step either. Verified with `PD_FACE_LOG` extended to print the effective facing (visual yaw + the currently-relevant clip's own yaw, wrapped): turns from the pre-strafe facing to the stick direction through the pivot for every direction/gait, and the hand-off holds that value with no step. 3 new tests (`pivot_facing_starts_exactly_at_the_pre_strafe_facing`, `..._is_continuous_and_ends_on_the_movement_direction`, `..._handles_a_decisive_backward_direction_without_spinning`) check the continuity rule across all 8 (direction, gait) pairs. Proof `shots/strafe3_right_sheet.png` / `strafe3_backr_sheet.png` (every 3 ticks across the pivot -> run hand-off). 2026-09-29 follow-up -- Laurence: "the transition from strafe animation to running snaps." The hand-off re-based the model yaw by the whole (pivot-end minus gait-loop) baked difference, up to ~90 deg, on the switch frame, while the clip itself only cross-fades over the same 5 frames -- so the rendered sum visibly snapped even though the `PD_FACE_LOG` effective column (which assumed the new clip's baked yaw immediately) looked continuous. `main.rs::handoff_baked_yaw` now fades the correction from the pivot's end baked yaw to the gait loop's over exactly `CLIP_BLEND_SECONDS`, mirroring the clip switch's own cross-fade, so the rendered sum never steps (re-verified: max ~10 deg/tick through the whole transition, right/run; 2 new tests pin the fade and the switch-frame continuity for all 8 pairs; shown to Laurence as a before/after sheet at the time: the body twisted ~90 deg on the switch tick before, the back view held steady after). 2026-09-29 timing, same day -- Laurence: the turn lands close to a second too late. The pivot had pursued the sim (opponent-facing) yaw and only turned to the stick direction once the sidestep ended; it now pursues the stick direction from the pivot start, so the rotation happens *through* the sidestep (right/run: starts the tick the sidestep starts, complete ~9 ticks later, hand-off steady). Proof `shots/turn_sheet.png`. 2026-09-29 timing, round 2 -- Laurence: 0.4 s from first input before the turn (0.7 s first try, too early; then 0.4 s sooner -- corrected to exactly 0.4, not 0.3). `STRAFE_TURN_DELAY_SECONDS = 0.4`: the pivot holds the pre-strafe (opponent-facing) pose for 0.4 s, then turns to the stick direction (right/run: hold ticks 201-225, turn from 226 spanning the hand-off, steady after; shorter clips finish the turn just after hand-off since the post-pivot branch pursues the same target at the same rate). Proof `shots/turn_sheet.png` (re-captured for the delayed turn). 2026-09-29 footwork, same day -- Laurence's footage ("Run spasm.mp4") shows the turn itself whipping ~90 deg in ~0.15 s and reading as a flick: the sim's 600 deg/s rate is for aim, the rendered body gets its own `STRAFE_VISUAL_TURN_RATE_DEG_PER_S` (180 first -- sides perfect, but a full reversal took a whole second and read too slow; 300 now: sides in 0.3 s, reversals in 0.6 s), and the hand-off branch pursues on the rendered level so the slow turn stays monotonic while the model absorbs the clip fade underneath. 2026-09-29 release flick, same day -- stopping a backward run flickered 1-3 frames: the exponential smoothing's first step spanned ~54 deg back to the sim yaw. The strafe exit now paces the return at the same visual rate (`strafe_return_step`, until arrived; casts keep the fast path).

### Cameras

One `Camera3d` per player, left/right halves (`layout_viewports`, recomputed on resize). Camera sits 60 behind and 30 above the player on the line away from the opponent, looking past the player toward them. (Archivist 2026-10-07: the FOV half of the top/bottom text below, `TB_MIN_VFOV_SHARE` and `PD_TB_VSCALE`, was removed by camfov on 2026-10-03; the FOV and camera distance rules are in "FOV and camera distance settings" at the end of this file. `TB_LOOK_DROP` now applies in a strip only with `PD_CAM_OLD`, or while locked on with the default lock-on camera, i.e. not with `PD_CAM_LOCK_LIVE=1`; read from `follow_cameras` 2026-10-07.) **Top/bottom split:** keeping the side-by-side view width left a short strip only ~19 deg tall that cut off your own character, so the vertical view is kept at least 45% of the settings FOV (`TB_MIN_VFOV_SHARE`) and the camera aims 6 lower (`TB_LOOK_DROP`); Laurence picked variant A (closer) from an A-F sheet. The eye also sits 14u lower than the usual 30u (`TB_CAM_HEIGHT_DROP`, test override `PD_TB_CAM_DROP`) -- at 30u the short, wide strip still only showed the top half of your own character (Laurence 2026-09-27); A-F sheet `shots/cam_calib_tb_sheet.png`, recommendation D (14u) applied. Test overrides `PD_TB_VSCALE`, `PD_TB_LOOK_DROP`. Occlusion: raycast head→camera; pull in instantly when blocked, ease out slowly (prevents flicker at edges).

**Locked-on pitch:** the boom tilts to the foe's chest height so they stay centred (a foe above no longer goes off the top of the screen), but the raw chest-aim angle blows up as the horizontal gap shrinks -- at point-blank range even a small height difference wanted a steep downward angle, tipping the view toward the foe's torso (Laurence 2026-09-27). The allowed downward pitch now eases in from near-level (`LOCK_PITCH_DOWN_NEAR`) at point-blank to the full range by `LOCK_AIM_MIN_DIST` (25u), where `look` also switches back to full chest-aim. Proof `shots/cam_issue3_close_before_after.png`.

**Free-camera pitch:** right stick / keyboard R-F look up/down (`CamPitchInput`) is clamped `CAM_PITCH_MIN`..`CAM_PITCH_MAX` (-1.15..+0.85 rad: about 65 deg up, 50 deg down -- widened from 35 deg up so you can look up at sky/tall structures, Laurence 2026-09-27). Looking up further than ~35 deg used to swing the *camera's own position* under the player and into the floor or a low ceiling (it orbits the head the same way locked-on tracking does, which is fine for tracking a foe but not for free look) -- found via a `PD_CAM_DEBUG=1` trace showing `dir` flip to mostly -Y and the occlusion boom collapse to ~15u. Fixed by decoupling: the eye's own orbit is clamped to the old, proven-safe range (`FREE_EYE_PITCH_MIN/MAX`, -0.6..+0.85) so its position barely moves, while the look-at target still tilts the full range, so the view re-aims upward without relocating the camera. A-F sheet `shots/cam_calib_up_sheet.png`, recommendation D (-1.15 rad) applied and shipped. Test switches: `PD_FREE_CAM=1` (start unlocked), `PD_TEST_PITCH=1|-1` (hold look up/down), `PD_CAM_PITCH_MIN` (calibration override); proof `shots/cam_issue1_lookup_before_after.png`.

## Running

- Play: double-click `PLAY.bat`. With no arguments the game now opens on the **title menu** (VERSUS MODE / ONLINE / ARSENAL / OPTIONS MODE / EXIT GAME since 2026-10-03) instead of going straight into a match — see `docs/FRONTEND.md` for the menus, the versus setup screen, and the arsenal (deck) builder. Intended: one controller per player (left stick move, A jump, Y lock-on). Keyboard for testing: P1 WASD + Space + Y; P2 arrows + Enter + Right Shift.
- Args: `pd_game <cache dir> [--stage st06] [--p1 pc00a] [--p2 pc01a] [--demo] [--screenshot out.png --shot-frame N]`. Any of these pre-existing options (or `--loadout`, `--arsenal mixed`, `--open-menu`) skips the menus and goes straight into a match, exactly as before the front end existed — automated tests and screenshot scripts rely on this. Two more, hidden, switches exist only for the front end's own screenshots: `--open-screen <title|setup|arsenal|result>` opens directly on that screen, and `--menu-script <tokens>` (comma-separated `up`/`down`/`left`/`right`/`confirm`/`cancel`) clicks P1 through the menus by itself.
- `--demo` scripts both players (P1 strafes, P2 hops) for unattended screenshots; it exits 60 frames after the shot.

## Settings menu (`menu.rs`)

Esc or gamepad Start opens it (B/East also closes); the match pauses while open. Three tabs since 2026-10-08, switched with LB / RB (keyboard Q / E), Quit to title the last row of each in a match: **VIDEO SETTINGS** split screen (side by side / top and bottom), field of view (slider, 50-110, default 75), camera distance (slider, 70-160 %), camera height, display mode, resolution, aspect ratio, HUD size, lighting and look; **SOUND** master / music / effects volume (sliders 0-100, default 100 = the old loudness, `mixer.rs`); **CONTROLS** vibration (on / off, RUMBLE.md), invert camera Y, camera sensitivity (50-200 %, applied to the free camera's look input before it is quantised, `Settings::shape_look`; the turn never exceeds the sim's `CAM_TURN_SPEED`), button mapping (a page: dodge x2, four skills, lock on, switch target, per pad button and keyboard key, swap on conflict, reset; `bindings.rs`, read in `gather_input`). FRONTEND.md "Options tabs". Display rows are browsed with Left/Right and applied with A (no Apply row, no keep-these-settings prompt); the rest apply as you move. Everything: FRONTEND.md "Display options" and "Look options". Saved to `cache/settings.json`. The FOV value is the horizontal angle of a 16:9 view (json key `fov_h16`); each camera derives its own vertical angle from its viewport (`apply_fov`, "FOV and camera distance settings" below; the older "vertical angle of a side-by-side half" meaning was replaced 2026-10-03). UI draws on an overlay `Camera2d` (order 10, no clear). Test flag: `--open-menu`.

## Cloth (`dangle.rs`)

Extra-part joints on chains 3+ deep are exported named `c<K>j<N>` and swing as damped pendulums after transform propagation (cosmetic, not sim state).

- **Body collision:** the converter writes a capsule per skin-owning body bone as node extras (`capsule_child`, `capsule_radius` = median skin distance). `dangle.rs` pushes cloth tips and segment midpoints out of them (`SKIN_MARGIN` 0.6), and never lets a piece go deeper into a shape than the point it hangs from (+ `ATTACH_CLEARANCE`). Earlier versions either pushed pieces attached inside a shape out sideways ("spikes") or ignored those shapes (P1's back poked through his coat).
- **Long coats (generated rig, `pd_formats/src/skirt.rs`): PARKED** — no character rigged; see HANDOFF.md "Parked: long-coat cloth" (coat found by texture area; hybrid approach chosen for later).
- The viewer shares `dangle.rs` (`--no-cloth` to disable) for close-up checks. Tuning: `HANG` 0.55, `STIFFNESS` 6/s, `DAMPING` 0.7, `MAX_STEP` 0.25, `CARRY` 0.95 (share of body movement passed straight to the cloth — without it running tails trailed at a fixed angle and never swayed), `ATTACH_CLEARANCE` 0.4 (0 for the coat rig). History: first values flapped erratically → heavier; then tails trailed stiffly → CARRY; then P1's back poked through → depth rule below.

## Landings

By fall height (highest point reached minus the landing point), not airborne ticks (PROGRAM-RE.md "Landing by fall height"): below 40 u, no landing clip, straight back to idle/run/strafe; 40–80 u, light landing `JUMP_LAND1` (0.4 s); above 80 u, heavy landing `JUMP_LAND0` (0.967 s) plus a dust effect (not built here -- fx lane). Either landing now locks the player for its whole length (replaces the old "moving cancels a light landing" rule).

Known gap, left alone (M11 owns the dodge): the Jump dodge's own touch-down (apex ~14 u, under the 40 u floor) currently falls under this same fall-height rule and general Rise/Fall/Land state machine, so it shows a light `JUMP_LAND1` landing pose (or none) rather than the original's own `DASHEND_B` clip that the dodge always plays regardless of height (PROGRAM-RE.md "Dodge: Jump (750)"). Pre-existing (the dodge's jump never used a distinct clip path before this lane either); not fixed here since dodges are M11's.

## Combat (moves M1, `combat.rs` + `hud.rs`)

`combat.rs` is part of the deterministic sim (same contract as `sim.rs`: integer counters, tick timers, a seeded xorshift in the state). `Match::step` runs movement (movement input is ignored while a fighter is casting, dashing, stunned or down), then `Combat::step`, then overrides `Player::motion` with Cast / Lunge / Hit / Down. `hud.rs` only reads state.

**Skill numbers** come from `cache/skills.json` (loaded in `main.rs::load_skills`): capsule type, family (s42), cost, strength + strength rule, requirement, level/health penalties, DEF (s47), plus the whole raw record. **What each skill does once cast is data-driven** in `pd_game/src/moves/` (one module per family, registry in `moves/mod.rs`); per-family rules and sources: **docs/MOVES.md**. Aura Particle is built in (id 0).

**Cast timing** (moves M3, 2026-09-27): a cast fires at its own action-table row's fire tick and holds the caster for the row's own clip length (`main.rs::load_action_rows`/`load_clip_ticks`, `Combat::action_row`), replacing the old flat `CAST_TICKS` guess; a hit before the fire tick still cancels the pending shot. Short-range skills (sword/punch/kick) dash the row's own dash window, only when locked on and the target is within 80 u at the press, and strike anywhere in the row's hit window whether or not the dash arrived — see **docs/MOVES.md** "Cast timing" for the details and docs/PROGRAM-RE.md for where the numbers come from.

| Rule | Value | Source |
|---|---|---|
| Health | 20 at round start | Laurence |
| Level | starts 1, cap 16; aura particle +1 | Laurence |
| Aura | refills 1 point / s up to Level; skills cost aura | refill rate is a **placeholder** |
| Arsenal | 30 skills, shuffled each round; same test set for both: 10 aura particles, 8 Psycho Wave, 8 Laser, 4 Crystal Wall | Laurence (`TEST_ARSENAL` in main.rs) |
| Capsules | one of yours every 2 s while fewer than 3 are out, at **your three capsule spots from the stage layout** (`stNN.props.json`, beside your start point; Call's anchor overrides); stages without a layout: 25–70 from you on your floor level; only you can take yours | spots: the game's `.dat`; rate **placeholder** |
| Start points / jump zones | the stage's own start set 0 (starts 0 and 1); standing in a jump zone launches you (speed = the zone's param, unit unconfirmed) | FORMATS-STAGES.md |
| Capture | stand at one of your capsules and press a skill button: it goes into that slot, replacing what was there (pressing elsewhere uses the skill). On-screen prompt names the skill while you're at it | game's tutorial text (`Event/en/quest101.ssb`: "press one of the [A, B, X, Y] buttons. The skill will be assigned to the button you've pressed"); replacing an occupied slot is an **assumption** |
| Skills | used up when cast, except the three "Retain" skills (s41 = 1: Photon Barrier, Arrow of Artemis, Reincarnation), which stay on the button while the Aura left covers their cost | notes like "Retain if you have enough Aura to use again" imply this |
| Requirements | health ≤/≥ n, arsenal ≤ n, max aura, level ≤/≥ n | table (FORMATS-SKILLS.md) |
| Strength rules | 17 = target's Level, 18 = target's Aura, 20 = half target's Aura; else the table's STR | table notes |
| Short-range (motion 12/13, e.g. Psycho Wave) | dash at the target if within 130 u, strike at the skill's reach (s64: 13 u, Excalibur 30 u); otherwise whiff | dash range + speed 260 **placeholders**; reach from the table (unconfirmed units) |
| Projectiles | leave at the **end of the cast** (after the charge-up; lost if you're hit mid-cast); blocked by walls unless "Penetrates obstacles". Shells: table speed + homing (Blaster 420 u/s, Faith fires ~100 u/s sharp homing); `[Parabola]`: lobbed to land on the target, "Explodes on impact" ones blast (Bomb 30 u radius); `[Arc]`: curves out and back onto where the target stood (multi-laser fans); `[Crawl]`/Tornado along the floor; `[Fall]` drops from above; `[Hole]`/`[Rain]`/mines/Photon Wave are set areas that go off later; remaining families: generic 170 u/s homing shot | MOVES.md |
| Defence (all kinds) | Barrier / Reflect / Absorb / Brush cover the **front only**, Shelter all round; lasts the table time (Crystal Wall 0.77 s), rooted meanwhile. STR > DEF destroys it (no damage); Pierce gets STR − DEF through unless Anti-Pierce. Reflect returns weaker shots; Course bends shots aside, About Face/Reverse send them back; Absorb is no use against short-range; Brush stuns short-range attackers; automatic Shelters trigger themselves; Add: effects (heal, aura, erase, paralyse, freeze, capture) | game's guide text (MOVES.md) |
| Hit reactions | light/medium/heavy stagger by damage in the last 3 s (front/right/back/left by hit direction), or an air hit's automatic knock-down: real launch physics (speed/angle from the row, the program's own knock-down gravity, real ground collision), 2 s lying, 1 s getting up, then 2.5 s blinking invulnerability (untouchable by everything, not just downed-only gating) — MOVES.md | hit-reaction table (`cache/skills.json` "reactions") + PROGRAM-RE.md |
| Falling out | 3 damage, back to your spawn | guide |
| Countdown | 3-2-1 (1 s each) before every round, nobody moves or casts; START shows as it ends | original's banners (HUD.md) |
| Knock-out | health 0 → down, round point to the other, restart after 3 s (positions, health, arsenal, capsules reset; lock-on choice kept) | |
| Attack Add: effects | freeze buttons, poison presses, aura → 0, heal = damage, level drain (MOVES.md) | table s51/s52 |
| Not built | the stragglers listed as not built in MOVES.md | — |

**Cast animation** (main.rs `cast_clip`, by family/motion — no table field names the animation; the table is in MOVES.md "Open questions"): short-range → ATK_SORD (punches/kicks/drain → ATK_PUNCH/KICK/DRAIN), `[Arc]` → ATK_LASER, parabola → ATK_THROW, mines → ATK_SET_BOMB, Photon Wave → ATK_SPHERE, Rain/Fall → ATK_SKYSHOT_HIGH, Hole → ATK_STAMP, Crawl → ATK_GROUND, shells → ATK_SHOT / FAITH_SHOT / KI_SHOT, defences → DEF_*, aura particle → SPE_POWER_UP. Dash → DASH_A, hit → DMG_SHOT_FRONT, down → BLOW0_FRONT.

**Controls:** skill slots are the dial's left / right / top / bottom. Keyboard P1: WASD, Space jump, Y lock-on, Q/E turn the free camera, skills J K U I; arrows = P1's D-pad. Keyboard P2: Enter jump, Right Shift lock-on, numpad 1 2 4 5 (moves with a pad only). **Gamepad = the original's layout** (from the game's tutorial text, quest001/101 and battlestart): left stick move, **click left stick (or left bumper, as on Xbox One/PC) to dodge** — each character's own move: **Jump** (Hero, Freia, Meister, Chunky, Know, Tsubutaki), **Tumble** (JD, Cuff Button: a ground roll, no recovery) or **Cartwheel** (Edgar, pH, Sammah: faster, then a recovery that holds you still) — from the character table (`cache/chars.json`, FORMATS-ANIMATION.md "Character table"); tumble/cartwheel play the characters' own DASH_A / DASHEND_A clips; speeds and the Cartwheel's recovery are unconfirmed constants in `sim.rs`; proof `shots/dodge_game_sheet.png`, skills/capture **X B Y A** (left, right, top, bottom); **holding a D-pad direction shows that slot's skill card** (name, school, type, cost, STR/DEF, the game's note) above the dial, like the original ("D-pad direction corresponds to the skill button"), render-only; **right trigger** lock-on toggle, **right stick** turns the camera when not locked on (sim input `Input::cam`) and **looks up/down** (render-only pitch, `CamPitchInput`, clamped −1.15…+0.85 rad -- about 65 deg up, 50 deg down, see "Cameras" above -- eases back to level when you lock on); **moving never turns the free camera** (Laurence 2026-09-27: like modern third-person games); click right stick recentres. Keyboard P1: Q/E turn, R/F look up/down. Test switches: `PD_FREE_CAM=1` (start unlocked), `PD_TEST_PITCH=1|-1` (hold look up/down); proof `shots/pitch_sheet.png`. First pad = P1, second = P2 (Laurence). Start opens settings, B closes them. Not built: left trigger = switch target (only one opponent), camera up/down.

**On screen:** the original battle HUD per view (HUD.md): health panels, opponent aura/level, arsenal count, skill dial with costs and names, lock-on ring, 3-2-1-START and YOU WIN / YOU LOSE banners. In the world (`fx.rs`), capsules are small glowing balls (pale = aura particle, red = attack, blue = defence, yellow = other); each view shows **its own player's capsules glowing and the opponent's as small faint ghosts** (render layers: camera i sees layers 0 and 1 + i). **Skill effects** are the game's own (`effects.rs`, FORMATS-EFFECTS.md), render-only, driven by sim state: a new `cast_seq` plays slot 0 on the caster — except short-range skills, whose slot 0 is the swing: it waits for the dash to end and plays 18 frames in (`fx::STRIKE_LEAD_FRAMES`), with the strike clip seeked to the same point and left to finish its follow-through while the player stands still (`main.rs::strike_lead`); slot 2 is the impact, played where a shot ends, at the contact point of a hit or block, or where a blast goes off; `PDEFTOPT03/05` (screen distortion) pieces are hidden (`PD_FX_DISTORT=0.2` shows them faintly; the reflective `PDEFTOPT06` / `YKREFRECT` blades and mine shells are drawn); each projectile plays slot 1 (looping, facing its travel), including its sprite / puff emitters (moths, flames, dust) drawn as a stream of sprites left behind it; `Combat::events` Hit → slot 3 on the victim, Blocked → slot 4 of the victim's barrier, Burst (an exploding shot) → slots 2 and 3 at the blast point. Effects are children of the player visual so they follow; an effect's clock starts once its model has loaded (at most 1 s wait). Where a flight slot shows nothing (Bomb Shot, Spore Schism) the stand-in ball remains; the blue barrier bubble likewise. Skills 301–340 have no effect file and borrow a similar skill's (FORMATS-EFFECTS.md). Effect speed: 30 frames/s (`PD_FX_FPS`). `PD_FX_LOG=1` logs, per frame, player 1's action, health, distance and shots, every combat event and what each new shot's flight effect holds (test runs).

**Four-player test switches (lane fourview):** `--players N` (1-4; `--p1`..`--p4` characters, `--teams 0,0,1,1`) starts N fighters on the stage's own start points (add `--near` to ring them round the centre); `PD_TEST_STICK2/3/4` drive players 2-4 like `PD_TEST_STICK` drives player 1; `PD_TEST_SWITCH=<player>,<tick>[,<ticks>]` presses switch-target; `PD_FRAME_STATS=<frames>[,<skip>]` prints uncapped wall-clock frame time. Layouts (views go only to human seats since 2026-10-03, `LocalViews::for_kinds`; CPU seats get none, so 1 human + CPUs is one full window): 1 view full window, 2 as the settings say, four a 2x2 grid, three: P1 top-left, P2 top-right, P3 the whole bottom half (no map quarter since 2026-10-03). Switch target is the left trigger / keyboard T (unconfirmed). Details: `audits/P0-FOURPLAYER.md` section 8.

**Test switches:** `--loadout 1,65,26,65` keeps both players' slots stocked with those ids at level 8 (sim state, deterministic); `PD_SHOT_ON_KO=1` saves `<shot>_ko.png` at the first knock-out. `--demo` runs to capsules, closes in and taps filled slots (a test script, not an AI); when it stops making progress it jumps and side-steps round the obstacle. `--near` starts both players close together near the stage centre (the old survey spawns) instead of the original start points far apart; use it with `--demo` for screenshot bursts that need a fight (`PD_DEMO_LOG=1` prints distance and health every 2 s). `PD_INPUT_LOG=1` (or `tools/bats/PLAY_DEBUG_CONTROLS.bat`) logs what the game receives from keys and pads. `PD_DEMO_DODGE=1` makes the demo dodge every 1.5 s. `--start-level N` / `--start-hand N` set the match rules (`combat::Rules`; defaults = the original: Level 0, 4 skills dealt). **Tick-exact screenshots** (2026-09-27): `PD_SHOT_TICKS=a,b,c` with `--screenshot out.png` saves `out_t<tick>.png` from the frame rendered right after each listed sim tick (60 per second; the action table's 30 Hz ticks × 2); plain numbers count from the match start (the countdown is 180 ticks), `p`-prefixed ones (`p0,p36`) from the `PD_TEST_PRESS` press tick (`out_tp036.png`). While it is set every frame advances the clock by exactly one tick (`TimeUpdateStrategy::ManualDuration`), so effects and animation land on the same picture on every run however slow the machine; the run exits 30 frames after the last capture and the frame-counted `--shot-frame` / `PD_SHOT_COUNT` burst is skipped. E.g. Psycho Knife's strike: `PD_TEST_PRESS=240,0,225 PD_SHOT_TICKS=p32,p40,p48 --stage st04 --near --loadout 3,3,3,3` (`PD_TEST_SPLIT=sbs` for side-by-side views). `PD_FX_ROOT_ONLY=1` plays skill effects the pre-2026-09-27 way (every piece on the effect's root at the feet, no sword ribbons or ray bursts) for before/after sheets. `PD_FX_GAMMA=1` blends additive effects the old way (linear light + tone-mapping) for A/B against the game-style adding. `PD_TONEMAP=none` turns off the cameras' tone-mapping (the original has none) for A/B.

**In-game dev tools** (`devtools.rs`, keyboard, any build): **F1** overlay of live sim state for every active seat, CPUs included (position/velocity/yaw, motion and gait, grounded/locked, team, health/level/aura, action, slots, this tick's quantised input; capsule/projectile/area/prop counts; last combat events). **F2** pauses the match (sim only, via `DevTools::take_tick` in `step_sim`, like the settings menu's pause; animation and effects keep playing), **F3** steps exactly one sim tick while paused. **F8** files a bug report: screenshot first (the note box appears 3 frames later so it is never in the picture), sim paused, then a note box -- Enter saves, Esc cancels and deletes the folder. Saved to `engine/bug_reports/<UTC date_time>/`: `note.txt`, `screenshot.png`, `state.txt` (`{:#?}` dump of the roster and every active `Player`/`Fighter`, capsules, projectiles, areas, events, match setup) and `inputs.csv` (last 600 ticks of all four seats' `Input`). Game/menu input systems are gated by `devtools::not_typing` while the note is open. `engine/bug_reports/` is git-ignored (screenshots show game assets). Test hooks: `PD_DEV_OVERLAY=1` starts with the overlay on; `PD_DEV_TEST_REPORT=<tick>` files a report automatically at that match tick (note filled in, saved ~240 frames later).

**Tests:** `cargo test -p pd_game` — cost/aura, used-up slots, projectile damage, dash + level-based damage, barrier block, hit/block events, KO + round restart, capsule spawn/pickup, replay determinism; per family (MOVES.md): shell speed + homing, sniper wait, parabola lob + blast, barrier front-only, shelter all-round, STR > DEF breaks, Pierce / Anti-Pierce, barrier time.

## Sound (`sfx.rs`)

Played render-side from sim state. Each skill's action row gives a **cast** (w3), a looping **travel** (s20) and an **impact** (s21); battle voices come from the character's own voice bank by the row's voice group (w4). `Combat::events` are mapped to cues by one function, `cues_for_event` — the single place an event becomes a sound:

| Event | Cue |
|---|---|
| `Hit` | the skill's impact + the victim's hurt voice |
| `Blocked` | the impact, quieter unless the guard broke |
| `Burst` | the skill's impact where the blast went off |
| `Capsule` | **`common/34`** — the game's own pickup sound |
| `Denied` | a short buzz (made here; the original's refusal sound is unidentified) |

**Capsule pickup** (2026-09-29, the sound Laurence noticed missing): the sim logs `CombatEvent::Capsule { who, slot, skill }` from the arm that captures a capsule into a button, and `cues_for_event` turns it into `common/34.wav` (the id read out of the program, `FUN_14014fd10`). The wav is also **preloaded** — the existing preload only covered skill cast/travel/impact, so without that the first capture of a round would be silent. Three tests cover the chain. ⚠️ Note the `--demo` script never walks onto a capsule, so a scripted run shows 0 captures: this only fires when a human picks one up. FORMATS-AUDIO.md lists the three ids still unwired (85 appear, 92 erase, 79 jump-zone hum) and why their trigger points aren't identified yet.

## School aura (`school_aura.rs`)

> **Superseded (2026-10-01).** The shell described below was replaced: `school_aura.rs` is now only the pure rules (who has an aura, tier, colour; from the program), `aura_fx.rs` plays effect 600's wisps, `aura_pulse.rs` the Aura Particle consume pulse, and `aura_glow.rs` the halo copy of the original's HUD aura object (**off by default**, `PD_AURA_GLOW=1` turns it on: live evidence says the original does not draw it in versus). The shell, `school_aura.wgsl` and its `PD_AURA_WIDTH/FALLOFF/GAIN/HEAD/ARM/RIM` switches no longer exist. Current facts: `audits/P0-AURA.md`, `FORMATS-EFFECTS.md` "School aura glow", `handoffs/H3-school-aura-handoff.md`. The halo itself is parked by Laurence. The text below is the v2 history.

The glow outline round each player (VFX-AUDIT.md item 6, from a GameFAQs guide via Laurence): colour = the arsenal's dominant school — Psycho red, Optical yellow, Nature green, Ki blue, Faith purple, ties broken in that order — visible only while `Fighter::level >= 1`. Render-only: reads `Combat::arsenals`/`skills` and `Fighter::level` (via the `World` resource), never writes sim state; `arsenal.rs`/`combat.rs` are untouched. Reading `PDUWP.exe`'s Ghidra export this pass found no per-character rim/fresnel shader or colour table (see `school_aura.rs`'s own module doc for the addresses checked) — the colour table and tie order are the community guide's, unconfirmed against the program.

Drawn as a glow "shell" on each fighter's own skinned mesh (v2, replacing a flat oval hoop quad Laurence rejected): a child entity per body mesh piece shares its `SkinnedMesh` joints, so it follows every animation frame; the vertex shader pushes the surface out along rebuilt normals and the fragment shader draws only the shell's back faces, `(-N·V)^k` — a soft band hugging the silhouette, strongest at head/shoulders (height ramp), faded away from outstretched hands (distance off the body axis), thinner when the camera is close. Additive, no depth write, render layer shared (both split views). The normals are rebuilt at spawn because `pd_convert` exports every character vertex normal as a constant (0,0,1) (the real one is in the normal map) — that constant is what made the two earlier on-mesh attempts paint whole limbs solid. Constants in `school_aura.rs` (`AURA_WIDTH_U` 1.4, `AURA_FALLOFF` 1.8, `AURA_GAIN` 1.1, `AURA_HEAD_BIAS` 1.0, `AURA_ARM_FADE_U` 2.0, `AURA_BODY_RIM` 0) are unconfirmed, tuned by eye against the Lock-On footage; env overrides `PD_AURA_WIDTH/FALLOFF/GAIN/HEAD/ARM/RIM` for tuning. Test with `--arsenal school:<skill id>` (a calibration-only single-school 30-card deck, e.g. `school:65` for Optical) and `--start-level N`.

## Stage look (`stage_look.rs`)

Stages are unlit glTF with the original's per-vertex light already baked in by `pd_convert` (FORMATS-ALR.md "Stage lighting"), plus the sky dome and backdrop. At match start `StageLook::load` reads `cache/<stage>.light.json` and lights the characters with the script's character set: `DirectionalLight` colour = its sun colour's hue, strength `LIGHT_SUN × luma / CHAR_LIGHT_REF` (0.5), direction = its light direction (Z mirrored); ambient = the average of its sky and ground colours, `LIGHT_AMBIENT × luma / CHAR_LIGHT_REF` ⚠️ tuned by eye on Lost Palace. No `light.json` → the old white lights. Distance fog from the script only with `PD_STAGE_FOG=1` (the PC original draws none). `sky_without_fog` turns fog off on the sky nodes (`m2_*`). Tone mapping is left at Bevy's default (the viewer's `--tonemap-none` shows raw colours for comparison).

`--near` now surveys the floor under the stage's first original start point (st03's hall floor is exactly y 50, which the old "y > 50" survey skipped, putting players on the roof).

### The sky drifts (`sky_scroll.rs`)

The backdrop is two shells: an untextured vertex-colour gradient and a **tiled cloud layer** (one 512² image, REPEAT sampler, UVs authored to a 3.475 box — deliberately wrapped, so it is meant to move). `sky_scroll::scroll` advances the textured shells' `uv_transform.translation`, in `PostUpdate`.

Shipped rate **0.004 texture repeats/sec** (≈4 min per tile) — ⚠️ **eyeballed, not recovered**: the original did it with the fixed-function texture transform, and no shader of ours (or the original's) has a time term, so the real speed is not in the binary. Picked to sit near the threshold of notice, which is the 2004 look. `PD_SKY_SCROLL=<repeats/sec>` overrides it for A/B.

Verified by pixel A/B, same stage/frame/sim, off vs on: **st06 90× and st02 297×** the frame's own static baseline, with the cloud shapes *shifted* rather than different. Three things that are easy to get wrong, all found by measurement and documented in the module:

- the backdrop is **not** always `m2_*` (st08's is `m82_*`), and *every* model instance is named `m<N>_d<M>`, so the filter needs the name **and** a mesh world-extent ≥ 15,000 units (shells 19,698–37,959; largest prop 11,101);
- an `Assets<T>` write in a normal system is **deferred to the next frame's apply** (Bevy `b0004`), so an `Added<>` filter writes into the void and it is silently dropped — hence `PostUpdate`;
- Bevy **synthesises** glTF entity names (`Mesh`, `d0`), so the real node name is two parents up.

⚠️ Limit worth knowing: a stage whose *visible* backdrop is the untextured gradient (st08's sand) shows **no motion** — the cloud layer is behind it there. Whether the original animated that gradient is still unknown.

## Arena objects (`props.rs`, `props_view.rs`)

The stage's objects (`stNN.props.json`, FORMATS-STAGES.md) are sim state (`Combat::props`): intact → damaged → broken, thrown, gone. Every rule below is a reading, **unconfirmed** — neither toughness, weight nor "throwable" is in the stage file; each number is a named constant in `props.rs`.

| Rule | Built as |
|---|---|
| What can change | class 2 objects with a damaged or broken model; st08's collapsing ground blocks (kind 58) and st01's tower pieces (64+) are left alone (their scripted behaviour isn't decoded) |
| Wear | shots that meet an object (the collision raycast now reports which object, `Collision::raycast_owner`), exploding shots and areas going off (Hole, Rain, mines, Photon Wave) within their radius wear it by the attack's STR. Toughness = largest side ÷ 12 (chair/drum 2, desk 2, floor section 6, container 9). Half worn → damaged model; fully worn → broken model (if it has one; guardrails only bend) |
| Broken | its collision is switched off (`Collision::set_object_gone`, synced from the sim every tick by `Match::step`), so a broken floor section or bridge (st04 `yuka`, st03 bridges, kind 23) drops whoever stood on it and the path is open |
| Psycho Kinesis / Power | "Requires: Available object": the nearest throwable object within 70 u and on your level — small (≤ 60 u), not a thin panel (own hull ≥ 4 u thick), has a broken model, not structural (kinds 6, 23, 26, 28, 48). It lifts 15 u above its spot, hangs 0.25 s, then flies at the table's shell speed/homing; "STR depends on object" = size ÷ 10 (1–6). It shatters wherever its flight ends (gone). Walls and other objects it meets are worn by it |
| New round | every object back as placed |
| Drawing | `props_view.rs`: damaged/broken → hide the stage glTF's `propNNN_*` nodes, show the library model (`stNN.props.glb` `typeTT_damaged/broken`) at the object's placement; thrown → the placed model follows its shot, tumbling; gone → hidden |

Break/hit effects come from the stage's EF00 records (debris, flying fragments, smoke, sounds), and shots hitting objects play their impact sound (FORMATS-STAGES.md "Object effects"; `pd_convert stagefx`; test switches `PD_FX_BREAK_AT=<frame>`, `PD_FX_BREAK_TYPE=st03:tableA`). `mdl_hit_*` are crater-mark models, not sounds. Not built: camera shake, ambient records (jump-zone hum, lamp sparks), crater decals, contaB's collapse animation (`OA01`) and collapsed hull, rubble falling on players, melee hits on objects.

Test switches: `--props-state damaged|broken` starts every breakable object in that state; `PD_PROP_LOG=1` logs state changes; `PD_SHOT_ON_THROW=1` saves 4 screenshots after the first throw. Proof: `shots/propsB_st06_sheet.png` (intact / damaged / broken), `shots/pkw_sheet.png` (tanks lifted before a throw).

## Footsteps (`footsteps.rs`) — built 2026-09-29

A small pale puff at the feet on each footfall, timed off the **walk cycle's own clock** rather than velocity, so a run plants feet faster than a walk. Measured from the converted glTF (`pc00a.glb` animation sampler maxima: WALK 28 frames, RUN 19 at 30 fps), so it cannot drift from the data.

The original's rules we could honour, and the one we could not:

- **10-frame minimum gap** between steps ✅ (`MIN_STEP_TICKS`, decoded from the same dispatch)
- **only while grounded and actually moving** ✅
- **strafe steps** ✅ — a locked-on player plays a one-shot `TURN_*` pivot then the ordinary WALK/RUN clip, so the feet move on the walk/run clock. The first cut excluded `Strafe` and the effect never fired at all: the demo AI is permanently locked on, so the "built" lane was shipping silently dead. There is a regression test for exactly this.
- **per-surface footstep** ❌ **not possible** — nothing in our converted data carries a ground material. Hull face `attr` values differ between *near-identical* object types (st04 `yukaA` 0x34 vs `yukaC` 0x0B), so they are provably not materials, and `Collision` has no per-face material at all. So there is **one** footstep, not sand/dust/water. `Surface::of(stage, pos)` is the seam — give it a real answer and the other three variants light up with no change to the caller. Guessing from the stage *name* would be inventing a mapping, so `Surface` is deliberately hardcoded to `Hard` with a test that fails if anyone "helpfully" adds a guess.

⚠️ **Verification took three attempts and the failures are the useful part**, because each one is a trap a future change can fall back into:

1. `PD_STEPS=0` A/B gave 1.12× noise — the `--demo` AI is **wall-clock driven**, so two runs of the same frame differ everywhere. Fixed by driving with `PD_SHOT_TICKS` (one sim tick per frame) + `PD_TEST_STICK`, which makes the sim deterministic.
2. Even then, ON-vs-ON noise was 0.43 against an OFF-vs-OFF floor of 0.10 — with a deterministic sim that *should* be zero. It isn't: the camera lerps on `delta_secs` and particle draw order is not stable, so **19% of pixels differ between two identical-config runs**. Any whole-frame A/B in this project is bounded by that, not by the effect under test.
3. So the proof is a **localised diff** instead: one tight cluster of 268 changed pixels at the character's feet (x≈1553, y≈1461) and nothing else anywhere on screen. `PD_FREE_CAM=1` (players not locked on) is what keeps the fighter in frame for this.

⚠️ **The size is a hard-won number.** First pass was 0.34 units — sub-pixel at 3200×1800, so the lane fired 15 times a run and showed *nothing*. Second pass tried a flat scuff (0.9 × 0.2), but `tick_particles` scales a `Shape::Puff` with `Vec3::splat(size[0])`: **the second size is discarded**, so a puff is a soft ball and "flat" is not available. What actually made it read was the alpha — 0.55 grey over a mid-grey stage floor (~0.45 luma) is invisible, so it needs to be clearly brighter than its background. Final: 1.1 units, 7 motes, alpha 0.95. ⚠️ Eyeballed, not recovered.

⚠️ **One footstep sound, and the game has two** (corrected by Laurence's ears, 2026-09-29). The original has **two** footstep dispatchers — `FUN_140230d60` builds the `Type0` particle classes, `FUN_140230cd0`/`FUN_140231490` the `Type1` ones — and on Lost Palace that is the difference between a hard heel on concrete (~80%) and a wet, puddle-like step (~20%). The choice is **a downward collision raycast, not a random roll**: `FUN_140256390` returns the nearest face's class byte, and no `rand()` exists anywhere in the chain, so the 80/20 is that stage's *geometry*. A face the probe misses gives **no footstep sound at all**.

We play `common/30` (byte-verified id) once per footfall. The original's 20% wet step is **not implemented** — the collision class that drives it does not exist in our converted data (the only `attr` values in all 30 stages are `0x77FC`/`0x34`/`0x0`/`0x0B`, and neither runtime class gate appears anywhere). FORMATS-AUDIO.md has the measured tables; the second clip is still unidentified and blocks shipping the 20%.

`PD_STEP_LOG=1` logs each footfall with the floor height it found.

Hit-stop and KO slow-motion are in (`hitstop.rs`, `774c1d4`). Enable with **`PD_HITSTOP=1`**; off by default. The sim is untouched — a hit slows `Time<Virtual>`, which `Time<Fixed>` follows, so `FixedUpdate` stops and the renderer holds the pose at the moment of impact. `Combat` stays a pure function of its input list, which is what replays and netcode depend on. Weight scales with damage (55/85/120 ms; a block only 35 ms so defending never reads as winning) and both fighters down holds 0.35 speed for 1.6 s. Measured: a 200 ms freeze costs the sim 14 ticks.

Decoded `setup/configdata.dat` (2026-09-29): **312 bytes = 78 floats = 6 blocks of 13, no header.** Four columns are constant across every block (20, 10, 13, 13), which is how the structural constants separate from the per-mode tuning. The pitch limit is the symmetric pair, **±70°** in the five normal camera modes (the sixth block is an outlier: 25 / -32.5, col 12 = 3.5625). `CAM_PITCH_MIN`/`MAX` now ship at ±1.2217 rad — the previous `-1.15` / `+0.85` was set by eye, capping the downward tilt at 49°.

⚠️ The column identity is **inferred** from the values' symmetry and from matching the old constants, not program-verified. If the camera ever misbehaves at the pitch extremes, re-check that mapping first.

**The original does not raycast the camera.** `FUN_140256390` has exactly three callers in the binary — the two footstep dispatchers and itself — so nothing camera-related uses it. Our boom pull-in is therefore our own addition, not a fidelity loss. `FREE_EYE_PITCH_MAX` is deliberately *not* widened to match `CAM_PITCH_MAX`: the view already reaches the full ±70° through `tilt`, so a wider eye orbit would buy nothing while re-introducing a documented floor/ceiling clip.

Top/bottom split framing: the strip only holds about **17 units of world height** at the 60u boom, and a 20u character below the view axis cannot fit in it — so the eye drops by the chosen camera height to keep your own character on screen. Two controls do this together: the eye drop brings the character into the strip, and `TB_LOOK_DROP` (4.0) sets how much floor sits under their feet.

**Camera height is a settings row** (`menu::CameraHeight`, row 3): **HIGH / SLIGHTLY LOW / MID / LOW / VERY LOW** = eye drops of 0 / 4 / 7 / 11 / 14 world units, stored in `settings.json` as `cam_drop`, `MID` by default. `PD_TB_CAM_DROP=<n>` overrides for measurement.

**LIVE battle camera of the original (lane moverec, 2026-10-02; versus match, player 0 driven by pad memory, `mr_menu.py camera --lean`, one sample per scene pass, ~10 s still / 40 s moving).**

- **Where it lives:** the scene pass (`FUN_140299310`, two calls per frame, view 0 then view 1) reads per-view camera constants at `0x1405a5c60..0x1405a5cdc`: projection `[1.3032, 0, 0, 0 / 0, 2.3159, 0, 0 / 0, 0, 1, 1 / 0, 0, -2, 0]`, then a **view matrix in row-vector layout** (rows `0x1405a5ca0`, `cb0`, `cc0`, translation `cd0`): the right / up / forward axes are its **columns**, and the eye is `-(t.x * right + t.y * up + t.z * forward)`. Checked: the eye so computed lies exactly on the line through the fighter along `forward`, 55.0 away.
- **Projection (LIVE, constant in every sample):** y scale `2.3159` = vertical FOV **46.71 degrees** (`2 atan(1/2.3159)`), x scale `1.3032` = horizontal **75.0 degrees** (= cot 37.5 degrees; earlier text said 74.7, an arithmetic slip); aspect (x/y) = 16 : 9 for each half of the split screen (the same in view 0 and view 1). Near/far terms (1, -2) are the engine's own depth mapping.
- **Free camera (not locked on), standing still, 150 samples:** the eye is **55.0 units behind the fighter in the horizontal plane and 13.0 units above the fighter's position** (`FIGHTER_POS` y), looking **exactly level (pitch 0.0 degrees)**; the fighter sits 13.0 below the view axis and dead centre sideways (lateral 0.0). These are the `13` and `55` of `configdata.dat` block 0 (`6,3,20,10,13,55,13,55,70,-70,-9,-9,1`: columns 5-8 are height / distance pairs; the +-70 are the pitch limits), so the "inferred" column identity in GAME.md is now supported for those four.
- **Moving (free camera):** distance and height hold at 55 / 13 with a small spring: running straight 54.3..56.8 and 12.3..13.6 high, backing 46.4..55.2. The camera **swings its yaw behind a sideways-running fighter** (strafing right turned the view axis from 170.8 to 124.0 degrees in 50 ticks, about 28 degrees per second) and the horizontal distance dips to **25..45** during that swing (not a wall effect: it is the lag while the yaw changes). When the fighter **falls** (off a stage edge) the eye rises with it, 13 -> 18.0 above, and it follows at 55.0.
- **Locked on (RT pressed once, opponent far away, steady state, 90 samples):** distance **52.0**, eye **31.0 above** the fighter, **pitch -19.1 degrees** (looking down at the foe), fighter 12.3 below the view axis. During the 12 ticks after the press the camera swings and pitches (down to -38 degrees at the extreme) before settling; with the opponent close the distance falls to 10..13 and the pitch to -11..-28 (the camera frames both). Look-at = the lock-on target.
- **Wall / pull-in: not observed.** Four 300-tick runs in the four directions in free mode never showed a distance below the swing dips above, and the stage's open edges are drops, not walls; nothing in the samples is a minimum-distance clamp. This agrees with "the original never raycasts the camera" (above): UNCONFIRMED only in the sense that no real wall behind the camera was reached (the stage used, stage index 6, has none near its start points).
- Derived constants for our camera: boom 55.0, height 13.0, pitch 0 (free), vFOV 46.71 degrees, lock-on boom 52.0 / height 31.0 / pitch -19.1 degrees.

⚠️ **HIGH (0) is the broken end and is offered, not recommended.** At 0 your own character falls off the bottom of the strip — measured at **1px of floor below the feet** versus 52–99px for the other four. Laurence hit exactly this: the whole of his character was below the bottom edge, leaving only the opponent visible. It is in the list because it is a real position, and a player who wants it should be able to have it.

Measured across the five, lowest character pixel in the 900px P1 strip: 899 (0), 834 (4), 848 (7), 840 (11), 801 (14).

Anti-aliasing is **4x MSAA, fixed** (`main.rs::spawn_scene`, `Msaa::Sample4` on each match camera). An OFF/2X/4X settings toggle was built and then **parked 2026-09-29** — the toggle's "off" state broke the HUD's compositing over the match views on the first match, and 4x turned out to be what the game has always used anyway.

`Msaa` **is** a required component of `Camera3d` in Bevy 0.19 and its default is `Sample4`, so 4x was the effective setting the whole time. Measured, not assumed: a probe reading the component off both match cameras reported `Some(4)` with no MSAA code in the codebase. The value is now stated explicitly so it is a documented choice rather than a hidden default.

⚠️ **`Some(Msaa::Off)` is NOT the same as no `Msaa` component.** The component also governs MSAA writeback between cameras sharing a render target, so inserting `Off` changed how the HUD's 2D camera composited over the match views — the Setup screen, the countdown banner and the in-world lock-on marker all drew at once, and the crosshair/nametag stamped where the camera looked. Do not reintroduce an "off" state as a faithfulness gesture without re-testing the first-match path through the real menus.

⚠️ **A whole-frame A/B cannot prove MSAA here, and the control proves it.** Two runs with *identical* settings differ by **90.8%** of pixels (mean |Δ| 111) because the harness camera lerps on `delta_secs` — more than the 94.2% of a 1-vs-4 pair. What works is measuring **edge softness**: an aliased edge is a hard one-pixel step, an antialiased one a multi-pixel ramp. Mid-range horizontal gradient pixels run 0.93% aliased vs 7.34% at 4x.

⚠️ **The interactive menu path is not covered by the headless harness.** `--near` goes straight to a match, so the Setup→Match transition and the countdown are unreachable from it. Two bugs (the first-match overlay and the stamped crosshair) were reported from the real menus and were invisible to every headless capture. Drive `--menu-script` when a change touches that path.

## Not built yet

HUD aura meter segments, resuscitation (teams exist since setup4; revive does not), a test of online play between two real PCs (online itself was built by Brett and merged 2026-10-03, ONLINE-PLAN.md), gamepad remapping, the original deck-menu art and status icons. The camera pull-in when a fight hugs a wall is **our own addition, not a missing feature** — the original never raycasts the camera (3 callers of the raycast, none camera-related), and what shows is our `4.0`-unit minimum boom. Kept as-is per Laurence; see archive/AUDIT-2026-09-29.md.

**Footsteps** are now built — see "Footsteps (`footsteps.rs`)" above, including why only one ground surface is possible.

**Camera matched to the original (lane camcloud, 2026-10-02).** Free: 55 behind, 13 above the fighter (before the settings' camera-height drop), level look. Locked on: 52 behind, 31 above, with a steady -19.1 degree look toward the foe once the foe is further than `LOCK_AIM_MIN_DIST` (closer keeps the approved easing). The FOV is now per view (lane camfov, 2026-10-03: see "FOV and camera distance settings" below); the top/bottom strip's own 74.7 override is gone. Yaw, strafe and swing are untouched; our swing was not re-measured against the 28 deg/s and 25-45 dips (UNCONFIRMED). `PD_CAM_OLD=1` restores the 60/30 boom positions; `PD_CAM_HFOV`, `PD_CAM_DIST`, `PD_CAM_UP` override. **Wall pull-in kept** (3 u off the wall, 4 u minimum; `PD_CAM_NOPULLIN=1` turns it off): the program has no camera raycast we could find (READ: `FUN_140256390` has footstep callers only) but film takes 1, 60, 80 show the original's camera coming in near walls; the rule is UNCONFIRMED. **What the pull-in ray hits** (2026-10-03): the program collision PLUS the camera-only `stNN.camcol` set (render walls, hull-less objects, 0x1F object faces), i.e. exactly what it hit while the collision carried `dat+walls`; the sim's collision no longer has those (COLLISION.md "Camera-only occluders"). The camera-height setting is now an offset from MID, which gives exactly the original's 13 / 31 (HIGH +7, SLIGHTLY LOW +3, LOW -4, VERY LOW -7 u). Open-catwalk proof (st06, takes 002 locked / 105 free): `shots/cc_cam_open.png`; fighter height over strip width is about 0.8-1.0x the original's. Proof `shots/cc_cam.png`.

**The sky drift is now the original's own rate (lane camcloud).** READ `FUN_1402a07b0`: each scrolling material's six int16 fields (+0x3c..+0x46) x frame counter (30 Hz) x 1e-5 feed c90/c91; `pd_convert skyscroll <Map dir> <cache dir>` writes `stNN.sky.json` (draw node name -> six fields) and `sky_scroll.rs` drifts each listed backdrop draw at `(f0, f1) x 3e-4` repeats/s (e.g. st02/04/05/06/12 `m2_d1` (-100, 50) = (-0.030, +0.015); st03/13 (130, -72) = (+0.039, -0.022); st01/08 have layered shells at different speeds). This is about 10x the old eyeballed 0.004 and has a V component. A draw not listed (fields 0) does not move; a cache without `.sky.json` falls back to the old rate. UNCONFIRMED: the two other layers (normal maps) of the sky material are not drawn by us. Also fixed: the old code wrote the offset only once at spawn (it never advanced). Proof `shots/cc_sky.png` (st06; frames 10 s apart on a frozen clock, `PD_SKY_FRAME_DT`).

## FOV and camera distance settings (lane camfov, 2026-10-03)

Laurence: "FOV is broken (changing doesn't work) and make it a slider", "in 2 player top and bottom split the FOV is broken, camera is too close", "camera is too close in general - make camera distance a slider".

- **Cause (read from the code, then proved with the before pictures `shots/camfov/before_*`):** `apply_fov` overwrote the FOV of every top/bottom strip with the original's horizontal 74.7 degrees across the 32:9 strip (vertical about 23 degrees), ignoring the setting: that is why the setting "did nothing" and the strip was a thin telephoto slice. Every other layout used `fov_for`, which read the setting as the vertical angle of an 8:9 side-by-side half, so a full-screen 16:9 view got only about 27 degrees vertical (the original shows 46.7).
- **The setting now (`menu::Settings::fov_deg`, json key `fov_h16`):** the HORIZONTAL FOV of a 16:9 view, `FOV_MIN` 50 / `FOV_MAX` 110 / `FOV_DEFAULT` 75.0 (LIVE: the original's projection x scale 1.3032 = cot 37.5 degrees; the 74.7 quoted before was a slip). An old saved `fov_deg` (the old meaning) is ignored on purpose.
- **Per view (`views::fov_for(h16, aspect)`, `apply_fov`):** each camera's own viewport aspect decides. At least 16:9 (a strip, an ultrawide): vertical = the 16:9 vertical angle (Hor+, sees more to the sides). Narrower (a side-by-side half, 4:3): the 16:9 horizontal angle is kept (Vert+, sees more above and below). So every view shows at least the original's 16:9 picture. The old strip floor (`TB_MIN_VFOV_SHARE`, `PD_TB_VSCALE`) and the strip override are gone. `PD_CAM_HFOV=<deg>` overrides the setting for a capture; `PD_CAM_OLD` now only restores the 60/30 boom positions.
- **Camera distance (`Settings::cam_dist_pct`, 70-160, default 100 = today):** one factor scales the boom, the height (including the top/bottom camera-height drop) and the look-at offsets, so the angle to the fighter is kept. Exactly 1.0 at 100: positions are the old ones (`camera_setting_tests`). Read every frame; a change snaps the boom so it shows at once. `PD_CAM_DIST_PCT=<pct>` overrides for a capture. The wall pull-in's 3 u clearance and 4 u minimum boom stay ABSOLUTE (not scaled) because the approved near-wall dissolve (`cam_fade.rs`, 9 to 4 u eye-to-head) is defined on those distances; the ray is as long as the scaled boom.
- **Which layout a match uses:** a CPU seat is an active roster seat with its own view, so a 1-human-vs-CPU match is a 2-player split (top/bottom or side by side per the setting), not a full-screen view. *(Superseded 2026-10-03, lane online: only human seats get a view, `LocalViews::for_kinds`, so 1 human vs CPUs is one full-window view.)*
- **Capture switches:** `PD_SETTINGS=<file>` reads and saves settings in that file instead of `cache/settings.json` (a file that does not exist = every default, as on a first run), and a screenshot run never saves at all (`main.rs`; it replaced camfov's `PD_TEST_FRESH_SETTINGS`, which is gone, checked 2026-10-07); `PD_WINDOW` is in LOGICAL pixels (this machine has a 200% scale: `960x540` gives a 1920x1080 picture).
