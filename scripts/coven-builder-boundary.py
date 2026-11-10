#!/usr/bin/env python3
"""Keep every coven builder bae takes in one function.

A builder from `Coven::builder()` has coven's random ids and the system clock
unless a caller sets its own. bae's libraries are written with its
time-ordered ids on one clock, made once by the process's composition, so the
only place a `.builder()` call on an installation may stand is
`bae_store::store_builder`, which takes the clock and the ids together. Every
other bae site, the app's host and every test and bench included, gets its
builder from that function (tests through `bae_store::test_devices`), so a
library cannot be built without them.

Rust code is read with comments and strings blanked out, so a mention in
either is not a call.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ALLOWED = {"bae-store/src/builder.rs"}
CALL = re.compile(r"\.\s*builder\s*\(\s*\)")


def blanked(source: str) -> str:
    """`source` with comments and string contents replaced by spaces."""
    out = []
    i = 0
    n = len(source)
    while i < n:
        two = source[i : i + 2]
        if two == "//":
            while i < n and source[i] != "\n":
                out.append(" ")
                i += 1
        elif two == "/*":
            depth = 0
            while i < n:
                if source[i : i + 2] == "/*":
                    depth += 1
                    out.append("  ")
                    i += 2
                elif source[i : i + 2] == "*/":
                    depth -= 1
                    out.append("  ")
                    i += 2
                    if depth == 0:
                        break
                else:
                    out.append("\n" if source[i] == "\n" else " ")
                    i += 1
        elif source[i] == '"':
            out.append('"')
            i += 1
            while i < n and source[i] != '"':
                if source[i] == "\\" and i + 1 < n:
                    out.append(" ")
                    i += 1
                out.append("\n" if source[i] == "\n" else " ")
                i += 1
            out.append('"')
            i += 1
        else:
            out.append(source[i])
            i += 1
    return "".join(out)


def violations(root: Path) -> list[str]:
    files = subprocess.run(
        ["git", "ls-files", "*.rs"], cwd=root, capture_output=True, text=True, check=True
    ).stdout.split()
    found = []
    for name in files:
        if name in ALLOWED or not (root / name).exists():
            continue
        text = blanked((root / name).read_text())
        for match in CALL.finditer(text):
            line = text.count("\n", 0, match.start()) + 1
            found.append(f"{name}:{line}: a `.builder()` call outside bae_store::store_builder")
    return found


def main() -> int:
    root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
    found = violations(root)
    for line in found:
        print(line, file=sys.stderr)
    if found:
        print(
            "take the builder from bae_store::store_builder (tests: bae_store::test_devices), "
            "which sets the clock and the ids together",
            file=sys.stderr,
        )
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
