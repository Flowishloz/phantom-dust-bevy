"""Tests of the per-skill audit tool (synthetic data; one optional live check).

Run from engine/re:  python -m unittest test_skill_audit
The live check runs only when a probe dump exists (set PD_SKILL_AUDIT_OURS to its path, made by
`PD_SKILL_AUDIT_OUT=<file> cargo test -p pd_game skill_audit_dump`) and the converted cache is there.
"""
import json
import os
import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import skill_audit as A  # noqa: E402
import skill_audit_guards as G  # noqa: E402
import skill_audit_report as REP  # noqa: E402
import skill_audit_rules as R  # noqa: E402


def mk(raw=None, capsule="attack", ground=None, air=None, sid=1, record=0, h=None, name="X"):
    r = [0] * 70
    for k, v in (raw or {}).items():
        r[k] = v
    return {"id": sid, "name": name, "capsule": capsule, "raw": r, "s70_73": h or [0, 0, 0, 0], "record": record, "action": {"ground": ground, "air": air}, "registered": True}


def row(anim="ATK_SHOT", anim_id=65, fire=20, dash=(0, 0), hit_end=0, effect=1, cast_sound=0, charge=0):
    return {"anim_name": anim, "anim_id": anim_id, "fire_tick": fire, "dash_start": dash[0], "dash_end": dash[1], "hit_window_end": hit_end,
            "effect": effect, "cast_sound": cast_sound, "charge_cycle": charge, "voice_group": 1, "raw": [0] * 57, "row": 1}


def ours_shot(speed=12.963, life=23, spawn=(0, 14.375, 10)):
    """What the Rust probe writes for a straight shell flying 23 program ticks at 12.963 u per tick."""
    pts = [[spawn[0], spawn[1], spawn[2] + speed * (k + 1)] for k in range(life)]
    return {"behaviour": "Shoot:Shell", "detail": {"debug": "Shell { speed: %f, end_speed: 0.0, ramp: 0, turn: 0.026179, life: %d }" % (speed, life), "placeholder": False},
            "pos0": spawn, "pts": pts}


class ProgramRules(unittest.TestCase):
    def test_life_matches_the_read_function(self):
        # FUN_1402636f0: Blaster range -30 x 10 = 300 u at 140 / 10.8 = 12.96 u per tick -> 23 ticks.
        self.assertEqual(R.prog_life(-30, 140, 0, 0), 23)
        self.assertEqual(R.prog_life(90, 0, 0, 0), 90)  # >= 0 is the life in ticks
        # a ramp longer than the range ends at the ramp's own length (Psycho Sniper -80, 0 -> 380 over 60)
        self.assertEqual(R.prog_life(-80, 0, 380, 60), 60)

    def test_speed_ramp(self):
        self.assertEqual(R.prog_ramp(1.0, 3.0, 0, 5), 1.0)
        self.assertAlmostEqual(R.prog_ramp(0.0, 10.0, 10, 5), 5.0)
        self.assertEqual(R.prog_ramp(0.0, 10.0, 10, 99), 10.0)

    def test_range_band(self):
        self.assertEqual(R.prog_band(0, 6, 40.0), (80.0, False))  # pulled out to 80, target dropped
        self.assertEqual(R.prog_band(0, 6, 150.0), (150.0, True))
        self.assertEqual(R.prog_band(1, 6, 150.0), (300.0, False))  # Optical lasers cannot hit closer than 300
        self.assertEqual(R.prog_band(1, 6, 800.0), (600.0, False))
        self.assertEqual(R.prog_band(2, 20, 700.0), (700.0, True))  # no limit
        self.assertEqual(R.prog_band(0, 16, 700.0), (700.0, True))  # every motion-16 Hole
        self.assertEqual(R.prog_band(0, 6, None), (190.0, False))  # nothing locked
        self.assertEqual(R.prog_band(1, 6, None), (300.0, False))
        self.assertEqual(R.prog_band(2, 6, None), (190.0, False))  # FUN_14026e410 reads 190 for anything but s44 = 1

    def test_str_rules_follow_the_disassembled_table(self):
        state = {"attacker": {"level": 7, "aura": 7, "empty": 3, "used_attack": 2, "used_defence": 1}, "target": {"level": 5, "aura": 4.5, "empty": 2}, "aura_capsules": 2}
        f = lambda rule, s46=2, s47=0: R.prog_str(mk({45: rule, 46: s46, 47: s47}), 17, 12, state)[0]
        self.assertEqual(f(0, 3), 3)
        self.assertEqual(f(1), 0)  # (12 - 17) x 2 is negative: never below 0
        self.assertEqual(R.prog_str(mk({45: 1, 46: 2}), 12, 17, state)[0], 10)
        self.assertEqual(R.prog_str(mk({45: 2, 46: 2}), 17, 12, state)[0], 10)
        self.assertEqual(f(3), 4)
        self.assertEqual(f(4), 2)
        self.assertEqual(f(17, 1), 5)
        self.assertEqual(f(18, 1), 4)  # whole points of Aura
        self.assertEqual(f(20, 2), 2)  # whole points / s46
        self.assertEqual(f(23), 4)
        self.assertEqual(f(24), 6)
        self.assertEqual(f(30), 4)
        self.assertEqual(R.prog_str(mk({45: 28, 46: 3}), 17, 12, state), (None, [0, 3, 6]))  # rand x 3 / 32768
        self.assertEqual(R.prog_str(mk({45: 21, 46: 1}), 17, 12, state), (None, None))  # the cast's stored value: needs a cast

    def test_capsule_field_filters(self):
        caps = [(1, 0, 0), (2, 0, 65), (3, 1, 0), (4, 1, 80)]
        ids = lambda s44, touched: [c[0] for c in R.capsule_targets(s44, caps, touched, 0)]
        self.assertEqual(ids(0, 2), [2])  # the touched one, any kind
        self.assertEqual(ids(1, 2), [])  # touched Aura Particle only: it is a skill capsule
        self.assertEqual(ids(1, 1), [1])
        self.assertEqual(ids(2, 2), [1, 3])  # every Aura Particle
        self.assertEqual(ids(3, 2), [2])  # touched skill capsule only
        self.assertEqual(ids(4, 2), [3, 4])  # the other team's, both kinds
        self.assertEqual(ids(5, 2), [1, 2, 3, 4])  # everyone's, both kinds

    def test_erase_selection(self):
        a = mk({9: 1, 22: 1}, sid=65)
        b = mk({9: 1, 22: 4}, sid=80)
        c = mk({9: 2, 22: 1}, capsule="defence", sid=26)
        P = R.Program({"skills": [a, b, c], "reactions": []})
        skill = mk({48: 1, 49: 1, 50: 1}, capsule="erase")  # attack colour, highest cost first, one of them
        cand, count, ordered, _ = R.erase_expect(P, skill, [65, 80, 26, None])
        self.assertEqual((cand, count, ordered), ([0, 1], 1, [1, 0]))
        low = mk({48: 1, 49: 2, 50: 1}, capsule="erase")
        self.assertEqual(R.erase_expect(P, low, [65, 80, 26, None])[2], [0, 1])
        pressed = mk({48: 9, 49: 4, 50: 1}, capsule="erase")
        self.assertEqual(R.erase_expect(P, pressed, [65, 80, 26, None])[0], [0])  # only the pressed button's slot


