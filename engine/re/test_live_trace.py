"""Unit tests for the pure parts of live_trace.py (no process is touched)."""
import os
import struct
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import live_trace as lt  # noqa: E402


def make_ctx(**regs):
    ctx = bytearray(lt.CTX_SIZE)
    for name, v in regs.items():
        struct.pack_into("<Q", ctx, lt.REG64[name], v)
    return bytes(ctx)


class DerefTests(unittest.TestCase):
    def test_deref_dword_float_and_negative_offset(self):
        ctx = make_ctx(rcx=0x10000, rsp=0x5000)
        mem = {0x10000 + 0x12F04: (0x1C).to_bytes(4, "little"), 0x10000 + 0x12E40: struct.pack("<f", 12.5),
               0x10000 - 8: (7).to_bytes(8, "little"), 0x5000 + 0x28: (9).to_bytes(8, "little")}
        rd = lambda addr, n: mem.get(addr, b"\0" * 8)[:n]  # noqa: E731
        self.assertEqual(lt.ArgSpec("d[rcx+0x12f04]").read(ctx, rd), 0x1C)
        self.assertEqual(lt.ArgSpec("f[rcx+0x12e40]").read(ctx, rd), 12.5)
        self.assertEqual(lt.ArgSpec("[rcx-8]").read(ctx, rd), 7)
        self.assertEqual(lt.ArgSpec("[rsp+0x28]").read(ctx, rd), 9)
        self.assertIsNone(lt.ArgSpec("d[rdx+4]").read(ctx, lambda a, n: None))
        self.assertEqual(lt.fmt_val(12.5), "12.500")
        self.assertEqual([a.name for a in lt.BpSpec("0x14024ec80:tick:d[rcx+0x14]*,f[rcx+0x12e40]").args], ["d[rcx+0x14]", "f[rcx+0x12e40]"])


class AddressTests(unittest.TestCase):
    def test_round_trip(self):
        base = 0x7FF78D760000
        self.assertEqual(lt.ghidra_to_runtime(0x140276330, base), base + 0x276330)
        self.assertEqual(lt.runtime_to_ghidra(base + 0x276330, base), 0x140276330)

    def test_outside_image(self):
        base = 0x7FF78D760000
        self.assertIsNone(lt.runtime_to_ghidra(0x1000, base))
        self.assertIsNone(lt.runtime_to_ghidra(base + 0x5000000, base, 0x4000000))
        self.assertEqual(lt.fmt_ret(0x1234, base, 0x4000000), "ext:0x1234")
        self.assertEqual(lt.fmt_ret(base + 0x251B92, base, 0x4000000), "0x140251b92")


class ArgParseTests(unittest.TestCase):
    def test_bp_with_preset(self):
        s = lt.BpSpec("0x140276330:sound")
        self.assertEqual(s.addr, 0x140276330)
        self.assertEqual(s.name, "sound")
        self.assertEqual([a.name for a in s.args], ["ecx", "edx", "r8d", "r9d", "d[rsp+0x28]", "[rsp+0x30]"])
        self.assertEqual([a.key for a in s.args], [True, True, False, False, False, False])

    def test_bp_explicit_args_and_default_keys(self):
        s = lt.BpSpec("0x14024ec80:tick:rcx,rdx,r8")
        self.assertEqual([a.key for a in s.args], [True, True, False])
        s = lt.BpSpec("0x14024ec80:tick:rcx,rdx*,r8")
        self.assertEqual([a.key for a in s.args], [False, True, False])

    def test_bad_inputs(self):
        with self.assertRaises(ValueError):
            lt.BpSpec("0x1000:x")              # not a ghidra address
        with self.assertRaises(ValueError):
            lt.BpSpec("0x140276330:x:foo")     # not a register

    def test_read_registers_and_stack(self):
        ctx = make_ctx(rcx=0xFFFFFFFF00000005, rdx=86, rsp=0x5000)
        mem = {0x5028: (0x1000).to_bytes(8, "little"), 0x5030: (0xABCDEF123456).to_bytes(8, "little")}
        rd = lambda addr, n: mem.get(addr, b"\0" * 8)[:n]  # noqa: E731
        self.assertEqual(lt.ArgSpec("ecx").read(ctx, rd), 5)           # low 32 bits only
        self.assertEqual(lt.ArgSpec("rcx").read(ctx, rd), 0xFFFFFFFF00000005)
        self.assertEqual(lt.ArgSpec("edx").read(ctx, rd), 86)
        self.assertEqual(lt.ArgSpec("d[rsp+0x28]").read(ctx, rd), 0x1000)
        self.assertEqual(lt.ArgSpec("[rsp+0x30]").read(ctx, rd), 0xABCDEF123456)
        self.assertIsNone(lt.ArgSpec("[rsp+0x30]").read(ctx, lambda a, n: None))


class CsvTests(unittest.TestCase):
    def test_header_and_row(self):
        self.assertEqual(lt.csv_header(3), ["ms", "tid", "bp", "ret", "a0", "a1", "a2"])
        row = lt.csv_row(12.345, 99, "sound", "0x140251b92", [0, 86, 0x7FF700001234], 4)
        self.assertEqual(row, ["12.3", "99", "sound", "0x140251b92", "0", "86", "0x7ff700001234", ""])

    def test_summary(self):
        s = lt.BpSpec("0x140276330:sound")
        counts = {}
        for vals in ([0, 86, 86, 4096, 4096, 0], [0, 86, 86, 4096, 4096, 5], [1, 2, 2, 4096, 4096, 0]):
            k = lt.summary_key(s, vals)
            counts[k] = counts.get(k, 0) + 1
        lines = lt.summary_lines(counts)
        self.assertEqual(lines[0].split(), ["2", "sound", "0", "86"])
        self.assertEqual(lines[1].split(), ["1", "sound", "1", "2"])


class PeTests(unittest.TestCase):
    def test_rva_to_offset(self):
        pe = bytearray(0x400)
        struct.pack_into("<I", pe, 0x3C, 0x80)
        struct.pack_into("<H", pe, 0x80 + 6, 1)       # one section
        struct.pack_into("<H", pe, 0x80 + 20, 0xF0)   # optional header size
        sec = 0x80 + 24 + 0xF0
        struct.pack_into("<IIII", pe, sec + 8, 0x2000, 0x1000, 0x2000, 0x400)  # vsize, va, rawsize, rawptr
        self.assertEqual(lt.pe_rva_to_offset(bytes(pe), 0x1010), 0x410)
        self.assertIsNone(lt.pe_rva_to_offset(bytes(pe), 0x500))



class XmmArgs(unittest.TestCase):
    def test_xmm_argument_reads_the_low_float_lane(self):
        import struct
        a = lt.ArgSpec("xmm2*")
        ctx = bytearray(lt.CTX_SIZE)
        struct.pack_into("<f", ctx, lt.XMM_OFF + 32, 0.2)
        self.assertTrue(a.key)
        self.assertAlmostEqual(a.read(bytes(ctx), None), 0.2, places=6)

    def test_xmm_index_is_limited_to_16_registers(self):
        with self.assertRaises(ValueError):
            lt.ArgSpec("xmm16")


if __name__ == "__main__":
    unittest.main()
