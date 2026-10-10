import unittest

import mr_aurahalo as h


class PlanParsing(unittest.TestCase):
    def test_labels_names_ticks(self):
        plan = h.parse_plan("base:,wisp:wisp@30,both:wisp+composite")
        self.assertEqual(plan, [("base", (), 40), ("wisp", ("wisp",), 30), ("both", ("wisp", "composite"), 40)])

    def test_lift_and_pseudo_names(self):
        plan = h.parse_plan("on:-draw@45,obj:obj17180,fx:spawn600")
        self.assertEqual([p[1] for p in plan], [("-draw",), ("obj17180",), ("spawn600",)])

    def test_unknown_name_is_refused(self):
        with self.assertRaises(ValueError):
            h.parse_plan("x:nothing")


class SkipperSpecs(unittest.TestCase):
    def test_specs_cover_every_skip_and_the_spawn_filter(self):
        sk = h.Skipper(h.SKIPS)
        specs = sk.specs()
        self.assertEqual(len(specs), len(h.SKIPS) + 1)
        self.assertTrue(any("skip_spawn600:ecx,edx" in s for s in specs))


if __name__ == "__main__":
    unittest.main()
