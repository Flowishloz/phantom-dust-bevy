import contextlib, io, os, tempfile, unittest
import strings_scan as ss


def run(*args):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(io.StringIO()):
        rc = ss.main(list(args))
    return rc, buf.getvalue()


class T(unittest.TestCase):
    def test_scan_compare(self):
        with tempfile.TemporaryDirectory() as t:
            a, b = os.path.join(t, "a.bin"), os.path.join(t, "b.bin")
            body = b"\0\0alfn_Test\0junkjunk\0\0" + "pdeft_wide".encode("utf-16le") + b"\0\0xbox stuff\0"
            open(a, "wb").write(body + b"c:\\usr\\xbox\\src\\x.cpp\0D3DAssert here\0")
            open(b, "wb").write(body + b"newfile.alr\0")
            rc, out = run(a)
            self.assertEqual(rc, 0)
            self.assertIn("0x00000002 a alfn_Test", out)
            self.assertIn(" u pdeft_wide", out)
            self.assertNotIn("junkjunk", out)
            self.assertIn("D3DAssert", out)
            self.assertNotIn("D3DAssert", run(a, "--mask-sdk")[1])
            _, out = run("--compare", a, b)
            self.assertIn("only in " + a, out); self.assertIn("newfile.alr", out)
            self.assertEqual(out.count("alfn_Test"), 0)
            self.assertIn("junkjunk", run(a, "--all")[1])

    def test_missing(self):
        self.assertEqual(run("/nonexistent.xbe")[0], 1)
        self.assertEqual(run()[0], 1)


if __name__ == "__main__":
    unittest.main()
