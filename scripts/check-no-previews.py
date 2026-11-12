#!/usr/bin/env python3
"""Refuse a new IDE preview in the apps' Swift and Kotlin sources.

The design canvas is where views are drawn; the code's picture of a board is a
shot drawn from the real view in a test (baeShots on macOS, Roborazzi on
Android). So the apps carry no SwiftUI `#Preview` or `PreviewProvider` and no
Compose `@Preview`. This check refuses each in the code of every tracked Swift
and Kotlin file; a name in a comment or a string is not a use.

The album grid's code belongs to the person and keeps the previews it has;
`ALLOWED` lists those files by path.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from source_tokens import Token, tokenize

ALLOWED = {
    "bae-android/app/src/main/java/fm/bae/app/ui/library/LibraryGrid.kt",
    "bae-android/app/src/main/java/fm/bae/app/ui/library/LibraryGridScenes.kt",
    "bae-macos/bae/bae/Views/Library/Grid/AlbumCardMenuItems.swift",
    "bae-macos/bae/bae/Views/Library/Grid/AlbumExpansionSlot.swift",
    "bae-macos/bae/bae/Views/Library/Grid/AlbumGridView.swift",
    "bae-macos/bae/bae/Views/Library/Grid/CardMenuButton.swift",
    "bae-macos/bae/bae/Views/Library/Grid/MacAlbumTile.swift",
}
LANGUAGES = {".swift": "swift", ".kt": "kotlin"}


def listed_files(root: Path) -> list[str]:
    listed = subprocess.run(
        ["git", "ls-files", "-co", "--exclude-standard", "-z"],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout.decode()
    return sorted(name for name in listed.split("\0") if name)


def previews(tokens: list[Token], language: str) -> list[tuple[int, str]]:
    """Each preview in `tokens`: its line, and what it is."""
    found = []
    for index, token in enumerate(tokens):
        if token.kind != "id":
            continue
        before = tokens[index - 1].text if index else ""
        if language == "swift":
            if token.text == "Preview" and before == "#":
                found.append((token.line, "#Preview"))
            elif token.text == "PreviewProvider":
                found.append((token.line, "PreviewProvider"))
        elif token.text.startswith("Preview") and before == "@":
            found.append((token.line, f"@{token.text}"))
        elif token.text == "Preview" and before == "." and not imported(tokens, index):
            # `@androidx.compose.ui.tooling.preview.Preview`
            start = index
            while start >= 2 and tokens[start - 1].text == "." and tokens[start - 2].kind == "id":
                start -= 2
            if start and tokens[start - 1].text == "@":
                found.append((token.line, "@Preview"))
    return found


def imported(tokens: list[Token], index: int) -> bool:
    """Whether the name at `index` is part of an `import` line."""
    while index and not tokens[index].newline:
        index -= 1
    return tokens[index].text == "import"


def check(root: Path) -> list[str]:
    violations = []
    for relative in listed_files(root):
        language = LANGUAGES.get(Path(relative).suffix)
        if language is None or relative in ALLOWED:
            continue
        path = root / relative
        if not path.is_file():
            continue
        for line, what in previews(tokenize(path.read_text(), language), language):
            violations.append(
                f"{relative}:{line}: {what}: the apps carry no IDE previews; "
                "draw the view as a shot in a test"
            )
    return violations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="repository root",
    )
    args = parser.parse_args()
    violations = check(args.root)
    for violation in violations:
        print(violation)
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
