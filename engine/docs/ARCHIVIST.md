# Archivist: keeping the docs true

The archivist lane (docs only, never code) is re-run by the orchestrator after each merge wave. Pass 1 was 2026-10-03, pass 2 2026-10-07. This file is its checklist. Status of each doc: `INDEX.md`.

## Rules for the archivist

- Docs only. Never edit code, never touch game data, never push. Commit on `lane/archivist`, message ending with the Co-Authored-By line.
- `WAVES.md` and `BUG-GRAVEYARD.md` are append-only logs owned by the code lanes. Do not edit them. If a log entry is wrong, say so in the report; the owning lane or the orchestrator fixes it.
- While code lanes are open, do not rewrite their area docs; make small, targeted fixes and leave a pointer. Do the larger fixes at the end of the wave.
- Check every claim against the code (grep). A statement you cannot confirm gets marked UNCONFIRMED, not deleted.
- Never delete history. Superseded text gets a one-line banner pointing at the current doc. A doc moves to `archive/` only when INDEX.md no longer needs it AND `WAVES.md` and `BUG-GRAVEYARD.md` do not link it (they cannot be edited to follow the move).

## After each merge, check (in this order)

1. **What merged.** `git log --oneline` since the last pass, plus the new `WAVES.md` entries. Each merged lane should have: a WAVES entry, its area doc updated, a DECISIONS row if it departs from the original or Laurence chose something, a BUG-GRAVEYARD entry if it fixes a report.
2. **Top blocks.** Rewrite the top block of `NEXT-SESSION.md` (scorecard line, landed, open wave, waiting on Laurence, next). Update the "2026-xx-xx (read this first)" block at the top of `AGENTS.md` "Current state". Update `TODO-LAURENCE.md` only for things he must do.
3. **Things lanes remove or rename.** For every feature, row, switch, file or function a lane removed, run:
   - env switches: list `PD_*` in docs and in code (`grep -rhoE '\bPD_[A-Z0-9_]+\b'`) and diff the two sets; living docs must not name a switch the code lacks (history docs may, with a banner);
   - files: every backticked `*.rs / *.py / *.bat / *.wgsl / *.md` named in a doc exists (`git ls-files` by basename);
   - CLI flags: every `--flag` in GAME / PIPELINE / FRONTEND / VERIFY / skills appears as a quoted string in `crates/*/src`;
   - `pd_convert` subcommands in PIPELINE and the skills match `crates/pd_convert/src/main.rs`;
   - markdown links resolve (`](x.md)` relative to the file);
   - menu rows and screens: `frontend/title.rs` ROWS, `menu.rs` rows, `AppState` in `frontend/state.rs` against FRONTEND.md, MENUS.md, GAME.md "Settings menu", `PLAY.bat` help text.
4. **Pending-change notes.** Search docs for "pending", "being merged", "not merged yet", "running", "in progress" and resolve each against git (is the lane merged now?). Known open ones at pass 1 (all resolved in pass 2, 2026-10-07, after wave 1003 merged): MODES title row replaced by ONLINE + a GAMEMODES row in local setup (online lane: FRONTEND "Game modes", DESIGN-BRAWL "Flow", title rows in FRONTEND "Screens"); FOV meaning and camera-distance use (camfov lane: GAME "Settings menu", FRONTEND "Display options"); player 3 bottom half (split3: GAME four-player section); skill-info range, music selection (infomusic: FRONTEND, FORMATS-AUDIO); vibration rows (rumble: FRONTEND Options, new doc `RUMBLE.md` if the lane creates it, then add it to INDEX); `engine/vendor/steam` and the DLL (TOOLCHAIN, AGENTS layout, PLAY.bat note).
5. **Counts and dates.** Test counts and scorecards are quoted from the newest merged WAVES entry or a run you saw; write the date next to them. Do not carry a count forward unlabelled.
6. **INDEX.md.** Every new doc gets one line (what it covers, status current / reference / log / history). Every moved doc is updated in INDEX and in links.
7. **Skills.** `.claude/skills/*/SKILL.md` commands, paths and env names still exist. The skills list in `CLAUDE.md`, `AGENTS.md` layout and INDEX is the actual folder list.
8. **Commit** and report: what was stale (counts, worst examples), what changed, what moved, what needs a code owner.

## One owner per topic

If two docs say the same thing, the owner is the truth; the other links to it.

| Topic | Owner doc |
|---|---|
| Mission, hard rules, layout, toolchain summary, everyday commands | `AGENTS.md` |
| Current state, scorecard, what is next | `NEXT-SESSION.md` (top block); `AGENTS.md` carries a short copy |
| What each lane did and what is UNCONFIRMED | `WAVES.md` |
| Bugs Laurence reported and their pins | `BUG-GRAVEYARD.md` |
| Why a choice was made | `DECISIONS.md` |
| Build setup and traps | `TOOLCHAIN.md` (skill `pd-build-run` has the commands) |
| `pd_convert` commands and cache regeneration | `PIPELINE.md` |
| How to prove a change | `VERIFY.md` (skills `pd-visual-verify`, `pd-film-compare`) |
| Simulation, input, cameras, animation states, sound, stage look, props, footsteps | `GAME.md` |
| Per-family skill behaviour | `MOVES.md` |
| Collision, stairs, camera occluders | `COLLISION.md` |
| Title, setup, builder, result, modes, Options overlay | `FRONTEND.md` |
| Menu look (art, font, panel sizes) | `MENUS.md` |
| Battle HUD | `HUD.md` |
| Fighter / stage lighting, shadows | `CHAR-LIGHT.md`, `STAGE-LIGHT.md`, `SHADOWS.md` |
| Effect formats, blends, school aura glow | `FORMATS-EFFECTS.md` |
| Skill, audio, stage, ALR, animation, CPU file formats | the matching `FORMATS-*.md` |
| Facts read from the original's program | `PROGRAM-RE.md` (function addresses and numbers) |
| Recording the original | `MOVE-RECORDER.md` |
| Modes / 2v2 / replays design | `DESIGN-BRAWL.md` / `DESIGN-2V2.md` / `DESIGN-REPLAYS.md` |
| Lane rules and tags | `WAVE6-BRIEF.md` (+ `handoffs/WAVE-1003-COMMON.md` for the current wave) |
| Doc map and ownership | `INDEX.md`, this file |

