# P0 audit: LIGHTING (deep pass P4) — 2026-09-30

Read-only audit. Laurence's complaint ("directional lighting / depth / atmosphere
is missing") is **confirmed, and the reasons are specific**. The earlier
RESEARCH-LIGHTING.md claims "stage is lit per-vertex only", "effect lights feed
characters only", "our current build is already the faithful one" are each
**wrong or only half true** (details below). Evidence: the original's compiled
shaders (`tools/disasm/disasm.exe game_files/*.cso`, all 219 pixel shaders
re-disassembled for this audit), the decompile (`decomp_all.c`), our code, the
stage data (`pd_convert walkprobe ... --draws` over all 30 stages, read-only),
the reference screenshots (`Reference Images/Lost Palace 1/3.png`,
`engine/shots/ft2_pk_locked_hit.png`) and our own shots
(`cachecheck_st03_noaura_t0300.png`, `orch_tonemap_ab.png`).

What Laurence sees, in one line each (from the screenshots): the original has
**a dark cast shadow under the fighter, dappled light pools / shadowed patches
on the floors, a coloured wash on floor and fighters during a skill**; ours is a
uniformly lit floor, a fighter with no ground contact, no wash on the surroundings.

## 1. Ledger

Legend: ✅ yes, ❌ no, ~ partial. "Approved" = Laurence said so in DECISIONS/HANDOFF.

