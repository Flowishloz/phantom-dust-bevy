# Research: open STAGE questions vs public sources (2026-09-30)

Scope: answer the open stage questions from FORMATS-STAGES.md and ROADMAP "Open questions" using public information only. No game data was available. Nothing here is copied code: layouts are restated as facts in my own words.

Bottom line: public sources are thin. Esper-RE only *confirms* our header, PS entry, CP00 and OC00 layouts. It adds **nothing** on EF00/OE records, OB00, hit points, weight, start-point-set selection, or DSP_*.bin. The one genuinely relevant public claim (st01 is an unfinished, unused stage) agrees with our `"versus": false` decision but comes only from search snippets.

Legend for each source: **READ** = I fetched the raw file and read it in full. **SNIPPET** = web-search summary only, page not readable (wiki returns 403 to bots).

## Sources tried

| Source | Status |
|---|---|
| https://raw.githubusercontent.com/Torphedo/Esper-RE/trunk/src/formats/st00.h | READ (4 KB, the whole stage .dat header/record definitions) |
| https://raw.githubusercontent.com/Torphedo/Esper-RE/trunk/src/polaris/gui/mapdata_editor.cxx | READ (how Polaris walks the .dat) |
| https://raw.githubusercontent.com/Torphedo/Esper-RE/trunk/src/formats/eft.h | READ (effect `.eft` in Assets/Data/Effect, NOT stage EF00; see Q4) |
| https://raw.githubusercontent.com/Torphedo/Esper-RE/trunk/src/formats/sth2.h | READ (sound bank `.bin` STH2 layout) |
| https://raw.githubusercontent.com/Torphedo/Esper-RE/trunk/src/formats/ssb.h | READ (SSB header/opcode enum; marked "may be out of date" by its author) |
| https://raw.githubusercontent.com/Torphedo/Esper-RE/trunk/src/formats/charaset.h | READ (character set table; unrelated to stages) |
| https://raw.githubusercontent.com/Torphedo/Esper-RE/trunk/README.md | READ (feature list) |
| https://raw.githubusercontent.com/Torphedo/Esper-RE/trunk/src/formats/alr.h | fetched, only grepped for stage-related words (nothing relevant); not read in full |
| https://raw.githubusercontent.com/VSaige3/hantu/main/README.md | READ (one line: visual scripting for SSB). `src/` listing not reachable; no stage content expected |
| VSaige3/pd-frameviewer, Torphedo/pd_loader_core (raw README on main/master/trunk) | 404 on every branch name tried; not reachable |
| https://phantomdust.miraheze.org/wiki/Stage_List and other wiki pages | 403 to curl and WebFetch. Only search-result snippets available (SNIPPET) |
| GitHub tree pages and api.github.com for Esper-RE | tree HTML returned no links; API returned 403 (repo not enabled for session). `src/polaris/editor_alr.cxx` guess = 404. No file listing, so other headers could only be guessed by name (`ef00.h`, `map.h`, `dsp.h` etc. all 404) |

The Esper-RE README lists the complete set of C headers: alr, alr_animations, deck, dxbc_cso, eft (+ e.i), eventpack (.ak/.mk), questdata, ssb, st00 (`.dat`), sth2 (`.bin`), stx, wav. So **there is no public header for EF00, OB00, OE, OA, SP00, MD00 or DSP_*.bin**. The Esper `.dat` support is only what st00.h and mapdata_editor.cxx show.

## What Esper-RE's st00.h says (READ), and how it lines up with ours

Header size 0x13C, matching ours. I recomputed every named field's offset from its struct order and they agree with our header table:

