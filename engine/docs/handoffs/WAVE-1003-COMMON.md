## Wave 2026-10-03 (Laurence's play-test list): rules for every lane

You are a Sonnet lane on the Phantom Dust rebuild (Rust + Bevy 0.19, GNU/MinGW). Laurence (non-coder creative director) played the build and sent a list of fixes. The orchestrator (Opus) verifies and merges your branch. Your report is not proof: leave evidence (tests, before/after pictures).

**Read first:** `AGENTS.md`, `engine/docs/WAVE6-BRIEF.md` (shared lane rules), and the area doc named in your brief. Skills worth reading in `.claude/skills/`: `pd-build-run`, `pd-visual-verify`, `pd-laurence-bug`, plus `pd-reverse-engineer` / `pd-film-compare` if your brief says so.

**Build environment (PowerShell):**
- `. engine\env.ps1` from your worktree.
- `$env:CARGO_TARGET_DIR = "<repo>\engine\target"` (shared and warm).
- Always build with `-j 2`. Six lanes share this folder, so cargo builds queue on its lock. Iterate with `cargo check -j 2 -p pd_game` and run the full build or tests in batches.
- Your exe is `target\debug\pd_game.exe`, and other lanes overwrite that file. Right after your build, copy it to your own scratch file (e.g. `%TEMP%\<lane>_pd_game.exe`) and run that copy. If the link fails with "access denied", copy `target\debug\deps\pd_game-<hash>.exe` instead.
- `engine\cache` in your worktree is a junction to the shared cache. **Never write into it.** To regenerate cache files, write them to a scratch folder and point the game at it (see the docs for `PD_FILM_CACHE`, or add a dev env switch). The orchestrator copies regenerated files in at merge.

**Hard rules:**
- **Evidence:**
  - Every value that claims to match the original must be READ (from the program or data), MEASURED (from footage or takes) or LIVE.
  - Otherwise write it down as UNCONFIRMED in your WAVES.md entry.
  - A Laurence-requested option or quality-of-life change needs no evidence. Its *default* must still be the original's behaviour, unless your brief says otherwise.
- **The sim stays deterministic:** no time, randomness or render state in `sim`/`combat`. Settings are presentation only.
- **Approved feel:** the locked-on camera framing (60/30 chest-aim), locked-on movement, the near-wall dissolve and footsteps. Don't change their defaults except where your brief says to.
- **Never:**
  - touch `engine\play` (Laurence's game copy) or `engine\userdata`;
  - push;
  - run the original game;
  - send OS input (keys/mouse) to any window;
  - commit game data, cache files, shots, footage or audio;
  - copy Esper-RE code;
  - change system settings.
- **Disk:**
  - C: has about 20 GB free. Check `df -h /c` (Bash) before heavy runs.
  - Stop and report if free space is under 8 GB.
  - Delete your own scratch frames and exes when you are done.
  - Keep pictures under `engine\shots\<lane>\` (gitignored).
- **Tests before you finish:**
  - `cargo test -j 2 -p pd_game -p pd_formats -p pd_convert -p pd_viewer`, all green;
  - `python -m unittest` in `engine/re` if you touched Python.
  - Add tests that pin your fixes.
  - Mocked-pad input tests: see `pause_menu_input_tests` in `pd_game/src/main.rs`.
- **Commits:** on your lane branch only. End every message with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- **Docs:**
  - add one entry to `engine/docs/WAVES.md` (cause, files, tests, pictures, UNCONFIRMED residue);
  - add a `engine/docs/BUG-GRAVEYARD.md` entry for each real bug;
  - update the area doc where behaviour changed.

**Visual proof:**
- Screenshot tooling: `--screenshot <png> --shot-frame N`, `PD_SHOT_TICKS`, `PD_TEST_PRESS`, `PD_TEST_SPLIT=tb|sbs`, `--demo`, `--stage`, `--loadout`. See `pd-visual-verify`.
- Read your own PNGs to check them. Build a before/after sheet (`montage.ps1` or PIL) and name it in your report.

**Final report (short):**
- what was wrong (cause + evidence);
- what changed (files);
- tests;
- picture paths;
- anything UNCONFIRMED or needing Laurence's eyes.
