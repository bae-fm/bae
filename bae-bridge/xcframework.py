#!/usr/bin/env python3
"""BaeKit's one bridge xcframework, which holds a slice per platform built.

BaeKit/Package.swift declares one binary target, always: a manifest that
looked on disk for which xcframeworks exist is cached by its text, not by
what it found, so a framework deleted after an evaluation still read as
present. Each platform's build script writes only its own slices with
`write`, and the slices of the other platform already in the xcframework are
carried over unchanged, so a checkout that builds both platforms keeps both.

    xcframework.py has <macos|ios> <xcframework>
        Exit 0 when the xcframework holds a slice for the platform.
    xcframework.py write <macos|ios> <xcframework> -library <lib> -headers <dir> ...
        Replace the platform's slices with the libraries given.
"""

import os
import plistlib
import shutil
import subprocess
import sys
import tempfile

PLATFORMS = ("macos", "ios")


def libraries(xcframework):
    """The slices the xcframework at `xcframework` holds, or none when absent."""
    info = os.path.join(xcframework, "Info.plist")
    if not os.path.isfile(info):
        return []
    with open(info, "rb") as handle:
        return plistlib.load(handle)["AvailableLibraries"]


def has(platform, xcframework):
    return any(library["SupportedPlatform"] == platform for library in libraries(xcframework))


def carried_arguments(platform, xcframework):
    """The `-library`/`-headers` arguments that carry every slice of another
    platform in `xcframework` into the next one written."""
    carried = []
    for library in libraries(xcframework):
        if library["SupportedPlatform"] == platform:
            continue
        root = os.path.join(xcframework, library["LibraryIdentifier"])
        carried += ["-library", os.path.join(root, library["LibraryPath"])]
        if "HeadersPath" in library:
            carried += ["-headers", os.path.join(root, library["HeadersPath"])]
    return carried


def write(platform, xcframework, own_arguments):
    carried = carried_arguments(platform, xcframework)
    parent = os.path.dirname(os.path.abspath(xcframework))
    os.makedirs(parent, exist_ok=True)
    staging = tempfile.mkdtemp(prefix=".xcframework.", dir=parent)
    try:
        written = os.path.join(staging, os.path.basename(xcframework))
        subprocess.run(
            ["xcodebuild", "-create-xcframework", *own_arguments, *carried, "-output", written],
            check=True,
        )
        replaced = os.path.join(staging, "replaced")
        if os.path.exists(xcframework):
            os.rename(xcframework, replaced)
        os.rename(written, xcframework)
    finally:
        shutil.rmtree(staging)


def main(arguments):
    if len(arguments) < 3 or arguments[0] not in ("has", "write") or arguments[1] not in PLATFORMS:
        sys.exit(__doc__)
    command, platform, xcframework, rest = arguments[0], arguments[1], arguments[2], arguments[3:]
    if command == "has":
        if rest:
            sys.exit(__doc__)
        sys.exit(0 if has(platform, xcframework) else 1)
    if not rest:
        sys.exit(__doc__)
    write(platform, xcframework, rest)


if __name__ == "__main__":
    main(sys.argv[1:])
