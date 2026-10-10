"""The guard battery, the automatic Shelters / Orb / Amulet and the other probes of lane `probes` (wave 6).

Program side of what `crates/pd_game/src/combat/skill_audit.rs` observes through `Combat::strike` and the
incoming-attack scenarios. Every rule is READ from `%USERPROFILE%/.pdtools/re-out/decomp_all.c` (addresses
in the comments). `FUN_140264340` (the guard object's update) is not in the Ghidra export (Ghidra never made it a function; it
sits between `FUN_1402642f0` and `FUN_140264570`): it and the Add-effect argument of `FUN_140271ed0` case 2 were read by
disassembling PDUWP.exe with capstone in a scratch venv (`exe_const.va2f` gives the file offset of an address).

Assumption throughout: the battle-mode field `*(*(game + 0x61a0) + 0x10)` is 0, as in a versus match (`FUN_14026dcb0` and
`FUN_140271760` simplify every guard to a sphere when it is 1).

    FUN_140271760  guard resolution (attack object p1 meets guard object p2): the order below
    FUN_14026dcb0  contact test: kinds 0 / 3 / 4 a disc in front, 1 / 2 / 5 / 6 a sphere, 7 never
                   (`uVar4 < 7` falls through to "no contact")
    FUN_1402723b0  an attack's s54, read as 0 under status 31 (Necronomicon), -1 for a non-object
    FUN_140167910  short range = s43 0 and motion 3 / 12 / 13 / 14
    FUN_14026cd70  case 7 (brush): motion 3 / 12 / 13 stun the attacker (reaction 26), anything else is
                   brushed aside; case 5 = turned back; case 3 = erased; case 9 = the guard lingers 5 ticks
    FUN_140271ed0  the guard's Add effect (s51 / s52)
    FUN_14026f530  the automatic Shelter / Orb / Amulet trigger
    FUN_140264340  the guard object's update: a kind 7 object ends at its FIRST update
"""

import skill_audit_rules as R

SHORT = (3, 12, 13, 14)  # FUN_140167910
BRUSH_STUN = (3, 12, 13)  # FUN_14026cd70 case 7
AREA_LIST = (7, 10, 20)  # Rain column / Tornado / Rain cloud: Barrier and Reflect never stop them
BOTH_LISTS = (7, 10, 8, 9, 20)  # + mines and around-self: Absorb and Brush never stop them
REVERSE_MOTIONS = (0, 1, 2, 5, 6, 11)  # About Face / Reverse turn these back

GROUPS = {
    "bat_block": "whether the guard stops the attack, breaks, how much DEF it keeps, lingers, leaves its button (FUN_140271760)",
    "bat_damage": "what reaches the body: all of it, nothing, or STR - DEF for Pierce through a broken wall (+ Digestion's Health)",
    "bat_react": "what happens to the attack and the attacker: sent back, Brush stun",
    "bat_add": "the guard's Add effect (FUN_140271ed0)",
    # one cell per cause that the battery found, so a fix lane owns each:
    "bat_blast": "a shot's blast (the program's object state 3, FUN_14026dbd0) meeting a guard: Absorb, Brush and About Face let it pass (return 0), a Reflect does not send it back, a capture / Add effect needs a stopped attack",
    "bat_brush_pursuit": "FUN_14026cd70 case 7 stuns the attacker only for motion 3 / 12 / 13 (a Pursuit, motion 14, is brushed aside instead)",
    "bat_aura_gain": "Add 2 (Absorb Energy): FUN_140271ed0 case 2 = FUN_1402618e0(holder, attack STR): the holder gains the attack's STR in Aura (the disassembly reads [attack + 0x60]), not its Aura cost",
}


