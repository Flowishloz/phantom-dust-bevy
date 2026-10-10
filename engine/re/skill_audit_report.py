"""The prose half of the ledger: fix lanes, footage asks, the spot check (used by `skill_audit.py`).

Kept apart from the rules so that the numbers (skill_audit_rules.py) and the proposals (here) can be
read, tested and changed independently. Nothing here reads game data; it works on the `Row`s that
`skill_audit.build_rows` produced.
"""

import skill_audit_rules as R


def _A():
    import skill_audit as A  # late: skill_audit imports this module from main()

    return A


# Each lane: which crosses it clears, and the fences that keep two lanes from touching the same code.
# `match(row, cell)` decides membership; the value of a lane = the sum over the skills it fixes of
# (rows - usage rank + 1), i.e. the most-used skills count most.
LANES = [
    dict(
        id="INDEX",
        title="Built-in arsenals read deck entries as skill ids; they are table indices",
        match=lambda r, c: False,  # not a per-skill cross: its skills are the ones in the built-in decks (see `index_skills`)
        why="The original's Quick decks and the four starter decks hold TABLE INDICES (record + 1, 0 = Aura Particle; `FUN_140262670` indexes the skill table with them); `frontend/discovery.rs` keeps them as `SkillDef.id` (s3), which names a different skill for almost every entry (see the index-space section above). Laurence's built-in 'Psycho / Optical / Ki / Nature' and 'Quick Q000-Q007' arsenals therefore hold the wrong skills.",
        fence="`crates/pd_game/src/frontend/discovery.rs`: only `school_decks`, `quick_decks_from_json`, `quick_decks` and their tests (map `record + 1 -> id` from `skills.json`'s `skills[].record`). Nothing else reads deck entries.",
        proof="the 8 Quick decks hold 2 schools each and each starter deck one school (the school-purity numbers of the index-space section turn to 100 %).",
    ),
    dict(
        id="LOAD",
        title="Duplicate skill ids read the characters' dodge record (Judge of Anubis 257, Fusion 294)",
        match=lambda r, c: r.id in (257, 294) and c.st == R.BAD,
        why="`main.rs::load_action_rows`, `load_shot_markers` and `sfx.rs::load_voice_groups` collect records into a map, so the LATER record of an id wins; ids 257 and 294 appear twice and the second record is a character's dodge move (records 749 / 750, FORMATS-SKILLS.md 'Phase 3'). Judge of Anubis casts with the clip WAIT0 and fires at once, Fusion plays DASH_A. `load_skills` and `fx::SkillFx::load` already keep the first record.",
        fence="`crates/pd_game/src/main.rs`: only the functions `load_action_rows`, `load_reaction_rows`, `load_shot_markers`; `crates/pd_game/src/sfx.rs`: only `load_voice_groups`, `load_effect_skills`. New test beside `load_skills` in main.rs (first record wins for 257 / 294); `combat.rs` test `the_instant_skills_are_the_ones_whose_row_names_no_clip` (it lists 257 as an instant because it reads the dodge record: it must skip the second record of an id).",
        proof="`python engine/re/skill_audit.py --run`: the `cast.*`, `snd.cast` and `melee.*` crosses of ids 257 and 294 clear.",
    ),
    dict(
        id="INSTANT",
        title="Instant effects (motion 4) land at the fire tick; the program lands them s65 ticks later",
        match=lambda r, c: c.ch == "add" and c.key == "instant_delay" and c.st == R.BAD,
        why="`FUN_140265ea0` stores s65 as the object's countdown and `FUN_140265f50` applies the effect when it reaches 0 (to every player for s64 = 2). 25 ticks for most (Purify, Fairy Ring, Quantum Decay, Armaros, Chill Breeze, Blessed Rain, Vicious Balance...), 5 for Frighten, 0 for the crystal erasers. Ours applies it inside `fire`.",
        fence="`crates/pd_game/src/combat.rs`: only `Combat::fire` (the `Behaviour::Effect(TargetNow | Everyone)` arm) and `take_effect` (a small queue of due tick / caster / target / skill beside `pending_effects`). Sim state: append-only, replay hash untouched.",
        proof="the `add.instant_delay` crosses clear; unit test: a Purify cast changes the opponent exactly 2 x (fire tick + s65) ticks after the press.",
    ),
    dict(
        id="RULE26",
        title="STR rule 26 (bundled skills) picks the kind by the record's s10 field, not by name",
        match=lambda r, c: c.ch == "str" and c.key == "bundle" and c.st == R.BAD,
        why="`0x140274990` sums s46 of the caster's equipped skills whose s10 equals this skill's s46 (Bundled Laser 2, Bundled Blaster 3, Crystallization 4, Dance of Death 1). `Combat::bundled_strength` matches by name / school: it misses Octolaser, wrongly adds Laser Blade, and counts Ki Palm, Aura Leak and Aura Backflow for Dance of Death.",
        fence="`crates/pd_game/src/combat.rs`: only `Combat::bundled_strength` (and its test). It needs `raw[10]` of each equipped skill: already in `SkillDef::raw`.",
        proof="the `str.bundle` crosses clear.",
    ),
    dict(
        id="MOVE",
        title="Skywalk / Levitate rise instantly to a fixed height; Dash teleports 109.8 u",
        match=lambda r, c: c.ch == "movement" and c.st == R.BAD,
        why="Program (`FUN_140270200`, PROGRAM-RE.md 'Dodges and movement skills'): Levitate / Skywalk set float status 11 for s46 ticks and make the s44 = 0 jump of height s45 (apex 10 x s45 = 100 / 50 u under gravity 0.12), floating at the top; Dash moves s45 x 10 = 100 u over the row's dash window (15 ticks). Ours snaps the body to 109.8 / 54.9 u in one tick (`hover_height` 30 ft / 15 ft from the note text) and Dash is an instant 109.8 u offset.",
        fence="`crates/pd_game/src/moves/special.rs`: only the `move_self` arms LEVITATE and DASH (and Skywalk's float path); `crates/pd_game/src/sim.rs`: an additive scripted-dash / float-rise state on `Player` next to `begin_jump` (excluded from the replay hash until used).",
        proof="the `movement.*` crosses clear; the movement probe's apex 50 / 100 u and 15-tick dash match the program.",
    ),
    dict(
        id="GIFT",
        title="Annoying Gift also overwrites the pressed button",
        match=lambda r, c: c.ch == "add" and c.key.startswith("code50") and c.st == R.BAD,
        why="`FUN_140272bc0` case 0x32 skips the pressed button (`param_2 + 0xc != k`): the target gets only the caster's OTHER buttons. `special.rs::target_effect` ADD ANNOYING_GIFT copies all four, so the opponent receives Annoying Gift itself.",
        fence="`crates/pd_game/src/moves/special.rs`: only the `add::ANNOYING_GIFT` arm of `target_effect`.",
        proof="the `add.code50` cross clears.",
    ),
    dict(
        id="SWAP",
        title="Change the World erases capsules instead of swapping them",
        match=lambda r, c: c.ch == "capsules" and c.st == R.BAD,
        why="`FUN_14026ff20` case s45 = 2 swaps the capsule's skill for a random undrawn card of its owner's deck (`FUN_14014fa20`); ours erases every capsule.",
        fence="`crates/pd_game/src/moves/erase.rs`: only the capsule-field arm for s45 = 2 (needs read access to `Fighter::arsenal`, already public).",
        proof="the `capsules.*` crosses clear.",
    ),
    dict(
        id="CHARGE",
        title="Dragon Bullet is a hold-to-charge skill in the program",
        match=lambda r, c: c.ch == "cast" and c.key == "charged" and c.st == R.BAD,
        why="Its action row carries the charge wind-up (w39 = 13, like Gungnir / Ki Cannon / Ki Palm); `SkillDef::charged` decides by `strength_rule == 21 && cost == 0`, which Dragon Bullet does not meet.",
        fence="`crates/pd_game/src/moves/mod.rs`: only `SkillDef::charged` (and `arsenal.rs::is_charge_skill` if it duplicates the rule).",
        proof="the `cast.charged` cross clears.",
    ),
    dict(
        id="AURA",
        title="Aura Particle use: the original holds nobody and plays only sound 89",
        match=lambda r, c: r.id == 0 and c.st == R.BAD,
        why="MEASURED live on the original by the moverec lane (MOVE-RECORDER.md): the use fires 0 ticks after the press, the fighter keeps running at 1.85 u / tick before, at and after it, and only common/89 goes through the sound routine. Ours holds the fighter 46 ticks (`AURA_ABSORB_TICKS`, owner-directed 2026-10-01) and also plays common/90 (`SFX_AURA_USED`, fitted to footage audio).",
        fence="`crates/pd_game/src/combat.rs`: only the `Behaviour::AuraParticle` arm of `Combat::cast_from` (leave `action_ticks` at 0, DECISIONS caststill 'revert = ...'); `crates/pd_game/src/sfx.rs`: only the `AuraParticleUsed` arm of `cues_in_batch`. The body glow (`aura_pulse.rs`) is separate and not in question.",
        proof="the `cast.hold` and `snd.use` crosses of the Aura Particle clear. Laurence's call: it reverses his own earlier instruction.",
    ),
    dict(
        id="AUTOSHELTER",
        title="Automatic Shelters are one-block objects: every attack triggers a fresh use, no standing guard",
        match=lambda r, c: c.ch == "guard" and c.key in ("auto_life", "auto_second", "auto_pay", "auto_first"),
        why="READ: `FUN_14026f530` (called by the hit function `FUN_14026f870` before any damage) takes the holder's first slot with an s43 = 2, s44 = 7 skill that can pay, uses it (`FUN_1402626d0`: pay, consume the use) and resolves THIS attack against the new guard object with `FUN_140271760`. The object is never in contact with anything on its own (`FUN_14026dcb0`: kind 7 falls through to 'no contact') and `FUN_140264340` (disassembled with capstone; Ghidra never made it a function) ends a kind 7 object at its first update. So one block per use: a second attack in the same moment triggers another Shelter (or lands, if the skill was used up), and every trigger pays again (Guardian Angel: 2 Health each time). Ours (`Combat::auto_guard`) stores a standing guard that lingers 10 sim ticks and blocks every attack meanwhile without paying again; probe: two weak shots 0 / 2 / 12 ticks apart. What the holder's body does while the Shelter is used (`FUN_1402626d0` plays the row's clip) is not read and not touched.",
        fence="`crates/pd_game/src/combat.rs`: only `Combat::auto_guard` and its one call in `strike` (resolve the attack against a guard that is NOT stored in `Fighter::guard`); tests beside `auto_capture_puts_the_next_skill_on_the_button_at_once`.",
        proof="the `guard.auto_life`, `auto_second` and `auto_pay` crosses of Auto Shell, Guard of Water, Mind's Eye and Guardian Angel clear.",
    ),
    dict(
        id="ORBPAY",
        title="Orb / Amulet are used (and paid) when an erase attack hits their holder; ours is free",
        match=lambda r, c: c.ch == "guard" and c.key == "orb_pay",
        why="READ: the second loop of `FUN_14026f530`: an erase-colour attack object (s9 = 3) hitting the holder of an s37 = 1 skill (Orb 180, Amulet 299) calls `FUN_1402626d0` on it, which pays the skill's cost first (`FUN_1402621f0`: Orb 2 Aura, Amulet 1; a holder that cannot pay does not trigger it). Ours (`moves/erase.rs::resolve`) negates / bounces the erase for free. What the use does to the erase (negate / reverse) is UNCONFIRMED (the `T` cell `guard.orb_effect`: the code that makes it not land was not found), so take one recording first: an Orb holder hit by Interference at Aura 0 and at Aura 8 (does an unpaid Orb still negate?).",
        fence="`crates/pd_game/src/moves/erase.rs`: only `resolve` and the Aura payment of the Orb / Amulet it finds (needs `&mut Combat`).",
        proof="the `guard.orb_pay` crosses clear (after the take decides the unpaid case).",
    ),
    dict(
        id="BLAST3",
        title="A shot's blast reaching a guard is a blast to the program (state 3), a shot to ours",
        match=lambda r, c: c.ch == "guard" and c.key == "bat_blast",
        why="READ: `FUN_14026dbd0` puts an exploding attack in object state 3; `FUN_140271760` then lets it through Absorb and Brush (`return 0` before any DEF), through About Face / Reverse (state 3 returns 0), does not send it back for a Reflect (the Reflect branch needs state != 3: a plain erase), and a blast is not erased by the block (flag 0x20). Ours decides 'blast' by `arrival == Area && blast_radius > 0` (`barrier::never_stops`), so the blast of a shot (`tick_blasts` with `area == false`, a Bomb landing on a wall or floor beside a holder, or a splash on a third player) is struck as `Arrival::Shot`: Absorb / Brush / About Face stop it, a Reflect Barrier sends a Bomb Shot's blast back, Digestion heals on it, Learning captures it. Reached in 1v1 by a bomb that lands next to a holder; the target's own blast is skipped, as in the program.",
        fence="`crates/pd_game/src/moves/barrier.rs`: the `Arrival` enum (add `Blast`), `never_stops` and `resolve`; `crates/pd_game/src/combat.rs`: only `tick_blasts` (pass `Arrival::Blast` for both shot and area blasts). Replay hashes unaffected unless a recorded run has a bomb blast beside a guard.",
        proof="the `guard.bat_blast` crosses of Absorb / Brush / Reflect / About Face / Reverse and the Add-effect guards clear.",
    ),
    dict(
        id="GUARDADD",
        title="Two guard side effects: Absorb Energy gains the blocked attack's STR (not its cost); a Brush stuns only motion 3 / 12 / 13",
        match=lambda r, c: c.ch == "guard" and c.key in ("bat_aura_gain", "bat_brush_pursuit"),
        why="READ (capstone, `FUN_140271ed0` case 2): the Add effect 'Aura increase' calls `FUN_1402618e0(holder, (float)[attack + 0x60])`: the attack's STR, the same field `FUN_140271760` subtracts from the guard's DEF; ours adds the attacked skill's Aura cost (`barrier::add::AURA_FROM_COST`: `attack.cost`). READ (`FUN_14026cd70` case 7): the Brush stun (reaction 26) is played only for attack motions 3, 12 and 13; a Pursuit (motion 14) takes the 'brushed aside' branch with no stun; ours stuns every `is_short_range` attacker (3, 12, 13, 14) and every melee arrival.",
        fence="`crates/pd_game/src/combat.rs`: only `Combat::guard_extra` (the `AURA_FROM_COST` arm: add `strength`; the Brush stun: motions 3 / 12 / 13). `barrier::is_short_range` stays as it is (the Add-effect gate `FUN_140167910` includes 14).",
        proof="the `guard.bat_aura_gain` cross of Absorb Energy and the `guard.bat_brush_pursuit` crosses of the five Brush guards clear.",
    ),
    dict(
        id="REQ5",
        title="Requirement 5 ('N or fewer skills left') counts the player's capsules too",
        match=lambda r, c: c.ch == "pay" and c.key == "requirement" and c.st == R.BAD,
        why="READ: `FUN_140262470` case 5 refuses when skills left > N, and skills left is `(30 - dealt) + DAT_140585c20` (the player's capsule objects not retired: lying, appearing, waiting to regenerate; `FUN_14014ff80` returns the same number for the HUD). Ours (`Combat::requirement_met`, arm 5) compares the undealt deck only: with the deck at the limit and one capsule lying, Excalibur / Cluster Bomb / Rail Gun still cast.",
        fence="`crates/pd_game/src/combat.rs`: only the `5 =>` arm of `Combat::requirement_met` (owner's `capsules` + `pending_capsules()` for that player).",
        proof="the `pay.requirement` crosses of Excalibur, Cluster Bomb and Rail Gun clear.",
    ),
    dict(
        id="COPYNO",
        title="Recollection / Copy with nothing remembered play the 'no' sound",
        match=lambda r, c: c.ch == "cast" and c.key == "empty_no_sound" and c.st == R.BAD,
        why="READ: `FUN_140258180` gets the skill index -1 from `FUN_140261e90` (nothing remembered) and, while the player's mode flag is set, plays `FUN_140276330(0, 2, 2, ...)` = common/2, the sound ours plays for `CombatEvent::Denied`; ours refuses the press silently.",
        fence="`crates/pd_game/src/combat.rs`: only the Recollection / Copy arm of `Combat::cast_from` (log `Denied` when the copied skill is `None`).",
        proof="the `cast.empty_no_sound` crosses of Recollection and Copy clear.",
    ),
    dict(
        id="INSTANT-HOLD",
        title="DECISION, not a fix: ten instant casts keep the clip hold",
        match=lambda r, c: r.id != 0 and c.ch == "cast" and c.key == "hold" and c.st == R.BAD and c.prog == 0,
        why="The program enters no cast state for a row naming clip 0 (WAIT0): Optical Camo, Jump, Retreat, Skywalk, Return, Levitate, Surprise Attack, Void, Demon's Wing, Charge (Recollection / Copy resolve at the press) are usable at once. DECISIONS 2026-10-01 (caststill) kept the clip hold for the ten roster skills (the Aura Particle's old 46-tick hold was owner-directed, MEASURED wrong live and since removed). Listed so a lane does not 'fix' the ten unasked.",
        fence="(none: Laurence decides) `Combat::cast_from`, the arms that set `action_ticks`, if he reverts it.",
        proof="the `cast.hold` crosses with program 0 clear.",
    ),
]


