"""Reader and checker for the CPU's nav-graph files (`Data/com/stNN/comdata*.cad`), our own tool.

Read-only. Layout from the program's accessors (`FUN_140176f90`, `FUN_140176e90`, `FUN_1401771b0`,
`FUN_140177460`, `FUN_140178d00`, `FUN_140179420`, ...); see engine/docs/FORMATS-CPU.md section 5.
The file is a fixed 674,312-byte memory image that the program uses as is (no relocation), little-endian.
Never commit anything this prints from game data. Python standard library only.

Usage:
  python cad_dump.py <file.cad>                 summary + invariants
  python cad_dump.py <file.cad> --node-at X Y Z node containing a point (strict / non-strict)
  python cad_dump.py <file.cad> --route A B     the stored routes (all five tables) from node A to node B
"""
import math
import struct
import sys

# ---- layout (READ) -------------------------------------------------------------------------------
FILE_SIZE = 674312
OFF_VCOUNT, OFF_VERTS, MAX_VERTS = 0x0000, 0x0004, 1000
OFF_NCOUNT, OFF_NODES, NODE_SIZE, MAX_NODES = 0x2EE4, 0x2EE8, 0x54, 128
OFF_ECOUNT, OFF_EDGES, EDGE_SIZE, MAX_EDGES = 0x58E8, 0x58EC, 0x2C, 512
OFF_UNREAD = 0xB0EC            # 0xB0EC..0xB3BF: not referenced by any nav function (see doc)
OFF_JCOUNT, OFF_JPTS, JP_SIZE, MAX_JPTS = 0xB3C0, 0xB3C4, 16, 30
ROUTE_TABLES = (0xB5A4, 0x1B5A4, 0x2B5A4, 0x3B5A4, 0x4B5A4)   # T0..T4, 128 x 128 x i32 each
ROUTE_ROW = 128
OFF_CLASS, OFF_POOL_LEN, OFF_POOL = 0x5B5A4, 0x5B624, 0x5B628

# node flag bits (byte at node+0): 1 = tested after load for a missing floor, 2 = jump-zone node,
# 4 = blocked (set at run time), 8 = unused by the code seen (the nav code treats `& 0xC` as "blocked").
NF_TEST, NF_JUMPZONE, NF_BLOCKED = 1, 2, 4
# edge flag bits (u32 at edge+0x20): 1 = one-way (A -> B only), 2 = dynamic door tested each tick, 4 = closed.
EF_ONEWAY, EF_DYNAMIC, EF_CLOSED = 1, 2, 4

HEIGHT_TOL = 20.0   # DAT_1402f7fc4: node_at accepts a quad whose plane is within this of the point's y


