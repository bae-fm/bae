import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The person's place in the import list's table: the row on top stays on
/// top as deliveries move it, the list narrowed anew opens at its first
/// selected row or its top, coming back to the tab finds the place again,
/// and the rows in view hold their pages.
@Suite("Import list place")
struct ImportCandidateTablePlaceTests {
    // MARK: The table

    @MainActor
    @Test("the row on top stays on top when rows land above it")
    func deliveryKeepsTheRowOnTop() async throws {
        let source = MutableImportList(
            items: (0..<80).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            Self.scroll(table, toRow: 30)
            try await Wait.until { slot.place?.position == 30 }

            // Five folders land above the one on top.
            await source.replaceItems(
                (100..<105).map(Self.candidateItem)
                    + (0..<80).map(Self.candidateItem)
            )
            try await Wait.until(notices: [.scroll(of: table)]) {
                Self.topRow(of: table) == 35
            }
            #expect(slot.list?.idAt(35) == Self.stableKey(30))
        }
    }

    @MainActor
    @Test("coming back to the tab finds the row that was on top")
    func remountFindsThePlace() async throws {
        let source = MutableImportList(
            items: (0..<80).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            Self.scroll(table, toRow: 30)
            try await Wait.until { slot.place?.position == 30 }
        }

        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            try await Wait.until(notices: [.scroll(of: table)]) {
                Self.topRow(of: table) == 30
            }
            #expect(slot.list?.idAt(30) == Self.stableKey(30))
        }
    }

    /// The list narrowed anew while the tab was away opens at its top, not
    /// at the row that was on top under the narrowing before.
    @MainActor
    @Test("coming back to a list narrowed anew opens it at its top")
    func remountUnderANewNarrowingOpensAtTheTop() async throws {
        let source = MutableImportList(
            items: (0..<80).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            Self.scroll(table, toRow: 30)
            try await Wait.until { slot.place?.position == 30 }
        }

        slot.setFilterText("release")
        try await Wait.until {
            slot.answersView && slot.place?.position == 0
        }
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            try await Wait.until(notices: [.scroll(of: table)]) {
                Self.topRow(of: table) == 0
            }
        }
    }

    /// When the person narrows the list again while the table shows it,
    /// the table scrolls to the top, wherever the person had scrolled.
    @MainActor
    @Test(
        "the table scrolls to the top when the person narrows the list it shows"
    )
    func anOpeningPutsItsRowOnTop() async throws {
        let source = MutableImportList(
            items: (0..<80).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            Self.scroll(table, toRow: 30)
            try await Wait.until { slot.place?.position == 30 }
            let content = try #require(table.window?.contentView)

            slot.setFilterText("release")

            // Until core sends the narrowed list's count, the sidebar shows
            // in place of the table, so the wait looks for the table without
            // failing the test while the table is absent.
            try await Wait.until {
                slot.answersView
                    && SnapshotTestSupport.descendants(of: content)
                        .compactMap { $0 as? ImportCandidateTableView }
                        .first
                        .map { Self.topRow(of: $0) == 0 } ?? false
            }
        }
    }

    @MainActor
    @Test("the rows in view hold their pages")
    func rowsInViewHoldTheirPages() async throws {
        let source = MutableImportList(
            items: (0..<200).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        #expect(slot.list?.idAt(120) == nil)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            Self.scroll(table, toRow: 120)
            try await Wait.until { slot.list?.idAt(120) != nil }
            #expect(slot.list?.idAt(120) == Self.stableKey(120))
        }
    }

    /// Accessibility reads rows off screen, and the table draws them for it:
    /// a row drawn before its page lands asks for the page, so what it reads
    /// is the row and not a blank.
    @MainActor
    @Test("a row drawn off screen loads its page")
    func rowDrawnOffScreenLoadsItsPage() async throws {
        let source = MutableImportList(
            items: (0..<200).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            _ = table.view(atColumn: 0, row: 150, makeIfNecessary: true)
            // The list releases the page after the source delivers it, so the
            // test waits for the source to deliver the window that holds row
            // 150.
            try await Wait.until { source.hasDelivered(Self.stableKey(150)) }
            #expect(Self.topRow(of: table) == 0)
        }
    }

    // MARK: What a delivery redraws

    @MainActor
    @Test("a row added below the view draws no cell")
    func aRowBelowTheViewDrawsNoCell() async throws {
        let source = MutableImportList(
            items: (0..<200).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            let coordinator = try Self.coordinator(of: table)
            let drawn = coordinator.cellsShown

            await source.replaceItems(
                (0..<200).map(Self.candidateItem) + [Self.candidateItem(200)]
            )
            try await Wait.untilLaidOut(table) { table.numberOfRows == 201 }
            #expect(coordinator.cellsShown == drawn)
        }
    }

    @MainActor
    @Test("a value changing one row draws that row's cell only")
    func aChangedRowDrawsItsCellOnly() async throws {
        let source = MutableImportList(
            items: (0..<200).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            let coordinator = try Self.coordinator(of: table)
            let drawn = coordinator.cellsShown
            var items = (0..<200).map(Self.candidateItem)
            items[2] = Self.candidateItem(2, folderName: "Renamed")

            await source.replaceItems(items)
            try await Wait.untilLaidOut(table) {
                coordinator.cellsShown > drawn
            }
            // Let any further cell the same delivery asks for be drawn.
            try await Wait.until { slot.list?.row(at: 2) == items[2] }
            #expect(coordinator.cellsShown == drawn + 1)
        }
    }
}

