import BaeKit
import Foundation
import Testing

@testable import bae

/// A list of albums in the order of their ids whose view answers the way
/// core's does: it keeps a window of three screens around the rows on screen,
/// starting a screen above them, and holds the window at its first row's id,
/// so rows inserted above it move the window's start and not its rows. A test
/// changes the list and delivers the change to every view the list opened.
private final class ProbeList: @unchecked Sendable {
    final class View: @unchecked Sendable {
        private let feed = Feed<ListViewDelivery<BridgeAlbum>>()
        fileprivate(set) var stopped = false
        private let lock = NSLock()
        private var shownRows: ClosedRange<Int>
        private var reports: [ClosedRange<Int>] = []
        /// The id of the window's first row; nil at the list's top.
        fileprivate var from: String?
        fileprivate var limit = 0

        init(_ shown: ClosedRange<Int>) {
            shownRows = shown
            reports = [shown]
        }

        /// Every range of rows on screen the list reported, the one it
        /// opened the view with first.
        var reported: [ClosedRange<Int>] { lock.withLock { reports } }

        var shown: ClosedRange<Int> { lock.withLock { shownRows } }

        fileprivate func show(_ rows: ClosedRange<Int>) {
            lock.withLock {
                shownRows = rows
                reports.append(rows)
            }
        }

        fileprivate func emit(_ value: ListViewDelivery<BridgeAlbum>) {
            feed.emit(value)
        }

        fileprivate func pull() async throws -> ListViewDelivery<BridgeAlbum> {
            Wait.happened()
            return try await feed.next()
        }

        fileprivate func fail(_ error: any Error) { feed.fail(error) }
    }

    private let lock = NSLock()
    private var albums: [BridgeAlbum]
    private var open: [View] = []
    private let counts = Feed<BridgeListCount>()
    private var countHeld = false

    init(_ ids: [String]) {
        albums = ids.map { makeBridgeAlbum(id: $0) }
    }

    var views: [View] { lock.withLock { open } }

    /// Answer no count until `releaseCount`: the view answers while the list
    /// still waits for the count.
    func holdCount() { lock.withLock { countHeld = true } }

    /// Fail the count read with `error`.
    func failCount(_ error: any Error) { counts.fail(error) }

    /// Fail every open view's read with `error`.
    func failViews(_ error: any Error) {
        for view in views where !view.stopped {
            view.fail(error)
        }
    }

    /// Answer the count a held read waits for.
    func releaseCount() {
        let count = lock.withLock {
            countHeld = false
            return albums.count
        }
        emitCount(count)
    }

    private func emitCount(_ count: Int) {
        counts.emit(
            BridgeListCount(
                rows: UInt64(count),
                items: UInt64(count),
                extra: .none
            )
        )
    }

    /// Replace the list's rows and deliver the change to every view, each
    /// window held at its first row, and to the count.
    func set(_ ids: [String]) {
        let (views, count) = lock.withLock {
            albums = ids.sorted().map { makeBridgeAlbum(id: $0) }
            return (open.filter { !$0.stopped }, albums.count)
        }
        emitCount(count)
        for view in views { deliver(to: view) }
    }

    /// Place `view`'s window around the rows on screen, as core does: three
    /// screens from a screen above them.
    private func place(_ view: View) {
        lock.withLock {
            let shown = view.shown
            let screen = shown.count
            let start = min(max(0, shown.lowerBound - screen), albums.count)
            view.from = start == 0 ? nil : albums[start].id
            view.limit = 3 * screen
        }
        deliver(to: view)
    }

    /// The window's rows as they stand: from the row at its first row's id.
    private func deliver(to view: View) {
        let delivery = lock.withLock {
            let start =
                view.from.map { from in
                    albums.firstIndex { $0.id >= from } ?? albums.count
                } ?? 0
            let end = min(start + view.limit, albums.count)
            return ListViewDelivery(
                start: start,
                rows: start < end ? Array(albums[start..<end]) : []
            )
        }
        view.emit(delivery)
    }

    /// Deliver `value` to `view` unchanged, whatever rows the list holds.
    func deliver(_ value: ListViewDelivery<BridgeAlbum>, to view: View) {
        view.emit(value)
    }

