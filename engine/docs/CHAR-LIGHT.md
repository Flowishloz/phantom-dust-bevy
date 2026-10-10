# Character lighting (`SWBOS`) — rebuilt from the program (lane L1, 2026-09-30)

Code: `pd_game/src/char_light.rs` + `char_light.wgsl` (shader, called from
`dissolve.wgsl`), `stage_look.rs` (`CharLightSet`, `char_light_from_json`),
exporter bits in `pd_formats` (`character.rs::light_tangent`, `alr.rs`
`SkinVertex::{mirror,spec_weight}` / `MaterialEntry::spec_bgra`,
`gltf.rs::Materials::get_lit`). Tests: `char_light::tests::*`,
`character::tests::light_tangent_gives_the_shader_frames`.
Evidence sheets (local, not committed): `engine/shots/l1_*.png` (list at the end).

## 1. The shader, instruction by instruction

`SWBOS_ps.cso` (`tools/disasm/disasm.exe game_files/SWBOS_ps.cso`), inputs: v1.yz
UV, v2 = COLOR0 (xyz = point-light vector, w = specular weight), v3 UV1 (same
UV), v4 = light dir in model space, v5 = half vector in model space,
v6.xyz = hemisphere axis in model space; t0 colour, t1 object-space normal map,
t3 = `spec77.dds`.

```
N   = 2*t1.rgb - 1                          (not normalised)
lut = t3( N.v4 , N.v5 )                     clamp/bilinear (FUN_1402a4170: sampler 3 = address 3, linear)
Lp  = 2*clamp(v2.xyz,-1,1) - 1 ;  P = sat(N.Lp)
h   = (2*cb5.w*(N.A' - 0.5) + cb5.w)^2 = (N.A)^2   with cb5.w = 0.5
rgb = base * ( cS0 + cDL*lut.rgb + cP0*P + cS1*h )  +  sat( v2.w * t1.a * lut.a * cSPEC )
out = rgb (unsaturated; the UNORM target clamps), o1 = cb0[17] (glow mask)
```
VS (`SWBOS_vs.cso`): two-bone skin (`cbase[i..i+2]` = 3x4 rows); `o4 = -M^T cDL_VEC * s`,
`o5 = -M^T cDL_SVEC * s`, `o6 = 0.5 * (-M^T cS0_VEC * s) + 0.5`,
`o2.xyz = 0.5 * (-M^T dir * P0.w / d^2 * s) + 0.5`, `o2.w = weight.w`,
`s = cbase[94 + weight.z]` = (1,1,-1) or (-1,1,-1) (`DAT_1402f7b28`, uploaded by
`FUN_1402a4170`: `FUN_14001e540(0xfffffffe, ...)`), i.e. **weight byte 16 = 255 marks the
mirrored (left-side) parts**.

## 2. What feeds the constants (read, `FUN_1402a42b0` -> `1402a7490` / `1402a7880`)

The getter `FUN_1402a7a00` gives four light records `L0..L3` = `DAT_14057f3d0/3d8/3e0/3e8`.
Record layout (0x70 bytes, `0x1405a6a40 + 0x70*i`): colour1 +0, colour2 +0x10, vector +0x20
(+0x30 for point lights), type +0x68, scalar +0x64. Set `s` uses records `0x40+8s ..`.

| Shader constant | Source | Script block field (`DAT_1403e8a40 + 0x58*set`) |
|---|---|---|
| PS c3 `cDL_COL` | `L1` colour | +8 |
| PS c6 `cS0_COLp` | `L0` colour1 (`.w`=0.5) | +0 |
| PS c7 `cS1_COLp` | `L0` colour2 (`.w`=0.5) | +4 |
| PS c5 `cP0_COLp` | `L2` colour (`.w`=0.5 -> the hemisphere factor) | +0xC (point light 0) |
| PS c2 `cSPECULAR` | material bytes (`+0x36,+0x35,+0x34` = R,G,B) x **2/255** x `L1` colour (`FUN_1402a75e0`, `DAT_1402f7c68` = 0.00784) | material entry +0x34 |
| VS c4 `cDL_VEC` | `L1` vector | +0x14 |
| VS c10 `cS0_VEC` (axis) | `L0` vector (`.w`=0.5) | +0x20 |
| VS c7 `cP0_POS` | `L2` +0x30, `.w` = `L2` +0x64 | +0x2C / +0x44 |
| VS c5 `cDL_SVEC` | `normalize(D + 0.5 F)` (`FUN_1402a69c0`; D = `L1` vector, F = camera axis `0x1405a5cc0..`) | — |

