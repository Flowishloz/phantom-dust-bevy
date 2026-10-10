#!/usr/bin/env python3
"""One row per recorded take: what its trace shows (our own tool; reads move-recorder output).

  python mr_trace_summary.py TAKES_DIR [OUT.csv] [--list all_skills.csv]

Writes full_trace_summary.csv: press / fire ticks, the Aura / Level / Health of both fighters
before the press, at the fire tick and at the end, the first hit on the opponent, the sounds and
effects the original requested, the state sequence and the outcome. skill_audit_live.py turns the
same rows into live checks against the ledger.
"""
import argparse
import csv
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

COLUMNS = ["rank", "id", "name", "family", "status", "press_tick", "fire_tick", "press_to_fire",
           "cast_state", "cast_state_ticks", "state_seq", "p1_state_seq",
           "p0_aura_before", "p0_aura_at_fire", "p0_level_before", "p0_level_after",
           "p0_health_before", "p0_health_at_fire", "p0_health_end",
           "p1_aura_before", "p1_aura_end", "p1_level_before", "p1_level_end",
           "p1_health_before", "p1_health_end", "hit_tick", "hit_damage",
           "cast_sounds", "sounds", "effects", "fire_index", "index_ok", "opp_press_tick", "opp_fire_tick",
           "notes", "flag"]


def num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def events_of(row):
    return [e for e in row.get("events", "").split("|") if e]


def compress(seq, limit=10):
    out = []
    for s in seq:
        if not out or out[-1] != s:
            out.append(s)
    return ">".join(str(x) for x in out[:limit]) + ("..." if len(out) > limit else "")


def summarize_take(meta, rows, rank_row=None, index=None, opponent=None):
    """A dict of COLUMNS for one take. rows: the trace rows (dicts). The press / fire / index values are
    recomputed from the trace (owner-aware: a scripted opponent's own casts are not the caster's)."""
    import move_recorder as mr
    rank_row = rank_row or {}
    meta = dict(meta)
    if rows and not meta.get("crashed"):
        idx = meta.get("game_index", index)
        opp = bool(rank_row.get("opponent_skill")) if opponent is None else opponent
        meta.update(mr.take_summary(rows, idx, opponent=opp))
    pt, ft = meta.get("press_tick"), meta.get("fire_tick")
    out = {c: "" for c in COLUMNS}
    out.update({"rank": rank_row.get("rank", ""), "id": meta["skill"], "name": meta["name"],
                "family": rank_row.get("family", ""), "flag": rank_row.get("flag", ""),
                "press_tick": "" if pt is None else pt, "fire_tick": "" if ft is None else ft,
                "fire_index": "" if meta.get("fire_index") is None else meta["fire_index"],
                "index_ok": "" if meta.get("index_ok") is None else meta["index_ok"],
                "opp_press_tick": "" if meta.get("opp_press_tick") is None else meta["opp_press_tick"],
                "opp_fire_tick": "" if meta.get("opp_fire_tick") is None else meta["opp_fire_tick"],
                "notes": " | ".join(meta.get("notes", []))})
    from mr_batch import status_of
    out["status"] = status_of(meta)
    if not rows or meta.get("crashed"):
        return out
    by_tick = {int(r["rec_tick"]): r for r in rows}
    last = rows[-1]
    first = rows[0]

    def at(t, key):
        r = by_tick.get(max(0, min(t, int(last["rec_tick"]))))
        return r.get(key, "") if r else ""

    for who in ("p0", "p1"):
        out["%s_level_before" % who] = at(pt - 1 if pt else 0, who + "_level")
        out["%s_health_before" % who] = at(pt - 1 if pt else 0, who + "_health")
        out["%s_aura_before" % who] = at(pt - 1 if pt else 0, who + "_aura")
    out["p0_level_after"] = at((ft + 2) if ft is not None else int(last["rec_tick"]), "p0_level")
    out["p0_aura_at_fire"] = at((ft + 2) if ft is not None else 0, "p0_aura") if ft is not None else ""
    out["p0_health_at_fire"] = at((ft + 2) if ft is not None else 0, "p0_health") if ft is not None else ""
    out["p0_health_end"] = last["p0_health"]
    out["p1_aura_end"], out["p1_level_end"], out["p1_health_end"] = last["p1_aura"], last["p1_level"], last["p1_health"]
    out.pop("p1_health_before", None)
    out["p1_health_before"] = first["p1_health"]
    anchor = pt if pt is not None else ft
    if anchor is not None:
        pt = anchor
        out["cast_state"] = at(pt, "p0_state")
        n = 0
        t = pt
        st = at(pt, "p0_state")
        while by_tick.get(t) and by_tick[t]["p0_state"] == st:
            n += 1
            t += 1
        out["cast_state_ticks"] = n
    if pt is not None and ft is not None:
        out["press_to_fire"] = ft - pt
    # first hit on the opponent: its health dropping
    h0 = num(first["p1_health"])
    for r in rows:
        h = num(r["p1_health"])
        if h is not None and h0 is not None and h < h0:
            out["hit_tick"] = int(r["rec_tick"])
            out["hit_damage"] = int(h0 - h)
            break
    out["state_seq"] = compress(r["p0_state"] for r in rows if int(r["rec_tick"]) >= max(0, (pt or 0) - 1))
    out["p1_state_seq"] = compress(r["p1_state"] for r in rows)
    cast_sounds, sounds, effects, seen_fx = [], {}, [], set()
    for r in rows:
        t = int(r["rec_tick"])
        for e in events_of(r):
            name, _, arg = e.partition(":")
            if name == "sound":
                sounds[arg] = sounds.get(arg, 0) + 1
                if pt is not None and t == pt:
                    cast_sounds.append(arg)
            elif name == "effect" and arg not in seen_fx:
                seen_fx.add(arg)
                effects.append("%s@%d" % (arg, t))
    out["cast_sounds"] = " ".join(cast_sounds)
    out["sounds"] = " ".join("%s x%d" % kv for kv in list(sounds.items())[:40])
    out["effects"] = " ".join(effects[:12])
    return out