| Offset | Esper name | Ours | Verdict |
|---|---|---|---|
| 0x04 | "chunk_size", used as the offset of the `PS0/` list | PS0/ offset (0x140) | CONFIRMS. Esper's name is misleading; it is the list offset |
| 0x08 | unk1: only st09 area files, only values 1, 2, 3, 9 | not in our table | NEW (minor) |
| 0x0C / 0x10 | ps01_offset (points just past the tag) / ps02_offset "only used in st24" | same; st24 PS02-only | CONFIRMS both, incl. st24 having only PS02 |
| 0x14 / 0x18 / 0x1C | area_ssb, area_dynlight, area_ssb2 (st09_02 only) | not listed | NEW (minor): area-file fields inside the ST00 header |
| 0x20..0x2F | 4 unknown u32; "in st00 the 2nd points to file end" | not listed | NEW (minor, st00 only) |
| 0x30 / 0x38 / 0x3C | ps00_count / ps01_count / ps02_count | same | CONFIRMS |
| 0x34 | unk4, "only used in st00" | not listed | NEW (minor) |
| 0x5C / 0x60 | NM00 offset / count | same | CONFIRMS |
| 0x64 / 0x68 / 0x6C | OC00 / OC01 / OC02 | same | CONFIRMS |
| 0x70 | OE00 | same | CONFIRMS |
| 0x74 | named "EF00_offset1" | we call it OE01 | Esper's name suggests OE01 blocks start with EF00, which agrees with our "OE blocks contain EF00 records" |
| 0x78 | OE02 | same | CONFIRMS |
| 0x7C / 0x80 / 0x84 | OA00 / OA01 / OA02 | same | CONFIRMS |
| 0x88 / 0x8C | two unknown | not listed | NEW (minor) |
| 0x90 | OB00 | same | CONFIRMS |
| 0x94 | unk8 | our "untagged ~150 KB hull block" | both unknown |
| 0x98 | "EF00_offset2" | stage EF00 | CONFIRMS |
| 0x9C / 0xA0 | SP00 / MD00 | same | CONFIRMS |
| 0xA4, 0xA8, 0xAC | CP00 sets 1, 2, 3 | same | CONFIRMS |
| 0xB0, 0xB4 | two unknown ("unk9[2]") | we say 0xB0 is a real CP00 set (one start, capsules spread along the map), 0xB4 always -1 | **Esper does not know the 0xB0 set exists.** Its editor shows only 5 CP00 tabs (0xA4, A8, AC, B8, BC). Our sixth set is new relative to Esper |
| 0xB8 / 0xBC | CP00 sets 4, 5 | same | CONFIRMS |
| 0xC0..0xCF | 4 unknown | | |
| 0xD0 | ssb_offset | embedded light script start | CONFIRMS. Esper's area header says the SSB has a single exported function named "maplight" (matches the exe string `maplight%d.ssb` in our docs) |
| 0xD4..0xF3 | 8 unknown | | |
| 0xF4 | dynlight_alr_offset | "ALR-chunk run ... identical in st03/13/23 ... not their lighting" | **Contradicts our doubt.** Esper names it a dynamic-light ALR. Our own finding that it is an ALR run (chunk 0x0C, 0x05 animation of 90 frames) fits "an ALR holding a light animation". So it may well be an animated/dynamic light, just not the static maplight script. Plausible, unverified |
| 0xF8 / 0xFC | footer_offset / ssb_end_offset | footer / SSB end | CONFIRMS |
| 0x100..0x137 | 14 unknown | | |
| 0x138 | filesize | same | CONFIRMS |

Esper also notes: in st00 unused offsets are 0 instead of -1 (ours: st00 is an older 0x28 header; the two claims differ in detail, but st00 is not an arena and is not decoded by us anyway). Esper does not mention st32's all -1 header.

**Area (st09_NN) header, 0x40 bytes (READ):** magic `AR01..`, then an offset-looking value and a count-looking value, MD00 offset, -1, SSB offset (single function "maplight"), dynlight-ALR offset, a second SSB offset (only st09_02), seven -1s, file size. This matches our "0x40 header, script + light data, no placed objects" and adds concrete field positions: 0x0C = MD00, 0x14 = SSB, 0x18 = dynlight ALR, 0x1C = second SSB (st09_02 only), 0x3C = size. NEW detail. Status: READ.

## Q1. Which placement list does versus use? Which start-point set?

