import json, os, struct, tempfile, unittest, wave
import sound_sheet


def mkwav(path, amp):
    with wave.open(path, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(8000)
        w.writeframes(b"".join(struct.pack("<h", amp if i % 2 else -amp) for i in range(8000)))


class T(unittest.TestCase):
    def test_sheet(self):
        with tempfile.TemporaryDirectory() as d:
            sfx = os.path.join(d, "sfx", "common"); os.makedirs(sfx)
            for i, amp in ((10, 16384), (20, 8000), (30, 100)):
                mkwav(os.path.join(sfx, f"{i}.wav"), amp)
            json.dump({"skills": [{"s21": 10, "x": 1}]}, open(os.path.join(d, "skills.json"), "w"))
            out = os.path.join(d, "shots", "s.html")
            self.assertEqual(sound_sheet.main(["--cache", d, "--out", out, "--template", os.path.join(d, "t.json")]), 0)
            h = open(out).read()
            self.assertNotIn('data-id="10"', h)      # referenced by a skill
            self.assertIn('data-id="20"', h); self.assertIn("1.00", h); self.assertIn("0.24", h)
            self.assertIn("Blob", h)
            sound_sheet.main(["--cache", d, "--out", out, "--ids", "10", "--template", os.path.join(d, "t.json")])
            self.assertIn('data-id="10"', open(out).read())

    def test_missing(self):
        self.assertEqual(sound_sheet.main(["--cache", "/nonexistent", "--out", "/tmp/x.html"]), 1)


if __name__ == "__main__":
    unittest.main()
