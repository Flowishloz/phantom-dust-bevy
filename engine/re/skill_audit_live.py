#!/usr/bin/env python3
"""LIVE checks for the skill ledger: what the ORIGINAL did in a recorded take vs the program values
the ledger derives from its tables (our own tool; the probes lane owns skill_audit*.py, this file
only imports skill_audit_rules for its Cell / status types).

  python skill_audit_live.py [--summary full_trace_summary.csv] [--skills skills.json] [--out live.md] [--json live.json]

Input: full_trace_summary.csv (mr_trace_summary.py, one row per take of the move recorder).
Each take gives cells in channel "live":

  fire_tick    press -> fire ticks vs the action row's fire tick (w43; 0 when the row names clip 0)
  cast_sound   the row's cast sound (bank 0) played at the press
  cast_effect  the row's effect requested for the caster at the press
  aura_cost    Aura missing at the fire tick vs the cost rule of chk_pay (slot cost, Level penalty clamp, Retain x2)
  level        Level after the fire vs Level -= s23
  health       Health after the fire vs Health -= s25 (-1 = half, rounded up)
  skill_index  the skill index the program fired is the one asked for
  castable     the press was accepted (a refused press is a hole, not a finding: requirement / cost not met)

A live OK means the original did what the table-derived rule says; a live BAD means the rule (or its
table reading) is wrong for that skill, MEASURED on the original. Values a trace cannot show are not
checked here. For the report: live_section() returns Markdown; skill_audit.py may include it.
"""
import argparse
import csv
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import skill_audit_rules as R  # noqa: E402

DEFAULT_SUMMARY = os.path.expandvars(r"%USERPROFILE%\.pdtools\moverec\full_trace_summary.csv")
DEFAULT_SKILLS = r"..\..\cache\skills.json"


