#!/usr/bin/env python3
"""Compare an ORIGINAL golden trace with a trace from OUR sim (stdlib only, Python 3.8+).

  python3 trace_compare.py ORIG.csv OURS.csv [--scenario ID] [--step 5] [--out report.txt]
                           [--hz 30] [--run-speed 50] [--strict] [--tol name=value ...]

Format: engine/docs/GOLDEN-TRACE.md. Missing columns are tolerated: any metric
that needs one prints SKIP. Exit code 1 only with --strict and at least one FAIL.
"""
import argparse
import csv
import math
import sys

ALIASES = {
    "tick": ["tick", "t", "frame"], "scenario_id": ["scenario_id", "scenario", "sid"],
    "stick_x": ["stick_x", "sx", "lx"], "stick_y": ["stick_y", "sy", "ly"],
    "buttons": ["buttons", "btn"], "pos_x": ["pos_x", "x"], "pos_y": ["pos_y", "y"], "pos_z": ["pos_z", "z"],
    "facing": ["facing"], "heading": ["heading"], "vertical_speed": ["vertical_speed", "vspeed", "vy"],
    "state": ["state"], "clip_id": ["clip_id", "clip"], "clip_time": ["clip_time"],
    "lock_active": ["lock_active", "locked"], "lock_band": ["lock_band", "band"],
    "speed_status": ["speed_status"], "grounded": ["grounded"],
}
B_JUMP, B_DODGE = 1, 4
FREE_STATES = (0, 1, 8)
# name: (absolute tolerance, relative tolerance)
TOL = {
    "speed_mean_err": (3.0, 0), "speed_max_err": (8.0, 0), "first_divergence": (4.0, 0),
    "distance": (2.0, 0.03), "steady_speed": (1.5, 0.05), "time_to_run": (2, 0), "time_to_stop": (2, 0),
    "max_turn_rate": (3.0, 0), "facing_settle": (2, 0), "clip_switches": (1, 0),
    "strafe_flips": (1, 0), "takeoff_delay": (1, 0), "apex": (1.0, 0.05), "airtime": (1, 0),
    "busy_ticks": (2, 0), "dodge_distance": (3.0, 0.05), "dodge_peak_speed": (4.0, 0.08),
}


def num(s):
    if s is None:
        return None
    s = str(s).strip()
    if s == "" or s.lower() in ("nan", "none", "null", "-"):
        return None
    try:
        return float(s)
    except ValueError:
        return s  # keep strings (e.g. clip names)


class Trace:
    def __init__(self, path):
        self.path = path
        self.meta = {}
        lines = []
        with open(path, newline="") as f:
            for line in f:
                if line.startswith("#"):
                    k, _, v = line[1:].partition(":")
                    self.meta[k.strip()] = v.strip()
                elif line.strip():
                    lines.append(line)
        reader = csv.DictReader(lines)
        names = [(n or "").strip().lower() for n in (reader.fieldnames or [])]
        remap = {}
        for canon, al in ALIASES.items():
            for a in al:
                if a in names and canon not in remap.values():
                    remap[reader.fieldnames[names.index(a)]] = canon
                    break
        self.columns = set(remap.values())
        self.scen = {}
        order = []
        for i, r in enumerate(reader):
            row = {remap[k]: num(v) for k, v in r.items() if k in remap}
            sid = row.get("scenario_id")
            sid = str(sid) if sid is not None else self.meta.get("scenario", "S00")
            if sid not in self.scen:
                self.scen[sid] = []
                order.append(sid)
            row.setdefault("tick", float(i))
            self.scen[sid].append(row)
        self.order = order


def col(rows, name):
    return [r.get(name) for r in rows]


def has(rows, *names):
    return all(any(r.get(n) is not None for r in rows) for n in names)


def wrap(a):
    return (a + 180.0) % 360.0 - 180.0


def speeds(rows, hz):
    """Horizontal speed in u/s from position differences (x/z are the ground plane)."""
    if not has(rows, "pos_x", "pos_z"):
        return None
    out = [0.0]
    for a, b in zip(rows, rows[1:]):
        try:
            out.append(math.hypot(b["pos_x"] - a["pos_x"], b["pos_z"] - a["pos_z"]) * hz)
        except TypeError:
            out.append(None)
    return out


def stick_info(rows):
    mags = [math.hypot(r.get("stick_x") or 0, r.get("stick_y") or 0) for r in rows]
    if not has(rows, "stick_x", "stick_y"):
        return None, None, None
    start = next((i for i, m in enumerate(mags) if m > 0.05), None)
    release = None
    if start is not None:
        release = next((i for i in range(start, len(mags)) if mags[i] <= 0.05), None)
    return mags, start, release