class Cad:
    def __init__(self, data):
        if len(data) < OFF_POOL:
            raise ValueError("too short for a .cad (%d bytes)" % len(data))
        self.data = data
        self.nverts = struct.unpack_from("<I", data, OFF_VCOUNT)[0]
        self.nnodes = struct.unpack_from("<i", data, OFF_NCOUNT)[0]
        self.nedges = struct.unpack_from("<i", data, OFF_ECOUNT)[0]
        self.njumps = struct.unpack_from("<i", data, OFF_JCOUNT)[0]
        if not (0 <= self.nverts <= MAX_VERTS and 0 <= self.nnodes <= MAX_NODES and 0 <= self.nedges <= MAX_EDGES):
            raise ValueError("counts out of range: %d/%d/%d" % (self.nverts, self.nnodes, self.nedges))
        if not 0 <= self.njumps <= MAX_JPTS:
            raise ValueError("jump point count out of range: %d" % self.njumps)
        self.verts = [struct.unpack_from("<3f", data, OFF_VERTS + 12 * i) for i in range(self.nverts)]
        self.nodes = [self._node(i) for i in range(self.nnodes)]
        self.edges = [self._edge(i) for i in range(self.nedges)]
        self.jumps = [struct.unpack_from("<3f", data, OFF_JPTS + JP_SIZE * i) for i in range(self.njumps)]

    def _node(self, i):
        o = OFF_NODES + NODE_SIZE * i
        d = self.data
        return {
            "flags": d[o],
            "links": list(struct.unpack_from("<8i", d, o + 4)),     # jump-link ids, see doc
            "corners": list(struct.unpack_from("<4i", d, o + 0x24)),
            "packed": list(struct.unpack_from("<8I", d, o + 0x34)),  # not read by the nav code
        }

    def _edge(self, i):
        o = OFF_EDGES + EDGE_SIZE * i
        d = self.data
        p0 = struct.unpack_from("<3f", d, o)
        p1 = struct.unpack_from("<3f", d, o + 12)
        a, b, flags, length, gate = struct.unpack_from("<iiIfi", d, o + 0x18)
        return {"p0": p0, "p1": p1, "a": a, "b": b, "flags": flags, "length": length, "gate": gate}

    # ---- geometry --------------------------------------------------------------------------
    def corners(self, n):
        return [self.verts[c] for c in self.nodes[n]["corners"]]

    def centroid(self, n):
        cs = self.corners(n)
        return tuple(sum(c[k] for c in cs) * 0.25 for k in range(3))

    def blocked(self, n):
        return bool(self.nodes[n]["flags"] & 0xC)

    def inside(self, n, p):
        """FUN_1401771b0: XZ point-in-quad (all edge cross products of one sign) + plane height.

        Returns the plane height (y) at the point, or None when the point is outside or the node is blocked.
        """
        if self.blocked(n):
            return None
        v = self.corners(n)
        pos = neg = False
        for k in range(4):
            a, b = v[k], v[(k + 1) & 3]
            ex, ez = b[0] - a[0], b[2] - a[2]
            dx, dz = p[0] - a[0], p[2] - a[2]
            cr = ex * dz - dx * ez
            pos |= cr > 0.0
            neg |= cr < 0.0
        if pos and neg:
            return None
        return plane_height(v[0], v[1], v[2], p)

    def node_at(self, p, strict=True):
        """f31 (strict=True: FUN_140176e90) / f18 (strict=False: FUN_140176f90). -1 for none."""
        best, best_h = -1, 0.0
        for n in range(self.nnodes):
            h = self.inside(n, p)
            if h is None:
                continue
            if best == -1 or abs(h - p[1]) < abs(best_h - p[1]):
                best, best_h = n, h
        if best != -1 and abs(best_h - p[1]) < HEIGHT_TOL:
            return best
        if strict:
            return -1
        # non-strict fallback: nearest unblocked node by distance to its four boundary segments
        near, near_d = -1, 0.0
        for n in range(self.nnodes):
            if self.blocked(n):
                continue
            cs = self.corners(n)
            for k in range(4):
                dd = point_segment_distance(p, cs[k], cs[(k + 1) & 3])
                if near == -1 or dd < near_d:
                    near, near_d = n, dd
        return near

    # ---- routes ----------------------------------------------------------------------------
    def route(self, table, a, b):
        """Stored route a -> b in table T`table` (0..4): the node list AFTER a up to and including b, or None."""
        off = struct.unpack_from("<i", self.data, ROUTE_TABLES[table] + (a * ROUTE_ROW + b) * 4)[0]
        if off == -1:
            return None
        o = OFF_POOL + off
        out = []
        while len(out) < self.nnodes + 1 and o < len(self.data):
            x = self.data[o]
            if x == 0xFF:
                return None
            out.append(x)
            if x == b:
                return out
            o += 1
        return None

    def node_class(self, n):
        """Hub node of node n (byte at +0x5B5A4+n); None for 0xFF."""
        c = self.data[OFF_CLASS + n]
        return None if c == 0xFF else c

    def pool_len(self):
        return struct.unpack_from("<I", self.data, OFF_POOL_LEN)[0]

    def edge_between(self, a, b):
        """Index of the edge joining a -> b (b -> a only when the edge is not one-way), else -1."""
        for i, e in enumerate(self.edges):
            if (e["a"] == a and e["b"] == b) or (e["a"] == b and e["b"] == a and not e["flags"] & EF_ONEWAY):
                return i
        return -1

    def ground_ok(self, a, b):
        """f24 (FUN_140179f80): walkable straight line between points a and b along plain portals. 1 / 0."""
        na, nb = self.node_at(a, True), self.node_at(b, True)
        if na == -1 or nb == -1 or self.blocked(na) or self.blocked(nb):
            return 0
        if na == nb:
            return 1
        cur, last = na, [-1, -1, -1]
        for _ in range(16):
            found = -1
            for i, e in enumerate(self.edges):
                if i in last:
                    continue
                fl, gate = e["flags"], e["gate"]
                if (fl & 2 or gate != 0) and ((gate != 0 and not self.nodes[gate]["flags"] & 4) or (fl & 6) == 2):
                    continue
                if fl != 0 or cur not in (e["a"], e["b"]):
                    continue
                if not segments_cross_xz(e["p0"], e["p1"], a, b):
                    continue
                found = i
                break
            if found == -1:
                return 0
            e = self.edges[found]
            nxt = e["b"] if e["a"] == cur else e["a"]
            last = [last[1], last[2], found]
            if nxt == -1 or self.nodes[nxt]["flags"] & 0xE:
                return 0
            cur = nxt
            if cur == nb:
                return 1
        return 1

    # ---- invariants ------------------------------------------------------------------------
    def check(self):
        """Returns a list of problems (empty = consistent). Route adjacency is reported as counts, not errors:
        steps that are not edges are jump-zone hops (see the doc)."""
        bad = []
        if len(self.data) != FILE_SIZE:
            bad.append("size %d != %d" % (len(self.data), FILE_SIZE))
        for i, n in enumerate(self.nodes):
            for c in n["corners"]:
                if not 0 <= c < self.nverts:
                    bad.append("node %d corner %d out of range" % (i, c))
        for i, e in enumerate(self.edges):
            for k in ("a", "b"):
                if not 0 <= e[k] < self.nnodes:
                    bad.append("edge %d node %s=%d out of range" % (i, k, e[k]))
            if e["gate"] and not 0 <= e["gate"] < self.nnodes:
                bad.append("edge %d gate %d out of range" % (i, e["gate"]))
        for t in range(5):
            for a in range(self.nnodes):
                for b in range(self.nnodes):
                    r = self.route(t, a, b)
                    if r is not None and any(not 0 <= x < self.nnodes for x in r):
                        bad.append("T%d %d->%d leaves the graph" % (t, a, b))
        return bad