def index_skills(rows):
    """The skills that sit in the original's built-in decks (Quick + starter): the ones the INDEX lane puts right."""
    return [r for r in rows if r.id and (r.usage["quick_decks"] + r.usage["debug_decks"]) > 0]


# set by `write` from `skill_audit.index_check`: True when our own `school_decks` builds the original's decks (the INDEX lane is done)
STATE = {"index_ok": False, "closed": []}


def lane_rows(rows):
    n = len(rows)
    out = []
    claimed = set()
    STATE["closed"] = []
    for lane in LANES:
        hit = {}
        if lane["id"] == "INDEX" and STATE["index_ok"]:
            continue
        if lane["id"] == "INDEX":
            for r in index_skills(rows):
                hit[r.id] = (r, [])
        for r in rows:
            for c in r.bad():
                if lane["match"](r, c):
                    hit.setdefault(r.id, (r, []))[1].append(c)
                    claimed.add((r.id, c.ch, c.key))
        value = sum(n - r.rank + 1 for r, _ in hit.values())
        if not hit:
            STATE["closed"].append(lane["id"])  # no cross left for it: the lane was merged (verified by this run)
            continue
        out.append((lane, hit, value))
    out.sort(key=lambda t: (-t[2], -len(t[1])))
    unclaimed = [(r.id, c.ch, c.key) for r in rows for c in r.bad() if (r.id, c.ch, c.key) not in claimed]
    return out, unclaimed


