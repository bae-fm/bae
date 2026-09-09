import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import XCTest

@testable import bae

// MARK: - Import candidate viewport integration

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
            firstUnidentifiedPosition: { [self] _, target in
                lock.withLock {
                    items.firstIndex {
                        $0.id == target.stableKey
                    }
                }
            },
            waitForView: { _ in }
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
        let slot = ImportListSlot(
            importStore: store,
            uiStore: uiStore,
            makeSource: { _ in source.pages },
            locateCandidate: { _, _ in nil }
        )
        slot.startLoad()
        await viewportSettle { slot.list?.idAt(30) != nil }
        let geometry = GeometryObservation()
        let root = candidateList(store: store, uiStore: uiStore, slot: slot)
            .onPreferenceChange(ImportCandidateListGeometryKey.self) {
                geometry.value = $0
            }
        let hosting = NSHostingView(rootView: root)
        let window = makeWindow(hosting: hosting)
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await settleViewportLayout(geometry)

        let table = try XCTUnwrap(
            descendants(of: hosting).compactMap { $0 as? NSTableView }.first
        )
        let scrollView = try XCTUnwrap(table.enclosingScrollView)
        let anchorIndex = 30
        scrollView.contentView.scroll(
            to: table.rect(ofRow: anchorIndex).origin
        )
        scrollView.reflectScrolledClipView(scrollView.contentView)
        await settleViewportLayout(geometry)
        let anchorKey = "candidate:\(viewportCandidateKey(anchorIndex))"
        let anchor = try XCTUnwrap(
            geometry.value.rows.first { $0.stableKey == anchorKey }
        )
        let viewport = try XCTUnwrap(geometry.value.viewport)
        XCTAssertEqual(anchor.bounds.minY, viewport.minY, accuracy: 1)

        uiStore.setFolderCandidateSelection([viewportCandidateKey(35)])
        let changed = (0..<80)
            .map { index in
                index < 20 ? groupHeaderItem(index) : candidateItem(index)
            }
        await source.replaceItems(changed)
        await settleViewportLayout(geometry)

        let retained = try XCTUnwrap(
            geometry.value.rows.first { $0.stableKey == anchorKey }
        )
        XCTAssertEqual(retained.bounds.minY, anchor.bounds.minY, accuracy: 1)
    }

    func testRowsBehindTheHeaderCannotBecomeTheRetainedAnchor() {
        var state = ImportCandidateListViewport()
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
                contentRevision: 1,
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertEqual(
            state.update(
                rows: rows,
                viewport: viewportBounds,
                contentRevision: 2,
                revealInProgress: false,
                positionOf: { $0 == self.viewportCandidateKey(30) ? 30 : nil }
            ),
            30
        )
    }

    /// A restore asks the list to scroll, and every layout until that scroll
    /// runs still measures the list where the person left it — with the rows
    /// above the anchor already resized, so a different row sits at the top.
    /// That row is nobody's choice: a page landing in the meantime has to put
    /// the anchor back, not the row the unscrolled list happened to show.
    func testALayoutBeforeTheRestoreScrollCannotReplaceTheAnchor() {
        var state = ImportCandidateListViewport()
        let anchorKey = viewportCandidateKey(30)
        XCTAssertNil(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                contentRevision: 1,
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertEqual(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                contentRevision: 2,
                revealInProgress: false,
                positionOf: { $0 == anchorKey ? 30 : nil }
            ),
            30
        )

        XCTAssertNil(
            state.update(
                rows: [viewportRow(viewportCandidateKey(45), y: 84)],
                viewport: viewportBounds,
                contentRevision: 2,
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )

        XCTAssertEqual(
            state.update(
                rows: [viewportRow(viewportCandidateKey(45), y: 84)],
                viewport: viewportBounds,
                contentRevision: 3,
                revealInProgress: false,
                positionOf: { $0 == anchorKey ? 30 : 45 }
            ),
            30
        )
    }

    /// Once the restore's scroll has run, the list reports where it really
    /// left its rows, and the row at the top is the anchor again — otherwise
    /// the next page would drag the person back to the row they scrolled off.
    func testTheAnchorFollowsTheListOnceTheRestoreScrollHasRun() {
        var state = ImportCandidateListViewport()
        let anchorKey = viewportCandidateKey(30)
        let scrolledKey = viewportCandidateKey(45)
        XCTAssertNil(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                contentRevision: 1,
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )
        XCTAssertEqual(
            state.update(
                rows: [viewportRow(anchorKey, y: 84)],
                viewport: viewportBounds,
                contentRevision: 2,
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
                contentRevision: 2,
                revealInProgress: false,
                positionOf: { _ in nil }
            )
        )

        XCTAssertEqual(
            state.update(
                rows: [viewportRow(scrolledKey, y: 84)],
                viewport: viewportBounds,
                contentRevision: 3,
                revealInProgress: false,
                positionOf: { $0 == scrolledKey ? 45 : 30 }
            ),
            45
        )
    }

    func testExplicitRevealOwnsItsScrollThenEstablishesTheRetainedAnchor() {
        let targetIndex = 61
        let targetKey = viewportCandidateKey(targetIndex)
        var viewport = ImportCandidateListViewport()
        XCTAssertNil(
            viewport.update(
                rows: [
                    ImportCandidateListRowBounds(
                        stableKey: viewportCandidateKey(30),
                        bounds: CGRect(x: 0, y: 0, width: 400, height: 58)
                    )
                ],
                viewport: CGRect(x: 0, y: 0, width: 400, height: 600),
                contentRevision: 1,
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
                contentRevision: 2,
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
                contentRevision: 2,
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
                contentRevision: 3,
                revealInProgress: false,
                positionOf: { $0 == targetKey ? targetIndex : nil }
            ),
            targetIndex
        )
    }

}