def first_press(rows, mask):
    if not has(rows, "buttons"):
        return None
    for i, r in enumerate(rows):
        b = r.get("buttons")
        if b is not None and int(b) & mask:
            return i
    return None


class Result:
    def __init__(self, name, o, t, verdict, note=""):
        self.name, self.o, self.t, self.verdict, self.note = name, o, t, verdict, note


def fmt(v):
    if v is None:
        return "-"
    if isinstance(v, list):
        return "[" + ",".join(str(x) for x in v[:12]) + (",.." if len(v) > 12 else "") + "]"
    if isinstance(v, float):
        return "%.2f" % v
    return str(v)


def cmpnum(name, o, t, tols, note=""):
    if o is None and t is None:
        return Result(name, o, t, "SKIP", note or "not measurable in either trace")
    if o is None or t is None:
        return Result(name, o, t, "FAIL", "only one trace has a value" + (" (" + note + ")" if note else ""))
    a, r = tols[name]
    err = t - o
    ok = abs(err) <= max(a, r * abs(o))
    return Result(name, o, t, "PASS" if ok else "FAIL", "err %+.2f tol %.2f %s" % (err, max(a, r * abs(o)), note))


def skip(name, why):
    return Result(name, None, None, "SKIP", why)


def time_to_thresh(sp, start, thresh):
    for i in range(start, len(sp)):
        if sp[i] is not None and sp[i] >= thresh:
            return i - start
    return None


def clip_switches(rows):
    c = col(rows, "clip_id")
    return [i for i in range(1, len(c)) if c[i] is not None and c[i - 1] is not None and c[i] != c[i - 1]]


def strafe_flips(rows):
    bands = [int(b) for b in col(rows, "lock_band") if b is not None]
    side = {2: 1, 4: 1, 3: -1, 5: -1}
    seq = [side[b] for b in bands if b in side]
    return sum(1 for a, b in zip(seq, seq[1:]) if a != b)


def facing_settle(rows):
    """After the last >60 deg stick change (free camera), ticks until facing is within 10 deg of the stick."""
    if not has(rows, "stick_x", "stick_y", "facing"):
        return None
    angs = []
    for i, r in enumerate(rows):
        m = math.hypot(r["stick_x"] or 0, r["stick_y"] or 0)
        if m >= 0.4:
            angs.append((i, math.degrees(math.atan2(r["stick_x"], r["stick_y"]))))
    change = None
    for (i0, a0), (i1, a1) in zip(angs, angs[1:]):
        if abs(wrap(a1 - a0)) > 60:
            change = (i1, a1)
    if change is None:
        return None
    i1, a1 = change
    for i in range(i1, len(rows)):
        f = rows[i].get("facing")
        if f is not None and abs(wrap(f - a1)) < 10:
            return i - i1
    return 999


