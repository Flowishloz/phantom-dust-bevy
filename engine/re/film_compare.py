#!/usr/bin/env python3
"""Film comparison: every skill filmed from the ORIGINAL (move recorder takes) against OUR game.

For each skill the same scenario is run in pd_game (same skill in slot 0, same distance / lock /
Level / Aura / Health, the same opponent skill for defences, the press at the same tick with ticks
mapped 30 Hz -> 60 Hz after our 180-tick countdown), our frames are saved at the same landmarks, and
a pair sheet is built: the original's frames on top, ours below, a text strip of numeric differences.

  python film_compare.py run  [--ids 80,75] [--limit N] [--jobs 3] [--redo] [--real] [--no-split]   # play ours + build sheets
      (--split, the default: our normal side-by-side 2-player split at 1440x900 like the takes; PD_WINDOW hook)
  python film_compare.py index                                                # index.csv + INDEX.html

Hooks it uses (all env vars, inert otherwise; pd_game main.rs "Film comparison hooks"):
  PD_FILM, PD_FILM_DIST, PD_FILM_VIEW, PD_FILM_OPP, PD_FILM_LOG, plus PD_TEST_SPAWN / PD_TEST_PRESS /
  PD_SHOT_TICKS / PD_SFX_LOG / PD_MUTE and `--near --stage st06`.
Outputs go to %USERPROFILE%\\.pdtools\\compare (never commit them).
"""
import argparse
import csv
import json
import math
import os
import re
import subprocess
import sys
import concurrent.futures as cf

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mr_contact_sheet as mcs  # noqa: E402
from PIL import Image, ImageDraw, ImageFont  # noqa: E402

HOME = os.path.expanduser("~")
MOVEREC = os.path.join(HOME, ".pdtools", "moverec")
TAKES = os.path.join(MOVEREC, "takes_full")
OUT = os.environ.get("PD_COMPARE_OUT") or os.path.join(HOME, ".pdtools", "compare")   # lanes running side by side set their own
WORK = os.path.join(OUT, "_work")
ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
MAIN = r"<repo>"
CACHE = os.environ.get("PD_FILM_CACHE") or os.path.join(MAIN, "engine", "cache")   # PD_FILM_CACHE: a scratch cache (collision A/B)
EXE = os.environ.get("PD_GAME_EXE", os.path.join(ROOT, "target-lane", "debug", "pd_game.exe"))
COUNTDOWN = 180            # our sim ticks before the fight starts (combat::COUNTDOWN_TICKS)
SPAWNS = "-46.6,-260.166;90,-260.166"   # st06, open flat catwalk; PD_FILM_DIST then sets the distance
TILE_W = 400
# Our collision moves a fighter that starts at some anchors 3-5 u (the original's trace stands on the exact start point): 130 of the 522 real
# starts of the propsound run, in a clean bimodal split (392 under 1 u, none between 1 and 2, the rest 2-5 u). The original's wall / prop
# there is not ours, so the take is not reproduced; the flat catwalk is the fair scenario. (Collision is the collide lane's.)
START_OFF_TOL = 2.0
Y_OFF_TOL = 6.0   # a start spot counts as off when the floor under it is more than this from the take's own height
# PD_FILM_REAL=1 / `run --real`: start the fighters at the ORIGINAL take's own start coordinates (x, z; the floor under y + 2)
# instead of two points on the flat catwalk at the same distance. The takes were filmed on st06 (its floor heights 330.0 / 270.0
# match the traces), so a wall, a slope or a ledge that stopped or helped a shot there is then in our run too. A spot with no
# floor leaves our start point unchanged, which `analyze` notes.
REAL = bool(os.environ.get("PD_FILM_REAL"))
# `run --split` (the default for a full run; `--no-split` for the old full-window view): our normal 2-player split at the
# original footage's 1440x900 (PD_WINDOW), side by side like the original (PD_TEST_SPLIT=sbs; the takes are LEFT/RIGHT, P1 on
# the left, not top/bottom), no PD_FILM_VIEW. Sheets crop both pictures to the P1 half (CROP: x0,y0,x1,y1 of the 1440x900 frame).
SPLIT = True
WINDOW = "1440x900"
# FAIR FRAMING (2026-10-02; DECISIONS: our layout stays top/bottom). The original's 2-player picture is side by side: P1's 3D view is a
# letterboxed widescreen strip, 720x385, at x 0..720, y 186..571 of the 1440x900 frame (HUD in the black bars). Ours is the normal
# top/bottom split: P1's strip is the top half, 1440x450 (aspect 3.2, vs the original's 1.87). Both are scaled to the SAME height and
# aspect: ours is centre-cropped to 1.87 (842x450, its fighter is centred). `PD_COMPARE_FULLSTRIP=1` keeps ours' whole 3.2 strip.
ORIG_STRIP = (0, 186, 720, 571)
OURS_STRIP = (0, 0, 1440, 450)
CROP = ORIG_STRIP            # kept for old callers: the original's P1 strip
NOISE_SOUNDS = {"0.4", "0.30", "0.31", "0.26", "0.27"}   # 0.4 lock-on cue (the recorder toggles lock, we do not); 0.30/0.31 and 0.26/0.27 are FOOTSTEPS
# on two floor classes of the recorder's arena (READ: FUN_1402311e0 plays DAT_1402f6270[class*8 + col] + foot index; class 2/3/5 -> 28/30 -> 30/31,
# class 4/6 -> 24/26 -> 26/27). The takes' walkers (the opponent stumbling, the confused, the knocked-up) step whatever the skill; the footstep
# sound is Laurence-approved and protected (common 35 / wet 52, 54), so these are set aside here, not "fixed".
NOISE_BANKS = ("1.",)                    # bank 1 = UI sounds, in most takes whatever the skill
NOISE_EFFECTS = {"600"}    # logged once at tick 0 in every run (not a skill effect)

# tolerances (original 30 Hz ticks / points)
TOL_TICK = 2
TOL_AURA = 0.5


def mine(t):
    """Original 30 Hz recorder tick -> our 60 Hz match tick."""
    return COUNTDOWN + 2 * int(t)


def theirs(t):
    return (t - COUNTDOWN) / 2.0


def fnum(x, d=None):
    try:
        return float(x)
    except (TypeError, ValueError):
        return d


# ------------------------------------------------------------------ original side
def load_orig(skill_id, name):
    import glob
    hits = sorted(glob.glob(os.path.join(glob.escape(TAKES), "%03d_*_trace.csv" % skill_id)))
    if not hits:
        return None
    base = hits[0][: -len("_trace.csv")]
    jp, tp = base + ".json", base + "_trace.csv"
    if not (os.path.exists(jp) and os.path.exists(tp)):
        return None
    meta = json.load(open(jp, encoding="utf-8"))
    rows = mcs.read_trace(tp)
    if not rows:
        return None
    return {"meta": meta, "rows": rows, "base": base, "dir": TAKES}


def orig_sounds_effects(rows):
    sounds, effects = {}, {}
    for r in rows:
        t = int(r["rec_tick"])
        for e in [x for x in r["events"].split("|") if x]:
            n, _, a = e.partition(":")
            if n == "sound":
                bank = a.split(".")[0]
                key = "V" if bank in ("3", "4") else a
                sounds.setdefault(key, t)
            elif n == "effect":
                effects.setdefault(a.split(".")[-1], t)
    return sounds, effects


def row_at(rows, t):
    t = max(0, min(t, len(rows) - 1))
    return rows[t]


