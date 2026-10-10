# Audio

## Current gameplay acceptance — 2026-09-30

Laurence: **steps are good; capsule use and collection are not**. Current
85/92 wiring is not proof that those audible behaviours match. Collection
into a slot and activation FROM that slot are separate cases; validate both
for Aura/status/ordinary skills. **CLAUDE-DEEP-PASSES.md P1** specifies original
reference audio, event routing/timing, real capture→use and ear approval.
Protect regular steps common35. Do not mark capsule audio complete from tests
which only assert the currently selected IDs.

## Live-play correction (2026-09-29) — regular step is id 35

Laurence reiterated that ordinary stepping must use **`audition/27_id035.wav`**.
The audition WAV and `cache/sfx/common/35.wav` have identical SHA256 hashes.
`SFX_FOOTSTEP` is now **35** (wet steps remain a separate selection). Earlier
id-30 program-call observations below are historical research, not the
approved ordinary-step sound. The research label must not overrule his ear.

Code: `crates/pd_formats/src/stx.rs` (music), `crates/pd_formats/src/sound.rs` (sound banks), `crates/pd_convert/src/sounds.rs` (`pd_convert sounds`), `crates/pd_game/src/sfx.rs` (playback). Reference: Esper-RE `stx.h`, `sth2.h`, `esper_sth2.hexpat`; vgmstream `sthd.c`.

Legend: ✅ confirmed · ⚠️ assumed / partly verified · ❓ unknown. **The decoder is confirmed correct by ear** — Laurence has listened to the converted music and sound effects (2026-09-28) and they play as intended, so everything below about the *format* is settled by playback, not just by numbers. Earlier drafts of this line said nobody had listened; that is no longer true. **The gap is not decoding, it is wiring** — see "What is not wired up" below.

## STX music ✅

- 0x800-byte blocks, each starting with a 0x20 header whose magic is `STHD`.
- Block 0 = file header. `u16 @0x10` block count, `u16 @0x1A` loop start block, `u16 @0x1C` loop end block. Channel table starts at 0x20: `u32` sample rate (44099 observed on 04_Highway).
- Blocks 1..n: 0x20 header, then 504 left samples then 504 right samples (s16). Interleave for WAV.
- Verified: 04_Highway → 129 s WAV, lag-1 autocorrelation 0.98 (real audio, not noise). Laurence has not listened to it.
- Loop points: the game honours the loop region since 2026-10-03 (lane infomusic: intro once, then the region repeats; `u16 @0x1A` / `@0x1C` = 65535 means no loop, a jingle that plays once; tracks are cached as `.pdm` by `pd_convert musictracks`). Which track plays when: PIPELINE.md "Music". (Before that, Bevy looped the whole WAV.)

## STH2 sound banks (`Sound/*.bin`) — established 2026-09-26

83 files. 73 are STH2 banks; the 10 `DSP_*.bin` (25,836 bytes each, start with zeros) are not — probably DSP/reverb settings per stage ❓.

| Bank | What |
|---|---|
| `common.bin` (7.7 MB) | all skill / battle sounds: 553 sound ids in 0–634, 201 clips |
| `BattleVo_<name>_<en/ja>.bin` | battle voices, 19 ids each (0, 10–12, 20–22 … 60–62). 7 names × 2 langs are 1-clip **silent placeholders** (Belono, Catoptro, Ceno, Germano, Mechano, euroto, hedono) |
| `MapSE<stage>.bin` | stage ambience (mostly looped) |
| `BossSE_*.bin` | boss sounds (out of scope) |
| `Title_Logo.bin` | title screen |

Esper-RE's exporter writes the whole WAVE block as one 22,050 Hz mono stream; the data says otherwise (per-clip formats below).

### Layout ✅ (parsed on all 73 banks: every tag where expected, 0 errors)

| Where | Field | Evidence |
|---|---|---|
| header `@0x24` | offset of `REAL` (always 0x100) | ✅ all files |
| header `@0x28` | offset of `WAVE` | ✅ all files |
| `REAL @+8` | sound count; then `i32` offsets from +0x10, relative to REAL, **−1 = no sound**. The index is the **sound id** | ✅ skill-table ids land on populated entries (below) |
| `REAT @+8` | track count; `@+0x10` volume 0–127 ⚠️; track offsets from +0x30 relative to the REAT, **−1 = empty slot** | ✅ |
| `TRAT @+8` | event count; `@+0x10` volume 0–127 ⚠️; events from +0x30, 0x100 bytes each | ✅ |
| `WAVE @+4` | size **excluding** the 8-byte tag+size (WAVE + 8 + size = file end on every bank); `@+8` clip count; offsets from +0x10 relative to WAVE | ✅ |
| clip data | raw little-endian **16-bit PCM**, stereo interleaved; clip = offset..next offset (padded to 4 bytes) | ✅ lag-1 autocorrelation median 0.88 mono / 0.99 stereo over 880 clips; stereo split-halves reading gives noise |

