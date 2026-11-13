#!/usr/bin/env python3
"""Fails when a tracked file passes -derivedDataPath anywhere but
scripts/xcode-trees.sh, the one definition of the apps' Xcode build trees.
Reads the files named as arguments, or every tracked file with none.

A second spelling of a tree is how a build lands in a folder nothing else knows
about and goes stale unseen.
"""

import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
FLAG = "-derivedDataPath"
# The definition, and this checker, which names the flag.
ALLOWED = {"scripts/xcode-trees.sh", "scripts/check-xcode-trees.py"}


def main() -> int:
    if len(sys.argv) > 1:
        tracked = sys.argv[1:]
    else:
        tracked = subprocess.run(
            ["git", "ls-files", "-z"], cwd=REPO, check=True, capture_output=True
        ).stdout.decode().split("\0")
    found = []
    for name in tracked:
        if not name or name in ALLOWED:
            continue
        path = REPO / name
        if not path.is_file() or path.is_symlink():
            continue
        try:
            text = path.read_text()
        except UnicodeDecodeError:
            continue
        for number, line in enumerate(text.splitlines(), 1):
            if FLAG in line:
                found.append(f"{name}:{number}: {line.strip()}")
    if found:
        print(f"{FLAG} belongs in scripts/xcode-trees.sh only; source it and pass", file=sys.stderr)
        print("its XCODE_*_ARGS instead:", file=sys.stderr)
        print("\n".join(found), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
