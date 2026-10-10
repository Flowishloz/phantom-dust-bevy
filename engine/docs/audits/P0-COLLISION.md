# P0 audit: COLLISION (ground / wall / step) -- 2026-09-30

Read-only audit. Trigger: Laurence, "the stairs in Lost Palace (st03) can't be
walked up; only jumping climbs them; in the original you walk up stairs."
Nothing was run in the game; nothing in the repo was changed except this file.

## 0. Verdict in five lines

1. **Reproduced, with a cause that is not step height.** The Palace's two big
   stair flights (x -110..-90 and x +90..+110, export z -125 .. -233, y 50 ->
   109) have risers of only 2.0-3.1 u -- far under our 6 u limit. Our render-
   geometry collision has **floor-coverage gaps** between treads (the step
   bevels are steeper than our 0.6 floor limit, so they count as walls). On the
   first gap the body drops into the hollow under the stairs, then a riser
   shoves it back, every tick, for ever. Only the bottom ~3 steps trap;
   from step 4 up the same walk climbs to the top.
2. **The original does not collide with those steps.** Its stage collision
   (the `.dat` header-0x94 block, already parsed for footsteps) has a **smooth
   30 degree ramp** over the same flight, with no tread/bevel gaps. Our own
   movement rules, run over that 0x94 ramp, climb the whole flight to the
   balcony (y 108.9) in both gaits.
3. **The fix is data first, and can run now without touching `sim.rs`**:
   build the stage part of `.pdcl` from the 0x94 block (minus class 0x1F)
   instead of ALR render geometry.
4. Separately, our step/ground constants and classification differ from the
   program's (step window, snap distance, signed floors, slope limit); those
   need `sim.rs` and belong in lane B after the four-player lane.
5. Survey: the same gap trap is confirmed on st03 only; st07 and st08 have one
   suspected each; the other flagged spots are "render has a wall where the
   original has open ramp" and need eyes, not code (section 2, D9).

Caveat on "reproduced": done with a line-for-line **Python replica** of
`collision.rs` (`ground_below`, `push_out_of_walls`) and `sim.rs:776-810`
(movement block) reading `engine/cache/st03.pdcl`, not a Rust run (cargo is not
ours to run). Lane A's first test must reproduce it in Rust before any fix.

## 1. Ledger

Legend: Y = yes, N = no, ~ = partial.

