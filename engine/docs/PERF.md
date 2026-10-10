# Performance (lane perf 2026-10-03, merged; round 2 lane fxperf 2026-10-08; round 3 overnight 2026-10-09 at the end; MEASURED on the PC 2026-10-08, see "PC measurement 2026-10-08" at the end)

## Method and tooling
- `perf_probe.rs`: `PD_FRAME_STATS=<frames>,<skip>` (mean / p99 / max, now WALL clock via Instant), render-thread split by phase, material churn counters; `PD_GPU_TIMING=1` for GPU passes.
- Scenario: `<cache> --players N --near --demo --screenshot x.png --shot-frame 999999`, `PD_WINDOW` (logical, x2 here: 960x540 = 1080p, 1920x1080 = 4K), `PD_TEST_ASPECT=auto`.
- Scripts (gitignored, in `engine/shots/perf/`): `bench.ps1`, `agg.py`, `shot.ps1`, `diff.py`.
- Do NOT use `PD_SHOT_TICKS` for timing: the clock is fixed so it is deterministic, but 4K runs then show a ~30 ms swapchain stall (prepare-views).
- Conditions were poor: laptop ON BATTERY (53%), other lanes running; even 1P idle at 4K sat at ~46 ms (prepare-views stall) in this session, whereas it was 17 ms earlier. Absolute numbers are unreliable; compare before/after only.

## What each change does
1. Fighter pieces (`cam_fade.rs`, `char_light.rs`, `dissolve.wgsl`): coverage, grain and the character light moved out of every piece's material into ONE shared storage buffer (`FighterBlock`), written in place when it changes (`upload_fighter_block`). Fighter material rewrites per frame: ~100 -> 0.
2. Effect pieces (`effects.rs`, `fx_gamma.wgsl`): `animate_pieces` computes its values, and writes the material only if one differs (`PieceWrite`); other per-frame colour writers (blades, light decals, trails, ms_particle, aura_wisp, tick_particles) compare before writing. `FxGamma`'s seven uniform buffers merged into one (`FxUniform`, binding 100): fewer GPU buffers per re-prepared material.
3. Not changed: shadows (stage extents are ~1300 units, so no pixel-identical cascade trim exists; `PD_SHADOW_CASCADES_TRIM` stays opt-in).

## Measured (median of 3, demo, mean ms; before -> after; battery, noisy)
- 4P 1080p: 60.0 -> 48.6 (p99 148 -> 95); prepare-assets 16.9 -> 9.2
- 3P 1080p: 33.5 -> 29.4 (p99 86 -> 49)
- 4P 4K: 109 -> 102 (stall-dominated); prepare-assets 25.2 -> 6.6
- 2P: 20.1 -> 22.2, 1P 1080p 16.8 -> 28.7 (noise/stall; no effects there, not a regression I can explain - unverified)
- Pictures: fixed-tick captures (2P and 4P, tick 300) are BYTE-IDENTICAL before/after; with effects (tick 900/1500) old-vs-new differ 1.4-2% of pixels, same as old-vs-old reruns (0.2-5%), i.e. within noise (effect randomness).

## Unverified / remaining
- Tests: last full run passed in all crates seen (pd_game 1078 passed, others ok); new tests: `the_fighter_block_*`, `upload_writes_the_block_once_per_change`, `a_piece_write_*`, `an_unchanged_piece_write_raises_no_modified_event`.
- Still short of 60 fps in heavy 4P fights: ~100+ effect materials genuinely change each frame; `render(encode)` ~18 ms (unbatched, one material per piece, per view).
- Next: share effect materials + per-instance buffer (MeshTag index) so pieces batch; re-run table on mains power; shadow cost measured with `PD_SHADOWS=0` on the new build; WAVES.md entry not yet written.

## Round 2 (2026-10-08, lane fxperf) - casting hitches (measured on the PC the same day: see the last section)

Laurence: "the effects are great but they cause frame drops when casting moves" and "implement any other performance enhancements that we haven't yet". Built in a cloud container with no GPU and no game data: every change below is picture-identical BY CONSTRUCTION (tests pin the new code to the old), each has an off switch, and every speed claim is UNCONFIRMED until the PC recipe below is run. Builds on the effect preload (BUG-GRAVEYARD "Frame drops when casting", `PD_FX_PRELOAD`).