| Feature | Mapped | Implemented | Reproduced | Verified | Approved | Evidence |
|---|---|---|---|---|---|---|
| Stage directional + hemisphere light, vertex shader (`SD_vs`) | ✅ | ~ baked to `COLOR_0` (`gltf.rs:light_vertices`, `:107`) | ✅ (flat stage look in shots) | ❌ the hemisphere term `t = 0.5+0.5·N·axis` is an assumption (`gltf.rs:105`); no pixel/constant check | ❌ | `SD_vs` disasm; FORMATS-ALR "Stage lighting" |
| Stage **per-pixel** lighting, bump/normal-mapped (`SBTS_vs/ps`, vertex format 0x03, flag 0x20) | ✅ (this audit) | ❌ stage materials are `KHR_materials_unlit`, no normal map (`gltf.rs:715,756`) | n/a | n/a | ❌ | walkprobe: 31–98 % of visible stage triangles are format 0x03 on 27 of 30 stages (table §1a) |
| Stage specular (flag 0x04 + vertex alpha + `cEYEVEC`) | ✅ (`SD_vs` o3, `SBTS_ps`) | ❌ | n/a | n/a | ❌ | 28–100 % of triangles carry flag 0x04 |
| Stage point light `cP0` (inverse-square, per vertex / per pixel) | ✅ shader side | ❌ (script call 07 not parsed, `stagedat.rs:read_lights`) | n/a | n/a | ❌ | `SD_vs` lines 34-40, `SBTS_vs` |
| Stage **shadow volumes** (`SSHADOW/SSHADOWC`, flag 0x2000) cast onto floors and fighters | ✅ data + shader (`SSHADOWCH_vs`: extrude along `cDL_VEC`, PS = black) ~ compositing pass not read | ❌ dropped at export (`gltf.rs:is_shadow_volume`, `:278`) and from collision | ❌ | ❌ | ❌ | FORMATS-STAGES "Stage shadow volumes"; reference shots show the pools |
| Character **shadow volumes** (`SSHADOWCH`, pos+normal = fmt 0x07, helper joints) | ~ shader signature match; meshes per character not counted | ❌ skipped ("probably shadow volumes", FORMATS-ANIMATION:74) | ❌ | ❌ | ❌ | `Lost Palace 1.png` (clear hard shadow from the feet) |
| Character lighting model (`SWBOS_vs/ps`) | ✅ (this audit, §2 D1) | ~ Bevy PBR: 1 directional + flat ambient + tangent-space-faked object normal map (`main.rs:912-925`, `gltf.rs:get_lit`) | ✅ (dark, flat fighters in shots) | ❌ light values "tuned by eye" (`main.rs:42`, `stage_look.rs:16`) | ❌ | SWBOS disasm §2 |
| Character specular (normal-map alpha × LUT alpha × vertex weight.w × `cSPECULAR`) | ✅ | ❌ `matte_characters` forces reflectance 0 (`main.rs:2018`) | n/a | n/a | ❌ | `SWBOS_ps` last 6 instructions |
| Which light set feeds characters | ~ partly (§2 D2) | set 1 sky/ground averaged to one flat ambient (`stage_look.rs:84-87`) | n/a | ❌ | ❌ | `FUN_140199e70` |
| Effect lights (LIGHT02) on characters and stage | ~ (RESEARCH-LIGHTING A1 corrected below) | ❌ only an additive floor decal (`effects.rs:264 FxLight`) | ✅ (no wash in our shots) | ❌ | ❌ | `FUN_14010c990`, `FUN_140199970/140199e70` |
| Scene-wide light flashes/dims on game events | ✅ numbers (§2 D6) | ❌ | n/a | n/a | ❌ | `FUN_14022e1d0`, `FUN_14017ba00`, `FUN_14021bc20`, storm `FUN_1402269b0` |
| Fog | ✅ none on PC (69 pixel shaders declare FOG, all 69 leave it unused) | ✅ off by default (`stage_look.rs:74`) | ✅ | ✅ (re-checked here) | ❌ not asked | per-shader "Used" column blank |
| Tone mapping | ✅ original = straight gamma-space output | ✅ Bevy default TonyMcMapface kept | ✅ | n/a | ✅ Laurence chose default (DECISIONS 2026-09-2x, `orch_tonemap_ab.png`) | `main.rs:966` |
| Glow / bloom (`GFilter`, `msRGBLUR`, mask colour `cb0[17]` written by every pixel shader) | ✅ (RESEARCH A2) | ❌ **no `Bloom` or `Hdr` anywhere in `pd_game`** (grep) | ✅ | n/a | ❌ | RESEARCH-LIGHTING "do it first" was never built |
| Sky (two shells, gradient + tiled cloud) | ✅ | ✅ shells drawn, cloud UV drift (`sky_scroll.rs`) | ✅ | ~ drift rate eyeballed (0.004/s) | ❌ | FORMATS-STAGES "Sky scroll" |
| Sky/default light sets 2 and 3 (programs binds set 2/3 around sky draws, `FUN_1401c87f0`) | ~ | ❌ "not applied" (`gltf.rs:594`) | n/a | n/a | ❌ | `decomp_all.c:379733,379793` |
| LBTS lightmap (format 0x1F) | ✅ shader (`LBTS_ps`, lightmap on t2) | ❌ ignored | n/a | n/a | ❌ | 1–11 % of triangles, st09_xx hub stages only |
| Backdrop / clear colour from script call 02 | ~ ❓ | ❌ fixed `ClearColor(0.05,0.05,0.08)` (`main.rs:788`) | n/a | n/a | ❌ | `StageLights::backdrop` parsed, unused |
| Script call 02 "factor" (0.7 st03, 0.9 st05, 1.0 st04/06/08, 1.1 st07/st12) | ❌ meaning unknown | ❌ | n/a | n/a | ❌ | `02 sub 3` |

### 1a. Stage material mix (measured, `walkprobe --draws`, shadow volumes excluded)

Share of visible triangles per vertex format (0x01 = `SD` per-vertex light, 0x03 =
`SBTS` per-pixel bump, 0x25 / 0x1F = other shaders) and with flags 0x04 (specular),
0x20 (bump), 0x4000 (unknown, ❓):

