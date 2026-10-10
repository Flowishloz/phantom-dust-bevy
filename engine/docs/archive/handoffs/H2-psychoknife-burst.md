## Handoff from Claude Code: Psycho Knife ray-burst size (VFX-AUDIT item 4)

**Goal:** Psycho Knife's FLAIR01 ray burst shoots off-screen in ours; in the original it is a contained, fist-sized starburst. Make ours match.

**Key decisions:**
- Likely a units/field-read bug in how FLAIR01 size/length words are converted (already flagged "unconfirmed" in `engine/docs/FORMATS-EFFECTS.md`). Find the evidence in the effect data (`pd_convert` effect dumps) before changing numbers; no by-eye fudge without a named constant marked "unconfirmed".
- Render-only. Do not touch sim, `fx.rs`, `orb_material.rs` or any `*.wgsl` (another lane is editing those). Keep changes to the FLAIR01 code in `effects.rs`.
- Reference: `engine/shots/vfxaudit/sheet_psychoknife.png`, `engine/shots/kfx_pk_before_after_footage.png`, `engine/docs/VFX-AUDIT.md` item 4.

**Files touched or planned:** `engine/crates/pd_game/src/effects.rs` (FLAIR01 only), `engine/docs/FORMATS-EFFECTS.md`, `engine/docs/VFX-AUDIT.md`.

**Tests added or planned:** `cargo test -p pd_formats -p pd_game -p pd_convert` green (7 + 36 + 272). Tick-matched before/after sheet vs footage (method in VFX-AUDIT.md, `PD_SHOT_TICKS`).

**Risks identified:** FLAIR01 may be used by other skills — check that a fix doesn't shrink effects that were already right (re-render one other FLAIR01 user).

**Next concrete step:** find every effect using FLAIR01, dump Psycho Knife's FLAIR01 fields, compare to how `effects.rs` reads them.

**Commands Claude Code already ran:** none for this task. See `.claude/skills/pd-build-run` and `pd-visual-verify`. Work on branch `lane-knifeburst`; commit only named files; never commit images/game data; do not push.
