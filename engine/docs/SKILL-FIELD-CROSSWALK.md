# Skill record: community field names vs ours

Written 2026-09-30 (cloud session, no game data available, so nothing below was re-checked against the file; status columns compare *documents*, not bytes).

**Sources:** the community Skill Editor's label file (`res/labels.yaml` in github.com/Torphedo/SkillEditor, GPL-3.0; we record offsets and short names only, meanings in our own words) and our own `FORMATS-SKILLS.md` / `PROGRAM-RE.md`.

**Index conversion:** our `sN` is the N-th `i16` of our record (record offset `4 + 2N`). The community offset is the program's own record offset, which is 8 bytes earlier in our framing (`FORMATS-SKILLS.md`, "Hidden fields s70-s73"). So **`sN` = community offset `2N - 4`**, i.e. community `0x14` = our s12, `0x22` = s19, `0x52` = s43. The conversion lines up with every field we had already confirmed (s12 ground action row, s13 air row, s19-s21 sounds, s22-s26 costs, s38/s39 requirement, s57-s60 hit reactions, s62 can-hit, s63 projectile behaviour), which is good evidence it is right.

## Fields outside the selector (same meaning for every skill)

| ours | community offset | community name | our current doc | note |
|---|---|---|---|---|
| s2 | 0x00 | skill text id | `TEXT_ID` | agree |
| s3 | 0x02 | displayed skill id | `SKILL_ID` | agree |
| s4 | 0x04 | register id ("10 = usable") | "10 on all but 10 records" | agree; explains the 10 |
| s5 | 0x06 | optional skill id (DLC don't use it) | `CATALOG_NO` | ours is a guess; check what the DLC records hold |
| s6 | 0x08 | rarity | `RARITY` | agree |
| s7 | 0x0A | unknown ("additional 1") | unnamed | both unknown |
| **s8** | 0x0C | "old sound file id", unknown | not documented | **look at its values** (see audit script) |
| s9 | 0x0E | capsule type | `CAPSULE` | agree (1 attack .. 6 environment; 0 = aura) |
| **s10** | 0x10 | skill interaction category (groups skills that other skills key on, e.g. bundled variants) | not documented | new lead for Bundled Laser / Bundled Blaster / Crystallization style skills |
| s11 | 0x12 | school | `SCHOOL` | agree (0 is "aura" in the community tool, "enemy-only" in ours) |
| s12 / s13 | 0x14 / 0x16 | animation profile ground / air | action rows `GROUND_ROW` / `AIR_ROW` | agree; the community says a profile holds animation, visual effects, hitbox frames, activation, lunge, cancel frames, grunts, vibration. Ours decodes effect (w2), cast sound (w3), voice group (w4), rumble, anim, start frame, dash, fire tick. |
| s14-s17 | 0x18-0x1E | combo 1-4 animation profiles | "combo action rows" | agree |
| s18 | 0x20 | boss skill specific (0 for normal skills, 3012, 3015, 4031, 5001 ... on bosses) | not documented | enemy-only; ignore for versus |
| **s19** | 0x22 | **on projectile shot SFX** | "PostHitSFX", ⚠️ "played at cast" | **rename**: this is the shot/launch sound |
| s20 | 0x24 | projectile flight SFX (looping, ids 601+) | `StartUpSFX`, "travel hum" | agree in behaviour; our name is wrong, rename to flight |
| s21 | 0x26 | collision SFX | "CollisionSFX", impact | agree |
| s22-s26 | 0x28-0x30 | aura cost, level decrease on use, cost change per use, health penalty, number of uses | agree | agree |
| s27-s35 | 0x32-0x42 | "allow multi skill" (skill to chain into) then unnamed extras | s27-s36 chain list | agree (ours: program record indices) |
| **s36** | 0x44 | **self effect** (effect applied to the user; "needs research") | folded into the s27-s36 chain list | **conflict**: we treat s36 as the last chain entry; the community has a separate self-effect meaning. Check which skills have s36 ≠ 0 |
| **s37** | 0x46 | extra button effects: 0 none, 1 can't use, 2 can be held/charged, 3 clickable with error sound | not documented | may explain hold-to-charge skills (we use action w39 charge cycle) |
| s38 / s39 | 0x48 / 0x4A | requirement type / amount | `REQUIREMENT` | agree |
| **s40** | 0x4C | **ground/air**: 0 ground only, 1 air only, 2 both, 3 both but uses the air animation on both | "maybe mid-air" (2 on 56 skills) | **resolved**: 0 ground only, 1 air only, 2 both, 3 both using the air animation; check our use of s13 |
| s41 | 0x4E | skill button effect: 0 normal, 1 retainable, 2 lock skill | `PROTECTED` (2 = can't be erased, 1 = retain) | agree |
| s42 | 0x50 | "unknown 2" (DLC all 0) | `FAMILY` | community does not know it; our FAMILY reading is ahead |
| s43 | 0x52 | **skill set selector**: decides how s44-s56 are read | "target kind" | same field; see the table below |

## The selector (s43) and the fields it switches (s44-s56)

| s43 | community set | our reading |
|---|---|---|
| 0 | attack (also erase/special) | attacks |
| 1 | erase (capsule-targeted) | "some erase skills" |
| 2 | defence | every defence skill |
| 3 | special | capsule-targeted specials |
| 4 | status | self-targeted status/special |
| 5 | environment | every environment skill |
| 6 | copy / recollection | Recollection / Copy |

Mostly the same. One difference to check: the community puts status at 4 and special at 3, ours lumps "4 self-targeted status/special".

Field-by-field, attack set (s43 = 0): s44 range, s45 damage calculation (31 formulas, e.g. judge of anubis, compressor, scaling by aura or level), s46 damage or factor, s47 damage increase per use, s48 target hand filter, s49 target hand priority, s50 target hand skill amount, s51 hit effect (about 52 kinds: aura/level changes, explode, paralysis, freeze, poison, steal, stat buffs, status effects ...), s52 and s53 duration/potency, s54 shield penetration, **s55 collision behaviour**, s56 unknown. The defence set (s43 = 2): s44 defence type, s46 shield active time, s47 shield strength, s49 anti-pierce, s50 erase all or melee only, s51 successful shield effect, s52 effect duration. Our `FORMATS-SKILLS.md` s44-s52 rows agree where checked.

For the erase, defence, special, environment and copy sets (s43 = 1, 2, 3, 5, 6): **s54 = "projectile properties"** and **s55 = "projectile ID"** (the attack set uses the same slots for shield penetration and collision behaviour; the status set uses them for status timers). The community's generic fallback layout also lists **s56 = "collision skill ID"**, though every set then marks s56 unused or unknown. This is a lead: non-attack skills can spawn a projectile, and a skill may name a "collision skill" that fires on contact. Not yet checked against our data.

## Fields after the selector block

| ours | community offset | community name | our doc |
|---|---|---|---|
| s57-s59 | 0x6E-0x72 | on-hit animation by damage (0-2, 3-4, 5+) | reaction rows light/medium/heavy: agree |
| s60 | 0x74 | on-hit animation for erase/status/effects | "after an Add effect": agree |
| s61 | 0x76 | unknown | "hit in the air" (0 everywhere) |
| s62 | 0x78 | can-hit requirement | agree |
| s63 | 0x7A | projectile behaviour (21 kinds) | `MOTION`: agree |
| s64-s73 | 0x7C-0x8E | ten behaviour parameters (hitbox size/duration, start speed, acceleration, end speed or rain count, homing or accuracy, **animation size and height**, extras) | s64-s69 `MOTION_PARAMS`, s70-s73 hidden | agree where checked. "Animation size / height / width" names three parameters that scale the visual, worth testing against our effect scale guesses |

## To do when the game data is at hand

Run `python engine/re/skill_field_audit.py engine/cache` and look at: s8, s10, s36, s37, s40, s54-s56, s61 values by capsule type; whether any skill-sound id (s8, s19-s21, action w3, reaction sound) points at an id with no decoded clip.

## Second source: the pdskedit notes (saved by Laurence from dustguide.neocities.org, 2026-09-30)

A guide page of someone's test notes on phlsphr420's older skill editor ("pdskedit"; the page author says they did not write the tool and lack its source). Facts only, in our words; reliability is "hands-on testing notes", below the program read.

**Index rule (checked):** its "Unknown N" is **our s(N+1)**. Checks that land exactly: its "Unknown 26" note (only High-speed Punch, Rapid Cannon, Trench Mortar have values) = our s27 chain list (same three skills, `FORMATS-SKILLS.md`); "Unknown 40" (1 retain, 2 locked) = our s41; "Unknown 42 = 5 makes an environmental crystal" = our s43 = 5; "Unknown 61" (Gravity Press 7, Backdraft Bullet 6) = our s62 can-hit; "Unknown 60, all 0" = our s61; "Unknown 56 / 57" (hit-reaction type, 45 = twister) = our s57 / s58 reaction rows; "Unknown 20 = collision FX" = our s21.

**New or confirming facts**

| ours | what the notes say | status against ours |
|---|---|---|
| s42 | "tells the **AI** what type of skill this is": 5 walk-through shots (Scream of Evil), 6 Charged Particle, 10 melee, 11 Photon Wave/Burst, 12 mid-range, 13 long straight shot, 14 Sonic Boom / Arc of Fire, 15 Laser, 16 Parabola, 17 Throwing Stone, 18 crawlers/twister/whirlwind, 19 Psycho Kinesis, 20 Arrow of Artemis / fall, 21 Rain, 22 Gravity Press / Jackpot, 23 Restrain / Exhaust, 24 Tiger Slayer, 25 Dragon Slayer / Lost Dragon, 26 Wind Pressure, 27 Giant Slayer / Lack / Disremember, 28 Hungry Demon / Trigger, 29 Disrupt, 31 Neutralization / Armaros, 32 Fusion, 33 Angel's Breath, 40 Decoy teleport, 49 friendly targeting, 50 Plant Landmine, 51 boost mine, 60 Diabolical Trick, 61 Eliminate, 62 Shatter, 72 Refresh / Angel Tears, 73 Disintegrate, 74 Evaporate / Violent Change / Particle Convert, 75 Massive Quake, 77 Return to Nature, 78 Starting point crush, 80-86 barrier, reflect, shelter, absorb, brush, change course, about face, 89 Auto Shell, 90 plant environment, 99 Orb / Amulet, **70 = AI uses it whenever possible, 71 = seldom, 100 = AI never uses it** | Matches our `FAMILY` values where we had them. **It is an AI hint**, so it is probably irrelevant to human play: anything in our code that drives behaviour or animation from `s42` should come from the action row / motion fields instead. Switching lasers from 15 to 21 made the AI spam them (AI-only effect). |
| s38 / s39 | Requirement codes: 1 health at most n, 2 health at least n, **3 aura at most n (unused)**, **4 aura at least n**, 5 "7 or fewer capsules" (Bravery skills), **6 capsules at least n**, 7 maximum aura, 8 level at most n, 9 level at least n | Adds 3, 4 and 6 (unused or unseen in ours); our "5 = skills left in arsenal <= n" is the same rule seen from the other side (capsule count). |
| s40 | 0 cannot use in the air, 2 can | Consistent with the Skill Editor labels. |
| s44 (environment set) | The environment "type" list, which looks like the **program's own status-slot ids**: 0 strength buff/debuff (Holy Ray, Lunar Force, Unison), 1 Reduce Entropy / Abnormal Gravity, 2 Dense Thought, 3 Desertification, **4-6 paralysis, ice, poison**, **8 everyone invisible**, **11 infinite Skywalk**, 13-16 frostbite on the assigned / adjacent / alternate adjacent / opposite button, 17 Backdraft, 19 Nature's Blessing, 20 Berserk, 29 Fatigue, 31 Necronomicon, 32 Memory (Lapse), **33 everyone invulnerable**, 34 Accelerate, **35 Confuse**, 36 Dazzle (said not to work), 38 no buttons except pick-up and aura, 39 Chaos, 41 Providence, 42 Vanishing Light, 43 Stagnant Air, 44 Spiritual World, 45 Scientism | **Five of these agree with the program read** in `PROGRAM-RE.md`: status 4 = can't move (paralysis), 8 = invisible to lock-on, 11 = floating, 33 = no damage, 35 = confusion (rotates the stick). That makes the rest a plausible map of the program's status ids: 17 Backdraft, 19 Nature's Blessing, 20 Berserk, 29 Fatigue, 31 Necronomicon, 32 Memory Lapse, 34 Accelerate. Unnamed in the notes: 7, 9, 10, 12, 26-28, 37, 40. This resolves most of the "unmapped status ids" in `audits/AUDIT-DEFENCE-STATUS-CAPSULES.md` (11, 12 still open; 16 = frostbite variant; 20 Berserk; 22 "Ath"; 23 unknown; 31 Necronomicon; 35 Confuse; 37 unknown). Different from the `s51` hit-effect codes (in which 19 Backdraft, 20 Soul Cage, 21 Nature's Blessing, 22 Berserk, 33 Necronomicon, 35 Void, 37 Confusion): two numbering systems, the hit-effect code maps onto the status id. |
| s45 (defence set) | Shelter-type guards erase at Mod Type 3; a value of 1 in the next field restricts an erasing shield to melee; Flash Hole erases only projectiles because the Absorb type does not stop other attacks, and switching it to Shelter makes it erase any attack | Explains the per-kind ignore lists in `audits/AUDIT-DEFENCE-STATUS-CAPSULES.md`: the rule is by guard *kind*, not a per-skill flag. |
| s57-s58 | 28 or more hit-reaction kinds (lift, pin down, stagger, ...); 45 = twister | Agrees with the reaction-table rows (PROGRAM-RE.md). |
| s64 | "Projectile life / duration": -40 for E.L. bomb, -30 for Blaster and Vibration Blaster (negative); includes the trailing hitbox of Flame Sword | New detail: **negative values exist** in this field for shots. Check our handling. |
| s65 | start speed / "skill width": E.L. bomb 20, Blaster 140; 1 barely moves a sword; 2,800 is a valid long reach; negative values send you backwards with melee at close range | Same field our motion table reads; the negative-melee remark is new. |
| s66 | acceleration / animation size; 380 on Flame Sword made the swing unwalkable-around | |
| s67 | end speed / **explosion size of Land Mines** (can be huge); Rico at 100 is fast | Mine blast radius coming from this field is new (ours: s52 for Bomb, mines separate). |
| s68 | "the true homing value": Dazzle 100, Crawlers 750, Rain and Hole higher than Dazzle but less accurate | Agrees with "homing" reading; values per family are a check for our scale. |
| s69 | animation height: 100 crawlers, 1000 rain | |
| s72 / s73 | s72: initial angle of ricochet lasers; s73: number of bounces pursued (Impact Burst, Ricochet Laser) | Matches the hidden fields s70-s73 being used. |
| s20 | "effect of getting knocked on the ground" (their "Unknown 19") | **Conflicts** with the Skill Editor labels and our reading (projectile flight sound, looping 601+). The Skill Editor is newer; treat this note as unreliable for s20. |
| all | Skills numbered 301+ do not follow the same rules; some do not edit cleanly (Impact Burst) | Matches our "records 301-340 are the extra skills" caveat. |