| Stage | fmt 0x01 | fmt 0x03 | other | flag 0x04 | flag 0x4000 |
|---|---|---|---|---|---|
| st01 | 52 % | 46 % | 2 % | 98 % | 47 % |
| st02 | 11 | 82 | 7 | 28 | 2 |
| st03 / 13 / 23 (Palace) | 47 | 51 | 2 | 73–77 | 56 |
| st04 / 14 | 26–28 | 69–71 | 3 | 71–75 | 3 |
| st05 / 15 | 43 | 54 | 3 | 92 | 78 |
| st06 / 16 / 26 | 33–36 | 56–58 | 6–9 | 84–86 | 75–78 |
| st07 / 17 | 45 | 44–45 | 9 | 90 | 10 |
| st08 | 15 | 75 | 10 | 87 | 10 |
| st12 / 22 | 12 | 86 | 2 | 30 | 2 |
| st09_xx hubs | ~0–8 | 0–42 | fmt 0x25 = 52–94 (shader unread ❓) | 88–100 | mixed |

So on the versus arenas the majority of pixels are lit by a shader our export
throws away (bump lighting + specular + point light). The baked vertex colour is
only the per-vertex half of the original's picture.

## 2. Discrepancies (most important first)

**D1 — CONFIRMED. Characters use a different light model.**
Original `SWBOS_ps` (from `game_files/SWBOS_ps.cso`, pixel stage of the skinned
character): `rgb = base × ( cS0_COLp  +  cDL_COL × LUT.rgb(N·L, N·S)  +  cP0_COLp × sat(N·Lp)  +  cS1_COLp × (2w·N·A)² ) + spec`,
with `spec = sat( COLOR0.w(vertex weight) × normalmap.a × LUT.a × cSPECULAR )`.
`N` = object-space normal map (t1). `Lp` = the point-light direction × 1/d²
(vertex shader, `cP0_POS`, from `SWBOS_vs` `o2`), `A` = hemisphere axis in object
space (`o6`, from `cS0_VEC`). The 2-D LUT t3 is **`Assets/Data/spec77.dds`**
(256×256 BGRA8, only referenced in the exe strings at file offset 0x2f62c8; never
mentioned in any of our docs — FORMATS-ANIMATION "t3 not in the character file
❓" is now answered): RGB = a pure linear ramp in u (= N·L itself, so diffuse is
plain Lambert) and **alpha = a specular lobe in v** (non-zero only in the last
rows, i.e. N·H ≈ 1). Everything is in **gamma space, no tone map**.
Ours: Bevy `StandardMaterial` (roughness 1, reflectance 0), one `DirectionalLight`
(illuminance `3000 × luma/0.5`, no shadows) + a flat `GlobalAmbientLight`
(`main.rs:917-924`), tonemapped, linear space. Missing: specular sheen, the hemisphere
term, the point-light term, the per-vertex specular mask, the gamma-space product.
Visible effect: fighters read flat and dark; no highlight on the jacket/hair.

**D2 — CONFIRMED read, SUSPECTED meaning. The ambient of a character is not set 1.**
`FUN_140199e70` (called right after `FUN_140199db0` in every character draw:
`decomp_all.c:278008, 283310, 285675, 287718, 293088, 307548`) binds for a character:
DL and the type-1 light from **records 0x48/0x49 = light group 1**, but when the
battle-light flag `DAT_140530034 & 0x10` is set it binds S0/S1 (the hemisphere
slots the PS reads as `cS0_COLp`/`cS1_COLp`) from **records 0x42/0x43 = group 0
(the stage set)**, and then **replaces them with the (up to) two nearest effect-light
records** (`FUN_140199db0` → `FUN_140299be0`, slots `0x60+2·view`).
`stage_look.rs:84-87` instead averages set-1 sky+ground into a single flat
ambient. For st03 set 0 sky/ground is (105,120,130)/(20,27,30) vs set 1
(210,225,255)/(20,27,30): if the read holds, our characters' ambient is about 1.8×
too bright and has no up/down gradient. What is not settled: which script colour
lands in which record field (script call `0A` handler not located, §3 R1).

