import AppKit
import BaeKit
import Foundation
import Observation
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
    @Test("a page's ids land at their positions, and its rows in the store")
    func pagePopulatesPositions() async {
        let store = LibraryStore()
        let list = makeList(
            store: store,
            albums: (0..<60)
                .map {
                    makeBridgeAlbum(id: "a\($0)", title: "Title \($0)")
                }
        )
        await list.loadInitial()
        #expect(list.idAt(55) == nil)

        let held = await list.withPage(containing: 55) {
            (list.idAt(50), list.idAt(59))
        }

        #expect(held == ("a50", "a59"))
        #expect(store.albumSummaries["a55"]?.title == "Title 55")
    }

    @MainActor
    @Test("a page already live is not asked for again")
    func livePageNotAskedAgain() async {
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
        #expect(source.pageCallCount == 1)

        await list.withPage(containing: 1) {}

        #expect(source.pageCallCount == 1)
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
    @Test("a page's failure is reported")
    func pageReportsErrors() async {
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
        await list.withPage(containing: 50) {}

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

        await list.withPage(containing: 0) {}

        #expect(RowLoadID(epoch: list.loadEpoch, index: 0) == before)
    }
}

// MARK: - Segment coalescing

/// Segments are private, so these drive them through the pages that fill
/// them and observe the merged result through `allLoadedIds` / `idAt`.
/// Without coalescing the loaded ids would carry duplicates, gaps, or stale
/// positions.
@Suite("PaginatedList segment management")
struct PaginatedListSegmentTests {
    @MainActor
    private func albumList(_ store: LibraryStore) -> AlbumList {
        makeList(
            store: store,
            albums: (0..<200).map { makeBridgeAlbum(id: "a\($0)") }
        )
    }

    @MainActor
    @Test("adjacent pages coalesce into one contiguous run")
    func adjacentCoalesce() async {
        let list = albumList(LibraryStore())
        await list.loadInitial()

        await list.withPage(containing: 50) {}

        #expect(list.allLoadedIds == (0..<100).map { "a\($0)" })
        #expect(list.idAt(99) == "a99")
    }

    @MainActor
    @Test("disjoint pages stay separate and sort by position")
    func disjointSortsByPosition() async {
        let list = albumList(LibraryStore())
        await list.loadInitial()

        await list.withPage(containing: 150) {}
        await list.withPage(containing: 100) {}

        #expect(
            list.allLoadedIds
                == (0..<50).map { "a\($0)" } + (100..<200).map { "a\($0)" }
        )
        #expect(list.idAt(50) == nil)  // the gap stays unloaded
        // Each loaded id at its own position, skipping the gap.
        let entries = list.loadedEntries
        #expect(entries.map(\.position) == Array(0..<50) + Array(100..<200))
        #expect(entries.map(\.id) == list.allLoadedIds)
    }

    @MainActor
    @Test("callers asking for a page on its way wait on its one fetch")
    func concurrentPageAsksCoalesce() async throws {
        let store = LibraryStore()
        let source = GatedAlbumPageSource(
            albums: (0..<100).map { makeBridgeAlbum(id: "a\($0)") }
        )
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()

        let first = Task {
            await list.withPage(containing: 50) { list.idAt(50) }
        }
        // The first subscription is now held at the gate.
        await source.waitForPageEntry()

        var secondStarted = false
        let second = Task {
            secondStarted = true
            return await list.withPage(containing: 51) { list.idAt(51) }
        }
        // Started on the main actor, the second caller has run up to where
        // it waits: on the first's fetch, or on one of its own.
        try await Wait.until { secondStarted }
        #expect(source.pageCallCount == 2)
        await source.openGate()

        // Each body ran once the page had answered.
        #expect(await first.value == "a50")
        #expect(await second.value == "a51")
        #expect(source.pageCallCount == 2)
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
        await list.withPage(containing: 50) {}

        await source.setCount(52)

        #expect(list.totalCount == 52)
        #expect(list.idAt(50) == "a50")
        #expect(list.idAt(51) == "a51")
        #expect(list.idAt(52) == nil)
        #expect(list.allLoadedIds == (0..<52).map { "a\($0)" })
    }

    @MainActor
    @Test("a page value repeating the held rows notifies no observer")
    func repeatedPageValueIsNotAChange() async {
        let source = MutableAlbumPageSource(count: 120)
        let ingests = IngestCount()
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in ingests.value += 1 },
            onError: { _ in },
        )
        await list.loadInitial()
        await list.withPage(containing: 60) {}
        let revision = list.contentRevision
        let ingested = ingests.value
        let notified = ObservationFlag()
        withObservationTracking {
            _ = list.idAt(0)
            _ = list.totalCount
            _ = list.contentRevision
        } onChange: {
            notified.set()
        }

        // Every page answers again with what it answered before, the way the
        // import list's source answers all of them when another is asked for.
        await source.setCount(120)

        #expect(!notified.isSet)
        #expect(list.contentRevision == revision)
        #expect(ingests.value == ingested)
    }

    @MainActor
    @Test("a page value with a changed row is taken in")
    func changedRowIsTakenIn() async {
        let source = MutableAlbumPageSource(count: 120)
        let store = LibraryStore()
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()
        let revision = list.contentRevision

        await source.retitle("a3", "Retitled")

        #expect(list.contentRevision > revision)
        #expect(store.albumSummaries["a3"]?.title == "Retitled")
        #expect(list.idAt(3) == "a3")
    }

    @MainActor
    @Test("pages nobody holds stay bounded while scrolling")
    func unheldPagesStayBounded() async {
        let source = MutableAlbumPageSource(count: 500)
        var errors: [String] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { errors.append($0.localizedDescription) },
        )
        await list.loadInitial()

        for offset in stride(from: 50, through: 250, by: 50) {
            await list.withPage(containing: offset) {}
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
        // was just asked for — and dropped any for it — but before that page's
        // first value lands. That gap is a frame the list renders, so a row on
        // screen has to resolve there too.
        source.onBeforeDelivery { viewport.sample() }

        // A screenful walking down the list one row at a time, the way a
        // `List` mounts rows: each row that appears holds the page holding
        // it, and each row that leaves lets its hold go. The row that just
        // appeared may be a placeholder until its page answers; the rows
        // already above it may not.
        var rows: [Int: Task<Void, Never>] = [:]
        for index in 0..<total {
            viewport.positions = max(0, index - 18)..<index
            for (row, hold) in rows where !viewport.positions.contains(row) {
                await release(hold)
                rows.removeValue(forKey: row)
            }
            rows[index] = await hold(index, of: list)
            viewport.sample()
        }

        #expect(viewport.blanked.isEmpty)
        // Four aligned pages answer all 180 rows. A window centred on the row
        // that asked would be a fresh page per row instead.
        #expect(source.subscribedOffsets == [0, 50, 100, 150])
    }

}

