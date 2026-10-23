#!/usr/bin/env bash
# Print the iOS simulator bae builds, tests and runs on: the newest available
# iPhone, as "<udid><TAB><name>". run.sh, check.sh, the screenshot capture and
# iOS CI all choose through this, so they agree on one device. Exits non-zero,
# saying so, when no iPhone simulator is installed.
set -euo pipefail

xcrun simctl list devices available -j | python3 -c '
import json, sys

iphones = [
    device
    for runtime in json.load(sys.stdin)["devices"].values()
    for device in runtime
    if device.get("isAvailable") and device["name"].startswith("iPhone")
]
if not iphones:
    sys.exit(
        "No available iPhone simulator. Install an iOS runtime and an iPhone "
        "simulator in Xcode > Settings > Components."
    )
print(iphones[-1]["udid"] + "\t" + iphones[-1]["name"])
'