def prog_case(G, A, S, side, necro=False, berserk=False):
    """The program's outcome of attack `A` of STR `S` meeting guard `G` standing at its holder.
    side 0 front, 1 back, 2 from above / below. Returns a dict, or None when the geometry is not read."""
    k, m, st3 = G["kind"], A["motion"], A["state3"]
    t = 0 if necro else A["s54"]  # FUN_1402723b0
    short = m in SHORT
    o = dict(blocked=False, turned=False, broken=False, landed_full=True, leftover=0, def_after=G["def"], lingering=False,
             reflect=False, stun=False, add=False, slot_erased=False)

    def through():
        return o

    # contact (FUN_14026dcb0): a disc in front for 0 / 3 / 4: an attack from behind never crosses its plane
    disc = k in (0, 3, 4)
    if disc and side == 1:
        return through()
    if disc and side == 2 and m not in AREA_LIST:
        return None  # a Hole under a disc kind: the contact segment of FUN_14026c5b0 is not read
    # FUN_140271760, in the program's order
    if k in (0, 1, 3, 4, 5) and not necro and A["s54"] == 2:
        return through()  # s54 = 2 penetrates defensive skills
    if k in (0, 4) and m in AREA_LIST:
        return through()
    if k == 5 and (st3 or short or m in BOTH_LISTS):
        return through()
    if k == 3 and (st3 or m in BOTH_LISTS):
        return through()
    if k == 6:
        if t == 3 or st3 or m not in REVERSE_MOTIONS:
            return through()
        o.update(blocked=True, turned=True, landed_full=False)  # no DEF spent, nothing lingers, no Add effect
        return o
    if k == 2:
        return through()  # [Course] only flags the attack (0x100); it stops nothing
    left = G["def"] - S
    if left < 0:
        o.update(blocked=True, broken=True, def_after=None, slot_erased=True, landed_full=False)
        pierce_through = (berserk or t == 1) and not G["anti"]
        if pierce_through:
            o["leftover"] = S - G["def"]
        o["stun"] = k == 3 and m in BRUSH_STUN
    else:
        o.update(blocked=True, def_after=left, lingering=True, landed_full=False)
        plain = t == 3 or st3
        o["reflect"] = k == 4 and not plain and m in (0, 6)
        o["stun"] = k == 3 and not plain and m in BRUSH_STUN
    o["add"] = m != 18 and (not G["short_only"] or short)
    return o


def _label(atk, side, S):
    return "%s %s STR %d" % (atk["label"], ("front", "back", "above")[side], S)


def _fmt(d):
    return ", ".join("%s=%s" % (k, v) for k, v in d.items())


