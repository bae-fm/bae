#!/usr/bin/env python3
"""Exercise the frozen-rows guard on a temporary git repository."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check-frozen-shrinks.py"
SPEC = importlib.util.spec_from_file_location("check_frozen_shrinks", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
GUARD = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = GUARD
SPEC.loader.exec_module(GUARD)

POLICY = "owner-construction-check.toml"
APPS = "scripts/ui-row-work.toml"


def policy(*rows: tuple[str, str, str, int]) -> str:
    lines = ["[read_shapes]", "frozen = ["]
    for path, item, shape, count in rows:
        lines.append(
            f'    {{ path = "{path}", item = "{item}", shape = "{shape}", count = {count}, finding = "f" }},'
        )
    lines.append("]")
    return "\n".join(lines) + "\n"


def apps(*rows: tuple[str, str, int]) -> str:
    lines = ["frozen = ["]
    for path, operation, count in rows:
        lines.append(
            f'    {{ path = "{path}", operation = "{operation}", count = {count}, finding = "f" }},'
        )
    lines.append("]")
    return "\n".join(lines) + "\n"


class FrozenShrinksTests(unittest.TestCase):
    def run_change(self, before: dict[str, str] | None, after: dict[str, str]) -> list[str]:
        """Commit `before` (when given), write `after`, and check the change."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            git = lambda *args: subprocess.run(
                ["git", "-c", "user.name=t", "-c", "user.email=t@t", *args],
                cwd=root,
                check=True,
                capture_output=True,
            )
            git("init", "-q")
            if before is None:
                (root / "README").write_text("start\n")
                git("add", "README")
            else:
                for name, text in before.items():
                    (root / name).parent.mkdir(parents=True, exist_ok=True)
                    (root / name).write_text(text)
                    git("add", name)
            git("commit", "-q", "-m", "start")
            for name, text in after.items():
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text(text)
            return GUARD.violations(root)

    def test_an_unchanged_file_passes(self) -> None:
        files = {POLICY: policy(("a.rs", "f", "sort", 2)), APPS: apps(("A.swift", "filter", 1))}
        self.assertEqual(self.run_change(files, files), [])

    def test_a_removed_row_passes(self) -> None:
        before = {POLICY: policy(("a.rs", "f", "sort", 2), ("b.rs", "g", "pick", 1))}
        after = {POLICY: policy(("a.rs", "f", "sort", 2))}
        self.assertEqual(self.run_change(before, after), [])

    def test_a_lowered_count_passes(self) -> None:
        before = {POLICY: policy(("a.rs", "f", "sort", 2))}
        after = {POLICY: policy(("a.rs", "f", "sort", 1))}
        self.assertEqual(self.run_change(before, after), [])

    def test_a_new_row_fails_naming_it(self) -> None:
        before = {POLICY: policy(("a.rs", "f", "sort", 2))}
        after = {POLICY: policy(("a.rs", "f", "sort", 2), ("b.rs", "g", "pick", 1))}
        found = self.run_change(before, after)
        self.assertEqual(len(found), 1)
        self.assertIn("b.rs", found[0])
        self.assertIn("g", found[0])
        self.assertIn("pick", found[0])

    def test_a_raised_count_fails_naming_it(self) -> None:
        before = {POLICY: policy(("a.rs", "f", "sort", 2))}
        after = {POLICY: policy(("a.rs", "f", "sort", 3))}
        found = self.run_change(before, after)
        self.assertEqual(len(found), 1)
        self.assertIn("a.rs", found[0])
        self.assertIn("2", found[0])
        self.assertIn("3", found[0])

    def test_the_apps_list_is_keyed_by_path_and_operation(self) -> None:
        before = {APPS: apps(("A.swift", "filter", 1))}
        grown = {APPS: apps(("A.swift", "filter", 1), ("A.swift", "sorted", 1))}
        raised = {APPS: apps(("A.swift", "filter", 2))}
        shrunk = {APPS: apps()}
        self.assertEqual(len(self.run_change(before, grown)), 1)
        self.assertEqual(len(self.run_change(before, raised)), 1)
        self.assertEqual(self.run_change(before, shrunk), [])

    def test_a_file_absent_at_head_passes(self) -> None:
        after = {POLICY: policy(("a.rs", "f", "sort", 2)), APPS: apps(("A.swift", "filter", 1))}
        self.assertEqual(self.run_change(None, after), [])

    def test_a_list_absent_at_head_passes(self) -> None:
        before = {POLICY: "[read_shapes]\nrule = \"r\"\n"}
        after = {POLICY: policy(("a.rs", "f", "sort", 2))}
        self.assertEqual(self.run_change(before, after), [])

    def test_a_row_moving_between_keys_counts_as_new(self) -> None:
        before = {POLICY: policy(("a.rs", "f", "sort", 2))}
        after = {POLICY: policy(("a.rs", "f", "filter", 2))}
        self.assertEqual(len(self.run_change(before, after)), 1)


if __name__ == "__main__":
    unittest.main()
