# Lighting research — the original's effect lights, and a modern look we can afford

> **Status (archivist, 2026-10-03):** research. What shipped: bloom is `glow.rs` (Options > Lighting and look > BLOOM; `PD_GLOW=0|1` is the override, there is no `PD_BLOOM` and no `fx_post.rs`), shadow and effect-light options are in `FRONTEND.md` "Look options", the school-aura red glow answer lives in `GAME.md` "School aura". The `school_aura.wgsl` shell referenced below is gone.


## Current acceptance correction — 2026-09-30

Laurence reports the recreation needs a dedicated lighting pass: directional
lighting is not apparent in the result. A DirectionalLight exists in code, but
shadows are disabled and stage lighting is largely baked vertex colour. Earlier
claims below that the current build is "already faithful" are **not accepted
visual proof**. Revalidate the research against current materials and runtime.
**CLAUDE-DEEP-PASSES.md P4** plans direction/depth, mood, shadows/effect lighting,
reference-stage comparisons and four-view performance. P5 is a separate modern
school-aura rebuild; the shell approach praised below was subsequently rejected.

2026-09-28. Research lane: no code changed, no commits. Part A was read out of the game program (`PDUWP.exe`) with Ghidra; Part B was read out of the Bevy 0.19.1 source we already have on disk. Addresses are given so anything here can be re-found; no game code is copied into the repo.

## The short version, first

**The original does not have bloom. It has something better, and we can copy it.**

The 2004 game drew **two render targets per split-screen view** and wrote a *constant colour* into the second one on every single draw, then blurred it and added it back over the scene. The result is a soft coloured halo that leaks out from behind whatever drew brightest — the thing modern engines sell as "bloom", except it is tinted per object rather than derived from the image. That is a real, cheap, faithful glow, and Bevy 0.19 has everything needed to do the modern equivalent. (One caveat, checked later: Bevy 0.19 has no MRT, so we reproduce the *result* with emissive-threshold bloom, not the mechanism.)

**The effect lights are simpler than I first reported, and that is good news.** ✅ verified from the shaders: the original has a small effect-light table (**16 lights at once**, corrected 2026-09-28), but **the stage shader does not read it at all**. Stage geometry is lit by exactly one directional and one point light, **per vertex**. The effect lights feed **characters only** — and we already do that. So no lightmap, no clustered lighting, no new lighting work is needed for fidelity. Our current build is already the faithful one.

**What actually needs building is short:** glow, a per-stage colour grade, shadows, and anti-aliasing for the 4K target. Everything else is optional.

**Two claims I made earlier in this doc were wrong and are corrected in place:** the stage/character light-consumer split (above), and the knife ray width (our math was already right). A third, the **32-light cap, is actually 16**. All three were caught by reading the shader or the code rather than trusting an earlier note.

Everything below stays render-side. Nothing here touches `sim.rs` or `combat.rs`.

---

# Part A — the original's effect look

## A1. LIGHT02: what an effect's point light actually is ✅

Confidence: ✅ read from the code and the data, with the numbers cross-checked against `cache/fx/*.fx.json`.

- **Where.** The emitter class is `pdEftLight02`; its update is `FUN_14010c990` (spawn/wire-up `FUN_14010c480`). 511 uses across the converted effects.
- **The light table.** A single global array at `0x1405a6a40`, constructed in `FUN_1400037d0` as `0x100` 8-byte words with stride **0x70** → **64 records** total. The first **4 records** (index +0x40..+0x43) are the **per-view stage light set**; the dynamic effect lights occupy index **+0x80 .. +0xBF** (64 records).
- **How many at once: 16 effect lights, not 32 — corrected 2026-09-28.** I originally derived "32" by arithmetic on the 0x70 table. The binding limit is a **different** array: the emitter's free-slot pool spans `0x1405aa240`..`0x1405ab040` at stride `0xe0` (`do { … puVar20 = puVar20 + 0xe; } while (puVar20 < 0x1405ab040)`), which is exactly **16 entries**. Each allocated slot makes the emitter write **4 records — one per split-screen view** (`(slot + 0x80) × 0x70`). So **16 emitters alive at once × 4 views = 64 records**, which fills +0x80..+0xBF exactly. ✅ The two arrays agree, so **16 simultaneous effect lights** is the real cap, and a 17th steals the oldest slot (first-come, first-served). In 2-view split-screen the same 16 emitters are shared, so per-view you see at most 16.
- **Colour, radius, duration.**
  - Colour: the emitter's words 12 → 13 (ARGB), lerped across the light's life.
  - Radius: words 0 → 1, lerped the same way. Psycho Knife's `eft0003` LIGHT02 is **0 → 1000**. Across all 511 emitters the authored radius clusters hard: 1000 (167 effects), 1500 (55), 1200 (39), 2000 (32), 200 (38), 3000 (10) — these are **world units, 1000 ≈ 80 body heights** (a character is ~20 u). These are big area washes, not tight practical lights.
  - Duration: the effect frame window (Psycho Knife: frames 20–30, i.e. 10 ticks = 0.33 s).
  - Fade: the per-view light update `FUN_14019a240` runs a **15-frame (0xf) fade-in, then exponential decay**; a separate one-shot "flash" path (`DAT_140527c80/8c`) snaps a light to full and releases it after its own timer, easing between 0.999 and 1.001 to detect arrival.
- **Falloff — this is the good bit.** The program keeps a **1024-entry attenuation lookup table** at `0x140528cd4`, rebuilt when the light moves (`FUN_14019a740`), indexed by distance clamped to 0..100 (world units, `DAT_1402f8550..5c` hold the basis). Two regimes:
  - distance ≥ 100 u → a **straight linear ramp** `i / 1024` (0 at the light, 1 at 100 u).
  - distance < 100 u → **`pow(i/1024, (100 − d) × 0.01)` remapped to [0.5, 1.0]** — a softer, flatter core near the light so it doesn't blow out.
  - So it is a hand-tuned, distance-adaptive curve, not inverse-square. Our current flat additive decal (`effects::FxLight`, FORMATS-EFFECTS "LIGHT02") is close in spirit but is a screen-plane circle, not a shaded light.