def compare_scenario(sid, ro, rt, hz, run_speed, tols):
    res = []
    so, st = speeds(ro, hz), speeds(rt, hz)
    n = min(len(ro), len(rt))
    curve = None
    if so and st:
        errs = [None if so[i] is None or st[i] is None else st[i] - so[i] for i in range(n)]
        ev = [abs(e) for e in errs if e is not None]
        curve = (so, st, errs)
        res.append(cmpnum("speed_mean_err", 0.0, sum(ev) / len(ev) if ev else None, tols, "mean |err| u/s"))
        res.append(cmpnum("speed_max_err", 0.0, max(ev) if ev else None, tols, "max |err| u/s"))
        thr = tols["first_divergence"][0]
        div = next((i for i in range(n - 1) if errs[i] is not None and errs[i + 1] is not None
                    and abs(errs[i]) > thr and abs(errs[i + 1]) > thr), None)
        res.append(Result("first_divergence", "-", div if div is not None else "none",
                          "PASS" if div is None else "FAIL", "first tick |speed err| > %.1f u/s for 2 ticks" % thr))
        res.append(cmpnum("distance", sum(x for x in so if x) / hz, sum(x for x in st if x) / hz, tols, "path length u"))
    else:
        for m in ("speed_mean_err", "speed_max_err", "first_divergence", "distance"):
            res.append(skip(m, "needs pos_x,pos_z in both"))
    mo, s0, rel0 = stick_info(ro)
    if so and st and s0 is not None:
        end = rel0 if rel0 is not None else len(ro)
        w0 = max(s0, end - 20)
        def steady(sp):
            v = [x for x in sp[w0:end] if x is not None]
            return sum(v) / len(v) if v else None
        res.append(cmpnum("steady_speed", steady(so), steady(st), tols, "mean of last 20 push ticks"))
        a, b = time_to_thresh(so, s0, run_speed), time_to_thresh(st, s0, run_speed)
        if a is None and b is None:
            res.append(Result("time_to_run", None, None, "PASS", "neither reaches %.0f u/s" % run_speed))
        else:
            res.append(cmpnum("time_to_run", a, b, tols, "ticks from push to >= %.0f u/s" % run_speed))
        if rel0 is not None:
            def stop(sp):
                return next((i - rel0 for i in range(rel0, len(sp)) if sp[i] is not None and sp[i] < 1.0), None)
            res.append(cmpnum("time_to_stop", stop(so), stop(st), tols, "ticks from release to < 1 u/s"))
    if has(ro, "facing") and has(rt, "facing"):
        def mt(rows):
            f = col(rows, "facing")
            d = [abs(wrap(b - a)) for a, b in zip(f, f[1:]) if a is not None and b is not None]
            return max(d) if d else None
        res.append(cmpnum("max_turn_rate", mt(ro), mt(rt), tols, "max deg/tick"))
        a, b = facing_settle(ro), facing_settle(rt)
        if a is not None or b is not None:
            res.append(cmpnum("facing_settle", a, b, tols, "ticks to settle within 10 deg after a reversal (999 = never)"))
    else:
        res.append(skip("max_turn_rate", "needs facing in both"))
    if has(ro, "clip_id") and has(rt, "clip_id"):
        co, ct = clip_switches(ro), clip_switches(rt)
        ok = abs(len(co) - len(ct)) <= tols["clip_switches"][0] and all(
            abs(a - b) <= tols["clip_switches"][0] for a, b in zip(co, ct))
        res.append(Result("clip_switches", co, ct, "PASS" if ok else "FAIL",
                          "count %d vs %d; ticks matched in order (ids may differ between engines)" % (len(co), len(ct))))
    else:
        res.append(skip("clip_switches", "needs clip_id in both"))
    locked = any((r.get("lock_active") or 0) for r in ro + rt)
    if locked:
        if has(ro, "lock_band") and has(rt, "lock_band"):
            fo, ft = strafe_flips(ro), strafe_flips(rt)
            res.append(cmpnum("strafe_flips", float(fo), float(ft), tols, "left/right side flips of lock_band"))
        else:
            res.append(skip("strafe_flips", "needs lock_band in both"))
    jo, jt = first_press(ro, B_JUMP), first_press(rt, B_JUMP)
    do_, dt = first_press(ro, B_DODGE), first_press(rt, B_DODGE)
    if jo is not None and jt is not None and has(ro, "grounded") and has(rt, "grounded"):
        def lift(rows, p):
            return next((i - p for i in range(p, len(rows)) if rows[i].get("grounded") == 0), None)
        res.append(cmpnum("takeoff_delay", lift(ro, jo), lift(rt, jt), tols, "ticks press -> leaves ground"))
        def air(rows):
            return sum(1 for r in rows if r.get("grounded") == 0)
        res.append(cmpnum("airtime", air(ro), air(rt), tols, "ticks airborne"))
    if jo is not None and jt is not None and has(ro, "pos_y") and has(rt, "pos_y"):
        def apex(rows):
            return max(r["pos_y"] for r in rows if r.get("pos_y") is not None) - rows[0]["pos_y"]
        res.append(cmpnum("apex", apex(ro), apex(rt), tols, "max height above start u"))
    for p_o, p_t, label in ((jo, jt, "jump"), (do_, dt, "dodge")):
        if p_o is not None and p_t is not None and has(ro, "state") and has(rt, "state"):
            def busy(rows, p):
                i = p
                while i < len(rows) and rows[i].get("state") in FREE_STATES:
                    i += 1  # wait for state to leave the free set
                    if i - p > 10:
                        return None
                j = i
                while j < len(rows) and rows[j].get("state") not in FREE_STATES:
                    j += 1
                return j - p
            res.append(cmpnum("busy_ticks", busy(ro, p_o), busy(rt, p_t), tols,
                              "%s: ticks press -> free state (0/1/8) again" % label))
            break
    if do_ is not None and dt is not None and so and st:
        def dd(sp, p):
            return sum(x for x in sp[p:p + 60] if x) / hz
        def pk(sp, p):
            return max(x for x in sp[p:p + 60] if x is not None)
        res.append(cmpnum("dodge_distance", dd(so, do_), dd(st, dt), tols, "distance in 60 ticks after press"))
        res.append(cmpnum("dodge_peak_speed", pk(so, do_), pk(st, dt), tols, "u/s"))
    return res, curve