    var queries: BridgeListQueries<BridgeAlbum> {
        BridgeListQueries(
            view: { [self] first, last in
                let view = View(Int(first)...Int(last))
                lock.withLock { open.append(view) }
                place(view)
                return ListViewQuery(
                    setVisible: { [self] first, last in
                        view.show(Int(first)...Int(last))
                        place(view)
                    },
                    values: PulledValues(
                        read: { try await view.pull() },
                        cancel: { view.stopped = true }
                    )
                )
            },
            count: { [self] in
                let (held, rows) = lock.withLock { (countHeld, albums.count) }
                if !held { emitCount(rows) }
                return PulledValues(read: { try await self.counts.next() })
            }
        )
    }
}

/// The failures a list sent to its `onError`.
private final class Failures: @unchecked Sendable {
    private let lock = NSLock()
    private var errors: [any Error] = []

    func record(_ error: any Error) {
        lock.withLock { errors.append(error) }
        Wait.happened()
    }

    var count: Int { lock.withLock { errors.count } }
}

private func ids(_ range: ClosedRange<Int>) -> [String] {
    range.map { String(format: "a%04d", $0) }
}

@Suite("PaginatedList")
struct PaginatedListTests {
    @MainActor
    private func open(_ list: ProbeList) async throws
        -> PaginatedList<BridgeAlbum>
    {
        let list = PaginatedList(
            queries: list.queries,
            onError: { Issue.record("List failed: \($0)") }
        )
        await list.loadInitial()
        try await Wait.until { list.count != nil }
        return list
    }

    @MainActor
    private func held(
        _ list: PaginatedList<BridgeAlbum>,
        _ positions: ClosedRange<Int>
    ) -> [Task<Void, Never>] {
        positions.map { position in
            Task { await list.holdPage(containing: position) }
        }
    }

    @MainActor
    private func idsAt(
        _ list: PaginatedList<BridgeAlbum>,
        _ positions: ClosedRange<Int>
    ) -> [String?] {
        positions.map { list.idAt($0) }
    }

    @MainActor
    @Test("loadInitial reads the list's view from its top and the count")
    func loadInitialReadsTopAndCount() async throws {
        let probe = ProbeList(ids(0...29))
        let list = try await open(probe)

        #expect(list.count?.rows == 30)
        #expect(probe.views.first?.reported.first == 0...0)
        #expect(list.idAt(0) == "a0000")
        list.cancel()
    }

    @MainActor
    @Test(
        "a list shows its first rows before its count arrives",
        .timeLimit(.minutes(1))
    )
    func rowsBeforeCount() async throws {
        let probe = ProbeList(ids(0...199))
        probe.holdCount()
        let list = PaginatedList(
            queries: probe.queries,
            onError: { Issue.record("List failed: \($0)") }
        )
        await list.loadInitial()

        #expect(list.idAt(0) == "a0000")
        #expect(
            list.count == nil,
            "the list never uses the rows its view holds as the count"
        )