### EVNT (0x100 bytes)

| Off | Meaning | Status / evidence |
|---|---|---|
| 0x18 | start delay, ms | ✅ strong: voice layers start at 150–500; echo events step by a constant (30, 40, 80 ms) |
| 0x1C | play length, ms; 0 = clip length / loop forever | ⚠️ 0.99–1.02 × the clip length on 800 of 2,128 unpitched events; shorter values cut the clip; longer only on looped clips; 0 on every looped 6xx sound |
| 0x20 | event kind: 0 = start a clip; 2/3/5/6 = follow-ups with no clip that sweep pan (0x58) and pitch (0x5C) over time (echo tails, the [Rain] whistle's octave rise) | ⚠️ not played |
| 0x24 | wave format tag: 1 = PCM. 0x69 (Xbox ADPCM) appears only on no-clip follow-up events | ✅ every clip-bearing event is 1 |
| 0x28 / 0x2C / 0x30 | sample rate / channels / bits | ✅ WAVEFORMAT fields; rates 8,000–44,100 (mostly 20,000 and 22,050), 35 stereo clips |
| 0x34 / 0x38 | loop start / end, sample frames (−1 = not looped) | ✅ inside the clip on all 299 looped events; the 6xx sounds and stage ambience loop |
| 0x3C | clip size, bytes | ✅ = next clip offset − this one (± 2-byte padding) |
| 0x58 | pan −100..100 | ⚠️ |
| 0x5C | pitch shift, semitones | ⚠️ echo tails step it by 1, [Rain] whistle 0→12 over 1 s (an octave); not checked by ear |
| 0x60 | always 12 (pitch-bend range?) | ❓ |
| 0x68–0xA7 | eight (key, value) pairs; keys 0–5 = speaker levels 0–127 — front L, front R, centre, LFE, rear L, rear R; keys 11, 12 unknown | ⚠️ centre/LFE always 0; two layers of `common` 618 have only a left or only a right level |
| 0xF0 | clip index in WAVE, −1 = none | ✅ |
| 0xF4 | a running number (unique-ish per event) | ❓ |
| others | 0x10 = 232 always; 0x48 0/1/2; 0x64 100–1000 | ❓ |

**Tracks play together** (layers) ⚠️: sword impacts 502–504 = the plain sword clip (501's) + an element layer (lightning 69, fire 58, wind 61); layers carry their own delays. Empty (−1) track slots are common, so a slot index may also mean something to the game code ❓.

### Our rendering (`sound::render`) ⚠️

Per sound id: every event of kind 0 with a PCM clip is resampled to 22,050 Hz stereo with pitch `2^(semitones/12)`, placed at its delay, cut to its length (5 ms fade), scaled by speaker levels × track volume × sound volume (each /127), and summed. Looped events with length 0 also give a seamless loop body (`<id>_loop<k>.wav`) the game repeats. `pd_convert` scales a mix down to 0.95 peak when layers sum past full scale (`trim` in index.json; 334 of 553 `common` sounds needed it — the console mixer's headroom is unknown).

**Checks (2026-09-26):** 1,291 sounds from 45 banks (Japanese voice banks skipped). Every non-placeholder bank has median lag-1 autocorrelation 0.35–1.0 (real audio; the low ones are breathy voices and hissy effects), no clipping after the trim, and only `MapSEUnderground/25` silent. Waveforms looked at (`cache/sfx/waves/`, via `--wave`): `common/521` (bombs) = sharp attack and long decay like an explosion; `common/305` = a sustained charge with a burst at 1.26 s where its three delayed layers start (**not the lasers' sound** — see the `s13` row below, corrected 2026-09-27: 305 is Fire Storm's cast sound); `BattleVo_Hero_en/20` = a voiced, periodic 1.15 s yell.

### Skill table → sound ids (`common` bank)

Field numbers as in FORMATS-SKILLS.md (sN = the N-th i16). Esper-RE's names in brackets.

| Field | Values | Reading | Status / evidence |
|---|---|---|---|
| s21 (CollisionSFX) | 501–527, 178 skills | **impact** | ✅ strong: all 27 values are populated one-shots of 0.5–1.7 s built from a few shared base clips by kind — 98 blades (501–504, 527), 99 fists/feet (505–507), 100 shots (508–515), 95 earth (517–519), 97 bombs (521–522) — plus an element layer; every laser = 509, every bomb = 521. Footage: Psycho Knife's 501 cross-correlates at 3.525 s (score 0.67, the strongest match in the clip) in `Psycho Knife - Not locked-on And Locked-on.mp4`, right on the hit (damage text at 3.567 s); its cast sound 203 at 2.874 s (0.39). For swords the reaction row's own w3 is the same id (rows 6/7 = 501, 62–69 = 502/503/504/527), so a sword hit is one blade sound |
| s20 (StartUpSFX) | 601–617, 141 skills (+ 10035–10039 on enemy skills, not in the bank) | **travel hum** while the projectile flies | ⚠️ every 6xx entry is 2–3 layers that all loop forever (length 0 + loop points); shells/lobs 601, lasers 603, blasters 604 |
| s13 | 301–354, 56 skills | **not a sound — the mid-air action-table row** ✅ | **Corrected 2026-09-27** (PROGRAM-RE.md "Validation check", moves track M1): s13 picks the row of the action table (`gsdata` offset 0x1B00C, FORMATS-SKILLS.md "Action table and hit-reaction table") used when the skill is cast in mid-air — `AIR_ROW`, the sibling of s12's `GROUND_ROW`. The previous reading ("launch/cast sound", value 305 for every laser) was a coincidence of numbering: 305 is a valid row index (the lasers' mid-air cast row, clip `ATK_SKYSHOT_LOW`, fires at tick 20), not a sound id. The `common` bank sound actually numbered 305 is unrelated — it's the cast sound of action row 105 (**Fire Storm**, a 2.5 s rain skill with a late burst, which is what the "10 slots, burst at 1.26 s" waveform read was actually hearing). Esper-RE's "animation id" guess was closer than "sound". A laser's real cast sound is `common/242` (its ground row's cast-sound field, w3), played at cast start, 13 ticks (0.43 s) before it fires |
| s19 (PostHitSFX) | 313, 314, 618 (+ 10040–10052 enemy) | 618 on 17 [Rain]/[Fall] skills = a rising whistle (pitch sweep 0→12); 313/314 like s13 on Ki shots | ⚠️ played at cast |
| s8 (Esper-RE "SoundFileID", marked unconfirmed there) | 1–42, 287 skills | **not a sound** ✅ (2026-09-27, blades lane) | The program never passes s8 to the sound player: the battle code's only sound calls with skill-derived ids are the action row's w3 at cast start (`FUN_14025a3e0`), s19 at the fire tick (`FUN_1402626d0`), s19/s20 when the attack object is made (`FUN_140263a00`), s21 on impact (`FUN_14026cd70` via `FUN_14026ed60`) and the reaction row's w3 with the hit effect (`FUN_140258690` → `FUN_140259310`). The only read of record offset 0xC (= s8) found in the whole decompile is `FUN_1401cd540`, which range-checks it as a 0–43 category in a menu/collection check ❓. We used to play it at every cast: Mist Blade's 27, Excalibur's 28, Muramasa's 12, True Muramasa's 41 played on top of the real cast sound. |
| s18 | 3012, 3015, 4031, 4049, 5001–5017 | not in `common` (enemy-only skills) | ❓ |

Ids 44–300 and 355–500 of `common` are not referenced by the skill table: system sounds (UI, capsules, aura, guard …) ❓ — needs a listen to label.

### Characters → voice banks ✅

`setup/charaset/charaset_en.dat` (layout as Esper-RE `charaset.h`): per character the model name (`pc00a`) at +0 and the voice bank file (`BattleVo_Hero.bin`, no language suffix) at +0x3C, both offsets into the string pool. pc00a Hero (the player), pc01a Edgar, pc02a Freia, pc03a Meister, pc04a Chunky, pc05a Cuff Button, pc06a pH, pc07a Know, pc08a Tsubutaki, pc09a JD, pc10a Sammah, enm00–15a the enemy types, boss03 Demented Freia. `pd_convert sounds` writes this as `voices` in index.json.

**Voice ids**: 0 (4 banks only) and six groups of three (10–12 … 60–62); the three in a group often share clips (variants). Echo tails sit on 62 (Edgar, Freia, JD), 61 (pH), 42 (Anthro, Hero), 32 (Chunky).

**Battle voices** ✅ (program, 2026-09-27, blades lane): a voice is picked from a **voice group** 1–6 through a 10-entry i32 list per group at gsdata 0x33004 + 4·(10·((g−1) mod 6 + 10) + n), n random (`FUN_14025a3e0`, `FUN_140258690`). The lists hold mostly `10·g … 10·g+2` (group 1: 10, 11, 10, 12, 11, 0, 20, 10, 11, 12; group 4: 42, 41, 50, 40, 42, 41, 40, 40, 41, 0; group 5: 50, 52, 51, 60, 40, …). **Who speaks:** the caster at cast start, group = action row w4 (Psycho Knife / Mist Blade / Laser / Empty Hand 1, Excalibur / Muramasa / Tiger Kick / Blaster 3); the victim on a hit, group = the hit-reaction row's w4 (light and medium rows 2, 4, 6, 46, 58–68 even: 4; knock-down rows 3, 5, 7, 59–69 odd: 5). So **10–12 is an attack shout, not a hurt sound**: we used to play it on the victim. `sfx.rs` now plays the caster's `10·w4` group at cast, and on a hit the victim's 40 group, or 50 when the victim is being knocked down (render-side guess from the victim's motion: the sim has no reaction rows). Group 6 (60–62) is still our KO guess ❓. Footage: in `Psycho Knife - Not locked-on And Locked-on.mp4` no voice cross-correlated clearly (best Hero 11 at 7.69 s, 0.38) — not confirmed by ear.