extension ImportCandidateTablePlaceTests {
    // MARK: Rows of two heights

    /// A group header is a shorter row than a release's. The row on top is
    /// found by position, a header on top pins itself, and the header of the
    /// group whose last row is on top is still pinned, lifted with that row.
    @MainActor
    @Test("a header is pinned over its group until the group leaves")
    func headerPinnedWhileItsGroupLeaves() async throws {
        let source = MutableImportList(
            items: Self.groupedItems(headersAt: [0, 20])
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            let coordinator = try Self.coordinator(of: table)
            let clipView = try #require(table.enclosingScrollView?.contentView)
            try await Wait.untilLaidOut(table) {
                table.rect(ofRow: 0).height
                    == ImportCandidateTableLayout.headerRowHeight
                    && table.rect(ofRow: 1).height
                        == ImportCandidateTableLayout.rowHeight
            }

            // The second header is on top: found by position, not by
            // dividing the offset by one height.
            Self.scroll(table, toRow: 20)
            try await Wait.until { slot.place?.position == 20 }
            #expect(Self.topRow(of: table) == 20)
            #expect(coordinator.pinnedHeader()?.header == Self.header(20))
            #expect(!coordinator.pinnedStrip.isHidden)
            #expect(
                coordinator.pinnedStrip.frame.height
                    == ImportCandidateTableLayout.headerRowHeight
            )

            // The first group's last row is part way off the top: its header
            // is still the one pinned, and the group ends with the row.
            clipView.scroll(
                to: NSPoint(x: 0, y: table.rect(ofRow: 19).minY + 20)
            )
            table.enclosingScrollView?.reflectScrolledClipView(clipView)
            #expect(Self.topRow(of: table) == 19)
            let pinned = try #require(coordinator.pinnedHeader())
            #expect(pinned.header == Self.header(0))
            #expect(pinned.ends(below: 19) { coordinator.item(at: $0) })
            #expect(!coordinator.pinnedStrip.isHidden)
        }
    }

    /// A page landing above the rows in view can turn a release's row into a
    /// header: the rows in view stay where they are on screen.
    @MainActor
    @Test("a row above the view changing height moves nothing in view")
    func heightChangeAboveTheViewKeepsTheViewStill() async throws {
        let source = MutableImportList(
            items: (0..<80).map(Self.candidateItem)
        )
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = try await Self.loadedSlot(source, store, uiStore)
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            let clipView = try #require(table.enclosingScrollView?.contentView)
            Self.scroll(table, toRow: 30)
            try await Wait.until { slot.place?.position == 30 }
            let key = slot.list?.idAt(30)
            let offset = table.rect(ofRow: 30).minY - clipView.bounds.minY

            var items = (0..<80).map(Self.candidateItem)
            items[5] = Self.headerItem(5)
            await source.replaceItems(items)
            try await Wait.untilLaidOut(table) {
                table.rect(ofRow: 5).height
                    == ImportCandidateTableLayout.headerRowHeight
            }

            #expect(slot.list?.idAt(30) == key)
            #expect(Self.topRow(of: table) == 30)
            let moved = table.rect(ofRow: 30).minY - clipView.bounds.minY
            #expect(abs(moved - offset) < 0.5)
        }
    }
}

