"""Hand-computed cases for threat_predict.py (formulas read from FUN_14020f9a0 / FUN_14020f8c0)."""
import math
import unittest

import threat_predict as T

ME = 1


def skill(s63, **kw):
    d = {"s43": 0, "s63": s63, "s55": 0, "s52": 0, "s64": 0, "s65": 0}
    d.update(kw)
    return d


def obj(**kw):
    d = {"state": 1, "skill": 7, "age": 100, "hitmask": 0b10, "attacker": 0, "target": None,
         "dir": (0.0, 0.0, 1.0), "pos": (0.0, 0.0, 0.0), "radius": 2.0,
         "f1c0": 2.0, "f1c4": 2.0, "f1cc": 0.1, "f1d0": 2.0, "i1cc": 33, "i1d4": 44, "yaw": 0}
    d.update(kw)
    return d


def run(o, s, pos=(0.0, 0.0, 20.0), margin=3.0, ground_ok=1, clear=True):
    return T.predict(o, ME, pos, margin, ground_ok, lambda _id: s, lambda a, b: clear)


class Gates(unittest.TestCase):
    def test_not_for_me_or_inactive_or_non_attack(self):
        self.assertEqual(run(obj(hitmask=0b01), skill(0)), 0)
        self.assertEqual(run(obj(state=0), skill(0)), 0)
        self.assertEqual(run(obj(), skill(0, s43=2)), 0)
        self.assertEqual(run(None, skill(0)), 0)

    def test_excluded_motions(self):
        for m in (3, 4, 12, 13, 14):
            self.assertEqual(run(obj(), skill(m)), 0, m)

    def test_unknown_motion_is_harmless(self):
        self.assertEqual(run(obj(), skill(8)), 0)
        self.assertEqual(run(obj(), skill(0x11)), 0)


class Straight(unittest.TestCase):
    def test_state3_per_second_speed(self):
        # distance 20, margin 3: d = 17; speed 60/s -> 2 per tick; (17 - 2) / 2 = 7.5 -> 7
        self.assertEqual(run(obj(state=3, f1c0=60.0), skill(0)), 7)

    def test_state3_too_slow_to_matter_within_a_second(self):
        self.assertEqual(run(obj(state=3, f1c0=10.0), skill(0)), 0)

    def test_state3_minimum_one_tick_and_blocked_ray(self):
        self.assertEqual(run(obj(state=3, f1c0=60.0, pos=(0, 0, 19.0)), skill(0)), 1)
        self.assertEqual(run(obj(state=3, f1c0=60.0), skill(0), clear=False), 0)

    def test_motion9_uses_skill_speed(self):
        # speed = s64 * 0.1 = 60 per second
        self.assertEqual(run(obj(), skill(9, s64=600)), 7)


class Homing(unittest.TestCase):
    def test_case0_homing_on_me(self):
        # dist 20, m2 = 3 + 2, v = 2/tick: (20 - 5) / 2 = 7.5 -> 7; heading toward me (dot = 1)
        o = obj(target=ME)
        self.assertEqual(run(o, skill(0)), 7)

    def test_case0_homing_on_someone_else_or_moving_away(self):
        self.assertEqual(run(obj(target=0), skill(0)), 0)
        self.assertEqual(run(obj(target=ME, dir=(0, 0, -1.0)), skill(0)), 0)

    def test_case0_older_than_its_life_is_dropped(self):
        self.assertEqual(run(obj(target=ME, age=5), skill(0)), 0)

    def test_case0_free_flight_closest_approach(self):
        # dir +z, 2 per tick for 100 ticks, I stand at z = 20 on the line: closest point is me, t = 20 / 2 = 10
        self.assertEqual(run(obj(), skill(0)), 10)
        # I stand 10 to the side: beyond margin + radius (5) -> harmless
        self.assertEqual(run(obj(), skill(0), pos=(10.0, 0.0, 20.0)), 0)

    def test_case2_needs_me_below_the_object(self):
        o = obj(target=ME, pos=(0, 30.0, 20.0))
        self.assertEqual(run(o, skill(2), pos=(0, 0, 20.0)), int((30.0 - 5.0) / 2.0))
        self.assertEqual(run(obj(target=ME), skill(2)), 0)

    def test_case5_needs_ground_and_uses_horizontal_speed(self):
        o = obj(target=ME)
        self.assertEqual(run(o, skill(5)), 7)
        self.assertEqual(run(o, skill(5), ground_ok=0), 0)

    def test_case6_returns_precomputed_when_homing_on_me(self):
        self.assertEqual(run(obj(target=ME), skill(6)), 33)
        self.assertEqual(run(obj(target=None), skill(6)), 0)

    def test_case1_homing_returns_stored_time(self):
        self.assertEqual(run(obj(target=ME), skill(1)), 44)
        self.assertEqual(run(obj(target=0), skill(1)), 0)


class Ballistic(unittest.TestCase):
    def test_lob_landing_on_me(self):
        # a = 0.1, vy = 2: t_apex = 20, apex = 0.5*0.1*400 = 20 (+ y0 0); I stand at y = 0 -> fall = sqrt(40/0.1) = 20
        # total 40 ticks; horizontal 2/tick toward yaw 0 (+z) -> lands at z = 80
        o = obj(f1c4=2.0, f1cc=0.1, f1c0=2.0)
        self.assertEqual(run(o, skill(1, s52=100), pos=(0, 0, 80.0)), 40)
        self.assertEqual(run(o, skill(1, s52=100), pos=(0, 0, 200.0)), 0)

    def test_lob_cases_that_are_harmless(self):
        o = obj(f1c4=2.0, f1cc=0.1, f1c0=2.0)
        self.assertEqual(run(o, skill(1, s52=100), pos=(0, 500.0, 80.0)), 0)   # above the apex
        self.assertEqual(run(obj(f1c4=-2.0, f1cc=0.1), skill(1, s52=100), pos=(0, 10.0, 80.0)), 0)  # falling, I am above its start


class Delays(unittest.TestCase):
    def test_case7_age_minus_delay_and_own_object(self):
        self.assertEqual(run(obj(age=90), skill(7, s65=20)), 70)
        self.assertEqual(run(obj(attacker=ME), skill(7, s65=20)), 0)

    def test_case7_delay_is_signed_16_bit(self):
        self.assertEqual(run(obj(age=10), skill(0xF, s65=0xFFFF)), 11)

    def test_case10_line_on_ground(self):
        # runs along +z from (0,0,0): 2/tick, 100 ticks; I am at z = 20 (margin + radius = 5): (20 - 5) / 2 = 7
        self.assertEqual(run(obj(), skill(10)), 7)
        self.assertEqual(run(obj(), skill(10), ground_ok=0), 0)


class Helpers(unittest.TestCase):
    def test_closest_point_clamps(self):
        self.assertEqual(T.closest_point_on_segment((5, 5, 0), (0, 0, 0), (10, 0, 0)), (5.0, 0.0, 0.0))
        self.assertEqual(T.closest_point_on_segment((-5, 5, 0), (0, 0, 0), (10, 0, 0)), (0.0, 0.0, 0.0))
        self.assertEqual(T.closest_point_on_segment((50, 5, 0), (0, 0, 0), (10, 0, 0)), (10.0, 0.0, 0.0))

    def test_trunc_like_cvttss2si(self):
        self.assertEqual(T._trunc(-7.9), -7)
        self.assertEqual(T._trunc(7.9), 7)
        self.assertEqual(T._trunc(float("nan")), -2147483648)


if __name__ == "__main__":
    unittest.main()
