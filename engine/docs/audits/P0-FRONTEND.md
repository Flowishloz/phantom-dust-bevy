# P0 audit — FRONTEND (P7 arsenal builder + P8 gamemodes), 2026-09-30

Read-only audit. Nothing was built or run except one copy of `pd_convert` (in the
session scratchpad) to look at `Labo.alr`. Sources read: `frontend/*.rs`,
`arsenal.rs`, `combat.rs` (`Rules`, `start_round`, end/draw), `main.rs`
(`enter_match`, `check_match_end`), docs (FRONTEND, MENUS, DESIGN-BRAWL/2V2,
DECISIONS, PROGRAM-RE), the original's menu scripts (`Menu/arsenaledit_en.ssb`,
`vsmenu_en.ssb`, `livearsel/liverulesel/battleend/pausemenu`, `mb0*`), the skill
file `gstorage/gsdata_en.dat`, and the decompile (`decomp_all.c`).

**Headline:** the original's builder and its "game rules" are much more specific
than our docs assumed, and most of it is recoverable from data. Found this pass:
a 12-entry **rule table** (health / Level range / time limit per rule) inside
`gsdata`, the **rule-legality bit tests** (`FUN_14015b830`), the **aura-speed
multiplier by arsenal kind** (`gsdata+0x33034`), and **8 preset "Quick" arsenals**.
Our builder cannot even add an Aura Particle, and our aura-speed and time-limit
numbers are not the original's.

Legend: ✅ yes / ❌ no / ~ partial. "Reproduced" = our runtime seen doing it (I
only re-read existing screenshots, e.g. `engine/shots/ui_arsenal.png`; I did not run the game).

---

## 1. Ledger

### 1a. Arsenal builder (P7)

