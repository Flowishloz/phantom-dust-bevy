# P0 audit — AURA (deep pass P5: the school aura)

2026-09-30, read-only audit. Sources: `H3-school-aura-handoff.md`, `school_aura.rs/.wgsl`, `cam_fade.rs`/`dissolve.wgsl`, `main.rs` camera setup, `decomp_all.c` (addresses below), shader disassembly (`tools/disasm`), `cache/fx/eft0600.fx.json`, footage stills. Nothing launched, nothing edited except this file.

## 0. Headline (changes the premise of P5)

**The original's school aura is not a silhouette rim. It is effect 600 ("オーラ", `pdEftAura01`) — skinned, additive wisp meshes bolted to the fighter's arm bones, in three Level tiers, tinted by a school colour table that is in the program.** The earlier lane searched by name, missed it, and called `pdEftAura01` "a boss/PSP transition object". It is the per-fighter battle aura:

- The aura object `DAT_1404bbee0` is created when a scene starts with mode flag set (`FUN_140128990` → `FUN_1401275d0`, 0x1d0 bytes, then `FUN_14019e280` preloads effect 600).
- **Every fighter tick** (`FUN_14024ec80`, fighter vtable slot 4, tail) it is fed the fighter's world matrix (`FUN_14019e5d0`) and the fighter's Level (`FUN_14019e6c0(aura, player, level)`), with Level forced to **0 when status 8 (invisible) is on** or the fighter's +0x20/+0x24 activity flags are zero.
- `FUN_14019e6c0`: **tier = min(3, (Level+3)/4)** → Level 0 none; 1-4 tier 1; 5-8 tier 2; 9+ tier 3. Effect 600 is started with sub-slot `DAT_1402dbb04[tier]` = **0 / 4 / 5** (bytes `00 00 04 05`). In `eft0600.fx.json` `slots = [[0],[],[],[],[1],[2]]`, i.e. slot 0 → model m0 (4 pieces: RHAND, LHAND, RTEKUB, LTEKUB), slot 4 → m1 (8 pieces, adds RNINOU upper arm, bigger), slot 5 → m2 (10 pieces, white-tinted, biggest). The "three models of escalating richness" found in CAPSULE-VISUALS are the three Level tiers.
- **Colour**: the effect is registered in a colour-override list (`FUN_14018e8e0`, 64 x 0x14-byte records {handle, r, g, b, player}). When a piece is drawn (`FUN_14011b450`, near `DAT_1405bb2e0 + 0x11088`) its RGB is multiplied by the override and **its alpha by the fighter's model opacity** (`*(DAT_1404bbd50 + player*0x120 + 0x1d0) / 255`).
- The override colour is indexed by the fighter's **school index at fighter +0x60**, computed by `FUN_14024d9d0`: count the 30 arsenal entries by skill-record field +0x12 (our s11 SCHOOL, 1-5), then walk schools 1..5 with strict greater-than → **ties go to the lower school id; none = index 0**. Table built in `FUN_14019e280` from `.rdata` floats (`exe_const.py frows`):

| idx | school | RGB float | RGB 8-bit |
|---|---|---|---|
| 1 | Psycho | 0.8745, 0.1569, 0.0588 (`0x1402f8860`) | DF 28 0F red |
| 2 | Optical | 0.8784, 0.7490, 0.0588 (`0x1402f8870`) | E0 BF 0F yellow |
| 3 | Nature | 0.0588, 0.9098, 0.1569 (`0x1402f8880`) | 0F E8 28 green |
| 4 | Ki | 0.0588, 0.1882, 0.8784 (`0x1402f8920`) | 0F 30 E0 blue |
| 5 | Faith | 0.8745, 0.0275, 0.8784 (`0x1402f8910`) | DF 07 E0 purple |
| 0 | none | same as Ki (`0x1402f8920`) | blue |

This **confirms the community guide's colours and tie order from the program** (previously unconfirmed) and replaces our hand-picked shades. The `0x1402dca00` table Laurence found remains the capsule colour table, as the handoff concluded.

- Look: `PDEFTAURA01` (vs/ps disassembled): 2-bone skinned mesh, output = tex1.rgb x (tex0 x tint x tint.a), premultiplied by tex0.w, two UV sets with scroll constants, second target `blend_colour`. Textures (`orb/eft0600_tex/tex0{0,2,4}`) are soft white S-shaped streaks on black (DXT1, additive). Per-piece tint words are grey-ish whites; the school colour is the multiplier. So the original is: **coloured, flowing wisps around hands/wrists (and upper arms at Level 5+), plus the glow pass softening them** (every pixel shader also writes the `blend_colour` second target — RESEARCH-LIGHTING A2; consumer of that target still not traced).

