# P0 audit — EFFECTS (deep-pass P3, visual side) — 2026-09-30

Read-only audit. Nothing was run, built or launched; all program reads are from
`decomp_all.c` (addresses below), the PE (`exe_const.py`), the shaders
(`tools/disasm`), the game data (`game_files/Assets/Data/Effect`, packed
archive `e.i`/`a*.e`) and our code. Scratch scripts were kept in the session
scratchpad, not in the repo. **Every blend/word statistic below was measured on the
PACKED archive (what the game plays), not the loose `eftNNNN.alr` files — the two
differ in 317 of 433 shared numbers** (e.g. loose Crystal Wall is opaque, packed
Crystal Wall is additive; I first read the loose one and corrected myself).
"Loose-only" numbers are not reachable by the program (see D12).

Method per `METHOD-FIDELITY.md`: read the spawn call sites, the manager, the
per-draw class, then compared each word/flag with what our runtime does.

## 0. Headline

Our effect pipeline plays the right files on the right events with the right
bones, but it **renders every piece as an oriented additive quad** while the
program chooses, per draw, a blend equation, billboarding, culling, a world-latch
mode, a flipbook rate, a UV scroll and (for 9 effects incl. Laser/Blaster) a
perspective point-sprite shader. Six of those were `❓` in the docs; they are
decoded below. The "not perfect" verdict of 2026-09-30 is therefore explained
mostly by **missing render semantics**, not by bad timing.

## 1. Ledger

`✅` yes · `❌` no · `~` partial. "Reproduced" = our runtime seen doing it (per
VFX-AUDIT/FORMATS sheets; this audit did not re-view the images). Nothing in the
effects area is Laurence-approved (no DECISIONS/HANDOFF entry; his 2026-09-30
verdict is "not perfect").

