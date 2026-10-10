# Stage light (lane L3, 2026-10-01)

Code: `pd_game/src/stage_material.rs` + `stage_material.wgsl` (the `SBTS` pixel
shader), `shadow_grid.rs` + `shadows.wgsl::volume_winding` (stage shadow
volumes), `shadows.rs` (wiring, hull casters), exporter in `pd_formats`
(`gltf.rs`: bump draws, `GRID_NODE`, `extrude_volume_vertex`; `character.rs`:
`HULL_TAG` nodes; `stagedat.rs`: the light call; `alr.rs`: `MaterialEntry::
{fresnel, extrude}`). Tests: `stage_material::tests::*` (golden values from the
asm), `shadow_grid::tests::*`, `gltf::collision_filter_tests::{stage_export_
carries_the_volume_grid_and_bump_draws, volume_extrusion_follows_the_vertex_
shader}`, `character::tests::the_authored_shadow_hulls_are_exported_under_
their_joints`, `stagedat::tests::reads_light_script`. The independent reference
for the golden values is `engine/re/sbts_golden.py` (SBTS_vs + SBTS_ps one
instruction at a time over the real `spec77.dds`).

## 1. The big finding: the light script was read in the wrong order

The call `0A` handler is at `0x140206850` (`tools/disasm`-style disassembly,
capstone; not in the Ghidra decompile because it has no function boundary):

```
slot k of rcx = the call's argument pushed (n-1-k)th: the LAST pushed argument is slot 0
+0x00 <- slot1 (a[8])   +0x04 <- slot2 (a[7])   +0x08 <- slot3 (a[6])      colours
+0x14 <- slot4 (a[5])   +0x18 <- slot5 (a[4])   +0x1c <- slot6 (a[3])      directional light x, y, z
+0x20 <- slot7 (a[2])   +0x24 <- slot8 (a[1])   +0x28 <- slot9 (a[0])      hemisphere axis x, y, z
```
(`a[]` = the pushes in file order, the layout of `FORMATS-STAGES.md` "Lighting
script"; the fog call `05` at `0x1402068c0` confirms the reversal: its set
number, the last push, is slot 0.) Consequences, all fixed here:

1. **Every vector was mirrored.** The file lists a vector as (z, y, x): the
   program's x is the THIRD float. st03's light travels `(0.546, -0.677,
   -0.4925)` in the game's left-handed space, not `(-0.4925, -0.677, 0.546)`.
   The two differ by ~174 degrees around the vertical for the Palace: lit and
   shadowed sides, and the direction of every shadow, were reversed in L1/L2.
   Independent check on data: the authored pre-extruded volumes
   (`SSHADOWC`, fmt 0x10) have long slanted edges parallel to the authoring
   light: st05 `(0.57, -0.62, -0.52)` (cos 0.985 with the corrected script
   direction `(0.504, -0.678, -0.534)`, -0.15 with the old reading), st03
   `(0.17, -0.80, -0.575)` (0.92 vs 0.14), st07 0.83 vs 0.01.
2. **The colour roles.** `a[6]` (the first colour, "sky" in the json) is the
   **directional light colour** (`cDL_COL`, block +8); `a[8]` (the third,
   "sun") the hemisphere colour for normals ALONG the axis (`cS0`, block +0);
   `a[7]` ("ground") the colour AGAINST it (`cS1`, +4). st05 = warm
   (130,85,50) sun + cool (95,105,114) / (60,70,80) sky ambient, as a dawn
   scene should look. This settles the "reversed" A/B of `CHAR-LIGHT.md` §4: L1's
   `c1 -> DL, c2 -> S1, c3 -> S0` was right. The stage bake
   (`gltf.rs::light_vertices`) had the roles the other way round.
3. Point lights (call `07`, handlers `0x140206920` / `0x140206960`): same
   reversal, `[range, z, y, x, colour, index, set]` in file order. Every
   arena's set 0 lights are zero-colour (st03: range 26.76 at the origin;
   st05: 1e8 at the origin; st06: 1e8 at (-3272, -614, 0); st07: 0), so no stage
   has a static point light.

`stagedat.rs` now stores the vectors in the program's order and the json carries
`"vec_order": "zyx"`; `stage_look::script_vec` swaps an older cache's untagged
json at load, so a cache that was not re-exported still lights correctly (the
stage vertex colours are baked, so those need the re-export). `PD_STAGE_LIGHT=0`
reads the vectors the old way (see §7).