def f(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def i(v, default=None):
    x = f(v)
    return default if x is None else int(round(x))


def live_cell(key, prog, got, src, tol=0.0, note=""):
    return R.cmp("live", key, prog, got, src, tol=tol, note=note, none_ok=False)


def program_cost_drop(rec, aura0, level0):
    """(Aura drop, Level after, own_aura, own_effect) the cost rules predict for a cast at Aura aura0 / Level
    level0, as chk_pay computes them."""
    r = rec["raw"]
    s22, s23, s24 = r[22], r[23], r[24]
    if s23 < 0:
        l1 = 0
        a1 = min(aura0, l1)
    else:
        cost = max(0, min(99, s22))
        l1 = max(0, level0 - s23)
        a1 = min(max(0.0, aura0 - cost), l1)
    drop = aura0 - a1
    if s23 >= 0 and r[41] == 1 and r[26] == 1:
        drop = aura0 - min(max(0.0, aura0 - 2 * cost), l1)       # Retain: the cast is paid again at once
    kinds = r[45:49] if r[43] == 4 else []
    non_attack = rec["capsule"] in ("erase", "status", "special")
    lands_at_fire = r[43] != 0 or r[63] == 4
    own_aura = (non_attack and lands_at_fire and r[51] != 0) or any(k in (2, 9) for k in kinds)
    own_effect = r[51] in (9, 11) or (
        non_attack and lands_at_fire and (r[51] != 0 or r[43] == 3 or any(k in (1, 3, 10) for k in kinds)))
    return drop, l1, own_aura, own_effect


def live_cells(rec, row):
    """Cells (R.Cell) for one skill record `rec` (skills.json entry, or R.Program.aura_particle) and one summary row."""
    cells = []
    status = row.get("status", "")
    if status == "crashed":
        return [R.gap("live", "castable", "the game died twice while this take was recorded")]
    if status == "WRONG-SKILL":
        cells.append(R.Cell("live", "skill_index", R.BAD, "the table index asked for", row.get("fire_index"),
                            "the program fired another skill index than the one put in the slot", "FUN_140262670"))
    if status == "refused" or (row.get("press_tick") in ("", None) and row.get("fire_tick") in ("", None)):
        return cells + [R.gap("live", "castable", "the live press was refused (requirement / cost not met in the setup): %s" % (row.get("notes") or row.get("flag") or "no reason logged"))]
    pt, ft = i(row.get("press_tick")), i(row.get("fire_tick"))
    r = rec["raw"]
    reactive = rec.get("capsule") == "defence"
    if rec.get("synthetic"):
        # Aura Particle: row 343 = WAIT0, fire in the press, Level +1
        if ft is not None:
            cells.append(live_cell("fire_tick", 0, ft - pt, "0x14025a3e0 returns 0 for clip 0: fired in the press"))
        l0, l1 = i(row.get("p0_level_before")), i(row.get("p0_level_after"))
        if l0 is not None and l1 is not None:
            cells.append(live_cell("level", l0 + 1, l1, "FUN_140271590 / FUN_14024d930: Level +1"))
        return cells
    ground = (rec.get("action") or {}).get("ground")
    air = (rec.get("action") or {}).get("air")
    row_a = ground or air
    if ft is None:
        return cells + [R.gap("live", "fire_tick", "the cast was accepted but no fire event was seen in the take")]
    if pt is None:
        pt = ft                         # a reactive skill has no press of its own: the fire is the reference
    # --- fire tick
    if reactive:
        opp = i(row.get("opp_fire_tick"))
        cells.append(R.unk("live", "fire_tick", "a guard fires when the attack arrives (no accepted press of its own); the delay after the attacker's fire is not derived from the table",
                           None, "%d ticks after the attacker's fire" % (ft - opp) if opp is not None else "attacker fire not seen", "FUN_14025a3e0"))
    elif r[43] == 6:
        cells.append(R.unk("live", "fire_tick", "Recollection / Copy resolve at the press as the copied skill", None, ft - pt, "FUN_140258180"))
    elif row_a is not None and row_a.get("charge_cycle", 0) > 0:
        cells.append(R.unk("live", "fire_tick", "hold-to-charge: the take presses once, the release is not scripted", None, ft - pt, "FUN_14025c720"))
    else:
        # A clip-0 row fires in the press (FUN_14025a3e0 returns 0). Any other row sets the countdown (+0x64) to w43 at the
        # press and FUN_14025c720 fires it in the NEXT tick's state update at the earliest, so w43 = 0 fires one program
        # tick after the press, like w43 = 1 (livefix lane, LIVE Dash: press 39, fire 40).
        want = 0 if (row_a is None or row_a.get("anim_id") == 0 or row_a.get("anim_name") == "WAIT0") else max(row_a["fire_tick"], 1)
        cells.append(live_cell("fire_tick", want, ft - pt, "FUN_14025a3e0 / FUN_14025c720: action row w43 (program ticks), at least 1 for a real clip"))
    # --- cast sound / effect at the press
    if row_a is not None:
        snd = row_a.get("cast_sound", 0)
        got = (row.get("cast_sounds") or "").split()
        if reactive:
            got = re.findall(r"(\d+\.\d+) x\d+", row.get("sounds") or "") or got
        auto_shelter = reactive and R.passive_kind(rec) == "automatic Shelter (s44 = 7)"
        if auto_shelter and snd > 0:
            # An automatic Shelter puts itself up through FUN_14026f530 -> FUN_1402626d0, which starts the clip directly; the row's
            # cast sound (and voice) are played only by FUN_14025a3e0 (the press path). LIVE: Mind's Eye, Guard of Water, Auto
            # Shell, Guardian Angel never play it, the other 39 (pressed) guards do. The program value is therefore "none".
            ok = "0.%d" % snd not in got
            cells.append(R.Cell("live", "cast_sound", R.OK if ok else R.BAD, "none (auto path)", " ".join(got), "no row cast sound for an automatic Shelter", "FUN_14026f530 -> FUN_1402626d0 (no FUN_140276330 for row w3)"))
        elif snd > 0:
            ok = "0.%d" % snd in got
            cells.append(R.Cell("live", "cast_sound", R.OK if ok else R.BAD, "0.%d" % snd, " ".join(got), "bank 0 id at the press tick", "FUN_14025a3e0 -> FUN_140276330"))
        eff = row_a.get("effect", 0)
        if eff > 0:
            fx = (row.get("effects") or "").split()
            ok = any(e.startswith("0.%d@" % eff) and abs(int(e.split("@")[1]) - pt) <= 1 for e in fx)
            cells.append(R.Cell("live", "cast_effect", R.OK if ok else R.BAD, "0.%d" % eff, " ".join(fx[:4]), "effect requested for the caster at the press", "FUN_14025a3e0 -> FUN_140259140 -> FUN_14018d2f0"))
    # --- pay: Aura, Level, Health at the fire tick
    a0, a1 = f(row.get("p0_aura_before")), f(row.get("p0_aura_at_fire"))
    l0, l1 = i(row.get("p0_level_before")), i(row.get("p0_level_after"))
    h0, h1 = i(row.get("p0_health_before")), i(row.get("p0_health_at_fire"))
    if None not in (a0, a1, l0):
        drop, want_l1, own_aura, own_effect = program_cost_drop(rec, a0, l0)
        if own_aura:
            cells.append(R.gap("live", "aura_cost", "the skill's own effect moves the caster's Aura: not separable"))
        else:
            cells.append(live_cell("aura_cost", round(drop, 2), round(a0 - a1, 2), "FUN_1402621f0 / FUN_1402623b0 (slot cost s22, Level penalty clamp)", tol=0.1))
        if not own_effect and l1 is not None:
            cells.append(live_cell("level", want_l1, l1, "FUN_1402621f0: Level -= s23"))
        if not own_effect and h0 is not None and h1 is not None:
            s25 = r[25]
            want_h = (h0 - (h0 + 1) // 2) if s25 == -1 else h0 - s25
            cells.append(live_cell("health", max(want_h, 0), h1, "FUN_1402621f0: Health -= s25 (-1 = half, rounded up)"))
    return cells


def load_summary(path=DEFAULT_SUMMARY):
    with open(path, newline="", encoding="utf-8") as fh:
        return list(csv.DictReader(fh))


def audit_live(summary_rows, skills_doc):
    """[(row, rec, cells)] for every summary row (rank order)."""
    prog = R.Program(skills_doc)
    out = []
    for row in sorted(summary_rows, key=lambda r: (i(r.get("rank"), 9999), i(r.get("id"), 0))):
        sid = i(row["id"])
        rec = prog.aura_particle if sid == 0 else prog.rec.get(sid)
        if rec is None:
            continue
        out.append((row, rec, live_cells(rec, row)))
    return out


def tally(results):
    t = {R.OK: 0, R.BAD: 0, R.UNK: 0, R.GAP: 0, R.TAKE: 0}
    for _row, _rec, cells in results:
        for c in cells:
            t[c.st] = t.get(c.st, 0) + 1
    return t


def mismatch_classes(results):
    """{class key: [(rank, name, cell)]} for the BAD cells."""
    out = {}
    for row, rec, cells in results:
        for c in cells:
            if c.st == R.BAD:
                out.setdefault(c.key, []).append((i(row.get("rank"), 9999), rec["name"], c))
    return out


def live_section(summary_path=DEFAULT_SUMMARY, skills_path=DEFAULT_SKILLS, results=None):
    """Markdown for the ledger report: counts, the mismatch classes and every live cross."""
    if results is None:
        results = audit_live(load_summary(summary_path), json.load(open(skills_path, encoding="utf-8")))
    t = tally(results)
    skills_bad = sum(1 for _r, _c, cells in results if any(c.st == R.BAD for c in cells))
    skills_ok = sum(1 for _r, _c, cells in results if cells and all(c.st in (R.OK, R.GAP, R.UNK) for c in cells) and any(c.st == R.OK for c in cells))
    w = ["## LIVE checks: the original's own takes (move recorder)", "",
         "Source: `full_trace_summary.csv` (one take per skill on the original PC game, versus match, caster at Level 12 unless its requirement says otherwise; "
         "engine/docs/MOVE-RECORDER.md). A live %s means the original did what the table-derived rule says; a live %s is a MEASURED mismatch with the rule or its table reading. "
         "Only values a trace shows are checked: fire tick, cast sound / effect at the press, Aura / Level / Health paid at the fire." % (R.OK, R.BAD), "",
         "- Takes: **%d**; skills with a live cross: **%d**; with every live value matching: **%d**." % (len(results), skills_bad, skills_ok),
         "- Cells: %d %s, %d %s, %d unconfirmed, %d holes (refused press, charge, copy, own-effect skills)." % (t[R.OK], R.OK, t[R.BAD], R.BAD, t[R.UNK], t[R.GAP]), ""]
    classes = mismatch_classes(results)
    if classes:
        w += ["### Live mismatch classes", "", "| class | skills | first skills (by usage rank) |", "|---|---:|---|"]
        for key, items in sorted(classes.items(), key=lambda kv: -len(kv[1])):
            items.sort()
            w.append("| `live.%s` | %d | %s |" % (key, len(items), ", ".join("%s (#%d)" % (n, r) for r, n, _c in items[:5])))
        w += ["", "### Every live cross", "", "| # | skill | check | program (table rule) | live | note |", "|---:|---|---|---|---|---|"]
        rows = [(r, n, c) for items in classes.values() for r, n, c in items]
        for r, n, c in sorted(rows, key=lambda t: (t[0], t[2].key)):
            w.append("| %d | %s | `%s` | %s | %s | %s |" % (r, n, c.key, c.prog, c.ours, (c.note or c.src).replace("|", "/")))
    return "\n".join(w)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--summary", default=DEFAULT_SUMMARY)
    ap.add_argument("--skills", default=DEFAULT_SKILLS)
    ap.add_argument("--out", help="write the Markdown section here")
    ap.add_argument("--json", help="write every live cell here")
    a = ap.parse_args(argv)
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass
    results = audit_live(load_summary(a.summary), json.load(open(a.skills, encoding="utf-8")))
    md = live_section(results=results)
    if a.out:
        with open(a.out, "w", encoding="utf-8") as fh:
            fh.write(md + "\n")
    else:
        print(md)
    if a.json:
        with open(a.json, "w", encoding="utf-8") as fh:
            json.dump([{"id": r["id"], "name": r["name"], "rank": r.get("rank"), "cells": [c.as_dict() for c in cells]}
                       for r, _rec, cells in results], fh, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