// MARK: - Fixtures

extension ImportCandidateTablePlaceTests {
    private static let size = NSSize(width: 460, height: 600)

    /// A slot over `source` whose source has delivered its first window.
    @MainActor
    private static func loadedSlot(
        _ source: MutableImportList,
        _ store: ImportStore,
        _ uiStore: UiStore
    ) async throws -> ImportListSlot {
        let slot = ImportListSlot(
            importStore: store,
            uiStore: uiStore,
            selection: ImportSelection(),
            makeHandle: { _ in source.handle }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(0) != nil }
        return slot
    }

    /// Put `row` at the top of the table, as a person's scroll does.
    @MainActor
    private static func scroll(_ table: NSTableView, toRow row: Int) {
        guard let scrollView = table.enclosingScrollView else {
            Issue.record("the table is not in a scroll view")
            return
        }
        scrollView.contentView.scroll(to: table.rect(ofRow: row).origin)
        scrollView.reflectScrolledClipView(scrollView.contentView)
    }

    /// The row under the table's top edge.
    @MainActor
    private static func topRow(of table: NSTableView) -> Int? {
        guard let clipView = table.enclosingScrollView?.contentView else {
            return nil
        }
        return table.row(at: NSPoint(x: 0, y: clipView.bounds.minY))
    }

    private static func folderPath(_ index: Int) -> String {
        "/library/release-\(index)"
    }

    private static func stableKey(_ index: Int) -> String {
        candidateItem(index).id
    }

    @MainActor
    private static func coordinator(
        of table: NSTableView
    ) throws -> ImportCandidateTableCoordinator {
        try #require(table.delegate as? ImportCandidateTableCoordinator)
    }

    private static func candidateItem(_ index: Int) -> BridgeImportListItem {
        candidateItem(index, folderName: "Release \(index)")
    }

    /// The header row of the group at `index`.
    private static func headerItem(_ index: Int) -> BridgeImportListItem {
        .groupHeader(header: header(index))
    }

    /// The header of the group at `index`.
    private static func header(_ index: Int) -> BridgeImportGroupHeader {
        PreviewData.groupHeader(
            key: BridgeFolderReleaseDecisionKey(
                watchedFolderPath: Data("/library".utf8),
                relativeFolder: Data("Group \(index)".utf8)
            ),
            name: "Group \(index)",
            entryCount: 19
        )
    }

    /// Ninety rows with a group header at each of `headersAt`, each row after
    /// one listed under the last.
    private static func groupedItems(
        headersAt: [Int]
    ) -> [BridgeImportListItem] {
        (0..<90)
            .map { index in
                if headersAt.contains(index) {
                    return headerItem(index)
                }
                let header = headersAt.last { $0 < index }
                return candidateItem(
                    index,
                    folderName: "Release \(index)",
                    header: header.map { Self.header($0) }
                )
            }
    }

    private static func candidateItem(
        _ index: Int,
        folderName: String,
        header: BridgeImportGroupHeader? = nil
    ) -> BridgeImportListItem {
        PreviewData.candidateItem(
            BridgeTriageRow(
                candidateKey: Data(folderPath(index).utf8),
                folderName: folderName,
                watchedFolderPath: Data("/library".utf8),
                actionable: true,
                placement: .skipped,
                live: BridgeCandidateLiveState(
                    identifying: nil,
                    import: nil,
                    actions: [],
                    standing: .notLookedUp,
                    badge: nil
                ),
                metadataSummary: nil,
                cover: nil,
                importStatus: nil,
                upload: nil,
                metadataProvenance: nil,
                reading: .unidentified
            ),
            header: header
        )
    }
}
