# Skeletons, skinning, animation

Code: `crates/pd_formats/src/anim.rs` (tracks), `pose.rs` (posing, Euler conventions), `character.rs` (skinned glTF export). This area was solved in-house; Esper-RE only had rigid skinning.

## Rotation convention ✅ (confirmed by Laurence, 2026-09-26)

Joint local = `T(position) · R · S(scale)` with **R = Rz · Ry · Rx** (X applied first), angles as stored, no sign flips. In code: `pose::Order::GAME` = `Custom { apply: [0,1,2], neg: 0 }`. Stages use the same (`gltf::joint_local`).

How it was settled (so nobody re-litigates it): `pd_convert eulertest` scores all 48 conventions by impossible knee/elbow bends; it narrows the field but **cannot distinguish mirror-equivalent candidates**, and an early run had the character's forward axis backwards (face points to game −Z). Two by-eye picks were wrong. An A–F calibration sheet (VERIFY.md) shown to Laurence settled it in one round: candidate C.

## Animation data (0x5 chunks)

- The header offset table slot `id` points at the first track of animation `id`; tracks are consecutive 0x5 chunks, one per animated joint, ending at a chunk with another id. Names by id: `anim::ANIM_NAMES` (from the game's debug viewer; e.g. 0 WAIT0, 7 WALK, 12 RUN, 37 JUMP_START, 38 JUMP_UP, 39 JUMP_DOWN, 40 JUMP_LAND0, 65 ATK_SHOT, 66 DEF_SHIELD).
- Track header (0x20):

| Off | Type | Meaning |
|---|---|---|
| 0x08 | f32 | length in frames (**30 fps**, see "Playback rate" below) |
| 0x0C | u16 | joint index |
| 0x0E | u16 | rotation key size |
| 0x10 | u32 | translation key count |
| 0x14 | u32 | rotation key count |
| 0x18 | u32 | scale key count (never seen non-zero) |
| 0x1C | u8 | **rotation axis code** |
| 0x1D | u8 | **translation axis code** |
| 0x1E | u16 | translation key size |

- Keys follow the header: all translation keys, then all rotation keys.
  - size 3/5/7: `u8 frame` + 1–3 × `s16` angle, `value / 32767 × 2π` radians.
  - size 4/8/12/16: `f32 frame` + 1–3 × `f32`.
- **Axis codes** ✅ (verified against bind positions on many joints): which axes a key stores. Translation codes 0–6, rotation codes 7–13, same order: `xyz, xy, xz, yz, x, y, z` (`pose::axes_for_code`, uses `code % 7`). Axes not stored keep the bind value.
- Keyed values **replace** the bind local values (not added). Interpolate linearly between keys; wrap angle deltas to ±π.
- Root joint (parent −1; index varies by file -- e.g. pc00a's base file joint 6, its colour-variant files' joint 20, since the variants insert extra unnamed helper joints earlier in the list -- `character::root_motion` picks it by finding the parentless joint that is the ultimate ancestor of the most *skinned* joints, not just the first `parent < 0` entry) carries xz root drift. We **strip root translation** in the export — gameplay owns position. Since the motions lane (2026-09-27), `pd_convert character` also writes that stripped drift out per clip as `<id>.rootmotion.json` (`character::root_motion`), for a later sim lane -- see "Program evidence" below.
- JUMP_UP raises the pelvis (joint 8) ~5 units within the animation; physics adds the real jump. Laurence flagged JUMP_UP as "suspect"; the in-game look after tuning is unconfirmed ⚠️.

## Playback rate ✅ (`PROGRAM-RE.md` "Animation clock", 2026-09-27)

The program plays **one authored keyframe per 30 Hz logic tick, not 24 fps**: `FUN_140257510` adds the clip's speed (1.0, set by `FUN_140257310`/`FUN_1402573e0` on every clip start) to the clip time once per player update, which runs once per 30 Hz fixed step — so the "length in frames" field above is a tick count. `pd_formats::character::FPS` (glTF export) converts frame counts to seconds at **30 fps** (changed from an incorrect 24 fps assumption this session); every clip used to play 25% too slow (e.g. ATK_LASER's 34 frames lasted 1.417 s instead of 1.133 s). Confirmed a second way, against Laurence's own footage of the real game (`FOOTAGE.md` "24-vs-30 fps verdict" and "More 24-vs-30 fps evidence"): Blaster's cast-pose lock (~1.33 s measured) and the Jump clip's landing recovery (~300–370 ms) both sit close to the 30 fps prediction and well short of the 24 fps one.