- **What consumes them — CORRECTED 2026-09-28. Characters yes, stages NO.** I had written that both the character and stage shaders read the light table. Checking the actual disassembly: **`SD_ps` (the stage pixel shader) reads only `cb0[0]`, `cb0[16]`, `cb0[17]`, `cb0[23]`** — the stipple mask, the alpha-kill threshold, `blend_colour` and `misc_constant`. There is **no light array and no light loop** in it. `SD_vs` (the stage vertex shader) carries `cDL_COL`/`cDL_VEC`/`cDL_SVEC` (one directional light) plus `cP0_COL`/`cP0_POS` (one point light) and does the lighting **per vertex**, which is then interpolated. `SWBOS_ps` (the character pixel shader) does read `cP0_COLp`/`cS0_COLp`/`cS1_COLp`/`cDL_COL`/`cSPECULAR` — the same small fixed set.
  - **So the original's stage geometry is lit by exactly one directional and one point light, per-vertex, with no effect lights at all.** ✅ verified from the shader constant lists and the instruction streams of both stages.
  - **The purple floor wash is therefore not stage shading.** It is a separate additive floor decal — the same kind of thing our `FxLight` already draws — or the stage's own `cP0` point light being re-aimed. The effect-light pool feeds **characters** only.
  - This is a better result than my original claim: it means the original's effect lights are *cheap by construction* (characters only, 2-4 of them on screen), and our modern equivalent doesn't need a lightmap at all — **baked vertex colours plus a floor decal reproduces it exactly**, which is what we already do. The lightmap idea is demoted to an optional extra (see below).

**Revised practical consequence for us:** our current approach — baked stage vertex colours, a `FxLight` additive floor decal, real lights on characters only — **is the faithful one, and it's already built.** No lane 2 lightmap is required for fidelity. What a lightmap would still buy is a per-stage mood grade and free indirect bounce for the enhanced tier; keep it as an optional Part B item, not a fidelity requirement.

**One thing this leaves open** ⚠️: if the floor wash is a decal rather than stage shading, the *program's* floor-decal system is the thing to look for (an additive quad drawn on the floor under the light), not a light record. Not read; and no longer blocking, since our decal already approximates it.

## A2. The second render target: a tinted glow mask, not a blur ❓→✅

> **Correction 2026-10-01 (lane glow): A2 is wrong about the second render target.** `o1 = blend_colour` is the D3D11 dual-source blend input, one render target is bound, the "four surfaces" are four samplers, and GFilter / msRGBLUR are not a bloom chain; the original draws no scene glow. See FORMATS-EFFECTS.md "The glow pass: re-read 2026-10-01". The build that followed is a modern optional bloom (`glow.rs`). **Narrowed 2026-10-01 (lane auralook): "no scene glow" still holds, but the original does blur one thing -- the school aura's own silhouette tile (`GFilter2` is referenced, by the HUD aura object); see FORMATS-EFFECTS.md "School aura glow".**

Confidence: ✅ for the mechanism (disassembly of 8 shaders), ⚠️ for the final composite step (located the surfaces, did not trace the last blit).

- **Every game pixel shader writes two targets.** `ps_4_0` with `dcl_output o0.xyzw; dcl_output o1.xyzw` — confirmed on `SD_ps` (stage), `SWBOS_ps` (character), `PDEFTBB01_ps`, `Flair1P_ps`, `Part1P_ps`, `Blade01P_ps`, `GFilter_ps`, `vk_glow_ps`.
- **Target 1 gets a constant, not the image.** Every one of those shaders ends with the same instruction: `mov o1.xyzw, cb0[17].xyzw`. `cb0[17]` is the constant buffer entry the disassembly names **`blend_colour`** (offset 272, 16 bytes). So the second target is a **flat per-draw colour stamped over the draw's screen area**.
- **What it is for.** The blur shaders tell you the shape of the pass:
  - `msRGBLUR00_ps`: 3 texture taps, weighted by `cb0[0..2]`, summed → `o0`. A 3-tap blur.
  - `GFilter_ps`: 4 taps, a weighted cross/diamond kernel, then **`dp3` with the constant `(0.15, 0.30, 0.05)`** — a green-weighted luminance test — then an alpha-kill against `cb0[16]`. It is a **threshold-and-blur** pass: take the mask, keep the bright parts, blur, add back.
  - `vk_glow_ps`: 1 tap × colour, saturate, add. That is the **composite back onto the scene**.
