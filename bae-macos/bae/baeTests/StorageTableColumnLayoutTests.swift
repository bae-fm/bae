import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct StorageTableColumnLayoutTests {
    /// The person hid the artist, media and date added and narrowed the
    /// rest: each column but Release keeps its width, and Release takes the
    /// width the others leave, as the board's `minmax(280px, 1fr)` track
    /// does.
    @Test("The release column fills the width the others leave")
    func theReleaseColumnFillsTheWidthTheOthersLeave() async throws {
        let widths: [StorageTableColumn: CGFloat] = [
            .storage: 240, .files: 44, .size: 76,
        ]
        try await Self.withTable(width: 700, showing: widths) { table in
            let shown = table.tableColumns.filter { !$0.isHidden }
            #expect(
                shown.map(\.identifier.rawValue)
                    == ["release", "storage", "files", "size"]
            )
            for column in shown where column.identifier.rawValue != "release" {
                let id = try #require(
                    StorageTableColumn(rawValue: column.identifier.rawValue)
                )
                #expect(column.width == widths[id])
            }
            let release = try #require(shown.first)
            let others = shown.dropFirst().map(\.width).reduce(0, +)
            let visible = try #require(table.enclosingScrollView)
                .contentView.bounds.width
            #expect(abs(release.width - (visible - others)) < 1.5)
            #expect(release.width > StorageTableColumn.release.minWidth)
        }
    }

    /// Where the others leave less than 280 pt, Release keeps 280 and the
    /// table scrolls to reach the rest.
    @Test("The release column keeps 280 pt in a narrow table")
    func theReleaseColumnKeepsItsMinimumInANarrowTable() async throws {
        let widths: [StorageTableColumn: CGFloat] = [
            .storage: 240, .files: 44, .size: 76,
        ]
        try await Self.withTable(width: 500, showing: widths) { table in
            let release = try #require(
                table.tableColumns.first { $0.identifier.rawValue == "release" }
            )
            #expect(release.minWidth == 280)
            #expect(release.width == 280)
            let storage = try #require(
                table.tableColumns.first { $0.identifier.rawValue == "storage" }
            )
            #expect(storage.width == 240)
        }
    }

    /// `check` on the table of a window `width` points wide, laid out to
    /// show the columns `widths` names at those widths.
    private static func withTable(
        width: CGFloat,
        showing widths: [StorageTableColumn: CGFloat],
        check: (NSTableView) async throws -> Void
    ) async throws {
        let layout = try #require(Self.layout(showing: widths))
        let fixture = StorageManagerPreviewFixture(
            rows: Array(PreviewData.storageRows.prefix(3)),
            downloads: [],
            outputs: [],
            uploads: []
        )
        let runner = StorageActionRunner(
            releaseEditor: .stub(),
            sync: .stub(),
            downloads: .stub(),
            outputs: .stub(),
            configStore: PreviewData.configStore(),
            uiStore: fixture.uiStore,
            openSaveAsSettings: {}
        )
        let size = NSSize(width: width, height: 200)
        try await SnapshotTestSupport.withHostedWindow(
            fixture.environment(
                of: LayoutTable(
                    store: fixture.storageManagerStore,
                    layout: layout,
                    runner: runner
                )
                .frame(width: size.width, height: size.height)
            ),
            size: size
        ) { _, host in
            try await Wait.until {
                fixture.storageManagerStore.list != nil
            }
            try await SnapshotTestSupport.settle(host)
            try await check(try Self.table(in: host))
        }
    }

    /// The layout that shows the columns `widths` names at those widths and
    /// hides the rest but Release, which always shows.
    private static func layout(
        showing widths: [StorageTableColumn: CGFloat]
    ) -> StorageTableLayout? {
        StorageTableLayout(
            holding: StorageTableColumn.allCases.map {
                StorageTableLayout.Column(
                    column: $0,
                    width: widths[$0] ?? $0.width,
                    hidden: widths[$0] == nil && $0 != .release
                )
            }
        )
    }

    /// The hosted tree's table.
    private static func table(in host: NSView) throws -> NSTableView {
        try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTableView }
                .first
        )
    }
}

/// The table over `store`'s list once its first rows land, laid out as
/// `layout` says.
private struct LayoutTable: View {
    let store: StorageManagerStore
    let layout: StorageTableLayout
    let runner: StorageActionRunner

    var body: some View {
        Group {
            if let list = store.list {
                MacTable(
                    list: list,
                    selection: .constant([]),
                    sort: .constant(
                        BridgeStorageSort(
                            field: .albumTitle,
                            direction: .ascending
                        )
                    ),
                    inspectorPresented: .constant(false),
                    layout: .constant(layout),
                    runner: runner
                )
            }
        }
        .task {
            store.update(
                filter: .all,
                sort: BridgeStorageSort(
                    field: .albumTitle,
                    direction: .ascending
                ),
                search: ""
            )
        }
    }
}
