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
        let source = MutableImportListPageSource(
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
            try await Wait.until { Self.topRow(of: table) == 35 }
            #expect(slot.list?.idAt(35) == Self.stableKey(30))
        }
    }

    @MainActor
    @Test("coming back to the tab finds the row that was on top")
    func remountFindsThePlace() async throws {
        let source = MutableImportListPageSource(
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
            try await Wait.until { Self.topRow(of: table) == 30 }
            #expect(slot.list?.idAt(30) == Self.stableKey(30))
        }
    }

    /// The list narrowed anew while the tab was away opens at its top, not
    /// at the row that was on top under the narrowing before.
    @MainActor
    @Test("coming back to a list narrowed anew opens it at its top")
    func remountUnderANewNarrowingOpensAtTheTop() async throws {
        let source = MutableImportListPageSource(
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

        store.applySummary(Self.summary(narrowedBy: "release"))
        try await ImportListHosting.withList(
            store: store,
            slot: slot,
            uiStore: uiStore,
            size: Self.size
        ) { table in
            try await Wait.until {
                slot.place?.narrowing.filterText == "release"
            }
            #expect(Self.topRow(of: table) == 0)
        }
    }

    @MainActor
    @Test("the rows in view hold their pages")
    func rowsInViewHoldTheirPages() async throws {
        let source = MutableImportListPageSource(
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
        let source = MutableImportListPageSource(
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
            try await Wait.until { slot.list?.idAt(150) != nil }
            #expect(slot.list?.idAt(150) == Self.stableKey(150))
            #expect(Self.topRow(of: table) == 0)
        }
    }

    // MARK: Where a delivery scrolls

    @Test("a delivery under the same narrowing keeps the place")
    func sameNarrowingKeepsThePlace() {
        var content = ImportCandidateTableContent(Self.read(1))
        #expect(content.take(Self.read(1), revealing: false) == nil)
        #expect(content.take(Self.read(2), revealing: false) == .keepPlace)
    }

    /// Narrowed anew — the text changed — the list opens at its top, where
    /// the first rows and their group header are, not at the row it showed
    /// before; with a selected row still in the result, at that row.
    @Test("a list narrowed anew opens at its first selected row, or its top")
    func narrowedAnewOpensAtItsTop() {
        var content = ImportCandidateTableContent(Self.read(1))
        #expect(
            content.take(
                Self.read(2, filterText: "title"),
                revealing: false
            ) == .showOnTop(0)
        )
        #expect(
            content.take(
                Self.read(3, pendingFilter: .needsYou, openedAt: 4),
                revealing: false
            ) == .showOnTop(4)
        )
    }

    /// A reveal scrolls to its own row; the deliveries it brings are taken
    /// without a scroll of their own, and do not scroll once it has landed.
    @Test("a reveal's deliveries scroll nothing")
    func revealOwnsItsScroll() {
        var content = ImportCandidateTableContent(Self.read(1))
        #expect(
            content.take(Self.read(2, tab: .done), revealing: true) == nil
        )
        #expect(content.take(Self.read(2, tab: .done), revealing: false) == nil)
        #expect(
            content.take(Self.read(3, tab: .done), revealing: false)
                == .keepPlace
        )
    }

    /// A narrowing that leaves the rows as they were moves nothing, and the
    /// next delivery is read against it.
    @Test("a narrowing with the same rows scrolls nothing")
    func narrowingWithTheSameRowsScrollsNothing() {
        var content = ImportCandidateTableContent(Self.read(1))
        #expect(
            content.take(Self.read(1, filterText: "t"), revealing: false)
                == nil
        )
        #expect(
            content.take(Self.read(2, filterText: "t"), revealing: false)
                == .keepPlace
        )
    }

}

// MARK: - Fixtures

extension ImportCandidateTablePlaceTests {
    private static let size = NSSize(width: 460, height: 600)

    /// A slot over `source` whose first page has landed.
    @MainActor
    private static func loadedSlot(
        _ source: MutableImportListPageSource,
        _ store: ImportStore,
        _ uiStore: UiStore
    ) async throws -> ImportListSlot {
        let slot = ImportListSlot(
            importStore: store,
            uiStore: uiStore,
            selection: ImportSelection(),
            makeSource: { _ in source.pages }
        )
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(30) != nil }
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

