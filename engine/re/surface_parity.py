"""Surface-sidecar parity: every <stage>.surf.json must reproduce
engine/re/wetzone_probe.py's 0x94-block parse exactly -- per-class face
counts AND triangle positions (right-handed: game z mirrored).

Usage: python3 surface_parity.py <Map dir> <sidecar dir>
Exit 0 on full parity, 1 with a diff otherwise.
"""
import json
import os
import struct
import sys
from collections import Counter


def parse_block(d):
    """Mirror wetzone_probe.py + surface.rs: return (class_counts, rh_tris)."""
    blk = struct.unpack_from("<i", d, 0x94)[0]
    if blk <= 0 or blk >= len(d):
        return None
    o = blk
    pending = []  # (flags, [(a,b,c)])
    verts = []
    tris = []  # (flags, ((x,y,z)x3)) in RH space
    bad = 0
    chunks = 0
    while True:
        flags = d[o]
        cid = struct.unpack_from("b", d, o + 1)[0]
        size = struct.unpack_from("<I", d, o + 4)[0]
        body = o + 12
        chunks += 1
        if cid == -1 and size == 0:
            break
        if cid == 3:
            if size < 48 or (size - 48) % 20 != 0:
                bad += 1
            else:
                cnt = struct.unpack_from("<H", d, body + 40)[0]
                if 48 + cnt * 20 != size:
                    bad += 1
                else:
                    faces = []
                    for i in range(cnt):
                        e = body + 48 + i * 20
                        faces.append(struct.unpack_from("<3H", d, e))
                    pending.append((flags, faces))
        elif cid == 8:
            base = len(verts)
            for i in range(size // 12):
                verts.append(struct.unpack_from("<3f", d, body + i * 12))
            for flags, faces in pending:
                for a, b, c in faces:
                    try:
                        tri = tuple((verts[base + k][0], verts[base + k][1], -verts[base + k][2]) for k in (a, b, c))
                    except IndexError:
                        bad += 1
                        continue
                    tris.append((flags, tri))
            pending = []
        o += 12 + size
    if pending:
        bad += sum(len(f) for _, f in pending)
    return chunks, bad, tris


def main():
    mapdir, sidedir = sys.argv[1], sys.argv[2]
    fails = 0
    checked = 0
    for f in sorted(os.listdir(mapdir)):
        if not f.endswith(".dat"):
            continue
        d = open(os.path.join(mapdir, f), "rb").read()
        if d[:4] != b"ST00":
            continue
        stage = f[:-4]
        parsed = parse_block(d)
        if parsed is None:
            print("%s: no 0x94 block (no sidecar expected)" % stage)
            continue
        chunks, bad, tris = parsed
        want_counts = Counter(c for c, _ in tris)
        side = os.path.join(sidedir, stage + ".surf.json")
        if not os.path.exists(side):
            print("%s: MISSING sidecar" % stage)
            fails += 1
            continue
        j = json.load(open(side))
        got_counts = Counter({int(k): v for k, v in j["class_counts"].items()})
        ok = True
        if Counter(j["class"]) != want_counts or got_counts != want_counts:
            print("%s: COUNT MISMATCH probe=%s sidecar=%s" % (stage, dict(sorted(want_counts.items())), dict(sorted(got_counts.items()))))
            ok = False
        if j["bad"] != bad or j["faces"] != len(tris):
            print("%s: META MISMATCH faces %d/%d bad %s/%d" % (stage, j["faces"], len(tris), j["bad"], bad))
            ok = False
        # Position check: every sidecar triangle vs the probe's RH triangle.
        xyz = j["xyz"]
        cls = j["class"]
        if len(xyz) != 3 * len(tris) or len(cls) != len(tris):
            print("%s: ARRAY LENGTH MISMATCH" % stage)
            ok = False
        else:
            worst = 0.0
            for i, (c, tri) in enumerate(tris):
                if cls[i] != c:
                    print("%s: tri %d class %s != %s" % (stage, i, cls[i], c))
                    ok = False
                    break
                for k in range(3):
                    for m in range(3):
                        worst = max(worst, abs(xyz[3 * i + k][m] - tri[k][m]))
            if worst > 0.0015:
                print("%s: POSITION DRIFT worst %.4f" % (stage, worst))
                ok = False
        if ok:
            checked += 1
            print("%s: parity OK (%d faces, %d chunks, bad %d)" % (stage, len(tris), chunks, bad))
        else:
            fails += 1
    print("checked=%d fails=%d" % (checked, fails))
    sys.exit(1 if fails else 0)


if __name__ == "__main__":
    main()
