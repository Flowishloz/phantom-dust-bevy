#!/usr/bin/env python3
"""Live values of the school-aura glow in the ORIGINAL (our own tool; reads only, plus one optional
skip of the draw for a comparison take). See engine/docs/FORMATS-EFFECTS.md "School aura glow".

  python mr_auralog.py [--ticks 90] [--out DIR] [--no-skip-take] [--no-video]

Starts (or reuses) the original, gets to a versus match, gives P0 Level 9 and P1 Level 0 so only
P0 has a school aura, then logs every call of the aura object's functions for `--ticks` program
ticks (30 per second) with their arguments:

  0x14019b520 update (rcx = the HUD aura object)         0x14019b920 draw (rcx)
  0x14019c3d0 silhouette (pass 1)                        0x140286c60 MFilter (pass 2)
  0x140287460 GFilter2 (pass 3, ecx / edx = first fighter, count)
  0x1402864e0 weight table (rcx = 8 floats out, xmm1 / xmm2 = float args: the c4 argument)
  0x140289140 composite (ecx = int, xmm1..3 = x y w, stack = h, matrix pointer, opacity)
  0x140292d40 field set (xmm0..3 = args, [rsp+0x28] = the field that becomes DAT_14057f390)

Per draw / composite call it also reads DAT_14057f36c / 370 / 374 / 388 / 390, the 8 weights each of
the first four fighters (0x14054c260) and the object's per-fighter flags and opacity. A second take
returns from the draw at once (no game code is changed on disk; the thread is redirected in the
debugger at the call) with a video of both takes, to see whether the halo disappears.
"""
import argparse
import csv
import json
import os
import struct
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import live_trace as lt  # noqa: E402
import move_recorder as mr  # noqa: E402
import mr_batch as mb  # noqa: E402
import save_guard as sg  # noqa: E402

OUT = os.path.join(mr.OUT_ROOT, "auralog2")
HOOKS = (
    "0x14019b520:aura_update:rcx",
    "0x14019b920:aura_draw:rcx",
    "0x14019c3d0:silhouette:ecx,rdx",
    "0x140286c60:mfilter:ecx,edx,r8d",
    "0x140287460:gfilter2:ecx,edx",
    "0x1402864e0:weights:rcx,xmm1,xmm2,xmm3",
    "0x140289140:composite:ecx,xmm1,xmm2,xmm3,f[rsp+0x28],[rsp+0x30],d[rsp+0x38]",
    "0x140292d40:field:xmm0,xmm1,xmm2,xmm3,f[rsp+0x28]",
)
FIELD_GLOBALS = (("DAT_14057f36c", 0x14057F36C), ("DAT_14057f370", 0x14057F370), ("DAT_14057f374", 0x14057F374),
                 ("DAT_14057f388", 0x14057F388), ("DAT_14057f390", 0x14057F390))
TARGETS = (("DAT_14054c208", 0x14054C208), ("DAT_14054c228", 0x14054C228))   # source / destination textures
PERSIST = 0x14054C1F0           # int per fighter slot: the blend constant (f35 as 0..255) FUN_140287460 uses
WEIGHTS = 0x14054C260           # 8 floats per fighter slot, 0x20 bytes each (FUN_1402864e0 output)
REC0, REC_STRIDE = 0x190, 0x120  # per-fighter record of the aura object: lVar13 = object + 400, step 0x120
PLAYER_FLAGS, PLAYER_OPACITY = 0xC0, 0x1D0


def fl(rig, addr):
    v = rig.rdf(addr)
    return None if v is None else round(v, 6)


