#!/usr/bin/env python3
"""Inventory a game folder (or extracted disc) and diff two inventories.

  file_inventory.py <folder> -o inv.csv         columns: path,size,extension,sha256_64k,magic
  file_inventory.py --diff old.csv new.csv      files added / removed / changed (size or hash)
sha256_64k = SHA-256 of the first 64 KiB; magic = first 4 bytes as hex (+ printable ASCII).
Changes beyond the first 64 KiB with equal size are not detected. Output is derived from
game data: never commit.
"""
import argparse, csv, hashlib, os, sys

FIELDS = ["path", "size", "extension", "sha256_64k", "magic"]


def inventory(root):
    rows = []
    for dp, dn, fn in os.walk(root):
        dn.sort()
        for f in sorted(fn):
            p = os.path.join(dp, f)
            rel = os.path.relpath(p, root).replace(os.sep, "/")
            try:
                with open(p, "rb") as fh:
                    head = fh.read(65536)
                size = os.path.getsize(p)
            except OSError as e:
                print(f"skip {rel}: {e}", file=sys.stderr)
                continue
            m = head[:4]
            magic = m.hex() + (" " + m.decode("ascii") if m and all(32 <= c < 127 for c in m) else "")
            rows.append({"path": rel, "size": size, "extension": os.path.splitext(f)[1].lower(),
                         "sha256_64k": hashlib.sha256(head).hexdigest(), "magic": magic})
    return rows


def write_csv(rows, out):
    with open(out, "w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, FIELDS)
        w.writeheader(); w.writerows(rows)


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as fh:
        return {r["path"]: r for r in csv.DictReader(fh)}


def diff(a, b, out=sys.stdout):
    added, removed = sorted(set(b) - set(a)), sorted(set(a) - set(b))
    changed = sorted(p for p in set(a) & set(b)
                     if (a[p]["size"], a[p]["sha256_64k"]) != (b[p]["size"], b[p]["sha256_64k"]))
    for tag, lst in (("ADDED", added), ("REMOVED", removed)):
        for p in lst:
            print(f"{tag}\t{p}", file=out)
    for p in changed:
        print(f"CHANGED\t{p}\tsize {a[p]['size']} -> {b[p]['size']}", file=out)
    print(f"{len(added)} added, {len(removed)} removed, {len(changed)} changed, "
          f"{len(set(a) & set(b)) - len(changed)} identical", file=out)
    return len(added), len(removed), len(changed)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("folder", nargs="?")
    ap.add_argument("-o", "--out", default="inventory.csv")
    ap.add_argument("--diff", nargs=2, metavar=("OLD_CSV", "NEW_CSV"))
    a = ap.parse_args(argv)
    try:
        if a.diff:
            diff(read_csv(a.diff[0]), read_csv(a.diff[1])); return 0
        if not a.folder or not os.path.isdir(a.folder):
            print(f"no such folder: {a.folder}", file=sys.stderr); return 1
        rows = inventory(a.folder)
        write_csv(rows, a.out)
        print(f"{len(rows)} files -> {a.out}")
        return 0
    except (OSError, KeyError, csv.Error) as e:
        print(f"error: {e}", file=sys.stderr); return 1


if __name__ == "__main__":
    sys.exit(main())