## 2. SBTS: per-pixel bump lighting

Format 0x03 stage vertices with material flag 0x20 (215k + 47k of 400k
triangles on st03/04/05/06/07/08/01/02/12) are drawn by `SBTS_vs` / `SBTS_ps`.
Vertex: position, packed normal (+12), UV, colour (+20, BGRA), tangent (+24),
binormal (+28). Frames are orthonormal and `B = -(N x T)` on every vertex (checked
on 4M..23M vertices of st03/05/06/07), i.e. after the Z mirror `B = cross(N, T)`,
glTF tangent handedness `w = +1` always.

`SBTS_vs` (per vertex; `T`, `B`, `N` = the frame, `L` = to-light = `-cDL_VEC`,
`V` = to-eye):

```
o3.xyz = 0.5 * (-S0.T, -S0.B, +S0.N) + 0.5      hemisphere axis in tangent space
o3.w   = a * (k - max(N.V, 0))                  a = COLOR0.w, k = cEYEVEC.w = (short at material +0x40) * 0.01 + 1
o4.xyz = 0.5 * (-to.T, -to.B, +to.N) * range / d^2 + 0.5     point light, to = unit vector to the light
o4.w   = COLOR0.x                               the vertex colour's R, used as a multiplier
o5     = ( L.T,  L.B,  L.N)                     directional light, tangent space
o6     = ( H.T,  H.B,  H.N)                     H = normalize(V + L)
```
`SBTS_ps` (gamma space, no tone map; `t0` base, `t1` normal map, `t3` = `spec77.dds`):

```
N     = 2 * (t1.rgb - 0.5)                      unnormalised
lut   = t3(N.o5, N.o6)                          rgb = the u coordinate itself (so N.L), alpha = floor(255 v^20) / 255
A     = 2 * (0.8 * sat(lut.b) + 0.00806) * (2 * t0.a - 1.5)     the alpha the shadow pass blends with
Lp    = 2 * clamp(o4.xyz, -1, 1) - 1 ;  P = sat(N.Lp)
h     = 0.5 * N.(2 (o3 - 0.5)) + 0.5            (c7.w = 0.5)
amb   = sat( sat(h) * cS0 + (1 - h) * cS1 )
dlp   = sat( sat(P * cP0) * cP0 + sat(lut.rgb * cDL) )        (the colour of the point light enters squared)
lit   = sat( dlp * t0.rgb + t0.rgb * amb ) * clamp(o4.w)
spec  = sat( t1.a * lut.a * clamp(o3.w) * cSPEC )              cSPEC = material (R,G,B) * 2/255 * cDL_COL (zero unless flag 0x04)
out   = lit + spec ;  out.w = A
```
Frame signs are the program's, literally: the directional light and half vector
use (+T, +B, +N), the hemisphere and the point light (-T, -B, +N).
`stage_material.rs::bump` mirrors the WGSL; golden values in the tests come from
`engine/re/sbts_golden.py`. The vertex colour of a bump draw is NOT a colour:
only R (an occlusion-like multiplier, grey on every stage except st03) and A
(the specular weight) are used. We carry them in TEXCOORD_1 (raw bytes / 255);
`COLOR_0` stays the old baked light so a build or switch that ignores the new
attributes draws the old picture. Constants found: PS `c0` = (0.8, 0.8, 0.8,
0.00806) (`DAT_140490c00`), `c5/c6/c7.w` = 0.5 (`FUN_1402a7880`).