Clip changes **blend over 5 frames** (the 5.0 passed to `FUN_140257310` as blend length) — `pd_game`'s `drive_animation` uses a matching 5/30 s transition (`CLIP_BLEND_SECONDS` in `main.rs`).

Root motion and the jump clips' time-stretching (not yet built here) are now confirmed in detail — see "Program evidence" below; we still strip root translation on export and play jump clips at their authored length.

## Cast clips ✅ (blades lane, 2026-09-27)

Laurence: "the blades (Mist Blade, Psycho Knife…) show the player kicking instead of slicing". Checked in order:

- **Clip name ↔ ALR slot mapping is right** ✅, in the colour-variant files too. The program's motion-name table (136 pointers at `0x1402db400`, `bas01::WAIT0` … `atk02::ATK_SORD` at index 64 … `atk38::ATK_WHAND_SORD` at 71) is in exactly our `ANIM_NAMES` order, and the ALR header's slot *n* is that motion: `pc00a.alr` (131 slots) and `pc00a0.alr` (132) both give slot 64 = 40 frames, 65 = 45, 71 = 54, 76 = 34, 90 = 35, 94 = 35 — the lengths the action rows' clips run for (ATK_SORD 40, ATK_SHOT 45, ATK_WHAND_SORD 54, ATK_LASER 34, ATK_TUKI / ATK_KICK 35). Each animation's tracks end at a chunk of type 0, so `read_animation` never runs into the next slot.
- **ATK_SORD is the real sword swing, raised knee included** ✅. Viewer sheet `shots/bl_v0_sord_side.png`: 0.2–0.4 s a high knee-up stride with the arm raised, 0.5–0.7 s a lunge and cross-body swing, 0.8–1.0 s the arm flung out. Laurence's `Psycho Knife - Not locked-on And Locked-on.mp4` (right half, the target's camera) shows the same poses in the same order: frame 100 (≈0.43 s after the cast sound at 2.874 s) the attacker dashes in knee-up with the arm high, frames 106–109 the swing with the magenta blade, 110–116 the arm out (`shots/bl_pk_before_after_footage.png`, bottom row). All 11 playable characters' ATK_SORD have that shape (`shots/bl_allchars_sord.png`). So the knee-up that reads as a kick is authentic.
- **What was wrong: which part of which clip we showed, and when.** The program starts the action row's clip (w38) at the cast start (`FUN_14025a3e0`) and plays it through the wind-up, the dash and the hit. We showed RUN_FAST during the wind-up and dash, then jumped into ATK_SORD at its fire tick (frame 18, mid-lunge, blended in from a run) — a leg-forward lunge pose appearing out of a run, with no stride or swing build-up. And the clip came from a hand table, not the row: the 8 two-handed swords (Excalibur, Psycho Blade, Blazing Sword, Vacuum Slash, Muramasa, True Muramasa, Excaliber, Frostbite) played ATK_SORD instead of their row's ATK_WHAND_SORD; skills with kick sound 507 played ATK_KICK though 3 of them are ATK_BODY and 1 ATK_TUKI in their rows; and so on for many families.
- **Now** (`pd_game::main.rs` `cast_clip_now`, `drive_animation`): every cast plays its action row's own clip (ground or mid-air row, as `Combat::action_row` picks) **from the cast start**, through a short-range skill's wind-up and dash (`Motion::Lunge`) into the strike without restarting; picked once per cast. A held charge plays it from the start and the release restarts it at the row's `start_frame` (w40). The hand table (`cast_clip`) is only a fallback for skills with no row (14 enemy-only) or a clip the character lacks. The sim reaches the fire tick exactly when the clip does, so the swing and the hit still line up (the slot-0 swing effect, still started at the strike by `fx.rs` from the fire tick in, stays in step). Before/after: `shots/bl_pk_before_after_footage.png`, `shots/bl_pk_contact_zoom.png`, `shots/bl_mura_before_after.png`; non-blade casts still right: `shots/bl_after_nonblade.png` (Laser, Blaster, Tiger Kick, Empty Hand).
- Still different from the original ⚠️: root motion is not applied (ATK_SORD's own 18 u forward stride plays in place, on top of the sim's dash), and the original's blade effect is much bigger and brighter than ours (effects lane).

