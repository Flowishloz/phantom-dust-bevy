#!/usr/bin/env python3
"""Save-safety tool for the ORIGINAL Phantom Dust Store package (our own code).

Launching the original game rewrites its save container at start-up (new container file number,
re-written blobs, same sizes) even if nothing is played, so every run of the move recorder can
change Laurence's package folder. This tool makes that harmless:

  * snapshot  - copy the package folder to a safe place BEFORE a run (small, about 0.6 MB);
  * compare   - SHA-256 every file against a reference (a snapshot or the dated backup);
  * restore   - copy back any file that differs or is missing. It NEVER deletes anything.

  python save_guard.py                      compare live package vs the 2026-10-01 backup
  python save_guard.py --restore            ... and copy the backup's version over differences
  python save_guard.py --ref DIR [--restore]  use another reference folder (e.g. a pre-run snapshot)
  python save_guard.py --snapshot DIR       copy the live package folder into DIR

Restore only while the game is NOT running (it holds its save container open). Files under AC/
(caches, logs) are volatile and reported separately; the save lives in SystemAppData/wgs and
Settings.
"""
import argparse
import hashlib
import os
import shutil
import sys

LIVE = os.path.join(os.environ.get("LOCALAPPDATA", ""), "Packages", "Microsoft.MSEsper_8wekyb3d8bbwe")
BACKUP = os.path.expandvars(r"%USERPROFILE%\.pdtools\original-save-backup-20261001")
VOLATILE_PREFIX = ("AC/",)


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def tree(root):
    out = {}
    for dp, _dn, fn in os.walk(root):
        for f in fn:
            p = os.path.join(dp, f)
            out[os.path.relpath(p, root).replace("\\", "/")] = p
    return out


def compare(live, ref):
    """Return (changed, missing, extra) relative paths of live vs the reference folder.
    changed: in both with different content; missing: in the reference only; extra: in live only."""
    a, b = tree(live), tree(ref)
    changed = sorted(k for k in b if k in a and sha(a[k]) != sha(b[k]))
    missing = sorted(k for k in b if k not in a)
    extra = sorted(k for k in a if k not in b)
    return changed, missing, extra


def is_save(rel):
    return not rel.startswith(VOLATILE_PREFIX)


def save_differences(live, ref):
    changed, missing, _extra = compare(live, ref)
    return [k for k in changed + missing if is_save(k)]


def snapshot(live, dest):
    """Copy every file of the live package folder into dest (created if needed). Returns the count."""
    n = 0
    for rel, p in tree(live).items():
        d = os.path.join(dest, rel)
        os.makedirs(os.path.dirname(d), exist_ok=True)
        shutil.copy2(p, d)
        n += 1
    return n


def restore(live, ref, only_save=True):
    """Copy the reference's version over every differing / missing file. Never deletes.
    Returns the list of restored relative paths."""
    changed, missing, _extra = compare(live, ref)
    done = []
    for k in changed + missing:
        if only_save and not is_save(k):
            continue
        dst = os.path.join(live, k)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(os.path.join(ref, k), dst)
        done.append(k)
    return done


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--restore", action="store_true")
    ap.add_argument("--live", default=LIVE)
    ap.add_argument("--ref", default=BACKUP, help="reference folder (default: the 2026-10-01 backup)")
    ap.add_argument("--snapshot", metavar="DIR", help="copy the live folder into DIR and stop")
    a = ap.parse_args(argv)
    if a.snapshot:
        print("snapshot: %d files -> %s" % (snapshot(a.live, a.snapshot), a.snapshot))
        return 0
    changed, missing, extra = compare(a.live, a.ref)
    sv = [k for k in changed + missing if is_save(k)]
    vol = [k for k in changed + missing if not is_save(k)]
    print("save files differing from the reference: %d" % len(sv))
    for k in sv:
        print("  SAVE  ", k)
    print("volatile cache/log files differing: %d (AC/, not the save)" % len(vol))
    for k in extra:
        print("  extra file not in the reference (left alone):", k)
    if a.restore and sv:
        for k in restore(a.live, a.ref):
            print("restored", k)
        sv = save_differences(a.live, a.ref)
        print("after restore, save files differing: %d" % len(sv))
    print("RESULT:", "OK (save identical to the reference)" if not sv else "DIFFERENT")
    return 0 if not sv else 1


if __name__ == "__main__":
    sys.exit(main())