    /// The row under the table's top edge: every row is one height.
    @MainActor
    private static func topRow(of table: NSTableView) -> Int? {
        guard let clipView = table.enclosingScrollView?.contentView else {
            return nil
        }
        return Int(clipView.bounds.minY / ImportCandidateTableLayout.rowHeight)
    }

    private static func candidateKey(_ index: Int) -> String {
        "/library/release-\(index)"
    }

    private static func stableKey(_ index: Int) -> String {
        "candidate:\(candidateKey(index))"
    }

    private static func candidateItem(_ index: Int) -> BridgeImportListItem {
        PreviewData.candidateItem(
            BridgeTriageRow(
                candidateKey: candidateKey(index),
                folderName: "Release \(index)",
                watchedFolderPath: "/library",
                displayPath: "Release \(index)",
                actionable: true,
                placement: .skipped,
                live: BridgeCandidateLiveState(
                    identifying: nil,
                    readingPick: false,
                    import: nil,
                    actions: [],
                    standing: .notLookedUp,
                    badge: nil
                ),
                metadataSummary: nil,
                cover: nil,
                importStatus: nil,
                metadataProvenance: nil,
                reading: .unidentified,
                selected: false
            )
        )
    }

    /// A summary read under `filterText`, with nothing else in it.
    private static func summary(
        narrowedBy filterText: String
    ) -> BridgeImportQueueSummary {
        BridgeImportQueueSummary(
            counts: BridgeTriageTabCounts(pending: 80, done: 0, skipped: 0),
            watchedFolders: [],
            hasGroups: false,
            narrowed: nil,
            narrowing: BridgeImportListNarrowing(
                tab: .pending,
                filterText: filterText,
                pendingFilter: .all
            ),
            openedAt: 0
        )
    }

    /// A delivery at `revision`, narrowed as given.
    private static func read(
        _ revision: UInt64,
        tab: BridgeTriageTab = .pending,
        filterText: String = "",
        pendingFilter: BridgePendingFilter = .all,
        openedAt: Int = 0
    ) -> ImportCandidateListContentRead {
        ImportCandidateListContentRead(
            revision: revision,
            narrowing: BridgeImportListNarrowing(
                tab: tab,
                filterText: filterText,
                pendingFilter: pendingFilter
            ),
            openedAt: openedAt
        )
    }
}

/// An import list whose items a test replaces, delivering every live page
/// again as core's list does.
private final class MutableImportListPageSource: PageSource,
    @unchecked Sendable
{
    typealias Row = BridgeImportListItem

    private struct Sink {
        let offset: Int
        let limit: Int
        let value: @MainActor @Sendable ([Row], Int) -> Void
    }

    private struct Delivery {
        let sink: Sink
        let page: [Row]
        let total: Int
    }

    private let lock = NSLock()
    private var items: [Row]
    private var sinks: [UUID: Sink] = [:]

    init(items: [Row]) {
        self.items = items
    }

    var pages: ImportListPages {
        .stub(source: self)
    }

    func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([Row], Int) -> Void,
        onError _: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let id = UUID()
        let sink = Sink(offset: offset, limit: limit, value: onValue)
        let value = lock.withLock { () -> ([Row], Int) in
            sinks[id] = sink
            return page(for: sink)
        }
        Task { @MainActor in onValue(value.0, value.1) }
        return MutableImportListPageSubscription { [weak self] in
            _ = self?.lock
                .withLock {
                    self?.sinks.removeValue(forKey: id)
                }
        }
    }

    func replaceItems(_ items: [Row]) async {
        let deliveries = lock.withLock { () -> [Delivery] in
            self.items = items
            return sinks.values.map { sink in
                let value = page(for: sink)
                return Delivery(
                    sink: sink,
                    page: value.0,
                    total: value.1
                )
            }
        }
        for delivery in deliveries {
            await delivery.sink.value(delivery.page, delivery.total)
        }
    }

    private func page(for sink: Sink) -> ([Row], Int) {
        let start = min(sink.offset, items.count)
        let end = min(start + sink.limit, items.count)
        return (Array(items[start..<end]), items.count)
    }
}

private final class MutableImportListPageSubscription: PageSubscription,
    @unchecked Sendable
{
    private let cancelAction: @Sendable () -> Void

    init(cancel: @escaping @Sendable () -> Void) {
        cancelAction = cancel
    }

    func cancel() {
        cancelAction()
    }
}
