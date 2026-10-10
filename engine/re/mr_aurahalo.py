#!/usr/bin/env python3
"""Which draw makes the visible school halo? Elimination on the ORIGINAL (our own tool).

  python mr_aurahalo.py [--level 9] [--segments SPEC] [--out DIR]

Same setup as mr_auralog.py (versus match, P0 at Level `--level`, P1 at Level 0, window filmed). The
run is a list of SEGMENTS; in each one a set of game functions is skipped: the debugger redirects the
game thread at the function's entry to its return address (rax = 0), so the function does nothing.
Nothing is changed on disk. Default plan:

  base      nothing skipped (40 ticks)
  wisp      skip 0x14011b450, the eft0600 PDEFTAURA01 piece draw (40)
  wisp+..   (further stages named on the command line: --segments base:,wisp:wisp,fx600:wisp+spawn600)
  base2     nothing (30)
  cyc       composite skipped 15 ticks / normal 15 ticks, four times (the HUD aura object's only sampler of its 512 target)

Skippable names (SKIPS below): wisp = 0x14011b450, draw = 0x14019b920 (whole HUD aura object draw),
composite = 0x140289140, spawn600 = the effect creator 0x14018d2f0 only when it is asked for an effect 600..609
(returns -1: no instance), swbos = (set with --skip-addr name=0xADDR to try another candidate).

Outputs per run: segments.json (video times), the master video; the analysis (red-excess area round the
fighter per segment, pixel difference of toggled segments) is printed by `analyze()`.
"""
import argparse
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

OUT = os.path.join(mr.OUT_ROOT, "aurahalo")
SKIPS = {"wisp": 0x14011B450, "draw": 0x14019B920, "composite": 0x140289140, "stat3e0": 0x14024D3E0}
OBJ_OFF = 0x17180      # fighter + 0x17180: a per-fighter object whose draw (vtable +0x20) FUN_14024f2f0 calls after the model; "obj17180" nulls it for a segment
SPAWN = 0x14018D2F0            # FUN_14018d2f0(player, effect id, flag, matrix): the central effect creator
DEFAULT_PLAN = [("base", (), 40), ("wisp", ("wisp",), 40), ("base2", (), 30)] + \
               [("cyc_%d_%s" % (i, "skip" if k == 0 else "norm"), ("composite",) if k == 0 else (), 15)
                for i in range(4) for k in (0, 1)]


def parse_plan(text, skips=SKIPS):
    """'base:,wisp:wisp,both:wisp+composite' -> [(label, (names...), ticks)]; ticks default 40, 'label:names@30'."""
    out = []
    for item in [t for t in text.split(",") if t.strip()]:
        label, _, rest = item.partition(":")
        names, ticks = rest, ""
        head, sep, tail = rest.rpartition("@")
        if sep and tail.isdigit() and (not head.startswith(("key:",)) or "@" in head):
            names, ticks = head, tail
        elif rest.isdigit() is False and rest.startswith("@"):
            names, ticks = "", rest[1:]
        ns = tuple(n for n in names.split("+") if n)
        for n in ns:
            if n.lstrip("-") not in skips and n.lstrip("-") not in ("spawn600", "obj17180") and not n.startswith(("key:", "seq:", "ptr:")):
                raise ValueError("unknown skip %r (known: %s, spawn600)" % (n, ", ".join(sorted(skips))))
        out.append((label.strip(), ns, int(ticks) if ticks else 40))
    return out


CENSUS = {"mesh": 0x14001D730, "imm": 0x14001DBE0, "idx": 0x14001EDE0, "b6df": 0x1401B6DF0, "b712": 0x1401B7120,
          "a640": 0x14027A640, "e70": 0x140279E70, "d7c0": 0x14028D7C0, "a130": 0x14028A130, "adb0": 0x14029ADB0,
          "b020": 0x14029B020, "b360": 0x14029B360, "b890": 0x14029B890, "bb60": 0x14029BB60, "8590": 0x140286590,
          "7080": 0x140287080, "7b10": 0x140287B10, "8100": 0x140288100}