def chk_guard_battery(P, rec, O):
    gb = O.get("guardbat")
    if not gb or gb.get("automatic") or "cases" not in gb:
        return []
    r = rec["raw"]
    G = dict(kind=r[44], anti=(r[49] == 1), short_only=(r[50] == 1), add=r[51], add_amt=r[52])
    G["def"] = r[47]
    protected = r[41] != 0  # Retain / can't be erased: the slot rules differ
    atks = gb["attackers"]
    src = "FUN_140271760 / FUN_14026dcb0 / FUN_140271ed0"
    bad = {g: [] for g in GROUPS}
    n = {g: 0 for g in GROUPS}
    for ai, S, side, obs in gb["cases"]:
        a = atks[ai]
        arec = P.rec.get(a["id"])
        A = dict(motion=arec["raw"][63], s54=arec["raw"][54], state3=a["state3"], label=a["label"], colour=arec["capsule"],
                 retained=arec["raw"][41] != 0, cost=arec.get("cost", 0))
        shot_blast = a["state3"] and a.get("of_shot", a["arrival"] == "Shot")  # the blast of a shot (ours: Arrival::Blast since lane guardfix)
        if "err" in obs:
            bad["bat_block"].append("%s: probe error %s" % (_label(a, side, S), obs["err"]))
            continue
        p = prog_case(G, A, S, side)
        if p is None:
            continue  # the contact geometry of a Hole against a disc guard is not read (stated in the ledger)
        lab = _label(a, side, S)
        # each check fills {field: (program, ours)}; a mismatch goes into the cell of its cause
        found = {"bat_block": {}, "bat_damage": {}, "bat_react": {}, "bat_add": {}}
        # -- block
        if obs["blocked"] != p["blocked"]:
            found["bat_block"]["blocked"] = (p["blocked"], obs["blocked"])
        elif p["blocked"]:
            if obs["broken"] != p["broken"]:
                found["bat_block"]["broken"] = (p["broken"], obs["broken"])
            if p["broken"]:
                if obs["guard"] is not None:
                    found["bat_block"]["guard_removed"] = (True, False)
                if not protected and p["slot_erased"] and obs["slot0"] is not None:
                    found["bat_block"]["guard_leaves_button"] = (True, False)
            else:
                g_ = obs["guard"] or {}
                if g_.get("def") != p["def_after"]:
                    found["bat_block"]["def_left"] = (p["def_after"], g_.get("def"))
                if g_.get("lingering", False) != p["lingering"]:
                    found["bat_block"]["lingers"] = (p["lingering"], g_.get("lingering"))
        # -- damage to the body
        dmg = (S if p["landed_full"] else p["leftover"]) if A["colour"] == "attack" else 0
        heal = S if (G["add"] == 1 and p["add"] and p["blocked"] and not p["turned"]) else 0
        if obs["vhealth"] != -dmg + heal:
            found["bat_damage"]["health_change"] = (-dmg + heal, obs["vhealth"])
        # -- sent back / stun
        want_back = 1 if (p["reflect"] or p["turned"]) else 0
        if obs["reflected"] != want_back:
            found["bat_react"]["sent_back"] = (want_back, obs["reflected"])
        if obs["stunned"] != p["stun"]:
            found["bat_react"]["attacker_stunned"] = (p["stun"], obs["stunned"])
        # -- Add effect
        if G["add"]:
            on = p["blocked"] and not p["turned"] and p["add"]
            c, amt = G["add"], G["add_amt"]
            if c == 2:
                want_a = min(20.0, 8.0 + S) - 8.0 if on else 0.0  # FUN_1402618e0(holder, attack STR): + STR, clamped to Level 20
                if abs(obs["vaura"] - want_a) > 0.05:
                    found["bat_add"]["holder_aura"] = (want_a, obs["vaura"])
            elif c == 3:
                want_e = on and A["colour"] == "attack" and not A["retained"]
                got_e = obs["a_slot0"] is None
                if got_e != want_e:
                    found["bat_add"]["attacker_button_erased"] = (want_e, got_e)
            elif c in (4, 5, 6):
                key = {4: "a_bound", 5: "a_frozen", 6: "a_poison"}[c]
                want_t = 2 * amt if on else 0
                if obs[key] != want_t:
                    found["bat_add"][key] = (want_t, obs[key])
            elif c == 7:
                want_c = on and not p["broken"]
                got_c = obs["slot0"] == a["id"]
                if got_c != want_c:
                    found["bat_add"]["captured_on_guard_button"] = (want_c, got_c)

        def note(d):
            return "%s: %s" % (lab, "; ".join("%s program %s ours %s" % (k, v[0], v[1]) for k, v in d.items()))

        # -- count and route by cause
        if shot_blast:
            n["bat_blast"] += 1
            allf = {k: v for d in found.values() for k, v in d.items()}
            if allf:
                bad["bat_blast"].append(note(allf))
            continue
        pursuit_brush = G["kind"] == 3 and A["motion"] == 14
        if pursuit_brush:
            n["bat_brush_pursuit"] += 1
            st = found["bat_react"].pop("attacker_stunned", None)
            if st:
                bad["bat_brush_pursuit"].append(note({"attacker_stunned": st}))
        aura_gain = G["add"] == 2
        if aura_gain:
            n["bat_aura_gain"] += 1
            ha = found["bat_add"].pop("holder_aura", None)
            if ha:
                bad["bat_aura_gain"].append(note({"holder_aura": ha}))
        for g in ("bat_block", "bat_damage", "bat_react"):
            n[g] += 1
            if found[g]:
                bad[g].append(note(found[g]))
        if G["add"] and not aura_gain:
            n["bat_add"] += 1
            if found["bat_add"]:
                bad["bat_add"].append(note(found["bat_add"]))
    cells = []
    for g, why in GROUPS.items():
        if n[g] == 0:
            continue
        if bad[g]:
            sample = "; ".join(bad[g][:3]) + (" ... (%d of %d cases differ)" % (len(bad[g]), n[g]) if len(bad[g]) > 3 else " (%d of %d cases differ)" % (len(bad[g]), n[g]))
            cells.append(R.Cell("guard", g, R.BAD, "%d cases as the rule reads" % n[g], "%d differ" % len(bad[g]), sample, src + ": " + why))
        else:
            cells.append(R.Cell("guard", g, R.OK, "%d cases" % n[g], "%d cases" % n[g], "", src + ": " + why))
    return cells


# ---------------------------------------------------------------------------
# Automatic Shelters (FUN_14026f530 -> FUN_1402626d0 -> FUN_140271760), Orb and Amulet


