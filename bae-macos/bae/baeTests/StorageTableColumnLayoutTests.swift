import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct StorageTableColumnLayoutTests {
    /// The person hid the artist, media and date added and narrowed the
    /// rest: each column that shows keeps its width, the release column
    /// too, which the table would otherwise widen by what the others give
    /// up.
    @Test("A narrowed layout keeps its widths")
    func aNarrowedLayoutKeepsItsWidths() async throws {
        let widths: [StorageTableColumn: CGFloat] = [
            .release: 200, .storage: 240, .files: 44, .size: 76,
        ]
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
        let size = NSSize(width: 560, height: 200)
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
            let table = try Self.table(in: host)
            let shown = table.tableColumns.filter { !$0.isHidden }
            #expect(
                shown.map(\.identifier.rawValue)
                    == ["release", "storage", "files", "size"]
            )
            for column in shown {
                let id = try #require(
                    StorageTableColumn(rawValue: column.identifier.rawValue)
                )
                #expect(column.width == widths[id])
            }
        }
    }

    /// The layout that shows the columns `widths` names at those widths and
    /// hides the rest.
    private static func layout(
        showing widths: [StorageTableColumn: CGFloat]
    ) -> StorageTableLayout? {
        StorageTableLayout(
            holding: StorageTableColumn.allCases.map {
                StorageTableLayout.Column(
                    column: $0,
                    width: widths[$0] ?? $0.width,
                    hidden: widths[$0] == nil
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
