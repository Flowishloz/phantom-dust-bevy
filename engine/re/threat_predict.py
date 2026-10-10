"""Reference model of the CPU's threat predictor `FUN_14020f9a0` (script native g4.f186), our own tool.

Read from the assembly (capstone; Ghidra loses the return value). Returns the predicted number of 30 Hz ticks
until an attack object reaches player `me`, or 0 for "not a threat". Pure arithmetic on plain numbers so the
formulas can be unit-tested; the world queries are callbacks. See engine/docs/FORMATS-CPU.md section 4.4.

Object fields (names ours; offsets are the program's object struct, `FUN_14020f4f0`):
  state +0x10, skill +0x14, age +0x18 (int), hitmask +0x24, attacker +0x40 (player index of the caster),
  target +0x48 (None, or the player index it homes on), dir +0xf0, pos +0x100, radius +0x190,
  f1c0/f1c4/f1cc/f1d0 (float views of +0x1c0/c4/cc/d0), i1cc/i1d4 (int views of +0x1cc/+0x1d4), yaw (int view of +0x1d0).
Skill record fields (stride 0x90, `DAT_1404bdc50`): s43 (+0x52), s63 (+0x7a), s55 (+0x6a), s52 (+0x64),
  s64 (+0x7c), s65 (+0x7e).
"""
import math

NO_AIM_MOTIONS = (3, 12, 13, 14)   # FUN_140167910: plus s63 == 4 is excluded in the predictor itself
TICKS_PER_SECOND = 30.0            # DAT_1402f7fd8


def _sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _len(a):
    return math.sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2])


def _trunc(x):
    """cvttss2si: truncate toward zero; NaN / out of range gives INT_MIN like the CPU."""
    if x != x or abs(x) >= 2147483648.0:
        return -2147483648
    return int(x)


def _s16(v):
    v &= 0xFFFF
    return v - 0x10000 if v & 0x8000 else v


def closest_point_on_segment(p, a, b):
    """FUN_140193fd0: closest point to p on segment a-b (3-D)."""
    ab = _sub(b, a)
    l2 = ab[0] ** 2 + ab[1] ** 2 + ab[2] ** 2
    ap = _sub(p, a)
    if l2 == 0.0:
        return a
    t = (ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / l2
    t = max(0.0, min(1.0, t))
    return (a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t)


def straight_shot(pos, obj_pos, margin, radius, speed, ray_clear):
    """FUN_14020f8c0: object flying straight at pos. speed is per SECOND here (divided by 30)."""
    if not ray_clear(pos, obj_pos):
        return 0
    d = _len(_sub(pos, obj_pos)) - margin
    if speed < d:           # cannot cover the distance within one second: ignored
        return 0
    t = _trunc((d - radius) / (speed / TICKS_PER_SECOND))
    return max(t, 1)


def _free_segment(obj, pos, m2, v):
    """Unowned object flying along dir at v per tick for `age` ticks: closest approach to pos."""
    p0 = obj["pos"]
    n = float(obj["age"])
    e = (p0[0] + obj["dir"][0] * v * n, p0[1] + obj["dir"][1] * v * n, p0[2] + obj["dir"][2] * v * n)
    c = closest_point_on_segment(pos, p0, e)
    if m2 <= _len(_sub(pos, c)):
        return 0
    return _trunc(_len(_sub(p0, c)) / v)


def predict(obj, me, pos, margin, ground_ok, skill_of, ray_clear):
    """Ticks until `obj` reaches player `me` standing at pos (margin = my body radius, P+0x12e9c)."""
    if obj is None or obj["state"] == 0 or not (obj["hitmask"] >> me) & 1:
        return 0
    sk = skill_of(obj["skill"])
    if sk["s43"] != 0 or sk["s63"] in NO_AIM_MOTIONS or sk["s63"] == 4:
        return 0
    if obj["state"] == 3:
        return straight_shot(pos, obj["pos"], margin, obj["radius"], obj["f1c0"], ray_clear)
    k = sk["s63"]
    tgt = obj["target"]
    m2 = margin + obj["radius"]
    if k in (0, 11):
        v = obj["f1c0"] if k == 0 else obj["f1d0"]
        if tgt is not None:
            if tgt != me:
                return 0
            if sk["s55"] not in (2, 3) and not ray_clear(pos, obj["pos"]):
                return 0
            d = _sub(pos, obj["pos"])
            dist = _len(d)
            if dist == 0.0:
                return 0
            dot = sum(d[i] / dist * obj["dir"][i] for i in range(3))
            if dot <= 0.5:
                return 0
            t = _trunc((dist - m2) / v)
            return 0 if t > obj["age"] else t
        return _free_segment(obj, pos, m2, v)
    if k == 1:
        if tgt is not None:
            return obj["i1d4"] if tgt == me else 0
        a, vy, y0, py = obj["f1cc"], obj["f1c4"], obj["pos"][1], pos[1]
        t_apex = vy / a
        apex = abs(a * t_apex * t_apex * 0.5) + y0
        if py > apex:
            return 0
        fall = math.sqrt((apex - py) * 2.0 / a)
        t_total = t_apex + fall
        if (py > y0 and vy < 0.0) or t_total <= 0.0:
            return 0
        yaw = obj["yaw"] & 0x7FFF
        dd = t_total * obj["f1c0"]
        s, c = math.sin(2 * math.pi * yaw / 32768.0), math.cos(2 * math.pi * yaw / 32768.0)
        e = (obj["pos"][0] + s * dd, obj["pos"][1], obj["pos"][2] + c * dd)
        dist2d = math.hypot(pos[0] - e[0], pos[2] - e[2])
        hit_radius = _s16(sk["s52"]) * 0.1
        if hit_radius <= dist2d:
            return 0
        return _trunc(t_total)
    if k == 2:
        if tgt is None:
            return _free_segment(obj, pos, m2, obj["f1c0"])
        if tgt != me:
            return 0
        if sk["s55"] not in (2, 3) and not ray_clear(pos, obj["pos"]):
            return 0
        dy = pos[1] - obj["pos"][1]
        if 0.0 <= dy:
            return 0
        dist = _len(_sub(pos, obj["pos"]))
        return _trunc((dist - m2) / obj["f1c0"])
    if k == 5:
        if not ground_ok:
            return 0
        if tgt is None:
            return _free_segment(obj, pos, m2, obj["f1c4"])
        if tgt != me:
            return 0
        dist2d = math.hypot(pos[0] - obj["pos"][0], pos[2] - obj["pos"][2])
        return _trunc((dist2d - m2) / obj["f1c4"])
    if k == 6:
        return obj["i1cc"] if tgt is not None and tgt == me else 0
    if k in (7, 15, 16):
        if obj["attacker"] == me:
            return 0
        return obj["age"] - _s16(sk["s65"])
    if k == 9:
        return straight_shot(pos, obj["pos"], margin, obj["radius"], _s16(sk["s64"]) * 0.1, ray_clear)
    if k == 10:
        if not ground_ok:
            return 0
        p0 = (obj["pos"][0], 0.0, obj["pos"][2])
        w = float(obj["age"]) * obj["f1c4"]
        e = (p0[0] + obj["dir"][0] * w, obj["dir"][1] * w, p0[2] + obj["dir"][2] * w)
        flat = (pos[0], 0.0, pos[2])
        c = closest_point_on_segment(flat, p0, e)
        if m2 <= _len(_sub(flat, c)):
            return 0
        d2 = math.hypot(p0[0] - flat[0], p0[2] - flat[2])
        return _trunc((d2 - m2) / obj["f1c4"])
    return 0
