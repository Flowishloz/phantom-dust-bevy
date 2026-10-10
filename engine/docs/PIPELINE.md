# Asset pipeline (`pd_convert`)

Game files (`../game_files/Assets/Data/...`) → converted cache (`cache/`). The game and viewer only ever read `cache/`. Cache is regenerable; never commit it.

## Source data map (`game_files/Assets/Data`)

| Folder / ext | Contents | Status |
|---|---|---|
| `Map/stNN.alr` | arenas (models + textures + transforms) | converts; 30 files, 19 are real versus arenas (`cache/stages.json`, see "Stages" below) |
| `Map/stNN.dat` | stage layout: objects (props), jump zones, start points | read by `model`/`collision`/`stages` automatically (FORMATS-STAGES.md) |
| `setup/stageset/stageset_en.dat` | developer's own stage-select table: long name, short/family name, internal code per stage slot | read by `pd_convert stages` (not a standalone command; see `pd_formats::stageset`) |
| `Player/<id>/<id>.alr` | character: skeleton, skinned mesh, 129 animations | not what the game plays from -- read only as the fallback when no colour sibling exists (25 of 28 folders have `<id>0.alr`..`<id>3.alr`; `enm03a`, `enm12a`, `evpc00a` don't and use the base file, still unhandled) |
| `Player/<id>/<id>0..3.alr` | the 4 costume colours: what the game actually loads (charaset +0x3A colour count). Same motion set across all 4 (checked pc00a/pc01a colour 0 vs 1); differs from the base file (FORMATS-ANIMATION.md "Program evidence" "Animation files") | converts (`character` defaults to colour 0) |
| `Player/<id>/exmot0..5.ala` | intro/outro motions, different header | not handled |
| `BGM/*.stx` | music | converts to WAV |
| `gstorage/gsdata_<lang>.dat` | skill table (cost, damage, school, family) + names | converts (`skills`) |
| `setup/deckdata_debug` | 4 developer test arsenals | read by `skills --decks` |
| `Sound/*.bin` | sound effects + battle voices (STH2 banks) | converts (`sounds`); FORMATS-AUDIO.md |
| `setup/charaset/charaset_en.dat` | character → voice bank, English name, dodge move | read by `sounds`; `pd_convert chars <Data dir> <cache>` → `cache/chars.json` (dodge: jump / tumble / cartwheel) |
| `Effect/eftNNNN.alr` + `.eft` | skill effects (NNNN = skill text id) | converts (`effects`); FORMATS-EFFECTS.md |
| `*.ssb` | scripts (skill/boss logic) | not handled; see pd-ssb-decomp / hantu repos |
| `Event/*.mk/.ak` | NPC model/anim bundles | not handled |
| `*.mp4` | cutscenes | out of scope, not copied |
| root `*_vs.cso/*_ps.cso` | D3D11 shaders (DXBC) | readable with `tools/disasm/disasm.exe` |

## Commands

Run from `engine/` after `cargo build --release -p pd_convert`. `D=..\game_files\Assets\Data`.

| Command | Output |
|---|---|
| `pd_convert model $D\Map\st06.alr cache\st06.glb` | stage glTF + `st06.bounds.json` (core bounds, sky excluded). If `st06.dat` sits next to the `.alr`, its objects are placed (nodes `propNNN_<name>`) and `st06.props.json` + `st06.props.glb` are written next to the glb; `--no-props` turns that off |
| `pd_convert collision $D\Map\st06.alr cache\st06.pdcl` | collision file (stage + object hulls; `--no-props` = stage only) + floor-height survey + ASCII top-down map |
| `pd_convert tracecheck <a.pdcl> [<b.pdcl>] --takes <takes_full> [--list N] [--falls] [--cross]` | does a collision agree with where the ORIGINAL's fighters stood? Walks every take trace (mirrored z) through the file(s): steps with the body inside a wall, real moves crossing a face, falls; two files = the difference. COLLISION.md "dat+walls vs the program" |
| `pd_convert character $D\Player\pc00a\pc00a.alr cache\chars\pc00a.glb` | skinned character + all animations + extra parts, plus `cache\chars\pc00a.rootmotion.json` (per-clip root-joint drift for the sim, FORMATS-ANIMATION.md "Root motion"). Reads `pc00a0.alr` (colour 0), not the base file passed on the command line: the game itself only ever plays a colour variant (FORMATS-ANIMATION.md "Program evidence" -- "Animation files"); `--colour base\|0\|1\|2\|3` overrides. Folders without a colour sibling (e.g. `evpc00a`) fall back to the exact path given. Also writes `cache\chars\pc00a.c0..c3.png` (see "Costume-colour textures" below); `--no-colour-textures` skips that |
| `pd_convert charcolours $D\Player cache\chars` | just the costume-colour PNGs (`<id>.c0..c3.png`) for every character with colour siblings, without touching any glb/bounds/rootmotion -- cheap to (re-)run alone (see "Costume-colour textures" below) |
| `pd_convert music $D\BGM\04_Highway.stx cache\music.wav` | WAV (the old single-track fallback) |
| `pd_convert musictracks $D\BGM cache\music` | every BGM track as `<name>.pdm` with its loop region (tracks 1-41 and 60, about 1.2 GB); the game's music player reads these ("Music" below). Without them a match plays the old per-stage WAV and the menus are silent |
| `pd_convert sounds $D\Sound cache\sfx [--clips] [--wave common:509,...]` | every sound id as `cache/sfx/<bank>/<id>.wav` (22,050 Hz stereo, layers mixed) + `<id>_loop<k>.wav` for looping layers + `cache/sfx/index.json` (lengths, signal stats, character → voice bank). `--clips` adds the raw clips, `--wave` waveform PNGs in `cache/sfx/waves/`; Japanese voices skipped unless `--all-langs`. ~2 s, ~220 MB (the stage ambience is most of it). FORMATS-AUDIO.md |
| `pd_convert skills $D\gstorage\gsdata_en.dat cache\skills.json --decks $D\setup\deckdata_debug` | skill table as JSON (+ `skills.txt` readable table); see FORMATS-SKILLS.md |
| `pd_convert effects $D\Effect cache\fx [1,42,115]` | every skill effect as `cache/fx/eftNNNN.glb` + `.fx.json` (event slots + per-piece timing). Preview one: `.\shot.ps1 x pc00a --close 70 --fx eft0042 --fx-slot 0 --fx-frame 13` |
| `pd_convert hudtex $D\Menu\BtMenuEn.alr cache\ui` | battle HUD atlases `hud0..2.png` (HUD.md); also copy the HUD font: `Copy-Item C:\Windows\Fonts\bahnschrift.ttf cache\ui\hud.ttf` |
| `pd_convert sprites <alr> <tex> [min alpha]` | bounding boxes of the pieces in a UI atlas |
| `pd_convert scan $D` | parses every ALR, reports failures (regression check: expect `831 converted, 0 failed`) |
| `pd_convert stages $D\Map cache` | every stage: model + collision + `cache/stages.json` (+ `cache/music/<id>.wav` where known); see "Stages" below |
| `pd_convert capsules $D\Map\st00.alr $D\Map\st00.dat cache\fx\capsules` | the capsule/Aura Particle art: one named `.glb` per `st00.alr` model slot (idle orbs, materialise flare, environment-crystal models, ...), a `.points.json` sidecar for slots whose draws are point sprites (materialise sparkles, pickup/erase bursts -- no UV, so the normal mesh path drops them), `tex00/01/02/04.png` (their textures) and `capsules.json` (slot -> file names + `st00.dat`'s per-slot animation frame count). See `docs/CAPSULE-VISUALS.md` "Built 2026-09-28" |

Options: `model --untextured` (include position-only geometry), `character --order <c:210:0|game|zyx|xyz>` (rotation convention; default `game`), `character --anims 0,7,12`.

### Research / probe subcommands

| Command | Shows |
|---|---|
| `info <alr>` | chunk count, texture & vertex format histogram |
| `anims <alr> [id]` | per-model joints/draws; one animation's tracks with settings bytes |
| `keys <alr> <anim> <joint>` | every key of one track |
| `joints <alr> <model>` | joints (bind world pos, names) and draws with palettes |
| `skinprobe <alr>` | skin palette usage per draw, joint flags |
| `texprobe <alr>` | per-texture format/size/mean colour/usage |
| `eulertest <alr>` | ranks the 48 Euler conventions by impossible knee/elbow bends |
| `stagedat <stNN.dat \| Map dir> [-v \| --floors]` | stage layout probe: types (model vs hull box check), placement stats, objects vs floor, rotation-order scores, start points (FORMATS-STAGES.md) |
| `propcheck <stNN.alr> <cache>` | do players collide with the placed objects in `cache/stNN.pdcl`? |

## Stages

`Map/` has 30 `.alr` stage files. `pd_convert stages $D\Map cache` converts every one (model + collision: the 0x94 mesh + object hulls + the per-triangle `CLAS` class trailer, COLLISION.md; `--collision alr` / `dat+walls` are A/B switches only, not shipped) also writes `stNN.camcol` per stage that needs one (camera-only occluders, COLLISION.md; `pd_game` reads it for the camera's wall pull-in only) and writes `cache/stages.json`: `[{"id", "name", "versus", "note", "music"?}, ...]`, versus arenas first.

**Names are not guessed from renders.** `setup/stageset/stageset_en.dat` is the original versus stage-select table: a small header (file size, entry count, string-table offset — ✅ confirmed by matching the header ints against the file), then, per stage slot in order, three NUL-terminated strings: long name, short name/family, internal code (e.g. `"Spiral Highway"` / `"Highway"` / `"st02Highway-evening"`). **The slot's position in the table is the stage id** — slot 2 is `st02`, slot 12 is `st12` — verified because every internal code with an `stNN` prefix has `NN` matching its own slot, and every slot without a real stage (`"09 Dummy"`, `"10 Dummy"`, …) has no corresponding `Map/stNN.alr`. Parser: `pd_formats::stageset::parse` (see FORMATS-ALR.md for the byte layout).

This gives 19 real versus arenas in 5 three-way recolour families plus one two-way family, and 2 one-offs:

| Family | Stages | Example names |
|---|---|---|
| Highway | st02, st12, st22 | Spiral / Lightning / Sunlight Highway |
| Palace | st03, st13, st23 | Lost / Light / Noble Palace |
| Panorama | st04, st14, st24 | Panorama Earthquake / Building / Boss |
| Strange City | st05, st15, st25 | Dawn / Dusk / Midnight City |
| Refinery | st06, st16, st26 | Storm / Sunset / Acid Refinery |
| Lane | st07, st17 | Twilight / Silent Lane |
| — (one-off) | st01 | Edgar's Dream (code `LD_InsideofRuin`) |
| — (one-off) | st08 | Sein (code `st08Ruin`) |

Everything else is **not** a versus arena and `stages.json` marks it `"versus": false`: `st00` and `st09` are empty ("dummy") slots in the table; `st09_01..08` are further pieces of `st09` (large, don't share `st09`'s bounds, and look like an explorable hub when screenshotted — floating disconnected platforms, cracked cliffs, and in `st09_02`'s case an untextured teal dev-grid floor (textures all decoded fine, so this is the stage's own texture, not a conversion failure) — matching the guide text's "Jump Zone"/irregular-gravity descriptions and reinforcing that this is dev/exploration content, not a battle arena); `st32` isn't in the table at all and is a small flat untextured plane (likely a leftover test/practice arena).

Unconfirmed: which Surface Log entry ("World Beyond", "Tower of Sein") pairs with which of the above — the guide text names 9 locations but the table only gives family names for 7; `st01`/`st09` are plausible candidates for "World Beyond" (a dream/ruin one-off; the hub) but not verified. Also unconfirmed: whether `st01` (a story dream sequence by name) and `st24` ("Panorama Boss") are actually selectable in versus mode or only in story — both are non-dummy table rows so `stages.json` includes them as versus, but neither has recolour siblings the way a real versus family does. See ROADMAP.md.

**Music** (SUPERSEDED 2026-10-03: the stage -> track map below was a guess; the program's table is in "Music (lane infomusic)" at the end of this file): `BGM/` track basenames are romanized Japanese location names. Matching them to the internal codes above (`YojoPlant` = "Plant" = Refinery, `KiokuNoMachi` "Memory Town" = `TownofMemory` = Lane, `Iseki` "Ruins" = `st08Ruin` = Sein) gives a confirmed per-stage track for Highway, Panorama, Refinery, Lane and Sein (12 stages; each family's three BGM variants — base/1/2 — assigned to its three stage variants in ascending `stNN` order). **Palace = `ShoppingMall` (01–03), Strange City = `JuryokuHanten` "Gravity Reversal" (13–15)** ⚠️ (2026-09-27): the exe's BGM table (`PDUWP.exe` file offset 0x3E6A00: pointer to the name + a word whose second byte is a stage group) puts every track in a group — ShoppingMall 0, Highway 1, Panorama 2, JuryokuHanten 3, YojoPlant 4, KiokuNoMachi 5, Iseki 6, menus 0xFF — so these two are the only stage groups left for the two unmatched families; Lost Palace is visibly a ruined shopping mall (escalators, shop shutters, café tables) and Strange City is the stage with flying cans and signs. The group numbers don't give the stage ids directly, so this is a strong reading, not a proof. `pd_convert stages` converts the matched tracks straight to `cache/music/<id>.wav`.

**Objects (props)**: each stage's `.dat` places its objects (desks, drums, railings, floor sections…) into `stNN.glb` and their collision hulls into `stNN.pdcl`, and writes `stNN.props.json` (every object: type, name, class/kind/param with meanings where known, position, matrix, bbox, solid, intact/broken/damaged model; the per-type table; the other layout variants; the six original start-point sets) plus `stNN.props.glb` (each type's intact/broken/damaged model at the origin, nodes `typeNN_intact|broken|damaged`, for swapping in when something breaks). Details and evidence: FORMATS-STAGES.md. Before/after proof: `engine/shots/props_sheet.png`.

**Core bounds** (`bounds.json` `min`/`max`, used by the viewer's framing and the game's spawn survey) are the 5th–95th percentile of draw boxes, now ignoring draws larger than 1500 units (sky shells, sea planes, distant backdrops) when at least 20 smaller draws remain, and counting placed objects. Without this, removing the object templates from the origin blew st04's box up to ±10,000. Most stages' boxes got tighter (e.g. Highway ±5,900 → ±450); every stage re-checked by screenshot below.

Verification: every stage was screenshotted with `pd_game --stage <id> --demo --screenshot ... --shot-frame 400` and looked at — players stand on a textured floor, not the sea, in every case checked. Montage: `engine/shots/stages_sheet.png` (all 30, labelled).

## Costume-colour textures (2026-09-27, altcolour lane)

`Player/<id>/<id>0..3.alr` (above) are the four costume colours the game
itself offers for every playable character; `pd_convert character` reads
only colour 0 into the glb. Rather than exporting a full duplicate model per
colour (4x the same mesh/rig/animations), the converter now also decodes
just the one texture that differs and writes it as a small PNG:
`cache/chars/<id>.c0..c3.png`, next to `<id>.glb`.

**Which texture**: found, not assumed — `pd_formats::character::
base_colour_texture_index` picks whichever `MaterialEntry::texture_idx` covers
the most drawn vertex indices across every skinned draw (the same method as
`pd_convert texprobe`'s "used-indices" column), then that same index is read
out of each colour sibling (they share layout; only the bytes at that
texture's slot differ). Confirmed on all 11 playable characters (`pc00a`..
`pc10a`, every one has colour 0-3 siblings): it's always index 0 (a 512x512
Dxt1 covering ~9,000+ of the character's drawn indices) — texture 1 (a
256x256 Bgra5551, an object-space normal map referenced only via
`MaterialEntry::normal_idx`, PROGRAM-RE.md) never differs and is never used
as a base colour — but the index is computed per-character rather than
hardcoded in case a character differs. `pd_convert texprobe <id>0.alr` vs
`<id>1/2/3.alr` on each character shows only this one texture's mean colour
changing (same format/size/resource pointer). Decoded, the four colours are
real, distinct costumes, not a hue shift — e.g. pc00a's jacket is
green/maroon/black/silver across colours 0-3, pc08a's outfit purple/yellow/
orange/teal (`engine/shots/alt_colour_sheet.png`, looked at, all 11
characters x 4 colours).

**Commands**: `pd_convert character ... <out>.glb` writes all four PNGs as a
side effect (colour 0 too, even though it's already baked into the glb —
cheap, and keeps every colour the same kind of file); `--no-colour-textures`
skips them. `pd_convert charcolours $D\Player cache\chars` writes just the
PNGs for every character with colour siblings, without rewriting any
glb/bounds/rootmotion — what actually regenerated the main cache (below),
so nothing another lane had already converted was touched.

**Game side** (`pd_game`): not read directly by any converter code, but see
FRONTEND.md "Alternate costume colour" for how the game swaps P2's material
onto `<id>.c1.png` when both players pick the same character.

## Regenerate the whole cache

```powershell
$c=".\target\release\pd_convert.exe"; $D="..\game_files\Assets\Data"
& $c stages "$D\Map" cache
& $c music "$D\BGM\04_Highway.stx" cache\music.wav
& $c sounds "$D\Sound" cache\sfx
& $c skills "$D\gstorage\gsdata_en.dat" cache\skills.json --decks "$D\setup\deckdata_debug"
& $c effects "$D\Effect" cache\fx
& $c stagefx "$D\Map" cache          # arena-object break effects -> cache/fx/stage (FORMATS-STAGES.md "Object effects")
& $c hudtex "$D\Menu\BtMenuEn.alr" cache\ui; Copy-Item C:\Windows\Fonts\bahnschrift.ttf cache\ui\hud.ttf
& $c hudtex "$D\Menu\DckMenu.alr" cache\ui\dck   # menu backdrop: the original arsenal menu's stormy sky (texture 1, 640x380 top left)
& $c menus "$D" cache\ui\menu   # front-end art, the game's menu font (System/dbfont.ddx), bios (MENUS.md)
& $c chars "$D" cache                # chars.json: each character's dodge move (FORMATS-ANIMATION.md)
& $c skyscroll "$D\Map" cache        # stNN.sky.json: sky drift rates (GAME.md "The sky drifts")
& $c capsules "$D\Map\st00.alr" "$D\Map\st00.dat" cache\fx\capsules
& $c shotpoints "$D" cache           # SHOTnn markers for the sim
& $c cpu "$D" cache                  # CPU opponent data -> cache/com (FORMATS-CPU.md)
& $c musictracks "$D\BGM" cache\music  # <name>.pdm music with loop regions ("Music" below; added 2026-10-07 by the archivist from the infomusic lane)
Get-ChildItem "$D\Player" -Directory | % { $n=$_.Name; if (Test-Path "$D\Player\$n\$n.alr") { & $c character "$D\Player\$n\$n.alr" "cache\chars\$n.glb" } }
```

`engine/regen_cache.sh` (Git Bash) runs the same list into a new folder; including `musictracks` (added 2026-10-07).

The `character` loop above already writes each character's `cache/chars/<id>.c0..c3.png` costume-colour textures as a side effect (2026-09-27; "Costume-colour textures" above); to refresh only those without re-exporting every glb, run `pd_convert charcolours "$D\Player" cache\chars` instead.

To refresh only the stages without touching `stages.json`/music: `Get-ChildItem "$D\Map" -Filter *.alr | ? { $_.Name -notmatch 'cam' } | % { $s=$_.BaseName; & $c model "$D\Map\$s.alr" "cache\$s.glb"; & $c collision "$D\Map\$s.alr" "cache\$s.pdcl" }`.

Stage lighting: `model` and `stages` read the light script from the sibling `.dat` (FORMATS-STAGES.md "Lighting script"), bake it into the glb's vertex colours, draw the sky model's backdrop, and write `cache/<stage>.light.json` (read by the game for character light). `model --unlit` = the old unlit export. Regenerated so far: st03, st13, st23 only (2026-09-27, palace lane): `pd_convert model $D\Map\st03.alr cache\st03.glb` (same for st13, st23), `pd_convert music $D\BGM\01_ShoppingMall.stx cache\music\st03.wav` (02_ShoppingMall1 → st13, 03_ShoppingMall2 → st23) and a `"music"` entry added to those three in `stages.json`. Every other stage needs a regenerate to get lighting + sky (and st05/15/25 their music).

Cache layout the game expects: `cache/<stage>.glb`, `cache/<stage>.light.json` (optional), `cache/<stage>.bounds.json`, `cache/<stage>.pdcl`, `cache/<stage>.props.json` + `.props.glb` (not read by the game yet), `cache/stages.json`, `cache/music.wav`, `cache/music/<stage>.wav` (where known), `cache/sfx/` (optional; no sound without it), `cache/chars/<id>.glb` + `.bounds.json` (+ `.rootmotion.json`, not read by the game yet -- for the sim lane), `cache/chars/<id>.c0..c3.png` (optional -- no P2 costume swap without them, "Costume-colour textures" above), `cache/skills.json`, `cache/fx/eftNNNN.glb` + `.fx.json`, `cache/ui/hud0..2.png` + `cache/ui/hud.ttf`.

## Future (iw4L model)

The shipped runtime should convert on first launch from a user-selected game folder into a cache next to the exe, exactly like iw4L / skate-3-rust-engine. Today conversion is a manual CLI step.

## Music (lane infomusic): what the original plays and when (2026-10-03, lane infomusic)

Laurence: "The music just loops 1 song." Verdict: the original does not loop one song everywhere, and ours had three real differences. Code: `music.rs` (mapping, loop player, tests), `pd_formats::stx` (loop region, `.pdm`), `pd_convert musictracks`.

READ (`PDUWP.exe` + the game's scripts; method: the BGM table, the VM natives, the quest scripts' `gamestart`):
- **Who plays music.** `FUN_14019cdf0(track, mode)` is the only reader of the BGM table (`0x1403e8c00`: name pointer + flags + a stage-group byte; track number = the file's number, 04_Highway = 4); it is reached only through the script natives `g1.f16` (play) / `g1.f17` (stop) / `g1.f18` / `g1.f19` (volume). Asking for the track already playing does nothing. The group byte is the Xbox "music manager" slot: a custom soundtrack set for that group replaces the track (not in scope).
- **Menus** (the scripts' `push N; g1.f16`): title / main menu 31 (Title), versus menus 34 (TaisenSelect: set-up, character, stage), arsenal editor 29 (DeckEdit), options 33, shop 60, Live menus 35. **Results** (`battleend_en.ssb`): 37 BattleWin or 38 BattleLose, jingles that play once. Ours is built that way now (Title 31 (the Modes screen this named was removed 2026-10-03), Setup 34, Arsenal 29, Result 37 or 38; a draw = 38, UNCONFIRMED for versus).
- **Battle: one track per match, chosen once.** The battle runs a quest script (`Event/en/questNNN.ssb`); its `gamestart` export calls a function that reads the stage number (`g4.f425` returns `DAT_1404c01d8`) and looks the track up in a 29-word table (`quest000.ssb` at 0x208; the same table sits in 138 of the 149 quest scripts): stage 2 -> 5, 3 -> 1, 4 -> 8, 5 -> 14, 6 -> 11, 7 -> 16, 8 -> 19, 12 -> 6, 13 -> 2, 14 -> 7, 15 -> 15, 16 -> 10, 17 -> 16, 18 -> 19, 22 -> 4, 23 -> 3, 24 -> 9, 25 -> 13, 26 -> 12 (1, 11, 21 -> 24 LastBoss; dummies 0). **Tracks 16 and 19 get `rand * 3 >> 15` added**: Twilight Lane / Silent Lane play one of 16/17/18 and Sein one of 19/20/21, drawn at the start of each match. Nothing else in the battle starts music (the only music natives in the script are in that function), so there is **no low-health, round or time-out switch**; the match's track fades out at its end (`FUN_14012b300`, fade rate not measured).
- **Tracks never move on by themselves.** The STX header gives a loop region (`u16 @0x1A` start block, `@0x1C` end block; block 0 is the header): the intro plays once, then the region repeats (04_Highway: block 181 to the end, about 2 s of intro). The jingles (37, 38, 41, 61) carry 65535 = no loop and play once.
- **Our old mapping was wrong for 10 of the 15 stages of the three-variant families.** The old guess gave the three variants in ascending stage order; the program says Highway Spiral 5 / Lightning 6 / Sunlight 4, Panorama Earthquake 8 / Building 7 / Boss 9, City Dawn 14 / Dusk 15 / Midnight 13, Refinery Storm 11 / Sunset 10 / Acid 12 (the three Palaces were right). The family assignment (Palace = ShoppingMall, City = JuryokuHanten, ...) is now confirmed too: the table itself puts them there.
- **Ours before:** one WAV per stage looped whole (so the intro repeated every time), no menu music, the Lane/Sein stages one fixed variant.

UNCONFIRMED: which quest script a versus battle loads (`FUN_1401b9f40` takes a quest number from a mission row, 7 when none; all quests share the stage table, and quest007's story-only override table keys on `g7.f33(7)`, which a versus battle should not hit); whether the match track restarts each round (the music call sits in `gamestart`, which the battle state machine reaches once per match through state 2, so we start it once per match); the fade length at the end; the result jingle for draws and per player in versus; `g1.f19(4096)` volume (we play at full volume as before); that a menu track is not restarted when the next menu asks for the same number (the program's own rule; ours only changes track on a change of screen).

Cache: `pd_convert musictracks "<Data>\BGM" <cache>\music` writes `<name>.pdm` for tracks 1-41 and 60 (about 1.2 GB, 38 tracks); `pd_convert stages` now writes the READ stage track as the fallback `music/stNN.wav` (re-run if the old WAVs should be fixed). Without `.pdm` files a match plays the old per-stage WAV and the menus stay silent.
