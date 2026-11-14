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
        fileprivate let feed = Feed<ListWindowDelivery<BridgeAlbum>>()
        fileprivate(set) var request: BridgeListWindow
        fileprivate(set) var stopped = false

        init(_ request: BridgeListWindow) { self.request = request }
    }

    private let lock = NSLock()
    private var albums: [BridgeAlbum]
    private var open: [Window] = []
    private let counts = Feed<BridgeListCount>()
    private var keyReads: [UInt64] = []

    init(_ ids: [String]) {
        albums = ids.map { makeBridgeAlbum(id: $0) }
    }

    func key(_ id: String) -> [BridgeSortValue] { [.text(value: id)] }

    var windows: [Window] { lock.withLock { open } }

    var requests: [BridgeListWindow] { windows.map(\.request) }

    var keyAtReads: [UInt64] { lock.withLock { keyReads } }

    /// Replace the list's rows and deliver the change to every window and the
    /// count.
    func set(_ ids: [String]) {
        let (windows, count) = lock.withLock {
            albums = ids.sorted().map { makeBridgeAlbum(id: $0) }
            return (open.filter { !$0.stopped }, albums.count)
        }
        counts.emit(BridgeListCount(rows: UInt64(count), extra: .none))
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
        window.feed.emit(delivery(of: window.request))
    }

    /// A value for `window` as it is, whatever the list holds.
    func deliver(_ value: ListWindowDelivery<BridgeAlbum>, to window: Window) {
        window.feed.emit(value)
    }

    var queries: BridgeListQueries<BridgeAlbum> {
        BridgeListQueries(
            window: { [self] first in
                let window = Window(first)
                lock.withLock { open.append(window) }
                deliver(to: window)
                return ListWindowQuery(
                    setWindow: { [self] next in
                        window.request = next
                        deliver(to: window)
                    },
                    values: PulledValues(
                        read: { try await window.feed.next() },
                        cancel: { window.stopped = true }
                    )
                )
            },
            count: { [self] in
                counts.emit(
                    BridgeListCount(
                        rows: UInt64(lock.withLock { albums.count }),
                        extra: .none
                    )
                )
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

/// One feed of values a probe answers a read with: a test emits, the read
/// takes them in order.
private final class Feed<Value: Sendable>: @unchecked Sendable {
    private let stream: AsyncStream<Value>
    private let continuation: AsyncStream<Value>.Continuation
    private var iterator: AsyncStream<Value>.AsyncIterator

    init() {
        (stream, continuation) = AsyncStream.makeStream()
        iterator = stream.makeAsyncIterator()
    }

    func emit(_ value: Value) { continuation.yield(value) }

    func next() async throws -> Value {
        guard let value = await iterator.next() else {
            throw CancellationError()
        }
        return value
    }
}

private func ids(_ range: ClosedRange<Int>) -> [String] {
    range.map { String(format: "a%04d", $0) }
}

@Suite("PaginatedList")
struct PaginatedListTests {
    @MainActor
    private func open(_ list: ProbeList) async -> PaginatedList<BridgeAlbum> {
        let list = PaginatedList(
            queries: list.queries,
            onError: { Issue.record("List failed: \($0)") }
        )
        await list.loadInitial()
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
    func loadInitialReadsTopAndCount() async {
        let probe = ProbeList(ids(0...29))
        let list = await open(probe)

        #expect(list.totalCount == 30)
        #expect(idsAt(list, 0...29) == ids(0...29).map(Optional.init))
        #expect(idsAt(list, 30...30) == [nil])
    }

    @MainActor
    @Test("a window cut by its limit closes at the key beyond it")
    func closesAtBeyond() async throws {
        let probe = ProbeList(ids(0...199))
        let list = await open(probe)
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
        let list = await open(probe)
        let tasks = held(list, 0...10)
        defer {
            for task in tasks { task.cancel() }
        }
        try await Wait.until { list.idAt(49) == "a0049" }

        // Rows land inside the first window's range, so it holds more than
        // its limit.
        let added = (1...5).map { "a0010x\($0)" }
        let all = (ids(0...199) + added).sorted()
        probe.set(all)

        try await Wait.until {
            idsAt(list, 0...(all.count - 1 > 100 ? 100 : all.count - 1))
                == all.prefix(101).map(Optional.init)
        }
        #expect(list.totalCount == all.count)
    }

    @MainActor
    @Test("a window that goes empty is dropped into its neighbour")
    func emptyWindowIsDropped() async throws {
        let probe = ProbeList(ids(0...129))
        let list = await open(probe)
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
        let list = await open(probe)

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
        let list = await open(probe)
        let tasks = held(list, 0...10)
        defer {
            for task in tasks { task.cancel() }
        }
        try await Wait.until {
            probe.requests.first
                == .forward(from: nil, until: probe.key("a0050"), limit: 50)
        }
        let first = try #require(probe.windows.first)

        // The window answered the request it was opened with, which it has
        // since left.
        probe.deliver(
            ListWindowDelivery(
                window: .forward(from: nil, until: nil, limit: 50),
                rows: [makeBridgeAlbum(id: "stale")],
                first: probe.key("stale"),
                beyond: nil
            ),
            to: first
        )
        // Let the value be taken, if it is going to be.
        for _ in 0..<20 { await Task.yield() }

        #expect(idsAt(list, 0...49) == ids(0...49).map(Optional.init))
    }

    @MainActor
    @Test("the rows of a window are its last value only")
    func rowsAreTheLastValueOnly() async throws {
        let probe = ProbeList(ids(0...29))
        let list = await open(probe)

        probe.set(ids(0...9))

        try await Wait.until { list.totalCount == 10 }
        try await Wait.until { list.idAt(10) == nil }
        #expect(idsAt(list, 0...9) == ids(0...9).map(Optional.init))
    }
}