| Feature | Mapped | Implemented | Reproduced | Verified | Approved | Evidence |
|---|---|---|---|---|---|---|
| Original stage collision = header-0x94 block (faces with stored normals + class byte) | ~ strong inference: node list walked in `FUN_140255020`, `FUN_140256390` has node +2 = class, footstep class byte proven to be the 0x94 hull flags (COLLISION.md "runtime footstep class byte"). Builder of the runtime list from the block not traced | ~ parsed (`pd_formats/src/surface.rs`, sidecar `stNN.surf.json`), **not used for movement** | Y (st03 cross-section below) | ~ top-down floor coverage of 0x94 vs `.pdcl` matches on st03 (both cover the same hall/wings/tree hole); 0x94 has 810 floor tris vs 19,754 | N | see section 2 D1 |
| Stairs are a smooth ramp in the original's collision | Y (st03 x=-100: 0x94 ramp top from (z -125, y 50) to (z -233, y 109), slope tan 0.577 = 30 deg, stored normal.y 0.866) | N | Y | Y (our rules on 0x94 climb it, 2 gaits) | N | `sec_094.png` vs `sec_pdcl_stage.png` (scratch) |
| Step-up rule: grounded body ignores wall triangles whose three vertices all lie below feet + 7.5 | Y `FUN_1401959c0` / `FUN_140195580` (args confirmed at call sites `0x1402554cc`, `0x14025561a`: param_5 = height*0.5, param_6 = grounded flag `[obj+8]`); constant `0x1402f7f58` = 7.5 | ~ ours: wall samples start at feet + 7 (`sim.rs:782`, `STEP_HEIGHT+1`) and need `dy < 1` | Y (riser top 6.46 above the dropped feet blocks, st03) | N | N | D3 |
| Floor window: floors with height in [feet - 7.5 - horizontal travel, centre + 9) = up to **feet + 18** when grounded; [feet, feet + 18) in the air; highest wins | Y `0x1402558a9` / `0x140255874` (disassembled), constants 7.5 / 9.0 at `0x1402f7f58` / `0x1402f7f64`, body half-height `height*0.5` (`0x1402f7d40`) | ~ ours: highest floor <= feet + 6 (`sim.rs:784`), snap down only if <= 6 below (`sim.rs:794`) | Y | N | N | D3 |
| Slope limit: a face is ground only if stored normal.y > 0.643 (`0x1402f7d50`); faces with normal.y <= 0.1 never (`0x1402f7cdc`); signed, so undersides are never ground | Y (`0x140255811`, `0x1402558c1`) | ~ ours `WALKABLE_NORMAL_Y` 0.6 on **abs(normal.y)** (`collision.rs:17,151`), so ceilings stand | Y (30-50 % of our floor-class triangles are undersides, per-stage table in D2) | N | N | D2 |
| Body shape: sphere-vs-triangle push, radius 5 (`0x1402f7f18`), substeps <= 2.5 (`0x1402f7eb8`) against **every** triangle (floors and ceilings too); plus 3 probe rays at centre, centre -/+ (half-height - 5) | Y `FUN_140255e00` -> `FUN_140195f30` | ~ ours cylinder, radius 4.5, samples 7 / 13.5 / 20 (`sim.rs:108-109`, `collision.rs:217-245`), no ceilings ("Known gaps") | N | N | N | D5 |
| Grounded -> airborne: stays grounded until the feet are more than 5 u below the last ground height (`FUN_140254590`, `0x1402f7f18`) | Y | ~ ours 6 **ticks** of coyote time (`sim.rs:83`) | N | N | N | D6 |
| Class 0x1F faces are never used by the body or rays; class 0x14 faces ignored while state is 5 or 7 | Y (`FUN_140255020`, `FUN_140255b70`, `FUN_140163950`, `FUN_140164770`, `FUN_140256210`, `FUN_140256390`) | N (no face classes in `.pdcl`) | Y (309 class-0x1F faces in st03) | N | N | D4 |
| Edge stop: in states 6, 10, 13, 25, 26, 28 (mask `0x16002440`) a body with no floor ahead is put back at last tick's position instead of falling | ~ read in `FUN_140254590`, states not named (cast / stunned / timers per PROGRAM-RE list) | N | N | N | N | D7, Reads |
| Large movement (> 12.5 u per 30 Hz tick) takes a ray path (`FUN_140163950`) | Y | N needed? our fastest mover (cartwheel 180 u/s = 6 u per 30 Hz tick) stays under | - | - | N | none |
| Other ground movers: crawlers "follow the floor within +/-7.5 u, no floor ends, wall stops" (PROGRAM-RE line 304) | ~ | ~ `combat.rs:2168` uses 6 u and a ray from y + 3 | N | N | N | D8 |
| Shadow volumes / sky / oversized patches excluded from our collision | Y | Y | Y | Y (COLLISION.md) | ~ | already right; 0x94 data makes these filters unnecessary |
| Surface class for footsteps from the same mesh | Y | Y (`surface_ground.rs`) | Y | Y | Y (DECISIONS 2026-09-29) | unaffected |

## 2. Discrepancies (most important first)

**D1 -- wrong data source. CONFIRMED (both sides read). Root cause of the
report.**
- Original: bodies and rays query the world face list (nodes: +0 kind 3, +2
  class, normal +0x38, vertex pointers +0x48/+0x50/+0x58), filled by
  `FUN_1401d5860`; the same list that carries the footstep class byte, i.e. the
  header-0x94 block. On st03 that block holds the flight as one smooth ramp.
- Ours: `.pdcl` stage part = every ALR render triangle
  (`pd_convert/src/main.rs:1258 stage_collision`, `gltf::world_triangles`).
  The flight is ~25 real steps (tread flats 2.1-4.3 u deep, risers 2.0-3.1 u,
  mean ~2.5) plus a soffit (underside) 12 u below.
- Cross-section (scratch, x = -100): pdcl shows the sawtooth and the soffit;
  0x94 shows one straight line 50 -> 109 over the same z span and the soffit.
