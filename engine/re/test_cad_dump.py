"""Tests for cad_dump.py on synthetic nav graphs (no game data needed)."""
import math
import unittest

import cad_dump as C

# two flat 10x10 quads side by side on y = 0 (x 0..10 and x 10..20), plus one at y = 100 above the first
VERTS = [
    (0, 0, 0), (0, 0, 10), (10, 0, 10), (10, 0, 0),        # node 0
    (10, 0, 0), (10, 0, 10), (20, 0, 10), (20, 0, 0),      # node 1
    (0, 100, 0), (0, 100, 10), (10, 100, 10), (10, 100, 0),  # node 2 (high)
]
NODES = [(0, (0, 1, 2, 3)), (0, (4, 5, 6, 7)), (0, (8, 9, 10, 11))]


def sample(flags1=0, edge_flags=0):
    nodes = [NODES[0], (flags1, NODES[1][1]), NODES[2]]
    edges = [((10, 0, 0), (10, 0, 10), 0, 1, edge_flags, 0)]
    routes = {(0, 0, 1): [1], (0, 1, 0): [0], (4, 0, 2): [1, 2]}
    return C.Cad(C.build_cad(VERTS, nodes, edges, jumps=[(5, 1, 5)], routes=routes, classes={0: 1, 1: 0}))


class Parse(unittest.TestCase):
    def test_counts_and_records(self):
        c = sample()
        self.assertEqual((c.nverts, c.nnodes, c.nedges, c.njumps), (12, 3, 1, 1))
        self.assertEqual(c.nodes[1]["corners"], [4, 5, 6, 7])
        self.assertEqual(c.edges[0]["a"], 0)
        self.assertAlmostEqual(c.edges[0]["length"], 10.0)
        self.assertEqual(c.jumps[0], (5.0, 1.0, 5.0))

    def test_centroid(self):
        self.assertEqual(sample().centroid(1), (15.0, 0.0, 5.0))

    def test_rejects_wrong_counts(self):
        d = bytearray(C.build_cad(VERTS, NODES))
        d[C.OFF_NCOUNT] = 200
        with self.assertRaises(ValueError):
            C.Cad(bytes(d))

    def test_layout_constants_are_tied_together(self):
        # the gaps between regions are what the program's tables imply
        self.assertEqual(C.OFF_NODES + C.MAX_NODES * C.NODE_SIZE, C.OFF_ECOUNT)
        self.assertEqual(C.OFF_EDGES + C.MAX_EDGES * C.EDGE_SIZE, C.OFF_UNREAD)
        self.assertEqual(C.OFF_JPTS + C.MAX_JPTS * C.JP_SIZE, C.ROUTE_TABLES[0])
        self.assertEqual(C.ROUTE_TABLES[4] + 0x10000, C.OFF_CLASS)
        self.assertEqual(C.OFF_POOL_LEN + 4, C.OFF_POOL)


class NodeAt(unittest.TestCase):
    def test_inside_each_quad(self):
        c = sample()
        self.assertEqual(c.node_at((5, 0, 5)), 0)
        self.assertEqual(c.node_at((15, 0, 5)), 1)

    def test_height_picks_closest_plane(self):
        c = sample()
        self.assertEqual(c.node_at((5, 95, 5)), 2)
        self.assertEqual(c.node_at((5, 10, 5)), 0)

    def test_too_far_vertically_strict_none_nonstrict_nearest(self):
        c = sample()
        p = (5, 50, 5)  # 50 above node 0, 50 below node 2: both beyond the 20 tolerance
        self.assertEqual(c.node_at(p, True), -1)
        self.assertIn(c.node_at(p, False), (0, 2))

    def test_outside_everything(self):
        c = sample()
        self.assertEqual(c.node_at((50, 0, 5), True), -1)
        self.assertEqual(c.node_at((50, 0, 5), False), 1)  # nearest unblocked boundary (x=20)

    def test_blocked_node_is_skipped(self):
        c = sample(flags1=C.NF_BLOCKED)
        self.assertEqual(c.node_at((15, 0, 5), True), -1)
        self.assertEqual(c.node_at((15, 0, 5), False), 0)

    def test_either_winding_works(self):
        d = C.build_cad([(0, 0, 0), (10, 0, 0), (10, 0, 10), (0, 0, 10)], [(0, (0, 1, 2, 3))])
        self.assertEqual(C.Cad(d).node_at((5, 0, 5)), 0)

    def test_plane_height_on_a_slope(self):
        # y = x
        v = [(0, 0, 0), (0, 0, 10), (10, 10, 10), (10, 10, 0)]
        c = C.Cad(C.build_cad(v, [(0, (0, 1, 2, 3))]))
        self.assertAlmostEqual(c.inside(0, (4, 4, 5)), 4.0)
        self.assertEqual(c.node_at((4, 4, 5)), 0)
        self.assertEqual(c.node_at((4, 40, 5), True), -1)

    def test_point_segment_distance(self):
        self.assertAlmostEqual(C.point_segment_distance((5, 3, 0), (0, 0, 0), (10, 0, 0)), 3.0)
        self.assertAlmostEqual(C.point_segment_distance((-3, 4, 0), (0, 0, 0), (10, 0, 0)), 5.0)
        self.assertAlmostEqual(C.point_segment_distance((1, 1, 1), (2, 2, 2), (2, 2, 2)), math.sqrt(3))


