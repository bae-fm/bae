import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import XCTest

@testable import bae

// MARK: - PaginatedList tests

@Suite("PaginatedList")
struct PaginatedListTests {

    @MainActor
    @Test("loadInitial delivers the first live page and total count")
    func loadInitialAllocates() async {
        let store = LibraryStore()
        let list = makeList(
            store: store,
            albums: [
                makeBridgeAlbum(id: "a1"),
                makeBridgeAlbum(id: "a2"),
                makeBridgeAlbum(id: "a3"),
            ]
        )

        await list.loadInitial()

        #expect(list.totalCount == 3)
        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(2) == "a3")
    }

    @MainActor
    @Test("loadRange populates ids at correct positions and interns via ingest")
    func loadRangePopulatesPositions() async {
        let store = LibraryStore()
        let list = makeList(
            store: store,
            albums: [
                makeBridgeAlbum(id: "a1", title: "Alpha"),
                makeBridgeAlbum(id: "a2", title: "Beta"),
                makeBridgeAlbum(id: "a3", title: "Gamma"),
            ]
        )
        await list.loadInitial()

        await list.loadRange(offset: 0, limit: 3)

        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(1) == "a2")
        #expect(list.idAt(2) == "a3")
        #expect(store.albumSummaries["a1"]?.title == "Alpha")
        #expect(store.albumSummaries["a2"]?.title == "Beta")
        #expect(store.albumSummaries["a3"]?.title == "Gamma")
    }

    @MainActor
    @Test("loadRange is idempotent — skips fully-loaded ranges")
    func loadRangeIdempotent() async {
        let store = LibraryStore()
        let source = CountingAlbumPageSource(albums: [
            makeBridgeAlbum(id: "a1", title: "Alpha"),
            makeBridgeAlbum(id: "a2", title: "Beta"),
        ])
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 2)
        #expect(source.pageCallCount == 1)

        await list.loadRange(offset: 0, limit: 2)

        #expect(source.pageCallCount == 1)
    }

    @MainActor
    @Test("rowCount computes correctly")
    func rowCountComputation() async {
        let store = LibraryStore()
        let albums = (0..<7)
            .map {
                makeBridgeAlbum(id: "a\($0)", title: "Album \($0)")
            }
        let list = makeList(store: store, albums: albums)
        await list.loadInitial()

        #expect(list.rowCount(columnCount: 4) == 2)
        #expect(list.rowCount(columnCount: 3) == 3)
        #expect(list.rowCount(columnCount: 1) == 7)
        #expect(list.rowCount(columnCount: 0) == 0)
    }

    @MainActor
    @Test("empty page source yields totalCount == 0 and empty ids")
    func emptyList() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [])

        await list.loadInitial()

        #expect(list.totalCount == 0)
        #expect(list.idAt(0) == nil)
    }

    @MainActor
    @Test("loadInitial surfaces a cold count failure as initialLoadError")
    func loadInitialSurfacesInitialLoadError() async {
        // A cold count-load failure is not an empty library: it lands on the
        // list's `initialLoadError` (which the grid renders as error + Retry),
        // not on `onError` (which would surface a redundant banner over the
        // empty grid).
        let source = ThrowingAlbumPageSource(
            albums: [],
            countError: PaginatedListTestError(message: "count failed")
        )
        var errors: [DisplayError] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { error in DisplayError(error).map { errors.append($0) } },
        )

        await list.loadInitial()

        #expect(list.initialLoadError == DisplayError(line: "count failed"))
        #expect(errors.isEmpty)
        #expect(list.totalCount == 0)
    }

    @MainActor
    @Test("a successful retry clears initialLoadError and sets the count")
    func retryClearsInitialLoadError() async {
        let source = ThrowingAlbumPageSource(
            albums: [makeBridgeAlbum(id: "a1")],
            countError: PaginatedListTestError(message: "count failed")
        )
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { _ in },
        )
        await list.loadInitial()
        #expect(list.initialLoadError != nil)

        source.countError = nil
        await list.loadInitial()

        #expect(list.initialLoadError == nil)
        #expect(list.totalCount == 1)
    }

    @MainActor
    @Test("loadRange reports page errors")
    func loadRangeReportsPageErrors() async {
        let albums = (0..<51).map { makeBridgeAlbum(id: "a\($0)") }
        let source = ThrowingAlbumPageSource(
            albums: albums,
            pageError: PaginatedListTestError(message: "page failed")
        )
        var errors: [DisplayError] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { error in DisplayError(error).map { errors.append($0) } },
        )

        await list.loadInitial()
        await list.loadRange(offset: 50, limit: 1)

        #expect(errors == [DisplayError(line: "page failed")])
    }
}

