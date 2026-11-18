import BaeKit
import Foundation

@testable import bae

/// An import list whose items a test replaces. The list's view answers the
/// rows on screen with the items around them, as core sizes a window. When
/// a test replaces the items, the list sends the new rows to every open view
/// and count.
final class MutableImportList: @unchecked Sendable {
    private final class View: @unchecked Sendable {
        let feed = ImportFeed<ListViewDelivery<BridgeImportListItem>>()
        var shown: ClosedRange<Int>
        var stopped = false

        init(_ shown: ClosedRange<Int>) { self.shown = shown }
    }

    private let lock = NSLock()
    private var items: [BridgeImportListItem]
    private var views: [View] = []
    private var counts: [ImportFeed<BridgeListCount>] = []
    private var deliveredIds: Set<String> = []

    init(items: [BridgeImportListItem]) {
        self.items = items
    }

    /// Whether the list has sent the item `id` to any view; the answer stays
    /// true after the list closes that view.
    func hasDelivered(_ id: String) -> Bool {
        lock.withLock { deliveredIds.contains(id) }
    }

    private func delivery(of shown: ClosedRange<Int>)
        -> ListViewDelivery<BridgeImportListItem>
    {
        let answer = lock.withLock {
            let delivery = BridgeListQueries<BridgeImportListItem>
                .fixedDelivery(
                    first: shown.lowerBound,
                    last: shown.upperBound,
                    rows: items
                )
            deliveredIds.formUnion(delivery.rows.map(\.id))
            return delivery
        }
        Wait.happened()
        return answer
    }

    /// Replace the list's items and deliver the change to everything open.
    func replaceItems(_ next: [BridgeImportListItem]) async {
        let views = lock.withLock {
            items = next
            return self.views.filter { !$0.stopped }
        }
        for feed in lock.withLock({ counts }) {
            feed.emit(count)
        }
        for view in views {
            view.feed.emit(delivery(of: view.shown))
        }
    }

    private var count: BridgeListCount {
        let rows = UInt64(lock.withLock { items.count })
        return BridgeListCount(
            rows: rows,
            items: rows,
            extra: .import(
                narrowed: nil,
                tabs: BridgeTriageTabCounts(pending: 0, done: 0, skipped: 0)
            )
        )
    }

    /// A handle over this list, whose view changes open at the top, and
    /// which says where a candidate sits among the items as they are now.
    var handle: ImportListHandle {
        .stub(
            queries: queries,
            setView: { _, _ in
                BridgeImportListViewChange(opening: .top, left: [])
            },
            locateCandidate: { [self] key in
                position(of: key)
                    .map { _ in
                        BridgeImportCandidateListLocation(
                            tab: .pending,
                            groupKey: nil
                        )
                    }
            },
            candidatePosition: { [self] key in
                position(of: key).map(UInt64.init)
            }
        )
    }

    /// Where the candidate `key` sits among the items now, on Found.
    private func position(of key: Data) -> Int? {
        lock.withLock {
            items.firstIndex { item in
                if case .candidate(_, let row, _) = item {
                    row.candidateKey == key
                }
                else {
                    false
                }
            }
        }
    }

    private var queries: BridgeListQueries<BridgeImportListItem> {
        BridgeListQueries(
            view: { [self] first, last in
                let view = View(Int(first)...Int(last))
                lock.withLock { views.append(view) }
                view.feed.emit(delivery(of: view.shown))
                return ListViewQuery(
                    setVisible: { [self] first, last in
                        view.shown = Int(first)...Int(last)
                        view.feed.emit(delivery(of: view.shown))
                    },
                    values: PulledValues(
                        read: { try await view.feed.next() },
                        cancel: { view.stopped = true }
                    )
                )
            },
            count: { [self] in
                let feed = ImportFeed<BridgeListCount>()
                lock.withLock { counts.append(feed) }
                feed.emit(count)
                return PulledValues(read: { try await feed.next() })
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
