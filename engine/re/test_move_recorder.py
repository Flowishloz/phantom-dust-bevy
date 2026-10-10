"""Tests for the move recorder's pure parts: script parser, timeline, pad packing, lists, owned
inventory rows and the capture command. None of these touch the game."""
import csv
import os
import struct
import tempfile
import unittest

import move_recorder as mr


def script(**kw):
    d = {"name": "Test", "skill": 65, "timeline": [{"t": 0.5, "do": "press", "buttons": "Y"}]}
    d.update(kw)
    return d


class PadBits(unittest.TestCase):
    def test_button_mask_combines_and_is_case_insensitive(self):
        self.assertEqual(mr.button_mask(["a", "RT"]), 0x10 | 0x800)
        self.assertEqual(mr.button_mask([]), 0)

    def test_unknown_button_rejected(self):
        with self.assertRaises(ValueError):
            mr.button_mask(["Z"])

    def test_skill_buttons_match_the_input_block_read_live(self):
        # slot masks read from a live fighter: 0x100, 0x80, 0x20, 0x10
        self.assertEqual(mr.SLOT_BUTTON, (0x100, 0x80, 0x20, 0x10))

    def test_stick_int_scales_and_clamps(self):
        self.assertEqual(mr.stick_int(1.0), 32767)
        self.assertEqual(mr.stick_int(-1.0), -32767)
        self.assertEqual(mr.stick_int(7.0), 32767)
        self.assertEqual(mr.stick_int(0.0), 0)

    def test_pack_pad_is_20_bytes_in_the_games_field_order(self):
        raw = mr.pack_pad(0x110, 5, -6, 7, -8)
        self.assertEqual(len(raw), 20)
        self.assertEqual(struct.unpack("<Iiiii", raw), (0x110, 5, -6, 7, -8))

    def test_pad_object_addresses(self):
        self.assertEqual(mr.pad_object_addr(0), 0x1405A5CE0)
        self.assertEqual(mr.pad_object_addr(1), 0x1405A5D10)
        # sticks of pad 0 are the DAT_1405a5d00 .. d0c the program reads
        self.assertEqual(mr.pad_object_addr(0) + mr.PAD_STICK_OFF, 0x1405A5D00)


class Parser(unittest.TestCase):
    def test_minimal_script(self):
        sc = mr.parse_script(script())
        self.assertEqual((sc.name, sc.skill, sc.slot), ("Test", 65, 0))
        # lead-in 1.0 s + 0.5 s = tick 45; press defaults to 0.1 s = 3 ticks
        e = sc.events[0]
        self.assertEqual((e.start, e.length, e.kind), (45, 3, "press"))
        # recorded until 2.0 s after the last event (tail)
        self.assertEqual(sc.record_frames, 48 + 60)

    def test_explicit_record_length_wins(self):
        sc = mr.parse_script(script(record=10.0))
        self.assertEqual(sc.record_frames, 300)

    def test_lead_in_shifts_every_event(self):
        sc = mr.parse_script(script(lead_in=0.0))
        self.assertEqual(sc.events[0].start, 15)

    def test_missing_skill_rejected(self):
        with self.assertRaises(mr.ScriptError):
            mr.parse_script({"timeline": []})

    def test_aura_particle_id_zero_is_allowed(self):
        self.assertEqual(mr.parse_script(script(skill=0)).skill, 0)

    def test_bad_skill_values(self):
        for bad in (-1, "x", None):
            with self.assertRaises(mr.ScriptError):
                mr.parse_script(script(skill=bad))

    def test_slot_range(self):
        with self.assertRaises(mr.ScriptError):
            mr.parse_script(script(slot=4))
        self.assertEqual(mr.parse_script(script(slot=3)).slot, 3)

    def test_unknown_option_rejected(self):
        with self.assertRaises(mr.ScriptError):
            mr.parse_script(script(colour="red"))

    def test_only_still_opponent_is_built(self):
        with self.assertRaises(mr.ScriptError):
            mr.parse_script(script(opponent="walk"))

    def test_event_validation(self):
        bad = [{"do": "press", "buttons": "A"},                      # no time
               {"t": 0, "do": "dance"},                                # unknown kind
               {"t": 0, "do": "hold", "buttons": "A"},                 # hold needs a length
               {"t": 0, "do": "press"},                                # press needs buttons
               {"t": 0, "do": "press", "buttons": "Q"},                # unknown button
               {"t": 0, "do": "stick", "x": 2.0, "for": 1},            # out of range
               {"t": -1, "do": "press", "buttons": "A"}]               # negative time
        for ev in bad:
            with self.assertRaises(mr.ScriptError, msg=str(ev)):
                mr.parse_script(script(timeline=[ev]))

    def test_buttons_can_be_written_with_plus(self):
        sc = mr.parse_script(script(timeline=[{"t": 0, "do": "hold", "buttons": "RT+Y", "for": 1}]))
        self.assertEqual(sc.events[0].buttons, ("RT", "Y"))

    def test_nothing_to_record_rejected(self):
        with self.assertRaises(mr.ScriptError):
            mr.parse_script({"skill": 1, "timeline": [], "tail": 0, "lead_in": 0})

    def test_attempts_and_level_range(self):
        with self.assertRaises(mr.ScriptError):
            mr.parse_script(script(attempts=0))
        with self.assertRaises(mr.ScriptError):
            mr.parse_script(script(level=100))

    def test_opponent_timeline_is_parsed_and_shifted(self):
        sc = mr.parse_script(script(opponent_skill=1, opponent_timeline=[{"t": 0.0, "do": "press", "buttons": "Y"}]))
        self.assertEqual(sc.opponent_events[0].start, 30)
        self.assertGreaterEqual(sc.record_frames, 30 + 3)

    def test_load_script_reads_json_file(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "s.json")
            with open(p, "w") as f:
                f.write('{"skill": 3, "name": "Knife", "timeline": [{"t": 0, "do": "press", "buttons": "X"}]}')
            sc = mr.load_script(p)
            self.assertEqual((sc.skill, sc.name), (3, "Knife"))