class Census:
    """Counts, per phase, who calls each draw primitive (return address of the call)."""

    def __init__(self, hooks=CENSUS):
        self.hooks = dict(hooks)
        self.phase = None
        self.counts = {}
        self.ticks = {}

    def specs(self):
        return ["0x%x:cen_%s:rcx" % (a, n) for n, a in self.hooks.items()]

    def hit(self, rig, name, vals, tick):
        if not name.startswith("cen_") or self.phase is None:
            return
        raw = rig.ctx.bytes()
        rsp = struct.unpack_from("<Q", raw, lt.CTX_RSP_OFF)[0]
        ret = struct.unpack("<Q", rig.read(rsp, 8))[0]
        base = rig.base
        key = "%s@%x" % (name[4:], ret - base + 0x140000000)
        d = self.counts.setdefault(self.phase, {})
        d[key] = d.get(key, 0) + 1


class Skipper:
    def __init__(self, skips):
        self.skips = dict(skips)
        self.active = set()
        self.pre = set()
        self.count = {}
        self.seen = {}
        self.draw_sites = False
        self.seq = self.seq_prev = self.maxseq = 0
        self.seqkeys = {}
        self.ptr_order = []
        self.ptr_rank = {}
        self.ptr_info = {}
        self.ptr_frozen = False
        self.phase = None
        self.phase_ptrs = {}
        self.phase_frames = {}

    def specs(self):
        out = ["0x%x:skip_%s:rcx" % (a, n) for n, a in self.skips.items()]
        out.append("0x%x:skip_spawn600:ecx,edx" % SPAWN)
        if self.draw_sites:
            out += ["0x%x:ds_%s:rcx,rdx,r8,r9" % (a, n) for n, a in CENSUS.items()]
            out.append("0x%x:ds_frame:rcx" % 0x1401C78C0)
        return out

    def _return_now(self, rig, rax=0):
        raw = rig.ctx.bytes()
        rsp = struct.unpack_from("<Q", raw, lt.CTX_RSP_OFF)[0]
        ret = rig.read(rsp, 8)
        rig.ctx.set_u64(lt.CTX_RIP_OFF, struct.unpack("<Q", ret)[0])
        rig.ctx.set_u64(lt.CTX_RSP_OFF, rsp + 8)
        rig.ctx.set_u64(lt.REG64["rax"], rax)
        rig.redirect = True

    def draw_hit(self, rig, name, vals, tick):
        """Draw-site skipping: 'key:idx@140298b28' skips every call of that site (hook name @ caller);
        'seq:LO-HI' skips the calls number LO..HI-1 of the indexed-draw wrapper within a scene pass."""
        if name == "ds_frame":
            self.seq_prev, self.seq = self.seq, 0
            if self.phase is not None:
                self.phase_frames[self.phase] = self.phase_frames.get(self.phase, 0) + 1
            return
        raw = rig.ctx.bytes()
        rsp = struct.unpack_from("<Q", raw, lt.CTX_RSP_OFF)[0]
        ret = struct.unpack("<Q", rig.read(rsp, 8))[0] - rig.base + 0x140000000
        key = "%s@%x" % (name[3:], ret)
        n = None
        if name == "ds_idx":
            n = self.seq
            self.seq += 1
            self.seqkeys[n] = key
            self.maxseq = max(self.maxseq, self.seq)
        r9 = vals[3] if len(vals) > 3 else None
        if self.phase is not None and name == "ds_idx" and r9 is not None:
            e = self.phase_ptrs.setdefault(self.phase, {}).setdefault(hex(r9), [0, vals[1], key])
            e[0] += 1
        if name == "ds_idx" and r9 is not None:
            if any(a.startswith("ptr:") for a in self.active):
                self.ptr_frozen = True
            if not self.ptr_frozen and r9 not in self.ptr_rank:
                self.ptr_rank[r9] = len(self.ptr_order)
                self.ptr_order.append(r9)
                mem = rig.read(r9, 0x48)
                self.ptr_info[r9] = (vals[1], key, vals[0], vals[2], mem.hex() if mem else None)
        for a in self.active:
            if a.startswith("ptr:") and r9 in self.ptr_rank:
                lo, hi = (float(x) for x in a[4:].split("-"))
                f = self.ptr_rank[r9] if hi > 1.0 else self.ptr_rank[r9] / max(1, len(self.ptr_order))
                if lo <= f < hi:
                    self.count[a] = self.count.get(a, 0) + 1
                    return self._return_now(rig)
            if a.startswith("key:") and a[4:] == key:
                self.count[a] = self.count.get(a, 0) + 1
                return self._return_now(rig)
            if a.startswith("seq:") and n is not None:
                lo, hi = (int(x) for x in a[4:].split("-"))
                if lo <= n < hi:
                    self.count[a] = self.count.get(a, 0) + 1
                    return self._return_now(rig)

    def hit(self, rig, name, vals, tick):
        if name.startswith("ds_"):
            return self.draw_hit(rig, name, vals, tick)
        if not name.startswith("skip_"):
            return
        which = name[5:]
        if which == "spawn600" and vals[1] is not None:
            self.seen[vals[1]] = self.seen.get(vals[1], 0) + 1
        if which not in self.active:
            return
        if which == "spawn600" and not (vals[1] is not None and 600 <= vals[1] <= 609):
            return
        self.count[which] = self.count.get(which, 0) + 1
        raw = rig.ctx.bytes()
        rsp = struct.unpack_from("<Q", raw, lt.CTX_RSP_OFF)[0]
        ret = rig.read(rsp, 8)
        rig.ctx.set_u64(lt.CTX_RIP_OFF, struct.unpack("<Q", ret)[0])
        rig.ctx.set_u64(lt.CTX_RSP_OFF, rsp + 8)
        rig.ctx.set_u64(lt.REG64["rax"], 0xFFFFFFFF if which == "spawn600" else 0)
        rig.redirect = True


