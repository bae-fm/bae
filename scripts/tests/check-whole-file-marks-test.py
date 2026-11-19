#!/usr/bin/env python3
"""Exercise the whole-module lint mark check on small sources and on the real tree."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
MODULE_PATH = ROOT / "scripts" / "check-whole-file-marks.py"
SPEC = importlib.util.spec_from_file_location("check_whole_file_marks", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
CHECK = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECK
SPEC.loader.exec_module(CHECK)



class WholeFileMarkTests(unittest.TestCase):
    def found(self, source: str, name: str = "a/src/lib.rs") -> list[str]:
        return CHECK.file_violations(name, source)

    def test_a_file_mark_is_refused(self) -> None:
        found = self.found('//! Docs.\n\n#![expect(dead_code, reason = "x")]\nfn f() {}\n')
        self.assertEqual(len(found), 1)
        self.assertIn("a/src/lib.rs:3", found[0])
        self.assertIn("dead_code", found[0])

    def test_an_allow_inside_cfg_attr_is_refused(self) -> None:
        found = self.found("#![cfg_attr(test, allow(clippy::unwrap_used))]\n")
        self.assertEqual(len(found), 1)
        self.assertIn("clippy::unwrap_used", found[0])

    def test_an_inline_module_mark_is_refused(self) -> None:
        found = self.found("mod inner {\n    #![allow(unused)]\n    fn f() {}\n}\n")
        self.assertEqual(len(found), 1)
        self.assertIn(":2:", found[0])

    def test_an_outer_mark_on_a_module_is_refused(self) -> None:
        source = '#[cfg(test)]\n#[expect(unused, reason = "x")]\npub(crate) mod tests;\n'
        self.assertEqual(len(self.found(source)), 1)

    def test_a_mark_on_one_item_passes(self) -> None:
        source = (
            '#[expect(clippy::disallowed_methods, reason = "x")]\nfn f() {}\n'
            'fn g() {\n    #![expect(unused, reason = "x")]\n}\n'
            "impl S {\n    #[allow(unused)]\n    fn h() {}\n}\n"
        )
        self.assertEqual(self.found(source), [])

    def test_marks_that_turn_a_lint_on_pass(self) -> None:
        self.assertEqual(self.found("#![deny(clippy::wildcard_enum_match_arm)]\n"), [])

    def test_comments_and_strings_are_not_marks(self) -> None:
        source = '// #![allow(unused)]\nconst S: &str = "#![allow(unused)]";\n'
        self.assertEqual(self.found(source), [])

    def test_the_workspace_holds_to_it(self) -> None:
        self.assertEqual(CHECK.violations(ROOT), [])


if __name__ == "__main__":
    unittest.main()
