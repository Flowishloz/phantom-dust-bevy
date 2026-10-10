# Stage layout files (`Map/stNN.dat`): objects, start points

Code: `crates/pd_formats/src/stagedat.rs` (parser + `build_stage_props`), export in `gltf.rs` (`ModelInstance`, `skip_models`), CLI `pd_convert stagedat` (probe), `pd_convert propcheck` (collision check), `model` / `collision` / `stages` (use it automatically). Outputs: `cache/stNN.glb` (objects placed), `cache/stNN.pdcl` (object collision added), `cache/stNN.props.json`, `cache/stNN.props.glb`.

Reference read (GPL, not copied): Esper-RE `src/formats/st00.h`, `src/polaris/gui/mapdata_editor.cxx`. Every fact below was re-checked with byte dumps across all 30 files (`pd_convert stagedat <Map dir>`), established 2026-09-26.

Legend: ✅ confirmed by evidence (stated) · ⚠️ probable · ❓ unknown.

**st01 (Edgar's Dream) and st24 (Panorama Boss) are not versus stages** ✅ (2026-09-27, UI polish lane): `setup/stageset/stageset_en.dat`'s per-slot selectable flag (docs/MENUS.md "Stage table fact") is 0 for exactly these two of the 19 named slots; `pd_convert stages` now marks them `"versus": false` (they still convert and play fine for inspection), so `cache/stages.json` lists 17 versus arenas and the setup screen's stage list no longer offers them.

## Which files

| File | Magic | Holds |
|---|---|---|
| `stNN.dat` (arenas) | `ST00`, header 0x140 | everything below ✅ |
| `st00.dat` | `ST00`, older 0x28 header | not decoded; no arena objects (st00 is the common-assets stage) |
| `st09_01..08.dat` | `AR01..AR08` (area files, st09 hub) | 0x40 header, script + light data; no placed objects ❓ not decoded |
| `st32.dat` | `ST00`, all offsets −1 | nothing |

## Header (i32 each; −1 = absent) ✅

Each section starts with a 4-byte tag and the header offset points **just past the tag** (checked for every section in st04/st06: the 4 bytes before each offset are the tag).

| Off | Meaning |
|---|---|
| 0x04 | offset of the `PS0/` list (= 0x140, tag at 0x13C) or −1 (st24) |
| 0x0C / 0x10 | offset of `PS01` / `PS02` list |
| 0x30 / 0x38 / 0x3C | counts of `PS0/` / `PS01` / `PS02` |
| 0x5C / 0x60 | `NM00` type names offset / count |
| 0x64 / 0x68 / 0x6C | `OC00` / `OC01` / `OC02` hulls |
| 0x70 / 0x74 / 0x78 | `OE00` / `OE01` / `OE02` per-type effect blocks (contain "EF00" records) ❓ contents |
| 0x7C / 0x80 / 0x84 | `OA00` / `OA01` / `OA02` object animations (OA01 = collapse keyframes, see below) |
| 0x90 | `OB00` per-type records |
| 0x94 | untagged ~150 KB block of OC-style hull chunks ❓ (probably whole-stage collision; we use the ALR geometry instead) |
| 0x98 | `EF00` stage effects ❓ |
| 0x9C | `SP00` ❓ (floats ±5, 0.125-step grids: sprite/UV tables?) |
| 0xA0 | `MD00` aux model names |
| 0xA4, 0xA8, 0xAC, 0xB0, 0xB8, 0xBC | six `CP00` start-point sets (0xB4 always −1) |
| 0xD0 / 0xFC | embedded SSB script start / end ⚠️ = the light script below; **-1 in st12–15, st22–25 although the script is there**, so we find it by content |
| 0xF4 | an ALR-chunk run (chunk 0x0C holding a 0x05 animation of 90 frames) + footer ❓ — identical in st03/st13/st23, so not their lighting |
| 0xF8 | **per-type sound table** ✅ (READ, `FUN_1401fc750` / `FUN_1401fc710` / `FUN_1401df340`): `NM00 count` records of 8 × i16 (16 bytes), the table ends exactly at end of file in every stage that has one (st02-06, 08, 12-16, 22-26); −1 = none; absent (−1) in st01, st07, st09, st17, st32. Slot 1 = the **sound group** the object plays when it breaks (see "Object sound groups" below); slot 2 = the group stored on the debris it spawns (`FUN_1401e8960`); slot 4 = collapse delay in seconds (st08's 28 blocks, `FUN_1401df340` × 30 ticks), not a sound; slots 0, 3, 5-7 are −1 in every shipped file. Exported as `types[].sound_slots` in `stNN.props.json`. |
| 0x138 | file size ✅ |

## Placements (`PS0/`, `PS01`, `PS02`) — 0x24 bytes each

| Off | Type | Meaning | Status |
|---|---|---|---|
| 0x00 | i32 | object type (index into NM00/OB00) | ✅ all ids in range in every file |
| 0x04 | u8 | **class**: 2 = physical prop, 4 = trigger zone (jump zones, st09 `trigger`/`genkai` boundary), 1 = fixture/sign (st04 `kanban`, st02 car, st09 lights), 3 = door/event (st09 hub; the kind byte is then an event number 11..41) | ⚠️ from which names carry which value |
| 0x05 | u8 | **kind** (behaviour code). Jump zone = 16. Props use 1, 2, 3, 4, 6, 8, 17, 18, 23, 26, 28, 48, 53, 54, 58 and st01's 65..74 | ❓ see "Behaviour codes" |
| 0x06 | u16 | **param**: jump zones = launch strength (st05 120, st06/st08 250, st07 100, st08 also 200); st08's collapsing blocks 0..75 in steps of 5; st01 tower pieces 0..5 | ⚠️ jump ✅ that it varies with the stage; meaning of the others ❓ |
| 0x08 | u32 | always 0 | ✅ all files |
| 0x0C | f32×3 | position, game (left-handed) world space, same space as the stage ALR | ✅ see evidence |
| 0x18 | f32×3 | rotation, Euler X/Y/Z radians, **same convention as ALR joints: R = Rz·Ry·Rx** | ✅ see evidence |

No scale field; all objects are scale 1.

**Which list is the versus layout?** `PS0/` is used (falls back to the first list present: st24 only has `PS02`). ⚠️ Evidence: the guide's versus description of Panorama ("rooms with office desks … levels can be damaged or destroyed") matches `PS0/` (desks, chairs and the `yuka` floor sections are only in `PS0/`); `PS01` is a lighter variant (st06: 4 corner jump zones instead of 1 central one; st03: different bridge pieces). When the game uses `PS01` is ❓ (2v2/4-player? story?). Both lists are in `props.json`.

### Evidence for position / rotation

- **Positions:** with the Z mirror, the lowest point of each placed hull lies within 3 units of a floor (stage or another object) for st06 264/266, st03 158/179, st04 135/158, st07 4/4, st24 51/59 (`pd_convert stagedat stNN.dat`, line "lowest hull point vs floor"). Visually (`engine/shots/props_sheet.png`): st06 fence panels fill exactly the gaps between the stage's own posts; st04 `yuka` floor sections exactly fill the holes in the office floors; st02 `ironRoad` plates fill the gaps in the broken road. A mirror or offset error could not produce those fits.
- **Rotation order:** 30 % of placements have more than a yaw. For every tipped object that rests on something, the probe measures how far its hull's lowest vertices sit above/below the floor directly under them for 6 Euler orders. Totals (lower = better): ZYX (joint convention) 279 vs next best ZXY 385, XZY 406, XYZ 443 over st01–st08. Clearest cases: st06 tankA lying on its side (ZYX gap −1.0, others −6 to −15) and the 12 st02 `kabeC` barriers along the sloped highway (ZYX consistently −1.2 each, other orders scatter from −15 to +18). Visually: st03 café chairs sit upside-down on the tables, st06 drums lie flat on the deck.
- The `(±π, y, ±π)` pattern is common and is just an upright object turned by π−y (Rz(π)·Ry(θ)·Rx(−π) = Ry(π−θ)).

## Types: `NM00` names + `OB00` records (0x38 bytes each)

Names look like `st06_oilcanA_`; recolour stages reuse their family's names (st12 lists `st02_*`).

| OB00 i32 | Meaning | Status |
|---|---|---|
| 0 | **intact model**: index into the stage ALR's model list (= header offset-table order) | ✅ hull box matches model box for 16/20 st06 types, 40/43 st04 (`stagedat` "model box vs hull box"); renders are correct |
| 1 | **broken model** (pieces/wreck): e.g. st06 contaA → 19-joint debris model; heli → wreck | ⚠️ from model shapes (many joints, same footprint) |
| 2 | **damaged model**: same object bent/tilted (st06 guardrail with its joint rotated 0.32 rad, oilcan tipped 0.42 rad) | ⚠️ from joint rotations |
| 4 | offset of a collapse animation in `OA01` (only st06 `contaB`: frame numbers 0,1,2… with a value falling −0.02, −0.09, −0.20 … i.e. accelerating) | ⚠️ |
| 6 | offset of the **collision hull** in `OC00` | ✅ |
| 7 | offset of the collapsed-state hull in `OC01` (contaB) | ⚠️ |
| 9 | offset of the type's effect blocks in `OE00` | ⚠️ (blocks start with "EF00"); contents ❓ |
| 10 | offset in `OE01` (st06 bombe only) ❓ |
| 3, 5, 8, 11 | always −1 so far ❓ |
| 12, 13 | 0 or 0xCDCDCDCD (uninitialised padding) ✅ |

Types with no intact model (−1) are invisible: jump zones, st09 triggers/doors/lights, st04 `entA`.

**ALR model list layout** ✅: slots 0–7 are the stage itself (st06: 0 = main mesh, 2 = a 24,000-unit shell, probably the sky; others empty); then every model any OB00 record references (intact, broken, damaged); then the `MD00` aux models in order, skipping `NOUSE` (st06: `mdl10_sea` → model 63, a flat 7300-unit plane at y 0; `mdl12_flag` → a 6-joint flag; `crs04_contaB` → a contaB-sized crash effect; `mdl_hit_iron/stone/soil` → hit effects). We export slots 0–7 and the aux models as before, place the object models only where the `.dat` puts them (they used to be drawn once at the origin, under the sea).

## ✅ The sky is two shells, and one of them is built to scroll (2026-09-28) — this is why "the sky doesn't move"

Laurence observed in-game that the sky is static. The stage file says it should not be.

Stage model 2 (`gltf::SKY_MODEL`, the ~11,000-unit shell) is **two nodes, `m2_d0` and `m2_d1`**, and they are not two copies of the same thing:

| Node | Attributes | Texture | Meaning |
|---|---|---|---|
| `m2_d0` | `POSITION`, `COLOR_0` | **none** | the sky **gradient** — vertex-coloured, no texture. This is the sky/ground ambient ramp, and it is why the backdrop reads as a flat wash. |
| `m2_d1` | `POSITION`, `COLOR_0`, `TEXCOORD_0` | **image 20, a 512×512 PNG** | the sky **cloud/debris layer**, a texture. Sampler 0 is `wrapS: REPEAT, wrapT: REPEAT` ✅. |

**The cloud layer is authored to tile, not to sit still** ✅. Its UVs span **exactly 3.475 in both U and V** (U −1.735…1.740, V −0.738…2.737 — a 3.475 box, i.e. the author wrapped the shell deliberately), 236 distinct U and 244 distinct V values across 377 vertices, and the sampler repeats. A single 512² image covering a 22,000-unit shell at 3.5× tiling is a **scrolling cloud/dust layer**, and the only thing that makes it move is a UV offset over time. The geometry confirms the shell: x ±11,166, y −1,225…3,345, z ±11,185, radius 54…11,742.

**So: the sky *is* an untextured gradient (`m2_d0`) plus a tiled cloud layer (`m2_d1`) and nothing moves — implemented 2026-09-29 as `pd_game/src/sky_scroll.rs`, with three Bevy traps documented there and in AGENTS.md terms.** See "Sky scroll" below for the verified build and the A/B proof.

### ✅ Sky scroll — built and A/B verified 2026-09-29

`pd_game/src/sky_scroll.rs` drifts the textured shells' `uv_transform.translation`. **Verified by pixel A/B, not by inspection**: two runs of the same stage, same frame, same deterministic sim, scroll off vs on —

| Stage | Static baseline | Sky band | Ratio |
|---|---|---|---|
| st06 Storm Refinery | 0.14/255 | 12.98/255 (y1100–1200) | **90×** |
| st02 Spiral Highway | 0.23/255 | 69.26/255 (y900–1000) | **297×** |

and the A/B crop shows the cloud shapes **shifted**, not different. `cargo test -p pd_game`: **281 passed, 0 failed**. It resolves the right shells on every stage tried: st06 `m2_d1` (the untextured `m2_d0` correctly skipped), st02 `m2_d0`+`m2_d1`, st08 `m2_d1/d2/d3` + `m82_d0`.

**Three Bevy traps, all found by measurement, all now in the module docs:**

1. **`m2_` is not the backdrop's name on every stage.** st08's shell is `m82_d0`/`m82_d1`; st06's is `m2_*`. And *every* model instance is named `m<N>_d<M>`, so the name alone is not enough either. The filter is the name pattern **plus** a mesh world-extent ≥ 15,000 units (`is_backdrop_draw`), which cleanly separates the shells (19,698–37,959) from the largest prop (11,101). ⚠️ Note st08's `m82_d0`/`d1` are *identical* 1,319-vertex slabs at y −8868…−2985 — a floor, not sky, and they **are** textured, so they scroll too. Harmless, but the shell is `m2_d1` on st08, not `m82_*`.
2. **Asset writes are deferred.** Writing `Assets<StandardMaterial>` in an ordinary system schedules the change for the *next* frame's apply (Bevy `b0004`), so an `Added<MeshMaterial3d>` filter writes into the void and it is **silently dropped** — the A/B measured *zero* difference. The system must run in `PostUpdate`, after the apply.
3. **glTF entity names are synthesised by Bevy.** The file has no `Mesh` or `d0` nodes; Bevy names the mesh entity `Mesh`, its parent the draw with the model prefix stripped, and the real node name (`m82_d1`) is one level higher. So the node name must be found by walking up two parents, and `Mesh3d` is read from the material's *own* entity.

⚠️ **Still eyeballed, and honestly unverified against the original:** the *rate* — `SKY_SCROLL_RATE = 0.004` texture repeats/sec, i.e. ~4 minutes per tile. The original's exact speed is not recoverable (it was almost certainly the fixed-function texture-transform stage — neither `SD_ps` nor `PDEFTBB01_ps` contains a `sin`/`cos`/time term, and `pdvkSkyVision` is a stub with no decompiled body). The shipped value is chosen to sit near the threshold of notice, which is the 2004 look. `PD_SKY_SCROLL=<repeats/sec>` overrides it for A/B; the proof above used 0.9, ~225× the shipped rate, because the sim is not bit-identical between runs past a few hundred frames and the shipped drift is too slow to measure that way. ❓ A separate limit worth knowing: the shells scroll by UV, so **a stage whose visible backdrop is the untextured gradient (st08's sand) shows no motion at all** — the cloud layer is behind it there. Whether the original animated that gradient is still unknown.

## ✅ Lightning: the weather system, decoded (2026-09-28) — and it is NOT an effect emitter

Answering "are we done with lightning?" — **no, and the answer is more useful than an emitter would have been.** The class is `pdvkEleThunder` (RTTI at `0x4b6300`). Its ctor `FUN_1402251c0` builds a `pdProcessBase` (size 0x3d8) holding a 4-element array of 0x70-byte sub-objects at +0x58, and its vtable at **`0x14048cce0`** has four real slots — one dtor, one reset, one registration shim, and **`FUN_140226980`**, which just installs `LAB_140226d30` as a per-frame callback on the world object (`DAT_140533440 + 0x1b0`). The real work is `FUN_1402269b0` (161 decompiled lines), which is the **per-frame storm update**, and it is fully readable:

- **A 10-column weather schedule table** at `0x1402f5f10`, indexed `(column + band*10) * 0x10` = **10-byte records, 10 bands**. Column 0 = strike type, +0x04/+0x08 = two floats, +0x0c = a count, +0x10 = a second count. Read as int/float pairs, row 0 begins `(3, 1.0f, 1.0f, 90, 0)` and row 1 `(1, 1.0f, 0, 20, 2, 0, 1.0f, 0, 3, 0.5f)`. ⚠️ The 10 bands are almost certainly the storm phases (clear → building → strike → decay, or the ten weather states the stage can request) — the exact per-band meaning is not recovered.
- **A frame counter driven by 1/30 s**: `DAT_1402f7ca0 = 0.0333333…`, added to a float accumulator at `+0x4c` each tick ✅, confirming 30 fps and that this is a per-frame script, not a timed event.
- **The strike trigger**: `iVar13 = DAT_1403e0304 % 10` — a global counter modulo 10, and **type 1 only fires when `iVar13 == 2`** (`if (iVar13 != 2) return;` before the spawn). So a type-1 strike is a **1-in-10 gate on top of its own random timer**, which is why lightning reads as irregular. ✅
- **`rand()` drives everything**: the type-1 payload calls `rand() & 0x7fff` scaled into a small range, and type 3 (mode value 3) **re-rolls the band** with `rand() & 0x7fff >> 13` — i.e. it jumps to a random weather band, so the storm can restart anywhere. ✅
- **The lightning colour is a hard-coded constant: `0xFFDCDCFF`** — that is R=0xFF, G=0xDC, B=0xDC, A=0xFF, i.e. a **pale warm white with a faint pink cast** (a BGRA dword), passed to `FUN_140218a80` for each of the four segment vertices ✅. **This is the one genuinely useful number here**: it tells you the exact tint of the bolt geometry, and it is *not* one of our "electric blue" guesses. If we ever draw lightning, the bolt should be near-white, slightly pink — not cyan.
- **The bolt is a 4-segment strip** (loop `iVar15 < 4`, stepping a vertex cursor by `0x16` = 11 dwords = 44 bytes per vertex), built by interpolating between the previous and next strike point, with the third vertex of each segment **halved** (`fVar17 = DAT_1402f7d40 = 0.5`) — i.e. **each segment tapers to half-width toward the middle**, producing the classic jagged bolt rather than a constant-width ribbon. ✅ `fVar24` lerps the strike position from `+0xa0` toward `+0x9c` by a normalised `+0xa8` timer, and the whole thing is clamped by `DAT_1402f7da8 = 1.0`.
- **A countdown in frames**: `+0xa4` decrements each tick and `+0xa8 = frames_left / DAT_1402f5f1c[band]` is the 0..1 progress used for the lerp and taper. So the *duration* of each strike is a per-band frame count at that table offset, and the bolt visibly **grows and shrinks** over its life. ✅
- It then calls `FUN_140199a30()` (a light/effect flush) and returns.

**So: lightning is geometry the renderer builds, not an `.eft` emitter.** That is consistent with `FORMATS-EFFECTS.md` never listing a thunder kind among the 13 decoded emitter kinds — the `THUNDER01` emitter kind that *does* exist (53 instances, 6 effects) is a **skill VFX** (a lightning-column attack), a completely separate thing from the ambient storm. ❓ Two open threads, both cheap: the exact per-band meaning of the ten weather bands, and whether a stage ever requests a non-default band (which would explain the original's sky changing with weather). **We are not done with lightning, but we now know exactly what it is and what colour it is** — and the answer is that it never goes through our effect pipeline at all.

## ⚠️ The RTTI *names* are present (1,235 of them) but the RTTI is **not** wired up — read this before trusting it (2026-09-28, corrected twice)

**This section was written wrong twice today and the correction matters, so here is the whole story.** The binary contains **1,235 `AV<Name>@@` type-descriptor name strings** and they are a spectacular index of the engine — but they are **orphans**. The decisive test: for every one of the **1,060** real classes, search the entire file for an 8-byte pointer to its `TypeDescriptor` and you get **zero hits — including for `pdvkEleThunder`, whose vtable (`0x14048cce0`) I located by hand and confirmed by disassembly.** A working MSVC x64 RTTI chain is `TypeDescriptor` ← `CompleteObjectLocator` ← `vtable[-1]`, and **not one `CompleteObjectLocator` in this build points back at any of them.** The `pVFTable` in each descriptor is the same stub (`0x1402d47e8`) and `spare` is 0 in all of them.

**So: the names are a naming table, not runtime type information.** They are still enormously useful — they tell you the game's own name for every subsystem — but you **cannot** walk name → vtable the way you normally can, and any claim that "the RTTI is intact" is wrong. **A5's original judgement ("type descriptors are stripped, so a static walk from the name is a dead end") was correct, and my two corrections of it were both wrong.** The lesson: I twice asserted a positive finding from a partial search, and the check that settles it is the *negative* one — count the failures, not the successes.

**What the names actually give us** (all verified present in the binary, and this is the part worth keeping):

- **The engine's own class names, in its own spelling.** Note the real prefixes are **`alfn_` and `alsh_`**, not `valfn_`/`valsh_` — an earlier version of this section had that wrong too. Emitter function classes: `alfn_FunctionPARTICLE01/02`, `alfn_FunctionmsPARTICLE00/01/02`, `alfn_FunctionmsBillBeam00`, `alfn_FunctionBLADE01/02`, `alfn_FunctionFLAIR01`, `alfn_FunctionLIGHT01/02`, `alfn_FunctionTHUNDER01`, `alfn_FunctionTALE01/02`, `alfn_FunctionAURA01`, `alfn_FunctionESPHERE`, `alfn_FunctionFILTER01`, `alfn_FunctionECUBE`, `alfn_FunctionEVECT`, `alfn_FunctionFSPR`, `alfn_FunctionFMP`, `alfn_FunctionVKBLUR`, `alfn_FunctionVKEMITTER`, `alfn_FunctionykFADE00`, `alfn_FunctionykPARTICLE00/01`, `alfn_FunctionykSOUND00`, `alfn_FunctionykTRASH00`, plus `alfnFadeFunc`. This **confirms all 13 emitter kinds by the game's own names** and independently confirms the three I decoded from the packed base-40 pairs (`msBillBeam00`, `THUNDER01`, `ykFADE00`) ✅ — three independent methods now agree.
- **A near-complete shader inventory** — ~130 `alsh_Shader<Name>` classes, including the `msRGBLUR00/01` blur pair that consumes the glow target (A2) ✅, the `vkCOLANIM*` dissolve family, and things nobody has looked at: `alsh_ShaderSBOSHDR` (character HDR), `alsh_ShadervkRAYLIGHT` (a volumetric light shaft!), `alsh_ShadervkFRESNEL`, `alsh_ShadervkBLINK`, `alsh_ShadervkAZIERPIPE`, `alsh_ShaderSXRAY`, `alsh_ShadervkSAND`, `alsh_ShadervkWATER`/`WATER2`/`WATER3`/`WATER3EDGE`, `alsh_ShaderSCREFC*` (screen reflections!), `alsh_ShaderisYOGORE`, `alsh_ShaderPDEFTFILLET01`. ⚠️ **This list is the single best "what have we never looked at" inventory in the project** and it is not in any doc until now.
- **The particle class names the research wanted**: `MSParticleFunc`, `MSParticleFunc00`, `01`, `02`, **`03`** (five, at file offsets `0x4b2e1c`–`0x4b2f7c`), alongside `MSEffect`, `MSEffectCache`, `MSEffectDraw`, `MSEffectEI`, `MSEffectFile`, `MSEffectParticle`, `MSEffectParticleSimple`, `YKFadeFunc00`, `YKParticleFunc00/01`, and base classes `alFunctionBase`, `alFunctionExec`, `alShaderBase`, `alShaderFunc`, `alBaseObjectClass`, `alToolPack`, `alTime`, `alWindow`. ⚠️ **`MSEffectEI` is a name nobody has ever seen in a doc and is an obvious "EffectEmitter/EffectInfo" candidate** — worth opening if MSPARTICLE's per-emitter layout is wanted.
- **The 70 `pdvk*` classes are a readable index of the engine** (see the table below), and these *are* findable: the class is stamped by name in the decompile (`*param_1 = pdvkXxx::vftable`), which is the route that worked for `EleThunder`, `Glow2`, `GlowExec` and the footstep classes. **So the working method is: grep the decompile for `ClassName::vftable`, then read the vtable.** That is the recipe for any system on the list below.

| Group | Classes | What it tells us |
|---|---|---|
| **Game shell** | `pdvkGame`, `pdvkBattle`, `pdvkMenu`, `pdvkTitle`, `pdvkService`, `pdvkCamera` | the six top-level modes; `Camera` is its own element class, so round framing is a system, not a constant |
| **Effects** | `pdvkEff`, `pdvkEffArea`, `pdvkEmitterExec`, `pdvkGlow2`, `pdvkGlowExec`, `pdvkCapEffect`, `pdvkSpecial`, `pdvkPlyEffBase`, `pdvkEffBATTLEEFFECT_00`, `pdvkEffBUFOFFALPHA_00`, `pdvkEffPRERENDER_00`, `pdvkPrimitive` | the effect pipeline, **including `Glow2`/`GlowExec`** — the A2 glow pass is its own named class, confirming it is a first-class subsystem and not a shader side-effect. `PRERENDER_00` matches the fullscreen `PreRender_ps` pass ✅ |
| **Weather / ambience** ❓ **new** | `pdvkSStorm`, `pdvkSkyVision`, `pdvkPlyDustType0`, `pdvkPlySandType0/1`, `pdvkPlyWaterType0/1`, `pdvkPlyFootBase`, `pdvkEbSmoke`, `pdvkEbSteam`, `pdvkEbBeltSmoke`, `pdvkEbSpark`, `pdvkEbWaterDrp` | **`pdvkSStorm` is the storm system that owns `pdvkEleThunder`** (it was constructed by the same `DAT_140533440` world object). And there are **seven distinct footstep/ground-effect classes** — dust, two sands, two waters, plus a foot base — i.e. **the original picks footstep VFX per ground surface**. Nobody has looked at that. |
| **Feedback / juice** | `pdvkComFeedBack`, `pdvkFeedBack`, `pdvkEbScrShk` (screen shake), `pdvkEbCrash` | screen shake and hit feedback are a named subsystem; the 32-level stipple fade I found earlier (`FUN_1402a5f60`) is the same family |
| **Collision / physics props** | `pdvkCollapse`, `pdvkCollapse2`, `pdvkDestroy`, `pdvkCrash`, `pdvkEbEffMdl`, `pdvkJmpPoint`, `pdvkEbJmpPoint` | destructibles have a two-stage collapse system |
| **Characters** | `pdvkPlayerBase`, `pdvkPlayer2`, `pdvkNonPlayer`, `pdvkCapsule2`, `pdvkEnvCapsule` | the capsule is its own system (ties to the capsule-colour table found earlier) |
| **UI** | `pdvkMenu_AnimTex`, `pdvkMenu_BlurTex`, `pdvkMenu_FadeTex`, `pdvkMenu_ColMes`, `pdvkSPaper`, `pdvkSPaperNear` | the menus have a blur pass and a paper-stack effect — the `qdraw`/`pdevrect` debug shaders found earlier are the UI's own |

**Correction to record:** A5's "no RTTI descriptors" is retracted in the same file (see its update). The practical upshot is that **every remaining ❓ about the effect system now has a named class to open**, and the naming is a gift — `SStorm` is obviously "sand storm", `PlyFootBase` is obviously the footstep selector, `Glow2` is obviously the glow subsystem. That turns blind address-hunting into a lookup. ⚠️ RTTI gives the name and the vtable address, **not** the behaviour; each still needs its method read, which is what a lane does well. But the map is now on paper.

## ✅ Footsteps are selected by ground material — the full 18-case dispatch, recovered (2026-09-28)

Completely untouched until now: `grep` for `footstep` / `surface_id` / `ground_material` returns **nothing** in `pd_game` and nothing in the docs. The original has a **ground-material → footstep-VFX dispatch**, and it is a single readable `switch` (`FUN_140230e10` area, `decomp_all.c:452366`):

| `param_2` (surface id) | Footstep class spawned |
|---|---|
| **3, 9, 0x0C** | `pdvkPlySandType0` — **sand** |
| **5, 6, 8, 0x0B, 0x15** | `pdvkPlyDustType0` — **dust/dirt** |
| **0x0D (13)** | `pdvkPlyWaterType0` — **water** |
| **4, 7, 0x0A, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14** | **nothing** — no footstep effect at all (10 ids: presumably hard floors, walls, and interior surfaces) |
| default | return, no spawn |

✅ Facts that hold across every spawning branch: the object is a **0x60-byte** instance from the game's object allocator (`FUN_140285aa0(DAT_14052dfe8 + 0x10, 0x60, 0x10)`), the vtable is stamped in one shared tail (`LAB_140230e9c`), the instance stores its owning player at `+0x0b` and the player's **surface index at +0x54** (`*(param_1 + 0x128)`, i.e. read from the *player's* record — a 0x58-stride array, so this is per-player state, not per-object), `_Dst[2] = -1` (no effect yet), `_Dst[5] = 2` (**the ground-effect class id 2**), and each spawn calls **`FUN_1401cbbc0(vtable, *(param_1 + 0x50), _Dst, 0x1e0c350, 2, 0)`** — the fourth argument `0x01E0C350` is a **packed effect/position reference**, almost certainly the stage's effect-data table entry that names *which* puff effect to use for this surface. `*(int *)(param_1 + 0x58)++` counts the live effect, and a companion function (`FUN_140231010`) enforces a **10-frame minimum gap** between footstep spawns and reads the stage record at `DAT_1404bc9d8[*(param_1 + 0x60)]` — i.e. **the surface table is per-stage** ✅.

**What this means:** the original plays a different footstep puff depending on what you're standing on, chooses it from a per-stage material table, and rate-limits it. We have no footsteps at all. This is a **clean, self-contained lane** — the class list is 3 named behaviours, the spawn call shape is known, and the only genuine unknown is the per-stage surface-id table's meaning (❓ which id is "grass" vs "concrete"; `0x0D` = water is the one confident label). **A cheap faithful default is three VFX picked by a surface tag the stage already has.**

## ✅ `vkCOLANIM*` is a two-texture dissolve, and it confirms the glow target on 5 more shaders (2026-09-28)

Never disassembled before, despite being in the RTTI as `alsh_ShadervkCOLANIM`, `…2`, `…3`, `…4`, `…A` — **five variants of one idea**. `vkCOLANIM_ps` is only 17 instructions and reads completely:

```
sample r0, v1.zw, t0, s0      # texture 0
sample r1, v1.xy, t1, s1      # texture 1   <-- a SECOND sampler+texture
…alpha-kill both…
ge   r0.w, cb0[16].x, cb0[0].w # cb0[16].x >= cb0[0].w  -> discard
mul  r0.xyz, r0.xyz, r1.xyz    # MULTIPLY the two together
mul_sat o0.xyz, r0.xyz, cb0[0].xyz
mov  o0.w, cb0[0].w
mov  o1.xyzw, cb0[17].xyzw     # <-- the glow target, written unconditionally
```

✅ Three things fall out. **(1) It is a dissolve/threshold blend**: two textures multiplied, with `cb0[0].w` (the material alpha) compared against `cb0[16].x` — a 0..1 progress value that erodes the surface as it rises, which is exactly how the original does its "materialise/dissolve" transitions (Phantom Dust's dust-to-solid arrival and its object despawns). **(2) It writes `o1 = cb0[17]`**, i.e. the glow-mask colour — so the glow target claim in RESEARCH-LIGHTING (A2) now stands on **217 of 219** pixel shaders *plus* these five named ones, and the mask colour for a dissolving object is the material's own `cb0[17]`. **(3) The variants are a size ladder**, not different maths: `_ps` 1,568 B, `_2`/`_3` 1,936 B, `_4` 2,240 B, `_A` 2,096 B — and `vkCOLANIM2_ps` immediately widens from **2 samplers to 3** with `Texture2`, so the family is "dissolve with N extra decal/overlay layers" ✅.

❓ Not read: which of the five is used for what (arrival vs. despawn vs. ghosting), and whether `cb0[16].x` is animated per-frame or driven by a skill effect. **This matters more than it looks**: if our engine has no dissolve at all, then every Phantom Dust arrival — a fighter materialising out of dust at round start — is currently a pop-in where the original had a 0.5 s erosion. Worth a look at our own load-in path. ⚠️ (Named `COLANIM` = "colour animation", so the family likely also covers the aura/tint pulsing the school-aura lane is building — the two may share a shader.)

## ✅ The glow pass is a named subsystem with a known shape: `pdvkGlow2` + `pdvkGlowExec` (2026-09-28)

This closes most of what A2 left open. The RTTI gave us the two class names; their vtables give the structure:

- **`pdvkGlowExec`** (`DAT_1405332f8`) is a **0x50-byte** executor object. `FUN_1402243e0` — its vtable slot 2 — is a one-line reset (`_DAT_1403e4dd8 = 0`), and slot 4 (`FUN_1402243f0`) is the per-frame draw: it **gates on a bitmask** (`DAT_140530034 & 8` — bit 3 of a global render-enable word, so the glow pass is *switchable at runtime*, presumably a graphics option) and then does the constant upload + two setup calls: `FUN_14001e540(0xd, &DAT_1405a6a00)` (set shader consts, id `0x0D`) then `FUN_14028d720()` and `FUN_14028d7c0()` ✅. **So the glow pass is a normal per-frame draw call, not a post-process hook** — it runs inside the frame like any other pass.
- **`pdvkGlow2`** (`DAT_140533300`) is a **0xF8-byte** object that starts life as a `pdStageObjEffBase` (the vtable is stamped as `pdStageObjEffBase` then **overwritten** with `pdvkGlow2`'s — the standard two-step class init), and carries its defaults in the ctor: `puVar5[0x18] = 0`, `[0x11] = [0x13] = [0x14] = [0x15] = [0x16] = [0x17] = 0` (offsets +0x88…+0xBC zeroed), a default pair loaded from `DAT_1405b1890`/`DAT_1405b1898`, and **`*(u8 *)(+0x84) = 1`** — a single enable byte, default on ✅. It is registered with `FUN_1401cbbc0` like every other stage effect.
- **Its init (`FUN_140224370`) is the most informative line in the whole subsystem** ✅:
  - `memset(&DAT_1405a3860, 0, 0x2400)` — a **9,216-byte table** of glow state, zeroed at startup. 9216 / 4 views = 2,304 bytes per view; 9216 / 2 views = 4,608 per view. Either way it is a **per-view array**, consistent with one glow surface set per split-screen view.
  - It then creates **exactly two shader objects**, once each, guarded by `DAT_1403e4ddc` / `DAT_1403e4dd8`: `FUN_14001bcd0(&DAT_140410040, PTR_DAT_1403e4de8, …)` and `FUN_14001bf80(PTR_DAT_1403e4de0, …)`. **Two shaders for the whole glow pass** — and the two template pointers resolve to `0x140410170` and `0x1404101f0`, matching the `msRGBLUR00` / `msRGBLUR01` + `GFilter_ps` / `vk_glow_ps` chain already identified ✅. So the two stages are: **filter/threshold, then blur** — the classic 2004 selective-bloom pair.
  - `DAT_140410040` is the **fullscreen-quad vertex template**, and it is unambiguous: `(-1,-1) (1,1) (-1,1) (1,-1)` corners with a matching `1.0/-1.0` texcoord set, plus a `0x20000000`/`0xFFFFFFFF` header pair. **The glow pass is a fullscreen quad** ✅ — no geometry, no per-object re-draw.
- `FUN_1402243e0`'s `_DAT_1403e4dd8 = 0` and the `FUN_14028d720` / `FUN_14028d7c0` pair are the two RTs of `DAT_1405b0760` being set and cleared.

**Net result for A2:** the original's glow is **threshold → blur → additive composite, as a fullscreen quad, per view, with a runtime enable bit and a per-object mask colour from `cb0[17]`** ✅. That is *exactly* what Bevy's `Bloom` + an emissive threshold does, which is why the earlier recommendation (emissive bloom, not literal MRT) holds — and it is now justified by the program's own structure rather than by analogy.

### ✅ The blur kernel, read from both `msRGBLUR` shaders (2026-09-28) — the last big piece of A2

Both are `ps_4_0`, both **four-tap**, and both read three colour weights from the constant buffer. `msRGBLUR00_ps` (25 instructions, 3 samplers s1–s3 / t1–t3) and `msRGBLUR01_ps` (28 instructions, **4** samplers s0–s3 / t0–t3 — the extra tap is the centre read of a 5-tap box). The weight chain is a textbook `mad` cascade:

```
mul  r0.xyz,  t0,            cb0[0].xyz      # tap0 * w0
mad  r0.xyz,  t1, cb0[1].xyz, r0.xyz          # + tap1 * w1
mad  o0.xyz,  t2, cb0[2].xyz, r0.xyz          # + tap2 * w2        <-- msRGBLUR00 ends here (3 taps)
mad  o0.xyz,  t3,            r0.xyz          # + tap3 * 1.0       <-- msRGBLUR01 only
mov  o0.w,   <alpha of the base tap>
mov  o1.xyzw, cb0[17].xyzw                    # glow target again
```

✅ **So the kernel is a 3-tap (00) or 4-tap (01) weighted box with runtime weights in `cb0[0..2]` and an implicit unit weight on the final tap — no Gaussian, no radial falloff, no mip pyramid.** That is a genuinely 2004 choice: a cheap linear box over 3–4 offset samples, and the offsets come from the *vertex* stage (each tap has its own UV set as a separate varying — `v1.xy`, `v1.zw`, `v2.xy`, `v2.zw` — so the tap positions are baked per-vertex in the fullscreen quad, not computed in the shader). **This is a one-line-per-tap bilinear sample, which is why it was affordable at 4 views on 2004 hardware.**

Also confirmed in both: a **threshold discard** `ge cb0[16].x, <alpha>` → `discard` (so the blur only runs on pixels that passed the glow test), and in `msRGBLUR00` a clamp `max(v1.z, -1) … min(…, 1)` applied to a varying before the compare — a per-vertex fade mask ✅, i.e. **the glow can be faded per-quad, not just globally**.

❓ Now closed for A2: kernel shape ✅, pass order ✅, fullscreen-quad ✅, per-view ✅, mask colour source ✅, runtime enable ✅. **Still open, and both are runtime-only:** the actual `cb0[0..2]` weight *values* (set on the CPU each frame, so not in the binary) and the mask surface resolution ratio. Neither blocks anything: for the Bevy route we pick our own weights anyway, and the recommendation is unchanged — **emissive bloom, not literal MRT.**

## ✅ The original had screen-space reflections and Fresnel in 2004 — `SCREFC*` and the `SBOSREFS*` family (2026-09-28)

Found via the `alsh_Shader*` inventory. Two never-disassembled families, both **genuinely screen-space reflections** — which matters directly for the "modern lighting" brief, because it means the 2004 game *already had* the effect that was supposed to be unaffordable then.

**`SCREFC_ps` (29 instructions) — a Fresnel-weighted reflection off a scene texture.** It is fully readable:

```
sample r0, v1.xy, t0        # the surface / reflection source
sample r1, v3.xy, t1        # the SCENE texture (a second sampler + texture)
add    r0.w, r0.w, -0.5
mad    r0.w, r0.w, 2.0, -0.5      # remap alpha to -0.5..1  (Fresnel term)
dp2    r0.w, cb0[0].ww, r0.ww      # x the material's specular power
…threshold discard…
max/min r2, v2, ±1                  # clamp the per-vertex tint
add    r3.x, -r2.w, 1.0             # fresnel = 1 - n·v
mul    r1.w, r1.w, r2.w
mul_sat r1.xyz, r1.xyz, 4.0         # reflection x4 gain
mad    o0.xyz, r2.xyz, r0.xyz, r1.xyz   # lit surface + fresnel*scene*4
mov    o1.xyzw, cb0[17]             # glow target, as everywhere
```

✅ So it is **Fresnel (1 − N·V) modulating a sample of a scene render target, with a ×4 gain and a specular-power threshold.** That is textbook screen-space reflection, on a 2004 console, as a *material* effect — not a global pass. `SCREFCAL` / `SCREFCAL` (alpha variant) share the vertex shader, so it is the same effect with different fixed-function state — the same suffix-means-blend-state pattern proven for `PDEFTBB01*` ✅.

**And the character shader has it too.** `SBOSREFS_ps` binds **3 samplers / 3 textures** and takes **6 varyings** (`v1.yz`, `v2.xy`, `v2.zw`, `v3.xyzw`, `v4.xyz`, `v5.xyz`) — a scene sample plus normal/position plus a view vector. So **characters reflect their surroundings.** The `SBOS*` family is a suffix ladder over one vertex shader family (all `_vs` 2,608–2,892 B): `SBOS` (3,792 B ps) plain, `SBOSAL` alpha, `SBOSHDR` (2,996 B) HDR, and then `SBOSREFP`/`SBOSREFS`/`SBOSREFC*` — **REFS = reflection source, REFP = reflection profile, REFC = reflection colour** ⚠️ (naming read, not verified).

**What this changes for the modern-lighting decision.** The research brief asked me to weigh SSR and noted it as a candidate modern effect. The finding is that **the original already had per-material SSR and Fresnel on characters, and it is cheap** — one extra texture fetch and about a dozen ALU ops, in a shader that was already fetching three textures. It is *not* a modern luxury; it is a 2004 technique we have simply never implemented. ❓ What is not recovered: which materials/stages actually select the `S`/`P`/`C` variants (the model data's material flag, not read), and what `cb0[0].w` (the specular power) is per surface. **Recommendation stands unchanged but for a better reason: if we implement SSR at all, do it as this game did — a per-material Fresnel-weighted scene fetch, not a full modern SSR pass with roughness cones, temporal accumulation and a depth pyramid.** That is both more faithful and materially cheaper, and it is the same conclusion the blur-kernel read produced: copy the result, not the modern mechanism.

Also from the same inventory, still unread and now the obvious next names: `alsh_ShadervkFRESNEL` (Fresnel on its own), `alsh_ShaderSBOSREFC*` (character reflection colour), `alsh_ShadervkRAYLIGHT` — which turned out **not** to be a raymarch: `vkRAYLIGHT_ps` is 14 instructions, a single texture fetch on a textured quad with a `cb0[16].x` threshold, and its vertex shader is a 7-instruction `dp4` chain with **no ray loop** ✅. **"RayLight" is beam geometry** (a stretched quad, `o1 = v2 * cb0[12].z`), i.e. a fake volumetric shaft drawn as a billboard. Same trick as the lightning bolt. ❓ Which lights get shafts is still unread, but the mechanism is settled and it is *cheap* — one additive quad per light.

## ✅ ALR chunk `0x12` — per-stage cull volumes, decoded (2026-09-28)

`0x12` was listed as a bare "❓" in the chunk table. It is now read, at the mechanism level, from the program and the data together.

**The handler is `FUN_140295a70`** (chunk-handler table at `0x140410608`, row `0x12` → `0x140295a70` / second pass `0x140295f90`). What it does, in order:

1. Reads a **float3 position** from the record, transforms it by a **global 3×4 matrix** (`DAT_14057f378` + `(index+1)*0x40`, i.e. a per-object row — so `+0x1a` is an **object index into a global transform table** ✅), producing a world-space point.
2. Applies a **colour transform** to a second float3 (the `DAT_1405a5ca0..cd0` coefficient block) — i.e. the record also carries a **colour**, and the handler colours it.
3. Calls **`FUN_140293140(&colour, radius)`**, which is unambiguously a **visibility cull test**: it evaluates a set of half-space planes (`DAT_14057f374`, `DAT_14057f388`, and the four `_DAT_1405a69b4/8` + `_DAT_1405a6a30/38` normal pairs) and returns true only if the point is inside all of them. That is a **frustum / bounding-sphere reject** ✅. If the test fails, the function `goto`s past the work and the object is skipped.
4. Only on a pass does it gate on `0x800` in a debug word and on material flag `0x2000`, then proceeds to register the object.

**The data confirms the shape.** `0x12` is a **fixed 44-byte body, one record per chunk** (250 of them across the 30 stage files — 54 in `st08`, 196 in `st09`, and **zero in the other 28 stages**). Layout read off the bytes:

| Offset | Type | Reading |
|---|---|---|
| `+0x00` | `float3` | world position ✅ (the handler's x/y/z) |
| `+0x0C` | `float` | the **radius** passed to the cull test ✅ (handler reads `+0x14` chunk-relative = `+0x0C` body-relative) |
| `+0x1A` | `u16` | an **object index** into the global transform table ✅ — sequential and grouped in the data (st08 starts 49,50,51,52,61,61,28,29,…; st09 starts 22,93,94,139,191,1,2,3,…) |
| `+0x1C` | 16 bytes | always zero in every record checked |

✅ The radius field behaves like a radius: `st08` spans 117–4133 (a big open stage), `st09` spans **1.45–112** (small, tight geometry) — the magnitudes track the stage, which is what a cull radius should do. ❓ Not resolved: what the `+0x0C` radius is *for* when the handler also has a colour (a light? a fog volume? a spawn?), and why only two stages use it. **`0x13` (1,789 chunks, fixed 136-byte body, handler `FUN_140295d90`) is the next untouched id and is the obvious follow-up** — same table, same session, same method.

## Hulls (`OC00`, `OC01`) ✅

A type's hull is a run of 12-byte-headed chunks: `{u8 flags, i8 id, u8, u8, u32 body size, u32 0}`.

- id 3 = faces: 10 f32 (centre xyz, radius, min xyz, max xyz), u16 face count, 3×u16, then per face `{u16 a, b, c, u16 attr, f32 normal[3]}` (20 bytes).
- id 8 = vertices, f32×3 each; the faces index the vertex chunk that follows them.
- id −1 (0xFF), size 0 = end of this type.

Object-local, same space as the model (checked by box comparison above). Mostly boxes (12 triangles); handrails are thin slabs, st06 heli 48 faces. `attr` is 0x77FC on most st06 faces, 52 on the heli ❓ (surface material? MD00 names hit effects for iron/stone/soil). Face-chunk flags byte: 1 normally, 6 on contaB, 0x10 in the untagged 0x94 block ❓ -- it IS the surface class (2026-10-03, COLLISION.md "Per-triangle class": which queries skip which class is READ); `HullFace::class` carries it per face into the `.pdcl` `CLAS` trailer. Jump-zone types (`stNN_jump_`) have a hull but no model: a launch trigger, not a wall.

## Start points (`CP00`) ✅ layout, ⚠️ use

`"CP00"`, u16 count, u16 byte size (count × 0x50), then per start:

| Off | Meaning |
|---|---|
| 0x00 | player position (game space) ✅ on a floor |
| 0x0C | yaw (radians); with the capsules below, the player faces game (sin yaw, 0, cos yaw) ✅ in sets 0xA4/0xB8 |
| 0x10 | f32 ~0 ❓ |
| 0x14 | three capsule spawn positions ✅ (in sets 0xA4/0xB8 they lie exactly 50/100/150 units ahead of the player; in the other sets they are spread along paths) |
| 0x38 | 6 u32: 0xFFFFFFFF, 0x80, 3, 0x18, 0, a pointer-like value ❓ |

Palace (st03/13/23) set 0xA4: the player y is 2.4–11 above the hall floor (y 50), the other sets ~10 above like every other stage (st06 +11): starts are on the floor, not below it. With the current build players start standing ✅ (`shots/palace_rp_sheet.png`). The "falling in a black void" before-shot came from `--near`, whose floor survey skipped st03's floor (exactly y 50, the survey wanted y > 50) and put players on the roof outside the hall; `--near` now surveys the floor under the first original start.

Six sets per stage, four starts each (0xB0 has one start whose "capsules" are spread across the map: story/solo?). 0xB8 duplicates 0xA4 in st06. Which set the game uses for a 1v1 (random? by player count?) ❓.

## Behaviour codes (kind byte) — what we know ❓

Per-type, constant except where noted:
- 3: breakable things with a broken model (st06 drums/crates/fans/tank/heli, st03 tables/benches/signs, st04 desks/counters/lockers, st02 wall blocks/lamp posts).
- 1: guardrails, handrails, cardboard boxes, ashtrays, benches, lockers, stairs pieces, fences; mostly no broken model but a damaged model.
- 2: small light objects without a broken model (st02 barricades, cones, road signs).
- 6: st06 `contaB` (huge container that collapses: animation + collapsed hull).
- 16/17/18: 16 = jump zone; 17/18 = st03 gift boxes, chairs and glass railings **per instance** (the same type also appears with 1 or 3) — bit 4 looks like a per-instance flag ❓.
- 23: floor/bridge sections (st04 `yuka`, st03 `bridgeA/B/C`) — the "levels that can be destroyed". 28: pillars in rooms / bridge supports. 26: st02 iron road plates. 48: pillars and big signboards. 53/54/58: st05 flying cans / signs / debris, st08 building blocks (with param = collapse order ⚠️). 65–74: st01 tower pieces (enum 64+n).

Read as a bit field, bit 0 ("1") and bit 1 ("2") line up with "has a damaged model" / "has a broken model" for most but not all types (e.g. st02 `kabeA_a` is 1 but has a broken model). So: ❓, recorded raw in `props.json` for the sim lane.

Not found in the file: hit points, weight/STR, or an explicit "throwable" flag. They are presumably in the exe or the SSB scripts ❓.

## Surveys

| Stage | Layout used | Objects placed (visible) | Notes |
|---|---|---|---|
| st06/16/26 Refinery | PS0/ 266 | 265 + 1 jump zone | fences, drums, gas bottles, containers, tanks, heli wreck |
| st04/14 Panorama | PS0/ 158 | 157 + `entA` | desks, chairs, lockers, floor sections, pillars, stairs |
| st24 Panorama Boss | PS02 59 | 58 | |
| st03/13/23 Palace | PS0/ 179 | 179 | café tables/chairs, signs, benches, railings, bridges; a ruined shopping mall around a giant tree (matches Laurence's reference screenshots of Lost Palace: tree, escalators, shutters) |
| st02/12/22 Highway | PS0/ 216 | 216 | road plates, barriers, cones, lamp posts, gate |
| st05/15/25 Strange City | PS0/ 62 | 57 + 5 jump zones | signs, cans, debris |
| st07/17 Lane | PS0/ 4 | 0 + 4 jump zones | |
| st08 Sein | PS0/ 49 | 45 + 4 jump zones | the ground itself is 28 collapsible blocks |
| st01 Edgar's Dream | PS0/ 106 | 106 | modular tower pieces at the origin, rotated in 60° steps; param 0..5 ⚠️ = sector |

## Aux models are not scenery: the st08 "black void" (lane FIXST08, 2026-10-01) ✅

The `.dat`'s `MD00` names (80 slots, "NOUSE" = empty) the extra models stored **last** in the stage ALR, in slot order
(`pd_convert stagefx` already exports them as effects). They are effect assets: per-object glow halos
(`stNN_mdl_glow_<object>`), fall/tower/particle emitters, hit sprites, debris chips. The game draws them through effect records only.
The stage export used to treat everything that was not an object template as scenery, so it also drew these in place.
On st08 that put 30 glow halos (models 72-81 and 103-122: 80 triangles each, vertex format 0x17, material flag 0x11, one
256x256 texture: black fingers fading into sand orange, alpha all 255 - a texture meant for additive blending, black = nothing) around
the islands, **opaque**: huge hard-edged black masses over the sand backdrop and, mid-break, a grey card with a sawtooth top edge
(`critic_st08_t0250.png`, `critic_fall08_t1100.png`). They also fed collision: `--collision dat+walls` (retired 2026-10-03) added the render's steep triangles, and
the halos are all steep (80 of 80 "walls"), so st08 (and every stage with aux models: st01-st08, st12-st17, st22-st26, st32) had invisible
wall triangles at the debris/glow models' place.

Evidence: stripping nodes from the exported glb one model range at a time (`m72..m122`) is the only change that removes the black on st08, nothing else
(the sky shells `m2_*`, the sand floor `m82_*`, `m0`, `m1` do not matter; `PD_SHADOWS=0`, `PD_STAGE_LIGHT=0` and the pre-L2/L3 glb all show the same black).
Models 96-98 = `hit_stone` x3, 99 = `hit_smoke`, 100-102 = `hahen_stone0-2` line up with the names, which is what pins the base.
Rule: `StageDat::aux_models(model_count)` = the last (non-NOUSE names) models; empty if an object template falls in that range
(a count mismatch must never hide real stage models). `build_stage_props` adds them to `skip_models`.
Tests: `stagedat::tests::{aux_models_are_the_last_models_by_non_nouse_names, st08_glow_halos_are_not_part_of_the_stage_export}`.
Not decided: how the original blends the glow halos when an effect plays them (additive is the natural reading of the texture).
Open: st08's cloud sky stays grey cloud over a near-black dome (unchanged since the first lit export, `fix_st08_sheet3.png`).

### st09_07 black slabs = the same aux-model fault, in the area files (lane STAGEFIX, 2026-10-01) ✅

The area files (`st09_01..08.dat`, magic `AR0n`) were returned empty by `StageDat::parse`, so their MD00 aux-model names were never read and `aux_models` found nothing to skip. The MD00 section offset of an area file is the dword at +0x0C (st09_07: 0x120; the ST00 files keep it at 0xA0). st09_07's four names (`m_ray`, `m_neon`, `m_lamp`, `glare_dds_dummy`) are its last four models 10-13; `m_ray` (model 10) is three 40-47 u wide, 400 u tall vertical strips (vertex format 0x09, material flag 0x11, a 64x256 near-black texture): light shafts for additive blending, drawn opaque = the black slabs. Now skipped like st08's halos (test `stagedat::tests::st09_07_area_file_aux_models_are_skipped`). Proof `engine/shots/sf_st09.png`. Remaining dark polygons in st09_03 (non-versus) are not this cause (UNCONFIRMED).

## Stage shadow volumes (collision lane, 2026-09-27)

Not in the `.dat` but in the stage `.alr`: each stage model 0 (Palace, Lane, Dawn City, Sein, Panorama, Highway, Panorama Boss, hub areas 02/08) carries one or more **position-only draws with material flag 0x2000** (vertex format 0x10 = position, or 0x07 = position + packed normal). They are the stencil **shadow volumes** ✅: the two formats match the input signatures of `SSHADOWC_vs` / `SSHADOW_vs` exactly, `SSHADOW_vs` extrudes back-facing vertices along the light direction, both pixel shaders output only black + a constant, and the meshes span below the floors (st03 y −58 … 317) with caps that don't sit on visible surfaces (st03: 43 under the hall floor at 50). Survey (`pd_convert walkprobe <Map> stNN --draws`, all 30 stages): st03 draw 1 (2,282 tris), st13 draw 1, st23 draw 9, st04/14 draw 2 (6,374), st05/15 draw 0 (2,312), st07/17 draw 160 (2,586), st08 draws 30–41, st02/12/22 draw 5, st24 draw 3, st09_02 model 2 draws 3–7, st09_08 draws 4–15. The collision leaves them out (COLLISION.md "Shadow volumes are not solid"); the render already skipped them (no UVs). Position-only format 0x10 **without** 0x2000 (flags 0x0003: st06 draws 130–141, st02 draw 1, st04 draw 14, st24 draw 13) is something else ❓ and stays solid, except st02/12/22 draw 1, an oversized outlier excluded by its footprint alone (COLLISION.md "Highway's far floor"). Vertex format 0x0B (position + vertex colour, FORMATS-ALR.md's "SVC", "the sky model's backdrop") is never solid either (COLLISION.md "Panorama Boss's sky floor"): it was already known to be the sky, just not yet excluded from collision.

## Lighting script ✅ layout, ✅ roles and order (corrected 2026-10-01, lane L3)

**Correction (read from the call handlers `0x140206850` / `0x1402068c0` / `0x140206920` / `0x140206960`, STAGE-LIGHT.md §1):** the program stores a call's arguments last-pushed-first. In call `0A`'s table row below, the three floats of the axis and of the direction are each listed (z, y, x): the program's x is the THIRD float; the colours land `a[8]` -> cS0 (hemisphere along the axis, "sun" in the json), `a[7]` -> cS1 ("ground"), `a[6]` -> **cDL_COL, the directional light's colour** ("sky" in the json), not "sky ambient / sun". `stagedat` stores the vectors in the program's order (`"vec_order": "zyx"` in the json). Call `07` is `[range, z, y, x, colour, index, set]`, only indices 0 and 1 are handled, and every arena's set 0 lights have zero colour. Call `02` sub 1's colour is the shadow pass's colour (`SHADOWS.md`). The text below is the 2026-09-27 reading, kept for the record where it is not contradicted.

Every arena `.dat` carries a small SSB byte-code block (the exe names `maplight%d.ssb` and a `pdBattleLight` class; no such files ship, the script lives in the `.dat`). Byte code: `0E 00 <i16>` push small int, `0F 00 <u16 low half> <u32>` push 32-bit (float or colour), `01 <fn> <sub> 20` call. Parser: `stagedat::parse_light_script` (located by its first call `01 0A 00 20` preceded by exactly 10 pushes, because the header offset 0xD0 is -1 in several files). Output: `cache/stNN.light.json`.

| Call | Args | Meaning |
|---|---|---|
| `0A` | 3 floats, 3 floats, 3 colours `0x00RRGGBB`, set | hemisphere axis (≈ up, (z, y, x) in the file), light travel direction (down, (z, y, x)), then the colours: first = **directional**, second = hemisphere "down", third = hemisphere "up" (corrected, see above) — the `SD`/`SBTS` shaders' cS0_VEC / cDL_VEC / cDL_COL / cS1 / cS0 (FORMATS-ALR.md "Stage lighting") |
| `05` | a, b, colour, mode, set | fog: mode = D3DFOGMODE (0 none, 1 exp, 2 exp2, 3 linear); linear a = end, b = start (a > b in every file); exp a = density. Not drawn on PC (no pixel shader reads fog) |
| `07` | 7 values, index 0–2, set | three point lights per set, mostly zero ❓ |
| `02` | value, colour | two calls at the end; first colour dark (st03 (15,18,25)) ❓ backdrop/clear colour; second a 0.7–1.1 factor ❓ |

Four sets (last argument): **0 = stage** (has the fog), **1 = characters** (same directions, brighter sky colour, no fog), **2 = sky** (light pointing up, fog ~13,000 out: only the sky dome is that far), **3 = a neutral default, identical in all files** ⚠️ roles read from the numbers. Why we trust the colours: recolour variants differ exactly as their internal codes say — st03 "Palace-blue" sky (105,120,130) / ground (20,27,30) / sun (125,130,135); st13 "Palace-yellow" (117,127,93) / (40,47,40) / (116,125,80); st23 "Palace-red" (254,204,151) / (90,80,79) / (43,41,17).

Use: `pd_convert model/stages` bakes set 0 into the stage's vertex colours; the game (`pd_game/src/stage_look.rs`) lights characters with set 1 (colour, direction; brightness scaled by `CHAR_LIGHT_REF` ⚠️ tuned by eye). Fog is read but only drawn with `PD_STAGE_FOG=1`. Sets 2/3, point lights and call 02 are unused.

**Confirmed 2026-09-28 against the stage shaders, and it settles two open points.** `SD_ps` (the stage *pixel* shader) reads only `cb0[0]`, `cb0[16]`, `cb0[17]`, `cb0[23]` — no light array, no light loop. `SD_vs` (the *vertex* shader) carries `cDL_COL`/`cDL_VEC`/`cDL_SVEC` (one directional) and `cP0_COL`/`cP0_POS` (one point light) and does the lighting **per vertex**. So: (1) **the stage is lit per-vertex, not per-pixel** — which is why baking set 0 into vertex colours is a faithful reproduction rather than a shortcut, and (2) **the effect-light table (16 slots) does not light stage geometry at all**; it feeds characters only (`SWBOS_ps` reads the same small fixed set). The purple floor wash under a skill is therefore a separate additive floor decal, not stage shading. Full write-up in RESEARCH-LIGHTING.md "A1".

## Object effects (`OE00` / `OE01` / stage `EF00`) — effects lane, 2026-09-27

Code: `crates/pd_formats/src/stage_effects.rs` (parser + tests), `pd_convert stagefx <Map dir> <cache dir>` (`crates/pd_convert/src/effects.rs`), playback `pd_game/src/fx.rs` (object effects) + `effects.rs` (particles), sounds `sfx.rs`. Output: `cache/fx/stage/stNN.objfx.json` and one effect per aux model used, `cache/fx/stage/stNN_sSS.glb/.fx.json/_dN.png` (SS = MD00 slot). Survey scripts established every fact below over all 30 stage files.

**Where:** OB00 word 9 points at the type's `OE00` block, word 10 at an `OE01` block (78 types, mostly trigger 0); header `0x98` at a stage-wide block. Each pointer lands on the `"EF00"` tag itself (not past it). One block per type, blocks back to back (st06 bombeA 0x77F1 + 284 = contaA's 0x790D). ✅

**Block:** `"EF00"`, u16 record count, u16 byte size, then the records. Size = count × 92 in every block of every file (0 mismatches). ✅

**Record (92 bytes):**

| Off | Meaning | Status / evidence |
|---|---|---|
| 0 | u8 **kind** (below) | ✅ 11 values; each kind has its own consistent parameter pattern |
| 1 | u8 **trigger**: 0 = always (jump zones, lamps: ambient), 1 = **on hit** (floors / road plates / contaB: a light shake + a few chips), 2 = **on break** (almost everything: smoke + debris + shake), 3 = scripted collapse (st08 blocks, st01 stairs) | ⚠️ from which objects carry which value and what the records under each do |
| 2, 3 | 0xFF, 0 (a few stage-block records differ) | |
| 4 | f32×3 position, **object space** (left-handed, like the object model; floor sections placed at the origin carry world positions) | ✅ smoke sits on top of drums/tanks, debris at fence ends, contaB's at its four corners |
| 16 | f32×3 rotation, Euler XYZ like placements | mostly 0 |
| 28 | f32×3 scale (smoke puff size 3.73 / 5 / 8.9 / 10) | ⚠️ |
| 40 | 12 kind-specific bytes | |
| 52 | 10 kind-specific f32 (p0..p9) | |

**Kinds** (counts over all files):

| Kind | n | What | Fields |
|---|---|---|---|
| 9 | 1062 | **debris chips**: a burst of the "crs" sprite from **MD00 slot 20 + byte 0** | ✅ slot link: st06 contaA/bombe → `crs00_conta`, fences/rails → `crs01_fence`, tankA → `crs03_tank`, contaB → `crs04_contaB`, st02 rails → `crs01_tesuri`, st03 bridges → `crs02_bridge`, st01 walls → `crs00_wall01/02`, stairs trigger 3 → slot 29 `crs_break_stair`. Byte 2 = life in frames (45/60/70/75/90/120) ⚠️. p1/p2 = sprite size (0.03–0.1), p4 = spread angle (30–150°), p5 = extra up speed, p6 = gravity per frame (−0.01…−0.1), p8 = speed per frame (1–3) ⚠️ — the same numbers as the crs model's own 0x13 block (`0.03 0.03 0 90 0 1.5 0 1 0 −0.05 0 1`, pairs), which the record overrides |
| 10 | 787 | **model**: plays **MD00 slot byte 0** (0–19, the "mdl" models) for p0 frames | ✅ st01 `wallNN` → `mdl_wallNN`, st03 table/bench → `mdl03_table`, pole → `mdl05_pole`, glass → `mdl04_glass`, st04 counter → `mdl03_counter`, floors → `mdl01_yuka`, st05 kanbans → their `crs` signs; byte 0 = 16 → `mdl_particle` (sparks). The mdl models are **fragment sets**: 8–20 draws, one joint each, no timing (st03 table: 12 plank pieces) |
| 3 | 810 | **smoke**: bytes = 0, RGB start, blend (0/1/2), alpha start, alpha end (0), RGB end | ⚠️ colours: dark grey `32 33 32` → lighter `47 4C 45`, oilcan grey→white with blend 2, st06 bombe's second set red `78 01 0F` → near-black with blend 1 (embers). p0 = life s (0.7–5), p5 = spin °/frame (±2, records come in mirrored pairs with p4 0/90 = start angle) |
| 8 | 6 | smoke variant (st06 oilcanA, bigger p2) | ❓ |
| 16 | 168 | **camera shake**: bytes 1/2 = duration / fade frames (30/15 … 150/30), p0/p1 = amplitude, p2/p3 = inner / outer radius (150/500, 50/100) | ⚠️ — contaB's collapse 0.25 over 150–500 units, oilcan 0.1 over 50–100 |
| 11 | 40 | **sound** (`common` bank id = p1, delay frames = p0, byte 0 = 1 looped) | ⚠️ jump zones → 79 (a 2.3 s looping sound: a hum), contaB break → 17 (2.8 s) 52 frames after the break (when the container lands); st08 collapses → 1 ❓ |
| 7 | 8 | jump zones only, all zero | ❓ |
| 5, 13, 18, 31 | 4/3/12/21 | lamp posts (5), stage-block lights (13), st03 bridges (18), st04 fences / st03 pole (31) | ❓ |

**MD00 is a fixed 80-slot table** ✅ (NOUSE = empty; same slots in every stage): 0–15 "mdl" models (fragment sets, ambient emitters: st06 `spark`, `splash`, `flash`, `sea`, `flag`), 14/15 jump emitters, **16 `mdl_particle`**, 20–34 **"crs" debris sprites**, 35 `crs_chakudan` (shell landing), **40/41/42 `mdl_hit_iron/stone/soil`**, 45 `hit_smoke`, 53–55 `hahen_stone0-2` (fragment sets), 70+ extras. Aux model index in the stage ALR = (models − non-NOUSE names) + rank ✅ (st06 base 57: `mdl10_sea` = 63 as found before).

**Stage 0x13 blocks differ from skill effects** ✅: the crs / particle / smoke draws have 0 or 3–30 indices, no UVs (vertex formats 0x1A / 0x18) and a texture: they are **shader-drawn particle emitters** and their 0x13 words are emitter parameters (crs: size, angle, speed, gravity pairs; `mdl_particle` 100 / 90; `hit_smoke` 1000 / 60), not piece timing. Their textures: crs = a debris atlas (chips top-left, sticks top-right; st05 = the sign texture), `mdl_particle` = an orange spark streak, `hit_smoke` = a 1-pixel colour ramp.

**`mdl_hit_iron/stone/soil` are not sounds** ✅: one textured mesh each, no timing, a 34 × 10 × 37 disc with a cratered, scorched texture — **impact crater decals** (for blasts on the floor, by floor material). No object or hull field names a material: hull face `attr` values (0, 0x0B, 0x34, 0x77FC) differ between near-identical types (st04 yukaA 0x34, yukaC 0x0B), so they are not materials ❓. No per-material object hit sound exists in the data; see FORMATS-AUDIO.md "Arena objects".)

**Played (fx.rs, render-only, reads `Combat::props`):** more wear while standing or intact → damaged: trigger 1; → broken, or a thrown object shattering (at its last flight position): triggers 2 + 3. Kind 9 → tumbling textured chips (one quadrant of the atlas each; size p1 × 100 units ⚠️), kind 10 → the fragment model's pieces fly outward and shrink away (45 u/s, gravity 200 u/s², for p0 frames) or, for `mdl_particle`, orange spark streaks, kind 3/8 → soft puffs (a generated round texture; the original's smoke shader isn't reproduced) coloured and blended as the record says. Emitter numbers are per frame, played at 30 fps (FORMATS-EFFECTS.md "Frame rate"). **Not built:** camera shake (kind 16: main.rs owns the cameras), ambient trigger-0 records (jump-zone hum, lamp sparks), crater decals, kinds 5/7/13/18/31. Test: `PD_FX_BREAK_AT=<frame>` plays every nearby object's break records; with `PD_FX_BREAK_TYPE=st03:tableA` one type's, in front of player 1. Proof: `shots/fxlane_breaks1.png`, `fxlane_breaks2.png`, `fxlane_realbreak_sheet.png`.

## Header 0x94: the whole-stage collision mesh, decoded (2026-09-29, wetzone lane)

The "untagged ~150 KB block of OC-style hull chunks ❓ (probably whole-stage collision)" in the header table above is now read. **It is the stage's own simplified collision mesh, and each hull carries the per-surface class byte the footstep code switches on.** Probe: `engine/re/wetzone_probe.py`. Full provenance in COLLISION.md "The runtime footstep class byte". (Note: the surface class is not any face `attr` — face attrs in this block are only {0x0, 0x34}.)

**Layout** ✅ (parses with zero mismatches on all 22 ST00 stages; `bad=0` everywhere):

- A run of hull chunks in the same 12-byte-header shape as `OC00` (`parse_hull`): id 3 = faces (10 f32 centre/radius/min/max, u16 count, 3×u16 pad, then `count × {u16 a,b,c, u16 attr, f32 normal}`), then **a single id-8 vertex pool after ALL face chunks** (not one per hull — `parse_hull`'s per-hull pool assumption needs a small adaptation), then the end marker. st03: 1039 face chunks → 2723 faces → pool of 1921 verts; max face index is exactly 1920 ✅.
- Face-chunk header bytes are `[flags, 3, 3, 0xFF]` (bytes 2–3 constant in every chunk of every file, OC00 included). **Byte 0 is the surface class** ✅ by exact correspondence with the footstep dispatch switch (`FUN_140230cd0`/`FUN_140230d60`): dust {5, 6, 8, 0xB, 0x15}, sand {9}, water {0xD}, silent {4, 7, 0xA, 0x10, 0x12, 0x14}, probe-excluded {0x1F}, plus silent-default {1, 2, 0x16, 0x18–0x1E, 0x20}. Recolour families are identical (st03 = st13 = st23, etc.).
- The id-8 pool chunk's byte 0 (0x21/0x23 by stage family) is a different field's reuse, not a class — vertex chunks are never faces, and the runtime probe requires byte0 == 3. OC00 id-8 chunks similarly carry {22, 33, 35}.
- ⚠️ Two dispatcher cases, sand **3 and 0xC, exist in no stage data** (0x94 or OC00, all 30 files). ❓ Source unknown — possibly story-only geometry or legacy cases. The exact runtime builder (chunk-flags → node +2) was not traced either.
- The end marker (id −1, size 0) carries trailer word `0xCCCCCCCC`, not 0 — the probe stops before checking it, and so does `surface.rs` (established 2026-09-29, surface lane, by direct read of st03 at `0x2F755`).

## Export: `<stage>.surf.json` (2026-09-29, surface lane)

`pd_convert stages` writes the 0x94 mesh plus its per-face classes next to
each `.pdcl` (`pd_formats::surface::SurfaceMesh`, `"format":
"surface-v1"`; coords are right-handed export space, same as the `.pdcl`).
JSON sidecar rather than a `.pdcl` trailer: the two meshes have different
triangle counts, so no per-triangle array could align. Parity with
`wetzone_probe.py` (counts + positions) is 21/21 stages, `bad=0` everywhere;
per-stage numbers and the sim-lane API (`class_below`, `class_kind`) are in
COLLISION.md "The surface class sidecar".


## Object sound groups (2026-10-02, propsound lane) ✅ READ

`FUN_1401fc750(obj, pos, event)` (a virtual method of the ~100 prop classes, vtable slot 0x88) looks up `short[type * 8 + event]` in the 0xF8 table and queues that **group** (`FUN_140201ce0`, 64 entries). The handlers that break an object (the ones that spawn its debris: `FUN_1401e85b0`, `FUN_1401e9550`, ... 16 sites) call it with event 1; event 0 (the "damaged" step, `FUN_1401eaa20` / `FUN_1401eac80`) has no sound in any shipped file. `FUN_140201d30` (once per tick, from `FUN_140202710`) drops the whole queue while a 30-tick cooldown runs; otherwise it plays the first group at once (`FUN_140276330(0, id, id, 0x1000)`, no position), restarts the cooldown, and plays each later entry that is **500 units or more** (`DAT_1402f81b8` = 500.0; the earlier note said "4+") from every position already played, at most five. The id is `DAT_1402dc640[group * 2 + (rand() >> 14 & 1)]`: groups 1-6 -> 8-13 (the other slot -1), 8 -> 14, 9 -> 16, **17-23 -> (17, 172) ... (23, 178), 24-29 -> (179, 180) ... (189, 190), 36-39 -> (191, 192) ... (197, 198)**, the rest silent.

What the shipped stage data uses: only groups 17-29 (slot 1) and 36-38 (slot 2). st06: gas bottle / container C / fan / mac / oilcan B = 17, container A = 22, container B and the heli = 25, oil can A / tank = 23, **every fence and rail = 29**. Groups 1-9 (common 8-16) are **not** reachable from the stage tables; those ids are the surface-hit sounds of a landing / shot / blast on the stage (P0-SOUND.md section 11), a different mechanism.
