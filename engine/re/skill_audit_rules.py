"""Program-side rules and the per-skill comparison of `skill_audit.py`.

Two halves meet here:

* PROGRAM side: for each skill, every value that decides how it plays, derived from
  the game's own tables (`cache/skills.json`: skill record s0-s73, action-table row,
  hit-reaction rows, `shotpoints.json`, the character hand path) through the rules read
  from the program (`PDUWPMain`, image base 0x140000000; docs/PROGRAM-RE.md, with the
  address of each read next to the rule below, and re-read from
  `%USERPROFILE%/.pdtools/re-out/decomp_all.c` where the doc was not enough).
* OURS side: what `pd_game` actually did for that skill id, measured by the Rust probe
  `crates/pd_game/src/combat/skill_audit.rs` (it drives the real `Combat::cast` /
  `step`, the real `moves::*` flights, `pick_reaction`, `sfx::skill_sounds`...).

Every comparison is a `Cell` with a status:

    OK   (checkmark)  match: the program value is READ (address) or MEASURED (footage)
                      and ours equals it;
    BAD  (cross)      mismatch with a READ / MEASURED program value;
    UNK  (?)          UNCONFIRMED: the program value is not settled (the rule table is
                      unread, the sign convention needs footage, ...). Standing rule:
                      no guessed values, so these are never counted as match or mismatch;
    TAKE (T)          needs a recorded take: not settled from the program, but the move
                      recorder can measure it on the original (press->fire ticks, per-tick
                      positions and states, sounds, Level / Health / Aura);
    GAP  (...)        the tool could not probe it (setup the arena cannot make, a family
                      with no probe yet): a hole in the stick, not a finding.

`skill_audit.py` collects the cells, ranks the skills and writes the ledger.
"""

import math
import re

OK, BAD, UNK, GAP = "\u2705", "\u274c", "?", "\u2026"
# TAKE: the program value is not settled, but the move recorder (engine/re/move_recorder.py, MOVE-RECORDER.md)
# can measure it live on the original: "needs take", not unconfirmed for ever.
TAKE = "T"
SEVERITY = {OK: 0, GAP: 1, UNK: 2, TAKE: 2, BAD: 3}

SIM = 2  # sim ticks (1/60 s) per program tick (1/30 s)
U = 1.0 / 10.8  # units per program tick per table speed point (0x1402f7cd8)
DEG_PER_POINT = 0.1  # degrees per program tick per homing / turn point (0x1402f7c84 x 0x1402f7cdc)
APORA = 60000  # Aura ticks per point in our sim (combat::AURA_TICKS_PER_POINT)

MOTION_CLASS = {
    0: "shell", 1: "parabola", 2: "fall", 3: "short3", 4: "instant", 5: "crawl", 6: "arc",
    7: "rain_column", 8: "mine", 9: "around_self", 10: "tornado", 11: "object", 12: "sword",
    13: "punch", 14: "pursuit", 15: "hole", 16: "hole", 17: "ricochet", 18: "none", 19: "none",
    20: "rain_cloud",
}
S43_CLASS = {1: "erase_field", 2: "guard", 3: "movement", 4: "status_self", 5: "environment", 6: "copy"}
CAPSULE_COLOUR = {"attack": 1, "defence": 2, "erase": 3, "status": 4, "special": 5, "environment": 6}


class Cell:
    __slots__ = ("ch", "key", "st", "prog", "ours", "note", "src")

    def __init__(self, ch, key, st, prog=None, ours=None, note="", src=""):
        self.ch, self.key, self.st, self.prog, self.ours, self.note, self.src = ch, key, st, prog, ours, note, src

    def as_dict(self):
        return {"ch": self.ch, "key": self.key, "st": self.st, "prog": self.prog, "ours": self.ours, "note": self.note, "src": self.src}

    def __repr__(self):
        return f"Cell({self.ch}.{self.key} {self.st} prog={self.prog!r} ours={self.ours!r} {self.note})"


def num_eq(a, b, tol):
    try:
        return abs(float(a) - float(b)) <= tol
    except (TypeError, ValueError):
        return a == b


def cmp(ch, key, prog, ours, src="", tol=0.0, note="", none_ok=False):
    """READ program value `prog` vs ours. `none_ok`: None is a legitimate value (no sound, no effect), not
    "the probe saw nothing"."""
    if ours is None and not none_ok:
        return Cell(ch, key, GAP, prog, ours, note or "no value observed", src)
    return Cell(ch, key, OK if num_eq(prog, ours, tol) else BAD, prog, ours, note, src)


def unk(ch, key, why, prog=None, ours=None, src=""):
    return Cell(ch, key, UNK, prog, ours, why, src)


def gap(ch, key, why, prog=None, ours=None):
    return Cell(ch, key, GAP, prog, ours, why)


def worst(cells):
    """The status a group of cells reports: the most severe one (none = no cell)."""
    if not cells:
        return None
    return max((c.st for c in cells), key=lambda s: SEVERITY[s])


# ---------------------------------------------------------------------------
# helpers


def rawv(rec, k):
    return rec["raw"][k]


def params(rec):
    r = rec["raw"]
    return r[64], r[65], r[66], r[67], r[68], r[69]


def hidden(rec):
    h = rec.get("s70_73") or [0, 0, 0, 0]
    return h


def parse_dbg(text):
    """`Shell { speed: 1.0, life: 23 }` -> {'speed': 1.0, 'life': 23.0} (scalar fields only)."""
    return {k: float(v) for k, v in re.findall(r"(\w+): ([-+0-9.eE]+)", text or "")}


def row_for(rec, airborne=False):
    """The action row a cast uses (FUN_140258180 picks the cast type, FUN_14025a3e0 the row:
    s12 for a normal cast, s13 for type 11 = s40 1 or 3, or 2 while airborne)."""
    s40 = rawv(rec, 40)
    ground, air = rec["action"]["ground"], rec["action"]["air"]
    want_air = s40 in (1, 3) or (s40 == 2 and airborne)
    return (air or ground) if want_air else (ground or air)


def vsub(a, b):
    return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]


def vlen(a):
    return math.sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2])


def vdot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def vnorm(a):
    n = vlen(a)
    return [a[0] / n, a[1] / n, a[2] / n] if n > 1e-9 else [0.0, 0.0, 1.0]


def angle_between(a, b):
    c = max(-1.0, min(1.0, vdot(vnorm(a), vnorm(b))))
    return math.degrees(math.acos(c))


# ---------------------------------------------------------------------------
# Program side: ranges, speeds, lifetimes (read)


def prog_ramp(v0, v1, ticks, age):
    """FUN_140264710: speed at program age `age`: v0 when there is no ramp, else a straight
    line v0 -> v1 over `ticks` ticks and v1 after."""
    if ticks == 0:
        return v0
    if age < ticks:
        return (v1 - v0) / ticks * age + v0
    return v1


def prog_life(s64, s65, s66, s67):
    """FUN_1402636f0, read in decomp_all.c: s64 >= 0 is the life in ticks; s64 < 0 is a range of
    -s64 x 10 u walked at the speed ramp s65 -> s66 over s67 ticks; the result is truncated."""
    if s64 >= 0:
        return s64
    rng = -s64 * 10.0
    v0, v1 = s65 * U, s66 * U
    if s67 == 0:
        return int(rng / v0) if v0 > 0 else 0
    ramp_len = (v0 + v1) * s67 * 0.5
    if rng <= ramp_len:
        return s67
    return int((rng - ramp_len) / v1) + s67 if v1 > 0 else s67


def prog_band(s44, motion, target_dist):
    """FUN_140273d40 (the aim helper FUN_14026e410): the aim point is pulled to 80-300 u (s44 = 0) or
    300-600 u (s44 = 1) from the spawn point, no limit for s44 = 2 and every motion-16 Hole;
    a pulled aim drops the target. No target: 190 u (any s44 but 1) / 300 u (s44 = 1) straight ahead
    (FUN_14026e410 reads 0x1402f80cc = 190 and 0x1402f8138 = 300; the clamp constants 80 / 300 / 600 at
    0x1402f8040 / 0x1402f8138 / 0x1402f81e0).
    Returns (aim distance, kept)."""
    if target_dist is None:
        return (300.0 if s44 == 1 else 190.0), False
    if s44 == 2 or motion == 16:
        return target_dist, True
    lo, hi = (80.0, 300.0) if s44 == 0 else (300.0, 600.0)
    if target_dist < lo:
        return lo, False
    if target_dist > hi:
        return hi, False
    return target_dist, True


# ---------------------------------------------------------------------------
# Context: program-side data tables


class GsData:
    """`gsdata_en.dat` read directly (our own reader, independent of the converter that wrote
    cache/skills.json): the program's records of 0x90 bytes from file offset 0x18 (record k = our
    record k - 1; its 72 shorts are our s2..s73), the action table at 0x1B00C (574 x 0x72 bytes) and
    the hit-reaction table at 0x2B00C (FUN_14017dcb0 parses them; docs/FORMATS-SKILLS.md)."""

    RECORD0, RECORD, ACTION, ROW, REACTION = 0x18, 0x90, 0x1B00C, 0x72, 0x2B00C

    def __init__(self, data):
        import struct

        self._struct = struct
        self.b = data

    @classmethod
    def open(cls, path):
        from pathlib import Path

        return cls(Path(path).read_bytes())

    def record(self, k):
        return self._struct.unpack_from("<72h", self.b, self.RECORD0 + self.RECORD * k)

    def action_row(self, i):
        return self._struct.unpack_from("<57h", self.b, self.ACTION + self.ROW * i)

    def reaction_row(self, i):
        return self._struct.unpack_from("<57h", self.b, self.REACTION + self.ROW * i)


class Program:
    """The data the program-side rules read, loaded from the cache (and gsdata_en.dat when found)."""

    def __init__(self, skills_doc, shotpoints=None, hands=None, gs=None):
        self.gs = gs
        self.doc = skills_doc
        self.rec = {}
        for s in skills_doc["skills"]:
            self.rec.setdefault(s["id"], s)
        self.reactions = {r["row"]: r for r in skills_doc.get("reactions", [])}
        self.markers = (shotpoints or {}).get("markers", {})
        self.hands = (hands or {}).get("clips", {})
        self.aura_particle = self._aura_particle()

    def _aura_particle(self):
        """Program record 0 (the tail of the gsdata header, P0-SKILLS.md section 1): read from the file when
        it is there (text 301, s12 = 343, s26 = 1, s43 = 4, s45 = 1 = Level +, s49 = 1; action row 343 = an
        empty row: WAIT0, fire tick 0), else the values the audit recorded."""
        raw = [0] * 70
        raw[26], raw[43], raw[45], raw[49] = 1, 4, 1, 1
        if self.gs is not None:
            rec0 = self.gs.record(0)
            raw = [0, 0] + list(rec0[:68])
        raw[9] = 5  # shown as Special by `SkillDef::aura_particle`; the program has no colour for record 0
        return {
            "id": 0, "name": "Aura Particle", "capsule": "special", "raw": raw, "s70_73": [0, 0, 0, 0],
            "cost": 0, "strength": 0, "action": {"ground": None, "air": None}, "registered": True,
            "synthetic": True,
        }

    def marker_list(self, rec, air=False):
        row = row_for(rec, air)
        if not row or not row.get("effect"):
            return None
        return self.markers.get(str(row["effect"]))

    def hand_at(self, clip, frame):
        c = self.hands.get(clip)
        if not c or not c.get("pos"):
            return None
        pos = c["pos"]
        f = max(0, min(len(pos) - 1, frame))
        a = int(math.floor(f))
        b = min(a + 1, len(pos) - 1)
        t = f - a
        return [pos[a][i] + (pos[b][i] - pos[a][i]) * t for i in range(3)]

    def spawn_point(self, rec):
        """FUN_14018a310: the first SHOTnn marker of the cast effect (the caster-frame offset), else the
        right-hand bone at the cast clip's fire frame. None when neither is known."""
        m = self.marker_list(rec)
        if m:
            return list(m[0]), "marker SHOT01 of eft%04d" % row_for(rec)["effect"]
        row = row_for(rec)
        if row:
            h = self.hand_at(row["anim_name"], row["fire_tick"])
            if h:
                return h, "right hand at fire frame %d of %s" % (row["fire_tick"], row["anim_name"])
        return None, "no marker, no hand path"

    def shot_count(self, rec):
        m = self.marker_list(rec)
        return len(m) if m else 1


# ---------------------------------------------------------------------------
# DISPATCH


def program_class(rec):
    """`FUN_140263a00` picks the handler: s43 = 0 -> by motion s63 (table 0x1403fb240), else by s43
    (table 0x1403fb190)."""
    r = rec["raw"]
    if r[43] == 0:
        return MOTION_CLASS.get(r[63], "none")
    return S43_CLASS.get(r[43], "none")


def ours_class(rec, O):
    st = O["static"]
    b = st["behaviour"]
    det = st.get("detail") or {}
    r = rec["raw"]
    if b == "AuraParticle":
        return "status_self"
    if b.startswith("Shoot:"):
        kind = b.split(":")[1]
        if kind == "Shell":
            if det.get("placeholder"):
                return "placeholder"
            return "object" if r[63] == 11 else "shell"
        return {"Parabola": "parabola", "Arc": "arc", "Crawl": "crawl", "Tornado": "tornado", "Fall": "fall", "Ricochet": "ricochet"}[kind]
    if b == "Melee":
        return {"Sword": "sword", "Sphere": "punch", "Stamp": "pursuit"}[det.get("shape", "Sphere")]
    if b == "Place":
        return {"Hole": "hole", "RainColumn": "rain_column", "RainCloud": "rain_cloud", "Mine": "mine", "Wave": "around_self"}[det["kind"]]
    if b == "Guard":
        return "guard"
    if b.startswith("Effect:"):
        k = b.split(":")[1]
        return {"Environment": "environment", "Field": "erase_field", "TargetNow": "instant", "Everyone": "instant", "Caster": "effect_caster", "Travels": "placeholder"}.get(k, k)
    if b == "NotBuilt":
        return "not_built"
    return b