## Skinning ✅ (discovered here)

Shader `SWBOS_vs.cso` (disassembled with `tools/disasm`): each vertex blends two 3-row matrices from the constant buffer: `M = w.x·cb[i.x..i.x+2] + w.y·cb[i.y..i.y+2]`.

Skinned vertex (format 0x11, 24 bytes): `pos f32×3 | uv s16×2 /32767 | BLENDWEIGHT 4 bytes read as BGRA: weight0 = byte 18, weight1 = byte 17, byte 16 = lighting selector, byte 19 = alpha | BLENDINDICES u16×2 at 20, 22`.

Blend indices are **shader register numbers**: register `120 + 3k` = slot k of the draw's bone palette. Register 96 appears with weight 0 (ignore).

The **palette** is stored after each skinned index buffer's indices: pad to 4, `u32 0`, `u32 count`, `u8[count]` joint indices, pad, `count × 4×4` identity placeholders. Slot k → joint `palette[k]`. (Found by matching vertex-group centroids to bind joint positions, then locating the list.)

Vertices are stored in bind-pose model space, so skin matrix = `animWorld(j) · inverse(bindWorld(j))` (glTF inverseBindMatrices).

## Multi-part characters ✅

A character ALR can contain several skinned models. Model 0 is the body; every other one is a **cloth part** on its own small skeleton (pc00a model 1 = the lower jacket: harness, band and straps, 5 strips × 4 joints, 23 joints). How the program attaches and moves them was read from the code (PROGRAM-RE.md "Character parts / cloth"): the part root is named **CLCnn** and takes the world matrix of the body joint **CLPnn** (pc00a: body joint 77 CLP00, a child of the chest BODY2); joints named CL<d><row><col> are nodes of a cloth grid simulated at 30 Hz with gravity, per-node angle limits and links between neighbouring strips. `character.rs` grafts CLCnn under CLPnn (falls back to the nearest named joint within 0.5 units when a part has no such pair) and writes each node's `cloth_row`, `cloth_col`, `cloth_keep`, `cloth_limit` as node extras; `pd_game/src/dangle.rs` runs the program's cloth on them. Parts have no animation tracks (checked: WAIT0/WALK/RUN tracks only address body joints).

Before 2026-09-27 the part was rigid or on a home-made pendulum; with WAIT0's chest bent ~45° forward that made pc00a's jacket jut out behind as a long flat flap (Laurence: "none are correct"). A–F sheet: `shots/jk_sheet_wait0.png`.

## Non-skinned draws in characters

Draws on position-only buffers (formats 0x07/0x0B) attached to helper joints (b36 flag 0x04) are skipped: probably shadow volumes ⚠️.

## Character table (`setup/charaset/charaset_en.dat`) and dodge clips

Layout from the community modding wiki ("charaset.dat"), checked against the file 2026-09-27 (`pd_formats::charaset`): 16-byte header (u32 size, u32 count = 47, u32 string-pool offset, pad), then 0x4C-byte records; strings are pool offsets. +0 model folder, +8 English name, +0x3C voice bank, **+0x36 u16 dodge move: 750 or 751** (wiki: "750=Jump, 751=Cartwheel"). +0x0C u16 = 120–285 (180 Hero, 200 Meister/JD/Cuff Button) — looks like height but a fan guide calls JD and Cuff Button short: unknown.