**D3 — CONFIRMED. No cast shadows of any kind.** Original: real stencil shadow
volumes from both the stage (`SSHADOW`/`SSHADOWC` meshes, flag 0x2000, present on
st02–05/07/08/12–15/17/22–24 + st09_02/08, e.g. st03 draw 1 = 2,282 triangles
spanning y −58…317) and the characters (`SSHADOWCH_vs`: position+normal input,
extrudes vertices facing away from `cDL_VEC` by `cb0[4].w`; PS writes black).
The program swaps these three shaders' variant tables whenever a battle light
exists (`FUN_140199ce0/140199b00 callers`, `DAT_140530034 |= 0x10`,
`DAT_14057f380 |= 0x500`). The pools of light/shadow on the Palace floor in
`Lost Palace 1/3.png` are this. Ours: `shadow_maps_enabled: false`
(`main.rs:919`), volumes dropped (`gltf.rs:278`). Also: our stage is **unlit**, so
even enabling a Bevy shadow map would not darken it — the stage material has no
place to receive a shadow. Effect: no ground contact, no depth cue; the biggest
single reason for "flat".

**D4 — CONFIRMED (corrects RESEARCH-LIGHTING A1). Stage shading is per-pixel for most surfaces.**
`SBTS_ps` (vertex format 0x03) does, per pixel: normal map t1 (tangent space),
`N·L` (directional, `cb0[5]`), hemisphere blend from `cb0[7].w`, point light from
the vertex shader (`o4`), and the same spec77 lookup for specular (`t3`) × `cSPECULAR`.
A1 reasoned from `SD_ps` only. Ours bakes the vertex normal only, no bump, no
specular, no point light (§1a).

**D5 — CONFIRMED read (corrects A1). Effect lights DO enter the stage slot table.**
`FUN_140199970` (the "select lights for this draw group" call, made around every
stage draw; `decomp_all.c:349143`): for group 0 (param 0) and an active LIGHT02
(`DAT_140527cc8`, written by the LIGHT02 update `FUN_14010c990` at
`decomp_all.c:254657`), the **slot-2 light pointer `DAT_14057f3e0` is replaced by
that effect-light record (index +0x80)**. `SD_vs`/`SBTS_vs` compute an inverse-square
point light from `cP0_POS`; `FUN_1402a7a00` just returns the four pointers
(DL, type-1, slot 2, slot 3). SUSPECTED (not read end to end): slot 2 → `cP0_*`.
It matches the footage: in `ft2_pk_locked_hit.png` frames f0108–f0112 the floor
turns white-pink under the hit and both fighters tint magenta. Ours: a screen-plane
additive decal only (`effects.rs:264`), nothing on surfaces or fighters.

**D6 — CONFIRMED read, SUSPECTED trigger. The program animates scene light.**
`FUN_14022e1d0` (per tick, `pdBattleLight`-owning object at `DAT_140533440`)
multiplies the group colours per channel by a fade factor (`1 → target → 1` over
`+0x1dc` frames) and rewrites all four groups via `FUN_14022e450`. Writers of the
target: `FUN_14017ba00` (scale R 1.5 G 1.5 B 2.0, 15 frames, 0.05 in-point; called
on what looks like a capsule capture, `FUN_14017c1d0` follows — identity ❓) and
`FUN_14021bc20` (scale 0.2 / 0.1 / 0.6, 45 frames — a dim; a countdown on
`param_1+0xcc/0xd0`, likely end of round ❓). The **storm** `FUN_1402269b0` blends
the lights toward `0xFFDCDCFF` per view (lightning flash lights the stage and the
fighters). Ours: constant lights; stage baked so it cannot flash at all.

**D7 — CONFIRMED. Glow/bloom not built.** Every original pixel shader writes the
glow mask colour `cb0[17]` and a threshold→blur→add pass runs per view
(RESEARCH-LIGHTING A2, FORMATS-STAGES "Glow pass"). `grep -rn "Bloom\|Hdr"
crates/pd_game` = no hits. Characters, skill effects, capsules lose the soft
halo that contributes to "atmosphere".

