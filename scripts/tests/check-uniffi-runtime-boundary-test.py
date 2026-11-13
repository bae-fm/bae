#!/usr/bin/env python3
"""Run the UniFFI runtime boundary check on small trees, each holding only the
files its case reads."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECKER = ROOT / "scripts" / "check-uniffi-runtime-boundary.py"
BRIDGE = "bae-bridge/src/handle/fixture.rs"
LOGGER = "bae-avalonia/BaeLogger.cs"
SWIFT = "bae-macos/bae/bae/Views/FixtureScreen.swift"
KOTLIN = "bae-android/app/src/main/java/fm/bae/app/Fixture.kt"

OWNED = """
#[uniffi::export(async_runtime = "tokio")]
impl Fixture {
    pub async fn load(&self) -> Result<(), BridgeError> {
        self.runtime.run(async { Ok(()) }).await
    }
}
"""


class RuntimeBoundaryTests(unittest.TestCase):
    def run_check(self, files: dict[str, str]) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "scripts" / CHECKER.name
            script.parent.mkdir()
            shutil.copyfile(CHECKER, script)
            for relative, text in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            return subprocess.run(
                [sys.executable, str(script)],
                capture_output=True,
                text=True,
                check=False,
            )

    def assert_passes(self, files: dict[str, str]) -> None:
        result = self.run_check(files)
        self.assertEqual(result.returncode, 0, result.stderr)

    def assert_fails(self, files: dict[str, str], message: str) -> None:
        result = self.run_check(files)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(message, result.stderr)

    def test_an_export_on_the_owned_runtime_passes(self) -> None:
        self.assert_passes({BRIDGE: OWNED, LOGGER: "class BaeLogger {}\n"})

    def test_an_async_export_off_the_runtime_fails(self) -> None:
        self.assert_fails(
            {
                BRIDGE: OWNED.replace("self.runtime.run(async { Ok(()) }).await", "Ok(())"),
                LOGGER: "",
            },
            f"{BRIDGE}::load",
        )

    def test_a_comment_naming_run_exported_is_no_run(self) -> None:
        self.assert_fails(
            {
                BRIDGE: OWNED.replace(
                    "self.runtime.run(async { Ok(()) }).await",
                    "// run_exported would go here\n        Ok(())",
                ),
                LOGGER: "",
            },
            f"{BRIDGE}::load",
        )

    def test_a_string_naming_spawn_is_no_task(self) -> None:
        self.assert_passes(
            {
                BRIDGE: OWNED.replace(
                    "Ok(()) }).await",
                    'Ok(()) }).await?;\n        log("spawn(x) is not called");\n        Ok(())',
                ),
                LOGGER: "",
            }
        )

    def test_a_task_built_before_scheduling_fails(self) -> None:
        self.assert_fails(
            {
                BRIDGE: OWNED.replace(
                    "Ok(()) }).await", "Ok(()) }).await?;\n        tokio::spawn(work());\n        Ok(())"
                ),
                LOGGER: "",
            },
            "constructs a task before scheduling",
        )

    def test_the_flush_on_a_worker_thread_fails(self) -> None:
        self.assert_fails(
            {
                LOGGER: "var e = await Task.Run(() => NativeBae.FlushDiagnostics(Handle));\n",
            },
            "Avalonia diagnostics flush duplicates the bridge runtime dispatch",
        )

    def test_the_flush_on_a_worker_thread_fails_in_any_spacing(self) -> None:
        self.assert_fails(
            {
                LOGGER: "var e = await Task . Run(\n    () =>\n        NativeBae.FlushDiagnostics( host ));\n",
            },
            "Avalonia diagnostics flush duplicates the bridge runtime dispatch",
        )

    def test_a_comment_naming_the_flush_on_a_worker_thread_passes(self) -> None:
        self.assert_passes(
            {
                LOGGER: "// Task.Run(() => NativeBae.FlushDiagnostics(Handle)) is not used\n"
                "var e = await NativeBae.FlushDiagnostics(host);\n",
            }
        )

    def test_an_async_bridge_call_in_task_detached_fails(self) -> None:
        self.assert_fails(
            {
                LOGGER: "",
                SWIFT: "func f() {\n    Task.detached {\n        try await handle.load()\n    }\n}\n",
            },
            f"{SWIFT}:2 wraps an async bridge call in Task.detached",
        )

    def test_an_async_export_in_with_context_fails(self) -> None:
        self.assert_fails(
            {
                BRIDGE: OWNED,
                LOGGER: "",
                KOTLIN: "suspend fun f() {\n    withContext(Dispatchers.IO) {\n        handle.load()\n    }\n}\n",
            },
            f"{KOTLIN}:2 wraps async load in withContext",
        )


if __name__ == "__main__":
    unittest.main()
