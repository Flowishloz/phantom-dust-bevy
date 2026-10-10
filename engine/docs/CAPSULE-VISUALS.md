# Capsule and Aura Particle visuals — where the game stores them (research, 2026-09-27)

> **Status (archivist, 2026-10-03):** research history plus the draw recipe. The orb is now drawn by `orb_material.rs` with `orb_sphere_lit.wgsl` / `orb_sphere_unlit.wgsl` / `orb_rim.wgsl`; `orb_core.wgsl` and the `PD_ORB_*` env tunables / `OrbTuning` described in the 2026-09-28 "Shader" sections no longer exist. Ownership of capsule draw layers: `handoffs/H5-orb-ownership.md`; birth effect: `capsule_birth.rs`.


Goal: replace the placeholder capsules (`fx.rs`: a 2.5 u unlit sphere per capsule, colours from `CAPSULE_COLORS`) with the original's look. This page is about the **art and effects only**; the rules (spawn spots, 14 s respawn, 15-tick appear) are already in PROGRAM-RE.md "Aura and capsules".

Written in a cloud session **without game files or the exe**, so everything below is leads from our own docs plus the web, ranked. Each needs a probe on Laurence's machine (pd-reverse-engineer). Legend: ✅ confirmed · ⚠️ likely · ❓ unknown.

## Leads, most likely first

1. **Effect numbers 600–609, named "aura" in `Effect/name.lst`** ⚠️ (FORMATS-EFFECTS.md line 22/124). They "render nothing visible in the viewer". Likely reasons: (a) they are drawn with the `PDEFTAURA01` shader, which the viewer may not handle; (b) they are made only of emitter kinds (PARTICLE01/02, LIGHT02, glow) that the viewer didn't draw when that note was written — several emitter kinds have been added since; (c) they're only in the packed archive or only loose. **Probe:** list 600–609 in `e.i` vs loose; dump pieces/emitters/shaders per slot with `pd_convert effects`; re-render them in today's viewer. If they hold a floating orb + a pickup burst, this is the Aura Particle (and maybe every capsule, tinted).
2. **The 43 loose-only effects** (500s + others, not in `e.i`) ❓: the game reaches them "some other way" — the same other way system objects like capsules would be loaded. 505/506/507 are cost/auto-capture; check the rest of 500–517 and anything else loose-only for a capsule appear / pickup / vanish.
3. **`PDEFTAURA01` shader** ⚠️: an effect shader by name (FORMATS-EFFECTS "shader names"). **Probe:** which effect numbers use it; read `PDEFTAURA01_ps.cso`.
4. **`hud1` glow sprites** ⚠️ (HUD.md: 70×70 cells, "aura/capsule glows, likely in-world capsule art") in `Menu/BtMenuEn.alr`. If leads 1–3 give a mesh, these may be the billboard halo round it; if not, a camera-facing sprite of these is the cheapest faithful capsule.
5. **The program's glow-sprite system** `pdvkGlowExec` / `FUN_14028d7c0` (occlusion-queried; FORMATS-EFFECTS LIGHT02 note) ❓: separate from effects; could be how capsules shine. **Probe (Ghidra):** callers of `FUN_14028d7c0`, and what the capsule code draws.
6. **The capsule code itself** ✅ addresses: `FUN_14014ee70` setup, `FUN_14014fd10` take/remove, `FUN_14014ed10` per tick, `FUN_14014f320`. The web lists a dev source file name `capsule.cpp` among the exe's strings (Phantom Dust Wiki, Modding/Executable) — search that string in Ghidra, follow its xrefs, and look for effect requests (`FUN_14018cd10` with a constant number), model loads (a `.alr` path string) or glow calls. Appearing plays sound 0x55; the effect/model call is probably next to it. This is the one lead that gives a definite answer — do it first if Ghidra is open.
7. **Story/system effects 3000s/4000s** (packed-only): unlikely but cheap to scan once lead 1's tool exists.

## Colour check (affects HUD and capsules)

Community sources give the capsule colours as **Aura white, Attack red, Defence blue, Erase purple, Status green, Environment aqua, Special yellow** (StrategyWiki "Phantom Dust/Combat"). Our HUD.md assumption has **erase cyan, status magenta, environment green** — i.e. erase and status look swapped, and environment is aqua not green. Our own `fx.rs` already calls the environment crystal "aqua". ⚠️ Not changed in code yet: confirm against a screenshot/footage before swapping (`hud.rs::capsule_color`, `fx.rs::CAPSULE_COLORS`, the hud0 orb mapping).

## Footage to capture (for the A–F sheet later)

A capsule appearing (15 ticks), idle on the floor, being captured, and an Aura Particle being used — close up, both a skill capsule and an Aura Particle.

## Next steps (local machine)

1. Ghidra: `capsule.cpp` string → capsule draw/effect calls (lead 6).
2. `pd_convert effects` dump of 600–609 and the loose-only 500s; re-render in the viewer (leads 1–2).
3. Whatever is found: export into `cache/`, replace the sphere in `fx.rs`, A–F sheet for Laurence.

## Probe results 2026-09-28 (research pass, no game-code changes)

Ran on Laurence's machine with `pd_convert effects`, `drawinfo`, `texprobe`, `dumptex`, `sprites`, `hudtex`, and `pd_viewer --fx`. No `crates/pd_game` code changed; no tooling source changed either (all existing subcommands already covered what was needed). Images below are local-only (`engine/shots/orb/`, gitignored) — not committed.

### Lead 1 — effects 600–609 ("aura") — ⚠️ partially confirmed: real geometry exists, but our glTF exporter currently drops it

- **Existence**: only **600–606** exist (loose `.eft`+`.alr` and packed in `e.i`; `pd_convert effects` reports "7 from the packed archive, 0 loose-only" for `600-609`). **607–609 don't exist at all**, packed or loose.
- **Names** (`Effect/name.lst`, Shift-JIS, decoded directly with `System.Text.Encoding.GetEncoding(932)`): all seven (600–606) are literally **オーラ** ("aura"), confirming the existing name.lst read.
- **600 is a real, non-trivial effect** — three models (`m0`/`m1`/`m2`, escalating complexity: 4 / 8 / 10 draws), each built from a 160-vertex, 468-index mesh (`drawinfo`, `texprobe`: bounding box ≈ 1.7 × 0.65 × 0.67 units, **not a degenerate/placeholder shape**). Every piece is attached (`joint` field) to a **character bone**: `RHAND`, `LHAND`, `RTEKUB`, `LTEKUB` (wrists), `RNINOU` — i.e. **this plays on the caster's own hands/wrists/knee, not as a floating pickup on the ground.** Shader is `PDEFTAURA01` on every piece.
- **601–606 are genuinely empty placeholders**, not a viewer bug: `grow` is `[0,0,0]` and `colors` are all `0` on their one piece (shader `PDEFTBB01`, not `PDEFTAURA01`) — zero-size, invisible by construction. `607–609` don't exist to check.
- **Textures** (`dumptex` on the loose `eft0600.alr`; `shots/orb/eft0600_tex/tex0{0,2,4}_Dxt1_64x64.png`): three used textures, all soft white/grey **S-shaped wispy glow streaks** on black (DXT1, additive-looking) — exactly what a rising wisp of aura energy should look like. Two more textures in the file (tex01 plain white 32×32, tex03 a vertical soft cloud 32×128) are in the ALR but **unused by any draw** (`texprobe`: 0 used-indices each) — leftovers/shared-pool textures, not part of this effect.
- **Why the viewer shows nothing** (`shots/orb/eft0600_f{0,10,20,30}.png`, zoomed crop `eft0600_f20_crop.png` — nothing visible at the hand at any frame): `pd_convert drawinfo` shows the vertex format is **0x20 with a 7-entry skin palette** — i.e. **this is a skinned mesh** (like a character part), not the plain static-quad format every other effect piece uses. `pd_convert effects`'s glTF export (`gltf::export_glb`, shared with the generic exporter) produced **zero meshes** for `eft0600.glb` even though `texprobe`/`drawinfo` prove the geometry and indices exist — the palette/skinning path for this vertex format isn't handled by the effect exporter (it works fine for skinned characters via a different code path in `character.rs`). **This is a tooling gap, not evidence the source data is blank** — the note in this doc's lead 1 ("render nothing visible in the viewer") is explained: the exporter silently drops the mesh, so nothing ever reached the glb to render. Fixing it (teaching `pd_convert effects`'s export path to handle a palette/skin vertex format like `character.rs` does) is a real follow-up task, out of scope for this research-only pass.
- **Net read**: 600 is very likely the **on-character aura glow** (wisps of light rising from the hands/wrists when aura is used/gained), tiered in three levels of richness (m0 < m1 < m2, maybe low/med/high aura). It is **not** shown to be the floating ground capsule/orb model — nothing in 600–606 places geometry away from the character's own skeleton.

### Lead 2 — loose-only 500s — ❌ ruled out for a capsule appear/pickup/vanish effect

Decoded every loose-only number's name (same Shift-JIS method): 501 environment crystal appear, 502 environment crystal break, 505 life cost, 506 level cost, 507 auto-capture, 508 ability restore, 509 ability erase, 510 level erase, 511 ability seal, 513 ability-use damage, 514–517 status UP/DOWN/RESET/change. **522–550 have no name.lst entry at all** (consistent with the doc's existing note that `ecopy.pl` copied `eft0517` into them as dev placeholders — nothing new there). None of the 500s name a generic capsule appear/idle/pickup/vanish; the closest are 501/502, already known to be the **environment crystal** specifically, not the general capsule. **Ruled out** as the source of a generic capsule appearance.

### Lead 3 — shader `PDEFTAURA01` — ✅ confirmed: effect 600 is the only user, in the whole game

Searched every already-exported `cache/fx/*.fx.json` (539 files, the full converted catalogue): `grep -l PDEFTAURA01 cache/fx/*.fx.json` returns **only `eft0600.fx.json`**. So whatever `PDEFTAURA01` draws, effect 600 (aura) is its sole use across all 476+63 effects — this shader is not shared with any skill or hit effect.

### hud1 glow sprites — ⚠️ strong new lead for the in-world orb look

`pd_convert hudtex`/`sprites` on `Menu/BtMenuEn.alr`'s `hud1` atlas (512×512): the 70×70 cell block at x=150/222/294/366, y=0..432 (`pd_convert sprites ... 1 32`) is **28 glow sprites, 4 columns × 7 rows** (`shots/orb/hud1_glows_4x.png`, 4×-zoomed nearest-neighbour crop; full atlas `cache/ui/hud1.png`, not committed):

| Column | Content |
|---|---|
| 1 | soft colour-tinted outer halo (warm gold/orange rows 1–4, blue/purple row 5, red/pink rows 6–7) |
| 2 | a scatter of tiny twinkling sparkle/dust points (gold dust rows 1–4, a solid blue-purple core row 5, a bright radiating starburst with visible rays rows 6–7) |
| 3 | a small, plain hot-white core (the "filament") |
| 4 | either a colour-tinted ring/halo border (rows 1–3) or a plain grey ring (rows 4–7) |

This reads as a **layered billboard sprite for a glowing floating orb** — halo + sparkle + hot core + ring, in several colour/intensity variants — exactly the shape a camera-facing capsule/pickup glow would need, and matches this doc's existing note ("likely in-world capsule art"). It is **not yet matched to specific capsule types** (only 3 colour families are visible — warm gold, blue-purple, red-pink — not all 7 capsule colours), and nothing here proves it's used for capsules specifically rather than some other in-world glow (lock-on, hit spark). **Best next step**: lead 6 (Ghidra `capsule.cpp` xrefs) to find the actual draw call and confirm whether it points at this atlas.

### Images (all under `engine/shots/orb/`, local only)

- `eft0600_tex/tex00_Dxt1_64x64.png`, `tex02_...png`, `tex04_...png` — the three wisp textures effect 600 actually uses.
- `eft0600_f0.png` / `_f10.png` / `_f20.png` / `_f30.png` — effect 600 played on `chars/pc00a` in the viewer at four frames; all four show **no visible glow** (confirms the exporter gap above).
- `eft0600_f20_crop.png` — zoomed crop on the right hand at frame 20, same result.
- `hud1_glows_4x.png` — the 28-sprite glow grid from `hud1`, 4× zoomed, the strongest lead so far for the floating capsule/orb's look.

### Footage (Laurence's recordings, 2026-09-28)

Laurence recorded close-ups (`engine/shots/orb/footage/`, gitignored; source videos `%USERPROFILE%\Pictures\Phantom Dust - Orbs`): `red_idle.png`, `mat_close.png` (materialise sheet, ~1.5 s), `collect_close.png` / `collect_sheet.png` (pickup, both an attack capsule and an Aura Particle visible together).

- **Idle capsule** (`red_idle.png`): a sphere with an **animated dark, swirling interior texture** (an attack capsule shows red/orange flame; the Aura Particle, from `collect_sheet.png`, shows a dark grey smoky/skull-like swirl instead), a **bright fresnel-style rim** in the capsule's colour (red for attack, white for Aura Particle), a **soft outer halo**, and a few tiny sparks drifting around it.
- **Materialise** (`mat_close.png`, ~1.5 s): many **gold sparkle points** scattered over a wide floor area drift inward and gather into a spinning ball/vortex; the orb then **pops in with a red flare burst**, and its halo fades down to the steady idle look above.
- **Pickup** (`collect_sheet.png`): the player walks into the orb; it vanishes with a few **gold sparks**. When the *Aura Particle* is consumed specifically, the tooltip confirms it ("Aura Particle, COST 0 … Increases Level by 1") and the **player briefly glows green**.

**Matching against what this pass found:**

