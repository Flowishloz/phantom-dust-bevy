## Handoff: fresh accuracy scorecard + sound fixes (opened 2026-10-09, Laurence remote)

**Goal / scope:** Laurence (remote, 2026-10-09 ~10:50) picked "Scorecard + sound fixes": (1) re-run the full film comparison of all 341 skills against his recordings of the original, so we know where accuracy stands after this week's work; (2) fix the missing / extra SOUND classes it reports. Direction stands: gameplay accuracy first, no new features. He is away from the PC: nothing in this task needs him; show him 1-3 sheets at the end, not tables.

**Starting point (read first):**
- `AGENTS.md`, then `engine/docs/NEXT-SESSION.md` top block (perf round 3 just finished: 4 players locked 60 fps; all pushed, main = origin/main at ab8f7f3 or later).
- Skill `pd-film-compare` (how to run and judge; classes, not per-skill fixes; READ rules from skill fields).
- Last full scorecard: 266 of 341 clean, 128 mismatches (sound_missing 49, sound_extra 32, gameplay in 13 skills + 3 never-fired takes), run `%USERPROFILE%\.pdtools\compare_full_1003b` (main 60f227f, 2026-10-03). Sound classes from that run: `engine/docs/audits/P0-SOUND.md` section 12 (soundfill2 lane) and section 4.
- Takes from the original: `%USERPROFILE%\.pdtools\moverec\takes_full` (334 ok / 7 refused).

**Steps:**
1. Build the dev game on main (`cargo build -p pd_game` from `engine\`, env `. .\env.ps1`; check it compiled the main path). Copy the exe to a fixed name (e.g. `.pdtools\pd_game_base_1009.exe`) so the run is pinned.
2. Full run into a NEW folder: `$env:PD_GAME_EXE=<that exe>; $env:PD_COMPARE_OUT="%USERPROFILE%\.pdtools\compare_full_1009"; python engine\re\film_compare.py run --real --jobs 1` (hours; 5-14 GB: check `df -h /c` first, ~35 GB free on 10-09; `--jobs 2` can GPU-OOM a take, rerun singly). Keep the PC on mains.
3. Compare with `compare_full_1003b`: new clean count, which skills changed status (better / worse), mismatch classes by kind x family. Anything that got WORSE since 10-03 is a regression to chase before new fixes.
4. Sound classes: group sound_missing / sound_extra by family and trigger (cast, hit, block, impact, landing, capsule), find the READ rule (skill fields, the program's sound tables, PROGRAM-RE.md / FORMATS-AUDIO.md), fix by class, re-run only the affected ids, then the full set again at the end.
5. Each verified fix: commit (tests green: `cargo test -p pd_game`), update `P0-SOUND.md` / `FORMATS-AUDIO.md` with evidence (READ vs MEASURED vs UNCONFIRMED). Do not push without Laurence.
6. Refresh `engine\play` only with the `play` profile (`cargo build --profile play -p pd_game`, skill `pd-build-run`) after the fixes are verified.

**Model split (Laurence's global rules):** a Haiku scout can run the full comparison and tabulate classes; Sonnet lanes for bounded sound fixes; Opus verifies every result (re-run takes, listen to nothing: compare event logs and sheets).

**Watch out:**
- The OLED-care screensaver starts while he is away and slows frames: film takes are wall-clock driven, so timing-sensitive mismatches can flicker. Re-run a flagged take before believing it (memory `pd-perf-benchmarking`).
- Never commit game data, sheets with game assets stay local.
- `engine\play` is Laurence's copy: only replace it when he is not running it (`Get-Process pd_game`).

**Done so far (2026-10-09 afternoon):** Laurence stopped the full scorecard ("most skills look good; focus on reverse engineering the sound"). Read and built, film-proven on the 52 sound-mismatch skills (`.pdtools/compare_defaults_1009`): sphere probe = first stage chunk in file order (c2674d9), shot / burst sweep contacts + impact marks radius 24.57 (c325483). Clean 5 -> 10 of 52, sound_missing 36 -> 27, sound_extra 25 -> 21. Details and addresses: P0-SOUND.md section 13, PROGRAM-RE.md "Stage contacts and impact marks". The first full-run attempt failed because `steam_api64.dll` must sit beside a copied exe (now in the pd-film-compare skill).

**Next:** remaining sound gaps are mostly gameplay (our knock-downs where the original has none; rain drops landing elsewhere). Open sound reads: common/127 / 128 (data-driven, `FUN_1402506c0`), reaction row 100 (bind reactions), who sets the bullet probe's +0xc silence word.
