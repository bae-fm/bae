#!/usr/bin/env python3
"""Run the design-script parse check on a board, a preview and bundle sources
that parse, and on each with a missing parenthesis."""

from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECK = ROOT / "scripts" / "check-design-scripts.mjs"

BOARD = """<!doctype html>
<x-dc></x-dc>
<script type="text/x-dc" data-dc-script data-props='{}'>
class Component extends DCLogic {
  renderVals() { return { yes: %s; }
}
</script>
"""
PREVIEW = """<!doctype html>
<body>
<script src="./other.js"></script>
<script>
  var h = React.createElement;
  h('div', null, h('span', null)%s;
</script>
</body>
"""


class DesignScriptTests(unittest.TestCase):
    def run_check(self, files: dict[str, str]) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative, text in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            return subprocess.run(
                ["node", str(CHECK), *files],
                cwd=root,
                capture_output=True,
                text=True,
            )

    def test_a_board_that_parses_passes(self) -> None:
        found = self.run_check({"design/mocks/a.dc.html": BOARD % "true }"})
        self.assertEqual((found.returncode, found.stderr), (0, ""))

    def test_a_board_missing_a_brace_fails_at_its_line(self) -> None:
        found = self.run_check({"design/mocks/a.dc.html": BOARD % "true"})
        self.assertEqual(found.returncode, 1)
        self.assertIn("design/mocks/a.dc.html:5:", found.stderr)

    def test_a_preview_that_parses_passes(self) -> None:
        found = self.run_check({"design/system/components/A/preview.html": PREVIEW % ")"})
        self.assertEqual((found.returncode, found.stderr), (0, ""))

    def test_a_preview_missing_a_parenthesis_fails_at_its_line(self) -> None:
        found = self.run_check({"design/system/components/A/preview.html": PREVIEW % ""})
        self.assertEqual(found.returncode, 1)
        self.assertIn("design/system/components/A/preview.html:6:", found.stderr)

    def test_bundle_sources_parse_as_one_script(self) -> None:
        sources = "design/system/components/src"
        found = self.run_check(
            {f"{sources}/00-a.js": "(function () {\n", f"{sources}/99-b.js": "})();\n"}
        )
        self.assertEqual((found.returncode, found.stderr), (0, ""))

    def test_a_bundle_source_missing_a_parenthesis_fails_in_its_file(self) -> None:
        sources = "design/system/components/src"
        found = self.run_check(
            {f"{sources}/00-a.js": "(function () {\n", f"{sources}/99-b.js": "f(1;\n})();\n"}
        )
        self.assertEqual(found.returncode, 1)
        self.assertIn(f"{sources}/99-b.js:1:", found.stderr)


if __name__ == "__main__":
    unittest.main()