| Feature | Mapped | Implemented | Reproduced | Verified | Approved | Evidence |
|---|---|---|---|---|---|---|
| Skill → effect number = action row w2 (ground s12 / air s13); packed `e.i` is the only source | ✅ | ✅ `fx.rs:25-45` | ✅ | ✅ 323 skill effects all in `e.i` (1 exception: skill 294 → eft0355) | ❌ | `FUN_14025a3e0`→`FUN_140259140`→`FUN_14018d2f0`; `FUN_14018d0b0` |
| Slot 0 (cast) starts on the cast-start tick, same call as the clip | ✅ | ✅ `fx.rs:87-107` | ✅ | ~ tick-matched sheets vs Laser/Slide footage | ❌ | `FUN_14025a3e0`; FOOTAGE.md |
| Slot 0 root follows the caster live; slots 1-5 use a spawn-time snapshot matrix (except what the owner updates) | ✅ this audit | ~ slot 0 child of player visual; slot 3 also follows (should not) | ~ | ❌ | ❌ | `FUN_14018a040` (`[0x37]` flag: slot 0 and effect<1000), `FUN_14018a8b0` |
| Effect clock: +1.0 per update of the effect manager; frame window start ≤ t ≤ end (inclusive); duration = max end of all 0x13 chunks; one-shot removed when t ≥ duration | ✅ (update rate ⚠️ assumed 30 Hz) | ~ wall-clock `delta×30` (`effects.rs:1270-1282`), end exclusive (`:2009`), length padded by emitter tails (`:587-597`) | ✅ | ~ | ❌ | `FUN_1401899b0` (vtable `0x14047bd98`), `FUN_140188fd0`, `FUN_14018aee0` |
| Slot 1 (flight) loops until released; follows the attack object | ✅ | ✅ `fx.rs:1374-1400` | ✅ | ~ | ❌ | `FUN_14018a040` (`param_4==1`→loop), `FUN_140263a00`, `FUN_14026cd70` |
| Slot 2 at impact (codes 1-3/8) | ✅ | ~ plays, **no scale** (`fx.rs:146-165`) | ✅ | ❌ | ❌ | `FUN_14026cd70`; D4 |
| Slot 3 on the victim when hurt | ~ (attacker effect is a stack arg, see Reads) | ~ `fx.rs:148` | ✅ | ❌ | ❌ | `FUN_140258690`/`FUN_140259310` |
| Slot 4/5 of defence = block / break | ✅ | ~ slot 4 only, at victim root; **slot 5 never played** | ~ | ❌ | ❌ | `FUN_14026cd70` codes 9/10 |
| Slot 4/5 other uses: charge hold loop + release, Rain drops + cloud, persistent status loops (518/519/521), warps, env-crystal destroy | ✅ this audit | ❌ | ❌ | ❌ | ❌ | `FUN_14025c720`/`140259540`, `FUN_14026b480`/`b8d0`, `FUN_14025cbd0`+table `0x1403e2158`, `FUN_140257e90`, `FUN_1402729c0` |
| Shared effects 512 (reaction row 42), 520 (landing dust), 518/519/521 (status 8/11/33) | ✅ | ❌ (no code plays 5xx numbers) | ❌ | ❌ | ❌ | `FUN_140254590` plays 0x208; table `0x1403e2158` |
| Named joints ride the character bone | ✅ | ✅ `effects.rs:777-826` | ✅ | ✅ Psycho Knife sheet | ❌ | `FUN_140189420`, `FUN_14018a7d0/aa50` |
| **Blend state per draw** (material +0x0C bit 0x10 + +0x14 state word) | ✅ this audit | ❌ everything `AlphaMode::Add` (`effects.rs:1248`) | ❌ | ❌ | ❌ | D1 |
| **Billboard (0x13 word 19 = 0) / roll (word 7)** | ✅ this audit | ❌ | ❌ | ❌ | ❌ | D2 |
| **Root-matrix mode (word 28: world-latch, world-aligned)** | ✅ this audit | ❌ | ❌ | ❌ | ❌ | D3 |
| **Point-sprite shader `MSEFTBB01S`** (Laser charge 240 draws, Blaster 72, 7 more effects) | ✅ this audit (shader disassembly) | ❌ treated as quads | ❌ | ❌ | ❌ | D2 |
| Cull (word 19 ≠ 0 → back-face cull) | ✅ | ❌ all double-sided (`effects.rs:1249`) | ❌ | ❌ | ❌ | D9 |
| Colour start→end by easing word 10 | ✅ | ✅ `:2044` | ✅ | ~ | ❌ | `FUN_1401a9ae0` |
| Colour extras: word 30 parity flicker (w12/13 vs w14/15), word 31 fade-in/hold/fade-out | ✅ this audit | ❌ | ❌ | ❌ | ❌ | D8 |
| Easing curves 0/1/2 | ✅ | ~ curve 2 uses the wrong class's formula (`effects.rs:322-331`) | ✅ | ❌ (test pins the wrong formula) | ❌ | D13 |
| Move (w4-6/w8) and grow (w16-18/w9) | ✅ | ✅ | ✅ | ~ | ❌ | `FUN_1401a5ff0` |
| Flipbook rate | ✅ this audit | ~ stretched over life only | ~ | ❌ | ❌ | D6 |
| UV scroll (words 22/23) | ✅ this audit | ❌ | ❌ | ❌ | ❌ | D7 |
| Distortion (PDEFTOPT03/05: 1,367 skill draws) | ~ | ❌ hidden (`effects.rs:341-346`) | ❌ | ❌ | ❌ | known gap |
| Glow/mask second target (`o1 = blend_colour`) | ~ | ❌ | ❌ | ❌ | ❌ | known gap |
| PARTICLE01/02 | ✅ | ~ `emit_points` | ✅ | ~ | ❌ | `FUN_1401a1c00` etc. |
| FLAIR01 ray burst | ✅ | ✅ (size fixed 2026-09-28) | ✅ | ✅ footage sheet | ❌ | FORMATS-EFFECTS "Effect units" |
| BLADE01/02 swing ribbon | ✅ | ~ | ✅ | ~ | ❌ | `FUN_14019ec80` |
| LIGHT02 | ✅ reading | ~ by-eye floor decal (`RANGE_SCALE`) | ✅ | ❌ | ❌ | `FUN_14010c990` |
| TALE01 trail (Laser/Slide/Blaster beams) | ~ class found (`pdEftTale03`, vtable `0x14047a180`, draw `0x1401a9fa0`) | ~ fixed crossed-quad, length `5×w18` (`effects.rs:34-39,407`) | ✅ | ❌ | ❌ | D10 |
| MSPARTICLE puffs, THUNDER01, MSBILLBEAM00, YKFADE00 | ❌ | ~ guesses / not drawn | ~ | ❌ | ❌ | FORMATS-EFFECTS |
| SHOT01 markers choose shot count + spawn point | ✅ | ❌ (sim constants) | ❌ | ❌ | ❌ | `FUN_14018a310`, `FUN_14018ff00`; no `SHOT01` use in `pd_game` |
| Cast effect plays over its whole window during a dash/turn | ✅ | ✅ | ✅ | ~ | ❌ | `fx.rs:92-106` |

## 2. Discrepancies (most important first)