class Comparators(unittest.TestCase):
    def P(self, *recs):
        p = R.Program({"skills": list(recs), "reactions": []})
        p.clips = {"ATK_SHOT": 90, "ATK_LASER": 68, "WAIT0": 80}
        return p

    def test_a_straight_shell_matches_then_breaks_when_ours_is_off(self):
        rec = mk({63: 0, 64: -30, 65: 140, 68: 15, 42: 12}, ground=row())
        P = self.P(rec)
        O = {"static": ours_shot()}
        det = O["static"]["detail"]["debug"]
        launch = {"shots": [{"pos": [0, 14.375, 10], "heading": [0, 0, 1], "life": 23}], "marks": [[0, 14.375, 10]]}
        track = {"age0": 1, "pts": O["static"]["pts"]}
        O["flight"] = {"free": {"fired": True, "count_at_fire": 1, "launch0": launch, "tracks": [track]}, "steer": {"tracks": []}}
        cells = R.chk_flight(P, rec, O)
        by = {c.key: c for c in cells}
        self.assertEqual(by["speed"].st, R.OK)
        self.assertEqual(by["life"].st, R.OK)
        self.assertEqual(by["life_ticks"].st, R.OK)
        self.assertEqual(by["speed_profile"].st, R.OK)
        # ours flies faster: the speed, the profile and the path now disagree
        O["static"]["detail"]["debug"] = det.replace("12.963", "14.0")
        O["flight"]["free"]["tracks"][0]["pts"] = [[0, 14.375, 10 + 14.0 * (k + 1)] for k in range(23)]
        cells = R.chk_flight(P, rec, O)
        by = {c.key: c for c in cells}
        self.assertEqual(by["speed"].st, R.BAD)
        self.assertEqual(by["speed_profile"].st, R.BAD)

    def test_cast_timing_reads_the_action_row(self):
        rec = mk({12: 1, 42: 12}, ground=row(fire=13, anim="ATK_LASER", anim_id=76))
        P = self.P(rec)
        g = {"accepted": True, "fired": True, "fire_step": 26, "hold_steps": 68, "clip": "ATK_LASER", "release_step": None}
        O = {"static": {"behaviour": "Shoot:Arc", "charged": False}, "cast": {"ground": g, "air": {"accepted": False}, "ground_allowed": True, "air_allowed": False, "air_row_anim": "ATK_LASER", "ground_row_anim": "ATK_LASER"}}
        cells = {c.key: c for c in R.chk_cast(P, rec, O)}
        self.assertEqual(cells["fire_tick"].st, R.OK)
        self.assertEqual(cells["hold"].st, R.OK)
        g["fire_step"] = 40
        self.assertEqual({c.key: c for c in R.chk_cast(P, rec, O)}["fire_tick"].st, R.BAD)

    def test_an_instant_row_has_no_hold(self):
        rec = mk({12: 1}, capsule="special", ground=row(anim="WAIT0", anim_id=0, fire=1, effect=0))
        P = self.P(rec)
        g = {"accepted": True, "fired": True, "fire_step": 2, "hold_steps": 80, "clip": "WAIT0"}
        O = {"static": {"behaviour": "Effect:Caster", "charged": False}, "cast": {"ground": g, "air": {"accepted": False}, "ground_allowed": True, "air_allowed": False}}
        hold = {c.key: c for c in R.chk_cast(P, rec, O)}["hold"]
        self.assertEqual((hold.st, hold.prog, hold.ours), (R.BAD, 0, 80))

    def test_dispatch_flags_a_placeholder_for_a_family_the_program_has_no_handler_for(self):
        rec = mk({63: 18})
        O = {"static": {"behaviour": "Shoot:Shell", "detail": {"placeholder": True}}}
        self.assertEqual(R.chk_dispatch(None, rec, O)[0].st, R.BAD)
        rec = mk({63: 6})
        O = {"static": {"behaviour": "Shoot:Arc", "detail": {}}}
        self.assertEqual(R.chk_dispatch(None, rec, O)[0].st, R.OK)

    def test_guard_numbers(self):
        rec = mk({43: 2, 44: 0, 45: 130, 46: 23, 47: 3, 49: 1}, capsule="defence")
        det = {"kind": 0, "def": 3, "ticks": 44, "anti_pierce": True, "field": 13.0, "add": 0, "add_amount": 0, "short_range_only": False, "all_round": False}
        O = {"static": {"behaviour": "Guard", "detail": det}, "guard": {"def_after_two_uses": 3, "up_for_after_fire": 44}}
        cells = {c.key: c for c in R.chk_guard(None, rec, O)}
        for k in ("kind", "size", "time", "def", "anti_pierce", "short_range_only", "all_round", "stood_for"):
            self.assertEqual(cells[k].st, R.OK, k)
        det["ticks"] = 46
        self.assertEqual({c.key: c for c in R.chk_guard(None, rec, O)}["time"].st, R.BAD)

    def test_payment_at_the_fire_step(self):
        rec = mk({22: 3, 23: 1, 25: 2, 26: 0}, ground=row())
        before = {"aura_ticks": 20 * R.APORA, "level": 20, "health": 20}
        end = {"aura_ticks": 17 * R.APORA, "level": 19, "health": 18}
        g = {"accepted": True, "fired": True, "before": before, "end": end, "at_fire_before": before, "at_fire_after": end}
        O = {"cast": {"ground": g}, "uses": {"cost_series": [3.0, 3.0], "unlimited": True, "aura_before": 20.0}, "gate": {}, "static": {}}
        cells = {c.key: c for c in R.chk_pay(self.P(rec), rec, O)}
        self.assertEqual(cells["aura_cost"].st, R.OK)
        self.assertEqual(cells["level"].st, R.OK)
        self.assertEqual(cells["health"].st, R.OK)
        self.assertEqual(cells["uses"].st, R.OK)

    def test_unsettled_values_that_a_take_can_measure_are_marked_T(self):
        rec = mk({43: 3, 44: 7}, capsule="special", ground=row(anim="WAIT0", anim_id=0))
        P = self.P(rec)
        O = {"static": {"behaviour": "Effect:Caster"}, "cast": {"ground": {"accepted": False}, "air": {"accepted": False}}, "movement": {"end": [0, 0, 0], "spawn": [0, 0, 0], "track": [], "max_y": 0, "end_rel": [0, 0, 0]}}
        cells = R.audit_skill(P, rec, O)
        warp = [c for c in cells if c.key == "warp_target"]
        self.assertTrue(warp and warp[0].st == R.TAKE and warp[0].note.startswith("needs take"))

    def test_none_is_a_value_for_sounds(self):
        rec = mk({}, ground=row(cast_sound=0))
        O = {"sound": {"cast": [], "fire": None, "travel": None, "impact": None}}
        self.assertTrue(all(c.st == R.OK for c in R.chk_sound(self.P(rec), rec, O)))

    def test_severity_orders_the_statuses(self):
        cells = [R.Cell("a", "b", R.OK), R.Cell("a", "c", R.GAP), R.Cell("a", "d", R.TAKE)]
        self.assertEqual(R.worst(cells), R.TAKE)
        cells.append(R.Cell("a", "e", R.BAD))
        self.assertEqual(R.worst(cells), R.BAD)
        self.assertIsNone(R.worst([]))


