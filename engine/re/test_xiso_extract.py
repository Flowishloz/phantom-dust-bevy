import io, os, struct, tempfile, unittest
import xiso_extract as X

S = X.SECTOR


def entry(left, right, start, size, attrs, name):
    raw = struct.pack("<HHIIBB", left, right, start, size, attrs, len(name)) + name.encode()
    return raw + b"\xff" * (-len(raw) % 4)


def build_iso(prefix_bytes=0):
    """Tiny xiso: root holds a.txt and sub/ ; sub holds b.bin and c.txt (two-level binary trees)."""
    img = bytearray(prefix_bytes + 80 * S)

    def put(sector, data):
        o = prefix_bytes + sector * S
        if len(img) < o + len(data):
            img.extend(b"\0" * (o + len(data) - len(img)))
        img[o:o + len(data)] = data

    a, b, c = b"hello xbox", bytes(range(256)) * 20, b"third"
    put(40, a)
    put(41, b)
    put(50, c)
    # sub directory: b.bin is the tree root, c.txt its right child (offset after b.bin's entry)
    b_len = len(entry(0, 0, 41, len(b), 0, "b.bin"))
    sub = entry(0, b_len // 4, 41, len(b), 0, "b.bin") + entry(0, 0, 50, len(c), 0, "c.txt")
    put(60, sub.ljust(S, b"\xff"))
    # root: 'sub' is the tree root, 'a.txt' its left child
    sub_len = len(entry(0, 0, 60, S, 0x10, "sub"))
    root = entry(sub_len // 4, 0xFFFF, 60, S, 0x10, "sub") + entry(0, 0, 40, len(a), 0, "a.txt")
    put(70, root.ljust(S, b"\xff"))
    desc = X.MAGIC + struct.pack("<IIQ", 70, S, 0)
    put(32, desc.ljust(S, b"\0"))
    return bytes(img), {"a.txt": a, "sub/b.bin": b, "sub/c.txt": c}


class XisoTest(unittest.TestCase):
    def test_walk(self):
        img, files = build_iso()
        got = {p: (st, sz) for p, d, st, sz in X.walk(io.BytesIO(img)) if not d}
        self.assertEqual(set(got), set(files))
        self.assertEqual(got["sub/b.bin"], (41, len(files["sub/b.bin"])))

    def test_extract_bytes(self):
        img, files = build_iso()
        with tempfile.TemporaryDirectory() as t:
            n, tot = X.extract(io.BytesIO(img), t, log=lambda *_: None)
            self.assertEqual(n, 3)
            for p, data in files.items():
                with open(os.path.join(t, p), "rb") as fh:
                    self.assertEqual(fh.read(), data)

    def test_partition_offset_detected(self):
        img, files = build_iso(prefix_bytes=0x2080000)
        f = io.BytesIO(img)
        base = X.find_offset(f)
        self.assertEqual(base, 0x2080000)
        self.assertEqual(len([1 for p, d, *_ in X.walk(f, base) if not d]), 3)

    def test_bad_image(self):
        with self.assertRaises(X.XisoError):
            X.find_offset(io.BytesIO(b"\0" * (40 * S)))

    def test_unsafe_name_rejected(self):
        img, _ = build_iso()
        bad = bytearray(img)
        i = bad.find(b"a.txt")
        bad[i:i + 5] = b"a/txt"
        with self.assertRaises(X.XisoError):
            list(X.walk(io.BytesIO(bytes(bad))))


if __name__ == "__main__":
    unittest.main()