def orig_metrics(o, end=None):
    """`end`: compare only up to this original tick (our match restarted its round there)."""
    rows, meta = o["rows"], o["meta"]
    if end is not None:
        rows = rows[: end + 1]
    first = rows[0]
    press, fire = meta.get("press_tick"), meta.get("fire_tick")
    m = {"press": press, "fire": fire}
    # The cast state's end (castpose lane): the first tick from the press that P0's state is no longer 10 / 11 / 12.
    m["cast_end"] = None
    if press is not None and press < len(rows) and fnum(rows[press].get("p0_state")) in (10, 11, 12):
        # A movement special (s43 = 3: Dash, Jump, Skywalk...) leaves the cast state at its fire tick for the program's state 3
        # ("special move", PROGRAM-RE.md state list) and keeps the body there for the dash; our sim keeps `Cast` for the whole
        # move (the program pauses the Aura refill and the controls in both states alike), so the end of the move is the end
        # of the original's state-3 run when the cast went straight into it (LIVE `240_Dash`: 10 at 39, 3 from 40 to 59).
        d = skill_record(meta.get("skill")) if meta.get("skill") is not None else None
        move_special = bool(d) and d["raw"][43] == 3
        in_cast = (10, 11, 12)
        for r in rows[press:]:
            st = fnum(r.get("p0_state"))
            if st not in in_cast:
                if move_special and st == 3 and in_cast == (10, 11, 12):
                    in_cast = (3,)
                    continue
                m["cast_end"] = int(r["rec_tick"])
                break
    h0 = fnum(first["p1_health"])
    m["hit"] = None
    base = h0
    prev_row = rows[0]
    for r in rows:
        h = fnum(r["p1_health"])
        # A -3 with the body jumping across the stage is the respawn after falling off an edge (`FALL_OUT_DAMAGE`), not a hit
        # (LIVE `313_Vacuum_Palm`: P1 flung off the arena at tick 138, -3, and sent back to its spawn).
        fell = fnum(prev_row["p1_health"]) is not None and h is not None and fnum(prev_row["p1_health"]) - h == 3             and math.dist([fnum(prev_row["p1_" + a]) for a in "xyz"], [fnum(r["p1_" + a]) for a in "xyz"]) > 40
        prev_row = r
        if fell:
            base -= 3
        if h is not None and base is not None and h < base:
            m["hit"] = int(r["rec_tick"])
            m["hit_damage"] = base - h
            break
    end = len(rows) - 1
    p = (press if press is not None else 0)
    m["end"] = end
    ref = row_at(rows, max(0, p - 1))
    last = rows[-1]
    for who in ("p0", "p1"):
        m[who + "_aura"] = fnum(last[who + "_aura"]) - fnum(ref[who + "_aura"])
        m[who + "_level"] = fnum(last[who + "_level"]) - fnum(ref[who + "_level"])
        m[who + "_health"] = fnum(last[who + "_health"]) - fnum(ref[who + "_health"])
    m["sounds"], m["effects"] = orig_sounds_effects(rows)
    m["p1_dy"] = fnum(last["p1_y"]) - fnum(first["p1_y"])   # where P1 ended up relative to its start (a ledge / a lower floor)
    # The takes were recorded on a stage with edges: a body that falls off respawns with -3 Health
    # (`FALL_OUT_DAMAGE`, combat.rs). Those drops are the stage, not the skill: counted, then set aside.
    m["fall_outs"] = {"p0": 0, "p1": 0}
    prev = rows[0]
    for r in rows[1:]:
        for who in ("p0", "p1"):
            dh = fnum(prev[who + "_health"]) - fnum(r[who + "_health"])
            jump = math.dist([fnum(prev[who + "_" + a]) for a in "xyz"], [fnum(r[who + "_" + a]) for a in "xyz"])
            if dh == 3 and jump > 40:
                m["fall_outs"][who] += 1
                m[who + "_health"] += 3
        prev = r
    return m


def _yaw(v):
    """Trace facing (u16-ish, 32768 per turn (the program's 0x7fff angle mask; checked against atan2(dx,dz) on locked takes), same sense as sim::yaw_of = atan2(dx, dz)) -> radians in (-pi, pi]."""
    f = fnum(v)
    if f is None:
        return None
    a = (f / 32768.0) * 2 * math.pi
    return math.atan2(math.sin(a), math.cos(a))