class Timeline(unittest.TestCase):
    def ev(self, *items):
        return [mr.parse_event(i) for i in items]

    def test_press_is_active_only_inside_its_window(self):
        evs = self.ev({"t": 1.0, "do": "press", "buttons": "Y", "for": 0.1})
        self.assertEqual(mr.pad_state_at(evs, 29).mask, 0)
        self.assertEqual(mr.pad_state_at(evs, 30).mask, 0x100)
        self.assertEqual(mr.pad_state_at(evs, 32).mask, 0x100)
        self.assertEqual(mr.pad_state_at(evs, 33).mask, 0)

    def test_holds_overlap_by_or(self):
        evs = self.ev({"t": 0, "do": "hold", "buttons": "RT", "for": 2},
                      {"t": 1, "do": "press", "buttons": "X"})
        self.assertEqual(mr.pad_state_at(evs, 31).mask, 0x800 | 0x80)
        self.assertEqual(mr.pad_state_at(evs, 10).mask, 0x800)

    def test_release_clears_a_held_button(self):
        evs = self.ev({"t": 0, "do": "hold", "buttons": "RT", "for": 3},
                      {"t": 1, "do": "release", "buttons": "RT", "for": 1})
        self.assertEqual(mr.pad_state_at(evs, 20).mask, 0x800)
        self.assertEqual(mr.pad_state_at(evs, 35).mask, 0)
        self.assertEqual(mr.pad_state_at(evs, 70).mask, 0x800)

    def test_stick_values_and_later_event_wins(self):
        evs = self.ev({"t": 0, "do": "stick", "x": 0, "y": 1, "for": 2},
                      {"t": 1, "do": "stick", "x": 1, "y": 0, "for": 2})
        a = mr.pad_state_at(evs, 10)
        self.assertEqual((a.lx, a.ly), (0, 32767))
        b = mr.pad_state_at(evs, 40)
        self.assertEqual((b.lx, b.ly), (32767, 0))

    def test_right_stick_is_independent(self):
        evs = self.ev({"t": 0, "do": "rstick", "x": -1, "y": 0.5, "for": 1})
        s = mr.pad_state_at(evs, 3)
        self.assertEqual((s.lx, s.ly, s.rx), (0, 0, -32767))

    def test_base_mask_is_kept(self):
        self.assertEqual(mr.pad_state_at([], 0, base_mask=0x800).mask, 0x800)

    def test_marks_are_sorted(self):
        evs = self.ev({"t": 2, "do": "mark", "label": "b"}, {"t": 1, "do": "mark", "label": "a"})
        self.assertEqual(mr.marks(evs), [(30, "a"), (60, "b")])

    def test_rotate_turns_sticks_by_quarter_turns(self):
        evs = self.ev({"t": 0, "do": "stick", "x": 0, "y": 1, "for": 1}, {"t": 0, "do": "press", "buttons": "Y"})
        r1 = mr.rotate_events(evs, 1)
        self.assertEqual((r1[0].x, r1[0].y), (-1.0, 0.0))
        r2 = mr.rotate_events(evs, 2)
        self.assertEqual((r2[0].x, r2[0].y), (0.0, -1.0))
        self.assertEqual(mr.rotate_events(evs, 4)[0].y, 1.0)
        self.assertEqual(r1[1].buttons, ("Y",))

    def test_seconds_round_to_ticks(self):
        self.assertEqual(mr.secs_to_frames(0.1), 3)
        self.assertEqual(mr.secs_to_frames(1 / 30.0), 1)
        self.assertEqual(mr.secs_to_frames(2.0), 60)


