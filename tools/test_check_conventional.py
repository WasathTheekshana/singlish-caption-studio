import os
import tempfile
import unittest

import check_conventional as cc


class Titles(unittest.TestCase):
    def test_good_titles(self):
        for title in [
            "feat: add a usage tab",
            "fix: keep captions from overlapping",
            "feat(ui): add a usage tab",
            "fix(audio): handle files without sound",
            "feat!: change the settings file format",
            "refactor(core)!: rename the draft module",
            "build(deps): bump sha2 from 0.10.9 to 0.11.1",
            "ci: pin the release action",
            "docs: explain the release flow",
            "chore: release 0.1.0",
            "revert: feat(ui): add a usage tab",
            "perf(audio): resample without a copy",
            "test: run the navigation checks",
            "style: format the ui files",
        ]:
            self.assertEqual(cc.problems(title), [], title)

    def test_bad_titles(self):
        for title in [
            "Update stuff",
            "Add the usage tab",
            "feat:",
            "feat: ",
            "feat : spaced colon",
            "feat:no space",
            "Feat: capital type",
            "feature: not a known type",
            "feat(UPPER): scope must be lowercase",
            "feat(): empty scope",
            "feat(ui) missing colon",
            "",
        ]:
            self.assertNotEqual(cc.problems(title), [], title)

    def test_length_and_period(self):
        self.assertTrue(any("characters" in p for p in cc.problems("feat: " + "x" * 80)))
        self.assertTrue(any("period" in p for p in cc.problems("fix: all done.")))
        self.assertEqual(cc.problems("fix: " + "x" * (cc.MAX_LENGTH - 5)), [])  # exactly at the limit

    def test_git_generated_messages_are_only_allowed_for_commits(self):
        for msg in ["Merge branch 'main' into feature", 'Revert "feat: add a thing"', "fixup! feat: add a thing"]:
            self.assertEqual(cc.problems(msg, allow_git_generated=True), [], msg)
            self.assertNotEqual(cc.problems(msg, allow_git_generated=False), [], msg)


class CommitFiles(unittest.TestCase):
    def write(self, text):
        fd, path = tempfile.mkstemp(suffix=".txt")
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as f:
            f.write(text)
        self.addCleanup(os.remove, path)
        return path

    def test_comments_and_blank_lines_are_skipped(self):
        path = self.write("\n# a template comment\n# another\nfix(core): keep the lock\n\nbody text\n")
        self.assertEqual(cc.first_line_of_commit_file(path), "fix(core): keep the lock")
        self.assertEqual(cc.main(["--commit-file", path]), 0)

    def test_windows_line_endings(self):
        path = self.write("feat: add a thing\r\n\r\nbody\r\n")
        self.assertEqual(cc.first_line_of_commit_file(path), "feat: add a thing")
        self.assertEqual(cc.main(["--commit-file", path]), 0)

    def test_an_empty_message_is_rejected(self):
        self.assertEqual(cc.main(["--commit-file", self.write("# only comments\n")]), 1)

    def test_a_bad_commit_is_rejected(self):
        self.assertEqual(cc.main(["--commit-file", self.write("Add the thing\n")]), 1)


class Command(unittest.TestCase):
    def test_title_exit_codes(self):
        self.assertEqual(cc.main(["--title", "feat: add a thing"]), 0)
        self.assertEqual(cc.main(["--title", "Add a thing"]), 1)
        self.assertEqual(cc.main(["--title", ""]), 1)

    def test_only_the_first_line_of_a_title_counts(self):
        self.assertEqual(cc.main(["--title", "feat: add a thing\nnot checked"]), 0)


if __name__ == "__main__":
    unittest.main()
