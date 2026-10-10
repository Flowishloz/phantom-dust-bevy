"""Knock-down landing / get-up displacement in the original's own takes (livefix lane, 2026-10-01).

Reads the move recorder's per-tick traces (`takes_full_trace\\<id>_<name>_trace.csv`, engine/docs/MOVE-RECORDER.md)
and, for every take where a fighter is knocked down (state 17 flying / sliding, 15 bound -> lie down, 18 lying,
19 getting up), measures what the BODY does on the floor:

  flight   horizontal distance from the launch to the first tick on the ground
  slide    net / path from the first ground tick to 25 program ticks later (the 0.85 per tick slide plus the
           landing clip's root track; FUN_14025c260 + FUN_140257510)
  getup    net displacement over state 19 (the DOWN_*_STAND clip's root track: 30 frames)
  rise     highest point above the landing floor (the landing class: < 40 u S, 40..80 M, > 80 L; FUN_140254c80)

    python engine/re/knock_live.py [--takes DIR] [--summary CSV]

The victim is P1 (character index 1 = pc01a) in every take; P0 (pc00a) in the few takes where it is knocked down.
Never writes anything.  Findings: engine/docs/MOVE-RECORDER.md "livefix".
"""
import argparse
import csv
import glob
import math
import os

HOME = os.path.expanduser("~")
TAKES = os.path.join(HOME, ".pdtools", "moverec", "takes_full_trace")
SUMMARY = os.path.join(HOME, ".pdtools", "moverec", "full_trace_summary.csv")


def load(path, who):
    rows = list(csv.DictReader(open(path, newline="", encoding="utf-8")))
    return [(float(r[who + "_x"]), float(r[who + "_y"]), float(r[who + "_z"]), r[who + "_state"]) for r in rows]


def measure(P):
    ks = [i for i, p in enumerate(P) if p[3] in ("15", "17", "18")]
    if not ks:
        return None
    k0, k1 = ks[0], ks[-1]
    gy = P[k1][1]
    land = next((i for i in range(k0, k1 + 1) if abs(P[i][1] - gy) < 0.02 and P[i][3] != "0"), None)
    if land is None:
        return None

    def d(a, b):
        return math.hypot(P[b][0] - P[a][0], P[b][2] - P[a][2])

    def path(a, b):
        return sum(math.hypot(P[j + 1][0] - P[j][0], P[j + 1][2] - P[j][2]) for j in range(a, b))

    rise = max(p[1] for p in P[k0:land + 1]) - gy
    end = min(land + 25, len(P) - 1)
    s19 = next((i for i, p in enumerate(P) if p[3] == "19"), None)
    e19 = next((i for i in range(s19, len(P)) if P[i][3] != "19"), len(P) - 1) if s19 is not None else None
    return {
        "flight": d(k0, land), "rise": rise, "slide_net": d(land, end), "slide_path": path(land, end),
        "getup_net": d(s19, e19) if s19 is not None else None,
        "lying_ticks": (s19 - land) if s19 is not None else None,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--takes", default=TAKES)
    ap.add_argument("--summary", default=SUMMARY)
    a = ap.parse_args()
    names = {r["id"]: r["name"] for r in csv.DictReader(open(a.summary, newline="", encoding="utf-8"))}
    print("id name who flight rise class slide_net slide_path getup_net lying_ticks")
    for f in sorted(glob.glob(os.path.join(a.takes, "*_trace.csv"))):
        sid = str(int(os.path.basename(f).split("_")[0]))
        for who in ("p1", "p0"):
            m = measure(load(f, who))
            if not m or m["flight"] < 1:
                continue
            cls = "S" if m["rise"] < 40 else ("M" if m["rise"] <= 80 else "L")
            g = "-" if m["getup_net"] is None else "%.2f" % m["getup_net"]
            print(sid, names.get(sid, "?").replace(" ", "_"), who, "%.1f" % m["flight"], "%.1f" % m["rise"], cls,
                  "%.2f" % m["slide_net"], "%.2f" % m["slide_path"], g, m["lying_ticks"])


if __name__ == "__main__":
    main()
