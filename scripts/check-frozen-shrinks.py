#!/usr/bin/env python3
"""Keep the read rule's frozen lists from growing.

`owner-construction-check.toml` (`read_shapes.frozen`) and
`scripts/ui-row-work.toml` (`frozen`) list the violations bae has today, keyed
and counted exactly. They only shrink: a row may be removed or its count
lowered, never added or raised. This compares each file's `frozen` rows with
the same file at HEAD and fails on a new key or a higher count. A file that
or a list that does not exist at HEAD has nothing to compare with (the commit
that adds the list).
"""

from __future__ import annotations

import subprocess
import sys
import tomllib
from pathlib import Path

# Each file and the fields that key one of its frozen rows.
LISTS = {
    "owner-construction-check.toml": (("read_shapes", "frozen"), ("path", "item", "shape")),
    "scripts/ui-row-work.toml": (("frozen",), ("path", "operation")),
}

REMEDY = (
    "Frozen rows only shrink: remove the code the row covers, or, when the "
    "values are not a read's rows, put the use in `allowed` with what they are."
)


def rows(text: str, location: tuple[str, ...]) -> list[dict] | None:
    """The frozen rows, or None when the file has no such list."""
    table = tomllib.loads(text)
    for name in location:
        if not isinstance(table, dict) or name not in table:
            return None
        table = table[name]
    return table if isinstance(table, list) else None


def counts(found: list[dict], fields: tuple[str, ...]) -> dict[tuple, int]:
    return {tuple(row[field] for field in fields): row["count"] for row in found}


def head_text(root: Path, name: str) -> str | None:
    shown = subprocess.run(
        ["git", "show", f"HEAD:{name}"], cwd=root, capture_output=True, text=True
    )
    return shown.stdout if shown.returncode == 0 else None


def violations(root: Path) -> list[str]:
    found: list[str] = []
    for name, (location, fields) in LISTS.items():
        path = root / name
        before_text = head_text(root, name)
        if before_text is None or not path.exists():
            continue
        before_rows = rows(before_text, location)
        after_rows = rows(path.read_text(), location)
        if before_rows is None or after_rows is None:
            continue
        before = counts(before_rows, fields)
        for key, count in counts(after_rows, fields).items():
            label = ", ".join(f"{field} {value}" for field, value in zip(fields, key))
            if key not in before:
                found.append(f"{name}: new frozen row ({label}) with count {count}. {REMEDY}")
            elif count > before[key]:
                found.append(
                    f"{name}: frozen row ({label}) raised from {before[key]} to {count}. {REMEDY}"
                )
    return found


def main() -> int:
    root = Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
        ).stdout.strip()
    )
    found = violations(root)
    for line in found:
        print(line, file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