`FUN_140199e70` (per character draw): group = 1 for DL/`L1`/hemisphere; slots 2/3
(= point lights, **not** S0/S1 as the P0 audit assumed) come from group 0's records
0x42/0x43 and, while the battle-light flag `DAT_140530034 & 0x10` is set, are replaced by
the nearest effect-light records (`DAT_1405a6a80 + (0x60+2*view)*0x70`). Effect light
records are written by `FUN_14010c990` (LIGHT02): colour = lerp(word12, word13), position =
the emitter's, scalar (`cP0.w`) = lerp(range word0, word1) -> the point light is
`colour * sat(N.Lp)`, `|Lp| = range / d^2` (built: `gather_effect_lights`).

Constants found in the exe: PS `c0` = (0.8, 0.8, 0.8, 0.00806) (`DAT_140490c00`), VS `c0` =
(0,1,2,0.5). The alpha test `2*(sat(lut.b)*c0.z + c0.w)*(2a - 1.5) <= cb0[16].x`
would cut at alpha ~0.75 if `cb0[16].x` = 0; kept at 0.5 (Bevy `Mask`) because `cb0[16]`
and the `cb0[23]` kill flags were not located.

## 3. `spec77.dds` (exact)

256x256 BGRA8, every row identical in rgb: texel x = (x,x,x)/255; every column identical in
alpha: texel y = `floor(255 * (y/255)^20) / 255` (0 for y <= 190, 1 at 200, 255 at 255) —
checked on all 256 rows (`lut_alpha_lobe_is_the_twentieth_power`). The diffuse term is plain
Lambert; the specular power is 20 (the glb's material power byte 0x14 = 20 agrees; pc05a's
0x42 is not used by this shader).

## 4. Script colours: settled by lane L3 (2026-10-01)

**Read, not guessed**: the handler of script call `0A` is at `0x140206850` and stores the
call's arguments last-pushed-first (`docs/STAGE-LIGHT.md` §1), so the first colour is the
directional light (`cDL_COL`), the second the "down" hemisphere colour (`cS1`), the third
the "up" one (`cS0`): the default **reversed** reading below is right and
`PD_CHAR_LIGHT_ORDER=forward` is only an A/B. The same handler shows the light's
direction and the hemisphere axis are stored as (z, y, x) of the file's floats: the
program's x is the third float. This lane read them in file order, which mirrored the
fighters' key light (and L2's shadow direction); `stage_look::script_vec` now applies
the right order (and swaps an older cache's json at load). The paragraphs below are the
original reasoning, kept for the record.

## 4a. (original text) Script colours: the one thing NOT read (A/B switch)

The static default block in the exe has colours (+0,+4,+8) = (0x505050, 0x505050, 0xffffff)
and vectors (+0x14, +0x20) = (down-ish direction, up axis). The script's call `0A` pushes
(axis, dir, c1, c2, c3). Axis/dir evidently land **reversed** (axis -> +0x20, dir -> +0x14), so
the colours most likely do too: **c1 -> `cDL_COL`, c2 -> `cS1`, c3 -> `cS0`** (default, "reversed").
The natural-order reading (c1 -> S0, c3 -> DL, "forward") is `PD_CHAR_LIGHT_ORDER=forward`.
Evidence for "reversed" (not proof, the SSB handler for call `0A` was not located):
- memory order == pop order for both vectors and colours;
- character set 1 then has a **bright warm key and a dim cool fill** (st05: key (245,198,166),
  fill (76,78,80); st07 key (225,190,170), fill (80,60,55); st03 key (210,225,255), fill
  (81,86,90)), the standard look; "forward" would give a near-white *ambient* (0.8-1.0) with
  a weak key, i.e. exactly the flat look the game does not have.
Consequence for the stage (not this lane): `gltf.rs::light_vertices` used sky=c1, sun=c3;
under "reversed" the stage's key is c1 and its hemisphere top is c3 (fixed by L3: the stage
bake now uses `LightSet::{dl_col, hemi_up, hemi_down}`). On st03 c1~c3 so the
stage bake looks the same either way; on st05/st07 they differ a lot (st07 stage c1
(173,107,68) vs c3 (61,40,22)).

