import BaeKit
import Foundation
import Testing

@testable import bae

// MARK: - PaginatedList tests

@Suite("PaginatedList")
struct PaginatedListTests {

    @MainActor
    @Test("loadInitial sets totalCount, no positions loaded until loadRange")
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
        #expect(list.idAt(0) == nil)
        #expect(list.idAt(2) == nil)
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
        let source = ThrowingAlbumPageSource(
            albums: [makeBridgeAlbum(id: "a1")],
            pageError: PaginatedListTestError(message: "page failed")
        )
        var errors: [DisplayError] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { error in DisplayError(error).map { errors.append($0) } },
        )

        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 1)

        #expect(errors == [DisplayError(line: "page failed")])
    }

    @MainActor
    @Test("invalidate reports count errors")
    func invalidateReportsCountErrors() async {
        let source = ThrowingAlbumPageSource(albums: [
            makeBridgeAlbum(id: "a1")
        ])
        var errors: [DisplayError] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { error in DisplayError(error).map { errors.append($0) } },
        )
        await list.loadInitial()
        source.countError = PaginatedListTestError(message: "reload failed")

        list.invalidate()
        await list.awaitReload()

        #expect(errors == [DisplayError(line: "reload failed")])
    }

    @MainActor
    @Test(
        "invalidate keeps old state visible, then atomically swaps in refreshed shape"
    )
    func invalidateRefetches() async {
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
        #expect(list.totalCount == 3)

        list.invalidate()

        // Synchronous state immediately after invalidate(): old totalCount
        // and stale segments remain visible while the count fetch runs.
        #expect(list.totalCount == 3)
        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(2) == "a3")

        // After reload: count confirmed, generation bumped, stale segments
        // stay in place until tasks re-fetch them.
        await list.awaitReload()

        #expect(list.totalCount == 3)
        #expect(list.generation == 1)
        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(2) == "a3")
    }

    @MainActor
    @Test(
        "invalidate re-fetches previously loaded ranges and surfaces new rows"
    )
    func invalidateSurfacesNewRows() async {
        let store = LibraryStore()
        // Start with one album; load it.
        let source = MutablePageSource(albums: [
            makeBridgeAlbum(id: "a1", title: "Alpha")
        ])
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 1)
        #expect(list.idAt(0) == "a1")

        // Simulate an import: the page source now returns two rows.
        source.albums = [
            makeBridgeAlbum(id: "a1", title: "Alpha"),
            makeBridgeAlbum(id: "a2", title: "Beta"),
        ]
        list.invalidate()
        await list.awaitReload()

        #expect(list.totalCount == 2)
        // Stale gen-0 segment keeps a1 visible at position 0. Position 1
        // is unloaded — the per-row `.task(id:)` in the consuming view drives
        // lazy fills once the view restarts its task for the new generation.
        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(1) == nil)
    }
}

// MARK: - Mutable page source (for invalidate tests)

/// Test-only page source whose backing array can be mutated between
/// calls. Used to simulate a shape change (import, delete) that
/// `invalidate()` picks up on the next reload.
final class MutablePageSource: PageSource, @unchecked Sendable {
    var albums: [BridgeAlbum]

    init(albums: [BridgeAlbum]) {
        self.albums = albums
    }

    func count() async throws -> Int {
        albums.count
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        let start = min(offset, albums.count)
        let end = min(start + limit, albums.count)
        return Array(albums[start..<end])
    }
}

// MARK: - RowLoadID (row task-restart identity)

/// Every paginated row keys its load `.task(id:)` on a `RowLoadID` (the list's
/// `loadEpoch` + the row position). These exercise that id — the value the views
/// actually key on — across the three transitions that decide whether a row
/// refetches: a swap to a fresh list instance, an invalidation, and a plain
/// content load. The id must change on the first two (or a swapped-in/invalidated
/// row stays stuck on its placeholder) and must NOT change on the third (or every
/// page fetch needlessly restarts every visible row).
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

        // Both fresh lists sit at generation 0, so generation (or position)
        // alone can't tell them apart — the instance identity in the epoch is
        // what makes the swapped-in row's task restart.
        #expect(first.generation == second.generation)
        #expect(
            RowLoadID(epoch: first.loadEpoch, index: 0)
                != RowLoadID(epoch: second.loadEpoch, index: 0)
        )
    }

    @MainActor
    @Test("a row's task id changes when the list is invalidated")
    func changesOnInvalidate() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [makeBridgeAlbum(id: "a1")])
        await list.loadInitial()
        let before = RowLoadID(epoch: list.loadEpoch, index: 0)

        list.invalidate()
        await list.awaitReload()

        #expect(RowLoadID(epoch: list.loadEpoch, index: 0) != before)
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
/// through `allLoadedIds` / `idAt`. Coalescing and stale-generation discard are
/// visible there: without them the loaded ids would carry duplicates, gaps, or
/// stale positions.
@Suite("PaginatedList segment management")
struct PaginatedListSegmentTests {
    @MainActor
    private func fiveAlbumList(_ store: LibraryStore) -> AlbumList {
        makeList(
            store: store,
            albums: (0..<5).map { makeBridgeAlbum(id: "a\($0)") }
        )
    }

