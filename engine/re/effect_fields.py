#!/usr/bin/env python3
"""Histogram every numeric field of the effect files to find enum / flag fields.

Input : a cache/fx folder with eftNNNN.fx.json (from `pd_convert effects`: pieces[].raw = the
        32 u32 words of the 0x13 chunk, pieces[].shader, emitters[].raw) and/or raw
        eftNNNN.eft files (412 bytes: 12 x 20-byte model entries, slots at 244, see
        docs/FORMATS-EFFECTS.md ".eft layout"). Unknown/short/corrupt files are skipped.
Report: per field -> distinct count, min/max, top values, bit histogram (flag candidates),
        and value-vs-shader correlation (how much of the field is explained by the shader).
Modes : effect_fields.py <fx dir> [--csv out.csv]      histogram report
        effect_fields.py --diff <dirA> <dirB>          files / fields that differ
Output derives from game data: never commit.
"""
import argparse, collections, csv, json, math, os, re, struct, sys

EFT_SIZE = 412


def f32(u):
    return struct.unpack("<f", struct.pack("<I", u & 0xFFFFFFFF))[0]


def load_dir(d):
    """{stem: {"pieces": [(shader, [words])], "eft": {field: value}}} - tolerant."""
    out = {}
    if not os.path.isdir(d):
        return out
    for f in sorted(os.listdir(d)):
        m = re.match(r"(eft\d+)\.(fx\.json|eft)$", f)
        if not m:
            continue
        rec = out.setdefault(m.group(1), {"pieces": [], "eft": {}})
        try:
            data = open(os.path.join(d, f), "rb").read()
            if m.group(2) == "eft":
                rec["eft"] = parse_eft(data)
            else:
                rec["pieces"] += parse_json(json.loads(data.decode("utf-8", "replace")))
        except Exception as e:  # corrupt file: report, go on
            print(f"skip {f}: {e}", file=sys.stderr)
    return out


def parse_json(j):
    res = []
    for p in j.get("pieces", []):
        raw = p.get("raw")
        if isinstance(raw, list) and raw:
            res.append((p.get("shader") or "?", [int(x) & 0xFFFFFFFF for x in raw]))
    for e in j.get("emitters", []):
        raw = e.get("raw")
        if isinstance(raw, list) and raw:
            res.append(("emitter:" + str(e.get("kind", "?")), [int(x) & 0xFFFFFFFF for x in raw]))
    return res


def parse_eft(b):
    if len(b) < 244:
        return {}
    f = {}
    for i in range(12):
        w0, w1 = struct.unpack_from("<II", b, i * 20)
        if any(b[i * 20:(i + 1) * 20]):
            f[f"eft.model{i}.word0"] = w0
            f[f"eft.model{i}.flags"] = w1
    for s in range(6):
        off = 244 + s * 28
        if off + 28 <= len(b):
            flag, zero = struct.unpack_from("<hh", b, off)
            f[f"eft.slot{s}.flag"] = flag & 0xFFFF
            f[f"eft.slot{s}.word1"] = zero & 0xFFFF
    return f


def collect(recs):
    """fields[name] -> Counter(value); by_shader[name][shader] -> Counter(value)."""
    fields = collections.defaultdict(collections.Counter)
    by_shader = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    for rec in recs.values():
        for shader, words in rec["pieces"]:
            for i, w in enumerate(words):
                n = f"word{i:02d}"
                fields[n][w] += 1
                by_shader[n][shader][w] += 1
        for n, v in rec["eft"].items():
            key = re.sub(r"model\d+", "modelN", re.sub(r"slot\d+", "slotN", n))
            fields[key][v] += 1
    return fields, by_shader


def explained(by_shader_field, total):
    """Fraction of pieces whose value equals their shader's most common value."""
    return sum(c.most_common(1)[0][1] for c in by_shader_field.values()) / total if total else 0.0


