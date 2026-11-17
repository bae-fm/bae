import BaeKit
import Foundation
import Testing

@testable import bae

/// A list of albums in the order of their ids, which answers each window with
/// the range of ids it names: a key is a row's id, so a key is as stable as the
/// row, and a test changes the list and delivers the change to every window
/// the list opened.
private final class ProbeList: @unchecked Sendable {
    final class Window: @unchecked Sendable {
        private let feed = Feed<ListWindowDelivery<BridgeAlbum>>()
        fileprivate(set) var request: BridgeListWindow
        fileprivate(set) var stopped = false
        private let lock = NSLock()
        private var emits = 0
        private var pulls = 0

        init(_ request: BridgeListWindow) { self.request = request }

        /// Whether the list has taken every value this window answered: it
        /// asks for one more than the window gave.
        var drained: Bool { lock.withLock { pulls > emits } }

        fileprivate func emit(_ value: ListWindowDelivery<BridgeAlbum>) {
            lock.withLock { emits += 1 }
            feed.emit(value)
        }

        fileprivate func pull() async throws -> ListWindowDelivery<BridgeAlbum>
        {
            lock.withLock { pulls += 1 }
            Wait.happened()
            return try await feed.next()
        }

        fileprivate func fail(_ error: any Error) { feed.fail(error) }
    }

    private let lock = NSLock()
    private var albums: [BridgeAlbum]
    private var open: [Window] = []
    private let counts = Feed<BridgeListCount>()
    private var keyReads: [UInt64] = []
    private var countHeld = false
    private var windowsHeld = false
    private var heldWindows: [Window] = []

    init(_ ids: [String]) {
        albums = ids.map { makeBridgeAlbum(id: $0) }
    }

    func key(_ id: String) -> [BridgeSortValue] { [.text(value: id)] }

    var windows: [Window] { lock.withLock { open } }

    var requests: [BridgeListWindow] { windows.map(\.request) }

    var keyAtReads: [UInt64] { lock.withLock { keyReads } }

    /// Answer no count until `releaseCount`: the windows answer while the
    /// list still waits for the count.
    func holdCount() { lock.withLock { countHeld = true } }

    /// Answer no window the list opens until `releaseWindows`.
    func holdWindows() { lock.withLock { windowsHeld = true } }

    /// Answer each window a held read opened.
    func releaseWindows() {
        let held = lock.withLock {
            windowsHeld = false
            defer { heldWindows = [] }
            return heldWindows
        }
        for window in held { deliver(to: window) }
    }

    /// Fail the count read with `error`.
    func failCount(_ error: any Error) { counts.fail(error) }

    /// Fail every open window's read with `error`.
    func failWindows(_ error: any Error) {
        for window in windows where !window.stopped {
            window.fail(error)
        }
    }

    /// Answer the count a held read waits for.
    func releaseCount() {
        let count = lock.withLock {
            countHeld = false
            return albums.count
        }
        counts.emit(
            BridgeListCount(
                rows: UInt64(count),
                items: UInt64(count),
                extra: .none
            )
        )
    }

    /// Replace the list's rows and deliver the change to every window and the
    /// count.
    func set(_ ids: [String]) {
        let (windows, count) = lock.withLock {
            albums = ids.sorted().map { makeBridgeAlbum(id: $0) }
            return (open.filter { !$0.stopped }, albums.count)
        }
        counts.emit(
            BridgeListCount(
                rows: UInt64(count),
                items: UInt64(count),
                extra: .none
            )
        )
        for window in windows { deliver(to: window) }
    }

    private func seek(_ key: [BridgeSortValue]) -> Int {
        guard case .text(let id)? = key.first else { return 0 }
        return albums.firstIndex { $0.id >= id } ?? albums.count
    }

    private func delivery(of window: BridgeListWindow) -> ListWindowDelivery<
        BridgeAlbum
    > {
        lock.withLock {
            let start: Int
            let stop: Int
            let beyond: Int?
            switch window {
            case .forward(let from, let until, let limit):
                start = from.map(seek) ?? 0
                let end = until.map(seek) ?? albums.count
                stop = min(start + Int(limit), end)
                beyond = stop < end ? stop : nil
            case .backward(let until, let limit):
                stop = seek(until)
                start = max(0, stop - Int(limit))
                beyond = start > 0 ? start - 1 : nil
            }
            let rows = start < stop ? Array(albums[start..<stop]) : []
            return ListWindowDelivery(
                window: window,
                rows: rows,
                first: rows.first.map { key($0.id) },
                beyond: beyond.map { key(albums[$0].id) }
            )
        }
    }

