#!/usr/bin/env python3
"""Exercise the keyed value stream boundary against the real sources."""

from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "keyed-value-stream-boundary.py"
# The check reads sources through the module beside it, which a script run
# from `scripts/` imports by name.
sys.path.insert(0, str(MODULE_PATH.parent))
SPEC = importlib.util.spec_from_file_location("keyed_value_stream_boundary", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
BOUNDARY = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BOUNDARY
SPEC.loader.exec_module(BOUNDARY)

SWIFT_ATTACH = "BaeKit/Sources/BaeKit/Services/Store/LatestValues.swift"
KOTLIN_ATTACH = "bae-android/fastvalues/src/main/java/fm/bae/fastvalues/LatestValues.kt"
SWIFT_VIEW = "bae-macos/bae/bae/Views/FixtureStream.swift"
KOTLIN_VIEW = "bae-android/app/src/main/java/fm/bae/app/ui/FixtureStream.kt"


class KeyedValueStreamBoundaryTests(unittest.TestCase):
    def copied_tree(self, destination: Path) -> None:
        """The bridge's sources and every app tree the check reads."""
        roots = [BOUNDARY.BRIDGE_ROOT]
        for app in BOUNDARY.APPS:
            roots.extend(app.roots)
        for root in roots:
            source = ROOT / root
            if source.is_dir():
                shutil.copytree(source, destination / root, dirs_exist_ok=True)

    def write(self, root: Path, relative: str, text: str) -> None:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def test_production_tree_passes(self) -> None:
        self.assertEqual(BOUNDARY.check(ROOT), [])

    def test_a_view_naming_the_stream_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                SWIFT_VIEW,
                "import BaeKit\n\nstruct FixtureStream {\n    let stream: KeyedValueStream\n}\n",
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{SWIFT_VIEW}:4: touches the bridge's KeyedValueStream, "
                    f"which only {SWIFT_ATTACH} may"
                ],
            )

    def test_a_store_calling_a_feed_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                KOTLIN_VIEW,
                "package fm.bae.app.ui\n\n"
                "fun fixture(appHandle: AppHandleInterface, activity: Activity) {\n"
                "    appHandle.feedDownloadTransfers(activity)\n"
                "}\n",
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{KOTLIN_VIEW}:4: touches the bridge call "
                    f"feedDownloadTransfers, which only {KOTLIN_ATTACH} may"
                ],
            )

    def test_reading_a_value_enum_case_through_an_interpolation_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                SWIFT_VIEW,
                'let text = "\\(BridgeFastValue.exportPercent(percent: 1))"\n',
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{SWIFT_VIEW}:1: touches the bridge's BridgeFastValue, "
                    f"which only {SWIFT_ATTACH} may"
                ],
            )

    def test_a_comment_or_a_string_naming_the_stream_passes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                SWIFT_VIEW,
                "// The KeyedValueStream feeds this view.\n"
                'let label = "KeyedValueStream"\n',
            )
            self.write(
                root,
                KOTLIN_VIEW,
                "/* feedDownloadTransfers is the attach API's */\n"
                'val label = "$name feedDownloadTransfers"\n',
            )
            self.assertEqual(BOUNDARY.check(root), [])

    def test_app_code_writing_a_stream_like_a_test_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                KOTLIN_VIEW,
                "val positions = LatestStream.written<BridgeTrackPosition>()\n",
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{KOTLIN_VIEW}:1: touches the test writer written, "
                    f"which only {KOTLIN_ATTACH} may"
                ],
            )

    def test_the_skeleton_touching_the_stream_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            relative = "bae-avalonia/FixtureStream.cs"
            self.write(
                root,
                relative,
                'var label = $"{stream} KeyedValueStream";\nKeyActivityCallback activity;\n',
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:2: touches the bridge's KeyActivityCallback, "
                    "which only an attach API may"
                ],
            )

    def test_tests_and_generated_bindings_are_not_the_apps_code(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                "bae-macos/bae/bae/bae_bridge.swift",
                "open class KeyedValueStream {}\n",
            )
            self.write(
                root,
                "bae-avalonia/bae-avalonia.Tests/FixtureTest.cs",
                "KeyedValueStream stream;\n",
            )
            self.assertEqual(BOUNDARY.check(root), [])

    def test_an_attach_api_touching_no_stream_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(root, SWIFT_ATTACH, "// Moved elsewhere.\n")
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{SWIFT_ATTACH}: the attach API touches no stream; name "
                    "the file that does"
                ],
            )

    def test_a_new_feed_is_covered_as_the_bridge_adds_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                f"{BOUNDARY.BRIDGE_ROOT}/handle/fixture_levels.rs",
                "impl AppHandle {\n"
                "    pub fn feed_meter_levels(\n"
                "        &self,\n"
                "        activity: Box<dyn KeyActivityCallback>,\n"
                "    ) -> Arc<KeyedValueStream> {\n"
                "        todo!()\n"
                "    }\n"
                "}\n",
            )
            self.write(root, SWIFT_VIEW, "_ = appHandle.feedMeterLevels(activity: a)\n")
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{SWIFT_VIEW}:1: touches the bridge call feedMeterLevels, "
                    f"which only {SWIFT_ATTACH} may"
                ],
            )


if __name__ == "__main__":
    unittest.main()
