# Collision

Code: `crates/pd_formats/src/collision.rs` (engine-agnostic, used by the sim), export in `gltf::world_triangles` (filter: `gltf::collision_keeps_draw`), CLI `pd_convert collision`, walk probe `pd_convert walkprobe` (`crates/pd_convert/src/walkprobe.rs`).

## Data

- Built from every stage triangle in right-handed world space, including position-only geometry, **except** the object template models (they used to sit at the origin) and the stage's **shadow volumes** (below; since 2026-09-27) — plus, for every visible object the `.dat` places, the game's own collision hull for it (`OC00`, mostly boxes; model triangles if a type has no hull), transformed like the render. Jump zones and other invisible triggers are not solid (they are in `props.json`). FORMATS-STAGES.md.
- Check: `pd_convert propcheck <stNN.alr> cache` — st06 203/265, st04 129/140, st03 175/176 objects push a player-sized cylinder touching their side out (the rest sit against stage walls that already block, or are thin rails); box tops are standable (st06 180/265).
- File `cache/<stage>.pdcl`: `"PDCL"`, u32 triangle count, then 9 × f32 per triangle. Optional trailer (since 2026-09-27, written by `pd_convert stages/collision`): `"OBJT"` then one u32 per triangle = the object (placement index in `props.json`) it belongs to, `0xFFFFFFFF` = the stage. The game switches a broken or thrown object's triangles off (`Collision::set_object_gone`; GAME.md "Arena objects"). Files without the trailer still load (no objects). A second, later trailer `CLAS` (one class byte per triangle, 2026-10-03) follows `WIND`: "Per-triangle class" below.
- On load, triangles split into **floors** (|normal.y| ≥ 0.6, either winding; signed 0.643 for the stage part of a `WIND` file, see "The stage collision is the 0x94 block") and **walls**, each bucketed into a 32-unit XZ grid. Triangles spanning > 8192 units (sky, sea) are dropped from the grid.

## Shadow volumes are not solid (2026-09-27)

Laurence could not leave his start area in Lost Palace (st03). Cause: the stage's stencil **shadow-volume** mesh was in the collision as walls and floors.

- **Reproduced** with `pd_convert walkprobe <Map dir> st03`: from P1's start (171.5, 50, 271.2) every compass direction stopped within 18–162 units; the flood fill reached 7,488 square units and none of the other three start points. The blocking triangles (south wall at z 105, west wall at x 152) all came from **model 0 draw 1**: position-only (vertex format 0x10), material flags 0x200b, 2,282 triangles spanning the whole stage (y −58 … 317). P1's alcove is closed on the other three sides by ordinary visible walls; its only opening (south, into the hall) was sealed by draw 1.
- **What it is** ✅: format 0x10 (position only, stride 12) is exactly the input of the game's `SSHADOWC_vs`; format 0x07 (position + packed normal) is exactly `SSHADOW_vs`, which pushes back-facing vertices along the light direction (`cDL_VEC`) — classic stencil shadow-volume extrusion. Both pixel shaders write only black plus a constant (no texture, no colour). Every draw with these formats and material flag **0x2000** is one big shadow mesh per stage (st02/12/22, st03/13/23, st04/14, st05/15, st07/17, st08, st09_02, st09_08, st24); its "floors" sit below or above the visible ones (st03: 43 under a floor at 50) and its boxes run through visible walls.
- **Rule** (`gltf::is_shadow_volume`): vertex format 0x07 or 0x10 **and** material flag 0x2000 → not solid. Position-only format 0x10 with flags 0x0003 (st06/16/26 draws 130–141, st02 draw 1, st04 draw 14, st24 draw 13) stays solid, except Highway's one oversized draw (below). Format 0x08 ("suspected collision") stays solid.
- **Effect** (`walkprobe --compare`: flood fill from all four start points with and without the filter; "new holes" = spots reachable both ways where only the new collision lets a player walk off into the void):

| Stage | Start pairs connected on foot, before → after | New holes |
|---|---|---|
| st03 / st13 / st23 Palace | 2/12 → 12/12 (reachable area ×50) | 5, all on top of the giant tree (y 150–158) |
| st07 / st17 Lane | 2/12 → 12/12 | 0 |
| st04 / st14 Panorama | 12/12 → 12/12 | 0 |
| st08 Sein | 0/12 → 0/12 (floating islands: jump zones) | 22; central ring and bridges now reachable |
| st05 / st15 Dawn City | 3/12 → 1/12 | 510: before, players walked on the shadow mesh across the gaps between rooftops; after, those gaps are open to the void (jump zones cross them) |
| st02 / st12 / st22, st24 | unchanged | 0 |
| st01, st06 / st16 / st26, st25 | file identical | — |

## Highway's far floor and Panorama Boss's sky floor (2026-09-27, collision lane)

Two separate bugs, both "solid geometry that was never meant to be a floor", found and fixed with `pd_convert walkprobe --histo` (new: bins a draw's surviving triangles on a 200-unit XZ grid, printing tri count, wall/floor split and y-range per bin — tells a real floor patch from a disjoint outlier) plus `--at`.

**Highway's far floor** — `pd_convert walkprobe <Map> st02`: the flood fill from some start points covered 22.4M square units (the probe's 2,500-unit search radius, i.e. unbounded within it) instead of the ~380k a road this size should have. `--only 0:1 --histo` shows why: model 0 draw 1 (position-only, vertex format 0x10, material flags 0x0003, 420 triangles) is a single mesh whose footprint (x −1,265…1,858, z −1,841…1,142: 3,123 × 2,983 units) is 2–3× the whole visible stage's own bounding box (x −607…731, z −645…839: 1,338 × 1,484) and runs continuously from the road's height (up to y 280, matching the visible geometry directly above it) down to y −191 far outside the road, with no gap. It's the odd one out: every *other* position-only, flags-0x0003 draw on any of the 30 stages (Refinery's edge-closers, Panorama's draw 14, Panorama Boss's draw 13) is a small patch sized like the wall next to it (largest 284 × 262 units) — this is the only one that's an entire second, much bigger ground plane. Dropping it entirely (`--drop 0:1`) bounds the reachable area to 377,632 sq. units with no change anywhere else; two of Highway's four start points (0 and 2) then land on separate un-connected road segments, which the `--map` picture shows are two different ramps of the highway interchange (an "X" of crossing roads) — not something walking should bridge anyway, and true whether or not draw 1 is solid (removing it only removes the accidental bridge between ramps, never a real path — the direct 8-way walk from those starts already fell off the same edges with draw 1 present). **Fix**: `gltf::is_oversized_floor_patch` — a position-only (0x10), flags-0x0003 draw is not solid if its world-space XZ footprint exceeds 1,000 units in either axis (`SLAB_LIMIT`; the next-largest legitimate draw is 284 units, an 11× margin). Applies to st02/12/22 draw 1 only, confirmed by re-running `--draws` across all 30 stages.

