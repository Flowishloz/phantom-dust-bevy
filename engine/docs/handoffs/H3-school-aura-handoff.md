# Handoff: school aura (H3 job 1) — after three failed rounds

**For:** the next model taking over the school aura. **From:** Hermes/GLM-5.3 lane, 2026-09-28 (late). **Verdict up front:** Laurence rejected every version. His live words, in order: "unsatisfactory but playable" (round 2, merged as `82e194b`), then "worse than ever. It's boxy, has no trails, no animation", then "still bulky. I can see clear defined geometry… Your approach looks completely wrong", then on the analytic rebuild: "utterly failed". He asked for a handoff "for everything you tried so a better model can take over". This is that document. Read it before writing any code — every wrong turn below was taken with green tests and a proof sheet that looked fine.

## What Laurence actually wants (the bar)

From his brief and reference stills: **"a simply glow with a subtle texture with wisps of aura"** around the player — soft, saturated from the inside out, no hard edges, no visible geometry, subtle solar-flare shimmer that trails behind movement, plus a soft floor light pool. The original is a PS2 effect; modern techniques are welcome, but the *result* must read as glow around a body, not a shape on a body.

Reference material:
- Footage frame: `engine/shots/schoolaura/v2/vs_footage_v2.png` (left panel = original; middle/right = our old v2), clip `%USERPROFILE%\Pictures\Phantom Dust - Moves 2\Lock-On Animation.mp4`.
- His two reference stills were pasted into chat (Hermes composer temp images, may no longer exist on disk — re-ask him if needed). The Nature-school still shows clear vertical wisp trails rising off the shoulders.
- **Ask him for fresh screenshots early.** My single biggest failure mode: proof sheets rendered at the demo camera's far distance passed review, then failed instantly in live play up close on his bulky character. Close-camera, bulky-character captures FIRST, then a live test EARLY — do not iterate three rounds on far-camera sheets like I did.

## Every approach tried, and why it failed

| Version | Approach | Verdict | Why it failed |
|---|---|---|---|
| v1 (6028cac) | Camera-facing oval "hoop" quad | Rejected | Reads as a ring, not a silhouette glow |
| v2 (2ee203a) | Inflated copy of the character's own skinned mesh, back faces shaded `(-N·V)^k`, additive | Rejected after two live looks | Hard-edged band; at close range the inflated hair/coat triangles show as flat pink panels |
| H3 r1 (e9011a6) | Same shell, wider/softer, hard cap `min(i, 0.25)` | Rejected ("unsatisfactory but playable") | The hard cap flattens every hot triangle to ONE value — the cap itself draws the flat panels and a hard outer edge |
| H3 r2 (82e194b, merged) | Soft knee `cap·(1−e^(−i/cap))`, width 1.9, falloff 1.4, gain 0.5/cap 0.45, mottle + flare filaments + velocity trailing (FeetTracker), 13 u floor pool | **Live-rejected**: "worse than ever — boxy, no trails, no animation" | (a) proof sheets were far-camera so everything tuned too subtle to read in play; (b) the white-blend `k^2` flattened whole shell triangles = slabs; (c) the trail/shimmer were invisible at play camera speed |
| H3 r3 hotfix (never committed; shipped to PLAY.bat briefly) | Veil off, white `k^3` 0.35, flare 0.55 / rise 5 u/s, mottle 0.35, trail 3.0 | **Live-rejected**: "still bulky, clear defined geometry" | Fundamental: the glow IS the character's own mesh, inflated. Up close on a bulky character you see the mesh. No shader can hide that. Laurence: "I feel it doesn't need to be a 3D mesh and more of a shader around the player" |
| v6 analytic billboard (uncommitted in the worktree, currently in PLAY.bat) | One camera-facing quad per fighter; fragment shader sums ten spherical "probes" fitted to the body (head 17.3u/1.7r, chest 12.5/2.6, shoulders ±2.3·14.3/1.8, pelvis 8.2/2.4, thighs ±1.5·4.8/1.9, shins 1.6/2.0, two velocity trail lobes); shell-profile `exp(-((d−1)·2.2)²)` peaked at each sphere's surface; cutoff `step(d,2.6)`; head trim `smoothstep(20,26)`; floor-spill guard near the feet axis; white blend `k^6` amount 0.22; gain 0.62, cap 0.45; mottle 0.35; flares 0.55 @ 5 u/s; trail stretch 2.6×tune.w | **Live-rejected**: "utterly failed" | Do NOT assume this is just unpolished. The probe-envelope idea itself did not survive contact: the envelope doesn't track the animated silhouette (it's spheres around a chest pivot, not the actual pose), so it reads as a blob/cloud that doesn't hug the body. Laurence saw through it immediately |

**The pattern across all six:** anything keyed to the character's MESH shows the mesh; anything keyed to SPHERES around the body doesn't track the pose and reads as a blob. Three mesh variants, one hoop, one billboard-probe variant — all rejected. The remaining untried idea that matches Laurence's own framing ("a shader around the player with peripheral elements") is in the next section.

## The strongest lead not yet followed (from Laurence + the RE lane)

Laurence found a colour table in the program: **`0x1402dca00`, eight ARGB entries — white, red, blue, purple, green, yellow, aqua, zero.** I chased it and it is **NOT the school colour table**: it's the capsule/card-UI colour table (the consumers `FUN_14021ba60` / `FUN_14021b840` read the skill record's *capsule* field — program records start at our s2, so in-memory `+0xe` = our s9 CAPSULE, not s11 SCHOOL; capsule values run 0–5, exactly six kinds + white for 0, plus a zero terminator). Consequence: the aura's colour source is still the community guide (Psycho red, Optical yellow, Nature green, Ki blue, Faith purple, ties broken in that order), **unconfirmed against the program**. But the seven capsule shades are real data — free for CAPSULE-VISUALS.md / Job 5: attack `FF3333`, defence `3333FF`, erase `FF33FF`, status `33FF33`, special `FFFF33`, environment `33FFFF`, white for none.

