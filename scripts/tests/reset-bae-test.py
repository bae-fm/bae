#!/usr/bin/env python3
"""Tests for the exit wait in scripts/reset-bae.py, on throwaway child
processes. Never touches bae."""

import importlib.util
import subprocess
import sys
import time
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "reset-bae.py"
spec = importlib.util.spec_from_file_location("reset_bae", SCRIPT)
reset_bae = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reset_bae)


class WaitForExitTests(unittest.TestCase):
    def spawn(self, seconds):
        child = subprocess.Popen(["sleep", str(seconds)])
        self.addCleanup(child.wait)
        self.addCleanup(child.kill)
        return child

    def test_returns_when_the_process_exits(self):
        child = self.spawn(0.5)
        started = time.monotonic()
        reset_bae.wait_for_exit(child.pid, 10)
        self.assertLess(time.monotonic() - started, 5)
        self.assertGreaterEqual(time.monotonic() - started, 0.3)

    def test_returns_for_a_process_already_gone(self):
        child = self.spawn(0)
        child.wait()
        reset_bae.wait_for_exit(child.pid, 10)

    def test_fails_loudly_naming_the_pid_when_it_outlives_the_timeout(self):
        child = self.spawn(30)
        with self.assertRaises(SystemExit) as raised:
            reset_bae.wait_for_exit(child.pid, 0.3)
        self.assertEqual(raised.exception.code, 1)

    def test_failure_message_names_the_pid(self):
        child = self.spawn(30)
        captured = subprocess.run(
            [sys.executable, "-c",
             "import importlib.util as u,sys;"
             f"s=u.spec_from_file_location('r',{str(SCRIPT)!r});"
             "m=u.module_from_spec(s);s.loader.exec_module(m);"
             f"m.wait_for_exit({child.pid},0.3)"],
            capture_output=True, text=True,
        )
        self.assertEqual(captured.returncode, 1)
        self.assertIn(f"pid {child.pid}", captured.stderr)


if __name__ == "__main__":
    unittest.main()
