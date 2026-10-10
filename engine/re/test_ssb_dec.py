"""Tests for ssb_dec.py on synthetic scripts."""
import struct
import unittest

import ssb_dec as C
import ssb_dis as D
from test_ssb_dis import build, native, w


def dec(words, extra=b""):
    return C.decompile(D.Ssb(build(words, extra_after_code=extra)))


class Decompile(unittest.TestCase):
    def test_native_call_and_store(self):
        # R[7] = self() + 1 ; setvar(a0 = index pushed last)
        text = dec([native(4, 112, 0), w(0x10), w(0x0E, 0, 1), w(0x12), w(0x0E, 0, 7), native(4, 0, 2), w(0x05)])
        self.assertIn("t1 = self();", text)
        self.assertIn("t2 = setvar(7, (t1 + 1));", text)

    def test_comparison_orientation_and_branch(self):
        # if (5 > 3) goto end
        words = [w(0x0E, 0, 5), w(0x0E, 0, 3), w(0x17, 0, 2), w(0x07, 2, 1), w(0x0E, 0, 9), w(0x0D, 0, 1), w(0x05)]
        text = dec(words)
        self.assertIn("if (!((5 > 3))) goto L", text)
        self.assertIn("L", text.split("goto")[1])

    def test_case_chain(self):
        words = [w(0x0E, 0, 2), w(0x08, 1, 1), w(0x0D, 0, 0), w(0x08, 2, 1), w(0x0D, 0, 0), w(0x05)]
        text = dec(words)
        self.assertIn("!= 1) goto", text)
        self.assertIn("// case 2", text)

    def test_float_immediate_is_annotated(self):
        words = [w(0x0F), struct.pack("<i", -1083808154), w(0x0E, 0, 0), native(4, 0, 2), w(0x05)]
        self.assertIn("-0.9f", dec(words))

    def test_call_arguments_are_named(self):
        main = [w(0x0E, 0, 5), w(0x0E, 0, 7), w(0x03, 0, 1), w(0x05), w(0x06, 0, 0), w(0x05, 0, 2)]
        text = dec(main)
        self.assertIn("func_000030(7, 5)", text)

    def test_native_names_cover_the_script_natives(self):
        for fn in (0, 1, 48, 112, 186, 229, 425):
            self.assertNotIn("g4_f%d" % fn, C.native_name(4, fn))
        self.assertEqual(C.native_name(0, 16), "tbl")


if __name__ == "__main__":
    unittest.main()
