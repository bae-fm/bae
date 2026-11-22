#!/usr/bin/env python3
"""Run the design type check on component CSS that takes its type from tokens,
and on CSS that writes literal sizes, weights and line heights."""

from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECK = ROOT / "scripts" / "check-design-type.py"
SOURCE = "design/system/components/src/10-a.css"


class DesignTypeTests(unittest.TestCase):
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

    def test_a_drawing_named_in_the_check_keeps_its_measure(self) -> None:
        found = self.run_check(".bae-msym { font-size: 24px; font-weight: 400; }\n")
        self.assertEqual((found.returncode, found.stderr), (0, ""))

    def test_files_outside_the_component_sources_are_not_read(self) -> None:
        found = self.run_check(".a { font-size: 11px; }\n", path="design/mocks/a.css")
        self.assertEqual((found.returncode, found.stderr), (0, ""))


if __name__ == "__main__":
    unittest.main()
