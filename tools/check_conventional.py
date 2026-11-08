"""Check that a pull request title or a commit message follows Conventional Commits.

    python tools/check_conventional.py --title "feat(ui): add a usage tab"
    python tools/check_conventional.py --commit-file .git/COMMIT_EDITMSG

It exits with 0 when the message is fine, and with 1 and an explanation when it is not.
The same check runs in CI on pull request titles and, if you turn it on, as a commit-msg hook
(see CONTRIBUTING.md).
"""
import argparse
import re
import sys

TYPES = ("feat", "fix", "perf", "revert", "docs", "style", "refactor", "test", "build", "ci", "chore")
MAX_LENGTH = 72

PATTERN = re.compile(
    r"^(?P<type>" + "|".join(TYPES) + r")"
    r"(?:\((?P<scope>[a-z0-9][a-z0-9._/-]*)\))?"
    r"(?P<breaking>!)?"
    r": (?P<subject>\S.*)$"
)

# Messages Git writes itself. They are fine in a commit, but a pull request title must be a real one.
GIT_GENERATED = ("Merge ", "Revert \"", "fixup! ", "squash! ", "amend! ")

HELP = (
    "Use the form  type(scope): short summary  where type is one of: " + ", ".join(TYPES) + ".\n"
    "The scope is optional, and a ! after it marks a breaking change. For example:\n"
    "    feat(ui): add a usage tab\n"
    "    fix: keep captions from overlapping\n"
    "    feat!: change the settings file format"
)


def problems(first_line, allow_git_generated=False):
    """Return a list of reasons the line is not acceptable. Empty means it is fine."""
    line = first_line.rstrip()
    if allow_git_generated and line.startswith(GIT_GENERATED):
        return []
    found = []
    m = PATTERN.match(line)
    if not m:
        found.append("does not start with a type like \"feat:\" or \"fix(scope):\"")
        return found
    if len(line) > MAX_LENGTH:
        found.append(f"is {len(line)} characters long, the limit is {MAX_LENGTH}")
    if m.group("subject").endswith("."):
        found.append("ends with a period")
    return found


def first_line_of_commit_file(path):
    """The subject of a commit message file: the first line that is not blank or a # comment."""
    with open(path, encoding="utf-8") as f:
        for raw in f:
            if raw.strip() and not raw.startswith("#"):
                return raw.rstrip("\r\n")
    return ""


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    group = ap.add_mutually_exclusive_group(required=True)
    group.add_argument("--title", help="a pull request title")
    group.add_argument("--commit-file", help="a file holding a commit message")
    args = ap.parse_args(argv)

    if args.title is not None:
        line, allow = args.title.strip().splitlines()[0] if args.title.strip() else "", False
    else:
        line, allow = first_line_of_commit_file(args.commit_file), True
    issues = problems(line, allow_git_generated=allow)
    if issues:
        print(f"Not a valid Conventional Commit: \"{line}\"", file=sys.stderr)
        for issue in issues:
            print(f"  - it {issue}", file=sys.stderr)
        print(HELP, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