**D8 — CONFIRMED. Stage vertex light is baked with an assumed hemisphere ramp and no saturate.**
The shader computes `saturate(tex×diffuse + tex×ambient) + specular` in
gamma space; we store `gamma_to_linear(light)` and let unlit Bevy multiply, then
tone-map (TonyMcMapface, Laurence's choice). Values >1 are extrapolated
(`gltf.rs:95`) instead of clamped, so bright stages (st08 sky 255,180,140) behave
differently from the original; the ramp `t` uses our guess for `cS0_VEC`
(`gltf.rs:105`). Low visible effect, but nothing has verified the stage bake
against a pixel of the original.

**D9 — CONFIRMED (minor/unknown). Script data we parse and ignore or never parsed.**
Call `07` point lights (7 values per light ×3 ×4 sets; non-zero mostly in set 2,
e.g. st06 `colour 0x19c318 at (0,−259,−596)`, st03/st12 set 0 carries an odd first
value `26.76` / `2552.75` with zero colour), call `02` second colour, and the
`02 sub 3` factor (0.7–1.1). Set 2 lights are never bound to the sky draw.

**D10 — SUSPECTED. Stage layer flag 0x4000 (56–78 % of Palace/Refinery/Dawn
triangles) unread** — may be "receives shadow" or "reflective"; `SBTSREF*`/`SCREF*`
shader families (RESEARCH "original had SSR") exist for some materials.

## 3. Reads still needed

- **R1 (gates lane 1).** The handler of script call `0A` (and `05`, `07`, `02`):
  which colour/vector goes into which record field of the `+0x48`/`DAT_1403e8a40`
  per-set block (0x58 bytes per set: colours +0,+4,+8,+0xC,+0x10, vectors
  +0x14/+0x20/+0x2C/+0x38, scalars +0x44/+0x48). Start: `FUN_140199b00` (loads
  `maplight%d.ssb`, `decomp_all.c:349215`), the `pdBattleLight` vtable
  (ctor `decomp_all.c:423704/423827`), and `thunk_FUN_14022e050`'s writers of
  `+0x48..+0x1a0` of the object at `DAT_140533440`. Also the `cS0_VEC` encoding
  (`w`, factor) that `SD_vs` applies as `t = N·xyz + w`.
- **R2.** What the wrapper uploads into PS constants `cb0[2,3,5,6,7]` and VS
  `cb0[7,8,9,10,14,15]` from the four light pointers
  (`FUN_14001e540(0xd/0xe, …)`, `FUN_1402a4e30` sets `cEYEVEC`), i.e. confirm slot 2 → `cP0`.
- **R3.** The shadow composite: the pass that draws `SSHADOW*` into the stencil and
  the darkening quad that uses it (alpha, colour, which draws are masked — floor
  only? fighters too?). Start from the three-shader switch `FUN_140199b00/140201b40`
  and the render-state ids passed to `FUN_140020660` around it.
- **R4.** Character shadow hulls: count and shape of format-0x07 draws on helper
  joints per character (`pd_convert` export skips them); are they rigid per joint?
- **R5.** `spec77` binding for stage/character (`t3`): confirm it is the file at
  `Assets/Data/spec77.dds` for both shaders, and `EnvMap.dds` (exe string 0x2ef4b8)
  for the `*REF*` shaders.
- **R6.** Identify the events behind `FUN_14017ba00` and `FUN_14021bc20` (what causes
  the 1.5/1.5/2.0 flash and the 0.2/0.1/0.6 dim).
- **R7.** Meaning of material flag 0x4000, of vertex format 0x25 (52–94 % of the
  st09_xx hub triangles) and of script `02 sub 3`.
- **R8.** Whether the glow mask colour (`cb0[17]`) is non-zero for ordinary stage
  and character draws (if zero, only effects glow; if not, characters halo).

## 4. Proposed build lanes (ordered)

All render-side; `sim.rs`/`combat.rs` untouched. Each lane closes with an A/B sheet
at four sample stages — Palace st03 (indoor, shadow volumes), Refinery st06
(outdoor night, no volumes), Dawn City st05 (outdoor), Lane st07 (dark) — and
Laurence's pick, per `pd-visual-verify`. No global brightening; compare
mean luma against the reference shots before/after.

**Lane 1 — Character light model (after R1/R2 reads; 1–2 days).**
Goal: port `SWBOS_ps` exactly as a custom material extension: gamma-space
`base×(S0 + DL·N·L + P0 + S1·hemi²) + spec`, spec77 lobe, per-vertex specular
weight, bound light set/slots per D2, nearest-two effect lights as S0/S1 (D5),
scene-light fade state (D6) as a render resource fed by the existing events.
Owns: `pd_game/src/stage_look.rs` (character light parse), new
`pd_game/src/char_light.rs` (material + `matte_characters` replacement),
`pd_formats/src/gltf.rs: Materials::get_lit` + `character.rs` (export vertex
weight.w / normal-map alpha). Tests (cite addresses): LUT x-ramp is identity, alpha
lobe at N·H≈1 (`spec77.dds`); N·L and hemisphere term numerically equal to the PS
instruction sequence at `SWBOS_ps` (golden values hand-computed from the asm);
slot binding follows `FUN_140199e70`. Runtime proof: fixed fighter, fixed yaw
turntable, four stages, original-shader vs ours crop strips; `PD_SHOT_TICKS`
captures of Psycho Knife for the magenta tint.

**Lane 2 — Shadows + receivers (after R3/R4; 2–3 days).**
Goal: characters and (where the original has volumes) stage pieces cast shadows
along the set-0 `dir` onto floors and fighters. Needs a **stage material that can
receive** (custom extension: `baked COLOR_0 × texture × shadow term`, not Bevy
unlit). Faithful target = stencil volumes; modern route = Bevy directional shadow
map using the authored volume meshes as casters, softened, with a cheap blob under
each fighter as the low tier. Owns: new `pd_game/src/shadows.rs`, new
`pd_game/src/stage_material.rs` (receiver), `pd_formats/src/gltf.rs` only the
`is_shadow_volume` export path (write volumes as separate named nodes), `main.rs`
only the light spawn (`shadow_maps_enabled`) and the camera plugin lines.
Tests: extrusion direction/length equals `SSHADOW_vs`; volume present on exactly
the stages listed in D3; shadow term is 1.0 outside volumes (no global dimming).
Proof: Palace floor crops vs `Lost Palace 1/3.png`, same camera; two-view and
four-view frame-time table (measured, not estimated).

**Lane 3 — Stage per-pixel lighting, effect lights on surfaces, glow (2–4 days).**
Goal: bump/specular/point-light per the `SBTS_ps`; effect lights as stage point
lights (D5); bloom from a glow value (D7); ignore-list closed (D9, D10). Owns:
`pd_formats/src/gltf.rs` stage `Materials::get*` (+ tangent/normal-map export for
format 0x03), `pd_formats/src/stagedat.rs` (parse call 07, 02), extends
`stage_material.rs` (from lane 2 — schedule **after** lane 2), new
`pd_game/src/fx_post.rs` (Bevy `Hdr` + `Bloom`; note `Hdr` interacts with the
kept tone mapping and with `fx_gamma.wgsl`; measure 2 vs 4 views). Tests: baked +
per-pixel result equals the asm for a sample normal; effect-light falloff equals
the program's distance rule; bloom off = bit-identical to today. Proof: the same
four-stage sheets; Psycho Knife sequence vs `ft2_pk_locked_hit.png` frames
f0108–f0112 (floor wash).

## 5. Questions only Laurence can answer

1. Shadow look to aim for: **A** faithful (only the original's authored casters, crisp
   and translucent, like `Lost Palace 1.png`); **B** modern soft shadows from all
   geometry; **C** ship both behind the existing faithful/enhanced switch (recommended).
2. Characters during skills: should fighters and the floor take the original's
   coloured wash (magenta for Psycho, etc., as in `ft2_pk_locked_hit.png`)?
   **A** yes, like the original (recommended); **B** only on the floor; **C** no.
3. Glow: add the original-style tinted glow (bloom) — **A** on by default (recommended
   for the 4K target), **B** only in the enhanced tier, **C** not wanted.
4. Stage brightness: which do you want as the target for "how dark is a stage" —
   **A** match the original's measured values exactly (recommended, tone mapping
   kept), **B** match the original with a global lift on dark stages (st07, st09_xx).
