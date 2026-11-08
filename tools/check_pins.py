"""Check that every GitHub Action in the workflows is pinned to a commit that really exists.

Usage: python tools/check_pins.py
"""
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

root = Path(__file__).resolve().parent.parent
pattern = re.compile(r"uses:\s*([\w.-]+/[\w.-]+)@([0-9a-f]{40})")
bad = 0
seen = set()
for wf in sorted((root / ".github" / "workflows").glob("*.yml")):
    for line_no, line in enumerate(wf.read_text(encoding="utf-8").splitlines(), 1):
        if "uses:" in line and not pattern.search(line):
            print(f"NOT PINNED  {wf.name}:{line_no}: {line.strip()}")
            bad += 1
            continue
        m = pattern.search(line)
        if not m or m.groups() in seen:
            continue
        seen.add(m.groups())
        repo, sha = m.groups()
        try:
            with urllib.request.urlopen(f"https://api.github.com/repos/{repo}/commits/{sha}", timeout=30) as r:
                ok = r.status == 200
        except urllib.error.HTTPError as e:
            ok = False
            print(f"HTTP {e.code}", end="  ")
        print(("ok       " if ok else "MISSING  ") + f"{repo}@{sha[:12]}  ({wf.name}:{line_no})")
        bad += 0 if ok else 1
sys.exit(1 if bad else 0)
