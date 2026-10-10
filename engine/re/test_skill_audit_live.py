"""Tests for skill_audit_live.py and mr_trace_summary.py (synthetic traces; no game needed)."""
import csv
import json
import os
import tempfile
import unittest

import mr_trace_summary as ts
import skill_audit_live as live
import skill_audit_rules as R


def rec(**kw):
    raw = [0] * 70
    r = {"id": 5, "name": "Test", "capsule": "attack", "raw": raw,
         "action": {"ground": {"fire_tick": 13, "cast_sound": 242, "effect": 42, "anim_id": 1, "anim_name": "ATK_LASER",
                               "charge_cycle": 0}, "air": None}}
    raw[22] = 3                     # cost 3
    r.update(kw)
    return r


def row(**kw):
    d = {"rank": "1", "id": "5", "status": "ok", "press_tick": "39", "fire_tick": "52", "cast_sounds": "0.242 3.10",
         "effects": "0.42@39 1.42@70", "p0_aura_before": "12.00", "p0_aura_at_fire": "9.00", "p0_level_before": "12",
         "p0_level_after": "12", "p0_health_before": "20", "p0_health_at_fire": "20", "notes": ""}
    d.update(kw)
    return d


def by_key(cells):
    return {c.key: c for c in cells}


class LiveCells(unittest.TestCase):
    def test_all_match(self):
        c = by_key(live.live_cells(rec(), row()))
        for k in ("fire_tick", "cast_sound", "cast_effect", "aura_cost", "level", "health"):
            self.assertEqual(c[k].st, R.OK, k)

    def test_wrong_fire_tick_is_a_cross(self):
        c = by_key(live.live_cells(rec(), row(fire_tick="60")))
        self.assertEqual(c["fire_tick"].st, R.BAD)
        self.assertEqual((c["fire_tick"].prog, c["fire_tick"].ours), (13, 21))

    def test_clip_zero_row_fires_in_the_press(self):
        r = rec()
        r["action"]["ground"].update({"anim_id": 0, "anim_name": "WAIT0", "fire_tick": 0})
        self.assertEqual(by_key(live.live_cells(r, row(fire_tick="39")))["fire_tick"].st, R.OK)

    def test_wrong_cost_and_cast_sound(self):
        c = by_key(live.live_cells(rec(), row(p0_aura_at_fire="8.00", cast_sounds="0.99")))
        self.assertEqual(c["aura_cost"].st, R.BAD)
        self.assertEqual(c["cast_sound"].st, R.BAD)

    def test_level_penalty_pulls_aura_down(self):
        r = rec()
        r["raw"][23] = 4                 # Level -= 4 -> Level 8, Aura clamped to 8
        c = by_key(live.live_cells(r, row(p0_aura_at_fire="8.00", p0_level_after="8")))
        self.assertEqual((c["aura_cost"].st, c["level"].st), (R.OK, R.OK))

    def test_health_cost_and_half(self):
        r = rec()
        r["raw"][25] = 3
        self.assertEqual(by_key(live.live_cells(r, row(p0_health_at_fire="17")))["health"].st, R.OK)
        r["raw"][25] = -1
        self.assertEqual(by_key(live.live_cells(r, row(p0_health_at_fire="10")))["health"].st, R.OK)

    def test_retain_pays_twice(self):
        r = rec()
        r["raw"][41], r["raw"][26] = 1, 1
        self.assertEqual(by_key(live.live_cells(r, row(p0_aura_at_fire="6.00")))["aura_cost"].st, R.OK)

    def test_own_aura_effect_is_a_hole_not_a_finding(self):
        r = rec(capsule="status")
        r["raw"][51] = 5
        r["raw"][63] = 4
        self.assertEqual(by_key(live.live_cells(r, row()))["aura_cost"].st, R.GAP)

    def test_refused_press_is_a_hole(self):
        cells = live.live_cells(rec(), row(status="refused", press_tick=""))
        self.assertEqual([c.key for c in cells], ["castable"])
        self.assertEqual(cells[0].st, R.GAP)

    def test_wrong_skill_is_a_cross(self):
        cells = live.live_cells(rec(), row(status="WRONG-SKILL", fire_index="7"))
        self.assertEqual(by_key(cells)["skill_index"].st, R.BAD)

    def test_charge_rows_are_not_compared(self):
        r = rec()
        r["action"]["ground"]["charge_cycle"] = 10
        self.assertEqual(by_key(live.live_cells(r, row()))["fire_tick"].st, R.UNK)

    def test_aura_particle(self):
        ap = {"id": 0, "name": "Aura Particle", "synthetic": True, "raw": [0] * 70, "action": {"ground": None, "air": None},
              "capsule": "special"}
        cells = by_key(live.live_cells(ap, row(id="0", fire_tick="39", p0_level_before="3", p0_level_after="4")))
        self.assertEqual((cells["fire_tick"].st, cells["level"].st), (R.OK, R.OK))
        cells = by_key(live.live_cells(ap, row(id="0", fire_tick="39", p0_level_before="3", p0_level_after="3")))
        self.assertEqual(cells["level"].st, R.BAD)


