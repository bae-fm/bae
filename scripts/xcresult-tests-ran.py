#!/usr/bin/env python3
"""Refuse a baeTests run where a test filter ran no tests.

xcodebuild reports TEST SUCCEEDED when an `-only-testing:baeTests/<name>`
filter matches nothing, so a misnamed filter (a file name instead of the Swift
type name the filter takes) passes without running a test. This reads the
test list `xcrun xcresulttool get test-results tests` prints for the run's
result bundle and counts, for each filter, the tests under it that ran; a
filter with none fails the check. With no filters, the run as a whole must
have run a test.

With `--bundle <name>` first after the file, the filters are under that test
bundle (`baeShots`) instead of baeTests.

Usage: xcresult-tests-ran.py <tests.json> [--bundle <name>] [<Type>[/<test>] ...]
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

BUNDLE_PREFIX = "test://com.apple.xcode/bae/{}/"


def ran_tests(nodes: list[dict], bundle: str = "baeTests") -> list[str]:
    """The identifiers, relative to <bundle>, of every test case that ran."""
    prefix = BUNDLE_PREFIX.format(bundle)
    identifiers = []
    for node in nodes:
        url = node.get("nodeIdentifierURL", "")
        if (
            node.get("nodeType") == "Test Case"
            and node.get("result") != "Skipped"
            and url.startswith(prefix)
        ):
            identifiers.append(url[len(prefix):])
        identifiers.extend(ran_tests(node.get("children", []), bundle))
    return identifiers


def under(identifier: str, test_filter: str) -> bool:
    return identifier == test_filter or identifier.startswith(test_filter + "/")


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    tests = json.loads(Path(argv[1]).read_text())
    rest = argv[2:]
    bundle = "baeTests"
    if rest[:1] == ["--bundle"]:
        if len(rest) < 2:
            print("--bundle needs a name", file=sys.stderr)
            return 2
        bundle, rest = rest[1], rest[2:]
    ran = ran_tests(tests.get("testNodes", []), bundle)
    filters = rest
    if not filters:
        if not ran:
            print("the run ran no tests", file=sys.stderr)
            return 1
        print(f"{len(ran)} tests ran")
        return 0
    empty = []
    for test_filter in filters:
        count = sum(1 for identifier in ran if under(identifier, test_filter))
        print(f"{test_filter}: {count} test{'' if count == 1 else 's'}")
        if count == 0:
            empty.append(test_filter)
    for test_filter in empty:
        print(
            f"{test_filter} ran no tests: a filter names a Swift test type "
            "(or Type/test), not a file",
            file=sys.stderr,
        )
    return 1 if empty else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