class Decks(unittest.TestCase):
    def deck(self, ids, name="E1"):
        body = struct.pack("<I", 0) + name.encode().ljust(16, b"\0") + struct.pack("<30h", *(list(ids) + [-1] * (30 - len(ids)))) + struct.pack("<H", 1) + b"\0" * 18
        self.assertEqual(len(body), 100)
        return body

    def test_game_decks_are_read_and_empty_ones_dropped(self):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "DECK001").write_bytes(self.deck([3, 3, 31]))
            (Path(d) / "DECK002").write_bytes(self.deck([]))
            (Path(d) / "DECK003").write_bytes(b"\0" * 660)  # not a 100-byte deck
            out = A.read_decks_dir(d)
            self.assertEqual([(n, s) for n, _, s in out], [("DECK001", [3, 3, 31])])

    def doc(self):
        # records 0..3 are table indices 1..4; the skill ids (s3) are NOT the table indices
        return {"skills": [mk({3: 7, 11: 1}, sid=7, record=0), mk({3: 9, 11: 1}, sid=9, record=1), mk({3: 2, 11: 2}, sid=2, record=2), mk({3: 5, 11: 2}, sid=5, record=3),
                           mk({3: 2, 11: 2}, sid=2, record=749, name="dodge")],
                "decks": [{"name": "PSYCHO", "skills": [1, 1, 2, 2]}], "quick_decks": [{"name": "Q000", "kind": 2, "skills": [3, 4, -1, 23428]}]}

    def test_deck_entries_are_table_indices_not_skill_ids(self):
        m = A.table_index_map(self.doc())
        self.assertEqual((m[1], m[2], m[3], m[4]), (7, 9, 2, 5))
        self.assertNotIn(750, m)  # the duplicate id's second record is a dodge, not a skill

    def test_usage_counts_in_the_programs_index_space(self):
        use, invalid, n = A.usage_table(self.doc(), None)
        self.assertEqual(use[7]["debug"], 2)  # entry 1 twice -> id 7
        self.assertEqual(use[9]["debug_decks"], 1)
        self.assertEqual(use[2]["quick"], 1)  # entry 3 -> id 2
        self.assertNotIn(1, use)  # entry 1 is NOT skill id 1
        self.assertEqual(invalid[23428], 1)  # not an index of the table: counted as empty
        self.assertEqual(n, 0)

    def test_rank_key_orders_by_decks_then_copies(self):
        a = {"quick": 0, "quick_decks": 0, "cpu": 5, "cpu_decks": 2, "debug": 0, "debug_decks": 0}
        b = {"quick": 1, "quick_decks": 1, "cpu": 0, "cpu_decks": 0, "debug": 0, "debug_decks": 0}
        c = {"quick": 0, "quick_decks": 0, "cpu": 9, "cpu_decks": 2, "debug": 0, "debug_decks": 0}
        self.assertEqual(sorted(["a", "b", "c"], key=lambda k: A.usage_score({"a": a, "b": b, "c": c}[k])), ["c", "a", "b"])

    def test_the_evidence_prefers_the_table_index_reading(self):
        doc = self.doc()
        doc["decks"] = [{"name": "MIXED", "skills": [1, 2, 3, 4]}]  # indices -> psycho, psycho, optical, optical
        ev = A.index_space_evidence(doc, None)
        name, as_index, as_id = ev["named"][0]
        self.assertEqual(name, "MIXED")
        self.assertAlmostEqual(as_index, 0.5)  # two schools of two
        self.assertEqual(ev["agree"], 0)  # no record's table index equals its s3 id here
        self.assertEqual(ev["records"], 5)


