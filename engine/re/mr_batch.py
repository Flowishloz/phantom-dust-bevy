#!/usr/bin/env python3
"""Unattended batch runner for the move recorder (our own code).

Builds the full skill list (all roster skills + the Aura Particle, in the usage order of
engine/docs/audits/SKILL-BY-SKILL.md, with a script per family) and records it for ~90 minutes
without anyone touching the PC:

  * watchdog: no game tick for 25 s, a take past its wall-clock limit, or a match that will not
    start -> the session ends, the game this run started is closed, the save is restored and
    hash-checked, the game is started again and the run carries on with the takes not done yet;
  * resume: a take with a finished <stem>.json + trace is never redone (use --no-resume to redo);
  * the save guard runs after EVERY session (each relaunch) and at the end;
  * the video is one master recording per chunk of 25 takes, cut into clips in the background;
  * summary.csv at the end: one row per take with press / fire / hit ticks and flags.

  python mr_batch.py --make-list [PATH]      write all_skills.csv from the ledger and skills.json
  python move_recorder.py --list all_skills.csv --out takes_full      (what tools/bats/RECORD_ALL_MOVES.bat runs)
"""
import argparse
import csv
import ctypes
import glob
import json
import os
import re
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import move_recorder as mr  # noqa: E402
import save_guard as sg  # noqa: E402

LEDGER = os.path.normpath(os.path.join(HERE, "..", "docs", "audits", "SKILL-BY-SKILL.md"))
ALL_SKILLS_CSV = os.path.join(mr.OUT_ROOT, "all_skills.csv")
FULL_OUT = os.path.join(mr.OUT_ROOT, "takes_full")
CHUNK_TAKES = 25            # takes per master recording
WATCH_S = 25.0              # no tick for this long = the game is stalled
MAX_FAILED_SESSIONS = 4     # sessions in a row that finished no take = give up
SESSION_TAKES = 6           # takes per game process: the game's main thread hangs inside the effect-manager update
                            # after about 7-10 in-battle relaunches (seen live: 5 of 5 sessions), so restart it first
ATTACKER_ID = 80            # Blaster: the generic attacker thrown at a defence
CSV_COLUMNS = ["rank", "id", "name", "family", "distance", "lock", "tail", "level", "health",
               "opponent_skill", "opponent", "flag"]

# ----------------------------------------------------------------------------- the list (pure)


def parse_ledger(path=LEDGER):
    """Rows (rank, id, name, family) from the ledger table, in its usage-rank order."""
    rows = []
    with open(path, encoding="utf-8") as f:
        for ln in f:
            m = re.match(r"\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*([^|]+?)\s*\|\s*([a-z]+/[^|]+?)\s*\|", ln)
            if m:
                rows.append((int(m[1]), int(m[2]), m[3], m[4]))
    return rows


MELEE = ("punch", "sword", "pursuit")


def family_plan(family):
    """(distance, lock, tail seconds, opponent, flag) for a ledger family such as 'att/shell'."""
    kind, sub = family.split("/", 1)
    distance, lock, tail, opp, flag = 60, True, 4.0, "", ""
    if sub in MELEE:
        distance = 35
    elif sub in ("arc", "lob", "fall"):
        distance, tail = 70, 5.0
    elif sub in ("rain", "rain-col", "tornado"):
        distance, tail = 70, 7.0
    elif sub in ("crawl", "hole"):
        distance, tail = 50, 6.0
    elif sub == "mine":
        distance, tail = 50, 6.0
        flag = "mine: the opponent never walks onto it (trigger not scripted)"
    elif sub == "object":
        distance, tail = 40, 5.0
    elif sub == "ricochet":
        tail = 6.0
    elif sub == "around-self":
        distance, tail = 30, 5.0
    elif sub == "erase-field":
        tail = 5.0
    elif sub == "movement":
        distance, lock, tail = 100, False, 4.0
        flag = "movement: pressed once, no stick direction scripted"
    elif sub == "self":
        lock, tail = False, 3.5
    elif sub == "instant":
        tail = 4.5
    if kind == "def":
        distance, lock, tail, opp = 35, True, 6.0, "attack"
        flag = "defence: the opponent casts Blaster at t+0.5 s (generic timing, unverified)"
    elif kind == "env":
        lock, tail = False, 6.0
    return distance, lock, tail, opp, flag