## Conventions

- **Names:** area docs `UPPER-KEBAB.md` in `engine/docs/`; format docs `FORMATS-<AREA>.md`; design docs `DESIGN-<NAME>.md`; audits and research in `audits/`; wave handoffs `handoffs/WAVE-<mmdd>-<NAME>.md`; finished history in `archive/` (handoffs in `archive/handoffs/`).
- **Dates:** ISO `YYYY-MM-DD`. Every dated section heading names the lane in brackets or after a comma: `## Display options (2026-10-03, lane settingsux)`.
- **Evidence tags** (from WAVE6-BRIEF): READ (decompiled function address), MEASURED, LIVE (trace of the original), UNCONFIRMED. A number without a tag is a guess and must say so.
- **Order inside a doc:** area docs are layered, newest section last; a banner at the top says which sections win. Logs (`WAVES`, `BUG-GRAVEYARD`, `DECISIONS`) are newest first or per their own header; do not reorder.
- **Superseded text:** keep it, add `> Superseded (date): <what replaced it>, see <doc>` directly under the heading. Do not leave "current" in the heading of an old block.
- **Style:** plain English first, short paragraphs and bullets, backticks for code names, no emoji, UTF-8 (check for `â€` mojibake after edits).
- **Replies to Laurence:** outcome first, no code, pictures for visual work (AGENTS.md).

## Pass 1 findings that need a code owner (2026-10-03)

Status at pass 2 (2026-10-07) in brackets.

- `pd_convert stages` still accepts `--collision dat+walls` / `alr` (A/B switches kept in code); docs now say "A/B only". Confirm whether to remove them.
- `GAME.md` still carries the 2026-09 shell-aura text under a "Superseded" banner; `school_aura.rs` header and `H3-school-aura-handoff.md` are the only live aura references. The lane that owns aura should confirm and prune.
- ~~`engine/docs/handoffs/WAVE-1003-STATE.md` exists only in the main checkout (untracked).~~ [Resolved: tracked in git since 7171c24, 2026-10-03.]
- ~~Test counts differ between notes (1007 in NEXT-SESSION, 1013 in bodyradius, 1033 in settingsux).~~ [Resolved: 1138 green on main twice after the wave 1003 merges, 2026-10-03; the per-suite split was not recorded, so the next full run should log it per suite.]
- (`dat+walls` / `alr` switches and the aura text: still open at pass 2.)

## Pass 2 findings that need a code owner (2026-10-07)

- Resolved 2026-10-07: `PD_TEST_FRESH_SETTINGS` (camfov) was superseded by settingsux's `PD_SETTINGS=<file>` in the merge b1e8de9; GAME.md now names `PD_SETTINGS`.
- Resolved 2026-10-07 (help text now says Online): `PLAY.bat` help text (option 1) still lists "Modes"; the title has no MODES row since 2026-10-03 (it is now VERSUS MODE / ONLINE / ARSENAL / OPTIONS MODE / EXIT GAME).
- Resolved 2026-10-07 (the script runs it now): `engine/regen_cache.sh` does not run `pd_convert musictracks` (lane infomusic); without it a regenerated cache has no `.pdm` music, so menus are silent and matches fall back to the old stage WAVs. PIPELINE.md's list now has it.
- `hud.rs::layout_hud` takes a `local: Res<LocalViews>` it no longer reads (Brett's `full_scales` branch was dropped in 19a8bb8).
- `engine/Cargo.lock` as committed records `bevy_gilrs` 0.19.1 as a crates.io package; a build with the `[patch.crates-io]` vendored copy rewrites that entry (seen in the working tree 2026-10-07). Commit the rewritten lock with the next code change so the patch is what the lock says.
- `WAVES.md` (append-only, not edited): the online entry says Brett's `full_scales` HUD applies "only when `LocalViews` is restricted"; on main it was dropped (see above). The split3 entry's "until they use each camera's own aspect" note was resolved by camfov. The owning lane or the orchestrator may add a correction line.
- Resolved 2026-10-07 (three rows added, the 2026-09-26 row marked withdrawn): wave 1003 lanes without a DECISIONS row: camfov (FOV setting = horizontal angle of a 16:9 view, Hor+/Vert+ per camera; it withdraws the 2026-09-26 "top/bottom: vertical view at least 45% of the FOV" row), online's menu structure (MODES row removed, ONLINE row, GAMEMODES in setup; Laurence 2026-10-03) and "only human seats get a view" (1 human vs CPUs = full window), which replaces camfov's "a CPU seat gets its own view". The DECISIONS.md 2026-10-02 game-modes row still describes the MODES title row. Add rows (the orchestrator owns DECISIONS).
