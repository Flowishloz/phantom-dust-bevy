# Bug graveyard

Fixed bugs, newest first. One entry per Laurence report: what he saw, what
caused it, what fixed it, and the proof. Closes the loop on a report so it
stays fixed -- each entry names the regression test that pins it.

## Bomb: a square while it is thrown (2026-10-09, bombsq lane) -- OPEN, needs Laurence's pick

- **Symptom:** Laurence: "Bomb still has a square texture whilst being thrown" (play copy fa724aa).
- **Cause (MEASURED):** not the bomb's sprites (round, additive, billboarded). The bomb's own flight light (LIGHT02, eft0031 slot 1, range 1500) lights the stage face nearest it fully orange, bounded by the face's hard edges; `PD_STAGE_EFFECT_LIGHT=0` removes exactly that. Pictures `engine/shots/bombsq/bomb_square_before_after.png`.
- **Changed (READ, does not remove it):** LIGHT02 fade = word 7 with the program's envelope on range and colour (`effects::light_fade`); the stage point light interpolated per vertex (`stage_material.wgsl::vertex`; `PD_STAGE_POINT_PIXEL=1` = old). Tests `a_light02_fades_its_range_by_word_7_and_never_dims_its_colour`, `the_light_fade_length_is_word_7`, `the_point_light_is_interpolated_between_vertices_then_clamped`.
- **Open:** by the program's maths as read the original lights that face too (UNCONFIRMED: no warm footage). Ask Laurence: keep the faithful light, or effect lights off on the stage (`PD_STAGE_EFFECT_LIGHT=0`). Details VFX-AUDIT.md "2026-10-09".

## Four players well under 60 fps; frame drops when casting (2026-10-08 night to 10-09 morning)

- **Symptom:** Laurence: "the effects are great but they cause frame drops when casting moves"; four-player matches ran at about 20 fps (52 ms a frame on 2026-10-08) with stutters; asked to "continue overnight until all performance issues are fixed".
- **Cause (MEASURED, Tracy + `PD_FRAME_STATS`):** the GPU was nearly idle (~4 ms a frame); Bevy's render thread was the bottleneck. Every effect piece, aura wisp and particle owned a material rewritten every frame (each rewrite = a new uniform buffer and bind group on the render thread); the stage light rewrote every stage material when an effect light changed; idle overlay cameras still drew; each fighter's ~39 pieces had their own material (no batching); GPU light clustering ran per view for lights the game does not have; each view's render schedule paid the multi-threaded executor's hand-off for ~45 mostly idle systems; the play copy was a lightly optimised debug build.
- **Fix:** nine commits (PERF.md "Round 3" lists each with its numbers and off switch): per-instance rows for effects, wisps and particles; one shared stage-light buffer; sleeping overlay cameras; shared fighter materials; CPU light clustering; single-threaded view schedules; two main-thread walks trimmed; Laurence's copy built with the optimised `play` profile.
- **Proof:** 4 players, the same deterministic fight every run: 61.5 ms (p99 141) at the start of the night to 19.0 ms in the dev build with the OLED screensaver running (it adds ~1.8 ms); 17.8 ms in the optimised PLAY build with the median frame at the 60 fps cap (render thread 15.0 ms of 16.7). Pictures: fixed-tick captures old vs new within rerun noise at every step, t300 byte-identical for most; forced aura byte-identical; footstep puffs within 2/255 (PERF.md). Tests: pd_game 1222 pass; new `a_particle_material_draws_like_its_standard_material`, plus round 1-2's row / material tests.
- **Final (later on 10-09, quiet machine):** with Laurence's shadow pick (2 cascades per light from three views, 5163d4c) the PLAY copy holds 4 players at a locked 60 fps: 16.67 ms mean, p99 17.4, render thread 12 ms. Test: `shadows::tests::cascades_trim_from_three_views_only`.
- **Still open:** Laurence's play-test; 4K at 4 players not measured; `PD_RENDER_ST=1` to re-measure on an idle machine.