def report(recs, top, out=sys.stdout):
    fields, by_shader = collect(recs)
    npieces = sum(len(r["pieces"]) for r in recs.values())
    print(f"{len(recs)} effect files, {npieces} pieces/emitters", file=out)
    for n in sorted(fields):
        c = fields[n]; total = sum(c.values())
        vals = sorted(c)
        print(f"\n== {n}: {total} values, {len(vals)} distinct, min 0x{vals[0]:08x} max 0x{vals[-1]:08x}", file=out)
        print("   top: " + ", ".join(f"0x{v:x}(f={f32(v):.4g})x{k}" for v, k in c.most_common(top)), file=out)
        if len(vals) <= 8 and not n.startswith("eft"):
            print("   ENUM candidate (<=8 distinct values)", file=out)
        bits = [sum(k for v, k in c.items() if v >> b & 1) for b in range(32)]
        used = [(b, k) for b, k in enumerate(bits) if k]
        if used and max(vals) < 1 << 16:
            print("   bits: " + " ".join(f"b{b}={k}" for b, k in used), file=out)
        if n in by_shader:
            e = explained(by_shader[n], total)
            tag = "  <- shader-determined" if e > 0.9 and len(vals) > 1 else ""
            print(f"   shader explains {e:.0%} of values{tag}", file=out)
    return fields, by_shader


def write_csv(path, fields, by_shader):
    with open(path, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["field", "shader", "value_hex", "value_float", "count"])
        for n in sorted(fields):
            if n in by_shader:
                for sh, c in sorted(by_shader[n].items()):
                    for v, k in sorted(c.items()):
                        w.writerow([n, sh, f"0x{v:08x}", f"{f32(v):.6g}", k])
            else:
                for v, k in sorted(fields[n].items()):
                    w.writerow([n, "", f"0x{v:08x}", "", k])


def diff(a, b, out=sys.stdout):
    ra, rb = load_dir(a), load_dir(b)
    print(f"only in A ({a}): {sorted(set(ra) - set(rb))}", file=out)
    print(f"only in B ({b}): {sorted(set(rb) - set(ra))}", file=out)
    changed = 0
    for stem in sorted(set(ra) & set(rb)):
        x, y = ra[stem], rb[stem]
        lines = []
        if len(x["pieces"]) != len(y["pieces"]):
            lines.append(f"  piece count {len(x['pieces'])} -> {len(y['pieces'])}")
        for i, (p, q) in enumerate(zip(x["pieces"], y["pieces"])):
            if p[0] != q[0]:
                lines.append(f"  piece {i}: shader {p[0]} -> {q[0]}")
            for k, (u, v) in enumerate(zip(p[1], q[1])):
                if u != v:
                    lines.append(f"  piece {i} word{k:02d}: 0x{u:08x} -> 0x{v:08x}")
        for k in sorted(set(x["eft"]) | set(y["eft"])):
            if x["eft"].get(k) != y["eft"].get(k):
                lines.append(f"  {k}: {x['eft'].get(k)} -> {y['eft'].get(k)}")
        if lines:
            changed += 1
            print(f"{stem}:", *lines, sep="\n", file=out)
    print(f"{changed} of {len(set(ra) & set(rb))} common files differ", file=out)
    return changed


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dir", nargs="?", help="cache/fx folder")
    ap.add_argument("--csv", help="also write a long-form CSV (field, shader, value, count)")
    ap.add_argument("--top", type=int, default=8, help="values listed per field")
    ap.add_argument("--diff", nargs=2, metavar=("A", "B"), help="compare two cache/fx folders")
    a = ap.parse_args(argv)
    if a.diff:
        for d in a.diff:
            if not os.path.isdir(d):
                print(f"no such folder: {d}", file=sys.stderr); return 1
        diff(*a.diff); return 0
    if not a.dir or not os.path.isdir(a.dir):
        print(f"no such folder: {a.dir}", file=sys.stderr); return 1
    recs = load_dir(a.dir)
    if not recs:
        print("no eftNNNN.fx.json / .eft files found", file=sys.stderr); return 1
    fields, by_shader = report(recs, a.top)
    if a.csv:
        write_csv(a.csv, fields, by_shader); print(f"\nCSV: {a.csv}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
