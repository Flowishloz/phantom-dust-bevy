# Shadows (lane L2, 2026-10-01)

Code: `pd_game/src/shadows.rs` (+ `shadows.wgsl`), `stage_material.rs` (+ `.wgsl`), the
one-line fighter hook in `dissolve.wgsl`, the volume export in
`pd_formats/src/gltf.rs` (`volume_node_name`, `VOLUME_TAG`). Tests: `shadows::tests::*`,
`gltf::collision_filter_tests::{volume_node_names_are_tagged_and_distinct,
shadow_volumes_exist_on_exactly_these_stage_files}`. Evidence sheets (local, not committed):
`engine/shots/l2_*.png`.

## 1. What the original does (read from the program, 2026-10-01)

- **Volumes.** Stage draws with material flag 0x2000 and vertex format 0x07 (position +
  normal, shader `SSHADOW`) or 0x10 (position only, `SSHADOWC`); the fighters carry 19 hull
  draws each (20 on pc04a: fmt 0x07, flags 0x200b, one rigid draw per helper joint, 48-192
  triangles, **not** skinned). `SSHADOW_vs`: `pos += DL.xyz * DL.w` for vertices whose normal
  satisfies `N.(-DL) < c0.x` (c0.x = 0); `sshadowch_vs` first adds `DL.xyz` (a one-unit bias).
  The `*C` variant (fmt 0x10, st03's big model-0 volume) is **not extruded at all**; it is drawn as
  stored (axis-aligned blocks). Pixel shaders write black + the glow mask and discard on `alphakill`.
- **Pass** (`FUN_140299310`): stencil on, colour writes off, depth test on / depth write off,
  `INCR_WRAP` on depth-fail, volumes drawn; then a full-screen quad (colour `DAT_14057f3bc`)
  with stencil `NOTEQUAL 0`, RGB writes on, blend `SRC = DEST_ALPHA, DST = INV_DEST_ALPHA`
  (state word 0x7601 through `FUN_14028e4e0`): `out = C*A + dst*(1-A)` in gamma space.
- **C = the shadow colour** = script call `02` sub 1's colour (`DAT_1403e8ba8` -> `DAT_14057f3bc`
  in `FUN_140199a30`; handler `0x1402068a0`). st03 (15,18,25), st04 (19,19,19), st07 (3,0,14),
  st08 (2,2,3), st23 (23,23,25), black on st05. (`stagedat` had parsed it as "backdrop".)
- **A = the alpha every surface pixel shader writes**: `SD_ps`, `SBTS_ps`, `SWBOS_ps` all end
  `o0.w = 2*(sat(lut_or_vertex_NL)*0.8 + 0.00806)*(2*tex_alpha - 1.5)`, so for an opaque
  texture `A = 0.8*N.L + 0.00806`: darkening scales with how directly the surface faces the light
  (a floor under st03's light, N.L 0.677, is darkened by A = 0.55).
- **Light.** Stage volumes use light group 0 (`FUN_140199970`), fighter hulls group 1
  (`FUN_140199e70`, `DAT_14057f408` = record 0x49).

## 2. What is built, and why shadow maps instead of a true stencil

The game renders 4x MSAA on Bevy's `Depth32Float`: no stencil plane, multisampled depth. A true
z-fail pass means changing the depth format of every pipeline (fighters, stage, effects,
dissolve, fx) or a custom multisampled accumulation pass in the render graph, with the
two split-screen views, the wall dissolve and the glow of lane 3 all sharing the depth buffer.
Bevy's directional shadow maps do the same test (is this surface behind a caster along the
light?) and are supported with MSAA, skinned meshes and the dissolve material.

- The authored volumes are exported as named glb nodes (`shadowvol_m<model>_d<draw>`; for placed
  objects `<propNNN_name>_shadowvol_d<draw>`, children of the object's node so they move, hide
  and vanish with it). They sit on render layer 7 (no camera draws them) and are the stage light's
  only casters. A re-exported stage glb is also safe with `PD_SHADOWS=0` (volumes forced hidden).
- Fighters' own body meshes (the dissolve pieces) cast from layer 8 (their light = set 1's direction).
- Receivers apply the original's blend with the shadow-map lookup standing in for the stencil
  test: `stage_material.wgsl` (the authored `StandardMaterial` + shadow blend; unlit, vertex colours,
  alpha cutout, fog, tone mapping untouched) and `dissolve.wgsl` (fighters). The weight `A` uses the
  face normal (stage) / mapped normal (fighters) against the light; the colour is the stage's.
- **No global dimming**: outside a shadow `k == 0` and `shadow_darken` returns its input. Proof: `l2_identity_st06.png`,
  a 400x400 unshadowed floor crop, shadows ON vs OFF: 0 differing pixels (max diff 0); over the whole
  view 91.9% of pixels are bit-identical, the rest are the shadows.

## 3. Measured

- **Darkening.** Original (Lost Palace 2, hand-sampled across a fighter-shadow edge on one tile):
  shadow / lit luma 0.45-0.52. Ours (Palace floor, sRGB luma, on/off at the same pixels, all
  darkened floor pixels of `l2_bh1_*`): median 0.60, p10 0.57. Model value: `(1-A) + A*C/dst`
  with A = 0.55 -> 0.56 at lit luma 80. The ~10% gap is the face normal standing in for SBTS's
  bump-mapped N.L (lane 3) and the kept tone mapper; the constants are the program's, nothing was tuned.