## Still an invisible barrier deep under the Palace stairs (2026-10-07, cloud session)

- **Symptom:** Laurence, after the 2026-10-03 stairs fix: "the stairs still have a hitbox that is disingenuous underneath (invisible barrier when walking under)", stopped "deep in, where it's low" (Lost Palace big flights).
- **Cause (MEASURED from the numbers in `stairs_under.rs`):** the kept pocket's cross wall stood where the soffit is 22 u up (z -186.3), and walls hold a body's centre a wall radius (5 u) off, so a fighter walking in from the hall stopped at z -191.3 under 24.9 u of soffit: about 5 u of air over a 20 u fighter's head.
- **Fix:** the cross wall now stands one wall radius past the head line (soffit = `sim::BODY_HEIGHT` = 20 u, z -182.85), so the body stops with its head just under the soffit; the open-side wall is cut at the head line. `PD_STAIRS_ORIGINAL=1` still restores the original.
- **Proof:** `stairs_under::tests::the_cross_wall_sits_a_wall_radius_past_the_head_line` (no cache needed) and `walking_in_from_the_high_end_stops_where_the_head_meets_the_soffit` (real st03 file: needs the cache, so it only ran on a PC with the cache; it SKIPS in a cloud session). PC 2026-10-08: the real-file test RAN (both flights stop at z -182.8 with 0.00 u between head and soffit); picture `engine/shots/w1008/stairs_before_after.png` (walked in from the hall end: original stops at the edge, now walks in under the slab). Laurence's try pending.

## Frame drops when casting (2026-10-07, cloud session)

- **Symptom:** Laurence: "the effects are great but they cause frame drops when casting moves."
- **Cause (READ, not measured):** a cast loaded its effect's glb and every texture from disk (`effects::play_from`, `load_fx_image`, `spawn_trails`), and the effect definition `.fx.json` on first use; nothing held the handles after the effect ended, so Bevy freed them and the next cast of the skill loaded everything again. Other cast-time costs found and NOT fixed yet: a new material per piece / particle (bind groups; the perf branch's "shared materials + per-instance data"), first-use pipeline variants, per-frame mesh rebuilds for blades / flares / ribbons.
- **Fix (first step):** `effects::FxPreload` + `fx::preload_match_effects`: when a match's arsenals are set, every skill effect in them (ground and air rows) and the three status loops are read and their glb and textures loaded with the same paths and loader settings as a cast, and held until the next match. `PD_FX_PRELOAD=0` turns it off for an A/B.
- **Proof:** builds, suites as before (cloud: 1136 pass, the 4 known Linux-only failures). NOT measured: on a PC run a fight with `PD_FRAME_STATS=1800,200` with and without `PD_FX_PRELOAD=0` and compare max / p99 frame time at casts; if hitches remain, bisect with `PD_FX_OFF=ms,points,sprite` / `piece` / `blade,flare,trail`.
- **Fix (second step, 2026-10-08, lane fxperf):** the other cast-time costs above. Effect materials are shared (never-written flares, MSPARTICLE ribbons) or reused from a pool (everything animated), `PD_FX_MATCACHE=0` = old; blade / flare / ribbon / wisp meshes are refilled in place and not rewritten when unchanged, `PD_FX_MESHREUSE=0` = old; the arsenals' effect pipelines are compiled at match start by invisible zero-area draws, `PD_FX_WARMUP=0` = off; effect env switches are read once. Picture-identical by construction (tests pin the new code to the old). UNCONFIRMED: the frame-time gain; PC recipe in PERF.md "Round 2".

## Rapid Cannon's thick black smoke (2026-10-03, fxfix)