    @MainActor
    @Test("overlapping same-generation ranges merge without duplicates")
    func overlappingMerge() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 0, limit: 3)  // [0, 3)
        await list.loadRange(offset: 1, limit: 3)  // [1, 4) overlaps

        #expect(list.allLoadedIds == ["a0", "a1", "a2", "a3"])
    }

    @MainActor
    @Test("adjacent same-generation ranges coalesce into one contiguous run")
    func adjacentCoalesce() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 0, limit: 2)  // [0, 2)
        await list.loadRange(offset: 2, limit: 2)  // [2, 4)

        #expect(list.allLoadedIds == ["a0", "a1", "a2", "a3"])
        #expect(list.idAt(3) == "a3")
    }

    @MainActor
    @Test("a new range absorbs a same-generation segment it fully contains")
    func absorbsContainedSegment() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 1, limit: 2)  // [1, 3)
        await list.loadRange(offset: 0, limit: 4)  // [0, 4) contains it

        #expect(list.allLoadedIds == ["a0", "a1", "a2", "a3"])
    }

    @MainActor
    @Test("disjoint ranges stay separate and sort by position")
    func disjointSortsByPosition() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 3, limit: 2)  // [3, 5) loaded first
        await list.loadRange(offset: 0, limit: 2)  // [0, 2)

        #expect(list.allLoadedIds == ["a0", "a1", "a3", "a4"])
        #expect(list.idAt(2) == nil)  // the gap stays unloaded
    }

    @MainActor
    @Test("a fresh-generation range discards the stale segment it overlaps")
    func staleGenerationDiscard() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 4)  // generation 0: [0, 4)

        list.invalidate()
        await list.awaitReload()  // generation 1

        await list.loadRange(offset: 0, limit: 2)  // generation 1: [0, 2)

        // The stale generation-0 [0, 4) is dropped wholesale, not kept for its
        // non-overlapping tail — positions 2 and 3 revert to unloaded.
        #expect(list.allLoadedIds == ["a0", "a1"])
        #expect(list.idAt(2) == nil)
    }

    @MainActor
    @Test("concurrent loadRange for the same range issues a single fetch")
    func concurrentLoadRangeCoalesces() async {
        let store = LibraryStore()
        let source = GatedAlbumPageSource(albums: [
            makeBridgeAlbum(id: "a1"),
            makeBridgeAlbum(id: "a2"),
        ])
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()

        async let first: Void = list.loadRange(offset: 0, limit: 2)
        // The first fetch's page() is now running and blocked on the gate, so
        // loadRange has already registered its in-flight task. The second caller
        // dedupes onto it instead of issuing its own query.
        await source.waitForPageEntry()

        async let second: Void = list.loadRange(offset: 0, limit: 2)
        await Task.yield()
        await source.openGate()

        _ = await (first, second)

        #expect(source.pageCallCount == 1)
        #expect(list.idAt(0) == "a1")
    }

    @MainActor
    @Test("a fetch whose generation is bumped mid-flight drops its result")
    func generationDropMidFetch() async {
        let store = LibraryStore()
        let source = GatedAlbumPageSource(albums: [
            makeBridgeAlbum(id: "a1"),
            makeBridgeAlbum(id: "a2"),
        ])
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()

        async let load: Void = list.loadRange(offset: 0, limit: 2)
        await source.waitForPageEntry()  // fetch in flight at generation 0

        list.invalidate()
        await list.awaitReload()  // generation bumps to 1

        await source.openGate()
        await load

        // fetchRange sees the generation moved past its own and drops the rows —
        // no segment is inserted.
        #expect(list.idAt(0) == nil)
        #expect(list.allLoadedIds.isEmpty)
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
/// generation-drop guard, both of which need a fetch held mid-flight.
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

    func count() async throws -> Int {
        albums.count
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        pageCallCount += 1
        entryContinuation.yield(())
        await gate.wait()
        let start = min(offset, albums.count)
        let end = min(start + limit, albums.count)
        return Array(albums[start..<end])
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