def lanes_text(rows):
    A = _A()
    lanes, unclaimed = lane_rows(rows)
    w = []
    w.append("## Proposed fix lanes")
    w.append("")
    w.append("Ordered by value = the sum over the skills a lane fixes of (rows - usage rank + 1): number of skills x how used they are. Fences are disjoint: no two lanes touch the same function. `INSTANT-HOLD` is a decision for Laurence, not a lane.")
    w.append("")
    w.append("| # | lane | skills | best rank | value |")
    w.append("|---:|---|---:|---:|---:|")
    for k, (lane, hit, value) in enumerate(lanes, 1):
        best = min((r.rank for r, _ in hit.values()), default=0)
        w.append("| %d | **%s** %s | %d | %s | %d |" % (k, lane["id"], lane["title"], len(hit), best or "-", value))
    w.append("")
    for k, (lane, hit, value) in enumerate(lanes, 1):
        names = ", ".join("%s (#%d)" % (r.name, r.rank) for r, _ in sorted(hit.values(), key=lambda t: t[0].rank))
        w.append("%d. **%s** - %s" % (k, lane["id"], lane["title"]))
        w.append("   - Skills: %s." % (names or "none"))
        w.append("   - Why: %s" % lane["why"])
        w.append("   - Fence: %s" % lane["fence"])
        w.append("   - Done when: %s" % lane["proof"])
    if STATE["index_ok"]:
        STATE["closed"].insert(0, "INDEX")
    if STATE["closed"]:
        w.append("")
        w.append("Lanes of earlier waves with no cross left in this run (merged and verified by the probe): %s." % ", ".join(STATE["closed"]))
    if unclaimed:
        w.append("")
        w.append("Crosses not claimed by any lane (the tool needs a lane for them): " + ", ".join("%d %s.%s" % u for u in unclaimed))
    w.append("")
    w.append("### Reads and probes that turn UNCONFIRMED / not-probed values into matches or crosses (no game change)")
    w.append("")
    w.append("Tool-only work, parallel to the lanes above; each names the program function to read and the probe to add.")
    w.append("")
    ug = {k: v[1] for k, v in A.unsettled(rows).items()}  # needs-take (T) and unconfirmed (?) cells by class
    n_def = sum(1 for r in rows if r.rec.get("capsule") == "defence")
    w.append("Wave 6 (lane probes) built the probes below; **done** items are in the ledger as cells, the rest is what is still open.")
    w.append("")
    w.append("1. **STR rule 21**: **done** for the hold-to-charge skills (Gungnir, Ki Cannon, Ki Palm: STR = (program ticks held after the wind-up) / 10 x s46 at three hold times) and Finisher (2 x Level at the press), `str.damage`. **Open**: Power and Psycho Kinesis ([Object]: the stored value comes from the thrown object), %d skills still `T` / `?`." % len(ug.get("str.damage", [])))
    w.append("2. **Guard battery for the %d defences** (`guard.bat_*`, `FUN_140271760`): **done**. Every guard is struck through `Combat::strike` by one attacker of each roster class (the first skill of each colour / motion / s54 / blast kind: shells, lobs, falls, crawlers, arcs, rain, holes, mines, waves, tornadoes, objects, swords, punches, Pursuit, ricochet, erase shells, a Pierce and a penetrating one of each, and the blast of every blast shot) from the front, the back (disc guards cover the front only) and, for Rain / Hole / Tornado, from above, at STR 1, DEF, DEF + 1 and DEF + 3. The program side (`skill_audit_guards.py`) is transcribed from `FUN_140271760` / `FUN_14026dcb0` / `FUN_14026cd70` / `FUN_140271ed0`: block, break, Pierce leftover, Anti-Pierce, s54 = 2 ignore, the per-kind ignore lists, the 5-tick linger, the guard leaving its button, sent-back (Reflect / About Face), Brush stun, and every Add effect. **Not compared**: a Hole's contact with a disc guard (the segment of `FUN_14026c5b0` is unread, those cases are skipped), Course's steering (it only flags the attack; the turn happens in the shot's own update), and statuses 20 / 31 (Berserk / Necronomicon) as probe variants." % n_def)
    w.append("3. **Automatic Shelters, Orb, Amulet** (6 skills): **done** through an incoming attack (`guard.auto_*`, `guard.orb_*`). `FUN_140264340` was disassembled with capstone (a kind 7 object ends at its first update; the whole battery above skips kind 7 because `FUN_14026dcb0` never gives it a contact). **Open**: what the Orb / Amulet use does to the erase (`T`), and what the holder's body does while a Shelter is used.")
    w.append("4. **Effect kinds 17, 20, 22**: kind 22 (Bravery Decision) **done** (`FUN_140270b00` case 0x16 removes every Aura Particle from the undealt deck, order kept); kind 17 (Mind Reading) **done** (it only sets the team-chat listening flag, `FUN_140187180`: no sim change, as ours); kind 20 (Mephisto's Pact) **open** (the program records the deck and enters player state 24, a pick whose handler is unread: `T`).")
    w.append("5. **Course / About Face** (%d skills): still open: a behavioural probe (a shot passing 30 u from the holder is turned / sent back) instead of `all_round()`. About Face / Reverse sending shots back is covered by the battery." % len(ug.get("guard.all_round", [])))
    w.append("6. **Requirement 5 'skills left'**: **done** (`FUN_140262470` case 5; skills left = undealt deck + the player's capsule objects, `DAT_140585c20`): `pay.requirement`.")
    w.append("7. **Recollection / Copy** timing: **done** (`FUN_140261e90` + `FUN_140258180`: the copied skill's cast type and row; with nothing remembered, a refusal and the 'no' sound): `cast.fire_tick` / `hold` / `clip` / `empty_*`.")
    w.append("8. **Own-effect skills' payment**: **done**: movement skills (s43 = 3), instants (motion 4: the snapshot is at the fire tick, before the effect lands s65 ticks later) and travelling skills compare through the plain pay cells; the self effects (s43 = 4, kinds 1 / 2 / 3 / 10) compare the net of payment + effect in `pay.net_level` / `net_aura` / `net_health`.")
    return "\n".join(w)


