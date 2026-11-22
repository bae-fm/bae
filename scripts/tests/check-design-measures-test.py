#!/usr/bin/env python3
"""Run the design measures check on component CSS that takes its type, spacing
and radii from tokens, and on CSS that writes them as literals."""

from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECK = ROOT / "scripts" / "check-design-measures.py"
SOURCE = "design/system/components/src/10-a.css"


class DesignMeasuresTests(unittest.TestCase):
    def run_check(self, css: str, path: str = SOURCE) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / path
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text(css)
            return subprocess.run(
                ["python3", str(CHECK), path], cwd=directory, capture_output=True, text=True
            )

    def test_type_from_token_variables_passes(self) -> None:
        found = self.run_check(
            ".a { font: var(--type-body-weight) var(--type-body-size) var(--font-sans); }\n"
            ".b { font-size: var(--type-detail-size); line-height: var(--type-detail-line-height); }\n"
            ".c { font: inherit; line-height: normal; height: 20px; }\n"
            ".d { font: var(--type-body-weight) var(--type-body-size)/var(--h, var(--size-control)) var(--font-sans); }\n"
        )
        self.assertEqual((found.returncode, found.stderr), (0, ""))

    def test_each_literal_measure_fails_at_its_line(self) -> None:
        found = self.run_check(
            "/* 13px in a comment is no measure */\n"
            ".a { font: 400 13px var(--font-sans); }\n"
            ".b { color: red; font-size: 11px; }\n"
            ".c { font-weight: bold; }\n"
            ".d {\n  line-height: 1.2;\n}\n"
        )
        self.assertEqual(found.returncode, 1)
        lines = [line.split(": ")[0] for line in found.stderr.splitlines()]
        self.assertEqual(lines, [f"{SOURCE}:2", f"{SOURCE}:3", f"{SOURCE}:4", f"{SOURCE}:6"])

    def test_spacing_and_radii_from_tokens_pass(self) -> None:
        found = self.run_check(
            ".a { padding: 0 var(--space-edge); margin: calc(-1 * var(--space-group)) auto; }\n"
            ".b { gap: var(--space-related); margin: calc((1lh - var(--size-control)) / 2) 0; }\n"
            ".c { border-radius: var(--radius-card) var(--radius-card) 0 0; }\n"
            ".d { border-radius: 999px; border-top-left-radius: 0; width: 300px; }\n"
        )
        self.assertEqual((found.returncode, found.stderr), (0, ""))

    def test_each_literal_spacing_or_radius_fails_at_its_line(self) -> None:
        found = self.run_check(
            ".a { padding: 0 16px; }\n"
            ".b { margin-left: calc(var(--space-edge) - 12px); }\n"
            ".c { gap: 0.5em; }\n"
            ".d { border-radius: 8px; }\n"
            ".e { border-bottom-right-radius: 4px; }\n"
        )
        self.assertEqual(found.returncode, 1)
        lines = [line.split(": ")[0] for line in found.stderr.splitlines()]
        self.assertEqual(lines, [f"{SOURCE}:{n}" for n in range(1, 6)])

    def test_a_drawing_named_in_the_check_keeps_its_measure(self) -> None:
        found = self.run_check(
            ".bae-a-key-mod { font-size: 13px; }\n.bae-m-titlebar { padding: 0 var(--space-edge) 0 80px; }\n"
        )
        self.assertEqual((found.returncode, found.stderr), (0, ""))

    def test_files_outside_the_component_sources_are_not_read(self) -> None:
        found = self.run_check(".a { font-size: 11px; }\n", path="design/mocks/a.css")
        self.assertEqual((found.returncode, found.stderr), (0, ""))


if __name__ == "__main__":
    unittest.main()