- **Size and count.** The frame path `FUN_14001cc60` calls the device's `SetRenderTargets` (vtable +0x50) with **Count = 4** and an array of 4 surface pointers at `0x1405b0760` — **one colour target per split-screen view**, with `+0x58` binding each view's depth and `+0x48` clearing it. Surfaces are created through `FUN_14001ca40` / `FUN_14001c0b0` (D3DX helpers, `D3DCREATE_MULTISAMPLE` at 0x15) and swapped on resolution change (`DAT_140594a24` holds the last width).
- **Two surface kinds, and the mask is the small one (2026-09-28).** `FUN_14001cc60` maintains two D3DX-image descriptors in a cache keyed by `param_1` and compared with `memcmp`: **type 1** is a **0x108-byte** descriptor (the main colour target, `memcmp` against `DAT_1405b07e0`) and **type 2** is a **0x38-byte** descriptor (compared against `DAT_1404bada8`) — 4.4× smaller. `param_1 == 1` binds via vtable `+0x118` (SetRenderTarget), `== 2` via `+0x120` (SetDepthStencilSurface), `== 3` via `+0x158`. So type 2 is the **depth** surface, not a second colour target — meaning **the game's two colour targets are the per-view backbuffer and the glow mask, and the mask's descriptor is the 0x38 one**. ⚠️
- ❓ **The mask's exact dimensions are runtime-only — closed as unreachable statically (2026-09-28).** Both descriptors are `.bss`, written at runtime, and the width used for the create comes from `descriptor + 0x34` (`FUN_14001ca40`, cached into `DAT_140594a24`). There is no static constant to read: the ratio is decided by code that only runs on a live device. The `FUN_140184c90` writer of that same global turned out to be a **display-mode enumeration** (0xC-byte records up to `0x140594a24`), not the mask's size logic. **Settling it needs one live run**, and it does not block anything — Bevy's bloom is resolution-relative, so whatever the original used, our modern equivalent is tuned to our own output.
- **Every game pixel shader writes two targets — and the two exceptions are legacy.** Disassembling **all 219 pixel shaders** in `game_files`: **217 write `>= 2` render targets.** The only two that do not are **`diffuse_ps` and `solid_ps`** — and `solid_ps` compiles to **`ps_2_0`** (the Xbox-1 shader model; the entire rest of the set is `ps_4_0`). Neither name appears anywhere in the decompiled program. So: **the two-target contract is universal in the PC version, and the only shaders that predate it are dead files.** ✅ That is much stronger than the eight-shader sample this doc originally rested on, and it settles the "is the second target optional" question — it is not; a draw that skipped it would corrupt the glow mask for everything behind it.
- **So the answer is: a per-object tinted glow/bloom mask**, not velocity, not depth. It is the 2004 version of selective bloom — the mask carries an artist-chosen colour per draw, so a Psycho Knife flare glows magenta and a Laser beam glows blue, and the halo takes that colour.

**This is the strongest finding for Part B.** The original already has a coloured-glow pass. We can drive Bevy's bloom threshold from an emissive value and get the same look for a fraction of the cost — and Bevy's own docs say the `prefilter` knob exists precisely to emulate "the 1990s-2000s game look" (`bevy_post_process::bloom::settings.rs`).

## A3. The red glow round the attacker — SOLVED, and it is the school aura ✅