## 5. Exporter / data facts (measured)

- The normal map is exported as stored (no Z flip, alpha kept). In the game's left-handed space
  N = (sx*dx, dy, -dz); our Z mirror cancels the -dz: N_bevy = (sx*dx, dy, dz), sx = -1 for
  mirrored vertices. Measured on pc00a (cosine to the mesh normals): raw 0.46, old Z-flip
  **-0.10**; the two mirrored limb draws 0.98 with the x flip vs 0.68 without. So the old
  export had the normal map's Z wrong (fighters read flat/dark partly for that reason).
- Vertex bytes 16..19 (`BLENDWEIGHT.zw`): mirror flag (45% of pc00a's vertices) and the
  specular weight (per-vertex, varies 0..1 per character). Exported as the tangent frame
  (`light_tangent`) and TEXCOORD_1.x.
- Material specular colour bytes: pc00a 0x7f, pc01a 0xcc (x2/255 -> 1.0 / 1.6), exported in
  `asset.extras.pd_spec`.

## 6. Built / not built

Built: the full pixel model, gamma-space product and saturate, spec77 lobe, per-vertex mask,
mirrored frames, per-view camera half-vector, point-light slot fed by playing LIGHT02 decals,
scene-light fade state (`CharLightFade`, exact `FUN_14022e1d0` envelope; **not wired to any
event**: R6 unresolved), integrated into the wall-dissolve material (the same fragment path).
Not built: group-0 static point lights (`07` call, zero colour on all arenas' set 0), the glow
mask output (`cb0[17]`), shadows (lane 2), the `cb0[23]` alpha-kill flags.

Switches: `PD_CHAR_LIGHT_ORDER=forward`, `PD_CHAR_LIGHT_YAW=<deg>` (turntable of the light),
`PD_CHAR_EFFECT_LIGHT=0`, `PD_CHAR_LIGHT_LOG=1`; `PD_CAMFADE=0` falls back to Bevy PBR on the
StandardMaterial (the old look).

## 7. Golden values (independent script over the real `spec77.dds`)

N=(0,1,0), D=(0,-1,0), F=(0,0,-1), base 0.5, cDL (0.8,0.6,0.4), cS0 (0.2,0.25,0.3), cS1
(0.1,0.1,0.05), spec byte 127 -> out (0.637332, 0.540499, 0.418666); lobe(N.H=0.894427) =
0.109595. Facing away, weight 0: (0.15, 0.175, 0.175). Sideways: (0.1, 0.125, 0.15). N.L=0.5: (0.3125,
0.2875, 0.25625). Point light 50/d^2 at d=10 with colour 0.5: 0.125.

## 8. Aura Particle glow (lane aurapulse, 2026-10-01)

Code: `pd_game/src/aura_pulse.rs` + `aura_pulse.wgsl`; render-only counter
`Fighter::absorb_glow_ticks` / `absorb_age()` and `CombatEvent::AuraParticleUsed`
(combat.rs, additive; the counter is not in the state hash), the second sound in
`sfx.rs`. Tests: `aura_pulse::tests::*`,
`combat::tests::absorb_age_counts_an_aura_particle_use_and_nothing_else`,
`sfx::tests::status_effects_play_the_programs_ids`.
Evidence (local, not committed): `engine/shots/ap_sheet.png` (footage on top,
ours below, press +8 / +14 / +24 / +40 / +52 ticks = frames 164 / 167 / 172 / 180 /
186), `engine/shots/ap_sound.png` (the footage's audio against common/86, 89 and
90 laid at their measured offsets); footage frames, wav and scripts in
`~/.pdtools/footage/aurapulse/`.

### 8.0 Timeline of one use (MEASURED, `Orb Collect & Aura Consumed.mp4`, 30 fps)

| frame | what | how found |
|---|---|---|
| 134-135 | the orb is touched and vanishes in one frame (gold sparks follow); the fighter's boots stop dead from frame 134 | frames 130-139; boot template matching |
| 135.6 | `common/86` (capsule pickup) starts | audio cross-correlation 0.57 (the method's check: the program plays 0x56 = 86 at a capture, `FUN_140261a00`) |
| 160 | the HUD dial's Level goes 00 -> 01 and the Aura Particle slot flashes white | frames 152-167 of the dial |
| 159.87 | `common/89` starts (correlation 0.68) and `common/90` at 159.89 (0.50; 0.44 after subtracting 89) | audio; joint fit of both: R^2 0.56 against 0.44 (89 alone) and 0.25 (90 alone) |
| 162.5 | the green glow first shows (1,500 px at 162, 19,000 at 163) | green-pixel count |
| 166-169 | peak | same |
| 186.5 | glow gone (80 px at 186) | same |

So the press, both sounds and the Level change are one instant; the glow follows
2.6 frames (5 sim ticks) later and lasts 24 frames (48 ticks): `AURA_GLOW_TICKS` = 53
ticks from the press (`GLOW_DELAY_TICKS` = 5 + the 48). The audio and the picture
line up to within a frame (86 vs the orb's vanishing frame: 0.6 frame).

**What the original shows** (`Orb Collect & Aura Consumed.mp4`, 30 fps; the
file repeats frames in threes, so only every third frame is new). Using the
Aura Particle (HUD Level 00 -> 01) wraps the whole body in a pale-green
outline glow that is **not** on the capsule capture (frames 100-160: the orb
touches the fighter, vanishes in one frame, a few gold sparks follow; no glow,
no pose change). Measured on the green-dominant pixels round the fighter
(G > 150, G > 1.25 R, B < 0.7 G): none until frame 161, 1,500 px at 162,
**19,000 at 163-165 (75% of peak)**, **25,500 at 166-168 (peak)**, 23,600 at
169, 17,000 at 170-172, 11,600 at 173, 8,700 at 175, 5,600 at 176, 4,400 at
178, 2,800 at 180, 1,400 at 182, 700 at 183, 80 at 186, nothing after:
23 frames = 0.77 s = 46 sim ticks (the glow; the use itself holds nobody, MOVES.md). Look: the body's own
colours are untouched; the light is added outside the silhouette, brightest at
the edge (the pixels clip: (167,255,134) on frame 167), pale green, added
light R : G : B = 0.52 : 1 : 0.46 on the unclipped pixels; along a row through
the torso the added green falls 220 -> 170 -> 127 -> 91 -> 64 -> 47 -> 33 -> 20
over about 100 px (~3 u), a scale length of ~1 world unit in display space
(~33 footage px per unit); reach about a tenth of the body height. It also
spills onto the floor and wall behind. Envelope used (`aura_pulse::envelope`,
by the visual brightness of each frame): 0.75 on the first frame, 1.0 over
frames 166-169, 0.85 / 0.65 / 0.45 / 0.30 / 0.14 / 0.07 at frames 170 / 173 /
176 / 179 / 182 / 184, gone by 186.5.

**What the program does** (decomp reads, none of them a green body draw):
- The use is skill record 0 (`0x140258180` -> `0x1402626d0`); its action row
  343 has effect id 0, clip `WAIT0` and only rumble entries, so no effect
  starts from the row. Level +1 goes through `FUN_14024d930` (add + clamp
  `FUN_14024d970`) from the ability switch `FUN_140270b00`, whose tail
  `FUN_140271590` only plays the Level sound (sound 0x59 / 0x5a). The effect
  list has no Aura Particle effect that draws a body glow: `eft0301`
  ("Aziel particle", the skill-301 name) is twelve LIGHT02 point lights, white,
  31 frames, and no piece; names 0505-0517 are the erase / status effects; ids
  >= 1000 are bosses.
- The Level change restarts effect 600 slot 0 (`FUN_14019e6c0`, tier 1) -- four
  hand / wrist wisp meshes tinted by the school colour (`audits/P0-AURA.md`),
  already played by `aura_fx.rs`. Not a body outline.
- `FUN_14022e8a0` (the audit's "pickup glow") only clears one word in each
  record of a list (`DAT_140533448 + 0x70`, stride 0x60) when called from the
  capsule vtable's state-3 handler `FUN_14021b4f0` (the capture burst); it draws
  nothing. Capture shows no glow (above).
- What the program does contain that looks like this is a flat-colour skinless
  draw with a **screen-space offset** (`SAURA_vs.cso`: `add o0.xy, r0.xy,
  cb0[24].xy` after the matrix; `SAURA_ps.cso`: a constant colour, alpha 1,
  `o1 = cb0[17]`), the building block of a body outline made by drawing the
  silhouette several times shifted. Who instantiates it was not traced
  (`audits/P0-AURA.md` "who else instantiates SAURA"). **So the draw behind the
  footage is unconfirmed; the look, colour, strength, length and envelope are
  measured.** Whether the colour follows the arsenal's school (the footage
  fighter's school is not known; the glow is Nature green here) or is always
  green is also unconfirmed: it is fixed pale green.

**What we draw**: each body piece of the fighter (skinned, opaque or
alpha-cutout, on a layer a camera draws; the shadow hulls and effect meshes are
skipped) gets 64 "ghost" children sharing its mesh and joints (hung from the
`PlayerVisual` root, which has the visibility; on layer 0 explicitly because
the root's own layer is a private one) the first time that fighter uses an
Aura Particle. While `absorb_age()` is `Some` they are shown additively in flat
green; the vertex shader adds `shift * projection` to the clip xy (a fixed
distance in the camera plane: the `SAURA_vs` offset) and pushes the vertex 2 u
back along the view ray; the pixel shader keeps only the side of the piece that
faces away from the camera (outward normals rebuilt from the triangles, since
the exporter writes a constant normal), honours the piece's colour-texture
alpha cut-out, and adds `weight x tint` in scene-linear light. The 64 shifts are
a golden-angle spiral over a 1.2 u disc, radius `R t^1.4`, group weights fitted
by non-negative least squares so the stacked sum along a straight edge matches
`(1.25 e^(-d/0.43))^1.5` (rms 0.03); display-space light goes to linear by
`v^1.5` (fitted for a ~0.15 grey background; the original adds in gamma space,
Bevy blends in linear), and the whole profile is stretched x1.3 by eye against
the sheet. The body is untouched (pushing only 0.45 u back let rounded chest
and shoulders show a film of ghost; 2 u does not).

### 8.1 The sound (SETTLED: 89 only, read + live trace)

READ: the Level +1 reaches `FUN_140270a00` -> `FUN_140270b00` -> `FUN_140271590`
(kind 1, amount 1 -> `(amount < 1) + 0x59` = **89**, `FUN_140276330(0, 89, 89, ...)`
at the fighter's position); program record 0 (the Aura Particle; the header tail,
file offset 0x18: id 301, row 343 at +0x14, target kind +0x52 = 4 "self",
ability kind +0x56 = 1, amount +0x5e = 1) has one ability slot, so that path plays
one sound. LIVE (2026-10-01, `.pdtools/moverec/takes/000_Aura_Particle_held_stick_trace.csv`,
every call of `FUN_140276330` logged): the use tick logs `sound:0.89` and a footstep,
nothing else. An earlier footage audio fit had also proposed common/90 (`ap_sound.png`,
correlation 0.50); the live trace overrules it, so ours plays 89 only
(`CombatEvent::AuraParticleUsed` is silent in sfx.rs).

### 8.2 The hold (SETTLED: there is none, live trace on the PC original)

READ: the use is `0x140258180` for record 0; `0x14025a3e0` plays the row's rumble /
sound / effect (row 343: all 0) and, because the row's clip id is 0, returns 0 =
"fire at once, no state change" (`0x14025a3e0`: `row[+0x4c] == 0 -> return 0`); the
fire `0x1402626d0` for record 0 ends by setting the player block's state word to 8
(`DAT_14052ad00 + player * 0x120 + 0x15c`, a render/HUD block next to the opacity
value, read by no movement code found) and Level +1 (`FUN_14024d930`) touches no
movement field. Nothing in the path zeroes speed, writes the input block or sets a
timer movement reads. MEASURED: the boots do not move from frame 134 (the capture) to
the end of the clip (frame 195, template match, error 1-3 grey levels up to frame
168), through the press and the glow, but they were not moving before the press
either (the player had released the stick), so the footage cannot say whether the use
holds a pushed stick. **LIVE TRACE (moverec, 2026-10-01, four separate runs on the PC
original, stick held forward through an Aura Particle use; `~/.pdtools/moverec/takes/
000_Aura_Particle_held_stick_trace.csv`, `pilot_run2.log`; method MOVE-RECORDER.md):**
speed stayed 1.85 u per program tick (run) before, at and after the press, the state
stayed "run" (1), Level went 3 -> 4 with sound 89. The use holds nobody, as the
program read says; ours no longer holds either (`Combat::cast_from` returns before any
cast state, the `AURA_ABSORB_TICKS` hold is gone). The glow does not depend on it.
**Live trace recipe:** `tools/bats/PLAY_ORIGINAL_TRACE_AURA.bat` (engine/re/live_trace.py,
now with `[reg+off]` / `d[..]` / `f[..]` memory arguments) logs per program tick each
fighter's state (`+0x12f04`), clip (`+0x12f00`) and position (`+0x12e40..48`) at the
fighter update `0x14024ec80`, plus the press (`0x140258180`), the fire
(`0x1402626d0`), the Level add (`0x14024d930`), every sound and every capture, to
`%USERPROFILE%\.pdtools\trace\aura_use.csv`. Laurence captures one Aura Particle,
then holds the stick and presses its button, holding for 3 seconds: if the position
keeps changing and the state stays 0/1 the use holds nothing; if it freezes, the
state number and the frames it lasts are the rule. (The register / offset meanings are
read from `FUN_14024ec80` / `FUN_140251d80` / `FUN_140259310`, not yet seen live.)

Knobs: `PD_AP_WIDTH` (default 1.3), `PD_AP_PUSH` (2.0), `PD_AP_GAIN` (1.0),
`PD_AP_FORCE=<age ticks>` (hold player 1's glow at that age, for stills),
`PD_AP_LOG=1`. Capture recipe: `--demo --near --loadout 0,1,65,26` (slot 0 =
the Aura Particle) with `PD_FREE_CAM=1 PD_MUTE=1 PD_TEST_PRESS=260,0
PD_SHOT_TICKS=p0,p4,p14,p24,p40`.

Known limits: the glow ignores the wall / near-camera dissolve (`cam_fade`), so
a camera inside the fighter shows the halo of a grainy body; the glow runs on its
own 53-tick counter and a hit does not stop it (the original's behaviour on a hit is
not measured); no floor-light pool (the footage's spill is only the halo drawn
over the floor); ~1,300 skinned draws per fighter while it runs (0.9 s).

**Update 2026-10-01 (lane auralook):** the Aura Particle glow described in this section is the
second-guess pulse (`aura_pulse.rs`, now off unless `PD_AP_LEGACY=1`). The original's halo is the
HUD aura object's blurred silhouette of the fighter (`FUN_14019b920`, `GFilter2`, `AuraBox`), and
its pulse is the script's state-8 formula; built as `aura_glow.rs`. See FORMATS-EFFECTS.md
"School aura glow" (its width against the footage is still UNCONFIRMED).