The RE-lane note that should drive the next attempt (2026-09-28): *"the glow mask colour comes from `cb0[17]`, the material's own slot, and vkCOLANIM — the dissolve family — writes it too. If the aura wants to glow, that constant is where it goes."* **Nobody has read what writes `cb0[17]` for the character material, or what mask/texture it gates.** If the original's Level≥1 glow is a dissolve-mask pass on the character material driven by that constant, the right modern reproduction is neither a mesh shell nor body spheres — it's a **silhouette-masked glow**: mark the character's screen pixels (or its UV silhouette), glow outward from THAT mask in screen space. That tracks the animated pose perfectly by construction, cannot show mesh triangles, and is exactly "a shader around the player". Also already searched and not found (don't redo): no rim/fresnel/outline class names in the decompile; `pdEftAura01` is a boss/PSP transition thing, not this effect.

## What survives regardless of approach (keep these)

- **`FeetTracker`** (`school_aura.rs`): velocity from frame-to-frame `PlayerVisual` transform diffs, teleport-guarded, never sim state. Any trailing effect needs it.
- **Floor pool**: unlit additive `StandardMaterial` quad under each fighter, radius 13 u, gain 0.55, colour fed as **linear** (an earlier `Color::srgb()` wrap double-decoded it to pastel — `Color::linear_rgba(...)` is correct). Laurence has never complained about the pool itself.
- **Every knob is an env override** (current defaults): `PD_AURA_WIDTH` 1.9, `PD_AURA_FALLOFF` 1.4, `PD_AURA_GAIN` 0.62, `PD_AURA_CAP` 0.45, `PD_AURA_HEAD` 0.88, `PD_AURA_ARM` 2.0, `PD_AURA_RIM` 0.15, `PD_AURA_MOTTLE` 0.35, `PD_AURA_FLARE` 0.55, `PD_AURA_RISE` 5.0, `PD_AURA_TRAIL` 3.0, `PD_AURA_WHITE` 0.22, `PD_AURA_VEIL` 0.0 (off — his explicit rejection), `PD_AURA_FLOOR` 0.55, `PD_AURA_FLOOR_R` 13.0, `PD_AURA_GLOW_R` 1.0, `PD_AURA_PROFILE` (1 = soft knee, 0 = old hard clamp, kept for before/after sheets). Tuning without rebuilds is possible; structural changes need one.
- The noise helpers in the shader (`hash13`/`vnoise`/`fbm2`, t = `globals.time * 30` counts 30 fps frames) and the wisp/flare recipe that DID read in motion once the amplitudes were raised.

## Current state of the tree (do not lose this)

- Worktree `%USERPROFILE%\Documents\GODOT\pd-lane-h3`, branch `lane-h3`. Last commit `82e194b` (H3 r2, merged-era school aura). **Uncommitted in the worktree**: the v6 analytic billboard rewrite (3 files: `main.rs` one line, `school_aura.rs` full rewrite ~533 lines, `school_aura.wgsl` full rewrite ~166 lines). Tests were 276 green at last run. This code is the rejected approach — decide whether to keep it as scaffolding for the silhouette-mask version (the billboard spawn/update/visibility plumbing and FeetTracker ARE reusable) or `git checkout -- .` and start from `82e194b`. Either is defensible; the shader field code is not worth saving.
- **`main` has moved** (a sidestep commit, `36e0f60`, landed while this lane ran). `lane-h3` is now both ahead (its 2 commits) and behind. Per the handoff rules, recreate the worktree from current `main` (`git worktree add -b lane-h3b ..\pd-lane-h3b main`) and re-apply what's kept — do not switch branches in the shared folder.
- `PLAY.bat`'s exe (`engine/target/debug/pd_game.exe` in the main folder) currently holds the v6.3 analytic build. Overwriting that exe is fine and expected (build output, not source).
- Tests: `cargo test -p pd_formats -p pd_game -p pd_convert` must stay green (7 + 36 + 276 at hand-off). `school_aura.rs` tests cover dominant-school pick/tie order/no-school, and (v6) probe geometry — replace/extend if the approach changes. Never commit shots/cache/game_files; never push.
- Frozen-file rule for this job: `school_aura.rs`, `school_aura.wgsl`, `GAME.md` "School aura", `VFX-AUDIT.md` item 6. `sim.rs`/`combat.rs`/`moves/`/`arsenal.rs` are off-limits (render-only lane).