class Log:
    def __init__(self):
        self.rows = []
        self.on = False
        self.seq = 0
        self.skip_draw = False
        self.skipped = 0
        self.extra = {}

    def hit(self, rig, name, vals, tick):
        if not self.on:
            return
        self.seq += 1
        row = {"seq": self.seq, "tick": rig.tick, "name": name, "args": [("%.6g" % v) if isinstance(v, float) else v for v in vals]}
        if name == "aura_draw":
            obj = vals[0]
            row["players"] = []
            for p in range(4):
                fb = rig.read(obj + PLAYER_FLAGS + REC_STRIDE * p, 1)
                op = rig.read(obj + PLAYER_OPACITY + REC_STRIDE * p, 4)
                row["players"].append([fb[0] if fb else None, struct.unpack("<i", op)[0] if op and len(op) == 4 else None])
            if "record0" not in self.extra:      # one raw dump of fighter 0's record (find f35 / f36 by value)
                raw = rig.read(obj + REC0 - 0xD0, 0x120)
                if raw and len(raw) == 0x120:
                    self.extra["record0_ints"] = list(struct.unpack("<72i", raw))
                    self.extra["record0_floats"] = [round(x, 4) for x in struct.unpack("<72f", raw)]
                    self.extra["record0"] = True
            if self.skip_draw:
                rsp = struct.unpack_from("<Q", rig.ctx.bytes(), lt.CTX_RSP_OFF)[0]
                ret = rig.read(rsp, 8)
                rig.ctx.set_u64(lt.CTX_RIP_OFF, struct.unpack("<Q", ret)[0])
                rig.ctx.set_u64(lt.CTX_RSP_OFF, rsp + 8)
                rig.ctx.set_u64(lt.REG64["rax"], 0)
                rig.redirect = True
                self.skipped += 1
        if name in ("composite", "aura_draw", "weights", "gfilter2"):
            row["globals"] = {n: fl(rig, a) for n, a in FIELD_GLOBALS}
            row["targets"] = {n: rig.rd64(a) for n, a in TARGETS}
            pr = rig.rd(PERSIST, 16)
            row["persist"] = list(struct.unpack("<4i", pr)) if pr and len(pr) == 16 else None
            raw = rig.rd(WEIGHTS, 0x20 * 4)
            if raw and len(raw) == 0x80:
                row["weights"] = [[round(x, 6) for x in struct.unpack_from("<8f", raw, 0x20 * k)] for k in range(4)]
        self.rows.append(row)


def summarize(rows, ticks):
    """Calls per program tick for each function, and the distinct values seen."""
    by = {}
    for r in rows:
        by.setdefault(r["name"], []).append(r)
    out = {"ticks": ticks, "calls": {}, "per_tick": {}}
    for name, rs in by.items():
        out["calls"][name] = len(rs)
        out["per_tick"][name] = round(len(rs) / float(ticks), 3)
    comp = by.get("composite", [])
    out["composite_distinct_args"] = sorted({tuple(r["args"]) for r in comp})[:12]
    out["field_values"] = sorted({tuple(sorted((r.get("globals") or {}).items())) for r in comp})[:6]
    out["weights_args"] = sorted({tuple(r["args"][1:]) for r in by.get("weights", [])})[:12]
    out["field_set_args"] = sorted({tuple(r["args"]) for r in by.get("field", [])})[:12]
    out["gfilter2_args"] = sorted({tuple(r["args"]) for r in by.get("gfilter2", [])})[:8]
    out["weights_seen"] = [r.get("weights") for r in by.get("composite", [])[:1]]
    return out


