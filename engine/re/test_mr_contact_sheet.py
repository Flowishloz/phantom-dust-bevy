"""Tests for the contact-sheet helpers that need no video."""
import unittest

import numpy as np

import mr_contact_sheet as cs


def row(t, events="", hp=40):
    return {"rec_tick": str(t), "events": events, "p1_health": str(hp), "ms": str(t * 33.3)}


class Landmarks(unittest.TestCase):
    def test_press_fire_hit_and_end_are_found(self):
        rows = [row(t) for t in range(100)]
        rows[39] = row(39, "press:1.2|sound:0.2")
        rows[52] = row(52, "fire:1.2")
        for t in range(60, 100):
            rows[t] = row(t, hp=36)
        lm = dict(cs.landmarks(rows))
        self.assertEqual(lm["press"], 39)
        self.assertEqual(lm["fire"], 52)
        self.assertEqual(lm["first hit"], 60)
        self.assertEqual(lm["end"], 99)
        self.assertEqual(lm["before press"], 33)

    def test_no_events_gives_only_the_end(self):
        self.assertEqual(cs.landmarks([row(t) for t in range(5)]), [("end", 4)])

    def test_sounds_alone_are_not_a_press(self):
        self.assertNotIn("press", dict(cs.landmarks([row(0, "sound:0.2"), row(1)])))


class Refine(unittest.TestCase):
    def test_finds_the_biggest_change_near_the_prediction(self):
        frames = np.zeros((60, 4, 4), dtype=np.float32)
        frames[31:] = 50.0        # a jump between frame 30 and 31 (15 fps -> 2.07 s)
        shift = cs.refine_offset(frames, 15, predicted_press_t=1.8)
        self.assertAlmostEqual(shift, 31 / 15.0 - 1.8, places=3)

    def test_far_jumps_outside_the_window_are_ignored(self):
        frames = np.zeros((90, 4, 4), dtype=np.float32)
        frames[80:] = 50.0
        self.assertEqual(cs.refine_offset(frames, 15, 1.0, search=1.0), cs.refine_offset(frames, 15, 1.0, search=1.0))
        self.assertLess(abs(cs.refine_offset(frames, 15, 1.0, search=1.0)), 1.1)

    def test_video_times_follow_the_wall_clock(self):
        meta = {"video_t0_s": 5.0, "video_clip_start_s": 4.2}
        vt = cs.video_times(meta, [row(0), row(30)])
        self.assertAlmostEqual(vt[30] - vt[0], 0.999, places=2)
        self.assertAlmostEqual(vt[0], 0.8, places=2)


if __name__ == "__main__":
    unittest.main()