### What each change does
1. **Material cache and pool** (`effects/mat_cache.rs`, switch `PD_FX_MATCACHE=0`). A cast used to create a new effect material for every piece, trail, blade, flare, point, puff and MSPARTICLE particle it spawned and free it when it faded; each creation and each free is work for the render thread (a uniform buffer and bind group made, later torn down). Per site, read from the code, what happens after spawn:
   - never written again: flares (colours are vertex colours) and MSPARTICLE ribbons (white, vertex colours) -> **shared**: one material per exact value (the key is the material's full `Debug` text: every texture, alpha mode, blend, cull, colour and uniform), so two spawns share only when the materials they would have made are equal field for field;
   - written every frame per instance: pieces (`animate_pieces`), trails, blades, light decals, points, puffs, MSPARTICLE quads -> **pool**: when the last user of a material has gone (the pool holds the only handle left, checked once a frame in `First`), it is filed under its static key (textures, alpha mode, cull, sidedness, transmission, the `FxKey` blend, shader family); the next spawn with the same key takes it and overwrites the WHOLE material with the fresh value it would have created. Two live users never share. All reuse happens in `Update`, before Bevy publishes asset events, so the reset reaches the render thread in the same frame as its new user (and Bevy drops a modified material's old GPU copy before preparing the new one).
   - aura wisp bodies: unchanged (created in `PostUpdate`, where a reused material's reset could arrive a frame late; they live for the whole effect anyway).
   - Not done: the per-instance storage buffer (MeshTag index) that would let pieces batch and stop re-preparing materials whose colour changes every frame. It means rewriting `fx_gamma.wgsl`'s colour/UV path, which cannot be checked without a GPU. The pool saves the create/free churn, not the per-frame re-preparation of animated materials (PERF round 1 already skips unchanged writes).
2. **Meshes rewritten in place** (`effects/mesh_reuse.rs`, switch `PD_FX_MESHREUSE=0`). Blades, ray bursts and MSPARTICLE ribbons used to build a brand-new mesh every frame for every live one, and aura wisps a new position list. Now the same vectors are refilled in place (same values, same order), and a mesh whose inputs did not change since its last write is not touched at all (a blade after its swing, a burst with no rays left, a ribbon or wisp that did not move, a paused game), so it is not copied and uploaded again. Tests compare the in-place writers with verbatim copies of the old builders.
3. **Pipeline warm-up** (`effects/warmup.rs`, switch `PD_FX_WARMUP=0`). The first draw of a new material variant (alpha mode, blend equation, cull, scene-copy use) with a given vertex layout makes Bevy compile a render pipeline; until it is ready the draw is skipped, so a skill's first cast could show late and the compile competes with the frame. When a match's effects are preloaded, the warm-up spawns one draw per variant those effects can use: the effect's own material recipe (the spawn sites' code was factored into `piece_fx_material`, `plain_add_material`, `point_material`, `puff_material`, `ms_particle::ribbon_material` / `quad_material`; a test pins them to the old inline literals) on a copy of the effect's own mesh (pieces: the glb's meshes under that node) with every vertex moved to one point. Same pipeline, but every triangle has zero area and covers no pixel in any pass; the draws are removed after 60 frames, and a variant is warmed once per run. Needs `PD_FX_PRELOAD` on (it warms the preloaded set).
4. **Small per-frame waste** (no switch: pure caching). `effects::env_once`: the effect switches (`PD_FX_*`, `PD_LIGHT*`, `PD_MSP*`, `PD_POINT_BILLBOARD`, `PD_AURA_WISPS`, `PD_WISP_LOG`, ...) and a few per-frame reads in `fx.rs` (`PD_FX_LOG`, `env_f32`), `char_light.rs` (`PD_LIGHT_SCALE`, `PD_CHAR_EFFECT_LIGHT`) and `hud.rs` (`PD_TEST_HEALTH`, `PD_TEST_PAD_LOST`) are read from the environment once per run instead of per frame / per piece / per particle. Nothing in the game sets them; switches that tests toggle at run time (`PD_CAMFADE`, `PD_HITSTOP`) were left alone. Emitter, blade and flare specs are shared (`Arc`) so the per-frame copy `tick_sprites` / `update_blades` / `update_flares` take is a counter bump, not a string copy. Nothing O(n^2) was found in the effect updates.
5. **Measurement**: `PD_FRAME_STATS` now also prints `[fx-churn]`: effects started, Fx materials created (per effect, and the most in one frame), freed, the cache's made / reused / shared counts, meshes created / rewritten / freed per frame, warm-up draws.

### PC measurement recipe (not run yet)
Same scenario as round 1 (`engine/shots/perf/bench.ps1` or by hand), ON MAINS POWER, nothing else compiling:
```
$env:PD_FRAME_STATS="1800,200"; $env:PD_WINDOW="960x540"; $env:PD_TEST_ASPECT="auto"
.\target\debug\pd_game.exe "$PWD\cache" --players 4 --near --demo --screenshot shots\perf\x.png --shot-frame 999999 2> shots\perf\fxperf_new.txt
```
Then the same with everything off (`$env:PD_FX_MATCACHE="0"; $env:PD_FX_MESHREUSE="0"; $env:PD_FX_WARMUP="0"`, -> `fxperf_old.txt`), and one run per single switch off to see which one helps. Three runs each, compare the medians of mean / p99 / max frame time, `prepare-assets` / `prepare-meshes` / `specialize` in `[render-split]`, and `[fx-churn]` (with the cache on, "Fx materials created per effect" and "freed" should fall and "reused" / "shared" rise; with mesh reuse on, "meshes rewritten" should fall). For the first-cast hitch specifically: `PD_FRAME_STATS=600,0` (no warm-up skip) and compare `max` with and without `PD_FX_WARMUP=0`. Repeat at 2P and 1080p / 4K if time allows.

Picture check (all switches must be invisible): fixed-tick captures as round 1 (`--screenshot ... --shot-frame 300` and `900` with `PD_SHOT_TICKS`, 2P and 4P) with defaults vs with the three switches at 0, diffed with `shots/perf/diff.py` (expect byte-identical at tick 300; effect frames within the old-vs-old noise); and a film-compare sheet pair: `python engine\re\film_compare.py run --ids 147,102,3 --real --redo` once with `PD_COMPARE_OUT=...\fx_new` and once with the switches at 0 and `PD_COMPARE_OUT=...\fx_old`, then look at the two sheets side by side (blade, flares, MSPARTICLE smoke and an aura).

### Tests (Linux container)
1185 passed (1169 at the start + 16 new), the 4 known Linux-only failures (3 `combat::tests::replay` goldens, `sfx` footstep cache) unchanged. New: `mat_cache::tests::*` (5: shared keys never collide across textures / blends / alpha / cull / colour / uniforms, static keys, live pooled materials never share, a released material is reset to the fresh value, switch off = a new material each spawn), `mesh_reuse::tests::*` (2), `blades_/flares_rewritten_in_place_match_the_old_builder`, `ms_particle::tests::ribbons_rewritten_in_place_match_the_old_builder`, `aura_wisp::tests::posing_into_a_used_buffer_gives_the_fresh_pose`, `warmup::tests::*` (3: collapsed meshes keep layout and have no area, a collapsed billboard stays one point, warm variants use the effects' own recipes), `the_factored_material_recipes_make_what_the_inline_ones_made`, `an_environment_switch_is_read_once_per_run`. `pd_viewer` (shares effects.rs) checks clean.

### UNCONFIRMED / open
- Any frame-time gain (not measured). The likely biggest one for the "first cast" hitch is the warm-up; for repeat casts, the pool / shared cache and mesh reuse. If casts still hitch: bisect with the switches above and `PD_FX_OFF`, and look at `specialize` / `prepare-assets` in `[render-split]`.
- That the warm-up covers every variant: pieces whose glb node has no mesh, meshes with morph targets or skins, and draws whose look depends on runtime data the warm-up cannot see (a puff's texture alpha: both variants are warmed) may still compile on first use.
- Next (not done): the per-instance buffer for effect materials (batching); shadow cascade cost (`PD_SHADOWS=0` on the new build).

## PC measurement 2026-10-08 (mains power, nothing else running; wave 1008 merge on main)

Scenario: `--players N --near --demo`, `PD_WINDOW=960x540` (1080p), `PD_TEST_ASPECT=auto`, median of 3 runs. Script and logs: `engine/shots/w1008/bench.ps1`, `shots/w1008/perf/` (gitignored).

**Old vs new exe** (Laurence's previous PLAY copy, main before perf round 1, vs the merged build with every switch at its default), `PD_FRAME_STATS=500,300`, mean / p99 / max ms:

| players | old | new |
|---|---|---|
| 1 | 16.7 / 19.7 / 20.9 | 16.7 / 17.7 / 18.8 |
| 2 | 17.2 / 21.3 / 24.4 | 17.1 / 20.6 / 21.2 |
| 3 | 32.1 / 90.3 / 105.1 | 26.6 / 53.7 / 66.2 |
| 4 | 77.3 / 171.8 / 197.6 | 52.5 / 105.0 / 128.8 |

So rounds 1 + 2 together: 4 players about a third faster on average and the worst frames ~35 % shorter; 1-2 players sit at the 60 fps cap either way.

**Round 2 switches A/B** (same new exe, `1800,200`), 4 players mean / p99 / max: all on 46.8 / 94.7 / 116.2; all off 46.0 / 83.2 / 108.0; preload off 50.2 / 102.5 / 138.7; matcache off 42.2 / 86.3 / 114.2; meshreuse off 47.1 / 99.8 / 127.5; warm-up off 46.8 / 105.9 / 134.0. 2 players: 16.7 mean in every case (capped). **Verdict: within run-to-run noise** (the same config spread 36-51 ms mean across its three runs: the demo's CPU fights differ run to run, e.g. 46 vs 129 effects started in the same 1800 frames). Preload off was the worst in all three runs (weak signal it helps). Nothing got worse, so all switches stay ON.
- `[fx-churn]` confirms the cache works: all on = 591 Fx materials created / 4086 reused / 0 freed; all off = 16383 created / 16348 freed.
- **First-cast hitch** (`600,0`, 4P): max ~560-630 ms in every config (warm-up on, off, all off). That max is the match-start frame, not a cast, so this recipe cannot isolate the first cast; UNCONFIRMED whether the warm-up removes a first-cast hitch. Laurence's play-test is the real judge.
- Remaining cost: 4P is still ~50 ms (20 fps) at 1080p; the next step is still the per-instance effect buffer (batching), below.

**Picture identity:** fixed-tick captures (2P / 4P, ticks 300 / 900 / 1500) old exe vs new: 0.07-1.76 % of pixels differ; new vs new rerun: 0-1.74 % (effect randomness), looked at side by side, same picture (`shots/w1008/pic/old_vs_new_t0900.png`). `film_compare.py run --real` on 12 effect-heavy skills (75 Rapid Cannon, 64 Photon Burst, 10 Bomb, 4 Psycho Blade, 62 Laser Blade, 122 Lightning Sword, 147, 102, 3, 1, 193 Aura Leak, 321 Excaliber) with every switch on vs every switch off: identical numbers (0 mismatches both), sheets 0.02-2.7 % pixel noise, looked at (`shots/w1008/rapid_cannon_off_vs_on.png`). No regression, no switch turned off.

**Prop Hunt disguise (2026-10-08, Laurence: "FPS drops when becoming a prop and moving around as a prop", on battery):** NOT reproduced on mains. 2-player Prop Hunt countdown, hider walking, `PD_FRAME_STATS=1100,100,25` (third number = log every frame over 25 ms with its tick): plain hider mean 16.87 / p99 21.6 ms, disguised hider 16.90 / 19.2 ms; no spike at the frame the disguise appears (tick 400-410). Both runs show the SAME background hitches, 30-105 ms every few seconds whether disguised or not (also with no Prop Hunt): an existing, unexplained stutter worth hunting next (`[frame-spike]` lines). Battery mode makes every frame slower on this laptop (round 1 notes).

## Round 3: overnight 2026-10-08/09 (PC, mains power; handoff `handoffs/OVERNIGHT-PERF-1009.md`)

Laurence: "continue overnight until all performance issues are fixed" (frame drops when casting, 4-player frame rate, stutters). Every step is a commit on main with its own off switch; none pushed.

### Method (the deterministic bench; scripts in `engine/shots/n1009/`, gitignored)
- `bench.ps1`: env `PD_SHOT_TICKS=99999` (fixed 60 Hz sim clock per frame = the SAME fight every run), `PD_FRAME_STATS=2400,900,25` (wall-clock stats over 2400 frames after skipping 900, log frames over 25 ms), `PD_WINDOW=960x540` (1080p), `PD_TEST_ASPECT=auto`; args `<cache> --players 4 --near --demo --screenshot x.png --shot-frame 999999`. Prints `[frame-stats]`, `[render-split]`, `[cpu-split]`. Repeatable to ~0.3 ms on a quiet machine. (Round 1's warning about `PD_SHOT_TICKS` was the 4K swapchain stall; at 1080p it is the best A/B there is.)
- `pics.ps1` + `diff.py`: the same env with `PD_SHOT_TICKS=300,900,1500`, old exe vs new exe, % of pixels differing (>0 and >8 per channel) plus a side-by-side sheet. Rerun noise of one exe against itself: 0-2.5 % (effects whose models finish loading at a different tick; whole aura wisps can appear in one run and not the other).
- `tracy.ps1` + `agg_tracy.py` / `selftime.py` / `nest.py`: Tracy 0.14 capture of the tracing build (`%USERPROFILE%\.pdtools\target-tracy`, `--features bevy/trace_tracy`), ms per frame per zone, render-thread self time, a camera's nested zones.
- **The OLED-care screensaver** (`OLED Care Screensaver.scr`) starts after a while idle and costs this laptop ~1.8 ms a frame plus periodic 30-60 ms spikes (p95 / p99 rise). It protects the panel, so it was NOT blocked: from 07:58 on, A/B runs were interleaved (old, new, old, new) and compared against each other, never against numbers taken before it started.

### Results, 4 players 1080p, ms per frame mean (p99)
| step | commit | before | after | notes |
|---|---|---|---|---|
| effect per-instance rows | 4724560 | 60.6 (138) | 41.2 (88) | night of 10-08 |
| stage light set in one buffer | 6b6d50f | 41.2 (88) | 35.8 (75) | night of 10-08 |
| overlay cameras sleep | 014704c | 35.8 (75) | 31.6 (36) | night of 10-08 |
| fighter dissolve materials shared | c5da577 | 31.6 (36) | 24.7 (33) | night of 10-08 (21.3 / 28 re-measured at 07:30 on a quiet machine) |
| light clustering on the CPU | 4d74ea2 | 21.3 (28) | 19.8 (24) | quiet machine |
| per-camera render schedules single-threaded | 5c0ad60 | 21.7 | 19.95 | screensaver; encode 10.7 -> 9.1 ms |
| attach_fragments / NotShadowCaster | 5760657 | - | - | main thread only (it is not the bottleneck): headroom, no wall-time change |
| aura wisps on rows | 9015669 | 20.0 | 19.15 | screensaver; Fx material rewrites 8.3 -> 0.6 / frame |
| particles on rows | fa81b6d | 19.5 | 19.0 | screensaver; StandardMaterial rewrites 13 -> 1.1 / frame |
| optimised `play` profile (Laurence's copy) | (docs commit) | dev 18.95 | 17.8, median 16.7 | screensaver; render thread 17.2 -> 15.0 ms; most frames at the 60 fps cap |
| thin LTO on top (`play-lto`, removed) | - | 17.8 | 17.7 | no gain worth 3x longer builds |
| shadow cascades 2 per light from 3 views | 5163d4c | 17.15 | 16.9 | dev, quiet machine (Laurence back, screensaver off); render thread 15.0 -> 14.0; Laurence picked B on the A/B sheet |
| **final PLAY copy** | 5163d4c, play profile | 61.5 (141) at the start of the night | **16.67 (17.4), locked 60 fps** | quiet machine; render thread 12 ms of the 16.7 budget; 3 players 16.67 (render 8.8 ms) |

What each change does:
1. **CPU light clustering** (`main.rs::cpu_light_clustering`, `PD_GPU_CLUSTERING=1` = Bevy's default). The game has no point / spot lights, light probes or clustered decals, yet Bevy's GPU clustering ran three compute passes and a raster pass per player view per frame and rebuilt its buffers each frame. On the CPU path with nothing to sort it is a no-op. Pictures: t300 byte-identical, rest within rerun noise.
2. **Single-threaded view schedules** (`main.rs::single_thread_view_schedules`, `PD_VIEW_MT=1` = Bevy's). Each camera's `Core3d` / `Core2d` schedule (~45 systems, mostly idle for this game: no SSAO, TAA, DoF, atmosphere...) ran on the multi-threaded executor; the per-system hand-off cost more than parallel encoding saved. Command buffers are queued in the same topological order either way. `PD_RENDER_ST=1` also single-threads the main `Render` schedule: -0.6 ms (dev) / -0.3 ms (play) with the screensaver running, NOT adopted until measured on an idle machine. `ExtractSchedule` single-threaded: no gain.
3. **Main thread** (no wall-time change today, the render thread is the bottleneck): `effects::attach_fragments` walked every named entity and its ancestors every frame (~0.6 ms); `shadows::skip_non_casters` gives every mesh no shadow light can see `NotShadowCaster`, so Bevy's 8 per-(light, view) caster walks skip the stage, props, effects and orbs (same casters by construction: the render-layer test Bevy applies is the one used to decide).
4. **Aura wisp bodies on rows** (`effects/aura_wisp.rs`, `PD_FX_INST_SKIP=wisp` = old). Each wisp body had its own material rewritten every frame (colour, alpha, scroll); now a row of the shared buffer and the shared neutral material. Forced aura (`PD_AURA_LEVEL=12`, 2P, ticks 200/400/600): byte-identical old vs new, while wisps-off differs 1-2 %.
5. **Particles on rows** (`effects.rs::PartMats` / `particle_look`, `PD_FX_INST_SKIP=particle` = own materials). Footstep puffs, prop debris and capsule sparkles owned a `StandardMaterial` rewritten every frame as they faded. Now an `FxMaterial` in `fx_gamma.wgsl` mode 0 without gamma re-encoding (Bevy's unlit StandardMaterial path), shared per static look, colour in a row; their pipelines join the warm-up. With own FxMaterials the puffs are byte-identical to the StandardMaterial ones; with rows a puff differs by at most 2/255 on scattered pixels, mean zero (not explained; most likely the draw order of overlapping translucent puffs). Invisible, but not byte-identical: recorded here.
6. **`play` profile** (`engine/Cargo.toml`): release optimisation for every crate, no debug assertions (wgpu and Bevy run extra checks in debug-assertion builds). Laurence's copy is built with it from now on.

### Final state
**Quiet machine, final PLAY copy (5163d4c, play profile):** 4 players 16.67 ms mean, p99 17.4, max 18-19 ms = locked 60 fps, render thread 12 ms (4.7 ms of headroom); 3 players 16.67 locked, render thread 8.8 ms. Film compare (75, 62, 193 `--real`): same results as before.

Earlier the same morning (play build before the shadow trim, screensaver running, so the means read high): 4 players 17.8 ms mean, median 16.7 (= the 60 fps cap: the game never runs faster than 60 even with vsync off), render thread 15.0 ms; 3 players 17.3 mean, median 16.7, render thread 11.4 ms; 2 players 16.67 locked. The frames above 16.7 ms coincide with the screensaver's periodic spikes (p95 22-28 ms). On a quiet machine 4 players should hold 60 fps, with ~1.5 ms of render-thread margin: Laurence's play-test decides. Film compare (6 effect-heavy skills, `--real`): new PLAY copy = previous PLAY copy (same mismatch list: 193 and 64 sound-only, both before and after; sheets differ only by drifting aura wisps).

### Where the time still goes (Tracy, 4P, dev build, after step 5)
Render thread ~17 ms of a ~19 ms frame (main thread ~9.5 ms, not the bottleneck). Biggest items: wgpu recording each pass at the end of its system (`RenderContextState::apply`: transparent 1.2, shadows 1.0, unpack_bins 0.7, opaque 0.7 ms traced), `queue_submit` of 44 command buffers (~2.5 ms traced), `prepare_preprocess_bind_groups` (per view, recreated every frame), `write_binned_instance_buffers`. All Bevy / wgpu internals that scale with the number of views: 4 player views plus 7 shadow cascades each (stage light 4 + fighter light 3) = 32 views a frame.
- Shadows cost ~3 ms (`PD_SHADOWS=0`: 19.8 -> 16.8 on the quiet machine). From three views each light now keeps 2 cascades (Laurence picked B on the A/B sheet `engine/shots/n1009/shadow_AB_3p.png`, DECISIONS 2026-10-09); `PD_SHADOW_CASCADES_TRIM=0` restores the full set.
- `WGPU_BACKEND=dx12` is much worse (33 ms): stay on Vulkan.
- Next ideas: patch Bevy (vendored, like `bevy_gilrs`) to cache the per-view preprocess bind groups and to merge a camera's command buffers; re-measure `PD_RENDER_ST=1` on an idle machine.