- Mechanism of the trap (replica, tick log at x -100, z -128, run speed):
  1. Each tread is followed by **two short bevel faces** (st03 x -100, first
     gap z -132.30 .. -133.10: |normal.y| 0.45 and 0.50). Below our 0.6 floor
     limit they go into the wall set, so `ground_below` has **no floor** for
     that 0.8 u band. 11 such bands along x = -100 (widths 0.15-1.05 u).
  2. With no floor, `ground_below(..., feet + 6)` returns the **hall floor
     y 50.0 beneath the hollow stairs**, 1.9 below the feet: the "stick to the
     floor when <= 6 below" rule (`sim.rs:794-796`) snaps the feet down to 50.
  3. At feet 50 the wall sample height 7 (`sim.rs:782`) reaches y 57, the top
     of the next riser (z -136.61, top 56.46): `dy 0.71 < 1`
     (`collision.rs:236`), so the body is pushed back 1.45 u to z -132.11.
  4. There the tread (51.93) is found again; the next tick repeats. Logged
     oscillation: z -132.11 <-> -133.04, y 51.93 <-> 50.0, forever (run and
     walk; x = -95, -100, -105). Higher gaps drop > 6 u (6.3-10.5), so the
     snap is not triggered and the 6-tick coyote bridges them: starting on the
     4th/6th/12th tread the replica walks to y 109.2 and the wall at the top.
     Down the flight works in both gaits.
  5. A jump (dodge jump, 85 u/s) clears the bottom gap -- matches "only
     jumping climbs them".
- Not a stair-height problem: riser 2.0-3.1 vs limit 6; not an object
  (`stage-only` triangles trap identically); not a missing-faces problem.
- Expected visible effect of the fix: the stair flight walks smoothly, up and
  down, at any x across its 20 u width; the body rides a straight 30 degree
  line instead of stepping (feet follow the ramp, as in the original, where the
  walk clip shows no stair steps either -- unconfirmed by footage).
- Only this pair of flights traps. The smaller flights on the z > 0 side (x
  about -81, -54, +54, +74 ... z 190-270) walk fine in the replica (their
  bevels are >= 0.6).

