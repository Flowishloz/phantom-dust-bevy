# Golden trace: original vs our sim, numerically

Purpose: end "feel" arguments (walk/run, strafe, turning, jumps, dodges; see `MOVEMENT-AUDIT.md`, `audits/AUDIT-JUMPS-DODGES.md`) by logging the ORIGINAL game's per-tick movement under fixed scripted inputs and comparing it with our sim under the same script. Nothing here needs game data to run the tools; the tools are tested on synthetic traces only (`engine/re/trace_synth.py`). A trace holds numbers only, no game assets, so it may be committed.

Tools: `engine/re/trace_compare.py` (report), `engine/re/trace_synth.py` (fake traces + input scripts), `engine/re/test_trace_compare.py` (tests: `cd engine/re && python3 -m unittest test_trace_compare`). Stdlib only, Python 3.8+.

## 1. File format

Plain UTF-8 CSV. Leading `#` lines are header (`# key: value`), then one column-name line, then one row per ORIGINAL game tick (30 Hz). One file may hold many scenarios (`scenario_id` column; `tick` restarts at 0 for each) or one file per scenario.

Header keys (all optional for the tool, all wanted): `format: golden-trace v1`, `build` (hash of the game exe, e.g. SHA-256 of PDUWP.exe, first 12 chars), `character`, `stage`, `scenario` (name, or `all`), `hz: 30`, `camera` (fixed yaw used), `notes`.

