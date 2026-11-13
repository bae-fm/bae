#!/usr/bin/env python3
"""Refuse a Rust file in a workspace crate that none of its targets compiles.

A file no `mod` declaration, `#[path]` or `include!` reaches is never built,
so its tests never run and its code never fails a check: a test file whose
declaration a rename dropped went unnoticed. Every tracked `.rs` file under a
workspace crate's directory must be one `rust_modules` reaches from Cargo's
targets.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from rust_modules import reached_files


def crate_directories(root: Path, names: list[str]) -> list[Path]:
    """The directory of each workspace crate, or of the crates in `names` alone
    when it is not empty."""
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root,
            check=True,
            capture_output=True,
        ).stdout
    )
    return [
        Path(package["manifest_path"]).parent.resolve()
        for package in metadata["packages"]
        if not names or package["name"] in names
    ]


def tracked_rust_files(root: Path) -> list[Path]:
    listed = subprocess.run(
        ["git", "ls-files", "-co", "--exclude-standard", "-z", "--", "*.rs"],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout.decode()
    return [(root / name).resolve() for name in listed.split("\0") if name]


def check(root: Path, names: list[str] | None = None) -> list[str]:
    root = root.resolve()
    crates = crate_directories(root, names or [])
    reached = reached_files(root, set(crates) if names else None)
    return sorted(
        str(path.relative_to(root))
        for path in tracked_rust_files(root)
        if path.is_file()
        and path not in reached
        and any(path.is_relative_to(crate) for crate in crates)
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="repository root",
    )
    parser.add_argument(
        "-p",
        dest="crates",
        action="append",
        default=[],
        help="a workspace crate to check; every crate when none is named",
    )
    args = parser.parse_args()
    violations = check(args.root, args.crates)
    for violation in violations:
        print(f"{violation}: no target compiles it")
    if violations:
        print(
            "Declare the file's module where it belongs, or delete the file.",
            file=sys.stderr,
        )
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