- **Symptom:** Laurence: "rapid cannon's effect produces a thick black smoke that obscures the user's view".
- **Cause (READ):** MSPARTICLE emitters with word 26 = 1 (85 effects, 91 skills, 49 of them attacks) set the blend 0x5401 (SRC_ALPHA, 1 - SRC_ALPHA, `FUN_14010e260` -> `FUN_14028e4e0`), but `ms_particle.rs` drew them opaque (the literal pixel shader writes alpha 1), with the colour fading to black. Rapid Cannon's cast smoke is `0x337f7f7f -> 0` at 5 -> 20 u, so it became an opaque dark cloud over the caster. Same rule gave Fire of Gehenna a full-screen orange fill and Jack Pot black soot.
- **Fix:** blend-1 puffs now take their opacity from the colour's alpha byte (`ramp_alpha`), blended SRC_ALPHA / 1 - SRC_ALPHA. `PD_MSP_OPAQUE=1` restores the opaque draw. How the original turns that alpha into transparency is UNCONFIRMED (FORMATS-EFFECTS.md "MSPARTICLE blend 1").
- **Proof:** `engine/shots/fxfix/final/rapid_cannon_before_after.png`; tests `ms_particle::tests::blend_one_puffs_fade_by_the_colours_alpha`, `blend_words`.
- **Not fixed / open:** Psycho Knife's "weird square" -- see VFX-AUDIT.md "2026-10-03 sweep" (cause UNCONFIRMED; the leading candidate is the sword ribbon's flat pink sheet; a plain alpha-blend change was tried and reverted because the program's shader makes it additive for white colours).

## FOV setting did nothing; top/bottom and full-screen cameras too close (2026-10-03, camfov)

