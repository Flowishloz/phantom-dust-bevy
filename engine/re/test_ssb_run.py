"""Tests for ssb_run.py: opcode semantics on synthetic scripts, and a smoke run of the real eneprog006
against a fake world (skipped when the game files are not present)."""
import math
import os
import struct
import unittest

import ssb_dis as S
import ssb_run as R
from test_ssb_dis import build, native, w

REAL = r"../../../game_files/Assets/Data/com/ai/eneprog006.ssb"


class Rec(R.Host):
    """Records natives; native (4, 0) is setvar(i, v) and (4, 1) is getvar(i)."""

    def __init__(self):
        self.regs = [0] * 50
        self.log = []

    def native(self, group, fn, a):
        self.log.append((group, fn, tuple(a)))
        if fn == 0:
            self.regs[a[0]] = a[1]
            return 0
        if fn == 1:
            return self.regs[a[0]]
        return 0


def run(code, extra=b""):
    h = Rec()
    vm = R.Vm(build(code, extra_after_code=extra), h)
    status = vm.tick()
    return status, h, vm


def setvar0(value_words):
    """value; push 0; setvar(0, value): a0 = the index is the LAST pushed word."""
    return value_words + [w(0x0E, 0, 0), native(4, 0, 2)]


class Ops(unittest.TestCase):
    def test_add_sub_mul_order(self):
        # 10 - 3 = 7 ; 7 * 4 = 28
        code = setvar0([w(0x0E, 0, 10), w(0x0E, 0, 3), w(0x13), w(0x0E, 0, 4), w(0x14)]) + [w(0x05)]
        st, h, _ = run(code)
        self.assertEqual((st, h.regs[0]), ("returned", 28))

    def test_compare_orientation(self):
        # the left operand is pushed first: s=2 is >, 3 is <, 4 is >=, 5 is <=, 1 is !=, 0 is ==
        for s, a, b, want in ((2, 5, 3, 1), (3, 5, 3, 0), (4, 5, 5, 1), (5, 3, 5, 1), (1, 4, 4, 0), (0, 4, 4, 1)):
            code = setvar0([w(0x0E, 0, a), w(0x0E, 0, b), w(0x17, 0, s)]) + [w(0x05)]
            self.assertEqual(run(code)[1].regs[0], want, (s, a, b))

    def test_bit_test_compare(self):
        # s = 6: (S0 & S1) == S0 with S0 = the last pushed word
        code = setvar0([w(0x0E, 0, 0b111), w(0x0E, 0, 0b101), w(0x17, 0, 6)]) + [w(0x05)]
        self.assertEqual(run(code)[1].regs[0], 1)   # S0 = 0b101 is a subset of 0b111
        code = setvar0([w(0x0E, 0, 0b101), w(0x0E, 0, 0b111), w(0x17, 0, 6)]) + [w(0x05)]
        self.assertEqual(run(code)[1].regs[0], 0)

    def test_div_mod_truncate_toward_zero(self):
        code = setvar0([w(0x0E, 0, -7), w(0x0E, 0, 2), w(0x15)]) + [w(0x05)]
        self.assertEqual(run(code)[1].regs[0], -3)
        code = setvar0([w(0x0E, 0, -7), w(0x0E, 0, 2), w(0x16)]) + [w(0x05)]
        self.assertEqual(run(code)[1].regs[0], -1)

    def test_branch_and_case(self):
        # switch (2): case 1 -> R0 = 11, case 2 -> R0 = 22, default pops the value
        body = [w(0x0E, 0, 2)]
        body += [w(0x08, 1, 4)]
        body += [w(0x0E, 0, 11), w(0x0E, 0, 0), native(4, 0, 2), w(0x07, 0, 5)]
        body += [w(0x08, 2, 4)]
        body += [w(0x0E, 0, 22), w(0x0E, 0, 0), native(4, 0, 2), w(0x07, 0, 1)]
        body += [w(0x0D, 0, 1)]
        body += [w(0x05)]
        st, h, _ = run(body)
        self.assertEqual((st, h.regs[0]), ("returned", 22))

    def test_call_with_args_locals_and_return_value(self):
        # main: R0 = f(7, 5) with f(a, b) { local = a - b; return local * 2 }; a0 = 7 is pushed LAST
        main = [w(0x0E, 0, 5), w(0x0E, 0, 7), w(0x03, 0, 4), w(0x10), w(0x0E, 0, 0), native(4, 0, 2), w(0x05)]
        f = [w(0x06, 0, 1), w(0x09, 0, 0), w(0x09, 0, 1), w(0x13), w(0x0A, 0, -1),
             w(0x09, 0, -1), w(0x0E, 0, 2), w(0x14), w(0x05, 1, 3)]
        st, h, _ = run(main + f)
        self.assertEqual((st, h.regs[0]), ("returned", 4))

    def test_store_plus_minus(self):
        code = [w(0x06, 0, 1), w(0x0E, 0, 10), w(0x0A, 0, -1), w(0x0E, 0, 5), w(0x0A, 1, -1),
                w(0x0E, 0, 3), w(0x0A, 2, -1)]
        code += setvar0([w(0x09, 0, -1)]) + [w(0x0D, 0, 1), w(0x05)]
        self.assertEqual(run(code)[1].regs[0], 12)

    def test_push32_shifts_and_logic(self):
        code = setvar0([w(0x0F), struct.pack("<i", -1083808154)]) + [w(0x05)]
        self.assertEqual(run(code)[1].regs[0], -1083808154)
        code = setvar0([w(0x0E, 0, 1), w(0x0E, 0, 4), w(0x1E, 0, 0)]) + [w(0x05)]
        self.assertEqual(run(code)[1].regs[0], 16)
        code = setvar0([w(0x0E, 0, 0), w(0x1D)]) + [w(0x05)]
        self.assertEqual(run(code)[1].regs[0], 1)

    def test_table_lookup_sign_extends(self):
        # table {13, -1, 7} after the code; main stores tbl(table, 1) = -1
        table = struct.pack("<3i", 13, -1, 7)
        words = [w(0x0E, 0, 1), None, native(0, 16, 2), w(0x10), w(0x0E, 0, 0), native(4, 0, 2), w(0x05)]
        # op 1f is word 1 (at 0x24): pc_after = 0x28; the table starts at 0x20 + 7 * 4 = 0x3c -> 5 words ahead
        words[1] = struct.pack("<I", 0x1F | ((5 & 0xFFFFFF) << 8))
        h = Rec()
        vm = R.Vm(build(words, extra_after_code=table), h)
        self.assertEqual(vm.tick(), "returned")
        self.assertEqual(h.regs[0], -1)

    def test_main_that_never_returns_hits_the_budget(self):
        vm = R.Vm(build([w(0x07, 0, -1)]), Rec(), budget=100)
        self.assertEqual(vm.tick(), "budget")

    def test_invalid_opcode_faults(self):
        with self.assertRaises(R.VmFault):
            run([w(0x0B)])


