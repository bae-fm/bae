#!/usr/bin/env python3
"""Exercise the UI projection boundary against production leaf declarations."""

from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "ui-projection-boundary.py"
# The check reads its sources through the module beside it, which a script run
# from `scripts/` imports by name.
sys.path.insert(0, str(MODULE_PATH.parent))
import ui_view_sources as SOURCES  # noqa: E402
SPEC = importlib.util.spec_from_file_location("ui_projection_boundary", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
BOUNDARY = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BOUNDARY
SPEC.loader.exec_module(BOUNDARY)


class UiProjectionBoundaryTests(unittest.TestCase):
    def copied_inventory(self, destination: Path) -> None:
        # The whole UI source trees: a row's children, and the views the
        # check holds open findings for, are declared all over them.
        for source_root, suffix in (
            *((root, ".swift") for root in SOURCES.SWIFT_SOURCE_ROOTS),
            (SOURCES.KOTLIN_SOURCE_ROOT, ".kt"),
        ):
            for source in (ROOT / source_root).rglob(f"*{suffix}"):
                target = destination / source.relative_to(ROOT)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)

    def test_production_inventory_passes(self) -> None:
        self.assertEqual(BOUNDARY.check(ROOT), [])

    def test_selected_detail_input_in_a_real_leaf_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path(
                "bae-macos/bae/bae/Views/Import/Candidates/TriageRowView.swift"
            )
            path = root / relative
            source = path.read_text()
            insertion = "struct TriageRowView: View {"
            source = source.replace(
                insertion,
                f"{insertion}\n"
                "    let selectedCandidate: Candidate",
                1,
            )
            path.write_text(source)
            injected_line = source.count("\n", 0, source.index("Candidate")) + 1
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:{injected_line}: TriageRowView reaches secondary "
                    "entity projection Candidate",
                ],
            )

    def test_extension_cannot_reach_selected_detail_store(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path(
                "bae-macos/bae/bae/Views/Import/Candidates/TriageRowView.swift"
            )
            path = root / relative
            source = path.read_text()
            insertion = "extension TriageRowView {"
            source = source.replace(
                insertion,
                "extension\nTriageRowView {\n"
                "    private var selectedDetailStore: ImportStore { fatalError() }",
                1,
            )
            path.write_text(source)
            injected_line = source.count(
                "\n", 0, source.index("selectedDetailStore: ImportStore")
            ) + 1
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:{injected_line}: TriageRowView reaches entity-data owner "
                    "ImportStore"
                ],
            )

    def test_projection_presentation_inputs_and_local_state_pass(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path("bae-macos/bae/bae/Views/FixturePresentationList.swift")
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(
                """
import SwiftUI

struct FixturePresentationRow: View {
    static let coverPointSize: CGFloat = 44
    let row: FixtureProjection
    let coverContent: ImageContent?
    let selection: Binding<Bool>?
    let onSkip: (_ skipped: Bool) -> Void
    @State private var isHovered = false
    @Environment(\\.displayScale) private var displayScale
    @Environment(UiStore.self) private var uiStore
    var body: some View { Text(row.title) }
}

struct FixturePresentationList: View {
    let rows: [FixtureProjection]
    var body: some View {
        ForEach(rows) { row in
            FixturePresentationRow(
                row: row,
                coverContent: nil,
                selection: nil,
                onSkip: { _ in }
            )
        }
    }
}
"""
            )
            self.assertEqual(BOUNDARY.check(root), [])
            path.write_text(
                path.read_text().replace(
                    "coverPointSize: CGFloat = 44", "coverPointSize: CGFloat = 50"
                )
            )
            self.assertEqual(BOUNDARY.check(root), [])

    def test_entity_projection_in_local_state_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path(
                "bae-macos/bae/bae/Views/Import/Candidates/TriageRowView.swift"
            )
            path = root / relative
            source = path.read_text().replace(
                "struct TriageRowView: View {",
                "struct TriageRowView: View {\n"
                "    @State private var selectedCandidate: Candidate",
                1,
            )
            path.write_text(source)
            injected_line = source.count("\n", 0, source.index("Candidate")) + 1
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:{injected_line}: TriageRowView reaches secondary "
                    "entity projection Candidate",
                ],
            )

    def test_new_repeated_child_is_discovered_and_checked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path("bae-macos/bae/bae/Views/FixtureProjectionList.swift")
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(
                """
import SwiftUI

struct FixtureProjectionRow: View {
    let row: FixtureProjection
    @Environment(ImportStore.self) private var selectedDetailStore
    var body: some View { Text(row.title) }
}

struct FixtureProjectionList: View {
    let rows: [FixtureProjection]
    var body: some View {
        ForEach(rows) { row in
            FixtureProjectionRow(row: row)
        }
    }
}
"""
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:6: FixtureProjectionRow reaches entity-data owner "
                    "ImportStore",
                    f"{relative}:6: FixtureProjectionRow takes ImportStore from the "
                    "environment",
                ],
            )

    def test_environment_object_of_any_type_is_an_undeclared_input(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path("bae-macos/bae/bae/Views/FixtureEnvironmentList.swift")
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(
                """
import SwiftUI

struct FixtureEnvironmentRow: View {
    let row: FixtureProjection
    @Environment(Automation.self) private var automation
    @ObservedObject var legacy: LegacyModel
    var body: some View { Text(row.title) }
}

struct FixtureEnvironmentList: View {
    let rows: [FixtureProjection]
    var body: some View {
        ForEach(rows) { row in
            FixtureEnvironmentRow(row: row, legacy: LegacyModel())
        }
    }
}
"""
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:6: FixtureEnvironmentRow takes Automation from the "
                    "environment",
                    f"{relative}:7: FixtureEnvironmentRow takes LegacyModel from the "
                    "environment",
                ],
            )

    def test_child_views_built_under_a_repeated_row_are_checked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            list_relative = Path("bae-macos/bae/bae/Views/FixtureChildList.swift")
            list_path = root / list_relative
            list_path.parent.mkdir(parents=True, exist_ok=True)
            list_path.write_text(
                """
import SwiftUI

struct FixtureChildRow: View {
    let row: FixtureProjection
    var body: some View {
        VStack {
            FixtureChildName(row: row)
            FixtureChildProgress(row: row)
        }
    }
}

private struct FixtureChildName: View {
    let row: FixtureProjection
    @Environment(UiStore.self) private var uiStore
    var body: some View { Text(row.title) }
}

struct FixtureChildList: View {
    let rows: [FixtureProjection]
    var body: some View {
        List(rows) { row in
            FixtureChildRow(row: row)
        }
    }
}
"""
            )
            child_relative = Path(
                "bae-macos/bae/bae/Views/Components/FixtureChildProgress.swift"
            )
            child_path = root / child_relative
            child_path.parent.mkdir(parents=True, exist_ok=True)
            child_path.write_text(
                """
import SwiftUI

struct FixtureChildProgress: View {
    let row: FixtureProjection
    @Environment(OutboxStore.self) private var outboxStore
    var body: some View {
        progressLine
    }

    private var progressLine: some View {
        fixtureChildCaption(row)
    }
}

func fixtureChildCaption(_ row: FixtureProjection) -> some View {
    FixtureChildCaption(library: nil)
}

struct FixtureChildCaption: View {
    let library: ImportStore?
    var body: some View { Text("") }
}
"""
            )
            under = "FixtureChildRow > FixtureChildProgress"
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{child_relative}:6: FixtureChildProgress (built under "
                    "repeated FixtureChildRow) reaches entity-data owner OutboxStore",
                    f"{child_relative}:6: FixtureChildProgress (built under "
                    "repeated FixtureChildRow) takes OutboxStore from the environment",
                    f"{child_relative}:21: FixtureChildCaption (built under "
                    f"repeated {under} > fixtureChildCaption) reaches entity-data "
                    "owner ImportStore",
                ],
            )

    def test_child_resolution_follows_swift_visibility(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            list_relative = Path("bae-macos/bae/bae/Views/FixtureScopedList.swift")
            list_path = root / list_relative
            list_path.parent.mkdir(parents=True, exist_ok=True)
            list_path.write_text(
                """
import SwiftUI

struct FixtureScopedRow: View {
    let row: FixtureProjection
    var body: some View {
        fixtureScopedBadge(row)
            .fixtureScopedChrome()
    }
}

struct FixtureScopedList: View {
    let rows: [FixtureProjection]
    var body: some View {
        ForEach(rows) { row in
            FixtureScopedRow(row: row)
        }
    }
}
"""
            )
            other_relative = Path("bae-macos/bae/bae/Views/FixtureScopedOther.swift")
            (root / other_relative).write_text(
                """
import SwiftUI

private func fixtureScopedBadge(_ row: FixtureProjection) -> some View {
    FixtureScopedStoreReader(library: nil)
}

struct FixtureScopedOwner: View {
    var body: some View { EmptyView() }

    func fixtureScopedChrome() -> some View {
        FixtureScopedStoreReader(library: nil)
    }
}

extension View {
    func fixtureScopedChrome() -> some View {
        FixtureScopedStoreReader(library: nil)
    }
}

struct FixtureScopedStoreReader: View {
    let library: ImportStore?
    var body: some View { EmptyView() }
}
"""
            )
            # The private function and the other type's member are out of the
            # row's reach; the `View` modifier is what `.fixtureScopedChrome()`
            # calls.
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{other_relative}:23: FixtureScopedStoreReader (built under "
                    "repeated FixtureScopedRow > fixtureScopedChrome) reaches "
                    "entity-data owner ImportStore",
                ],
            )

    OPEN_LIST = """
import SwiftUI

struct FixtureOpenRow: View {
    let row: FixtureProjection
    var body: some View {
        FixtureOpenReader(key: row.id)
    }
}

struct FixtureOpenList: View {
    let rows: [FixtureProjection]
    var body: some View {
        ForEach(rows) { row in
            FixtureOpenRow(row: row)
        }
    }
}

struct FixtureOpenReader: View {
    let key: String
    @Environment(ImportStore.self)
    private var importStore
    var body: some View {
        Text(importStore.title(key))
    }
}
"""

    def test_open_child_finding_passes_and_fails_once_it_stops_reading(
        self,
    ) -> None:
        relative = Path("bae-macos/bae/bae/Views/FixtureOpenList.swift")
        finding = BOUNDARY.OpenFinding(
            "swift",
            relative.as_posix(),
            "FixtureOpenReader",
            "ImportStore",
            "a fixture reading its owner under a repeated row",
        )
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(
            BOUNDARY, "OPEN_CHILD_FINDINGS", (finding,)
        ):
            root = Path(directory)
            self.copied_inventory(root)
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(self.OPEN_LIST)
            self.assertEqual(BOUNDARY.check(root), [])
            path.write_text(
                self.OPEN_LIST.replace("@Environment(ImportStore.self)", "")
                .replace("importStore.title(key)", "key")
                .replace("    private var importStore\n", "")
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{finding.path}: {finding.symbol} no longer reaches "
                    f"{finding.owner}; remove it from OPEN_CHILD_FINDINGS",
                ],
            )

    KEYED_LIST = """
import SwiftUI

struct FixtureKeyedRow: View {
    let row: FixtureProjection
    var body: some View {
        FixtureKeyedProgress(key: row.id)
    }
}

struct FixtureKeyedList: View {
    let rows: [FixtureProjection]
    var body: some View {
        ForEach(rows) { row in
            FixtureKeyedRow(row: row)
        }
    }
}

struct FixtureKeyedProgress: View {
    let key: String
    @Environment(Importer.self)
    private var importer: Importer?
    @State private var runtime: BridgeCandidateRuntimeSnapshot?
    var body: some View {
        Text("")
            .task(id: key) {
                guard let importer else { return }
                runtime = importer.candidateRuntime(key)
            }
    }
}
"""

    def keyed_fixture(self, root: Path, source: str) -> Path:
        relative = Path("bae-macos/bae/bae/Views/FixtureKeyedList.swift")
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source)
        return relative

    def keyed_findings(self, relative: Path, source: str) -> list[str]:
        line = source.count("\n", 0, source.index("@Environment(Importer.self)")) + 1
        under = "FixtureKeyedProgress (built under repeated FixtureKeyedRow)"
        return [
            f"{relative}:{line}: {under} reaches entity-data owner Importer",
            f"{relative}:{line}: {under} takes Importer from the environment",
        ]

    def test_leaf_reading_its_own_keyed_stream_passes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            self.keyed_fixture(root, self.KEYED_LIST)
            self.assertEqual(BOUNDARY.check(root), [])

    def test_leaf_reading_its_own_keyed_stream_by_a_labelled_key_passes(
        self,
    ) -> None:
        unread = BOUNDARY.KeyedStream(
            "Importer", "candidateSignals", "a stream read by a labelled key"
        )
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(
            BOUNDARY, "KEYED_STREAMS", (*BOUNDARY.KEYED_STREAMS, unread)
        ):
            root = Path(directory)
            self.copied_inventory(root)
            self.keyed_fixture(
                root,
                self.KEYED_LIST.replace(
                    "runtime = importer.candidateRuntime(key)",
                    "runtime = importer.candidateRuntime(key)\n"
                    "                _ = importer.candidateSignals(forCandidate: key)",
                ),
            )
            self.assertEqual(BOUNDARY.check(root), [])

    def test_keyed_stream_read_by_a_key_not_passed_in_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            source = self.KEYED_LIST.replace(
                "runtime = importer.candidateRuntime(key)",
                'let other = "candidate"\n'
                "                runtime = importer.candidateRuntime(other)",
            )
            relative = self.keyed_fixture(root, source)
            self.assertEqual(
                BOUNDARY.check(root), self.keyed_findings(relative, source)
            )

    def test_keyed_stream_owner_used_beyond_its_read_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            source = self.KEYED_LIST.replace(
                "runtime = importer.candidateRuntime(key)",
                "runtime = importer.candidateRuntime(key)\n"
                "                _ = importer.candidateSignals(key)",
            )
            relative = self.keyed_fixture(root, source)
            self.assertEqual(
                BOUNDARY.check(root), self.keyed_findings(relative, source)
            )

    def test_keyed_stream_no_view_reads_fails(self) -> None:
        unread = BOUNDARY.KeyedStream(
            "Importer", "candidateSignals", "a stream no row's leaf reads"
        )
        with mock.patch.object(
            BOUNDARY, "KEYED_STREAMS", (*BOUNDARY.KEYED_STREAMS, unread)
        ):
            self.assertEqual(
                BOUNDARY.check(ROOT),
                [
                    "Importer.candidateSignals: no view under a repeated row "
                    "reads it by its own key; remove it from KEYED_STREAMS"
                ],
            )

    def test_compose_child_built_under_a_repeated_row_is_checked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            row_relative = Path(
                "bae-android/app/src/main/java/fm/bae/app/ui/FixtureChildList.kt"
            )
            row_path = root / row_relative
            row_path.parent.mkdir(parents=True, exist_ok=True)
            row_path.write_text(
                """
@Composable
internal fun FixtureChildRow(row: FixtureProjection) {
    Column {
        FixtureChildProgress(row)
    }
}

@Composable
internal fun FixtureChildList(rows: List<FixtureProjection>) {
    LazyColumn {
        items(rows) { row -> FixtureChildRow(row) }
    }
}
"""
            )
            child_relative = Path(
                "bae-android/app/src/main/java/fm/bae/app/ui/FixtureChildProgress.kt"
            )
            child_path = root / child_relative
            child_path.write_text(
                """
@Composable
internal fun FixtureChildProgress(row: FixtureProjection) {
    val outbox = LocalOutboxStore.current
    Text(row.title)
}
"""
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{child_relative}:4: FixtureChildProgress (built under "
                    "repeated FixtureChildRow) reaches entity-data owner "
                    "LocalOutboxStore",
                ],
            )

    def test_alternate_swift_and_compose_repeat_forms_are_discovered(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            swift_relative = Path(
                "bae-macos/bae/bae/Views/FixtureContentProjectionList.swift"
            )
            swift_path = root / swift_relative
            swift_path.parent.mkdir(parents=True, exist_ok=True)
            swift_path.write_text(
                """
import SwiftUI

struct FixtureContentProjectionRow: View {
    let row: FixtureProjection
    let selectedCandidate: Candidate
    init(_ row: FixtureProjection) {
        self.row = row
        self.selectedCandidate = Candidate()
    }
    var body: some View { Text(row.title) }
}

private func fixtureLowerProjectionRow(
    _ row: FixtureProjection,
    selectedCandidate: Candidate
) -> some View {
    Text(row.title)
}

struct FixtureContentProjectionList: View {
    let rows: [FixtureProjection]
    var body: some View {
        ForEach(rows, content: FixtureContentProjectionRow.init)
        ForEach(rows) { row in
            fixtureLowerProjectionRow(row, selectedCandidate: Candidate())
        }
    }
}
"""
            )
            kotlin_relative = Path(
                "bae-android/app/src/main/java/fm/bae/app/ui/FixtureProjectionList.kt"
            )
            kotlin_path = root / kotlin_relative
            kotlin_path.parent.mkdir(parents=True, exist_ok=True)
            kotlin_path.write_text(
                """
@Composable
internal fun fixtureProjectionRow(row: FixtureProjection, selected: Candidate) {
    Text(row.title)
}

@Composable
internal fun FixtureProjectionList(rows: List<FixtureProjection>) {
    LazyColumn {
        items(items = rows, itemContent = { row ->
            fixtureProjectionRow(row, Candidate())
        })
    }
}
"""
            )
            violations = BOUNDARY.check(root)
            for relative, symbol in (
                (kotlin_relative, "fixtureProjectionRow"),
                (swift_relative, "FixtureContentProjectionRow"),
                (swift_relative, "fixtureLowerProjectionRow"),
            ):
                self.assertTrue(
                    any(
                        item.startswith(f"{relative}:")
                        and f"{symbol} reaches secondary entity projection Candidate"
                        in item
                        for item in violations
                    ),
                    violations,
                )

    def test_same_named_swift_children_are_scoped_per_platform(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative_paths = (
                Path("bae-macos/bae/bae/Views/FixtureDuplicateList.swift"),
                Path("bae-ios/bae/bae/Views/FixtureDuplicateList.swift"),
            )
            for relative in relative_paths:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(
                    """
import SwiftUI

struct DuplicateProjectionRow: View {
    let row: FixtureProjection
    let selectedCandidate: Candidate
    var body: some View { Text(row.title) }
}

struct FixtureDuplicateList: View {
    let rows: [FixtureProjection]
    var body: some View {
        ForEach(rows) { row in
            DuplicateProjectionRow(row: row, selectedCandidate: Candidate())
        }
    }
}
"""
                )
            violations = BOUNDARY.check(root)
            for relative in relative_paths:
                self.assertTrue(
                    any(
                        item.startswith(f"{relative}:")
                        and "secondary entity projection Candidate" in item
                        for item in violations
                    )
                )


if __name__ == "__main__":
    unittest.main()
