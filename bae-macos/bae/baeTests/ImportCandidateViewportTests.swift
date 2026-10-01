import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import XCTest

@testable import bae

// MARK: - Import candidate viewport integration

/// The rows the list draws selected, standing in for core's selection.
@Observable
private final class SelectedRows {
    var keys: Set<String> = []
}

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
        ImportListPages(
            source: self,
            setView: { _ in },
            waitForView: { _ in },
            pendingFilterEntries: { [] }
        )
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

@MainActor
final class ImportCandidateViewportTests: XCTestCase {
    private final class GeometryObservation {
        var value = ImportCandidateListGeometry()
    }

    func testLivePageDeliveryKeepsTheVisibleCandidateAnchored() async throws {
        let initial = (0..<80).map(candidateItem)
        let source = MutableImportListPageSource(items: initial)
        let store = ImportStore()
        let uiStore = UiStore()
        let slot = Self.slot(source: source, store: store, uiStore: uiStore)
        slot.startLoad()
        try await Wait.until { slot.list?.idAt(30) != nil }
        let geometry = GeometryObservation()
        let selected = SelectedRows()
        let root = candidateList(
            store: store,
            uiStore: uiStore,
            slot: slot,
            selected: selected
        )
        .onPreferenceChange(ImportCandidateListGeometryKey.self) {
            geometry.value = $0
        }
        try await SnapshotTestSupport.withHostedWindow(
            root,
            size: NSSize(width: 460, height: 600)
        ) { _, hosting in
            try await settleFirstLayout(geometry, slot)

            let table = try XCTUnwrap(
                descendants(of: hosting).compactMap { $0 as? NSTableView }.first
            )
            let scrollView = try XCTUnwrap(table.enclosingScrollView)
            let anchorIndex = 30
            let unscrolled = geometry.value
            scrollView.contentView.scroll(
                to: table.rect(ofRow: anchorIndex).origin
            )
            scrollView.reflectScrolledClipView(scrollView.contentView)
            try await settleViewportLayout(
                geometry,
                slot,
                changedFrom: unscrolled
            )
            let anchorKey = "candidate:\(viewportCandidateKey(anchorIndex))"
            let anchor = try XCTUnwrap(
                geometry.value.rows.first { $0.stableKey == anchorKey }
            )
            let viewport = try XCTUnwrap(geometry.value.viewport)
            XCTAssertEqual(anchor.bounds.minY, viewport.minY, accuracy: 1)

            selected.keys = [viewportCandidateKey(35)]
            let changed = (0..<80)
                .map { index in
                    index < 20 ? groupHeaderItem(index) : candidateItem(index)
                }
            await source.replaceItems(changed)
            try await settleViewportLayout(geometry, slot)

            let retained = try XCTUnwrap(
                geometry.value.rows.first { $0.stableKey == anchorKey }
            )
            XCTAssertEqual(
                retained.bounds.minY,
                anchor.bounds.minY,
                accuracy: 1
            )
        }
    }