| Feature | Mapped | Implemented | Reproduced | Verified | Laurence-approved | Evidence |
|---|---|---|---|---|---|---|
| Arsenal = fixed 30 slots; empty slot (-1) = Aura Particle | ✅ | ✅ `arsenal.rs` `ARSENAL_SIZE`, `discovery.rs` (-1→0) | ~ | ~ | ❌ | `FUN_14014f830` (histogram: slot 0 = 30 − valid skills), `FUN_14015a800` compacts skills to front, pads -1 |
| Max 3 copies of a skill (Aura exempt) | ✅ | ✅ `arsenals::add_skill`, `arsenal::check` | ~ | ✅ unit tests | ❌ | script text "cannot place more than 3 of the same skill" + debug toggle "Disable the 3 same-skill limit?" (`arsenaledit_en.ssb`); the check itself is script-side, not read |
| **Aura Particle can be added in the builder** | ✅ (category "AURA", type list) | ❌ catalog = `registered` skills only (`main.rs:765`), no Aura entry; no key adds one | ✅ seen: only real skills listed | — | ❌ | `builder.rs` `filtered()`, `main.rs:765-807`; script "Replace with Aura" (`arsenaledit_en.ssb`) |
| School limit: per-arsenal declared "Number of Schools" 1–5, chosen on creation | ~ (kind stored at deck+0x50 by `FUN_14015afe0`; "SYSMAX … limited to %d schools"; AAKIND dialog 1–5) | ❌ global "≤3 schools" from fan guide | ✅ | ❌ | ❌ | `FUN_14015afe0` writes +0x50 = arg; `arsenaledit_en.ssb` AAKIND/SYSMAX; the limit comparison is script-side, not read |
| Aura-speed by arsenal kind | ✅ ×1.3 / ×1.0 / ×0.7 for kind 1/2/3; any other value = ×1.0 | ~ `arsenal::aura_rate_pct` = +20 / 0 / −20 **additive on the actual school count**, 0 schools = +20 | ✅ (builder shows "Aura rate +20%" on an empty arsenal) | ❌ | ❌ | `gsdata_en.dat` floats at 0x33034 = 1.3, 1.0, 0.7 (**read from the file**); users `FUN_14012a750`, `FUN_140187710`, `FUN_1401d0750`, refill `FUN_14024de20` |
| Rule-legality marks (Single School / Highlander / Puny Only / Heavy Only) | ✅ bit tests recovered | ❌ | — | — | ❌ | `FUN_14015b830` (see appendix A); mask per rule table at `0x1402dbc78`; check `FUN_14015b9e0`, `FUN_1402152d0` |
| "Mark Illegal Skills" (RULE MARK: No Mark / Single school / Highlander / Puny / Heavy) | ~ script text only | ❌ | — | — | ❌ | `arsenaledit_en.ssb` ("Use this when creating an arsenal for battle with special rules…") |
| All-Aura (empty) arsenal is not usable | ✅ requires ≥1 skill (bit 0x4000) for every rule except Quick | ❌ legal (`arsenal.rs` test `aura_particles_can_fill_the_arsenal`) | — | ❌ | ❌ | `FUN_14015b830` tail (0x4000), table 0x1402dbc78 |
| Max saved arsenals | ✅ 16 per profile | ❌ unlimited | — | — | ❌ | `FUN_14015a730` loops to 0x10; `FUN_14015afe0` refuses at ≥16 |
| Arsenal name | ✅ ≤15 chars (+ default "Arsenal %02d") | ~ 28 chars, default "New Arsenal" | ✅ | — | ❌ | `FUN_14015afe0` `strncpy(…,0xf)` |
| Edit flow: pick a slot → Replace with stock skill / Replace with Aura / Add same skill; Stock list with page switching | ✅ | ❌ ours = append/remove list (Enter toggles) | ✅ | — | ❌ | `arsenaledit_en.ssb` "ARSENAL EDIT MODE", prompts "Replace with Aura / Add Same Skill / Replace", "STOCK / Switch Pages" |
| Type tabs: AURA, ATTACK, DEFENSE, ERASE, STATUS, SPECIAL, ENVIRONMENTAL | ✅ | ~ 6 types (no Aura tab); keyboard A/D only | ✅ | — | ❌ | `arsenaledit_en.ssb` |
| Sort: up to 3 keys from None/School/Cost/Strength/Number of Uses/Range/Rarity/Amount/Favorite | ✅ (labels) | ❌ fixed table order | — | — | ❌ | `arsenaledit_en.ssb` SORT; rarity = `s6`, range band no field found (FORMATS-SKILLS) |
| Mask school (hide/show per school); Favourites; Help page | ✅ labels | ❌ (school filter Q/E only) | — | — | ❌ | `arsenaledit_en.ssb` "Mask School", "Favorite", "Help for Editing the Arsenal" |
| Arsenal detail: TYPE counts, SCHOOL counts, "Arsenal Ratio" | ✅ | ❌ (only school list + aura %) | — | — | ❌ | `FUN_14015a8d0` (per-deck counts by school and capsule type), script "ARSENAL DATA / DETAIL" |
| Skill card (name/school/type/cost/STR-DEF/note) | ~ | ✅ same text as HUD card (local copy, `builder.rs:76`) | ✅ | ~ | ❌ | FRONTEND.md; no card art found (Mpt*.alr are story photos) |
| Duplicate-name save overwrites silently | — | ✅ by name (`builder.rs` Ctrl+S) | — | — | ❌ | `builder.rs:416-427` |
| Back with unsaved edits | ✅ original asks "Finish editing?" / "Undo edits?" / "Reflect changes?" | ❌ Esc/B leaves to Title and **discards** | ✅ code read | — | ❌ | `builder.rs:307-310`; script strings RETDECK / ENDCHECK / SAVEEND |
| Pad-only journey | ✅ original is pad menu | ❌ pad gets D-pad + A/B only: no list switch, filter, rename, save, delete, new, randomize | ✅ code read | — | ❌ | `builder.rs` `input()` (Tab/Q/E/A/D/R/N/G/Ctrl+S/Delete are `keys.just_pressed`); `input.rs` `read` |
| Focus/back behaviour, no traps | ~ | ~ rename mode traps input until Enter/Esc (fine); otherwise no trap | — | — | ❌ | `builder.rs:278-304` |
| Save/load/delete persistence | n/a (fan feature; original has profile storage) | ✅ `userdata/arsenals.json`, 4 round-trip tests | ~ | ✅ tests | ❌ | `frontend/arsenals.rs` |
| Randomize arsenal (G) | n/a house feature (DECISIONS) | ✅ 3 tests | ✅ | ✅ | ~ (roadmap "Future features") | `builder.rs:178` |
| DckMenu / Labo art and layout | ~ | ❌ **not used**: with `cache/ui/menu` present every screen uses the StMenu teal clouds; DckMenu sky is only a fallback. `DckMenu.alr` = stormy-sky backdrop + 3 small bar sprites; `Labo.alr` = lab-tunnel backdrop (checked by eye) | ✅ | — | ❌ | `ui.rs:160-165, 328-338`; FRONTEND.md "Backdrop (2026-09-26)" is stale; which original screen uses which art not read |
| Unlocks / stock / shop | n/a | n/a (no unlocks — Laurence) | | | ✅ ROADMAP "explicitly not wanted" | original checks ownership ("%s isn't in your stock", 99 cap, `FUN_14015ab90`) — deliberately dropped |
| Catalog membership | ~ | ~ 365 `registered` skills incl. 8 school-less enemy skills (701 Diffused Particle, 714/730/731 Body Attack, 724 Egg Bomb, 738 Dust Cannon, 740/741 "( debris)/( trash)") | ✅ | — | ❌ | `cache/skills.json`; SUSPECTED these are not player-collectable |