- **Public answer: none found.** Esper's editor just shows `PS0/` and `PS01` as two tabs, `CP00 (1)..(5)` as five tabs, with no comment on when the game uses which. SNIPPET-level searches found no wiki text on it. Status: unanswered.
- Esper's object-index rule: object id 0 = ALR mesh index 8 (constant `FIRST_OBJ_IDX`). CONFIRMS our "slots 0-7 are the stage, then object models". However Esper's viewport mapping only uses OB00-word-0-style intact model order (it adds the NM00 id to 8 directly), whereas we go through OB00 word 0. Both agree for the intact model; ours is the more complete rule.
- Esper never reads `PS02` at all. Only our sim and the st24 fallback use it.
- Esper bug worth knowing if anyone compares tool output: its PS01 tab loops over `ps00_count` for **both** lists (the code uses ps00_count for PS0/ and the same loop function for PS01). So Polaris will mis-size PS01 wherever the two counts differ. Do not treat Polaris PS01 output as evidence. READ.
- Public-source hint only (weak): a 17-versus-map list exists on the wiki (see Q3), and our docs already note the guide describes Panorama with destructible office floors, which matches `PS0/`. Nothing contradicts keeping `PS0/` as the default.
- What would settle it (needs game data or the exe): how the exe indexes header 0x0C/0x30 vs 0x04 at stage load, and the CP00 set index passed per player count. Suggest a disassembly pass or a runtime probe rather than more web work.

## Q2. Where do object hit points / weight live?

- **Not in the `.dat` as far as any public reader shows.** Esper's PS01 entry has three unknown small fields (u8, u8, u16) that we already map to class/kind/param; it has no HP/weight field. There is no OB00 struct in any public header (OB00 offset is only named, never parsed). Status: no public evidence. CONFIRMS our "not found in the file" but adds no location.
- The SSB header/opcode file (READ) defines a script VM with function-table calls, COND_JMP, stack push etc. and a call to "native" functions (`UNK_NATIVE`, `FUNC_TABLE_CALL`). Nothing ties it to object stats.
- My inference (not sourced): `kind` byte + `param` are the only per-instance data; PK damage "depends on object" is most likely a per-kind table inside the exe (or a gsdata-like table), because the stage `.dat` has no spare per-type scalar beyond the 0x38-byte OB00 record, where our words 3, 5, 8, 11 are always -1 and 12/13 are padding. One cheap data check we can do ourselves: compare OB00 words 12/13 and the `attr` hull values against type names for a correlation with "heavy" objects. Flagged as a suggestion, not a finding.

## Q3. Do st01 "Edgar's Dream" and st24 "Panorama Boss" appear in versus?

All SNIPPET (wiki page unreadable; summaries came from the search engine, so treat wording as second-hand):
- https://phantomdust.miraheze.org/wiki/Stage_List (snippet): Edgar's Dream has internal name **LD_InsideofRuin**, id **st01**; described as an **unfinished/unused stage that hackers found by scouring the disc**, present in the stage list but **not available in normal gameplay** in the original release. CONFIRMS our `"versus": false` for st01 and gives a stronger reason than our "story-only" guess: it is unfinished/unused rather than story-only. Note that our surveys show st01 has 106 placed tower pieces, consistent with a half-built stage.
- Same snippet: Panorama variants are st04 "Panorama Earthquake", st14 "Panorama Building", st24 "Panorama Boss"; the **17 multiplayer maps** are Lost/Light/Noble Palace, Spiral/Lightning/Sunlight Highway, Panorama Earthquake/Building, Dawn/Dusk/Midnight City, Storm/Sunset/Acid Refinery, Twilight Lane (that is 15 named in the snippet; the snippet text dropped two, presumably Sein, e.g. st08, and one more). Panorama Boss is **not** in the versus list as summarised. CONFIRMS our decision to drop st24 and the count of 17.
- Cross-check with our docs: our 17 versus arenas come from the `stageset_en.dat` selectable flag = 0 for exactly st01 and st24. The public 17-map list is the same count. Two independent routes agree, which closes the ROADMAP question (note the ROADMAP bullet is stale: FORMATS-STAGES.md already says versus false for both; the ROADMAP text still says `stages.json` marks them true).
- Mapping of wiki names to ours, for a cache label check: Lost Palace = st03 (blue), Light = st13?, Noble = st23? Our docs say "Palace-blue / yellow / red" for 03/13/23, so the wiki's "Light Palace / Noble Palace" correspond to yellow/red in some order. Unverified, names only.
- Caveat: I could not open the page, so the exact flag semantics ("unused" vs "story only") are the search engine's paraphrase. Low risk for the decision, but do not quote it as verbatim wiki text.