    func testRowsBehindTheHeaderCannotBecomeTheRetainedAnchor() {
        let state = ImportCandidateListViewport()
        let rows = (28...31)
            .map { index in
                viewportRow(
                    viewportCandidateKey(index),
                    y: CGFloat((index - 28) * 62 - 40)
                )
            }
        XCTAssertNil(
            state.update(
                rows: rows,
                viewport: viewportBounds,
                content: read(1),
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertEqual(
            state.update(
                rows: rows,
                viewport: viewportBounds,
                content: read(2),
                revealInProgress: false,
                positionOf: { $0 == self.viewportCandidateKey(30) ? 30 : nil }
            ),
            30
        )
    }

    /// A page landing before a restore's scroll runs puts the anchor back at
    /// the top, not the row the unscrolled list shows.
    func testALayoutBeforeTheRestoreScrollCannotReplaceTheAnchor() {
        let state = ImportCandidateListViewport()
        let anchorKey = viewportCandidateKey(30)
        XCTAssertNil(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                content: read(1),
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertEqual(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                content: read(2),
                revealInProgress: false,
                positionOf: { $0 == anchorKey ? 30 : nil }
            ),
            30
        )

        XCTAssertNil(
            state.update(
                rows: [viewportRow(viewportCandidateKey(45), y: 84)],
                viewport: viewportBounds,
                content: read(2),
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )

        XCTAssertEqual(
            state.update(
                rows: [viewportRow(viewportCandidateKey(45), y: 84)],
                viewport: viewportBounds,
                content: read(3),
                revealInProgress: false,
                positionOf: { $0 == anchorKey ? 30 : 45 }
            ),
            30
        )
    }

    /// Once the restore's scroll has run, the row at the top is the anchor.
    func testTheAnchorFollowsTheListOnceTheRestoreScrollHasRun() {
        let state = ImportCandidateListViewport()
        let anchorKey = viewportCandidateKey(30)
        let scrolledKey = viewportCandidateKey(45)
        XCTAssertNil(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                content: read(1),
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertEqual(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                content: read(2),
                revealInProgress: false,
                positionOf: { $0 == anchorKey ? 30 : nil }
            ),
            30
        )

        state.restoreScrolled()
        XCTAssertNil(
            state.update(
                rows: [viewportRow(scrolledKey, y: 84)],
                viewport: viewportBounds,
                content: read(2),
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )

        XCTAssertEqual(
            state.update(
                rows: [viewportRow(scrolledKey, y: 84)],
                viewport: viewportBounds,
                content: read(3),
                revealInProgress: false,
                positionOf: { $0 == scrolledKey ? 45 : 30 }
            ),
            45
        )
    }

    func testExplicitRevealOwnsItsScrollThenEstablishesTheRetainedAnchor() {
        let targetIndex = 61
        let targetKey = viewportCandidateKey(targetIndex)
        let viewport = ImportCandidateListViewport()
        XCTAssertNil(
            viewport.update(
                rows: [
                    ImportCandidateListRowBounds(
                        stableKey: viewportCandidateKey(30),
                        bounds: CGRect(x: 0, y: 0, width: 400, height: 58)
                    )
                ],
                viewport: CGRect(x: 0, y: 0, width: 400, height: 600),
                content: read(1),
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertNil(
            viewport.update(
                rows: [
                    ImportCandidateListRowBounds(
                        stableKey: viewportCandidateKey(42),
                        bounds: CGRect(x: 0, y: 0, width: 400, height: 58)
                    )
                ],
                viewport: CGRect(x: 0, y: 0, width: 400, height: 600),
                content: read(2),
                revealInProgress: true,
                positionOf: { _ in nil }
            )
        )
        XCTAssertNil(
            viewport.update(
                rows: [
                    ImportCandidateListRowBounds(
                        stableKey: targetKey,
                        bounds: CGRect(x: 0, y: 0, width: 400, height: 58)
                    )
                ],
                viewport: CGRect(x: 0, y: 0, width: 400, height: 600),
                content: read(2),
                revealInProgress: true,
                positionOf: { _ in nil }
            )
        )

        XCTAssertEqual(
            viewport.update(
                rows: [
                    ImportCandidateListRowBounds(
                        stableKey: viewportCandidateKey(49),
                        bounds: CGRect(x: 0, y: 0, width: 400, height: 58)
                    )
                ],
                viewport: CGRect(x: 0, y: 0, width: 400, height: 600),
                content: read(3),
                revealInProgress: false,
                positionOf: { $0 == targetKey ? targetIndex : nil }
            ),
            targetIndex
        )
    }
}

// MARK: - A list narrowed anew

extension ImportCandidateViewportTests {
    /// Narrowed anew — the text changed — the list opens at its top, where
    /// the first rows and their group header are, not at the row it showed
    /// before.
    func testANarrowedListOpensAtItsTop() {
        let state = ImportCandidateListViewport()
        let anchorKey = viewportCandidateKey(30)
        let typed = BridgeImportListNarrowing(
            tab: .pending,
            filterText: "melv",
            pendingFilter: .all
        )
        XCTAssertNil(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                content: read(1),
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertEqual(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                content: ImportCandidateListContentRead(
                    revision: 2,
                    narrowing: typed,
                    firstSelected: nil
                ),
                revealInProgress: false,
                positionOf: { $0 == anchorKey ? 30 : nil }
            ),
            0
        )
    }

    /// Narrowed anew with a selected row still in the result, the list shows
    /// that row.
    func testANarrowedListKeepsTheSelectedRowInView() {
        let state = ImportCandidateListViewport()
        XCTAssertNil(
            state.update(
                rows: [viewportRow(viewportCandidateKey(30), y: 84)],
                viewport: viewportBounds,
                content: ImportCandidateListContentRead(
                    revision: 1,
                    narrowing: viewportNarrowing,
                    firstSelected: 30
                ),
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertEqual(
            state.update(
                rows: [viewportRow(viewportCandidateKey(30), y: 84)],
                viewport: viewportBounds,
                content: ImportCandidateListContentRead(
                    revision: 2,
                    narrowing: BridgeImportListNarrowing(
                        tab: .pending,
                        filterText: "",
                        pendingFilter: .needsYou
                    ),
                    firstSelected: 4
                ),
                revealInProgress: false,
                positionOf: { _ in 30 }
            ),
            4
        )
    }
}

// MARK: - Fixtures and layout helpers

extension ImportCandidateViewportTests {
    @MainActor
    private static func slot(
        source: MutableImportListPageSource,
        store: ImportStore,
        uiStore: UiStore
    ) -> ImportListSlot {
        ImportListSlot(
            importStore: store,
            uiStore: uiStore,
            selection: ImportSelection(),
            makeSource: { _ in source.pages },
            locateCandidate: { _, _ in nil },
            firstIdentifyingCandidate: { _ in nil }
        )
    }

    private func candidateList(
        store: ImportStore,
        uiStore: UiStore,
        slot: ImportListSlot,
        selected: SelectedRows
    ) -> some View {
        ImportCandidateListContent(
            importStore: store,
            listSlot: slot,
            selectedKeys: Binding(
                get: { selected.keys },
                set: { selected.keys = $0 }
            ),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onCombineFolder: { _ in },
            onSeparate: { _ in },
            onReveal: { _ in },
            onPerform: { _ in },
            onCancelAllIdentification: {},
            onCancelAllImports: {}
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(uiStore)
        .environment(ImportSelection())
        .environment(PreviewData.artImageStore())
        .frame(width: 460, height: 600)
    }

    private func candidateItem(_ index: Int) -> BridgeImportListItem {
        PreviewData.candidateItem(
            BridgeTriageRow(
                candidateKey: viewportCandidateKey(index),
                folderName: "Release \(index)",
                watchedFolderPath: "/library",
                displayPath: "Release \(index)",
                actionable: true,
                placement: .skipped,
                live: BridgeCandidateLiveState(
                    identification: nil,
                    import: nil,
                    actions: [],
                    standing: .notLookedUp,
                    badge: nil
                ),
                matched: nil,
                metadataSummary: nil,
                cover: nil,
                importStatus: nil,
                metadataProvenance: nil,
                reading: .unidentified,
                selected: false
            )
        )
    }

    private func groupHeaderItem(_ index: Int) -> BridgeImportListItem {
        PreviewData.groupHeaderItem(
            key: BridgeFolderReleaseDecisionKey(
                watchedFolderPath: "/library",
                relativeFolderPath: "Group \(index)"
            ),
            name: "Group \(index)",
            entryCount: 1
        )
    }

    private func viewportCandidateKey(_ index: Int) -> String {
        "/library/release-\(index)"
    }

    /// A delivery at `revision` under the narrowing a test keeps the same,
    /// with no selected row.
    private func read(_ revision: UInt64) -> ImportCandidateListContentRead {
        ImportCandidateListContentRead(
            revision: revision,
            narrowing: viewportNarrowing,
            firstSelected: nil
        )
    }

    /// What the list is narrowed by while a test keeps it narrowed the same.
    private var viewportNarrowing: BridgeImportListNarrowing {
        BridgeImportListNarrowing(
            tab: .pending,
            filterText: "",
            pendingFilter: .all
        )
    }

    /// The list's frame below the sidebar's 84-point header.
    private var viewportBounds: CGRect {
        CGRect(x: 0, y: 84, width: 460, height: 516)
    }

    private func viewportRow(
        _ stableKey: String,
        y: CGFloat
    ) -> ImportCandidateListRowBounds {
        ImportCandidateListRowBounds(
            stableKey: stableKey,
            bounds: CGRect(x: 0, y: y, width: 460, height: 62)
        )
    }

    private func descendants(of view: NSView) -> [NSView] {
        [view] + view.subviews.flatMap { descendants(of: $0) }
    }

    /// Lay the hosted list out until it reports rows and comes to rest.
    private func settleFirstLayout(
        _ geometry: GeometryObservation,
        _ slot: ImportListSlot
    ) async throws {
        try await Wait.until {
            layOutOnce()
            return !geometry.value.rows.isEmpty
        }
        try await settleViewportLayout(geometry, slot)
    }

    /// The rows the list laid out, and the page delivery they came from.
    private struct ViewportState: Equatable {
        let geometry: ImportCandidateListGeometry
        let contentRevision: UInt64?
    }

    /// Run the list until its rows and content stop moving and loading; with
    /// `before`, until it has also moved off it.
    private func settleViewportLayout(
        _ geometry: GeometryObservation,
        _ slot: ImportListSlot,
        changedFrom before: ImportCandidateListGeometry? = nil,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        try await Wait.untilSteady(
            changedFrom: before.map {
                ViewportState(
                    geometry: $0,
                    contentRevision: slot.list?.contentRevision
                )
            },
            file: file,
            line: line
        ) {
            layOutOnce()
            return ViewportState(
                geometry: geometry.value,
                contentRevision: slot.list?.contentRevision
            )
        }
    }

    /// One run-loop turn, from a synchronous context.
    private func layOutOnce() {
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.005))
    }

}