### D1 — Blend state per draw ignored. CONFIRMED (read both sides)
- **Original**: `FUN_1402a6010` — if material flags (`+0x0C`) bit `0x10` is set, `FUN_14028e4e0(state & 0xffe0 | 1)` with `state` = material `u16 +0x14`:
  source factor = `tbl[(state>>8)&0xF]`, dest factor = `tbl[(state>>12)&0xF]`, operation = `tbl2[(state>>5)&7]`; `tbl` at `0x1402f77c0` = GL enums
  `0 ZERO, 1 ONE, 2 SRC_COLOR, 3 1-SRC_COLOR, 4 SRC_ALPHA, 5 1-SRC_ALPHA, 6 DST_ALPHA, 7 1-DST_ALPHA, 8 DST_COLOR, 9 1-DST_COLOR, 10 SAT`;
  `tbl2` at `0x1402f7798` = `ADD, SUBTRACT, REVERSE_SUBTRACT, MIN, MAX, …`. `FUN_140020660(0x3e/0x3f/0x4a)` converts them for D3D11 (`FUN_140020420/1400204f0`). If bit `0x10` is clear → blending off (alpha-test only). (The docs' "false trail" verdict on these tables was wrong: they are consumed.)
  Observed states (packed): `0x1100` ONE,ONE (additive); `0x1400` SRC_ALPHA,ONE; `0x1200` SRC_COLOR,ONE; `0x1800` **DST_COLOR,ONE**; `0x3000` **ZERO,1-SRC_COLOR (multiply-darken)**; `0x2000` ZERO,SRC_COLOR; `0x5400` SRC_ALPHA,1-SRC_ALPHA (alpha); `0x1240`/others use REVERSE_SUBTRACT; flags without `0x10` (`0x1`,`0x9`,`0xd`,`0x19` minus blend bit) = no blend.
- **Ours**: the converter never reads `+0x0C`/`+0x14` (`pd_formats/src/effect.rs:257-275` writes shader/raw/joint only); `effects.rs:1245-1252` gives every non-solid piece `AlphaMode::Add`.
- **Measured reach** (323 effects reached by a skill action row, visible draws): 8,101 ONE,ONE · 1,666 SRC_ALPHA,ONE · **1,229 DST_COLOR,ONE (32 effects)** · **853 SRC_COLOR,ONE (44)** · **725 ZERO,1-SRC_COLOR (63 effects)** · 359 alpha-blend (42) · 290 no-blend (45) · ~100 subtract/odd. ≈ 25% of visible skill-effect draws are not plain additive.
- **Named samples**: Psycho Wave `eft0001` slot 0: 7 of 12 visible draws (4 rings + both `PDMTEX01` lightning sheets + 1) are `0x3000` — the original **darkens** the scene under them; we add a glow. Laser `eft0042`: its 240 charge sprites are `0x1800` (DST_COLOR×src + dst: brightens what is behind, near-invisible over black); Blaster `eft0053`: 72 sprites ONE,ONE plus 17 `0x1200` (src²) draws; Excalibur `eft0004`: 90 `0x1200` ray draws; `eft0026` (PDEFTBB04) has 10 `0x3000` and 7 `0x1800` draws.
- **Expected visible effect**: Psycho Wave looks bright/white where the original is dark/inky; Laser/Blaster charge reads as a flat glow instead of a scene-lit shimmer.

### D2 — Billboards and point sprites drawn as fixed-orientation quads. CONFIRMED (code + shader)
- **Original, plain pieces**: `FUN_14011bc30` (PDEFTBB01 draw, vtable `0x14047a7c8`) → `FUN_1401a5ff0`: if 0x13 **word 19 == 0** the piece's matrix is reduced to *scale only in view space* (`view × M`, rotation discarded) and rolled by **word 7** degrees × move-curve (`FUN_140290800`, degrees × 32768/360) — a **camera-facing billboard**; word 19 ≠ 0 keeps the model orientation (and `FUN_14011bc30` then enables back-face culling: `0x93` = `0x901`). 11,206 of 20,859 visible plain pieces are billboards (Psycho Wave 16/16, Psycho Knife flash, Laser 242/254, Blaster 93/106).
- **Original, `MSEFTBB01S`** (shader `msEFTBB01S_vs/ps`, disassembled): a **perspective point sprite** — vertex input `v2.x` = point size, corner from `vertex_id`, screen size ∝ `psize ÷ (2·w)` × (0.001172, 0.002083). 1,398 draws in 9 effects: Laser charge (240 sprites), Blaster (72), …. Always facing the camera, constant angular size.
- **Ours**: no billboarding anywhere in `effects.rs` (`animate_pieces` only scales/translates), so the ~240 Laser sprites are flat world-sized quads. This matches VFX-AUDIT #1/#2 exactly ("reads as flat rings", "dark blob from one camera", "charge too big") — and `CAST_REACH` (`effects.rs:98`) is probably compensating for it, not for a units error.
- Word 7 was listed as "spin, not applied": for plain pieces it is the billboard roll; for emitters it is speed (as already read).

### D3 — Root-matrix mode (word 28) and snapshot matrices ignored. CONFIRMED
- **Original**: `FUN_1401a5940` selects the matrix each piece uses by `(int)word28`: 0 joint matrix as is · **1 latch the matrix in world space the first time it is drawn, reuse it** (`FUN_14018e880`/`14018a9b0`) · **2 latch position+scale, world-aligned axes** · 3/7 live effect root · 4/5/6/8 position+scale with world-aligned axes (joint or root). 5,087 of 20,859 pieces have mode ≠ 0; 4,144 are mode 1. Psycho Wave: 12 of 16 pieces are mode 1; Psycho Knife's flash (mode 1) and hit sparks (mode 2); Crystal Wall's shards.
  Also `FUN_14018a040`: the instance root matrix is a **copy** taken at spawn (`+0x50`/`+0x90`); only slot 0 of effect numbers <1000 follows the live skeleton root (`FUN_14018a8b0`); slot 3 (hit on victim) and everything else keep the spawn matrix unless the owner refreshes it (flight effects are refreshed each tick by their handlers; status loops by `FUN_14025cbd0`).
- **Ours**: every piece rides its bone or the effect root for its whole life; slot 3 is a child of the victim's visual (`fx.rs:73-80,148`).
- **Expected**: rings/flashes that the original leaves behind in the world (Psycho Wave, knife flash) instead follow the dashing caster; hit sparks follow a knocked-back victim instead of staying where they landed.

### D4 — Impact (slot 2) effects are not scaled by the blast radius. CONFIRMED
- **Original**: `FUN_14026cd70` cases 1-3 (skills with s51 = 5) and case 8 (mines) scale the effect root matrix by **radius × 0.2** (`fVar24 = (s52·0.1)·DAT_1402f7d08` = `0x1402f7d08` = 0.2; mines: `FUN_1402637f0` = s67·0.1). Motion 9: `FUN_140268050` scales by s64·0.1·0.2. So Bomb/Cluster Bomb (s52 300) ×6, Big Bomb (400) ×8, Timer Mine (500) ×10, Remote Mine ×6, Photon Wave (s64 400) ×8, Photon Burst ×10. Other impacts ×1.
- **Ours**: `fx.rs:146-165` plays slot 2 (and slot 3) at unit scale; `Burst` plays slot 3 too, which the program does only per victim.
- **Expected**: every bomb/mine/wave explosion is 6-10× too small.

### D5 — Whole classes of effect playback are missing. CONFIRMED (program plays them; we have no code)
Slot 5 (defence break, 47 defence skills have slots 4+5); charge skills (Gungnir, Ki Cannon, Dragon Bullet, Young White Dragon, Ki Palm: slot 4 looping charge cue ×2 from `FUN_140259540`, slot 5 flash on release `FUN_14025c720`); Rain (9 skills, all six slots: cloud = slot 5 100 u above the target, one slot-4 per drop `FUN_14026b480`/`14026b8d0` — ours plays slot 1 at the area, `fx.rs:1405-1411`); warps/movement specials (slot 4 at origin, slot 5 at destination, `FUN_140257e90`; 4 of 10 have them); status loops 518/519/521 (status 8/11/33 → slot-4 loop following the player, slot 5 on end, table `0x1403e2158`, `FUN_14025cbd0`); landing dust 520 (`FUN_140254590`); reaction row 42 → effect 512; environment-crystal destroy (slot 5 at the crystal +10 u, `FUN_1402729c0`). SUSPECTED extra: `Burst` slot 3 double-plays (above). Needs new `CombatEvent`s for some (charge hold/release, guard broken, landing).

### D6 — Flipbook rate. CONFIRMED read
`FUN_1401a5f20`: **word 21 == 0 → one cell per tick, wrapping** (cell = (t − start) mod cols·rows); ≠ 0 → stretched over the life. 2,281 flipbook pieces have word 21 = 0 (Psycho Wave's three 4×2 lightning sheets); ours always stretches (`effects.rs:2051-2056`) — speed differs, and the program loops the sheet.

### D7 — UV scroll (words 22/23) unapplied. CONFIRMED read
`FUN_1401a5ff0`: constant `0x22` = (w22, w23) × move-curve → texture offset over the life. 3,785 of 20,859 pieces (Psycho Wave 9/16, Psycho Knife 1, Crystal Wall 1, …). Never read in `effects.rs`.

### D8 — Colour model: extras and the alpha semantics. CONFIRMED read; look impact SUSPECTED
- `FUN_1401a9ae0`: **word 30 ≠ 0 → colour pair (w12,w13) on even frames, (w14,w15) on odd frames** (529 pieces; Blaster 3); **word 31 > 0 → colour goes w12→w13 over the first w31 frames, holds, then back over the last w31** (4,317 pieces). Ours: plain w12→w13 lerp only (`effects.rs:2044`).
- `PDEFTBB01_ps` (disassembled): `rgb = tex.rgb × vertex.rgb`, `a = tex.a × vertex.a`, alpha only **discards** (`a ≤ cb0[16].x`). With ONE,ONE (75% of draws) alpha never attenuates; our additive path multiplies rgb by alpha (`fx_gamma.wgsl:34`). 6,798 pieces fade by alpha only (end colour `0x00RRGGBB`): original holds full brightness then pops off; ours fades. This is a likely contributor to "lacking the brighter FX".

### D9 — Culling. CONFIRMED
Word 19 ≠ 0 (9,653 pieces) → back-face culled (`FUN_14011bc30`); ours draws both faces (`effects.rs:1249`, additive → double brightness on domes/cones).

### D10 — Trail ribbon (TALE01). SUSPECTED
Program: a ring-buffer ribbon of history samples (`pdEftTale03`: init `0x1401a9e10`, draw `0x1401a9fa0`, additive `0x1101`, no cull), alive `+0x74` ticks after the emitter stops. TALE01 words: w16/17 width, **w18 = 17 / 25 / 10 / 20** (Laser / Slide / Blaster ×2) — presumably node count. With 12.96-14.8 u per tick the ribbon would be ~130-370 u long; ours is a fixed crossed quad 5×w18 long (85-125 u) that grows over 3 frames (`effects.rs:34-39,407,1640-1641`). Unread: which word is the node count and whether a node is recorded per tick or per drawn frame.

### D11 — Shot spawn from SHOT01 markers, sim-side. CONFIRMED gap (owner: attacks audit)
`FUN_14018ff00` places each attack object at the caster's aim orientation with the **position of the SHOT01 marker** of the running cast effect (`FUN_14018a310`); our sim uses constants; nothing in `pd_game` reads SHOT01.

### D12 — Loose-only effects (resolves the long-standing ❓) — CONFIRMED
In `e.i` the 500-699 range holds only 512, 518, 519, 520, 521, 600-606. 501/502/505-511/513-517/522-550 (43 numbers) exist only as loose files and are never loaded by the program (`FUN_14018d0b0` reads `e.i` only); no skill action row points at them. The loose 518-521 are placeholders (copies of 517); the packed 518/519/520/521 are real (99/1/70/7 pieces) and are the ones D5 needs.

### D13 — Easing curve 2 formula and clock details. CONFIRMED (small)
BB01-class helper `FUN_1401a5ff0` computes curve 2 = `e(2d − e + 1) / (d(d+1))` (→ 1 at e = d); `effects.rs:322-331` uses `e(2d − e − 1)/…`, which is the `ykEDGESLUR00` shader class's formula (`FUN_140122ac0`). ≤ 7% error, but the unit test (`effects.rs:2142-2150`) pins the wrong numbers. Also: the program's frame is an integer that steps once per update and the piece window is inclusive (`FUN_140188fd0`); ours is a continuous wall-clock float with an exclusive end, driven by `Time`, not the sim tick (`tick_effects`).

### D14 — Release semantics / cancel. SUSPECTED
`FUN_14018d3d0` only sets the end flag; the manager drops the instance on the next update (no play-out). A new cast releases the previous cast effect (`FUN_140259140`). Ours lets an old cast effect run to its natural end when a new cast starts or the cast is interrupted; flight/cast cleanup on hit-cancel not read (see Reads).

## 3. Priority-sample trace (program spawn → ours → differences)

All cast spawns: `FUN_14025a3e0` → `FUN_140259140(player, row.w2, slot 0, 0)` → `FUN_14018d2f0(playerIdx, eff, 0, &player+0xbe0)` at cast-start tick, same call as the clip (`FUN_1402573e0`). Ours: `fx.rs:90-106` (same tick, child of the player visual). Flight: slot 1 created with the attack object in `FUN_140263a00` (`FUN_14018d2f0(owner, eff, 1)`, looping, matrix copied from the object each tick by the handler). Ours: `fx.rs:1374-1400`.

| Sample (effect) | Slots present | Program specifics | Ours / diffs |
|---|---|---|---|
| Psycho Knife (eft0003) | 0 (models 0,1), 3 | slot 0: BLADE01 on RSWORD 17-23, flash m0_d1 billboard on RHAND 13-29 **latched in world (w28=1)**, FLAIR01/PARTICLE01/LIGHT02 on RHAND; slot 3: 3 pieces mode 2 (world-aligned latch) on BODY2 | bone attach ✅, FLAIR/BLADE ✅; flash not billboarded/latched (D2, D3); hit sparks follow victim (D3) |
| Psycho Wave (eft0001) | 0 (0,1), 3 (2,3) | slot 0: 7 of 12 draws `0x3000` darken, 12 of 16 pieces latched, all billboards, 9 UV-scroll, three 4×2 sheets at 1 cell/tick | all additive, bone-following, stretched flipbook (D1, D3, D6, D7); no Psycho Wave footage exists to compare |
| Laser (eft0042) | 0,1,2,3 | slot 0: **240 `MSEFTBB01S` point sprites** (`0x1800`) + 8 `0x1400`; slot 1: TALE01 w18 = 17; fires tick 13 | flat quads (D2), additive (D1), `CAST_REACH` hack, short trail (D10) |
| Slide Laser (eft0043) | 0,1,2,3 | 24 plain pieces (2 billboards); 4 draws `0x1800` (DST_COLOR,ONE), 2 `0x1200`; TALE01 w18 = 25 | same; the missing "crescent" (FORMATS-EFFECTS "Still open") is SUSPECTED to be D1 (the `DST_COLOR`/`SRC_COLOR` draws) + D8, not missing data |
| Blaster (eft0053) | 0 (0,1), 1, 2, 3 | 72 `MSEFTBB01S` sprites (ONE,ONE), 17 `0x1200` draws, 2 TALE01 (w18 10 and 20), w30 flicker on 3 pieces | as Laser; comet trail plausible but short |
| Crystal Wall (eft0115) | 0 (0,1), 4 (2,3), 5 (4) | packed pieces are `PDEFTBB08` `0x1400` (additive by alpha, not opaque), latched shards (w28=1), cull on, slot-0 window 0-35; block = slot 4 at the guard object, break = slot 5 | slot 4 at victim root ✅~; **slot 5 never played** (D5); guard-object matrix unread |
| Shell family (Blaster) / arc (Laser) / ricochet (eft for Ricochet Laser: 0-3) | 0-3 | roll of the arc plane in the object matrix | `rotation_arc` facing has no roll (`fx.rs:1377`) — cosmetic |
| Lob (Bomb eft0031: 0-3) | 0-3 (+4/5 on one skill) | slot 2 ×6 on impact | unit scale (D4) |
| Fall (Meteor), crawl (Root of Tree), hole (Gravity Press), column (Lightning Bolt: 0,1,3), mine (Timer Mine: 0-3), tornado (Twister: 0,1) | per motion | column: slot 2 replaces the marker (`FUN_140267410`); mine: slot 2 ×10 | area effect played as slot 1 loop (`fx.rs:1405-1411`); marker disc is a stand-in |
| Rain (9 skills) | 0-5 | slot 5 cloud, slot 4 per drop | not implemented (D5) |
| Status self-skill (e.g. Optical Camo; 34 skills, slot 0) | 0 (+1 with 4/5) | slot 0 at cast; persistent loops only for statuses 8/11/33 (effects 518/519/521) | slot 0 ✅; loops ❌ |
| Environment (30 skills) | 0 only | slot 0 at cast; crystal itself is not an effect (`FUN_14017bcc0` writes a table + sound) | cast ✅; crystal = cube stand-in (`fx.rs:1426-1436`) |

## 4. Effect-field decoding: unknown or guessed fields that change the look

`engine/re/effect_fields.py` is only a histogram tool (no decoding). Measured with it over `cache/fx`, then matched to the program's per-draw code (`FUN_1401a5940/5e90/5f20/5ff0/a9ae0`, BB01 class):

| 0x13 word | Program meaning (BB01 class) | Our use |
|---|---|---|
| 0,1 | flipbook columns, rows | ✅ |
| 2,3 | start, end (inclusive) | ~ exclusive end |
| 4,5,6 | move along own axes × curve(w8) | ✅ |
| 7 | **roll degrees** × curve(w8) (billboard or mesh) | ❌ |
| 8,9,10,11 | curve index for move / scale / colour / (unread here) | ✅ (w11 unused) |
| 12,13 | colour a → b | ✅ |
| 14,15 | colour pair used on odd frames when w30 ≠ 0 | ❌ |
| 16,17,18 | scale added × curve(w9) | ✅ |
| 19 | **0 = billboard, no cull; ≠ 0 = mesh, back-face culled** | ❌ |
| 21 | flipbook mode (0 = 1 cell/tick loop, ≠ 0 = stretched) | ❌ |
| 22,23 | **UV scroll** × curve(w8) | ❌ |
| 28 | **root-matrix mode 0-8** | ❌ |
| 29 | time clamp 1 = hold first, 2 = hold last, 3 both (16 pieces) | ❌ |
| 30 | **odd/even colour flicker** | ❌ |
| 31 | **colour fade in/hold/out window** | ❌ |
| 20, 24-27 | not read by this class (20 = 0/1/.5…; 25 and 27 default 20; 24/26 = 1 on 4×2 sheets) — ours reads 24-27 as a flipbook range, harmless because it equals the default | ❓ |
| material +0x0C bit 0x10, +0x14 | blend on/off and equation (D1); +0x0C bit 0x4000 and `+0x3c..` shorts pick sampler address modes (`FUN_1401a5e90`, default 2) | ❌ |

## 5. Reads still needed

1. **Attacker's slot-3 hit effect**: `FUN_140258690`'s 6th argument (`param_6` = effect number played on the victim, `FUN_140258690`:262) is a stack argument the decompiler drops at `FUN_14024fd80` (slot 9 of `pdNormalPlayerClass`, vtable `0x14048e3b0`) and in the debug path `FUN_14015d980:140`. Needs a disassembly of the call site (list for Ghidra owner).
2. **Guard object** (`0x140264020` init / `0x140264340` update; `DecompAt.java`): matrix used for slot 4/5, whether slot 0 loops while the guard is up, how long Crystal Wall's 35-frame slot 0 lives vs its ~23-tick guard.
3. **Who releases a cast effect on interruption**: callers of `FUN_140259200`, `FUN_14025a1c0`, `FUN_140259670`; what a hit-cancel does to the running slot 0.
4. **Effect update call rate**: confirm the manager's `update` (vtable `0x14047bd98`, `FUN_1401899b0`) runs once per 30 Hz step like the player's (`0x14048e3d0`); PROGRAM-RE ❓ still open.
5. **`MSEFTBB01S` inputs**: vertex format, where the per-vertex point size comes from, `cb0[32].x`, `cb2[...]` corner table; how our exporter turned those draws into quads.
6. **TALE01**: registration of the kind → `pdEftTale03/04`, which word is the node count, sampling per tick vs per draw; `pdEftThunder2` (THUNDER01), MSBILLBEAM00, YKFADE00, MSParticleFunc00-03 (puff counts/sizes).
7. **Other shader classes** (`PDEFTBB02/04/08`, `PDMTEX01`, vtables `0x14047a958` …): confirm they share `FUN_1401a5940/5f20/5ff0/a9ae0` (BB02/PDMTEX01 draw fns seen: `0x14011c6c0…`, `0x140122090…`).
8. **Distortion** (`PDEFTOPT05`, 1,367 skill draws) and the glow/mask target (`o1`): what the game does with the mask; screen-offset texture.
9. **Environment crystal** readers of table `0x14058bd30`; motion-4 instant effects' visible part (13 of 20 have slot 3, 3 have slot 5).
10. Stage `EF00` object effects are outside this audit (FORMATS-STAGES.md).

## 6. Proposed build lanes (ordered)

**Lane A — Render semantics (D1, D2, D8, D9, D13).** Goal: per-draw blend, billboard/roll, cull, alpha rules, point-sprite draws, correct curve 2.
Files/functions owned: `pd_formats/src/effect.rs` (+ `alr.rs` material `+0x0C/+0x14`), `pd_convert effects` (write `flags`, `blend` state word, `mode`), `pd_game/src/effects.rs` (`FxDef::parse`, `attach_pieces` material build, `curve`, `animate_pieces` billboard/roll/colour), `fx_gamma.wgsl` (src/dst factor selection; ONE,ONE must not multiply by alpha), a new point-sprite material/shader for `MSEFTBB01S`.
Tests (from the reads): decode table `0x1402f77c0/0x1402f7798` → `0x3000` = (ZERO, 1−SRC_COLOR), `0x1800` = (DST_COLOR, ONE), flag bit `0x10` off → opaque/alpha-test; `eft0001` packed m0_d0 = darken, `eft0042` = 240 point sprites; w19 = 0 ⇒ billboard; curve 2 equals `e(2d−e+1)/(d(d+1))` and reaches 1 at e = d; fresh `pd_convert effects` reproduces the new fields (no cache patches).
Runtime proof: `PD_SHOT_TICKS` sheets (both split views, same tick) for Psycho Wave p0-p30, Laser p0/p27/p57, Blaster; before/after; no-effect control; localised pixel diffs on the caster cluster only. Laurence picks from an A–F sheet for the dark-vs-glow Psycho Wave look.

**Lane B — Piece timeline semantics (D3, D6, D7, D8-flicker/fade, D13 clock).** Files: `effects.rs` (`FxPiece`, `bind_bones`, `animate_pieces`, `tick_effects`). Word 28 modes (latch on first drawn frame, world-aligned), slot-3 snapshot root, flipbook rate (w21), UV scroll (w22/23), w30/w31 colour, inclusive window, optional 30 Hz frame quantisation (Laurence's choice). Sequential after A (same file). Tests: a latched piece keeps its first-frame matrix when the bone moves; cell index = (t−start) mod cells when w21 = 0; UV offset = w22/23 × curve; colour parity. Proof: tick-matched Psycho Wave with the dash (frames 10-20), knife flash, hit sparks on a knocked-back victim.

**Lane C — Events and missing slots (D4, D5, D10, D11 prep).** Files: `fx.rs::skill_effects` / `sync_combat_visuals`; minimal additive `CombatEvent` variants in `combat.rs` (charge hold/release, guard broken, landing, status start/end) — **combat.rs is shared; an integrator must own that part**; slot-2 scale = radius × 0.2 from the skill data; slot 5 break; Rain cloud/drops; status loops 518/519/521, 520, 512; warps. TALE01 ribbon once read (Reads 6). Tests: Bomb burst scale 6.0, Big Bomb 8.0, Timer Mine 10.0; Crystal Wall break plays slot 5; charge skill plays slot 4 then 5; Rain plays 5 then 4. Proof: scripted captures of Bomb, Crystal Wall block+break, Gungnir charge, Fire-rain.

Order: A → B, C in parallel with A after the event API is agreed.

## 7. Questions only Laurence can answer (multiple choice)

1. **Psycho Wave look** — the program darkens under the rings (multiply-darken), we add light. Faithful means it will look darker/inkier than today. (a) Faithful (recommended) (b) Show me an A–F sheet first (c) Keep the bright glow. Also: can you record the original's Psycho Wave (cast + hit, close, one take)? There is no footage of it.
2. **Charge glow size** — the original's Laser/Blaster charge sprites keep a constant on-screen size (they get bigger when the camera is close). (a) Faithful (recommended) (b) Fixed world size like today.
3. **Effect motion** — the original steps effects once per 30 Hz tick; ours is smooth. (a) Smooth (current) (b) Stepped like the original (c) Show both on a clip.
4. **Extra original effects** not played today (landing dust, status glows for invisible/floating/void, charge loops, guard-break shards, rain clouds). (a) Add them all as faithful (recommended) (b) Add only guard-break + charge first (c) Skip for now.
5. **Next footage to record** (pick up to two): (a) Crystal Wall block then break (b) Bomb explosion from the side (c) Laser charge close-up (d) Gungnir charge and release.

## 8. Events lane results (lane C, 2026-10-01)

Built in `fx.rs`: slot-2 impact scale = blast radius x 0.2 (`BlastStarted`: Bomb x6, Timer Mine x10, Photon Wave x8); the slot-3 hit effect is made with a copy of the victim's matrix at the hit (bone-bound pieces still ride their bone); a guard break plays slot 5 and never slot 4 (a block plays slot 4); charge skills loop slot 4 twice from the charge-cycle tick and play slot 5 on release; Rain's cloud is slot 5 following the target with a slot 4 per drop; persistent status loops 518 / 519 / 521 with slot 5 at the end (Optical Camo's 518 slot 4 is all screen distortion, hidden until `PD_FX_DISTORT`); `Burst` no longer plays slot 3 (a victim's `Hit` does).

Corrections: **520 landing dust is the jump-pad launch** (floor attribute 6), not a landing; **reaction row 42 -> 512** is reachable only from the debug viewer; the Rain cloud's slot 5 also needs no slot 1 at the area. Not built: the second charge loop's half-loop phase shift (needs the effect length, private to `effects.rs`), s20 / slot 1 per Rain drop.