class Lists(unittest.TestCase):
    def test_output_stem_pads_the_id_and_cleans_the_name(self):
        self.assertEqual(mr.output_stem(5, "Psycho Knife"), "005_Psycho_Knife")
        self.assertEqual(mr.output_stem(0, "Aura Particle (held)"), "000_Aura_Particle__held")
        self.assertEqual(mr.output_stem(139, "???"), "139_skill")

    def test_read_skill_list_skips_comments_and_blank_lines(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "l.csv")
            with open(p, "w") as f:
                f.write("# my list\nid,name,script\n\n65,Laser,\n3,Psycho Knife,x.json\n")
            rows = mr.read_skill_list(p)
            self.assertEqual([r["id"] for r in rows], [65, 3])
            self.assertEqual(rows[1]["script"], "x.json")

    def test_skill_list_needs_an_id_column(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "l.csv")
            with open(p, "w") as f:
                f.write("name\nLaser\n")
            with self.assertRaises(mr.ScriptError):
                mr.read_skill_list(p)

    def test_default_script_depends_on_the_capsule_kind(self):
        atk = mr.parse_script(mr.default_script_for({"id": 65, "name": "Laser", "capsule": "attack"}))
        self.assertTrue(atk.lock)
        stat = mr.parse_script(mr.default_script_for({"id": 49, "name": "Speed Up", "capsule": "status"}, slot=2))
        self.assertFalse(stat.lock)
        self.assertEqual(stat.events[0].buttons, ("B",))   # slot 2 = B

    def test_pilot_scripts_parse(self):
        skills = [{"id": i, "name": n, "capsule": c} for i, n, c in
                  [(65, "Laser", "attack"), (3, "Psycho Knife", "attack"), (80, "Blaster", "attack"), (49, "Speed Up", "status")]]
        scripts = [mr.parse_script(d) for d in mr.pilot_script_dicts(skills)]
        self.assertEqual([s.skill for s in scripts], [65, 3, 80, 49, 0])
        aura = scripts[-1]
        self.assertTrue(any(e.kind == "stick" for e in aura.events))   # the stick is held
        self.assertEqual(aura.attempts, 4)