def footage_text(rows):
    A = _A()
    ug = {k: v[1] for k, v in A.unsettled(rows).items()}
    w = []
    w.append("## What needs Laurence's footage, a live trace or a recorded take of the original")
    w.append("")
    w.append("Everything below could not be settled from the program alone (marked `T` = needs take, or `?`). Tools: the **move recorder** (`engine/re/move_recorder.py`, MOVE-RECORDER.md: scripted input written into the original's memory, any skill in any slot, per-tick trace of positions, states, Level / Health / Aura, sounds and effect requests; unowned skills work) answers anything a per-tick trace shows; `engine/re/live_trace.py` logs the arguments of functions given by Ghidra address (`tools/bats/PLAY_ORIGINAL_TRACE_AURA.bat` is the newest pattern); footage = a recording of the installed original at 30 or 60 fps.")
    w.append("")
    w.append("| what | skills | how to settle it |")
    w.append("|---|---:|---|")
    w.append("| Which way each arc swings (left / right, above / below the line of fire) | %d (Laser, Slide Laser, Twist, Double / Triple / Octolaser, Sonic Boom...) | needs take **plus an object trace**: the recorder logs fighters, not attack objects yet; log the object matrix (`+0xd0..+0x108`) of the objects `FUN_140263a00` makes, or footage cast from behind in an empty arena (PROGRAM-RE.md footage check 2) |" % len(ug.get("flight.swing_side", [])))
    w.append("| (settled live, then fixed in the game) Aura Particle use holds nobody; only sound 89 plays | 1 | MEASURED by moverec; ours was changed to match (DECISIONS 2026-10-01), and the tool now compares what our game does (no longer an unconditional cross) |")
    w.append("| (settled) The 10 instant casts: the program enters no cast state for a row naming clip 0; ours no longer holds them either (no `cast.hold` cross left) | 10 | optional: one take of Jump / Optical Camo would confirm it live (the recorder logs the state every tick) |")
    w.append("| Call: the +10 u in the stored capsule spot; Surprise Attack / Charge: where 'next to a target' is | %d | needs take: positions before / after the cast; for Call the position of the next capsule |" % (len(ug.get("movement.capsule_spot", [])) + len(ug.get("movement.warp_target", []))))
    w.append("| (READ now, no take needed) Automatic Shelter lifetime: the object ends at its first update, one block per use | 4 | `FUN_140264340` disassembled; a take would only confirm it: fire two shells at a Shelter holder in one tick and trace the effect requests (slot 0 / 4 / 5) |")
    w.append("| What an Orb / Amulet use does to an erase attack (negate / reverse), and whether an Orb that cannot pay still works | %d | needs take: hit an Orb holder with Interference at Aura 0 and at Aura 8, trace Level / Aura of both players (`FUN_14026f530` second loop is READ, the effect is not) |" % len(ug.get("guard.orb_effect", [])))
    w.append("| Boost Mine: whose Level rises (the owner or whoever trips it) | 1 | needs take: opponent trips it, watch the Level of both (`0x14024d930` is traced) |")
    w.append("| Whether Recall can put itself back (order of consume and effect at the fire tick) | 1 | needs take: Recall with a known used list; read the deck arrays after |")
    w.append("| Which index the program uses for skills 301-340 (text id 304... or table position 301...) and whether moverec's `owned_skills.csv` is right for them | 40 | needs take: put index 301 in a slot, cast it, see which skill answers (`FUN_140261a00` is traced) |")
    w.append("| STR of the thrown-object skills (rule 21: Power, Psycho Kinesis; the charge skills and Finisher are READ and probed now) | %d | needs take: throw a known object, read the damage |" % len(ug.get("str.damage", [])))
    w.append("| Course / About Face behaviour | 5 | needs take plus an object trace (the shot's path past the holder) |")
    return "\n".join(w)


