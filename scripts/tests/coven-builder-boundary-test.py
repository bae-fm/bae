#!/usr/bin/env python3
"""Exercise the coven builder boundary on small trees and on the real one."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "coven-builder-boundary.py"
SPEC = importlib.util.spec_from_file_location("coven_builder_boundary", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
BOUNDARY = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BOUNDARY
SPEC.loader.exec_module(BOUNDARY)


class CovenBuilderBoundaryTests(unittest.TestCase):
    def tree(self, files: dict[str, str]) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            for name, text in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            subprocess.run(["git", "add", "-A"], cwd=root, check=True)
            return BOUNDARY.violations(root)

    def test_a_call_on_an_installation_is_refused(self) -> None:
        found = self.tree({"a/src/lib.rs": "fn f(i: &Coven) {\n    i\n        .builder();\n}\n"})
        self.assertEqual(len(found), 1)
        self.assertIn("a/src/lib.rs:3", found[0])

    def test_the_one_function_is_allowed(self) -> None:
        self.assertEqual(
            self.tree({"bae-store/src/builder.rs": "fn f(i: &Coven) { i.builder(); }"}), []
        )

    def test_associated_builders_comments_and_strings_are_not_calls(self) -> None:
        source = 'fn f() { MemoryStorage::builder(); } // x.builder()\nconst S: &str = "x.builder()";\n'
        self.assertEqual(self.tree({"a/src/lib.rs": source}), [])

    def test_the_workspace_holds_to_it(self) -> None:
        self.assertEqual(BOUNDARY.violations(ROOT), [])


if __name__ == "__main__":
    unittest.main()
