import unittest

import film_traj as ft


def orig_rows(ys):
    return [{"rec_tick": str(t), "p0_x": "0", "p0_y": str(y), "p0_z": "0"} for t, y in enumerate(ys)]


class FilmTrajTests(unittest.TestCase):
    def test_a_body_that_never_rises_is_flagged_at_the_rise(self):
        orig = orig_rows([0, 0, 10, 18.5, 18.5])
        ours = {ft.COUNTDOWN + 2 * t: {"p0_x": 0.0, "p0_y": 0.0, "p0_z": 0.0} for t in range(5)}
        dy, at, dh, _ = ft.path_diff(orig, ours, "p0")
        self.assertEqual((dy, at, dh), (18.5, 3, 0.0))

    def test_the_same_path_is_clean_and_z_mirroring_does_not_matter(self):
        orig = [{"rec_tick": str(t), "p0_x": str(5 * t), "p0_y": "270", "p0_z": "100"} for t in range(4)]
        ours = {ft.COUNTDOWN + 2 * t: {"p0_x": 5.0 * t, "p0_y": 270.0, "p0_z": -100.0} for t in range(4)}
        self.assertEqual(ft.path_diff(orig, ours, "p0"), (0.0, None, 0.0, None))


if __name__ == "__main__":
    unittest.main()