class Routes(unittest.TestCase):
    def test_route_lookup_and_missing(self):
        c = sample()
        self.assertEqual(c.route(0, 0, 1), [1])
        self.assertEqual(c.route(4, 0, 2), [1, 2])
        self.assertIsNone(c.route(0, 0, 2))
        self.assertEqual(c.pool_len(), 2 + 3 + 2)

    def test_node_class(self):
        c = sample()
        self.assertEqual(c.node_class(0), 1)
        self.assertIsNone(c.node_class(2))

    def test_edge_between_respects_one_way(self):
        c = sample(edge_flags=C.EF_ONEWAY)
        self.assertEqual(c.edge_between(0, 1), 0)
        self.assertEqual(c.edge_between(1, 0), -1)
        self.assertEqual(sample().edge_between(1, 0), 0)


class Check(unittest.TestCase):
    def test_clean(self):
        self.assertEqual(sample().check(), [])

    def test_bad_corner_index_reported(self):
        d = C.build_cad(VERTS, [(0, (0, 1, 2, 99))])
        self.assertTrue(any("corner" in p for p in C.Cad(d).check()))

    def test_bad_edge_node_reported(self):
        d = C.build_cad(VERTS, NODES, [((0, 0, 0), (1, 0, 0), 0, 9, 0, 0)])
        self.assertTrue(any("edge 0" in p for p in C.Cad(d).check()))


class Los(unittest.TestCase):
    def test_same_node_and_adjacent_nodes(self):
        c = sample()
        self.assertEqual(c.ground_ok((2, 0, 2), (8, 0, 8)), 1)
        self.assertEqual(c.ground_ok((2, 0, 5), (18, 0, 5)), 1)   # crosses the shared portal x = 10

    def test_line_that_misses_the_portal_or_leaves_the_graph(self):
        c = sample()
        self.assertEqual(c.ground_ok((2, 0, 5), (50, 0, 5)), 0)   # target off the graph
        self.assertEqual(c.ground_ok((2, 0, 5), (5, 95, 5)), 0)   # node 2 has no plain edge to node 0

    def test_blocked_or_closed_gates_and_flags(self):
        self.assertEqual(sample(flags1=C.NF_BLOCKED).ground_ok((2, 0, 5), (18, 0, 5)), 0)
        self.assertEqual(sample(edge_flags=C.EF_ONEWAY).ground_ok((2, 0, 5), (18, 0, 5)), 0)   # flags != 0: skipped
        self.assertEqual(sample(edge_flags=C.EF_DYNAMIC).ground_ok((2, 0, 5), (18, 0, 5)), 0)

    def test_segment_cross(self):
        self.assertTrue(C.segments_cross_xz((0, 0, 0), (0, 0, 10), (-5, 0, 5), (5, 0, 5)))
        self.assertFalse(C.segments_cross_xz((0, 0, 0), (0, 0, 10), (-5, 0, 15), (5, 0, 15)))
        self.assertTrue(C.segments_cross_xz((0, 0, 0), (0, 0, 10), (0, 0, 10), (5, 0, 10)))   # touching counts


REAL = r"../../../game_files/Assets/Data/com/st%s/comdata0.cad"


class RealFiles(unittest.TestCase):
    """Only runs where the game files exist: checks the reader against the shipped data."""

    def _cad(self, st):
        import os
        path = REAL % st
        if not os.path.exists(path):
            self.skipTest("game files not present")
        with open(path, "rb") as fh:
            return C.Cad(fh.read())

    def test_centroids_resolve_to_their_own_nodes(self):
        # validates the point-in-quad + plane-height logic on real arenas (stacked floors may differ)
        for st, min_hits in (("03", 76), ("04", 85), ("06", 122), ("07", 127)):
            c = self._cad(st)
            hits = sum(1 for n in range(c.nnodes) if c.node_at(c.centroid(n), True) == n)
            self.assertEqual(hits, min_hits, st)

    def test_invariants_hold_on_all_arenas(self):
        for st in ("02", "03", "04", "05", "06", "07", "08"):
            self.assertEqual(self._cad(st).check(), [], st)

    def test_stored_routes_end_where_asked(self):
        c = self._cad("05")
        self.assertEqual(c.route(0, 0, 10), [2, 5, 6, 3, 18, 7, 8, 9, 10])


if __name__ == "__main__":
    unittest.main()
