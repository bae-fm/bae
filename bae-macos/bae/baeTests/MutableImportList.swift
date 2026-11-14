import BaeKit
import Foundation

@testable import bae

/// An import list whose items a test replaces: it answers each window with
/// the range of items its keys name, where a key is an item's stable key, as
/// stable as the item, and delivers the change to every window, the count and
/// the position the list has open.
final class MutableImportList: @unchecked Sendable {
    private final class Window: @unchecked Sendable {
        let feed = ImportFeed<ListWindowDelivery<BridgeImportListItem>>()
        var request: BridgeListWindow
        var stopped = false

        init(_ request: BridgeListWindow) { self.request = request }
    }

    private final class Position: @unchecked Sendable {
        let feed = ImportFeed<BridgeListPosition>()
        var key: [BridgeSortValue]

        init(_ key: [BridgeSortValue]) { self.key = key }
    }

    private let lock = NSLock()
    private var items: [BridgeImportListItem]
    private var windows: [Window] = []
    private var positions: [Position] = []
    private var counts: [ImportFeed<BridgeListCount>] = []
    private var deliveredIds: Set<String> = []

    init(items: [BridgeImportListItem]) {
        self.items = items
    }

    private func key(_ item: BridgeImportListItem) -> [BridgeSortValue] {
        [.text(value: item.id)]
    }

    /// Where the item `key` names stands, or the end when no item has it.
    private func seek(_ key: [BridgeSortValue]) -> Int {
        guard case .text(let id)? = key.first else { return 0 }
        return items.firstIndex { $0.id == id } ?? items.count
    }

    /// Whether a window has been answered with the item `id`, which stays
    /// true once it has: a window the list lets go of takes its rows with it.
    func hasDelivered(_ id: String) -> Bool {
        lock.withLock { deliveredIds.contains(id) }
    }

    private func delivery(of window: BridgeListWindow)
        -> ListWindowDelivery<BridgeImportListItem>
    {
        let answer = answer(window)
        Wait.happened()
        return answer
    }

    private func answer(_ window: BridgeListWindow)
        -> ListWindowDelivery<BridgeImportListItem>
    {
        lock.withLock {
            let start: Int
            let stop: Int
            let beyond: Int?
            switch window {
            case .forward(let from, let until, let limit):
                start = from.map(seek) ?? 0
                let end = until.map(seek) ?? items.count
                stop = min(start + Int(limit), end)
                beyond = stop < end ? stop : nil
            case .backward(let until, let limit):
                stop = seek(until)
                start = max(0, stop - Int(limit))
                beyond = start > 0 ? start - 1 : nil
            }
            let rows = start < stop ? Array(items[start..<stop]) : []
            deliveredIds.formUnion(rows.map(\.id))
            return ListWindowDelivery(
                window: window,
                rows: rows,
                first: rows.first.map(key),
                beyond: beyond.map { key(items[$0]) }
            )
        }
    }

    /// Replace the list's items and deliver the change to everything open.
    func replaceItems(_ next: [BridgeImportListItem]) async {
        let (windows, positions) = lock.withLock {
            items = next
            return (
                self.windows.filter { !$0.stopped },
                self.positions
            )
        }
        for feed in lock.withLock({ counts }) {
            feed.emit(count)
        }
        for window in windows {
            window.feed.emit(delivery(of: window.request))
        }
        for position in positions {
            position.feed.emit(placed(position.key))
        }
    }

    private var count: BridgeListCount {
        BridgeListCount(
            rows: UInt64(lock.withLock { items.count }),
            extra: .import(narrowed: nil)
        )
    }

    private func placed(_ key: [BridgeSortValue]) -> BridgeListPosition {
        BridgeListPosition(
            key: key,
            rowsBefore: UInt64(lock.withLock { seek(key) })
        )
    }

    /// Apply a selection change the way core does: each candidate's row says
    /// whether it is selected, and the change reaches every window the way any
    /// other change of the items does.
    func apply(_ change: BridgeSelectionChange) async {
        let next = lock.withLock { items }
            .map {
                item -> BridgeImportListItem in
                guard
                    case .candidate(let stableKey, var row, let headerKey) =
                        item
                else { return item }
                switch change {
                case .replace(let keys):
                    row.selected = keys.contains(row.candidateKey)
                case .toggle(let add, let remove):
                    if add.contains(row.candidateKey) { row.selected = true }
                    if remove.contains(row.candidateKey) {
                        row.selected = false
                    }
                case .extend:
                    break
                }
                return .candidate(
                    stableKey: stableKey,
                    row: row,
                    headerKey: headerKey
                )
            }
        await replaceItems(next)
    }

    /// A handle over this list, whose view changes open at the top.
    var handle: ImportListHandle {
        .stub(queries: queries, setView: { _ in .top })
    }

    private var queries: BridgeListQueries<BridgeImportListItem> {
        BridgeListQueries(
            window: { [self] first in
                let window = Window(first)
                lock.withLock { windows.append(window) }
                window.feed.emit(delivery(of: first))
                return ListWindowQuery(
                    setWindow: { [self] next in
                        window.request = next
                        window.feed.emit(delivery(of: next))
                    },
                    values: PulledValues(
                        read: { try await window.feed.next() },
                        cancel: { window.stopped = true }
                    )
                )
            },
            count: { [self] in
                let feed = ImportFeed<BridgeListCount>()
                lock.withLock { counts.append(feed) }
                feed.emit(count)
                return PulledValues(read: { try await feed.next() })
            },
            position: { [self] key in
                let position = Position(key)
                lock.withLock { positions.append(position) }
                position.feed.emit(placed(key))
                return ListPositionQuery(
                    setKey: { [self] next in
                        position.key = next
                        position.feed.emit(placed(next))
                    },
                    values: PulledValues(read: {
                        try await position.feed.next()
                    })
                )
            },
            keyAt: { [self] position in
                lock.withLock {
                    guard !items.isEmpty else { return nil }
                    return key(items[min(Int(position), items.count - 1)])
                }
            }
        )
    }
}

/// One feed of values a fixture answers a read with: a test emits, the read
/// takes them in order.
final class ImportFeed<Value: Sendable>: @unchecked Sendable {
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
