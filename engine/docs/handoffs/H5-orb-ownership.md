# H5 — orb ownership: own/opponent capsule looks were cross-contaminated (fixed, view presentation only)

**For:** orchestrator / Laurence. **From:** lane-orb-ownership, 2026-09-29.
**Scope kept:** `pd_game/src/fx.rs` only (+ this doc). Sim/combat capture rules and
owners untouched; `main.rs`, sfx, school_aura, cache/assets/game files untouched.
No reconvert, no merge, no push.

## User's report (exact)

"capsule aesthetics have flipped — opponent capsules now look like how ours are
supposed to look and vice versa."

## Cause (reproduced live, not a stale build, nothing blindly flipped)

`sync_combat_visuals` spawns **two** entities per capsule: the owner's own
full-colour model (`COLOUR_TO_ORB_SLOT`, slots 4–10) on `view_layer(owner)` and
the opponent look (slots 11/12) on `view_layer(1-owner)` (`fx.rs`, `orb_presentations`).
That mapping was and is correct. What broke is **where the layer lived**: since
`spawn_orb` was introduced (commit `6bebd29`, "replace placeholder sphere with
st00's own art"), the layer went only on the orb **root**, while the actual
meshes spawn as `ChildOf(root)` children **with no `RenderLayers`**
(default layer 0 → both split-screen cameras render them).

Bevy 0.19 culls each entity by its OWN mask (`bevy_camera` visibility:
`maybe_entity_mask.unwrap_or_default()`, no walk up to the parent), and a
`ChildOf` chain inherits nothing (same reason `frontend/charselect.rs` avoids
`RenderLayers` for its preview gap — see its `SLOT_GAP` comment). Before
`6bebd29` this worked: the placeholder spheres (commit `4272173`) carried
`view_layer(view)` on the **mesh entity itself**. The refactor moved it to a
mesh-less root, silently disabling per-view isolation: **every capsule drew its
own-model AND its opponent-model superimposed, in both views.** Your own
(capturable) orbs read dark/smoky (ghost's billboarded dark core + multiply
disc dominate the centre) while the foe's read bright (your superimposed
own-model colours) — backwards in both directions, matching the report.

The `st00.alr` slot labelling was checked and is **not** the cause:
`cache/fx/capsules/capsules.json` draw counts per slot match
docs/CAPSULE-VISUALS.md's per-kind table (4→3, 5→2, 6→3, 7→3, 8→2, 9→3, 10→2,
11→3, 12→3), and `ST00_SLOT_NAMES` (`pd_convert/src/main.rs`) matches the
doc's dev-name order. Capture ownership (`combat.rs`: `owner` = who spawns at
whose spots = who can pick it up, `capsule_in_reach`) was verified read-only
and is unchanged.

## Fix (presentation only)

- `spawn_orb` takes `view: Option<usize>` and puts that view's layer on the
  **root and on every draw child** (`fx.rs`). Live call passes `Some(view)`;
  `PD_CAPSULE_PREVIEW=mat`/`close_*` pass `Some(0)`; the `=1` line-up passes
  `None` (all views, as before).
- Owner/view/slot mapping extracted into pure `orb_presentations(owner,
  colour) -> [(view, slot); 2]`, used by `sync_combat_visuals` (index 0 = own,
  1 = ghost). Behaviour identical, now unit-pinned.
- `PD_ORB_LOG=1` (new, env-gated like `PD_FX_LOG`): logs
  `[orb] cap <id> owner <o> colour <c> ghost <g> -> view <v> slot <s> at <pos>`
  once per spawned presentation entity.
