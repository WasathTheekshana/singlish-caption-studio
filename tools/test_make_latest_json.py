import json
import os
import tempfile
import unittest

import make_latest_json as m

SIG = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTS3p5eDdjUlFtTA=="
GOOD = dict(
    version="0.2.0",
    repo="WasathTheekshana/singlish-caption-studio",
    tag="singlish-caption-studio-v0.2.0",
    asset="Singlish-Caption-Studio_0.2.0_x64-setup.exe",
    signature=SIG,
    notes="### Features\n\n* a new thing",
)


class Manifest(unittest.TestCase):
    def test_shape_matches_what_the_updater_reads(self):
        out = m.build_manifest(**GOOD, pub_date="2026-11-08T10:00:00Z")
        self.assertEqual(out["version"], "0.2.0")
        self.assertEqual(out["pub_date"], "2026-11-08T10:00:00Z")
        entry = out["platforms"]["windows-x86_64"]
        self.assertEqual(entry["signature"], SIG)
        self.assertEqual(
            entry["url"],
            "https://github.com/WasathTheekshana/singlish-caption-studio/releases/download/"
            "singlish-caption-studio-v0.2.0/Singlish-Caption-Studio_0.2.0_x64-setup.exe",
        )

    def test_the_url_is_always_https_on_github(self):
        url = m.build_manifest(**GOOD)["platforms"]["windows-x86_64"]["url"]
        self.assertTrue(url.startswith("https://github.com/"))

    def test_a_default_date_is_a_valid_timestamp(self):
        stamp = m.build_manifest(**GOOD)["pub_date"]
        self.assertRegex(stamp, r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")

    def test_prerelease_versions_are_fine(self):
        self.assertEqual(m.build_manifest(**{**GOOD, "version": "1.0.0-rc.1"})["version"], "1.0.0-rc.1")

    def test_signature_whitespace_is_removed(self):
        spaced = SIG[:30] + "\n" + SIG[30:] + "\r\n"
        self.assertEqual(m.build_manifest(**{**GOOD, "signature": spaced})["platforms"]["windows-x86_64"]["signature"], SIG)

    def test_notes_keep_unicode_and_quotes_and_are_capped(self):
        notes = 'Fixed "quotes", ඔයාට new phone එකක්\nline two'
        self.assertEqual(m.build_manifest(**{**GOOD, "notes": notes})["notes"], notes)
        self.assertEqual(len(m.build_manifest(**{**GOOD, "notes": "x" * 20000})["notes"]), m.MAX_NOTES)


class Notes(unittest.TestCase):
    RELEASE_PLEASE = (
        "## 0.2.0 (2026-11-08)\n\n\n### Features\n\n"
        "* **ui:** show usage for each clip ([1a2b3c4](https://github.com/o/r/commit/1a2b3c4d5e6f))\n"
        "* update from inside the app ([9f8e7d6](https://github.com/o/r/commit/9f8e7d6c5b4a))\n\n\n"
        "### Bug Fixes\n\n* keep captions from overlapping ([abcdef0](https://github.com/o/r/commit/abcdef012345))\n"
    )

    def test_release_please_markdown_becomes_plain_text(self):
        self.assertEqual(
            m.plain_notes(self.RELEASE_PLEASE),
            "Features:\n\n- **ui:** show usage for each clip\n- update from inside the app\n\n"
            "Bug Fixes:\n\n- keep captions from overlapping",
        )

    def test_links_keep_their_text(self):
        self.assertEqual(m.plain_notes("See [the guide](https://example.com/x) now"), "See the guide now")

    def test_plain_text_and_unicode_pass_through(self):
        text = 'Fixed "quotes", ඔයාට new phone එකක්\nline two'
        self.assertEqual(m.plain_notes(text), text)
        self.assertEqual(m.plain_notes(""), "")
        self.assertEqual(m.plain_notes("\n\n  \n"), "")

    def test_the_manifest_uses_plain_notes(self):
        notes = m.build_manifest(**{**GOOD, "notes": self.RELEASE_PLEASE})["notes"]
        self.assertNotIn("](", notes)
        self.assertNotIn("###", notes)
        self.assertTrue(notes.startswith("Features:"))


class Validation(unittest.TestCase):
    def rejected(self, **change):
        with self.assertRaises(ValueError):
            m.build_manifest(**{**GOOD, **change})

    def test_bad_versions(self):
        for v in ["", "v0.2.0", "0.2", "latest", "0.2.0 beta", "0.2.0;calc", "../0.2.0"]:
            self.rejected(version=v)

    def test_bad_repos(self):
        for r in ["", "owner", "a/b/c", "owner/name with space", "../x/y", "owner/name?x=1"]:
            self.rejected(repo=r)

    def test_bad_tags_and_assets(self):
        for t in ["", "a/b", "a b", "..", "v1..0", "tag?x", "x" * 101]:
            self.rejected(tag=t)
        for a in ["", "dir/file.exe", "my file.exe", "a..b.exe", "file.exe#frag", "file.exe?x=1"]:
            self.rejected(asset=a)

    def test_bad_signatures(self):
        for s in ["", "short", "has spaces!!!!!!!!!!!!!!!!!!!!!!", "<script>alert(1)</script>" * 2]:
            self.rejected(signature=s)

    def test_the_error_never_prints_the_signature(self):
        try:
            m.build_manifest(**{**GOOD, "signature": "bad bad bad bad bad bad bad!"})
        except ValueError as e:
            self.assertNotIn("bad bad", str(e))


class Command(unittest.TestCase):
    def write(self, text):
        fd, path = tempfile.mkstemp()
        with os.fdopen(fd, "w", encoding="utf-8", newline="") as f:
            f.write(text)
        self.addCleanup(os.remove, path)
        return path

    def args(self, **over):
        v = {**GOOD, **over}
        out = self.write("")
        return out, ["--version", v["version"], "--repo", v["repo"], "--tag", v["tag"], "--asset", v["asset"],
                     "--sig-file", self.write(v["signature"] + "\n"), "--notes-file", self.write(v["notes"]), "--out", out]

    def test_writes_valid_json(self):
        out, argv = self.args()
        self.assertEqual(m.main(argv), 0)
        with open(out, encoding="utf-8") as f:
            data = json.load(f)
        self.assertEqual(data["version"], "0.2.0")
        self.assertIn("windows-x86_64", data["platforms"])

    def test_a_bad_value_writes_nothing_and_fails(self):
        out, argv = self.args(version="not-a-version")
        self.assertEqual(m.main(argv), 1)
        with open(out, encoding="utf-8") as f:
            self.assertEqual(f.read(), "")

    def test_a_missing_file_fails(self):
        out, argv = self.args()
        argv[argv.index("--sig-file") + 1] = os.path.join(tempfile.gettempdir(), "no-such-file.sig")
        self.assertEqual(m.main(argv), 1)


if __name__ == "__main__":
    unittest.main()