## In the game (`pd_game/src/sfx.rs`)

Render-side, from sim state: new cast → the action row's cast sound (w3) + s19 + the caster's voice group (w4) (s8 and s13 dropped: neither is a sound); projectiles in flight → s20 loop layers (one set per player and sound); `Hit` → s21 impact + victim's hurt voice (40 group, 50 when knocked down); `Blocked` → s21 impact (quieter unless the guard broke); `Burst` → s21 impact; knock-out → loser's KO voice. At most 24 one-shots at once, the same sound once per frame. `PD_SFX_LOG=1` logs every sound started. No sound folder → no sound, no error.

**Known bug (flagged 2026-09-27, not this lane's file to fix — `pd_game/src/sfx.rs`):** playing "s13" as a sound at cast time is wrong per the corrected reading above; it happens to still make a sound because most `s13` values collide with a real (if unrelated) `common` bank entry, not because s13 means what the code assumes. The real cast sound is the skill's action-table row's `cast_sound` (w3, in `cache/skills.json`'s new `"action"` field) — see the moves-track cast-timing step (M3, `combat.rs`/`fx.rs`) in `docs/PLAN.md`.

## Arena objects (2026-09-27)

- **`mdl_hit_iron / stone / soil` are not sounds**: they are aux models in the stage ALR (MD00 slots 40–42), crater decals (FORMATS-STAGES.md "Object effects"). No sound bank has names, and no object or hull field names a material, so there is **no per-material object hit sound in the data** we can point to ❓.
- What plays instead (`sfx.rs` `object_sfx`, render-side from `Combat::props`): a shot that wears or breaks an object plays **its skill's impact sound (s21)** — Esper-RE calls s21 "CollisionSFX", the sound of the shot meeting something — at 0.35 on a hit, full on a break. The shot is found as the one that vanished this frame nearest the object's box (≤ 40 units; `shot_that_hit`, tested). Blasts already play their own impact.
- Objects with an `EF00` **kind 11** record (trigger 2) play that `common` sound after its delay: st06 contaB → 17 (2.8 s) 52 frames after it breaks ⚠️. Jump zones' kind 11 → 79 (a 2.3 s loop, likely their hum) is ambient and not played yet.
- `PD_SFX_LOG=1` logs them (`sfx: object sound 521 at 0.55`). Checked in a demo fight on st06 with Bomb Shots: objects 40 / 62 broke → 521 (the bomb impact) played. Not listened to.