def write_summary(takes_dir, out_csv, rows_by_id=None):
    rows_by_id = rows_by_id or {}
    items = []
    for jp in glob.glob(os.path.join(takes_dir, "*.json")):
        try:
            meta = json.load(open(jp, encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if not meta.get("finished"):
            continue
        tp = jp[:-5] + "_trace.csv"
        rows = []
        if os.path.exists(tp) and not meta.get("crashed"):
            with open(tp, newline="") as f:
                rows = list(csv.DictReader(f))
        r = rows_by_id.get(meta["skill"], {})
        items.append((int(r.get("rank") or 9999), meta["skill"], summarize_take(meta, rows, r)))
    items.sort(key=lambda t: (t[0], t[1]))
    os.makedirs(os.path.dirname(os.path.abspath(out_csv)), exist_ok=True)
    with open(out_csv, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=COLUMNS)
        w.writeheader()
        for _r, _i, row in items:
            w.writerow(row)
    return len(items)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("takes_dir")
    ap.add_argument("out_csv", nargs="?", default=os.path.expandvars(r"%USERPROFILE%\.pdtools\moverec\full_trace_summary.csv"))
    ap.add_argument("--list", default=os.path.expandvars(r"%USERPROFILE%\.pdtools\moverec\all_skills.csv"))
    a = ap.parse_args(argv)
    rows_by_id = {}
    if os.path.exists(a.list):
        with open(a.list, newline="", encoding="utf-8") as f:
            rows_by_id = {int(r["id"]): r for r in csv.DictReader(f)}
    print("wrote %d rows -> %s" % (write_summary(a.takes_dir, a.out_csv, rows_by_id), a.out_csv))
    return 0


if __name__ == "__main__":
    sys.exit(main())