// MARK: - RowLoadID (row task-restart identity)

/// Every paginated row keys its load `.task(id:)` on a `RowLoadID` (the list's
/// `loadEpoch` + the row position). These exercise that id — the value the views
/// actually key on. A list swap changes the identity; page deliveries do not,
/// because the active subscription updates that list in place.
@Suite("PaginatedList row load identity")
struct PaginatedListRowLoadIDTests {
    @MainActor
    @Test("a row's task id differs across a list swap, at a fixed position")
    func differsAcrossSwap() async {
        let store = LibraryStore()
        let albums = [makeBridgeAlbum(id: "a1")]
        let first = makeList(store: store, albums: albums)
        let second = makeList(store: store, albums: albums)
        await first.loadInitial()
        await second.loadInitial()

        // Position alone cannot tell the lists apart. The instance identity in
        // the epoch makes the swapped-in row's task restart.
        #expect(
            RowLoadID(epoch: first.loadEpoch, index: 0)
                != RowLoadID(epoch: second.loadEpoch, index: 0)
        )
    }

    @MainActor
    @Test("a content load leaves a row's task id unchanged")
    func stableAcrossContentLoad() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [makeBridgeAlbum(id: "a1")])
        await list.loadInitial()
        let before = RowLoadID(epoch: list.loadEpoch, index: 0)

        await list.loadRange(offset: 0, limit: 1)

        #expect(RowLoadID(epoch: list.loadEpoch, index: 0) == before)
    }
}

// MARK: - Segment coalescing

/// `insertSegment` is private, so these drive it through the real `loadRange`
/// path — the only way segments enter the list — and observe the merged result
/// through `allLoadedIds` / `idAt`. Without coalescing the loaded ids would
/// carry duplicates, gaps, or stale positions.
@Suite("PaginatedList segment management")
struct PaginatedListSegmentTests {
    @MainActor
    private func albumList(_ store: LibraryStore) -> AlbumList {
        makeList(
            store: store,
            albums: (0..<55).map { makeBridgeAlbum(id: "a\($0)") }
        )
    }

    @MainActor
    @Test("overlapping ranges merge without duplicates")
    func overlappingMerge() async {
        let list = albumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 50, limit: 3)  // [50, 53)
        await list.loadRange(offset: 51, limit: 3)  // [51, 54) overlaps