class Ledger(unittest.TestCase):
    def rows(self):
        rec1 = mk({}, sid=1, name="Alpha")
        rec2 = mk({}, sid=2, name="Beta")
        u0 = {"quick": 1, "quick_decks": 1, "cpu": 4, "cpu_decks": 3, "debug": 0, "debug_decks": 0}
        u1 = {"quick": 0, "quick_decks": 0, "cpu": 0, "cpu_decks": 0, "debug": 0, "debug_decks": 0}
        r1 = A.Row(rec1, [R.Cell("pay", "uses", R.OK), R.Cell("cast", "hold", R.BAD, 0, 80, "x", "FUN_1")], u0, 1)
        r2 = A.Row(rec2, [R.Cell("pay", "uses", R.OK), R.Cell("str", "damage", R.TAKE, None, 3, "needs take: y")], u1, 2)
        return [r1, r2], {1: u0, 2: u1}

    def test_row_status_and_columns(self):
        (r1, r2), _ = self.rows()
        self.assertEqual((r1.status(), r2.status()), ("bad", "take"))
        self.assertEqual(r1.col(["cast"]), R.BAD)
        self.assertIsNone(r1.col(["flight"]))

    def test_classes_group_the_crosses(self):
        rows, _ = self.rows()
        g = A.mismatch_classes(rows)
        self.assertEqual(list(g), ["cast.hold"])
        self.assertIn("str.damage", A.unsettled(rows))

    def test_the_ledger_has_every_section(self):
        rows, usage = self.rows()
        text = A.render_ledger(rows, usage, 402, {}, {}, lanes_text=REP.lanes_text(rows), footage_text=REP.footage_text(rows), spot_text=REP.spot_text(rows), index_text="## Top-priority row: which skill number each part of the game uses")
        for needle in ("## Summary", "## Top mismatch classes", "## Proposed fix lanes", "## The ledger", "## Mismatch detail", "needs take", "Top-priority row", "| 1 | 1 | Alpha |"):
            self.assertIn(needle, text)

    def test_the_index_lane_is_first_and_lists_the_built_in_decks_skills(self):
        rows, _ = self.rows()
        lanes, _ = REP.lane_rows(rows)
        self.assertEqual(lanes[0][0]["id"], "INDEX")
        self.assertEqual(list(lanes[0][1]), [1])  # only Alpha sits in a built-in deck

    def test_every_lane_claims_distinct_crosses(self):
        ids = [lane["id"] for lane in REP.LANES]
        self.assertEqual(len(ids), len(set(ids)))