## Voice / tutorial `.w10` ❓

Not examined.

## What is not wired up (2026-09-28; pickup ✅ fixed 2026-09-29)

The bank decodes and plays; the game simply **doesn't fire every id the program does**. Confirmed by reading `sfx.rs`: it preloads and plays only each skill's action-row **cast (w3), travel (s20) and impact (s21)** sounds, the two voice banks, plus stage object effects. So any `common` id the original triggers outside a skill cast is currently silent. Known cases, all read from the program:

| Sound | Program | Why it is silent | Fix |
|---|---|---|---|
| **Capsule pickup — the "collect container orb" Laurence noticed missing** | `FUN_14014fd10` (capsule take/remove) calls `FUN_14024d4d0(…, 0x22)` = **`common/34.wav`**, and separately `FUN_14021b4f0(…, 3)` plays the `cap03_death` pickup effect. `common/34.wav` **exists in the cache and is 21 KB** — decoded correctly, just never triggered. | no capsule cue in `sfx.rs` at all | ✅ **DONE 2026-09-29** — see below |
| Capsule appear (the orb fading in) | `FUN_14014ed90` path, sound `0x55` = `common/85` | same | ⏳ one cue when a slot's state goes 0 → 1 |
| Capsule erased | `FUN_14021b4f0(…, 4)`, sound `0x5C` = `common/92` + the `cap04_braek` effect | same | ⏳ one cue on erase |
| Jump-zone hum | `EF00` kind 11 → `common/79`, a 2.3 s **loop** | noted above as ambient, not played | ⏳ a looping ambient cue, gated per stage |

### ❌ Capsule pickup — the id was wrong, now silent pending an ear label (2026-09-29)

