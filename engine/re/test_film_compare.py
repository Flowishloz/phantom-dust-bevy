import os
import unittest

import film_compare as fc


class FilmCompareTests(unittest.TestCase):
    def test_tick_mapping_round_trips_after_the_countdown(self):
        self.assertEqual(fc.mine(0), 180)
        self.assertEqual(fc.mine(39), 258)
        self.assertEqual(fc.theirs(fc.mine(59)), 59)

    def test_sound_keys_match_the_originals_bank_dot_id(self):
        self.assertEqual(fc.sound_key("sfx/common/358.wav"), "0.358")
        self.assertEqual(fc.sound_key("sfx/BattleVo_Hero_en/12.wav"), "V")
        self.assertIsNone(fc.sound_key("music/x.ogg"))

    def test_diff_flags_ticks_stats_sounds_and_effects_but_not_noise(self):
        om = {"fire": 59, "hit": None, "p0_aura": -4.0, "p0_level": 0, "p0_health": 0, "p1_aura": 0, "p1_level": 0, "p1_health": 0}
        um = {"fire": 70.0, "hit": 62.0, "hit_damage": 4, "p0_aura": -4.0, "p0_level": 0, "p0_health": 0, "p1_aura": 0, "p1_level": 0, "p1_health": -4}
        d = fc.diff(om, um, {"0.604": 1, "0.30": 1, "1.1": 2}, {"53": 1}, {"0.6": 1}, {"53": 1, "600": 0})
        kinds = [k for k, _ in d]
        self.assertEqual(kinds, ["fire_tick", "hit_tick", "p1_health", "sound_missing", "sound_extra"])

    def test_footsteps_and_sounds_after_the_takes_end_are_not_mismatches(self):
        z = {"fire": 59, "hit": None, "end": 100, "p0_aura": 0, "p1_health": 0}
        o = {"fire": 59, "hit": None, "p0_aura": 0, "p1_health": 0}
        # 0.26 / 0.27: footsteps (original-only here); 0.5 at t140 is after the take's last tick (100); 0.5 at t90 is inside it
        d = fc.diff(z, o, {"0.26": 5, "0.27": 6}, {}, {"0.5": 140}, {})
        self.assertEqual(d, [])
        d = fc.diff(z, o, {}, {}, {"0.5": 90}, {})
        self.assertEqual([k for k, _ in d], ["sound_extra"])

    def test_cast_end_is_compared(self):
        d = fc.diff({"fire": 1, "hit": None, "cast_end": 93}, {"fire": 1, "hit": None, "cast_end": 99.5}, {}, {}, {}, {})
        self.assertEqual([k for k, _ in d], ["cast_end"])
        self.assertEqual(fc.diff({"fire": 1, "hit": None, "cast_end": 93}, {"fire": 1, "hit": None, "cast_end": 93.5}, {}, {}, {}, {}), [])

    def test_close_ticks_are_not_a_mismatch(self):
        z = {"fire": 59, "hit": 70, "p0_aura": 0, "p1_health": 0}
        o = {"fire": 60.5, "hit": 69.0, "p0_aura": 0.2, "p1_health": 0}
        self.assertEqual(fc.diff(z, o, {}, {}, {}, {}), [])

    def _row(self, tick, h0, h1, x1=0.0, lv=12, aura=12.0):
        # T,tick,p0_health,p0_level,p0_aura,p0_action,p0_skill,p0_x,p0_z,p1_health,p1_level,p1_aura,p1_action,p1_skill,p1_x,p1_z
        return ["T", str(tick), str(h0), str(lv), "%.2f" % aura, "Ready/0", "0", "0", "0", str(h1), str(lv), "%.2f" % aura, "Ready/0", "0", str(x1), "0"]

    def test_a_fall_out_respawn_is_set_aside_and_a_round_restart_ends_the_comparison(self):
        sc = {"press": 0}
        rows = [self._row(t, 20, 40) for t in range(181, 200)]
        rows += [self._row(200, 20, 37, x1=0.0), self._row(201, 20, 34, x1=300.0)]      # a hit, then a fall-out (-3, jumps)
        rows += [self._row(t, 20, 34, x1=300.0) for t in range(202, 210)]
        rows += [self._row(210, 20, 20, lv=0, aura=0.0)]                                # our round restarts: level 0, aura 0, 20 / 20
        um = fc.our_metrics(rows, [], sc, 100)
        self.assertEqual(um["fall_outs"]["p1"], 1)
        self.assertEqual(um["p1_health"], -3.0, "the -3 of the respawn is the stage, the hit's -3 is the skill")
        self.assertEqual(um["p1_level"], 0.0, "rows from the restart on are not compared")
        self.assertEqual(um["cut"], 14)

    def test_an_automatic_guard_and_a_cost_free_swing_count_as_fired(self):
        sc = {"press": 0}
        rows = [self._row(t, 20, 40) for t in range(181, 200)]
        um = fc.our_metrics(rows, [(260, "Blocked { attacker: 1, victim: 0, skill: 80, broken: false }")], sc, 100)
        self.assertEqual(um["fire"], fc.theirs(260))

    def _orig_row(self, t, h1, x1=0.0, h0=20):
        r = {"rec_tick": str(t), "events": ""}
        for who, h, x in (("p0", h0, 0.0), ("p1", h1, x1)):
            r.update({who + "_health": str(h), who + "_level": "12", who + "_aura": "12.0", who + "_x": str(x), who + "_y": "0", who + "_z": "0"})
        return r

    def test_a_fall_out_is_not_the_original_hitting(self):
        # LIVE 313_Vacuum_Palm: P1 flung off the arena, -3 with a jump across the stage, sent back to its spawn: the stage, not a hit.
        rows = [self._orig_row(t, 40) for t in range(60)] + [self._orig_row(60, 37, x1=300.0)] + [self._orig_row(61, 37, x1=300.0)]
        om = fc.orig_metrics({"rows": rows, "meta": {"press_tick": 0, "fire_tick": 5}})
        self.assertIsNone(om["hit"])
        rows[30]["p1_health"] = "38"       # a real hit before it is still found
        rows[31:] = [dict(r, p1_health="38") if r["p1_health"] == "40" else r for r in rows[31:]]
        om = fc.orig_metrics({"rows": rows, "meta": {"press_tick": 0, "fire_tick": 5}})
        self.assertEqual(om["hit"], 30)

    def test_random_strength_and_rain_drops_are_set_aside_but_other_damage_is_not(self):
        fc._SKILLS = {
            319: {"id": 319, "capsule": "attack", "raw": [0] * 45 + [28, 3] + [0] * 23, "s70_73": [0, 0, 0, 0]},
            1: {"id": 1, "capsule": "attack", "raw": [0] * 45 + [0, 3] + [0] * 23, "s70_73": [0, 0, 0, 0]},
            21: {"id": 21, "capsule": "attack", "raw": [0] * 63 + [20, 30, 30, 6, 5, 0, 1000], "s70_73": [300, 15, 90, 0]},
        }
        try:
            d = [("hit_damage", "x"), ("p0_health", "y"), ("p1_health", "z")]
            # Jack Pot rolled 3 in the original and 6 here: the same rule, a different draw
            res = {"notes": []}
            out = fc.set_aside_random(res, None, {"skill": 319}, {"hit_damage": 3, "p0_health": -3, "p1_health": -3}, {"hit_damage": 6, "p0_health": -6, "p1_health": -6}, list(d))
            self.assertEqual(out, [])
            self.assertEqual(len(res["notes"]), 1)
            # A damage of 4 is outside {0, 3, 6}: still a mismatch; and a fixed-STR skill is never set aside
            res = {"notes": []}
            self.assertEqual(len(fc.set_aside_random(res, None, {"skill": 319}, {"hit_damage": 3, "p0_health": 0, "p1_health": -3}, {"hit_damage": 4, "p0_health": 0, "p1_health": -4}, list(d))), 2)
            self.assertEqual(len(fc.set_aside_random(res, None, {"skill": 1}, {"hit_damage": 3, "p0_health": -3, "p1_health": -3}, {"hit_damage": 6, "p0_health": -6, "p1_health": -6}, list(d))), 3)
            # Rain of Rocks: first drop lands 31 + 12 ticks after the fire tick, the sixth 61 + 12; the original +41, ours +54 are both inside
            res = {"notes": []}
            om, um = {"fire": 69, "hit": 110}, {"fire": 69.5, "hit": 123.5}
            self.assertEqual(fc.set_aside_random(res, None, {"skill": 21}, om, um, [("hit_tick", "t")]), [])
            um = {"fire": 69.5, "hit": 190.0}      # a hit long after the last drop is a real difference
            self.assertEqual(len(fc.set_aside_random(res, None, {"skill": 21}, om, um, [("hit_tick", "t")])), 1)
        finally:
            fc._SKILLS = None

    def test_sounds_a_take_cannot_be_fair_about_are_set_aside(self):
        fc._SKILLS = {
            216: {"id": 216, "capsule": "erase", "raw": [0] * 48 + [1, 2, 1] + [0] * 19},     # erase-on-hit, attack colour
            221: {"id": 221, "capsule": "erase", "raw": [0] * 48 + [9, 3, 4] + [0] * 19},     # filter 9 matches an Aura Particle
            188: {"id": 188, "capsule": "attack", "raw": [0] * 70},
        }
        try:
            diffs = lambda: [("sound_missing", "sounds missing in ours: 0.86 0.91 0.8"), ("sound_extra", "sounds extra in ours: 0.2")]
            # the recorder never fired it: no sound of the take is compared
            res = {"notes": []}
            self.assertEqual(fc.set_aside_sound(res, {"meta": {}}, {"skill": 188}, {"fire": None}, diffs()), [])
            self.assertEqual(len(res["notes"]), 2)
            # the original's dash captured a capsule: only 0.86 goes
            res = {"notes": []}
            out = fc.set_aside_sound(res, {"meta": {"capsule_touched": True}}, {"skill": 188}, {"fire": 59}, diffs())
            self.assertEqual(out, [("sound_missing", "sounds missing in ours: 0.91 0.8"), ("sound_extra", "sounds extra in ours: 0.2")])
            # an erase-on-hit filter that an Aura-only hand cannot match: 0.91 goes; filter 9 keeps it
            res = {"notes": []}
            out = fc.set_aside_sound(res, {"meta": {}}, {"skill": 216}, {"fire": 59}, diffs())
            self.assertEqual(out[0], ("sound_missing", "sounds missing in ours: 0.86 0.8"))
            out = fc.set_aside_sound({"notes": []}, {"meta": {}}, {"skill": 221}, {"fire": 59}, diffs())
            self.assertEqual(out[0], ("sound_missing", "sounds missing in ours: 0.86 0.91 0.8"))
            # a missing list that empties disappears
            out = fc.set_aside_sound({"notes": []}, {"meta": {"capsule_touched": True}}, {"skill": 188}, {"fire": 59}, [("sound_missing", "sounds missing in ours: 0.86")])
            self.assertEqual(out, [])
        finally:
            fc._SKILLS = None

    def test_real_placement_is_off_when_the_game_left_a_fighter_at_its_own_start(self):
        sc = {"p0xyz": [30.32, 154.22, 270.0], "p1xyz": [-38.7, 165.89, 270.0]}
        row = self._row(180, 20, 40)
        row[7], row[8], row[14], row[15] = "31.4", "-155.7", "-38.7", "-165.9"     # our z is the mirror of the original's; pushed 1.5 u: still the original's spot
        rows = [row] * 200
        self.assertEqual(fc.placement_off(rows, sc), [])
        far = list(row)
        far[7], far[8] = "32.4", "-156.7"          # our collision pushed P0 3 u off a wall the original stands against: not reproduced (START_OFF_TOL 2 u)
        self.assertEqual(fc.placement_off([far] * 200, sc), ["P0"])
        row = list(row)
        row[7], row[8] = "-46.6", "-260.2"                                          # no floor under the original's spot: P0 stayed at the start
        self.assertEqual(fc.placement_off([row] * 200, sc), ["P0"])

    def test_field_from_take_reads_the_recorders_field_off_what_the_skill_did(self):
        """LIVE takes: Meltdown P0 Level -4 / P1 -2 (s52 = 2) = 2 + 1 Aura Particles; Overheat Health -3 (s46 = 1) = 3; Violent Change = six common/92."""
        if not (os.path.exists(fc.CACHE) and os.path.isdir(fc.TAKES)):
            self.skipTest("needs the cache and the recorder takes")
        got = {}
        for r in fc.skill_list():
            if int(r["id"]) in (102, 86, 284, 69):
                o = fc.load_orig(int(r["id"]), r["name"])
                got[int(r["id"])] = fc.field_from_take(o, fc.skill_record(int(r["id"]))) if o else "no take"
        if got.get(102) == "no take":
            self.skipTest("no takes")
        self.assertEqual(got[102], (2, 1, 1, 2))
        self.assertEqual(got[86], (2, 1, 1, 2))
        self.assertEqual(got[284], (0, 0, 3, 3))
        self.assertIsNone(got.get(69))        # an ordinary skill: the film keeps its empty field

    def test_original_spots_are_mirrored_into_our_world(self):
        # Our collision and render are right-handed (z mirrored); the program's trace is in the .dat's own space.
        self.assertEqual(fc.to_sim_xz([314.51, -337.3]), [314.51, 337.3])
        import math
        # facing +z in the original (yaw 0) is facing -z in ours; a facing along +x stays along +x
        self.assertAlmostEqual(abs(fc.to_sim_yaw(0.0)), math.pi, places=6)
        self.assertAlmostEqual(fc.to_sim_yaw(math.pi / 2), math.pi / 2, places=6)
        self.assertIsNone(fc.to_sim_yaw(None))

    def test_split_mode_is_the_default_and_crops_both_to_the_same_aspect(self):
        self.assertTrue(fc.SPLIT)
        self.assertEqual(fc.WINDOW, "1440x900")
        self.assertEqual(fc.ORIG_STRIP, (0, 186, 720, 571))
        x0, y0, x1, y1 = fc.crop_ours()                  # our top strip, centre-cropped to the original strip's 720:385
        self.assertEqual((y0, y1), (0, 450))
        self.assertAlmostEqual((x1 - x0) / (y1 - y0), 720 / 385, delta=0.01)
        self.assertEqual((x0 + x1) // 2, 720)
        from PIL import Image
        for orig in (True, False):
            t = fc.tile(Image.new("RGB", (2880, 1800), (200, 0, 0)), 400, 214, "x", orig=orig)
            self.assertEqual(t.size, (400, 214))

    def test_start_yaw_from_the_trace(self):
        import math
        self.assertAlmostEqual(fc._yaw("8192"), math.pi / 2)    # 32768 per turn
        self.assertAlmostEqual(fc._yaw("-4871"), math.radians(-4871 * 360 / 32768), places=5)


if __name__ == "__main__":
    unittest.main()