    func deliver(to window: Window) {
        window.emit(delivery(of: window.request))
    }

    /// Deliver `value` to `window` unchanged, whatever rows the list holds.
    func deliver(_ value: ListWindowDelivery<BridgeAlbum>, to window: Window) {
        window.emit(value)
    }

    var queries: BridgeListQueries<BridgeAlbum> {
        BridgeListQueries(
            window: { [self] first in
                let window = Window(first)
                let held = lock.withLock {
                    open.append(window)
                    if windowsHeld { heldWindows.append(window) }
                    return windowsHeld
                }
                if !held { deliver(to: window) }
                return ListWindowQuery(
                    setWindow: { [self] next in
                        window.request = next
                        deliver(to: window)
                    },
                    values: PulledValues(
                        read: { try await window.pull() },
                        cancel: { window.stopped = true }
                    )
                )
            },
            count: { [self] in
                let (held, rows) = lock.withLock { (countHeld, albums.count) }
                if !held {
                    counts.emit(
                        BridgeListCount(
                            rows: UInt64(rows),
                            items: UInt64(rows),
                            extra: .none
                        )
                    )
                }
                return PulledValues(read: { try await self.counts.next() })
            },
            position: { [self] key in
                let feed = Feed<BridgeListPosition>()
                let emit: @Sendable ([BridgeSortValue]) -> Void = { key in
                    feed.emit(
                        BridgeListPosition(
                            key: key,
                            rowsBefore: UInt64(
                                self.lock.withLock { self.seek(key) }
                            )
                        )
                    )
                }
                emit(key)
                return ListPositionQuery(
                    setKey: emit,
                    values: PulledValues(read: { try await feed.next() })
                )
            },
            keyAt: { [self] position in
                lock.withLock {
                    keyReads.append(position)
                    guard !albums.isEmpty else { return nil }
                    return key(albums[min(Int(position), albums.count - 1)].id)
                }
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
    @Test("loadInitial reads the top window and the count")
    func loadInitialReadsTopAndCount() async throws {
        let probe = ProbeList(ids(0...29))
        let list = try await open(probe)

        #expect(list.count?.rows == 30)
        #expect(idsAt(list, 0...29) == ids(0...29).map(Optional.init))
        #expect(idsAt(list, 30...30) == [nil])
    }

    @MainActor
    @Test(
        "a list shows its first window before its count arrives",
        .timeLimit(.minutes(1))
    )
    func windowBeforeCount() async throws {
        let probe = ProbeList(ids(0...199))
        probe.holdCount()
        let list = PaginatedList(
            queries: probe.queries,
            onError: { Issue.record("List failed: \($0)") }
        )
        await list.loadInitial()

        #expect(idsAt(list, 0...49) == ids(0...49).map(Optional.init))
        #expect(
            list.count == nil,
            "the list never uses the rows its windows hold as the count"
        )

        probe.releaseCount()
        try await Wait.until { list.count != nil }
        #expect(list.count?.rows == 200)
        list.cancel()
    }

    @MainActor
    @Test(
        "a count that fails before it delivers fails the first read, though a window delivered rows"
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
        "a reopened list whose window read fails before its count delivers keeps its rows and reports the failure"
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
        await list.reopen(at: nil, position: 0)
        probe.failWindows(
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
        await list.reopen(at: nil, position: 0)
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
                window: rows.window,
                count: {
                    .fixed(
                        BridgeListCount(
                            rows: 2,
                            items: 1,
                            extra: .albumGrid(canShuffle: false, grouped: false)
                        )
                    )
                },
                position: rows.position,
                keyAt: rows.keyAt
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
    @Test("a window cut by its limit closes at the key beyond it")
    func closesAtBeyond() async throws {
        let probe = ProbeList(ids(0...199))
        let list = try await open(probe)
        let tasks = held(list, 0...10)
        defer {
            for task in tasks { task.cancel() }
        }

        try await Wait.until {
            probe.requests.first
                == .forward(
                    from: nil,
                    until: probe.key("a0050"),
                    limit: 50
                )
        }
    }

    @MainActor
    @Test("a split on beyond keeps every row once")
    func splitKeepsEveryRowOnce() async throws {
        let probe = ProbeList(ids(0...199))
        let list = try await open(probe)
        let tasks = held(list, 0...10)
        defer {
            for task in tasks { task.cancel() }
        }
        try await Wait.until { list.idAt(49) == "a0049" }

        // The test adds rows inside the first window's range, so it holds
        // more than its limit.
        let added = (1...5).map { "a0010x\($0)" }
        let all = (ids(0...199) + added).sorted()
        probe.set(all)

        try await Wait.until {
            idsAt(list, 0...(all.count - 1 > 100 ? 100 : all.count - 1))
                == all.prefix(101).map(Optional.init)
        }
        try await Wait.until { list.count?.rows == UInt64(all.count) }
    }

    @MainActor
    @Test("the list merges a window that goes empty into its neighbour")
    func emptyWindowIsDropped() async throws {
        let probe = ProbeList(ids(0...129))
        let list = try await open(probe)
        let tasks = held(list, 0...110)
        defer {
            for task in tasks { task.cancel() }
        }
        try await Wait.until { list.idAt(129) == "a0129" }

        // Every row of the middle window goes.
        let rest = ids(0...49) + ids(100...129)
        probe.set(rest)

        try await Wait.until { idsAt(list, 0...79) == rest.map(Optional.init) }
        #expect(list.idAt(80) == nil)
    }

    @MainActor
    @Test("a jump starts a chain at the key the list has there")
    func jumpStartsAtKey() async throws {
        let probe = ProbeList(ids(0...999))
        let list = try await open(probe)

        await list.withPage(containing: 900) {
            #expect(list.idAt(900) == "a0900")
        }

        #expect(probe.keyAtReads == [900])
        #expect(list.idAt(0) == nil)
    }

    @MainActor
    @Test("a value for a replaced window draws nothing")
    func replacedWindowDrawsNothing() async throws {
        let probe = ProbeList(ids(0...199))
        let list = try await open(probe)
        let tasks = held(list, 0...10)
        defer {
            for task in tasks { task.cancel() }
        }
        try await Wait.until {
            probe.requests.first
                == .forward(from: nil, until: probe.key("a0050"), limit: 50)
        }
        let first = try #require(probe.windows.first)

        // This value answers the request the list opened the window with,
        // which the list has since replaced.
        probe.deliver(
            ListWindowDelivery(
                window: .forward(from: nil, until: nil, limit: 50),
                rows: [makeBridgeAlbum(id: "stale")],
                first: probe.key("stale"),
                beyond: nil
            ),
            to: first
        )
        try await Wait.until { first.drained }

        #expect(idsAt(list, 0...49) == ids(0...49).map(Optional.init))
    }

    @MainActor
    @Test("the rows of a window are its last value only")
    func rowsAreTheLastValueOnly() async throws {
        let probe = ProbeList(ids(0...29))
        let list = try await open(probe)

        probe.set(ids(0...9))

        try await Wait.until { list.count?.rows == 10 }
        try await Wait.until { list.idAt(10) == nil }
        #expect(idsAt(list, 0...9) == ids(0...9).map(Optional.init))
    }
}

extension PaginatedListTests {
    /// Another order starts the windows over at the key the list now has at
    /// the first position on screen, and the total count stays the same.
    @MainActor
    @Test("another order rereads the windows from the first row on screen")
    func reorderStartsAtTheFirstRowOnScreen() async throws {
        let probe = ProbeList(ids(0...999))
        let list = try await open(probe)
        let holds = held(list, 400...410)
        try await Wait.until { list.idAt(400) != nil }

        probe.holdCount()
        await list.reorder()

        #expect(probe.keyAtReads.last == 400)
        #expect(list.idAt(400) == "a0400")
        #expect(list.count?.rows == 1000, "another order keeps the count")
        for hold in holds { hold.cancel() }
        list.cancel()
    }

    /// The list draws the rows it has until the first window in the new
    /// order answers, so the screen never empties when the order changes.
    @MainActor
    @Test("another order keeps the rows on screen until its window answers")
    func reorderKeepsTheRowsUntilItsWindowAnswers() async throws {
        let probe = ProbeList(ids(0...999))
        let list = try await open(probe)
        let holds = held(list, 400...410)
        try await Wait.until { list.idAt(400) != nil }
        let opened = probe.windows.count

        probe.holdWindows()
        let reorder = Task { await list.reorder() }
        try await Wait.until { probe.windows.count > opened }

        #expect(idsAt(list, 400...410) == ids(400...410).map(Optional.init))
        probe.releaseWindows()
        await reorder.value
        #expect(list.idAt(400) == "a0400")
        for hold in holds { hold.cancel() }
        list.cancel()
    }
}
