#!/usr/bin/env python3
"""Exercise bae-bridge/find-python.sh against planted python3 executables."""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "bae-bridge" / "find-python.sh"


def plant(folder: Path, name: str, version: str) -> Path:
    """An executable `name` in `folder` that reports `version` as its Python."""
    folder.mkdir(exist_ok=True)
    path = folder / name
    major, minor = version.split(".")
    path.write_text(
        "#!/bin/sh\n"
        'case "$2" in\n'
        f'  *print*) echo "{version}" ;;\n'
        f"  *) exit $(( {major} == 3 && {minor} >= 11 ? 0 : 1 )) ;;\n"
        "esac\n"
    )
    path.chmod(0o755)
    return path


def run(path_folders: list[Path]) -> subprocess.CompletedProcess:
    env = {
        "PATH": ":".join(str(folder) for folder in path_folders),
        "FIND_PYTHON_EXTRA_FOLDERS": "",
    }
    return subprocess.run(
        ["/bin/bash", str(TOOL)], env=env, capture_output=True, text=True
    )


class FindPythonTest(unittest.TestCase):
    def test_skips_an_old_python3_ahead_of_a_current_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plant(root / "xcode", "python3", "3.9")
            current = plant(root / "brew", "python3", "3.14")
            result = run([root / "xcode", root / "brew"])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), str(current))

    def test_finds_a_versioned_python_when_python3_is_old(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            plant(root / "bin", "python3", "3.9")
            versioned = plant(root / "bin", "python3.12", "3.12")
            result = run([root / "bin"])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), str(versioned))

    def test_names_the_python_it_found_when_none_is_current(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            old = plant(root / "xcode", "python3", "3.9")
            result = run([root / "xcode"])
            self.assertEqual(result.returncode, 1)
            self.assertIn(f"{old} (3.9)", result.stderr)
            self.assertIn("3.11", result.stderr)


if __name__ == "__main__":
    unittest.main()
