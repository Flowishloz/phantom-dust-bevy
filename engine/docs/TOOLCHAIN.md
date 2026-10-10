# Toolchain

## Setup (what the project's own machine uses)

- Rust stable via rustup, default host **x86_64-pc-windows-gnu** (no Visual Studio install needed). `~/.cargo/bin`.
- Portable MinGW-w64 (WinLibs GCC, UCRT), unzipped to `%USERPROFILE%\.pdtools\mingw64` (a folder with no spaces in its path), or anywhere else if you set `$env:PD_MINGW` to it.
- `engine/.cargo/config.toml` names `gcc` and `dlltool` and nothing else; `env.ps1` puts them on PATH.

Every PowerShell session that builds must first run, from the repository root:

```powershell
. .\engine\env.ps1
```

That adds MinGW and cargo to PATH and moves to `engine/`.

## Known traps (each cost time once)

| Symptom | Cause | Fix |
|---|---|---|
| `error calling dlltool 'dlltool.exe': program not found` | GNU target needs MinGW binutils for `raw-dylib` crates (windows-sys) | MinGW on PATH / config.toml `-C dlltool=` |
| `ld.exe: cannot find %USERPROFILE%/Documents/GODOT/Phantom` | MinGW linker splits paths at spaces | Keep MinGW in a space-free path (`.pdtools`) |
| `%1 is not a valid Win32 application` running the exe | Bevy + full debug info > 2 GB PE limit | `[profile.dev] debug = "line-tables-only"`, deps `debug = false` (in workspace Cargo.toml) |
| `expected glam::Vec3, found bevy_math::Vec3` | two glam versions | keep `glam` in our crates equal to Bevy's (0.32 for Bevy 0.19) |
| `failed to remove file ...pd_game.exe (os error 5)` | the game is running | ask Laurence to close it; don't kill it |
| `â€”` and similar garbage appears in files | Windows PowerShell 5.1 `Get-Content`/`Set-Content` read UTF-8 as ANSI | Edit files with the Edit tool; in scripts use `[IO.File]::ReadAllText(p, [Text.Encoding]::UTF8)` / `WriteAllText(p, s, (New-Object Text.UTF8Encoding $false))`. Check with a grep for `â€` |
| Windows Security quarantines the community dump tool | flagged as hack tool | user must allow it (agents must not change AV settings) |

## Bevy 0.19 API notes

- Spawn a glTF scene: `WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset("x.glb")))`.
- Messages (old "events"): `MessageReader<T>`, `MessageWriter<AppExit>` → `exit.write(AppExit::Success)`.
- Screenshots: `commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path))`.
- Animation: `AnimationGraph::new()` + `add_clip`, `AnimationGraphHandle`, `AnimationTransitions::play(&mut player, node, Duration)`.
- Gamepads are entities: `Query<&Gamepad>`, `pad.left_stick()`, `pad.pressed(GamepadButton::South)`.
- Split screen: `Camera { viewport: Some(Viewport { physical_position, physical_size, .. }), order, .. }`.
- Fixed timestep: `Time::<Fixed>::from_hz(60.0)`, systems in `FixedUpdate`, `Time<Fixed>::overstep_fraction()` for interpolation.

When unsure of an API, grep the registry source: `~/.cargo/registry/src/index.crates.io-*/bevy_<crate>-0.19.1/src`.

## Build profiles

- `cargo build -p pd_game` → `target/debug/pd_game.exe` (opt-level 1, deps opt-level 3; fast enough to play). The game Laurence plays is a separate copy, `engine\play\pd_game.exe`, refreshed only after a verified merge (`pd-build-run` skill); PLAY.bat runs that copy.
- `cargo build --release -p pd_convert` → converter; release because it crunches 10 GB of data.
- First Bevy build ≈ 8 minutes; incremental ≈ 15–40 s.

## Vendored pieces (`engine/vendor/`, since 2026-10-03)

- **`vendor/steam/`** (lane online): `steam_api64.dll` (Valve's redistributable) and `steam_appid.txt` (`480`, Valve's test app). `pd_game` builds with the `steam` feature on by default (`crates/pd_game/Cargo.toml`); its `build.rs` copies both files next to the built exe in `target/<profile>/` (falling back to the DLL `steamworks-sys` ships). `engine\play` is not touched by the build: when refreshing Laurence's copy, put both files next to `engine\play\pd_game.exe` too, or the exe will not start.
- **`vendor/bevy_gilrs/`** (lane rumble): upstream `bevy_gilrs` 0.19.1 sends weak-motor rumble to the strong motor; this copy fixes that one line and logs force-feedback support when a pad connects. Wired in by `[patch.crates-io]` in `engine/Cargo.toml`; delete the patch to go back to upstream (everything still builds, only the right motor then buzzes on the left). `RUMBLE.md`.

## Building on Linux (cloud sessions, 2026-10-07)

- A Claude cloud session runs Linux with no `game_files/` and no `engine/cache/`. `cargo test -p pd_game --bin pd_game` builds there after `apt-get install libwayland-dev libudev-dev libasound2-dev libxkbcommon-dev pkg-config` (first build about 25 minutes on 4 cores).
- Expected failures there, MEASURED 2026-10-07 on main e730169 and on 9ca4cfb alike (1134 pass, 4 fail): the three `combat::tests::replay` golden hashes (the goldens are recorded on Windows; Linux float maths gives other hashes even for the movement-only duel) and `sfx::tests::the_footstep_wav_is_in_the_cache...` (needs the cache). Treat any other failure as real; the goldens are only checked on Laurence's PC.