def guard(kind, d=3, anti=False, short_only=False, add=0, add_amt=0):
    return {"kind": kind, "def": d, "anti": anti, "short_only": short_only, "add": add, "add_amt": add_amt}


def atk(motion=0, s54=0, state3=False):
    return {"motion": motion, "s54": s54, "state3": state3}


class GuardBattery(unittest.TestCase):
    """The program model of FUN_140271760 (wave 6, lane probes), one rule at a time."""

    def test_a_barrier_blocks_drains_and_lingers_then_breaks(self):
        p = G.prog_case(guard(0), atk(), 2, 0)
        self.assertEqual((p["blocked"], p["broken"], p["def_after"], p["lingering"]), (True, False, 1, True))
        p = G.prog_case(guard(0), atk(), 3, 0)  # STR = DEF: still holds (DEF - STR = 0 is not below 0)
        self.assertEqual((p["broken"], p["def_after"]), (False, 0))
        p = G.prog_case(guard(0), atk(), 4, 0)
        self.assertEqual((p["blocked"], p["broken"], p["def_after"], p["leftover"], p["slot_erased"]), (True, True, None, 0, True))

    def test_pierce_goes_through_a_broken_wall_unless_anti_pierce(self):
        self.assertEqual(G.prog_case(guard(0), atk(s54=1), 5, 0)["leftover"], 2)
        self.assertEqual(G.prog_case(guard(0, anti=True), atk(s54=1), 5, 0)["leftover"], 0)
        self.assertEqual(G.prog_case(guard(0), atk(s54=1), 3, 0)["leftover"], 0)  # a block that holds lets nothing through
        self.assertEqual(G.prog_case(guard(0), atk(s54=1), 5, 0, necro=True)["leftover"], 0)  # status 31 reads s54 as 0

    def test_s54_2_penetrates_everything_but_the_orbit_guards(self):
        for kind in (0, 1, 3, 4, 5):
            self.assertFalse(G.prog_case(guard(kind), atk(s54=2), 1, 0)["blocked"], kind)
        self.assertTrue(G.prog_case(guard(6), atk(s54=2), 1, 0)["turned"])
        self.assertTrue(G.prog_case(guard(0), atk(s54=2), 1, 0, necro=True)["blocked"])

    def test_the_ignore_lists(self):
        for m in (7, 10, 20):
            self.assertFalse(G.prog_case(guard(0), atk(m), 1, 2)["blocked"], m)  # Barrier / Reflect: Rain and Tornado
            self.assertFalse(G.prog_case(guard(4), atk(m), 1, 2)["blocked"], m)
        self.assertTrue(G.prog_case(guard(0), atk(8), 1, 0)["blocked"])  # a mine is stopped by a Barrier
        for m in (3, 12, 13, 14, 7, 10, 8, 9, 20):
            self.assertFalse(G.prog_case(guard(5), atk(m), 1, 0)["blocked"], m)  # Absorb: close range, Rain, mines, waves
        self.assertFalse(G.prog_case(guard(5), atk(0, state3=True), 1, 0)["blocked"])  # ... and a blast
        self.assertFalse(G.prog_case(guard(3), atk(0, state3=True), 1, 0)["blocked"])  # Brush: a blast
        self.assertTrue(G.prog_case(guard(3), atk(12), 1, 0)["blocked"])  # Brush stops (and stuns) a sword

    def test_front_kinds_cover_the_front_and_round_kinds_everything(self):
        self.assertFalse(G.prog_case(guard(0), atk(), 1, 1)["blocked"])  # a Barrier from behind: nothing
        self.assertTrue(G.prog_case(guard(1), atk(), 1, 1)["blocked"])  # a Shelter from behind
        self.assertTrue(G.prog_case(guard(5), atk(0), 1, 1)["blocked"])  # Absorb is a sphere in the contact test
        self.assertIsNone(G.prog_case(guard(0), atk(15), 1, 2))  # a Hole against a disc: geometry not read

    def test_reflect_and_about_face_send_back_only_the_listed_motions(self):
        self.assertTrue(G.prog_case(guard(4), atk(0), 1, 0)["reflect"])
        self.assertTrue(G.prog_case(guard(4), atk(6), 1, 0)["reflect"])
        self.assertFalse(G.prog_case(guard(4), atk(1), 1, 0)["reflect"])  # a lob is erased, not reflected
        self.assertFalse(G.prog_case(guard(4), atk(0, state3=True), 1, 0)["reflect"])  # a blast neither
        for m in (0, 1, 2, 5, 6, 11):
            p = G.prog_case(guard(6, d=0), atk(m), 1, 0)
            self.assertTrue(p["turned"] and p["def_after"] == 0 and not p["lingering"], m)
        self.assertFalse(G.prog_case(guard(6, d=0), atk(12), 1, 0)["blocked"])
        self.assertFalse(G.prog_case(guard(6, d=0), atk(0, state3=True), 1, 0)["blocked"])

    def test_brush_stuns_only_motion_3_12_13(self):
        for m, stun in ((3, True), (12, True), (13, True), (14, False), (0, False)):
            self.assertEqual(G.prog_case(guard(3), atk(m), 1, 0)["stun"], stun, m)
        self.assertTrue(G.prog_case(guard(3), atk(12), 9, 0)["stun"])  # also when it breaks

    def test_the_add_effect_gate(self):
        self.assertTrue(G.prog_case(guard(0, add=3), atk(0), 1, 0)["add"])
        self.assertFalse(G.prog_case(guard(0, add=3, short_only=True), atk(0), 1, 0)["add"])  # s50 = 1: short range only
        self.assertTrue(G.prog_case(guard(0, add=3, short_only=True), atk(14), 1, 0)["add"])
        self.assertFalse(G.prog_case(guard(0, add=3), atk(18), 1, 0)["add"])

    def test_course_stops_nothing(self):
        p = G.prog_case(guard(2, d=0), atk(), 3, 0)
        self.assertFalse(p["blocked"])

    def O_cases(self, rows):
        atks = [{"label": "Shot", "id": 1, "arrival": "Shot", "above": False, "state3": False}]
        return {"guardbat": {"attackers": atks, "cases": rows}}

    def obs(self, **kw):
        o = {"blocked": True, "broken": False, "hit": False, "vhealth": 0, "vaura": 0.0, "guard": {"def": 1, "lingering": True}, "slot0": 26, "reflected": 0,
             "stunned": False, "a_bound": 0, "a_frozen": 0, "a_poison": 0, "a_slot0": 1}
        o.update(kw)
        return o

    def test_the_battery_cells_say_ok_or_cross(self):
        rec = mk({44: 0, 47: 3, 41: 0}, capsule="defence", sid=26)
        class Prog:
            rec = {1: mk({63: 0, 54: 0}, sid=1)}

        good = self.O_cases([[0, 2, 0, self.obs()]])
        self.assertEqual({c.key: c.st for c in G.chk_guard_battery(Prog, rec, good)}["bat_block"], R.OK)
        bad = self.O_cases([[0, 2, 0, self.obs(guard={"def": 2, "lingering": True})]])  # ours drained too little
        cells = {c.key: c for c in G.chk_guard_battery(Prog, rec, bad)}
        self.assertEqual(cells["bat_block"].st, R.BAD)
        self.assertIn("def_left", cells["bat_block"].note)

    def test_a_shots_blast_has_its_own_cell(self):
        rec = mk({44: 5, 47: 3}, capsule="defence", sid=27)

        class Prog:
            rec = {1: mk({63: 1, 54: 0}, sid=1)}

        atks = [{"label": "Bomb blast", "id": 1, "arrival": "Shot", "above": False, "state3": True}]
        # the program lets a blast through an Absorb (full damage 2); ours stops it
        O = {"guardbat": {"attackers": atks, "cases": [[0, 2, 0, self.obs(blocked=True)]]}}
        cells = {c.key: c for c in G.chk_guard_battery(Prog, rec, O)}
        self.assertEqual(cells["bat_blast"].st, R.BAD)
        self.assertNotIn("bat_block", cells)


