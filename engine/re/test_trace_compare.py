import io
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import trace_compare as tc  # noqa: E402
import trace_synth as ts  # noqa: E402


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = tempfile.mkdtemp()
        cls.orig = os.path.join(cls.dir, "orig.csv")
        cls.ours = os.path.join(cls.dir, "ours.csv")
        ts.write_csv(cls.orig, ts.generate("orig"), "orig")
        ts.write_csv(cls.ours, ts.generate("ours"), "ours")

    def run_cmp(self, o, t, sid=None):
        class A:
            scenario, step, hz, run_speed = sid, 5, 30.0, 50.0
        text, totals = tc.report(tc.Trace(o), tc.Trace(t), A, dict(tc.TOL))
        return text, totals

    def metrics(self, o, t, sid):
        ro, rt = tc.Trace(o).scen[sid], tc.Trace(t).scen[sid]
        res, _ = tc.compare_scenario(sid, ro, rt, 30.0, 50.0, dict(tc.TOL))
        return {r.name: r for r in res}


class Tests(Base):
    def test_identical_passes(self):
        _, totals = self.run_cmp(self.orig, self.orig)
        self.assertEqual(totals["FAIL"], 0)
        self.assertGreater(totals["PASS"], 30)

    def test_header_and_blank_values(self):
        p = os.path.join(self.dir, "h.csv")
        with open(p, "w") as f:
            f.write("# build: abc123\n# character: Hero\ntick,pos_x,pos_z,facing\n0,0,0,\n1,0,1,nan\n")
        t = tc.Trace(p)
        self.assertEqual(t.meta["build"], "abc123")
        self.assertIsNone(t.scen["S00"][0]["facing"])

    def test_detects_direction_snap(self):
        m = self.metrics(self.orig, self.ours, "S04_reversal_180")
        self.assertEqual(m["max_turn_rate"].verdict, "FAIL")
        self.assertEqual(m["facing_settle"].verdict, "FAIL")
        self.assertEqual(m["first_divergence"].verdict, "FAIL")

    def test_detects_strafe_flips(self):
        m = self.metrics(self.orig, self.ours, "S08_lock_pullback_wobble")
        self.assertEqual(m["strafe_flips"].o, 0)
        self.assertGreaterEqual(m["strafe_flips"].t, 5)
        self.assertEqual(m["strafe_flips"].verdict, "FAIL")
        self.assertEqual(m["clip_switches"].verdict, "FAIL")

    def test_detects_stepped_gait(self):
        m = self.metrics(self.orig, self.ours, "S02_steady_075")
        self.assertEqual(m["steady_speed"].verdict, "FAIL")
        m = self.metrics(self.orig, self.ours, "S09_walk_to_run_ramp")
        self.assertEqual(m["speed_max_err"].verdict, "FAIL")

    def test_detects_jump_and_dodge_differences(self):
        m = self.metrics(self.orig, self.ours, "S10_jump")
        self.assertEqual(m["takeoff_delay"].verdict, "FAIL")
        self.assertEqual(m["busy_ticks"].verdict, "FAIL")
        m = self.metrics(self.orig, self.ours, "S12_tumble")
        self.assertEqual(m["dodge_distance"].verdict, "FAIL")
        self.assertEqual(m["busy_ticks"].verdict, "FAIL")

    def test_missing_columns_skip(self):
        cols = [c for c in ts.COLUMNS if c not in ("facing", "clip_id", "lock_band", "grounded", "state")]
        p = os.path.join(self.dir, "lean.csv")
        ts.write_csv(p, ts.generate("orig"), "orig", columns=cols)
        text, totals = self.run_cmp(p, self.ours)
        self.assertIn("lacks columns", text)
        m = self.metrics(p, self.ours, "S04_reversal_180")
        self.assertEqual(m["max_turn_rate"].verdict, "SKIP")
        self.assertEqual(m["clip_switches"].verdict, "SKIP")

    def test_cli_writes_report(self):
        out = os.path.join(self.dir, "r.txt")
        with redirect_stdout(io.StringIO()):
            rc = tc.main([self.orig, self.ours, "--out", out, "--strict"])
        self.assertEqual(rc, 1)
        self.assertIn("SUMMARY", open(out).read())


if __name__ == "__main__":
    unittest.main()
