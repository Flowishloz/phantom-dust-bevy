"""Wet-zone probe: where does the footstep collision "class byte" live?

Background: FUN_140256390 (PDUWP.exe) walks the runtime collision faces under
the player and returns the nearest face's class byte (node offset +2). The
footstep dispatchers (FUN_140230cd0 Type1 / FUN_140230d60 Type0) switch on it:
  sand  {3, 9, 0xC}   dust {5, 6, 8, 0xB, 0x15}   water {0xD}
  silent {4, 7, 0xA, 0xE..0x14}  + default (silent)
The probe skips class 0x1F faces outright and only walks 0x14 faces
conditionally. Our OC00-derived HullFace.attr only ever holds
{0x77FC, 0x34, 0x0, 0x0B}, so attr is NOT the class byte.

This script reads the stage .dat header-0x94 block (untagged whole-stage
collision, never parsed by pd_formats) and histograms the hull-chunk FLAGS
byte (header byte 0), the per-face attr field, and face counts per flags
value -- the prime candidate for the runtime class byte.

Usage: python3 wetzone_probe.py <Map dir>   (game files, read-only)
"""
import os
import struct
import sys
from collections import Counter

SAND = {3, 9, 0xC}
DUST = {5, 6, 8, 0xB, 0x15}
WATER = {0xD}
SILENT_CASE = {4, 7, 0xA, 0xE, 0xF, 0x10, 0x11, 0x12, 0x13, 0x14}


def label(v):
    if v in SAND:
        return "sand"
    if v in DUST:
        return "dust"
    if v in WATER:
        return "WATER"
    if v in SILENT_CASE:
        return "silent-case"
    return "silent-default"


def walk_block(d, base):
    """Yield (flags, cid, size, body_off) for each chunk; stop at id -1/size 0."""
    o = base
    n = 0
    while True:
        if o + 12 > len(d):
            return None, "truncated header at 0x%X" % o
        flags = d[o]
        cid = struct.unpack_from("b", d, o + 1)[0]
        size = struct.unpack_from("<I", d, o + 4)[0]
        z = struct.unpack_from("<I", d, o + 8)[0]
        yield flags, cid, size, o + 12
        n += 1
        if cid == -1 and size == 0:
            return n, None
        if z != 0:
            return n, "nonzero trailer word at chunk %d" % n
        o += 12 + size
        if n > 100000 or o >= len(d):
            return n, "runaway at 0x%X" % o


def parse_stage(path):
    d = open(path, "rb").read()
    if d[:4] != b"ST00":
        return {"file": os.path.basename(path), "magic": d[:4].decode("ascii", "replace"), "skip": True}
    blk = struct.unpack_from("<i", d, 0x94)[0]
    if blk <= 0 or blk >= len(d):
        return {"file": os.path.basename(path), "block": blk, "skip": True}
    faces = 0
    flags_faces = Counter()
    flags_chunks = Counter()
    flags_bytes = Counter()
    face_attrs = Counter()
    flags_orient = Counter()
    bad = 0
    chunks = 0
    vert_chunks = 0
    vert_total = 0
    n, err = 0, None
    gen = walk_block(d, blk)
    try:
        while True:
            flags, cid, size, body = next(gen)
            chunks += 1
            flags_chunks[flags] += 1
            flags_bytes[flags] += size + 12
            if cid == 3:
                if size < 48 or (size - 48) % 20 != 0:
                    bad += 1
                    continue
                cnt = struct.unpack_from("<H", d, body + 40)[0]
                if 48 + cnt * 20 != size:
                    bad += 1
                    continue
                faces += cnt
                flags_faces[flags] += cnt
                for i in range(cnt):
                    e = body + 48 + i * 20
                    attr = struct.unpack_from("<H", d, e + 6)[0]
                    face_attrs[attr] += 1
                    # Face entry is {u16 a,b,c, u16 attr, f32 nx,ny,nz} (20 B).
                    # Orientation is what proves WHY the probe can skip 0x1F
                    # (walls/ceilings) -- asserted in COLLISION.md, so count it.
                    ny = struct.unpack_from("<f", d, e + 12)[0]
                    o = "floor" if ny > 0.7 else ("ceil" if ny < -0.7 else "wall")
                    flags_orient[(flags, o)] += 1
            elif cid == 8:
                vert_chunks += 1
                vert_total += size // 12
    except StopIteration:
        pass
    return {
        "file": os.path.basename(path), "block": blk, "chunks": chunks,
        "faces": faces, "bad": bad, "vert_chunks": vert_chunks,
        "vert_total": vert_total, "flags_faces": flags_faces,
        "flags_chunks": flags_chunks, "face_attrs": face_attrs,
        "flags_orient": flags_orient,
    }