This matches Laurence's own words ("a simple glow with a subtle texture with wisps of aura"; "vertical wisp trails rising off the shoulders") better than any of the six rejected approaches, which all tried to *invent* a whole-body halo. The footage still (`shots/schoolaura/v2/vs_footage_v2.png`, left) shows a very soft red-pink cloud around head/shoulders/upper back trailing onto the floor behind, no geometry — consistent with arm wisps (upper arms sit at the shoulders) blurred by the glow pass, but **whether the original's cloud also has a body-silhouette component is not proven** (see Reads).

## 1. Ledger

Legend: Y done, N not done, ~ partial. "Mapped" = we know the original's rule (address/field).

| Feature | Mapped | Implemented | Reproduced | Verified | Laurence-approved | Evidence |
|---|---|---|---|---|---|---|
| Aura exists in battle; per-fighter, Level driven | Y | ~ (own mesh shell, not effect 600) | ~ | N | N (every version rejected) | `FUN_14024ec80` tail -> `FUN_14019e6c0`; `school_aura.rs:313 update_auras` |
| Gate: Level >= 1 | Y (tier formula; Level 0 -> none) | Y (`fighter.level >= 1`, `school_aura.rs:318`) | Y | ~ (rule agrees; no test cites `FUN_14019e6c0`) | guide-level only | `FUN_14019e6c0` |
| Gate: hidden while invisible (status 8) / inactive | Y (`FUN_14024d4d0(stats, 8)` -> Level 0, `FUN_14024ec80`) | N (`invisible_ticks` never read by the aura) | N | N | N | `sim.rs:439`, `moves/status.rs:253` |
| Level tiers 1-4 / 5-8 / 9+ -> different model (m0/m1/m2) | Y | N (one look at every Level) | N | N | N | `DAT_1402dbb04` = 00 00 04 05; `eft0600.fx.json` slots |
| Colour per school | Y (table above) | ~ (right hues by name, wrong values: Psycho pink (1,.28,.4) etc. by eye; `school_aura.rs:132`) | ~ | N | N | `FUN_14019e280`, floats at `0x1402f8860/70/80`, `0x1402f8910/20` |
| Dominant-school rule + tie order | Y (`FUN_14024d9d0`: strict >, schools 1..5) | Y (`school_aura.rs:113`, tested) | Y | ~ rule matches; test not address-cited | via guide | `FUN_14024d9d0` |
| No schooled skill -> blue aura | Y (idx 0 = Ki blue) | N (we show nothing, `school_aura.rs:318`) | N | N | N | same |
| Aura alpha = fighter model opacity (per view) | Y | N | N | N | N | `FUN_14011b450` (x `0x1d0` value / 255), `FUN_14024f2f0` (per-view fade min with `FUN_14015c930`) |
| Wall / camera-near dissolve drags the aura with it | Y (same 0..255 value) | N (aura shell ignores `cam_fade`; `cam_fade.rs` deliberately skips it) | N | N | dissolve itself: Y approved 2026-09-29 | FRONTIER O3; `cam_fade.rs` header |
| Geometry/look: skinned wisps on RHAND/LHAND/RTEKUB/LTEKUB (+RNINOU), UV-scrolled, additive, school-tinted | Y (assets + shader read) | N | N | N | N | `eft0600.fx.json`, `PDEFTAURA01_ps/vs` |
| Soft glow halo | ~ (second-target mask exists; blur chain `msRGBLUR00`/`GFilter`/`vk_glow` read; consumer not traced) | ~ (mesh shell, rejected) | N | N | N | RESEARCH-LIGHTING A2 |
| Motion trail / shimmer | N (only what the wisp UV scroll + glow give) | ~ (`FeetTracker` in lane-h3 only; not on main) | N | N | N | handoff |
| Consume pulse (saturation + opacity) | N (see Reads) | N | N | N | Laurence observed it in the original (FRONTIER O1) | FRONTIER O1 |
| Depth / occlusion behind scenery | Y by construction (effect draws are ordinary depth-tested additive, `FUN_14011b450`) | ~ (shell: depth-tested, no write) | ~ | N | N | |
| Per-view (split screen) | Y (alpha is per current view `DAT_140530060`; matrix/colour shared) | ~ (shared entity, every camera draws it) | ~ 2 views | N | N | |
| Floor light pool | N (no program evidence; only lane-h3 invention) | N on main (only in unmerged lane-h3) | N | N | never complained | handoff |

