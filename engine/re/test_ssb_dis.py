"""Tests for ssb_dis.py on synthetic scripts (no game data needed)."""
import struct
import unittest

import ssb_dis as S


def w(op, b1=0, s=0):
    return struct.pack("<I", op | (b1 << 8) | ((s & 0xFFFF) << 16))


def native(group, fn, pops):
    return struct.pack("<I", 0x01 | (pops << 8) | (fn << 16) | (group << 28))


def build(code, entry_word=None, extra_after_code=b""):
    """header(0x20 bytes) + code + one-entry function table for `main` pointing at word `entry_word`."""
    hdr_len = 0x20
    body = b"".join(code)
    ft = hdr_len + len(body) + len(extra_after_code)
    h1, h2 = S.name_hash("main")
    entry = struct.pack("<III", h1, h2, (hdr_len // 4) if entry_word is None else entry_word)
    end = ft + len(entry)
    header = struct.pack("<8I", hdr_len, ft, end, end, 0, 0xFFFFFFFF, 0, 0)
    return header + body + extra_after_code + entry


class NameHash(unittest.TestCase):
    def test_main(self):
        # computed by hand from the base-40 table: (((23*40+11)*40+19)*40+24)*40*40
        self.assertEqual(S.name_hash("main"), (0x8E225400, 0))

    def test_long_name_second_half(self):
        h1, h2 = S.name_hash("abcdefghi")
        self.assertEqual(h1, ((((11 * 40 + 12) * 40 + 13) * 40 + 14) * 40 + 15) * 40 + 16)
        self.assertEqual(h2, ((((17 * 40 + 18) * 40 + 19) * 40 + 0) * 40 + 0) * 40 + 0)

    def test_unmapped_char(self):
        self.assertEqual(S.name_hash("a-b")[0], -1)


class Parse(unittest.TestCase):
    def test_header_and_export(self):
        d = build([w(0x06, 0, 1), w(0x05, 0, 1)])
        s = S.Ssb(d)
        self.assertEqual(s.code_start, 0x20)
        self.assertEqual(s.exports(), [(0x8E225400, 0, 0x20)])
        self.assertEqual(s.find_export("main"), 0x20)
        self.assertIsNone(s.find_export("other"))

    def test_decode_push32_is_two_words(self):
        d = build([w(0x0F), struct.pack("<i", -123456), w(0x05)])
        s = S.Ssb(d)
        ins = S.decode(s)
        self.assertEqual([i.op for i in ins], [0x0F, 0x05])
        self.assertEqual(ins[0].imm, -123456)
        self.assertEqual(ins[1].off, 0x28)

    def test_branch_target_is_relative_to_next_word(self):
        d = build([w(0x07, 0, 1), w(0x00), w(0x05)])
        i = S.decode(S.Ssb(d))[0]
        self.assertEqual(i.target(), 0x20 + 4 + 4)

    def test_native_fields(self):
        i = S.Instr(0, 0x40B10101)
        self.assertEqual((i.group, i.fn, i.b1), (4, 0xB1, 1))

    def test_push_data_address_signed_24(self):
        word = ((-3 & 0xFFFFFF) << 8) | 0x1F
        i = S.Instr(0x40, word)
        self.assertEqual(i.target(), 0x40 + 4 - 12)


class Flow(unittest.TestCase):
    def test_reachable_skips_inline_table(self):
        # main: jmp over a 2-word table, then ret. The table words must not be decoded.
        d = build([w(0x07, 0, 2), struct.pack("<i", 0x40000001), struct.pack("<i", 0x40000001), w(0x05)])
        r = S.reachable(S.Ssb(d))
        self.assertEqual(sorted(r), [0x20, 0x2C])

    def test_stack_balanced_function_with_args(self):
        # main calls f(arg) ; f has 1 arg + 1 local and returns a value
        main = [w(0x0E, 0, 7), w(0x03, 0, 1), w(0x05, 0, 0)]  # push 7; call f (+1 word); ret pop 0
        f = [w(0x06, 0, 1), w(0x09, 0, 0), w(0x0A, 0, -1), w(0x09, 0, -1), w(0x05, 1, 2)]
        d = build(main + f)
        bad, sig = S.verify_stack(S.Ssb(d))
        self.assertEqual(bad, [])
        self.assertEqual(sig[0x20 + 12], 1)  # one argument

    def test_stack_imbalance_detected(self):
        d = build([w(0x0E, 0, 1), w(0x05, 0, 0)])  # leaves a word behind
        bad, sig = S.verify_stack(S.Ssb(d))
        # one return path with depth 1 and s=0 gives a (phantom) argument count of -1
        self.assertEqual(sig[0x20], -1)


class Text(unittest.TestCase):
    def test_text_forms(self):
        self.assertEqual(S.text(S.Instr(0, 0x00020E0E | 0)), "push 2")
        self.assertEqual(S.text(S.Instr(0, 0x40700001)), "native g4.f112 pop=0")
        self.assertEqual(S.text(S.Instr(0, 0x00010017)), "cmp.NE")
        self.assertTrue(S.text(S.Instr(0, 0x00030107)).startswith("jnz"))

    def test_disassemble_labels(self):
        d = build([w(0x06, 0, 1), w(0x05, 0, 1)])
        out = S.disassemble(S.Ssb(d))
        self.assertIn("export#0:", out)
        self.assertIn("alloc 1", out)

    def test_native_usage(self):
        d = build([w(0x0E, 0, 1), w(0x0E, 0, 2), native(4, 0, 2), w(0x05, 0, 0)])
        self.assertEqual(S.native_usage(S.Ssb(d)), {(4, 0): {2}})


if __name__ == "__main__":
    unittest.main()
