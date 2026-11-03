#!/usr/bin/env python3
"""Tests for scripts/xcresult-tests-ran.py on the test lists xcresulttool
prints for a run."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

CHECK = Path(__file__).resolve().parent.parent / "xcresult-tests-ran.py"
BUNDLE = "test://com.apple.xcode/bae/baeTests"


def case(identifier, result="Passed"):
    return {
        "name": identifier,
        "nodeIdentifierURL": f"{BUNDLE}/{identifier}",
        "nodeType": "Test Case",
        "result": result,
    }


def suite(name, *children):
    return {
        "children": list(children),
        "name": name,
        "nodeIdentifierURL": f"{BUNDLE}/{name}",
        "nodeType": "Test Suite",
        "result": "Passed",
    }


def report(*suites):
    """The shape of `xcresulttool get test-results tests`: a test plan holding
    the unit test bundle holding its suites."""
    return {
        "testNodes": [
            {
                "children": [
                    {
                        "children": list(suites),
                        "name": "baeTests",
                        "nodeType": "Unit test bundle",
                        "result": "Passed",
                    }
                ],
                "name": "bae",
                "nodeType": "Test Plan",
                "result": "Passed",
            }
        ]
    }


def run(tests, *filters):
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "tests.json"
        path.write_text(json.dumps(tests))
        return subprocess.run(
            [sys.executable, str(CHECK), str(path), *filters],
            capture_output=True,
            text=True,
        )


RUN = report(
    suite(
        "PlaybackStoreVolumeTests",
        case("PlaybackStoreVolumeTests/volumeFollowsCore()"),
        case("PlaybackStoreVolumeTests/muteFollowsCore()"),
    ),
    suite(
        "LibraryDetailTests",
        suite(
            "LibraryDetailTests/Composer",
            case("LibraryDetailTests/Composer/movesOneRead()"),
        ),
    ),
    suite("DisabledTests", case("DisabledTests/notYet()", result="Skipped")),
    suite("AppDelegateTests", case("AppDelegateTests/testQuitStopsPlayback")),
)


class TestsRan(unittest.TestCase):
    def test_a_type_name_that_ran_passes(self):
        result = run(RUN, "PlaybackStoreVolumeTests")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PlaybackStoreVolumeTests: 2 tests", result.stdout)

    def test_a_file_name_matches_no_suite_and_fails(self):
        result = run(RUN, "PlaybackStoreTests")
        self.assertEqual(result.returncode, 1)
        self.assertIn("PlaybackStoreTests ran no tests", result.stderr)

    def test_one_empty_filter_fails_the_run_beside_ones_that_ran(self):
        result = run(RUN, "PlaybackStoreVolumeTests", "AppServiceSubscriptionTests")
        self.assertEqual(result.returncode, 1)
        self.assertIn("AppServiceSubscriptionTests ran no tests", result.stderr)
        self.assertNotIn("PlaybackStoreVolumeTests ran no tests", result.stderr)

    def test_a_single_test_and_a_nested_suite_count_their_cases(self):
        result = run(
            RUN,
            "PlaybackStoreVolumeTests/muteFollowsCore()",
            "LibraryDetailTests",
            "AppDelegateTests/testQuitStopsPlayback",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PlaybackStoreVolumeTests/muteFollowsCore(): 1 test\n", result.stdout)
        self.assertIn("LibraryDetailTests: 1 test\n", result.stdout)

    def test_a_name_prefix_is_not_a_match(self):
        result = run(RUN, "PlaybackStore")
        self.assertEqual(result.returncode, 1)

    def test_skipped_tests_did_not_run(self):
        result = run(RUN, "DisabledTests")
        self.assertEqual(result.returncode, 1)
        self.assertIn("DisabledTests ran no tests", result.stderr)

    def test_an_unfiltered_run_needs_a_test(self):
        self.assertEqual(run(RUN).returncode, 0)
        result = run(report())
        self.assertEqual(result.returncode, 1)
        self.assertIn("the run ran no tests", result.stderr)


if __name__ == "__main__":
    unittest.main()