def plane_height(v0, v1, v2, p):
    """FUN_140177460: y of the plane through v0, v1, v2 at the point's (x, z). Zero normal.y gives +-inf/nan."""
    ax, ay, az = (v1[k] - v0[k] for k in range(3))
    bx, by, bz = (v2[k] - v0[k] for k in range(3))
    nx = ay * bz - az * by
    ny = az * bx - ax * bz
    nz = ax * by - ay * bx
    if ny == 0.0:
        return math.inf
    return v0[1] - ((p[2] - v0[2]) * nz + (p[0] - v0[0]) * nx) / ny


def segments_cross_xz(p0, p1, a, b):
    """FUN_140196420: do segment p0-p1 and segment a-b intersect in the XZ plane (inclusive)?"""
    def cr(o, u, v):
        return (u[0] - o[0]) * (v[2] - o[2]) - (u[2] - o[2]) * (v[0] - o[0])
    return cr(p0, p1, a) * cr(p0, p1, b) <= 0.0 and cr(a, b, p0) * cr(a, b, p1) <= 0.0


def point_segment_distance(p, a, b):
    """FUN_140193bc0: 3-D distance from point p to segment a-b."""
    ab = [b[k] - a[k] for k in range(3)]
    ap = [p[k] - a[k] for k in range(3)]
    l2 = sum(x * x for x in ab)
    t = 0.0 if l2 == 0.0 else max(0.0, min(1.0, sum(ap[k] * ab[k] for k in range(3)) / l2))
    c = [a[k] + ab[k] * t for k in range(3)]
    return math.sqrt(sum((p[k] - c[k]) ** 2 for k in range(3)))