**D2 -- floors are unsigned, ceilings stand. CONFIRMED (code read, counts
measured).** Ours accepts `abs(normal.y) >= 0.6` (`collision.rs:151`), "either
winding". The program needs a stored normal with y > 0.643 (ground) or > 0.1
(height candidate) and never stands on undersides. Counted in the `.pdcl`
(stage triangles, export winding where up-facing has negative computed y):
floor-class triangles that are undersides: st01 52 %, st02 31 %, st03 41 %,
st04 40 %, st05 43 %, st06 40 %, st07 32 %, st08 36 %, st09 48 %, st24 38 %,
st26 41 %. Visible effect is only where a floor and an underside lie within
6 u (low canopies, stair soffits, tables); it feeds the st03 trap (the "hollow
floor" caught under the stairs can be the soffit). SUSPECTED elsewhere, not
demonstrated.

**D3 -- step window. CONFIRMED (read + disassembly).** Program: wall triangles
below feet + 7.5 do not block a grounded body; floors accepted up to feet + 18;
snap-down reaches 7.5 + |horizontal move of the tick| below. Ours: 6 / 6 /
6, and the wall sample at 7. With the data fix (D1) none of this matters on
st03 (the ramp is continuous). It matters for real ledges 6-7.5 u high, which the
original walks up and we do not. SUSPECTED visible effect: on stages with
6-18 u kerbs/ramps the original is more permissive; not yet measured.
Note the tick-rate trap: the program's 7.5 + |move| is per 30 Hz tick; our sim is 60 Hz,
so "|move|" must be the 30 Hz tick's travel (2 x our per-tick distance).

**D4 -- face classes. CONFIRMED (read both sides).** Program ignores class 0x1F
everywhere (body, rays, sphere pushes) and class 0x14 in states 5 and 7. Ours
has no class; st03 has 309 class-0x1F faces (5 floor / 278 wall / 26 ceiling
by stored normal). Not part of the st03 bug, but any move to the 0x94 data
must drop them for body and ray queries. Who does use 0x1F (camera? a
trigger?) is unread (Reads).

**D5 -- body shape. CONFIRMED (read).** Sphere radius 5 + 3 probe rays vs our
cylinder radius 4.5 with samples 7/13.5/20; no ceiling contact in ours. Effect:
slightly different wall clearance (0.5 u) and head bumps under low
ceilings; SUSPECTED minor.

**D6 -- leaving the ground. CONFIRMED (read).** Program: drop of more than 5 u
below the last ground height; ours: 6 ticks of coyote time. At run speed 6 ticks
is 5.6 u of travel, so gaps up to 5.6 u wide are bridged, where the program
bridges by height, not time. Effect: edge walk-off feel; SUSPECTED minor.

**D7 -- edge stop. SUSPECTED (read, states unnamed).** Mask `0x16002440` =
states 6, 10, 13, 25, 26, 28: no walking off ledges in those states (position
restored). Ours lets casting/stunned bodies slide off (lunges, `combat.rs:1650`,
only push out of walls, never follow the floor at all). Needs the states named.

**D8 -- other movers. SUSPECTED.** Crawlers: program +/-7.5 u, ours 6 u plus
the same gap problem (`combat.rs:2168`). Capsule spawns
(`combat.rs:2369-2372`) and jump zones (`sim.rs:446`) use 6 u. Lunges keep
the old y (`combat.rs:1650`).

**D9 -- survey of other stages (proxy, NOT validated).** Method: for every
large up-facing 0x94 ramp (area >= 100, slope 0.64-0.985, rise >= 4) start on
the floor 6 u below its low edge at four positions across, walk uphill at run
speed with the replica on (a) the full `.pdcl`, (b) stage triangles only,
(c) the 0x94 mesh (minus 0x1F). Counted: starts where (c) reaches >= 90 % of
the ramp and (a) stays under 60 %.

| Stage | Starts tested | Blocked on render only | of which gap-trap pattern | blocked by an object hull | other ("wall on the render ramp") |
|---|---|---|---|---|---|
| st03 / 13 / 23 | 30 | 15 | **13** (both flights) | 2 | 0 |
| st07 / 17 | 268 | 2 | 1 (-9, 105, 164 area, rise 18) | 0 | 1 |
| st08 | 1271 | 36 | 1 (506, 149, 53) | 4 | 31 |
| st02 / 12 / 22 | 315 | 46 | 0 | 28 | 18 (road ramps, rise 14-46) |
| st06 / 16 / 26 | 159 | 32 | 0 | 1 | 31 (decks 270-370, rise 30-45) |
| st05 / 15 / 25 | 76 | 5 | 0 | 0 | 5 |
| st04 / 14 | 46 | 2 | 0 | 0 | 2 (x +-188, y 147, rise 29) |
| st01 | 206 | 2 | 0 | 2 | 0 |
| st09, st24 | 213 / 20 | 0 / 0 | - | - | - |

"Other" means the render geometry has a wall on the ramp path that the 0x94
mesh does not (rails, barriers, the ramp edge): **not a stair bug until someone
looks at it**; the original probably lets the body walk there. These are leads
for a picture sheet, not findings. No stair-like sequence in any stage has a
riser above 7.5 that we could detect (st04's stepped ramps have risers 5-6, and
walked fine); so the "riser exceeds our limit but not the original's" case is
**not shown** anywhere; the only confirmed stair failure is the gap trap.
The scan method (ramp-base starts) can miss stairs with no matching 0x94 ramp.

## 3. Reads still needed

1. **How the world node list is filled from the 0x94 block** (builder of
   node +0 = 3, +2 = class, +0x38 normal, +0x48.. vertex pointers; the
   `FUN_1401d5130` / `FUN_1401d5530` side of `FUN_1401d5860`, called with
   filter argument 0 by the movement code and 1 by a few other callers, at
   `decomp_all.c` lines 312526, 452149, 453288, 461173). Settles "is 0x94 the only
   world list" and what filter value 1 selects. Until then D1 is a strong
   inference (same footstep class byte, same footprint, smooth ramp) not a proof.
2. **Who consumes class 0x1F** faces (309 in st03, mostly walls/ceilings):
   bodies, rays, sphere pushes and the footstep probe skip them; camera
   occlusion (`raycast` here) and lock-on line of sight may not. Start at the
   camera code and the `param_2 == 1` callers of `FUN_1401d5860`.
3. **Names of states 6, 13, 28** for the edge-stop mask; `FUN_140256e80`
   dispatch and `state_table.py`.
4. **Whether the original's walk up the Palace stairs looks like a ramp**
   (feet following a straight line, no step bob): footage check
   (`FOOTAGE.md`, none of it covers the stair flights as far as grepped).
5. `FUN_1401959c0` / `FUN_140195580` internals beyond the gate (what the
   `param_3` flag "hit a face with normal.y < -0.643" drives, the vertical lift
   `param_1[1] += ...`): SUSPECTED read only; a lane that ports the rule
   needs a disassembly pass (capstone script is ~20 lines; no Ghidra).
6. Object hulls: whether `OC00` hulls join the same runtime list as the
   0x94 faces (COLLISION.md "unconfirmed"); we keep them either way.

## 4. Proposed build lanes (order: A, then B; C optional)

### Lane A -- "original stage collision" (data + tests; **no `sim.rs` change, can run now**)

Goal: the st03 flights (and every stage's ramps) walk as in the original by
making the stage part of `.pdcl` the 0x94 mesh.

Owns (no overlap with the four-player lane):
- `crates/pd_formats/src/surface.rs`: keep the stored normal per face
  (`SurfaceMesh::normals`), add `triangles_for_collision()` = faces with class
  != 0x1F, wound so the cross-product normal equals the stored normal
  (export space: stored normal.y is opposite to the winding-derived sign on
  99.5 % of st03 faces, so a flip is needed and must be checked).
- `crates/pd_convert/src/main.rs`: `stage_collision` (line 1258) and
  `stages` (line 1090-1165): stage part from that mesh, object hulls and the
  `OBJT` trailer unchanged; keep the `.surf.json` sidecar. Keep the old
  ALR-based builder behind a switch (`--collision alr`) for A/B and for stages
  with no usable `.dat` (st00, st09_01..08).
- `crates/pd_formats/src/collision.rs`: for stage triangles (owner
  `NO_OBJECT`) classify floors on the **signed** normal (`>= 0.643`), ceilings
  never floors; objects keep `abs` until their winding is checked. Change
  `WALKABLE_NORMAL_Y` 0.6 -> 0.643 with the signed test only.
- New test file `crates/pd_game/src/stair_walk.rs` (`#[cfg(test)]`, one
  `mod stair_walk;` line in `main.rs`; it uses only `Match::new` and
  `Match::step`, both `pub`).

Tests (from reads, not from old behaviour):
1. **Reproduce first (Rust, before the change):** with the current `.pdcl`,
   `Match::new([(-100, 50, -118), far], Combat::new(vec![], vec![]))`, a
   forward stick (`Input::from_axes(0.0, 1.0, false, false)`, camera yaw
   toward -z, start after `counting_down()` ends or call `step` past tick 180):
   assert it fails to pass z -134 in 300 ticks (this pins the bug).
2. **After:** same script reaches `pos.y >= 108.5` and `pos.z <= -230` within
   about 150 ticks at run speed (59 u up over ~108 u: 108 / 55.6 s = 117
   ticks at 60 Hz), both gaits (walk 20.8 u/s: ~320 ticks), at x = -95, -100,
   -105 and mirrored x = +100; and the return walk down.
3. Program-pinned data test: st03 0x94 ramp top at x = -100 has
   `ground_below(-100, z)` within 1.5 u of `49.9 + 0.577 * (-125 - z)` for z
   in [-226, -130] (straight ramp; cite `0x140255811` slope rule for why
   it is ground: stored normal.y 0.866 > 0.643).
4. No class-0x1F face is in the stage `.pdcl` (309 for st03), and no
   underside is a floor (`layers_at` over the soffit returns nothing above
   the hall floor at x -100, z -200 except the ramp).
5. **Wall behaviour unchanged elsewhere:** `walkprobe --compare` (existing tool,
   `crates/pd_convert/src/walkprobe.rs`) before/after for all 30 stages: same or
   more start pairs connected, reachable area within a stated tolerance, no new
   void-edge holes beyond a documented list (as COLLISION.md's table). Plus the
   existing `propcheck` counts (object hulls unchanged).

Runtime proof: `pd_game --stage st03` with a **new test-only spawn switch**
(`PD_TEST_SPAWN=x,z` for P1, one line next to the spawn logic at
`main.rs:659`; `--near` cannot place a player at the stairs), plus
`PD_TEST_STICK=1.0,0` and `PD_SHOT_TICKS=200,260,320,380,440` (plain numbers
count from the match start, which includes the 180-tick countdown; the stick
should start at tick 180): captures of the start, the first three steps (the
trap), mid-flight, the balcony; before/after strip of
P1's view, per `pd-visual-verify`. Log with `PD_MOVE_LOG` (P1's y should rise
monotonically while z falls).

Risks: (a) 0x94 may omit real blockers that the render mesh has (rails,
barriers): the survey's "other" column lists 100+ places to look at on a
picture sheet; (b) 0x94 may lack floors in places (Dawn City gaps, Sein
islands, Highway): covered by `walkprobe --compare`; (c) camera
occlusion and projectile wall tests use the same `Collision`: dropping class
0x1F may change them (Read 2); (d) the original's invisible arena walls, if
any, are then included for free, which would change where players can leave
the arena (COLLISION.md "Known gaps" suspects exactly this).

### Lane B -- "the program's ground rule in the sim" (after the four-player lane releases `sim.rs`)

Goal: port `FUN_140255020`'s numbers (D3, D5, D6, D7) so ledges, snap-down and
edge walk-off match, independent of data.

Owns: `crates/pd_formats/src/collision.rs` (new `Collision::move_body(prev, next,
grounded, half_height, radius) -> (pos, grounded, ground_class)`: floor window
`[feet - 7.5 - |dxz30|, feet + 2*half_height)` highest first, signed normal >
0.1 to take the height and > 0.643 to set grounded; wall gate "vertex above
feet + 7.5 or airborne"; radius 5; ceilings block; class 0x14 gate by state);
`crates/pd_game/src/sim.rs` lines 776-810 (replace push + ground +
coyote with the call; 5 u height rule instead of `COYOTE_TICKS`);
`combat.rs` 2168 (crawler) and 2369-2372 (capsule spawn) to reuse it.
Needs Read 5 (disassemble `FUN_1401959c0` / `FUN_140195580` bodies) first.
Tests cite the addresses: window constants 7.5 / 9.0 / 0.643 / 0.1, 5 u drop;
runtime proof: scripted `PD_TEST_STICK` walks over a 7 u kerb (walkable in the
program, not in ours today) on a stage with one (to be found in lane A's
survey list), log of feet y per tick.
Prerequisite: Lane A merged (otherwise the new rule would run on render
geometry and some step bevels would behave differently).

### Lane C (optional, small) -- survey sheet

Picture sheet of the ~100 "other" survey spots (render vs 0x94 cross-section
at each, in the `walkprobe --map` style) so Laurence / the lane can tell
rails from real blockers. Owns: a new `walkprobe` switch (`--ramps`) in
`crates/pd_convert/src/walkprobe.rs` only. The scan method used here was a
throwaway Python replica and is not in the repo.

## 5. Questions only Laurence can answer

1. Which staircases did you mean in Lost Palace?
   (a) the two long flights at the north/south ends of the hall, up to the
   high balcony, (b) the smaller flights along the sides, (c) both.
   (My replica only fails on (a); if you meant (b) we have more to find.)
2. Besides Lost Palace, on which maps did you see walking blocked or odd
   collision? (a) Lane, (b) Sein, (c) Refinery, (d) Highway, (e) other --
   the survey flags those four stages most (Sein and Refinery by far the
   most), so naming them tells us which picture sheets to make first.
3. In the original, when a fighter runs up the big Palace stairs, does the
   body glide up a straight slope, or do you see the feet step?
   (a) glides like a ramp, (b) visible stepping, (c) can't tell.
   (Decides whether "the original uses a ramp" needs a footage-based
   confirmation before lane A ships.)

## Appendix: how the numbers above were obtained (scratch, not in repo)

- Program: `decomp_all.c` functions `FUN_14024ec80` (per-tick player update,
  calls `FUN_140254590` -> `FUN_140255020`), `FUN_140255e00`, `FUN_140255b70`,
  `FUN_1401959c0`, `FUN_140195580`, `FUN_140195f30`, `FUN_1402559f0`,
  `FUN_140291e10` (point in triangle, XZ), `FUN_140163950`, `FUN_140164770`,
  `FUN_140256210`, `FUN_1401d5860`; capstone (scratch venv) disassembly of
  `0x140255020..0x1402559f0` for the floor-window branch and the call
  arguments; constants read with `engine/re/exe_const.py`
  (`0x1402f7f58` 7.5, `0x1402f7f64` 9.0, `0x1402f7d50` 0.643, `0x1402f7cdc`
  0.1, `0x1402f7d40` 0.5, `0x1402f7f18` 5.0, `0x1402f7eb8` 2.5,
  `0x1402f7fa0` 12.5, `0x1402f8330` -7.5).
- Data: `engine/cache/st03.pdcl` (53,273 triangles, 50,443 stage + 2,830
  object), `st03.surf.json` (2,723 0x94 faces, class equality and vertices
  verified against a direct `.dat` parse, max vertex diff 0.0005; stored normal
  sign opposite to the export winding on 99.5 %).
- Cross-sections at x = -100 (z -260 .. -110, y 40 .. 130), tread / riser /
  bevel list, the 11 gap bands and the tick log: Python replica of
  `collision.rs` + `sim.rs` movement; its source is in the session scratchpad
  only.