# The dozen already verified one by one (docs/FOOTAGE.md, PROGRAM-RE.md, FORMATS-SKILLS.md, DECISIONS.md):
# (skill id, [(cell key, value the docs record, where)]). The tool's program value must equal the doc value.
SPOT = [
    (65, [("cast.fire_tick", 26, "FOOTAGE.md claim 1: fires at 13 ticks (26 sim ticks); MEASURED live by moverec: press -> fire 13 ticks"), ("cast.hold", 68, "PROGRAM-RE.md: ATK_LASER 34 frames"), ("cast.air_fire_tick", 40, "PROGRAM-RE.md: the mid-air row fires at tick 20"), ("flight.speed", 12.963, "PROGRAM-RE.md: 12.96 u/tick = 389 u/s"), ("snd.cast", [242], "PROGRAM-RE.md: cast sound 242"), ("snd.travel", 603, "PROGRAM-RE.md: loop 603"), ("snd.impact", 509, "PROGRAM-RE.md: impact 509")]),
    (66, [("cast.fire_tick", 26, "FOOTAGE.md: Slide Laser fires at 13 ticks"), ("flight.speed", 14.815, "PROGRAM-RE.md: 14.8 u/tick = 444 u/s"), ("snd.cast", [243], "PROGRAM-RE.md: cast sound 243")]),
    (80, [("cast.fire_tick", 40, "PROGRAM-RE.md: Blaster fires at tick 20; MEASURED live by moverec: 20"), ("cast.hold", 90, "PROGRAM-RE.md: ATK_SHOT 45 frames"), ("flight.life_ticks", 23, "PROGRAM-RE.md: range 300 u = 23 ticks"), ("flight.homing_deg_per_tick", 1.5, "PROGRAM-RE.md: 1.5 deg a tick"), ("snd.cast", [253], "PROGRAM-RE.md: cast sound 253")]),
    (3, [("melee.reach", 13, "PROGRAM-RE.md: blade 13 u (press -> fire 18 ticks MEASURED live by moverec)"), ("melee.dash_travel", 55, "PROGRAM-RE.md: 8 u / tick, last step cut: ends at 55 of 60 u"), ("cast.hold", 80, "PROGRAM-RE.md: ATK_SORD 40 frames"), ("melee.dash_turn_deg", 1, "PROGRAM-RE.md: 1 deg a tick"), ("snd.cast", [203], "PROGRAM-RE.md: cast sound 203")]),
    (1, [("cast.hold", 80, "PROGRAM-RE.md: ATK_SPHERE 40 frames"), ("melee.reach", 13, "PROGRAM-RE.md: reach 13 u"), ("str.damage", 5, "FORMATS-SKILLS.md: STR = target's Level (the arena's target is Level 5)")]),
    (239, [("movement.apex", 100, "PROGRAM-RE.md: Jump apex 100 u (30 ft)"), ("movement.windup_ticks", 26, "PROGRAM-RE.md: JUMP_START wind-up 13 ticks")]),
    (49, [("effect.k1.status_speed.p1", [(20, None)], "PROGRAM-RE.md: Speed Up +20 % (permanent); press -> fire 20 ticks MEASURED live by moverec")]),
    (0, [("effect.level_plus_1", 11, "P0-SKILLS.md section 1: the Aura Particle is Level +1"), ("cast.hold", 0, "P0-SKILLS.md: record 0 / row 343 is empty: no cast state; MEASURED live by moverec: press -> fire 0 ticks, the fighter keeps running"), ("snd.use", [89], "MEASURED live by moverec: only common/89 at the use")]),
    (26, [("guard.kind", 0, "FORMATS-SKILLS.md: Barrier"), ("guard.size", 13, "PROGRAM-RE.md: 13 u"), ("guard.time", 44, "PROGRAM-RE.md: 22 program ticks"), ("guard.def", 3, "FORMATS-SKILLS.md: DEF 3"), ("guard.anti_pierce", True, "FORMATS-SKILLS.md: Anti-Pierce"), ("cast.fire_tick", 4, "P0-SKILLS.md: defence rows fire at tick 2")]),
    (10, [("flight.speed", 8.333, "PROGRAM-RE.md: Bomb 250 u/s"), ("flight.height", 40, "PROGRAM-RE.md: arc height 40 u"), ("add.blast_radius", 30, "PROGRAM-RE.md: blast 30 u")]),
    (23, [("area.warn", 150, "FORMATS-SKILLS.md: Timer Mine 150 = 5 s (confirmed by its note)"), ("area.radius", 50, "PROGRAM-RE.md: blast radius 50 u"), ("cast.fire_tick", 8, "PROGRAM-RE.md: mines fire at tick 4")]),
]

