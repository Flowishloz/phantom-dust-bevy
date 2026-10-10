"""Cross-check of the Rust CPU host's native-call log against the reference interpreter `ssb_run.py` (lane cpuhost).

The Rust test `cpu::tests::native_call_log_matches_the_python_reference_vm` runs the real `eneprog006` on the real sim for N program
ticks and writes every native call as one JSON line: {"tick", "g", "f", "a": [args], "r": result, "v": [index, x, y, z] | null}.
This tool runs the SAME script on the independent Python VM (`ssb_run.Vm`) and replays the recorded results as the world:

  1. the sequence of calls (group, function, argument words) must be identical, tick by tick (this proves the two VMs, fed the same
     answers, drive the script identically: the strongest VM + script test the P0-AI lane-B text asks for);
  2. the pure natives are recomputed independently here from the call history and compared with what the Rust host returned:
     f0/f1 (registers), f16 dist, f23 heading, f28-f30, f33 angdiff, f21 vadd, f2 vcopy (the vector file is followed from the
     `v` field of the vector-writing world natives).

Usage: python cpu_xcheck.py <eneprog006.ssb> <calls.jsonl> [max_ticks]      exit 0 = identical.
"""
import json
import math
import struct
import sys

import ssb_run as R


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


class Replay(R.Host):
    def __init__(self, ticks):
        self.ticks = ticks            # {tick: [records]}
        self.cur = []
        self.pos = 0
        self.regs = {}
        self.vecs = [[0.0, 0.0, 0.0] for _ in range(20)]
        self.mismatch = []
        self.checked = 0

    def start_tick(self, t):
        self.cur = self.ticks[t]
        self.pos = 0
        self.t = t

    def native(self, group, fn, a):
        if self.pos >= len(self.cur):
            self.mismatch.append((self.t, "script made an extra call g%d.f%d%s" % (group, fn, a)))
            return 0
        rec = self.cur[self.pos]
        self.pos += 1
        if (rec["g"], rec["f"], list(rec["a"])) != (group, fn, list(a)):
            self.mismatch.append((self.t, "call %d: python g%d.f%d%s vs rust g%d.f%d%s" % (self.pos, group, fn, a, rec["g"], rec["f"], rec["a"])))
            return rec["r"]
        self.checked += 1
        self.verify(rec)
        return rec["r"]

    def check(self, rec, want, tol=0, what=""):
        if abs(rec["r"] - want) > tol:
            self.mismatch.append((self.t, "%s: rust returned %d, independent value %s" % (what or "f%d" % rec["f"], rec["r"], want)))

    def verify(self, rec):
        f, a = rec["f"], rec["a"]
        if rec["g"] == 4:
            if f == 0:
                self.regs[a[0]] = a[1]
            elif f == 1 and a[0] in self.regs:
                self.check(rec, self.regs[a[0]], what="getvar(%d)" % a[0])
            elif f == 2:
                self.vecs[a[0]] = list(self.vecs[a[1]])
            elif f == 21:
                d = [struct.unpack("<f", struct.pack("<I", x & 0xFFFFFFFF))[0] for x in a[1:4]]
                self.vecs[a[0]] = [f32(self.vecs[a[0]][k] + d[k] * 10.0) for k in range(3)]
            elif f in (113, 114, 179, 193) and rec.get("v"):
                v = rec["v"]
                self.vecs[v[0]] = [v[1], v[2], v[3]]
            elif f == 16:
                p, q = self.vecs[a[0]], self.vecs[a[1]]
                self.check(rec, int(math.dist(p, q) * 0.1), tol=1, what="dist")
            elif f == 23:
                p, q = self.vecs[a[0]], self.vecs[a[1]]
                u = int(math.atan2(q[0] - p[0], q[2] - p[2]) * 5215.189)
                self.check(rec, int(u * 360.0 / 32768.0), tol=1, what="heading")
            elif f in (28, 29, 30):
                self.check(rec, int(self.vecs[a[0]][f - 28] * 0.1 * a[1]), tol=1, what="v%s" % "xyz"[f - 28])
            elif f == 33:
                d = a[0] - a[1]
                self.check(rec, d + 360 if d < -180 else d, what="angdiff")
            if f in (2, 21) and rec.get("v"):
                v = rec["v"]
                got = [v[1], v[2], v[3]]
                if any(abs(got[k] - self.vecs[v[0]][k]) > 0.05 for k in range(3)):
                    self.mismatch.append((self.t, "f%d vector %d: rust %s python %s" % (f, v[0], got, self.vecs[v[0]])))
                self.vecs[v[0]] = got


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    image = open(argv[0], "rb").read()
    ticks = {}
    for line in open(argv[1], encoding="utf-8"):
        rec = json.loads(line)
        ticks.setdefault(rec["tick"], []).append(rec)
    limit = int(argv[2]) if len(argv) > 2 else len(ticks)
    host = Replay(ticks)
    vm = R.Vm(image, host)
    done = 0
    for t in sorted(ticks)[:limit]:
        host.start_tick(t)
        st = vm.tick()
        if host.pos != len(host.cur):
            host.mismatch.append((t, "python made %d calls, rust logged %d (status %s)" % (host.pos, len(host.cur), st)))
        done += 1
        if host.mismatch:
            break
    print("ticks %d, native calls compared %d, mismatches %d" % (done, host.checked, len(host.mismatch)))
    for m in host.mismatch[:10]:
        print("  tick %s: %s" % m)
    return 1 if host.mismatch else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