**Panorama Boss's sky floor** — `pd_convert walkprobe <Map> st24 --start 1`: every compass direction reported "open" after walking the full duration, landing at y ≈ −2,000 — nowhere near the visible arena (y 50–230). `--at 109,-181` (start 1's position) found no stage or object geometry there at all, only **model 2 draw 0**: position-only, vertex format 0x0B ("SVC": position + vertex colour, no UVs), a sphere/shell from y −2,010 to 4,477 spanning ±13,500 units. FORMATS-ALR.md already documents format 0x0B as "the sky model's backdrop" (established from the `SVC` shader's input signature); it was never excluded from collision. Two of st24's four start points (1 and 3) sit over a gap in the visible arena (outside the boss stage's model-0 footprint, x ±175 / z −48…347) with nothing below but this sky shell, so a fall there landed on it instead of respawning. Surveyed the same format across every stage we could reach it on (st01, 03–08, 12, 14–17, 24): always a single huge (2,400 to 27,000-unit) dome/shell, never small helper geometry. **Fix**: `gltf::is_sky_backdrop` — vertex format 0x0B is never solid. After the fix, start 1 and start 3 fall straight into the void (respawn) as intended; every other stage's collision is byte-identical.

Both fixes are narrow (format 0x10 needs an oversized footprint too; format 0x0B is excluded outright only because every observed instance is the sky, never a helper). Verified with a full before/after `walkprobe --start 0/1/2/3` sweep of all 30 stages (`stagedat` probe list): identical reachable area, blocked-walk count and void-edge count on every stage except st02/12/22 (all 4 starts: 22.4M/22.4M/107k/22.5M → 378k/376k/418k/375k sq units) and st24 (starts 1 and 3: 6.4M/5.6M → 16 sq units, i.e. immediate fall). Screenshots: `shots/hw_st02_before_map.png` / `hw_st02_after_map.png` (and st12/st22 the same, sharing st02's mesh), `shots/hw_st24_before_map.png` / `hw_st24_after_map.png` (start 0, unaffected — the bug is on starts 1/3), `shots/hw_st24_start1_before_map.png` / `hw_st24_start1_after_map.png` (the sky-floor fix, start 1).

## Walk probe

`pd_convert walkprobe <Map dir> <stNN> [--start K | --from x,y,z] [--dirs N] [--secs S] [--map out.png] [--quiet]` builds the collision exactly as `stages` does and walks a player body (the sim's movement: run 72/s, step 6, body radius 4.5 / height 20, gravity, coyote ticks) out of each start point in N directions, printing where each walk stops and the triangles that stopped it (model, draw, vertex format, material; or the object). Then a flood fill: area reachable on foot, which other start points are reachable, "void edges" (walking off into no floor or a drop of 150+). `--map` writes a top-down picture (reachable green, walls at body height red, void edges magenta, starts white). Research switches: `--all-draws` (no shadow filter), `--drop m:d,…`, `--only m:d,…`, `--rule flag2000|fmt`, `--draws` (list every draw), `--at x,z` (every surface over a point), `--voids`, `--compare`, `--histo` (bins the surviving triangles — after `--only`/`--drop` — on a 200-unit XZ grid, printing tri count, wall/floor split and y-range per bin; tells a real floor patch from a disjoint outlier, used to find Highway's far floor).

## Queries

| Function | Use |
|---|---|
| `ground_below(x, z, max_y)` | highest floor at or below `max_y` → standing, stepping, falling |
| `layers_at(x, z)` | all floor heights at a point (diagnostics, spawn picking) |
| `push_out_of_walls(pos, radius, bottom, top)` | cylinder vs wall triangles, horizontal push, 3 relaxation passes |
| `raycast(from, dir, max)` | first hit vs floors + walls (camera occlusion, line of sight) |

## Stage st06 facts

- Sea plane at y ≈ 0 (and another surface around y ≈ 30); **decks at y ≈ 230–400**; main open deck ≈ 270. Big layers at ±2000 / ±3600 are sky/sea shells.
- Spawns (`pd_game::pick_spawns`): bucket top floors > 50 by 10-unit height, take the most common height, then choose two open points 100–160 apart at the same height with clear line of sight at head height, nearest the centre.

## Original start points

The `.dat` has the original player start points and capsule spawn points (six `CP00` sets, 4 starts each), exported in `cache/stNN.props.json` → `start_sets` (right-handed, like the pdcl). `pick_spawns` does not use them yet (request to the game lane).

## The runtime footstep class byte lives in the `.dat` 0x94 block (2026-09-29, wetzone lane)

**Verdict: FOUND. The class byte the footstep probe returns is the per-hull FLAGS byte (header byte 0) of the header-0x94 whole-stage collision block — a field we never parsed.** The earlier "unreachable from converted data" verdict (FORMATS-AUDIO.md "Footstep sound") measured only OC00 face `attr`s; the class was one header byte away the whole time. Per-surface footsteps — including Lost Palace's wet patches — are reachable from converted data after all.

**How it was established (program side, read-only Ghidra export at `%USERPROFILE%\.pdtools\re-out\`):**

- `FUN_140256390` (`decomp_all.c`, `//==== 140256390`) walks the linked list at `*(param_1 + 0x28) + 0x40` (param_1 = stage record + 0x12eb0, passed by `FUN_140230790`), node → node via `+0x20`. Per node it gates on **byte `pcVar1[2]`**: `== 0x14` → considered only if `(stage_state - 5) & ~2 != 0`; `== 0x1f` → **skipped outright** (`else if (pcVar1[2] != '\x1f') goto LAB` falls through); anything else → considered if `*pcVar1 == 3`. Of the considered faces it keeps the nearest (via `FUN_1401949a0` + `FUN_140124780` distance compare) and returns its byte-2 value in `cVar5` (decompiled `void` is a mistype — the caller assigns `cVar5 = FUN_140256390(...)`, and `cVar4` is set but otherwise unused, i.e. a `char` return in AL). Zero = no face → caller skips with no footstep at all.
- **Correction to FORMATS-AUDIO.md's framing:** the probe does NOT "gate on 0x14/0x1f to pick the wet path". 0x1f faces are excluded from the probe and 0x14 faces dispatch to *silence* (case 0x14 is in the do-nothing group). The wet/dry split is the ordinary surface dispatch in `FUN_140230cd0` on the returned byte: sand {3, 9, 0xC} → `PlySandType1`, dust {5, 6, 8, 0xB, 0x15} → `PlyDustType0`, water {0xD} → `PlyWaterType1`, everything else → nothing. Note dust produces `Type0` even on the "Type1" path — only sand and water visibly change.
- **Side kill for the audio lane:** all THREE emitter calls in the footstep region (`FUN_140230cd0`, `FUN_140230d60`, `FUN_140231490`) pass the identical constant `0x1e0c350` (= id 30). The "third call decodes to 35" lead is dead by direct read — do not ship anything assuming it.

**How it was established (data side, `engine/re/wetzone_probe.py`, game files read-only):**

- The header-0x94 block (1 chunk run per stage, `bad=0` parse on all 22 ST00 stages) is hull chunks in our `parse_hull` shape: id-3 face chunks (body = 10 f32 bbox + count + faces of `{u16 a,b,c, u16 attr, f32 normal}`), then **one** id-8 vertex pool **after** all face chunks, then the end marker. st03: 1039 face chunks, 2723 faces, pool of 1921 verts with max face index exactly 1920 ✅. Header bytes are `[flags, 3, 3, 0xFF]` on every face chunk (bytes 2–3 constant everywhere, including OC00) — byte 0 varies per chunk and is the only per-face-varying byte available.
- The flags values exactly span the dispatcher's domain: global faces-per-flags = dust {5: 10.3%, 6: 2.7%, 8: 29.9%, 0xB: 0.4%, 0x15: 0.4%}, sand {9: 9.6%}, water {0xD: 0.2%}, silent cases {4, 7, 0xA, 0x10, 0x12, 0x14}, plus silent-default values {1, 2, 0x16, 0x18–0x1E, 0x20} and **0x1F at 2.6%** — the probe-excluded class, present in st01/st03/st07/st09. Face-level `attr` in the same block is only {0x0, 0x34} ✅, so attr is definitively not the class byte.
- **Lost Palace (st03) focus:** 32 water-class faces, ALL flat floors (normalY = 1.0) at hall level y ≈ 49.9, clustered in ~10 puddle-sized patches (50×50-unit squares at e.g. x −149…−99 / z −149…−99 and x 51…151 / z −149…−99, plus ~30×30 patches near (±68, 74/125/223)); several patches are listed twice (coincident duplicate chunks). Dust classes cover 993 faces (36%); sand is absent. So the wet step is positional on real, mappable patches — minority by face count (1.2%) but large by area, consistent with Laurence hearing it ~20% of the time on paths that cross them.
- Characterisation (st03 floors/walls/ceilings per flags, orientation by face normal `ny` — floor `>0.7`, ceil `<-0.7`; reproducible from the committed `wetzone_probe.py`): 0xD = 100% floor (32/32); 0x20 = 100% floor (103/103) but silent (unlabelled floor class); 0x1F = floor 5 / wall 278 / ceil 26 — i.e. almost entirely walls and ceilings, which is *why* the downward probe can skip it. (An earlier draft here said "253 walls + 35 ceilings"; those integers were threshold/ad-hoc and did not reproduce — the counts above are the measured ones.) 0x8 (dust) is the majority catch-all on all orientations. Global over all stages, WATER (0xD) is **176 faces, 100% floor**.
- OC00 object hulls use the same header shape with their own flags (global: mostly 1 = silent-default, plus live 6/8/21 and silent others) — objects can therefore also produce dust steps (e.g. contaB's 6). Whether object-hull nodes join the same runtime list is ⚠️ unconfirmed (likely but not traced).

**What is still unconfirmed ❓:** the exact builder function that copies chunk-flags → runtime node +2 was not found (hunted `0x12eb0`/`0x12f04`/`0x12c70` users: `FUN_140254500` is only the sub-object init, `FUN_140254590`/`FUN_140255020` are per-frame movers, `FUN_1401d9210` is the effect-file loader). Dispatcher cases **3 and 0xC (sand) appear in NO stage data** (neither 0x94 nor OC00, all 30 files) — source unknown (story-mode geometry? legacy cases?). The meaning of the silent non-switch classes (2, 0x16, 0x18–0x1E, 0x20) is unread.

**What a build lane needs:** parse the 0x94 block with the existing `parse_hull` (one shared vertex pool at the end, not one per hull — small adaptation), keep each face chunk's flags byte as the face's surface class, and export it alongside the pdcl (e.g. a per-triangle class array like the existing OBJT trailer). Footstep VFX/sound selection then reads the class of the ground face: 0xD → wet, {3,9,0xC} → sand-Type1, {5,6,8,0xB,0x15} → dust, else silent; skip 0x1F faces in the ground query exactly as the original does.

## The surface class sidecar `<stage>.surf.json` (2026-09-29, surface lane)

The 0x94 mesh above is now exported: `pd_convert stages` writes one sidecar
per stage next to the `.pdcl`, parsed by `pd_formats::surface::SurfaceMesh`
(a reimplementation of `wetzone_probe.py`'s walk in Rust). **A JSON sidecar,
not a `.pdcl` trailer**, because the 0x94 mesh is a different mesh from the
`.pdcl` (simplified stage collision vs ALR geometry + object hulls, different
triangle counts) — no per-triangle array could align with the `.pdcl`. The
`Collision` type and its parse API are untouched (`pd_game` compiles as-is).

**Layout** (`"format": "surface-v1"`): `faces` / `chunks` / `vertices` /
`bad` counts, `class_counts` (`{"13": 32, ...}`), then the mesh itself —
`xyz` (one `[x,y,z]` per triangle vertex in order, mm-rounded) and `class`
(one surface class per triangle). Coordinates are right-handed export space,
the same as the `.pdcl` (game z mirrored). Stages with no usable `.dat`
(`st00`, the `st09_01..08` area files) get no sidecar; a parse failure warns
and converts the stage anyway, never failing the run.

**Class → meaning** (program evidence in the section above; audio evidence in
FORMATS-AUDIO.md "Third emitter constant"):

| Class | Family | Sound / VFX |
|---|---|---|
| 3, 9, 0xC | sand | `PlySandType1` — ⚠️ 3 and 0xC appear in no shipped stage data ❓ |
| 5, 6, 8, 0xB, 0x15 | dust | `PlyDustType0` (dust has no Type1) |
| 0xD | water | `PlyWaterType1` |
| 0x1F | skipped | the ground probe ignores these faces entirely |
| everything else | silent | no footstep |

**Parity proof** (`engine/re/surface_parity.py`: per-class face counts AND
every triangle position vs the probe, ≤1.5 mm): 21/21 stages OK, `bad=0`
everywhere.

| Stage | Faces | Notable classes |
|---|---|---|
| st03 / st13 / st23 Palace | 2723 | water 0xD: **32** (flat floors y 49.9, ~10 puddle patches); dust 993 (8: 885, 0xB: 108); skipped 0x1F: 309 |
| st02 / st12 / st22 Highway | 3645 | sand 9: 2368; dust 5: 852, 6: 282 |
| st06 / st16 Refinery | 3846 | dust 5: 1009, 6: 133; no sand, no water |
| st26 Refinery | 3922 | as st06/16 plus 76 class-2 faces |
| st07 / st17 Lane | 9343 | dust 8: 5927, 5: 995, 6: 178; skipped 0x1F: 77 |
| st04 / st14 Panorama | 2247 | dust 8: 2130 |
| st24 Panorama Boss | 694 | dust 8: 666 |
| st08 Sein | 3822 | dust 8: 2721 |
| st05 / st15 / st25 Strange City | 1195 | all silent (0x20 alone: 498) |
| st01 Edgar's Dream | 3009 | all silent (0x2 alone: 1948) |
| st09 hub | 8377 | water 0xD: **80** (unmapped patches — a future lane could check them); dust 0x15: 331, 6: 435, 5: 92; skipped 0x1F: 631 |
| st32 test arena | 800 | all class 2 (silent) |

**API for the future sim lane** (`pd_formats::surface`): `SurfaceMesh::parse`
reads a `.dat`; `class_below(x, z, max_y)` answers "what surface is under
this player?" (highest non-`0x1F` triangle at/below `max_y`, brute-force
reference — the game will want the same logic over a spatial index);
`class_kind(class)` maps the byte to `Sand` / `Dust` / `Water` / `Silent`.
The sidecar's `xyz` + `class` arrays are the serialised form of the same mesh.

**Unconfirmed ❓:** st09's 80 water faces were counted but not mapped to
patches; validity of `class_below`'s brute-force scan at game frame rates
(spatial index left to the sim lane); whether object-hull flags join the same
runtime list (see previous section).

## The stage collision is the 0x94 block (2026-10-01, collision lane C1)

Trigger: "the Palace stairs can't be walked up, only jumped" (audits/P0-COLLISION.md). **Reproduced in Rust** (`pd_game/src/stair_walk.rs`, real st03 file): from the foot of either big flight, both gaits, x = +-95/100/105, the body ends at z -132.1, y 51.9 and stays (render-mesh collision: tread bevels steeper than the floor limit leave floor gaps, the body drops into the hollow, a riser shoves it back). Fixed by building the stage part of `.pdcl` from the program's own mesh.

**Program evidence (read 2026-10-01, `decomp_all.c`):** the stage loader `FUN_140203720` sets `DAT_140532300 = dat + *(u32*)(dat + 0x94)`, then calls `FUN_1401d3040` (every face of every id-3 chunk goes into a per-triangle spatial grid with the chunk's flags byte), `FUN_1401d38f0` (world-tree bounds) and `FUN_1401d4070` + `FUN_1401d4ff0` (one static collision object per chunk, registered in the tree `DAT_140530200`, error string `##pdStageCol## Can't RegistTree`). `FUN_1401d5860` (what the movement code calls) walks that same tree with `FUN_1401d5530`/`FUN_1401d5130` (shape-vs-shape contact), and the contact node's +2 byte is copied from the shape's byte 0 = the chunk flags byte (the class `FUN_140255020` tests for 0x1F/0x14). No ALR render mesh is loaded into this tree. So the body/ray world is the 0x94 block; this closes audit Read 1. Still unread: which callers pass filter 1, and who (if anyone) uses class 0x1F (the file builder simply drops them).

**Build** (`pd_convert collision` / `stages`; default): stage triangles = 0x94 faces minus class 0x1F, each wound to its STORED normal (`SurfaceMesh::collision_triangles`; the z mirror makes the stored normal opposite to the raw winding on ~99.5%); object `OC00` hulls and the `OBJT` trailer unchanged; a `WIND` marker after the trailer says "stage winding is authoritative". Stages with no usable block (st00, st09_01..08) fall back to the ALR render triangles, no marker. `--collision alr` = the old builder (A/B); `--collision dat+walls` = 0x94 plus the render's steep triangles (keeps rails/edge closers the 0x94 lacks; not the program's rule).

**Loader rules** (`Collision::from_bytes`, signed files only): a stage triangle is ground only if its signed normal.y >= 0.643 (`0x140255811`); one pointing down that steeply is a ceiling (never stood on, no sideways push, rays still hit it); the rest are walls. Objects keep the old `abs(normal.y) >= 0.6` rule. Files without `WIND` load exactly as before.

**What changed, all 30 stages:** `pd_convert collcompare <old cache> <new cache> <out dir> [stNN] [--walk x,y,z]` compares the two caches cell by cell on floor a fighter can walk onto (flood fill from the start points), with a picture per stage and the walk-probe connectivity numbers. Categories and findings are in the lane report; headline: roofs / outdoor ground / the corridors beyond the Palace hall are gone (render-only), the 0x94 closes the hall and the arenas' edges with its own walls (yellow), stair flights are smooth ramps, every start point and capsule spot keeps its ground, and the render's rails/machinery that the 0x94 lacks are no longer solid (st03 tree ring, st06 deck edges ~500 spots, st08 ~1,450, st02 ~215, st05 ~110). Sheet: `engine/shots/c1_collision_changes_sheet.png`.

**Regenerate the cache on main** (the stair tests need it): `pd_convert collision "<Map>\stNN.alr" "<cache>\stNN.pdcl"` for each stage, or the whole `pd_convert stages "<Map>" "<cache>"`.

**Test-only spawn:** `PD_TEST_SPAWN=x,z[,y][;x2,z2[,y2]]` places P1 (and P2) on the floor under that point, for scripted captures.

## The takes are in the .dat's space, ours is mirrored (2026-10-02, collide lane)

**Finding: the film harness's `--real` placement put our fighters on the MIRROR IMAGE of the stage.** The original's traces (`takes_full/*_trace.csv`) are in the program's own space, which is the stage `.dat` as stored; every export of ours (`.pdcl`, render) is right-handed with z mirrored (INDEX.md), so a trace spot (x, z) is (x, -z) in our world. `film_compare.py` fed trace z to our sim unmirrored, so overhead plates, slanted walls and boxes that were not there in the original were over / beside our fighters.

- **MEASURED** (48,282 trace samples whose y is not on the flat 330 / 270 decks, `st06.dat` 0x94 floors with normal.y > 0.643): the body's y equals a floor under it at the take's (x, z) in 46,935 unmirrored vs 14,823 mirrored.
- **Two lanes' symptoms were this one cause.** (a) Jump take 239 (spot 314.5, -337.3): in the original's space nothing overhangs the spot (dat chunks at z -331.8 reach only y 330), so it falls straight to 330; mirrored, our body stood under a class-18 plate at 356, a class-2 ramp at 378 (normal.y 0.73) and a class-2 plate at 333, and landed on the ramp. (b) Crawlers / Spore Schism: a box (chunk 245) and slanted walls stood 3-18 u in front of our caster, none in the original.
- **READ, no hidden floor rule:** the player ground test `FUN_140255020` skips only class 0x1F, and class 0x14 while the state is 5 or 7 (`0x1402553f1..`), accepts a face whose stored normal.y > 0.643 (`0x140255811`, constant `0x1402f7d50`), and takes the plane inside the body window: grounded last tick, plane in [feet - 7.5 - horizontal step, feet + halfH + 9); airborne, plane in [feet, feet + halfH + 9) (the contact sphere is centred on the body, radius halfH + 7.5 + |step|, so a 4 u fall step cannot tunnel). Class 18 / 27 / 2 floors are ground like any other: with the correct mapping every one of the 56 trace crossings of an upward floor (> 0.643) by a falling fighter ends on that floor (classes 5, 6, 27), none passes through. Our `ground_below` + `STEP_HEIGHT` rule agrees.
- **READ, objects (shots, crawlers, props) do skip classes:** the object wall sweep `FUN_14026ca50` -> `FUN_1401633b0` -> `FUN_140163e80`, and the object ground probe `FUN_140163cb0`, ignore hull faces of class 0x10, 0x12, 0x14 and 0x1F (mask `0x80150000 >> class`); the segment-vs-face test `FUN_1401d22e0` is two-sided (no facing test); the owner's body is left out of the fighter loop (`FUN_140266060` clears its bit in the object's mask `+0x24`). NOT built: our `.pdcl` carries no per-triangle class (a `.surf.json` sidecar covers only the 0x94 mesh, whose first N triangles are the `.pdcl`'s first N), so a shot still stops on class 16 / 18 faces (st06: 446, st07: 767, st03: 459 faces); needs a class trailer and a cache regeneration. **Built 2026-10-03** (`CLAS` trailer, "Per-triangle class" below).
- **`dat+walls` is not the program's collision.** Spore Schism's remaining mismatches (caster hit by its own blast, cast end +8.5 ticks) vanish with a plain `--collision dat` st06 (stage = 0x94 faces + object hulls): the render's extra steep triangles (`dat+walls`) include a slanted wall 3.5 u in front of the caster, shielding it from the blast's line of sight. (Left as shipped at the time; **retired 2026-10-03**, see below.) The A/B switch is `PD_FILM_CACHE=<scratch cache with only the stage files replaced>` in `film_compare.py` (hard-link the rest of the cache).

**Film numbers** (`--real`, 74 skills: every 5th of the 341 + the six crawl / jump takes; before = `PD_FILM_NOMIRROR=1`, after = mirror-correct): mismatches 106 -> 78 (3 of the 78 are tool timeouts, 2 of those 3 pass on a re-run), clean skills 29 -> 35, hit_tick 18 -> 9, p1_health 17 -> 8, cast_end 8 -> 5, p0_health 4 -> 1. Jump 239: 1 -> 0 (y at original tick 60 / 92 / 125 / 160: ours 365.1 / 428.8 / 365.7 / 330.0, original 364.9 / 427.6 / 363.5 / 330.0; before: 365.1 / 428.8 / 378.6 / 378.6); Root of Tree 3 -> 1, Venom Fang 3 -> 0, Crawlers 3 -> 1, Crawling Bullet 3 -> 0 (the leftovers are sounds), Spore Schism 4 -> 3 (1 with plain `dat`). Nine skills got worse because the stage is now the original's: Throttle Blaster 306 (a render wall shields P1; 0 with plain `dat`), Tornado 311 and Blazing Storm 146 (hit / no-hit differences that remain with plain `dat`: a real rule, UNCONFIRMED: Tornado's path past the class-2 box at 18 u, rain-column cover), the rest sound-only. Old `--real` numbers are not comparable.

Harness fix: `film_compare.py` `to_sim_xz` / `to_sim_yaw` (z -> -z, yaw -> pi - yaw) in the real placement and in `placement_off`; `PD_FILM_NOMIRROR=1` restores the old placement for A/B. Every earlier `--real` film number (compare2, compare3, lane reports) used the mirror-image stage.

## Per-triangle class, the three query families, and `dat+walls` retired (2026-10-03, collclass lane)

Two things settled together: which faces each kind of query ignores (the program's own rule, READ), and whether the render's extra walls (`--collision dat+walls`) belong in the collision (they do not).

### The class trailer

`.pdcl` files now end with `"CLAS"` + one byte per triangle, after `OBJT` / `WIND` (order: triangles, `OBJT`, `WIND`, `CLAS`). The byte is the program's surface class: the flags byte (header byte 0) of the 0x94 face chunk the triangle came from, or of the object's OC00 hull chunk for an object triangle. Old files (no trailer) load with class 0 everywhere, i.e. nothing is skipped, exactly as before; a stage with no 0x94 mesh and no classed hull keeps its old bytes. Added render triangles (`--collision dat+walls`, A/B only) are class `0xFE`. Code: `Collision::to_bytes_classed` / `from_bytes`, `SurfaceMesh::collision_triangles_classed`, `HullFace::class`, `StageProps::collision_class`. The class-`0x1F` faces are now KEPT in the file (they were dropped before) but live in their own grid that only line-of-fire rays read.

### READ: who skips which class (`decomp_all.c`, image base 0x140000000)

| Query family | Program functions | Classes it ignores |
|---|---|---|
| **Fighters' bodies** (wall push, ground) | `FUN_140255020` (the mover), `FUN_140163950`, `FUN_140164770`, `FUN_140255b70`, `FUN_140256210`, `FUN_140256390` (footstep probe) | `0x1F`; `FUN_140255020` also `0x14` while the fighter state (`+0x12f04`) is 5 (airborne) or 7 (float). Class `0x14` exists only in st01 (178 faces + a 102-face hull), which is not a versus stage, so the airborne rule is NOT built (our `grounded` is not the program's state 5 / 7: UNCONFIRMED). |
| **Objects** (shot wall sweep, crawler floor, tornado, hole / column / mine markers) | wall sweep `FUN_140163e80` (nearest face; reached through `FUN_1401633b0` / `FUN_140163680` from the shot update `FUN_14026ca50`, the area update `FUN_140267410`, mine `FUN_140267b90`, tornado `FUN_140268940`, `FUN_14026a7f0`); ground probes `FUN_140163cb0` (crawler, from `FUN_140266290`) and `FUN_140164330` (hole aim `FUN_14026e410`, tornado, `FUN_140268590`, dodge `FUN_140270200`; also needs stored normal.y > 0.643); `FUN_140163b50` | `0x10`, `0x12`, `0x14`, `0x1F` (mask `0x80150000 >> class`). `FUN_1401644b0`, a sphere probe whose caller (`FUN_140253f20`) is unread, skips only `0x12` / `0x14`. The segment-vs-face test `FUN_1401d22e0` is two-sided (the product of the two end distances <= 0, no facing test), and the shooter's own body is left out of the fighter loop (`FUN_140266060`). |
| **Line of fire** (every hit test: bodies `FUN_14026c1c0`, blasts `FUN_14026c5b0`, areas `FUN_140267410`, blades, CPU) | `FUN_1401641b0` | NONE but `0x10` (the stage's outer box): for each contact, `class != 0x10` returns "blocked". Rails (`0x12`) and the class-`0x1F` faces therefore block it. The ray runs from the object's CURRENT position to the body, so a shot that has already flown through a rail has a clear line. |

Loader (`FUN_1401d4070` for the stage, `FUN_1401d3ef0` for objects): a chunk is registered in the collision tree only if header byte 2 has bit 0; byte 0 (the class) then ORs byte 2 with `0x10` (class `0x12`), `0x40` (`0x13`), `0x20` (`0x1F`) or `0x70` (anything else). An object registers a collider from its hull only (`FUN_1401fbbc0` -> `FUN_1401d3ef0`, which returns null for a null hull). Jump-zone hulls are not walls: the mover reads a contact whose collider kind (`+8`) is 6 as a launch trigger (`FUN_140255020`). The filter byte the shot sweeps store at `DAT_1404bec60 + 0x10` (`param_7` of `FUN_1401633b0`) belongs to the collider pointer of object contacts (stage faces have a null pointer): unread, UNCONFIRMED.

What each class is called is NOT read (the footstep table covers 3 - 0xD / 0x15 only). Counts: st06 `0x10` = 16 faces (one outer box, 3.06 M square units, bbox x -557..665, y 0..800), `0x12` = 430 faces (275 upright, 97 flat, scattered over the decks), no `0x14` / `0x1F`; st03 `0x1F` = 309 faces (278 upright, 26 down-facing). Counts per stage: the `class_counts` of each `.surf.json`.

### How our queries map (`Collision` API)

| Our call | Program query | Rule |
|---|---|---|
| `push_out_of_walls`, `ground_below`, `layers_at`, `raycast` | fighters' bodies, generic rays (dash, camera, CPU navigation) | hidden `0x1F` faces never seen; everything else solid |
| `raycast_object_normal` / `raycast_object` | shot sweep (`FUN_14026ca50`), tornado wall and roof, rain-column roof | skips `0x10 / 0x12 / 0x14 / 0x1F` |
| `ground_below_object` | crawler floor, tornado floor, hole / column / mine aim and marker slide (`Launch::aim`) | skips `0x10 / 0x12 / 0x14 / 0x1F` |
| `raycast_los` | shot-to-body, area and blast line of fire | blocked by every class except `0x10` (hidden `0x1F` included) |

Left on the generic ray because no program function was read for them: the rain drops' wall test, the Dash move's wall stop, CPU navigation, the spawn picker.

### `dat+walls` vs the program: retired

`--collision dat+walls` appended every steep render triangle to the 0x94 mesh (st06: 6,664 -> 24,820 triangles, 18,156 added). The program does not have them: its stage collision is the 0x94 block only (`FUN_140203720` -> `FUN_1401d3040` / `FUN_1401d4070`; header `0x98` is the stage-effects `EF00` block, not collision), plus object hulls. No ALR render mesh is loaded into the tree, and there is no separate wall block, step limit or invisible-wall layer beyond the classes above and the 0.643 ground limit already in the loader.

**MEASURED against the take traces** (`pd_convert tracecheck <a.pdcl> [<b.pdcl>] --takes <takes_full> [--falls] [--cross]`, new): the 682 fighter traces of the 341 takes (all on st06, mirrored to our export space) are walked sample by sample and every 0.5 u of the straight line between samples (steps over 25 u are teleports, not interpolated), testing the body cylinder (radius 4.5, 7 .. 20 u) against the wall faces the sim pushes out of.

| | `dat+walls` | `dat` (the program's mesh + hulls) |
|---|---|---|
| steps with the body 1+ u inside a wall | 22,749 (130 traces, worst push 18.9 u) | 3,876 (35 traces, worst 7.8 u) |
| of those, the deepest contact is an ADDED render wall (class `0xFE`) | 21,209 | 0 |
| of those, the fighter is in state 0 (standing / running) | 16,719 | 2,704 |
| real moves (0.5 .. 25 u between samples) that cross a face at body height | 172 of 6,333 (162 cross an added render wall) | 12 of 6,333 |

The original's fighters stand and walk inside the added walls, tick after tick, at spots such as (30, 270, -149) and (350, 328, -120): those walls are not in the original. What remains with plain `dat` is not render geometry: P1's recorder anchor sits 0.1 u inside the program's own class-2 box at x -9.8..19.6 / z 79..96 (a start spot in many takes), the class-2 chunk at (-160, 380, -140), and eight prop hulls (objects 92, 95, 72, 67, 81, 85, 240, 259, about 1,500 steps: thin rail / slab hulls at deck edges that earlier shots in the recorder arena may have moved or broken; our static file still has them). Fighters in state 17 (knocked down, 689 steps) are among them. UNCONFIRMED: why the program leaves a body that was placed behind a face where it is.

**Edges.** With pure `dat` the deck edges the render's rails guarded are open (`collcompare`, old = `dat+walls` against new, st06: wall removed 42,400 sq u, 846 new void-edge spots, 387 of them within 6 u of an old render wall). The original is open there too: 19 times in the takes a fighter is knocked off the Refinery decks (`tracecheck --falls`; neither file has a floor under any of the 19 first falling samples), in the 061 and 188 takes the falling body passes through an added render wall, and the fall costs 3 Health and a respawn (`FALL_OUT_DAMAGE`). The rails that do stop fighters in the original are object hulls (OC00 boxes) and the class-`0x12` faces of the 0x94 block; both stay in the file.

**Stairs.** The Lost Palace flights are the 0x94 ramps (30 degrees, normal.y 0.866 > 0.643), as the program reads them. `stair_walk.rs` (12 scripted walks, run and walk, x -105 / -100 / -95 / 95 / 100 / 105, both flights, plus the tread-by-tread ground check) passes on the plain `dat` file: they never needed the render's faces.

**Final build:** `pd_convert stages "<game_files>\Assets\Data\Map" "<cache>"` (default collision = the 0x94 mesh + object hulls + the `CLAS` trailer). Also changed: a visible object type with NO hull (st04 sign boards x10, st05 sign x1, st01 wall / stair pieces x9) no longer gets its model triangles as collision (the program registers colliders from hulls only); its props.json bbox is unchanged.

### Film proof (before = `dat+walls` cache, after = the shipped `dat` + `CLAS` cache; same exe, `film_compare.py run --real`)

Runs under `%USERPROFILE%\.pdtools\compare_collclass`: `before` / `after` = 96 skills on the older tool (11 of the "before" timed out under load); `B_before` / `B_after` = 49 skills on the merged hitdiff tool (fall-out and placement fixes). All takes are on st06 (the recorder's only arena), so "all stages" is covered by the class rules and `collcompare` over every versus stage, not by traces.

- **B set (49 skills, merged tool): 77 -> 42 mismatches, clean skills 19 -> 32.** Fixed: Bullet of Fire 130, Lightning Blade 135 (hit; one sound left), Fang of Tree 139, Twister 140, Whirlwind 141, Glacial Wall 149, Block 209, Demon's Fire 251, Mini-Meteor 302, Aura Cannon 305, Throttle Blaster 306, Spore Schism 260 (3 -> 1, a sound), Crawlers 259, Root of Tree 138, Blaster 80. Unchanged: 183 Leg Throw (cast end +3.5), 311 Tornado (original hits t91, ours none), 146 Blazing Storm (ours hits, original not), 239 Jump 0, 181 Empty Hand (hit in ours only).
- **A set (96 skills, older tool): 94 -> 89 mismatches, clean 44 -> 55.** Same fixes (95, 305, 306, 141, 140, 135, 260 ...); its apparent regressions (75 Rapid Cannon, 80, 130: missing frames / effects) are load timeouts and do not repeat in the B set.
- **Got worse, B set:** Triple Lasers 70 (1 -> 3: one of its arcing lasers now hits, -2 Health, where the original does not; the straight line P0 -> P1 is clear in the program's mesh, so the arc / bend of the three shots is the suspect, not a wall: UNCONFIRMED) and Debris Bullet 6 (two extra sound ids: a shot now flies on through a class-0x12 / 0x10 face and meets something further on; the pass-through is READ, what it meets UNCONFIRMED).
- **Hitdiff's edge-fall list** (28, 207, 268, 270, 327, 161, 197, 43 ...): the take's P1 recoils 8.3 u and stops; ours recoils 13 u and walks off at x ~277, z ~308 (skill 28: at tick 330 ours is at the original's end point, then keeps sliding). Floor under the whole original path and 14 u beyond it exists in BOTH collision files; no wall class explains the stop: a reaction / root-motion difference, not collision. **Empty Hand 181's dash stall at z 140.88** is 25 u short of P1 (z 115.8); the 0x94 mesh has only the floor there (st06 has no class-0x1F face), so it is not a stage face.
- **Tests:** `cargo test -j 2 -p pd_game -p pd_formats -p pd_convert -p pd_viewer` = 986 + 95 + 6 + 9 + 42 passed (1 ignored), `python -m unittest` in `engine/re` = 277 OK (4 skipped); the stair walks and the class check on the shipped file pass on the regenerated cache. No replay golden moved (they run on synthetic floors; nothing in the sim reads time or randomness differently).
- **Cache command** (scratch folder first; every file except 21 `.pdcl` byte-identical to the old cache; `st00` and `st09_01..08` unchanged): `pd_convert stages "<game_files>\Assets\Data\Map" "<cache>"`, then the 21 changed `.pdcl` copied in. Old files: `%USERPROFILE%\.pdtools\collclass\backup_pdcl_before`. (Earlier lanes' hard-linked scratch caches `cache-charlight`, `cache-fxfollow`, `cache-shotpoints`, `fxcache`, `fxcache-b` share the `st06.pdcl` inode and now show the new file.)
- Picture of what changed on st06 (blue = render walls removed, white = walls both files have): `%USERPROFILE%\.pdtools\collclass\collcmp\st06_cmp.png` (other versus stages: `stNN_cmp.png`; `collcompare` panics on st24 with a subtract overflow, an old bug).

### Not changed, UNCONFIRMED

- `st04_entA_` (st04 / st14 / st24): an invisible object type with a 2-face class-`0x10` hull (a 276 x 138 u quad at z 280). It is not made solid (invisible objects are not); the program probably registers it. Whether bodies are blocked by it is unread.
- The airborne `0x14` rule for bodies (st01 only), the exact query shape of the mine and the area markers, the rain drops' wall test, the unread filter byte above, `FUN_1401644b0`'s role.

## Known gaps

- No ceilings (jumping into overhangs passes through).
- Walls are thin-triangle tests; very fast movement could tunnel (not an issue at run speed 72/s).
- Falling off the rig respawns at your spawn point after dropping 150 units.
- Arena edges open to the void where the program's own collision (0x94 mesh + object hulls) has no wall: Dawn City rooftops, Sein islands, the ends of the Palace's north/south corridors. The original is open there too (the render's rails are not collision: "dat+walls vs the program").
- ~~st02 / st12 / st22 (Highway) and st24: from some start points the flood fill covers millions of square units~~ — fixed 2026-09-27, see "Highway's far floor and Panorama Boss's sky floor" above. The `.dat` 0x94 block above is still undecoded; it may be the original's own arena boundary (a Request to a future lane).
- Highway's two other start points (0/2) don't connect to start points (1/3) on foot even with the fix — the `--map` picture shows they're on different, non-adjacent ramps of the interchange. Unconfirmed whether the original connects them some other way (a jump, a different start-point set) or a 1v1 always uses two starts from the same ramp.

## Camera-only occluders: `stNN.camcol` (2026-10-03, lane camwalls)

Retiring `dat+walls` removed the render's walls from the collision, and the battle camera's wall pull-in (`follow_cameras`: boom to 3 u off the first hit, 4 u minimum; the near-wall dissolve follows the resulting eye-to-head distance) used that same ray. The camera then sat inside or behind walls and pillars (`shots/verify_coll.png`). The camera rule is UNCONFIRMED in the original (GAME.md "Camera matched to the original"), so the fix restores the old camera behaviour exactly instead of inventing a rule, and touches nothing the sim reads.

- **File:** `pd_convert stages` writes `<id>.camcol` (a `PDCL` triangle file with `OBJT` owner tags) next to each `.pdcl`. Contents = the old `dat+walls` `.pdcl` minus the new one: (1) the ALR render's steep triangles (`WALKABLE_NORMAL_Y`), only when the stage has a usable 0x94 mesh; (2) the model triangles of visible objects with no hull (st01 walls and stairs, st04 / st05 signs), owner = the object; (3) the class-`0x1F` faces of object hulls (the old file only dropped the stage's own 0x1F faces). Checked as triangle multisets for all 30 stages (`old == new minus 0x1F + camcol`). 21 stages get a file; st00 and st09_01..08 need none. The `.pdcl` files and every other cache file stay byte-identical.
- **Runtime:** `pd_game/src/cam_occluders.rs` (`CamOccluders`, loaded at start and per match). The camera ray = the nearer of `Collision::raycast` on the program collision and on the `camcol` set; a broken or thrown object's camera triangles go off with it (`sync_gone`). A cache without `.camcol` behaves as before this lane (program collision only). Gameplay (`raycast`, `push_out_of_walls`, shots, CPU, spawn pick) never reads it. `pick_spawns`' line-of-sight check is a spawn survey, not the camera, and stays on the program collision.
- **Proof:** test `camera_ray_equals_the_old_collision_ray_on_real_stages` (env `PD_CAMCOL_OLD` = old `.pdcl` folder, `PD_CAMCOL_CACHE` = new cache): 4,000 random rays per stage, 30 stages, identical distances. Picture A/B `engine/shots/camwalls_sheetA.png`, `camwalls_sheetB.png`.
- **Cache command** (scratch first): `pd_convert stages "<game_files>\Assets\Data\Map" "<scratch>"`, then copy the `*.camcol` files into the cache (every other file is identical to the shipped cache).

## The invisible block under the Palace flights, opened (2026-10-03, lane stairs; departs from the original at Laurence's request)

Laurence: "The staircase on the main arena has a bad hitbox underneath (in the original game and in ours). Walking underneath has a thick hitbox that obscures the player when nothing visually should be." Confirmed: "main arena" = Lost Palace (st03; st13 / st23 share its collision file byte for byte). **Default ON; `PD_STAIRS_ORIGINAL=1` restores the original's collision exactly** (read once at load, same for every player, the sim is untouched).

**What it is (MEASURED).** Each big flight (x -113..-87 and +87..+113, z -233..-125, y 50 -> 109) is a thin slab on the render: only the soffit (underside) and the hall floor are under it (checked against `st03.glb`: no render triangle at any of the walls below). The 0x94 block nevertheless carries three class-`0x12` walls that fill the wedge between soffit and floor on the flight's OPEN side (stage triangles 747 / 507 / 541 west, 2042 / 2089 / 2256 east): the open-side wall (x -86.65 / +87.17, z -204.6 .. -148.3) and the end wall (z -204.6, x -112.65 .. -86.65, y 50 .. 82.4). The outer wall (x -112.65 / +113.17) stands against a real render wall (x -112.1 / +112.x) and is legitimate. Effects, all reproduced: a body walking in from the hall stops at |x| 81.7 (`stairs_under` tests, both flights); the camera boom ray (the battle camera's wall pull-in, `follow_cameras`) from a fighter standing under the high end toward the hall stops at 13.5 u and the boom is pinned about 11 u from his head (`PD_CAM_DEBUG`: boom 11.2 of 61.6), so the player fills the view or is grained away by the near-wall dissolve; shots and line-of-fire rays stop on it too (class 0x12 blocks the line of fire, "Per-triangle class" above).

**What changed (load-time filter, `pd_game/src/stairs_under.rs`; the cache files are untouched, nothing to regenerate).** Per flight the three wedge walls are removed and replaced by the part that is really needed: the pocket where the soffit is lower than a fighter (`sim::BODY_HEIGHT` + 2 = 22 u of headroom, z -148.3 .. -186.3): the open-side wall's own corner triangle plus a cross wall at the headroom line, class `0x12` like the originals. There the slab itself would swallow a body (head and chest inside the render), so he is still stopped; everywhere with headroom he now walks under, up to the hall's outer wall. The soffit (a ceiling, rays still hit it), the ramp (the walkable stairs: both gaits, both flights, still reach the balcony), the handrail hulls and the outer walls are untouched. A file that does not hold exactly the expected three triangles per flight is left as it is. `RawCollision` (pd_formats) is the parse / edit / bucket API the filter uses.

**Proof.** Tests (`stairs_under::tests`): the expected triangles are on the real file; `apply` removes 6 / adds 6 on st03 / st13 / st23 only and only once; a body from the hall at z -196 stops at |x| 81.7 / 82.2 originally and walks to |x| 107.7 / 108.2 opened; the low pocket (z -155 .. -183) stays closed and z -192 is open; the camera ray toward the hall: 13.5 u -> free; the stairs still walk up (run + walk, six x each) and the ramp is still one straight slope. Picture `engine/shots/stairs/stairs_before_after_sheet.png` (P1's camera under the flight, before / after, plus the collision slice and plan; the parts are `overlay_side_x-100.png` and `overlay_plan_y58.png`).

**Other versus stages (brief survey, nothing changed).** A detector for the same signature (a class-`0x12` vertical wall that shares two vertices with a sloped soffit and stands on a floor, 12+ u tall) finds: **st04 / st14 (Panorama)**, both ramps x +-80..110, z 114.7..159.7, y 57 -> 79.6: class-0x12 walls on three sides (both sides and the high-end wall at z 114.7, render coincidence 24-48 %, the soffit itself is visible); a body walking toward the high end from z 95 stops at z 109.8, so the same kind of block is plausible. Not identical (the sides are closed too, the render may have side walls there) and not trivial, so NOT changed: needs Laurence's eyes. st06 / st16 / st26 (x 121..157, z 20..50, y 330..345, deck rail hulls) and st07 / st17 (two tall class-0x12 walls at z -283 and -372) matched the detector but look like rails and building walls, not a flight; st02 / st12 / st22, st05 / st15 / st25 and st08: none. The detector is a heuristic (scratch scripts, not committed); it does not prove the other stages are clean.

**Head line, 2026-10-07 (Laurence: still an invisible barrier deep under the flight).** The pocket above stood at 22 u of soffit, and the wall radius (5 u) held a body 5 u short of it, under 24.9 u of soffit. The cross wall now stands at z -177.85 (soffit 17.1 u), one wall radius past the head line z -182.85 (soffit 20 u = `sim::BODY_HEIGHT`), so a fighter walking in stops with his head just under the slab; the open-side wall runs from the low end to the head line. BUG-GRAVEYARD "Still an invisible barrier deep under the Palace stairs".

**Panorama ramps (st04 / st14), checked 2026-10-03 at Laurence's request to open them: NOT changed, nothing to open.** The ramp wedge (x +-80..110, z 114.7..159.7) was measured against the rule "keep a wall only where the soffit is too low to stand in, leave real render walls alone": the soffit is 23.6 u over the floor at its high end (z 114.7) and under a fighter's 22 u from z 117.8 on, so the whole wedge already IS the headroom pocket (3 u of slack at the end wall); the class-0x12 end wall at z 114.7 only moves to 117.8 if opened (a body then stops 3 u further in). The side walls at x 80 / 110 coincide with render faces (the class-8 walls at x 80.1 / 110.0 match the render at 100 %): the render has solid sides, as the south Palace ramps do. The bigger Panorama stairwells (x +-20..203) have class-8 walls that match the render. Pinned by `stairs_under::tests::panorama_ramp_wedge_is_already_just_the_headroom_pocket`. Pictures `engine/shots/stairs/p_south_t0200.png` (the real staircase with rails and solid sides), `st04_x95` slice in the scratch folder.