def requirement_setup(skill, base_level=12, base_health=20):
    """(level, health, flag) so the skill's own requirement and cost are met. Requirement kinds
    (skills.json requirement / requirement_amount, FORMATS-SKILLS): 1 health <= n, 2 health >= n,
    5 skills left in the arsenal <= n, 7 maximum aura, 8 level <= n, 9 level >= n."""
    level, health, flags = base_level, base_health, []
    kind, n = int(skill.get("requirement", 0) or 0), int(skill.get("requirement_amount", 0) or 0)
    if kind == 1:
        health = max(1, n)
    elif kind == 2:
        health = max(health, n)
    elif kind == 8:
        level = min(level, n)
    elif kind == 9:
        level = max(level, n)
    elif kind == 5:
        flags.append("requires few skills left in the arsenal: not set up")
    cost = int(skill.get("cost", 0) or 0)
    if level < cost:
        flags.append("Level %d is below the cost %d: the cast may be refused" % (level, cost))
    hc = int(skill.get("health_cost", 0) or 0)
    if hc and health <= hc:
        health = hc + 10
    return level, health, flags


def build_rows(ledger, skills):
    """Rows for all_skills.csv: one per ledger entry, in rank order."""
    by_id = {s["id"]: s for s in mr.unique_skills(skills)}
    out = []
    for rank, sid, name, family in ledger:
        sk = by_id.get(sid, {"id": sid, "name": name})
        distance, lock, tail, opp, flag = family_plan(family)
        level, health, rflags = requirement_setup(sk)
        flags = [f for f in [flag] + rflags if f]
        if sid == 0:
            flags.append("Aura Particle: standing press (the held-stick take is in the pilot)")
        out.append({"rank": rank, "id": sid, "name": name, "family": family, "distance": distance,
                    "lock": int(lock), "tail": tail, "level": level, "health": health,
                    "opponent_skill": ATTACKER_ID if opp else "", "opponent": opp, "flag": "; ".join(flags)})
    return out


def write_all_skills(path, ledger, skills):
    rows = build_rows(ledger, skills)
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=CSV_COLUMNS)
        w.writeheader()
        w.writerows(rows)
    return rows


def script_from_row(row, slot=0):
    """A Script from one all_skills.csv row."""
    tl = [{"t": 0.3, "do": "press", "buttons": "YXBA"[slot], "for": 0.1}]
    d = {"name": row["name"], "skill": int(row["id"]), "slot": slot, "distance": float(row["distance"]),
         "lock": bool(int(row["lock"])), "tail": float(row["tail"]), "level": int(row["level"]),
         "health": int(row["health"]), "timeline": tl, "attempts": 1}
    if row.get("opponent_skill"):
        d["opponent_skill"] = int(row["opponent_skill"])
        d["opponent_timeline"] = [{"t": 0.0, "do": "hold", "buttons": "RT", "for": float(row["tail"])},
                                  {"t": 0.5, "do": "press", "buttons": "Y", "for": 0.1}]
    return mr.parse_script(d)


def scripts_from_csv(path, skills):
    """[(Script, row)] for a list CSV. Rows with the full all_skills columns get their own script;
    a bare id/name list falls back to the generic per-capsule script."""
    by_id = {s["id"]: s for s in mr.unique_skills(skills)}
    out = []
    with open(path, newline="", encoding="utf-8") as f:
        lines = [ln for ln in f if ln.strip() and not ln.lstrip().startswith("#")]
    for r in csv.DictReader(lines):
        r = {k.strip(): (v or "").strip() for k, v in r.items() if k}
        if "distance" in r and r.get("distance"):
            out.append((script_from_row(r), r))
        else:
            sid = int(r["id"])
            sk = by_id.get(sid, {"id": sid, "name": r.get("name") or "skill%d" % sid, "capsule": "attack"})
            out.append((mr.parse_script(mr.default_script_for(sk)), r))
    return out


def take_done(out_dir, sc):
    """A take is finished when its json says so and its trace is there."""
    stem = mr.output_stem(sc.skill, sc.name)
    jp = os.path.join(out_dir, stem + ".json")
    if not (os.path.exists(jp) and os.path.exists(os.path.join(out_dir, stem + "_trace.csv"))):
        return False
    try:
        return bool(json.load(open(jp, encoding="utf-8")).get("finished"))
    except (OSError, ValueError):
        return False


