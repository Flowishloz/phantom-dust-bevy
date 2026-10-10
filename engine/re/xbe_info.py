#!/usr/bin/env python3
"""Parse an original Xbox XBE header to identify a build.

Prints: certificate title / title id / build timestamps / region / version, section table,
linked SDK libraries, kernel thunk ordinal imports, entry point (retail/debug key guess).
Layout from the published XBE header (xboxdevwiki.net "XBE"). Verified 2026-09-30 against the
real Phantom Dust (USA) default.xbe: header, certificate, section, library and kernel-thunk
offsets and the retail entry/thunk XOR keys all landed on sane values (entry inside .text,
thunk table at the start of .rdata). The debug/chihiro keys are still untested on real files.
  xbe_info.py default.xbe [--json]
"""
import argparse, datetime, json, struct, sys

# XOR keys: retail (verified on a real retail XBE) / debug (untested) (entry point, kernel thunk address)
ENTRY_KEYS = {"retail": 0xA8FC57AB, "debug": 0x94859D4B, "chihiro": 0x2290059D}
THUNK_KEYS = {"retail": 0x5B6D40B6, "debug": 0xEFB1F152}


REGIONS = {1: "North America", 2: "Japan", 4: "Rest of World", 7: "all regions"}


def ts(v):
    try:
        return datetime.datetime.fromtimestamp(v, datetime.timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    except (OverflowError, OSError, ValueError):
        return "invalid"


def parse(d):
    if len(d) < 0x178 or d[:4] != b"XBEH":
        raise ValueError("not an XBE (missing XBEH magic or header too short)")
    u = lambda o: struct.unpack_from("<I", d, o)[0]
    base = u(0x104)
    r = {"base": base, "size_of_headers": u(0x108), "size_of_image": u(0x10C), "timestamp": u(0x114),
         "init_flags": u(0x124)}
    v2o = lambda va: va - base

    def cstr(va):
        o = v2o(va)
        if not 0 <= o < len(d):
            return ""
        e = d.find(b"\0", o)
        return d[o:e if e >= 0 else len(d)].decode("latin1")

    # entry point and thunk: try each key, choose the one landing inside the image
    raw_entry, raw_thunk = u(0x128), u(0x158)
    r["entry"] = None
    for k, key in ENTRY_KEYS.items():
        e = raw_entry ^ key
        if base <= e < base + r["size_of_image"]:
            r["entry"] = {"va": e, "kind": k}
            break
    r["debug_path"] = cstr(u(0x14C)) if u(0x14C) else ""
    # certificate
    co = v2o(u(0x118))
    cert = {}
    if 0 <= co and co + 0xB0 <= len(d):
        size, t, tid = struct.unpack_from("<III", d, co)
        name = d[co + 12:co + 92].decode("utf-16le", "replace").split("\0")[0]
        media, region, rating, disk, ver = struct.unpack_from("<IIIII", d, co + 0x9C)
        cert = {"size": size, "timestamp": t, "title_id": tid, "title_id_str":
                "%s-%03d" % (d[co + 10:co + 12][::-1].decode("latin1"), tid & 0xFFFF), "title": name,
                "allowed_media": media, "region": region, "ratings": rating, "disk": disk, "version": ver}
    r["cert"] = cert
    # sections
    secs = []
    n, so = u(0x11C), v2o(u(0x120))
    for i in range(min(n, 256)):
        o = so + i * 56
        if o < 0 or o + 56 > len(d):
            break
        fl, va, vs, ra, rs, na = struct.unpack_from("<IIIIII", d, o)
        secs.append({"name": cstr(na), "flags": fl, "vaddr": va, "vsize": vs, "raw": ra, "rawsize": rs})
    r["sections"] = secs
    # libraries
    libs = []
    lo = v2o(u(0x164)) if u(0x164) else -1
    for i in range(min(u(0x160), 256)):
        o = lo + i * 16
        if lo < 0 or o + 16 > len(d):
            break
        nm = d[o:o + 8].rstrip(b"\0").decode("latin1")
        maj, mi, bld, fl = struct.unpack_from("<HHHH", d, o + 8)
        libs.append({"name": nm, "version": f"{maj}.{mi}.{bld}", "flags": fl})
    r["libraries"] = libs
    # kernel thunks
    r["thunk_kind"], r["kernel_imports"] = None, []
    for k, key in THUNK_KEYS.items():
        va = raw_thunk ^ key
        fo = va_to_file(secs, va, base, d)
        if fo is not None:
            r["thunk_kind"] = k
            while fo + 4 <= len(d):
                w = u(fo)
                if w == 0:
                    break
                r["kernel_imports"].append(w & 0x7FFFFFFF if w & 0x80000000 else w)
                fo += 4
            break
    return r


def va_to_file(secs, va, base, d):
    for s in secs:
        if s["vaddr"] <= va < s["vaddr"] + max(s["rawsize"], 1):
            fo = s["raw"] + va - s["vaddr"]
            return fo if fo < len(d) else None
    return None


def show(r):
    c = r["cert"]
    print(f"Title      : {c.get('title')!r}  id 0x{c.get('title_id', 0):08x} ({c.get('title_id_str')})")
    print(f"Cert built : {ts(c['timestamp']) if c else '?'}   header built: {ts(r['timestamp'])}")
    if c:
        print(f"Region 0x{c['region']:x} ({REGIONS.get(c['region'], 'other')})  media 0x{c['allowed_media']:x}  ratings 0x{c['ratings']:x}  "
              f"disk {c['disk']}  version {c['version']}")
    print(f"Base 0x{r['base']:08x}  image 0x{r['size_of_image']:x}  entry "
          + (f"0x{r['entry']['va']:08x} ({r['entry']['kind']} key)" if r["entry"] else "unknown key"))
    if r["debug_path"]:
        print(f"Debug path : {r['debug_path']}")
    print(f"Sections ({len(r['sections'])}):")
    for s in r["sections"]:
        print(f"  {s['name']:<10} va 0x{s['vaddr']:08x} size 0x{s['vsize']:07x} raw 0x{s['raw']:07x}+0x{s['rawsize']:x} flags 0x{s['flags']:x}")
    print(f"Libraries ({len(r['libraries'])}): " + ", ".join(f"{l['name']} {l['version']}" for l in r["libraries"]))
    print(f"Kernel imports ({len(r['kernel_imports'])}, {r['thunk_kind']}): {sorted(set(r['kernel_imports']))}")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("xbe"); ap.add_argument("--json", action="store_true")
    a = ap.parse_args(argv)
    try:
        r = parse(open(a.xbe, "rb").read())
    except (OSError, ValueError, struct.error) as e:
        print(f"{a.xbe}: {e}", file=sys.stderr); return 1
    print(json.dumps(r, indent=1)) if a.json else show(r)
    return 0


if __name__ == "__main__":
    sys.exit(main())
