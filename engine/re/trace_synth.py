#!/usr/bin/env python3
"""Synthetic golden traces (stdlib only, Python 3.8+).

Generates an 'orig' trace (graded speed, 20 deg/tick turn limit, sticky
back-left/right side, jump wind-up + landing lock, one 40-tick ~80 u dodge)
and an 'ours' trace with DELIBERATE differences (stepped gait, direction snap,
BR/BL flips, instant take-off, no landing lock, old tumble/cartwheel numbers).
The numbers are made up; only the DIFFERENCES matter for testing the compare tool.

  python3 trace_synth.py OUTDIR [--scripts]     # writes orig.csv, ours.csv
Scenario ids/scripts mirror engine/docs/GOLDEN-TRACE.md section 3.
"""
import csv
import math
import os
import sys

HZ = 30
LEN = 150
WALK, RUN = 20.8, 55.6
B_JUMP, B_LOCK, B_DODGE = 1, 2, 4
COLUMNS = ["tick", "scenario_id", "stick_x", "stick_y", "buttons", "pos_x", "pos_y", "pos_z",
           "facing", "heading", "vertical_speed", "state", "clip_id", "clip_time",
           "lock_active", "lock_band", "speed_status", "grounded"]


def pol(mag, deg):
    a = math.radians(deg)
    return mag * math.sin(a), mag * math.cos(a)


