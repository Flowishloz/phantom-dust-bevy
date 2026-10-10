#!/usr/bin/env python3
"""Scan binaries (xbe, exe, dll) for ASCII and UTF-16LE strings and filter by pattern.

  strings_scan.py game.xbe [more.exe ...]        hits with file offsets
  strings_scan.py --compare a.bin b.bin          matching strings present in only one file
Options: --min N (default 5), --pattern REGEX (repeatable; replaces the defaults),
  --all (no filtering), --mask-sdk (hide common Xbox SDK/XDK library strings).
Default patterns: allib albase alfn_ pdeft mseffect .alr .ala .alm ssb xact c:\\usr\\xbox
  assert .cpp .h   (case-insensitive). Output derives from game data: never commit.
"""
import argparse, re, sys

DEFAULT_PATTERNS = [r"allib", r"albase", r"alfn_", r"pdeft", r"mseffect", r"\.alr", r"\.ala", r"\.alm",
                    r"ssb", r"xact", r"c:\\usr\\xbox", r"assert", r"\.cpp", r"\.h\b"]
# Typical XDK / D3D / DirectSound / CRT strings (heuristic list, not exhaustive).
SDK_MASK = [r"^xapi", r"^xbox", r"^d3d", r"^dsound", r"^dmusic", r"^xgraphics", r"^xonline", r"^xnet",
            r"^xmv", r"^xc[A-Z]", r"xdk", r"\\xdk\\", r"^_?_?(cdecl|stdcall)", r"^Microsoft", r"Copyright",
            r"^kernel32|^ntdll|^user32", r"^\?\?", r"^__"]


def extract(data, minlen):
    """Yield (offset, encoding, text) for ASCII and UTF-16LE runs."""
    for m in re.finditer(rb"[\x20-\x7e]{%d,}" % minlen, data):
        yield m.start(), "a", m.group().decode("ascii")
    for m in re.finditer(rb"(?:[\x20-\x7e]\x00){%d,}" % minlen, data):
        yield m.start(), "u", m.group().decode("utf-16le")


def scan(path, minlen, pats, mask):
    data = open(path, "rb").read()
    hits = []
    for off, enc, s in extract(data, minlen):
        if pats and not any(p.search(s) for p in pats):
            continue
        if mask and any(p.search(s) for p in mask):
            continue
        hits.append((off, enc, s))
    return sorted(hits)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("files", nargs="*")
    ap.add_argument("--min", type=int, default=5, dest="minlen")
    ap.add_argument("--pattern", action="append", help="regex (case-insensitive); repeatable")
    ap.add_argument("--all", action="store_true", help="no pattern filter")
    ap.add_argument("--mask-sdk", action="store_true", help="hide common Xbox SDK/XDK library strings")
    ap.add_argument("--compare", nargs=2, metavar=("A", "B"))
    a = ap.parse_args(argv)
    pats = [] if a.all else [re.compile(p, re.I) for p in (a.pattern or DEFAULT_PATTERNS)]
    mask = [re.compile(p, re.I) for p in SDK_MASK] if a.mask_sdk else []
    files = list(a.compare) if a.compare else a.files
    if not files:
        ap.print_usage(sys.stderr); return 1
    res, rc = {}, 0
    for f in files:
        try:
            res[f] = scan(f, a.minlen, pats, mask)
        except OSError as e:
            print(f"cannot read {f}: {e}", file=sys.stderr); rc = 1
    if a.compare:
        if len(res) < 2:
            return 1
        sa, sb = ({s for _, _, s in res[f]} for f in a.compare)
        for name, only, other in ((a.compare[0], sa - sb, a.compare[1]), (a.compare[1], sb - sa, a.compare[0])):
            print(f"== only in {name} ({len(only)}), not in {other}")
            for off, enc, s in res[name]:
                if s in only:
                    print(f"  0x{off:08x} {enc} {s}")
    else:
        for f, hits in res.items():
            print(f"== {f}: {len(hits)} hits")
            for off, enc, s in hits:
                print(f"0x{off:08x} {enc} {s}")
    return rc


if __name__ == "__main__":
    sys.exit(main())