## Q4. Stage `.dat` layout: EF00, kinds, triggers, DSP_*.bin

**EF00 / OE / stage effects.** No public header or editor parses them. Esper's `eft.h` (READ) is a different thing: it describes the 412-byte `.eft` files in `Assets/Data/Effect/` (12 entries of 5 u32, a u32 that may be padding, then 6 entries of 14 i16 where the first ~4 are real and the rest -1) and the single file `e.i` (16-byte header + 495 entries of 12 bytes, total 0x1744, first field counts up by 1 per entry). All fields are unknown there. Relevance to us: none of it matches our 92-byte EF00 record (kind/trigger/pos/rot/scale/12 bytes/10 floats). So **our EF00 layout, kinds and triggers are new relative to all public sources**; no confirmation or contradiction is possible. Esper's header naming only tells us Esper knew two EF00-tagged regions exist (0x74 and 0x98), which agrees with our OE01 and stage EF00 pointers.

**DSP_*.bin (our FORMATS-AUDIO.md: 10 files, 25,836 bytes each, start with zeros, not STH2).**
- Esper's `sth2.h` (READ) covers only STH2 sound banks: a 0x40 header with magic, size, offsets to a "REAL" table and a "WAVE" table; TRAT/REAT tables of 0x100-byte EVNT headers carrying sample rate (UWP builds use 22050 Hz), clip size and clip index. This matches our statement that the 73 other `.bin` files are STH2 banks. It has **nothing** for DSP_*.bin; `--extract-audio` docs say `.bin` = "sound effect" files, presumably only STH2 ones.
- Web search for "Phantom Dust DSP bin" returned only unrelated synth/reverb pages. No public information. Status: unanswered; our "probably DSP/reverb settings per stage" stays a guess. Size 25,836 = 0x64EC, not a multiple of 0x40, consistent with a small table, not audio. Suggest: check whether the ten files' sizes/first non-zero region correlate with stage families (10 files vs 17 versus arenas / 30 stage files).

**Kinds/triggers.** Esper has no name for the `kind` byte. It only labels the two bytes plus u16 as unknown. So our class/kind/param split, the behaviour-code table and the trigger-byte meanings are **ours alone**.

## Q5. Object and effect record layouts

Object (PS) record, Esper: object id (s32), u8, u8, u16, u32 pad, position (3 f32), rotation Euler radians (3 f32), 0x24 total. CONFIRMS our layout field-for-field (class = first u8, kind = second u8, param = u16, always-zero u32 at 0x08). Esper's own comment on rotation says only "Euler rotation in radians" and does not give the order; our R = Rz·Ry·Rx with measured evidence remains the stronger statement. Esper never gives a scale and never uses one, consistent with our "no scale field".

CP00 (start point), Esper, 0x50 bytes: player position (3 f32), float unk, float unk2, three capsule positions (3 x 3 f32), a float that is "often NaN (FF FF FF FF)", then four u32 and a float. Mapping to ours:
- 0x00 position: CONFIRMS. 0x0C (first unk) = our yaw: Esper did not identify it, so **yaw is new to us**. 0x10 (unk2) = our "f32 about 0 (unknown)": agrees it is unknown.
- 0x14..0x37 capsules: CONFIRMS (Esper calls them "capsules", same word).
- 0x38 "often NaN" = our first word 0xFFFFFFFF: CONFIRMS. Esper's following fields (4 u32 + float) agree with our "0x80, 3, 0x18, 0, pointer-like" run; exact mapping: our six words vs its one NaN + four u32 + one float = six words. Matches in count. Esper calls the last one a float; ours "pointer-like". Minor difference, unresolved.
- Container: tag `CP00`, u16 count, u16 byte size = count x 0x50. CONFIRMS exactly.