# a cross on one of these is explained by a decision, not by a disagreement with the docs
EXPLAINED = set()


def _norm(v):
    if isinstance(v, (list, tuple)):
        return [_norm(x) for x in v]
    return v


def spot_check(rows):
    """[(id, name, key, doc, program, ours, status, where, tool_agrees_with_doc, ours_agrees)]"""
    by = {r.id: r for r in rows}
    out = []
    for sid, checks in SPOT:
        r = by.get(sid)
        for key, doc, where in checks:
            cell = next((c for c in (r.cells if r else []) if "%s.%s" % (c.ch, c.key) == key), None)
            if cell is None:
                out.append((sid, r.name if r else "?", key, doc, None, None, "no cell", where, False, False))
                continue
            if isinstance(doc, (list, tuple)):
                prog_ok = _norm(cell.prog) == _norm(doc)
                ours_ok = _norm(cell.ours) == _norm(doc) or cell.st == R.OK
            else:
                prog_ok = R.num_eq(cell.prog, doc, 0.02)
                # ours within the tool's own tolerance of the program value (a 60 Hz jump apex is 98.8 of 100)
                ours_ok = (R.num_eq(cell.ours, doc, 0.02) if cell.ours is not None else False) or cell.st == R.OK
            out.append((sid, r.name, key, doc, cell.prog, cell.ours, cell.st, where, prog_ok, ours_ok))
    return out