class WaveSixReads(unittest.TestCase):
    def test_the_own_effect_net_is_payment_then_effect(self):
        # Level Amp: cost 3, Level + 2 (kind 1): Aura 4 -> 1, Level 10 -> 12
        rec = mk({22: 3, 43: 4, 45: 1, 49: 2}, capsule="special")
        snap = lambda lv, au, h: {"level": lv, "aura_ticks": int(au * R.APORA), "health": h}
        e = {"started": True, "before": [snap(10, 4, 10), snap(8, 5, 14)], "at_fire": [snap(12, 1, 10), snap(8, 5, 14)]}
        net = R._own_net(rec, e)
        self.assertTrue(all(c.st == R.OK for c in net.values()), net)
        e["at_fire"][0]["level"] = 11
        self.assertEqual(R._own_net(rec, e)["level"].st, R.BAD)
        # Rebirth: Level to the minimum (s23 < 0), then Health + 10 capped at 20
        rec = mk({22: 0, 23: -1, 43: 4, 45: 3, 49: 10}, capsule="special")
        e = {"started": True, "before": [snap(10, 4, 8), snap(8, 5, 14)], "at_fire": [snap(0, 0, 18), snap(8, 5, 14)]}
        self.assertTrue(all(c.st == R.OK for c in R._own_net(rec, e).values()))

    def test_requirement_5_counts_capsules(self):
        rec = mk({38: 5, 39: 7})
        q = {"deck_n": True, "deck_n_plus_1": False, "deck_n_minus_1_capsule": True, "deck_n_capsule": False, "deck_n_minus_2_two_capsules": True, "deck_n_minus_1_two_capsules": False}
        self.assertEqual(R._req5_cell(rec, q, False).st, R.OK)
        q["deck_n_capsule"] = True  # ours ignores the capsule on the field
        c = R._req5_cell(rec, q, False)
        self.assertEqual(c.st, R.BAD)
        self.assertIn("deck_n_capsule", c.note)

    def test_str_rule_21(self):
        charged = mk({45: 21, 46: 1}, ground=row(charge=13))
        runs = [{"hold": h, "level": 20, "hit_step": 100, "damage": d} for h, d in ((76, 2), (116, 4), (156, 6))]
        self.assertEqual(R._str21_cell(None, charged, {"kind": "charge", "runs": runs}, "x").st, R.OK)
        runs[1]["damage"] = 2  # ours stopped charging
        self.assertEqual(R._str21_cell(None, charged, {"kind": "charge", "runs": runs}, "x").st, R.BAD)
        finisher = mk({45: 21, 46: 2, 23: -1})
        lv = [{"hold": 0, "level": L, "hit_step": 50, "damage": 2 * L} for L in (3, 8)]
        self.assertEqual(R._str21_cell(None, finisher, {"kind": "level", "runs": lv}, "x").st, R.OK)

    def test_automatic_shelters_are_one_block_objects(self):
        rec = mk({22: 3, 25: 0, 26: 1, 47: 3}, capsule="defence", sid=95)
        first = {"blocked": True, "vhealth": 0, "vaura": -3.0, "slot0": None, "broken": False}
        second_standing = {"blocked": True, "vhealth": 0, "vaura": 0.0}
        run = {"gap": 2, "first": first, "second": second_standing, "up_for": 10, "before": {"health": 20, "aura": 8.0}, "after": {"health": 20, "aura": 5.0}}
        O = {"auto": {"runs": [run], "strong": {"strength": 5, "first": {"blocked": True, "broken": True}}}}
        cells = {c.key: c for c in G.chk_auto(None, rec, O)}
        self.assertEqual(cells["auto_first"].st, R.OK)
        self.assertEqual(cells["auto_life"].st, R.BAD)  # ours stands 10 ticks, the program's object ends at its first update
        self.assertEqual(cells["auto_second"].st, R.BAD)  # a used-up skill cannot block again
        run["second"] = {"blocked": False, "vhealth": -1, "vaura": 0.0}
        run["after"]["health"] = 19  # the second shot landed
        run["up_for"] = 2
        cells = {c.key: c for c in G.chk_auto(None, rec, O)}
        self.assertTrue(all(c.st == R.OK for c in cells.values()), cells)

    def test_the_orb_is_paid_by_the_holder(self):
        rec = mk({22: 2}, capsule="special", sid=180)
        free = {"runs": [{"aura": 8, "before": {"aura": 8.0, "level": 8}, "after": {"aura": 8.0, "level": 8}},
                         {"aura": 0, "before": {"aura": 0.0, "level": 8}, "after": {"aura": 0.1, "level": 8}}]}
        cells = {c.key: c for c in G.chk_orb(None, rec, {"orb": free})}
        self.assertEqual(cells["orb_pay"].st, R.BAD)
        self.assertEqual(cells["orb_effect"].st, R.TAKE)
        free["runs"][0]["after"]["aura"] = 6.0
        self.assertEqual({c.key: c for c in G.chk_orb(None, rec, {"orb": free})}["orb_pay"].st, R.OK)

    def test_the_builtin_decks_are_checked_against_the_table_index_reading(self):
        doc = {"skills": [{"id": 7, "record": 40}, {"id": 18, "record": 40 + 1}, {"id": 41, "record": 3}],
               "decks": [{"name": "psycho", "skills": [42, -1, 41]}], "quick_decks": [{"name": "Q000", "kind": 2, "skills": [42, 4]}]}
        # table index 42 = record 41 = id 18; 41 = record 40 = id 7; 4 = record 3 = id 41
        ours = {"builtin_decks": [{"name": "Psycho", "skills": [18, 7] + [0] * 28}, {"name": "Quick Q000", "skills": [18, 41]}]}
        self.assertTrue(A.index_check(doc, ours)["ok"])
        ours["builtin_decks"][1]["skills"] = [42, 4]  # the old reading: the raw entries kept as ids
        r = A.index_check(doc, ours)
        self.assertFalse(r["ok"])
        self.assertEqual(r["total"] - r["match"], 2)
        self.assertIsNone(A.index_check(doc, {}))


