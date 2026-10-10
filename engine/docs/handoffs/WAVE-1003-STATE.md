## Handoff: wave 2026-10-03 (Laurence's play-test list)

> Archivist 2026-10-07: every lane listed under "Running lanes" and "Pending" below is merged on main (see "Update 16:45" at the end); only the perf lane is open (not merged). The current to-do list is the top block of `NEXT-SESSION.md`. The setup rule-line item was fixed 2026-10-07.

**Goal:** fix Laurence's play-test list. Common lane rules: WAVE-1003-COMMON.md.

**Merged on main:**
- stairs: Lost Palace under-stairs walls opened; `PD_STAIRS_ORIGINAL=1` restores the original.
- settingsux: browse + A, sliders, 4K / auto-detect.
- cam_dist_pct field.

**Running lanes (lanes\<name>, branch lane/<name>):**
- camfov: FOV horizontal@16:9 default 74.7, Hor+/Vert+ rule, cam_dist_pct applied.
- split3: P3 gets the bottom half; 3P frame-drop measurements.
- fxfix: Psycho Knife squares, Rapid Cannon black smoke, effects sweep.
- infomusic: skill-info range, music selection read from the exe, duplicated models on character select.
- online: merge Brett's origin/online-mp. Changes:
  - MODES row removed;
  - GAMEMODES row in local setup;
  - ONLINE on the title screen, with the host's gamemode selector;
  - DLL moved to engine/vendor/steam.
- rumble: vibration rules read from the exe; VIBRATION on/off row.
- stairs (resumed): Panorama st04/st14 ramps. Laurence said YES.

**Build rules for lanes:**
- camfov, settingsux and rumble build in engine\target2; the others in engine\target.
- Every lane bumps the pd_game version locally (0.1.0-<lane>) to stop exe hash clashes. The bump is NOT committed; check for it at merge.

**Pending:**
- Main tests + build (main tree's Cargo.toml has a local 0.1.0-main bump; revert it after).
- Then refresh engine\play.
- The online lane needs steam_api64.dll + steam_appid.txt next to engine\play\pd_game.exe.

**Settings:**
- engine\cache\settings.json was trimmed to split/cam_drop so first-run auto-detect fires.
- Backup: settings.before-1003.json.

**Next after split3 reports:** a performance lane. Target: 4K 60 fps, 1–4 players, no look change. Laurence was told this target.

**Ask Laurence:** send display_log.txt (written next to settings.json) after he runs on the TV.

**Update 15:50:**
- fxfix merged.
- stairs part 2 merged: Panorama has nothing to open; measured.
- archivist lane started (docs only). Re-run it after each merge wave.

**Laurence 2026-10-03:** "once all lanes are completed push to github". Push main to origin when every lane in this wave is merged and the tests are green.

**Update 16:45:**
- Merged on main: camfov, rumble, split3, infomusic (music .pdm in cache), online, archivist pass 1. Tests 1138 green twice. engine\play refreshed, with the steam DLLs.
- main pushed per Laurence.
- perf lane: WIP 344892f on lane/perf (pushed as a branch), NOT merged. 4P 1080p 60->49 ms, 3P 33->29 ms, pictures identical. Measured on BATTERY, so redo on mains. Next: shared effect materials + per-instance data. See PERF.md "where I stopped".
- To do next:
  - archivist pass 2;
  - the versus setup blurb says "Standard" while CLOSE QUARTERS is selected;
  - Psycho Knife square: Laurence saw no square in our tick-45 PNG; he will check in-game and say when it appears;
  - locked top/bottom framing A/B with Laurence;
  - side-by-side letterbox question;
  - online between two real PCs.
