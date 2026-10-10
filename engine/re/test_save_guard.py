"""Tests for the save-safety tool (temp folders only; the real package folder is never touched)."""
import os
import tempfile
import unittest

import save_guard as sg


def put(root, rel, data):
    p = os.path.join(root, *rel.split("/"))
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "wb") as f:
        f.write(data)


def get(root, rel):
    with open(os.path.join(root, *rel.split("/")), "rb") as f:
        return f.read()


class SaveGuard(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.live = os.path.join(self.tmp.name, "live")
        self.ref = os.path.join(self.tmp.name, "ref")
        for root in (self.live, self.ref):
            put(root, "SystemAppData/wgs/a/blob1", b"one")
            put(root, "SystemAppData/wgs/a/container.55", b"c55")
            put(root, "AC/cache/x", b"cache")

    def tearDown(self):
        self.tmp.cleanup()

    def test_identical_folders_compare_clean(self):
        self.assertEqual(sg.compare(self.live, self.ref), ([], [], []))
        self.assertEqual(sg.save_differences(self.live, self.ref), [])

    def test_changed_missing_and_extra_files_are_found(self):
        put(self.live, "SystemAppData/wgs/a/blob1", b"CHANGED")
        os.remove(os.path.join(self.live, "SystemAppData", "wgs", "a", "container.55"))
        put(self.live, "SystemAppData/wgs/a/container.57", b"new")
        changed, missing, extra = sg.compare(self.live, self.ref)
        self.assertEqual(changed, ["SystemAppData/wgs/a/blob1"])
        self.assertEqual(missing, ["SystemAppData/wgs/a/container.55"])
        self.assertEqual(extra, ["SystemAppData/wgs/a/container.57"])

    def test_cache_files_do_not_count_as_the_save(self):
        put(self.live, "AC/cache/x", b"other cache")
        self.assertEqual(sg.save_differences(self.live, self.ref), [])

    def test_restore_copies_back_but_never_deletes(self):
        put(self.live, "SystemAppData/wgs/a/blob1", b"CHANGED")
        os.remove(os.path.join(self.live, "SystemAppData", "wgs", "a", "container.55"))
        put(self.live, "SystemAppData/wgs/a/container.57", b"new")
        put(self.live, "AC/cache/x", b"other cache")
        done = sg.restore(self.live, self.ref)
        self.assertEqual(sorted(done), ["SystemAppData/wgs/a/blob1", "SystemAppData/wgs/a/container.55"])
        self.assertEqual(get(self.live, "SystemAppData/wgs/a/blob1"), b"one")
        self.assertEqual(get(self.live, "SystemAppData/wgs/a/container.55"), b"c55")
        self.assertEqual(get(self.live, "SystemAppData/wgs/a/container.57"), b"new")   # extra file kept
        self.assertEqual(get(self.live, "AC/cache/x"), b"other cache")                  # cache left alone
        self.assertEqual(sg.save_differences(self.live, self.ref), [])

    def test_snapshot_copies_every_file(self):
        dest = os.path.join(self.tmp.name, "snap")
        self.assertEqual(sg.snapshot(self.live, dest), 3)
        self.assertEqual(sg.compare(self.live, dest), ([], [], []))

    def test_main_reports_and_exit_code(self):
        self.assertEqual(sg.main(["--live", self.live, "--ref", self.ref]), 0)
        put(self.live, "SystemAppData/wgs/a/blob1", b"CHANGED")
        self.assertEqual(sg.main(["--live", self.live, "--ref", self.ref]), 1)
        self.assertEqual(sg.main(["--live", self.live, "--ref", self.ref, "--restore"]), 0)


if __name__ == "__main__":
    unittest.main()