- `PD_CAPSULE_PREVIEW=ghostcheck` (new): P1-view attack pair (slot 5 own vs
  slot 12 foe's) + P2-view aura pair through the same `spawn_orb` + layer path
  the live match uses, for paired A/B.
- `spawn_preview_camera` now sees layers {0,1,2}: required — with per-draw
  layers, a layer-0-only preview camera renders an empty frame (caught by
  re-running `close_attack` after the fix; would have shipped the preview
  modes blind otherwise).

Untouched deliberately: flare/burst one-shots stay layer-0 (the original draws
the appear flare in the shared world; both views should see it), fade
behaviour (`OrbFade` scale stand-in) unchanged, colours/effects/timing unchanged.

## Proof (all local, main checkout `engine/shots/`, none committed)

- `orb_ownership_sheet.png` (the verdict, 6 labelled panels):
  BEFORE view1 attack = flame core + **pink fringe** (slot-12 cup leaking in);
  AFTER same cluster = clean flame (`orb_after_near.log`: cap 7, owner 1,
  view 1, slot 5). BEFORE view1 aura = white halo + smoke + **black crescent**
  (depth interleave of the two billboarded cores); AFTER = clean white smoke
  (log: cap 5/6, owner 1, view 1, slot 4). AFTER view0 = P1's own attack, clean
  flame (log: cap 2/4, owner 0, view 0, slot 5). Ghostcheck = slot-12 dark
  single next to slot-5 flame single, both through the live spawn path.
- Full frames: `orb_ownership_before_near_*.png` (pre-fix binary) vs
  `orb_ownership_after_near_*.png` (fixed binary + `PD_ORB_LOG`), same
  `--demo --near` setup; `orb_ownership_after_locked_*.png` (locked cameras,
  mostly walls — not useful, kept for honesty); `orb_ownership_ghostcheck.png`,
  `orb_ownership_after_close_attack.png`; reference footage frames consulted:
  `shots/orb/footage/{red_idle,collect_sheet,mat_close}.png` (original: dark
  core + coloured rim + gold motes; own white aura on pickup).
- Reference: user's correction supersedes docs — the fix restores the doc's
  program-read mapping (own = 4+kind bright, foe = 11/12 dark), which is also
  what the footage shows; no A/B judgement call was needed beyond it.

## Tests (foreground, this worktree, `PD_CACHE` = main cache)

- New `fx::tests::orb_presentations_both_players`: both owners × all 7 colours
  against the production helper (own = owner + `COLOUR_TO_ORB_SLOT`, incl. the
  environment→10 / special→9 order bridge; ghost = other view, 11 iff aura
  else 12; views/slots disjoint). New `orb_presentation_slots_disjoint`.
- New `fx::tests::spawn_orb_children_carry_their_views_layer` — pins the
  ACTUAL bug (the mapping tests above passed on the broken code too). Drives
  the real `spawn_orb` on a headless ECS `World` (one `Schedule` run = spawn +
  flush) with synthetic cache data carrying the real production per-draw roles
  (`kind_spec`: slot 5 = lit core + rim, slot 12 = lit core + multiply disc +
  rim), then censuses every real `Mesh3d` descendant for BOTH views: root and
  each draw child carry exactly the viewer's mask; each child is rendered by
  its own camera (`{0, 1+view}`, the `main.rs::spawn_scene` contract, which is
  also asserted to contain layer 0 + the view layer and exclude the other)
  and by NOTHING in the opposing camera. Masks resolve exactly the way the
  renderer resolves them (no component = layer 0). Child count is asserted
  equal to the kind's draw count, so the test cannot pass vacuously.
  Fail-before evidence: with the child mask application removed (the old
  root-only production behaviour) it fails at
  `draw child mask == viewer mask (slot 5 view 0)` — children resolve to
  layer 0, which both cameras include. Restored after; `git diff` confirms the
  only production change is the fix itself.
- Full suite: `cargo test -p pd_game -p pd_formats -p pd_convert -p pd_viewer`
  → **343 + 42 + 7 + 10 pass**, 1 fail:
  `sfx::the_footstep_wav_is_in_the_cache_and_the_candidate_is_not_wired`
  expects `engine/cache/sfx/common/35.wav` **in the worktree** (resolved via
  `CARGO_MANIFEST_DIR`, ignores `PD_CACHE`; absent here, present in main
  cache). Pre-existing worktree-cache gap, unrelated to this diff (only
  `fx.rs` changed) — not called green.
- Build clean, no new warnings (one pre-existing `menu.rs` parens warning).

## Uncertainties / follow-ups (not claimed)

- BEFORE panels have no spawn log (logging didn't exist pre-fix); their
  "blend" read is by texture identity (pink cup = slot 12 only; smoke =
  slot 11 only) against pure single-model previews, not by logged id.
- Demo runs are wall-clock driven: before/after cameras differ slightly; the
  sheet compares same clusters/kinds, not pixels (per-AGENTS.md harness noise
  note this is the honest comparison).
- No status capsule appeared in these runs (log: aura + attack only); the
  mapping test covers all 7 colours at unit level, kind coverage in-shot is
  aura + attack as briefed.
- Parked orb items from CAPSULE-VISUALS.md still stand (fixed key-light
  half-dark core visible in panel E; procedural motes default off; scale —
  not dither — fade).
- Request (wall lane owns `main.rs`): nothing needed there — cameras already
  expose layers {0,1+i}; no change requested.