def field_from_take(o, rec):
    """The capsules the original take's stage held when it started, as (P0 Aura Particles, P1 Aura Particles, P0 other, P1 other), or
    None when the skill does not read the field (the film then plays the empty one it always did).

    The trace does not log the field; it shows only what a field-reading skill did to it. The recorder's stage held 3 capsules per
    fighter (Violent Change erased 6 with six common/92, Particle Convert and Change the World show 6 capsule effects at their fire
    tick), and the number of those that were Aura Particles is read from the skill's own outcome:
      * Meltdown (s51 = 49, "Level -s52 per own Aura Particle"): each fighter's Level loss / s52 (LIVE `102_Meltdown`: P0 -4, P1 -2 -> 2 and 1);
      * Overheat (s45 = 30, "STR = Aura Particles on the battlefield", times s46): the Health it took / s46 (LIVE `086_Overheat`: 3), split P0 first;
      * Evaporate (field skill s44 = 2): the number of common/92 erase sounds (LIVE `100_Evaporate`: 2), split P0 first;
      * Violent Change (field skill s44 = 5, s45 = 0): the number of common/92 sounds = every capsule (6), 3 each;
      * Particle Convert (s45 = 3) / Change the World (s45 = 2): the number of the skill's own effect events at the fire tick (6), 3 each."""
    if not rec:
        return None
    raw, cap = rec["raw"], rec.get("capsule")
    rows, meta = o["rows"], o["meta"]
    om = orig_metrics(o)
    split = lambda n: (min(3, (n + 1) // 2), min(3, n // 2))
    if cap == "erase" and raw[51] == 49 and raw[52]:
        p0, p1 = (int(round(-(om.get(w + "_level") or 0) / raw[52])) for w in ("p0", "p1"))
        p0, p1 = max(0, min(3, p0)), max(0, min(3, p1))
        return (p0, p1, 3 - p0, 3 - p1)
    if cap == "attack" and raw[45] == 30 and raw[46]:
        n = int(round(-(om.get("p1_health") or 0) / raw[46]))
        if n > 0:
            a, b = split(n)
            return (a, b, 3 - a, 3 - b)
        return None
    if cap == "erase" and raw[43] == 1 and (raw[44], raw[45]) in ((2, 0), (5, 0), (5, 2), (5, 3)):
        if (raw[44], raw[45]) in ((2, 0), (5, 0)):
            n = sum(e.split(":")[1] == "0.92" for r in rows for e in r["events"].split("|") if e.startswith("sound:"))
        else:
            fire = meta.get("fire_tick")
            fx = str(raw[2])
            n = sum(e == "effect:0." + fx for r in rows if fire is not None and int(r["rec_tick"]) == fire for e in r["events"].split("|"))
        if raw[44] == 2:
            a, b = split(n)
            return (a, b, 3 - a, 3 - b) if n else None
        return (0, 0, 3, 3) if n else None
    return None


def to_sim_xz(xz):
    """The original's (x, z) -> ours. The program's world is the stage `.dat` as stored; every export of ours (collision, render) is
    right-handed with z mirrored (INDEX.md: positions (x, y, -z)), so a spot of the original's trace is at (x, -z) in our world.
    Checked on 48k trace samples off the flat decks (y not 330 / 270): the body stands on the 0x94 floor at the take's z for 97% of
    them unmirrored, 31% mirrored. Before 2026-10-02 the real placement used z as is, i.e. the mirror image of the stage."""
    if os.environ.get("PD_FILM_NOMIRROR"):      # A/B switch: the pre-2026-10-02 placement (z as is)
        return [xz[0], xz[1]]
    return [xz[0], -xz[1]]


def to_sim_yaw(a):
    """A facing in the original's space (atan2(dx, dz)) -> ours: z mirrored turns it into pi - a."""
    if a is None or os.environ.get("PD_FILM_NOMIRROR"):
        return a
    b = math.pi - a
    return math.atan2(math.sin(b), math.cos(b))


def scenario(o, skill_row):
    rows, meta = o["rows"], o["meta"]
    f = rows[0]
    dist = math.hypot(fnum(f["p0_x"]) - fnum(f["p1_x"]), fnum(f["p0_z"]) - fnum(f["p1_z"]))
    sc = {"skill": int(meta["skill"]), "level": int(fnum(f["p0_level"], 12)), "aura": int(round(fnum(f["p0_aura"], 12))),
          "hp0": int(fnum(f["p0_health"], 20)), "hp1": int(fnum(f["p1_health"], 40)),
          "lock": 1 if meta.get("lock") else 0, "dist": round(dist, 2),
          "p0xyz": [fnum(f["p0_x"]), fnum(f["p0_z"]), fnum(f["p0_y"])], "p1xyz": [fnum(f["p1_x"]), fnum(f["p1_z"]), fnum(f["p1_y"])],
          "press": meta.get("press_tick") if meta.get("press_tick") is not None else 39,
          "opp_skill": None, "opp_press": None,
          "yaw0": _yaw(f["p0_facing"]), "yaw1": _yaw(f["p1_facing"])}
    try:      # the take's own P1 lock-on at the press (an unlocked P1 keeps its start facing; ours turns a locked one to the foe)
        pt = sc["press"]
        sc["p1lock"] = 1 if str(rows[max(0, min(len(rows) - 1, pt - 1))].get("p1_lock", "0")).strip() not in ("", "0") else 0
    except Exception:
        sc["p1lock"] = None
    sc["field"] = field_from_take(o, skill_record(sc["skill"]))
    osk = (skill_row.get("opponent_skill") or "").strip()
    if osk:
        sc["opp_skill"] = int(osk)
        op = meta.get("opp_press_tick")
        sc["opp_press"] = op if op is not None else sc["press"] + 15
    return sc


# ------------------------------------------------------------------ our side
def landmark_ticks(o):
    return mcs.landmarks(o["rows"])


def placement_off(T, sc):
    """Names of the fighters whose logged start spot is more than START_OFF_TOL (2 u) from the original's (no floor at that spot on st06:
    the game then leaves them at its own start points; or our collision pushed them off a wall the original stands against) or whose
    floor is not at the take's own height (Y_OFF_TOL)."""
    if not T or not sc.get("p0xyz"):
        return []
    r0 = T[min(len(T) - 1, COUNTDOWN)]
    out = []
    for w, xi, zi, yi, key in (("P0", 7, 8, 16, "p0xyz"), ("P1", 14, 15, 17, "p1xyz")):
        want = to_sim_xz(sc[key])
        if math.hypot(fnum(r0[xi]) - want[0], fnum(r0[zi]) - want[1]) > START_OFF_TOL:
            out.append(w)
        # The floor under the take's spot must also be at the take's own height: st06 has a floor 20 u lower under some takes' spots
        # (`027_Wall`: the original stood at y 328.3, ours drops to 308.3 and slides off the edge), so the take was filmed on another
        # level / stage: the flat catwalk at the same distance is the fair scenario there. (Old logs without p0_y / p1_y skip this.)
        elif len(r0) > yi and fnum(r0[yi]) is not None and abs(fnum(r0[yi]) - sc[key][2]) > Y_OFF_TOL:
            out.append(w)
    return out


def run_ours(sc, lms, workdir, real=None, tries=3):
    """run_ours_once, repeated (up to `tries`) while frames are missing: several games at once can lose the GPU device together."""
    for _ in range(tries):
        files = run_ours_once(sc, lms, workdir, real)
        if all(os.path.exists(f) for f in files.values()):
            break
    return files


def run_ours_once(sc, lms, workdir, real=None):
    real = REAL if real is None else real
    os.makedirs(workdir, exist_ok=True)
    for f in os.listdir(workdir):
        if f.startswith("s_t") or f in ("log.csv", "out.txt"):
            os.remove(os.path.join(workdir, f))
    ticks = sorted({mine(t) for _l, t in lms})
    env = dict(os.environ)
    env.update({
        "PD_FILM": "%d,0,%d,%d,%d,%d,%d,0,%d" % (sc["skill"], sc["level"], sc["hp0"], sc["hp1"], sc["lock"],
                                                  sc["opp_skill"] if sc["opp_skill"] is not None else -1, sc["aura"]),
        "PD_FILM_DIST": str(sc["dist"]), "PD_FILM_LOG": os.path.join(workdir, "log.csv"),
        "PD_TEST_SPAWN": SPAWNS, "PD_TEST_PRESS": "%d,0" % mine(sc["press"]),
        "PD_SHOT_TICKS": ",".join(str(t) for t in ticks), "PD_SFX_LOG": "1", "PD_FX_LOG": "1", "PD_MUTE": "1",
    })
    if os.environ.get("PD_COMPARE_NOBOOM"):      # off by default: the original's camera also comes in near walls (takes 1, 60, 80 show it)
        env["PD_FILM_NOBOOM"] = "1"
    if SPLIT:
        env["PD_WINDOW"] = WINDOW
        env["PD_TEST_SPLIT"] = "tb"
        env.pop("PD_FILM_VIEW", None)
    else:
        env["PD_FILM_VIEW"] = "1"
    if real and sc.get("p1lock") is not None and not os.environ.get("PD_COMPARE_NOP1LOCK"):
        env["PD_FILM_P1LOCK"] = str(sc["p1lock"])
    if real and sc.get("p0xyz"):
        a, b = sc["p0xyz"], sc["p1xyz"]
        (ax, az), (bx, bz) = to_sim_xz(a), to_sim_xz(b)
        env["PD_TEST_SPAWN"] = "%.2f,%.2f,%.2f;%.2f,%.2f,%.2f" % (ax, az, a[2] + 2, bx, bz, b[2] + 2)
        env.pop("PD_FILM_DIST", None)
        if sc.get("yaw0") is not None:     # the take's own start yaws (trace p0_facing / p1_facing, 32768 per turn)
            env["PD_FILM_YAW"] = "%.5f,%.5f" % (to_sim_yaw(sc["yaw0"]), to_sim_yaw(sc["yaw1"]))
    if sc.get("field"):
        env["PD_FILM_FIELD"] = ",".join(str(n) for n in sc["field"])
    if sc["opp_press"] is not None:
        env["PD_FILM_OPP"] = "%d,0" % mine(sc["opp_press"])
    for k in ("PD_TEST_HEALTH", "PD_TEST_AURA", "PD_FREE_CAM", "PD_TEST_STICK", "PD_TRACE"):
        env.pop(k, None)
    with open(os.path.join(workdir, "out.txt"), "wb") as out:
        subprocess.run([EXE, CACHE, "--near", "--stage", "st06", "--screenshot", os.path.join(workdir, "s.png")],
                       env=env, stdout=out, stderr=subprocess.STDOUT, timeout=120 + max(ticks) // 2)
    return {t: os.path.join(workdir, "s_t%04d.png" % t) for t in ticks}


def sound_key(path):
    m = re.match(r"sfx/([^/]+)/(\d+)\.wav", path)
    if not m:
        return None
    folder, n = m.group(1), m.group(2)
    if folder == "common":
        return "0." + n
    return "V" if folder.startswith("BattleVo") else "%s.%s" % (folder, n)


def parse_ours(workdir):
    """Rows of log.csv plus sounds / effects with the original tick (30 Hz) they happened at."""
    T, E = [], []
    p = os.path.join(workdir, "log.csv")
    if os.path.exists(p):
        for line in open(p, encoding="utf-8", errors="replace"):
            parts = line.rstrip("\n").split(",", 2 if line.startswith("E,") else 99)
            if parts[0] == "T" and parts[1].isdigit():
                T.append(parts)
            elif parts[0] == "E":
                E.append((int(parts[1]), parts[2]))
    sounds, effects = {}, {}
    pending_s, pending_f = [], []
    op = os.path.join(workdir, "out.txt")
    if os.path.exists(op):
        for line in open(op, encoding="utf-8", errors="replace"):
            m = re.search(r"sfx: play (\S+) at", line)
            if m:
                pending_s.append(m.group(1))
                continue
            m = re.search(r"sfx: object sound (\d+) at", line)
            if m:
                pending_s.append("sfx/common/%s.wav" % m.group(1))
                continue
            m = re.search(r"sfx: travel loop (\d+) ", line)
            if m:
                pending_s.append("sfx/common/%s.wav" % m.group(1))
                continue
            m = re.search(r"sfx: environment hum (\d+) ", line)   # the crystal's hum loop (common/87), logged apart from `play`
            if m:
                pending_s.append("sfx/common/%s.wav" % m.group(1))
                continue
            m = re.search(r"\[film\] fx fx/eft0*(\d+)\.glb", line)
            if m:
                pending_f.append(m.group(1))
                continue
            m = re.search(r"\[film\] frame tick (\d+)", line)
            if m:
                t = theirs(int(m.group(1)))
                for s in pending_s:
                    k = sound_key(s)
                    if k:
                        sounds.setdefault(k, t)
                for f in pending_f:
                    effects.setdefault(f, t)
                pending_s, pending_f = [], []
    return T, E, sounds, effects


def our_metrics(T, E, sc, end):
    m = {"press": sc["press"], "fire": None, "hit": None, "cut": None, "cast_end": None}
    if not T:
        return m
    # Our cast hold's end: the first row from the press whose action is no longer Cast / Lunge (state 10 / 11 / 12's counterpart).
    seen = False
    for r in T:
        if int(r[1]) < mine(sc["press"]):
            continue
        casting = r[5].startswith(("Cast", "Lunge"))
        seen = seen or casting
        if seen and not casting:
            m["cast_end"] = theirs(int(r[1]))
            break
    for t, ev in E:
        if "SkillFired" in ev and "who: 0" in ev:
            m["fire"] = theirs(t)
            break
    byt = {int(r[1]): r for r in T}
    # Our match restarts a round some seconds after a knock-out (health 20/20, Level 0, Aura 0); the
    # original takes stop before that, so rows from the restart on are not part of the comparison.
    seen_level, reset_at = False, None
    for t in sorted(byt):
        r = byt[t]
        if t < COUNTDOWN:
            continue
        if fnum(r[3]) and fnum(r[3]) > 0 or fnum(r[10]) and fnum(r[10]) > 0:
            seen_level = True
        elif seen_level and fnum(r[3]) == 0 and fnum(r[10]) == 0 and fnum(r[2]) == 20 and fnum(r[9]) == 20 and fnum(r[4]) == 0 and fnum(r[11]) == 0:
            reset_at = t
            break
    cut = None
    if reset_at is not None:
        byt = {t: r for t, r in byt.items() if t < reset_at}
        cut = int(theirs(reset_at - 1))
    if m["fire"] is None and byt:   # skills that log no SkillFired: the tick P1's Aura is first paid
        base = fnum(byt[min(byt, key=lambda k: abs(k - mine(max(0, sc["press"] - 1))))][4])
        for t in sorted(byt):
            if t >= mine(max(0, sc["press"] - 1)) and fnum(byt[t][4]) is not None and base is not None and fnum(byt[t][4]) < base - 0.5:
                m["fire"] = theirs(t)
                break

    if m["fire"] is None:
        # Skills that never log a SkillFired for P0: the Aura Particle (its own event), Guardian Angel (the guard
        # that fires itself shows as the attack it blocked), and cost-0 melee (the swing starts: Lunge -> Cast).
        for t, ev in E:
            if ("AuraParticleUsed" in ev and "who: 0" in ev) or ("Blocked" in ev and "victim: 0" in ev):
                m["fire"] = theirs(t)
                break
    if m["fire"] is None:
        prev = None
        for t in sorted(byt):
            # the swing starts on the Lunge -> Cast change (a charge holds in Cast before its Lunge, so Cast alone is not it)
            if t >= mine(max(0, sc["press"] - 1)) and byt[t][5].startswith("Cast") and prev is not None and prev.startswith("Lunge"):
                m["fire"] = theirs(t)
                break
            prev = byt[t][5]
    m["cut"] = cut
    ticks_sorted = sorted(byt)

    def at(t):
        """Our row at tick t, or the nearest earlier one we have (a run can end early)."""
        t = max(COUNTDOWN, t)
        ok = [k for k in ticks_sorted if k <= t]
        return byt[ok[-1] if ok else ticks_sorted[0]]

    # A fall-out (`FALL_OUT_DAMAGE`: Health -3 and the body is put back on its start spot) is the stage, not a hit: found first, so the hit
    # search below skips it. The respawn is a jump across the stage or UP from below the floor it fell to (a body that slid 9 u off an
    # edge and comes back 14 u away is not a 20 u jump, `207_Iron_Skin`: ours' P1 slides off the deck after its Blaster's root motion, the
    # original's stops at the edge; collision lane). Old logs without the y columns keep the 20 u rule only.
    keys = [t for t in sorted(byt) if COUNTDOWN <= t <= mine(end)]
    falls = {"p0": [], "p1": []}
    for a, b in zip(keys, keys[1:]):
        for who, (hi, xi, zi, yi) in (("p0", (2, 7, 8, 16)), ("p1", (9, 14, 15, 17))):
            dh = fnum(byt[a][hi]) - fnum(byt[b][hi])
            jump = math.hypot(fnum(byt[a][xi]) - fnum(byt[b][xi]), fnum(byt[a][zi]) - fnum(byt[b][zi]))
            up = len(byt[a]) > yi and fnum(byt[b][yi]) is not None and fnum(byt[b][yi]) - fnum(byt[a][yi]) > 6
            if dh == 3 and (jump > 20 or up):
                falls[who].append(b)
    h0 = fnum(at(mine(max(0, sc["press"] - 1)))[9]) if byt else None
    for t in sorted(byt):
        r = byt[t]
        if t > mine(end):      # the take stopped here: a drain / hit of ours after its last tick has nothing to be compared with (`161_Turbulence`: our powerless -1 at t180, the take ends t176)
            break
        h = fnum(r[9]) + 3 * sum(1 for f in falls["p1"] if f <= t) if fnum(r[9]) is not None else None
        if t >= mine(max(0, sc["press"] - 1)) and h is not None and h0 is not None and h < h0:
            m["hit"] = theirs(t)
            m["hit_damage"] = h0 - h
            break
    ref = at(mine(max(0, sc["press"] - 1)))
    last = at(mine(end))
    for who, (hi, li, ai) in (("p0", (2, 3, 4)), ("p1", (9, 10, 11))):
        m[who + "_aura"] = fnum(last[ai]) - fnum(ref[ai])
        m[who + "_level"] = fnum(last[li]) - fnum(ref[li])
        m[who + "_health"] = fnum(last[hi]) - fnum(ref[hi])
    # Same set-aside as the original's (a respawn: Health -3 and the body jumps across the stage).
    m["fall_outs"] = {w: len(falls[w]) for w in ("p0", "p1")}
    for who in ("p0", "p1"):
        m[who + "_health"] += 3 * len([f for f in falls[who] if f <= mine(end)])
    return m


def diff(om, um, os_, es, ou, eu):
    """List of (kind, text) mismatches between the original metrics and ours."""
    out = []

    def tick_cmp(kind, a, b):
        if a is None and b is None:
            return
        if a is None or b is None:
            out.append((kind, "%s: original %s, ours %s" % (kind, "none" if a is None else "t%d" % a, "none" if b is None else "t%.1f" % b)))
        elif abs(a - b) > TOL_TICK:
            out.append((kind, "%s: original t%d, ours t%.1f (%+.1f)" % (kind, a, b, b - a)))
    tick_cmp("fire_tick", om["fire"], um["fire"])
    tick_cmp("hit_tick", om["hit"], um["hit"])
    if om.get("cast_end") is not None and um.get("cast_end") is not None:
        tick_cmp("cast_end", om["cast_end"], um["cast_end"])
    if om.get("hit_damage") is not None and um.get("hit_damage") is not None and om["hit_damage"] != um["hit_damage"]:
        out.append(("hit_damage", "hit damage: original %g, ours %g" % (om["hit_damage"], um["hit_damage"])))
    for who in ("p0", "p1"):
        for stat, tol in (("aura", TOL_AURA), ("level", 0.5), ("health", 0.5)):
            a, b = om.get(who + "_" + stat), um.get(who + "_" + stat)
            if a is not None and b is not None and abs(a - b) > tol:
                out.append(("%s_%s" % (who, stat), "%s %s change: original %+.2f, ours %+.2f" % (who, stat, a, b)))
    def noisy(k):
        return k in NOISE_SOUNDS or k.startswith(NOISE_BANKS)
    # Our run goes on for the whole landmark schedule; the take ends where the recorder stopped. A sound of ours after the take's last
    # tick (an Aura regen tick, a get-up) has nothing to be compared with, so it is not an "extra".
    if om.get("end") is not None:
        ou = {k: t for k, t in ou.items() if t <= om["end"] + TOL_TICK}
    miss = sorted(k for k in os_ if k not in ou and not noisy(k))
    extra = sorted(k for k in ou if k not in os_ and not noisy(k))
    if miss:
        out.append(("sound_missing", "sounds missing in ours: " + " ".join(miss)))
    if extra:
        out.append(("sound_extra", "sounds extra in ours: " + " ".join(extra)))
    miss = sorted(k for k in es if k not in eu)
    extra = sorted(k for k in eu if k not in es and k not in NOISE_EFFECTS)
    if miss:
        out.append(("effect_missing", "effects missing in ours (model-less effects are not logged): " + " ".join(miss)))
    if extra:
        out.append(("effect_extra", "effects extra in ours: " + " ".join(extra)))
    return out


# ------------------------------------------------------------------ sheet
def font(sz):
    for p in (r"C:\Windows\Fonts\consola.ttf", r"C:\Windows\Fonts\arial.ttf"):
        if os.path.exists(p):
            return ImageFont.truetype(p, sz)
    return ImageFont.load_default()


def fit(im, w):
    return im.resize((w, int(im.height * w / im.width)), Image.LANCZOS) if im is not None else None


def crop_ours():
    """Our top strip (1440x450) centre-cropped to the original strip's aspect, 720:385."""
    x0, y0, x1, y1 = OURS_STRIP
    if os.environ.get("PD_COMPARE_FULLSTRIP"):
        return OURS_STRIP
    w = int(round((y1 - y0) * (ORIG_STRIP[2] - ORIG_STRIP[0]) / (ORIG_STRIP[3] - ORIG_STRIP[1])))
    cx = (x0 + x1) // 2
    return (cx - w // 2, y0, cx - w // 2 + w, y1)


def tile(im, w, h, label, orig=False):
    t = Image.new("RGB", (w, h), (30, 30, 30))
    if im is not None:
        im = im.convert("RGB")
        if SPLIT:    # P1's strip of the 1440x900 frame, same aspect for both
            im = im.resize((1440, 900), Image.LANCZOS) if im.size != (1440, 900) else im
            im = im.crop(ORIG_STRIP if orig else crop_ours())
        im = fit(im, w)
        t.paste(im, (0, 0))
    d = ImageDraw.Draw(t)
    d.rectangle([0, 0, w, 15], fill=(0, 0, 0))
    d.text((3, 1), label, fill=(255, 255, 0), font=font(12))
    return t


def build_sheet(path, title, o, lms, ours_files, diffs, sc):
    meta = o["meta"]
    video = os.path.join(o["dir"], meta["video"]) if meta.get("video") else None
    vt = mcs.video_times(meta, o["rows"])
    th = TILE_W * (ORIG_STRIP[3] - ORIG_STRIP[1]) // (ORIG_STRIP[2] - ORIG_STRIP[0]) if SPLIT else TILE_W * 9 // 16
    top, bot = [], []
    for label, t in lms:
        im = None
        if video and os.path.exists(video):
            im = mcs.grab(video, vt[min(t, max(vt))], 1440 if SPLIT else TILE_W)
        top.append(tile(im, TILE_W, th, "ORIG %s t%d" % (label, t), orig=True))
        f = ours_files.get(mine(t))
        bot.append(tile(Image.open(f) if f and os.path.exists(f) else None, TILE_W, th, "OURS %s t%d" % (label, mine(t))))
    lines = ["scenario: skill %d  level %d aura %d  hp %d/%d  dist %.1f  lock %d  press t%d  opponent skill %s @ t%s" % (
        sc["skill"], sc["level"], sc["aura"], sc["hp0"], sc["hp1"], sc["dist"], sc["lock"], sc["press"], sc["opp_skill"], sc["opp_press"]),
        "%d mismatches" % len(diffs)] + [" - " + d for _k, d in diffs]
    f12 = font(13)
    W = TILE_W * max(len(top), 4)
    H = 22 + th * 2 + 8 + 16 * len(lines) + 8
    sheet = Image.new("RGB", (W, H), (18, 18, 18))
    dr = ImageDraw.Draw(sheet)
    dr.text((4, 3), title, fill=(255, 255, 255), font=font(15))
    for i, (a, b) in enumerate(zip(top, bot)):
        sheet.paste(a, (i * TILE_W, 22))
        sheet.paste(b, (i * TILE_W, 22 + th))
    y = 22 + 2 * th + 6
    for i, ln in enumerate(lines):
        dr.text((6, y + 16 * i), ln[:W // 7], fill=(255, 120, 120) if i >= 2 else (200, 200, 200), font=f12)
    sheet.save(path)


# ------------------------------------------------------------------ driver
def skill_list():
    rows = list(csv.DictReader(open(os.path.join(MOVEREC, "all_skills.csv"), encoding="utf-8")))
    return [r for r in rows if r["id"].isdigit()]


_BLOCKED = None
BLOCKED_RADIUS = 30.0   # a take counts as inside a blocked region when P1 and P0 both start within this of a blocked take's


def blocked_spots():
    """Regions of the recorder's arenas where the original's ranged attacks never landed.

    The recorder teleports both fighters to anchor points on whatever stage the match was on and only
    checks that they stand at one height (`placement_verified`). The takes show the outcome follows the
    PLACEMENT, not the skill: from the same P0 anchor, every shell / arc / lob aimed at one P1 spot misses
    (P1 at (14.31, -63.85): Blaster, Storm Blade, Ki Cannon, Fire of Gehenna all miss) while the spots on
    the other side of the anchor give every one a hit. Short-range takes there show why: Dhampir Fang's
    dash stalls at z = -100.56 and Empty Hand's at z = 140.88, 14 u and 25 u short of the target, so
    something solid stands in front of P0 (a wall or prop of the original stage). This harness plays a flat
    open catwalk, so there is nothing to match.

    A P1 start spot (rounded to 1 u) is blocked when at least two ranged-attack takes sat on it and none
    hurt P1. Returns {spot: (take count, [P0 start of each take])}. Memoised."""
    global _BLOCKED
    if _BLOCKED is not None:
        return _BLOCKED
    spots = {}
    for r in skill_list():
        if not r["family"].startswith("att/") or r["family"] in ("att/punch", "att/sword"):
            continue
        o = load_orig(int(r["id"]), r["name"])
        if o is None or o["meta"].get("fire_tick") is None:
            continue
        f = o["rows"][0]
        key = (round(fnum(f["p1_x"]), 0), round(fnum(f["p1_z"]), 0))
        spots.setdefault(key, []).append((orig_metrics(o)["hit"] is not None, (fnum(f["p0_x"]), fnum(f["p0_z"]))))
    _BLOCKED = {k: (len(v), [p for _h, p in v]) for k, v in spots.items() if len(v) >= 2 and not any(h for h, _p in v)}
    return _BLOCKED


def in_blocked_region(first_row):
    """The blocked spot this take's placement lies in (P1 within BLOCKED_RADIUS of it and P0 within it of
    one of that spot's takes), else None."""
    p1 = (fnum(first_row["p1_x"]), fnum(first_row["p1_z"]))
    p0 = (fnum(first_row["p0_x"]), fnum(first_row["p0_z"]))
    for spot, (n, p0s) in blocked_spots().items():
        if math.dist(p1, spot) <= BLOCKED_RADIUS and any(math.dist(p0, q) <= BLOCKED_RADIUS for q in p0s):
            return spot, n
    return None


# diff kinds that only exist because ours landed a hit the original never did
HIT_ONLY = ("hit_tick", "hit_damage", "p1_health", "p1_aura", "p1_level")


def analyze(res, o, wd, sc, lms, files, sid, name, family):
    """Turn our run's log / output plus the original's trace into the diff list and the pair sheet."""
    T, E, us, ue = parse_ours(wd)
    om = orig_metrics(o)
    um = our_metrics(T, E, sc, om["end"])
    if um.get("cut") is not None and um["cut"] < om["end"]:
        # our round restarted after a knock-out before the take ends: compare both up to that moment
        om = orig_metrics(o, end=um["cut"])
        um = our_metrics(T, E, sc, om["end"])
    diffs = diff(om, um, om["sounds"], om["effects"], us, ue)
    res["notes"] = []
    for who in ("p0", "p1"):
        extra = (um.get("fall_outs") or {}).get(who, 0) - (om.get("fall_outs") or {}).get(who, 0)
        if extra > 0:
            res["notes"].append("harness: ours' %s fell off the stage %d time(s) (Health -3, respawn) where the original's did not; set aside as the stage / collision "
                                "(cast root motion slides ours off an edge the original stops at), not counted as a hit" % (who.upper(), extra))
    # A knocked body that ends on another level in the original (a lower floor, a ledge 60 u up) took, or was spared,
    # a landing's 1-2 Health that our flat catwalk cannot give: not the knock arc (apex 7.69 u from the same launch speed).
    h1 = um.get("p1_health"), om.get("p1_health")
    if abs(om.get("p1_dy") or 0) > 15 and None not in h1 and 0 < abs(h1[0] - h1[1]) <= 2:
        diffs = [(k, t) for k, t in diffs if k != "p1_health"]
        res["notes"].append("harness: the original's P1 ended %.0f u from its start height (a different floor); the 1-2 Health landing difference is set aside" % om["p1_dy"])
    f0 = o["rows"][0]
    blocked = in_blocked_region(f0) if om["hit"] is None and um["hit"] is not None and not res.get("real_used") else None
    if blocked:
        kept = [(k, t) for k, t in diffs if k not in HIT_ONLY]
        res["notes"].append("harness: this placement is in a region (blocked spot %.0f, %.0f, %d takes) where none of the original's ranged takes ever "
                            "landed a hit and short-range dashes stall short; ours hits on the open catwalk, %d hit-only mismatches set aside"
                            % (blocked[0][0], blocked[0][1], blocked[1], len(diffs) - len(kept)))
        diffs = kept
    diffs = set_aside_random(res, o, sc, om, um, diffs)
    diffs = set_aside_sound(res, o, sc, om, diffs)
    if res.get("real_used"):
        res["notes"].append("real placement: both fighters start at the original's coordinates on st06 (within 2 u)")
    elif res.get("real_off"):
        res["notes"].append("real placement not possible: %s have no floor of the take's own height on st06 at the original's start point, or our collision pushes them 2+ u off it (the recorder's arena there is not st06); flat catwalk at the same distance used" % "/".join(res["real_off"]))
    if not T:
        diffs.insert(0, ("our_run_failed", "our run produced no log"))
    if om["fire"] is None:
        diffs.insert(0, ("original_no_fire", "the original take never fired (recorder / script problem): numbers below are not a fair comparison"))
    gone = [t for t in files if not os.path.exists(files[t])]
    if gone:
        diffs.insert(0, ("our_frames_missing", "our run is missing %d frame(s) (crash or timeout; rerun)" % len(gone)))
    res.update(sc=sc, orig=om, ours=um, diffs=diffs, landmarks=lms, files={str(k): v for k, v in files.items()},
               osounds=sorted(om["sounds"]), usounds=sorted(us), oeffects=sorted(om["effects"]), ueffects=sorted(ue))
    sheet = os.path.join(wd, "sheet.png")
    build_sheet(sheet, "%03d %s  [%s]" % (sid, name, family), o, lms, files, diffs, sc)
    res["sheet"] = sheet
    res["status"] = "ok"


_SKILLS = None


def skill_record(sid):
    """The skill record of `skills.json` in the main cache (rule fields for the set-asides below)."""
    global _SKILLS
    if _SKILLS is None:
        _SKILLS = {s["id"]: s for s in json.load(open(os.path.join(CACHE, "skills.json"), encoding="utf-8"))["skills"]}
    return _SKILLS.get(sid)


def set_aside_random(res, o, sc, om, um, diffs):
    """Two things the original draws at random with `rand()` (seeded from the clock): a film cannot match the draw, so only the
    SET of outcomes is compared.
      * STR rule 28 (Excaliber, Jack Pot, Debris Bullet): STR = s46 x {0, 1, 2}, rolled once per cast (`FUN_140274a10`,
        `FUN_1402626d0`). A damage difference between two members of that set is a different roll, not a different rule.
      * Rain clouds (motion 20; Rain of Rocks, Celestial Ship, Lucifer's Arrow ...): each drop starts at a random point within s70 x 0.1 u
        of the cloud with a random lean (`FUN_14026b8d0`), about a third of them touch a body that stands still under the cloud, and the
        drops come every s67 + 1 program ticks from s64 + 1 after the fire tick, so WHICH drop hits (and so the tick) is the draw. A hit
        inside the schedule's window is the same outcome."""
    rec = skill_record(sc["skill"])
    if not rec:
        return diffs
    raw = rec["raw"]
    if rec.get("capsule") == "attack" and raw[45] == 28:
        s46 = raw[46]
        allowed = {0, s46, 2 * s46}
        keep = []
        for k, t in diffs:
            if k == "hit_damage" and om.get("hit_damage") in allowed and um.get("hit_damage") in allowed:
                continue
            if k in ("p0_health", "p1_health") and abs(om.get(k) or 0) in allowed and abs(um.get(k) or 0) in allowed:
                continue
            keep.append((k, t))
        if len(keep) != len(diffs):
            res["notes"].append("harness: STR rule 28 draws 0, %d or %d at random (rolled once per cast): the damage mismatch is a different draw, set aside" % (s46, 2 * s46))
        diffs = keep
    if rec.get("capsule") == "attack" and raw[63] == 20 and om.get("hit") is not None and um.get("hit") is not None and om.get("fire") is not None and um.get("fire") is not None:
        first, gap, drops, height, speed = raw[64] + 1, raw[67] + 1, raw[66], raw[69] * 0.1, (rec.get("s70_73") or [0, 0, 0, 0])[2] / 10.8
        fall = height / speed if speed else 12.0
        lo, hi = first + fall - 3, first + (drops - 1) * gap + fall + 3
        if lo <= om["hit"] - om["fire"] <= hi and lo <= um["hit"] - um["fire"] <= hi:
            keep = [(k, t) for k, t in diffs if k != "hit_tick"]
            if len(keep) != len(diffs):
                res["notes"].append("harness: rain drops land at random points (about a third touch a standing body): both hits fall inside the drop schedule "
                                    "(%.0f-%.0f program ticks after the fire tick; original +%d, ours +%.1f), the tick mismatch is which drop, set aside"
                                    % (lo, hi, om["hit"] - om["fire"], um["hit"] - um["fire"]))
            diffs = keep
    # A blast grows 0 -> full over 30 program ticks (`FUN_14026ae10`): a body at the very edge of the radius (Timer Mine / Fast Bomb:
    # P1 exactly 50 u from a 50 u mine) is touched ~26 ticks after the detonation (LIVE `323_Fast_Bomb`: P0 at 67, P1 at 93). The Timer
    # Mine take stops 25 ticks after its blast, one tick before that, so ours hitting P1 at the take's very end is the same outcome.
    if om.get("hit") is None and um.get("hit") is not None and (om.get("p0_health") or 0) < 0:
        h0 = fnum(o["rows"][0]["p0_health"])
        blast = next((int(r["rec_tick"]) for r in o["rows"] if fnum(r["p0_health"]) is not None and fnum(r["p0_health"]) < h0), None)
        if blast is not None and 20 <= um["hit"] - blast <= 30 and um["hit"] >= om["end"] - 4:
            keep = [(k, t) for k, t in diffs if k not in ("hit_tick", "p1_health")]
            if len(keep) != len(diffs):
                res["notes"].append("harness: the original's blast (tick %d) was still growing when the take ended (tick %d); a body at the radius edge is reached ~26 ticks after "
                                    "(Fast Bomb: P1 at +26), ours at +%.1f, set aside" % (blast, om["end"], um["hit"] - blast))
            diffs = keep
    # Rule 30 (Overheat): STR = Aura Particles lying on the battlefield. Our film clears the field (no capsules), so the original's
    # 3 (LIVE `086_Overheat`: both fighters -3) is 0 here: a setup difference.
    if rec.get("capsule") == "attack" and raw[45] == 30 and (om.get("p1_health") or 0) < 0 and (um.get("p1_health") or 0) == 0:
        keep = [(k, t) for k, t in diffs if k not in ("hit_tick", "hit_damage", "p0_health", "p1_health")]
        if len(keep) != len(diffs):
            res["notes"].append("harness: Overheat's STR counts the Aura Particles on the battlefield (the original's stage had %d); our film field is empty, so ours deals 0, set aside"
                                % abs(om.get("p1_health") or 0))
        diffs = keep
    # An erase-everything skill (Calamity) leaves our film with an empty arsenal and no capsules, so the powerless drain (-1 Health to
    # whoever has nothing left, on our 180-tick clock) starts; the original's players still hold their decks. The drain's rate is itself UNCONFIRMED.
    if om.get("hit") is None and um.get("hit") is not None and (um.get("p0_health") or 0) == (um.get("p1_health") or 0) < 0             and (mine(um["hit"]) - COUNTDOWN) % 180 == 0 and abs(om.get("p0_health") or 0) < 0.5 and abs(om.get("p1_health") or 0) < 0.5:
        keep = [(k, t) for k, t in diffs if k not in ("hit_tick", "p0_health", "p1_health")]
        if len(keep) != len(diffs):
            res["notes"].append("harness: both fighters -1 on our powerless clock (every 180 ticks, nothing left in the film's arsenal); the original's decks are not empty, set aside")
        diffs = keep
    return diffs


def _drop_sounds(diffs, kind, ids):
    """`diffs` without the sound ids `ids` in the `kind` entry ("sound_missing" / "sound_extra"); an emptied entry goes."""
    out, dropped = [], []
    for k, t in diffs:
        if k == kind:
            head, _, rest = t.partition(": ")
            keep = [x for x in rest.split() if x not in ids]
            dropped += [x for x in rest.split() if x in ids]
            if keep:
                out.append((k, "%s: %s" % (head, " ".join(keep))))
            continue
        out.append((k, t))
    return out, dropped


def set_aside_sound(res, o, sc, om, diffs):
    """Sound mismatches the take cannot be fair about (2026-10-03, soundfill2). Each is a property of the RECORDING, not of the game:
      * the recorder never fired the skill (5, 11, 14, 180, 205, 206, 299): nothing of the take's skill sounds is comparable;
      * `capsule_touched` (118, 181, 188, 196, 295): the original's dash ran over a capsule of its stage and captured it (common/86, the
        pick-up sound); our film field is empty (`PD_FILM_FIELD` only places what the skill's own outcome implies);
      * an attack / erase skill with an erase-on-hit filter (s48 = 1-5 a capsule colour, 7 everything but Aura Particles) plays common/91
        (`0x14026fb8e`) when the victim's hand held a skill of that kind. The recorder's decks were DEALT at random per launch
        (MOVE-RECORDER.md, rule 2 Quick), ours holds only Aura Particles, so the victim's hand is unknown: 122, 135, 215, 216, 217, 315 have
        the sound, 128, 134, 187, 309, 324 (same filters) do not. Filters 6 / 8 / 9 DO match an Aura Particle and stay compared."""
    rec = skill_record(sc["skill"])
    meta = o["meta"]
    out = list(diffs)
    if om.get("fire") is None:
        for kind in ("sound_missing", "sound_extra"):
            out, dropped = _drop_sounds(out, kind, {x for k, t in out if k == kind for x in t.partition(": ")[2].split()})
            if dropped:
                res["notes"].append("harness: the recorder never fired this skill; its %s sounds (%s) are set aside" % (kind.split("_")[1], " ".join(dropped)))
        return out
    if meta.get("capsule_touched"):
        out, dropped = _drop_sounds(out, "sound_missing", {"0.86"})
        if dropped:
            res["notes"].append("harness: the original's dash ran over a capsule of its stage (take json capsule_touched): the pick-up sound common/86 is set aside")
    if rec and rec["raw"][43] == 0 and rec["raw"][48] in (1, 2, 3, 4, 5, 7):
        out, dropped = _drop_sounds(out, "sound_missing", {"0.91"})
        if dropped:
            res["notes"].append("harness: the skill erases a button of the victim on hit (filter s48 = %d); the original's victim held a dealt hand we cannot see, ours holds only "
                                "Aura Particles: common/91 is set aside" % rec["raw"][48])
    return out


def do_one(r, redo=False):
    sid, name = int(r["id"]), r["name"]
    wd = os.path.join(WORK, "%03d" % sid)
    res_path = os.path.join(wd, "result.json")
    if os.path.exists(res_path) and not redo:
        old = json.load(open(res_path))
        crashed = any("our run is missing" in t for _k, t in old.get("diffs", []))   # a GPU device loss killed the game: rerun
        if old.get("status") != "error" and not crashed:
            return old
    os.makedirs(wd, exist_ok=True)
    res = {"id": sid, "name": name, "family": r["family"], "orig_rank": r["rank"], "diffs": [], "status": "ok"}
    o = load_orig(sid, name)
    if o is None:
        res.update(status="no_original", diffs=[("no_original", "no original take recorded")])
        json.dump(res, open(res_path, "w"))
        return res
    try:
        sc = scenario(o, r)
        lms = landmark_ticks(o)
        files = run_ours(sc, lms, wd)
        if REAL:
            off = placement_off(parse_ours(wd)[0], sc)
            if off:      # the original's spot is not on st06's floor: the flat catwalk at the same distance instead
                res["real_off"] = off
                files = run_ours(sc, lms, wd, real=False)
            else:
                res["real_used"] = True
        analyze(res, o, wd, sc, lms, files, sid, name, r["family"])
    except Exception as e:  # keep the batch going
        res.update(status="error", diffs=[("error", "tool error: %r" % e)])
    json.dump(res, open(res_path, "w"))
    if not os.environ.get("PD_COMPARE_KEEP"):    # the sheet holds the pictures; the 1440x900 frames are ~35 MB per skill (set PD_COMPARE_KEEP=1 to keep them)
        for f in os.listdir(wd):
            if f.startswith("s_t") and f.endswith(".png"):
                try:
                    os.remove(os.path.join(wd, f))
                except OSError:
                    pass
    return res


def cmd_run(a):
    global REAL, SPLIT
    SPLIT = not a.no_split
    if getattr(a, "real", False):
        REAL = True
    skills = skill_list()
    if a.ids:
        want = {int(x) for x in a.ids.split(",")}
        skills = [s for s in skills if int(s["id"]) in want]
    if a.limit:
        skills = skills[: a.limit]
    os.makedirs(WORK, exist_ok=True)
    done = 0
    with cf.ThreadPoolExecutor(a.jobs) as ex:
        for res in ex.map(lambda s: do_one(s, a.redo), skills):
            done += 1
            print("%3d/%d %s %s: %d mismatches %s" % (done, len(skills), res["id"], res["name"], len(res["diffs"]), res["status"]), flush=True)
    return cmd_index(a)


def cmd_reanalyze(_a):
    """Redo the diffs and sheets from the logs and frames already on disk (no game runs)."""
    n = 0
    for r in skill_list():
        sid = int(r["id"])
        wd = os.path.join(WORK, "%03d" % sid)
        o = load_orig(sid, r["name"])
        if o is None or not os.path.exists(os.path.join(wd, "log.csv")):
            continue
        res = {"id": sid, "name": r["name"], "family": r["family"], "orig_rank": r["rank"]}
        try:
            sc = scenario(o, r)
            lms = landmark_ticks(o)
            files = {mine(t): os.path.join(wd, "s_t%04d.png" % mine(t)) for _l, t in lms}
            analyze(res, o, wd, sc, lms, files, sid, r["name"], r["family"])
        except Exception as e:
            res.update(status="error", diffs=[("error", "tool error: %r" % e)])
        json.dump(res, open(os.path.join(wd, "result.json"), "w"))
        n += 1
    print("reanalyzed", n)
    return cmd_index(_a)


def cmd_index(_a):
    results = []
    for d in sorted(os.listdir(WORK)) if os.path.isdir(WORK) else []:
        p = os.path.join(WORK, d, "result.json")
        if os.path.exists(p):
            results.append(json.load(open(p)))
    # weight: structural kinds first, then count
    def score(r):
        return (-len(r["diffs"]), int(r["id"]))
    results.sort(key=score)
    for f in os.listdir(OUT):
        if re.match(r"^\d+_\d+_.*\.png$", f):
            os.remove(os.path.join(OUT, f))
    with open(os.path.join(OUT, "index.csv"), "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["rank", "id", "name", "family", "mismatches", "kinds", "details", "sheet"])
        items = []
        for i, r in enumerate(results, 1):
            png = ""
            if r.get("sheet") and os.path.exists(r["sheet"]):
                png = "%03d_%03d_%s.png" % (i, r["id"], r["name"].replace(" ", "_").replace("/", "_"))
                with open(r["sheet"], "rb") as s, open(os.path.join(OUT, png), "wb") as d:
                    d.write(s.read())
            kinds = " ".join(k for k, _t in r["diffs"])
            w.writerow([i, r["id"], r["name"], r["family"], len(r["diffs"]), kinds, " | ".join(t for _k, t in r["diffs"]), png])
            items.append((i, r, png, kinds))
    html = ["<!doctype html><meta charset=utf-8><title>Film compare</title><body style='background:#111;color:#ddd;font:14px sans-serif'>",
            "<h1>Original vs ours, %d skills, worst first</h1><p>Top row of each sheet: original footage; bottom: our game.</p>" % len(items)]
    for i, r, png, kinds in items:
        html.append("<h3 id='s%d'>%d. %s (id %d, %s): %d mismatches</h3><div style='color:#f99'>%s</div>" % (
            i, i, r["name"], r["id"], r["family"], len(r["diffs"]), kinds))
        if png:
            html.append("<img src='%s' style='max-width:100%%'>" % png)
    open(os.path.join(OUT, "INDEX.html"), "w", encoding="utf-8").write("\n".join(html))
    print("index.csv + INDEX.html written,", len(items), "skills")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--ids")
    r.add_argument("--limit", type=int)
    r.add_argument("--jobs", type=int, default=3)
    r.add_argument("--redo", action="store_true")
    r.add_argument("--real", action="store_true", help="start at the original's own coordinates on st06 (PD_FILM_REAL)")
    r.add_argument("--split", action="store_true", default=True, help="our normal 2-player split at 1440x900, sheets cropped to the P1 half (default)")
    r.add_argument("--no-split", action="store_true", help="the old PD_FILM_VIEW full-window view")
    r.set_defaults(fn=cmd_run)
    ra = sub.add_parser("reanalyze")
    ra.set_defaults(fn=cmd_reanalyze)
    i = sub.add_parser("index")
    i.set_defaults(fn=cmd_index)
    a = ap.parse_args(argv)
    os.makedirs(OUT, exist_ok=True)
    return a.fn(a)


if __name__ == "__main__":
    sys.exit(main())
