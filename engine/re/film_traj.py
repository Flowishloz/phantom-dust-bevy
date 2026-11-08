#!/usr/bin/env python3
"""Path check for a film_compare run: does each fighter MOVE like the original did?

`film_compare.py` compares events (fire / hit ticks, Aura, Health, sounds, effects) but not where the
bodies are. A self skill that should float, jump or dash and does not (Demon's Wing stood still while
the original rose 18.5 u and hovered, 2026-10-28) passes it clean. This reads the same original takes
and the `log.csv` of a finished run and compares, per fighter and per original tick:

  * dy: the height change since the start (mirror-proof, the stage heights are the same);
  * dh: the horizontal distance from the start (mirror-proof: the take's z is mirrored in ours).

Warps (Return, Retreat, Surprise Attack, Charge) land on spots our scenario does not know; their dh is
reported like any other and judged by hand. Usage:

  python film_traj.py <compare folder> [--ids 292,239] [--tol 3]
"""
import argparse
import csv
import glob
import math
import os
import sys

HOME = os.path.expanduser("~")
TAKES = os.path.join(HOME, ".pdtools", "moverec", "takes_full")
COUNTDOWN = 180   # our sim ticks before the fight starts (film_compare.COUNTDOWN)
LOG_COLS = "T,tick,p0_health,p0_level,p0_aura,p0_action,p0_skill,p0_x,p0_z,p1_health,p1_level,p1_aura,p1_action,p1_skill,p1_x,p1_z,p0_y,p1_y,p0_yaw,p1_yaw".split(",")


def read_ours(path):
    """tick -> {col: float} for the `T` rows of a run's log.csv."""
    ix = {h: k for k, h in enumerate(LOG_COLS)}
    rows = {}
    for line in open(path, encoding="utf-8", errors="replace"):
        p = line.rstrip("\n").split(",")
        if p[0] == "T" and len(p) > 1 and p[1].isdigit():
            try:
                rows[int(p[1])] = {c: float(p[k]) for c, k in ix.items() if c[0] == "p" and c[2:4] in ("_x", "_y", "_z")}
            except (ValueError, IndexError):
                pass
    return rows


def path_diff(orig, ours, who):
    """(max |dy|, tick, max |dh|, tick) of fighter `who` over the take. `orig`: list of dict rows with
    rec_tick and <who>_x/y/z; `ours`: read_ours()."""
    o0 = orig[0]
    u0 = ours.get(COUNTDOWN) or ours[min(ours)]
    best = [0.0, None, 0.0, None]
    for r in orig:
        t = int(r["rec_tick"])
        u = ours.get(COUNTDOWN + 2 * t)
        if u is None:
            continue
        dy = abs((float(r[who + "_y"]) - float(o0[who + "_y"])) - (u[who + "_y"] - u0[who + "_y"]))
        ho = math.hypot(float(r[who + "_x"]) - float(o0[who + "_x"]), float(r[who + "_z"]) - float(o0[who + "_z"]))
        hu = math.hypot(u[who + "_x"] - u0[who + "_x"], u[who + "_z"] - u0[who + "_z"])
        if dy > best[0]:
            best[0], best[1] = dy, t
        if abs(ho - hu) > best[2]:
            best[2], best[3] = abs(ho - hu), t
    return tuple(best)


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("folder")
    ap.add_argument("--ids", default="")
    ap.add_argument("--tol", type=float, default=3.0)
    a = ap.parse_args(argv)
    work = os.path.join(a.folder, "_work")
    ids = [int(x) for x in a.ids.split(",") if x] or sorted(int(d) for d in os.listdir(work) if d.isdigit())
    bad = 0
    for i in ids:
        hits = glob.glob(os.path.join(TAKES, "%03d_*_trace.csv" % i))
        log = os.path.join(work, "%03d" % i, "log.csv")
        if not hits or not os.path.exists(log):
            continue
        orig = list(csv.DictReader(open(hits[0], encoding="utf-8")))
        ours = read_ours(log)
        if not ours or not orig:
            continue
        res = {w: path_diff(orig, ours, w) for w in ("p0", "p1")}
        flag = any(max(r[0], r[2]) > a.tol for r in res.values())
        bad += flag
        print("%d %s %s%s" % (i, "p0 dy %.1f@%s dh %.1f@%s" % res["p0"], "p1 dy %.1f@%s dh %.1f@%s" % res["p1"], "  <<<" if flag else ""))
    print("paths off by more than %.1f u: %d of %d" % (a.tol, bad, len(ids)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
