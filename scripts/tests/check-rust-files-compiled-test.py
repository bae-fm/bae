#!/usr/bin/env python3
"""Exercise the compiled-files check against the real sources and a planted
workspace."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check-rust-files-compiled.py"
# The check reads sources through the modules beside it, which a script run
# from `scripts/` imports by name.
sys.path.insert(0, str(MODULE_PATH.parent))
SPEC = importlib.util.spec_from_file_location("check_rust_files_compiled", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
CHECK = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECK
SPEC.loader.exec_module(CHECK)

PLANTED = {
    "Cargo.toml": '[workspace]\nmembers = ["planted"]\nresolver = "2"\n',
    "planted/Cargo.toml": '[package]\nname = "planted"\nversion = "0.1.0"\nedition = "2024"\n',
    "planted/src/lib.rs": 'mod topic;\n\n#[cfg(test)]\n#[path = "lib_tests.rs"]\nmod tests;\n',
    "planted/src/lib_tests.rs": "",
    "planted/src/topic.rs": "fn topic() {}\n",
    # A test file whose declaration was dropped.
    "planted/src/topic_tests.rs": "",
    "planted/src/topic/unused.rs": "",
    "planted/tests/integration.rs": "",
    # Outside every crate: no target is meant to compile it.
    "tools/wrapper.rs": "fn main() {}\n",
}


class CompiledFilesTests(unittest.TestCase):
    def test_production_tree_passes(self) -> None:
        self.assertEqual(CHECK.check(ROOT), [])

    def test_a_file_no_target_compiles_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative, text in PLANTED.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            self.assertEqual(
                CHECK.check(root),
                ["planted/src/topic/unused.rs", "planted/src/topic_tests.rs"],
            )


if __name__ == "__main__":
    unittest.main()