// MARK: - Page holds

/// Who holds a page decides whether it stays live: rows on screen and a
/// reveal on its way each hold theirs, and only pages nobody holds are
/// dropped.
@Suite("PaginatedList page holds")
struct PaginatedListHoldTests {
    @MainActor
    private func scrolledList() async -> (AlbumList, MutableAlbumPageSource) {
        let source = MutableAlbumPageSource(count: 500)
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { _ in }
        )
        await list.loadInitial()
        return (list, source)
    }

    @MainActor
    @Test("a held page stays live however many pages are asked for after it")
    func heldPageNeverDropped() async {
        let (list, source) = await scrolledList()
        let held = await hold(0, of: list)

        for offset in stride(from: 50, through: 300, by: 50) {
            await list.withPage(containing: offset) {}
        }

        #expect(source.activeOffsets.contains(0))
        #expect(list.idAt(0) == "a0")
        // The held page and the three unheld pages nearest the last asked.
        #expect(source.activeOffsets == [0, 200, 250, 300])
        await release(held)
    }

    @MainActor
    @Test("a page let go of is dropped like any other unheld page")
    func releasedPageDropped() async {
        let (list, source) = await scrolledList()
        let held = await hold(0, of: list)
        for offset in stride(from: 50, through: 300, by: 50) {
            await list.withPage(containing: offset) {}
        }

        await release(held)

        #expect(source.activeOffsets == [200, 250, 300])
        #expect(list.idAt(0) == nil)
    }

    @MainActor
    @Test("the cap on live pages counts only pages nobody holds")
    func capCountsUnheldPages() async {
        let (list, source) = await scrolledList()
        var held: [Task<Void, Never>] = []
        for offset in stride(from: 0, through: 150, by: 50) {
            held.append(await hold(offset, of: list))
        }
        for offset in stride(from: 200, through: 300, by: 50) {
            await list.withPage(containing: offset) {}
        }
        #expect(source.activeCount == 7)

        await list.withPage(containing: 350) {}

        #expect(
            source.activeOffsets == [0, 50, 100, 150, 250, 300, 350]
        )
        for hold in held {
            await release(hold)
        }
    }
}

/// Hold `position`'s page from a task of its own, the way a row on screen
/// does, returning once the page has answered.
@MainActor
private func hold(_ position: Int, of list: AlbumList) async -> Task<
    Void, Never
> {
    let (answered, signal) = AsyncStream.makeStream(of: Void.self)
    let task = Task {
        await list.withPage(containing: position) {
            signal.yield()
            await Task.untilCancelled()
        }
    }
    for await _ in answered {
        break
    }
    return task
}

/// Let a hold go and wait until the list has taken it back.
@MainActor
private func release(_ hold: Task<Void, Never>) async {
    hold.cancel()
    await hold.value
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

@MainActor
private final class IngestCount {
    var value = 0
}

private final class ObservationFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var raised = false

    var isSet: Bool { lock.withLock { raised } }

    func set() {
        lock.withLock { raised = true }
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
    private var titles: [String: String] = [:]

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

    /// Give the album `id` a new title and answer every page again.
    func retitle(_ id: String, _ title: String) async {
        let subscriptions = lock.withLock {
            titles[id] = title
            return Array(active.values)
        }
        for subscription in subscriptions {
            await deliver(subscription)
        }
    }

    private func deliver(_ active: Active) async {
        let (count, titles) = lock.withLock { (self.count, self.titles) }
        let end = min(active.offset + active.limit, count)
        let rows =
            active.offset < end
            ? (active.offset..<end)
                .map { index -> BridgeAlbum in
                    let id = "a\(index)"
                    return titles[id].map { makeBridgeAlbum(id: id, title: $0) }
                        ?? makeBridgeAlbum(id: id)
                }
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