SPARK = " .:-=+*#%@"


def spark(vals, vmax, width=60):
    if not vals:
        return ""
    k = max(1, len(vals) // width)
    out = ""
    for i in range(0, len(vals), k):
        chunk = [v for v in vals[i:i + k] if v is not None]
        v = max(chunk) if chunk else 0
        out += SPARK[min(len(SPARK) - 1, int(v / vmax * (len(SPARK) - 1) + 0.5))] if vmax > 0 else " "
    return out


def report(orig, ours, args, tols):
    L = []
    L.append("GOLDEN TRACE COMPARISON")
    L.append("original: %s  build=%s char=%s stage=%s" % (orig.path, orig.meta.get("build", "?"),
                                                          orig.meta.get("character", "?"), orig.meta.get("stage", "?")))
    L.append("ours    : %s  build=%s" % (ours.path, ours.meta.get("build", "?")))
    miss = sorted(set(ALIASES) - orig.columns)
    if miss:
        L.append("original trace lacks columns: " + ", ".join(miss))
    L.append("")
    totals = {"PASS": 0, "FAIL": 0, "SKIP": 0}
    summary = []
    ids = [args.scenario] if args.scenario else orig.order
    for sid in ids:
        L.append("=" * 78)
        if sid not in orig.scen:
            L.append("%s: not in original trace" % sid)
            continue
        if sid not in ours.scen:
            L.append("%s: MISSING in ours" % sid)
            totals["FAIL"] += 1
            summary.append((sid, "MISSING", 0))
            continue
        ro, rt = orig.scen[sid], ours.scen[sid]
        res, curve = compare_scenario(sid, ro, rt, args.hz, args.run_speed, tols)
        L.append("%s   (%d vs %d ticks)" % (sid, len(ro), len(rt)))
        if curve:
            so, st, errs = curve
            vmax = max([v for v in so + st if v] or [1])
            L.append("speed u/s  orig |%s|" % spark(so, vmax))
            L.append("           ours |%s|   (peak %.1f)" % (spark(st, vmax), vmax))
            L.append("  tick    orig    ours     err")
            for i in range(0, len(errs), args.step):
                L.append("  %4d %7s %7s %7s" % (i, fmt(so[i]), fmt(st[i]), "-" if errs[i] is None else "%+.2f" % errs[i]))
        L.append("  %-18s %-16s %-16s %-5s %s" % ("metric", "original", "ours", "", "note"))
        nf = 0
        for r in res:
            totals[r.verdict] += 1
            nf += r.verdict == "FAIL"
            L.append("  %-18s %-16s %-16s %-5s %s" % (r.name, fmt(r.o)[:16], fmt(r.t)[:16], r.verdict, r.note))
        summary.append((sid, "FAIL" if nf else "PASS", nf))
    L.append("=" * 78)
    L.append("SUMMARY")
    for sid, v, nf in summary:
        L.append("  %-28s %s%s" % (sid, v, " (%d metric(s) out of tolerance)" % nf if nf and v == "FAIL" else ""))
    L.append("  metrics: %d PASS, %d FAIL, %d SKIP" % (totals["PASS"], totals["FAIL"], totals["SKIP"]))
    return "\n".join(L) + "\n", totals


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("orig")
    ap.add_argument("ours")
    ap.add_argument("--scenario")
    ap.add_argument("--step", type=int, default=5)
    ap.add_argument("--hz", type=float, default=30.0)
    ap.add_argument("--run-speed", type=float, default=50.0, help="u/s counted as 'reached run' (default 50)")
    ap.add_argument("--out", default="trace_report.txt")
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--tol", action="append", default=[], metavar="NAME=ABS")
    args = ap.parse_args(argv)
    tols = dict(TOL)
    for t in args.tol:
        k, _, v = t.partition("=")
        if k in tols:
            tols[k] = (float(v), tols[k][1])
    text, totals = report(Trace(args.orig), Trace(args.ours), args, tols)
    sys.stdout.write(text)
    with open(args.out, "w") as f:
        f.write(text)
    return 1 if args.strict and totals["FAIL"] else 0


if __name__ == "__main__":
    sys.exit(main())