def chk_auto(P, rec, O):
    a = O.get("auto")
    if not a:
        return []
    r = rec["raw"]
    cost, hcost, uses, DEF = r[22], r[25], r[26], r[47]
    unlimited = uses == 0 and r[41] == 0
    src = "FUN_14026f530 (trigger) -> FUN_1402626d0 (use: pay, consume) -> FUN_140271760 (resolve)"
    cells = []
    runs = a["runs"]
    # first strike: blocked, paid, consumed
    bad = []
    for run in runs:
        f = run["first"]
        want = dict(blocked=True, vhealth=-hcost, vaura=-float(cost), slot_kept=unlimited)
        got = dict(blocked=f["blocked"], vhealth=f["vhealth"], vaura=round(f["vaura"], 2), slot_kept=f["slot0"] == rec["id"])
        if not (want["blocked"] == got["blocked"] and want["vhealth"] == got["vhealth"] and abs(want["vaura"] - got["vaura"]) < 0.05 and want["slot_kept"] == got["slot_kept"]):
            bad.append("gap %d: program %s ours %s" % (run["gap"], _fmt(want), _fmt(got)))
            break
    cells.append(R.Cell("guard", "auto_first", R.BAD if bad else R.OK, "blocked, paid Aura %d / Health %d, %s" % (cost, hcost, "kept" if unlimited else "used up"), "; ".join(bad) or "as read", "", src))
    # the guard object stands exactly until its first update (a program tick = 2 sim ticks)
    up = [run["up_for"] for run in runs if run["up_for"] is not None]
    ours_up = max(up) if up else None
    ok = ours_up is not None and ours_up <= 2
    cells.append(R.Cell("guard", "auto_life", R.OK if ok else R.BAD, "<= 2 sim ticks (a kind 7 object ends at its first update)", ours_up,
                        "ours keeps the guard standing %s sim ticks and blocks everything meanwhile" % ours_up, "FUN_140264340 (disassembled): countdown == 0 or s43 == 2 and s44 == 7 -> FUN_14026cd70(obj, 0); FUN_14026dcb0: kind 7 is never in contact"))
    # a second attack after the first
    bad = []
    for run in runs:
        s2 = run["second"]
        want = unlimited
        if s2["blocked"] != want:
            bad.append("%d ticks later: program %s ours %s" % (run["gap"], "blocked again (a fresh Shelter is used)" if want else "lands on the body (the skill was used up)", "blocked" if s2["blocked"] else "lands"))
    cells.append(R.Cell("guard", "auto_second", R.BAD if bad else R.OK, "blocked again" if unlimited else "lands", "; ".join(bad) or "as read", "each trigger is a fresh use of the slot; the object lives one tick, so a standing guard never blocks the next attack", src))
    # what the two attacks cost together
    bad = []
    for run in runs:
        s2 = run["second"]
        n_uses = 1 + (1 if (unlimited and s2["blocked"] or (not unlimited and False)) else 0)
        want_h = -hcost * n_uses - (0 if (unlimited or s2["blocked"]) else 1)
        got_h = run["after"]["health"] - run["before"]["health"]
        want_a = -float(cost) * n_uses
        got_a = run["after"]["aura"] - run["before"]["aura"]
        # the Aura refills while the gap runs (up to ~0.5 point in 40 sim ticks): compare it only for the short gaps
        if got_h != want_h or (run["gap"] <= 12 and abs(got_a - want_a) > 0.2):
            bad.append("%d ticks apart: program Health %+d Aura %+.0f, ours Health %+d Aura %+.2f" % (run["gap"], want_h, want_a, got_h, got_a))
    cells.append(R.Cell("guard", "auto_pay", R.BAD if bad else R.OK, "%d Aura + %d Health per use" % (cost, hcost), "; ".join(bad[:2]) or "as read", "", "FUN_1402621f0 via FUN_1402626d0 at every trigger"))
    # a strong hit breaks it
    s = a["strong"]["first"]
    ok = s["blocked"] and s["broken"]
    cells.append(R.Cell("guard", "auto_break", R.OK if ok else R.BAD, "STR %d > DEF %d breaks it" % (a["strong"]["strength"], DEF), "blocked %s broken %s" % (s["blocked"], s["broken"]), "", "FUN_140271760 generic path"))
    return cells


def chk_orb(P, rec, O):
    o = O.get("orb")
    if not o:
        return []
    cost = rec["raw"][22]
    cells = []
    src = "FUN_14026f530 (second loop: an erase-colour attack object s9 = 3 hits the holder of an s37 = 1 skill) -> FUN_1402626d0 -> FUN_1402621f0 (pay)"
    bad = []
    for run in o["runs"]:
        affordable = run["aura"] >= cost
        want = -float(cost) if affordable else 0.0
        got = run["after"]["aura"] - run["before"]["aura"]
        if abs(got - want) > 0.2:
            bad.append("holder Aura %d: program %+.0f, ours %+.2f" % (run["aura"], want, got))
    cells.append(R.Cell("guard", "orb_pay", R.BAD if bad else R.OK, "the holder pays the skill's cost (%d) when an erase attack hits and can afford it" % cost, "; ".join(bad) or "as read", "ours: Orb / Amulet are free passive checks inside the erase effect", src))
    cells.append(R.Cell("guard", "orb_effect", R.TAKE, None, [(run["aura"], run["after"]["level"] - run["before"]["level"]) for run in o["runs"]],
                        "needs take: READ that the trigger uses the skill; the code that makes the erase not land (Orb) or reverse (Amulet) is not located (FUN_140272510 / FUN_140272bc0 read no mark of it); ours negates / bounces for free", src))
    return cells