State of the tree worth knowing: **`main` still has the v2 shell** (`6028cac`, `2ee203a`). H3 r1/r2 (`e9011a6`, `82e194b`) and the v6 billboard are only on `lane-h3` (`git branch --contains 82e194b` = lane-h3); the handoff's "merged" is wrong. GAME.md "School aura" describes v2.

## 2. Discrepancies (most important first)

1. **CONFIRMED — wrong thing drawn.** Original: effect 600 wisps on arm bones (`FUN_14019e6c0`, `eft0600`). Ours: inflated body-mesh shell (`school_aura.rs:253 spawn_aura_meshes`, `school_aura.wgsl`). Visible effect: Laurence sees clear body geometry; the original has none. Ours has no wisps, no texture, no Level tiers.
2. **CONFIRMED — colours.** Original per-school RGB table above; ours `school_color` (`school_aura.rs:132`) Psycho (1.0, .28, .4), Optical (1, .82, .08), Nature (.15, .95, .25), Ki (.2, .5, 1), Faith (.68, .22, 1) — all brighter/different, and multiplied into a white-ish texture in the original rather than used as flat colour.
3. **CONFIRMED — Level tiers missing**: Level 1-4/5-8/9+ should change the model (hands+wrists -> +upper arms, bigger -> biggest, white).
4. **CONFIRMED — invisible status does not hide the aura** (original forces Level 0). Also original keeps showing a **blue** aura for an arsenal with no schooled skills; ours shows none.
5. **CONFIRMED (two sides read) — aura ignores the wall dissolve.** Original multiplies aura alpha by the same per-view model opacity that drives the stipple/dissolve; ours leaves a full-strength glow on a dematerialised body.
6. **SUSPECTED quirk (read, not proven):** in `FUN_14019e6c0`, a tier change stops the old effect only when the *new* tier < 3, so **moving from tier 2 to tier 3 within a life does not swap the model** (handle stays valid, nothing restarts); tier 3 shows only when the aura starts at Level >= 9 (round start `FUN_14019e430`). Fidelity question, low priority (Level >= 9 is rare).
7. **CONFIRMED (docs) — stale claims.** `VFX-AUDIT.md` item 6 / `school_aura.rs` module doc ("`pdEftAura01` is a boss/PSP transition") and `GAME.md` "no per-character shader or colour table" are now wrong. The menu-side effect-600 callers (`FUN_1401620f0`, builder screen) are a second, separate use.

## 3. Reads still needed

- **Consume pulse.** No code read that pulses the aura. Candidates: (a) `FUN_14019e6c0` restarts effect 600 on a tier change (only Level 0->1, 4->5, 8->9, so cannot explain every consume); (b) the consume clip `SPE_POWER_UP` or the skill's own effect starts another effect or raises the override colour/alpha (look for `FUN_14018d2f0`/`FUN_14018e8e0` callers from the capsule-take path `FUN_14014fd10`, state 10-13 handlers, and what the fighter's slot-4 tick does in states where `+0x12f04` is the consume state); (c) the 0x1d0 opacity value. Needs Laurence's eyes first (Q3).
- **Effect 600's runtime behaviour:** piece `frames [a,b,c,d]` (e.g. [40,360,25,5] / [85,360,30,10]) and `puVar8[5]=1` (loop/hold flag set after start in `FUN_14019e6c0`) — does it loop, how long is one cycle, what do the four words mean for fade in/out? Start: `FUN_14018a040` (param_4 = sub-slot, `+0xf4 + slot*0x1c` flags), piece draw `FUN_14011b450`, UV scroll constants c128-c132 (same 0x13-chunk path as capsules: `FUN_140295f90`, `FUN_14023c440` family).
- **RESOLVED 2026-10-01 (lane auralook): the soft halo is the HUD aura object's silhouette blur (`FUN_14019b920`, `GFilter2`, `AuraBox`), not the wisps and not a character shader pass; see FORMATS-EFFECTS.md "School aura glow". Its width is still UNCONFIRMED (needs a live trace). The next bullet is the old question, kept for history.**
- **Glow pass consumer (old):** who samples the second colour target and how wide is the blur (`FUN_14001cc60` param 3, `msRGBLUR00/01`, `GFilter`, `vk_glow` users; tap offsets and weights are CPU constants). If the pass is real, the soft cloud in the footage is the original's glow blur of the wisps; if not, the cloud is the wisps alone. Also why every `blend_colour` writer I found (`FUN_140020660(0x4b, ...)`) is UI fade/alpha (`FUN_1402a6010`, `FUN_14024a780`, ...): which path gives a *character* draw a coloured second-target value.
- Who else instantiates `alsh_ShaderSAURA` (`0x14048f798` vtable, RTTI `0x140356ce8`) / `AuraBox`: flat-colour, screen-offset draws (look like HUD / aura-meter art, `auraInit`/`auraMove` SSB, `FUN_14019ac40` x4 objects). Probably not fighters; unconfirmed.
- Which fighter flags +0x20/+0x24 mean (both must be non-zero for the aura to show).
- Bone placement check: render eft0600 on each playable character (bulky, coat) to see whether the authored wisps sit right on every skeleton — the effect plays on `RHAND` etc. for all of them, but only pc00a was ever seen.