- **Direction.** The stage's script directions differ between set 0 and set 1 on 17 of 28 stages
  (e.g. st07 (0.87,-0.39,-0.30) vs (0.73,-0.65,-0.22)); each shadow light uses its own set.
  Hard-edged, translucent, bluish shadows with the floor texture showing through, lit strips with straight
  edges: same character as Lost Palace 1/2/3 (`l2_ref_compare.png`, `l2_palace_floor_crops.png`;
  the camera is matched by view, behind the fighter in the same hall, not pixel-registered).
- **Frame time** (two views top/bottom, 1600x900 window at 2x, `PD_FRAMETIME`, vsync request off but the
  presenter still holds ~60 fps; 500 frames each, alternated, 3 repeats): st03 off 17.0/17.0/17.2 ms mean,
  on 19.0/18.0/19.1 (p95 20.8 -> 25.7-28.9); st04 (largest volume set) off 17.9/16.9/20.7 (noisy), on 18.1/19.2/18.4.
  About +1-2 ms per frame (+6-11%), p95 +5 ms. Not measured with four views (not built).
- Shadow map 2048 px, stage light 4 cascades to 700 units, fighter light 3 cascades to 300
  (`PD_SHADOW_MAP`, `PD_SHADOW_DIST`).

## 4. Findings that correct the P0 lighting audit

- Volumes are **not only** on the audited stages: st06/st16/st26 (Refinery, 62 volume nodes placed in
  the stage glb: crates, containers, props) and st25 (22 volume draws in object templates that no
  placement uses) carry them too. 20 of 30 stage files have volume draws; the audit's list lacks those four.
  Most stages also carry volumes in **object templates** (st03 has 94 volume nodes, only 1 is the stage's own model).
- "Set-0 light direction" is only true for the stage's own volumes: fighters use set 1, whose direction
  differs on many stages. The audit's "same directions" note (FORMATS-STAGES) is wrong for 17 of 28 stages.
- The audit's "dir along set 0" for static volumes: st03's model-0 volume (fmt 0x10) is not extruded
  and its edges are axis-aligned, not parallel to the light.
- **Open, not settled:** while a battle light exists (`DAT_14057f380 |= 0x500`, set in
  `FUN_140199ce0(1)` / `FUN_140201b40`) the program swaps `SSHADOW`/`SSHADOWCH` to the **point-light**
  variants (`SSHADOWP_vs` / `SSHADOWCHP_vs`: extrude from `cP0_POS` = the slot-2 light record's position
  `+0x30`, length from the draw record's `+0x3c`). The static slot-2 position comes from script call 07
  (mostly (0,0,0) with zero colour, or an effect light while a skill's light plays). The reference screenshots
  show shadows falling in one steady direction, which matches a directional light; the point-light
  reading would put shadows radially away from the stage origin. Not resolved here (the extrusion length
  `+0x3c` and which record is live in a match are unread), so shadow direction follows the script's `dir`.
- Fighter hulls (R4): 19 rigid hull draws per fighter exist (listed above; pc04a also 19, not 20).
  Used from lane L3 on (STAGE-LIGHT.md §4).

## 4a. Lane L3 changes (2026-10-01, docs/STAGE-LIGHT.md)

- The shadow direction was mirrored (the light script's vectors are stored (z, y, x)): fixed in
  `ShadowLook::from_json` through `stage_look::script_vec`.
- The stage's own volumes are no longer shadow-map casters: `pdgrid_volumes` + a winding-number test
  (`shadow_grid.rs`, `shadows.wgsl::volume_winding`) gives the stencil's inside-the-volume semantics,
  extruded by the material's own length (float at material +0x3c: 12..150 units). This fixes st03's
  window-light pools (the fmt 0x10 volume is stored geometry, not an extrusion) and ends the
  extrusions; placed objects' volumes still cast through the map.
- The fighters cast from the authored hulls (19 per fighter).
- The in-battle `SSHADOWP/CHP` variants are still open: STAGE-LIGHT.md §4.

## 5. Re-export (the glbs in the shared cache have no volumes yet)

`pd_convert stages "<Map dir>" "<cache dir>"` regenerates every stage glb (about 40 s); only
`st*.glb` changes (bounds and collision files are byte-identical, verified on all 30 stages). Nothing else
is needed: without the volumes the game still runs (no stage casters, fighters still cast).
This lane's runs used `%USERPROFILE%\.pdtools\cache-shadows` (a copy of the cache: top-level files copied, chars/fx/sfx/ui junctioned, stage files regenerated).

## 6. Not built / unconfirmed

Extrusion length (`+0x3c`) and the point-light battle variants (above); authored fighter hulls; shadows from
skill-effect meshes (original: none known); per-pixel bump N.L in A (lane 3, `SBTS_ps`); four views.
Switches: `PD_SHADOWS=0`, `PD_SHADOW_MAP`, `PD_SHADOW_DIST`, `PD_SHADOW_FILTER=gaussian`,
`PD_SHADOW_BIAS`, `PD_SHADOW_NBIAS`, proof harness `PD_SHADOW_CAM`, `PD_SHADOW_CAM_AT`, `PD_SHADOW_LOG`, `PD_FRAMETIME`.