def chk_dispatch(P, rec, O):
    pc = program_class(rec)
    oc = ours_class(rec, O)
    src = "FUN_140263a00 -> tables 0x1403fb240 (s63) / 0x1403fb190 (s43)"
    if pc == "copy":
        return [Cell("dispatch", "handler", OK, pc, oc, "resolved at the press as the copied skill (Combat::cast_from)", src)]
    if pc in ("movement", "status_self") and oc == "effect_caster":
        return [Cell("dispatch", "handler", OK, pc, oc, "self effect (the body is chosen by s44 / s45 inside it, see effect)", src)]
    if pc == "none" and oc in ("placeholder", "shell", "not_built"):
        return [Cell("dispatch", "handler", BAD, pc, oc, "the program has no handler (motions 18 / 19): the object does nothing", src)]
    if pc == "short3":
        return [unk("dispatch", "handler", "motion 3 has a handler but no skill uses it", pc, oc, src)]
    return [cmp("dispatch", "handler", pc, oc, src)]


# ---------------------------------------------------------------------------
# PAY: cost, Level, Health, uses, gates (FUN_1402621f0 / FUN_1402623b0 / FUN_140262670 / FUN_140263410)


def passive_kind(rec):
    """Skills that work from their slot and cannot be pressed: FUN_140261e90 returns before any use for an s37 = 1 skill (Orb, Amulet),
    and for an s43 = 2 / s44 = 7 skill (automatic Shelter) unless the use is forced by an incoming attack (FUN_14026f530)."""
    r = rec["raw"]
    if rec.get("synthetic"):
        return None
    if r[37] == 1:
        return "Orb / Amulet (s37 = 1)"
    if rec["capsule"] == "defence" and r[43] == 2 and r[44] == 7:
        return "automatic Shelter (s44 = 7)"
    return None


def chk_pay(P, rec, O):
    cells = []
    r = rec["raw"]
    if passive_kind(rec):
        return []  # paid when an attack triggers it: `guard.auto_*` / `guard.orb_pay`
    if rec.get("synthetic"):
        # Record 0: no cost, no Level penalty; one use (s26 = 1), taken when it is used.
        u = O.get("uses") or {}
        got = None if u.get("unlimited") else u.get("casts_until_used_up")
        return [cmp("pay", "uses", 1, got, "FUN_140262670 (record 0: s26 = 1)")]
    g = O["cast"]["ground"]
    uses = O.get("uses") or {}
    src = "FUN_1402621f0 (pay), FUN_1402623b0 (cost)"
    s22, s23, s24, s25, s26 = r[22], r[23], r[24], r[25], r[26]
    if not g.get("accepted"):
        why = "the probe's press was refused in the arena (a passive skill or one that needs a stage object)"
        return [gap("pay", "cost", why)]
    before, end = g["before"], g["end"]
    # A skill that travels (s43 = 0: shells, melee, holes ...) pays at its fire tick and its own effect lands
    # later, on what it hits: read the payment at the fire step (melee has no fire event; its probe target is
    # untouchable, so the end of the cast is clean). Everything else shows its effect at the fire tick.
    if r[43] == 0 and g.get("at_fire_after"):
        end = g["at_fire_after"]
        before = g["at_fire_before"] if False else before
    # Aura at the end of the cast: the cost the program takes is the slot cost s22 + s24 x uses (cap 99,
    # status 30 halves / doubles; none here), clamped to Level afterwards. The Level-to-minimum skills
    # (s23 < 0) pay no Aura and set Level to the rule minimum (0): FUN_14024d8b0(player, 0).
    A0 = uses.get("aura_before", before["aura_ticks"] / APORA)
    L0 = before["level"]
    H0 = before["health"]
    if s23 < 0:
        L1 = 0
        A1 = min(A0, L1)
        cost_prog = A0 - A1
    else:
        cost = max(0, min(99, s22 + s24 * 0))
        L1 = max(0, L0 - s23)
        A1 = max(0.0, A0 - cost)
        A1 = min(A1, L1)
        cost_prog = A0 - A1
    cost_ours = None
    if r[43] == 0 and g.get("at_fire_before") and g.get("at_fire_after"):
        # the Aura missing at the fire step against the cast's start (refill is paused during the cast)
        cost_ours = (before["aura_ticks"] - g["at_fire_after"]["aura_ticks"]) / APORA
    elif uses.get("cost_series"):
        # the series' first entry is the first cast's Aura at Ready, measured in whole points of drop
        cost_ours = uses["cost_series"][0]
    # A skill whose own effect moves the caster's Aura (Add codes of erase / status / special skills,
    # a kind-2 / kind-9 self effect, Steal Power...) cannot be told from its cost at Ready.
    kinds = r[45:49] if r[43] == 4 else []
    non_attack_effect = rec["capsule"] in ("erase", "status", "special")
    # Only a self effect (s43 = 4) takes hold at the fire tick and moves the caster's Aura / Level / Health itself. A
    # movement skill (s43 = 3) carries movement numbers in those fields, an instant (motion 4) lands s65 ticks after the
    # fire tick (the snapshot above is taken AT the fire tick, before it), and a travelling skill's Add effect lands on
    # what it hits (wave 6, lane probes: the 41 'own effect' holes this opened).
    lands_at_fire = r[43] == 4
    own_aura = (non_attack_effect and lands_at_fire and r[51] != 0) or any(k in (2, 9) for k in kinds)
    net = _own_net(rec, O.get("effect")) if (own_aura or any(k in (1, 3, 10) for k in kinds)) else None
    if own_aura:
        if net:
            cells.append(net["aura"])
        else:
            cells.append(gap("pay", "aura_cost", "the skill's own effect moves the caster's Aura (Add code %d / self kind): not separable at Ready" % r[51]))
    elif s23 >= 0 and cost_ours is not None:
        # compare the plain cost (clamp to Level is a separate effect, checked through Level below)
        want = float(max(0, min(99, s22)))
        note = ""
        if s23 > 0:
            # Level -= s23 pulls Aura down to the new Level (FUN_1402621f0 tail): the net drop of Aura
            want = A0 - min(A0 - want, L0 - s23)
        if r[41] == 1 and r[26] == 1:
            # Retain (s41 = 1) on a one-use skill: the use-up test pays the cost AGAIN right away and keeps it
            # (FUN_140262fc0 / FUN_140263460), so the first cast takes the cost twice.
            want, note = want * 2, "Retain: the cast is paid again at once and the skill kept (FUN_140262fc0 / FUN_140263460)"
        cells.append(cmp("pay", "aura_cost", want, cost_ours, src + " (slot cost s22 + s24 x uses)", tol=0.05, note=note))
    elif cost_ours is not None:
        cells.append(cmp("pay", "aura_cost", float(cost_prog), cost_ours, src + " (s23 < 0: Level to the minimum, no Aura cost)", tol=0.01))
    # Level and Health at the end of the cast (skills whose own effect moves them are skipped: the
    # Add codes 9 / 11, and anything that changes Level or Health through its effect).
    own_effect = (rec["capsule"] == "attack" and r[51] in (9, 11) and r[43] != 0) or (non_attack_effect and lands_at_fire and any(k in (1, 3, 10) for k in kinds))
    if own_effect and net:
        cells.extend([net["level"], net["health"]])
    elif not own_effect:
        cells.append(cmp("pay", "level", L1, end["level"], src + " (Level -= s23, clamped to the rule's range)"))
        if s25 == -1:
            h1 = H0 - (H0 + 1) // 2
        else:
            h1 = H0 - s25
        cells.append(cmp("pay", "health", max(h1, 0), end["health"], src + " (Health -= s25; -1 = half, rounded up)"))
    else:
        cells.append(gap("pay", "level_health", "the skill's own effect changes Level / Health; read on the effect channel"))
    # Uses: s26 (0 = unlimited); Retain (s41 = 1) keeps the skill while Aura covers it again.
    if "refused" in uses or uses.get("unlimited") is None:
        cells.append(gap("pay", "uses", "the repeated cast was refused"))
    else:
        want = None if s26 == 0 else s26
        got = None if uses.get("unlimited") else uses.get("casts_until_used_up")
        note = ""
        if r[41] == 1:
            want, note = None, "Retain (s41 = 1): FUN_140262fc0 pays again and keeps it while Aura covers the cost"
        cells.append(cmp("pay", "uses", "unlimited" if want is None else want, "unlimited" if got is None else got, "FUN_140262670 / FUN_140263410", note=note))
    # Per-use cost change (s24).
    series = uses.get("cost_series") or []
    if s24 != 0 and series and s23 >= 0:
        want = [float(max(0, min(99, s22 + s24 * n))) for n in range(len(series))]
        cells.append(cmp("pay", "cost_per_use", want, series, "FUN_1402623b0 (cost = s22 + s24 x uses)", tol=0.05))
        # lists compare element-wise (the Aura refill after the cast adds a hundredth of a point at most)
        c = cells[-1]
        c.st = OK if all(num_eq(a, b, 0.05) for a, b in zip(want, series)) and len(want) == len(series) else BAD
    # Gates.
    gate = O.get("gate") or {}
    if gate:
        if s23 > 0 and gate.get("low_level") is not None:
            cells.append(cmp("pay", "level_gate", False, gate["low_level"], "FUN_1402621f0: Level >= s23 else the cast does not pay / fire", note="a press with Level below s23 must not cast"))
        if s22 > 0 and gate.get("low_aura") is not None:
            cells.append(cmp("pay", "aura_gate", False, gate["low_aura"], "FUN_1402621f0 / FUN_140261e90: Aura >= cost"))
        if r[38] in (1, 2, 5, 7, 8, 9) and gate.get("requirement_violated") is not None:
            c = cmp("pay", "requirement", False, gate["requirement_violated"], "FUN_140262470 (codes 1,2,5,7,8,9)")
            if r[38] == 5:
                c = _req5_cell(rec, O.get("req5"), gate["requirement_violated"])
            cells.append(c)
        if gate.get("ok") is False and rec["capsule"] != "defence":
            cells.append(Cell("pay", "castable", BAD, True, False, "the probe could not cast it with the requirement met", ""))
    return cells