@unittest.skipUnless(os.environ.get("PD_SKILL_AUDIT_OURS") and (A.DEFAULT_CACHE / "skills.json").exists(), "needs a probe dump (PD_SKILL_AUDIT_OURS) and the converted cache")
class LiveSpotCheck(unittest.TestCase):
    """The tool must agree with what the docs record for the dozen skills verified one by one."""

    @classmethod
    def setUpClass(cls):
        cls.P = A.load_program(A.DEFAULT_CACHE)
        ours = A.load_json(os.environ["PD_SKILL_AUDIT_OURS"])
        usage, invalid, n = A.usage_table(cls.P.doc, A.find_game_decks())
        cls.rows = A.build_rows(cls.P, ours, usage)

    def test_the_dozen_agree_with_the_docs(self):
        res = REP.spot_check(self.rows)
        self.assertGreater(len(res), 30)
        for sid, name, key, doc, prog, ours, st, where, prog_ok, ours_ok in res:
            self.assertTrue(prog_ok, "%s %s: the tool derives %r, the docs record %r (%s)" % (name, key, prog, doc, where))
            self.assertTrue(ours_ok or (sid, key) in REP.EXPLAINED, "%s %s: ours %r, docs %r" % (name, key, ours, doc))

    def test_no_comparator_crashed_and_the_table_matches_the_game_file(self):
        for r in self.rows:
            for c in r.cells:
                self.assertNotEqual(c.key, "tool_error", "%s: %s" % (r.name, c.note))
                if c.ch == "table":
                    self.assertEqual(c.st, R.OK, "%s table.%s" % (r.name, c.key))

    def test_the_known_crosses_are_still_found(self):
        by = {r.id: r for r in self.rows}
        keys = lambda sid: {"%s.%s" % (c.ch, c.key) for c in by[sid].bad()}
        for sid in (78, 192):  # lane instants: rule 26 sums by the record's s10 field now
            self.assertNotIn("str.bundle", keys(sid))
        for sid in (239, 115, 56, 51):  # Jump, Optical Camo, Levitate, Skywalk fire in the press, no hold
            self.assertNotIn("cast.hold", keys(sid))
        self.assertIn("cast.hold", keys(0))  # the Aura Particle hold
        self.assertNotIn("cast.clip", keys(257))  # lane loaders: the first record of an id wins, Judge of Anubis casts its own row
        self.assertNotIn("cast.charged", keys(334))  # lane moveskills: Dragon Bullet is a charge skill now

    def test_the_wave_6_findings(self):
        """What the guard battery / auto probes found (flip each line when its fix lane merges)."""
        by = {r.id: r for r in self.rows}
        keys = lambda sid: {"%s.%s" % (c.ch, c.key) for c in by[sid].bad()}
        self.assertIn("guard.auto_second", keys(95))  # AUTOSHELTER: Auto Shell is a one-block object in the program
        self.assertIn("guard.orb_pay", keys(180))  # ORBPAY
        self.assertIn("guard.bat_aura_gain", keys(212))  # GUARDADD: Absorb Energy gains the attack's STR
        self.assertIn("guard.bat_brush_pursuit", keys(210))  # GUARDADD: a Brush does not stun a Pursuit
        self.assertIn("guard.bat_blast", keys(33))  # BLAST3: About Face lets a blast through
        self.assertIn("pay.requirement", keys(14))  # REQ5: Rail Gun
        self.assertIn("cast.empty_no_sound", keys(206))  # COPYNO
        # and what matched: the other 30-odd guards survive the whole battery, rule 21 and the copy timing
        for sid in (26, 27, 88, 91, 150, 152, 89):  # Crystal Wall, Wall, Laser Barrier, Paralyze Barrier, Wall of Current, Cyclone, Eraser
            self.assertFalse({k for k in keys(sid) if k.startswith("guard.bat")}, sid)
        for sid in (73, 74, 195, 204):  # Gungnir, Finisher, Ki Palm, Ki Cannon
            self.assertNotIn("str.damage", keys(sid))
        self.assertFalse({k for k in keys(205) | keys(206) if k.startswith("cast.") and k != "cast.empty_no_sound"})


if __name__ == "__main__":
    unittest.main()
