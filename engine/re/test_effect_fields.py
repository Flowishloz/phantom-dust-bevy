import io, json, os, struct, tempfile, unittest
import effect_fields as ef


def piece(shader, w19, w0=1):
    raw = [0] * 32; raw[0] = w0; raw[19] = w19
    return {"shader": shader, "raw": raw}


def mk(d, stem, pieces, eft=False):
    os.makedirs(d, exist_ok=True)
    json.dump({"pieces": pieces, "emitters": []}, open(os.path.join(d, stem + ".fx.json"), "w"))
    if eft:
        b = bytearray(412); struct.pack_into("<II", b, 0, 0, 0x1D); struct.pack_into("<hh", b, 244, 1, 0)
        open(os.path.join(d, stem + ".eft"), "wb").write(b)


class T(unittest.TestCase):
    def test_report_diff(self):
        with tempfile.TemporaryDirectory() as t:
            a, b = os.path.join(t, "a"), os.path.join(t, "b")
            mk(a, "eft0001", [piece("PDEFTBB01", 1), piece("PDEFTOPT05", 2)], eft=True)
            mk(a, "eft0002", [piece("PDEFTBB01", 1)])
            mk(b, "eft0001", [piece("PDEFTBB01", 1), piece("PDEFTOPT05", 0)])
            mk(b, "eft0003", [])
            open(os.path.join(a, "eft0009.fx.json"), "w").write("{corrupt")
            recs = ef.load_dir(a)
            buf = io.StringIO(); ef.report(recs, 5, buf); s = buf.getvalue()
            self.assertIn("== word19: 3 values, 2 distinct", s)
            self.assertIn("shader explains 100%", s)
            self.assertIn("eft.modelN.flags", s)
            csvp = os.path.join(t, "o.csv"); fields, bs = ef.collect(recs); ef.write_csv(csvp, fields, bs)
            self.assertIn("word19,PDEFTOPT05,0x00000002", open(csvp).read())
            buf = io.StringIO(); n = ef.diff(a, b, buf); s = buf.getvalue()
            self.assertEqual(n, 1); self.assertIn("piece 1 word19: 0x00000002 -> 0x00000000", s)
            self.assertIn("only in B", s)

    def test_missing(self):
        self.assertEqual(ef.main(["/nonexistent"]), 1)
        self.assertEqual(ef.main(["--diff", "/nx1", "/nx2"]), 1)


if __name__ == "__main__":
    unittest.main()
