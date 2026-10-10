# Lane brief template (for parallel agent lanes)

Give each lane agent this brief plus a lane prompt naming the files it owns and its goal. Update the commit hash and test count in "Before anything" / "Build and run" to the current `main`. Used 2026-09-26 and 2026-09-27 (menus / palace / effects) for the erase/status/special/environment, front-end, stages, sound and arena-objects lanes.

You are one of several parallel lanes rebuilding Phantom Dust (2004) as a Rust + Bevy 0.19 fan engine for local split-screen versus. You work in your own git worktree (your current directory). The other lanes work at the same time on other files.

## Before anything
1. Run `git log -1 --oneline`. It must be `a894b34` (or later). If your branch is older, run `git merge --ff-only main` (it's a clean fast-forward), then check again.
2. Read `AGENTS.md` (hard rules), `engine/docs/INDEX.md`, and the docs your lane names below.

## Hard rules
1. **Only edit the files your lane owns** (listed in your lane prompt). If you need a change elsewhere, don't make it — list it under "Requests" in your final report with the exact change.
2. **Never commit game data or anything derived from it** (`game_files/`, `engine/cache/`, PNG/WAV/GLB/screenshots). Only source code and docs are committed. `engine/cache/` and `engine/userdata/` are gitignored.
3. The simulation (`combat.rs`, `sim.rs`, `moves/`) is deterministic and **not yours** in any lane — read it, don't edit it.
4. Never copy code from `tools/Esper-RE` (GPL). Reading it as reference is fine.
5. Evidence first: file-format facts go into `engine/docs/FORMATS-*.md` with how you established them; anything guessed is marked unconfirmed.
6. Visual work is proven with screenshots you look at yourself (see `engine/docs/VERIFY.md`, `engine/shot.ps1`, `engine/montage.ps1`). Save screenshots under the **main** checkout's `engine/shots/` (absolute path below) with your lane's prefix; never commit them.
7. Don't touch the user's other sessions/windows; don't push; don't change git config.
8. Only launch the game/viewer in `--screenshot` mode (its window then opens off-screen, behind everything). Never OS-level key/mouse injection. Laurence is working on this PC.

## Paths
- Main checkout (read-only for you, except the cache/shots folders noted): `<repo>`
- Game files (read-only): `..\..\..\game_files\Assets\Data`
- The converted cache the game runs on: `..\..\cache` (your worktree has no cache; pass this path to the game/viewer). Only the stages and sound lanes write into it, each into their own files.
- Screenshots: `<repo>\engine\shots\`
- The game's guide/tutorial text: extract it yourself from `game_files/Assets/Data/Menu/mb0*_en.ssb` (and menu/event text from `Menu/*_en.ssb`, `Event/en/*.ssb`, `questdata/en/questdata.qdt`) with Python: `re.finditer(rb'[ -~
%]{8,}', data)`.

## Build and run (PowerShell, from your worktree root)
```
. .\engine\env.ps1          # PATH (MinGW, cargo) and cd to engine/
$env:CARGO_TARGET_DIR = "%USERPROFILE%\.pdtools\target-<lane>"   # your own pre-built copy: target-react / target-move / target-knifefx
cargo build -p pd_game      # or -p pd_convert / -p pd_viewer
cargo test -p pd_game
& "$env:CARGO_TARGET_DIR\debug\pd_game.exe" "..\..\cache" --demo --screenshot "<repo>\engine\shots\<lane>_x.png" --shot-frame 400
```
The target dir is your own pre-built copy (first build ~1 min). `PD_SHOT_COUNT` / `PD_SHOT_EVERY` give a burst of numbered screenshots. `--stage stNN`, `--p1 pcNNa`, `--p2 pcNNa`, `--loadout a,b,c,d`, `--arsenal mixed` exist today. `cargo test -p pd_game` must stay green (308 tests at the start, plus 36 in `pd_formats`, 7 in `pd_convert` and 10 in `pd_viewer`; run `cargo test -p pd_game -p pd_formats -p pd_convert -p pd_viewer`) with no new warnings from your files.

## Finish
1. `git add` only your files and `git commit` in your worktree (message: what the lane built).
2. Final report, plain and concise: commit hash; what's built / not built; how you verified it (test names, screenshot paths you looked at); anything unconfirmed; **Requests** for changes outside your files.