def _own_net(rec, e):
    """The net Aura / Level / Health after the payment AND the skill's own self effect (s43 = 4) at the fire tick, from the
    program's own rules: pay (FUN_1402621f0: Aura - s22 clamped to the new Level, Level - s23 or to the minimum when s23 < 0,
    Health - s25) then FUN_140270b00 kinds in order: 1 Level + amount (clamped 0..99), 2 Aura + amount (99 = to Level, capped at
    Level), 3 Health + amount (cap 20 = the arena's maximum), 10 Level doubled. None when the effect probe did not run."""
    if not e or "at_fire" not in e or not e.get("started"):
        return None
    r = rec["raw"]
    b, a = e["before"][0], e["at_fire"][0]
    L, A, H = b["level"], b["aura_ticks"] / APORA, b["health"]
    s22, s23, s25 = r[22], r[23], r[25]
    if s23 < 0:
        L = 0
        A = min(A, L)
    else:
        L = max(0, L - s23)
        A = min(max(0.0, A - max(0, min(99, s22))), L)
    H = H - ((H + 1) // 2 if s25 == -1 else s25)
    for k in range(4):
        kind, amt = r[45 + k], r[49 + k]
        if kind == 1:
            L = max(0, min(99, L + amt))
        elif kind == 2:
            A = L if amt >= 99 else min(L, A + amt)
        elif kind == 3:
            H = min(20, H + amt)
        elif kind == 10:
            L = min(99, L * 2)
    A = min(A, L)
    src = "FUN_1402621f0 (pay) then FUN_140270b00 kinds %s at the fire tick" % [x for x in r[45:49] if x]
    return {
        "level": cmp("pay", "net_level", L, a["level"], src),
        "aura": cmp("pay", "net_aura", A, a["aura_ticks"] / APORA, src, tol=0.1),
        "health": cmp("pay", "net_health", H, a["health"], src),
    }


def _copy_cells(P, rec, O, g):
    """Recollection / Copy (s43 = 6): FUN_140261e90 resolves the button's skill at the press (s44 0: the player's last used attack,
    +0x61ac; s44 1: the last attack that hurt them, +0x61b0), FUN_140258180 then starts the cast of THE COPIED SKILL (its cast type
    and action row, `FUN_14025a3e0`) and FUN_1402626d0 uses it; with nothing remembered the index is -1 and FUN_140258180 only plays
    the 'no' sound (0, 2). The probe's player has used / been hurt by Laser (id 65), so the timing must be Laser's row."""
    src = "FUN_140261e90 + FUN_140258180 (the copied skill's row) + FUN_14025a3e0"
    copied = P.rec.get(65)
    row = row_for(copied, False)
    frames = P.clips.get(row["anim_name"])
    cells = [
        cmp("cast", "fire_tick", SIM * row["fire_tick"], g.get("fire_step"), src + ": Laser's row w43 x 2", tol=0),
        cmp("cast", "clip", row["anim_name"], g.get("clip"), src + ": Laser's clip"),
    ]
    if frames is not None:
        cells.append(cmp("cast", "hold", frames, g.get("hold_steps"), src + ": the clip length of the copied skill", tol=0))
    cp = O.get("copy") or {}
    if cp:
        cells.append(cmp("cast", "empty_refused", True, cp.get("refused_when_nothing_remembered"), "FUN_140258180: index -1 returns without a cast"))
        cells.append(cmp("cast", "empty_no_sound", True, cp.get("denied_sound"), "FUN_140258180 -> FUN_140276330(0, 2, 2...): common/2 (our `Denied`) plays when the remembered skill is -1",
                         note="ours refuses silently"))
    return cells


def _req5_cell(rec, q, violated):
    """Requirement 5 'N or fewer skills left in arsenal': FUN_140262470 case 5 refuses when skills left > N, and skills left is
    (30 - cards dealt) + the capsule objects of the player that are not retired (`DAT_140585c20 - DAT_140585c00 + 30`;
    FUN_14014ff80 returns the same number): the undealt deck PLUS the capsules lying, appearing or waiting to regenerate."""
    src = "FUN_140262470 case 5: skills left = (30 - dealt) + capsules (DAT_140585c20 counts the player's capsule objects not in state 3)"
    if not q:
        return unk("pay", "requirement", "requirement 5 was not probed with capsules", False, violated, src)
    want = {"deck_n": True, "deck_n_plus_1": False, "deck_n_minus_1_capsule": True, "deck_n_capsule": False,
            "deck_n_minus_2_two_capsules": True, "deck_n_minus_1_two_capsules": False}
    got = {k: q[k] for k in want}
    bad = [k for k in want if want[k] != got[k]]
    note = ""
    if bad:
        note = "ours counts the undealt deck only: with the deck at the limit and a capsule lying it still casts (%s)" % ", ".join(bad)
    return Cell("pay", "requirement", BAD if bad else OK, want, got, note, src)


# ---------------------------------------------------------------------------
# CAST: action row -> clip, fire tick, hold, air row, charge


def chk_cast(P, rec, O):
    cells = []
    r = rec["raw"]
    cast = O["cast"]
    g, a = cast["ground"], cast["air"]
    row = row_for(rec, False)
    arow = row_for(rec, True)
    # Hold-to-charge is the RECORD's s37 == 2 (FUN_140258180 0x14025823e: record +0x46 picks the cast mode +0x68 = 1; FUN_14015b830
    # tests the same field for the arsenal rules), not the action row's w39: FUN_14025c720 reads w39 as the wind-up countdown in every
    # mode and only freezes the fire countdown in mode 1. Dragon Bullet's row carries w39 = 13 but its s37 is 0.
    charged_rec = r[37] == 2
    src = "FUN_14025a3e0 (cast start), FUN_14025c720 (fire countdown), action row w38 / w43"
    if rec.get("synthetic"):
        # Aura Particle: row 343 = WAIT0, fire tick 0 -> instant: no cast state, used in the press (0x14025a3e0 returns 0
        # for clip 0; MEASURED live by moverec: press -> fire 0 ticks, the fighter keeps running). Compare what OUR game
        # does: the cell is a cross only while ours holds the fighter (it did, 46 ticks, until DECISIONS 2026-10-01 'aura').
        cells.append(cmp("cast", "hold", 0, g.get("hold_steps"), "0x14025a3e0 (clip 0: no state); MEASURED live by moverec: the fighter keeps running through the use", tol=0,
                         note="row 343 names clip 0 (WAIT0): the program enters no cast state"))
        return cells
    if passive_kind(rec):
        return [cmp("cast", "press_refused", True, not g.get("accepted"), "FUN_140261e90: pressing an %s starts nothing" % passive_kind(rec))]
    if not g.get("accepted"):
        return [gap("cast", "fire", "the probe's press was refused (a skill that needs a stage object)")]
    if r[43] == 6:
        return _copy_cells(P, rec, O, g)
    if row is None:
        # No action row: the cast fires at once with no state (caststill read, 0x140258180 / 0x14025a3e0).
        cells.append(cmp("cast", "fire", 0, g.get("fire_step"), src, tol=1, note="no action row"))
    else:
        melee = O["static"]["behaviour"] == "Melee"
        charged_prog = charged_rec
        no_clip = row["anim_id"] == 0 or row["anim_name"] == "WAIT0"
        if not melee and not charged_prog and g.get("fired") and no_clip:
            # A row naming clip 0 starts no cast state: FUN_140258180 calls FUN_1402626d0 in the press itself
            # (the countdown of FUN_14025c720, w43, only runs inside state 10 / 11).
            cells.append(cmp("cast", "fire_tick", 0, g.get("fire_step"), "0x140258180 -> 0x1402626d0 (clip 0: no state, fired in the press; w43 unused)", tol=0))
        elif not melee and not charged_prog and g.get("fired"):
            cells.append(cmp("cast", "fire_tick", SIM * max(row["fire_tick"], 1), g.get("fire_step"), src + " w43 x 2 sim ticks, at least one program tick: the countdown set at the press is first checked by the NEXT tick's FUN_14025c720 (LIVE: Dash, w43 = 0, press 39 / fire 40)", tol=0))
        elif charged_prog and g.get("fired") and g.get("release_step") is not None:
            want = g["release_step"] + SIM * (row["fire_tick"] - row["charge_cycle"])
            cells.append(cmp("cast", "fire_after_release", want, g.get("fire_step"), "FUN_14025c720: the shot leaves w43 - w39 ticks after the release", tol=1))
        # Hold: the whole clip (state 10 / 11), unless the row names clip 0 (WAIT0): no state at all.
        clips = P.clips
        if row["anim_id"] == 0 or row["anim_name"] == "WAIT0":
            want = 0
            note = "row names clip 0 (WAIT0): the program enters no cast state"
        else:
            frames = clips.get(row["anim_name"])
            want = None if frames is None else frames  # the table already holds sim ticks
            note = "state timer = the clip's length (FUN_14025a3e0)"
        if charged_prog:
            cells.append(unk("cast", "hold", "hold-to-charge: the state timer after the release (the clip restarts at w40) is not read", None, g.get("hold_steps"), "FUN_14025c720"))
        elif want is None:
            cells.append(unk("cast", "hold", "clip length of %s unknown on this character" % row["anim_name"], None, g.get("hold_steps"), src))
        else:
            c = cmp("cast", "hold", want, g.get("hold_steps"), src, tol=0, note=note)
            cells.append(c)
        cells.append(cmp("cast", "clip", row["anim_name"] if row["anim_id"] != 0 else "WAIT0", g.get("clip"), "action row w38 (FUN_14025a3e0)",
                         note="the character's own clip table decides whether it has the clip"))
    # Air use: s40 0 ground only, 1 air only, 2 / 3 both (FUN_14024fce0, mask 0x1a3); row s13 for the air cast.
    s40 = r[40]
    ga, aa = cast.get("ground_allowed"), cast.get("air_allowed")
    cells.append(cmp("cast", "ground_ok", s40 in (0, 2, 3), ga, "FUN_14024fce0"))
    cells.append(cmp("cast", "air_ok", s40 in (1, 2, 3), aa, "FUN_14024fce0"))
    if s40 in (1, 2, 3):
        want_row = (arow or row)
        cells.append(cmp("cast", "air_row", want_row["anim_name"] if want_row else None, cast.get("air_row_anim"), "FUN_140258180 cast type 11 -> row s13"))
        if a.get("accepted") and want_row and a.get("fired") and O["static"]["behaviour"] != "Melee" and not charged_rec:
            cells.append(cmp("cast", "air_fire_tick", SIM * want_row["fire_tick"], a.get("fire_step"), src))
    # Charge: the program charges when the record's s37 == 2 (Gungnir, Ki Cannon, Ki Palm, Young White Dragon); the row's w39 is only
    # the wind-up count (Dragon Bullet's row has w39 = 13 but s37 = 0: an ordinary cast).
    cells.append(cmp("cast", "charged", charged_rec, O["static"]["charged"], "FUN_140258180 (record +0x46 = s37 == 2 -> mode 1) / FUN_14025c720"))
    return cells


# ---------------------------------------------------------------------------
# STR (attack damage): FUN_140272400 + the 31 rule functions of the table at 0x1402dd0a0 (disassembled
# with capstone in this lane: R5 of audits/P0-SKILLS.md is now read)
#
#   STR = rule(s45)(skill) + caster's attack status + s47 x uses of the slot, never below 0   (FUN_140272400)
#   rule 0  s46                                          (0x140273f50)
#   rule 1  (target Health - caster Health) x s46         (0x140273f80)    rule 2 the other way (0x140273fe0)
#   rule 3  (attack skills used up) x s46                 (0x140274040)    rule 4 defence skills (0x1402740d0)
#   rule 17 target Level x s46                            (0x140274620)
#   rule 18 whole points of target Aura x s46             (0x140274680)    rule 20 whole points / s46 (0x140274730)
#   rule 21 the cast's stored value (+0x1c: Aura spent charging, Level at pay) x s46   (0x140274790)
#   rule 23 target's empty buttons x s46                  (0x140274810)    rule 24 caster's (0x140274890)
#   rule 26 sum of s46 of the caster's equipped skills whose s10 equals this skill's s46     (0x140274990)
#   rule 27-29 rand() x (s45 - 25) / 32768 (whole), x s46 (0x140274a10)
#   rule 30 Aura Particle capsules lying (state 2) on the field x s46   (0x140274a90)


def _str21_cell(P, rec, s21, src):
    """Rule 21 (0x140274790) = the cast's stored value (slot +0x1c) x s46. The stored value (FUN_1402626d0 / FUN_1402621f0):
    Level at the press for a skill that pays Level to the minimum (s23 < 0: Finisher, FUN_1402621f0 `+0x1c = player Level`);
    for a charged skill the base cost plus count / 10, count = program ticks the button was held after the wind-up (FUN_14025c720:
    +0x80 += 1 each tick the button is down and the next point is affordable)."""
    s46 = rec["raw"][46]
    bad, shown = [], []
    for run in s21["runs"]:
        if run.get("hit_step") is None:
            return gap("str", "damage", "the charged shot did not hit in the arena")
        if s21["kind"] == "level":
            want = [s46 * run["level"]]
        else:
            cc = (row_for(rec, False) or {}).get("charge_cycle", 0)
            count = (run["hold"] + 1) // 2 - cc  # the press step is the first tick the button is down
            want = sorted({s46 * ((count + d) // 10) for d in (-1, 0, 1)})
        ok = run["damage"] in want
        shown.append("%s -> STR %s (program %s)" % ("Level %d" % run["level"] if s21["kind"] == "level" else "held %d sim ticks" % run["hold"], run["damage"], "/".join(str(x) for x in want)))
        if not ok:
            bad.append(shown[-1])
    kind = "2 x Level at the press" if s21["kind"] == "level" else "(held program ticks - wind-up) / 10 x s46"
    return Cell("str", "damage", BAD if bad else OK, kind, "; ".join(shown), "; ".join(bad), src)


def prog_str(rec, att_health, tgt_health, state):
    """STR in the probe's fixed arena (`state` as the Rust probe reports it). Returns (value, set_of_allowed or None)."""
    r = rec["raw"]
    rule, s46, s47 = r[45], r[46], r[47]
    a, t = state["attacker"], state["target"]
    if rule == 0:
        v = s46
    elif rule == 1:
        v = (tgt_health - att_health) * s46
    elif rule == 2:
        v = (att_health - tgt_health) * s46
    elif rule == 3:
        v = a["used_attack"] * s46
    elif rule == 4:
        v = a["used_defence"] * s46
    elif rule == 17:
        v = t["level"] * s46
    elif rule == 18:
        v = int(t["aura"]) * s46
    elif rule == 20:
        v = int(int(t["aura"]) / s46) if s46 else 0
    elif rule == 23:
        v = t["empty"] * s46
    elif rule == 24:
        v = a["empty"] * s46
    elif rule == 30:
        v = state["aura_capsules"] * s46
    elif rule in (27, 28, 29):
        return None, sorted({max(0, k * s46) for k in range(rule - 25)})
    else:
        return None, None
    return max(0, v), None


def chk_str(P, rec, O, skills_by_id=None):
    if rec["capsule"] != "attack" or rec.get("synthetic"):
        return []
    r = rec["raw"]
    rule, s46, s47 = r[45], r[46], r[47]
    sp = O.get("str") or {}
    dmg = sp.get("damage")
    if not dmg:
        return [gap("str", "damage", "no STR probe")]
    cells = []
    src = "FUN_140272400 + the rule function of s45 = %d (table 0x1402dd0a0)" % rule
    if rule == 21:
        s21 = O.get("str21")
        if s21 and s21.get("runs"):
            cells.append(_str21_cell(P, rec, s21, src))
        else:
            cells.append(unk("str", "damage", "rule 21 reads the cast's stored value (+0x1c) x s46 (0x140274790); a thrown object's value (Power, Psycho Kinesis) comes from the object, not probed", None, [d["str"] for d in dmg], src))
    elif rule == 26:
        # Sum of s46 over the caster's equipped skills whose s10 == this skill's s46 (0x140274990): membership per skill.
        cat = s46
        want = {}
        for i, x in P.rec.items():
            if 1 <= i <= 340 and i != rec["id"] and x["raw"][10] == cat and x["raw"][46] != 0:
                want[str(i)] = x["raw"][46]
        got = sp.get("bundle") or {}
        miss = sorted(set(want) - set(got), key=int)
        extra = sorted(set(got) - set(want), key=int)
        wrongv = sorted((k for k in set(want) & set(got) if want[k] != got[k]), key=int)
        name = lambda k: P.rec[int(k)]["name"]
        if not (miss or extra or wrongv):
            cells.append(Cell("str", "bundle", OK, "%d members (s10 = %d)" % (len(want), cat), "%d members" % len(got), "", src))
        else:
            note = []
            if miss:
                note.append("misses " + ", ".join(name(k) for k in miss))
            if extra:
                note.append("wrongly adds " + ", ".join(name(k) for k in extra))
            if wrongv:
                note.append("wrong STR for " + ", ".join(name(k) for k in wrongv))
            cells.append(Cell("str", "bundle", BAD, "equipped skills with s10 == %d (%s)" % (cat, ", ".join(name(k) for k in sorted(want, key=int))), "ours: " + "; ".join(note), "ours picks the kind by name / school, the program by the record's s10 field", src))
    elif rule in (27, 28, 29):
        _, allowed = prog_str(rec, 17, 12, sp["state"])
        draws = sp.get("random_draws") or []
        ok = bool(draws) and all(d in allowed for d in draws) and set(draws) == set(allowed)
        cells.append(Cell("str", "random", OK if ok else BAD, "rand x %d / 32768, whole, x s46: one of %s" % (rule - 25, allowed), sorted(set(draws)), "60 draws", src))
    else:
        for d in dmg:
            want, _ = prog_str(rec, d["attacker_health"], d["target_health"], sp["state"])
            cells.append(cmp("str", "damage", want, d["str"], src, note="Health %d vs %d" % (d["attacker_health"], d["target_health"])))
            if rule == 0:
                break
    return cells


# ---------------------------------------------------------------------------
# BAND (aimed families)

BAND_MOTIONS = {1, 2, 6, 7, 15, 16, 20, 4}


def chk_band(P, rec, O):
    r = rec["raw"]
    if r[43] != 0 or r[63] not in BAND_MOTIONS or not O.get("band"):
        return []
    s44, motion = r[44], r[63]
    cells = []
    bad = []
    for row in O["band"]:
        t = row["target_at"]
        # The probe's spawn is 12 u up and the target's centre 9 u up: the straight distance.
        td = None if t is None else math.sqrt(t * t + 3.0 * 3.0)
        d, kept = prog_band(s44, motion, td)
        if abs(row["aim_dist"] - d) > 0.05 or (t is not None and row["kept"] != kept):
            bad.append((t, d, kept, row["aim_dist"], row["kept"]))
    src = "FUN_140273d40 / FUN_14026e410 (range band s44)"
    if s44 == 2 and r[63] != 16:
        extra = "no limit"
    cells.append(Cell("band", "aim", BAD if bad else OK, "s44=%d: %s; nobody locked: %d u" % (s44, {0: "80-300 u", 1: "300-600 u", 2: "no limit"}.get(s44, "?"), 300 if s44 == 1 else 190), "ok" if not bad else bad, "", src + "; no target: FUN_14026e410 adds 190 u (300 u when s44 = 1) along the facing"))
    return cells


# ---------------------------------------------------------------------------
# FLIGHT (projectiles): struct values and measured kinematics


def track_steps(tr, spawn):
    pts = tr["pts"]
    if tr.get("age0") == 1 and spawn is not None:
        pts = [spawn] + pts
    return pts, [vlen(vsub(pts[i], pts[i - 1])) for i in range(1, len(pts))]


def chk_flight(P, rec, O):
    cells = []
    fl = O.get("flight")
    st = O["static"]
    if rec["raw"][43] == 6:
        return cells
    kind = st["behaviour"].split(":")[1] if st["behaviour"].startswith("Shoot:") else None
    if fl is None or kind is None:
        return cells
    if "probe_error" in fl:
        return [gap("flight", "run", fl["probe_error"])]
    r = rec["raw"]
    s64, s65, s66, s67, s68, s69 = params(rec)
    h = hidden(rec)
    det = parse_dbg((st.get("detail") or {}).get("debug"))
    free, steer = fl["free"], fl["steer"]
    launch0 = free.get("launch0") or {}
    shots0 = launch0.get("shots") or []
    tracks = free.get("tracks") or []
    src = "motion handler of s63 = %d" % r[63]
    if st.get("detail", {}).get("placeholder"):
        return [Cell("flight", "family", BAD, "motion %d handler" % r[63], "generic placeholder shell", "the family is not built: a generic homing shot flies instead", src)]

    def fld(name, want, tol=0.01, srcx=src, note=""):
        got = det.get(name)
        cells.append(cmp("flight", name, want, got, srcx, tol=tol, note=note))

    # ---- struct-level (the family constructor's numbers) -------------------------------------------
    if kind == "Shell":
        obj = r[63] == 11
        fld("speed", s65 * U, srcx="FUN_140264570: speed = s65 / 10.8 per tick")
        fld("end_speed", s66 * U)
        fld("ramp", s67, tol=0)
        turn = (h[1] * DEG_PER_POINT if obj else s68 * DEG_PER_POINT)
        fld("turn", math.radians(turn), tol=1e-4, srcx="FUN_140264570: homing = s68 x 0.1 deg per tick x homing %%" if not obj else "FUN_140268ee0: turn cap s71 (argument not confirmed)")
        if obj:
            cells[-1].st = UNK if cells[-1].st == BAD else cells[-1].st
        fld("life", prog_life(s64, s65, s66, s67), tol=0, srcx="FUN_1402636f0")
    elif kind == "Parabola":
        sp = s64 if s64 != 0 else s65
        fld("speed", sp * U, srcx="FUN_140264b00: horizontal speed s64 / 10.8 (s65 if s64 = 0)")
        fld("height", s66 * 10.0, srcx="FUN_140264b00: arc height s66 x 10 u above the straight line")
        fld("steer", s67 * U, srcx="FUN_14026bf50: sideways steering s67 / 10.8 x homing %")
    elif kind == "Arc":
        fld("speed", s65 * U, srcx="FUN_1402666b0: speed s65 / 10.8")
        fld("bend", math.radians(s67), 1e-4, srcx="FUN_1402666b0: yaw bend s67 degrees")
        fld("tilt", math.radians(s68), 1e-4, srcx="FUN_1402666b0: roll tilt s68 degrees")
        fld("pattern", s69, 0, srcx="FUN_1402666b0: shot count s69 (stands in for the SHOTnn markers)")
        fld("bend_step", math.radians(h[0]), 1e-4, srcx="s70 per-shot bend step (hidden field)")
        fld("tilt_step", math.radians(h[1]), 1e-4, srcx="s71 per-shot tilt step (hidden field)")
        fld("turn", math.radians(s66 * DEG_PER_POINT), 1e-4, srcx="FUN_140266bf0: re-aim s66 x 0.1 deg per tick")
    elif kind == "Crawl":
        fld("speed", s65 * U, srcx="FUN_140266060: speed ramp s65 -> s66 over s67")
        fld("end_speed", s66 * U)
        fld("ramp", s67, 0)
        fld("accel", math.radians(s68 * 0.01), 1e-5, srcx="FUN_140266290: turn acceleration s68 x 0.01 deg per tick^2")
        fld("cap", math.radians(s69 * 0.1), 1e-4, srcx="FUN_140266290: max turn s69 x 0.1 deg per tick")
        fld("life", prog_life(s64, s65, s66, s67), 0, srcx="FUN_1402636f0")
    elif kind == "Fall":
        fld("up", s64 * 10.0, srcx="FUN_140265470: starts s64 x 10 u above the aim point")
        fld("back", s65 * 10.0, srcx="FUN_140265470: and s65 x 10 u back toward the caster")
        fld("speed", s66 * U, srcx="FUN_140265470: speed s66 / 10.8")
        fld("steer", s67 * U, srcx="FUN_1402657f0: steering s67 / 10.8 x homing %")
    elif kind == "Ricochet":
        fld("speed", s65 * U, srcx="FUN_14026af40: speed ramp s65 -> s66 over s67")
        fld("end_speed", s66 * U)
        fld("ramp", s67, 0)
        fld("life", float(prog_life(s64, s65, s66, s67)), 0, srcx="FUN_14026af40: life s64 ticks")
        fld("yaw", math.radians(s68), 1e-4, srcx="FUN_14026af40: yaw s68 + k x s71")
        fld("roll", math.radians(s69), 1e-4, srcx="FUN_14026af40: roll s69 + k x s72")
        fld("yaw_step", math.radians(h[1]), 1e-4)
        fld("roll_step", math.radians(h[2]), 1e-4)
        fld("pattern", h[0], 0, srcx="s70 = shot count (hidden)")
        fld("bounces", h[3], 0, srcx="s73 = bounce count (hidden)")
    elif kind == "Tornado":
        fld("life", float(s64), 0, srcx="FUN_140268590: life s64 ticks")
        fld("radius", s65 * 0.1, 0.01, srcx="FUN_140268590: column radius s65 x 0.1")
        fld("height", s66 * 10.0 - 2.0, 0.01, srcx="FUN_140268590: column height s66 x 10 - 2")
        fld("speed", s67 * U, srcx="FUN_140268590: speed ramp s67 -> s68 over s69")
        fld("end_speed", s68 * U)
        fld("ramp", s69, 0)

    # ---- measured: count, spawn, speeds, life ------------------------------------------------------
    n_prog = P.shot_count(rec)
    n_ours = free.get("count_at_fire") if free.get("fired") else None
    cells.append(cmp("flight", "count", n_prog, n_ours, "FUN_14018a310: one object per SHOTnn marker of the cast effect (else one)"))
    sp_prog, sp_src = P.spawn_point(rec)
    if shots0 and sp_prog is not None and kind in ("Shell", "Parabola", "Arc", "Ricochet"):
        ours_sp = shots0[0]["pos"]
        d = vlen(vsub(sp_prog, ours_sp))
        cells.append(Cell("flight", "spawn", OK if d <= 0.05 else BAD, [round(x, 2) for x in sp_prog], ours_sp, sp_src, "FUN_14018a310 / FUN_1402626d0"))
    elif kind in ("Crawl", "Tornado") and shots0 and sp_prog is not None:
        ours_sp = shots0[0]["pos"]
        d = math.hypot(sp_prog[0] - ours_sp[0], sp_prog[2] - ours_sp[2])
        cells.append(Cell("flight", "spawn_xz", OK if d <= 0.05 else BAD, [round(sp_prog[0], 2), round(sp_prog[2], 2)], [ours_sp[0], ours_sp[2]], sp_src + " (floor families: the marker's ground point)", "FUN_140266060 / FUN_140268590"))
    if tracks and shots0 and kind in ("Shell", "Crawl", "Ricochet", "Tornado"):
        pts, steps = track_steps(tracks[0], shots0[0]["pos"])
        v0, v1, ramp_t = s65 * U, s66 * U, s67
        if kind == "Tornado":
            v0, v1, ramp_t = s67 * U, s68 * U, s69
        exp = [prog_ramp(v0, v1, ramp_t, k) for k in range(len(steps))]
        # shells that stop on the floor / ground end early: compare the steps that exist
        if steps:
            dmax = max(abs(a - b) for a, b in zip(exp, steps))
            cells.append(Cell("flight", "speed_profile", OK if dmax <= 0.02 else BAD, [round(exp[0], 3), round(exp[-1], 3)], [round(steps[0], 3), round(steps[-1], 3)],
                              "per-tick step length vs s65 -> s66 over s67 (max diff %.3f u/tick)" % dmax, "FUN_140264710 (shell), FUN_140266290 (crawl)"))
    if tracks and kind == "Shell" and shots0:
        life = prog_life(s64, s65, s66, s67)
        pts, steps = track_steps(tracks[0], shots0[0]["pos"])
        cells.append(cmp("flight", "life_ticks", life, shots0[0]["life"], "FUN_1402636f0 / FUN_14026e720", tol=0, note="program ticks the object lives"))
        # What it covers in that life (a ramped shot can cover more than its nominal range: FUN_1402636f0 ends it at the ramp's end).
        exp_path = sum(prog_ramp(s65 * U, s66 * U, s67, k) for k in range(life))
        tot = sum(steps)
        if len(steps) >= life - 1:
            cells.append(cmp("flight", "path_length", round(exp_path, 1), round(tot, 1), "FUN_140264710 x FUN_1402636f0: the speeds summed over its life", tol=(max(steps) * 1.2 + 0.5) if steps else 1.0))
        else:
            cells.append(unk("flight", "path_length", "the free-flight shot stopped after %d of %d ticks (it met the floor / a wall): path not compared" % (len(steps), life), None, None, "FUN_140264710"))
        # homing: heading change per tick toward a target pulled 40 deg off, within the 60 deg cone
        trk = (steer.get("tracks") or [None])[0]
        if trk and len(trk["pts"]) >= 3:
            ph = trk["pts"]
            hs = []
            for i in range(2, len(ph)):
                a, b = vsub(ph[i - 1], ph[i - 2]), vsub(ph[i], ph[i - 1])
                if vlen(a) > 1e-6 and vlen(b) > 1e-6:
                    hs.append(angle_between(a, b))
            maxturn = max(hs) if hs else 0.0
            want = s68 * DEG_PER_POINT
            cells.append(cmp("flight", "homing_deg_per_tick", round(want, 3), round(maxturn, 3), "FUN_140264710: turns at most s68 x 0.1 deg per tick toward a target within 60 deg", tol=0.12))
    if tracks and kind == "Arc" and shots0 and len(tracks) >= 1:
        cells.extend(_arc_cells(P, rec, free, tracks, shots0, s64, s65, s66, s67, s68, s69, h, src))
    if tracks and kind == "Parabola" and shots0:
        pts, steps = track_steps(tracks[0], shots0[0]["pos"])
        sp = s64 if s64 != 0 else s65
        horiz = [math.hypot(pts[i][0] - pts[i - 1][0], pts[i][2] - pts[i - 1][2]) for i in range(1, len(pts))]
        if horiz:
            cells.append(cmp("flight", "horizontal_speed", round(sp * U, 3), round(horiz[0], 3), "FUN_140264b00", tol=0.02))
        # arc height above the straight line from the spawn to the aim point (free flight: 190 / 300 u ahead at spawn height)
        s44 = r[44]
        D, _ = prog_band(s44, 1, None)
        y0 = shots0[0]["pos"][1]
        apex = max(p[1] for p in pts) - y0
        # FUN_140264b00: T = flat distance / speed, g = 8 h / T^2, vy0 = g T / 2 + dy / T;
        # FUN_140265080 integrates y += vy then vy -= g once per tick, so the sampled apex sits a little
        # above h (by about 2 h / T). The aim point is level with the spawn (no target).
        T = max(D / (sp * U), 1.0)
        g = 8.0 * s66 * 10.0 / (T * T)
        vy = g * T / 2.0
        y, best = 0.0, 0.0
        for _ in range(int(T) + 3):
            y += vy
            vy -= g
            best = max(best, y)
        cells.append(cmp("flight", "arc_height", round(best, 1), round(apex, 1), "FUN_140264b00 (g = 8 h / T^2) + FUN_140265080 (y += vy; vy -= g)", tol=1.0, note="apex over the spawn height in free flight to the no-target aim point %.0f u ahead (nominal h = %.0f)" % (D, s66 * 10.0)))
    if tracks and kind == "Fall" and shots0:
        s44 = r[44]
        D, _ = prog_band(s44, 2, None)
        p0 = shots0[0]["pos"]
        # aim point = spawn + 190 ahead (no target, same height as the spawn)
        spawn_h = (P.spawn_point(rec)[0] or [0, 12, 0])
        want = [spawn_h[0], spawn_h[1] + s64 * 10.0, spawn_h[2] + D - s65 * 10.0]
        d = vlen(vsub(want, p0))
        cells.append(Cell("flight", "start_offset", OK if d <= 1.0 else BAD, [round(x, 1) for x in want], p0, "appears s64 x 10 u above and s65 x 10 u back toward the caster from the aim point", "FUN_140265470"))
        pts, steps = track_steps(tracks[0], p0)
        if steps:
            cells.append(cmp("flight", "speed", round(s66 * U, 3), round(steps[0], 3), "FUN_140265470: s66 / 10.8", tol=0.02))
    return cells


def _arc_cells(P, rec, free, tracks, shots0, s64, s65, s66, s67, s68, s69, h, src):
    cells = []
    r = rec["raw"]
    s44 = r[44]
    D, _ = prog_band(s44, 6, None)
    spawn0 = shots0[0]["pos"]
    n_exp = P.shot_count(rec)
    pts, steps = track_steps(tracks[0], spawn0)
    v = s65 * U
    if steps:
        cells.append(cmp("flight", "speed", round(v, 3), round(steps[0], 3), "FUN_1402666b0", tol=0.02))
    bend = abs(s67)
    if v > 0:
        if bend > 0:
            length = D * math.radians(bend) / math.sin(math.radians(bend))
        else:
            length = D
        want_steps = int(math.ceil(length / v)) + 30
        cells.append(cmp("flight", "life_ticks", want_steps, shots0[0]["life"], "FUN_1402666b0 / FUN_140266bf0: curve ticks (R = D / 2 sin bend, arc 2 x bend) + 30 straight", tol=3,
                         note="D = %.0f u (no-target aim)" % D))
    # Plane of the arc: tilt s68 + k x s71 (hidden field s71 = per-shot step, s69 = shot count): the first
    # heading of each shot leaves the line of fire in the direction of its plane (lateral angle = tilt).
    def fold(t):
        t = t % 180.0
        return min(t, 180.0 - t)

    got_fold = []
    for sh in shots0:
        hx, hy, hz = sh["heading"]
        if math.hypot(hx, hy) > 1e-6:
            got_fold.append(round(fold(math.degrees(math.atan2(hy, hx))), 1))
    exp_fold = sorted(round(fold(s68 + k * h[1]), 1) for k in range(n_exp))
    got_fold = sorted(got_fold)
    if exp_fold and got_fold:
        ok = len(exp_fold) == len(got_fold) and all(abs(x - y) <= 1.0 for x, y in zip(exp_fold, got_fold))
        cells.append(Cell("flight", "plane_tilt", OK if ok else BAD, exp_fold, got_fold, "angle of each shot's arc plane to the horizontal (folded: sign aside)", "FUN_1402666b0: roll s68 + k x s71"))
    cells.append(unk("flight", "swing_side", "which way the arc swings (left / right, up / down) is not settled: needs Laurence's footage (PROGRAM-RE.md, footage check 2)", None, None, "FUN_1402666b0"))
    return cells


# ---------------------------------------------------------------------------
# AREA (holes, rain, mines, around self)


def chk_area(P, rec, O):
    a = O.get("area")
    st = O["static"]
    if a is None or st["behaviour"] != "Place":
        return []
    cells = []
    r = rec["raw"]
    s64, s65, s66, s67, s68, s69 = params(rec)
    h = hidden(rec)
    d = st["detail"]
    kind = d["kind"]
    src = "motion %d handler" % r[63]
    cmpd = lambda k, want, srcx=src, tol=0.01, note="": cells.append(cmp("area", k, want, d.get(k), srcx, tol=tol, note=note))
    if kind == "Hole":
        cmpd("warn", s64, "FUN_14026a510: warning s64 ticks", 0)
        cmpd("active", s65, "FUN_14026a510: active s65 ticks", 0)
        cmpd("radius", s66 * 0.1, "FUN_14026a7f0: sphere s66 x 0.1 u")
        cmpd("ground_only", True, "FUN_14026a7f0: only players standing on the ground (+0x12e74)", 0)
    elif kind == "RainColumn":
        cmpd("warn", s64, "FUN_140266fb0: warning s64 ticks", 0)
        cmpd("active", s65, "FUN_140266fb0: active s65 ticks", 0)
        cmpd("height", s66 * 10.0, "FUN_140266fb0: column s66 x 10 u")
        cmpd("radius", s68 * 0.1, "FUN_140267410: radius s68 x 0.1 u")
    elif kind == "RainCloud":
        cmpd("warn", s64, "FUN_14026b480: first drop s64 ticks after the cast", 0)
        cmpd("active", s65, "FUN_14026b480: cloud lives s64 + s65", 0)
        cmpd("pulses", s66, "FUN_14026b480: s66 drops", 0)
        cmpd("every", s67 + 1, "FUN_14026b480: one drop every s67 + 1 ticks", 0)
        cmpd("cloud_height", s69 * 0.1, "FUN_14026b480: s69 x 0.1 above the target")
        cmpd("radius", h[0] * 0.1, "FUN_14026b8d0: drops start within s70 x 0.1 u (hidden field)")
        cmpd("follow", h[1] * U, "FUN_14026bbd0: the cloud follows at s71 / 10.8 per tick (hidden field)")
        cmpd("drop_speed", h[2] * U, "FUN_14026b8d0: drops fall at s72 / 10.8 per tick (hidden field)")
    elif kind == "Mine":
        trig = {0: "Timer", 1: "Sensor", 2: "Remote"}.get(s64, "?")
        trig_ours = (d.get("trigger") or "").split("{")[0].split(" ")[0]
        cells.append(cmp("area", "trigger", trig, trig_ours, "FUN_140267b90: s64 = 0 timer, 1 sensor, 2 remote"))
        if s64 in (0, 1):
            cmpd("warn", s66, "FUN_140267b90: s66 = fuse (timer) / arming time (sensor)", 0)
        cmpd("radius", s67 * 0.1, "FUN_140267e90: blast radius s67 x 0.1 u")
        if s64 == 1:
            m = re.search(r"radius: ([-0-9.eE]+)", d.get("trigger", ""))
            cells.append(cmp("area", "sensor_radius", s65 * 0.1, float(m.group(1)) if m else None, "FUN_140267e90: sensor radius s65 x 0.1 u", tol=0.01))
    elif kind == "Wave":
        cmpd("radius", s64 * 0.1, "FUN_140268050: blast radius s64 x 0.1 u")
    # Where it was set, free-aimed (nobody locked): the aim helper's default point ahead of the spawn.
    sp, sp_src = P.spawn_point(rec)
    sets = (a.get("set") or [])
    if kind in ("Hole", "RainColumn", "RainCloud") and sets and sp is not None:
        D, _ = prog_band(r[44], r[63], None)
        want = [sp[0], 0.0, sp[2] + D]
        why = "free-aimed: %.0f u ahead of the spawn point (%s)" % (D, sp_src)
        tol = 0.6
        if r[63] == 16:
            v = a.get("victim") or [0, 0, 0]
            want, why = [v[0], 0.0, v[2]], "motion 16 makes a hole under every player's feet (FUN_1402626d0 over all players)"
        elif r[63] == 20:
            tol = d.get("follow", 1.4) + 0.6
            why += "; the cloud has already moved one step toward its target"
        pos = sets[0]["pos"]
        dd = math.hypot(want[0] - pos[0], want[2] - pos[2])
        cells.append(Cell("area", "placement", OK if dd <= tol else BAD, [round(want[0], 2), round(want[2], 2)], [pos[0], pos[2]], why, "FUN_14026e410"))
    elif kind in ("Mine", "Wave") and sets or kind == "Wave":
        pass
    # Blast radius as it actually went off.
    be = a.get("blast_events") or []
    if kind in ("Mine", "Wave") and be:
        want = (s67 if kind == "Mine" else s64) * 0.1
        cells.append(cmp("area", "blast_radius", want, be[0][1], "FUN_14026dbd0: s67 x 0.1 (mines) / s64 x 0.1 (around self)", tol=0.01))
    # Timeline: first hit of a victim standing at the aim point.
    fh = a.get("first_hit_after_fire")
    if kind == "Hole" and fh is not None:
        cells.append(cmp("area", "first_hit_after_fire", SIM * s64, fh, "FUN_14026a7f0: active after the s64-tick warning", tol=3))
    if kind == "RainColumn" and fh is not None:
        cells.append(cmp("area", "first_hit_after_fire", SIM * s64, fh, "FUN_140267410: active after the s64-tick warning", tol=3))
    if kind == "Mine" and s64 == 0 and be:
        cells.append(cmp("area", "fuse_to_blast", SIM * s66, be[0][0], "FUN_140267e90: timer mine goes off after s66 ticks", tol=3))
    return cells


# ---------------------------------------------------------------------------
# MELEE


def chk_melee(P, rec, O):
    m = O.get("melee")
    st = O["static"]
    if m is None or st["behaviour"] != "Melee":
        return []
    r = rec["raw"]
    s64, s65, s66 = r[64], r[65], r[66]
    d = st["detail"]
    row = row_for(rec, False)
    cells = []
    src = "FUN_14026a020 / FUN_140269c60 / FUN_140265d60 (s63 = %d)" % r[63]
    cells.append(cmp("melee", "reach", s64 * 0.1, d.get("reach"), src + ": s64 x 0.1 (sphere radius, blade length or stamp radius)", tol=0.01))
    shape = {12: "Sword", 13: "Sphere", 14: "Stamp"}.get(r[63])
    cells.append(cmp("melee", "shape", shape, d.get("shape"), src))
    cells.append(cmp("melee", "dash_turn_deg", s66 / 10.0, d.get("dash_turn_deg"), "FUN_14018f0f0: s66 / 10 degrees per tick", tol=0.01))
    if not row:
        return cells
    dash0, dash1, fire, hit_end = row["dash_start"], row["dash_end"], row["fire_tick"], row["hit_window_end"]
    length = dash1 - dash0
    step = min(10.0 / length * s65, 10.0) if length > 0 else 0.0
    # Dash distance in the arena: opponent 60 u ahead, locked: it stops 11 u from the target's centre,
    # the last step shortened by (speed - (distance - 11)) (FUN_14025be40, "read as written").
    z, dist = 0.0, 60.0
    for _ in range(max(length, 0)):
        rem = dist - z
        if rem <= 11.0:
            break
        if rem - 11.0 >= step:
            z += step
        else:
            z += step - (rem - 11.0)
    free = m.get("free") or {}
    cells.append(cmp("melee", "dash_travel", round(z, 1), free.get("travel"), "FUN_14025be40 / FUN_14018f050: (10 / window) x s65 per tick, capped at 10; stops 11 u from the target", tol=0.6, note="measured with an untouchable target, so nothing knocks it away mid-dash"))
    if free.get("dash_first") is not None and length > 0:
        cells.append(cmp("melee", "dash_start_step", SIM * dash0, free["dash_first"], "action row w41 (dash starts)", tol=2))
    if m.get("hit_step") is not None and fire:
        if r[63] == 14:
            cells.append(cmp("melee", "hit_step", SIM * 19, m["hit_step"], "FUN_140265e00: a single test at tick 19", tol=2))
        else:
            lo, hi = SIM * fire, SIM * hit_end + 1
            got = m["hit_step"]
            cells.append(Cell("melee", "hit_window", OK if lo - 1 <= got <= hi else BAD, [lo, hi], got, "first hit inside the row's hit window w43 .. w44 (x 2 sim ticks)", "action row w43 / w44"))
    elif m.get("hit_step") is None:
        cells.append(Cell("melee", "hit_window", BAD, "hits at 60 u (dash closes to 11 u)", None, "no hit landed on a locked target 60 u away", src))
    return cells


# ---------------------------------------------------------------------------
# GUARD


def chk_guard(P, rec, O):
    cells = []
    st = O["static"]
    if st["behaviour"] != "Guard":
        return cells
    r = rec["raw"]
    d = st["detail"]
    src = "FUN_140264020 (init) / FUN_140264340 (update)"
    kind, s45, s46, s47, s48, s49, s50, s51, s52 = r[44], r[45], r[46], r[47], r[48], r[49], r[50], r[51], r[52]
    cells.append(cmp("guard", "kind", kind, d["kind"], src + ": s44"))
    cells.append(cmp("guard", "size", s45 * 0.1, d["field"], src + ": size = s45 x 0.1 u", tol=0.01))
    life = SIM * max(s46 - 1, 0)
    if kind == 7:
        if not O.get("auto"):
            cells.append(unk("guard", "time", "the automatic Shelter (s44 = 7) was not probed through an incoming attack", life, d["ticks"], src))
    else:
        cells.append(cmp("guard", "time", life, d["ticks"], src + ": stands s46 - 1 program ticks", tol=0))
    cells.append(cmp("guard", "def", s47, d["def"], "FUN_140272400: DEF = s47 + s48 x uses"))
    cells.append(cmp("guard", "anti_pierce", s49 == 1, d["anti_pierce"], "FUN_140271760: s49 = 1"))
    cells.append(cmp("guard", "short_range_only", s50 == 1, d["short_range_only"], "FUN_140271ed0: s50 = 1 -> Add only for short-range attacks"))
    cells.append(cmp("guard", "add", [s51, s52], [d["add"], d["add_amount"]], "FUN_140271ed0: Add effect s51 / s52"))
    # Contact geometry: front kinds (barrier 0, brush 3, reflect 4) are a disc in front; the rest all round (sphere).
    all_round = kind in (1, 2, 5, 6, 7)
    if kind in (2, 6):
        cells.append(unk("guard", "all_round", "Course / About Face are spheres in the program; ours turns their shots in move_projectiles before arrival and reports all_round() = false (a representation difference, not compared)", all_round, d["all_round"], "FUN_14026dcb0"))
    else:
        cells.append(cmp("guard", "all_round", all_round, d["all_round"], "FUN_14026dcb0: disc for kinds 0 / 3 / 4, sphere for 1 / 5 / 7"))
    g = O.get("guard") or {}
    if g and g.get("def_after_two_uses") is not None:
        want = s47 + s48 * 2
        cells.append(cmp("guard", "def_per_use", want, g["def_after_two_uses"], "FUN_140272400: DEF = s47 + s48 x uses of the slot"))
    if g and g.get("up_for_after_fire") is not None and kind != 7:
        cells.append(cmp("guard", "stood_for", life, g["up_for_after_fire"], src, tol=1, note="measured: ticks the guard stood after the fire tick"))
    return cells


# ---------------------------------------------------------------------------
# ENVIRONMENT


def chk_env(P, rec, O):
    e = O.get("env")
    if e is None:
        return []
    r = rec["raw"]
    src = "FUN_14017bcc0 (set), FUN_14017be00 (capture), FUN_140271660 (status read)"
    s44, s45, s46, s47 = r[44], r[45], r[46], r[47]
    cells = []
    sts = {int(k): v for k, v in (e.get("statuses") or {}).items()}
    want = {s44: s46} if s46 != 0 else {}
    cells.append(cmp("env", "status", want, sts, src + ": status id s44 holds amount s46 for everyone"))
    cells[-1].st = OK if want == sts else BAD
    cells.append(cmp("env", "capture_lock", SIM * s45, e.get("capture_lock"), src + ": cannot be captured until s45 program ticks have passed", tol=0))
    life = None if s47 <= 0 else SIM * s47
    cells.append(cmp("env", "lifetime", "until captured" if life is None else life, "until captured" if e.get("lifetime") is None else e["lifetime"], src + ": expiry = now + s47 when s47 > 0", tol=0))
    if life is not None and e.get("ended_after") is not None:
        cells.append(cmp("env", "ended_after", life, e["ended_after"], src, tol=2, note="measured: the crystal expired"))
    return cells


# ---------------------------------------------------------------------------
# EFFECT (s43 = 4 self / target / everyone effects, kinds from FUN_140270b00)


def _stat_mod(snap, stat):
    return [(m["amount"], m["ticks"]) for m in snap["mods"] if m["stat"] == stat]


def chk_effect(P, rec, O):
    e = O.get("effect")
    r = rec["raw"]
    s43 = r[43]
    cells = []
    if passive_kind(rec):
        return []  # works through an incoming attack: `guard.orb_*`
    if rec.get("synthetic"):
        # Aura Particle: record 0, kind 1 (Level +), amount 1.
        if not e:
            return []
        b, a = e["before"][0], e["at_fire"][0]
        cells.append(cmp("effect", "level_plus_1", min(99, b["level"] + 1), a["level"], "FUN_140270b00 kind 1 (Level +); FUN_14024d970 clamps to the rule range"))
        cells.append(Cell("effect", "no_other_change", OK if (a["health"] == b["health"] and a["aura_ticks"] <= b["aura_ticks"] + 0) else BAD, "no Aura / Health change", "aura %d->%d health %d->%d" % (b["aura"], a["aura"], b["health"], a["health"]), "", "FUN_140270b00"))
        if e.get("sfx_ids") is not None:
            cells.append(Cell("snd", "use", OK if sorted(e["sfx_ids"]) == [89] else BAD, [89], sorted(e["sfx_ids"]),
                              "MEASURED live (moverec, MOVE-RECORDER.md): only common/89 goes through the sound routine at the use, no 90; READ: FUN_140271590 plays 89 for a Level rise. Ours is compared as it plays now (sfx.rs: the Level +1 effect's 89, nothing for AuraParticleUsed)", "0x140276330 trace; FUN_140271590"))
        return cells
    if e is None or s43 == 0:
        return cells
    if "probe_error" in e:
        return [gap("effect", "run", e["probe_error"])]
    if not e.get("started"):
        return [gap("effect", "run", "the probe's press was refused in the arena")]
    src = "FUN_140270b00 (kinds), FUN_140271590 (apply)"
    if s43 == 4:
        reach = r[44]  # 0 caster, 1 target, 2 everyone
        who = {0: [0], 1: [1], 2: [0, 1]}.get(reach, [0])
        for k in range(4):
            kind, amt, dur = r[45 + k], r[49 + k], r[53 + k]
            if kind == 0:
                continue
            for w in who:
                b, a = e["before"][w], e["at_fire"][w]
                tag = "k%d" % (k + 1)
                nm = "p%d" % (w + 1)
                if kind == 1:
                    cells.append(cmp("effect", "%s.level.%s" % (tag, nm), min(99, b["level"] + amt), a["level"], src + " kind 1: Level + s49"))
                elif kind == 2:
                    want = min(b["level"], b["aura"] + amt) if amt < 99 else b["level"]
                    cells.append(cmp("effect", "%s.aura.%s" % (tag, nm), want, a["aura"], src + " kind 2: Aura + amount (capped at Level)"))
                elif kind == 3:
                    cells.append(cmp("effect", "%s.health.%s" % (tag, nm), min(20, b["health"] + amt), a["health"], src + " kind 3: Health + amount (to the maximum)"))
                elif kind in (4, 5, 6, 7):
                    stat = {4: "Attack", 5: "Speed", 6: "Homing", 7: "AuraRate"}[kind]
                    got = _stat_mod(a, stat)
                    cap = 9 if stat == "Attack" else 90
                    if dur == 0:
                        want = [(max(-cap, min(cap, amt)), None)]
                    else:
                        want = [(amt, SIM * dur)]
                    ok = bool(got) and got[0][0] == want[0][0] and ((want[0][1] is None and got[0][1] is None) or (want[0][1] is not None and got[0][1] is not None and abs(got[0][1] - want[0][1]) <= 2))
                    cells.append(Cell("effect", "%s.status_%s.%s" % (tag, stat.lower(), nm), OK if ok else BAD, want, got, "status %d: %s %+d%s" % (kind - 4, stat, amt, " for %d ticks" % dur if dur else " (permanent)"), src + " kinds 4-7 (FUN_14024d5c0)"))
                elif kind == 11:
                    want = 91 * SIM if False else 91 * SIM
                    cells.append(cmp("effect", "%s.regenerate.%s" % (tag, nm), want, a["regen_every"], "FUN_14025b5e0: +1 Health every 91 program ticks while standing (state 21)", note="amount s49 = %d" % amt))
                elif kind == 12:
                    cells.append(cmp("effect", "%s.invisible.%s" % (tag, nm), SIM * dur, a["invisible"], src + " kind 12: status 8 for s53 ticks", tol=2))
                elif kind == 13:
                    cells.append(cmp("effect", "%s.invisible_float.%s" % (tag, nm), SIM * dur, a["invisible"], src + " kind 13: invisible + float + jump", tol=2))
                elif kind == 14:
                    cells.append(cmp("effect", "%s.void.%s" % (tag, nm), SIM * dur, a["immune"], src + " kind 14: status 33 (no damage)", tol=2))
                elif kind == 15:
                    cells.append(cmp("effect", "%s.invulnerable.%s" % (tag, nm), True, a["still_immune"], src + " kind 15: state 22, no damage while still"))
                elif kind == 10:
                    cells.append(cmp("effect", "%s.level_double.%s" % (tag, nm), min(99, b["level"] * 2), a["level"], src + " kind 10: Level doubled"))
                elif kind == 9:
                    cells.append(unk("effect", "%s.aura_times.%s" % (tag, nm), "kind 9 (Aura x amount): no skill of the roster uses it", None, None, src))
                elif kind == 8:
                    cleared = not a["mods"] and not a["bound"] and not a["frozen"] and not a["poison"] and not a["invisible"] and not a["immune"]
                    cells.append(Cell("effect", "%s.reset.%s" % (tag, nm), OK if cleared else BAD, "every status slot cleared (FUN_14020d220: all 47)", {"mods": a["mods"], "bound": a["bound"]}, "", src + " kind 8"))
                elif kind == 18:
                    # the CASTER takes the target's four attribute statuses (FUN_14024d650 on the caster, read from the target)
                    tgt = [(m["stat"], m["amount"], m["ticks"] is not None) for m in e["before"][1]["mods"]]
                    got = [(m["stat"], m["amount"], m["ticks"] is not None) for m in e["at_fire"][0]["mods"]]
                    cells.append(Cell("effect", "%s.copy_statuses" % tag, OK if sorted(tgt) == sorted(got) else BAD, sorted(tgt), sorted(got), "the caster takes the target's four attribute statuses (value and timed amount)", src + " kind 18"))
                elif kind == 19:
                    want_slot = e["before"][0]["used"][-1] if e["before"][0]["used"] else None
                    cells.append(cmp("effect", "%s.relearn.%s" % (tag, nm), want_slot, a["slots"][0], src + " kind 19: the last consumed skill is captured onto this button"))
                elif kind == 20:
                    want = None
                    cells.append(unk("effect", "%s.pact.%s" % (tag, nm), "Mephisto's Pact (kind 20): READ that FUN_140270b00 case 0x14 records the whole undealt deck (FUN_1401582f0 is the statistics recorder, not a draw) and enters player state 24; the pick itself is that state's handler (unread): which card and which button are not settled; ours pops the deck's next card onto the first empty button", None, a["slots"], src + " kind 20"))
                elif kind == 21:
                    used = e["before"][0]["used"]
                    want_back = min(amt, len(used))
                    cells.append(cmp("effect", "%s.recall.%s" % (tag, nm), want_back, a["arsenal"] - e["before"][0]["arsenal"], src + " kind 21: up to s49 used skills go back into the deck at random places", note="whether Recall itself can be one of them (it is consumed at the fire tick too) is not read"))
                elif kind == 17:
                    # FUN_140270b00 case 0x11 only sets +0x6374 / +0x6378 / +0x637c and status 0x25; the one reader (FUN_140187180) is the
                    # online team-chat routing (DAT_1404bbc48 + 0xa8): no Level / Health / Aura / deck / status change at all.
                    keep = ["level", "health", "mods", "bound", "frozen", "poison", "arsenal"]
                    # the caster's Aura pays the skill's cost and its own button is used up (the pay channel's business)
                    same = all(e["before"][w][k] == e["at_fire"][w][k] for k in keep) and e["before"][w]["slots"][1:] == e["at_fire"][w]["slots"][1:]
                    same = same and all(e["before"][1][k] == e["at_fire"][1][k] for k in keep + ["slots"])  # (the Aura refills while the cast runs)
                    cells.append(Cell("effect", "%s.kind17.%s" % (tag, nm), OK if same else BAD, "no sim change (a voice-chat listening flag)", "unchanged" if same else "changed",
                                      "READ: kind 17 sets only the listen-in flag (status 0x25, read by FUN_140187180 for the team chat)", src + " kind 17 (case 0x11)"))
                elif kind == 22:
                    # FUN_140270b00 case 0x16: while the undealt deck holds an Aura Particle (0), move the deal pointer past it (shifting
                    # the cards in front of it up by one): every Aura Particle leaves the undealt deck, the rest keep their order.
                    deck = e.get("arsenal_before") or []
                    want = [x for x in deck if x != 0]
                    got = e.get("arsenal_at_fire") or []
                    cells.append(Cell("effect", "%s.kind22.%s" % (tag, nm), OK if want == got else BAD, want, got, "Bravery Decision: all Aura Particles leave the undealt deck, the others keep their order", src + " kind 22 (case 0x16)"))
                else:
                    cells.append(gap("effect", "%s.kind%d.%s" % (tag, kind, nm), "effect kind %d has no expectation written" % kind))
    elif s43 in (1, 3, 6):
        pass  # read on the movement / capsules channels
    return cells


# ---------------------------------------------------------------------------
# CAPSULE FIELD (s43 = 1): FUN_14026fcb0 -> FUN_14026ff20 (per capsule) -> FUN_140270150 (filter)
#   s44 0 the touched capsule (any kind), 1 the touched one if it is an Aura Particle, 2 every Aura Particle,
#   3 the touched one if it is a skill capsule, 4 every capsule of the other team (both kinds), 5 every capsule
#   of everyone (both kinds: the filter lets Aura Particles through for s44 = 4 and 5). Nothing touched: the
#   0 / 1 / 3 kinds do nothing.
#   s45 0 erase (FUN_14014fd10), 1 capture onto the cast button (FUN_140261a00, then erase), 2 swap for a random
#   undrawn card of the owner's deck (FUN_14014fa20), 3 turn into an Aura Particle.


def capsule_targets(s44, caps, touched_id, caster):
    def aura(c):
        return c[2] == 0

    out = []
    for c in caps:
        cid, owner, skill = c
        if s44 == 0:
            ok = cid == touched_id
        elif s44 == 1:
            ok = cid == touched_id and aura(c)
        elif s44 == 2:
            ok = aura(c)
        elif s44 == 3:
            ok = cid == touched_id and not aura(c)
        elif s44 == 4:
            ok = owner != caster
        elif s44 == 5:
            ok = True
        else:
            ok = False
        if ok:
            out.append(c)
    return out


def chk_capsules(P, rec, O):
    scen = O.get("capsules")
    if not scen or rec["raw"][43] != 1:
        return []
    r = rec["raw"]
    s44, s45 = r[44], r[45]
    cells = []
    src = "FUN_14026fcb0 / FUN_14026ff20 / FUN_140270150"
    for sc in scen:
        if "probe_error" in sc:
            return [gap("capsules", "run", sc["probe_error"])]
        tag = "touching_skill" if sc["touched_skill"] else "touching_aura"
        if not sc.get("started"):
            cells.append(gap("capsules", tag, "the probe's cast was refused"))
            continue
        before = [tuple(c) for c in sc["before"]]
        after = {c[0]: tuple(c) for c in sc["after"]}
        hit = capsule_targets(s44, before, 1, 0)
        want = {c[0]: c for c in before}
        for cid, owner, skill in hit:
            if s45 == 0:
                want.pop(cid, None)
            elif s45 == 1:
                want.pop(cid, None)
            elif s45 == 3:
                want[cid] = (cid, owner, 0)
        got_ids = set(after)
        if s45 == 2:
            # FUN_14014fa20: each ready capsule keeps its place and takes a random undrawn card of its owner's deck; its old
            # skill goes into the deck at that card's place, and the respawn draw of FUN_14014fd10 (mode 0, inside it) takes
            # the deck's next card, so each swap shrinks the owner's undrawn pool by one. The program's own pick can repeat a
            # skill (a pool card, or one an earlier swap put back, can equal the capsule's), so what is checked is the
            # conservation: every swapped capsule stays and now holds a card its owner could draw; per owner the cards left
            # (pool + capsules) are the cards there were minus one per swap; and something actually changed.
            ab, aa = sc.get("arsenal_before"), sc.get("arsenal_after")
            ok = got_ids == set(want)
            if ab is None or aa is None:
                ok = ok and all(after[c[0]][2] != c[2] for c in hit if c[0] in after)
            else:
                import collections as _co
                for owner in (0, 1):
                    mine = [c for c in hit if c[1] == owner]
                    avail = _co.Counter(ab[owner]) + _co.Counter(c[2] for c in mine)
                    left = _co.Counter(aa[owner]) + _co.Counter(after[c[0]][2] for c in mine if c[0] in after)
                    ok = ok and not (left - avail) and sum(left.values()) == sum(avail.values()) - len(mine)
                ok = ok and any(after[c[0]][2] != c[2] for c in hit if c[0] in after)
            exp = "targets %s keep their place, each with a card of its owner's deck (the owner's undrawn pool shrinks by one per swap)" % [c[0] for c in hit]
        else:
            ok = {k: v for k, v in after.items()} == want
            exp = "capsules left: %s" % [(c[0], c[2]) for c in want.values()]
        if s45 == 1 and hit:
            first = hit[-1] if s44 in (2, 4, 5) else hit[0]
            slot_ok = sc["slots"][0] == first[2]
            ok = ok and slot_ok
            exp += "; the cast button takes skill %s" % first[2]
        cells.append(Cell("capsules", tag, OK if ok else BAD, exp, "capsules left: %s; button %s" % ([(c[0], c[2]) for c in sc["after"]], sc["slots"][0]), "s44 = %d, s45 = %d" % (s44, s45), src))
    return cells


# ---------------------------------------------------------------------------
# MOVEMENT specials (s43 = 3): FUN_140270200 on s44


def chk_movement(P, rec, O):
    m = O.get("movement")
    r = rec["raw"]
    if m is None or r[43] != 3:
        return []
    if "probe_error" in m:
        return [gap("movement", "run", m["probe_error"])]
    s44, s45, s46 = r[44], r[45], r[46]
    src = "FUN_140270200 (s44 = %d)" % s44
    cells = []
    track = m.get("track") or []
    ys = [(t[0], t[1][1]) for t in track]

    def rise_ticks(frac=0.95):
        top = max((y for _, y in ys), default=0.0)
        if top <= 0:
            return None
        return next((t for t, y in ys if y >= frac * top), None)

    if s44 == 0:
        apex = 10.0 * s45
        cells.append(cmp("movement", "apex", apex, round(m["max_y"], 1), src + ": height h = s45 -> apex 10 h u (gravity 0.12 u/tick^2)", tol=max(3.0, apex * 0.03)))
        first = next((t for t, y in ys if y > 0.5), None)
        cells.append(cmp("movement", "windup_ticks", SIM * 13, first, "FUN_140257b90: JUMP_START plays first (clip length - 2 = 13 program ticks)", tol=4))
    elif s44 == 8:
        dist = 10.0 * s45
        got = math.hypot(m["end_rel"][0], m["end_rel"][2])
        cells.append(cmp("movement", "distance", dist, round(got, 1), src + ": s45 x 10 u", tol=1.5))
        row = row_for(rec)
        span = SIM * (row["dash_end"] - row["dash_start"]) if row else 30
        t95 = next((t for t, _ in [(t[0], 0) for t in track if math.hypot(t[1][0], t[1][2]) >= 0.95 * got]), None)
        cells.append(cmp("movement", "dash_ticks", span, t95, src + ": covered over the row's dash window (w42 - w41 program ticks)", tol=4, note="ticks until 95 % of the distance is covered"))
    elif s44 == 5:
        cells.append(cmp("movement", "float_ticks", SIM * s46, m.get("max_hover"), src + ": status 11 (float) for s46 ticks", tol=2))
        apex = 10.0 * s45
        cells.append(cmp("movement", "apex", apex, round(m["max_y"], 1), src + ": then the jump of height s45 (apex 10 s45 u), floating at its top", tol=max(3.0, apex * 0.03)))
        t95 = rise_ticks()
        need = int(math.sqrt(2.0 * apex / 0.12)) * SIM
        cells.append(Cell("movement", "rise_ticks", OK if (t95 is not None and t95 >= need - 4) else BAD, ">= %d (a jump under 0.12 u/tick^2)" % need, t95, "ticks until 95 % of the height is reached", src))
    elif s44 == 6:
        end, spawn = m["end"], m["spawn"]
        cells.append(Cell("movement", "return_to_start", OK if vlen(vsub(end, spawn)) < 1.0 else BAD, spawn, end, "", src))
    elif s44 == 2:
        end = m["end"]
        want = [60.0, 0.0, -120.0]
        cells.append(Cell("movement", "to_own_capsule_spot", OK if vlen(vsub(end, want)) < 1.0 else BAD, want, end, "warp to one of your own capsule spots", src))
    elif s44 == 4:
        cells.append(unk("movement", "capsule_spot", "Call stores your position 10 u up as the capsule spot (FUN_140270200); ours stores the position itself: which frame the +10 is in is not read", None, m.get("capsule_anchor"), src))
    elif s44 == 7:
        cells.append(unk("movement", "warp_target", "warps next to a target (FUN_140257e90 not read); ours lands on the target's start point", None, m["end"], src))
    return cells


# ---------------------------------------------------------------------------
# ADD effects of attacks (s51 / s52) and erase-on-hit (s48-s50), through the hit probe


def _slot_colour(P, sid):
    if sid is None:
        return None
    if sid == 0:
        return "aura"
    rec = P.rec.get(sid)
    return CAPSULE_COLOUR.get(rec["capsule"]) if rec else None


def erase_expect(P, rec, hand, pressed_slot=0):
    """FUN_140272510 (+ 140273a80 / 140273b80 / 1402739c0): which of the opponent's buttons an
    equipped-skill erase takes. Returns (candidates, count, ordered_or_None, ambiguous_note)."""
    r = rec["raw"]
    s48, s49, s50 = r[48], r[49], r[50]
    cand = []
    amb = None
    for k, sid in enumerate(hand):
        if sid is None:
            continue
        col = _slot_colour(P, sid)
        isaura = sid == 0
        if s48 in (1, 2, 3, 4, 5):
            if isaura:
                amb = "Aura Particle's colour in the program is not read"
                continue
            if col == s48:
                cand.append(k)
        elif s48 == 6:
            if isaura:
                cand.append(k)
        elif s48 == 7:
            if not isaura:
                cand.append(k)
        elif s48 == 8:
            if isaura:
                amb = "whether colours >= erase include the Aura Particle is not read"
            elif col is not None and col >= 3:
                cand.append(k)
        elif s48 == 9:
            cand.append(k)
    cost = lambda k: (P.rec[hand[k]]["raw"][22] if hand[k] else 0)
    ordered = None
    if s49 == 1:
        ordered = sorted(cand, key=lambda k: -cost(k))
        ties = len({cost(k) for k in cand}) < len(cand)
        if ties:
            ordered = None
    elif s49 == 2:
        ordered = sorted(cand, key=lambda k: cost(k))
        ties = len({cost(k) for k in cand}) < len(cand)
        if ties:
            ordered = None
    elif s49 == 3:
        cand = [k for k in cand if (P.rec[hand[k]]["raw"][26] == 0 if hand[k] else False)]
    elif s49 == 4:
        cand = [k for k in cand if k == pressed_slot]
    count = min(s50, len(cand)) if s50 else len(cand)
    return cand, count, ordered, amb


def chk_hit_effects(P, rec, O):
    cells = []
    r = rec["raw"]
    hit = O.get("hit")
    if not hit:
        return cells
    if "probe_error" in hit:
        return [gap("hit", "run", hit["probe_error"])]
    fh = hit.get("first_hit_step")
    b0, b1 = hit["before"]
    a0, a1 = hit["after"]
    code, amt = r[51], r[52]
    if rec["capsule"] == "attack" and code in (1, 5, 7, 8, 9, 11, 18):
        src = "FUN_140272bc0 (attack Add, code %d)" % code
        if fh is None:
            cells.append(gap("add", "code%d" % code, "the probe's attack never hit"))
        elif code == 1:
            cells.append(cmp("add", "aura_to_zero", 0, a1["aura"], src + ": Aura to 0"))
        elif code == 5:
            bl = hit.get("blasts") or []
            cells.append(cmp("add", "blast_radius", amt * 0.1, bl[0][1] if bl else None, "FUN_14026cd70 (impact code with s51 = 5): radius s52 x 0.1", tol=0.01))
        elif code == 7:
            elapsed = SIM * amt - a1["frozen"]
            cells.append(Cell("add", "freeze_buttons", OK if 25 <= elapsed <= 36 else BAD, "%d ticks frozen" % (SIM * amt), "%d left after ~30 ticks" % a1["frozen"], "", src))
        elif code == 8:
            elapsed = SIM * amt - a1["poison"]
            cells.append(Cell("add", "poison_buttons", OK if 25 <= elapsed <= 36 else BAD, "%d ticks poisoned" % (SIM * amt), "%d left after ~30 ticks" % a1["poison"], "", src))
        elif code == 9:
            dmg = b1["health"] - a1["health"]
            cells.append(cmp("add", "health_from_damage", b0["health"] + dmg if b0["health"] + dmg <= 20 else 20, a0["health"], src + ": Health + damage dealt"))
        elif code == 11:
            dmg = b1["health"] - a1["health"]
            cells.append(cmp("add", "level_drain", [b0["level"] + dmg, b1["level"] - dmg], [a0["level"], a1["level"]], src + ": caster's Level + damage, target's - the same"))
        elif code == 18:
            cells.append(cmp("add", "auto_capture", True, a0["slots"][0] is not None, "FUN_140263460: the next skill goes straight onto the button"))
    if rec["capsule"] in ("erase",) or (rec["capsule"] == "attack" and r[48] and r[50]):
        # Erase-equipped: try both hands.
        for key, hand in (("hit", [65, 80, 26, 0]), ("hit_b", [179, 36, 239, 65])):
            h = O.get(key)
            if not h or "probe_error" in h or h.get("first_hit_step") is None:
                continue
            hand_ids = [None if x is None else x for x in hand]
            P_hand = [x for x in hand_ids]
            cand, count, ordered, amb = erase_expect(P, rec, P_hand)
            before = h["before"][1]["slots"]
            after = h["after"][1]["slots"]
            erased = [k for k in range(4) if before[k] is not None and after[k] != before[k]]
            ok_cand = all(k in cand for k in erased)
            ok_n = len(erased) == count
            ok_ord = True if ordered is None else erased == ordered[:count]
            st_ = OK if (ok_cand and ok_n and ok_ord) else BAD
            note = "hand %s: erased slots %s; expected %d of %s%s" % (hand, erased, count, cand, (" in order %s" % ordered[:count]) if ordered else "")
            c = Cell("erase", "equipped_%s" % ("A" if key == "hit" else "B"), st_, {"count": count, "from": cand, "order": ordered[:count] if ordered else None}, erased, note, "FUN_140272510: s48 filter, s49 order, s50 count")
            if amb and st_ == BAD:
                c.st, c.note = UNK, c.note + " [" + amb + "]"
            cells.append(c)
    return cells


# ---------------------------------------------------------------------------
# Add effects of erase / status / special skills that travel or are instant (s43 = 0, s51 != 0):
# FUN_140272bc0 (the switch on s51, disassembled / decompiled in this lane), the status slots
# FUN_14024d5c0 / FUN_14024d650 (a status keeps one permanent value and ONE timed amount with its timer),
# and for motion 4 (instants) the s65 delay after the fire tick.
#   1 Aura -= s52 (clamped at 0)        2 Level -= s52         3 Aura x / ÷ s52       4 Level x -s52 (s52 < 1) or ÷ s52
#   6 can't move s52 ticks   7 buttons frozen s52 ticks   8 poison s52 ticks
#   12-15 attack / speed / homing / Aura-rate status, amount s52, duration s53 (0 = permanent; r9 = s53 in the call)
#   16 the caster's button locked (status 13 + button), duration s52 (0 = permanent)
#   17 reset all the target's statuses   37 stick turned 180 deg (status 35) s52 ticks   40 Decoy (status 9) s52 ticks
#   41 Dazzle (status 36) s52 ticks   43 Health += s52   44 Level += s52   45 warp the target to the caster's start
#   46 both Healths become the average   47 Level := the lowest Level among the players   50 hand the caster's skills over
#   51 reverse the four attribute statuses (value and timed amount)   38 / 39 end the environment crystal (39 also resets statuses)
STAT_OF_CODE = {12: "Attack", 13: "Speed", 14: "Homing", 15: "AuraRate"}


def _mods(snap, stat):
    return [(m["amount"], m["ticks"]) for m in snap["mods"] if m["stat"] == stat]


def chk_add_nonattack(P, rec, O):
    r = rec["raw"]
    code, amt, s53 = r[51], r[52], r[53]
    if rec["capsule"] == "attack" or code == 0 or r[43] != 0:
        return []
    hit = O.get("hit")
    cells = []
    src = "FUN_140272bc0 case %d" % code
    if not hit or "probe_error" in hit:
        return [gap("add", "code%d" % code, "no hit probe")]
    instant = r[63] == 4
    everyone = instant and r[64] == 2
    if instant:
        row = row_for(rec)
        if row and hit.get("effect_step") is not None:
            want = SIM * (row["fire_tick"] + r[65])
            cells.append(cmp("add", "instant_delay", want, hit["effect_step"], "FUN_140265f50: an instant takes effect s65 ticks after the fire tick (s65 = %d)" % r[65], tol=4, note="step the opponent first changes"))
    elif hit.get("first_hit_step") is None:
        return cells + [gap("add", "code%d" % code, "the probe's skill never reached the opponent")]
    # What the opponent (and the caster) look like the moment the effect lands.
    snaps = hit.get("at_effect") or hit["after"]
    vb, va = hit["before"][1], snaps[1]
    cb, ca = hit["before"][0], snaps[0]

    def left(total):
        return SIM * total

    def ticks_ok(want, got, tol=4):
        return want is not None and got is not None and abs(want - got) <= tol

    who = [("p2", vb, va)] + ([("p1", cb, ca)] if everyone else [])
    for nm, b, a in who:
        k = "code%d.%s" % (code, nm)
        if code == 1:
            if nm == "p2" or amt >= 99:
                # 99 = all of it: the caster's own Aura ends at 0 whatever the payment took
                cells.append(cmp("add", k, max(0, b["aura"] - amt), a["aura"], src + ": Aura -= s52 (clamped at 0)", tol=1))
            else:
                cells.append(gap("add", k, "the caster's Aura also pays the cost"))
        elif code == 2:
            cells.append(cmp("add", k, max(0, b["level"] - amt), a["level"], src + ": Level -= s52"))
        elif code == 4:
            want = (-(b["level"] * amt)) if amt < 1 else b["level"] // amt
            cells.append(cmp("add", k, want, a["level"], src + ": Level x -s52 (s52 < 1) or divided by s52 (whole)"))
        elif code == 6:
            w_ = left(amt)
            cells.append(Cell("add", k, OK if ticks_ok(w_, a["bound"]) else BAD, w_, a["bound"], "can't-move ticks left", src))
        elif code == 7:
            w_ = left(amt)
            cells.append(Cell("add", k, OK if ticks_ok(w_, a["frozen"]) else BAD, w_, a["frozen"], "frozen-button ticks left", src))
        elif code == 8:
            w_ = left(amt)
            cells.append(Cell("add", k, OK if ticks_ok(w_, a["poison"]) else BAD, w_, a["poison"], "poison ticks left", src))
        elif code in STAT_OF_CODE:
            stat = STAT_OF_CODE[code]
            cap = 9 if stat == "Attack" else 90
            got = _mods(a, stat)
            want_amt = max(-cap, min(cap, amt))
            if s53 == 0:
                ok = any(g[0] == want_amt and g[1] is None for g in got)
                want = [(want_amt, None)]
            else:
                ok = any(g[0] == amt and g[1] is not None and ticks_ok(left(s53), g[1]) for g in got)
                want = [(amt, left(s53))]
            cells.append(Cell("add", k, OK if ok else BAD, want, got, "%s %+d %s" % (stat, amt, "permanent" if s53 == 0 else "for %d ticks" % s53), src))
        elif code == 16:
            slot, ticks = a["locked_slot"]
            ok = ticks > 6000  # permanent (duration s52 = 0): status 13 + button is a plain value
            cells.append(Cell("add", k, OK if (amt == 0 and ok) or (amt != 0 and ticks_ok(left(amt), ticks)) else BAD, "permanent" if amt == 0 else left(amt), "slot %d for %d ticks" % (slot, ticks), "the button the caster pressed is locked on the target", src))
        elif code == 17:
            cleared = not a["mods"] and not a["bound"] and not a["frozen"] and not a["poison"] and not a["inverted"]
            cells.append(Cell("add", k, OK if (cleared and not a["mods"]) else BAD, "every status reset", a["mods"], "FUN_14020d220 clears all 47 status slots", src))
        elif code == 37:
            w_ = left(amt)
            cells.append(Cell("add", k, OK if ticks_ok(w_, a["inverted"]) else BAD, w_, a["inverted"], "stick-inverted ticks left", src))
        elif code == 40 and nm == "p2":
            fl = a.get("forced_lock")
            w_ = left(amt)
            cells.append(Cell("add", k, OK if (fl is not None and fl[0] == 0 and ticks_ok(w_, fl[1])) else BAD, [0, w_], fl, "can only lock onto the caster", src))
        elif code == 41:
            w_ = left(amt)
            cells.append(Cell("add", k, OK if ticks_ok(w_, a["scrambled"]) else BAD, w_, a["scrambled"], "scrambled-button ticks left", src))
        elif code == 43:
            cells.append(cmp("add", k, min(20, b["health"] + amt), a["health"], src + ": Health += s52 (to the maximum)"))
        elif code == 44:
            if rec["id"] == 307:
                cells.append(unk("add", k, "Boost Mine: which player the Level +s52 goes to (the mine's owner or whoever trips it) is not read; the note says 'your Level'", None, [ca["level"] - cb["level"], va["level"] - vb["level"]], src))
            else:
                cells.append(cmp("add", k, b["level"] + amt, a["level"], src + ": Level += s52"))
        elif code == 46 and nm == "p2":
            avg = (cb["health"] + vb["health"]) // 2
            cells.append(cmp("add", k, [avg, avg], [ca["health"], va["health"]], src + ": both Healths become the average"))
        elif code == 47:
            low = min(vb["level"], cb["level"])
            cells.append(cmp("add", k, low, a["level"], src + ": Level := the lowest Level of the players"))
        elif code == 49 and nm == "p2":
            cells.append(cmp("add", k, vb["level"], va["level"], src + ": Level -= s52 per Aura Particle lying near the target (none on the field here)"))
        elif code == 50 and nm == "p2":
            hand = hit["before"][1]["slots"]
            want = [hand[0]] + [x for x in (65, 26, 80)]
            cells.append(cmp("add", k, want, va["slots"], src + ": every button but the pressed one takes the caster's skill"))
        elif code == 51:
            neg = []
            for stat in ("Attack", "Speed", "Homing", "AuraRate"):
                for amount, ticks in _mods(b, stat):
                    neg.append((stat, -amount, ticks))
            got = sorted((m["stat"], m["amount"], m["ticks"]) for m in a["mods"])
            want = sorted((st_, am, tk) for st_, am, tk in neg)
            ok = len(got) == len(want) and all(g[0] == w_[0] and g[1] == w_[1] for g, w_ in zip(got, want))
            cells.append(Cell("add", k, OK if ok else BAD, want, got, "every attribute status negated", src))
        elif code in (38, 39) and nm == "p2":
            cells.append(cmp("add", "code%d.crystal" % code, False, hit.get("crystal_after"), src + ": the environment crystal ends"))
        elif code == 45 and nm == "p2":
            sp = hit["caster_spawn"]
            cells.append(Cell("add", k, OK if math.hypot(va["pos"][0] - sp[0], va["pos"][2] - sp[2]) < 1.0 else BAD, [sp[0], sp[2]], [va["pos"][0], va["pos"][2]], "the target is warped to the caster's start point", src))
        elif code == 10 and nm == "p2":
            cells.append(cmp("add", k, 0, a["aura"], src + ": absorbs the target's Aura (the amount is a float argument the decompiler lost; the 99 means all)", tol=1))
        elif nm == "p2" and code == 42:
            cells.append(gap("add", k, "Trigger stores the pressed button in a status slot of the target (case 0x2a) and forces its next cast: not probed"))
        elif nm == "p2" and code not in (38, 39, 40, 46, 49, 50, 45, 10):
            cells.append(gap("add", k, "no expectation written for Add code %d" % code))
    return cells


# ---------------------------------------------------------------------------
# REACTION (s57-s61 -> reaction table)


def chk_react(P, rec, O):
    if rec["capsule"] != "attack" or rec.get("synthetic"):
        return []
    s = O.get("str") or {}
    if "react_light" not in s:
        return []
    r = rec["raw"]
    cells = []
    src = "FUN_14024fd80 (pick), FUN_140258690 (play)"
    for key, want_row, label in (("react_light", r[57], "total < 3: s57"), ("react_medium", r[58], "total 3-4: s58"), ("react_heavy", r[59], "total >= 5: s59")):
        got = s[key]
        cells.append(cmp("react", key, want_row, got["row"], src + " (" + label + ")"))
        want_kind = (P.reactions.get(want_row) or {}).get("kind")
        if want_kind is not None:
            cells.append(cmp("react", key + "_kind", want_kind, got["kind"], "reaction table 0x2B00C: row kind", note="the row's own kind (clip / stagger / knock-down ...)"))
        else:
            cells.append(unk("react", key + "_kind", "reaction row %d has no entry in the exported table (all-zero row)" % want_row, None, got["kind"], src))
    air_row = r[61] if r[61] else 27
    cells.append(cmp("react", "react_air", air_row, s["react_air"]["row"], src + ": hit in the air -> s61 (0 on every skill) = row 27, a knock-down"))
    return cells


# ---------------------------------------------------------------------------
# SOUND and EFFECT ids


def chk_sound(P, rec, O):
    snd = O.get("sound")
    if not snd:
        return []
    r = rec["raw"]
    row = row_for(rec, False)
    cells = []
    if rec.get("synthetic"):
        return []
    cast = row["cast_sound"] if row else 0
    cells.append(cmp("snd", "cast", [cast] if cast > 0 else [], snd["cast"], "action row w3 (FUN_14025a3e0): the cast sound at cast start", note="our list is empty when the id is not in the decoded common bank", none_ok=True))
    # s19 is played at the fire tick for attack objects (s43 = 0, motion != 4), s20 loops with the flying object, s21 at the impact.
    attack_obj = r[43] == 0 and r[63] != 4
    s19, s20, s21 = r[19], r[20], r[21]
    cells.append(cmp("snd", "fire", s19 if (s19 > 0 and attack_obj) else None, snd["fire"], "FUN_1402626d0 @0x140262caa: s19 at the fire tick of an attack object", none_ok=True))
    cells.append(cmp("snd", "travel", s20 if s20 > 0 else None, snd["travel"], "FUN_140263a00: s20 loops with the object", none_ok=True))
    cells.append(cmp("snd", "impact", s21 if s21 > 0 else None, snd["impact"], "FUN_14026cd70: s21 at the impact point", none_ok=True))
    return cells


def chk_fx(P, rec, O):
    fx = O.get("fx")
    if not fx or rec.get("synthetic"):
        return []
    cells = []
    row_g, row_a = rec["action"]["ground"], rec["action"]["air"]
    src = "FUN_14025a3e0 -> FUN_140259140 -> FUN_14018d2f0: the effect is the action row's w2, read from the packed archive"
    wg = "eft%04d" % row_g["effect"] if row_g and row_g.get("effect") else None
    wa = "eft%04d" % row_a["effect"] if row_a and row_a.get("effect") else None
    cells.append(cmp("fx", "ground_effect", wg, fx["ground"], src, none_ok=True))
    if wa is not None:
        cells.append(cmp("fx", "air_effect", wa, fx["air"], src, none_ok=True))
    # Slots the program plays that our exported effect has no content for (slot 0 cast, 1 flight, 2 impact, 3 hit).
    gs = fx.get("ground_slots")
    if wg and gs is None:
        cells.append(Cell("fx", "exported", BAD, wg, None, "the effect file is missing from our converted cache", src))
    return cells


# ---------------------------------------------------------------------------
# TABLE: the converter's skills.json against the game file itself


def chk_table(P, rec, O):
    """Our whole pipeline starts at cache/skills.json (pd_convert skills). Every other check in this file
    reads it, so it is first checked against `gsdata_en.dat` read directly (our own reader above): the
    record (s2..s73), the action rows s12 / s13, and the reaction rows s57-s61."""
    if P.gs is None or rec.get("synthetic"):
        return []
    cells = []
    k = rec["record"] + 1
    prog = list(P.gs.record(k))
    mine = [rec["raw"][j] for j in range(2, 70)] + list(rec["s70_73"])
    cells.append(Cell("table", "record", OK if prog == mine else BAD, "gsdata record %d (72 fields)" % k, "skills.json" if prog == mine else [(j + 2, a, b) for j, (a, b) in enumerate(zip(prog, mine)) if a != b][:4], "", "FUN_14017dcb0 (record parse)"))
    for side, idx in (("ground", 12), ("air", 13)):
        row = prog[idx - 2]
        got = rec["action"][side]
        if row == 0:
            ok = got is None
            cells.append(Cell("table", "action_" + side, OK if ok else BAD, None, got and got.get("row"), "", "action table 0x1B00C"))
        else:
            f = list(P.gs.action_row(row))
            ok = got is not None and got["raw"] == f
            cells.append(Cell("table", "action_" + side, OK if ok else BAD, "row %d of the file" % row, "skills.json" if ok else (got or {}).get("row"), "", "action table 0x1B00C"))
    for idx, nm in ((57, "light"), (58, "medium"), (59, "heavy"), (60, "after_add"), (61, "air")):
        row = prog[idx - 2]
        if row == 0:
            continue
        f = list(P.gs.reaction_row(row))
        got = P.reactions.get(row)
        # skills.json lists a reaction row only when some field past w0 is non-zero
        ok = (got is not None and got["raw"] == f) or (got is None and not any(f[1:]))
        cells.append(Cell("table", "reaction_" + nm, OK if ok else BAD, "row %d of the file" % row, "skills.json" if ok else None, "", "reaction table 0x2B00C"))
    return cells


# ---------------------------------------------------------------------------
# The whole skill


def _checks():
    """The comparators, in column order. The guard battery lives in `skill_audit_guards.py` (imported here, not at the top, because
    it needs `Cell` and the helpers of this module)."""
    import skill_audit_guards as G

    return [chk_table, chk_dispatch, chk_pay, chk_cast, chk_str, chk_band, chk_flight, chk_area, chk_melee, chk_guard, G.chk_guard_battery, G.chk_auto, G.chk_orb,
            chk_env, chk_effect, chk_capsules, chk_movement, chk_hit_effects, chk_add_nonattack, chk_react, chk_sound, chk_fx]


# Ledger columns: which channels each column reports.
COLUMNS = [
    ("tbl", ["table"]),
    ("disp", ["dispatch"]),
    ("pay", ["pay"]),
    ("cast", ["cast"]),
    ("str", ["str"]),
    ("move", ["flight", "area", "melee", "guard", "env", "effect", "band", "add", "erase", "capsules", "movement"]),
    ("react", ["react"]),
    ("snd", ["snd"]),
    ("fx", ["fx"]),
]


# UNCONFIRMED cells a recorded take of the original can settle (state, position, sound, stat traces)
TAKEABLE = ("cast.timing", "cast.hold", "movement.warp_target", "movement.capsule_spot", "guard.time", "flight.swing_side", "str.damage",
            "add.code44", "pay.requirement", "effect.", "guard.all_round")


def audit_skill(P, rec, O):
    """All cells of one skill. `O` is the probe's record for it (None = the probe did not run it)."""
    cells = []
    if O is None:
        return [gap("all", "probe", "no probe record for this skill")]
    for chk in _checks():
        try:
            if chk is chk_str:
                cells.extend(chk(P, rec, O, None))
            else:
                cells.extend(chk(P, rec, O))
        except Exception as ex:  # a bug in a comparator must show up, not hide a skill
            cells.append(Cell(chk.__name__[4:], "tool_error", GAP, None, None, "comparator error: %r" % (ex,), ""))
    for c in cells:
        if c.st == UNK and any(("%s.%s" % (c.ch, c.key)).startswith(t) for t in TAKEABLE):
            c.st = TAKE
            c.note = "needs take: " + c.note
    return cells
