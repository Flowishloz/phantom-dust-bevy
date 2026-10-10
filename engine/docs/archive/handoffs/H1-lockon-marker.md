## Handoff from Claude Code: Lock-on marker (VFX-AUDIT item 5)

**Goal:** Replace our lock-on marker (thin orange ring drawn through the target's head) with the original's: a small green pulsing reticle floating above the target's head.

**Key decisions:**
- HUD-only change. Do not touch sim (`sim.rs`, `combat.rs`, `moves/`), `fx.rs`, `effects.rs`, `orb_material.rs` or any `*.wgsl` (another lane is editing those).
- Use the game's own lock-on art from the HUD atlas if it exists (see `engine/docs/HUD.md`; current piece is `LOCK_RING` in `hud.rs`). Guesses must be named constants marked "unconfirmed".
- Reference: Laurence's footage frames via `engine/docs/FOOTAGE.md` and `engine/docs/VFX-AUDIT.md` item 5; sheets in `engine/shots/vfxaudit/`.

**Laurence's lock-on clip (2026-09-28):** `%USERPROFILE%\Pictures\Phantom Dust - Moves 2\Lock-On Animation.mp4` (read-only; 30 fps, two locks + unlocks). Frame-by-frame close-up: `engine/shots/lockon/marker_frames.png` (one tile per frame, read left→right). What it shows, per lock:
- f0: nothing.
- f1–f5 (~0.17 s): a small green three-spoke "Y" icon appears on the target.
- f6–f7: three short green arc segments fly inward from a wide circle (~3× final size), converging on the icon.
- f8: the arcs close into a ring round the icon (dashed ring + crosshair ticks).
- f9: the full reticle — circle with four tick marks and a centre dot.
- f10 onward: plus a large sweeping green crescent/"scythe" arc curving under and to the left of the reticle; reticle + crescent hold, static, while locked.
- Unlock: everything vanishes at once (no fade), within one frame.
Colour: bright lime green, soft glow. Build this as a short render-only animation keyed off the lock-on start tick; look for the Y icon, ring, arcs and crescent pieces in the HUD atlases before drawing anything procedurally.

**Files touched or planned:** `engine/crates/pd_game/src/hud.rs` (LockRing / LockName binds), `engine/docs/HUD.md`, `engine/docs/VFX-AUDIT.md` (mark item 5).

**Tests added or planned:** `cargo test -p pd_game` must stay green (272). Proof = a screenshot of the marker over the opponent next to a footage frame, via `.claude/skills/pd-visual-verify`.

**Risks identified:** the marker must work in both split-screen views; pulsing must be render-time only (never write sim state).

**Next concrete step:** read `hud.rs` around `LOCK_RING` / `Bind::LockRing`, find the green reticle art in `cache/ui/hud*.png`, then build and screenshot.

**Commands Claude Code already ran:** none for this task. Build/screenshot recipe: `.claude/skills/pd-build-run/SKILL.md`. Work on a branch `lane-lockon`; commit only named files; never commit images or game data; do not push.
