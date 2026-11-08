"""Write latest.json, the update manifest that installed apps read to find a new version.

    python tools/make_latest_json.py --version 0.2.0 --repo owner/name --tag name-v0.2.0 \
        --asset Singlish-Caption-Studio_0.2.0_x64-setup.exe --sig-file installer.exe.sig \
        --notes-file notes.md --out latest.json

The release workflow runs this after the installer has been built and signed. The manifest
itself is not trusted by the app: it only installs a download whose signature matches the public
key built into the app and names the same version. Even so, every value that ends up in the
download address is checked here, so a bad value stops the release instead of being published.
"""
import argparse
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

PLATFORM = "windows-x86_64"
MAX_NOTES = 8000

VERSION = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")
REPO = re.compile(r"^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$")
TAG = re.compile(r"^[A-Za-z0-9._-]{1,100}$")
ASSET = re.compile(r"^[A-Za-z0-9._-]{1,200}$")
SIGNATURE = re.compile(r"^[A-Za-z0-9+/=]{20,4000}$")


def plain_notes(markdown):
    """Release notes as plain text for the update dialog. The release page keeps the markdown."""
    lines = []
    for line in markdown.splitlines():
        line = re.sub(r"\s*\(\[[0-9a-f]{7,40}\]\([^)]*\)\)", "", line)  # commit links like ([d2cf570](...))
        line = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", line)  # other links keep their text
        if re.match(r"^##\s+\S*\d", line):  # the "## 0.2.0 (date)" title: the dialog already says the version
            continue
        heading = re.match(r"^#{1,6}\s*(.+?)\s*$", line)
        if heading:
            line = heading.group(1) + ":"
        line = re.sub(r"^\s*[*-]\s+", "- ", line).rstrip()
        if line or (lines and lines[-1]):  # no leading blanks, no runs of blanks
            lines.append(line)
    return "\n".join(lines).strip()


def build_manifest(version, repo, tag, asset, signature, notes, pub_date=None):
    """Return the manifest as a dict. Raises ValueError when a value is not acceptable."""
    signature = "".join(signature.split())
    for name, value, pattern in (
        ("version", version, VERSION),
        ("repo", repo, REPO),
        ("tag", tag, TAG),
        ("asset", asset, ASSET),
        ("signature", signature, SIGNATURE),
    ):
        if not pattern.match(value) or (name in ("tag", "asset") and ".." in value):
            shown = "(hidden)" if name == "signature" else repr(value)
            raise ValueError(f"the {name} is not acceptable: {shown}")
    if pub_date is None:
        pub_date = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    return {
        "version": version,
        "notes": plain_notes(notes)[:MAX_NOTES],
        "pub_date": pub_date,
        "platforms": {
            PLATFORM: {
                "signature": signature,
                "url": f"https://github.com/{repo}/releases/download/{tag}/{asset}",
            }
        },
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    for flag in ("--version", "--repo", "--tag", "--asset", "--sig-file", "--notes-file", "--out"):
        ap.add_argument(flag, required=True)
    a = ap.parse_args(argv)
    try:
        manifest = build_manifest(
            a.version,
            a.repo,
            a.tag,
            a.asset,
            Path(a.sig_file).read_text(encoding="utf-8"),
            Path(a.notes_file).read_text(encoding="utf-8"),
        )
    except (ValueError, OSError) as e:
        print(f"Could not write the update manifest: {e}", file=sys.stderr)
        return 1
    Path(a.out).write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Wrote {a.out} for version {manifest['version']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