## Environment quirks that cost me time (Windows + MSYS + Bevy 0.19)

- `CARGO_TARGET_DIR` must be Windows form (`%USERPROFILE%\.pdtools\target-h3`), never `/c/...` — cargo is native and turns the MSYS path into `C:\c\...` (I built 3.2 GB into a junk tree before noticing; the exe timestamp not moving was the tell).
- Screenshot paths passed to the game must be Windows paths too; `$PWD` in an argument breaks native exes ("Cannot save screenshot, IO error 3").
- `. bashrc` lines I added: `export PD_ENGINE="%USERPROFILE%/Documents/GODOT/pd-lane-h3/engine"` and PATH with mingw64 + `~/.cargo/bin`. `rustfmt` is not installed for the GNU toolchain — lint errors from the patch tool are pre-existing noise, ignore them.
- WGSL (naga): no swizzle assignment (`sp.xz -= ...` is a compile error — rebuild the vector); `smoothstep(...).clamp(...)` chaining also failed — use `clamp(fn(...), 0.0, 1.0)`.
- Vision analysis of big sheets times out — analyse small crops (≤ ~1–2 MB). And always compute a numeric pixel-diff between frames when claiming "animation is visible".
- The demo camera sits FAR and the demo character is slim; Laurence plays CLOSE on bulky characters. Capture accordingly (see "the bar" above).

## If you take one lesson

My proof sheets said "same effect at a glance" three times; Laurence's eyes said no three times. The vision-model scores were measuring the wrong thing (structure at distance, not feel up close). Ground every round in (1) close-camera captures on the bulky character, (2) a live test as soon as anything moves, (3) his words over the model's verdict.

## Suggested first day (opinion, clearly labelled)

1. Read the `cb0[17]` lead: which shader(s) read it, what texture/mask it gates, what writes it and when (Level? a status?). `%USERPROFILE%\.pdtools\re-out\decomp_all.c` + `functions.txt`, disasm.exe for the HLSL, `tools/Esper-RE` reference-only.
2. If it confirms a mask/dissolve glow: prototype a **screen-space silhouette glow** — render the character's mask (stencil or offscreen), blur/glow it outward in a post pass, tint by school, add wisps as noise advected upward + FeetTracker trails, keep the floor pool. That is the only remaining approach that is "a shader around the player", tracks the pose by construction, and cannot show triangles.
3. Capture close + bulky first. One variant, live test, iterate HIS notes, not sheet scores.
4. Then, and only then, consider the school colour table hunt finished (it is: capsule table — see above) and move to Jobs 2–5 of `../archive/handoffs/H3-polish-batch.md` in order, one commit each. Job 2's half-done analysis is at the bottom of this file.

## Jobs 2–5 status (untouched by this lane)

- **Job 2 (Psycho Knife ray streaks)**: half-analysed. FLAIR01 drawing is a flat wedge (tip corners `width` apart at `length`, base at centre — `effects.rs::flare_mesh`). Psycho Knife's actual words (`cache/fx/eft0003.fx.json`): random flags ON (w24/w25), length words 4/5 = 50/50 with spans 6/7 = 10/10 → net span **−40** (mirrors the wedge), width words 8/9 = 20→0 with 10/11 = 5/5 → spans **−15 → +5**; 16 rays/frame, 4-frame life, frames 13–14 on RHAND. Suspicion to check against `FUN_1401a1620`: the width words may not mean tip-width, and/or the negative spans should clamp rather than mirror. Footage wants thin bright streaks (`engine/shots/vfxaudit/scale/sheet_psychoknife_zoom.png` top row). Also measure the ~4-tick press→cast-sound lag and report it (do not change sim timing). Regression check Psycho Wave + Crystal Wall before/after.
- **Job 3 (Slide Laser crescent)**: `effects.rs` + FORMATS-EFFECTS `m0_d4..m0_d8` family easing (word 9/10 both read as curve 2). Blazes within 150–300 ms, gone by 800 ms; ours ramps linearly and peaks late. Tick-matched sheet vs `engine/shots/vfxaudit/charge/sheet_slidelaser_vs_footage.png`.
- **Job 4 (lock-on scythe crescent)**: `hud.rs` marker only; keep range colours.
- **Job 5 (orb colours)**: now has REAL capsule shades from `0x1402dca00` (above) — start there instead of guessing; per-kind ring strength; render all-kinds before/after.
