#!/usr/bin/env python3
"""List and extract an Xbox xiso (XDVDFS game partition). Stdlib only; our own code.

  xiso_extract.py image.iso --list              print path, size, start sector
  xiso_extract.py image.iso --out DIR           extract every file under DIR
Optional: --offset BYTES (partition start when the image is a full dump; auto-detected
for xiso = 0, plus the known XGD1/2/3 game-partition offsets).

Layout (public XDVDFS description): 2048-byte sectors; volume descriptor at sector 32
(0x10000) starts with "MICROSOFT*XBOX*MEDIA", then u32 root-directory sector, u32
root-directory size, u64 filetime. A directory is a table of entries forming a binary
search tree: u16 left, u16 right (4-byte units from the table start, 0 or 0xFFFF = none),
u32 start sector, u32 size, u8 attributes (0x10 = directory), u8 name length, name bytes.
Output derives from game data: extract outside the repo, never commit.
"""
import argparse, os, struct, sys

SECTOR = 2048
MAGIC = b"MICROSOFT*XBOX*MEDIA"
DESC_SECTOR = 32
KNOWN_OFFSETS = (0, 0x18300000, 0xFD90000, 0x2080000)  # xiso, XGD1, XGD2, XGD3 game partitions
ATTR_DIR = 0x10


class XisoError(ValueError):
    pass


def find_offset(f, forced=None):
    cands = [forced] if forced is not None else list(KNOWN_OFFSETS)
    for off in cands:
        f.seek(off + DESC_SECTOR * SECTOR)
        if f.read(20) == MAGIC:
            return off
    raise XisoError("no XDVDFS volume descriptor found (tried offsets %s)" % ", ".join(hex(c) for c in cands))


def read_dir(f, base, sector, size, limit=1 << 20):
    """Yield (name, attrs, start_sector, size) for every entry of one directory table."""
    if size <= 0 or size > limit:
        raise XisoError("implausible directory size %d" % size)
    f.seek(base + sector * SECTOR)
    table = f.read(size)
    seen = set()
    stack = [0]
    while stack:
        off = stack.pop()
        if off in seen or off + 14 > len(table):
            continue
        seen.add(off)
        left, right, start, fsize, attrs, nlen = struct.unpack_from("<HHIIBB", table, off)
        if left == 0xFFFF and right == 0xFFFF and start == 0xFFFFFFFF:
            continue  # padding filler
        name = table[off + 14:off + 14 + nlen].decode("latin1")
        yield name, attrs, start, fsize
        for child in (left, right):
            if child not in (0, 0xFFFF):
                stack.append(child * 4)


def walk(f, base=0, prefix="", root=None):
    """Yield (path, is_dir, start_sector, size) depth-first, names sorted."""
    if root is None:
        f.seek(base + DESC_SECTOR * SECTOR)
        d = f.read(SECTOR)
        if d[:20] != MAGIC:
            raise XisoError("bad volume descriptor")
        root = struct.unpack_from("<II", d, 20)
    sector, size = root
    if size == 0:
        return
    for name, attrs, start, fsize in sorted(read_dir(f, base, sector, size), key=lambda e: e[0].lower()):
        if "/" in name or "\\" in name or name in ("", ".", ".."):
            raise XisoError("unsafe entry name %r" % name)
        path = prefix + name
        if attrs & ATTR_DIR:
            yield path + "/", True, start, fsize
            yield from walk(f, base, path + "/", (start, fsize))
        else:
            yield path, False, start, fsize


def extract(f, out, base=0, log=print):
    out = os.path.abspath(out)
    n = total = 0
    for path, is_dir, start, size in walk(f, base):
        dest = os.path.abspath(os.path.join(out, path))
        if os.path.commonpath([out, dest]) != out:
            raise XisoError("path escapes output dir: %r" % path)
        if is_dir:
            os.makedirs(dest, exist_ok=True)
            continue
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        f.seek(base + start * SECTOR)
        left = size
        with open(dest, "wb") as o:
            while left:
                chunk = f.read(min(left, 1 << 20))
                if not chunk:
                    raise XisoError("image truncated inside %s" % path)
                o.write(chunk)
                left -= len(chunk)
        n += 1
        total += size
        if n % 200 == 0:
            log("  %d files, %.1f MB" % (n, total / 1e6))
    return n, total


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("image")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--out")
    ap.add_argument("--offset", type=lambda s: int(s, 0))
    a = ap.parse_args(argv)
    if not a.list and not a.out:
        ap.error("give --list and/or --out DIR")
    try:
        with open(a.image, "rb") as f:
            base = find_offset(f, a.offset)
            if a.list:
                nf = tot = 0
                for path, is_dir, start, size in walk(f, base):
                    if not is_dir:
                        nf += 1
                        tot += size
                    print("%s\t%d\t%d" % (path, size, start))
                print("%d files, %d bytes" % (nf, tot), file=sys.stderr)
            if a.out:
                n, tot = extract(f, a.out, base)
                print("extracted %d files, %d bytes -> %s" % (n, tot, a.out))
    except (OSError, XisoError, struct.error) as e:
        print("%s: %s" % (a.image, e), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