def run(ticks=90, out_dir=OUT, skip_take=True, video=True, level=9, log=print):
    os.makedirs(out_dir, exist_ok=True)
    mr.keep_awake(True)
    snap = os.path.join(mr.OUT_ROOT, "save-before-aura-" + time.strftime("%Y%m%d_%H%M%S"))
    log("package folder snapshot: %d files" % sg.snapshot(sg.LIVE, snap))
    pid, ours = mr.launch_game()
    if not pid:
        raise RuntimeError("the game did not start")
    lg = Log()
    st = {"anchors": None, "master": None, "master_path": os.path.join(out_dir, "_master.mp4"), "takes": []}

    def flow(rig, drv):
        yield from mr.boot_to_profile(rig, drv)
        yield from mr.launch_versus(rig, drv)
        st["anchors"] = (rig.fighter_pos(0), rig.fighter_pos(1))
        if video:
            st["master"] = yield from mr.start_master_capture(st["master_path"])
            log("master capture: %s" % ("filming" if st["master"] else "NOT available"))
        cache = {}
        for take in (["toggle"] if skip_take else ["normal"]):
            if st["takes"]:
                yield from mr.launch_versus(rig, drv)
                st["anchors"] = (rig.fighter_pos(0), rig.fighter_pos(1))
                cache = {}
            drv.pads = [mr.PadState(), mr.PadState()]
            yield from mr.settle_placement(rig, drv, 60.0, st["anchors"], cache)
            mr.set_stats(rig, 0, level, health=20)
            mr.set_stats(rig, 1, 0, health=40)
            yield 90                                           # let the aura build up (tier / persistence settle)
            lg.rows.clear()
            lg.skip_draw = False
            lg.on = True
            t0 = time.monotonic()
            tick0 = rig.tick
            if take == "toggle":
                yield ticks // 2                       # first half: the draw runs as normal
                lg.skip_draw = True                    # second half: every call of FUN_14019b920 returns at once
                st["skip_at"] = time.monotonic()
                yield ticks - ticks // 2
            else:
                yield ticks
            lg.on = False
            t1 = time.monotonic()
            rows = list(lg.rows)
            st["takes"].append({"take": take, "rows": rows, "t0": t0, "t1": t1, "ticks": rig.tick - tick0,
                                "video_t0": (t0 - st["master"].epoch()) if st["master"] else None,
                                "skipped_calls": lg.skipped, "skip_at": st.get("skip_at")})
            lg.skip_draw = False
            yield 30
        if st["master"]:
            st["master"].stop()
            st["master"] = None

    holder = {}
    rig = mr.Rig(pid, lambda r, t: holder["d"].on_tick(r, t), extra_specs=HOOKS, on_hit=lg.hit,
                 out_path=os.path.join(mr.OUT_ROOT, "rig_raw.csv"))
    rig.ctx_flags = lt.CONTEXT_CONTROL | lt.CONTEXT_INTEGER | lt.CONTEXT_FLOATING_POINT
    rig.watch_s = 40.0
    drv = mr.Driver(rig, None)
    drv.gen = flow(rig, drv)
    holder["d"] = drv
    try:
        rig.run(0)
    finally:
        if st["master"]:
            st["master"].stop()
        mr.keep_awake(False)
    log("rig: stalled=%s fatal=%r drv.done=%s error=%r takes=%d dump=%s" % (rig.stalled, rig.fatal, drv.done, drv.error, len(st["takes"]), (rig.stall_dump or "")[:300]))
    mb.tidy_save(pid, ours, snap, log=log)
    if drv.error:
        raise drv.error
    result = {"takes": []}
    for t in st["takes"]:
        s = summarize(t["rows"], t["ticks"])
        s["take"], s["video_t0_s"], s["skipped_calls"] = t["take"], t["video_t0"], t["skipped_calls"]
        if t.get("skip_at") and t["video_t0"] is not None:
            s["video_skip_at_s"] = t["video_t0"] + (t["skip_at"] - t["t0"])
        half = [r for r in t["rows"]]
        if t["take"] == "toggle":
            mid = (min(r["tick"] for r in half) + max(r["tick"] for r in half)) // 2 if half else 0
            s["first_half"] = summarize([r for r in half if r["tick"] <= mid], t["ticks"] // 2)["calls"]
            s["second_half"] = summarize([r for r in half if r["tick"] > mid], t["ticks"] - t["ticks"] // 2)["calls"]
        s["targets_seen"] = sorted({tuple(sorted((r.get("targets") or {}).items())) for r in t["rows"] if r.get("targets")})[:8]
        s["persist_seen"] = sorted({tuple(r["persist"]) for r in t["rows"] if r.get("persist")})[:8]
        result["takes"].append(s)
        with open(os.path.join(out_dir, "calls_%s.json" % t["take"]), "w") as f:
            json.dump(t["rows"], f)
    result["record0"] = lg.extra
    with open(os.path.join(out_dir, "summary.json"), "w") as f:
        json.dump(result, f, indent=1)
    return result


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--ticks", type=int, default=90)
    ap.add_argument("--out", default=OUT)
    ap.add_argument("--level", type=int, default=9)
    ap.add_argument("--no-skip-take", action="store_true")
    ap.add_argument("--no-video", action="store_true")
    a = ap.parse_args(argv)
    res = run(a.ticks, a.out, not a.no_skip_take, not a.no_video, a.level)
    print(json.dumps(res["takes"][0], indent=1)[:3000])
    return 0


if __name__ == "__main__":
    sys.exit(main())