- 750: Hero, Freia, Meister, Chunky, Know, Tsubutaki. 751: Edgar, Cuff Button, pH, JD, Sammah. Matches the fan guide dustguide.neocities.org exactly ("Regular Jumpers ... Freia ... Tsubutaki"; "Human Rollers (only JD and Cuff Button)"; the rest "Cartwheelers") ✅.
- 751 characters' JUMP_START / JUMP_UP / JUMP_DOWN / JUMP_LAND0 slots hold **the same clips** as their DASH_A / DASHEND_A / DASH_B / DASHEND_B (identical per-joint rotation ranges); 750 characters' differ ✅. So the ground dodge is animated by the dash clips.
- Tumble vs cartwheel is not in the table: **in the base file**, DASHEND_A is 12 frames (0.4 s at 30 fps; was misreported as 0.5 s under the old 24 fps bug) for JD / Cuff Button (the guide's rollers, "no recovery") and 25 frames (0.83 s, was 1.04 s) for Edgar / pH / Sammah (cartwheelers, "you must recover") — `pd_convert chars` splits at 0.75 s (unchanged: 12 vs 25 frames straddles it at either fps). This split is **cosmetic only**: the colour-variant file the game actually plays from has no DASHEND_A to split on (see "Program evidence" below, "Animation files") -- `pd_game::sim::Dodge` (`Tumble` vs `Cartwheel`, different tick counts/speeds) does not match the real game's single 40-tick DASH_A lock for every 751 character; flagged under the motions lane's Requests.
- In game, Edgar's DASH_A goes through a handstand (upside down at ~0.3 s): the cartwheel is in the clip ✅ (`shots/dodge_game_sheet.png`). Which clip is the tumble's roll (DASHEND_A is played) ❓.
- Distances, speeds and the recovery length are not in the table (sim.rs constants, unconfirmed).

## Not yet handled

- `exmot0..5.ala` (intro/outro motions): different header; offset table count differs (166, 2, 91…) ❓.
- Animation blending rules, events (e.g. when a projectile spawns during ATK_SHOT) ❓ — likely in SSB scripts or the exe.

## TURN-clip hip curves across characters ❓ (measured 2026-09-29, unconfirmed meaning)

**Hero pivot-length verification (2026-09-30):** see
`MOVEMENT-AUDIT.md` → "Program read results, step 3c" for all eight
colour-0 lengths, inverted normal-gait clip selection and the strict
greater-than completion boundary (`0x1402575ca–0x1402575e0`). Read from
ALR slot offsets/type-5 chunk +8, matching loader `FUN_140256a20`.
Base-file lengths differ in three slots; use the playable colour file.

- Node-22 (`p0j22`) local yaw tracks, measured from the converted glbs: pc00a/pc01a/pc02a/pc09a are **identical** (0.000 rad deviation across all 8 TURN_* clips + RUN/WALK/WAIT0); pc05a is close (worst 0.38 rad); pc03a/pc04a/pc06a/pc07a differ hugely (worst 3.3–3.6 rad). Same clip lengths everywhere.
- CAVEAT: the skeletons are laid out differently per character (pc00a: 140 nodes, hip chain 20→21→22 with children 23/82/113; pc07a: 100 nodes, flatter branching), so "node 22" may not be the homologous joint everywhere -- the big sweep pc00a shows on node 22 appears on other node indices in pc07a. Do NOT build per-character tables from local node-22 tracks without establishing joint homology (or measuring world-space facing) first. The spasm investigation (2026-09-29) went down this path and backed out: the spasm reproduced on pc09a, whose curves are identical to pc00a's.

## Character lighting ✅ (approximation of the game's shader)

`SWBOS_ps.cso` (disassembled): `t0` = colour texture; `t1` = **object-space (bind-pose) normal map** (material entry +0x1A `normal_idx`), decoded `n = t1*2-1`; the vertex shader passes two light directions already transformed into bind space (`o4`, `o5`), and `n·L1, n·L2` index a 2D lighting ramp `t3` (not in the character file; likely per stage) multiplied by `cDL_COL`, plus point/ambient/rim terms from the constant buffer.

Our approximation: lit PBR material, base colour + normal map, every vertex given the bind-space axes as its tangent frame (`NORMAL=(0,0,1)`, `TANGENT=(1,0,0,1)`), so standard tangent-space normal mapping equals object-space lighting and skinning rotates the frame. Normal map Z channel flipped for the handedness mirror. Materials are single-sided (back faces culled: linings are modelled as reversed faces, FORMATS-ALR.md "Characters: one-sided faces"). Materials forced matte at runtime (`matte_characters`: reflectance 0) — Bevy's default 4% specular turned black trousers grey. Lights: one directional (`LIGHT_SUN` 3000) + global ambient (`LIGHT_AMBIENT` 500), overridable with env `PD_SUN` / `PD_AMBIENT`. Stages stay unlit (baked). The game's ramp `t3` is not reproduced ❓.
## Program evidence (PDUWP.exe, 2026-09-27)

Read from the game program (PROGRAM-RE.md "Phase 3", which has the addresses). ✅ = read in the code and checked against the data.

- ✅ **The game plays motions from `pcXXa<colour>.alr` (colour 0–3), not `pcXXa.alr`**, for every character whose charaset +0x3A (colour count) is non-zero — all playable ones. Those files carry a **different motion set**: every 751 character (Edgar, Cuff Button, pH, JD, Sammah) has **DASH_A = 40 frames** (the whole ground dodge, ~41–43 u of root drift forward) with DASHEND_A / DASH_B / DASHEND_B as 1-frame stubs, and ordinary JUMP_* clips (15 / 12 / 12 / 30 / 13–15); jumpers have DASH_A 2–3, DASHEND_A 10–13, DASH_B 9–12, DASHEND_B 12 (Freia / Tsubutaki 23); some walks differ (Edgar / pH / Sammah WALK 30). **Corrections to "Character table … and dodge clips" above**: "751 characters' JUMP_* slots hold the same clips as their DASH_*" and the DASHEND_A 0.5 s / 1.04 s tumble-vs-cartwheel split are true of the base file only; in game both kinds of ground dodge play DASH_A (40 frames) and lock for its length.
- ✅ **charaset fields** (loader `FUN_14017d300`; u16 → in-memory int/float): +0x0C ÷ 10 = height (18–20 u), +0x10 ÷ 10 = **body radius 5 u**, +0x12 ÷ 10 = **centre height 9 u**, +0x14 ÷ 100 = 1.0, +0x24…+0x30 in degrees — **+0x30 = turn rate, 20°/tick** (enemies 10–12), +0x36 = dodge = **a skill record index** (750 jump, 751 ground dodge), +0x3A = colour count, +0x3C = voice bank string.
- ✅ **Root motion**: when a clip is started with the root-motion flag, the root's x / z change each tick (rotated by the facing) is added to the player's position; the root's y is not. Flag set on every cast clip (action row w1), most hit reactions (reaction row w1), jump starts, landings, the dodges, knock-down landings, get-ups and emotes; not on idle / walk / run / strafe / JUMP_UP / JUMP_DOWN / FLOAT_*. Our export strips root translation — the sim needs the per-clip drift (a converter step can write each clip's root x/z per frame).
- ✅ **Jump clips are time-stretched**: the "up" clip is played over exactly the time to the apex (v / g ticks) and the "down" clip over the same duration; JUMP_START (with root motion) is played first and the take-off waits for its length − 2 ticks; the landing clip depends on fall height (< 40 u none, 40–80 u JUMP_LAND1, > 80 u JUMP_LAND0) and locks the player for its length − 1 ticks.
- ✅ **Blends**: 5 frames on most clip changes; turning / strafing clips are phase-matched to the clip they replace (same fraction of the cycle).
- ✅ **Walk / run clip sets** by mood (table `0x1403faed0`): normal WALK / RUN, fast (speed status > +49) WALK_FAST / RUN_FAST, slow (< −49) *_SLOW, confused (status 35) *_CON, wounded (**Health < 6**) WAIT_DAM / WALK_DAM / RUN_DAM and TURN_*DAM_*. Strafing while locked on uses TURN_WALK_* / TURN_RUN_* (R, L, BR, BL).

