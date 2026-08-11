#!/usr/bin/env python3
"""Keep source modules bounded and move tests out of long Rust modules."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


MAX_SOURCE_LINES = 1_500
INLINE_RUST_TEST_LIMIT = 1_000
SOURCE_SUFFIXES = {
    ".astro",
    ".c",
    ".cc",
    ".cpp",
    ".cs",
    ".css",
    ".gradle",
    ".h",
    ".hpp",
    ".html",
    ".java",
    ".js",
    ".kt",
    ".kts",
    ".mjs",
    ".ps1",
    ".py",
    ".rb",
    ".rs",
    ".scss",
    ".sh",
    ".swift",
    ".ts",
    ".tsx",
    ".xaml",
}
INLINE_TEST_MODULE = re.compile(
    rb"(?m)^[ \t]*#\[cfg\(test\)\][ \t]*\r?\n"
    rb"(?:^[ \t]*#\[[^\r\n]*\][ \t]*\r?\n)*"
    rb"^[ \t]*(?:pub(?:\([^\r\n)]*\))?[ \t]+)?mod[ \t]+[A-Za-z_][A-Za-z0-9_]*[ \t]*\{"
)


def repository_sources() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        check=True,
        stdout=subprocess.PIPE,
    )
    return [Path(raw.decode()) for raw in result.stdout.split(b"\0") if raw]


def line_count(contents: bytes) -> int:
    return contents.count(b"\n") + int(bool(contents) and not contents.endswith(b"\n"))


def main() -> int:
    paths = [Path(argument) for argument in sys.argv[1:]] or repository_sources()
    failures: list[str] = []

    for path in paths:
        if path.suffix.lower() not in SOURCE_SUFFIXES or not path.is_file():
            continue
        contents = path.read_bytes()
        lines = line_count(contents)
        if lines > MAX_SOURCE_LINES:
            failures.append(
                f"{path}: {lines} lines; source files may not exceed {MAX_SOURCE_LINES} lines"
            )
        if (
            path.suffix.lower() == ".rs"
            and lines > INLINE_RUST_TEST_LIMIT
            and INLINE_TEST_MODULE.search(contents)
        ):
            failures.append(
                f"{path}: move the inline #[cfg(test)] module to a sibling _tests.rs file"
            )

    if failures:
        print("Source file layout violations:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