def rle(d):
    """{index: key} -> ['lo-hi key', ...] runs of equal keys."""
    out, start, prev = [], None, None
    for i in sorted(d):
        if prev is None or d[i] != d[prev] or i != prev + 1:
            if prev is not None:
                out.append("%d-%d %s" % (start, prev, d[prev]))
            start = i
        prev = i
    if prev is not None:
        out.append("%d-%d %s" % (start, prev, d[prev]))
    return out


def run(plan, level=9, out_dir=OUT, extra=None, log=print, pre=(), census=False, sites=False, avoid_school=()):
    os.makedirs(out_dir, exist_ok=True)
    mr.keep_awake(True)
    snap = os.path.join(mr.OUT_ROOT, "save-before-halo-" + time.strftime("%Y%m%d_%H%M%S"))
    log("package folder snapshot: %d files" % sg.snapshot(sg.LIVE, snap))
    pid, ours = mr.launch_game()
    if not pid:
        raise RuntimeError("the game did not start")
    skips = dict(SKIPS)
    skips.update(extra or {})
    sk = Skipper(skips)
    sk.pre = set(pre)
    sk.draw_sites = sites
    sk.active = set(pre)
    cen = Census() if census else None
    st = {"master": None, "master_path": os.path.join(out_dir, "_master.mp4"), "segs": []}

    def flow(rig, drv):
        yield from mr.boot_to_profile(rig, drv)
        yield from mr.launch_versus(rig, drv)
        anchors = (rig.fighter_pos(0), rig.fighter_pos(1))
        st["master"] = yield from mr.start_master_capture(st["master_path"])
        log("master capture: %s" % ("filming" if st["master"] else "NOT available"))
        drv.pads = [mr.PadState(), mr.PadState()]
        yield from mr.settle_placement(rig, drv, 60.0, anchors, {})
        mgr = rig.rd64(0x1404BBEE0)
        if mgr:
            for pl in (0, 1):
                sch = rig.rd32(mgr + 0x60 * pl + 0x44, True)
                rgb = [rig.rdf(0x140599080 + 0x10 * sch + 4 * k) for k in range(3)] if sch is not None and 0 <= sch < 6 else None
                log("player %d aura school index %s colour %s" % (pl, sch, rgb))
                if pl == 0:
                    st["school"] = (sch, rgb)
            if st.get("school", (None,))[0] in avoid_school:
                log("P0 school %s is on the avoid list (invisible against this sky): ending this launch early" % (st["school"][0],))
                st["retry"] = True
                if st["master"]:
                    st["master"].stop()
                    st["master"] = None
                return
        for pl in (0, 1):
            fp = rig.fighter(pl)
            ob = rig.rd64(fp + OBJ_OFF) if fp else None
            vt = rig.rd64(ob) if ob else None
            fn = rig.rd64(vt + 0x20) if vt else None
            log("fighter %d +0x17180 object %s vtable(ghidra) %s draw(+0x20) %s" % (pl, hex(ob or 0), hex(vt - rig.base + 0x140000000) if vt else None, hex(fn - rig.base + 0x140000000) if fn else None))
        for label, names, ticks in [e for e in plan if e[0].startswith("L0_")]:
            sk.active = (set(n for n in names if not n.startswith("-")) | sk.pre) - set(n[1:] for n in names if n.startswith("-"))
            if cen:
                cen.phase = label
            sk.phase = label
            t0 = time.monotonic()
            saved = {}
            if "obj17180" in names:
                for pl in (0, 1):
                    fp = rig.fighter(pl)
                    saved[pl] = rig.rd64(fp + OBJ_OFF) if fp else None
                    if saved[pl]:
                        rig.wr(fp + OBJ_OFF, struct.pack("<Q", 0))
                log("obj17180 nulled: %s" % {k: hex(v) if v else v for k, v in saved.items()})
            yield ticks
            for pl, old in saved.items():
                if old:
                    rig.wr(rig.fighter(pl) + OBJ_OFF, struct.pack("<Q", old))
            st["segs"].append({"label": label, "skips": list(names), "ticks": ticks,
                               "video_t0": t0 - st["master"].epoch() if st["master"] else None,
                               "video_t1": time.monotonic() - st["master"].epoch() if st["master"] else None})
        mr.set_stats(rig, 0, level, health=20)
        mr.set_stats(rig, 1, 0, health=40)
        if cen:
            cen.phase = None
        yield 90
        for label, names, ticks in [e for e in plan if not e[0].startswith("L0_")]:
            sk.active = (set(n for n in names if not n.startswith("-")) | sk.pre) - set(n[1:] for n in names if n.startswith("-"))
            if cen:
                cen.phase = label
            sk.phase = label
            t0 = time.monotonic()
            saved = {}
            if "obj17180" in names:
                for pl in (0, 1):
                    fp = rig.fighter(pl)
                    saved[pl] = rig.rd64(fp + OBJ_OFF) if fp else None
                    if saved[pl]:
                        rig.wr(fp + OBJ_OFF, struct.pack("<Q", 0))
                log("obj17180 nulled: %s" % {k: hex(v) if v else v for k, v in saved.items()})
            yield ticks
            for pl, old in saved.items():
                if old:
                    rig.wr(rig.fighter(pl) + OBJ_OFF, struct.pack("<Q", old))
            st["segs"].append({"label": label, "skips": list(names), "ticks": ticks,
                               "video_t0": t0 - st["master"].epoch() if st["master"] else None,
                               "video_t1": time.monotonic() - st["master"].epoch() if st["master"] else None})
        sk.active = set(sk.pre)
        yield 20
        if st["master"]:
            st["master"].stop()
            st["master"] = None

    holder = {}
    rig = mr.Rig(pid, lambda r, t: holder["d"].on_tick(r, t), extra_specs=sk.specs() + (cen.specs() if cen else []), on_hit=(lambda r, n, v, t: (sk.hit(r, n, v, t), cen.hit(r, n, v, t) if cen else None)),
                 out_path=os.path.join(mr.OUT_ROOT, "rig_raw.csv"))
    rig.watch_s = 150.0 if (census or sites) else 40.0
    drv = mr.Driver(rig, None)
    drv.gen = flow(rig, drv)
    holder["d"] = drv
    try:
        rig.run(0)
    finally:
        if st["master"]:
            st["master"].stop()
        mr.keep_awake(False)
    log("rig: stalled=%s done=%s error=%r segments=%d skipped=%s effects_requested=%s idx_draws_per_pass(max)=%d/%d" % (rig.stalled, drv.done, drv.error, len(st["segs"]), sk.count, sorted(sk.seen.items()), sk.maxseq, sk.seq_prev))
    if rig.stalled:
        log("stall dump: %s" % (rig.stall_dump,))
    with open(os.path.join(out_dir, "segments.json"), "w") as f:
        json.dump({"segments": st["segs"], "skipped_calls": sk.count, "effects_requested": sorted(sk.seen.items()), "pre": sorted(sk.pre), "level": level, "census": cen.counts if cen else None, "school": st.get("school"), "seqkeys_last_pass": rle(sk.seqkeys), "phase_ptrs": sk.phase_ptrs, "phase_frames": sk.phase_frames, "ptr_count": len(sk.ptr_order), "ptr_info": [[i, hex(p)] + list(sk.ptr_info[p]) for i, p in enumerate(sk.ptr_order)]}, f, indent=1)
    for attempt in range(4):
        try:
            mb.tidy_save(pid, ours, snap, log=log)
            break
        except OSError as e:          # the game was still letting go of a save file
            log("save check retry %d: %r" % (attempt + 1, e))
            time.sleep(6)
    if drv.error:
        raise drv.error
    if st.get("retry"):
        return None
    return st["segs"], sk.count