def build_cad(verts, nodes, edges=(), jumps=(), routes=None, classes=None):
    """Synthetic writer used by the tests. nodes = [(flags, (c0,c1,c2,c3))], edges = [(p0, p1, a, b, flags, gate)],
    routes = {(table, a, b): [node ids]} (the pool is appended in order)."""
    d = bytearray(FILE_SIZE)
    struct.pack_into("<I", d, OFF_VCOUNT, len(verts))
    for i, v in enumerate(verts):
        struct.pack_into("<3f", d, OFF_VERTS + 12 * i, *v)
    struct.pack_into("<i", d, OFF_NCOUNT, len(nodes))
    for i, (fl, cs) in enumerate(nodes):
        o = OFF_NODES + NODE_SIZE * i
        d[o] = fl
        struct.pack_into("<8i", d, o + 4, *([-1] * 8))
        struct.pack_into("<4i", d, o + 0x24, *cs)
    struct.pack_into("<i", d, OFF_ECOUNT, len(edges))
    for i, (p0, p1, a, b, fl, gate) in enumerate(edges):
        o = OFF_EDGES + EDGE_SIZE * i
        struct.pack_into("<3f", d, o, *p0)
        struct.pack_into("<3f", d, o + 12, *p1)
        length = math.dist(p0, p1)
        struct.pack_into("<iiIfi", d, o + 0x18, a, b, fl, length, gate)
    struct.pack_into("<i", d, OFF_JCOUNT, len(jumps))
    for i, j in enumerate(jumps):
        struct.pack_into("<4f", d, OFF_JPTS + JP_SIZE * i, j[0], j[1], j[2], 0.0)
    for t in ROUTE_TABLES:
        d[t:t + ROUTE_ROW * ROUTE_ROW * 4] = b"\xff" * (ROUTE_ROW * ROUTE_ROW * 4)
    pool = bytearray()
    for (t, a, b), nodes_ in (routes or {}).items():
        struct.pack_into("<i", d, ROUTE_TABLES[t] + (a * ROUTE_ROW + b) * 4, len(pool))
        pool += bytes(nodes_) + b"\xff"
    d[OFF_POOL:OFF_POOL + len(pool)] = pool
    struct.pack_into("<I", d, OFF_POOL_LEN, len(pool))
    for n in range(MAX_NODES):
        d[OFF_CLASS + n] = 0xFF
    for n, c in (classes or {}).items():
        d[OFF_CLASS + n] = c
    return bytes(d)


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cad = Cad(open(argv[0], "rb").read())
    if "--node-at" in argv:
        i = argv.index("--node-at")
        p = tuple(float(x) for x in argv[i + 1:i + 4])
        print("strict", cad.node_at(p, True), "non-strict", cad.node_at(p, False))
        return 0
    if "--route" in argv:
        i = argv.index("--route")
        a, b = int(argv[i + 1]), int(argv[i + 2])
        for t in range(5):
            print("T%d" % t, cad.route(t, a, b))
        return 0
    flags = {}
    for n in cad.nodes:
        flags[n["flags"]] = flags.get(n["flags"], 0) + 1
    print("vertices %d, nodes %d (flags %s), edges %d, jump points %d, route pool %d bytes" %
          (cad.nverts, cad.nnodes, flags, cad.nedges, cad.njumps, cad.pool_len()))
    problems = cad.check()
    print("problems:", problems if problems else "none")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
