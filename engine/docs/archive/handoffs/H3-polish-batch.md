## Handoff from Claude Code: visual polish batch (H3)

**Goal:** Five small, independent render-only polish jobs left over from the 2026-09-28 visual cycle. Do them in order; each is its own commit. Stop and report after each if something is unclear rather than guessing.

**Rules for every job (read AGENTS.md first):**
- Work in your OWN git worktree, never by switching branches in the shared folder: `git worktree add -b lane-h3 ..\pd-lane-h3 main` from `<repo>`, and a separate build dir (e.g. `%USERPROFILE%\.pdtools\target-h3`). `game_files/` and `engine/cache/` stay in the main folder — point tools at them read-only.
- Render-only: never touch `sim.rs`, `combat.rs`, `moves/`, `arsenal.rs`.
- Guesses are named constants marked "unconfirmed" in code + docs.
- Proof = real-game screenshots (`pd_game --demo --near --screenshot <png> --shot-frame N`, `PD_SHOT_TICKS`, `PD_TEST_SPLIT=tb`; see `.claude/skills/pd-build-run` and `pd-visual-verify`) placed NEXT TO Laurence's footage frames. Look at every image; black or empty captures are not proof.
- `cargo test -p pd_formats -p pd_game -p pd_convert` green after each job (7 + 36 + 276 at hand-off). Commit only named files; never commit images, game data or cache; don't push. End any `pd_game.exe` you started before reporting.

### Job 1 — School aura polish
Files: `engine/crates/pd_game/src/school_aura.rs`, `school_aura.wgsl`, `GAME.md` "School aura", `VFX-AUDIT.md` item 6.
Current: skinned shell glow (2ee203a), sheets in `engine/shots/schoolaura/v2/`. Footage: `engine/shots/schoolaura/v2/vs_footage_v2.png` (left panel), clip `%USERPROFILE%\Pictures\Phantom Dust - Moves 2\Lock-On Animation.mp4`.
Fix: (a) softer, blurrier falloff like the footage (wider, lower-intensity outer band; the footage glow has no hard edge); (b) the spiky hair shows flat pink panels up close — fade the shell on thin/spiky geometry (e.g. by triangle area or view angle) or cap its brightness; (c) a limb crossing the body tints the body — reduce with depth-aware fade if cheap, else document. Use the `PD_AURA_*` overrides to make an A/B/C sheet next to the footage; ship the closest.

### Job 2 — Psycho Knife ray streaks
Files: `effects.rs` (FLAIR01 drawing only), `FORMATS-EFFECTS.md`, `VFX-AUDIT.md` item 4.
Current: rays are wide wedges; footage shows thin bright streaks (`engine/shots/vfxaudit/scale/sheet_psychoknife_zoom.png`, top row). Check FLAIR01's width words (tip/base width, easing) against how we build the ray quad; fix from data. Also measure the ~4-tick lateness (press vs cast sound, see FOOTAGE.md) and write down the cause; do not change sim timing — report it instead.

### Job 3 — Slide Laser crescent timing
Files: `effects.rs`, `FORMATS-EFFECTS.md` (lead: the `m0_d4`..`m0_d8` family), `VFX-AUDIT.md` item 2.
Footage: blazes hard within ~150–300 ms of cast, fades by ~800 ms (`engine/shots/vfxaudit/charge/sheet_slidelaser_vs_footage.png`). Ours ramps linearly and peaks late. Re-read that family's easing words (word 9/10 timing and easing kinds) and fix; tick-matched sheet vs footage.

### Job 4 — Lock-on scythe crescent
Files: `hud.rs` (lock-on marker only), `HUD.md`.
Current marker is close; its crescent is a neat bowl under the reticle. Footage (`engine/shots/lockon/marker_frames.png`, rows 2 and 6–8): a big sweeping scythe arc starting at the reticle's lower-left and trailing off to the left, thicker in the middle. Keep range colours (orange close / yellow medium / green far). Crop sheet vs footage.

### Job 5 — Orb colour notes (Laurence, parked list in `CAPSULE-VISUALS.md`)
Files: `orb_material.rs`, `orb_*.wgsl`, `CAPSULE-VISUALS.md`.
(a) Attack glow redder, less orange; (b) Defence a deeper, warmer blue (currently too cold/pale); (c) per-kind ring strength instead of one shared setting so Attack can be stronger without blowing out the others. Leave "glow too uniform" for later. Render `all_kinds` before/after + attack/defence next to `engine/shots/orb/footage/red_idle.png`.

**Tests added or planned:** keep existing tests green; add a unit test only where logic changes (e.g. per-kind ring table).

**Risks identified:** `effects.rs` jobs (2, 3) can regress other effects — re-render Psycho Wave and Crystal Wall before/after. Orb shader changes affect all nine kinds — always render the all-kinds sheet.

**Next concrete step:** create the worktree, run the tests to confirm 7 + 36 + 276, start Job 1.

**Commands Claude Code already ran:** merged everything to `main` at the school-aura merge; tests green; no worktrees in use by other lanes.
