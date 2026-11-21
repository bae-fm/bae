#!/usr/bin/env python3
"""Exercise the system-text-colours check against the real sources and a
planted workspace holding each shape of system label colour it refuses."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check-system-text-colours.py"
sys.path.insert(0, str(MODULE_PATH.parent))
SPEC = importlib.util.spec_from_file_location("check_system_text_colours", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
CHECK = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECK
SPEC.loader.exec_module(CHECK)

VIEW = "bae-macos/bae/bae/Views/Example.swift"


class SystemTextColourTests(unittest.TestCase):
    def violations(self, files: dict[str, str]) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            for relative, text in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            return CHECK.check(root)

    def test_production_tree_passes(self) -> None:
        self.assertEqual(CHECK.check(ROOT), [])

    def test_refuses_each_shape(self) -> None:
        for source in [
            "Text(x).foregroundStyle(.secondary)",
            "Text(x).foregroundStyle(.tertiary)",
            "Text(x).foregroundColor(.secondary)",
            "Text(x).foregroundStyle(on ? Theme.accent : .secondary)",
            "Text(x).foregroundStyle(a == nil ? .tertiary : .primary)",
            "let s = AnyShapeStyle(.quaternary)",
            "let c = Color.secondary",
            "field.textColor = .secondaryLabelColor",
            "let c = NSColor.tertiaryLabelColor",
            "let s = HierarchicalShapeStyle.secondary",
            "row(tint: .secondary)",
        ]:
            with self.subTest(source=source):
                self.assertEqual(len(self.violations({VIEW: source + "\n"})), 1)

    def test_allows_theme_colours_and_app_cases(self) -> None:
        for source in [
            "Text(x).foregroundStyle(Theme.macTextSecondary)",
            "Text(x).foregroundStyle(Theme.textTertiary)",
            "MacButton(kind: .secondary) {}",
            "StableOptionalText(foreground: .tertiary)",
            "// Text(x).foregroundStyle(.secondary)",
            'let s = "foregroundStyle(.secondary)"',
            "case secondary",
        ]:
            with self.subTest(source=source):
                self.assertEqual(self.violations({VIEW: source + "\n"}), [])

    def test_ignores_theme_and_other_trees(self) -> None:
        planted = "let c = Color(nsColor: .secondaryLabelColor)\n"
        self.assertEqual(self.violations({"BaeKit/Sources/BaeKit/Theme.swift": planted}), [])
        self.assertEqual(self.violations({"bae-macos/bae/baeShots/Shot.swift": planted}), [])


if __name__ == "__main__":
    unittest.main()