**The point light is per vertex, as the program (2026-10-09, bombsq lane):** `SBTS_vs` writes o4 per vertex (READ: `add r1.xyz, v0, -cb0[7]` ... `mad o4.xyz, r1, cb0[0].w, cb0[0].w`) and `SBTS_ps` clamps the INTERPOLATED o4 (`max v4, -1` ...). Since `range / d^2` does not interpolate like a direction, ours now computes o4 in the vertex stage (`stage_material.wgsl::vertex`, carried in the colour varying's rgb of a bump draw) instead of per pixel. `PD_STAGE_POINT_PIXEL=1` = the old per-pixel point light. Test `the_point_light_is_interpolated_between_vertices_then_clamped`. Visible effect: small (a lit face whose corners are all near the light looks the same either way).

Built differently: the other vectors are made per pixel from the interpolated frame
(the program interpolates the tangent-space vectors per vertex; same maths, not
same interpolation, and the point light is exact per pixel); the original's
screen-door stipple (`cb1`) is the wall dissolve's job elsewhere; `cb0[16].x`
(alpha cut) and `cb0[23]` (kill flags) not located: Bevy's 0.5 mask stands.
Not built: the reflection family `SBTSREFS/P` (flag 0x400 + a third texture:
environment map, plain N.L instead of the LUT; these draws get the SBTS light
without the reflection, as before they had no reflection), `SBTSAL`, `SDREF*`,
and `SD` (vertex format 0x01) per-pixel nothing: SD is genuinely per vertex.
Format 0x01 + bump flag (11k triangles) stays baked.

## 3. Effect lights on the stage (`FUN_140199970`, `FUN_14010c990`)

For the stage's draw group (group 0) slot 2 is, while a LIGHT02 is alive, that
effect's record `0x80 + DAT_140527cc8`: the record allocated LAST (the update
allocates one per effect per view; they hold the same content). So the stage
sees one point light: the effect that started most recently, with the
fighters' own values (colour lerp(word 12, 13) x fade, range lerp(word 0, 1) = the
raw scalar, position = the emitter). `update_effect_light` writes it into every
stage material on change. Per view: the fighters use the per-view records
`0x60 + 2 view` (nearest two); the stage does not differ per view.
`PD_STAGE_EFFECT_LIGHT=0` off; harness `PD_STAGE_TEST_LIGHT=x,y,z,range,r,g,b`.
**Unverified**: the emitter height. The program uses the emitter node's own
position; our effect root is at the fighter's feet, so the light is lifted 10 units
(`PD_STAGE_LIGHT_Y`); the LIGHT02 piece's bone is not read.

## 4. Shadow volumes: exact for the stage, shadow maps for the rest

Read from the program, see also `SHADOWS.md`: the volume is drawn as stored for
`SSHADOWC` (fmt 0x10) and extruded by the vertex shader for `SSHADOW` (fmt 0x07)
along the light by the float at **material +0x3c** (`FUN_1402a3bd0`:
`cb0[4].w = *(float*)(material + 0x3c)`): st03 15 / 100 / 120 / 150, st04 12 / 25
/ 30 / 60, st05 60, st06 100 / 150 units, fighters' hulls 100. The pools of st03
(the big fmt 0x10 volume, 2,282 triangles) are **not** extrusions of anything: the
volume is axis-aligned blocks that cover the hall except a lit corridor, a pixel is
dark when it is INSIDE. A shadow map cannot say that (it darkens everything
behind the nearest caster): L2's maps over-darkened the lit corridor and could not
end an extrusion.

So the stage's own volumes (model draws, not placed objects) are exported
(`pdgrid_volumes`: one world-space triangle soup, extruded like `SSHADOW_vs`) and
`shadows.wgsl::volume_winding` tests every shaded point of the stage AND of the
fighters: a ray straight up, the sum of `sign(n.y)` over the triangles above it
(an xz grid of triangle lists, `shadow_grid.rs`, ~1.5 MB for st03) -- the stencil's
winding number; non-zero = shadow. Overlaps add and an inverted mesh gives -1,
both still non-zero, as the stencil counts. Placed objects' volumes (props) stay
shadow-map casters (an object can move; infinite length is their remaining error).
`PD_SHADOW_GRID=0` restores L2's all-casters.

The fighters' casters are now the authored hulls (19 per fighter, pc00a and
pc04a alike; the L2 doc's "20 on pc04a" was a miscount): rigid fmt 0x07 draws
on helper joints (flags 0x0004, name `ffffffff`), exported as `shadowhull_d<n>`
child nodes of their joints (they follow the baked clips); the body meshes stop
casting for a scene that has them. `PD_SHADOW_HULLS=0` = L2.

**The in-battle point-light variants are not resolved.** `FUN_140199ce0(1)`,
`FUN_140201b40` and two more set `DAT_14057f380 |= 0x500` on entering a battle
scene and swap `SWBOS`/`SSHADOW`/`SSHADOWCH` to `SWBOSP`.. / `SSHADOWP` /
`SSHADOWCHP` (extrude from `cP0_POS`, radially, by `cP0_POS.w` = the slot-2
light's scalar; `SSHADOWCHP` adds a `c0.z` bias). The static slot 2 of group 0 and
of the fighters is set 0's call-07 light (`DAT_1405a8720`, type 2, always on;
`FUN_140199e70` only replaces it with an effect light). For st03 that is the
origin with length 26.76, st05 the origin with 1e8, st06 a far point, st07 zeros;
read literally the fighters' shadows would radiate from the stage origin, which
the reference shots contradict (one steady direction, long shadows). Not built;
shadow direction follows the script's directional light. Open: what the
SBTS/SWBOS "P" tables actually bind in a match.