### Motions lane (2026-09-27): converter switched to the colour-variant files, root-motion export

- ✅ **The four colours are motion-identical.** Checked all four `pc00a0..3.alr` files' hashes (all different -- full-file, so at least textures differ) but the `anims`/`keys` tools show colour 0 and colour 1 give byte-identical track lengths for pc00a's DASH_A/DASHEND_A/DASH_B/DASHEND_B/JUMP_*/WALK, and same for pc01a's colour 0 vs 1 (including the DASH_A=40/DASHEND_A=1 collapse). So which of the 4 colour files loads doesn't matter for motion; `pd_convert character` now defaults to colour 0 (`--colour base|0|1|2|3` to override) via `resolve_character_source` (main.rs). All 11 playable `pcNNa` folders and all 14 `enmNNa` ones used by `chars/*.glb` (25 of the 28 `Player/` folders; `enm03a`, `enm12a`, `evpc00a` still unhandled -- different naming) ship exactly 4 colour siblings, so the auto-swap covers every character `pd_convert character` is run on today; folders without a `<id><colour>.alr` sibling fall back to the exact path given (unaffected).
- ✅ **Every playable-character `.glb` in `engine/cache/chars/` was regenerated from its colour-0 file** (2026-09-27). Draw/joint counts changed for every one (colour files carry a few more helper joints and draws than the base file -- e.g. pc00a body model 41→54 draws, 101→115 joints) but triangle counts are close (pc00a 2498→2462) and a smoke screenshot (`shots/mo_smoke_game.png`, `shots/mo_smoke_viewer.png`) shows no visible regression. Root-joint index shifts per file (see "Animation data" above) -- `character::root_motion` finds it by ancestor-of-skinned-joints, not by a fixed index.
- ✅ **Root-motion drift is now exported** as `cache/chars/<id>.rootmotion.json` (`pd_formats::character::root_motion`, called from `pd_convert character`): per clip, `forward`/`side`/`up` totals plus the full per-frame curve, in our exported (right-handed, +Z-forward -- matching `pd_game::sim`'s zero-yaw `fwd`) units. We still strip root translation from the baked glTF and play every clip in place, as before -- this is a side file for a later sim lane. Cross-checked against PROGRAM-RE.md's "Root motion" table (which was measured off the *base* file's joint 6): pc00a colour-0 gives ATK_STAMP 42.3 u (doc 42), ATK_KICK 38.1 (38), ATK_BODY 27.4 (27), ATK_WHAND_SORD 26.0 (26), ATK_SPHERE 23.0 (23), ATK_SORD 17.7 (18), ATK_FAITH_SHOT 15.2 (15), ATK_WHAND_LASER 13.8 (14), ATK_DRAIN 11.0 (11), DEF_COURSE 8.1 (8), ATK_PUNCH 6.6 (7), ATK_SHOT −10.9 ("+11, a step back" -- sign matches once the doc's raw-game-space convention is un-mirrored), JUMP_START 7.2 (pc00a0 "−7") -- all within about a unit, as the doc's own "can differ by a few units" caveat predicts. **One outlier**: ATK_TUKI is 19.5 u in the colour-0 file we now use vs the doc's base-file 34 u (confirmed directly with `pd_convert keys pc00a.alr 90 6` vs `pc00a0.alr 90 20`: the base clip's joint-6 z goes 0.39→−33.70, the colour-0 clip's joint-20 z goes 0.39→−19.13) -- a real difference between the files, not a bug here, and bigger than the "few units" seen elsewhere; worth a footage check later.
- ✅ **Jacket (pc00a), bounded check**: HANDOFF.md's backlog item 2 asks whether a variant file holds different jacket geometry that would fix the long-flap/no-lower-harness look. Rendered both (`pd_convert character ... --colour base` vs default colour 0, viewer screenshots `shots/mo_jacket_base_back.png` / `shots/mo_jacket_var0_back.png`, sheet `shots/mo_jacket_sheet.png`): **the jacket looks the same in both** -- same long green flaps to mid-thigh, same missing lower harness, despite the colour file's body model having 13 more draws and 14 more joints than the base file. So swapping to the colour file does not fix the jacket; the extra draws/joints are something else (not visibly different geometry in the WAIT0 pose checked). Not investigated further (bounded effort, per the lane brief) -- HANDOFF.md's existing leads (per-draw visibility/costume flag) still stand.
- ✅ **`pd_convert chars`'s dodge classifier fixed to 30 fps** (was `/24.0`, one of the two remaining 24-fps assumptions along with the stale doc comment on `anim::Track::length`, now also fixed): DASHEND_A frame counts are unchanged (still 12 vs 25 for the five 751 characters), so `chars.json`'s tumble/jump/cartwheel labels are unchanged -- only the printed diagnostic seconds were wrong before (e.g. reported 1.04 s where the real clip is 0.83 s). See "Character table … and dodge clips" above for why this classification is cosmetic-only in the file the game actually uses.
