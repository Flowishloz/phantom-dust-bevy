"""Tests for mr_auralog's pure part (the per-call summary)."""
import unittest

import mr_auralog as al


def row(tick, name, args=()):
    return {"tick": tick, "name": name, "args": list(args)}


class Summary(unittest.TestCase):
    def test_calls_per_tick_and_distinct_values(self):
        rows = []
        for t in range(10):
            rows += [row(t, "aura_update"), row(t, "aura_draw"), row(t, "aura_draw"),
                     row(t, "composite", [254, "0.5", "0.5"]), row(t, "weights", [1, "0.2"])]
        s = al.summarize(rows, 10)
        self.assertEqual(s["per_tick"]["aura_update"], 1.0)
        self.assertEqual(s["per_tick"]["aura_draw"], 2.0)
        self.assertEqual(s["composite_distinct_args"], [(254, "0.5", "0.5")])
        self.assertEqual(s["weights_args"], [("0.2",)])

    def test_hooks_cover_the_functions_the_coordinator_named(self):
        text = " ".join(al.HOOKS)
        for addr in ("0x14019b920", "0x140289140", "0x1402864e0", "0x140292d40", "0x140287460"):
            self.assertIn(addr, text)


if __name__ == "__main__":
    unittest.main()