        #expect(list.allLoadedIds == (0..<54).map { "a\($0)" })
    }

    @MainActor
    @Test("adjacent ranges coalesce into one contiguous run")
    func adjacentCoalesce() async {
        let list = albumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 50, limit: 2)  // [50, 52)
        await list.loadRange(offset: 52, limit: 2)  // [52, 54)

        #expect(list.allLoadedIds == (0..<54).map { "a\($0)" })
        #expect(list.idAt(53) == "a53")
    }

    @MainActor
    @Test("a new range absorbs a segment it fully contains")
    func absorbsContainedSegment() async {
        let list = albumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 51, limit: 2)  // [51, 53)
        await list.loadRange(offset: 50, limit: 4)  // [50, 54) contains it

        #expect(list.allLoadedIds == (0..<54).map { "a\($0)" })
    }

    @MainActor
    @Test("disjoint ranges stay separate and sort by position")
    func disjointSortsByPosition() async {
        let list = albumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 53, limit: 2)  // [53, 55) loaded first
        await list.loadRange(offset: 50, limit: 2)  // [50, 52)

        #expect(
            list.allLoadedIds
                == (0..<52).map { "a\($0)" } + ["a53", "a54"]
        )
        #expect(list.idAt(52) == nil)  // the gap stays unloaded
    }

    @MainActor
    @Test("concurrent loadRange for the same range issues a single fetch")
    func concurrentLoadRangeCoalesces() async {
        let store = LibraryStore()
        let source = GatedAlbumPageSource(
            albums: (0..<52).map { makeBridgeAlbum(id: "a\($0)") }
        )
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()

        async let first: Void = list.loadRange(offset: 50, limit: 2)
        // The first subscription is now blocked on the gate, so
        // loadRange has already registered its in-flight task. The second caller
        // dedupes onto it instead of issuing its own query.
        await source.waitForPageEntry()

        async let second: Void = list.loadRange(offset: 50, limit: 2)
        await Task.yield()
        await source.openGate()

        _ = await (first, second)

        #expect(source.pageCallCount == 2)
        #expect(list.idAt(50) == "a50")
    }

    @MainActor
    @Test("a shrinking final page removes ids beyond the new total")
    func shrinkingFinalPageClipsLoadedIds() async {
        let source = MutableAlbumPageSource(count: 55)
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { _ in },
        )
        await list.loadInitial()
        await list.loadRange(offset: 50, limit: 5)

        await source.setCount(52)

        #expect(list.totalCount == 52)
        #expect(list.idAt(50) == "a50")
        #expect(list.idAt(51) == "a51")
        #expect(list.idAt(52) == nil)
        #expect(list.allLoadedIds == (0..<52).map { "a\($0)" })
    }

    @MainActor
    @Test("visible page subscriptions stay bounded while scrolling")
    func visiblePageSubscriptionsStayBounded() async {
        let source = MutableAlbumPageSource(count: 500)
        var errors: [String] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { errors.append($0.localizedDescription) },
        )
        await list.loadInitial()

        for offset in stride(from: 50, through: 250, by: 50) {
            await list.loadRange(offset: offset, limit: 50)
        }

        #expect(source.activeCount <= 3)
        #expect(!source.activeOffsets.contains(0))
        #expect(list.idAt(0) == nil)

        await source.setCount(501)

        #expect(list.totalCount == 501)
        #expect(list.idAt(0) == nil)

        await source.deliverCancelledValue(offset: 0, totalCount: 999)
        await source.deliverCancelledError(offset: 0)

        #expect(list.totalCount == 501)
        #expect(list.idAt(0) == nil)
        #expect(errors.isEmpty)
    }

    @MainActor
    @Test("a screenful of rows stays loaded while scrolling row by row")
    func scrollingNeverBlanksRowsOnScreen() async {
        let total = 180
        let source = MutableAlbumPageSource(count: total)
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { _ in }
        )
        await list.loadInitial()

        let viewport = Viewport(list: list)
        // Sampled on the main actor after the list has registered the page it
        // was just asked for — and evicted for it — but before that page's
        // first value lands. That gap is a frame the list renders, so a row on
        // screen has to resolve there too.
        source.onBeforeDelivery { viewport.sample() }

        // A screenful walking down the list one row at a time, the way a
        // `List` mounts rows: each row that appears asks for the page holding
        // it. The row that just appeared may be a placeholder until its page
        // answers; the rows already above it may not.
        for index in 0..<total {
            viewport.positions = max(0, index - 18)..<index
            await list.loadPage(containing: index)
            viewport.sample()
        }

        #expect(viewport.blanked.isEmpty)
        // Four aligned pages answer all 180 rows. A window centred on the row
        // that asked would be a fresh page per row instead, and evicting those
        // is what empties the screen.
        #expect(source.subscribedOffsets == [0, 50, 100, 150])
    }

}

