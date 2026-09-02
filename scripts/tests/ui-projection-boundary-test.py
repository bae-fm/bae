#!/usr/bin/env python3
"""Exercise the UI projection boundary against production leaf declarations."""

from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "ui-projection-boundary.py"
SPEC = importlib.util.spec_from_file_location("ui_projection_boundary", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
BOUNDARY = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BOUNDARY
SPEC.loader.exec_module(BOUNDARY)


class UiProjectionBoundaryTests(unittest.TestCase):
    def copied_inventory(self, destination: Path) -> None:
        for leaf in BOUNDARY.RENDER_LEAVES:
            source = ROOT / leaf.path
            target = destination / leaf.path
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

    def test_classified_playback_child_only_allows_playback_owner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path("bae-ios/bae/bae/Views/AlbumDetail/TrackList.swift")
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, path)
            source = path.read_text()
            track_start = source.index("private struct TrackRow: View")
            body_start = source.index("var body: some View {", track_start)
            source = (
                source[:body_start]
                + "var body: some View {\n        _ = ImportStore.shared"
                + source[body_start + len("var body: some View {") :]
            )
            path.write_text(source)
            injected_line = source.count("\n", 0, source.index("ImportStore.shared")) + 1
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:{injected_line}: TrackRow reaches entity-data owner "
                    "ImportStore"
                ],
            )

    def test_classified_avalonia_template_is_owner_checked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path("bae-avalonia/Views/Library/AlbumExpansionView.cs")
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, path)
            source = path.read_text().replace(
                "release?.DisplayName ?? string.Empty",
                "AppService.Current.SelectedRelease.Title",
                1,
            )
            path.write_text(source)
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:198: repeated Avalonia child TextBlock reaches "
                    "entity-data owner AppService"
                ],
            )

    def test_new_avalonia_item_template_requires_classification(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.copied_inventory(root)
            relative = Path("bae-avalonia/Views/FixtureList.cs")
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(
                """
var list = new ItemsControl
{
    ItemTemplate = new FuncDataTemplate<Row>((row, _) => new FixtureRow(row)),
};
"""
            )
            self.assertEqual(
                BOUNDARY.check(root),
                [
                    f"{relative}:4: repeated Avalonia child unclassified has no "
                    "projection-boundary classification"
                ],
            )


if __name__ == "__main__":
    unittest.main()
