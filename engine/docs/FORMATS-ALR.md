# ALR format (models, textures, stages)

Code: `crates/pd_formats/src/alr.rs`, `texture.rs`, `gltf.rs`. Reference (read-only, GPL): `tools/Esper-RE/src/formats/alr.h`, wiki `phantomdust.miraheze.org/wiki/Modding`.

Legend: ✅ confirmed by working output · ⚠️ assumed / partially verified · ❓ unknown.

## Container

- ✅ Sequence of chunks `{u32 id, i32 size (incl. 8-byte header), payload}` starting at offset 0, followed by a **resource buffer** holding raw vertex and texture bytes.
- ✅ Walk: stop at the resource buffer, at `size < 8`, or at two consecutive id-0 chunks.
- ✅ Validated: all 831 `.alr` files in the game parse (`pd_convert scan`).

## Header chunk 0x11 (always first)

| Off | Type | Meaning |
|---|---|---|
| 0x08 | u16 | version (0x52 on PC) |
| 0x0C | u32 | **resource buffer offset** (absolute) |
| 0x10 | u32 | offset table count |
| 0x14 | u32 | resource buffer size |
| 0x20 | i32[] | offset table: animations first (indexed by animation id), then models. Negative = absent. |

Character ALR: 129 animation slots then 1 offset per model (pc00a: 131 = 129 + 2 models).

## Model group

A model is: `0x1 materials` → `0x3 skeleton/transforms` → `0x16 vertex buffer table` → optional `0x13` → one or more `0x2 index buffers` → `0xD` → `0x0`. Our parser starts a new model at every 0x1 chunk.

### 0x1 materials — header 0x0C, entries 0x4C each