Laurence: "FOV is broken (changing doesn't work) and make it a slider"; "in 2 player top and bottom split screen the FOV is broken, camera is too close to the player"; "camera is too close in general".
Cause (two parts, both in `main.rs::apply_fov` / `views::fov_for`):
1. `apply_fov` overwrote the FOV of every top/bottom strip (`layout.is_strip() && !cam_old()`) with the original's 74.7 degrees HORIZONTAL across the 32:9 strip, i.e. about 23 degrees vertical, whatever the setting said (the setting changed nothing there; the camera was a thin telephoto slice, "too close"). The old strip floor (`TB_MIN_VFOV_SHARE`) was dead code behind it.
2. Everywhere else the setting meant "the vertical angle of an 8:9 side-by-side half" (default 52), so a full 16:9 view got only about 27 degrees vertical; the original is 46.7.
Fix: the setting is now the horizontal FOV of a 16:9 view (default 75.0 = the LIVE projection's cot 37.5 degrees, range 50-110, json `fov_h16`, the old `fov_deg` ignored) and every camera derives its vertical angle from its OWN viewport aspect (`views::fov_for`): at least 16:9 keeps the 16:9 vertical angle, narrower keeps the 16:9 horizontal angle. The strip override and floor are deleted. New camera-distance setting (`cam_dist_pct`) scales boom and height together.
Tests: `views::tests::fov_rule_per_aspect`, `the_default_is_the_originals_projection`, `no_view_shows_less_than_the_16_9_picture`; `camera_setting_tests::*` (real `follow_cameras`/`apply_fov`: 100 = today's positions, scaling, live apply, per-camera aspect, json key). Proof `shots/camfov/` (before_* vs after_* sheets). Note: the docs' "74.7 / 46.78" were an arithmetic slip; 2 atan(1/1.3032) = 75.00 and 2 atan(1/2.3159) = 46.71.
## Invisible block under the Palace stairs hid the player (2026-10-03, stairs)

Laurence: "The staircase on the main arena has a bad hitbox underneath (in the original game and in ours). Walking underneath has a thick hitbox that obscures the player when nothing visually should be."
Cause: the program's 0x94 collision fills the wedge under each Lost Palace flight with class-0x12 walls (the open side and an end wall at z -204.6) while the render has only the thin slab and the open floor there. A body stopped at |x| 81.7, and the camera's wall pull-in (it ray-casts the collision) was pinned 11 u from the head under the flight (boom 11.2 of 61.6), so the player filled the view or was grained away by the near-wall dissolve. The original has the same walls; this is a requested departure.
Fix: `stairs_under.rs` load-time filter (default ON, `PD_STAIRS_ORIGINAL=1` restores): the walls go, a short pocket wall stays where the soffit is under a fighter's height. Tests `stairs_under::tests::*` (`a_body_can_walk_under_the_flight_where_there_is_headroom`, `the_low_pocket_stays_closed`, `the_stairs_still_walk_up_with_the_wedge_opened`, `the_camera_ray_no_longer_meets_the_invisible_end_wall`). Proof `shots/stairs/stairs_before_after_sheet.png`.
## 4K never offered on a 4K TV; the game did not detect the monitor (2026-10-03, settingsux)

Laurence (55-inch 4K TV): "Max resolution did not detect or allow for 4K on a 4K TV", "the game does not automatically detect the monitor settings, it must", "change the 'press a to confirm settings'".
Causes found in the code (his TV's actual report is UNCONFIRMED: the new `display_log.txt` will say; each of these alone loses 4K, and the first two match a 4K TV left at a 1080p desktop or used as a second screen):
1. **The lists were cut to the monitor's current desktop.** `resolution_options` offered a window only sizes `<= Monitor::physical_size()` (the desktop mode now), so a TV whose desktop is 1920x1080 lost 3840x2160 although it reports 4K video modes. Exclusive also only offered the 21 fixed candidate sizes that matched a mode, never a mode the TV reports that is not on that list. Now: exclusive lists every reported mode, windowed everything up to the biggest mode (`MonitorInfo::max_extent`).
2. **The monitor never followed the window.** `track_monitor` re-read the monitor only on `Changed<Monitor>` or `Added<OnMonitor>`. Bevy re-inserts `OnMonitor` when a window is dragged to another screen: a change, not an addition. A game that opened on the laptop panel (2880x1800, max for every list) and was dragged to the TV kept the laptop's lists for the whole run. Now it re-reads every frame (`track_monitor`, test `track_monitor_follows_the_window_to_another_screen`).
3. **Nothing detected anything on first run.** The default was a 1600x900 window; on a 300 % scaled 4K desktop that is 4800x2700 physical (the window size is read as LOGICAL by bevy_winit's `create_window` although `WindowResolution::new` is physical), bigger than the screen. First run is now borderless fullscreen at the desktop's physical size, and a windowed "native" size is fitted to 90 % of the desktop (`display::fit_window`).
4. **The confirm flow.** Mode and resolution were staged, then A / an Apply row started a 10 s "keep these settings?" countdown. Replaced by browse-then-A (the choice shows dimmed, A applies at once; leaving the row drops it). `KeepPrompt` and its tests are gone.
Also: a `--screenshot` run used to save `settings.json` into the shared cache folder (the main checkout's file read exclusive / 21:9 / HUD 75 % / FOV 110 when this lane started, which no player picks; the likely writer is a lane's capture, UNCONFIRMED). Capture runs no longer write it, and `PD_SETTINGS=<file>` redirects a test run.
Tests: `a_4k_tv_lists_and_switches_to_4k_whatever_its_desktop_or_scale` (desktop 4K and 1080p x scale 1, 2, 3), `monitor_info_keeps_physical_pixels_and_the_scale_separately`, `the_default_window_is_fitted_to_the_screen`, `a_first_run_is_borderless_native_auto`, `settings_file_round_trip_...`; `pause_menu_input_tests::browsing_a_resolution_shows_it_and_a_applies_it_at_once`, `leaving_a_browsed_row_drops_the_choice_and_changes_nothing`.

## Player 2's own orbs drawn as opponent orbs (2026-10-02, capsulep2)

Laurence: P2's orbs look like opponent orbs and walking over them says nothing.
Cause: `shadows.rs::tag_cameras` REPLACED each camera's render-layer mask with
`[0, 1+view, CAMERA_LAYER(6)]`, dropping the ghost layers `fx::camera_layers`
had set; and the capsule ghost layers were numbered 5..8, so ghost layer 6
(player 2's capsules as others see them) equalled `CAMERA_LAYER`: seat 2's own
camera drew its own capsules in the opponent look (pink, dark) on top of the
own look, while seat 1 happened to see seat 2's ghost correctly. The older
test only checked the helper, not the mask after `tag_cameras`. Only with
shadows on (default; `PD_SHADOWS=0` hid it). The pick-up prompt
(`hud.rs` Bind::Capture, `capsule_in_reach(i, ..)`) and capture rule are per
seat already and were not wrong; the sim owner data was right.
Fix: `tag_cameras` adds the light layer to the existing mask; ghost layers
moved to 40.. (`GHOST_LAYER_BASE`). Teams: only your own capsules are own
(UNCONFIRMED whether the original shows a teammate's as own). Tests
`tagged_cameras_keep_own_orbs_own_and_see_others_as_ghosts` (2-4 seats),
`layer_numbers_never_collide`. Proof `shots/capsulep2_before_t0300.png` vs
`capsulep2_after_t0300.png` (P2 half: pink -> orange attack orb).
## Pause menu ignored the pad; D-pad opened skill info (2026-10-02)

Laurence (controller): "Player 1 can't use the pause menu with the d-pad or
stick; the d-pad opens info about the bound moves."

- **Cause 1** (`menu.rs::menu_input`): navigation read only "the first pad
  Bevy lists" (`input::read_repeating(0, ..)` -> `pads.iter().nth(0)`), D-pad
  only, not the pad that holds seat 1 and not any other seat; the left stick
  was never read by any menu.
- **Cause 2** (`main.rs::gather_input`): the D-pad -> skill-info view
  (`hud::SkillInfoView`) and the pending sim inputs were filled every frame,
  paused or not, so the D-pad lit the bound-skill panel over the pause menu.
- **Fix**: `input::read_pause` reads the keyboard and EVERY connected pad's
  D-pad and left stick (`stick_dirs`, deadzone 0.5, one direction per push)
  with the shared hold-to-repeat; `read_seat`/`held` gained the stick too.
  `gather_input` calls `silence_while_paused` (menu open, or a seat's pad
  lost): skill info, free-camera look and pending inputs are cleared, so
  nothing leaks into the sim (which does not step while paused anyway).
  Menu cue sounds are unchanged (move on up/down, unpause, back, confirm).
- **Pinned by** `main.rs::pause_menu_input_tests` (real `gather_input` +
  `menu_input` with mocked pads: P1 pad that is not first in query order,
  stick deadzone and repeat, P2 pad, keyboard, closed-menu control) and
  `input::tests::stick_has_a_deadzone_and_picks_one_direction`.

## Strafe/sidestep series (2026-09-27 â†’ 2026-09-29, closed)

1. **Sidestep kept looping under a held stick** (2026-09-27). The TURN_*
   clip looped instead of playing once. Cause: no sequencing -- the strafe
   clip played like any other looping clip. Fix: `strafe_sequence`
   (TURN_* once, then looping WALK/RUN). Proof
   `shots/strafe_sequence_sheet.png`.
2. **Body faced the lock-on target throughout the strafe** (2026-09-27).
   Cause: continuous strafing kept the sim's target-facing yaw. Fix: after
   the pivot the rendered yaw eases to the stick direction (sim yaw
   untouched -- aim still on target). Proof
   `shots/strafe2_facing_sheet.png`.
3. **Sidestep faced away, spun, flicked back at the run** (2026-09-28).
   Cause: the TURN_* clips bake their own hip yaw; rotating the model on
   top double-counted the turn. Fix: cancel each clip's measured baked
   curve exactly (`pivot_model_yaw`). Tests
   `pivot_facing_starts_exactly_at_the_pre_strafe_facing`,
   `pivot_facing_is_continuous_through_the_delayed_turn`,
   `pivot_facing_handles_a_decisive_backward_direction_without_spinning`.
4. **Strafe-to-run transition snapped** (2026-09-29). Cause: the hand-off
   re-based the model yaw by up to ~90 deg in one frame while the clip
   only cross-fades over 5 frames. The face log hid it by assuming the new
   clip's baked yaw immediately. Fix: fade the correction over the same
   window (`handoff_baked_yaw`); the log now mirrors the fade. Tests
   `handoff_baked_yaw_fades_from_clip_end_to_gait_yaw`,
   `handoff_blend_keeps_the_rendered_facing_continuous` (shown to Laurence
   as a before/after sheet at the time).
5. **Turn timing, three rounds same day** (2026-09-29). First the turn came
   ~1 s too late (held opponent-facing through the whole sidestep, turned
   after); then too early (turned from the first tick); settled at a 0.4 s
   hold after first input, then the turn (`STRAFE_TURN_DELAY_SECONDS`).
   Test `pivot_holds_the_pre_strafe_facing_until_the_turn_delay` pins the
   hold; `delayed_pivot_turn_settles_on_the_movement_direction_without_jumps`
   pins the full sequence for all 8 direction/gait pairs. Proof
   `shots/turn_sheet.png`.
6. **Settled backward run flicked back and forth** (2026-09-29). Pulling
   straight back sits exactly on the BackRight/BackLeft split; thumb
   tremor flipped it every few ticks and each flip replayed the TURN_*
   pivot. Fix: same +/-5 deg hysteresis as the forward/strafe split
   (`LOCK_STRAFE_SIDE_BAND`, persisted `Player::strafe_right`) --
   deterministic, sim stays netcode-clean. Test
   `locked_strafe_side_split_has_hysteresis_against_stick_jitter`.
7. **Firm diagonal holds spasmed on both sides** (2026-09-29, same day).
   Same disease one boundary over: a held 135Â°/225Â° sits exactly on the
   back-cone line (quantised diagonal normalises to one ulp past -âˆš2/2),
   so float dust flipped Right/BackRight nearly every tick -- each flip a
   fresh TURN_* pivot with ~0.6 rad facing jumps (a static scripted 135Â°
   run did it for 80+ ticks). Fix: hysteresis on the cone too
   (persisted `Player::strafe_back`: back past 140Â°, revert within 130Â°).
   Test
   `locked_strafe_back_cone_boundary_has_hysteresis_against_float_dust`.
8. **The turn itself read as a flick/spasm** (2026-09-29, same day).
   Laurence's footage ("Run spasm.mp4") shows a single hold-then-whip and
   a steady run after -- no ongoing oscillation, so the two facing rules
   are not fighting; all the energy is in the 0.15 s whip (sim 600 deg/s).
   Fix: the rendered body turns at its own 180 deg/s
   (`STRAFE_VISUAL_TURN_RATE_DEG_PER_S`; sim rate untouched), and the
   hand-off branch pursues on the rendered level so the slow turn stays
   monotonic while the model absorbs the clip fade (chasing
   movement-minus-baked with the slow rate lagged the fade and detoured
   mid-blend -- caught in the face log before shipping).
9. **The spasm: pivot replayed on every direction change** (2026-09-29,
   same day). Mid-strafe crossings replayed the TURN clip, the 0.4 s hold
   and the turn from scratch (our interpretation -- the program's per-tick
   pivot function never restarts TURN; its offset is chosen at strafe
   start). A wobble repro showed 14 replays with 0.6-2.4 rad jumps and RUN
   never starting. Fix: the pivot plays once per strafe engagement (clip
   and facing clock keyed to the entry direction); the carried facing
   retargets smoothly to the live stick. Wobble repro after: one TURN,
   RUN takes over, zero facing jumps. The face log had to follow the entry
   key too -- it printed phantom jumps across flips otherwise.
10. **Stick-release flicker after a backward run** (2026-09-29, same day).
    Stopping used the exponential smoothing, whose first step spanned ~54
    deg back to the sim yaw after a full reversal (1.37 rad measured in the
    face log the frame the stick was released) -- a 1-3 frame flicker with
    no motion, clip or facing-trace anomaly behind it. Fix: the strafe exit
    paces the return at the visual rate (`strafe_return_step`, until
    arrived); casts keep the fast path. Test
    `strafe_exit_return_is_rate_limited_not_exponential`; scripted
    press-release shows max one visual step per tick through the return.
11. **PARKED 2026-09-29: one-frame flicker left on run-backward (Know).**
    Everything below measured clean, so this entry is the restart point,
    not a fix. Ruled out, each with evidence: sim motion/gait (steady in
    scripted holds), clip sequencing (one TURN then RUN, anim log), facing
    (monotonic, face log), RUN loop wrap (dead flat at the wrap tick),
    fighter-specific curves (J.D's files identical to the tables, and he
    spasms too), footsteps (same code path both gaits), the TURN-RUN morph
    window (Bevy blends multi-frame by construction), camera boom (dives
    gradually, stays pinned -- no return snap), wall pinning (reproduces in
    clear space), lock-off control (clean -- it IS strafe-specific).
    Shrunk across the day from an ongoing oscillation (pivot replay on
    every crossing -- fixed by 9 above) to a multi-frame release snap
    (54 deg first step -- fixed by 10 above) to a single frame. The
    remaining suspects nobody has measured: the exact release-frame pose
    blend, per-frame (not per-tick) render behaviour at 60 fps+ in live
    play, anything live-only the scripted harness cannot produce. Reopen
    with fresh footage on the build of the day plus which release/hold
    produces it.

## Character select: preview models doubled up (2026-10-03, lane infomusic)

Laurence: "The player models in the player selection screen sometimes get duplicated on top of one and other."

**Cause.** `frontend/charselect.rs` spawned each seat's preview model with no `DespawnOnExit`, although its header comment says the screen's entities go with it. So (1) leaving the setup screen left every preview model alive (in a match the seat-0 one stood at the stage origin, the others 4000 u away per seat), and (2) coming back to the screen restarted the models' generation counter at 1 and spawned fresh models on the same spots as the old ones: two idling copies of the same fighter in one window, until the seat changed its pick (a reveal removes only models with another generation number, and the leftover could even share the number). "Sometimes" = from the second visit to the screen (a match, a result, the arsenal editor and back).

**Reproduced first** with scripted picks through the real systems (`charselect::lifecycle_tests`, a headless app with no assets: models wait out their load timeout, then show): before the fix `leaving_the_screen_takes_its_models_with_it` (1 model left after leaving) and `coming_back_to_the_screen_never_doubles_a_model` ((2, 2) on the second visit) failed; fast back-and-forth changes within one visit already settled on one model.

**Fix.** The preview model carries `DespawnOnExit(AppState::Setup)`; the generation number now comes from one process-wide counter, never repeating across visits. Tests (all five in `lifecycle_tests`): one model per joined seat; leaving removes them; three visits with a pick change each stay at one model per seat; 60 fast changes of two seats (characters and costume colours, a seat dropping out and back) end on exactly one model each with the last pick; a pick changed while the shown model is still loading keeps the shown one up.

**Music (not a crash, a wrong behaviour; same lane):** one WAV per stage looped whole, so the intro repeated every time, the stage -> track map was a guess (10 of 15 family stages wrong), no menu or result music. Details and the READ tables: PIPELINE.md "Music (lane infomusic)".
