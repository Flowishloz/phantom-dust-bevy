"""Synthetic regression tests; no game files required."""
import json
import struct
import tempfile
import unittest
import wave
from pathlib import Path

import skill_field_audit as audit
import sound_sheet as sheet


class SoundToolsTests(unittest.TestCase):
    def test_real_cache_sound_fields_not_categories_or_action_words(self):
        raw = [0] * 70
        raw[8], raw[13] = 12, 305
        raw[19:22] = [618, 603, 509]
        data = {"skills": [{"raw": raw, "s70_73": [1, 2, 3, 4],
                            "action": {"ground": {"cast_sound": 242,
                                                  "voice_group": 3, "raw": [77]}}}],
                "reactions": [{"sound": 501}, {"sound": -1}]}
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "skills.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            self.assertEqual(sheet.referenced_ids(path), {618, 603, 509, 242, 501})
        self.assertNotIn(8, {n for n, _ in audit.SOUND_FIELDS})

    def test_known_aliases_excluded_duplicates_grouped_and_output_local(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            wavdir = root / "sfx/common"
            wavdir.mkdir(parents=True)
            for sid, amplitude in [(35, 100), (34, 100), (85, 200),
                                   (90, 400), (91, 400), (603, 800)]:
                with wave.open(str(wavdir / f"{sid}.wav"), "wb") as wav:
                    wav.setnchannels(1)
                    wav.setsampwidth(2)
                    wav.setframerate(8000)
                    wav.writeframes(struct.pack("<h", amplitude) * 8000)
            raw = [0] * 70
            raw[20] = 603
            (root / "skills.json").write_text(json.dumps({"skills": [{"raw": raw}]}))
            out = root / "shots/sheet.html"
            self.assertEqual(sheet.build(root, None, out), 0)
            labels = json.loads((out.parent / "sound_labels_template.json").read_text())
            self.assertEqual(set(labels), {"90", "91"})
            text = out.read_text()
            self.assertEqual(text.count("<audio "), 1)
            self.assertIn('data-ids="90,91"', text)
            # Explicit selection can intentionally replay a known clip.
            self.assertEqual(sheet.build(root, "35", out), 0)
            self.assertIn('data-id="35"', out.read_text())


if __name__ == "__main__":
    unittest.main()
