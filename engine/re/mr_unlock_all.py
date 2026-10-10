"""Unlock every collectable skill in the ORIGINAL game's own profile, then let the game save it.

Asked for by Laurence (2026-10-01: "unlock all, dont bother with backup"). Our own code, Python
stdlib + the move recorder's debugger. No file is written by hand: the stock bytes are raised in
the running game's memory and the game's own profile save is called on its thread.

  * Stock: DAT_1404bec10 (profile manager) + 0x3a38 -> signed bytes by game index (READ: the add
    helpers FUN_14015aae0 / FUN_14015ab90 clamp at 99 and set the 'new' flag pair at +0x3a40,
    short[index*2+1] = 1, short[index*2] = 0, then FUN_14015a730(mgr, 0) re-checks the arsenals).
  * Save: FUN_1401cff20() queues the profile (DAT_1404bec10, 0x19d0 bytes) on the game's own save
    queue (FUN_1401cf800 -> FUN_1401ce9a0 -> FUN_1400ccfb0 "%d.dat").

  python mr_unlock_all.py [--count 9] [--aura 99]
"""
import argparse
import json
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import move_recorder as mr  # noqa: E402

FN_SAVE_PROFILE = 0x1401CFF20
FN_ARSENAL_RECHECK = 0x14015A730
NEW_FLAGS_OFF = 0x3A40
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def targets(skills, count, aura):
    """{game index: wanted count} for every registered (collectable) skill, plus the Aura Particle."""
    want = {0: aura}
    for s in mr.unique_skills(skills):
        if s.get("registered") and s["id"] != 0:
            want[mr.game_index(s)] = count
    return want


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--count", type=int, default=9)
    ap.add_argument("--aura", type=int, default=99)
    a = ap.parse_args(argv)
    skills = json.load(open(os.path.join(REPO, "engine", "cache", "skills.json"), encoding="utf-8"))["skills"]
    want = targets(skills, min(a.count, 99), min(a.aura, 99))
    pid, ours = mr.launch_game()
    if not pid:
        print("the game did not start")
        return 1
    result = {}

    def flow(rig, drv):
        yield from mr.boot_to_profile(rig, drv)
        before = mr.stock_counts(rig, 800)
        mgr = rig.rd64(mr.PROFILE_MGR)
        stock = struct.unpack("<Q", rig.read(mgr + mr.STOCK_PTR_OFF, 8))[0]
        flags = struct.unpack("<Q", rig.read(mgr + NEW_FLAGS_OFF, 8))[0]
        raised = 0
        for gi, n in sorted(want.items()):
            if before[gi] < n:
                rig.write(stock + gi, struct.pack("<b", n))
                if gi and struct.unpack("<h", rig.read(flags + gi * 4 + 2, 2))[0] == 0:
                    rig.write(flags + gi * 4, struct.pack("<hh", 0, 1))
                raised += 1
        rig.call(FN_ARSENAL_RECHECK, mgr, 0, tag="recheck")
        yield 5
        rig.call(FN_SAVE_PROFILE, tag="save")
        yield 300                      # ~10 s for the async save to finish
        after = mr.stock_counts(rig, 800)
        result.update(raised=raised, save_rc=rig.call_results.get("save"),
                      owned_before=sum(1 for gi in want if before[gi] > 0),
                      owned_after=sum(1 for gi in want if after[gi] > 0), total=len(want))

    holder = {}
    rig = mr.Rig(pid, lambda r, t: holder["d"].on_tick(r, t))
    drv = mr.Driver(rig, None)
    drv.gen = flow(rig, drv)
    holder["d"] = drv
    rig.run(0)
    print(result, "error:", drv.error)
    if ours:
        time.sleep(5)
        os.system("taskkill /PID %d /F >nul 2>&1" % pid)
    return 0 if result and drv.error is None else 1


if __name__ == "__main__":
    sys.exit(main())
