## Handoff from Claude Code: Lock-on marker, pass 2 (follow-up to 7519d14)

**Goal:** Make the lock-on marker from 7519d14 faithful to Laurence's clip, and make it change colour by range.

**What Claude Code found checking 7519d14 in the real game** (comparison: `engine/shots/lockon/verify_cmp.png`; footage frames: `engine/shots/lockon/marker_frames.png`; clip: `%USERPROFILE%\Pictures\Phantom Dust - Moves 2\Lock-On Animation.mp4`, read-only):
1. Strokes are thin dotted outlines; the original's are thick, chunky, glowing lime strokes (see the fat "Y" icon in the clip).
2. The crescent is a neat symmetric arc underneath; the original's is a large sweeping scythe-shaped arc curving off to the lower LEFT of the reticle.
3. The marker floats above the head (`CHEST * 1.85`); in the clip it sits lower, on the target's body. Measure the right height from the clip.
4. It's procedural. First check the HUD atlases (`cache/ui/hud*.png`, `engine/docs/HUD.md`) for the Y icon / ring / arc / crescent art and use it if present; say in the report whether you found it.
5. The proof screenshots in `engine/shots/lockon/h1/` came out black. Use the real game: `pd_game --demo --near --screenshot <png> --shot-frame N` with `PD_TEST_LOCK_MARKER=1` (see `.claude/skills/pd-build-run`, `pd-visual-verify`), and crop the marker next to the matching footage frame.

**New requirement from Laurence:** the reticle changes colour with distance to the target: far, medium and close range. Use the game's own range bands (skill field s44 / range bands in `engine/docs/PROGRAM-RE.md` and `MOVES.md`; the distance is already in the sim — read it, never write it). Colours (Laurence, 2026-09-28): **orange = close range, yellow = medium range, green = long range**. Take the exact shades from the program/HUD data if you can find them; otherwise pick shades matching the clip's lime green and mark the orange/yellow shades "unconfirmed".

**Key decisions:** render-only; don't touch `sim.rs`, `combat.rs`, `moves/`, `fx.rs`, `effects.rs`, `orb_material.rs` or any `*.wgsl` (another lane is on those). Keep the animation stages from 7519d14 (Y icon → arcs fly in → ring → reticle + crescent; vanish instantly on unlock).

**Files touched or planned:** `engine/crates/pd_game/src/hud.rs`, `engine/docs/HUD.md`, `engine/docs/VFX-AUDIT.md` (item 5).

**Tests:** `cargo test -p pd_game` green (272). Proof: real-game crops of each animation stage next to the clip frames, plus one crop per range colour.

**Next concrete step:** create branch `lane-lockon2` from `main` and work there (not directly on main). Commit only named files; never commit images or game data; don't push.
