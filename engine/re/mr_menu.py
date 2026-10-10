#!/usr/bin/env python3
"""Live looks at the ORIGINAL's menus and battle camera (our own tool; no OS input, pads written in memory).

  python mr_menu.py film   --seconds 5  --out DIR            main menu untouched, filmed
  python mr_menu.py sounds --actions "down,down,down,up,A,B" --out DIR [--film]
  python mr_menu.py consts --ticks 60   --out DIR            per-frame constant uploads + draws in the menu
  python mr_menu.py camera --out DIR                         versus battle: camera matrices per view each pass

`--actions` is a comma list of button names (move_recorder.BUTTONS) or `wait:N` ticks; every action is held 4 ticks and
followed by 40 ticks of silence. The sound hook is FUN_140276330 (bank, id, id): every call is logged with the action
that was running. The save is put back from the pre-run snapshot at the end (like every other run).
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

SND = 0x140276330
# constant uploads (vertex / pixel) and draw wrappers, for the menu look
CONST_HOOKS = {"cv": 0x14001E540, "cp": 0x14001ED70}
DRAW_HOOKS = {"idx": 0x14001EDE0, "imm": 0x14001DBE0}
SCENE_PASS = 0x140299310           # one call per view in a battle (the scene pass)
CAM_REGION = 0x1405A5C00           # 0x140 bytes: the per-view matrices the scene pass reads
VIEW_INDEX = 0x14054CA18


class Session:
    def __init__(self, log=print):
        self.log = log
        self.events = []          # (tick, wall, kind, payload)
        self.label = None
        self.t0 = time.monotonic()
        self.cap = None

    def ev(self, rig, kind, payload):
        self.events.append((rig.tick, round(time.monotonic() - self.t0, 3), self.label, kind, payload))


def run(mode, out_dir, flow_factory, extra_specs=(), on_hit=None, watch_s=40.0, log=print, video=True):
    os.makedirs(out_dir, exist_ok=True)
    mr.keep_awake(True)
    snap = os.path.join(mr.OUT_ROOT, "save-before-menu-" + time.strftime("%Y%m%d_%H%M%S"))
    log("package folder snapshot: %d files" % sg.snapshot(sg.LIVE, snap))
    pid, ours = mr.launch_game()
    if not pid:
        raise RuntimeError("the game did not start")
    S = Session(log)
    holder = {}
    rig = mr.Rig(pid, lambda r, t: holder["d"].on_tick(r, t), extra_specs=list(extra_specs),
                 on_hit=(lambda r, n, v, t: on_hit(S, r, n, v, t)) if on_hit else None,
                 out_path=os.path.join(mr.OUT_ROOT, "rig_raw.csv"))
    rig.watch_s = watch_s
    drv = mr.Driver(rig, None)
    drv.gen = flow_factory(S, rig, drv, os.path.join(out_dir, "_master.mp4") if video else None)
    holder["d"] = drv
    try:
        rig.run(0)
    finally:
        if S.cap:
            S.cap.stop()
        mr.keep_awake(False)
    log("rig: stalled=%s done=%s error=%r events=%d" % (rig.stalled, drv.done, drv.error, len(S.events)))
    if rig.stalled:
        log("stall dump: %s" % (rig.stall_dump,))
    with open(os.path.join(out_dir, "%s.json" % mode), "w") as f:
        json.dump({"events": S.events, "meta": getattr(S, "meta", {})}, f)
    for attempt in range(4):
        try:
            mb.tidy_save(pid, ours, snap, log=log)
            break
        except OSError as e:
            log("save check retry %d: %r" % (attempt + 1, e))
            time.sleep(6)
    if drv.error:
        raise drv.error
    return S


def press(drv, names, hold=4, wait=40):
    """Generator step: hold the named buttons on pad 0 for `hold` ticks, then release and wait."""
    alias = {"UP": "DPAD_UP", "DOWN": "DPAD_DOWN", "LEFT": "DPAD_LEFT", "RIGHT": "DPAD_RIGHT"}
    names = [alias.get(n.strip().upper(), n) for n in names]
    drv.pads[0] = mr.PadState(mr.button_mask(names))
    yield hold
    drv.pads[0] = mr.PadState()
    yield wait


def menu_flow(actions, seconds, film):
    def factory(S, rig, drv, video):
        def gen():
            yield from mr.boot_to_profile(rig, drv)
            S.meta = {"menu_state": rig.rd32(mr.MENU_STATE)}
            if video:
                S.cap = yield from mr.start_master_capture(video)
                S.log("capture: %s" % ("filming" if S.cap else "NOT available"))
                S.meta["epoch"] = time.monotonic() - S.t0
            S.label = "idle"
            S.ev(rig, "mark", "idle_start")
            yield int(seconds * 30)
            S.ev(rig, "mark", "idle_end")
            for a in actions:
                S.label = a
                S.ev(rig, "action", a)
                if a.startswith("wait:"):
                    yield int(a[5:])
                    continue
                yield from press(drv, a.split("+"))
                S.ev(rig, "state", rig.rd32(mr.MENU_STATE))
            S.label = "end"
            yield 30
            if S.cap:
                S.meta["epoch_wall"] = S.cap.epoch() - S.t0
                S.cap.stop()
                S.cap = None
        return gen()
    return factory


def cc_flow(ticks_menu, cam_plan, lean=False):
    """Main menu constant/draw windows, then a versus battle with the camera logged every pass."""
    def factory(S, rig, drv, video):
        def gen():
            yield from mr.boot_to_profile(rig, drv)
            yield 90
            S.win = False
            for k in range(0 if lean else 3):                    # three windows, 1.5 s apart, closed by event count
                S.label = "menu_win%d" % k
                S.nwin = 0
                S.win = True
                waited = 0
                while S.win and waited < 200:
                    yield 1
                    waited += 1
                S.win = False
                yield 45
            S.label = "menu_done"
            if cam_plan is None:
                return
            yield from mr.launch_versus(rig, drv)
            anchors = (rig.fighter_pos(0), rig.fighter_pos(1))
            drv.pads = [mr.PadState(), mr.PadState()]
            yield from mr.settle_placement(rig, drv, 60.0, anchors, {})
            S.cam = True
            for label, pad0, ticks in cam_plan:
                S.label = label
                S.ev(rig, "phase", label)
                drv.pads[0] = pad0
                n = 0
                while n < ticks:
                    S.ev(rig, "fighter", [rig.fighter_pos(0), rig.fighter_pos(1)])
                    yield 1
                    n += 1
            drv.pads[0] = mr.PadState()
            S.cam = False
        return gen()
    return factory


def cc_hit(S, rig, name, vals, tick):
    if getattr(S, "win", False):
        S.nwin = getattr(S, "nwin", 0) + 1
        if S.nwin > 2500:
            S.win = False
            return
    if name in ("cv", "cp") and getattr(S, "win", False):
        slot, ptr, cnt = vals[0], vals[1], vals[2]
        n = cnt if isinstance(cnt, int) and 1 <= cnt <= 8 else 1
        raw = rig.read(ptr, 16 * n) if ptr and ptr > 0x10000 else None
        extra = None
        if name == "cv" and slot == 90:
            r = rig.ctx.bytes()
            rsp = struct.unpack_from("<Q", r, lt.CTX_RSP_OFF)[0]
            stack = rig.read(rsp, 0x40) or b""
            rets = [hex(x - rig.base + 0x140000000) for x in struct.unpack("<%dQ" % (len(stack) // 8), stack)
                    if rig.base <= x < rig.base + rig.size]
            extra = [hex(ptr or 0), rets]
        S.ev(rig, name, [slot, cnt, raw.hex() if raw else None, extra])
    elif name in ("idx", "imm") and getattr(S, "win", False):
        raw = rig.ctx.bytes()
        rsp = struct.unpack_from("<Q", raw, lt.CTX_RSP_OFF)[0]
        ret = struct.unpack("<Q", rig.read(rsp, 8))[0] - rig.base + 0x140000000
        mem = rig.read(vals[3], 0x30) if name == "idx" and vals[3] and vals[3] > 0x10000 else None
        S.ev(rig, name, [vals[0], vals[1], hex(vals[2] or 0), hex(vals[3] or 0), hex(ret), mem.hex() if mem else None])
    elif name == "pass" and getattr(S, "cam", False):
        reg = rig.rd(CAM_REGION, 0x140)
        S.ev(rig, "pass", [rig.rd32(VIEW_INDEX, True), reg.hex() if reg else None])


def snd_hit(S, rig, name, vals, tick):
    if name == "snd":
        S.ev(rig, "snd", [vals[0], vals[1], vals[2]])


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mode", choices=["film", "sounds", "consts", "camera"])
    ap.add_argument("--seconds", type=float, default=5.0)
    ap.add_argument("--actions", default="")
    ap.add_argument("--out", default=os.path.join(mr.OUT_ROOT, "menu"))
    ap.add_argument("--no-video", action="store_true")
    ap.add_argument("--lean", action="store_true", help="camera: only the scene-pass hook (fast game), no menu windows")
    a = ap.parse_args(argv)
    actions = [x for x in a.actions.split(",") if x.strip()]
    if a.mode in ("film", "sounds"):
        run(a.mode, a.out, menu_flow(actions, a.seconds, True), ["0x%x:snd:ecx,edx,r8d" % SND], snd_hit,
            video=not a.no_video)
    else:
        P = mr.PadState
        B = mr.BUTTONS
        plan = None
        if a.mode == "camera":
            plan = [("still", P(), 90),
                    ("free_fwd", P(0, 0, 32767), 300),
                    ("free_back", P(0, 0, -32767), 300),
                    ("free_left", P(0, -32767, 0), 300),
                    ("free_right", P(0, 32767, 0), 300),
                    ("free_still2", P(), 60),
                    ("lock_press", P(B["RT"]), 12),
                    ("lock_still", P(), 90),
                    ("lock_strafe", P(0, 32767, 0), 90),
                    ("lock_fwd", P(0, 0, 32767), 90),
                    ("lock_back", P(0, 0, -32767), 90)]
        specs = [] if a.lean else ["0x%x:%s:ecx,rdx,r8d" % (v, k) for k, v in CONST_HOOKS.items()]
        specs += [] if a.lean else ["0x%x:%s:rcx,rdx,r8,r9" % (v, k) for k, v in DRAW_HOOKS.items()]
        specs += ["0x%x:pass:rcx" % SCENE_PASS]
        run(a.mode, a.out, cc_flow(None, plan, a.lean), specs, cc_hit, watch_s=60.0 if a.lean else 150.0, video=False)
    return 0


if __name__ == "__main__":
    sys.exit(main())