## 5. Findings that correct earlier docs

- FORMATS-STAGES "Lighting script": the argument layout and roles (§1); "the
  stage is lit per-vertex, not per-pixel" (that note read `SD`; format 0x03 is
  per pixel); "the effect-light table does not light stage geometry" is wrong
  (§3, `FUN_140199970`).
- P0-LIGHTING D5/D4/D8: confirmed; D8's hemisphere ramp `t = 0.5 + 0.5 N.axis`
  is right for the vertex shader (`FUN_1402a7050` scales the axis by 0.5 and
  sets w = 0.5) and the pixel shader (`h = 0.5 N.A + 0.5`).
- P0-LIGHTING D10: material flag 0x4000 appears in the volume draw class
  (`FUN_1402a52c0`: stencil mode / `+0x11 & 3`), not as a shading flag: left alone.
- CHAR-LIGHT.md: the direction and axis of the fighters' light were read in file
  order too (fixed through `script_vec`).
- SHADOWS.md: L2's "dir along set 0 for static volumes", "not extruded, axis-
  aligned edges" are explained by §4; its fighter hull count of 20 on pc04a is 19.

## 6. Measured

Sheets (local, not committed): `engine/shots/l3_ab_{st03,st05,st06,st07,all}.png` (before = main's build +
cache, after = this lane + scratch cache, same views, `PD_SHOT_TICKS=400`), `l3_palace_floor_crops.png`
(Lost Palace 1/2/3 above, ours before / after in three camera poses -- matched by view, not registered),
`l3_bump_onoff.png`, `l3_grid.png`, `l3_hulls.png`, `l3_effectlight.png`, `l3_4v_sheet.png`.

- **Mean luma** (sRGB, Rec.601, whole two-view frame): st03 52.9 -> 62.7 (the three Lost Palace references:
  52.7 / 52.9 / 54.0), st05 49.2 -> 50.3, st06 33.4 -> 39.1, st07 29.1 -> 32.2. Where it comes from: the
  corrected light direction (st03: the lit corridor stops being darkened by the old shadow-map strip) and
  the SBTS port alone (`PD_STAGE_LIGHT=0` on the same cache against 1): +0 / +5 / +9 / +13 %, the
  program's own sum, nothing scaled. Floor crops of the three references 62-75, ours 72-89.
- **Switch identity**: `PD_STAGE_LIGHT=0` on the unchanged main cache against main's build: 0.034 % of
  the pixels of a whole frame differ, the same as two runs of main's build against each other (0.039 %,
  particles); the floor crops differ in 0 pixels.
- **Frame time** (st03, 500 frames from 200, `PD_FRAMETIME`, vsync off but the presenter holds ~60 fps),
  three repeats each: two views main 17.7 / 19.4 / 17.9 ms, lane 17.6 / 38.2 (one outlier) / 17.4;
  four views main 31.3 / 31.2 / 30.5, lane 27.2 / 30.6 / 23.9; during a Psycho Knife cast (the effect light
  rewriting every stage material each frame) main 31.5 / 24.4, lane 30.7 / 23.6. No measurable cost
  against the noise of this machine; the stage materials' storage binding, the grid lookup and
  the per-pixel shader are paid for by dropping the stage volumes from the shadow maps.

## 7. Switches

`PD_STAGE_LIGHT=0` = the lane off, as before it: no per-pixel stage light, no
volume grid, no hull casters, no stage effect light, and the light script
vectors read in file order. `PD_STAGE_EFFECT_LIGHT=0`, `PD_SHADOW_GRID=0`,
`PD_SHADOW_HULLS=0` switch the parts. Logs: `PD_STAGE_LIGHT_LOG=1`.

## 8. Regenerate the caches this lane needs

Stages (the bump attributes, the grid node, the re-baked light) -- with the
default collision (the program's 0x94 mesh + object hulls + class trailer;
`--collision dat+walls` was retired 2026-10-03, COLLISION.md "Per-triangle class"):
`pd_convert stages "<game_files>\Assets\Data\Map" "<cache>"`
Characters (the hull nodes), per id: `pd_convert character
"<Player>\<id>\<id>.alr" "<cache>\chars\<id>.glb"` for the 25 ids that have a glb.