def main():
    mapdir = sys.argv[1]
    files = sorted(f for f in os.listdir(mapdir) if f.endswith(".dat"))
    results = [parse_stage(os.path.join(mapdir, f)) for f in files]
    all_attrs = Counter()
    all_flags = Counter()
    print("== 0x94 block survey: faces per flags byte (candidate class byte) ==")
    for r in results:
        if r.get("skip"):
            print("%-12s skip (magic=%s block=%s)" % (r["file"], r.get("magic"), r.get("block")))
            continue
        all_attrs.update(r["face_attrs"])
        all_flags.update(r["flags_faces"])
        parts = ["%s:%d" % (hex(k), v) for k, v in sorted(r["flags_faces"].items())]
        print("%-12s chunks=%4d faces=%5d verts=%6d bad=%d" % (
            r["file"], r["chunks"], r["faces"], r["vert_total"], r["bad"]))
        print("    faces/flags: %s" % " ".join(parts))
    print()
    print("== global face-attr histogram in 0x94 blocks ==")
    for k, v in sorted(all_attrs.items()):
        print("    attr %s: %d" % (hex(k), v))
    print()
    print("== global faces-per-flags histogram ==")
    tot = sum(all_flags.values()) or 1
    for k in sorted(all_flags):
        print("    flags %s (%s): %d (%.1f%%)" % (hex(k), label(k), all_flags[k], 100.0 * all_flags[k] / tot))
    wet = sum(v for k, v in all_flags.items() if k in WATER | SAND | DUST)
    print("    effect-producing (sand+dust+water): %d (%.1f%%)" % (wet, 100.0 * wet / tot))
    print()
    # Orientation per class: this is what proves WHY the original's downward
    # probe can exclude 0x1F (it must be walls/ceilings). COLLISION.md asserts
    # these counts, so derive them here rather than trusting a one-off script.
    all_orient = Counter()
    for r in results:
        if not r.get("skip"):
            all_orient.update(r.get("flags_orient", Counter()))
    by_flag = {}
    for (fl, o), v in all_orient.items():
        by_flag.setdefault(fl, Counter())[o] = v
    print("== orientation per flags byte (floor ny>0.7, ceil ny<-0.7) ==")
    for fl in sorted(by_flag):
        c = by_flag[fl]
        print("    flags %s (%s): floor %d wall %d ceil %d" % (
            hex(fl), label(fl), c["floor"], c["wall"], c["ceil"]))
    print()
    print("== focus: Lost Palace (st03) water-class (0xD) patches ==")
    for r in results:
        if r["file"] != "st03.dat" or r.get("skip"):
            continue
        d = open(os.path.join(mapdir, "st03.dat"), "rb").read()
        o = r["block"]
        n = 0
        water_faces = 0
        while True:
            flags = d[o]
            cid = struct.unpack_from("b", d, o + 1)[0]
            size = struct.unpack_from("<I", d, o + 4)[0]
            body = o + 12
            if cid == 3 and flags == 0xD:
                cx, cy, cz = struct.unpack_from("<3f", d, body)
                mn = struct.unpack_from("<3f", d, body + 16)
                mx = struct.unpack_from("<3f", d, body + 28)
                cnt = struct.unpack_from("<H", d, body + 40)[0]
                water_faces += cnt
                print("    chunk %d: centre (%.1f, %.1f, %.1f) "
                      "x %.0f..%.0f z %.0f..%.0f faces %d" % (
                          n, cx, cy, cz, mn[0], mx[0], mn[2], mx[2], cnt))
            o += 12 + size
            n += 1
            if cid == -1 and size == 0:
                break
        print("    TOTAL water-class faces in st03: %d" % water_faces)


if __name__ == "__main__":
    main()