## 4. Architecture proposal for Bevy 0.19 (two layers, one decision for Laurence)

Facts checked in 0.19.1 source: cameras take an `Image` via the `RenderTarget` component (`bevy_camera::RenderTarget::Image`); custom passes are systems in the `Core3d` schedule sets (`bevy_core_pipeline::schedule::Core3dSystems`), with `FullscreenMaterial` as a ready-made post hook; the game today has **no HDR, no Bloom, no custom render code** and one `Camera3d` (4x MSAA) per player drawing the shared layer 0 (+ layer `1+i` private) — `main.rs:952`.

**Layer A — the faithful aura (build first, all high-level API).** Play `eft0600` through the existing effect runtime (`effects::play(name, slot, at, looping)`, slot = 0/4/5 by the tier formula; `effects.rs:1143`), parented to each `PlayerVisual`, exactly like skill effects. Needs three small additions: a per-instance **RGB tint** (school table), a per-view **alpha multiplier** (read the same coverage `cam_fade.rs` already computes per fighter per view), and start/stop on tier change / invisible / Level 0. Pieces are skinned (`VERTFMT_EFFECT_SKIN` already exports since 2026-09-28) and additive, so depth occlusion, split-screen and dissolve behave like every other effect. No mesh shell, no normals, nothing that can show body triangles.