        probe.releaseCount()
        try await Wait.until { list.count != nil }
        #expect(list.count?.rows == 200)
        list.cancel()
    }

    @MainActor
    @Test(
        "a count that fails before it delivers fails the first read, though the view delivered rows"
    )
    func countFailingFirstFailsTheFirstRead() async throws {
        let probe = ProbeList(ids(0...9))
        probe.holdCount()
        let list = PaginatedList(
            queries: probe.queries,
            onError: { Issue.record("List failed: \($0)") }
        )
        await list.loadInitial()
        #expect(list.row(at: 0) != nil)

        probe.failCount(
            BridgeError.Diagnostic(category: .database, detail: "locked")
        )
        try await Wait.until { list.initialLoadError != nil }
        list.cancel()
    }

    @MainActor
    @Test(
        "a reopened list whose view read fails before its count delivers keeps its rows and reports the failure"
    )
    func reopenedReadFailureKeepsRows() async throws {
        let probe = ProbeList(ids(0...9))
        let failures = Failures()
        let list = PaginatedList(
            queries: probe.queries,
            onError: { failures.record($0) }
        )
        await list.loadInitial()
        try await Wait.until { list.count != nil }

        probe.holdCount()
        await list.reopen(at: 0)
        probe.failViews(
            BridgeError.Diagnostic(category: .database, detail: "locked")
        )
        try await Wait.until { failures.count > 0 }
        #expect(list.initialLoadError == nil)
        #expect(list.row(at: 0) != nil)
        list.cancel()
    }

    @MainActor
    @Test("reopening a list drops the count it held until the new one arrives")
    func reopenDropsCount() async throws {
        let probe = ProbeList(ids(0...199))
        let list = try await open(probe)
        #expect(list.count?.rows == 200)

        probe.holdCount()
        await list.reopen(at: 0)
        #expect(list.count == nil)
        list.cancel()
    }

    @MainActor
    @Test("the list reports the count query's number when rows repeat")
    func distinctItemCount() async throws {
        let album = makeBridgeAlbum(id: "shared")
        let rows = BridgeListQueries<BridgeAlbum>
            .fixed(
                [album, album],
                answers: UnansweredCount()
            )
        let list = PaginatedList<BridgeAlbum>(
            queries: BridgeListQueries(
                view: rows.view,
                count: {
                    .fixed(
                        BridgeListCount(
                            rows: 2,
                            items: 1,
                            extra: .albumGrid(canShuffle: false, grouped: false)
                        )
                    )
                }
            ),
            onError: { Issue.record("List failed: \($0)") }
        )
        await list.loadInitial()
        try await Wait.until { list.count != nil }
        #expect(list.count?.rows == 2)
        #expect(list.itemCount == 1)
        #expect(list.idAt(0) == "shared")
        #expect(list.idAt(1) == "shared")
        list.cancel()
    }

    @MainActor
    @Test("the list tells core the rows on screen each time they change")
    func reportsTheRowsOnScreen() async throws {
        let probe = ProbeList(ids(0...199))
        let list = try await open(probe)
        let view = try #require(probe.views.first)

        let tasks = held(list, 0...10)
        try await Wait.until { view.shown == 0...10 }
        // The view answers each report after it: the rows arrive once it
        // answers the last.
        try await Wait.until {
            idsAt(list, 0...10) == ids(0...10).map(Optional.init)
        }

        for task in tasks { task.cancel() }
        let further = held(list, 100...110)
        try await Wait.until { view.shown == 100...110 }
        try await Wait.until {
            idsAt(list, 100...110) == ids(100...110).map(Optional.init)
        }
        for task in further { task.cancel() }
        list.cancel()
    }

    /// Rows inserted above the rows on screen move the window's start, and
    /// each row sits one place lower: the list holds the same rows and tells
    /// the grid to scroll back to the row it showed on top.
    @MainActor
    @Test(
        "an insert above the rows on screen keeps their rows, one place lower"
    )
    func insertAboveKeepsTheRows() async throws {
        let probe = ProbeList(ids(0...199))
        let list = try await open(probe)
        let tasks = held(list, 100...110)
        try await Wait.until { list.idAt(100) == "a0100" }
        let revision = list.contentRevision

        probe.set(["a0000a"] + ids(0...199))

        try await Wait.until { list.idAt(101) == "a0100" }
        #expect(list.idAt(100) == "a0099")
        #expect(list.contentRevision > revision)
        for task in tasks { task.cancel() }
        list.cancel()
    }

    @MainActor
    @Test("a value from a replaced view draws nothing")
    func replacedViewDrawsNothing() async throws {
        let probe = ProbeList(ids(0...199))
        let list = try await open(probe)
        let first = try #require(probe.views.first)

        await list.reopen(at: 0)
        probe.deliver(
            ListViewDelivery(start: 0, rows: [makeBridgeAlbum(id: "stale")]),
            to: first
        )

        #expect(first.stopped)
        #expect(list.idAt(0) == "a0000")
        list.cancel()
    }

    @MainActor
    @Test("the rows of the view are its last value only")
    func rowsAreTheLastValueOnly() async throws {
        let probe = ProbeList(ids(0...29))
        let list = try await open(probe)
        let tasks = held(list, 0...9)
        try await Wait.until { list.idAt(9) != nil }

        probe.set(ids(0...4))

        try await Wait.until { list.count?.rows == 5 }
        try await Wait.until { list.idAt(5) == nil }
        #expect(idsAt(list, 0...4) == ids(0...4).map(Optional.init))
        for task in tasks { task.cancel() }
        list.cancel()
    }
}