/// The rows on screen, and every position that resolved to no id while it was
/// one of them.
@MainActor
private final class Viewport {
    var positions: Range<Int> = 0..<0
    private(set) var blanked: [Int] = []

    private let list: AlbumList

    init(list: AlbumList) {
        self.list = list
    }

    func sample() {
        blanked.append(contentsOf: positions.filter { list.idAt($0) == nil })
    }
}

private final class MutableAlbumPageSource: PageSource, @unchecked Sendable {
    private struct Active {
        let offset: Int
        let limit: Int
        let onValue: @MainActor @Sendable ([BridgeAlbum], Int) -> Void
        let onError: @MainActor @Sendable (any Error) -> Void
    }

    private let lock = NSLock()
    private var count: Int
    private var active: [UUID: Active] = [:]
    private var cancelled: [Active] = []
    private var subscribed: [Int] = []
    private var beforeDelivery: @MainActor @Sendable () -> Void = {}

    init(count: Int) {
        self.count = count
    }

    var activeCount: Int { lock.withLock { active.count } }
    var activeOffsets: Set<Int> {
        lock.withLock { Set(active.values.map(\.offset)) }
    }

    /// Every offset this source was asked for, in order, without repeats — how
    /// many distinct pages a scroll opened.
    var subscribedOffsets: [Int] { lock.withLock { subscribed } }

    /// Run `hook` on the main actor once per subscription, after the list has
    /// finished registering it and before its first value is delivered.
    func onBeforeDelivery(_ hook: @escaping @MainActor @Sendable () -> Void) {
        lock.withLock { beforeDelivery = hook }
    }

    func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([BridgeAlbum], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let id = UUID()
        let active = Active(
            offset: offset,
            limit: limit,
            onValue: onValue,
            onError: onError
        )
        let hook = lock.withLock { () -> @MainActor @Sendable () -> Void in
            self.active[id] = active
            if !self.subscribed.contains(offset) {
                self.subscribed.append(offset)
            }
            return self.beforeDelivery
        }
        Task { @MainActor in hook() }
        Task { await deliver(active) }
        return MutablePageSubscription { [weak self] in
            guard let self else { return }
            self.lock.withLock {
                if let removed = self.active.removeValue(forKey: id) {
                    self.cancelled.append(removed)
                }
            }
        }
    }

    func deliverCancelledValue(offset: Int, totalCount: Int) async {
        let subscription = lock.withLock {
            cancelled.first { $0.offset == offset }
        }
        guard let subscription else { return }
        let end = min(subscription.offset + subscription.limit, totalCount)
        let rows =
            subscription.offset < end
            ? (subscription.offset..<end)
                .map { makeBridgeAlbum(id: "stale-a\($0)") }
            : []
        await subscription.onValue(rows, totalCount)
    }

    func deliverCancelledError(offset: Int) async {
        let subscription = lock.withLock {
            cancelled.first { $0.offset == offset }
        }
        guard let subscription else { return }
        await subscription.onError(
            PaginatedListTestError(message: "stale error")
        )
    }

    func setCount(_ count: Int) async {
        let subscriptions = lock.withLock {
            self.count = count
            return Array(active.values)
        }
        for subscription in subscriptions {
            await deliver(subscription)
        }
    }

    private func deliver(_ active: Active) async {
        let count = lock.withLock { self.count }
        let end = min(active.offset + active.limit, count)
        let rows =
            active.offset < end
            ? (active.offset..<end).map { makeBridgeAlbum(id: "a\($0)") }
            : []
        await active.onValue(rows, count)
    }
}

