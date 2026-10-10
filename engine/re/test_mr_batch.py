"""Tests for the batch runner's pure parts: ledger, family plans, requirements, resume, summary."""
import csv
import json
import os
import tempfile
import unittest

import mr_batch as mb
import move_recorder as mr

LEDGER_TEXT = """| # | id | skill | family | decks |
|---:|---:|---|---|---:|
| 1 | 80 | Blaster | att/shell | 187 (2/184/1) | x |
| 2 | 218 | Giant Slayer | era/shell | 157 | x |
| 3 | 301 | Delay Bomb | att/mine | 3 | x |
| 4 | 0 | Aura Particle | spe/self | 0 | x |
not a row | 9 | x
"""


class Ledger(unittest.TestCase):
    def test_rows_are_read_in_rank_order(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "l.md")
            open(p, "w", encoding="utf-8").write(LEDGER_TEXT)
            rows = mb.parse_ledger(p)
        self.assertEqual(rows, [(1, 80, "Blaster", "att/shell"), (2, 218, "Giant Slayer", "era/shell"),
                                (3, 301, "Delay Bomb", "att/mine"), (4, 0, "Aura Particle", "spe/self")])

    def test_the_real_ledger_has_341_rows_and_the_aura_particle_last(self):
        if not os.path.exists(mb.LEDGER):
            self.skipTest("ledger not present")
        rows = mb.parse_ledger()
        self.assertEqual(len(rows), 341)
        self.assertEqual(rows[-1][1], 0)
        self.assertEqual(len({r[1] for r in rows}), 341)


class Plans(unittest.TestCase):
    def test_melee_is_close_and_locked(self):
        d, lock, tail, opp, flag = mb.family_plan("att/punch")
        self.assertEqual((d, lock, opp), (35, True, ""))

    def test_defence_gets_an_attacker_and_a_flag(self):
        d, lock, tail, opp, flag = mb.family_plan("def/guard")
        self.assertEqual(opp, "attack")
        self.assertIn("unverified", flag)

    def test_self_skills_do_not_lock(self):
        self.assertFalse(mb.family_plan("sta/self")[1])
        self.assertFalse(mb.family_plan("env/env")[1])

    def test_every_family_of_the_ledger_has_a_plan(self):
        if not os.path.exists(mb.LEDGER):
            self.skipTest("ledger not present")
        for _r, _i, _n, fam in mb.parse_ledger():
            d, lock, tail, opp, flag = mb.family_plan(fam)
            self.assertGreater(d, 0)
            self.assertGreater(tail, 0)

    def test_requirement_level_and_health(self):
        self.assertEqual(mb.requirement_setup({"requirement": 8, "requirement_amount": 2, "cost": 2})[:2], (2, 20))
        self.assertEqual(mb.requirement_setup({"requirement": 9, "requirement_amount": 20})[0], 20)
        self.assertEqual(mb.requirement_setup({"requirement": 1, "requirement_amount": 7})[1], 7)
        self.assertEqual(mb.requirement_setup({"requirement": 2, "requirement_amount": 30})[1], 30)
        self.assertTrue(mb.requirement_setup({"requirement": 5, "requirement_amount": 3})[2])

    def test_cost_above_level_is_flagged(self):
        self.assertTrue(mb.requirement_setup({"requirement": 8, "requirement_amount": 2, "cost": 5})[2])

    def test_health_cost_keeps_the_caster_alive(self):
        self.assertGreater(mb.requirement_setup({"health_cost": 25})[1], 25)


class Rows(unittest.TestCase):
    SKILLS = [{"id": 80, "record": 52, "name": "Blaster", "capsule": "attack", "requirement": 0, "cost": 4},
              {"id": 0, "record": -1, "name": "Aura Particle"}]
    LEDGER = [(1, 80, "Blaster", "att/shell"), (2, 0, "Aura Particle", "spe/self"), (3, 999, "Ghost", "def/guard")]

    def test_rows_scripts_and_defence_attacker(self):
        rows = mb.build_rows(self.LEDGER, self.SKILLS)
        self.assertEqual([r["rank"] for r in rows], [1, 2, 3])
        sc = mb.script_from_row(rows[0])
        self.assertEqual((sc.skill, sc.distance, sc.lock), (80, 60.0, True))
        d = mb.script_from_row(rows[2])
        self.assertEqual(d.opponent_skill, mb.ATTACKER_ID)
        self.assertTrue(d.opponent_events)
        self.assertIn("Aura Particle", rows[1]["flag"])

    def test_csv_round_trip(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "a.csv")
            mb.write_all_skills(p, self.LEDGER, self.SKILLS)
            got = mb.scripts_from_csv(p, self.SKILLS)
        self.assertEqual([sc.skill for sc, _r in got], [80, 0, 999])
        self.assertEqual(got[2][1]["family"], "def/guard")

    def test_bare_list_falls_back_to_the_generic_script(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "b.csv")
            open(p, "w").write("id,name\n80,Blaster\n")
            got = mb.scripts_from_csv(p, self.SKILLS)
        self.assertEqual(got[0][0].skill, 80)


class ResumeAndSummary(unittest.TestCase):
    def make(self, d, sc, **meta):
        stem = mr.output_stem(sc.skill, sc.name)
        open(os.path.join(d, stem + "_trace.csv"), "w").write("x\n")
        m = {"skill": sc.skill, "name": sc.name, "finished": True, "notes": []}
        m.update(meta)
        json.dump(m, open(os.path.join(d, stem + ".json"), "w"))

    def test_finished_takes_are_skipped_and_half_takes_are_not(self):
        sc = mr.parse_script({"skill": 80, "name": "Blaster", "timeline": [{"t": 0, "do": "press", "buttons": "Y"}]})
        with tempfile.TemporaryDirectory() as d:
            self.assertFalse(mb.take_done(d, sc))
            self.make(d, sc)
            self.assertTrue(mb.take_done(d, sc))
            self.make(d, sc, finished=False)
            self.assertFalse(mb.take_done(d, sc))
            self.make(d, sc)
            os.remove(os.path.join(d, "080_Blaster_trace.csv"))
            self.assertFalse(mb.take_done(d, sc))

    def test_status(self):
        self.assertEqual(mb.status_of({"crashed": True}), "crashed")
        self.assertEqual(mb.status_of({"pressed": False}), "refused")
        self.assertEqual(mb.status_of({"pressed": True, "fired": False}), "no-fire")
        self.assertEqual(mb.status_of({"pressed": True, "fired": True, "index_ok": False}), "WRONG-SKILL")
        self.assertEqual(mb.status_of({"pressed": True, "fired": True, "index_ok": True}), "ok")

    def test_summary_orders_by_rank_and_counts(self):
        a = mr.parse_script({"skill": 80, "name": "Blaster", "timeline": [{"t": 0, "do": "press", "buttons": "Y"}]})
        b = mr.parse_script({"skill": 65, "name": "Laser", "timeline": [{"t": 0, "do": "press", "buttons": "Y"}]})
        with tempfile.TemporaryDirectory() as d:
            self.make(d, a, pressed=True, fired=True, index_ok=True, press_tick=39, fire_tick=59)
            self.make(d, b, pressed=False)
            counts = mb.write_summary(d, {80: {"rank": "2", "family": "att/shell"}, 65: {"rank": "1", "family": "att/arc"}})
            rows = list(csv.DictReader(open(os.path.join(d, "summary.csv"))))
        self.assertEqual(counts, {"ok": 1, "refused": 1})
        self.assertEqual([r["name"] for r in rows], ["Laser", "Blaster"])


if __name__ == "__main__":
    unittest.main()