Confidence: ✅ on identity. **Not a lock-on, not a cast-charge, not a light record** — those were my hypotheses and all three are wrong. The answer was already in the project: `VFX-AUDIT.md` §6 records that the glow is the **school aura** (colour = the arsenal's dominant school; Psycho red, Optical yellow, Nature green, Ki blue, Faith purple; shown once Level ≥ 1), sourced from a community guide (GameFAQs 918888 faq 41762) and **built** by the `lane-school` session as `pd_game/src/school_aura.rs` + `school_aura.wgsl`.

I reached the wrong answer three times before checking, so record the correction: a red rim on the *attacker* looked like an attacker-state marker, and I read the frames as ruling the school aura in. The frames are consistent with the school aura, but they **do not** identify it — every Psycho-using clip is red, and the attacker is always the Psycho user in the footage. **The identity comes from the guide, not from these frames.** My "not lock-on vs cast-charge" open question was itself based on the false premise.

**What the frames do add** (independent of identity): it is a **silhouette-hugging edge term**, not a floor wash or a shared pass — it traces the body including the head and shoulders, and the target in the same frame has none. That is why the lane's v2 "shell" approach (extrude a skinned copy along rebuilt normals, show only back faces) is the right shape. Evidence: `engine/shots/ft2_pk_locked_hit.png` cell r0c0 (frame 0084, T−800 ms) and `shots/vfxaudit/redglow/red_rim_attacker_f0084.png` — game-derived, local, gitignored.

**Program side, still unconfirmed.** The lane searched for a rim/fresnel/outline shader and a per-school colour table by name and found neither; the only glow classes are the capsule/effect glow-sprite system and `pdEftAura01` (a boss/PSP transition object, not per-fighter). I now have a concrete lead they don't: the game's **school colour table at `0x1402dca00`** — 8 × ARGB, read as white, red, blue, purple, green, yellow, aqua, zero (7 schools + a pad). That is *not* the guide's Psycho/Optical/Nature/Ki/Faith order, so it is either indexed differently, a different table, or the aura reads its colour from the **skill school field (s9)** elsewhere. ⚠️ worth one cheap check: find who reads `0x1402dca00` and see whether the index is a school id or something else.

**The open question that survives:** whether the aura is keyed to *dominant school in the arsenal* (the guide) or to *whatever the player last cast* — a 4-way mixed-school fight, or one Aura Particle taken, would separate them.

## A4. Why our knife ray burst reads larger and denser than the footage ✅/⚠️

Confidence: ✅ on the numbers, ⚠️ on the weighting.

Our `effects.rs` **already reads FLAIR01 correctly** (the words-24/25 random-size fix, FORMATS-EFFECTS "Effect units"). Psycho Knife's real authored values, straight from `cache/fx/eft0003.fx.json`:

| | value | meaning |
|---|---|---|
| window | frames 13–14 | 2 effect frames |
| rays/frame | **16** (word 30) | per effect frame, on RHAND |
| ray life | **4** drawn frames (word 31) | |
| length | `r × (6 − 4)` = `r × (10 − 50)` → **0–40 u, mean 20** (words 4/5 = 50 are the *unused* fixed pair; 6/7 = 10 are the real span) |
| tip width | `r′ × (11 − 8)` = `r′ × (5 − 20)` → **0–15 u** at birth, 0 at death |
| colour | `0xFFFFFFFF` → `0x00000000` (white → transparent, additive) |
| colour ease | word 28 = 5 |

So the program's burst is **short, thin, and sparse**: mean ray 20 u on an 18–20 u character, i.e. roughly one body height, and 16 rays per effect frame × 4 drawn frames of life with a 2-frame window. The remaining differences are all ours, not the data's:

1. **Density.** Our `STEPS_PER_FRAME = 2.0` ages the burst at 60/s while the window is 30 Hz. With life 4 that is 8 drawn frames of rays instead of 4 — **twice the rays on screen at once**. ⚠️ this constant was inferred from footage frame counting (f101–f102) and is the single most likely cause of "denser than the footage".
2. ~~**Size.** Our wedges draw `width` as the **full** tip width~~ — **ruled out 2026-09-28.** The program's geometry is `tip ± 0.5 × width` (`FUN_1401a1620`, constants `0x1402f82d0` = −0.5 and `0x1402f7d40` = +0.5, both read from the binary), and **our `flare_mesh` already does exactly that**: `let w = (…).abs() * 0.5;` then `tip ± across * w` (`effects.rs:979–982`). ✅ matches. The remaining "our wedges read a little wider than the footage's thin streaks" note is therefore **not** a width bug — see (4).
3. **Camera distance.** The footage's close-up is shot from the opponent's camera, which sits further back and higher than our lock-on framing, so the same 20 u burst occupies fewer pixels. Framing difference, not a bug — do not chase it with a size fudge.
4. **The likeliest remaining cause: the ray *texture*, not the geometry.** Our wedge spans UV 0→1 across the full length, so whatever the effect's texture is, it is stretched over the whole ray. The original's `Flair1P_ps` samples with its own falloff. If our texture's bright core is wide, every ray reads thicker than the original's without any size being wrong. ⚠️ untested — this is the next thing to A/B, and it is cheaper than a geometry change.

**Recommendation (revised 2026-09-28):** the width suspect is dead, so A/B **`STEPS_PER_FRAME` 2.0 vs 1.0** first (the density double-count, (1)) — it is the one item with a real mechanism left. If that closes the gap, stop. If not, look at the texture mapping (4). Do **not** add a scale constant — the units are already right ("Effect units 2026-09-28" ✅) and the width math is confirmed correct.

## A5. MSPARTICLE — the real reading, partially recovered ⚠️

Confidence: ⚠️ structure, ❓ parameters. Reachable but **not** by static text search — see the blocker.

- The classes exist and are named. RTTI carries **five**: `MSParticleFunc`, `MSParticleFunc00`, `01`, `02` and **`03`** (the last was not on the earlier list; found 2026-09-28 at file offset `0x4b2f78`). The shaders are registered as `msPARTICLE00/01/02_ps` alongside `msICE*`, `msRGBLUR*`, `msEFTBB01S` in the shader registry built by `FUN_14018b750` / `FUN_14018bae0`.
- The emitter kind is the packed pair `90e7fd3e 760929ea` / `…e9` (754 + 307 uses) — our "puff emitter" (smoke, dust).
- **The three PARTICLE01 classes are confirmed** in the dispatcher `FUN_1401119c0` (the class the FORMATS-EFFECTS note names as `pdEftParticle1`): it constructs `FUN_1401a2050`, `FUN_1401a2bc0` and `FUN_1401a34a0` — three distinct sub-classes, matching `MSParticleFunc00/01/02`. ✅ consistent with the existing reading.
- ❓ **Blocker, and it is now the part that has changed (2026-09-28).** Earlier this said MSPARTICLE "cannot be found by name search" because the RTTI chain was absent. **Two parts of that were wrong and are corrected here:**
  1. ⚠️ **Corrected again 2026-09-28, and now settled with a negative test.** This section originally claimed "the RTTI `TypeDescriptor` → `CompleteObjectLocator` → `vtable[-1]` chain is not present in the binary". **That was right**, and two attempts to overturn it were wrong. The names are all here (1,235 `AV<Name>@@` strings, including `alfn_FunctionmsPARTICLE00/01/02` and `MSParticleFunc00–03`), but for all 1,060 real descriptors a file-wide search for a pointer to the `TypeDescriptor` returns **zero** hits — *including for `pdvkEleThunder`, whose vtable I found by hand and confirmed by disassembly*. No COL points back at any of them, so the names are an orphaned naming table, not live type information. Full write-up and the lesson in FORMATS-STAGES.md ("The RTTI names are present but not wired up"). **Consequence: name → vtable is genuinely not walkable, and the classes that *are* reachable are the `pdvk*` ones — the ctor stamps `*param_1 = pdvkXxx::vftable` in the decompile, which is how EleThunder, Glow2, GlowExec and the footstep dispatch were all read today.**
  2. ✅ **Still true, and the reason this stayed hard:** emitter kinds dispatch **by vtable, not by name compare** — `FUN_1401119c0` looks the emitter up by index and calls through the class vtable, so there is no `if (name == MSPARTICLE)` to grep.
- **What it needs instead:** one of (a) `DecompAt.java` on the vtable pointer that the effect load path writes for the `90e7fd3e` kind, (b) a Ghidra xref from the `0x14` chunk handler table (`0x140410608` / `0x140410778`, the tables FORMATS-ALR already documents) resolved at *runtime* in a debugger, or (c) set a breakpoint on the packed-name compare during a live game and read the constructed object's vtable. **(c) is the cheapest and is the recommendation** — one breakpoint answers it.
- **Meanwhile, the symptom is real and documented**: "Psycho Knife's hit puffs read as a big white cloud". Our reading treats words 30/31 as size and word 8/9 as life for these, and the PARTICLE01 reading proved *that* pattern wrong for the sibling class. **Treat our MSPARTICLE numbers as wrong until (c) says otherwise** — and note FORMATS-EFFECTS already keeps the puffs on the old blending for exactly this reason, which is the right instinct.

---

# Part B — modern lighting we could adopt

All of it **render-side only**. None of it may be read by `sim.rs` / `combat.rs`.

Bevy 0.19.1 has far more of this built in than the "render-only effects are hard" assumption suggests. Verified in the source on disk (`~/.cargo/registry/src/*/bevy_*‑0.19.1/`), not from memory:

| Technique | Bevy 0.19.1 status | Type / module | Effort |
|---|---|---|---|
| HDR + Bloom | ✅ first-party, needs `Hdr` on the camera | `bevy_post_process::bloom::Bloom` (`intensity`, `high_pass_frequency`, `scale`, `prefilter: BloomPrefilter{threshold, threshold_softness}`, `composite_mode`) | S |
| Colour grading / LUT | ⚠️ no built-in 3D LUT pass | `Camera3d::tonemapping` + `ColorGrading` for tone/contrast/gain; a real per-stage LUT needs a custom node | S (grading) / M (LUT) |
| SSAO | ✅ first-party | `bevy_pbr::ssao::ScreenSpaceAmbientOcclusion` + `…QualityLevel{Low,Medium,High,Ultra,Custom}` | S |
| SSR | ✅ first-party, **requires deferred + prepasses** | `bevy_pbr::ssr::ScreenSpaceReflections` | M (see caveat) |
| Volumetric fog / god rays | ✅ first-party | `bevy_light::{VolumetricFog, VolumetricLight, FogVolume}`; `VolumetricFog{step_count, jitter, ambient_color, ambient_intensity}` | S |
| Clustered lighting | ✅ first-party | `bevy_light::cluster::ClusterConfig::{None, Single, XYZ{dimensions,…}, FixedZ{total, z_slices,…}}` | S |
| Soft shadows | ✅ first-party | `ShadowFilteringMethod::{Hardware2x2, Gaussian (default), …TAA- randomised}` on the light | S |
| Cascaded shadows | ✅ first-party | `bevy_light::{CascadeShadowConfig, Cascades}` | S |
| Contact shadows | ✅ first-party | `bevy_pbr::contact_shadows::ContactShadows{linear_steps, thickness, length}` | S |
| Irradiance volumes | ✅ first-party | `bevy_light::{IrradianceVolume, LightProbe, EnvironmentMapLight}` | M (needs baked data) |
| Emissive → bloom | ✅ automatic | `StandardMaterial::emissive` feeds bloom when the camera is `Hdr` | S |

Three things to flag before the table below:

- **SSR is the wrong tool here — but the original did it anyway, cheaply, and that is the useful part (2026-09-28).** It needs `DeferredPrepass`, which is a large change to how we currently render (we're forward, and our stages are baked-vertex-colour), and it only reflects what is on screen — a metal floor in a split-screen view will flicker as other fighters walk across it. **But disassembling `SCREFC_ps` shows the 2004 game shipped per-material Fresnel-weighted screen-space reflections** (Fresnel = 1 − N·V modulating a scene-texture fetch, ×4 gain, specular-power threshold, 29 instructions) **and gave characters the same via `SBOSREFS_ps`** (3 textures, 6 varyings). ⚠️ So the faithful-and-cheap option is **not** Bevy's SSR pass but a *reproduction of theirs*: one extra scene fetch in the material, Fresnel-weighted, no roughness cones, no temporal accumulation, no depth pyramid. **Recommendation: ❌ no to `bevy_pbr::ssr`; ✅ yes to the original's cheap per-material version, in the enhanced tier only, if footage ever shows a stage that needs it.** Same shape as the bloom conclusion: copy the result, not the modern mechanism. Write-up in FORMATS-STAGES.md.
- **Volumetric light shafts: the original's are fake, and so should ours be.** `vkRAYLIGHT` is **14 instructions and has no ray loop** — it is a stretched additive quad (`o1 = v2 * cb0[12].z`) with a `cb0[16].x` threshold, i.e. beam geometry, the same trick as the lightning bolt. ❓ Which lights get shafts is unread, but the mechanism is settled. ⚠️ **This makes god-rays a *very* cheap item** — one billboard per light — rather than a real volumetric fog march at 0.8–2.5 ms. It should move **up** the ranking as a "modern look" item on faithfulness grounds, not be bundled with true volumetrics.
- **Bloom is per-camera, not shared.** Each `Camera3d` with `Hdr` gets its own mip chain, so in 4-view split-screen the bloom cost multiplies by the view count. That is the central budget fact, and it is why the rank below puts bloom first but caps it hard. Confirmed in source: `Bloom` is a per-camera component carrying `require(Hdr)`.
- **Bevy 0.19 has no MRT — so we cannot copy the original's glow mask literally.** `bevy_render`'s `ColorAttachment` is a single colour target (`texture` + optional `resolve_target`); `ViewTarget`'s `main_textures.a` / `.b` are a **ping-pong pair for post-processing**, not two render targets written in one pass. Reproducing the original's "every shader writes a second target" would mean hand-building a custom render graph with a second attachment on every pass. **Do not do that.** The modern route gets the same look far cheaper: put the glow value in `StandardMaterial::emissive` and let Bevy's bloom threshold pick it up. The original's *result* is the goal, not its mechanism.

## The costs, and the budget

Published frame-time numbers for these exact effects on this class of hardware are thin. What follows is **estimates labelled as such**, anchored to the two things that *are* solid: the passes are fullscreen-bandwidth-bound (so cost scales with **pixels, not entities**), and a split-screen view costs roughly its own pixel count again in every fullscreen pass. For an 8.16 Mpixel frame (4 × 1439×772, the footage's own split resolution) at 60 fps on an RTX 5060 laptop GPU, the practical budget is **~2 ms per view for all post combined**, not per effect. On a Radeon 890M at the same resolution, that is roughly **5–6 ms per view**, which is already most of a 16.6 ms frame — so the low tier must drop resolution, not just effects.

| Technique | Look gain | Faithfulness risk | Cost / view @1080p (est.) | Verdict |
|---|---|---|---|---|
| **Bloom off the emissive/glow value** | **High** — the whole point of A2 | **Low** — the original has a tinted glow pass; we match its intent | **0.6–1.2 ms** (dual-filter mip chain ≈ 0.55× base res in total samples) | ✅ **Do it first** |
| ~~**Lightmap the stage + real effect point lights**~~ | — | — | — | ❌ **Dropped 2026-09-28.** Sized on my wrong A1 claim. A1 corrected: the original lights **characters only**, so baked vertex colours + a floor decal is already exact (what we have). Kept below as an *optional* mood/bounce extra for the enhanced tier, not a fidelity item. |
| **Clustered lighting** | enabler, not visible | None | ~0.2–0.5 ms/frame | ⚠️ only if effect lights on characters are ever raised above ~4; **not needed today** |
| **Per-stage colour grading (no 3D LUT)** | **Medium-high** — biggest bang for the buck on "mood" | **Low** — additive grade, the original's own look is flat | **~0.1 ms** | ✅ cheap, do it |
| **Contact shadows / blob shadows** | **Medium** — grounds characters, we have none | **Medium** — the original's characters cast real stencil shadows (FORMATS-STAGES) | 0.2–0.6 ms (contact) / **~0** (blob) | ✅ blob at low tier, contact at high |
| **Soft shadows (Gaussian, 2048)** | Medium | Low–medium | 0.3–0.9 ms | ✅ high tier only |
| **SSAO** | Medium | **Medium-high** — will darken corners the original leaves flat | 0.5–1.5 ms (High preset) | ⚠️ opt-in, off by default |
| **Cascaded shadows** | Medium for outdoor stages | Medium | 0.5–1.5 ms | ⚠️ only if stages get big enough to need it |
| **Volumetric fog / god rays** | **High** where the stages are dark (Lost Palace, Strange City) | **Medium** — the original's fog is a flat D3D fog the shaders never even read (FORMATS-STAGES) | 0.8–2.5 ms at `step_count` 32–64 | ⚠️ high tier, gate on dark stages |
| **Irradiance volumes** | Medium | Low | runtime ~0.2 ms; **bake cost is the real price** | ❌ defer — needs an offline bake |
| **SSR** | Low here | **High** — wrong renderer, on-screen-only artefacts | 1–3 ms | ❌ **no** |
| **Meshlet / 3D LUT** | — | — | — | ❌ out of scope |

## The recommended "modern look" package

### Direction (Laurence, 2026-09-28)

**Faithful as the default, enhanced as a settings toggle.** The original look ships on; "enhanced" is a separate switch that adds shadows, ambient occlusion and particle work on top. The enhanced tier's target is **a 55-inch 4K TV**, not a laptop screen.

### The 4K target changes the budget completely

A 55-inch 4K panel is 3840×2160 ≈ **8.3 Mpixel per view**. At 2 views that is ~16.6 Mpixel, at 4 views ~33 Mpixel — **four times the 8.16 Mpixel the original's 4-way split ran at**, and we would be the only thing in the room that looks soft. This is the opposite of the "mimic 2004" instinct: on a big TV the correct goal is *sharper and cleaner than the original*, not softer.

Three consequences, all cheap:

1. **The original's look is the toggle-OFF state.** At 4K the flat, low-contrast picture will read as "unfinished" rather than "period". Faithful mode still has a job — it's the A/B baseline and the art direction — but it is not the showcase.
2. **Resolution is now the priority, before any effect.** At 4K the single biggest quality win is *not* rendering at reduced scale. The 0.6–0.75× render-scale idea from the low tier belongs to the laptop path only, and should be tied to a settings toggle rather than assumed.
3. **Anti-aliasing has to be answered.** The game currently has **no MSAA** and no `Hdr`; with `PD_TONEMAP=none` it matches the original's flat picture. Switching to the enhanced tier means choosing MSAA vs FXAA/TAA and accepting a real-hardware MSAA cost at 33 Mpixel.

**This is a genuine tension worth stating plainly: faithful-by-default and 4K-showcase are in mild conflict.** The recommended resolution is to keep both, and let the settings toggle carry the difference — but the enhanced tier should be the one that gets measured against a 4K display, and it should be the one that gets the budget.

### Tiers

All render-only, all switchable at runtime (which is also the A/B switch against the original).

| | **Low** (890M / Iris Xe floor) | **Medium** (mid laptop) | **Enhanced / 4K** (RTX 5060) |
|---|---|---|---|
| Bloom | on, `max_mip_dimension` 256, render at 0.6× view scale | on, full | on, full + `scale` for a wider, softer halo |
| Effect lights on characters | ✅ as today (2-4 on screen) | ✅ 8 lights/view | ✅ 16 lights/view — the original's own cap, reached exactly |
| Colour grading | ✅ per stage | ✅ per stage | ✅ per stage + optional 3D LUT (custom node) |
| Shadows | blob (cheap decal) | contact shadows | **soft directional + cascaded, the headline 4K win** |
| SSAO | off | off (opt-in) | **on, `Medium` preset** |
| Volumetric fog | off | off (opt-in on dark stages) | **on, `step_count` 48, gated per stage** |
| MSAA | off | 2× | 4× (the 4K anti-aliasing answer) |
| Budget | ~4 ms/view | ~7 ms/view | ~12 ms/view |

**Ranking, by gain per unit of risk** (revised 2026-09-28 after the A1 correction — the lightmap dropped out of second place because fidelity never needed it): 1) bloom off emissive, 2) per-stage colour grade, 3) blob/contact shadows, 4) soft directional shadows, 5) volumetric fog on dark stages, 6) SSAO as an opt-in, 7) optional stage lightmap for the enhanced tier's mood, 8) everything else deferred. For the 4K-enhanced goal specifically, **the 4K anti-aliasing decision (MSAA vs TAA) comes before effects** — it is the difference between "clean" and "jaggy".