def spot_text(rows):
    A = _A()
    res = spot_check(rows)
    w = []
    w.append("## Spot check: the dozen already verified")
    w.append("")
    w.append("The tool must agree with what the docs record for the skills the footage / the reads already settled. 'doc' = the value the docs give; 'program' = what this tool derives; 'ours' = measured from the runtime.")
    w.append("")
    w.append("| skill | check | doc | program (tool) | ours | status | source |")
    w.append("|---|---|---|---|---|---|---|")
    disagree = 0
    for sid, name, key, doc, prog, ours, st, where, prog_ok, ours_ok in res:
        if prog_ok and ours_ok:
            verdict = "agrees"
        elif prog_ok and (sid, key) in EXPLAINED:
            verdict = "agrees with the docs (ours differs: owner decision, now disproved live)"
        else:
            verdict = "**CHECK**"
            disagree += 1
        w.append("| %s (%d) | `%s` | %s | %s | %s | %s %s | %s |" % (name, sid, key, A.fmt_val(doc), A.fmt_val(prog), A.fmt_val(ours), st or "", verdict, where))
    w.append("")
    w.append("Disagreements between the tool and the docs: **%d** of %d checks." % (disagree, len(res)))
    w.append("")
    w.append("Notes on the cells that need one:")
    w.append("- Bomb's arc height: the docs say 40 u (`s66 x 10`); the sampled apex over the spawn is 43.6 u because the program integrates `y += vy; vy -= g` once per tick (`FUN_140265080`), which overshoots the continuous apex by about 2h/T. The tool derives the discrete apex from the program's own formulas, so it agrees with ours (and `flight.height` is the nominal 40).")
    w.append("- Jump: the apex is measured through the real match step (98.8 u of 100: the 60 Hz integration, tolerance 3 %); the wind-up is 26 sim ticks = 13 program ticks.")
    w.append("- Psycho Knife `dash_travel` 55 of 60 u: the last dash step is cut by the program's own quirk (`FUN_14025be40`), so it ends inside the 11 u stop distance; documented in PROGRAM-RE.md, and encoded by the tool.")
    w.append("- Aura Particle `cast.hold` / `snd.use`: the program has no cast state for record 0 (row 343 names clip 0) and plays only common/89; both are compared against what our game does now (a first draft of this tool marked them as crosses unconditionally).")
    return "\n".join(w)


def table_note(P, rows):
    if P.gs is None:
        return "- Table data: `gsdata_en.dat` was not found, so `cache/skills.json` was NOT cross-checked against the game file (every program value here reads it)."
    n_rec = sum(1 for r in rows if r.id and any(c.ch == "table" for c in r.cells))
    bad = sum(1 for r in rows for c in r.cells if c.ch == "table" and c.st == R.BAD)
    return "- Table data: `cache/skills.json` was cross-checked against `gsdata_en.dat` read directly (record s2-s73, action rows, reaction rows) for %d skills: %d differences. The program values in this ledger are derived from that data through the rules cited (program addresses; several re-read from `decomp_all.c` or disassembled with capstone in this lane: STR rules, the Add-effect switch, pay, shell life / update, the aim helper)." % (n_rec, bad)


def write(rows, usage, n_cpu, invalid, ours_doc, P, index_text="", idx=None):
    A = _A()
    STATE["index_ok"] = bool(idx and idx["ok"])
    return A.render_ledger(rows, usage, n_cpu, invalid, ours_doc, lanes_text=lanes_text(rows), footage_text=footage_text(rows), spot_text=spot_text(rows), header_extra=table_note(P, rows), index_text=index_text)
