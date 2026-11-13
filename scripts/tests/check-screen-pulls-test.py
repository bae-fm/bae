#!/usr/bin/env python3
"""Exercise the screen-pull check against the real sources."""

from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "check-screen-pulls.py"
# The check reads sources through the module beside it, which a script run
# from `scripts/` imports by name.
sys.path.insert(0, str(MODULE_PATH.parent))
SPEC = importlib.util.spec_from_file_location("check_screen_pulls", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
CHECK = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECK
SPEC.loader.exec_module(CHECK)

SWIFT_HELPER = CHECK.APPS[0].helper
KOTLIN_HELPER = CHECK.APPS[1].helper
SWIFT_VIEW = "bae-macos/bae/bae/Views/FixtureScreen.swift"
KOTLIN_VIEW = "bae-android/app/src/main/java/fm/bae/app/ui/FixtureScreen.kt"
BRIDGE_HANDLE = f"{CHECK.BRIDGE_ROOT}/handle/fixture_screen.rs"


class ScreenPullTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.callback_files = [
            relative
            for relative, tokens in CHECK.bridge_files(ROOT)
            if CHECK.callback_interfaces(tokens)
        ]

    def copied_tree(self, destination: Path) -> None:
        """Only the files every case needs: the bridge files that declare the
        listed callback interfaces, each app's helper, and each file the
        check's lists name. A case adds its own fixture file to these."""
        needed = {
            *self.callback_files,
            *(app.helper for app in CHECK.APPS if app.helper),
            *(relative for relative, _ in CHECK.OTHER_NEXT_CALLS),
            *(relative for relative, _ in CHECK.OTHER_FOR_AWAITS),
        }
        for relative in needed:
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, target)

    def write(self, root: Path, relative: str, text: str) -> None:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def test_production_tree_passes(self) -> None:
        self.assertEqual(CHECK.check(ROOT), [])

    def test_a_callback_interface_for_screen_data_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                BRIDGE_HANDLE,
                "#[uniffi::export(callback_interface)]\n"
                "pub trait AlbumsCallback: Send + Sync {\n"
                "    fn on_albums(&self, albums: Vec<BridgeAlbum>);\n"
                "}\n",
            )
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{BRIDGE_HANDLE}:2: callback interface AlbumsCallback is "
                    "not a listed platform service; a screen pulls its values"
                ],
            )

    def test_a_foreign_trait_for_screen_data_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                BRIDGE_HANDLE,
                "#[uniffi::export(with_foreign)]\n"
                "pub trait QueueSink: Send + Sync {\n"
                "    fn on_queue(&self, queue: BridgeQueueSnapshot);\n"
                "}\n",
            )
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{BRIDGE_HANDLE}:2: callback interface QueueSink is not "
                    "a listed platform service; a screen pulls its values"
                ],
            )

    def test_a_listed_callback_the_bridge_dropped_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            path = root / CHECK.BRIDGE_ROOT / "init.rs"
            path.write_text(
                path.read_text().replace(
                    "#[uniffi::export(callback_interface)]\npub trait BridgeBugReporter",
                    "pub trait BridgeBugReporter",
                )
            )
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{CHECK.BRIDGE_ROOT}: declares no callback interface "
                    "BridgeBugReporter; the list names what the bridge no "
                    "longer has"
                ],
            )

    def test_a_pull_object_outside_the_screens_module_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                BRIDGE_HANDLE,
                "#[derive(uniffi::Object)]\n"
                "pub struct RatingsPull {\n"
                "    inner: LiveValues<Vec<Rating>>,\n"
                "}\n"
                "\n"
                "#[uniffi::export(async_runtime = \"tokio\", cancellable)]\n"
                "impl RatingsPull {\n"
                "    pub async fn next(self: Arc<Self>) -> Result<Vec<BridgeRating>, BridgeError> {\n"
                "        todo!()\n"
                "    }\n"
                "}\n",
            )
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{BRIDGE_HANDLE}:8: RatingsPull is pulled from but "
                    "defined outside the screens module"
                ],
            )

    def test_a_stream_built_outside_the_screens_module_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                BRIDGE_HANDLE,
                "fn levels(runtime: OperationRuntime) -> Arc<KeyedValueStream> {\n"
                "    let values = StreamValues::new();\n"
                "    Arc::new(KeyedValueStream::new(values, CancellationToken::new(), runtime))\n"
                "}\n",
            )
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{BRIDGE_HANDLE}:2: builds a StreamValues outside the "
                    "screens module",
                    f"{BRIDGE_HANDLE}:3: builds a KeyedValueStream outside the "
                    "screens module",
                ],
            )

    def test_a_swift_screen_pulling_itself_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                SWIFT_VIEW,
                "import BaeKit\n"
                "\n"
                "func follow(_ subscription: QueueSubscription) async throws {\n"
                "    // subscription.next() in a comment is no pull\n"
                "    while true { _ = try await subscription.next() }\n"
                "}\n",
            )
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{SWIFT_VIEW}:5: pulls subscription.next() itself; a "
                    f"screen reads through {SWIFT_HELPER}"
                ],
            )

    def test_a_swift_screen_iterating_a_subscription_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                SWIFT_VIEW,
                "import BaeKit\n"
                "\n"
                "func follow(_ subscription: QueueSequence) async throws {\n"
                "    // for await x in subscription {} in a comment is no pull\n"
                "    for try await value in subscription {\n"
                "        use(value)\n"
                "    }\n"
                "}\n",
            )
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{SWIFT_VIEW}:5: iterates subscription with for await "
                    f"itself; a screen reads through {SWIFT_HELPER}"
                ],
            )

    def test_a_listed_sequence_the_app_dropped_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            ((relative, receiver), *_) = CHECK.OTHER_FOR_AWAITS
            path = root / relative
            path.write_text(
                path.read_text().replace(f"in {receiver}", "in other")
            )
            violations = CHECK.check(root)
            self.assertIn(
                f"{relative}: has no for await over {receiver}; the list "
                "names what the app no longer has",
                violations,
            )

    def test_a_kotlin_screen_pulling_itself_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(
                root,
                KOTLIN_VIEW,
                "suspend fun follow(queue: QueueSubscription?) {\n"
                "    val first = queue?.next()\n"
                "}\n",
            )
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{KOTLIN_VIEW}:2: pulls queue.next() itself; a screen "
                    f"reads through {KOTLIN_HELPER}"
                ],
            )

    def test_a_helper_that_pulls_nothing_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            self.write(root, SWIFT_HELPER, "// Moved elsewhere.\n")
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{SWIFT_HELPER}: the helper pulls nothing; name the file "
                    "that does"
                ],
            )

    def test_a_listed_receiver_the_app_dropped_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_tree(root)
            ((relative, receiver),) = CHECK.OTHER_NEXT_CALLS
            path = root / relative
            path.write_text(path.read_text().replace(f"{receiver}.next()", "()"))
            self.assertEqual(
                CHECK.check(root),
                [
                    f"{relative}: makes no {receiver}.next() call; the list "
                    "names what the app no longer has"
                ],
            )


if __name__ == "__main__":
    unittest.main()
