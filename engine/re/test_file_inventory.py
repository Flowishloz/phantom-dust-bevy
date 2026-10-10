import contextlib, io, os, tempfile, unittest
import file_inventory as fi


def put(root, rel, data):
    p = os.path.join(root, rel); os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "wb") as f:
        f.write(data)


class T(unittest.TestCase):
    def test_inventory_diff(self):
        with tempfile.TemporaryDirectory() as t:
            a, b = os.path.join(t, "a"), os.path.join(t, "b")
            put(a, "Data/x.ALR", b"ALR0same"); put(a, "gone.bin", b"zz"); put(a, "chg.xbe", b"XBEH1")
            put(b, "Data/x.ALR", b"ALR0same"); put(b, "new.stx", b"STX!"); put(b, "chg.xbe", b"XBEH22")
            ca, cb = os.path.join(t, "a.csv"), os.path.join(t, "b.csv")
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(fi.main([a, "-o", ca]), 0); self.assertEqual(fi.main([b, "-o", cb]), 0)
            rows = fi.read_csv(ca)
            self.assertEqual(rows["Data/x.ALR"]["extension"], ".alr")
            self.assertTrue(rows["chg.xbe"]["magic"].endswith("XBEH"))
            buf = io.StringIO()
            self.assertEqual(fi.diff(rows, fi.read_csv(cb), buf), (1, 1, 1))
            self.assertIn("CHANGED\tchg.xbe", buf.getvalue())

    def test_missing(self):
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(fi.main(["/nonexistent"]), 1)
            self.assertEqual(fi.main(["--diff", "/nx1.csv", "/nx2.csv"]), 1)


if __name__ == "__main__":
    unittest.main()
