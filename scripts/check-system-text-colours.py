#!/usr/bin/env python3
"""Refuse SwiftUI's and AppKit's own label colours in the Mac app's views.

Every colour on the Mac comes from the design system (`Theme`), so a view draws
its text in `Theme.textPrimary`, `Theme.macTextSecondary` or
`Theme.textTertiary`, never in SwiftUI's hierarchical styles (`.secondary`,
`.tertiary`, `.quaternary`, `.quinary`, `Color.secondary`) or in an AppKit
label colour (`NSColor.secondaryLabelColor` and the like). A Text's own
`.foregroundStyle(.secondary)` derives its colour from the primary text colour,
so it draws a different grey than the board's.

`Theme.swift` is where `macTextSecondary` is defined from the system's colour.
A name in a comment or a string is not a use, and a case of the app's own
enums (`kind: .secondary`) is not a colour.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from source_tokens import Token, tokenize

HIERARCHY = {"secondary", "tertiary", "quaternary", "quinary"}
# Calls whose argument is a colour or a shape style.
STYLE_CALLS = {
    "foregroundStyle",
    "foregroundColor",
    "AnyShapeStyle",
    "stroke",
    "fill",
    "tint",
    "background",
    "overlay",
}
STYLE_LABELS = {"tint", "color", "foregroundColor", "textColor"}
SCOPES = ("BaeKit/Sources/", "bae-macos/bae/bae/")
EXEMPT = {"BaeKit/Sources/BaeKit/Theme.swift"}


def listed_files(root: Path) -> list[str]:
    listed = subprocess.run(
        ["git", "ls-files", "-co", "--exclude-standard", "-z"],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout.decode()
    return sorted(name for name in listed.split("\0") if name)


def system_colours(tokens: list[Token]) -> list[tuple[int, str]]:
    """Each system label colour in `tokens`: its line, and what it is."""
    found = []
    calls: list[str] = []
    for index, token in enumerate(tokens):
        before = tokens[index - 1] if index else None
        if token.kind == "punct" and token.text == "(":
            named = before is not None and before.kind == "id"
            calls.append(before.text if named else "")
        elif token.kind == "punct" and token.text == ")":
            if calls:
                calls.pop()
        elif token.kind == "id":
            if token.text.endswith("LabelColor") or token.text == "HierarchicalShapeStyle":
                found.append((token.line, token.text))
            elif token.text in HIERARCHY and before is not None and before.text == ".":
                if system_style(tokens, index, calls):
                    found.append((token.line, f".{token.text}"))
    return found


def system_style(tokens: list[Token], index: int, calls: list[str]) -> bool:
    """Whether the `.name` at `index` is a SwiftUI style, not an app enum case."""
    owner = tokens[index - 2] if index >= 2 else None
    if owner is not None and owner.kind == "id":
        return owner.text == "Color"
    label = labelled(tokens, index - 1)
    if label is not None:
        return label in STYLE_LABELS
    return any(call in STYLE_CALLS for call in calls)


def labelled(tokens: list[Token], dot: int) -> str | None:
    """The argument label before the `.name` whose dot is at `dot`, if any."""
    if dot >= 3 and tokens[dot - 1].text == ":" and tokens[dot - 2].kind == "id":
        if tokens[dot - 3].text in ("(", ","):
            return tokens[dot - 2].text
    return None


def check(root: Path, names: list[str] | None = None) -> list[str]:
    """The system label colours in the Mac app's Swift files, or in `names`."""
    violations = []
    for relative in names if names else listed_files(root):
        if not relative.endswith(".swift") or relative in EXEMPT:
            continue
        if not relative.startswith(SCOPES):
            continue
        path = root / relative
        if not path.is_file():
            continue
        for line, what in system_colours(tokenize(path.read_text(), "swift")):
            violations.append(
                f"{relative}:{line}: {what}: draw text in Theme.textPrimary, "
                "Theme.macTextSecondary or Theme.textTertiary, not the system's label colour"
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
    parser.add_argument(
        "files",
        nargs="*",
        help="repo-relative files to check; every tracked file when none",
    )
    args = parser.parse_args()
    violations = check(args.root, args.files)
    for violation in violations:
        print(violation)
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
