#!/usr/bin/env python3
"""Exercise bae-bridge/xcframework.py's reading of an xcframework's slices
against a planted Info.plist."""

from __future__ import annotations

import importlib.util
import plistlib
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "bae-bridge" / "xcframework.py"
SPEC = importlib.util.spec_from_file_location("xcframework", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
XCFRAMEWORK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(XCFRAMEWORK)


def plant(root: Path, libraries: list[dict]) -> Path:
    """An xcframework at `root` whose Info.plist lists `libraries`."""
    xcframework = root / "BaeBridgeFFI.xcframework"
    xcframework.mkdir()
    with open(xcframework / "Info.plist", "wb") as handle:
        plistlib.dump({"AvailableLibraries": libraries, "XCFrameworkFormatVersion": "1.0"}, handle)
    return xcframework


MACOS = {
    "LibraryIdentifier": "macos-arm64",
    "LibraryPath": "libbae_bridge.a",
    "HeadersPath": "Headers",
    "SupportedPlatform": "macos",
}
IOS_DEVICE = {
    "LibraryIdentifier": "ios-arm64",
    "LibraryPath": "libbae_bridge_merged.a",
    "HeadersPath": "Headers",
    "SupportedPlatform": "ios",
}
IOS_SIMULATOR = {
    "LibraryIdentifier": "ios-arm64-simulator",
    "LibraryPath": "libbae_bridge_merged.a",
    "HeadersPath": "Headers",
    "SupportedPlatform": "ios",
    "SupportedPlatformVariant": "simulator",
}


class XcframeworkTest(unittest.TestCase):
    def test_an_absent_xcframework_holds_no_slice(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            absent = Path(root) / "BaeBridgeFFI.xcframework"
            self.assertFalse(XCFRAMEWORK.has("macos", str(absent)))
            self.assertEqual(XCFRAMEWORK.carried_arguments("macos", str(absent)), [])

    def test_a_platform_is_held_only_by_its_own_slices(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            xcframework = str(plant(Path(root), [IOS_DEVICE, IOS_SIMULATOR]))
            self.assertTrue(XCFRAMEWORK.has("ios", xcframework))
            self.assertFalse(XCFRAMEWORK.has("macos", xcframework))

    def test_writing_one_platform_carries_every_slice_of_the_other(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            xcframework = plant(Path(root), [MACOS, IOS_DEVICE, IOS_SIMULATOR])
            self.assertEqual(
                XCFRAMEWORK.carried_arguments("macos", str(xcframework)),
                [
                    "-library",
                    str(xcframework / "ios-arm64" / "libbae_bridge_merged.a"),
                    "-headers",
                    str(xcframework / "ios-arm64" / "Headers"),
                    "-library",
                    str(xcframework / "ios-arm64-simulator" / "libbae_bridge_merged.a"),
                    "-headers",
                    str(xcframework / "ios-arm64-simulator" / "Headers"),
                ],
            )
            self.assertEqual(
                XCFRAMEWORK.carried_arguments("ios", str(xcframework)),
                [
                    "-library",
                    str(xcframework / "macos-arm64" / "libbae_bridge.a"),
                    "-headers",
                    str(xcframework / "macos-arm64" / "Headers"),
                ],
            )


if __name__ == "__main__":
    unittest.main()