| column | meaning / unit |
|---|---|
| tick | 0 at the first logged tick of the scenario (input script tick 0) |
| scenario_id | id from section 3 |
| stick_x, stick_y | raw left stick as the original's input read it, -1..1. **Normalise so +y = forward/up, +x = right**; say so in the header if the raw read is inverted. Angle 0 = forward, 90 = right (matches `PD_TEST_STICK`) |
| buttons | bitmask: 1 = jump/dodge-jump, 2 = lock-on (toggle press), 4 = ground dodge, 8.. = skill slots (record the real bit map in the header `buttons:` line) |
| pos_x, pos_y, pos_z | world position, game units. **y = up.** x/z = ground plane |
| facing | body yaw, degrees, same zero as heading |
| heading | movement/desired direction yaw the controller is turning toward, degrees |
| vertical_speed | units per second, + = up (if the original stores u/tick, multiply by 30 and note it) |
| state | controller state int (0 idle, 1 walk/run, 4 jump wind-up, 8 turn/strafe; see `PROGRAM-RE.md`) |
| clip_id | active animation clip (int or name). Ids differ between engines, only the switch TICKS are compared |
| clip_time | ticks since that clip started |
| lock_active | 0/1 |
| lock_band | 0 none, 1 forward, 2 strafe right, 3 strafe left, 4 back-right, 5 back-left (map the original's direction value to this and note how) |
| speed_status | speed status multiplier (1.0 normal) |
| grounded | 0/1 |

Rules: missing columns may be omitted entirely or left blank / `nan`; the tool prints SKIP for any metric that needs them. Column aliases accepted: `t`, `frame`, `sid`, `x/y/z`, `vy`, `clip`, `locked`, `band`. Log AFTER the tick's update (position is where the player ended the tick). Speed is derived as `hypot(dx, dz) * 30` u/s, so pos precision matters more than any logged speed.

## 2. How to record the original (for Laurence / the logger)

Fixed conditions for every run: same character (record it), same flat stage, fixed camera yaw (do not touch the right stick), target dummy 300 u straight ahead of the player for the locked scenarios and not moving, no speed status, full health, start stationary on the ground. Begin logging at script tick 0, log 150 ticks (5 s) per scenario. Re-run each scenario twice; if the two runs differ, the original has hidden state (report it). A controller script/TAS-style feeder is ideal; a hand push is not repeatable, so if hand-driven, hold the stick against a fixed gate and log the real stick values (the compare tool reads the logged stick, not the script).

## 3. Scenarios (150 ticks each, stick as `x,y`; ticks are start inclusive, end exclusive)

Generated exactly by `trace_synth.py SCENARIOS` (`python3 trace_synth.py OUT --scripts` also writes `<id>.script.csv`, `tick,stick_x,stick_y,buttons`, for feeding our sim).

| id | lock | script |
|---|---|---|
| S01_steady_050 | no | 0 until t15; `0,0.5` t15-105; 0 after |
| S02_steady_075 | no | same with `0,0.75` |
| S03_release | no | `0,1` t15-75, then release (measures stop time) |
| S04_reversal_180 | no | `0,1` t15-75; `0,-1` t75-135; release |
| S05_lock_forward | yes | lock press (buttons=2) at t0; `0,1` t15-105 |
| S06_lock_strafe_r | yes | lock at t0; `1,0` t15-105 |
| S07_lock_strafe_l | yes | lock at t0; `-1,0` t15-105 |
| S08_lock_pullback_wobble | yes | lock at t0; t15-105: x = +0.08 for 3 ticks, -0.08 for 3 ticks, repeating; y = -sqrt(1-x^2) (dead-back, wobbling +-4.6 degrees) |
| S09_walk_to_run_ramp | no | magnitude rises linearly 0 to 1 over t15-105 (`0, (t-15)/89`) |
| S10_jump | no | no stick; buttons=1 (jump) at t15 for one tick |
| S11_jump_run | no | `0,1` t5-60; jump at t40 |
| S12_tumble | no | `0,1` t10-25; ground dodge (buttons=4) at t15 (forward) |
| S13_cartwheel | no | `1,0` t10-25; dodge at t15 (sideways) |
| S14_ground_dodge | no | no stick; dodge at t15 |

Unconfirmed: which dodge button/stick direction gives "tumble" vs "cartwheel" in the original (`PROGRAM-RE.md` says one move, 751). The logger should note the clip names seen; S12-S14 still compare distance/duration whatever they are called.

## 4. Metrics and default tolerances

Tolerances are absolute or relative (larger wins) and can be overridden: `--tol speed_max_err=12`. They are starting guesses for "a human cannot feel it", not facts about the original.

| metric | definition | tolerance |
|---|---|---|
| speed_mean_err / speed_max_err | mean / max abs difference of the per-tick speed curve, u/s | 3 / 8 |
| first_divergence | first tick with speed error above 4 u/s for 2 ticks in a row | none expected |
| distance | horizontal path length over the scenario | 3% or 2 u |
| steady_speed | mean speed over the last 20 ticks of the push | 5% or 1.5 u/s |
| time_to_run | ticks from push start until speed >= `--run-speed` (default 50 u/s; run is 55.6) | 2 ticks |
| time_to_stop | ticks from stick release until speed < 1 u/s | 2 ticks |
| max_turn_rate | largest per-tick change of facing, deg/tick (program: 20) | 3 |
| facing_settle | free camera, after the last >60 degree stick change: ticks until facing within 10 degrees of the stick | 2 ticks |
| clip_switches | list of ticks where clip_id changed; counts and ticks must match in order | 1 tick |
| strafe_flips | locked scenarios: left/right side flips of lock_band (2<->3, 4<->5, ...). The spasm (audit H) shows as a large number in S08 | 1 |
| takeoff_delay | ticks from jump press to grounded = 0 | 1 |
| apex / airtime | max height above start (u) / ticks airborne | 5% or 1 u / 1 tick |
| busy_ticks | ticks from press until state is back in 0/1/8 (covers wind-up, air, landing lock, dodge lock) | 2 |
| dodge_distance / dodge_peak_speed | distance in the 60 ticks after the dodge press / peak u/s | 5% or 3 u / 8% or 4 u/s |

Report: `python3 engine/re/trace_compare.py orig.csv ours.csv [--scenario S08_lock_pullback_wobble] [--step 5] [--out report.txt] [--strict]`. It prints, per scenario, an ASCII speed curve (orig above ours), a sampled table (tick, orig, ours, error), then one line per metric with PASS / FAIL / SKIP and the numbers, and a final SUMMARY; the same text is written to `--out` (default `trace_report.txt`). `--strict` exits 1 on any FAIL.

What to read first once real data exists: S09 (is speed graded? `time_to_run`), S02 (does 0.75 walk or run), S04 (`max_turn_rate`, `facing_settle`), S08 (`strafe_flips`, `clip_switches`), S10/S12 (`busy_ticks`).

## 5. Getting a trace out of OUR sim (implemented 2026-09-30 in `crates/pd_game/src/trace.rs`; built and unit-tested, NOT yet run against a real stage)

As built: `PD_TRACE`, `PD_TRACE_SCRIPT`, `PD_TRACE_ID` work as below. Differences from the spec: `heading` is the direction of horizontal velocity (blank when still); `state` is 0 idle / 1 walk-run / 8 strafe / 4 any other busy motion; `speed_status` is blank; `buttons` logs only jump (1) and lock (2) because jump and ground dodge share one button in our sim; `clip_time` counts 30 Hz ticks. The run exits after the last script row. The first real run may expose a wrong tick offset (check `tick` 0 lines up with the script's tick 0).

Existing: `PD_TEST_STICK=<mag>,<angle>[,start[,end]]` (`main.rs` ~1165, overrides P1's stick for a single segment; covers S01-S03 and S05-S07 by hand) and `PD_MOVE_LOG=1` (stderr text, not CSV, no position). The sim steps faster than 30 Hz (audit: 60 Hz), so a trace must be decimated.

Minimal new flags, all read in `main.rs` next to `PD_MOVE_LOG` (the writer only reads sim state, never writes it, so determinism is untouched):

- `PD_TRACE=<file>`: append one CSV row per **30 Hz** tick for P1 (write header + `# build:`/`# character:` lines on first row).
- `PD_TRACE_SCRIPT=<file>`: optional; a `tick,stick_x,stick_y,buttons` CSV (exactly what `trace_synth.py --scripts` writes); when set, overrides `axes[0]`, jump and lock for P1 from that row (tick = ticks since scenario start) and exits after the last row, so all 14 scenarios are batchable.
- `PD_TRACE_ID=<scenario_id>`: value for the `scenario_id` column (default `live`).

Field mapping (names are guesses, check `sim.rs` `Player` before coding): `stick` = the pre-deadzone `axes[0]` (raw, forward positive, before `Input::from_axes` quantises to i8, and say so in the header); `pos_x/z` = `p.pos.x/z`, `pos_y` = `p.pos.y`; `facing`/`heading` in degrees from the player's yaw and wish direction; `vertical_speed` in u/s; `state` = nearest controller-state equivalent (map gait/motion/action to 0/1/4/8 and document the map; unmapped = blank); `clip_id` = `clip_for(&world.0, 0)`; `clip_time` = ticks since `clip_id` last changed (keep in the writer); `lock_active` = `p.locked`; `lock_band` = the sim's direction enum mapped to 0-5; `speed_status` = `p.speed`; `grounded` = `p.grounded`.

Row writer outline:

```rust
// static mut-free: hold in a Bevy Resource `TraceWriter { file: Option<BufWriter<File>>, last_clip: String, clip_t: u32, start_tick: u64 }`
fn trace_row(w: &mut TraceWriter, world: &World, stick: Vec2, buttons: u8, id: &str) {
    let t = world.0.tick - w.start_tick;
    if world.0.tick % SIM_TICKS_PER_30HZ != 0 { return; }          // decimate (2 at 60 Hz)
    let p = &world.0.players[0];
    let clip = clip_for(&world.0, 0);
    if clip != w.last_clip { w.last_clip = clip.clone(); w.clip_t = 0 } else { w.clip_t += 1 }
    writeln!(f, "{},{},{:.4},{:.4},{},{:.4},{:.4},{:.4},{:.3},{:.3},{:.3},{},{},{},{},{},{:.2},{}",
        t / SIM_TICKS_PER_30HZ, id, stick.x, stick.y, buttons, p.pos.x, p.pos.y, p.pos.z,
        facing_deg(p), heading_deg(p), p.vel.y, state_int(p), clip, w.clip_t,
        p.locked as u8, band_int(p), p.speed, p.grounded as u8)?;
}
```

Call it once per sim tick after the step, from the same system that builds `pending`. Note the log must be taken AFTER the sim step so `pos` is end-of-tick like the original; if our step is applied one frame later, shift by one row and record it in the header. Run: `PD_TRACE=ours.csv PD_TRACE_SCRIPT=S08_lock_pullback_wobble.script.csv PD_TRACE_ID=S08_lock_pullback_wobble pd_game` for each scenario (concatenate files; keep one header), then `trace_compare.py original.csv ours.csv`.

Known comparison caveat: our sim has a 60 Hz air/dodge integration (audit), so sub-unit differences in apex/airtime are expected; the default tolerances allow 1 u / 1 tick.