private final class MutablePageSubscription: PageSubscription,
    @unchecked Sendable
{
    private let onCancel: @Sendable () -> Void

    init(onCancel: @escaping @Sendable () -> Void) {
        self.onCancel = onCancel
    }

    func cancel() {
        onCancel()
    }
}

/// One-shot async gate: `page()` awaits `wait()`; the test resumes every waiter
/// with `open()`. Lets a test hold a fetch mid-flight while it drives the list.
private actor FetchGate {
    private var waiters: [CheckedContinuation<Void, Never>] = []
    private var isOpen = false

    func wait() async {
        if isOpen { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func open() {
        isOpen = true
        for waiter in waiters { waiter.resume() }
        waiters.removeAll()
    }
}

/// Page source that blocks each `page()` on a gate the test opens, and signals
/// when a fetch enters. Used to exercise the in-flight dedupe and the
/// subscription coalescing, which needs the first subscription held in flight.
final class GatedAlbumPageSource: PageSource, @unchecked Sendable {
    let albums: [BridgeAlbum]
    var pageCallCount = 0

    private let gate = FetchGate()
    private let entries: AsyncStream<Void>
    private let entryContinuation: AsyncStream<Void>.Continuation

    init(albums: [BridgeAlbum]) {
        self.albums = albums
        (entries, entryContinuation) = AsyncStream.makeStream(of: Void.self)
    }

    func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([BridgeAlbum], Int) -> Void,
        onError _: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        pageCallCount += 1
        let albums = albums
        let gate = gate
        let entryContinuation = entryContinuation
        return TestPageSubscription(
            Task { @MainActor in
                if !(offset == 0 && limit == 50) {
                    entryContinuation.yield(())
                    await gate.wait()
                }
                let start = min(offset, albums.count)
                let end = min(start + limit, albums.count)
                onValue(Array(albums[start..<end]), albums.count)
            }
        )
    }

    /// Suspend until a `page()` call has entered (and is blocked on the gate).
    func waitForPageEntry() async {
        var iterator = entries.makeAsyncIterator()
        _ = await iterator.next()
    }

    func openGate() async {
        await gate.open()
    }
}

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
        let list = try XCTUnwrap(slot.list)

        let root = candidateList(store: store, uiStore: uiStore, slot: slot)
        let hosting = NSHostingView(rootView: root)
        let window = makeWindow(hosting: hosting)
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        drainViewportLayout()

        let table = try XCTUnwrap(
            descendants(of: hosting).compactMap { $0 as? NSTableView }.first
        )
        let scrollView = try XCTUnwrap(table.enclosingScrollView)
        let anchorIndex = 30
        scrollView.contentView.scroll(
            to: table.rect(ofRow: anchorIndex).origin
        )
        scrollView.reflectScrolledClipView(scrollView.contentView)
        drainViewportLayout()
        let anchor = try XCTUnwrap(list.idAt(topRow(in: table)))

        uiStore.setFolderCandidateSelection([viewportCandidateKey(35)])
        let changed = (0..<80)
            .map { index in
                index < 20 ? groupHeaderItem(index) : candidateItem(index)
            }
        await source.replaceItems(changed)
        drainViewportLayout()

        XCTAssertEqual(list.idAt(topRow(in: table)), anchor)
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
                contentRevision: 3,
                revealInProgress: false,
                positionOf: { $0 == targetKey ? targetIndex : nil }
            ),
            targetIndex
        )
    }

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
            onImportSelected: { _ in }
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
                matched: nil,
                selectable: false,
                importStatus: nil,
                picked: nil,
                claim: nil
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

    private func topRow(in table: NSTableView) -> Int {
        let y = table.enclosingScrollView?.contentView.bounds.minY ?? 0
        return table.row(at: NSPoint(x: 0, y: y + 1))
    }

    private func descendants(of view: NSView) -> [NSView] {
        [view] + view.subviews.flatMap { descendants(of: $0) }
    }

    private func drainViewportLayout() {
        for _ in 0..<20 {
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.01))
        }
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