**Laurence heard the wrong sound on pickup, and he was right.** `common/34` is **not a sound at all**. `FUN_14024d4d0` does not play audio: it returns a signed **intensity**, which the caller uses as `intensity + 0x1a4` to index a voice slot. I read a query as a play call and wired an intensity value as a sound id. The tests I wrote passed, because they only checked the id I had wired, never whether it was a sound.

**Proof it is not a play call: none of the 12 functions on the capsule-take path call the sound emitter `FUN_1401cbbc0` at all.** So the real pickup audio goes through an entry point that is still unidentified, and guessing again would just be a different wrong sound. **The pickup is now silent on purpose**, with the `CombatEvent::Capsule` hook and its sim-side test still in place, ready for the real id. A silent pickup is the honest state; a confidently wrong sound is not.

### 🟡 Footstep sound — id 30 confirmed, but Laurence hears TWO steps in-game (2026-09-29)

**What is solid.** `FUN_1401cbbc0`'s 4th argument is **packed**: `id = arg >> 20 & 0xfff`. Ghidra prints the whole constant, so the decompiled source shows a nonsense-looking `0x1e0c350` — that is how the pickup went wrong. The footstep dispatch passes `0x1e0c350` → **id 30**, byte-verified at `0x140230ece` (`mov r9d, 0x1e0c350`). `common/30.wav`, 0.78 s. Live check: `sfx: play sfx/common/30.wav at 0.38` on the same tick as each `[step]`.

