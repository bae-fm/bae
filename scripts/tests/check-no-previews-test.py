#!/usr/bin/env python3
"""Exercise the no-previews check against the real sources and a planted
workspace holding each shape of preview it refuses."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check-no-previews.py"
sys.path.insert(0, str(MODULE_PATH.parent))
SPEC = importlib.util.spec_from_file_location("check_no_previews", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
CHECK = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECK
SPEC.loader.exec_module(CHECK)

GRID_SWIFT = "bae-macos/bae/bae/Views/Library/Grid/AlbumGridView.swift"
GRID_KOTLIN = "bae-android/app/src/main/java/fm/bae/app/ui/library/LibraryGrid.kt"


class NoPreviewTests(unittest.TestCase):
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

    def test_a_swift_preview_macro_fails(self) -> None:
        found = self.violations(
            {"App/Row.swift": 'struct Row {}\n\n#Preview("Row") {\n    Row()\n}\n'}
        )
        self.assertEqual(len(found), 1)
        self.assertIn("App/Row.swift:3: #Preview", found[0])

    def test_a_swift_preview_provider_fails(self) -> None:
        found = self.violations(
            {"App/Row.swift": "struct RowPreviews: PreviewProvider {\n}\n"}
        )
        self.assertEqual(len(found), 1)
        self.assertIn("PreviewProvider", found[0])

    def test_a_kotlin_preview_fails(self) -> None:
        found = self.violations(
            {
                "app/Row.kt": "@Preview(showBackground = true)\n"
                "@Composable\nfun RowPreview() {}\n"
            }
        )
        self.assertEqual(len(found), 1)
        self.assertIn("app/Row.kt:1: @Preview", found[0])

    def test_a_qualified_kotlin_preview_fails(self) -> None:
        found = self.violations(
            {
                "app/Row.kt": "@androidx.compose.ui.tooling.preview.Preview\n"
                "@Composable\nfun RowPreview() {}\n"
            }
        )
        self.assertEqual(len(found), 1)

    def test_a_name_in_a_comment_or_string_passes(self) -> None:
        self.assertEqual(
            self.violations(
                {
                    "App/Row.swift": '// the old #Preview\nlet note = "#Preview"\n',
                    "app/Row.kt": '/* @Preview */\nval note = "@Preview"\n'
                    "import androidx.compose.ui.tooling.preview.Preview\n",
                }
            ),
            [],
        )

    def test_a_listed_grid_file_passes(self) -> None:
        self.assertEqual(
            self.violations(
                {
                    GRID_SWIFT: '#Preview("Grid") {\n}\n',
                    GRID_KOTLIN: "@Preview\n@Composable\nfun GridPreview() {}\n",
                }
            ),
            [],
        )

    def test_a_file_beside_the_grid_fails(self) -> None:
        found = self.violations(
            {"bae-macos/bae/bae/Views/Library/Grid/Other.swift": '#Preview("x") {\n}\n'}
        )
        self.assertEqual(len(found), 1)


if __name__ == "__main__":
    unittest.main()