| Off | Type | Meaning |
|---|---|---|
| 0x08 (hdr) | u16 | entry count |
| +0x10 | u8 | vertex format of buffers using it |
| +0x18 | u16 | **texture index** (into the file's 0x15 table) |
| +0x1A | u16 | normal map (or lightmap for format 0x1F) ❓ unused by us |

### 0x3 skeleton — header 0x10, joints 0x40 each

| Off | Type | Meaning |
|---|---|---|
| 0x08 (hdr) | u16 | joint count; 0x0A u16 has_joints |
| +0x00 | f32×3 | local position |
| +0x0C | f32×3 | local Euler rotation (radians) — convention in FORMATS-ANIMATION.md |
| +0x18 | f32×3 | local scale (0 treated as 1) |
| +0x24 | u16 | flags. Program-read bits ✅ (PROGRAM-RE.md "Character parts / cloth"): **0x800 = don't draw draws on this joint** (set at run time only), **0x400 = hidden at spawn** (the program sets 0x800 on these: the shadow-helper joints), 0x100 = cloth / attach joint (CLP, CLC, CL.., COL..), 0x200 = billboard, 0x04 = helper (swapped with 0x400 in a special pass). Low byte otherwise ❓ (0x13/0x33 deforming bones, 0x10/0x30 named leaves) |
| +0x26 | u16 | the joint's own index (animation tracks are matched against it) ✅ |
| +0x2A | i16 | parent index (−1 = root) |
| +0x2C | u32 | **name, a packed string** ✅: base-40 digits over " 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_", most significant first (0x70320580 = HEAD, 0x52CD4A68 = CLP00; `alr::joint_name`). 0xFFFFFFFF = unnamed |
| +0x30 | f32 × 3 | cloth node parameters (CL.. joints): velocity kept per tick, angle limit in degrees, unused; all 0 = 0.9 / 60°. COLF/COLP collision joints: [capsule radius (the program multiplies it by 5) ⚠️, bit mask of the cloth parts it pushes] |

On stages the skeleton is just a transform hierarchy; each draw is placed at `world(joint[transform_idx])`. ✅ (stage renders assembled correctly)

### 0x16 vertex buffer table — header 0x0C, entries 0x1C

| Off | Type | Meaning |
|---|---|---|
| +0x00 | u8 | vertex format id |
| +0x01 | u8 | stride (bytes) — use this over the table below when non-zero |
| +0x08 | u32 | vertex count |
| +0x14 | u32 | data offset in resource buffer |

Vertex formats (position is always f32×3 at 0). UV = 2×s16 at offset, divided by divisor:

| id | stride | UV off / divisor | notes |
|---|---|---|---|
| 0x01 | 0x18 | 16 / 4096 | common stage |
| 0x03 | 0x20 | 16 / 4096 | common stage |
| 0x05 | 0x1C | 16 / 32767 | |
| 0x07, 0x0B, 0x18 | 0x10 | — | **position only**: collision/shadow/helper; not rendered ✅ (were the "white walls") |
| 0x08 | 0x14 | — | position only (suspected collision) |
| 0x09 | 0x14 | 12 / 4096 | |
| 0x10 | 0x0C | — | position only |
| 0x11 | 0x18 | 12 / 32767 | **SWBOS skinned** — see FORMATS-ANIMATION.md |
| 0x15 | 0x18 | 12 / 32767 | |
| 0x16 | 0x1C | 12 / 4096 | |
| 0x17, 0x1E, 0x21, 0x25 | 0x20/0x1C | 16 / 4096 | |
| 0x1A | 0x28 | 16 / 4096 | |
| 0x1D | 0x24 | 16 / 4096 | |
| 0x1F | 0x20 | 16 / 4096 (+ lightmap UV at 28) | LBTS baked lighting (underground stages) |

Normals and vertex colours (✅ 2026-09-27, see "Stage lighting" below): formats 0x01 and 0x03 carry a packed normal at +12 and a colour at +20; 0x03 adds tangent/binormal at +24/+28 (⚠️ from the SBTS input signature, not decoded); 0x07 = position + packed normal (untextured, not drawn by us); 0x09 = position + uv at 12 + colour at 16; 0x0B = position + colour at 12 (untextured, vertex-coloured: the sky model's backdrop). Other larger formats' extra bytes ❓.

### 0x2 index buffer — header 0x68, then u16 indices

| Off | Type | Meaning |
|---|---|---|
| 0x08 | f32×3 | centre; 0x18/0x24 AABB min/max |
| 0x40 | u16 | **material index** (into this model's 0x1 table) |
| 0x42 | u16 | **transform joint** |
| 0x44 | u16 | vertex buffer index (into this model's 0x16 table) |
| 0x46 | u16 | primitive: 5 = triangle list, 6 = triangle strip (degenerate triangles separate strips; odd triangles have flipped winding) |
| 0x50 | u32 | index count |
| after indices | — | **skin palette** (skinned draws): pad to 4, u32 0, u32 count, u8[count] joints, pad, count×identity 4×4. ✅ See FORMATS-ANIMATION.md |

## 0x15 textures — header 0x0C, entries 0x1C

| Off | Type | Meaning |
|---|---|---|
| +0x04 | u32 | data offset in resource buffer |
| +0x0C | u8 | bit 2 = cubemap |
| +0x0D | u8 | low 5 bits = pixel format |
| +0x0E | u8 | low 4 = mip count, high 4 = log2 width |
| +0x0F | u8 | low 4 = log2 height |
| +0x10 | u32 | bits 0–11 width−1, 12–23 height−1 (used when either non-zero) |

Pixel formats: 0x0C DXT1, 0x0E DXT3, 0x0F DXT5, 0x06/0x12 BGRA8, 0x05 BGR565, 0x02 BGRA5551, 0x04 BGRA4444, 0x00/0x13 R8, 0x1F A8. ✅ All used stage textures decode with plausible colours (`pd_convert texprobe`); no swizzling on PC. We decode the top mip only.

Many textures are unused by any draw and have mean colour ≈ (127,127,255): **normal maps** ⚠️. Some have blue-purple mean colours and are sampled by effect shaders ❓.

## Other chunk ids

`0x5` animation (FORMATS-ANIMATION.md), `0x7` camera animation (see below — **read 2026-09-28**), `0x9` dynamic lights (see below — **read 2026-09-28**), `0x10` texture atlas (menus/UI), `0x12` per-stage cull volumes (see FORMATS-STAGES.md — **read 2026-09-28**), `0x13` **a per-model block, still unread** (see below), `0x14` **one instance in the whole game** (see below).

### `0x13` — per-model block between the vertex-buffer table and the index buffers ❓

**Structurally known, semantically not.** `alr.rs`'s module doc places it in the model group: `0x1` materials → `0x3` skeleton → `0x16` vertex buffers → **optional `0x13`** → `0x2` index buffers. There is **no `ID_*` constant for it** in `alr.rs` (unlike `0x1/0x2/0x3/0x11/0x15/0x16`), so the parser walks past it rather than reading it — ⚠️ which is why it has never been examined. **1,789 instances** across 28 stage files, **always a fixed 136-byte body** (128-byte payload + 8-byte header), so the record size is certain and the per-file counts are known (st08 763, st00 126, st06 74, st25 75, …; some stages have none).

Its handler is **`FUN_140295d90`** (row `0x13` of the table at `0x140410608`; second pass `0x140004660` = none). That handler is **not** a transform loader — it does a **class lookup and a virtual call**: it indexes a **256-entry table** at `DAT_1405762e0` (stride 24 bytes) comparing **two u32 keys** read from the record against a live object's ids, and on a match calls a virtual at vtable `+0x18` (first pass) or `+0x10` (second). It also threads a **u16 index × 0x4C** into `DAT_140574aa8` — the **0x4C stride is the material entry size** (FORMATS-ALR "0x1 materials"), so **the record names a material**. ⚠️ That is the readable part; what the looked-up class *is* (it appears to be a per-model material/effect binding, given it sits between the vertex buffers and the index buffers) is not settled, and the record's leading three floats — which look like a position or a scale triple, values like `100, 430, 350 / 355 / 360` — are unexplained. **Next step, if wanted: read the 256-entry table's population and the `+0x18` virtual.** It matters more than its size suggests: this is the one block sitting between a model's geometry and its indices, so it is the most likely place a model's *material assignment* lives.

### `0x14` — a sibling of `0x13`, and it occurs **exactly once in the entire game** (2026-09-28)

Was "effect/shader bindings ❓". Now read, and the finding is mostly about how rare it is.

**It is `0x13`'s twin.** Handler **`FUN_140295ef0`** (row `0x14`, second pass `FUN_140293a60`) and it does the *same three things* `0x13`'s handler does, in a shorter form:

```
local_68 = (u16 at +0x1a + 1) * 0x40 + DAT_14057f378   // per-object transform row
local_58 = DAT_140574aa8 + 0xc + (u16 at +0x18) * 0x4c  // MATERIAL entry (0x4c stride)
local_48 = *(int at +0x28) + self + 0x28                // payload pointer
vfunc    = DAT_140582980[*(u32 at +0x24)]->vtable[+0x20]  // the class's +0x20 method
call     vfunc(obj, local_68)
```

✅ Same **0x4C material stride** as `0x13`, same **global transform table** (`DAT_14057f378`, indexed `+1` so index 0 is reserved), same **class-dispatch array** (`DAT_140582980`) — the only difference is **which virtual it calls: `+0x20` here, `+0x10`/`+0x18` in `0x13`.** So `0x13` and `0x14` are two operations on the same kind of object, distinguished by the method they invoke. ⚠️ Which means the earlier guess "effect/shader bindings" for `0x14` was **not supported** — the shape is a *material-scoped object call*, same family as `0x13`, and neither is provably an effect.

**The rarity is the real finding.** Walking all 30 stage files: `0x14` appears **once**, in `st08.alr` at file offset `0xd46ac`, 44 bytes (36-byte body). Its body is almost entirely zero — the only non-zero dwords are `+0x08` and `+0x0C`, which as floats are **−18,884.4 and −0.0000**, i.e. a **position far outside the stage**, and a `+0x10` field reading `0x00010000`. ⚠️ Read as a position, that is roughly 18.9 km away on Z — **either a deliberately out-of-range marker, a sentinel/"nothing here" record, or a value in different units.** A single instance of a 0x13-shaped block, with a wild position and a `0x00010000` flag, reads much more like a **placeholder or an authoring leftover** than a used feature. ❓ Not resolved, and **not worth resolving** — with one instance in the whole game there is nothing to reproduce. Recording it so the id stops being an open ❓.

### `0x9` — the stage's own light records ✅ read 2026-09-28, but **unused by every shipped stage**

The chunk-type dispatch tables (`0x140410608` for stages, `0x140410778` for characters, 16 bytes per row) put `FUN_1402946f0` at **row 9**. It is the only handler that writes the renderer's light table, and it is a **different producer from the effect-light path**: it fills the same globals (`0x1405a6a40` + the flag/param arrays) straight from a chunk. So stages *can* author lights.

**Record layout (0x28 = 40 bytes each), read from `FUN_1402946f0`:** count at +8, then per light at +0x0C: `+0x0C` colour dword (ARGB → written to the record's diffuse triple × 1/255), `+0x10` u8 **type** (1 = a simple/diffuse-only light, 2–4 = flagged variants, 5 = another variant; anything else writes 0 = off), `+0x11` u8 **slot index** (used only when bit 1 of `+0x12` is clear — otherwise the light takes the next free slot from the running counter), `+0x12` u8 **flags** (bit 1 = explicit slot, bit 2 = …, bit 3 (0x08) = …, bit 4 (0x10) = …), then three float4-ish groups at `+0x14`/`+0x18`/`+0x1C` (position/direction), `+0x20` (range — written straight into the record's range field), `+0x24`/`+0x28`/`+0x2C` (a second colour set), `+0x30` (a scalar, forced to 1.0 for types 2–4). The diffuse/specular pair is filled from the same colour dword, so a stage light is a plain coloured point light with an explicit type and flags.

**But it does not fire.** Walking every container chunk in **all 30 `Map/stNN.alr` files** (chunk header = u32 id + i32 size, walked to the resource-buffer offset) gives the id histogram — **re-walked 2026-09-28 night, superseding the earlier version of this line which had the ids in decimal and one of them wrong**:

| id | count | body size | meaning |
|---|---|---|---|
| `0x00` | 1,888 | 8 (fixed) | chunk terminator/padding |
| `0x01` | 1,866 | 12…3,508 | materials |
| `0x02` | 18,877 | 104…47,488 | index buffers |
| `0x03` | 1,866 | 16…52,496 | skeleton/transforms |
| `0x0D` | 1,866 | 12 (fixed) | per-model tail |
| `0x11` | 30 | 48…612 | header (one per file) |
| **`0x12`** | **250** | **44 (fixed)** | **cull volumes — st08 (54) + st09 (196) only** |
| **`0x13`** | **1,789** | **136 (fixed)** | **per-model block, 28 files — see above** |
| **`0x14`** | **1** | **44** | **st08 only — see above** |
| `0x15` | 30 | 236…1,860 | textures |
| `0x16` | 1,888 | 12…208 | vertex buffer table |
| `0x07` / `0x0A` | 65 / 65 | — | camera tracks (`stNNcam.alr` only) |

**id `0x09` never appears in any of the 30 files** ✅. So the feature exists in the engine and the data format supports it, but **no shipped versus stage uses it** ✅. That is consistent with A1's correction: the original's stages are lit by the light *script* (FORMATS-STAGES "Lighting script") plus the per-vertex shader path, not by per-mesh light records. ⚠️ The `0x07`/`0x0A` rows come from the `stNNcam.alr` files, which are excluded from the 30 stage files above; every other id is from the stage files proper. `0x0D` (1,866, fixed 12-byte body) is a normal per-model tail and is not one of the open questions.

**What it means for us:** if we ever want a practical light in a stage (a flickering fluorescent, a fire), this chunk is the mechanism the game provides and it is already understood ⚠️ — but our converter would need to read it to round-trip a stage faithfully. **Nothing is lost by ignoring it today.**

### `0x7` — camera animation, and a whole unconverted asset family ⚠️

Chunk id 0x07 appears **65 times across 8 files** — and the files are `st01cam.alr` … `st08cam.alr`, **228 KB total, entirely unconverted**. `pd_convert model/stages` explicitly *skips* them (main.rs:1125, "skip the `stNNcam.alr` camera files"), which is why they never surfaced. **The file structure is a strict repetition** — `0x0a`, `0x00`, `0x07`, `0x00` per entry, and the `0x0a`/`0x07` pairs are 1:1 across all 8 files (65 of each) ✅.

**What is established** (all 65 chunks, read off the raw bytes):

- **The `0x0a` chunk (48 bytes) is the track's camera setup**, and it is the clearest part. It holds a **float3 position** at +0x08, three further floats at +0x14/+0x18/+0x1C, and zeros at +0x24. ✅
- **The last of those three (+0x1C) is a field of view, in radians** ✅ — across the 65 chunks it ranges **0.846 … 1.752 rad = 48.5° … 100.4°**, and its distinct values are all author-friendly round degrees (0.95, 1.0, 1.0808, 1.1427, 1.248, 1.4656, 1.5708 = 90°, …). That is a hand-authored FOV, not a derived value.
- **The `0x07` chunk is the keyframe track** and it is a **camera dolly/turn path with a continuous FOV ramp**: reading its float stream as 16-byte records gives, per key, a float3 plus one integer-ish field, where the float3's first component rises smoothly (92.49 → 92.74 → 93.25 → 93.50 …) and the record *before* it descends just as smoothly (154.57 → 153.88 → 152.50 → 151.81 …, a near-constant −0.7/frame step). That is a camera pushing in on a target **while the FOV narrows** — a dolly-zoom, authored per key ✅.
- Chunk sizes run 208 B to 11.4 KB, and the "count" fields at +0x10 range 21 … 300.

❓ **Not solved, and deliberately not guessed** — the keyframe **stride**. Three candidate readings were tried and all three fail to make the count divide the body: `+0x10` matches `body/16` in **0 of 65** chunks, `body/20` in 0, `body/12` in 0, and only stride **8** divides every body evenly, which contradicts the visible 16-byte float pattern. The `+0x10` and `+0x14` fields are *not* equal in 64 of 65 chunks either. So either the record is variable-length, or `+0x10` is not a record count (it may be a **duration in frames** — its values, 21…300, read exactly like frame counts for a 1–10 s move at 30 fps, which is our game's confirmed rate). ⚠️ **The safest reading, and the one to use: `+0x10` is most likely a frame duration, and the key count is implicit.** Settling it needs the program's camera-track reader (`0x0a` and `0x07` share a handler in the chunk table at `0x140410608` — row 10 is `0x140293d70` / `0x1402949e0`) and is a short decompile.

**Worth knowing, not worth building now:** if a "camera flies through the arena before the round" beat is ever wanted, the data is already there and roughly half-read — position and FOV are solved, the key spacing is not. That is a converter job, not a research project. ⚠️ It is **not** in scope today (no cutscenes per AGENTS.md; round intros are a different question and would be Laurence's call).

## UI files

`Menu/*.alr` hold menu/HUD art. `Menu/BtMenuEn.alr` (battle HUD) has 3 DXT5 textures and **no models**, so screen layout is not in the file (details and atlas pieces: HUD.md). Established 2026-09-26 with `dumptex` + `sprites`.

## Stage names (`setup/stageset/stageset_en.dat`)

Not an ALR file, but the authoritative source for stage display names — see `pd_formats::stageset` (doc comment has the full byte layout) and PIPELINE.md "Stages" for the evidence and the resulting name/family/versus table. Short version: ✅ a stage's slot position in this table (0-based) is its `stNN` id; each slot has long name / short name (recolour family) / internal code as three consecutive NUL-terminated strings.

## Stage lighting (vertex normals, colours, the stage shaders) ✅ 2026-09-27

Leads: the modding wiki ALR page (stage slots, baked-lighting note); verified here against the files and the game's shaders.

- **Shader inputs** (`tools/disasm/disasm.exe game_files/<name>_vs.cso`): `SD` = position, NORMAL, COLOR, TEXCOORD (= format 0x01, stride 0x18); `SBTS` = the same + TANGENT + BINORMAL (= 0x03, stride 0x20); `SC` = position, COLOR, TEXCOORD (= 0x09); `SVC` = position, COLOR (= 0x0B). Offsets in `alr::light_layout`.
- **Packed normal** (u32): x = bits 0–10, y = 11–21 (11-bit signed, /1023), z = 22–31 (10-bit signed, /511). `0x001FF800` = +Y, `0x7FC00000` = +Z. Check: 94 % of st03's first 7,387 triangles have their face normal within 25° of vertex 0's decoded normal (the rest are smooth-shaded) — `alr::unpack_normal`. Game (left-handed) model space; rotate by the draw's joint world transform.
- **Vertex colour** = D3DCOLOR, bytes B, G, R, A. Evidence: the sky backdrop of st23 ("Palace-red") reads `(13,1,1)` dark red only in BGRA order; st03's ("Palace-blue") reads `(76,80,102)` blue-grey. Stage colours are mostly white with 20–60 % greys under balconies; the alpha byte is *not* opacity on stage geometry (15,000 of st03's format-0x03 vertices, ordinary walls and floors, have alpha 0) — the SD shader uses it for specular. On the sky dome alpha fades the rim to 0.
- **SD lighting** (vertex shader, per vertex): `ambient = lerp(cS1_COLv, cS0_COLv, t)`, `t = dot(N, cS0_VEC.xyz) + cS0_VEC.w` (hemisphere); `diffuse = max(0, N·−cDL_VEC) × cDL_COL` (+ a point light cP0); both × vertex colour; pixel = `saturate(tex × diffuse + tex × ambient) + specular`. The constants come from the stage's light script (FORMATS-STAGES.md "Lighting script"). ⚠️ we assume `cS0_VEC = 0.5·axis, w = 0.5` (t runs 0..1 from ground to sky).
- **Fog**: every stage shader outputs a FOG value but no stage *pixel* shader reads it (the "Used" column is blank in SD, SBTS, SVC, SDYFOG, SBTSYFOG, SBTSCLOUD2): the PC version draws no distance fog. ✅
- **Stage model slots** (wiki: 1st model = stage, 2nd = background, 3rd = skybox) ✅ on st03/st06/st13/st23: slot 2 = a ~11,700-unit cloud dome (format 0x03, texture with alpha, vertex alpha 0 at the rim) + a 12,400-unit closed backdrop (format 0x0B vertex colours, or 0x09 textured in st13/st06: dark at the bottom, sky colour at the top) + an inner ~6,100 × 2,000 ellipsoid with material flag 0x10 whose blend is ❓ (we leave it out: drawn opaque it hides the sky). This backdrop was our "black void".
- **Material entry fields used by the stage shaders** (lane L3): +0x34..+0x36 specular colour B, G, R and +0x37 power (read when flag 0x04 is set, `FUN_1402a75e0`); the signed short at +0x40 is the stage vertex shader's specular fade constant `k = short * 0.01 + 1` (`cEYEVEC.w`); the float at +0x3c is a shadow-volume draw's extrusion length (`FUN_1402a3bd0`). Format 0x03 + flag 0x20 draws are `SBTS` (per-pixel bump), see STAGE-LIGHT.md.
- **Material flags** (u16 at entry +0x0C, `MaterialEntry::flags`) ⚠️ from which materials carry them: 0x01 always, 0x08 textured, 0x04 specular (spec colour + power at +0x34, e.g. `7f7f7f 14`), 0x20 bump (format 0x03), 0x10 an extra layer (second texture = small blue env/reflection texture on stage materials; the inner sky shell).
- **Export** (`gltf::ExportOptions::lighting`): the stage set's light is baked per vertex into `COLOR_0` (stored as light^2.2 so Bevy's linear maths reproduces the game's gamma-space `texture × light`); materials stay unlit. Objects placed from the `.dat` get their own colours (lit where they stand); the props library glb is lit as if upright. Colour accessors are shared per (vertex buffer, rotation) — one per draw made 300–800 MB glbs that loaded too slowly (st07 showed only black at the first screenshot). st08's sky is near-black on purpose: its backdrop's vertex colours are (25,0,0) / (12,0,0) with a bright (255,224,127) cap at the zenith, and `SVC_ps` outputs the vertex colour as is; the black masses that covered the arena were **not** that void: they were the MD00 aux glow models drawn opaque (FORMATS-STAGES.md "Aux models are not scenery", fixed 2026-10-01). No normal maps, specular or shadows yet.

## Characters: one-sided faces ✅ (2026-09-27, jacket lane 2)

Character models are built for back-face culling: inner linings are separate faces lying exactly on the outer shell with the **opposite winding** and a near-zero UV area (they sample one flat colour — on pc00a a dark olive at texel ≈ (73, 265)). pc00a's lower jacket (model 1): 178 of 202 textured faces point outward, 90 of 104 lining faces point inward; body draws likewise. Drawn double-sided, the lining shows through the jacket as large flat olive triangles beside the harness (Laurence 2026-09-27: "a missing texture on the sides"). All character SWBOS materials carry the same flags (0x82D), so there is no per-material two-sided switch. Evidence sheet `shots/jk2_sheet_sides.png`: A both sides (the bug), **B back faces hidden (matches the original)**, C front faces hidden (shows only the lining — confirms the winding). The character export (`gltf::Materials::get_lit`) is therefore single-sided; `PD_CHAR_DOUBLE_SIDED=1` / `PD_CHAR_FLIP_WINDING=1` (converter) reproduce A / C. Checked pc01a, pc02a, pc03a, pc08a, pc10a, enm01a front and back: no holes (`shots/jk2_others_sides.png`). The program's rasterizer state itself was not located (the port draws through a wrapper) ⚠️.

## Characters: costume-colour texture ✅ (2026-09-27, altcolour lane)

Confirmed which texture the four costume-colour siblings (`Player/<id>/<id>0..3.alr`, PIPELINE.md) actually swap, and that it's the same one on every playable character. `pd_convert texprobe <id>0.alr` vs `<id>1.alr`/`<id>2.alr`/`<id>3.alr` on all 11 playable characters (`pc00a`..`pc10a`) shows exactly one texture entry's mean colour changing per colour (same format/size/resource pointer as colour 0) — every other texture (each character has exactly two) is byte-identical across colours. That texture is always the one with by far the most drawn vertex indices pointing at it via `MaterialEntry::texture_idx` (a 512x512 Dxt1, ~9,000+ of the ~9,000-9,600 total indices on every character checked — essentially every draw); the other texture (a 256x256 Bgra5551) has 0 uses as a `texture_idx` on any character checked, so it's never the base colour — it's an object-space normal map, referenced only via `MaterialEntry::normal_idx` (unaffected by colour). `pd_formats::character::base_colour_texture_index` finds this by the same "most drawn indices" rule rather than hardcoding index 0, in case a character ever differs (none checked do). Decoded PNGs of all four colours per character (`shots/alt_colour_sheet.png`) show real costume recolours, not a uniform hue shift — e.g. pc00a's jacket: green (c0) / maroon (c1) / black (c2) / silver (c3).

## Known gaps

- ~~Sky~~: model slot 2 (see "Stage lighting"); drawn when lighting is baked. The inner flag-0x10 shell's blend ❓.
- ~~Stage props~~ done: object models live in the stage ALR itself (model slots 8+, referenced by the `.dat`), placed by `stNN.dat` — see FORMATS-STAGES.md. Stage ALR model order: slots 0–7 stage, then object models (intact/broken/damaged), then the `.dat`'s `MD00` aux models (sea, flags, hit effects). ✅
- Materials: exported unlit (stages with baked vertex lighting), alpha-mask if the texture has alpha. Normal maps, specular, stage shadows and additive/effect shaders (`vkBLINK` etc.) are not reproduced.