**Natural pairings with Part A.** Bloom is the modern restatement of A2's tinted glow mask (drive `BloomPrefilter.threshold` from the same value our `FxMaterial` already writes for glow). Colour grading is the only way the original's flat look gets mood without touching geometry. SSAO and volumetrics are the two that most risk fighting the original, so both are off in faithful mode. **Effect lights need no work at all** — A1 corrected shows the original feeds them to characters only, which is what we already do.

## Suggested implementation lanes

All render-side. None of these touch `sim.rs` / `combat.rs`. Sizes: S ≈ ½ day, M ≈ 1–2 days.

| # | Lane | Size | Owned files | Notes |
|---|---|---|---|---|
| 1 | **HDR + bloom** | S | `pd_game/src/main.rs` (plugin), new `pd_game/src/fx_post.rs` | `Hdr` on each player camera + `Bloom`. Gate `PD_BLOOM=0/1`. Set `composite_mode: Additive` when `prefilter` is non-energy-conserving, as Bevy's own doc recommends. A/B against `PD_BLOOM=0` with a tick-matched sheet. |
| 2 | ~~**Stage lightmap + real effect lights**~~ → **per-stage colour grade** | S | `pd_game/src/stage_look.rs`, `pd_game/src/main.rs` | **Re-scoped 2026-09-28.** The lightmap/effect-light half is unnecessary: A1 corrected shows the original lights characters only, and we already do that. What remains is the cheap, high-value item — `ColorGrading` per camera driven from `cache/stNN.light.json` (which already has per-stage sun/sky/ground). No new assets, no new formats. An optional stage lightmap can follow later for the enhanced tier's mood/bounce if the grade proves too weak. |
| 3 | *(merged into lane 2)* | — | — | Per-stage colour grade is now lane 2, re-scoped 2026-09-28. |
| 4 | **Blob → contact shadows** | S–M | `pd_game/src/main.rs`, `pd_game/src/fx.rs` | Blob first (a cheap decal under each fighter); contact shadows at high tier. |
| 5 | **Volumetric fog, dark stages only** | S | `pd_game/src/stage_look.rs`, `main.rs` | `VolumetricFog` on the camera, enabled per stage from the light script's fog set. Gate on the dark arenas. |
| 6 | **The remaining open Part A items** | S | none (research) | **Shrunk 2026-09-28.** (a) ~~the red glow~~ → **closed**: it is the school aura (A3). (b) `MSParticleFunc`'s words — **not statically reachable**, needs one live breakpoint (A5). (c) ~~the FLAIR01 `±0.5` width check~~ → **closed, our width math is correct**; what remains is A/B `STEPS_PER_FRAME` 2.0 vs 1.0, then the ray texture's falloff (A4). (d) new: if the floor wash is a decal rather than stage shading, find the program's floor-decal draw (A1). |