### 1b. Gamemodes / versus rules (P8)

| Feature | Mapped | Implemented | Reproduced | Verified | Laurence-approved | Evidence |
|---|---|---|---|---|---|---|
| Original's 12 game rules (Standard, Ante, Quick, Puny, Heavy, Special Environment, Single School, Highlander, Puny Only, Heavy Only, KO Battle, Annihilate the Odd One) | ✅ names, text, per-rule numbers | ❌ none selectable; only Standard-ish | — | — | ❌ | `vsmenu_en.ssb` 9252–11564; rule table (appendix B) |
| Per-rule starting Health and Level range | ✅ | ~ health fixed 20 (`START_HEALTH`); Level 0–5 setup row (house); cap 16 | ✅ | ❌ | ✅ health 20, Level 0 (DECISIONS 2026-09-26/27) | `gsdata` 0x33040 rows; `FUN_14024d8b0/d970/e790/d020` |
| Time limit | ✅ 10 min for all rules except KO Battle 3 min (`row[3]` × 1800 ticks @30 Hz) | ~ 5/10/**15 default**/none | ✅ | ❌ | ✅ 15 min (DECISIONS, from the guide) | `FUN_140128cc0:272915`; HUD clock `FUN_140150c30` (÷30 → seconds) |
| Rounds-to-win 1/2/3/Endless | ✅ original = one battle (`DAT_1404bc9b0 = 1`; >1 only for scripted sequences) | ✅ house option, default 1 | ✅ | ✅ | ✅ | `FUN_140128990`, `FUN_14012c510`; FRONTEND.md |
| Victory = last team standing; teams via +0x1c | ✅ | ~ 1v1 only | — | ❌ | ❌ | `FUN_14012c440`, `FUN_14012c510`, `FUN_14012c790` |
| Draw ends the battle ("VS Draw" result) | ✅ result scripts have win/lose/draw (`aVSWI/aVSLO/aVSDR`) | ❌ a draw restarts the round; the DRAW headline cannot be reached with a rounds target | ✅ code read | — | ❌ | `main.rs:1041-1055` (`winner <= 1` only), `result.rs` headline; `battleend_en.ssb` |
| Time-up winner | ❌ not read | ~ healthier wins (DECISIONS) | ✅ test | ❌ | ✅ (decision, not verified) | `combat.rs:1093-1102`; program starts at `FUN_14012ade0` (state 5, 90 ticks) |
| Simultaneous KO | ~ program counts deaths within 0x3c (60 ticks @30 Hz) of the earliest as simultaneous (SUSPECTED) | ~ same-tick only | — | ❌ | ❌ | `FUN_14012c510` (`iVar8 + -0x3c`), `FUN_14012c440`; ours `combat.rs:1151` |
| Starting hand (4 skills on buttons) | ❌ not in the program read; 3 capsule spots per player are full at round start | ✅ `start_hand`=4 default | ✅ | ❌ | ✅ ("original deals 4", from fan guides) | DECISIONS 2026-09-27; `PROGRAM-RE.md:602`; SUSPECTED (see Q1) |
| Capsule respawn | ✅ 420 ticks@30 Hz = 14 s + 15 appear | ~ 900 ticks@60 = 15 s | — | ❌ | ✅ (guide) | `PROGRAM-RE.md:480,602`; `combat.rs:30` |
| Level cap | ✅ 99 in normal rules (clamp), 5 in Puny | ❌ `MAX_LEVEL=16` | — | ❌ | ✅ "cap 16" (old) | `FUN_14024d970`; `combat.rs:31, 1468` |
| Quick (pre-made arsenals) | ✅ 8 decks `Q000`–`Q007` at `gsdata+0x33804`, 0x80 stride, 30×i16 at +0x14; one random deck per player | ❌ | — | — | ❌ | `FUN_14017df00`; dumped (16 skills + 14 Aura each, ≤2 copies, all 5 schools, kind stored = 2). Q003 slot 6 = 23428 (out of range → program counts it as empty) |
| Ante, Special Environment, KO Battle (respawn, 5 HP, 3 min), Annihilate the Odd One (host 40 HP, others 10) | ~ text + numbers; mechanics not read | ❌ | — | — | ❌ | `vsmenu_en.ssb`, table rows 10–12 |
| Brawl (house) | design only | ❌ no code (`grep -ri brawl` = 0 hits) | — | — | ✅ decisions 2026-09-27 (fixed curated sets; aura pool a setup option, no refill; one-press picker) | DESIGN-BRAWL.md |
| 2v2 / teams / revive (house) | design only; original has team ids (+0x1c) and "Tag Team" game type in its lobby | ❌ sim is `[X;2]` | — | — | ✅ decisions 2026-09-27 | DESIGN-2V2.md; `livecreategame`/`liverulesel` ("Battle Royale / Tag Team / One-on-One") |
| Setup flow | ✅ rule → stage → character → arsenal (per player, "View Arsenal") → lobby (Change Character / Arsenal / Level / Close) | ~ one two-column screen; ✅ only legal arsenals offered | ✅ | — | ✅ (Laurence's earlier rounds) | `vsmenu_en.ssb` 28513–30302 |
| Arsenal must fit the rule; error if none | ✅ "You cannot use this arsenal with the rules…", "Cannot create a session because there are no arsenals that meet the rules" | ❌ (no rules) | — | — | ❌ | `vsmenu_en.ssb` |
| Stage list | ✅ 17 versus stages (stageset table flag) | ✅ 17 (st01/st24 flagged false) | ✅ | ✅ | ✅ | MENUS.md "Stage table fact" (its claim that `stages.json` still lists 30 is stale: `cache/stages.json` marks them `versus:false`) |
| Result screen | ~ | ~ winner/draw headline + wins + fun stats | ✅ | ~ | ~ | `result.rs`; draw headline unreachable (above) |

---

## 2. Discrepancies (most important first)

1. **CONFIRMED — the builder cannot add an Aura Particle, and a legal arsenal must hold 30 real skills.** Original: a 30-slot deck where empties are Aura Particles (`FUN_14014f830`, `FUN_14015a800`), and an "AURA" category/"Replace with Aura". Ours: `SkillCatalog.skills` = registered skills only (`main.rs:765-807`, `builder.rs filtered()`); `arsenal::check` demands exactly 30 (`arsenal.rs:75`); no key adds `AURA_PARTICLE` (`arsenals::add_skill` supports it, nothing calls it). Only starter decks and Randomize ever contain Aura. Effect: Laurence can never build the typical ~14-Aura-Particle arsenal by hand.
2. **CONFIRMED — aura-recovery by arsenal is wrong in kind and amount.** Original: per-arsenal declared "Number of Schools" at `deck+0x50`; refill multiplier = `gsdata` floats **1.3 / 1.0 / 0.7** for 1 / 2 / 3, else 1.0 (`FUN_14012a750`, `FUN_140187710`, `FUN_14024de20`; values read from `gsdata_en.dat` 0x33034), multiplied with `(100 + Aura-rate status)/100`. Ours: additive +20 / 0 / −20 on the **actual** count of schools (`arsenal.rs:22`, `combat.rs:993`, rate line `combat.rs:1124`), and 0 schools (all-Aura) = +20. Effect: wrong Aura speed in every match; single-school arsenals get 1.2× not 1.3×, three-school 0.8× additive vs 0.7× multiplicative; the builder text "Aura rate +20%" on an empty arsenal is wrong.
3. **CONFIRMED — versus time limit.** Original: minutes = `row[3]` of the rule table → 10 min (Standard and most rules), 3 min (KO Battle) (`FUN_140128cc0:272915`; clock shown in seconds = ticks/30 in `FUN_140150c30`). Ours: default 15 min (from the story-mode "Time Regulation 15 minutes" guide text, which is the surface mission clock), options 5/10/15/none (`state.rs:61`, `combat.rs:98`).
4. **CONFIRMED — per-rule Health/Level.** Original table (appendix B): Puny 10 HP with Level capped at 5, Heavy 40 HP starting at Level 5, KO Battle 5 HP, Odd One host 40 / others 10; Standard Level range 0–99. Ours: one constant `START_HEALTH = 20` baked into heals and caps (`combat.rs:21, 1119, 1323, 2020, 2059`) and `MAX_LEVEL = 16` (`combat.rs:31`). Health must become per-fighter before any rule beyond Standard.
5. **CONFIRMED — no rule legality.** `FUN_14015b830` gives the four exclusion tests (appendix A); none exist in `arsenal.rs`. Setup therefore cannot offer Single School / Highlander / Puny Only / Heavy Only, and the builder cannot mark illegal skills.
6. **CONFIRMED — the builder is not pad-playable.** Only D-pad + A (add/remove/load) + B (leave) work on a pad; list switching, filters, rename, save, delete, new are keyboard-only (`builder.rs:328-437`). Laurence is pad-first.
7. **CONFIRMED — Back discards edits silently** (`builder.rs:307-310`); original asks three confirmation questions.
8. **CONFIRMED — a draw cannot end a match and the DRAW screen is unreachable** while a rounds target is set (`main.rs:1050`, `result.rs:38-47`); the original has a VS draw result. Legacy (`--demo`) path is exempt.
9. **SUSPECTED — starting hand of 4.** Not found in the program; fan guides only. The tutorial scripts say "Stand over one of the capsules / Let's see you capture a skill" for a fresh battle. If the original starts with empty buttons, `start_hand: 4` is a free 4-skill head start. Needs Laurence's memory (Q1).
10. **CONFIRMED — 16-arsenal cap and 15-character names** not honoured (`arsenals.rs` unbounded, `builder.rs:295` allows 28). Cosmetic.
11. **CONFIRMED — stale/incorrect docs:** FRONTEND.md "Backdrop (2026-09-26)" (DckMenu sky is unused once menu art exists); MENUS.md "`stages.json` still lists them as versus" (false now); FRONTEND.md "the original's 15-minute time" assumption. Builder screenshot `ui_arsenal.png` shows the 363/365-skill catalog with no sort, no detail panel, no Aura row.
12. **SUSPECTED — catalog includes 8 school-less enemy/object skills** (list in 1a). Decide with a read of how the program's stock/shop list is built (catalogue number `s5` ≠ 0 for these?).
13. **SUSPECTED — simultaneous-KO window.** Program treats a player whose death time is within 0x3c (60 ticks @30 Hz) of the earliest death as dying together (`FUN_14012c510`, `FUN_14012c440`: `iVar8 + -0x3c`); ours requires the same tick (`combat.rs:1151`).
14. **Cross-area (for the combat lane, not mine):** capsule respawn 14 s + 15 ticks vs our 15 s; the refill curve `max(0.25, 1.25 − Aura/10)`/s (GUESS-INVENTORY CMB-01) interacts with item 2's multiplier.

Already right (one row each above): 30 slots, 3-copy cap with Aura exempt, -1 → Aura, powerless drain (91 ticks@30 Hz = ours 180@60), 17 versus stages, single-battle default, legal-only arsenals in setup.

---

## 3. Reads still needed

- **Where the 3-copy check and the school-limit comparison live** (script natives, `0x140214000–0x140218000`; candidates `FUN_14015ab90` (add to slot), `FUN_14015bbf0` (returns deck+0x50 and, for kind 1, the single school — probably also the aura colour), natives around `FUN_140216b60`). Question: is the school limit exactly the declared kind N, and what do kinds 4–5 do (handicap = 1.0)?
- **How the rule id reaches `DAT_1404bc5dc`** in the local "Start Game" path (`FUN_1401d0750:386386` sets it from `DAT_1405b185a` for game kind 6; confirm vsmenu uses the same path) and what "Change Level" in the lobby changes.
- **Time-up / draw resolution:** state 5 `FUN_14012ade0` (90-tick hold), state 6 `FUN_14012b110`, result picker `FUN_14012c790` (`DAT_1404bbec8` 1 win / 2 lose / 6 draw), `FUN_14012c440` (the 60-tick window). Who wins on time-up (health? damage? draw)?
- **Ante** (how the ante skill is chosen/kept, `battleend` "Skill lost due to ante"), **Special Environment** interval/effects, **KO Battle** respawn delay (row field `3`? 3 s?) and scoring, **Annihilate the Odd One** roles — rule-specific code keyed on `DAT_1404bc5dc` (10 = KO at `FUN_14012c510`, 11 = Odd One at `FUN_14024d970`).
- **Quick deck selection:** `FUN_1401282f0` (rand >> 12 & 7 → `FUN_14017df00`) — confirm one independent deck per player and that kind/aura factor comes from the stored 2.
- **Which original screen uses `DckMenu.alr` vs `Labo.alr`** and how skill rows/cost discs are drawn there (draw natives `DRAW_SORTMENU`, `DRAW_DTDATAL`, `DRAW_ATTMSK` named in the script); `BtMenuEn`/`WusMnEn` glyphs for capsule-type icons.
- Whether a fresh battle deals any skills (search the round-start path after `FUN_14014ee70(…,1)`); the Xbox build's `Menu/*.ssb` (unpacked for H6) could confirm the rule list against 2004.
- Skill "Range" (short/medium/long) field for the sort list — none found (FORMATS-SKILLS s44 is a candidate).

---

## 4. Proposed build lanes

Constraint: a later four-player lane (P6) owns `frontend/setup.rs`, `frontend/state.rs` (`MatchSetup`), `frontend/input.rs`, controller joining, `main.rs` match start, `hud.rs`. The lanes below stay out of those files and communicate through pure APIs.

**Lane F1 — Arsenal rules + builder (P7). Order 1.**
- Goal: original-like legality and a pad-playable builder.
- Owns: `arsenal.rs` (all of it), `frontend/builder.rs`, `frontend/arsenals.rs` (saved format gains an optional `kind` 1–5, default from contents; 16-cap; 15-char name), `frontend/discovery.rs` (`school_decks`, quick-deck builtins), `frontend/script.rs` (append-only tokens for builder tests), new builder-only UI helpers in a new file `frontend/builder_ui.rs`. The builder reads pads itself (any pad, LB/RB switch list, X/Y filter or sort, Start save, Back = confirm-discard dialog) so it needs nothing from `input.rs`.
- Delivers pure API for the other lanes: `arsenal::rule_flags(deck, skills) -> u16` (0x20/0x40/0x80/0x100/0x4000), `arsenal::eligible(rule_id, …)`, `arsenal::aura_handicap(kind) -> f32` ([1.3,1.0,0.7], else 1.0), Aura Particle row in the catalog/AURA tab, slot-replace editing ("Replace with Aura", "Add same skill"), 3-key sort + favourites + illegal-mark, arsenal detail (type/school counts like `FUN_14015a8d0`).
- Tests (from reads, not old behaviour): `rule_flags` bit cases mirroring `FUN_14015b830` (single-school ignores school 0 and empties; Highlander ignores empties; Puny/Heavy use cost ≤3 / ≥3 and exclude `s37 == 2`; 0x4000 only with ≥1 skill); all-Aura deck illegal except for Quick (table `0x1402dbc78`); handicap table vs the 3 floats (data-dependent test, skipped when `game_files` absent); saved-format back-compat; 16-arsenal/15-char caps; builder state machine unit tests (focus, discard prompt, pad-only path reaches save/load/delete/new).
- Runtime proof: `--open-screen arsenal` + extended `--menu-script` (pad tokens) screenshots: build a Single-School arsenal entirely by script, save, reload; Aura row present; illegal-mark shown; discard prompt; every step with pad tokens only. Laurence does the real pad journey.
- Also: decide DckMenu/Labo art after the Labo read (fence: `ui.rs` additions only via new functions).

**Lane F2 — Rules contract in the sim + data readers (P8, sim side). Order 1 (parallel with F1; disjoint files).**
- Goal: a named `RuleSet` for the original's rules, replacing the lone `Rules` struct.
- Owns: `combat.rs` (`Rules`, `start_round`, `Fighter` max health, the refill-rate line in `tick_fighter`, time limit, draw/time-up/simultaneous-KO handling), `pd_formats/src/skills.rs` + `pd_convert skills` (write `rules` rows and `quick_decks` into `skills.json`; regenerate cache), `main.rs` only the two `set_rules` call sites (`:742`, `:997`) — one integrator touches them.
- Content: table rows (appendix B) → health, Level min/max, minutes; multiplicative handicap from F1's `aura_handicap` via a `Fighter.aura_handicap` field; per-fighter max health; Level cap from the rule row (99, Puny 5, Heavy ≥5); Standard first, then Puny/Heavy/Single/Highlander/Puny Only/Heavy Only (pure rule-table + legality), then Quick (8 decks), Ante, Special Environment. KO Battle and Annihilate only after the four-player foundation. Brawl and 2v2 per their design docs layer on the same `RuleSet` (`aura_refill`, `capsules` flags), not new code paths.
- Tests: rule-row parse test vs layout; fighter starts at `row[1]` Level / `row[0]` HP for each rule; Puny Level clamp at 5; Heavy starts at 5; time limit = minutes × 3600 ticks (60 Hz) per rule; draw result ends the battle; handicap multiplies the existing refill rate; all existing combat tests still green (`TEST_RULES` kept).
- Runtime proof: scripted `PD_SHOT_TICKS` captures per rule (HUD health/level/timer at tick 200 and at time-up), `PD_SFX_LOG`-style log of round end, deterministic replay test.

**Lane F3 — Mode wiring in the screens (P8 front end). Order 3 — only after P6 lands its setup/`MatchSetup` shape.**
- Goal: Game Rule row / per-rule arsenal eligibility in setup, result draw headline, Brawl title entry (+ trimmed picker), mode matrix tests.
- Owns: `frontend/setup.rs`, `frontend/state.rs`, `frontend/title.rs`, `frontend/result.rs` **after** P6; if P6 is not finished, P6 adds a single `rules: combat::RuleSet` field to `MatchSetup` and a row stub, and F3 fills them in. `check_match_end` draw handling is F2's semantics, F3's call site.
- Tests: invalid setups rejected ("no arsenals meet the rules"), full start → finish → rematch per approved rule, no rule leaking into the next match.

Order: F1 ∥ F2 → (P6 four-player foundation) → F3. The four-player lane can start without waiting for F1/F2.

---

## 5. Questions only Laurence can answer

1. In the original, when a normal battle begins, what is on your four skill buttons?
   (a) four skills already dealt, (b) empty, you grab capsules first (fan guides say (a); the tutorial text hints (b)), (c) not sure. **Default until answered: keep (a).**
2. How should our arsenal builder work on the pad?
   (a) copy the original: 30 fixed slots starting all Aura Particles, pick a slot then pick a replacement, "Replace with Aura" (recommended — it is what the original does and makes Aura Particles easy), (b) keep our add/remove list but auto-fill the rest with Aura Particles.
3. The original asks "Number of Schools" (1–5) when you create an arsenal: one school recovers Aura faster (×1.3), two normal, three slower (×0.7). Keep that choice?
   (a) yes, choose at creation like the original (recommended), (b) work it out automatically from the skills picked.
4. Versus time limit: the original's is 10 minutes (3 for KO Battle), ours defaults to 15.
   (a) default to 10, keep 5/15/no-limit as extra options (recommended), (b) exactly the original only, (c) stay at 15.
5. Which original rules first (after Standard)?
   (a) Puny, Heavy, Single School, Highlander, Puny Only, Heavy Only (only needs the rule table + legality; recommended), (b) those plus Quick (8 preset arsenals) and Ante, (c) everything including KO Battle / Odd One (needs four players, so later).

---

## Appendix — recovered original tables (evidence)

**A. Arsenal legality bits** (`FUN_14015b830`, deck = 100-byte record: +4 name[16], +0x14 skills[30] i16 (-1 = empty), +0x50 u16 declared school kind, +0x52 u16 computed flags). Skill record fields read at `DAT_1404bdc50 + id*0x90`: +0x12 school (our s11), +0x28 cost (s22), +0x46 = s37, +0xe capsule type (s9).
- 0x20 single school: every skill with school > 0 has the same school.
- 0x40 no duplicates: no id appears twice (empties ignored).
- 0x80 Puny Only: every skill has cost ≤ 3 and s37 ≠ 2 (2 = variable-cost "Cost X").
- 0x100 Heavy Only: every skill has cost ≥ 3 and s37 ≠ 2.
- 0x4000: at least one real skill.
- Required mask per rule id (`0x1402dbc78`, 12 entries): Standard 0x4000, Ante 0x4000, Quick 0 (no arsenal), Puny 0x4000, Heavy 0x4000, Special Env 0x4000, Single School 0x20, Highlander 0x40, Puny Only 0x80, Heavy Only 0x100, KO 0x4000, Odd One 0x4000 (read from `PDUWP.exe`).

**B. Rule table** (`gsdata_en.dat` at 0x33040, stride 0x10: health, Level min, Level max, minutes; read by `FUN_14024d8b0/d970/e790` → `FUN_14024d020`, `FUN_140128cc0`). Rule index = `DAT_1404bc5dc`.

| # | Rule | Health | Level min–max | Minutes |
|---|---|---|---|---|
| 0 | Standard | 20 | 0–99 | 10 |
| 1 | Ante | 20 | 0–99 | 10 |
| 2 | Quick | 20 | 0–99 | 10 |
| 3 | Puny | 10 | 0–5 | 10 |
| 4 | Heavy | 40 | 5–99 | 10 |
| 5 | Special Environment | 20 | 0–99 | 10 |
| 6 | Single School | 20 | 0–99 | 10 |
| 7 | Highlander | 20 | 0–99 | 10 |
| 8 | Puny Only | 20 | 0–99 | 10 |
| 9 | Heavy Only | 20 | 0–99 | 10 |
| 10 | KO Battle | 5 | 0–99 | 3 |
| 11 | Annihilate the Odd One (others) | 10 | 0–99 | 10 |
| 12 | (Odd One host, used when rule 11 and player 0) | 40 | 0–99 | 0 |

Start Level = Level min, Aura full. Menu text agrees (Standard "Level 0, 20 Health"; Puny "max Level 5, 10 Health"; Heavy "Level 5, 40 Health"; KO "Health 5"; Odd One "host 40, others 10").

**C. Aura factor by declared kind** (`gsdata` 0x33034, three floats): 1 school 1.3, 2 schools 1.0, 3 schools 0.7; kinds 0, 4, 5 → 1.0.

**D. Quick decks:** 8 × 0x80 bytes at `gsdata` 0x33804 (`FUN_14017df00`), names `Q000`–`Q007`, 16 skills + 14 empty, kind 2.

**E. Original versus flow** (`vsmenu_en.ssb`): Start Game → GAME RULE → STAGE LIST → character (per player) → ARSENAL LIST (per player, "View Arsenal"; rule-ineligible arsenals greyed with "Enabled for …" notes) → LOBBY (Session Info: Game Type One-on-One, Game Rule, Level; options Change Character / Change Arsenal / Change Level / Close Session; "Start Game"). Session types in the live lobby: Battle Royale, Tag Team, One-on-One.