// MARK: - Fixtures and layout helpers

extension ImportCandidateViewportTests {
    private func candidateList(
        store: ImportStore,
        uiStore: UiStore,
        slot: ImportListSlot
    ) -> some View {
        ImportCandidateListContent(
            importStore: store,
            listSlot: slot,
            selectedKeys: Binding(
                get: { uiStore.selectedFolderCandidates },
                set: { uiStore.setFolderCandidateSelection($0) }
            ),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onReleaseDecision: { _, _ in },
            onSkip: { _, _ in },
            onReveal: { _ in },
            onImportSelected: {}
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(uiStore)
        .environment(PreviewData.artImageStore())
        .frame(width: 460, height: 600)
    }

    private func makeWindow<Content: View>(
        hosting: NSHostingView<Content>
    ) -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(
                x: -10_000,
                y: -10_000,
                width: 460,
                height: 600
            ),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.contentView = hosting
        window.orderBack(nil)
        return window
    }

    private func candidateItem(_ index: Int) -> BridgeImportListItem {
        PreviewData.candidateItem(
            BridgeTriageRow(
                candidateKey: viewportCandidateKey(index),
                folderName: "Release \(index)",
                watchedFolderPath: "/library",
                displayPath: "Release \(index)",
                resolvedBoundaries: [],
                combineAncestorKey: nil,
                actionable: true,
                placement: .skipped,
                skipAction: .unskip,
                actions: [.restore],
                matched: nil,
                metadataSummary: nil,
                coverThumbnail: nil,
                selectable: false,
                importStatus: nil,
                metadataProvenance: nil
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

    /// The list's own frame in the tests that drive the viewport state
    /// directly: the sidebar's header takes the first 84 points.
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

    /// Run the list until it stops moving: SwiftUI lays out on the run loop
    /// and the scroll a restore asks for runs as main-actor work between those
    /// passes, so each needs its turn. Settled means the list moved and then
    /// held still — a fixed number of turns would assert on a list still on
    /// its way.
    private func settleViewportLayout(
        _ geometry: GeometryObservation
    ) async {
        var last = geometry.value
        var moved = false
        var held = 0
        for _ in 0..<400 {
            layOutOnce()
            await Task.yield()
            if geometry.value != last {
                last = geometry.value
                moved = true
                held = 0
                continue
            }
            guard moved else { continue }
            held += 1
            if held == 5 { return }
        }
    }

    /// One run-loop turn, from a synchronous context: SwiftUI lays out there,
    /// and `RunLoop.run(until:)` is spelled out of reach of an async one.
    private func layOutOnce() {
        RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.005))
    }

    private func viewportSettle(
        _ predicate: @MainActor () -> Bool
    ) async {
        for _ in 0..<500 {
            if predicate() { return }
            await Task.yield()
        }
    }
}