### Lane 0 — before anything else, on the 4K path

**Anti-aliasing, decided and measured (S).** The enhanced tier's target is a 55-inch 4K TV, where the difference between "clean" and "jaggy" is the whole perception. The game has no MSAA today. This needs a decision (MSAA 4× vs FXAA vs TAA — TAA needs temporal data Bevy 0.19 exposes via `TemporalAntiAliasing`, and interacts with the `ShadowFilteringMethod::TAA`-style randomised shadow filter), a measurement at 2 and 4 views on the 5060, and a look at the result on an actual 4K display. **Do this before lane 1's bloom work**, because at 33 Mpixel it changes the frame budget that every later lane is sized against. Files: `pd_game/src/main.rs` (camera config), a settings toggle.

**Unblockers to ask Laurence about first** (the click-panel candidates): whether bloom should be on by default on Low, whether SSAO is a look he wants at all given it fights the original, and — for the red glow — one clip with a lock-on but no incoming attack, to separate lock-on from cast-charge.

## Open / not established

- ✅ **The glow mask's size is runtime-only** (A2): the surface descriptors are `.bss` and the create width comes from a runtime field, so the ratio is unreachable statically and needs one live run. Non-blocking — Bevy's bloom is resolution-relative. Also corrected: the "second target" is the **0x38-byte descriptor** (a much smaller surface than the 0x108-byte main colour target), and type 2 in that cache is the **depth** surface, not a second colour target.
- ✅ **The stage light-record chunk exists and is understood, but no stage ships one** (2026-09-28): ALR chunk `0x09`, handler `FUN_1402946f0`, writes the *same* light table as the effect lights straight from the file (0x28-byte records: colour, type, slot, flags, position, range). Walking all 30 `Map/stNN.alr` files, **id 0x09 never appears** — the feature is in the engine, not in the data. This independently confirms the A1 correction: stages are lit by the light script and the per-vertex shader, not by per-mesh lights. Layout and evidence in FORMATS-ALR.md "Other chunk ids".
- ✅ **An unconverted asset family turned up while looking**: `st01cam.alr`…`st08cam.alr` (228 KB, 8 files, ALR chunk `0x07` × 65) are **keyframed camera tracks** — the original's hand-directed stage fly-throughs. `pd_convert` explicitly skips them. Half-read layout in FORMATS-ALR.md. Out of scope today (no cutscenes per AGENTS.md), but it's a free win if a round intro is ever wanted.
- ✅ **Capsule audio is decoded but never triggered** (2026-09-28, from Laurence noticing the missing pickup sound): `sfx.rs` only plays each skill's cast/travel/impact plus stage objects, so every `common` id the original fires outside a skill cast is silent. The program reads four: **pickup `common/34`**, appear `common/85`, erase `common/92`, jump-zone hum `common/79` (2 loop points). All four are present and decoded in `cache/sfx` — this is a wiring gap, not a format gap, and it's one small lane. Table in FORMATS-AUDIO.md "What is not wired up".
- ✅ **A2 is now essentially closed, including the composite.** The pass is a **named subsystem** (`pdvkGlow2` + `pdvkGlowExec`), it is a **fullscreen quad** (not a post-process hook), it runs **per view**, it is **runtime-switchable** (`DAT_140530034 & 8`), and it is **threshold → 3/4-tap weighted box blur → composite**, with the tap offsets baked per-vertex and the weights in `cb0[0..2]`. Both `msRGBLUR00_ps` and `msRGBLUR01_ps` are now disassembled. Full write-up in FORMATS-STAGES.md ("The glow pass is a named subsystem" + "The blur kernel").
- ❓ Whether the original's glow-mask pass would be worth reproducing literally in Bevy. **Recommendation: no — now settled on evidence, not analogy** (no MRT in Bevy 0.19, so a literal port means a custom render graph with a second attachment on every pass; and having read the kernel, it is a 3–4 tap box, which `Bloom`'s mip chain reproduces *better*). The cheaper emissive-threshold bloom gives the same result and keeps the per-object colour via `cb0[17]`. Only revisit if the emissive route visibly misses the footage's per-object colour.
- ✅ **The red glow is solved** (A3): it is the **school aura** — dominant-school colour, shown at Level ≥ 1 — already answered and built by `lane-school` (`school_aura.rs` / `school_aura.wgsl`; VFX-AUDIT.md §6). Not lock-on, not cast-charge, not a light record. The programme-side mechanism and the exact colour source are still unconfirmed.
- ❓ `MSParticleFunc`'s word meanings (A5). **A5 stands, after a third attempt to overturn it (2026-09-28) — and the way to settle it is now known.** The binary has all the names (`MSParticleFunc`, `…00/01/02/03`; also `MSEffectEI`, which is a new one), but the decisive test says they are **orphaned name strings, not live RTTI**: for all **1,060** `AV<Name>@@` descriptors, a file-wide search for a pointer to the `TypeDescriptor` returns **zero** hits — including for `pdvkEleThunder`, whose vtable I located by hand and confirmed. No `CompleteObjectLocator` points back. So the route "name → COL → vtable" genuinely does not exist, and my two earlier attempts to say otherwise were both wrong; the lesson is in FORMATS-STAGES.md ("The RTTI names are present but not wired up"). **The method that does work:** grep the *decompile* for `ClassName::vftable` (which is how `EleThunder`, `Glow2`, `GlowExec` and the footstep dispatch were all read today), or accept the one-breakpoint route: set a breakpoint on the packed-name compare during a live game and read the constructed object's vtable. **MSPARTICLE is the one item where that breakpoint is still the cheapest answer, because unlike the `pdvk*` systems nothing stamps its vtable by name.**
- ✅ **FLAIR01 ray width is correct** (A4): the program draws `tip ± 0.5 × width` and so do we (`effects.rs` `flare_mesh`). The "our wedges read wider" note is not a width bug — the remaining suspects are the density double-count (`STEPS_PER_FRAME`) and the ray texture's falloff.
- ⚠️ Every cost number in the table above is an **estimate**, not a measurement. The first thing lane 1 should do is measure, on both the RTX 5060 and the 890M, with and without, at 2 and 4 views.
- ❓ Whether Bevy 0.19's bloom mip chain is rebuilt per camera or shared across cameras sharing a target. Its `Bloom` is a per-camera component with `require(Hdr)`, so **assume per-camera** and measure.