def _wobble(t):
    x = 0.08 if ((t - 15) // 3) % 2 == 0 else -0.08
    return x, -math.sqrt(1 - x * x)


def _in(t, a, b):
    return a <= t < b


# id -> (name, locked, script(t) -> (x, y, buttons))
SCENARIOS = {
    "S01_steady_050": ("steady 0.5 push, free camera", 0, lambda t: (0, 0.5, 0) if _in(t, 15, 105) else (0, 0, 0)),
    "S02_steady_075": ("steady 0.75 push, free camera", 0, lambda t: (0, 0.75, 0) if _in(t, 15, 105) else (0, 0, 0)),
    "S03_release": ("full push then release", 0, lambda t: (0, 1, 0) if _in(t, 15, 75) else (0, 0, 0)),
    "S04_reversal_180": ("full push forward then 180 reversal", 0,
                         lambda t: (0, 1, 0) if _in(t, 15, 75) else ((0, -1, 0) if _in(t, 75, 135) else (0, 0, 0))),
    "S05_lock_forward": ("locked-on forward", 1, lambda t: (0, 1, B_LOCK if t == 0 else 0) if _in(t, 15, 105) else (0, 0, B_LOCK if t == 0 else 0)),
    "S06_lock_strafe_r": ("locked-on strafe right", 1, lambda t: (1, 0, 0) if _in(t, 15, 105) else (0, 0, B_LOCK if t == 0 else 0)),
    "S07_lock_strafe_l": ("locked-on strafe left", 1, lambda t: (-1, 0, 0) if _in(t, 15, 105) else (0, 0, B_LOCK if t == 0 else 0)),
    "S08_lock_pullback_wobble": ("locked-on pull back, wobble at dead-back", 1,
                                 lambda t: (_wobble(t) + (0,)) if _in(t, 15, 105) else (0, 0, B_LOCK if t == 0 else 0)),
    "S09_walk_to_run_ramp": ("stick ramp 0 to 1 over 90 ticks", 0,
                             lambda t: (0, (t - 15) / 89.0, 0) if _in(t, 15, 105) else (0, 0, 0)),
    "S10_jump": ("standing jump", 0, lambda t: (0, 0, B_JUMP if t == 15 else 0)),
    "S11_jump_run": ("jump while running", 0,
                     lambda t: (0, 1 if _in(t, 5, 60) else 0, B_JUMP if t == 40 else 0)),
    "S12_tumble": ("ground dodge, stick forward", 0,
                   lambda t: (0, 1 if _in(t, 10, 25) else 0, B_DODGE if t == 15 else 0)),
    "S13_cartwheel": ("ground dodge, stick right", 0,
                      lambda t: (1 if _in(t, 10, 25) else 0, 0, B_DODGE if t == 15 else 0)),
    "S14_ground_dodge": ("ground dodge, no stick", 0, lambda t: (0, 0, B_DODGE if t == 15 else 0)),
}


def wrap(a):
    return (a + 180.0) % 360.0 - 180.0


def simulate(sid, variant):
    """Return rows (list of dicts) for one scenario. variant: 'orig' | 'ours'."""
    _, locked, script = SCENARIOS[sid]
    orig = variant == "orig"
    x = z = y = vy = speed = 0.0
    facing = 0.0
    heading = 0.0
    lock = 0
    band = 0
    side = 1
    clip, clip_t, prev_clip = 10, 0, 10
    act = None  # dict(kind, t, ...)
    rows = []
    for t in range(LEN):
        sx, sy, btn = script(t)
        if btn & B_LOCK:
            lock = 1
        mag = math.hypot(sx, sy)
        stick_ang = math.degrees(math.atan2(sx, sy)) if mag >= 0.4 else None
        grounded = 1
        state = 0
        vel = None  # (dx, dz) per tick override
        if act is None and y == 0 and btn & B_JUMP:
            act = dict(kind="jump", t=0, wind=2 if orig else 0, vy=0.0, decay=0.98 if orig else 0.36)
            if not orig:
                vy = 85.0  # instant take-off
        if act is None and btn & B_DODGE:
            if orig:
                act = dict(kind="dodge", t=0, n=40, dist=80.0, rec=0, spd=None)
            elif sid == "S13_cartwheel":
                act = dict(kind="dodge", t=0, n=19, dist=114.0, rec=31, spd=6.0)
            else:
                act = dict(kind="dodge", t=0, n=15, dist=65.0, rec=0, spd=130.0 / HZ)
            act["dir"] = stick_ang if stick_ang is not None else facing
        if act and act["kind"] == "jump":
            a = act
            if a["t"] < a["wind"]:
                state, clip_base = 4, 40
            elif y > 0 or vy > 0 or a["t"] == a["wind"]:
                if a["t"] == a["wind"] and orig:
                    vy = 85.0
                state, clip_base = 5, 41
                steps = 1 if orig else 2
                for _ in range(steps):
                    vy -= 259.2 / HZ / steps
                    y += vy / HZ / steps
                if y <= 0:
                    y, vy = 0.0, 0.0
                    a["land"] = 11 if orig else 0
                    a["kind"] = "land"
                    a["t"] = -1
                else:
                    grounded = 0
                speed *= a["decay"]
                x += speed / HZ * math.sin(math.radians(facing))
                z += speed / HZ * math.cos(math.radians(facing))
            a["t"] += 1
            clip_id = 40 + (1 if state == 5 else 0)
        elif act and act["kind"] == "land":
            state, clip_id = (6, 42) if act["land"] > 0 else (0, 10)
            act["land"] -= 1
            speed *= 0.5
            if act["land"] <= 0:
                act = None
        elif act and act["kind"] == "dodge":
            a = act
            if a["t"] < a["n"]:
                if orig:
                    w = math.sin(math.pi * (a["t"] + 0.5) / a["n"])
                    tot = sum(math.sin(math.pi * (i + 0.5) / a["n"]) for i in range(a["n"]))
                    step = a["dist"] * w / tot
                else:
                    step = a["spd"]
                x += step * math.sin(math.radians(a["dir"]))
                z += step * math.cos(math.radians(a["dir"]))
                speed = step * HZ
                state, clip_id = 7, 50
            else:
                speed = 0.0
                state, clip_id = (9, 51) if a["t"] < a["n"] + a["rec"] else (0, 10)
            a["t"] += 1
            if a["t"] >= a["n"] + a["rec"]:
                act = None
        else:
            # ---- ordinary ground movement ----
            if orig:
                target = RUN * max(0.0, min(1.0, (mag - 0.3) / 0.6)) if mag >= 0.4 else 0.0
                lag = 0.3
            else:
                target = RUN if mag > 0.7 else (WALK if mag >= 0.4 else 0.0)
                lag = 0.5
            speed += (target - speed) * lag
            if speed < 0.3 and target == 0:
                speed = 0.0
            if stick_ang is not None:
                heading = stick_ang
            scale = 1.0
            if locked or lock:
                move_dir = heading if stick_ang is not None else facing
                face_to = 0.0
                if stick_ang is not None:
                    a = wrap(stick_ang)
                    if abs(a) <= 45:
                        nb = 1
                    elif abs(a) < 135:
                        nb = 2 if a > 0 else 3
                    else:
                        if orig and band in (4, 5):
                            nb = band  # sticky side
                        else:
                            nb = 4 if a > 0 else 5
                    band = nb
                else:
                    band = 0
                if orig:
                    d = wrap(face_to - facing)
                    facing += max(-20.0, min(20.0, d))
                else:
                    facing = face_to
                dirn = move_dir
            else:
                band = 0
                if stick_ang is not None:
                    if orig:
                        d = wrap(stick_ang - facing)
                        facing += max(-20.0, min(20.0, d))
                        scale = max(0.0, 1.0 - abs(wrap(stick_ang - facing)) / 90.0)
                    else:
                        facing = stick_ang
                dirn = facing
            facing = wrap(facing)
            x += speed * scale / HZ * math.sin(math.radians(dirn))
            z += speed * scale / HZ * math.cos(math.radians(dirn))
            moving = mag >= 0.4 or speed > 1.0
            gait = 2 if (speed >= 38 if orig else mag > 0.7) else 1
            state = 0 if not moving else (8 if band >= 2 else 1)
            clip_id = 10 if not moving else ((30 + band) if band else 20 + gait)
        if clip_id != prev_clip:
            clip_t = 0
        else:
            clip_t += 1
        prev_clip = clip_id
        rows.append(dict(tick=t, scenario_id=sid, stick_x=round(sx, 4), stick_y=round(sy, 4), buttons=btn,
                         pos_x=round(x, 4), pos_y=round(y, 4), pos_z=round(z, 4), facing=round(facing, 3),
                         heading=round(heading, 3), vertical_speed=round(vy, 3), state=state, clip_id=clip_id,
                         clip_time=clip_t, lock_active=lock, lock_band=band, speed_status=1.0, grounded=grounded))
    return rows


def generate(variant, ids=None):
    out = []
    for sid in (ids or SCENARIOS):
        out.extend(simulate(sid, variant))
    return out


def write_csv(path, rows, variant, columns=None):
    cols = columns or COLUMNS
    with open(path, "w", newline="") as f:
        f.write("# format: golden-trace v1\n# build: synthetic-%s\n# character: synthetic\n# stage: none\n" % variant)
        f.write("# scenario: all\n# hz: 30\n")
        w = csv.DictWriter(f, fieldnames=cols, extrasaction="ignore")
        w.writeheader()
        w.writerows(rows)


def write_scripts(outdir):
    """tick,stick_x,stick_y,buttons per scenario: the input script for our sim (PD_TRACE_SCRIPT)."""
    for sid, (_, _, script) in SCENARIOS.items():
        with open(os.path.join(outdir, sid + ".script.csv"), "w", newline="") as f:
            f.write("tick,stick_x,stick_y,buttons\n")
            for t in range(LEN):
                sx, sy, b = script(t)
                f.write("%d,%.4f,%.4f,%d\n" % (t, sx, sy, b))


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    d = argv[1]
    os.makedirs(d, exist_ok=True)
    write_csv(os.path.join(d, "orig.csv"), generate("orig"), "orig")
    write_csv(os.path.join(d, "ours.csv"), generate("ours"), "ours")
    if "--scripts" in argv:
        write_scripts(d)
    print("wrote", d)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