OC00 hull chunks, Esper: 12-byte chunk header (u8, s8 id, 2 bytes, u32 size, u32 pad). id 3 = index buffer chunk: 10 floats, u16 count, 3 u16, then count entries of 0x14 bytes {3 x u16 indices, u16 unknown, 3 f32}; id 8 = vertex buffer, size/12 points. CONFIRMS ours. **Difference in interpretation:** Esper calls the trailing 3 f32 of each face a "pos"; we measured it as the face **normal** (20-byte face = indices, attr, normal). Esper's own dump code ignores it, so ours is better supported. Also Esper's dump treats the OC00 block as running from OC00 offset to the OC01 offset, which matches our "runs of chunks, -1 id ends one type's hull", but Esper does not split per type.

OB00 (our 0x38 records), OA (animation), OE/EF00 blocks, SP00, MD00: **no public layout**. Everything in our FORMATS-STAGES.md for these is unconfirmed by the outside world.

NM00 (type names): Esper reads NM00 as `count` consecutive NUL-terminated strings from the offset. CONFIRMS our "type names". The ALR name/hull index link is via OB00 in ours.

## Flags: new / confirms / contradicts

**Confirms ours (all READ from st00.h and mapdata_editor.cxx):** header size and every offset above; PS entry 0x24 layout; st24 has PS02 only; CP00 container and 0x50 record with position + three capsules + NaN word; OC00 chunk ids 3 and 8 and face layout; ALR first-object index 8 (slots 0-7 = stage); the area file header; "maplight" single-function SSB for area files; 17 versus maps and st24 absent (SNIPPET).

**New to us:**
1. Esper names 0xF4 a dynamic-light ALR. We called it unexplained. Worth re-checking: is the 90-frame 0x05 animation an animated light? (READ header name; meaning unverified.)
2. st01 is described publicly as an unfinished/unused stage (internal name **LD_InsideofRuin**), not merely story-only. (SNIPPET)
3. Area header field positions (0x0C MD00, 0x14 SSB, 0x18 dynlight ALR, 0x1C SSB 2) and the st09 "unk1" values 1/2/3/9 at 0x08 of the arena header. (READ)
4. Esper's st00 note: unused offsets are 0, not -1, in st00. (READ)
5. Esper's PS01 tab uses the PS0/ count for both lists, so Polaris is unreliable for PS01 sizing. (READ)
6. The unused header words at 0x88/0x8C, 0xC0-0xCF, 0xD4-0xF3, 0x100-0x137 are unknown publicly; we have not documented them either.

**Contradicts ours:** nothing structural. Two soft tensions: (a) Esper treats 0xF4 as a dynamic-light ALR while we wrote it is "not their lighting" (we only ruled out the static light script, not dynamic lights); (b) Esper calls the OC face trailing floats "pos" and we call them normals (ours has measured evidence; keep ours). Also our ROADMAP open-question bullet for st01/st24 is out of date relative to FORMATS-STAGES.md (marks them versus true); suggest it be closed in the next docs pass (I did not edit it).

**Still unanswered after public research:** which PS list and which CP00 set a 1v1 uses; where HP/weight live; EF00/OB00/SP00/MD00 semantics outside our own reverse engineering; what DSP_*.bin is.

## Suggested next steps (need game data or the exe, not web work)
1. Probe 0xF4/0xF8 in st03/13/23 as a light ALR (check if animation channels look like colour/intensity).
2. Count distinct `kind` values per type against any per-type number in the exe's object table (hit points would sit next to a type-kind table).
3. Runtime or disassembly check of the CP00 set index vs player count (Esper numbering 1..5 = header 0xA4, 0xA8, 0xAC, 0xB8, 0xBC; our extra set at 0xB0 is unknown to the community).
4. Ask the Phantom Dust modding Discord/wiki maintainers (Torphedo, VSaige3) whether they know the PS0/ vs PS01 split; the modding wiki has a Stage List page but its text was not readable here.