class Inventory(unittest.TestCase):
    SKILLS = [{"id": 65, "record": 41, "text_id": 42, "name": "Laser", "capsule": "attack", "school": "optical"},
              {"id": 301, "record": 300, "text_id": 304, "name": "Delay Bomb", "capsule": "attack", "school": "x"},
              {"id": 294, "record": 242, "text_id": 243, "name": "Fusion", "capsule": "special", "school": "x"},
              {"id": 294, "record": 750, "text_id": 751, "name": "", "capsule": "special", "school": "x"},
              {"id": 3, "record": 2, "text_id": 3, "name": "Psycho Knife", "capsule": "attack", "school": "psycho"}]

    def test_game_index_is_record_plus_one_not_text_id(self):
        self.assertEqual(mr.game_index(self.SKILLS[0]), 42)
        self.assertEqual(mr.game_index(self.SKILLS[1]), 301)       # text_id says 304: wrong above id 300

    def test_duplicate_ids_keep_the_real_skill_not_the_dodge_record(self):
        u = {s["id"]: s for s in mr.unique_skills(self.SKILLS)}
        self.assertEqual(len(u), 4)
        self.assertEqual(mr.game_index(u[294]), 243)                # not 751

    def test_owned_rows_read_the_stock_at_the_game_index(self):
        counts = [0] * 800
        counts[42] = 2
        counts[301] = 1
        counts[65] = 9           # the catalogue id slot must NOT be used
        rows = {r[0]: r for r in mr.owned_rows(counts, self.SKILLS)}
        self.assertEqual(rows[65][:4], (65, 42, "Laser", 2))
        self.assertEqual(rows[301][3], 1)
        self.assertEqual(rows[294][1], 243)
        self.assertEqual(len(rows), 4)

    def test_negative_counts_read_as_zero(self):
        counts = [0] * 800
        counts[3] = -1
        self.assertEqual({r[0]: r for r in mr.owned_rows(counts, self.SKILLS)}[3][3], 0)

    def test_write_owned_csv(self):
        counts = [0] * 800
        counts[42] = 2
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "o", "owned.csv")
            self.assertEqual(mr.write_owned_csv(p, counts, self.SKILLS), (1, 3))
            rows = list(csv.reader(open(p, newline="")))
            self.assertEqual(rows[0], ["id", "game_index", "name", "count_owned", "capsule", "school"])
            self.assertEqual(rows[2][:4], ["65", "42", "Laser", "2"])


class TakeSummary(unittest.TestCase):
    def rows(self, spec):
        return [{"rec_tick": str(t), "events": e, "p1_health": str(h)} for t, e, h in spec]

    def test_press_fire_hit_and_index(self):
        rows = self.rows([(0, "", 40), (39, "press:5.42", 40), (52, "fire:5.42|level:1.0", 40), (60, "", 36)])
        s = mr.take_summary(rows, 42)
        self.assertEqual((s["press_tick"], s["fire_tick"], s["hit_tick"]), (39, 52, 60))
        self.assertEqual((s["fire_index"], s["index_ok"]), (42, True))

    def test_wrong_skill_is_flagged(self):
        s = mr.take_summary(self.rows([(0, "press:5.42", 40), (5, "fire:5.7", 40)]), 42)
        self.assertIs(s["index_ok"], False)

    def test_refused_press(self):
        s = mr.take_summary(self.rows([(0, "sound:0.2", 40), (5, "", 40)]), 42)
        self.assertEqual((s["pressed"], s["fired"], s["index_ok"]), (False, False, None))


class Capture(unittest.TestCase):
    def test_capture_command_targets_the_game_window_by_hwnd(self):
        cmd = mr.build_capture_command("ffmpeg.exe", 21563566, "out.mp4", 30)
        src = cmd[cmd.index("-i") + 1]
        self.assertIn("gfxcapture=hwnd=21563566", src)
        self.assertNotIn("window_exe", src)      # a hosted UWP window is not found by exe name
        self.assertNotIn("desktop", src)
        self.assertEqual(cmd[-1], "out.mp4")
        self.assertIn("libx264", cmd)

    def test_stop_does_not_write_to_an_ffmpeg_that_already_quit(self):
        class Dead:
            stdin = None            # would raise if touched
            def poll(self): return 1
        c = mr.Capture("nonexistent_dir/none.mp4", hwnd=1)
        c.proc = Dead()
        self.assertFalse(c.stop())

    def test_trace_columns_have_both_fighters_and_events(self):
        cols = mr.TRACE_COLUMNS
        self.assertEqual(cols[:3], ["rec_tick", "tick", "ms"])
        self.assertIn("p0_state", cols)
        self.assertIn("p1_health", cols)
        self.assertEqual(cols[-1], "events")
        self.assertEqual(len(set(cols)), len(cols))


class Placement(unittest.TestCase):
    def test_first_direction_points_at_the_other_start_and_there_are_eight_more(self):
        d = mr.candidate_directions((0, 5, 0), (0, 5, 10))
        self.assertEqual(len(d), 9)
        self.assertAlmostEqual(d[0][0], 0.0)
        self.assertAlmostEqual(d[0][1], 1.0)
        for ux, uz in d:
            self.assertAlmostEqual(ux * ux + uz * uz, 1.0)

    def test_same_start_points_do_not_divide_by_zero(self):
        d = mr.candidate_directions((1, 1, 1), (1, 1, 1))
        self.assertEqual(d[0], (0.0, 0.0))


if __name__ == "__main__":
    unittest.main()