- **The flame texture and the dark smoky/skull swirl are not eft0600–0606.** Those seven "aura" effects are joint-attached glow wisps (hand/wrist/knee), authored as thin grey/white S-shaped streak textures (`eft0600_tex/tex0{0,2,4}...png` above) with neutral grey vertex colours (checked: R=G=B on every piece, no tint baked in) — nothing sphere-shaped, nothing flame- or smoke-textured, nothing floating away from the character's own skeleton. They cannot be the idle orb's surface material. A flame flipbook and a smoke/skull flipbook of the kind the footage shows would most likely be found among the **skill effects that already use fire/smoke textures** (e.g. the fire-family skills' 4×2 flipbooks noted elsewhere in `FORMATS-EFFECTS.md`, or Bomb/Bomb Shot's dust-and-smoke pieces) if the orb reuses a shared texture — **not checked this pass**, since the idle orb's rim + fresnel + animated-swirl construction looks like a purpose-built shader (closer to `MSICE11`'s reflective-surface trick or a dedicated capsule shader than to a standard additive glow quad), which the Effect archive's per-skill files aren't likely to hold at all.
- **The gold sparkle materialise burst** (dust gathering into a spinning ball, `mat_close.png`) reads like the `PARTICLE01`/`PARTICLE02` point-sprite emitter kind already documented in `FORMATS-EFFECTS.md` ("point sprites… flying out in a random direction… accelerated… colour word 12→13"), run in reverse-feeling gather (or simply a wide spawn area with inward drift) — but **no effect number found this pass spawns particles over a wide floor area**; every `eft*` particle emitter looked at is small and centred on a character joint. This particle burst, like the idle sphere, is more likely a **capsule-specific system separate from the per-skill Effect archive** (consistent with this doc's original lead 2 read: "the game must reach [capsule-type visuals] some other way").
- **The red flare that pops when the orb materialises** could be a `FLAIR01` ray-burst (the emitter kind already documented for cast starbursts) — plausible by look, but not checked against any specific effect number this pass.
- **The green glow on aura pickup** was hoped to be eft0600 (it's the one "aura" effect that sits on the character's own hands/wrists), but its texture is neutral grey with no tint baked into any piece — if the game recolours it green at runtime that isn't visible in the exported data, and it's equally possible the green pickup flash is a **separate, not-yet-found effect or a hard-coded runtime tint**, not eft0600 itself. Left open.
- **Net conclusion**: this footage makes it clear the floating idle/materialise/pickup capsule visuals are **not** in the 600s "aura" effects, and don't match anything else dumped this pass either — they most likely live in whatever system lead 6 (`capsule.cpp` xrefs in Ghidra) will uncover, and possibly reuse fire/smoke textures from unrelated skill effects for the swirl. This raises leads 4/5/6 in priority relative to lead 1, which is now largely closed out (character-glow only, not the pickup).

### What remains unknown

- Whether the floating, pickable capsule on the ground uses the `hud1` glow sprites, a 3-D model somewhere unexamined, or something the packed effect archive doesn't cover at all.
- Whether fixing the skinned-vertex export for effect 600 would reveal a look worth reusing for capsules, or whether 600 is purely a self-buff aura glow unrelated to the pickup's appearance.
- Lead 6 (Ghidra `capsule.cpp` string and its xrefs) is still undone and remains the one lead likely to give a definitive answer — recommended next step, now more strongly indicated since 600–606 and the 500s are ruled out.
- Leads 5 and 7 (glow-sprite system callers, story/system 3000s/4000s effects) untouched this pass.
- Which effect number (if any) holds the flame flipbook / smoke-skull flipbook the footage shows for the idle orb's swirling interior, the wide-area gold sparkle-gather burst, and the materialise flare — none found in `eft0600`–`0606`, the loose-only 500s, or the stage object-effect system (`FORMATS-STAGES.md` "Object effects" — checked and ruled out this pass: that system is stage destructibles/debris, not capsules).
- Whether the green tint on Aura Particle pickup is a runtime recolour of some existing effect or a distinct one not yet found (eft0600's own vertex colours are neutral grey, not green).

## Program read 2026-09-28 (lead 6 done — definitive answer for the idle orb)

Read from `PDUWP.exe` with the existing Ghidra export (`%USERPROFILE%\.pdtools\re-out\decomp_all.c`, `engine/re/decomp_fn.py`, `xref.py`, `exe_const.py`), then checked against the game files with `pd_convert info / texprobe / dumptex / drawinfo` and byte walks. No engine code changed. Legend as above: ✅ read in code and matched in data · ⚠️ one link inferred · ❓ open. (There is no `capsule.cpp` string in this build; the way in was the RTTI class names `pdvkCapsule2` / `pdvkCapEffect` / `pdvkEnvCapsule` and the render object the capsule setup creates.)

### Where the capsule art lives ✅

**Not** in the Effect archive and **not** the hud1 glow sprites: every capsule visual is a **model in `Map/st00.alr`** (the "common assets" stage, 1.7 MB), with its animations in **`Map/st00.dat`**.

- `st00.alr` is a type-0x11 container of **25 entries: 24 models + one texture block (47 textures)**; our ALR reader already parses all 24 models (`pd_convert info`). `st00.dat` (`ST00` magic) holds, after its 0x28 header, a list of size-prefixed 0x5 animation chunks in the same order.
- The program fills a 24-slot {model, animation} table at `0x140530390` (16 bytes per slot) in `FUN_1401d9210` from `Map\st00` (slot *i* = container entry *i*; animation *i* = the *i*-th record of `st00.dat`), cleared by `FUN_1401d9510`. Slot names come from the dev-name table at `0x1403ee490`:

| Slot | Dev name | What it is | Anim length (st00.dat, f32 at +8) |
|---|---|---|---|
| 0 | `st00_cap01_birth` | materialise sparkles (point sprites) | 429 frames |
| 1 | `st00_cap02_birth2` | materialise flare (20×20 u glow quad), **tinted by capsule colour** | 59 |
| 2 | `st00_cap03_death` | **pickup** burst (point sprites) | 89 |
| 3 | `st00_cap04_braek` | **erased / broken** burst (point sprites) | 89 |
| 4 | `st00_cap_aura` | idle orb — Aura Particle | none (static) |
| 5 | `st00_cap_att` | idle orb — Attack | none |
| 6 | `st00_cap_def` | idle orb — Defence | none |
| 7 | `st00_cap_era` | idle orb — Erase | none |
| 8 | `st00_cap_sta` | idle orb — Status | none |
| 9 | `st00_cap_spe` | idle orb — Special | none |
| 10 | `st00_cap_env` | idle orb — Environment | none |
| 11 | `st00_cap_e_aura` | how an **opponent's** Aura Particle looks to you | none |
| 12 | `st00_cap_e_other` | how an **opponent's** skill capsule looks to you (one look for all six kinds) | none |
| 13 | `st00_cap_env_c` | empty placeholder | — |
| 14 | `st00_hit_particle` | not capsule | 59 |
| 15 | `st00_end_of_world` | not capsule (End of the World) | 89 |
| 16–20 | `st00_cap_envc_A..E` | environment-crystal models | 29–30 |
| 21–23 | `st00_mdl_jump_emitter / particle2 / glow` | jump-point effect, not capsule | 90 / 90 / — |

### Code path ✅

- Each capsule slot record (the per-player block set up by `FUN_14014ee70`) owns a **`pdvkCapsule2`** render object (vtable `0x14048cae8`, created by `FUN_14021b270`) which owns a **`pdvkCapEffect`** (vtable `0x14048cb20`). Environment crystals use `pdvkEnvCapsule` (vtable `0x14048cb58`). `FUN_14021b3d0` sets the object's matrix: identity **yawed by slot index × 90°** (`FUN_1402906e0`), placed at the slot's position. **No scale — the model is drawn at its authored size. No spin or bob in code.**
- **Idle orb draw** `FUN_14021b840` (Capsule2 vtable slot 4): only while the slot state (+0x14) is 1 (appearing) or 2 (present). Model slot = **4 + capsule kind** for your own capsules; for another player's capsules **11 if it is an Aura Particle, else 12**. Kind = in-memory skill record +0x0E = our **s9** (checked over all 399 skills: 1 attack 185, 2 defence 48, 3 erase 44, 4 status 45, 5 special 47, 6 environment 30; Aura Particle = 0). Drawn in its static pose through the normal model display list `FUN_14012e6a0`. **No colour tint** — each kind's colour comes from its own textures.
- **Camera-near fade** `FUN_140196c40`: per viewer, alpha −16 per tick while that viewer's camera is within **10 u** of the orb (squared distance 100.0 at object +0x80), +16 per tick otherwise.
- **Materialise and flare** `FUN_14021ba60` (CapEffect vtable slot 4; `FUN_14021ba20` counts ticks):
  - state 0 (the 420-tick respawn wait): draws **slot 0 `cap01_birth`** at animation frame = elapsed × 420 / wait length, i.e. its **429-frame animation is stretched over the whole 14 s wait** (the visible gather is presumably only its last part, ~1.5 s in the footage ⚠️), colour white.
  - states 1–2 (15-tick appear, then present): draws **slot 1 `cap02_birth2`** at frame = ticks since appearing began (59 frames ≈ 2 s, then held on the last frame), **tinted with the capsule colour table** below — the coloured flare pop (red for attack, as in the footage).
  - The colour's top byte is 1 only for the owner's own capsule (a flag to the draw; meaning ❓).
- **Take / remove** `FUN_14014fd10` → `FUN_14021b4f0`: a normal pickup (param 0) spawns **slot 2 `cap03_death`** at the orb with a random rotation and calls `FUN_14022e8a0` with the owner (not read; candidate for the on-player pickup glow ❓); an erase (param 1) spawns **slot 3 `cap04_braek`** + sound 0x5C. Appearing plays sound 0x55 (`FUN_14014ed10`, already documented).

### Capsule colour table ✅ (settles the erase / status / environment question)

Table at `0x1402dca00`, 7 × D3DCOLOR (ARGB), indexed by s9; used to tint the birth2 flare:

| s9 | Kind | ARGB | Colour |
|---|---|---|---|
| 0 | Aura Particle | FFFFFFFF | white |
| 1 | Attack | FFFF3333 | red |
| 2 | Defence | FF3333FF | blue |
| 3 | Erase | FFFF33FF | **purple / magenta** |
| 4 | Status | FF33FF33 | **green** |
| 5 | Special | FFFFFF33 | yellow |
| 6 | Environment | FF33FFFF | **aqua / cyan** |

StrategyWiki is right and our HUD.md assumption (erase cyan, status magenta, environment green) is wrong: **erase = purple, status = green, environment = aqua.** The orbs' own textures agree (erase purple nebula, status green, environment blue-white). Code not changed here: `hud.rs::capsule_color`, `fx.rs::CAPSULE_COLORS` and the hud0 orb mapping need the swap.

### What an idle orb is made of ✅ (geometry from `drawinfo`, textures from `dumptex`)

Every idle orb (slots 4–12) has the same three layers, centred on the model origin:

1. **Sphere, radius 4.5 u** (~400–490 vertices; vertex format 0x03 or 0x1D) — the swirling interior. Shader **`vkCAP02` / `vkCAP02M2`** (`game_files/vkCAP02*_ps.cso`, disassembled): base texture × lighting, perturbed by **two scrolling normal maps** (st00 tex 6 / 7, the green-red noise maps), hemisphere + directional light, and a **tight specular highlight (N·H)^16** — the bright coloured edge in the footage.
2. **Shell, radius ~6.3 u, 3.9 u deep** (format 0x09) with st00 tex 8 or 28 (white / grey cloudy noise) — the soft glow round the sphere. Shader family `vkCAPM2M` / `vkCAPM2A` (two scrolled layers, multiplied or added, × vertex colour).
3. **Flat disc, radius ~8 u** (format 0x15) with st00 tex 9 (white liquid swirl) — the halo.

Per-kind textures (sheet `engine/shots/orb/capsule_textures_by_type.png`; all 47 in `engine/shots/orb/st00_tex_sheet.png`, PNGs in `engine/shots/orb/st00_tex/`, local only):

| Kind (slot) | Sphere texture | Other layers |
|---|---|---|
| Aura (4) | tex5 grey chrome swirl | tex 8, 9 |
| Attack (5) | **tex10 red-orange flame** (the footage's flame) | tex 8 |
| Defence (6) | tex11 blue water | tex13 starfield, tex14 blue dots |
| Erase (7) | tex15 purple nebula | tex17 dark stars, tex 8 |
| Status (8) | tex18 green swirl | — |
| Special (9) | tex21 gold streaks | tex22 gold liquid, tex23 grey smoke |
| Environment (10) | tex24 blue-white | tex25 |
| Opponent's Aura (11) | tex28 dark grey smoke | tex 9 |
| Opponent's skill (12) | tex29 dark red / pink bands | tex 28, 9 |

The footage's "dark smoke / skull swirl" Aura Particle is most likely **slot 11 (tex28)**, i.e. the opponent's view, or slot 4 (tex5) lit dark ⚠️ — check whose capsule was filmed.

- **Swirl animation** ⚠️: the orbs have no animation; every capsule vertex shader offsets each UV layer by constants `c90.xy / c90.zw / c91.xy`. Each draw is preceded by a 0x13 chunk whose floats at +0x18 are small per-layer (u, v) pairs — attack sphere (0.002, 0.008), (−0.002, 0.003), (−0.003, 0.012); shell (−0.006, 0.001), (−0.002, 0); the first chunk also has 360.0 at +0x0C and later ones (0.5, 0.5) at +0x38. Read as **UV scroll per tick per layer** (0.5, 0.5 = UV pivot). Fits the look, but the 0x13 reader wasn't traced (`FUN_140295d90`, the 0x13 slot of the chunk-handler table at `0x140410608`, reads a different layout) ❓. **Resolved — see "Draw recipe (decoded 2026-09-28)" at the end: confirmed UV scroll per frame, read by the shader's own per-draw function; the (0.5, 0.5) is a fixed UV offset, 360.0 is not read by the orb shaders (no spin).**
- **Sparkles, pickup and erase bursts** (slots 0, 2, 3) use vertex format **0x18 = position + D3DCOLOR point sprites**, drawn with `vk_cappspriteV_vs` / `vk_cappsprite_ps` (screen-sized points, texture × vertex colour). Slot 0: 6 draws on 6 animated joints, 78 points each spread over ~11 u (the wide floor area in the footage), textures tex0 (orange gradient strip) / tex1 (white dot). Slots 2 / 3: tex4 (orange gradient) — the **gold sparks**. **For FORMATS-ALR:** 0x18 is "position only, not drawn" on stages, but here it is a drawn point-sprite format; our exporter drops these draws.
- **Flare** (slot 1): one 20×20 u flat quad (format 0x15), tex2 soft radial glow, 59-frame animation, tinted with the colour table.

### Unknown (not chased, budget)

- ❓ The exact 0x13 chunk layout (scroll speeds vs other meaning; what 360.0 is — possibly a rotation period, i.e. a slow spin).
- ❓ What the 0x5 animations in `st00.dat` do to slots 0–3 (sparkle-cloud joint motion, flare scale / fade); same chunk format our animation reader handles — not run.
- ❓ `FUN_14022e8a0` on pickup (green glow on the player for Aura Particles?) and the Aura Particle → Level +1 code (still not located).
- ❓ Blend mode per draw (material bytes not decoded); by look shell, halo, flare and sprites are additive.
- The hud1 glow sprites are **not** used by this draw path; probably HUD-only.

### Build recipe — replacing the placeholder sphere in `fx.rs`

1. **Export** (converter lane): `pd_convert model game_files/Assets/Data/Map/st00.alr <out>.glb --no-props --unlit` already writes 23 models / 22 textures (tested into `shots/orb/st00/`, local). Needed: one named node per container entry (slot table above) instead of one merged scene; keep the 0x18 point-sprite draws (as points or small camera-facing quads); attach `st00.dat`'s 0x5 animations to slots 0–3.
2. **Idle orb**: spawn slot `4 + s9` at the capsule spot at **native scale (sphere radius 4.5 u, halo ~8 u — the placeholder is 2.5 u)**, yawed slot × 90°. (Opponent looks 11 / 12 are per-viewer; in split-screen each camera would need its own layer — start with the owner look.) Materials: sphere = unlit texture + additive rim / specular in the kind's colour; shell and halo = additive, unlit; scroll each layer's UVs by its 0x13 values × 30 per second. Fade out when that player's camera is within 10 u.
3. **Materialise**: play slot 0's sparkles over the end of the 420-tick wait (the program stretches its 429 frames over the whole wait); at the start of the 15-tick appear, play slot 1's flare tinted with the colour table for 59 frames.
4. **Pickup**: slot 2 (gold sparks) at the orb, random rotation, 89 frames. **Erase**: slot 3 + sound 0x5C.
5. **Colours**: swap `CAPSULE_COLORS` / `hud.rs::capsule_color` to the table above.

## Built 2026-09-28

Replaced the placeholder sphere with the recipe above, close to the letter where `Combat` gives `pd_game` enough to go on, approximated where it doesn't.

### Converter (`pd_convert capsules <st00.alr> <st00.dat> <out_dir>`, new subcommand)

- One named `.glb` per model slot (`cap00_cap01_birth.glb` .. `cap23_mdl_jump_glow.glb`, `ST00_SLOT_NAMES` in `main.rs`), not a merged scene, so `pd_game` loads exactly the slot it wants. `capsules.json` lists each slot's glb (if it has drawn triangles), points sidecar (if it has point-sprite draws) and `st00.dat` animation length.
- **0x18 point-sprite draws kept**: `alr::light_layout` learned format 0x18 (position + D3DCOLOR, no UV -- previously undocumented for this format id) so `gltf::point_sprites` can read each point's position and colour directly, bypassing the triangle mesh path entirely (a point list isn't triangles; forcing it through the generic exporter under `--untextured` would triangulate nonsense). Slots 0/2/3's sparkle/burst pieces come out as `<stem>.points.json` (position + colour per point), not glb meshes.
- **`st00.dat` animation lengths**: byte-walked the format this session (`st00_dat_frame_lengths` in `main.rs`, full doc comment there) -- `ST00` magic, a slot count at +0x28, then per slot a u32 block size followed by that many bytes of one-chunk-per-joint-track animation data (same layout as `anim::ID_ANIMATION`) ended by a non-5 id. Walking all 24 slots this way lands exactly on EOF, and slot 0's 15 tracks all read length **429** -- exactly this doc's "Program read" table -- slots 1-3 read **59/89/89**, also matching. Recorded as `anim_frames` per slot in `capsules.json`; the actual per-joint keyframe curves (the sparkle-cloud drift, the flare's fade/scale) were **not decoded** -- `pd_game` times its own procedural playback off the frame count only, not the original curves.
- Also dumps `tex00/01/02/04.png` (st00's sparkle/flare/spark textures) alongside, for the point-sprite pieces' particle-burst approximation below.

### `pd_game` (render-only; `sim.rs`/`combat.rs` untouched)

- **Idle orb**: `fx.rs::CapsuleOrbs` loads every idle-orb slot's scene (4-10 by kind, 11/12 for the opponent's view via `COLOUR_TO_ORB_SLOT`, which also documents the s9-vs-our-colour-index ordering mismatch). Spawned as a `WorldAssetRoot` scene at the model's own native scale (no manual radius -- the exported geometry is already the doc's ~4.5/6.3/8 u layers), yawed by the capsule's own id mod 4 x 90 deg (an approximation: `Combat::CapsuleItem` doesn't carry which of the 4 skill slots a capsule is for, only its id, so the rotation is stable per capsule but not guaranteed to match the original's per-slot rotation).
- **Materials** (`style_orb_materials`): the generic exporter gives every layer a flat opaque texture. The innermost draw (`_d0`, the sphere by drawinfo's own increasing-radius order) is kept normal-blended so its swirl texture and colour read clearly; outer layers (shell, halo) are additive and dimmed 60% -- a first pass with every layer at full additive washed the sphere out into one bright veil (worth comparing against `shots/orb/build/sheet_attack.png` if it recurs). No per-vertex UV scroll (the doc's own per-layer scroll constants are marked unconfirmed and the 0x13 chunk that would carry them per-draw wasn't decoded this pass) -- the orb is static instead of slowly swirling; flagged here rather than guessed at.
- **Fade near camera** (`fade_capsule_orbs`): reproduces the ±16-per-tick/10 u rule (`ORB_FADE_PER_SEC`) but as a scale-to-near-zero on the whole entity rather than a true per-layer material alpha (which would need a runtime-cloned material per instance); reads as a shrink-and-vanish rather than a cross-fade.
- **Materialise / flare / pickup** (`materialise_burst`, `flare_burst`, `pickup_burst`): triggered off `Combat::capsules` appearing/disappearing between frames (`sync_combat_visuals`'s `prev_caps` diff), since `Combat` only ever lists a capsule once it's already present -- the 14 s respawn wait and its stretched 429-frame sparkle animation aren't visible to `pd_game` at all. What's built: a ~1.4 s gather/fade sparkle burst plus the colour-tinted flare quad (`tint_flares`) play the moment a capsule enters the list (approximating the last ~1.5 s of the wait plus the appear, matching the visible span of Laurence's `mat_close.png` rather than the full 14 s); a capsule leaving the list plays the same gold-spark burst regardless of whether it was captured or erased (`Combat` doesn't say which -- slot 3's dedicated erase burst isn't wired in for want of that signal). The sparkle/spark bursts don't instance the real point clouds (slot 0 alone has 1,710 points) -- `PointStats` reduces each to an average colour and RMS spread, fed into the existing `effects::spawn_burst`/`BurstSpec` particle system (`Shape::Puff`) rather than a bespoke point renderer.
- **`PD_CAPSULE_PREVIEW`** (screenshot/proof tool only, `capsule_preview`): `=1` lines up every kind in front of player 1; `=close_attack` / `=close_aura` place one idle orb close up; `=mat` plays the attack capsule's appear (flare + gather) close up; `=pickup` plays the gold pickup burst alone. Purely additive rendering, reads `World` only for camera-relative placement, never touches `Combat`.

### Proof and what it shows

`shots/orb/build/` (local, gitignored): `sheet_attack.png` (footage vs. ours, idle attack), `sheet_aura.png` (ours, idle Aura Particle -- footage's own Aura Particle still wasn't captured close-up, so the sheet reuses the attack still for colour/lighting reference only), `sheet_materialise.png` (footage vs. ours, appear moment), `preview2.png` (all nine kinds lined up, most legible single check that the colours and textures are each kind's own, not a shared tint).

Looked at ourselves: the geometry, per-kind textures (flame/chrome-swirl/water/nebula/etc.) and colour-tinted flare are all faithful and clearly distinct kind-to-kind (`preview2.png`). What's off against the footage: ours reads as a soft, fairly uniform glowing ball, where the original has a much tighter bright rim around a dark, sharply-defined swirling core and a smaller, subtler halo -- our shell/halo dimming (60%) isn't aggressive enough, and there's no true fresnel/specular rim (the doc's `vkCAP02` shader does a real N.H specular highlight; `StandardMaterial` here is unlit, so the "rim" is only ever the flat texture's own bright edge). The materialise burst is present but faint at normal viewing distance (small point count, `PointStats`-derived spread rather than the real 429-frame stretched gather) -- visible as a thin scatter of sparkle points rather than the footage's dense gold cloud. Further tuning (stronger shell dimming, an actual fresnel term, a denser/brighter materialise burst) is a reasonable next pass but wasn't chased further this session to stay within scope.

## Fixed 2026-09-28 (VFX audit follow-up: dark core, tighter halo, real materialise points)

Root cause of the "soft uniform glowing ball" / "milky disc" complaint above, and of the orb-shaped part of VFX-AUDIT.md #1/#2: the core (`_d0`) was drawn **double-sided `AlphaMode::Blend` with `cull_mode: None`** "so its swirl texture read clearly" -- but a double-sided alpha-blended mesh has no fixed front/back draw order (Bevy sorts entities, not a mesh's own triangles), so the same sphere came out as an opaque-ish translucent disc, and -- worse -- a different, camera-dependent result in split-screen (full root-cause writeup: `FORMATS-EFFECTS.md` "Camera-dependent 'dark blob' / milky orb"). Fixed in `fx.rs::style_orb_materials`, render-only:

- **Core (`_d0`)**: single-sided `AlphaMode::Opaque` (`cull_mode: Some(Face::Back)`) instead of double-sided `Blend` -- removes the camera-dependence outright, no sort needed. Also switched from `unlit` to a lit, `metallic: 0.85`, `perceptual_roughness: 0.15`, `reflectance: 1.0` material: the program's own `vkCAP02` shader is a lit sphere with a tight `(N.H)^16` specular highlight (this doc's "Program read"), and Bevy's stock PBR (Cook-Torrance) gives the same dark-unless-near-a-highlight core plus a bright, colour-tinted grazing rim for free from the scene's existing light -- no new shader needed, since the highlight's tint already comes from the sphere's own per-kind-coloured texture.
- **Shell/halo dimming**: tightened from 40% (60% dimmed) to 22% alpha, per this doc's own "not aggressive enough" note above.
- **Materialise sparkles**: `materialise_burst` now instances the real recorded points (`CapsuleOrbs::materialise_points`, loaded from `cap00_cap01_birth.points.json`'s 1,710 entries) through a new `effects::spawn_point_burst`, evenly sampling up to 350 of them with their own real position/colour/alpha (their alpha is genuinely low, ~15/255, in the source data -- individually faint by design, meant to read as a cloud when many overlap) instead of `PointStats`'s single averaged colour/spread fed into the generic procedural `spawn_burst`. Falls back to the old averaged-stats burst if no `.points.json` sidecar is present (older cache).

**Proof**: `shots/orb/build2/` (local, gitignored) vs. the pre-fix `shots/orb/build/` -- `close_attack.png` (core+halo, both builds, same demo/tick so directly comparable: the halo is visibly smaller/less obtrusive and the core reads as a contained, dim, coloured sphere rather than a bright milky disc bleeding past the character), `preview2.png` / `preview2b.png` (all nine kinds, same near-mode framing). The materialise burst (`mat_f100.png`) is still faint at normal viewing distance -- matches this doc's own "faint at normal viewing distance" note above and the source data's low per-point alpha, not a regression, and wasn't chased further (would need either far more points instanced or a brightness boost that departs from the recorded data).

**Not done this pass** (flagged, not guessed at): the per-layer UV scroll (this doc's "Swirl animation" numbers are still ⚠️ unconfirmed and would need a vertex-shader UV offset, not just the fragment-only change made here) -- the orb is still static instead of slowly turning.

### Tests and smoke

`cargo test -p pd_formats -p pd_game -p pd_convert`: 7 + 36 + 272 passed, unchanged from before this work (no sim-side test needed touching; this is all render/converter code). `pd_game.exe --demo --screenshot` smoke runs clean (`shots/orb/build/smoke4.png` etc.), including split-screen with a real in-match capsule (blue Defence, `smoke4.png`).

## Shader 2026-09-28 (custom orb-core material: dark centre, fresnel rim, second texture layer)

Replaced the "Fixed 2026-09-28" core's lit-`StandardMaterial`-plus-real-lighting approach with a small custom Bevy material (`crates/pd_game/src/orb_material.rs`, `orb_core.wgsl`), closer to the letter of the brief ("write a small custom Bevy material for the orb core ... darken toward the centre, add a view-dependent fresnel rim tinted by the capsule colour, output additively/emissive so it glows"). Render-only; `sim.rs`/`combat.rs`/`effects.rs` untouched.

### What was built

- **`OrbMaterial`** (`orb_material.rs`) = `ExtendedMaterial<StandardMaterial, OrbCore>`, the same pattern `effects.rs`'s `FxMaterial`/`FxGamma` already uses for the skill-effect gamma fix. `OrbCore` carries a `params` uniform (rim power, rim intensity, core darkness, scroll speed), a `tint` uniform (the capsule's own colour) and a second texture binding (`layer1`) -- the mesh's own base texture, sampled a second time at an independent scroll for the "two scrolling layers" the doc's "What an idle orb is made of" describes (the actual per-draw UV-scroll constants there are still marked unconfirmed -- the 0x13 chunk reader was never traced -- so this is a plausible two-layer swirl, not recovered data).
- **`orb_core.wgsl`**: blends the two texture samples, computes a fresnel term (`pow(1 - N.V, rim_power)`), darkens the surface toward the camera-facing centre (`mix(core_darkness, 1.0, rim)`) and adds the capsule-coloured rim as **emissive**, then outputs directly (`base + tint*rim*rim_intensity`) rather than running it through `apply_pbr_lighting`. An earlier pass of this shader *did* call `apply_pbr_lighting`: with a high-metallic material (the previous build's "Fixed" pass) that turned the whole sphere flat grey (metallic surfaces have near-zero diffuse albedo); switched to non-metallic and diffuse-lit, it instead put a hard, stage-light-dependent bright/dark hemisphere split across the sphere that had nothing to do with the fresnel rim (visible in `shots/orb/calib/test8.png`/`test9.png` if re-run) -- wrong for an orb that should glow from its own texture/rim regardless of the stage's own lighting, closer to the original's own additive/self-lit `vkCAP02` shader than to a normally-lit surface. The direct emissive-style output avoids both.
- **Halo/shell** (`_d1`/`_d2` draws): unchanged from the "Fixed" pass -- plain additive `StandardMaterial`, no depth write, dimmed by `halo_intensity` (now one of the five tunables below rather than the hardcoded `ORB_HALO_ALPHA`).
- **Env-var tunables** (`OrbTuning::from_env`, `orb_material.rs`): `PD_ORB_RIM_POWER`, `PD_ORB_RIM_INTENSITY`, `PD_ORB_CORE_DARKNESS`, `PD_ORB_HALO_INTENSITY`, `PD_ORB_SCROLL_SPEED`, plus `PD_ORB_VARIANT=A`..`F` to pick one of six built-in presets in one go (individual vars still override on top of whichever preset). No rebuild needed between renders.
- **Bug found and fixed along the way**: `style_orb_materials`' `_d0`/outer split (which layer gets which treatment) was keying off `Name` on the mesh **primitive** entity, which the glTF importer always calls `"Mesh"` -- the real draw name (`m5_d0`, `m5_d1`, ...) sits on its **parent** node one level up. This bug (confirmed with a one-off `PD_ORB_DEBUG=1` eprintln, since removed) meant the core was never getting the special treatment at all -- every layer fell through as "outer" (flat additive), which is what made the sphere read as a uniformly translucent, texture-less disc in this session's first few test renders. Fixed by looking the name up on `ChildOf::parent()` instead of on the mesh entity itself. This bug likely predates this session (the "Fixed 2026-09-28" build above never printed or asserted the split), but wasn't caught until this pass added `OrbMaterial` and the wrong-branch symptom became visually obvious.
- **Calibration camera** (`fx.rs`: `PreviewCamera`, `spawn_preview_camera`, `isolate_preview_cameras`): `PD_CAPSULE_PREVIEW=close_attack`/`close_aura` and `=1` now spawn their own camera and deactivate every other camera (both gameplay `PlayerCamera`s and the HUD's `Camera2d`) for the rest of the run, instead of relying on the normal gameplay camera. The gameplay camera always looks at the locked-on opponent (`sim.rs`: fighters start `locked: true` and nothing in `--demo` ever toggles it off), so the orb -- placed relative to the player, not the opponent -- was never actually in frame; earlier attempts this session also tried a ground-level camera placed a fixed distance directly behind the orb, which looked straight through the player's own legs. The fix floats both the preview orb and its camera well above head height (`PD_ORB_HEIGHT`, default 14 u) so the shot is over open air on every stage, and yaws the orb itself (`PD_ORB_YAW`, default 150°) toward the camera.
- **Pre-existing exporter/asset gap noticed, not fixed**: the core sphere's own texture only covers roughly one hemisphere of its UV space -- the other hemisphere reads flat grey regardless of lighting or shader (confirmed by testing with lighting fully bypassed; also visible, on close inspection, in the "Fixed 2026-09-28" pass's own `shots/orb/build2/preview2b.png`, so this predates this session). `PD_ORB_YAW` works around it for the calibration shots by turning the good hemisphere toward the camera; real capsules are static and simply show whichever hemisphere the fight's own camera happens to catch. Not chased further -- would mean digging into `pd_convert capsules`' UV export, out of this pass's scope (fx.rs/shaders/docs only).

### Proof: `engine/shots/orb/calib/` (local, gitignored)

- **`attack_AF.png`**: Laurence's footage crop (`red_idle.png`) plus six labelled variants of the attack orb, same camera/frame, spanning the five tunables (`OrbTuning::variant`):

  | Variant | rim power | rim intensity | core darkness | halo intensity | scroll speed |
  |---|---|---|---|---|---|
  | A | 2.0 | 1.3 | 0.30 | 0.30 | 0.5 |
  | B | 3.0 | 1.8 | 0.18 | 0.24 | 0.8 |
  | C | 4.0 | 2.6 | 0.08 | 0.22 | 1.0 |
  | D | 5.0 | 3.4 | 0.04 | 0.16 | 1.2 |
  | E | 3.5 | 3.0 | 0.02 | 0.36 | 1.0 |
  | F | 7.0 | 4.2 | 0.01 | 0.10 | 1.6 |

  **D is closest**: a tight, bright rim and a near-black core that still shows the flame texture's own streaks (unlike F, whose core goes essentially solid black and loses the texture read entirely); A/B/C all read as too uniformly lit/orange throughout, not matching the footage's sharp light/dark split. None of the six fully match the footage's own rim, which is thinner and hotter than any variant here, or its near-absent halo (ours is a visibly larger pale shape in every variant, from the shell/halo geometry's own silhouette from this camera angle) -- flagged, not chased further this pass. **D is now the shipped default** (`OrbTuning::default()`).
- **`all_kinds.png`**: all nine own-view orb kinds (Aura, Attack, Defence, Erase, Status, Special, Environment, opponent's-Aura, opponent's-other) lined up with variant D's settings, each clearly showing its own texture and rim colour (Defence's blue dot pattern, Special's gold streaks, Erase's starfield nebula, Environment's blue-white, etc.) -- confirms the per-kind textures/colours still come through correctly with the new material.
- **`smoke.png`**: `--demo --screenshot` with the normal gameplay/HUD cameras (no `PD_CAPSULE_PREVIEW`), a real in-match capsule near Player 1 visible split-screen -- confirms the new material also renders correctly in actual play, not just the isolated calibration camera.

### Tests and smoke (this pass)

`cargo test -p pd_formats -p pd_game -p pd_convert`: 7 + 36 + 272 passed, unchanged. `pd_game.exe --demo --screenshot` and the `PD_CAPSULE_PREVIEW` calibration runs all exit 0 with no shader/render errors in the log (one shader compile error -- `reflectance` is `vec3<f32>` in Bevy 0.19's WGSL `StandardMaterial`, not `f32` -- was caught and fixed during this pass; `pd_convert`/`pd_formats` untouched.

## Round 2 2026-09-28 (billboarded orb, orange rim, motes -- after Laurence's read of `attack_AF.png`)

Laurence's verdict on the first sheet: "F is closest but all are wrong." His own read: **"the original doesn't appear to have geometry -- I'd guess it's a 2D plane set to face the player at all times."** He also flagged **"a weird low-opacity white disk on the left side of each orb"** and wanted **brighter flames inside**. Separately: the orbs were missing the small glowing motes visible in `red_idle.png`.

### Evidence check (before changing anything)

- This doc's own "What an idle orb is made of" table already had the shell as a shallow **"~3.9 u deep" dish** and the halo as a **flat disc**, not spheres -- geometry that only reads correctly from whatever angle it was authored to be seen at. The core sphere itself does have real geometry (drawinfo: ~400-490 vertices, not a 4-vertex plane) -- so Laurence's "no geometry" guess is closest to literally true for the shell/halo, not the core -- but round 1's own screenshots showed exactly the symptom static, non-billboarded geometry produces from an off-axis camera: a hard vertical light/dark seam on the core, and a pale lens/vesica shape (the flat halo disc, seen edge-on) crossing the frame -- this session's own `attack_AF.png`, and, on closer inspection, already present (fainter, easy to miss at that size) in the "Fixed 2026-09-28" pass's own `shots/orb/build2/preview2b.png`, so it predates this session.
- `cache/fx/capsules/capsules.json` confirms `points: null` for every idle-orb slot (4-12) -- the idle state carries no point-sprite/mote data of its own in the exported archive (only slots 0/2/3, the materialise/pickup/erase bursts, have `.points.json` sidecars). The footage's drifting sparks are therefore **not recoverable per-orb data**; built as a small procedural emitter instead, matched by eye.
- Net decision: billboard the whole orb (core + halo) regardless of exactly how much of "no geometry" is literally true, because it fixes three things at once -- the seam, the edge-on halo, and (a bonus) the core's own broken-hemisphere UV coverage (confirmed in round 1 with lighting fully bypassed) -- by mapping the kind's texture across a procedural disc instead of the exported mesh's own partial UV unwrap.

### What was rebuilt

- **`orb_material.rs`**: `OrbMaterial`/`OrbCore` (`ExtendedMaterial<StandardMaterial, ...>`) replaced entirely by `OrbBillboard`, a plain `Material` with its own bind group (params, tint+mode, extra, one optional texture) and **both** a custom vertex and fragment shader (`enable_shadows`/`enable_prepass` off, since it's a self-lit glow with no need for either pipeline variant).
- **`orb_core.wgsl` vertex**: a hand-adapted copy of `bevy_pbr`'s own `mesh.wgsl` vertex (same feature `#ifdef`s, so it still matches whatever pipeline permutation Bevy picks), except the vertex's world position is reconstructed from the object's world **centre** (translation only) offset along the *current rendering camera's own* right/up axes (`view.world_from_view` columns 0/1) by the vertex's local X/Y, scaled by the object's own uniform scale (so `fade_capsule_orbs`' existing near-camera shrink still works). Because each split-screen camera renders its own pass with its own `view` binding, this faces **whichever camera is currently drawing**, with no per-camera entity duplication -- confirmed in `smoke_round2.png`, a real capsule rendered correctly (and differently-angled) in both split-screen halves at once.
- **`orb_core.wgsl` fragment**: recentres the quad's UV to [-1,1], discards outside the unit circle, and treats the remaining disc as a fake-sphere impostor (`z = sqrt(1 - x^2 - y^2)`, the standard trick) for the fresnel/darken maths -- the kind's texture is sampled across the **whole visible disc** (`in.uv` directly), not the mesh's own broken sphere unwrap, so the round-1 grey hemisphere is simply gone. The rim colour is `mix(capsule_tint, warm_orange, 0.45)` -- a fixed warm bias, not a tunable, fixing Laurence's "rim reads pink, not orange" on the attack capsule. Two independently-scrolling samples of the same texture stand in for the doc's still-unconfirmed two-layer UV scroll, as before.
- **Halo**: now a second billboard of the *same* shader in "halo mode" (`tint.w = 1`), a plain untextured radial falloff (`pow(1-r, 2) * halo_intensity`) -- no more separate shell mesh, no more edge-on white disk, always a clean camera-facing circle.
- **`fx.rs::style_orb_materials`** (renamed in spirit, not in code): now only reads the exported scene once, to pull the core draw's own texture off its `_d0` material (same parent-node name lookup fixed last round) -- then **hides every one of the exported mesh's own draws** and spawns two fresh `Mesh3d` billboard quads (`orb_material::billboard_quad`, radii 4.5/8 u per this doc's own geometry table) as children of the same root, so they still inherit its position/fade.
- **`fx.rs::orb_motes`** (new): every ~0.25 s, one small additive puff (`effects::BurstSpec`/`spawn_burst`, the same machinery `pickup_burst`/`materialise_burst` already use) tinted by the orb's own `OrbTint`, spawned a few units below and to the side of the orb -- not at its exact centre, which would spawn it hidden behind the now-opaque core billboard's flat depth plane. Unconfirmed spawn rate/spread/life, matched to the footage's "a few tiny sparks" by eye, not recovered point data (see evidence check above).
- **Tunables** (`OrbTuning`, env-var overridable as before): `rim_power`, `rim_intensity`, `core_darkness`, `halo_intensity`, `scroll_speed`, plus a new **`flame_boost`** (`PD_ORB_FLAME_BOOST`) for "brighter flames inside". Four round-2 presets (`PD_ORB_VARIANT=A`..`D`, replacing round 1's six), all based on round 1's variant F -- Laurence's own pick for darkness/rim tightness -- varying mainly halo faintness and flame brightness.

### Proof: `engine/shots/orb/calib/` (local, gitignored)

- **`attack_round2.png`**: footage + A-D, same camera/frame as round 1's sheet:

  | Variant | rim power | rim intensity | core darkness | halo intensity | scroll speed | flame boost |
  |---|---|---|---|---|---|---|
  | A | 6.0 | 3.2 | 0.02 | 0.05 | 1.4 | 1.6 |
  | B | 7.0 | 4.0 | 0.01 | 0.08 | 1.6 | 2.2 |
  | C | 8.0 | 4.6 | 0.01 | 0.04 | 1.6 | 2.8 |
  | D | 6.5 | 3.6 | 0.015 | 0.10 | 1.2 | 3.4 |

  **C is closest**: the tightest, brightest rim and the least halo bleed, closest to the footage's thin hot ring; A's rim is visibly softer/wider, B and D show progressively more halo wash around the rim. **C is now the shipped default.** All four fix the round-1 seam/white-disk/pink-rim complaints outright (they're structural, not tuning-dependent); none fully match the footage's own even-thinner rim -- flagged, a further round could push rim power past 8 if wanted.
- **`all_kinds_round2.png`**: all nine kinds at variant C -- clean camera-facing circles, correct per-kind textures/colours, motes visible near several orbs.
- **`smoke_round2.png`**: real split-screen gameplay (no `PD_CAPSULE_PREVIEW`) -- the same in-match capsule correctly billboarded from both players' independently-angled cameras in one screenshot, and a mote visible in the top half.

### Known remaining gaps (flagged, not chased this pass)

- A faint dither/banding pattern is visible in the near-black lower portion of the core in every screenshot (`shots/orb/calib/seam_crop.png`) -- looks like Bevy's own debanding dither becoming visible at very low pixel values, not a texture or geometry seam (confirmed by re-testing with lighting/scroll fully disabled; the pattern didn't move or change). Cosmetic, not raised by Laurence, not investigated further.
- The rim is still softer/wider than the footage's own, even at variant C's rim_power 8.
- Motes are sparse in a single still frame (by design -- footage shows only "a few" -- but worth a second look against video, not just a screenshot, if it still reads as too sparse).

### Tests and smoke (round 2)

`cargo test -p pd_formats -p pd_game -p pd_convert`: 7 + 36 + 272 passed, unchanged. `pd_game.exe --demo --screenshot` and every `PD_CAPSULE_PREVIEW` calibration run exit 0, no shader/render errors in the logs. No changes to `sim.rs`/`combat.rs`/`effects.rs`.

## Draw recipe (decoded 2026-09-28)

Research only, no engine code changed. Read from `PDUWP.exe` (Ghidra export + `engine/re/decomp_fn.py`, `exe_const.py`, raw bytes), the shader bytecode (`tools/disasm` on `game_files/vkCAP*.cso`, `vkCOLANIM`, `vk_cappsprite*`) and `Map/st00.alr` (chunk walk: materials, 0x13 blocks, joints, per-vertex bytes). Legend: ✅ read in code and matched in data · ⚠️ one link inferred · ❓ open. Everything below is for the **Attack** orb (model slot 5); the per-kind table at the end gives the other kinds' numbers.

### What the attack orb actually is (two draws, no halo disc)

| Draw | Mesh | Joint flags | Shader | Blend | Textures |
|---|---|---|---|---|---|
| Core | real sphere, r 4.5, 403 verts, fmt 0x1D (pos, packed normal, uv0, colour, tangent, binormal, uv1) | 0x0004, **not** billboarded | `vkCAP02M2` | **opaque**, depth write on | tex10 flame (base), tex6 twice (normal maps) |
| Rim / halo | a shallow **cup**, fmt 0x09 (pos, uv, colour), 210 verts, rings from r 2.59 at depth 3.89 out to r 6.28 at depth 0 | 0x0204, **billboarded** | `vkCAPM2A` | **additive ONE + ONE**, depth test on, depth write off, drawn after opaque, back-to-front | tex8 (grey cloud noise) twice |

There is no flat halo disc on the attack orb (Aura / Defence / Special / opponent views have one; see the table). The footage's "thick burning rim + soft halo" is the billboarded cup.

### 1. Billboarding ✅

- `FUN_14021b3d0` sets the object matrix once: identity, yaw = capsule slot × 90°, at the slot position. `FUN_14021b840` hands that matrix to the model display list (`FUN_14012e6a0`); it builds no camera matrix.
- Billboarding is **per joint, in the skeleton-chunk handler `FUN_140296190`**: a joint with flag **0x200** has its rotation replaced by the transpose of the view matrix's rotation (view matrix at `0x1405a5ca0`, the same one `FUN_140295420` / `FUN_140298dc0` use for view depth). Result: joint local X = camera right, Y = camera up, **local Z = camera forward, so local −Z points at the camera** (left-handed view space). Scale dropped, position kept. Not done in a shader.
- Attack: the sphere's joint is 0x0004 (a normal, yaw-only, real 3-D sphere); the cup's joint is 0x0204 (billboarded). ✅ from the file bytes.
- The cup is authored on the camera side: at every ring it sits just **outside** the sphere on the −Z side (r 4.45: cup depth 1.03 vs sphere surface 0.67; r 3.86: 2.65 vs 2.31; r 2.59: 3.89 vs 3.68). So it wraps the visible hemisphere like a contact lens and is never hidden by the opaque core. ⚠️ (the −Z = toward-camera direction follows from D3D's left-handed view; the snug fit is the cross-check).

### 2. Per-draw data (the 0x13 block) and UV transforms ✅

- Load: the 0x13 chunk's load-time handler (`0x140295f90`, 8 bytes: store the chunk address in `0x140582420`) remembers the chunk; the next 0x2 index-buffer chunk stores a relative pointer to it at +0x64 (`FUN_140294e10`). The draw record (`FUN_140294fa0`) carries it as record[4]. (The draw-time 0x13 handler is a no-op, which is why the earlier read found nothing.)
- Use: the shader's per-draw function (vkCAP02 family `FUN_14023c440`; the other capsule variants `FUN_14023d210`, `FUN_14023d630`, `FUN_14023cc80`, `FUN_14023c760` follow the same pattern) uploads vertex registers **c90/c91 = T × speed + base**, speed = floats at chunk **+0x18…+0x34** (one (u, v) pair per texture layer), base = floats at **+0x38…+0x54** (only the "+ base" variants add it). **T = `DAT_14057f398`, +1.0 per game update in `FUN_1401c7630`** (wraps at 4,000,000; reset to 0 on renderer init), i.e. one step per 30 Hz frame ⚠️.
- Vertex shader: `uv_layer = mesh_uv × c_SCALES.z + c90/c91`, **c_SCALES.z = 8.0** (`0x1402f8750`): the file's i16 UVs are SNORM × 8, which our reader already does (÷ 4096). So **scroll per frame in UV units = the listed speed**.
- **360.0 at +0x0C** (and 0 at +0x08) is **not read** by the orb shaders' per-draw code (they read +0x18 up only); other shader families (e.g. the colour-anim one, `FUN_14023a0d0`) read +0x08 / +0x0C as a frame window. **No spin** anywhere: no rotation in code, no animation in `st00.dat` for slots 4–12. ✅
- Attack numbers (per frame; × 30 for per second):
  - Core flame, uv0: (+0.002, +0.008)/frame = (0.06, 0.24) UV/s.
  - Core normal map A, uv1: (−0.002, +0.003)/frame = (−0.06, 0.09)/s.
  - Core normal map B, uv1: (−0.003, +0.012)/frame = (−0.09, 0.36)/s.
  - Cup layer 0: (0, −0.006)/frame + fixed offset (0.5, 0.5); cup layer 1: (+0.001, −0.002)/frame.
- Mesh UVs: **core uv0 is a latitude-longitude unwrap, u 0 → 3 around (the texture repeats 3× round the equator), v 0 → 1 pole to pole**; uv1 ≈ uv0. **Cup UVs: u 0 → 2 around, v 0.406 → 0.594 from inner ring to outer edge**, so the cup's v-scroll streams the cloud noise radially (the "licking flames" at the rim).

### 3. Shaders ✅ (bytecode disassembled)

**Core: `vkCAP02M2`** (vkCAP02 is the same with the bump maps on uv0)

- Vertex: full model-view-projection (no billboard in shader). Outputs three UV sets (above) and the directional-light vector, the half vector (light + eye) and a second "sky" light vector, all in **tangent space** (tangent / binormal / normal from the vertex), plus the vertex colour's **alpha only**.
- Pixel:
  1. `flame = tex10(uv0)`; `nA = tex6(uvA)`, `nB = tex6(uvB)`.
  2. **N = (nA − 0.5)·2 + (nB − 0.5)·2**, then one cheap renormalise step `N += N·0.5·(1 − |N|²)`. **The normal maps do NOT offset the flame lookup**; they only perturb the lighting. The green/magenta maps have large X/Y, so N·L swings hard from texel to texel; as the two maps slide past each other in different directions, bright streaks and black gaps crawl across the flame. That is the "bright readable flame streaks on black".
  3. `diffuse = sat(N·L) × cDL_COL` (scene directional light).
  4. Hemisphere: `d = N·S + 0.5`, `ambient = sat(d) × skyColour + (1 − d) × groundColour` (the 0.5 is written by `FUN_1402a7880` into cS0/cS1 .w; the (1 − d) term is not clamped).
  5. `lit = (diffuse + ambient) × vertex alpha`.
  6. `spec = sat((sat(N·H))^16 × cSPECULAR × vertex alpha)`; cSPECULAR = material bytes +0x34..0x37 (BGRA) × **2/255** × the light colour (`FUN_1402a75e0`). Attack: R 159 G 52 B 0 → **(1.25, 0.41, 0) × light colour, an over-bright orange highlight**.
  7. **out = lit × flame + spec**; alpha is a constant (opaque draw).
  - **No fresnel / rim term in the core shader.** Discards: texture-alpha-kill (off here) and the stipple fade (below).
- Core vertex alpha: 1.0 over most of the sphere, **0.56 / 0.23 / 0 on the last rings at each pole** (y ±4.3 … ±4.5): the poles go black, hiding the UV pinch.

**Cup: `vkCAPM2A`**

- Vertex: MVP; `uv0 = uv×8 + c90.xy`, `uv1 = uv×8 + c90.zw`; passes vertex colour.
- Pixel: **out = vcol.a × vcol.rgb × (tex8(uv0) + tex8(uv1))**, additive. Two layers of the same grey cloud noise **summed** (each ~0.5–0.9, so up to ~1.8×): the rim runs hot and flickers.
- The rim shape is **baked into the cup's vertex colours** (BGRA bytes per ring; alpha 255 everywhere):

  | Ring (screen radius, u) | Depth toward camera | Colour RGB |
  |---|---|---|
  | 2.59 (inner edge) | 3.89 | 0, 0, 0 |
  | 3.86 | 2.65 | 56, 6, 0 |
  | 4.33 | 1.74 | 138, 39, 0 |
  | **4.45 (peak, just inside the 4.5 core silhouette)** | 1.03 | **209, 68, 0** |
  | 4.78–4.87 | 0.89 | 56, 6, 0 |
  | 6.28 (outer edge) | 0 | 0, 0, 0 |

  So: a band of hot orange from ~r 3.9 to 4.8 peaking right at the core's edge (the thick burning rim, about a fifth of the radius), and a dim tail out to 6.28 (the soft halo). Inside r 2.6 the core shows untouched.
- Colour comes from the vertex colours, **not** the capsule colour table (idle orbs are never tinted; `FUN_14021b840` sets no colour).

**Sampler / blend state** ✅

- `FUN_14023c320` (vkCAP02 / M2 set-up; `FUN_140020c40(sampler, type, value)`) sets **address U and V = WRAP** (value 1) and linear filtering on samplers 0–2.
- Blend (`FUN_1402a6010` → `FUN_14028e4e0`): only materials with flag 0x10 blend; the mode is the material u16 at +0x14 (src = bits 8–11, dst = bits 12–15, op = bits 5–7; tables `0x1402f77c0` / `0x1402f7798`, GL-style enums). Cup 0x1100 = ONE, ONE, ADD. Core flags 0x0D (no 0x10) = opaque. Aura / opponent halo discs: 0x1300 (src ONE_MINUS_SRC_COLOR, dst ONE: a soft screen-like add) and **0x2000 (src ZERO, dst SRC_COLOR = multiply: the opponent orbs' halo darkens the background)**.
- Blended draws (flag 0x10) are queued depth-sorted (`FUN_140298dc0`, view Z) and drawn back-to-front after the opaque pass; material flag 0x200 clear → depth write off (`FUN_1402066a0`).
- Every shader also writes a constant `blend_colour` to a second render target (glow buffer ❓, as for effects).

**Near-camera fade** ✅, not alpha: `FUN_14021b840` puts `0xED000021 | alpha << 8` in the draw's state word while alpha < 255; `FUN_1402a5f60` sees the 0x21 bits and turns on a **32-level screen-door stipple** (pattern = alpha >> 3, tables at `0x14057f420`); the pixel shaders' first block discards pixels by that bit pattern. Alpha steps ±16 per frame (`FUN_140196c40`, 10 u). Applies to both draws.

### 4. The motes ❓ (not in the idle draw path)

- The idle orb draws nothing but the model above: `pdvkCapsule2`'s only draw is `FUN_14021b840`; `pdvkCapEffect`'s draw `FUN_14021ba60` in states 1–2 draws slot 1 (`cap02_birth2`) **held on its last frame**, and that flare's colour comes from a ramp (`vkCOLANIM`: `tex2 × tex3(u = frame / 60, v = row) × capsule colour`); rows 0–2 of tex3 are ≈ 5, 12, 2 / 255 at u ≈ 0.98, so **at idle the flare is effectively invisible** ⚠️ (frame → u mapping inferred from `FUN_14023a0d0`'s `(frame − w1)/(w0 − w1)` with w0 = 60).
- The idle models (slots 4–12) contain **no point-sprite draws** (none of their draws is format 0x18).
- The only capsule point sprites are slot 0 (`cap01_birth`: 6 × 285 points, textures tex0 orange strip / tex1 dot, position + colour points drawn by `vk_cappsprite`, additive, 0x13 window frames **100–430** of the 429-frame animation stretched over the 14 s respawn wait) and slots 2 / 3 (pickup / erase, window 50–90). So for ~10.7 s of every empty slot's respawn wait a slow cloud of faint orange points drifts around that spot.
- **Best explanation for the footage's motes: another capsule slot's respawn sparkles** (a player's 3 spots are near each other), or stage ambience, **not** part of the idle orb. Check the source video: are the motes there when all three of that player's orbs are present? If not, it is slot 0 of a respawning neighbour.
- `vk_cappsprite`: screen-sized square points (size c32.x × (0.001172, 0.002083) clip units per corner), texture × vertex colour, no alpha. Its texture `cappsprite.dds` is loaded by the shared primitive renderer (`FUN_140233b30`, `pdvkPrimitive`); other users not traced.

### 5. Why ours shows a hard horizontal line ⚠️

- Ours (round 2) maps the flame across a flat billboard with a time scroll; the straight horizontal edge plus a fainter vertical edge in `attack_round2.png` are what a **clamp-to-edge sampler** produces once a scrolled UV leaves 0–1 (the edge row / column smears). The original samples with **WRAP** on a **real sphere** whose uv0 runs u 0 → 3, v 0 → 1 pole to pole, hides the poles with vertex alpha, and never shows a texture edge. The flame textures (tex10 etc.) tile.
- Round 1's "texture only covers one hemisphere, the rest flat grey" has the same root: with clamping, the u 1 → 3 part of the sphere shows the clamped edge column. It is **not** an exporter bug; the UVs are right.
- "Muddy": ours multiplies the flame by a near-black core factor (no light model); the original keeps the full flame texture and lets the moving normal-map lighting cut it into streaks, then adds an over-bright orange specular, then the additive cup rim on top.

### Build recipe (numbered, for the build lane)

1. **Keep the exported meshes; drop the procedural billboard quads.** Export per draw: core with uv0, uv1, tangent, binormal, normal and vertex-colour alpha (fmt 0x1D: normal packed at +12, uv0 +16, colour +20 BGRA, tangent +24, binormal +28, uv1 +32; offsets from this probe's byte dump, tangent / binormal packing ⚠️ assumed the same as the stage SBTS format); cup with uv + vertex colour (fmt 0x09: uv +12, colour +16). ✅ data
2. **Samplers: Repeat on U and V**, linear. ✅
3. **Core: real sphere, yaw only** (slot × 90°), opaque, back-face culled, depth write on. Custom shader per §3 "Core": flame at uv0 + T·(0.002, 0.008); two normal-map samples at uv1 + T·(−0.002, 0.003) and uv1 + T·(−0.003, 0.012); N = sum + renormalise step; lit = (sat(N·L)·lightColour + hemisphere) × vertex alpha; out = lit × flame + sat(pow(sat(N·H), 16) × (1.25, 0.41, 0) × lightColour × vertex alpha). T in frames at 30/s. Use a fixed key light for L / H (stage light values not recovered ❓; start with a white light from above-front, sky ≈ 0.5 grey, ground ≈ 0.2). ✅ formula, ❓ light values.
4. **Cup: billboarded every frame**: rotation = camera rotation, so the cup's deep (inner, black) ring points at the camera; position = orb centre. Additive ONE + ONE, no depth write, drawn after opaque. out = vcol.a × vcol.rgb × (tex8(uv + (0.5, 0.5) + T·(0, −0.006)) + tex8(uv + T·(0.001, −0.002))). Keep the exported vertex colours: they are the rim profile. The game adds these 0–1 values in display space (FORMATS-EFFECTS "gamma"), so use `FxMaterial`-style re-encoding for the cup. ✅ (⚠️ camera-facing sign)
5. **No tint, no fresnel term, no spin, no bob.** ✅
6. **Fade**: screen-door dither on both draws while the viewing camera is within 10 u (alpha ±16/frame, 32 levels), or keep our shrink as a stand-in. ✅
7. **Motes**: remove the procedural per-orb motes, or keep them marked unconfirmed; the faithful source for drifting orange points is slot 0's sparkle cloud during a slot's respawn wait (visible frames 100–430 of the stretched 429). ❓

### Other kinds (same recipe, their own numbers)

Speeds are per frame, (u, v) per layer in order; "base" = fixed UV offset per layer; BB = billboarded joint.

| Slot | Draw | Shader | BB | Blend | Tex | Speeds | Base | Spec R,G,B |
|---|---|---|---|---|---|---|---|---|
| 4 Aura | core | vkCAP02 | **yes** | opaque | 5, 6, 7 | (.003,−.002) (.003,−.002) (.005,.01) | 0 | 35,56,93 |
| | disc (fmt 0x15) | vkCAPM2MM2 | yes | 0x1300 | 9, 9 | (0,−.007) (.001,−.004) | .5,.5 | — |
| | cup | vkCAPM2A | yes | add | 8, 8 | (0,−.002) (0,−.003) | .5,.5 | — |
| 5 Attack | core | vkCAP02M2 | no | opaque | 10, 6, 6 | (.002,.008) (−.002,.003) (−.003,.012) | 0 | 159,52,0 |
| | cup | vkCAPM2A | yes | add | 8, 8 | (0,−.006) (.001,−.002) | .5,.5 | — |
| 6 Defence | core | vkCAP02 | no | opaque | 11, 12, 12 | (−.002,0) (.01,−.003) (−.004,−.002) | .05,.05 / .03,.03 / .1,.1 | 52,89,152 |
| | disc | vkCAPM2AM2 | yes | add | 13, 13 | (0,.008) (0,.016) | 0 | — |
| | cup | vkCAPM2A | yes | add | 14, 11 | (0,−.01) (0,−.005) | .5,.5 | — |
| 7 Erase | core | vkCAP02M2 | no | opaque | 15, 6, 16 | (0,−.002) (0,.01) (−.003,−.005) | .5,.5 | 54,0,178 |
| | cup ×2 | vkCAPM2A | yes | add | 17,17 / 8,8 | (0,.02) (0,.01) / (0,−.002) (0,−.001) | .5,.5 | — |
| 8 Status | core | vkCAP01 (4 tex) | no | opaque | 18, 19, 19, 20 | (0,−.005) (0,−.003) (0,−.004) (.002,0) | 0 / .4,.4 | 127,127,127 |
| | cup | vkCAPM2A | yes | add | 18, 18 | (0,−.002) (0,−.001) | .5,.5 | — |
| 9 Special | core | vkCAP02 | no | opaque | 21, 16, 16 | (0,.002) (.001,.004) (−.001,0) | 0 / .5,.5 | 89,48,10 |
| | disc | vkCAPM2AM2 | **no** | add | 22, 22 | (.001,.002) (−.001,.005) | 0 / .5,.5 | — |
| | cup | vkCAPM2A | yes | add | 23, 23 | (0,−.002) (0,−.001) | .5,.5 | — |
| 10 Environment | core | vkCAP04M4 (4 tex) | no | opaque (flags 0x01) | 25, 26, 26, 27 | (.002,0) (0,−.002) (0,−.003) (−.001,0) | 0 / .4 / .3 / .2 | — |
| | cup | vkCAPM2A | yes | add | 24, 24 | (0,−.002) (0,−.001) | .5,.5 | — |
| 11 Opp. Aura | core | vkCAP02 | **yes** | opaque | 28, 6, 16 | (.002,0) (.003,−.004) (−.003,−.006) | 0 / .4,.4 | 127,127,127 |
| | disc | vkCAPM2MM2 | yes | **0x2000 multiply** | 9, 9 | (0,−.007) (.001,−.004) | .5,.5 | — |
| | cup | vkCAPM2A | yes | add | 28, 28 | (0,−.002) (0,−.001) | .5,.5 | — |
| 12 Opp. skill | as 11, core tex 29, spec 196,0,64 | | | | | | | |

Spec shown R,G,B (file order is BGRA; the alpha byte is not used by the shader). `vkCAP01` and `vkCAP04M4` are unlit layered blends (vkCAP01: t0 × vcol, t1 and t2 blended over by their own alpha × vcol.a, + t3 × vcol; vkCAP04M4: t0 × t1, t2 over by its alpha, + t3, × vcol, saturated), read from bytecode, not needed for the attack orb.

### Remaining unknowns

- ❓ The scene light values the core uses (cDL_VEC / COL, sky / ground cS0 / cS1): the frame's current light set (`FUN_1402a7a00` picks up to 4 global light slots), i.e. stage-dependent. A fixed light is the practical stand-in.
- ⚠️ Whether T advances per 30 Hz update or per rendered frame (`FUN_1401c7630` is the per-frame game update; 30/s assumed).
- ⚠️ Cup facing sign (−Z toward camera): strongly supported by the cup hugging the camera-side hemisphere, not yet seen on screen; an A/B render (cup toward vs away) settles it in one look.
- ❓ Mote source (§4): needs the video check.
- ❓ What the glow buffer (`blend_colour`, second render target) adds; may contribute to the footage's soft halo.

## Built from recipe 2026-09-28

Rebuilt the idle orb render-only, from the "Draw recipe" above -- real exported geometry replacing round 2's billboarded quads. `sim.rs`/`combat.rs` untouched; `effects.rs` untouched beyond calling its existing `spawn_burst`/`spawn_point_burst` helpers.

### Exporter (`pd_convert capsules`, `pd_formats::gltf::export_capsule_slot_glb`, new)

- A dedicated capsule exporter (not the shared `export_glb` path every other stage/character/effect export and its 272 tests use): one node/mesh per file draw, named `"d{index}"`, carrying whatever a capsule vertex format actually has -- `alr::read_capsule_vertices` (new) reads format 0x1D (sphere, "M2" kinds: normal, uv0, uv1, BGRA colour, packed tangent + binormal), format 0x03 (sphere, base kinds: same minus uv1 -- discovered this pass that `vkCAP02` without "M2" uses one UV set, reusing it for both flame and bump samples, not the two-UV `vkCAP02M2` layout originally assumed for every core), format 0x09 (cup: uv + colour) and format 0x15 (disc: uv + colour, offset assumed by analogy with 0x09, not byte-confirmed). TANGENT is written as a standard glTF vec4 with the handedness sign computed from the file's own separately-stored binormal (sign of the dot of the cross of normal and tangent, with binormal) rather than guessed, so `orb_sphere_lit.wgsl` can reconstruct a correct bitangent. `Materials::get_dual` (new) embeds the core's two distinct textures (flame + bump) without the normal-map blue-channel flip `get_lit` applies for real tangent-space normal maps -- the bump texture here is a plain noise texture sampled twice by a bespoke formula, not consumed as one.
- Per-draw resolved glTF texture indices are recorded in `capsules.json` (`draws[].base`/`secondary`) so `pd_game` never has to re-derive which texture a draw uses -- only the behavioural recipe numbers (scroll speeds, spec colour, billboard flags) are hardcoded, in `pd_game`, cited to this doc.
- Regenerated: `pd_convert capsules Map/st00.alr Map/st00.dat cache/fx/capsules` now reports the exact draw counts the per-kind table implies (2 for Attack/Status/Environment, 3 for Aura/Defence/Special/opponent views, 3 for Erase's two cups) -- checked kind by kind before writing any `pd_game` code.
- Samplers: already REPEAT/wrap for every exported texture (`gltf.rs::Materials::write_into`, unchanged) -- confirmed still in effect; `fx.rs::force_orb_texture_repeat` (new) additionally forces it in code once each texture loads, belt-and-braces against round 2's "hard horizontal line" (a scrolled UV leaving 0..1 on a clamped sampler).

### `pd_game` (render-only)

- **Core** (`orb_material.rs::OrbSphereLit`, `orb_sphere_lit.wgsl`): the real exported sphere mesh, single-sided opaque, no procedural billboard quad. Not billboarded except the kinds the per-kind table marks camera-facing (Aura, both opponent views) -- for those, the vertex shader replaces the mesh's local frame with the current camera's own right/up/forward basis (the round-2 quad technique, generalised from a flat quad to a full 3-D mesh's position *and* normal/tangent, so the sphere keeps its shape while still turning to face the camera). Shader: flame texture at uv0 + scroll; two bump samples (uv1 if the format has one, else uv0, per kind) combined by the disassembled formula (two texture samples each shifted and doubled, then summed) with a renormalise step (gain tuned by eye -- flagged below), folded into the normal via the mesh's own tangent/bitangent; a fixed stand-in key light (named constants, unconfirmed -- this doc's own "stage light values not recovered") drives a clamped N.L diffuse term plus a hemisphere ambient; an (N.H)^16 specular in the kind's own BGRA spec colour (x2/255, so it over-brightens exactly as this doc's own reading of the shader describes); vertex-alpha pole fade from the exported colour. Status/Environment (`OrbSphereUnlit`, `orb_sphere_unlit.wgsl`): their own vertex format (0x17) carries no normal/tangent/colour at all, so this is a plain scrolling base-texture + (averaged, to avoid clipping) second-layer sample -- not the real 3-4 layer `vkCAP01`/`vkCAP04M4` blend, flagged as an approximation (the material format only carries two texture slots).
- **Rim** (`orb_material.rs::OrbRim`, `orb_rim.wgsl`): the real exported cup (every kind) and, where the per-kind table has one, the flat disc -- both always billboarded every frame (the same camera-basis technique, applied to the cup/disc's own real geometry so its authored depth/shape survives), additive (or multiply for the opponent-Aura disc, the one blend-0x2000 kind), two scrolled samples of the same texture summed and modulated by the mesh's own baked vertex-colour rim profile. The vertex colour is gamma-decoded before use: it's a raw stored byte meant to add in display/gamma space (this doc's own step 4), and without decoding it, our fragment output -- otherwise linear light, gamma-encoded again at display -- came out doubled in brightness, which was round 3's own first-pass symptom (a rim far wider/brighter than the footage's thin ring). `PD_ORB_CUP_FACE=away` flips the recipe's own camera-forward A/B test; `toward` (the default) is the shipped pick -- matches the footage better in every kind checked this pass.
- **Per-kind table** (`orb_material.rs::kind_spec`, all 9 kinds: Aura, Attack, Defence, Erase, Status, Special, Environment, opponent's Aura, opponent's other): billboard flags, scroll speeds/bases and specular colour transcribed from this doc's own "Other kinds" table, keyed by file draw index (core, then disc if present, then cup(s)) -- checked against the regenerated `capsules.json`'s own draw count per slot before wiring, not assumed.
- **Removed**: `orb_material::OrbBillboard`/`billboard_quad`, the procedural core+halo quads, and `style_orb_materials`'s "hide the real mesh, spawn a billboard" hack -- `spawn_orb` (new) builds each draw's entity straight from the loaded mesh/texture handles the moment a capsule appears, no deferred pass needed (loading `GltfAssetLabel::Primitive`/`Texture` by index doesn't need a whole scene's node hierarchy to load first, unlike rounds 1/2's scene-traversal approach).
- **Motes**: kept, now behind `PD_ORB_MOTES` (default **off**) -- this pass's own read of the idle-orb export data confirms slots 4-12 carry no point-sprite draws at all, so the round-2 procedural motes are unconfirmed set-dressing, not recovered data, matching this doc's own "Draw recipe" section 4 conclusion.
- **Fade**: unchanged from round 2 (`OrbFade`, scale-to-near-zero) -- the recipe's real 32-level screen-door dither wasn't attempted this pass (would need a per-fragment discard pattern keyed to the fade alpha across three different shaders); flagged, not guessed at.
- **Tests/tools**: `pd_convert capsules` unchanged in interface; `PD_CAPSULE_PREVIEW`'s `close_<kind>` modes extended from `close_attack`/`close_aura` to all 9 kinds (`close_defence`, `close_erase`, `close_status`, `close_special`, `close_environment`, `close_opp_aura`, `close_opp_other`) for per-kind proof shots.

### Proof: `engine/shots/orb/calib/` (local, gitignored)

- `attack_round3.png`: Laurence's footage (`red_idle.png`) + ours at a similar angle/size, plus a 3-frame time strip (frames 60/150/240 of the same close-up) showing the flame streaks visibly crawl frame to frame.
- `all_kinds_round3.png`: all 9 kinds, individually close-cropped (`PD_CAPSULE_PREVIEW=close_<kind>`) -- each shows its own distinct texture/colour/rim; the lined-up single-camera sheet (kept as `all_kinds_round3c.png` for reference) framed several kinds at an angle their own non-billboarded core isn't lit well from, which the per-kind close-ups avoid.
- `splitscreen_round3.png`: `--demo --screenshot`, no `PD_CAPSULE_PREVIEW` -- a real Aura Particle and a real Attack capsule near Player 1, both split-screen halves, with the game's own "press a skill button to capture" prompt, proving the new render path works in actual play, not just the isolated preview camera.

### What matches, what's still off

Matches: real sphere + cup geometry (not flat billboards) with correct per-kind textures, colours and specular tint; the cup's baked rim profile reads as a thick, hot band near the core's own silhouette with a dim tail, much closer to the footage's proportions than round 2's oversized halo (fixed once the vertex-colour gamma bug above was found); the flame visibly scrolls/crawls frame to frame with no hard clamp seam; all 9 kinds render distinctly; a real in-match, split-screen capture with two different capsule kinds visible and correctly HUD-labelled.

Still off, flagged rather than guessed at further this pass:
- The core's dark/light balance is tuned by eye (bump gain, fixed light direction/colour) rather than recovered -- ours currently shows more of the sphere lit than the footage's mostly-dark ball with isolated bright streaks; the underlying bump texture's own frequency content may just not produce the footage's tighter streak pattern at any global tuning.
- Status/Environment's core is a 2-layer approximation of a real 3-4 layer unlit blend (format/material limit, not a bug) -- Environment in particular still reads flatter/paler than its "blue-white" description.
- No 32-level screen-door dither fade (kept round 2's scale-based stand-in).
- Disc format 0x15's vertex-colour offset is assumed by analogy with the cup's 0x09, not byte-confirmed against a hex dump.
- The stand-in key light/hemisphere values, the bump gain, and the cup-facing sign are still tuned-by-eye constants, not recovered ones (each is commented in place as such).

### Tests and smoke

`cargo test -p pd_formats -p pd_game -p pd_convert`: 7 + 36 + 272 passed, unchanged. `pd_game.exe --demo --screenshot` and every `PD_CAPSULE_PREVIEW` (`1`, `close_<kind>` x9, `mat`, `pickup`) calibration run exit 0, no shader/render errors in the logs.

## Fixed 2026-09-28 (orchestrator review of the first round-3 sheets)

Four concrete bugs found by an independent look at `attack_round3.png`/`all_kinds_round3.png`, fixed in place (no round-4 rewrite -- `orb_rim.wgsl`/`orb_material.rs`/`fx.rs` only):

1. **The cup rendered as an opaque black annulus, hiding the background.** Root cause: Bevy's generic `Material` trait maps `AlphaMode::Add` to its `BLEND_PREMULTIPLIED_ALPHA` pipeline key, *not* true `(One, One)` additive -- the actual blend function is `result = src + (1-src.a)*dst`, and `bevy_pbr::pbr_functions::premultiply_alpha` (which our custom shader bypasses, having its own fragment path) sets the output alpha to **0** for `Add` so that term reduces to `result = src + dst`. Our shader was outputting alpha **1**, so every fragment -- including the rim profile's own black inner ring -- fully *replaced* the destination instead of adding to it. Fixed: `orb_rim.wgsl`'s Add branch now outputs alpha 0.
2. **The orange rim sat outside the sphere with a black gap.** This was downstream of #1: the cup's own opaque black inner geometry was hiding the core sphere behind it before the fix; once additive blending actually adds, the rim sits at its authored radius against the sphere with no gap (confirmed in `attack_round3b.png`).
3. **Blocky flame texture.** `fx.rs::force_orb_texture_repeat` (forces REPEAT wrap once each texture loads) was overwriting the *whole* sampler descriptor with `..default()`, which defaults every filter to Nearest -- silently downgrading the exporter's own Linear/Linear glTF sampler. Fixed: the override now also sets mag/min/mipmap filter to Linear explicitly.
4. **Aura's big opaque white ring; OppOther's core reading as a solid black disc with a bright opaque outer ring.** Aura's disc was rendering with the same plain-Add approximation as a cup, and the recipe's own per-kind table gives it a different, non-additive blend flag (0x1300, "src ONE_MINUS_SRC_COLOR, dst ONE" -- a self-attenuating "screen-like" blend, distinct from a cup's plain 0x1100 ONE+ONE). Bevy has no built-in `AlphaMode::Screen`, so it's approximated by pre-computing `src*(1-src)` in the shader and feeding that through the same premultiplied-Add pipeline (algebraically identical to the recipe's own blend factors). OppOther's own disc was wrongly given the plain-Add approximation too: the recipe's row 12 ("Opp. skill") is explicitly "as 11" (opponent-Aura) except its core texture/spec, and row 11's disc is the one 0x2000-multiply kind -- carried over to row 12's disc as well. A byte dump of Aura's own disc (`pd_convert drawinfo ... 4 1`, vertex format 0x15, stride 24) confirmed the vertex-colour offset guess from the "Draw recipe" section above (BGRA at +16) reads real, sensible values (white/grey, not garbage) -- the visual bug was the blend approximation, not a wrong byte offset.

New: `orb_material::RimBlend` (`Add` / `Screen` / `Multiply`), replacing the old `multiply: bool` flag, so each kind's disc can carry the recipe's own blend rather than one binary choice.

Reproof: `attack_round3b.png` (footage vs ours, now genuinely close: tight rim on a dark sphere, background visible around it, readable flame streaks scrolling frame to frame), `all_kinds_round3b.png` (all 9 kinds, all clean now -- no opaque annuli, no blocky textures, Aura's disc a soft misty halo instead of a hard white ring, OppOther's disc correctly darkening instead of a bright ring), `splitscreen_round3b.png` (real in-match capture, both an Aura Particle and an Attack capsule visible with background readable through both). `cargo test -p pd_formats -p pd_game -p pd_convert`: 7 + 36 + 272 passed, unchanged; every calibration run exits 0.

## Fixed 2026-09-28 (Laurence on `attack_round3b.png`: darker centre, brighter/less-defined edges)

Laurence: "close, but the original is darker in the middle and brighter on the sides, as if it's hotter and is emitting more light, making the edges less defined." Three changes, all in `pd_game` (no exporter change except the texture-slot fix below):

1. **Core edge brightening.** The disassembled `vkCAP02`/`vkCAP02M2` pixel shader has no fresnel/rim term (confirmed in the "Shaders" section above) -- but it also writes to a second render target (`blend_colour`) this doc's own "Remaining unknowns" flagged as unchased and a likely source of exactly this look. Approximated in `orb_sphere_lit.wgsl` as a view-dependent (Fresnel) term on the *smooth* pre-bump normal (so the brightening reads as a clean silhouette glow, not a streaky one riding the bump noise): darkens the centre toward a floor, brightens/warms the true edge, tunable via a new `orb_edge` uniform (`OrbTuning::EdgeTuning`, `PD_ORB_EDGE_VARIANT=A/B/C` or the individual `PD_ORB_EDGE_POWER`/`_DARKEN`/`_GLOW` env vars). First pass (power 1.6-3.0) looked right on the non-billboarded Attack orb but blew the three billboarded kinds (Aura, both opponent views) out to a near-solid glow with a stark black centre dot: viewed dead-on, as a billboarded sphere always is, a low fresnel power covers most of the visible disc, not just a thin rim. Fixed two ways: raised the power range well past that (4-8) and damped the whole effect to 15% strength specifically for billboarded cores (`orb_flags.x`), which already had a look this doc's earlier rounds settled on and didn't need it. **C (power 8, darken 0.4, glow 1.1) is the closest of the three to the footage this pass and is the shipped default**, not yet confirmed by Laurence.
2. **Cup/disc soft bleed.** A gamma lift (`rgb^0.72`) on the rim's own colour before the Add/Screen/Multiply branch raises the dim tail relatively more than the hot peak, spreading the visible glow further out towards the mesh's own edge without new geometry -- reads as "less defined edges" per Laurence's own words.
3. **Status/Environment's real 4-texture unlit blend.** A raw byte dump of the material record (`alr::MaterialEntry`, 0x4C bytes) found two *more* texture-index fields right after the ones already read (`texture_idx`/`normal_idx` at +0x18/+0x1A): `texture_idx2`/`texture_idx3` at +0x1C/+0x1E. Status's own material reads exactly `18, 19, 19, 20` and Environment's exactly `25, 26, 26, 27` across these four fields, byte for byte matching the recipe's own per-kind numbers -- confirming they're real, additional texture layers, not padding, as this doc's own build recipe had assumed. `export_capsule_slot_glb` now embeds all four per draw (`CapsuleDrawTex::tertiary`/`quaternary`, new); `OrbSphereUnlit`/`orb_sphere_unlit.wgsl` now implement the real formulas from the disassembled bytecode (`vkCAP01`: t0, t1/t2 blended over by their own alpha, +t3; `vkCAP04M4`: t0*t1, t2 over by its own alpha, +t3, saturated) instead of a 2-layer average. Both kinds now show real layered detail instead of a flat fill.

**Aura's disc ("the grey one-sided smudge")**: its baked vertex colours (byte-dumped: alternating `0xFFFFFFFF`/`0x747474FF` per vertex, not a clean radial taper) triangulated into a lopsided smudge rather than a centred halo. Replaced for the `Screen`-blend case specifically with a clean, camera-centred radial falloff computed from the mesh's own UV (v maps radially, the same convention as the cup) and a flat white tint (this doc's own read of the texture: "white liquid swirl") instead of the uneven baked colours.

Reproof: `attack_round3c.png` (footage + labelled A/B/C edge variants -- C closest), `all_kinds_round3c.png` (all 9 kinds; Status/Environment now show real layered texture instead of a flat fill; Aura/OppAura/OppOther no longer blow out), `splitscreen_round3c.png` (real in-match capture, an Aura Particle and an Attack capsule both visible with the rest of the scene/HUD unaffected -- no global bloom/blowout). `cargo test -p pd_formats -p pd_game -p pd_convert`: 7 + 36 + 272 passed, unchanged; every calibration run exits 0.

**Not chased further, flagged**: the core's centre/edge balance is still tuned by eye, not recovered (the real glow-buffer contents are unread); Status/Environment still don't factor in the mesh's own vertex colour the way `vkCAP01`'s formula does (this vertex format carries none); the cup's soft-bleed exponent (0.72) is a plausible-looking constant, not a recovered one.

## Fixed 2026-09-28 (Laurence on `attack_round3c.png`: "none of A/B/C yet")

"The red outline is not thick enough and not loose enough. There is not enough glow emitting. Needs more contrast between edges and centre, making the centre darker and the outer ring thicker and looser." Also flagged: round 3c's own regressions (Aura's halo filling half the frame; OppAura/OppOther flattened to a beige/pink disc with a black centre dot) and Status/Environment still showing hard horizontal stripes rather than a swirl.

1. **Ring thickness/glow** (`orb_rim.wgsl`, new `orb_ring` uniform / `OrbTuning::RingTuning`, `PD_ORB_RING_VARIANT=A/B/C`): the existing soft-bleed gamma lift is now a tunable exponent (`soften`, lower = wider/softer) plus a flat multiplier (`boost`, more emitted glow) on top. Pushed **C** (boost 4.5, soften 0.22) well past round 3c's own single fixed constant, per Laurence's "push the strongest variant further than feels safe" (asked for more twice already) -- see `attack_round3d.png`'s A/B/C sheet. **Shipped default is A** (mild), not C: the cup/disc material is shared by all nine kinds, and B/C's strength, which reads right on Attack's own orange ring, blew Defence/Erase/Special/Aura/Status/Environment's own halos out to solid white/colour discs in a first all-kinds pass at variant C. Core centre-darken (`EdgeTuning`) also pushed darker (variant C floor 0.4 -> 0.05).
2. **Billboard-core regression, actually fixed this time**: round 3c's "damp to 15% for billboarded cores" didn't work -- `edge` is exactly 0 dead-centre regardless of how much `edge_strength` shrinks its ramp-up, so `centre_scale = mix(orb_edge.y, 1.0, 0 * edge_strength)` was still `orb_edge.y` at the centre no matter the damping, which is why Aura/OppAura/OppOther still looked washed out with a black centre dot. Fixed by skipping the whole centre-darken/edge-glow effect outright for billboarded cores (`select(..., is_billboard)`) instead of damping it, restoring the round-3b look those three kinds already had.
3. **Status/Environment's hard horizontal stripes -- a real bug, not the texture's own content.** Dumped the raw textures themselves (`pd_convert dumptex`): tex18-20 and tex25-27 are all smooth, organic, swirly/noisy images, nothing striped -- ruling out "that's just what the texture looks like". Isolated each of the 4 blended layers individually (`PD_ORB_UNLIT_LAYER=0..3`, new debug toggle) and found *every* layer showed the same stripe pattern, meaning it wasn't the blend formula either -- it had to be a UV problem shared by all four samples. A raw byte dump of the vertex buffer (`pd_convert drawinfo` + a temporary `PD_RAWDUMP=1`) found the shared `vertex_layout` table's UV offset for format 0x17 (+0x10, inherited from the skinned/palette formats that reuse this stride) is wrong for this mesh: +0x10..+0x13 is actually a BGRA **vertex colour** (three equal bytes + a 0xFF alpha, stepping in a clean grey gradient per latitude ring -- baked static shading), and the real UV0 is one field earlier, at +0x0C. Reading a per-ring grey-banded colour as a UV coordinate samples the texture in equally clean horizontal bands -- exactly the bug. Fixed with a format-0x17-specific override in `alr::read_capsule_vertices` (the shared `vertex_layout` table is left alone, since other unverified stage geometry may also use format 0x17 and rely on its current offset -- changing it there risked an unrelated regression for one capsule fix). Not chased further: the format appears to carry three *more* per-layer UV pairs at +0x14/+0x18/+0x1C (spot-checked on one vertex: +0x14 and +0x1C matched the corrected UV0 exactly, +0x18 differed only in U) -- layers 1-3 still reuse layer 0's UV0 rather than their own.

Reproof: `attack_round3d.png` (footage + labelled A/B/C ring variants -- C pushed deliberately past "safe"), `all_kinds_round3d.png` (all 9 kinds -- Status/Environment now show real organic texture instead of stripes, Aura/OppAura/OppOther back to a clean look, Attack's own ring visibly thicker/softer/darker-centred), `splitscreen_round3d.png` (real in-match capture, an Aura Particle and an Attack capsule both visible, rest of the scene/HUD unaffected). `cargo test -p pd_formats -p pd_game -p pd_convert`: 7 + 36 + 272 passed, unchanged; every calibration run exits 0.

**Not chased further, flagged**: layers 1-3 of Status/Environment's unlit core still reuse layer 0's UV rather than their own per-layer UV (data exists in the file, not wired up); the ring's boost/soften constants and the core's centre-darken floor are still tuned by eye; the shipped ring default (A) is deliberately milder than what Laurence approved for Attack specifically, since the material is shared across all nine kinds -- a per-kind ring tuning (rather than one shared default) would let Attack ship at B/C without affecting the others, not attempted this pass.

## Parked 2026-09-28 (Laurence on round 3d)

Shipped: ring variant A. Laurence parked the orbs here; open items for a later pass:
- Glow is too uniform in all variants (the original's glow varies/flickers around the ring, not an even band).
- Attack glow too orange — should be redder.
- Defence not blue enough (reads too cold / pale-white; wants a deeper blue).
- [x] Aura Particle's white halo too big — shrink. **Fixed 2026-09-28** (lane `lane-auraorb`, Aura-only): root cause was `pd_formats::alr::vertex_layout`'s format-0x15 entry reading the disc's UV with the wrong divisor (`i16::MAX` = 32767, inherited from an unrelated skinned/palette format that shares this format id, instead of the 4096 every other capsule vertex format uses) — confirmed with a raw byte dump of `Map/st00.alr` model 4 draw 1 (temporary `PD_RAWDUMP=1` probe, removed after use): the true per-frame v (divisor 4096) is a clean radial map, 0.6846 at the inner ring (r 3.93, hugging the core) to 0.9954 at the outer ring (r 7.96), but read with the wrong divisor every vertex landed in 0.0856..0.1244 — below the shader's old radial cutoff (0.4) on every single vertex, so the disc's "radial falloff" was 0 (i.e. full glow) everywhere: not a taper, a flat wash the shape of the noise texture — the reported "flat grey-white disc filling half the frame". Not fixed in `vertex_layout` itself (a shared table entry other, unverified geometry may also read, and out of this fix's render-only scope: `orb_material.rs`/`orb_*.wgsl`/`fx.rs` only) — undone instead in `orb_rim.wgsl`'s Screen-blend branch (the one code path only Aura's disc uses) by multiplying the incoming (wrongly-scaled) v back by the same wrong/right divisor ratio (32767/4096 = 8.0) before mapping radially over the real measured range, then tapering with a tighter power (3.0) and lower gain (0.35) so the halo reads as a small soft glow hugging the core's silhouette rather than a second disc the width of the whole mesh. All other 8 kinds' materials/shaders untouched (confirmed identical in `all_kinds_after.png` vs `all_kinds_round3d.png`). Proof: `engine/shots/orb/auraorb/aura_fix.png` (footage + before + after), `all_kinds_after.png`, `splitscreen_after.png` (local, gitignored).
- Per-kind ring tuning (so Attack can be stronger without blowing out the others).

## Regeneration motes (2026-10-01, lane capbirth)

Laurence: "the particle motes when a capsule is regenerating are not present yet." Built: `capsule_birth.rs` + `capsule_birth.wgsl` (render only), a read-only `Combat::pending_capsules`, and a converter export (`<stem>.sprites.json`, `pd_formats::capsprites`). Everything below is labelled READ (program address, `PDUWP.exe`), MEASURED (Laurence's footage `Orb materialize.mp4`) or UNCONFIRMED. Footage frames are kept out of git in `%USERPROFILE%\.pdtools\footage\capbirth\`.

### Corrections to the earlier sections of this page

- Slot 0's shader is **`vkPSPRITE`, not `vk_cappsprite`**. READ: the material's first 8 bytes are a base-40 name hash (`c69e6d6c 789a0600` -> `VKPSPR` + `ITE`; the flare's `c691a1bb 9578a600` -> `VKCOLA` + `NIM`, each u32 a 6-character base-40 number), the same packing as the effect kinds (`a07ac685 8891f680` = `PARTICLE01`). All point-sprite slots (0, 2, 3, 14, 21, 22) carry the same hash.
- The "window frames 100-430" in "The motes" were a misread. READ (`FUN_140235da0`): the 0x13 chunk before each draw is `+0x08 size (100.0)`, `+0x0C end frame (430.0)`, `+0x10 start frame`; start is 350 / 355 / 360 for the three big clouds and 70 / 100 / 130 for the three small ones. The cloud is drawn at every frame of the wait; the start frame only starts the colour ramp.
- The vertex bytes after the position are **UVs, not a colour** (`s16 u`, `s16 v`, x 8 / 32767; every u is 40 / 4096 = 0.0098, v is random 0..1). `gltf::point_sprites` still returns them as "colour" (and reads each draw's whole 285-point buffer, not its 78 / 15 / 18 indexed points, so 1,710 = 6 x 285); the new export reads the indexed points with their UVs.

### What the program does in the wait (READ)

- **State and frame.** `FUN_14014ed10` (30 Hz): state 0 decrements the countdown at slot `+0x18`; at < 1 it goes to state 1 (15 ticks, sound 0x55), then 2. `FUN_14014fd10` sets state 0 with countdown = wait = `FUN_14024d4d0(player, 0x22) + 0x1a4` = **420 + the environment's respawn status** (the `capsule_wait_delta` read `Combat` already uses). `FUN_14021ba60` in state 0 draws `st00` slot 0 at frame `(wait - countdown) * 420 / wait`: the 429-frame animation is stretched over the whole wait and never reaches frames 420-429.
- **Colour word.** `(viewer == owner) << 24 | 0xffffff`: white for everybody; the top byte only changes the blend (below).
- **Placement.** The effect object's position is the slot position itself (record `[2..4]`, copied each draw, no offset, no bob), yawed `spot index * 90` degrees (`FUN_14021b3d0`; record field `[1]` is the spot 0..2; `FUN_1402906e0` with `slot * 8192` of 32768 per turn). The idle orb is drawn at the very same point (`FUN_14021b840`). Our orb is lifted 5 u above the spot with a bob (`fx.rs`, inherited, not in the program); the motes use the same 5 u lift (`capsule_birth::ORB_LIFT`) so the gather forms where the orb pops. The yaw sign in our mirrored space is derived from the matrix rows, not seen (the big clouds are symmetric).
- **The draw.** Six draws, 285 points in all (78 + 78 + 78 + 15 + 18 + 18), one joint each (3, 6, 9, 11, 13, 15). Material blend `0x1100` = ONE + ONE add, no depth write, no tint. `FUN_140235da0` binds the material's first texture (`st00` tex0, the colour strip, **repeat**) to stage 0 and its second (tex1, the soft dot, **mirror**) to stage 3.
- **Colour of a point.** VS `uv' = uv * 8 + (progress, 0)`, `progress = max(0, frame - start) / (end - start)` (c33.x). PS: `strip(uv') * dot(quad uv)` (rgb), alpha = both alphas, discarded at 0. Every u starts at 0.0098, the dark end of the strip, so a point is black until its ramp runs: the three big clouds glow in the last 70-80 frames, the small ones over 300+ frames (a faint scatter early, a dense ball at the end; matches the footage). The strip's row (v) gives each point its own flicker; its right edge is pale gold `(255, 215, 173)`.
- **Size.** VS clip-space offset `quad_pos * c32.x * (0.001172, 0.002083)` with `c32.x = size * 10.0 (0x1402f7f7c) * display scale (1: the table at 0x1402f3e10, +8) * 1.0 (0x1402f7da8)` = 1000 for size 100 (outside online mode 6), i.e. 2.083 clip units per unit of quad corner on y. The quad is therefore the same fraction of the screen at any depth; its world size depends on the projection (`2.083 q / P11`). We keep the vertical size and make it square in any viewport (the program's x constant is for 16:9).
- **Owner vs everyone else.** `FUN_140235da0`: colour top byte 0 (not the viewer's own capsule) switches the blend to `0x3001` = ZERO, ONE_MINUS_SRC_COLOR (table `0x1402f77c0`): the opponent's views see **dark specks** `dst * (1 - colour)` instead of gold. Implemented as a second material on the owner's ghost layer; `PD_CAPBIRTH_OPPONENT=add` shows the additive look to everyone for comparison. Not in any footage (single-view capture); how the program picks the viewer index outside mode 6 (`FUN_14021ba60` takes the first valid player) is not fully read.
- **Joint animation** (`st00.dat`). The slot-0 block holds 15 tracks over 429 frames: scale keys (`f32 frame + 3 f32`) on joints 1 / 4 / 7 (bind scale 4.95/1.8/4.95, 6/1.86/6, 4.5/1.4/4.5 shrinking to 1 over frames ~359-429: the flat floor scatter pulls into a ball), 223 scale keys each on joints 10 / 12 / 14, and rotation keys (**`u16 frame + 3 s16` angles**, size 8: a layout the character reader does not have) on joints 2, 3, 5, 6, 8, 9, 11, 13, 15. The first keys equal the joints' bind pose, which settled both layouts. Baked world extent of the cloud: frame 0 `66.6 x 20.5 x 66.0`, frame 419.5 `11.9 x 11.2 x 11.8` (about the orb's size).

### Sim accessor

`Combat::pending_capsules() -> Vec<PendingCapsule { owner, spot, pos, elapsed, wait }>` (read-only; replay goldens unchanged; one new test, `pending_capsules_follow_the_respawn_wait`). A spot is listed from the tick after its capsule left until the capsule is back; the opening deal is never listed; only the soonest `arsenal.len()` spots per player (the program hands out the next skill at the pickup, the sim at the appear, so a spent arsenal never shows motes). Refactor: the two-line "hidden ticks" computation in `spawn_capsule` became `spot_hidden_ticks()` (same value, 840 normally).

### Retired

`fx.rs::materialise_burst` is no longer called from `sync_combat_visuals` (it was the ~1.4 s stand-in gather at the appear; with the real wait it would double-draw). The function stays only for the `PD_CAPSULE_PREVIEW=mat` calibration shot. The coloured flare (`flare_burst`) is unchanged: it is the program's states 1-2 draw.

### Footage (MEASURED, `Orb materialize.mp4`, 30 fps, 3.8 s, 116 frames; the pop is at m103)

- The clip starts about 3.4 s before the pop, so the first visibility is **not in the clip**: faint single orange points are already there at m12 (3.0 s before the pop) and the density grows steadily; a flat scatter on the floor over ~60 u at m50, a spinning shell by m82, a tight ball (~13 u incl. dots) at m98-m102, and it **vanishes at the pop** (gold pixels in the ball region 3,071 at m102 -> 281 at m103) while the orb appears in the same frame. The data says the same (start frames above). UNCONFIRMED how early the first dot is visible: the program draws the small clouds from frame 0 but their first dots are black until their ramp starts.
- Colour: gold to pale yellow, brighter over the lit floor (peak pixels about (255, 255, 145) at m100 over a gold floor; our dots over a dark bridge are orange). Density: about 250-300 dots in the final ball, matching the 285 points. Size: dots about 7-9 px across at 2878 px width next to an orb 195 px across, i.e. about 0.4-0.5 u; our quad half extent 1.0 (corners +-1) gives 0.43 u (FWHM). **The program's quad table (QuadMap constant buffer) was not found in the exe, so the quad half extent is UNCONFIRMED**: `QUAD_HALF = 1.0`, within the footage's measurement error; `PD_CAPBIRTH_QUAD` overrides.
- No bloom: the original has no glow pass (`glow.rs` header), so the footage's extra brightness is the gold floor under additive blending.

### Proof

`engine/shots/cb_*` in the main checkout (not committed): `cb_sheet.png` (footage on top, ours below, nine matching moments from 91 ticks before the pop to 9 after), `cb_run_t*.png` (scripted run: P1 captures a capsule at tick 250, then early / middle / last 2 s / appear; each file is the whole split screen, P1 on top), `cb_opp_t*.png` (P2's half shows P1's regenerating spot as dark specks). Command (PowerShell from `engine`, own build folder, `pd_game.exe <cache>` ...): `PD_MUTE=1 PD_CAPBIRTH_DIR=<converted folder> PD_TEST_SPAWN="-46.6,-260.166;90,-260.166" PD_TEST_PRESS=250,0 PD_TEST_STICK=1,180,262,270 PD_FREE_CAM=1 PD_SHOT_TICKS=240,300,420,650,900,908,948,988,1016,1048,1080,1088,1090,1092,1108,1130,1180` with `--near --start-hand 0 --stage st06 --screenshot shots\cb_run.png`. `PD_CAPBIRTH_LOG=1` prints elapsed / wait / frame every tick. The capsule is back at sim tick 1090 (840 hidden + 30 appearing after the press at 250).

### Converter

`pd_convert capsules <st00.alr> <st00.dat> <cache>\fx\capsules` also writes `cap00_cap01_birth.sprites.json` (and `cap02_cap03_death`, `cap03_cap04_braek`): per draw the joint, size / start / end, the points (x, y, z, u, v; right-handed) and the joint world matrix at every integer frame 0..429 (`F M F`, row-major 3x4). `pd_convert capprobe <st00.alr> <slot>` is the research dump used here (joints, materials, draws, hex of the odd chunks). `pd_game` finds the file in `fx/capsules` or in `PD_CAPBIRTH_DIR`.

### Unconfirmed / not done

- The quad half extent (above); the alpha-test reference (`cb0[16].x`, taken as 0: only the dot texture's transparent corners are cut); frames between integer frames blend two baked matrices instead of re-sampling the keys (the rotation keys are per frame, so the difference is below a pixel).
- The yaw sign for spots 1 and 2; the opponent look (not footage-checked); the pickup and erase bursts (slots 2 / 3) are baked the same way (`*.sprites.json`) but still drawn by the old stand-in (`pickup_burst`): same shader, strip tex4 + the dot, own start / end in their 0x13 chunks. A follow-up.
- The respawn fade-in of the orb (`OrbFade` alpha 0.05 on a respawn, `fx.rs`) is not in the program (`FUN_14021b3d0` starts the orb at alpha 255; the footage shows it nearly full-size within ~9 frames); left alone.

## Pickup and erase bursts, orb fade-in (2026-10-01, lane capfx)

Replaces the stand-in gold puff (`fx.rs::pickup_burst`, removed) and the flat 0.05 respawn fade. Code: `capsule_birth.rs` (`CapsuleBurst`, `drive_bursts`), `fx.rs` (`sync_combat_visuals` capsule arms, `OrbAlphaMemory`). Render only; no sim change. READ = program address (`PDUWP.exe`), MEASURED = `Orb Collect & Aura Consumed.mp4`.

### Bursts (READ)
- `FUN_14014fd10` calls `FUN_14021b4f0(record, 3)` on a normal take and `(record, 4)` when a skill erased the capsule (the erase also plays common/92 via `FUN_14014ed90`; `sfx.rs` already does). Mode 3 spawns an effect record of `st00` **slot 2 `cap03_death`**, mode 4 **slot 3 `cap04_braek`**, at the capsule record's position (`+8`), colour word `(owner has a view) << 24 | 0xffffff`, with a **random rotation**: three independent uniform angles (`rand() * (1/32767) * 360 * 32768 / 360`, `FUN_140290a40`). `FUN_14022e8a0` (called with the take) is not a spawn: it clears pool-0 records matching owner / spot / kind 1, i.e. nothing we draw (the earlier "on-player pickup glow?" guess is wrong).
- The record (`FUN_14022e7d0` spawn, `FUN_14022ea00` update, `FUN_14022eb30` draw): frame starts at 0, **+1 per program tick (30 Hz, 2 sim ticks)**, ends when the frame passes the animation length 89 with loop count 1: **90 program ticks = 3 s, drawn once, alpha 255, no fade**.
- Same `vkPSPRITE` material as slot 0 (name hash `c69e6d6c 789a0600`, blend bytes `00 11` = ONE + ONE, flags 0x19, identical record bytes except the textures): first texture `st00` **tex4** (the strip, **bright at u = 0 fading to black at u = 1**: white-gold to orange to nothing), second **tex1** (the dot). Seven draws of 3 points on joints 3, 5 .. 15; each draw's 0x13 chunk: size **50**, end **90**, start **0**, so a point's strip scroll is `frame / 90`. The points spread outward with the joint scales (mean radius from the centre 3.8 u at frame 0, 5.9 at 10, 9.2 at 30, 13.6 at 89).
- Drawn by the same code as slot 0 (`capsule_birth.wgsl`, mesh rebuilt per frame from the baked `.sprites.json`); the quad scales with the draw's size word, so half the regeneration dots'.
- Which one: `CapsuleErased { pos }` (the events lane's event) names an erased capsule; a capsule that vanishes at such a position plays slot 3, any other vanish slot 2. No sim change.
- **Gamma (new, calibrated against the footage):** the program adds display values; the regeneration shader adds the decoded linear product, which left these small specks invisible over a lit floor. The bursts add the stored value as `fx_gamma.wgsl` does for every skill effect (`params.w = 1`; `PD_CAPBURST_GAMMA=0` gives the old add). Slot 0 (the regeneration motes) is left as capbirth made it; the same switch would brighten them.
- UNCONFIRMED: the quad half extent (shared with the motes, 1.0); whether the original gives an AI player's capsules colour top byte 0 (dark specks like the opponent's regeneration cloud): every player of this build has a camera, so it is the additive gold for all; the rotation composition order (angles are uniform, so only the distribution of orientations is read); the exact pickup frame in the footage (about 133, +-2).

### Orb fade at a respawn (READ)
The 0xff is written to the render object's per-viewer fade bytes only when the round's slots are built (`FUN_14014ee70` -> `FUN_14021b3d0`, its only callers). A take (`FUN_14014fd10`) and the respawn never touch it, and the bytes move only while the orb is drawn (states 1-2, `FUN_140196c40`: -16 per tick within 10 u of the viewer, +16 otherwise). So the orb comes back at the level it was left at: a take from close by (the byte had faded toward 0 inside 10 u) climbs back over about half a second, an erase from afar leaves 255 and the orb is there at once. The opening deal and a new round start at 255. Ours kept a flat 0.05 on every respawn; now `fx.rs::OrbAlphaMemory` stores the last fade level per (owner, spot, own / ghost look) and a respawn resumes from it (`OrbFade::resume`). Test: `a_respawned_orb_resumes_the_fade_level_its_spot_left`. Our fade is still a scale, not the original's 32-level stipple. Not seen on screen (the spot is behind the camera after a walk-away); covered by the test only.

### Opening capsules
Unchanged and checked: they are in the list from the first tick (no `pending_capsules` entry, `respawn: false`), so no motes, no flare, no fade-in. `shots/capfx_open_t01*.png`: nothing at the spots up to tick 176, the orbs there at full size from tick ~180 (the countdown's START).

### Proof (`engine/shots`, not committed)
`cf_sheet.png` (footage frames 135..160 on top, ours at ticks 250..300 below: pickup), `capfx_pick_t*.png` (the scripted pickup run, command below), `capfx_erase_t*.png` (Violent Change, id 284 via `--loadout`, six slot-3 bursts at tick 302; `PD_CAPBIRTH_LOG=1` prints `[capburst] tick N slot S`), `capfx_open_t*.png`. Footage frames in `%USERPROFILE%\.pdtools\footage\capfx\`. Pickup command: `PD_MUTE=1 PD_FREE_CAM=1 PD_TEST_SPAWN="-46.6,-260.166;90,-260.166" PD_TEST_PRESS=250,0 PD_SHOT_TICKS=250,260,270,280,290,300,320,340 pd_game.exe <cache> --near --start-hand 0 --stage st06 --screenshot shots\capfx_pick.png`; erase: the same with `PD_TEST_SPAWN="-4,-260.166;90,-260.166"` and `--loadout 284,1,1,1` in place of `--start-hand 0`. `PD_CAPSULE_PREVIEW=pickup|erase` spawns one burst in front of player 1 (`--demo`; off-camera in the demo's own view).