class FakeWorld(R.Host):
    """Just enough of the world for eneprog006: me (0) and one foe (1) 150 units away, four attack slots."""

    def __init__(self):
        self.regs = [0] * 50
        self.vec = [[0.0, 0.0, 0.0] for _ in range(20)]
        self.pos = {0: [0.0, 0.0, 0.0], 1: [0.0, 0.0, 150.0]}
        self.target = 0
        self.tick_no = 0
        self.presses = []
        self.seed = 12345

    def rand(self, n):
        self.seed = (self.seed * 214013 + 2531011) & 0x7FFFFFFF
        return ((self.seed >> 16) & 0x7FFF) * n >> 15

    def native(self, g, f, a):
        v = self.vec
        if f == 0:
            self.regs[a[0]] = a[1]
            return 0
        if f == 1:
            return self.regs[a[0]]
        if f == 2:
            v[a[0]] = list(v[a[1]])
            return 0
        if f == 16:
            return int(math.dist(v[a[0]], v[a[1]]) * 0.1)
        if f == 20:
            return 1
        if f == 21:
            for i in range(3):   # the arguments are raw float bits
                v[a[0]][i] += struct.unpack("<f", struct.pack("<i", a[1 + i]))[0] * 10.0
            return 0
        if f == 23:
            return int(math.degrees(math.atan2(v[a[1]][0] - v[a[0]][0], v[a[1]][2] - v[a[0]][2])))
        if f in (28, 29, 30):
            return int(v[a[0]][f - 28] * 0.1 * a[1])
        if f == 32:
            return self.rand(a[0])
        if f == 33:
            d = a[0] - a[1]
            return d + 360 if d < -180 else d
        if f == 48:
            self.presses.append((self.tick_no, a[0]))
            return 0
        if f in (24, 53, 51, 60, 117, 148):
            return 1
        if f == 96:
            self.target = a[0]
            return 0
        if f == 97:
            return self.target
        if f in (113, 114):
            v[a[1]] = list(self.pos[a[0]])
            return 0
        if f == 118:
            return 20
        if f in (119, 120):
            return 30
        if f == 122:
            return a[0]
        if f == 123:
            return 99
        if f == 124:
            return 1
        if f == 134:
            return 751
        if f == 135:
            return -1
        if f == 144:
            return 100 + a[1]
        if f in (146, 161):
            return 1
        if f == 147:
            return 4
        if f in (168, 177, 192, 197, 200):
            return -1
        if f == 165:
            return 2
        if f == 175:
            return 12
        if f == 208:
            return self.tick_no
        if f == 210:
            return 100
        if f == 425:
            return 5
        return 0


@unittest.skipUnless(os.path.exists(REAL), "game files not present")
class RealScript(unittest.TestCase):
    def test_eneprog006_runs_and_fights_in_a_fake_world(self):
        h = FakeWorld()
        # epa row 0: rule 0, think 30, attack/bullet/melee/dodge 88 %, wander 3 %, idle 0, capsule strategy 0
        for i, val in enumerate((0, 30, 88, 88, 88, 88, 3, 0, 0)):
            h.regs[30 + i] = val
        with open(REAL, "rb") as fh:
            vm = R.Vm(fh.read(), h)
        modes = []
        for t in range(300):
            h.tick_no = t
            self.assertEqual(vm.tick(), "returned", "tick %d" % t)
            modes.append(h.regs[0])
        self.assertEqual(h.target, 1)
        self.assertTrue(h.presses, "the CPU never pressed a skill")
        self.assertIn(2, modes)
        self.assertTrue(all(0 <= m <= 9 for m in modes))
        self.assertTrue(all(slot in (0, 1, 2, 3) for _, slot in h.presses))


if __name__ == "__main__":
    unittest.main()