**Layer B — modern silhouette-aware glow (add only if A alone does not reach the footage's soft cloud).**
- Per-view **mask camera**: a child of each `PlayerCamera` (copies transform/projection automatically), `RenderTarget::Image` at half resolution, cleared black, `Msaa::Off`, no tonemapping/fog, `order` below the player cameras, rendering only a dedicated `RenderLayers` layer. Mask geometry = a flat-colour child per body piece **sharing the piece's `SkinnedMesh` and source mesh** (the exact child pattern `spawn_aura_meshes` already proves; no inflated copy, no rebuilt normals), using the piece's alpha cutoff (hair/strap cards) and the dissolve discard so the mask thins with the body near a wall. Optionally also the wisp pieces at higher weight.
- **Channel per fighter**: RGBA8 = fighters 0-3 (two used now; four later costs nothing), additive, so overlaps stay independent.
- **Composite** as one depth-tested, non-depth-writing additive billboard quad per (fighter, view) on that view's private layer (`1+i`), reading that view's mask. Hardware depth test vs scenery gives occlusion for free (no depth-texture read, no MSAA depth resolve), and the quad sits at the fighter's depth so a wall between camera and fighter hides the glow while scenery behind the fighter gets glow over it.
- Shader: one pass of ~24-48 jittered taps of the mask (golden-angle spiral, per-pixel rotation), halo = blur x (1 - mask) (outside the body only, no mesh edge can show), optional thin inner rim from a second small radius; **wisps** = taps displaced upward by animated fbm (flame-style domain warp) so the halo licks upward; **trail** = taps also displaced against the fighter's screen-space velocity (from `FeetTracker`-style frame diffs, render-only). Colour = school table x aura alpha (Level tier strength, consume pulse, per-view dissolve coverage).
- Cost: mask targets total about one half-res full window's pixels whatever the player count (4 quarter-screen views = same area as 1); quads cover a few percent of each view; skinned pieces are drawn one extra time per view (~5-10 draws per fighter). Negligible next to the stage.
- Interactions: **wall dissolve** — mask uses the same discard and alpha uses the per-view coverage, so body, wisps and halo thin together (cam_fade.rs stays untouched, only read; its `view.viewport.x + y` side test is the 2-view hack that P6 must generalise — note for 4 players); **4 players** — one mask camera and one composite quad per view, channel = player; **rematch/costume swap** — masks follow `HasAuraShell`-style markers; **MSAA writeback** caution (the `main.rs:934` comment): mask cameras render to images, never to the window, so they do not join the window writeback chain.
- Rejected alternatives: fullscreen post using the main depth prepass (MSAA depth + per-view plumbing, more code for the same result); another inflated/analytic body proxy (the six failures); `Bloom` (needs HDR on the camera and brightens everything; per-object colour would need emissive masking anyway).

**Decision needed:** the program evidence says A is the original. B is our own answer to "soft halo like the footage" and is justified by the footage plus the original's glow pass, not by an addresses-read mask colour. Build A, show it, then decide on B.

## 5. Prototype plan (shown to Laurence early, close camera)

1. **Proto A (small):** eft0600 on both fighters, school tint, tiers by Level, hidden at Level 0 / invisible. Env switches `PD_AURA_FX=0/1` so old shell and new run side by side. Captures FIRST at the close camera (`--near`, st06): slim, bulky (his usual), coat fighter; Level 1 / 5 / 9; all five schools; moving and standing; both views; behind a prop; against a wall. Use `PD_SHOT_TICKS`; crop small (big sheets time out).
2. **Show the A–F sheet** (his preference): A old shell, B wisps only, C wisps + halo small, D wisps + halo wide, E halo only (full silhouette), F wisps + halo + trail, next to footage `Lock-On Animation.mp4` stills. He picks; nothing tuned by eye before that.
3. **Proto B** behind `PD_AURA_HALO=1`, one view, close camera, then split view, then wall-dissolve cycle and a moving strafe for the trail. A live pad test early (handoff's main lesson).

## 6. Proposed build lanes (ordered)

**Lane 1 — faithful aura rules + wisps (first).** Owns `school_aura.rs` (rules: `tier_for_level` from `FUN_14019e6c0`, `school_color` table, dominant school incl. idx 0 = blue, invisible/Level 0 gate, tier-2->3 quirk behind a const), a new small `aura_fx.rs` driving `effects::play`, `school_aura.wgsl` removed once replaced, and a tiny tint/alpha hook in `effects.rs` (coordinate with the P3 effects lane; add, do not change behaviour). Tests: tier table (Level 0,1,4,5,8,9,50 -> none/slot 0/0/4/4/5/5, citing `0x14019e6c0`/`0x1402dbb04`), colour table bytes read from the exe-derived constants, dominant-school tie order and all-zero -> idx 0 (`0x14024d9d0`), invisible hides, alpha = per-view cam_fade coverage. Runtime proof: proto-A captures above; log of start/stop on consume tier edges.

**Lane 2 — silhouette halo (after Lane 1 approval, only if Laurence wants it).** Owns new `aura_halo.rs` + `aura_halo.wgsl` (mask camera, mask material, composite quad), a few lines in `main.rs` `spawn_scene` (mask camera child), a read-only accessor in `cam_fade.rs`. Tests: mask channel per player, camera parenting/resize, dissolve coverage reaches mask and alpha, no sim state read/write. Proof: split-view and 4-view headless layout test, wall cycle, scenery occlusion capture.

**Lane 3 — program reads (parallel, no code):** consume pulse, effect-600 frame words and loop, glow-pass consumer and blur widths (Reads above). Feeds both lanes.

## 7. Questions only Laurence can answer

1. Which target? (a) the original's wisps only, exactly as the program does it; (b) wisps plus a soft glow halo around the body (recommended: footage reads soft); (c) glow halo around the whole silhouette only, as you first described.
2. When you consumed an Aura Particle in the original, what did you see? (a) the whole glow flashes brighter/more solid for about a second; (b) the wisps burst or swell at the hands; (c) it only happens the moment the aura first appears or grows; (d) not sure, will record it.
3. At Level 5+ the original's aura gets bigger wisps reaching the upper arms, and at 9+ white and biggest. Do you have footage of a high-Level fighter to compare? (a) yes, will share; (b) no, go by the program.
4. Floor light pool under the fighter (from the rejected versions, not in the program): (a) drop it; (b) keep as a soft extra; (c) only when the halo is on.
5. Fighter with no schooled skills in the arsenal: the original shows a blue aura. (a) match the original; (b) show nothing.
