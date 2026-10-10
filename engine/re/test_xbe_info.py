import contextlib, io, os, struct, tempfile, unittest
import xbe_info

BASE = 0x10000


def build_xbe(title="Test Title", tid=0x4D530001, ordinals=(1, 49, 255), debug=False):
    d = bytearray(0x1000)
    w = lambda o, v: struct.pack_into("<I", d, o, v)
    d[0:4] = b"XBEH"
    w(0x104, BASE); w(0x108, 0x1000); w(0x10C, 0x3000); w(0x114, 1_090_000_000)
    cert_va, sec_va, names_va, thunk_va, lib_va = BASE + 0x200, BASE + 0x400, BASE + 0x600, BASE + 0x700, BASE + 0x680
    w(0x118, cert_va); w(0x11C, 2); w(0x120, sec_va)
    ek = xbe_info.ENTRY_KEYS["debug" if debug else "retail"]
    tk = xbe_info.THUNK_KEYS["debug" if debug else "retail"]
    w(0x128, (BASE + 0x1000) ^ ek)
    w(0x158, (BASE + 0x1800) ^ tk)           # thunk table lives in .rdata at file 0x1000+0x800
    w(0x160, 1); w(0x164, lib_va)
    struct.pack_into("<III", d, 0x200, 0x1D0, 1_095_000_000, tid)
    d[0x20C:0x20C + 80] = title.encode("utf-16le").ljust(80, b"\0")
    struct.pack_into("<IIIII", d, 0x200 + 0x9C, 1, 0x7, 0, 1, 3)
    for i, (nm, va, vs, ra, rs) in enumerate([(".text", BASE + 0x1000, 0x800, 0x1000, 0x800),
                                              (".rdata", BASE + 0x1800, 0x800, 0x1800, 0x100)]):
        o = 0x400 + i * 56
        struct.pack_into("<IIIIII", d, o, 6, va, vs, ra, rs, names_va + i * 8)
        d[0x600 + i * 8:0x600 + i * 8 + len(nm) + 1] = nm.encode() + b"\0"
    d[0x680:0x688] = b"XAPILIB\0"; struct.pack_into("<HHHH", d, 0x688, 1, 0, 5849, 0)
    d += bytes(0x1000)
    for i, o in enumerate(ordinals):
        struct.pack_into("<I", d, 0x1800 + i * 4, 0x80000000 | o)
    return bytes(d)


class T(unittest.TestCase):
    def test_parse(self):
        for dbg in (False, True):
            r = xbe_info.parse(build_xbe(debug=dbg))
            self.assertEqual(r["cert"]["title"], "Test Title")
            self.assertEqual(r["cert"]["title_id"], 0x4D530001)
            self.assertEqual(r["cert"]["title_id_str"], "MS-001")
            self.assertEqual([s["name"] for s in r["sections"]], [".text", ".rdata"])
            self.assertEqual(r["kernel_imports"], [1, 49, 255])
            self.assertEqual(r["entry"]["kind"], "debug" if dbg else "retail")
            self.assertEqual(r["libraries"][0]["name"], "XAPILIB")
            self.assertEqual(r["libraries"][0]["version"], "1.0.5849")

    def test_cli(self):
        with tempfile.TemporaryDirectory() as t:
            p = os.path.join(t, "default.xbe")
            with open(p, "wb") as fh: fh.write(build_xbe())
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                self.assertEqual(xbe_info.main([p]), 0)
            self.assertIn("Test Title", buf.getvalue()); self.assertIn("2004-", buf.getvalue())
            self.assertIn("entry 0x00011000 (retail key)", buf.getvalue())
            self.assertIn("(all regions)", buf.getvalue())
            bad = os.path.join(t, "bad")
            with open(bad, "wb") as fh: fh.write(b"MZ")
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(xbe_info.main([bad]), 1)
                self.assertEqual(xbe_info.main([os.path.join(t, "none")]), 1)


if __name__ == "__main__":
    unittest.main()