## Built as options (2026-10-03, lane look) -- what became of the "modern look" package

The package above is now an **optional, default-OFF Options page** ("Lighting and look", docs/FRONTEND.md "Look options"; `pd_game/src/look.rs`). What was built and what was not:

| Item from this document | Status |
|---|---|
| Bloom off the glow value | **Built** as BLOOM OFF / LOW / HIGH on the existing per-view glow (`glow.rs`), not Bevy's `Bloom` (that lane's own custom chain, which already handles viewports) |
| Soft shadows / shadow quality | **Built** as SHADOW QUALITY ORIGINAL / HIGH (Gaussian filter, 4096 px maps). The original's darkening stays on and stays the default |
| Per-stage colour grade | **Not built**; a global TONE CURVE ORIGINAL / FILMIC (AgX) is offered instead (see FRONTEND.md for what it does to dark stages) |
| Effect lights "16 per view" | **Corrected**: 16 is the effect-light table's size, but the character shader (SWBOS) has one point-light slot. EFFECT LIGHTS offers the original's 1 or the 4 strongest summed |
| MSAA / FXAA / TAA (Lane 0) | **Dropped by Laurence, 2026-10-03**: no anti-aliasing row; the game keeps its 4x MSAA. (Also: FXAA/SMAA run a full-window pass per view on the shared window target; TAA needs motion vectors from every custom material) |
| SSAO | **Not offered**: Bevy's `ScreenSpaceAmbientOcclusion` only attenuates the ambient term of Bevy PBR lighting (`apply_pbr_lighting`), which the unlit stage and the SWBOS fighters never use; it also needs MSAA off and prepasses. It would cost frame time and change nothing visible |
| Contact shadows, volumetric fog, clustered lights | **Not offered**, same reason (PBR-light features our own shaders do not read) |
| Volumetric / billboard light shafts | Not built (placement unread, FRONTIER F2) |

Cost estimates in the table further up remain estimates; the measured numbers for the built rows are in FRONTEND.md "Look options".