# ---------------------------------------------------------------------------- analysis (video)
def red_excess_area(frame_rgb, box, thr=25):
    import numpy as np
    x0, y0, x1, y1 = box
    f = frame_rgb[y0:y1, x0:x1].astype(int)
    # school-colour excess over the green sky: (R + B) / 2 - G (works for Faith purple and Psycho red)
    return int((((f[:, :, 0] + f[:, :, 2]) // 2 - f[:, :, 1]) > thr).sum())


def chroma_area(frame_rgb, box, rgb, thr=60):
    """Pixels whose colour leans towards the school colour: (c - mean c) . unit(school - mean school) > thr."""
    import numpy as np
    x0, y0, x1, y1 = box
    sc = np.array(rgb, dtype=float)
    u = sc - sc.mean()
    u = u / np.linalg.norm(u)
    f = frame_rgb[y0:y1, x0:x1].astype(float)
    m = ((f - f.mean(axis=2, keepdims=True)) * u).sum(axis=2)
    return int((m > thr).sum())


def analyze(out_dir=OUT, box=(280, 300, 480, 520), fps=20):
    import subprocess
    import numpy as np
    seg = json.load(open(os.path.join(out_dir, "segments.json")))["segments"]
    t_lo = max(0.0, min(s["video_t0"] for s in seg) - 0.5)
    t_hi = max(s["video_t1"] for s in seg) + 0.5
    ff = mr.Capture.FFMPEG
    p = subprocess.run([ff, "-loglevel", "error", "-ss", "%.2f" % t_lo, "-t", "%.2f" % (t_hi - t_lo), "-i",
                        os.path.join(out_dir, "_master.mp4"), "-vf", "fps=%d,scale=1440:900" % fps, "-pix_fmt", "rgb24",
                        "-f", "rawvideo", "-"], capture_output=True)
    frames = np.frombuffer(p.stdout, dtype=np.uint8).reshape(-1, 900, 1440, 3)
    res = []
    for s in seg:
        i0 = int(round((s["video_t0"] + 0.15 - t_lo) * fps))
        i1 = int(round((s["video_t1"] - 0.05 - t_lo) * fps))
        sch = json.load(open(os.path.join(out_dir, "segments.json"))).get("school")
        if sch and sch[1]:
            areas = [chroma_area(frames[i], box, sch[1]) for i in range(max(0, i0), min(len(frames), i1))]
        else:
            areas = [red_excess_area(frames[i], box) for i in range(max(0, i0), min(len(frames), i1))]
        res.append({"label": s["label"], "skips": s["skips"], "frames": len(areas),
                    "mean": round(float(np.mean(areas)), 1) if areas else None,
                    "sd": round(float(np.std(areas)), 1) if areas else None})
    return res, frames, t_lo


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--level", type=int, default=9)
    ap.add_argument("--segments", help="plan, e.g. base:,wisp:wisp,both:wisp+spawn600@30")
    ap.add_argument("--skip-addr", action="append", default=[], metavar="NAME=0xADDR")
    ap.add_argument("--out", default=OUT)
    ap.add_argument("--pre", default="", help="skip names active from boot, e.g. spawn600")
    ap.add_argument("--census", action="store_true", help="count draw-primitive callers per segment (L0_ segments run before the Level is raised)")
    ap.add_argument("--sites", action="store_true", help="hook the draw call sites so segments can use key:NAME@ADDR and seq:LO-HI")
    ap.add_argument("--avoid-school", default="3", help="comma list of P0 aura school indices to relaunch on (3 = green: invisible against the sky); empty = never")
    ap.add_argument("--analyze-only", action="store_true")
    a = ap.parse_args(argv)
    extra = {}
    for kv in a.skip_addr:
        n, _, v = kv.partition("=")
        extra[n] = int(v, 0)
    if not a.analyze_only:
        plan = parse_plan(a.segments, dict(SKIPS, **extra)) if a.segments else DEFAULT_PLAN
        avoid = tuple(int(x) for x in a.avoid_school.split(",") if x.strip())
        for attempt in range(6):
            r = run(plan, a.level, a.out, extra, pre=[n for n in a.pre.split('+') if n], census=a.census, sites=a.sites, avoid_school=avoid)
            if r is not None:
                break
            print("relaunch %d" % (attempt + 1))
    res, _f, _t = analyze(a.out)
    for r in res:
        print(r)
    return 0


if __name__ == "__main__":
    sys.exit(main())
