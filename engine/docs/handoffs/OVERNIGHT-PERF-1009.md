## Handoff: overnight performance work (2026-10-08 night -> 10-09 morning)

**Goal / scope:** Laurence: "continue overnight until all performance issues are fixed" (frame drops when casting, 4-player frame rate, stutters). Downloads for analysis allowed; research allowed. Do NOT push without Laurence. Do not change G-Helper / system settings. Keep the laptop on mains (battery throttles the GPU). Do not block the OLED-care screensaver (it protects the panel).

**Done (and how it was verified):** 4 players, the same deterministic 1080p fight every run, ms/frame mean | p99:

| commit | change | 4P mean | 4P p99 |
|---|---|---|---|
| (start) | main after wave 1008 merge | 61.5 | 141 |
| 4724560 | effect per-instance rows (`effects/inst.rs`) + aura effect preloaded | 41.2 | 88 |
| 6b6d50f | stage light set in one shared storage buffer | 35.8 | 75 |
| 014704c | overlay cameras (settings menu, dev tools) sleep while empty | 31.6 | 36 |
| c5da577 | fighter pieces share one dissolve material per (slot, source material) | 24.7 | 33 |
| (07:30 re-measure, quiet machine) | same build | 21.3 | 28 |
| 4d74ea2 | light clustering on the CPU (no clustered lights exist) | 19.8 | 24 |
| 5c0ad60 | each camera's Core3d/Core2d schedule single-threaded | -1.8 ms (A/B with screensaver: 21.7 -> 19.95) | |
| 5760657 | main thread: attach_fragments, auto NotShadowCaster | no wall change (main thread is not the bottleneck) | |
| 9015669 | aura wisp bodies on rows | -0.9 ms (20.0 -> 19.15) | |
| fa81b6d | particles on rows (FxMaterial mode 0) | -0.5 ms (19.5 -> 19.0) | |
| ead93a0 | `play` profile for Laurence's copy | 17.8 (median 16.7 = the 60 fps cap; render thread 15.0 ms) | |
| 5163d4c | shadow cascades 2 per light from 3 views (Laurence picked B) | quiet machine, final PLAY copy: **16.67, locked 60 fps** (render thread 12 ms) | 17.4 |

From 07:58 the OLED screensaver ran (adds ~1.8 ms and periodic 30-60 ms spikes), so later rows are interleaved A/B deltas, not absolute numbers. Every step: pd_game tests pass (1222 at the end), pictures checked (fixed-tick captures old vs new within rerun noise; forced-aura captures byte-identical; footstep puffs within 2/255 with particle rows, accepted in DECISIONS).

**Key decisions:** DECISIONS.md 2026-10-09 rows (play profile for Laurence's copy; CPU clustering + single-threaded view schedules, Render schedule stays multi-threaded until measured idle; particle rows despite the 2/255 puff difference; screensaver not blocked).

**Files touched:** `engine/Cargo.toml` (profile `play`; a thin-LTO `play-lto` was tried and removed: 17.7 vs 17.8 ms), `crates/pd_game/src/main.rs` (`cpu_light_clustering`, `single_thread_view_schedules`), `effects.rs` (`attach_fragments`, `PartMats`, `particle_look`, `tick_particles`), `effects/aura_wisp.rs`, `effects/inst.rs`, `effects/warmup.rs` (particle variants), `shadows.rs` (`skip_non_casters`), `footsteps.rs`, `fx.rs`; docs PERF / BUG-GRAVEYARD / DECISIONS / NEXT-SESSION / AGENTS, skill `pd-build-run`.

**How to measure (scripts in `engine/shots/n1009/`, gitignored, kept):** `bench.ps1 -Exe <exe> -Label <name> [-Players 4] [-Reps 2] [-Env "PD_X=1;PD_Y=0"]` (env `PD_SHOT_TICKS=99999`, `PD_FRAME_STATS=2400,900,25`, `PD_WINDOW=960x540`, `PD_TEST_ASPECT=auto`); `pics.ps1 -Exe <exe> -Label <name> [-Players 2] [-Ticks "300,900,1500"] [-Env ...]` then `python -I diff.py <labelA> <labelB> [players] [--sheet out.png]`; `tracy.ps1 -Name <n>` (tracing build in `%USERPROFILE%\.pdtools\target-tracy`, `--features bevy/trace_tracy`) then `agg_tracy.py`, `selftime.py --thread-of 'sub app{name=RenderApp}'`, `nest.py <csv> 'camera_schedule{camera="Camera 1'`. Interleave A/B runs while the screensaver may be running.

**Dead ends (reverted, do not retry blindly):**
- `PbrPlugin { use_gpu_instance_buffer_builder: false }`: stage and fighters vanish (custom materials need GPU preprocessing data).
- `ClusterConfig::None` on player cameras: wgpu validation error, app quits. (Turning GPU clustering off globally is the working route: 4d74ea2.)
- `NoAutoAabb` on glTF meshes: no wall-time gain.
- `WGPU_BACKEND=dx12`: 33 ms vs 19.8 (Vulkan stays).
- `ExtractSchedule` single-threaded: no gain.
- Light decals must keep their own materials (stage/fighter light read their colour; they are off by default anyway: `PD_LIGHT_DECAL=1`).

**Risks identified:** particle rows change footstep puffs by up to 2/255 (mean zero; cause not pinned down, probably blend order of overlapping puffs): `PD_FX_INST_SKIP=particle` restores byte-identical puffs. Single-threaded view schedules depend on Bevy queuing command buffers in topological order (true in 0.19: `RenderContextState` is a system buffer applied in system order); re-check on a Bevy upgrade.

**Remaining work / next concrete step:**
1. Laurence plays 3-4 players (casting especially) on the refreshed copy.
2. Re-measure `PD_RENDER_ST=1` on an idle machine (no screensaver); adopt if it still wins.
3. Shadow cascades: DONE (Laurence picked B, 5163d4c).
4. Bigger: vendored Bevy patch (like `vendor/bevy_gilrs`) to cache per-view preprocess bind groups and merge each camera's command buffers (render thread: 32 views a frame = 4 cameras + 28 shadow cascades; `queue_submit` of ~44 command buffers ~2.5 ms traced).
5. Thin LTO: measured, no gain, removed.