**What is not: "one sound for every surface" is WRONG** (I said this earlier; Laurence's ears corrected it). There are **two** footstep dispatchers, differing only in which particle class they build:

| Dispatcher | Classes | Sound arg | What Laurence hears |
|---|---|---|---|
| `FUN_140230d60` | `PlySandType0`, `PlyWaterType0`, `PlyDustType0` | `0x1e0c350` | hard heel on concrete (the ~80% case) |
| `FUN_140230cd0`, `FUN_140231490` | `PlySandType1`, `PlyWaterType1` (dust has **no** Type1) | `0x1e0c350` | wet / puddle step (the ~20% case) |

The selector is **a downward raycast, not a random roll** — `FUN_140256390` walks the collision faces under the player, keeps the nearest, and returns its class byte (gated on `0x14`/`0x1f`). `FUN_140230790` calls the Type1 dispatcher only when that probe succeeds, else the Type0 one. **No `rand()` and no 1-in-N test exists anywhere in the chain**, so the 80/20 split is a property of Lost Palace's collision geometry, not a randomised variation. **A face the probe misses produces no footstep sound at all** (`if (id == 0) goto SKIP`).

**The attr data cannot produce this split — measured, not assumed.** Parsing every hull face in all 30 stage `.dat` files with `pd_formats` gives only **four** distinct `attr` values in the whole game:

| attr | faces | share |
|---|---|---|
| `0x77FC` | 30,716 | 78% |
| `0x0034` | 8,481 | 27% |
| `0x0000` | 3,381 | 11% |
| `0x000B` | 2,249 | 7% |

Neither runtime class gate (`0x14`, `0x1f`) appears **in any stage**, and of the nine material ids the dispatcher switches on, only `0x0B` (11) exists. So the `attr` we parse into `HullFace.attr` is **not** the field the footstep code reads, and per-surface footsteps are unreachable from converted data. (`0x000B` is the one live lead — 7% of faces, on st02/st03/st08 — but it cannot be a 20% split.)

**Both dispatchers pass the SAME sound argument.** Reading each class's per-class virtual (vtable slot [3]) shows they are **particle tick** functions (advance age, spin, gravity), not sound selection. Three emitter calls exist in the whole footstep region and two carry the identical `0x1e0c350`. So the audible Type0/Type1 difference is **not a different emitter id** — it is downstream of the particle class, and the second clip has not been identified.

**Correction to my own "variant family" claim.** I read the low 20 bits (`50000`, `20001`, `100001`, …) as a variant selector. The 498-line list walker `FUN_1401aefe0` never unpacks the packed value — it only compares the whole dword against literals (one hand-picked special case, `0x1a15f90` = id 26). There is no `>>20`, no `&0xfff`, no `/10000` in the mix path, so the low bits are more likely a seed/priority/identity than a variant index. **Unresolved.**

**Decision (Laurence, 2026-09-29): ship a 20% chance of the wet step**, since the real trigger is collision-class data we do not have and cannot convert. It fakes the mechanism and matches the result on Lost Palace. ⚠️ Blocked on identifying the second clip — a narrow audition is the next step.

### The audition list (2026-09-29)

Scanning all **134** call sites of `FUN_1401cbbc0` and decoding each packed argument gives the **22 distinct ids** the original's emitter plays: **1, 2, 3, 6, 8, 9, 11, 13, 14, 16, 17, 20, 21, 26, 27, 28, 29, 30, 31, 32, 33** (also `sfx.rs::SOUND_EMITTER_IDS`).

**By ear (Laurence), the list is overwhelmingly breakage**, not capsules: rubble/glass/wood breaking and wooden knocks across ids 6–21 and 26–30, plus `26`/`27` = landing, `28` = stone drag, `29` = metallic room ambience, `32`/`33` = steps, `85` = shield, `92` = a move effect, `1` = menu-like (unidentified), `3` = menu traversal. **No orb-collect, aura-activate or green-capsule sound was in the set** — consistent with the earlier finding that no capsule-path function calls the emitter at all. **The capsule sounds are in another bank, not `common`.**

⚠️ **Audition defect, my fault:** `cache/sfx/common` contains **byte-identical duplicate pairs** (28/29, 30/31, 32/33, 24/25, 34/35), so 5 of the 31 clips repeated without warning. Any future audition must de-duplicate first.

### Audition pass 2 — Laurence's corrections (2026-09-29, evening)

Laurence relistened to `engine/audition/` (gitignored; filenames are `<index>_id<id>.wav`) and gave two firm labels plus one mechanism fact:

- **id 35 = Regular stepping** (file `27_id035`; his pass-1 label "landing 2 / stepping down" is corrected). Note the tension this creates: the program-proven footstep is **id 30**, and ids 34/35 are byte-identical in our cache, so either the duplicate is legitimate clip sharing (two REAL entries pointing at one WAVE clip — check the clip indices before assuming corruption) or the decoder duplicated wrongly and the real 35 is a different clip.
- **id 1 = exiting the pause menu (unpausing)** — heard **only when player 2 unpauses** (whether P1's unpause uses a different id, or he simply noticed P2's, is open). This labels the pass-1 "menu-like (unidentified)" entry. Candidate for wiring into our own pause menu.
- **The wet step is triggered by wet patches on the map** — Laurence confirms it is positional, not random. This corroborates the raycast selector (`FUN_140256390`) reading *something* per-face, and reopens the "unreachable from converted data" verdict: the patches are real, mappable locations, so their definition exists somewhere in the stage data we have not looked in (candidates: the placed-object records in `Map/stNN.dat`, a face field other than the `attr` we parse, or the collision hulls of placed objects rather than the stage hull). **The wet clip itself was not in the audition folder** (Laurence checked; may have missed it).

**New lead on the wet clip's id.** The earlier claim "both dispatchers pass the SAME sound argument" was measured on **two of the three** emitter calls in the footstep region ("Three emitter calls exist in the whole footstep region and two carry the identical `0x1e0c350`" — above). **The third call's constant was never decoded.** With the `>>20 & 0xfff` packing, id 35 would arrive as `0x23xxxxx` — if the third call (on the Type1/wet path, `FUN_140230cd0`/`FUN_140231490`) decodes to 35, then `common/35.wav` *is* the wet step and Laurence has already labelled it (a wet slap heard dry, out of context, plausibly reads as "regular stepping"). One Ghidra-export read settles it (`%USERPROFILE%\.pdtools\re-out\`, PROGRAM-RE.md "Method"). **Do not ship the 20 % random fake until this is checked** — the real mechanism may be one constant away.

**SUPERSEDED — the hypothesis above is dead.** lane-step read all three calls (all `0x1e0c350` → id 30) and found the real mechanism (`FUN_140276330` + the `DAT_1402f6270` material table); see "Third emitter constant" below. **Laurence's ear results (2026-09-29, on `engine/audition/wetcheck/`):**
- **All four wet candidates (52–55) sound correct.** DECISIONS.md records the result: wet ground plays the water-class pair **52/54**, cycled per wet step.
- **id 85 = the orb appearing — CONFIRMED by ear.** That also settles the swapped attribution: `FUN_14014ed10` (appear/tick) plays 85, not erase.
- **id 92 = the capsule pickup** (the take path, `FUN_14014fd10`/`FUN_14014ed90`) — corroborated by the 85 confirmation but **not heard back from Laurence yet**.

### The capsule pickup hook (built, sound retired)

The event chain is in place and tested; only the sound id is missing.

- **`combat.rs`** logs `CombatEvent::Capsule { who, slot, skill }` from the exact arm that captures a capsule into a button (the `Some(k)` arm of the button press — standing on your own orb captures it instead of casting). No new sim state, no behaviour change: the capture already happened, this only announces it. Tested by `capturing_a_capsule_logs_the_event_the_pickup_sound_plays_on` and `capturing_someone_elses_capsule_is_not_a_pickup`.
- **`sfx.rs`** maps it to **no cue at all** now (`CombatEvent::Capsule { .. } => {}`), asserted by `capturing_a_capsule_is_silent_until_the_real_id_is_known`. When the id is identified by ear, that test is where it gets wired — and it should then assert the cue.
- **`fx.rs`** has an explicit no-op arm: a capture spawns no effect (the orb is already on the field and simply goes away), so there is nothing to draw. The compiler caught this — a new `CombatEvent` variant makes every `match` on it non-exhaustive.

⚠️ **Why the sound was never heard in testing, which is worth knowing:** the `--demo` AI in a scripted run presses buttons but **never walks onto a capsule**, so the capture path does not execute — `PD_FX_LOG=1` over 1,200 frames showed 73 `Denied` events and **zero** captures. The sound is real and the tests prove the event fires; it just needs a human to walk over an orb. ❓ The other three ids (85 appear, 92 erase, 79 jump-zone hum) are still open — same wiring shape, but their trigger points (a slot's state change, an erase, a per-stage ambient) are not yet identified in our sim.

The remaining three are one small lane: **capsule audio**. The ids above are `common` bank numbers, so they resolve through the existing `sfx:common/<id>.wav` path — no new format work, no new assets, and the wavs are already in the cache.

✅ **All four verified present in `cache/sfx/index.json` and on disk** (checked 2026-09-28): `common` has 553 entries; `34.wav` 21 KB (loops 0), `85.wav` 105 KB (loops 0), `92.wav` 75 KB (loops 0), `79.wav` 200 KB (**loops 2** — two loop points, so it really is a looping ambient bed, not a one-shot). So this is purely a "never triggered" gap: the audio exists, decodes, and would play correctly the moment a cue is added. Nothing here needs the converter or a new format.

### Third emitter constant + duplicate-pair verdict (lane-step, 2026-09-29)

**Verdict first: the `0x23xxxxx` hypothesis is dead.** All three footstep-region emitter calls pass the identical constant `0x1e0c350` → **id 30**. There is no wet-path id 35 on the emitter, and **id 35 is not the wet step** (see below). The real per-surface variation goes through a *different* play call, `FUN_140276330`, with plain (unpacked) ids from a per-material table. The duplicate cache pairs are legitimate clip sharing — the converter is exonerated.

#### A. Every footstep-region emitter call, decoded

`decomp_all.c` line : function : source (4th arg `= param_4`, packed; `id = arg >> 20 & 0xfff`):

| Decomp line | Function (role) | Call | Decoded id |
|---|---|---|---|
| 452282 | `FUN_140230cd0` (Type1/wet A) | `FUN_1401cbbc0(ppuVar2,*(undefined8 *)(param_1 + 0x50),_Dst,0x1e0c350,2,0)` | 0x1e = **30** |
| 452394 | `FUN_140230d60` (Type0/dry) | `FUN_1401cbbc0(ppuVar3,*(undefined8 *)(param_1 + 0x50),_Dst,0x1e0c350,2,0)` | **30** |
| 452707 | `FUN_140231490` (Type1/wet B) | `FUN_1401cbbc0(ppuVar2,*(undefined8 *)(param_1 + 0x50),_Dst,0x1e0c350,2,0)` | **30** |

Byte evidence (PDUWP.exe, read-only): the whole exe contains exactly **16** occurrences of the `0x1e0c350` immediate; two sit in the footstep region and both are the same instruction, `41 B9 50 0C 1E 00` (`mov r9d, 0x1e0c350` — r9 = 4th arg) with identical surrounding bytes: file `0x2302bb` → VA **`0x140230ebb`** (inside `FUN_140230d60`) and file `0x230a0c` → VA **`0x14023160c`** (shared tail). So at machine level there are only **two** physical call sites, not three: `FUN_140230cd0` has no `0x1e0c350` in its own range (`0x2300d0–0x230160`) and reaches the second site through its `switch` (one indirect-jump `FF` byte) into the shared tail at `LAB_1402315c5` (VA `0x1402315c5`, bytes `8b862801…` match the decomp's `_Dst` setup). Decomp "three calls" = two machine sites, one shared.

**Directly reachable, checked, no emitter:** `FUN_140230d60` calls `FUN_140231010` / `FUN_1402311e0` (no `FUN_1401cbbc0` in either — they call `FUN_140276330`, next paragraph); `FUN_140230cd0` tail-calls `FUN_140230d60`; `FUN_140230b60` calls `FUN_140230cd0`; `FUN_140231660` has no emitter call; `FUN_140256390` (probe) has none. `FUN_140231490` has **no callers anywhere in the export** — its head looks dead, but its tail (`LAB_1402315c5` onward) is live via `FUN_140230cd0`'s switch ❓ (indirect call through a pointer remains possible but was not found).

#### B. The actual wet-step mechanism: `FUN_140276330` + the material table

`FUN_140276330(id, id, 0x1000, 0x1000, &pos)` is the plain-id sound-play call (106 caller functions): capsule code uses it with `0x55` = 85 (`FUN_14014ed10`) and `0x5c` = 92 (`FUN_14014ed90`, `FUN_14014fd10`) — the exact ids our docs already tied to capsule appear/erase/take, which corroborates that this (not the emitter) is the general SFX path. The footstep dispatchers reach it via `FUN_140231010` / `FUN_1402311e0` with an id read from the i32 table at **`DAT_1402f6270`** (exe VA `0x1402f6270`, read with `re-out/pe.py`): index = class byte × 8 + {0, 4}:

| class | pair | class | pair | class | pair |
|---|---|---|---|---|---|
| 2, 3, 5 | 28 / 30 | 4, 6 | 24 / 26 | 7, 8, 9 | 32 / 34 |
| 10 (0x0a) | 44 / 46 | 11 (0x0b) | 56 / 58 | 12 (0x0c) | 36 / 38 |
| **13 (0x0d, water)** | **52 / 54** | 21 (0x15)–30 (0x1e) | 101/103 … 137/139 | 32 (0x20) | 48 / 50 |
| 0, 1, 14–20, 31 | −1 (silent) | | | | |

And the odd ids come from `FUN_1402311e0` line 452657: `iVar4 = iVar4 + 1` (a speed/threshold branch) before the play call — so **33 = 32+1, 35 = 34+1** ⚠️ (one branch condition, exact trigger unconfirmed). That answers the audition tension: **id 35 is the +1 variant of the 34 footstep** (they share WAVE clip 135 — Task B), which is why Laurence heard it as "regular stepping". The wet patch on Lost Palace is class-gated (Type1 dispatcher handles classes 3/9/0x0c sand, 0x0d water), so the wet clip is the water-class id **52 (or 53/54/55)** ❓ — ear-check `common/52.wav`..`common/55.wav` against Laurence's wet-step memory before wiring anything.

Call graph as measured: `FUN_140231a40` → `FUN_140230790` (selector) → probe `FUN_140256390` → hit: `FUN_140230cd0` (Type1 particle + emitter 30); `FUN_140230d60` (Type0: particle + emitter 30 + material sound via `FUN_140231010`/`FUN_1402311e0`) runs when `*(param_1+100) != 0`. Which mode flag that is, and which of the pair ({0} vs {4} column — walk/run? left/right?) plays when, is **unconfirmed** ❓.

**Consequence for the shipped decision: do NOT ship the 20 % random wet-step fake.** The mechanism is identified (material class → table id through `FUN_140276330`, emitter always 30 alongside); faking it with randomness would play the wrong clip (35) at the wrong times. The remaining work is sfx-side, not format-side — **request for the sfx lane**: play the material-table id (class byte is sim-visible? if not, keep single-surface 30 + table id 30) instead of a random 35.

#### C. Duplicate-pair verdict: shared clips, converter exonerated

Probe: `engine/re/common_bank_probe.py` (stdlib only, reads the read-only bank, prints REAL offsets / tracks / clip indices / byte ranges / sha1 per clip). `common.bin` = 7,712,232 bytes, 635 sounds, 201 clips. Every pair shares **one** WAVE clip (same index, same bytes — not a converter duplication):

| Pair | Clip(s) | Bytes / sha1 | Notes |
|---|---|---|---|
| 24 / 25 | **174** both | 18,432 B `be08f7cb7955`, 20 kHz mono, 550 ms | identical events |
| 28 / 29 | **177** both | 10,240 B `b7d883478ba3`, 697 ms | identical events |
| 30 / 31 | **178 + 179** both | 15,616 B `8c87efebff08` + 14,976 B `2af356efe939` | same two clips, **track order swapped** (30: 178,179; 31: 179,178) — tracks sum, so the mix is identical |
| 32 / 33 | **136** both | 10,752 B `24d8f8b7467e`, 510 ms | identical events |
| 34 / 35 | **135** both | 9,728 B `2d98604df90f`, 510 ms | identical events |

So `cache/sfx/common/{24,25,28…35}.wav` being byte-identical is **correct output**: two REAL entries legitimately point at one WAVE clip (and 30/31 mix the same two layers). No reconvert needed; future auditions must de-duplicate adjacent ids first. The probe also prints every footstep-table id's clips (24→174, 26→175+176, 30→178+179, 34→135, 52→142, 54→141, …) for the ear-check in (B).

#### D. Capsule-lane lead (not this lane's file to act on)

The "capsule path never plays audio" verdict needs revisiting: `FUN_14014fd10` (take) returns `FUN_140276330(0,0x5c,…)` = **92**, `FUN_14014ed90` plays `0x5c` = **92**, `FUN_14014ed10` plays `0x55` = **85**. The earlier "no emitter on the capsule path" finding stands, but the path plays through `FUN_140276330`, not the emitter — so the pickup sound may already be identified (92 on take?) pending an ear-check, and the doc's ed10/ed90 attributions (above, "What is not wired up") look swapped. **Request for the capsule lane**: reconcile against these three call sites.