class Report(unittest.TestCase):
    def test_section_counts_and_lists_crosses(self):
        doc = {"skills": [rec(id=5, name="Test")]}
        doc["skills"][0]["record"] = 4
        results = live.audit_live([row(fire_tick="70")], doc)
        md = live.live_section(results=results)
        self.assertIn("`live.fire_tick`", md)
        self.assertIn("Takes: **1**", md)
        t = live.tally(results)
        self.assertEqual(t[R.BAD], 1)


class TraceSummary(unittest.TestCase):
    COLS = ts_cols = None

    def trace(self, n=100):
        import move_recorder as mr
        rows = []
        for t in range(n):
            ev = ""
            if t == 39:
                ev = "press:1.5|sound:0.242|effect:0.42|sound:3.10"
            if t == 52:
                ev = "fire:1.5|level:1.0"
            r = {c: "0" for c in mr.TRACE_COLUMNS}
            r.update({"rec_tick": str(t), "events": ev, "p0_state": "10" if 39 <= t < 80 else "0", "p0_aura": "12.00" if t < 52 else "9.00",
                      "p0_level": "12", "p0_health": "20", "p1_health": "40" if t < 60 else "36", "p1_state": "0", "p1_aura": "12.00", "p1_level": "12"})
            rows.append(r)
        return rows

    def test_summary_values(self):
        meta = {"skill": 5, "name": "Test", "pressed": True, "fired": True, "press_tick": 39, "fire_tick": 52,
                "fire_index": 5, "index_ok": True, "game_index": 5, "notes": []}
        s = ts.summarize_take(meta, self.trace(), {"rank": "3", "family": "att/shell"})
        self.assertEqual((s["status"], s["press_to_fire"], s["cast_state"], s["cast_state_ticks"]), ("ok", 13, "10", 41))
        self.assertEqual((s["p0_aura_before"], s["p0_aura_at_fire"]), ("12.00", "9.00"))
        self.assertEqual((s["hit_tick"], s["hit_damage"]), (60, 4))
        self.assertEqual(s["state_seq"], "0>10>0")
        self.assertEqual(s["cast_sounds"], "0.242 3.10")
        self.assertIn("0.42@39", s["effects"])

    def test_write_summary_reads_a_takes_dir(self):
        import move_recorder as mr
        meta = {"skill": 5, "name": "Test", "finished": True, "pressed": True, "fired": True, "press_tick": 39,
                "fire_tick": 52, "notes": []}
        with tempfile.TemporaryDirectory() as d:
            json.dump(meta, open(os.path.join(d, "005_Test.json"), "w"))
            with open(os.path.join(d, "005_Test_trace.csv"), "w", newline="") as f:
                w = csv.DictWriter(f, fieldnames=mr.TRACE_COLUMNS)
                w.writeheader()
                w.writerows(self.trace())
            out = os.path.join(d, "s.csv")
            self.assertEqual(ts.write_summary(d, out, {5: {"rank": "9", "family": "att/arc"}}), 1)
            got = list(csv.DictReader(open(out)))
        self.assertEqual((got[0]["rank"], got[0]["family"], got[0]["press_to_fire"]), ("9", "att/arc", "13"))

    def test_crashed_take_has_no_trace_values(self):
        s = ts.summarize_take({"skill": 5, "name": "T", "crashed": True, "notes": ["x"]}, [], {})
        self.assertEqual(s["status"], "crashed")
        self.assertEqual(s["press_to_fire"], "")



class OwnerSplit(unittest.TestCase):
    def test_two_pointers_split_caster_and_opponent(self):
        import move_recorder as mr
        base = 1000
        rows = [{"rec_tick": "45", "events": "press:%d.53" % (base + 0x60000)},
                {"rec_tick": "65", "events": "fire:%d.53" % (base + 0x60000)},
                {"rec_tick": "67", "events": "fire:%d.131" % base}]
        for r in rows:
            r["p1_health"] = "40"
        s = mr.take_summary(rows, 131, opponent=True)
        self.assertEqual((s["pressed"], s["fired"], s["fire_tick"], s["index_ok"]), (False, True, 67, True))
        self.assertEqual((s["opp_press_tick"], s["opp_fire_tick"]), (45, 65))

    def test_single_pointer_with_an_opponent_uses_the_index(self):
        import move_recorder as mr
        rows = [{"rec_tick": "45", "events": "press:5.53", "p1_health": "40"}]
        s = mr.take_summary(rows, 131, opponent=True)
        self.assertFalse(s["pressed"])
        self.assertEqual(s["opp_press_tick"], 45)

    def test_defence_without_a_press_is_not_refused(self):
        import mr_batch as mb
        self.assertEqual(mb.status_of({"pressed": False, "fired": True, "index_ok": True}), "ok")

    def test_reactive_cells(self):
        rec_ = rec(capsule="defence")
        r = row(press_tick="", fire_tick="67", opp_fire_tick="65", sounds="0.331 x1 0.509 x1", cast_sounds="")
        c = by_key(live.live_cells(rec_, r))
        self.assertEqual(c["fire_tick"].st, R.UNK)
        self.assertIn("2 ticks", c["fire_tick"].ours)


if __name__ == "__main__":
    unittest.main()