def status_of(meta):
    if meta.get("crashed"):
        return "crashed"
    if meta.get("fired"):
        return "WRONG-SKILL" if meta.get("index_ok") is False else "ok"
    if meta.get("pressed"):
        return "no-fire"
    return "refused"


SUMMARY_COLUMNS = ["rank", "id", "name", "family", "status", "press_tick", "fire_tick", "hit_tick",
                   "index_ok", "capsule_touched", "video", "notes", "flag"]


def write_summary(out_dir, rows_by_id):
    """summary.csv from every finished take json in out_dir. Returns {status: count}."""
    items = []
    for jp in glob.glob(os.path.join(out_dir, "*.json")):
        try:
            m = json.load(open(jp, encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if not m.get("finished"):
            continue
        r = rows_by_id.get(m["skill"], {})
        items.append((int(r.get("rank") or 9999), m, r))
    items.sort(key=lambda t: (t[0], t[1]["skill"]))
    counts = {}
    with open(os.path.join(out_dir, "summary.csv"), "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=SUMMARY_COLUMNS)
        w.writeheader()
        for rank, m, r in items:
            st = status_of(m)
            counts[st] = counts.get(st, 0) + 1
            w.writerow({"rank": r.get("rank", ""), "id": m["skill"], "name": m["name"], "family": r.get("family", ""),
                        "status": st, "press_tick": m.get("press_tick"), "fire_tick": m.get("fire_tick"),
                        "hit_tick": m.get("hit_tick"), "index_ok": m.get("index_ok"),
                        "capsule_touched": m.get("capsule_touched"), "video": m.get("video") or "",
                        "notes": " | ".join(m.get("notes", [])), "flag": r.get("flag", "")})
    return counts


# ----------------------------------------------------------------------------- environment


def input_desktop_name():
    u = ctypes.windll.user32
    u.OpenInputDesktop.restype = ctypes.c_void_p
    h = u.OpenInputDesktop(0, False, 1)
    if not h:
        return "?"
    buf = ctypes.create_unicode_buffer(256)
    n = ctypes.c_uint()
    u.GetUserObjectInformationW(ctypes.c_void_p(h), 2, buf, 512, ctypes.byref(n))
    return buf.value


def screen_saver_on():
    return input_desktop_name() in ("Screen-saver", "Winlogon")


def wait_for_screen(log, max_s=600):
    """True when the desktop is usable (waits up to max_s while a screen saver / lock screen is on)."""
    t0 = time.time()
    warned = False
    while screen_saver_on():
        if not warned:
            log("the screen saver / lock screen is on: no game window can exist. Waiting for it to go off ...")
            warned = True
        if time.time() - t0 > max_s:
            return False
        time.sleep(10)
    return True


class CutQueue:
    """Cuts clips out of master recordings on background threads while the next takes are filmed."""

    def __init__(self, out_dir, keep_master=False):
        self.out_dir, self.keep_master, self.threads = out_dir, keep_master, []

    def submit(self, master_path, metas):
        t = threading.Thread(target=self._run, args=(master_path, list(metas)), daemon=True)
        t.start()
        self.threads.append(t)

    def _run(self, master_path, metas):
        if not os.path.exists(master_path):
            return
        want = len([m for m in metas if m.get("video_t0_s") is not None])
        got = mr.cut_takes(master_path, self.out_dir, metas)
        if got == want and not self.keep_master:
            try:
                os.remove(master_path)
                if os.path.exists(master_path + ".log"):
                    os.remove(master_path + ".log")
            except OSError:
                pass

    def join(self):
        for t in self.threads:
            t.join()


def recover_cuts(out_dir, cutq, log):
    """Masters left behind by a crashed run: cut the clips of finished takes that have none yet."""
    for master in glob.glob(os.path.join(out_dir, "_master_*.mp4")):
        stamp = os.path.basename(master)
        metas = []
        for jp in glob.glob(os.path.join(out_dir, "*.json")):
            try:
                m = json.load(open(jp, encoding="utf-8"))
            except (OSError, ValueError):
                continue
            if m.get("finished") and m.get("master") == stamp and not m.get("video") and m.get("video_t0_s") is not None:
                metas.append(m)
        if metas:
            log("recovering %d clip(s) from %s" % (len(metas), stamp))
            cutq.submit(master, metas)


def tidy_save(pid, ours, snap_dir, tidy=True, log=print):
    """Close the game instance THIS run started (never one that was already running), put any
    changed save file back from the pre-run snapshot and verify by hash. Run after every session."""
    report = {"closed_game": False, "restored": [], "differs_from_snapshot": None, "differs_from_backup": None}
    if ours and tidy and pid:
        subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True)
        for _ in range(30):
            if mr.find_game_pid() != pid:
                break
            time.sleep(0.5)
        report["closed_game"] = mr.find_game_pid() != pid
        log("closed the game instance this run started (pid %d)" % pid)
        time.sleep(1.5)
        for attempt in range(3):            # the game can still be flushing a file just after it is killed
            report["restored"] += sg.restore(sg.LIVE, snap_dir)
            if not sg.save_differences(sg.LIVE, snap_dir):
                break
            time.sleep(3.0)
        for k in sorted(set(report["restored"])):
            log("  restored from the pre-run snapshot: " + k)
    report["differs_from_snapshot"] = sg.save_differences(sg.LIVE, snap_dir)
    report["differs_from_backup"] = sg.save_differences(sg.LIVE, sg.BACKUP)
    log("save check: %d file(s) differ from the pre-run snapshot, %d from the 2026-10-01 backup%s" % (
        len(report["differs_from_snapshot"]), len(report["differs_from_backup"]),
        "" if not report["differs_from_snapshot"] else "   <-- LOOK AT THIS"))
    return report


# ----------------------------------------------------------------------------- one session


RUN = {"t0": time.time(), "started_games": 0}


def process_created_at(pid):
    """Creation time of a process as epoch seconds (None when it cannot be read)."""
    k = ctypes.windll.kernel32
    k.OpenProcess.restype = ctypes.c_void_p
    h = k.OpenProcess(0x1000, False, pid)          # PROCESS_QUERY_LIMITED_INFORMATION
    if not h:
        return None
    ft = [ctypes.c_ulonglong() for _ in range(4)]
    ok = k.GetProcessTimes(ctypes.c_void_p(h), *[ctypes.byref(x) for x in ft])
    k.CloseHandle(ctypes.c_void_p(h))
    if not ok:
        return None
    return ft[0].value / 1e7 - 11644473600


def kill_our_leftover_game(log):
    """A game process that this run started earlier and that is still lingering (a killed game can take a
    few seconds to go) would be mistaken for 'already running, not ours'. Close it if it was created after
    this run began; never touch an older one (Laurence's)."""
    for _ in range(20):
        pid = mr.find_game_pid()
        if not pid:
            return
        created = process_created_at(pid)
        if RUN["started_games"] and created is not None and created >= RUN["t0"] - 2:
            if _ == 0:
                log("a leftover game from this run is still there (pid %d): closing it" % pid)
            subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True)
            time.sleep(1.5)
        else:
            return


def run_session(pending, out_dir, skills, stage, video, unowned, relaunch, cutq, owned_csv, log, session_no):
    """Start (or attach to) the game, record `pending` [(Script, row)] until done or stalled.
    Returns {done, stalled, reason, pid, ours}."""
    kill_our_leftover_game(log)
    pid, ours = mr.launch_game()
    if ours and pid:
        RUN["started_games"] += 1
    if not pid:
        return {"done": 0, "stalled": True, "reason": "the game did not start", "pid": None, "ours": True}
    uniq = mr.unique_skills(skills)
    index_of = {s["id"]: mr.game_index(s) for s in uniq}
    index_of[0] = 0
    st = {"current": None, "anchors": None, "done": 0, "master": None, "master_path": None, "master_metas": [], "master_fail": 0,
          "owned": {}, "used": False}
    stamp = [0]

    def new_master():
        stamp[0] += 1
        st["master_path"] = os.path.join(out_dir, "_master_s%d_%d_%s.mp4" % (session_no, stamp[0], time.strftime("%H%M%S")))
        st["master"] = yield from mr.start_master_capture(st["master_path"])
        st["master_metas"] = []
        if not st["master"]:
            st["master_fail"] += 1
            log("master capture NOT available (traces only for the next takes)")

    def close_master():
        cap, path, metas = st["master"], st["master_path"], st["master_metas"]
        st["master"] = None
        if cap:
            cap.stop()
        if path and metas:
            cutq.submit(path, metas)

    def flow(rig, drv):
        yield from mr.boot_to_profile(rig, drv)
        counts = mr.stock_counts(rig)
        if counts:
            st["owned"] = dict(enumerate(counts))
            if owned_csv:
                o, u = mr.write_owned_csv(owned_csv, counts, skills)
                log("owned skills: %d owned, %d not owned (%s)" % (o, u, owned_csv))

        def fresh_match():
            yield from mr.launch_versus(rig, drv, stage=stage)
            st["anchors"] = (rig.fighter_pos(0), rig.fighter_pos(1))
            return st["anchors"]

        yield from fresh_match()
        log("battle running")
        for sc, row in pending:
            if sc.skill not in index_of:
                log("skip %s: unknown skill id %d" % (sc.name, sc.skill))
                continue
            gi = index_of[sc.skill]
            n = st["owned"].get(gi)
            notes = []
            if sc.skill == 0:
                notes.append("Aura Particle (game index 0) placed in the slot")
            elif n is not None and n <= 0:
                notes.append("unowned (memory deck)")
                if unowned == "skip":
                    continue
            if video and st["master"] is None and st["master_fail"] < 3:
                yield from new_master()
            if st["used"] and relaunch:
                yield from fresh_match()
            st["used"] = True
            opp_index = index_of.get(sc.opponent_skill) if sc.opponent_skill is not None else None
            st["current"] = mr.output_stem(sc.skill, sc.name)
            log("take %s  idx %d  [%d/%d]" % (st["current"], gi, st["done"] + 1, len(pending)))
            meta = yield from mr.record_skill(rig, drv, sc, out_dir, st["anchors"], video=st["master"], notes=notes,
                                              index=gi, cache={}, relaunch_cb=fresh_match if relaunch else None,
                                              opp_index=opp_index)
            if meta is not None and st["master"]:
                meta["master"] = os.path.basename(st["master_path"])
                with open(os.path.join(out_dir, mr.output_stem(sc.skill, sc.name) + ".json"), "w", encoding="utf-8") as f:
                    json.dump(meta, f, indent=1)
                st["master_metas"].append(meta)
            st["done"] += 1
            st["current"] = None
            if st["done"] >= SESSION_TAKES:
                break               # planned restart (a clean end of session, not a stall)
            if st["master"] and len(st["master_metas"]) >= CHUNK_TAKES:
                close_master()

    holder = {}
    rig = mr.Rig(pid, lambda r, t: holder["d"].on_tick(r, t), extra_specs=mr.EVENT_HOOKS,
                 out_path=os.path.join(mr.OUT_ROOT, "rig_raw.csv"))
    rig.watch_s = WATCH_S
    drv = mr.Driver(rig, None)
    drv.gen = flow(rig, drv)
    holder["d"] = drv
    crashed = None
    try:
        rig.run(0)
    except Exception as e:  # attach failed etc.
        crashed = repr(e)
    finally:
        close_master()
    stalled, reason = False, None
    if crashed:
        stalled, reason = True, crashed
    elif rig.stalled:
        stalled, reason = True, "no game tick for %d s" % WATCH_S
        for ln in (rig.stall_dump or "").splitlines():
            log("   stall thread: " + ln)
    elif drv.stalled_reason:
        stalled, reason = True, drv.stalled_reason
    elif drv.error is not None:
        stalled, reason = True, "%s: %s" % (type(drv.error).__name__, drv.error)
    elif rig.fatal:
        stalled, reason = True, repr(rig.fatal)
    elif not drv.done:
        stalled, reason = True, "the game exited"
    return {"done": st["done"], "stalled": stalled, "reason": reason, "pid": pid, "ours": ours,
            "current": st["current"]}


# ----------------------------------------------------------------------------- the whole run


def run_full(scripts, out_dir, skills, stage=mr.DEFAULT_STAGE, video=True, unowned="include", relaunch=True,
             tidy=True, resume=True, owned_csv=mr.OWNED_CSV, log=None):
    """scripts: [(Script, row)]. Returns 0 when everything was attempted, 1 when the run gave up."""
    os.makedirs(out_dir, exist_ok=True)
    RUN["t0"], RUN["started_games"] = time.time(), 0
    logf = open(os.path.join(out_dir, "run.log"), "a", encoding="utf-8", buffering=1)

    def say(msg):
        line = "%s  %s" % (time.strftime("%H:%M:%S"), msg)
        print(line, flush=True)
        logf.write(line + "\n")

    log = log or say
    mr.keep_awake(True)
    try:
        if video and not wait_for_screen(log, 60):
            log("STOP: the screen saver is on. Turn it off (or touch the PC) and run this again; takes done so far are kept.")
            return 1
        snap = os.path.join(mr.OUT_ROOT, "save-before-" + time.strftime("%Y%m%d_%H%M%S"))
        log("package folder snapshot: %d files -> %s" % (sg.snapshot(sg.LIVE, snap), snap))
        rows_by_id = {int(r["id"]): r for _sc, r in scripts if r}
        cutq = CutQueue(out_dir)
        recover_cuts(out_dir, cutq, log)

        crashes = {}            # take stem -> sessions that died while recording it

        def pending_now():
            return [(sc, r) for sc, r in scripts
                    if not (resume and take_done(out_dir, sc)) and crashes.get(mr.output_stem(sc.skill, sc.name), 0) < 2]

        pending = pending_now()
        log("%d take(s) in the list, %d still to do" % (len(scripts), len(pending)))
        fails, session_no, gave_up = 0, 0, False
        while (pending or session_no == 0 and not scripts) and not gave_up:
            session_no += 1
            if video and not wait_for_screen(log, 600):
                log("STOP: the screen saver stayed on for 10 minutes.")
                gave_up = True
                break
            log("--- session %d: %d take(s) to do" % (session_no, len(pending)))
            res = run_session(pending, out_dir, skills, stage, video, unowned, relaunch, cutq,
                              owned_csv if session_no == 1 else None, log, session_no)
            report = tidy_save(res["pid"], res["ours"], snap, tidy=tidy, log=log)
            if not res["ours"] and res["stalled"]:
                log("STOP: the game was already running (not started by this run) and stalled; not touching it.")
                gave_up = True
            pending = pending_now()
            if res["stalled"] and res.get("current"):
                crashes[res["current"]] = crashes.get(res["current"], 0) + 1
                if crashes[res["current"]] >= 2:
                    log("the game died twice on %s: skipping it (status 'crashed')" % res["current"])
            pending = pending_now()
            if res["stalled"]:
                log("STALL (%s): %d take(s) done this session, %d left; relaunching" % (res["reason"], res["done"], len(pending)))
                fails = 0 if res["done"] else fails + 1
                if fails >= MAX_FAILED_SESSIONS:
                    log("STOP: %d sessions in a row finished no take." % fails)
                    gave_up = True
            else:
                fails = 0
                if not scripts:
                    break
        cutq.join()
        for sc, r in scripts:
            stem = mr.output_stem(sc.skill, sc.name)
            if crashes.get(stem, 0) >= 2 and not take_done(out_dir, sc):
                json.dump({"skill": sc.skill, "name": sc.name, "finished": True, "crashed": True, "pressed": False,
                           "notes": ["the game stalled or crashed twice while recording this take; skipped"]},
                          open(os.path.join(out_dir, stem + ".json"), "w", encoding="utf-8"), indent=1)
                open(os.path.join(out_dir, stem + "_trace.csv"), "w").write("crashed\n")
        counts = write_summary(out_dir, rows_by_id)
        log("summary.csv written: %s" % ", ".join("%s %d" % kv for kv in sorted(counts.items())))
        import mr_trace_summary
        n_rows = mr_trace_summary.write_summary(out_dir, os.path.join(out_dir, "full_trace_summary.csv"), rows_by_id)
        log("full_trace_summary.csv written: %d rows" % n_rows)
        final = tidy_save(None, False, snap, tidy=False, log=log)
        log("%s: %d take(s) left" % ("GAVE UP" if gave_up else "DONE", len(pending_now())))
        return 1 if gave_up and pending_now() else 0
    finally:
        mr.keep_awake(False)
        logf.close()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--make-list", nargs="?", const=ALL_SKILLS_CSV, metavar="PATH")
    ap.add_argument("--skills", default=mr.SKILLS_JSON)
    ap.add_argument("--ledger", default=LEDGER)
    a = ap.parse_args(argv)
    if a.make_list:
        skills = mr.load_skill_table(a.skills)
        rows = write_all_skills(a.make_list, parse_ledger(a.ledger), skills)
        print("wrote %d rows -> %s" % (len(rows), a.make_list))
        return 0
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
