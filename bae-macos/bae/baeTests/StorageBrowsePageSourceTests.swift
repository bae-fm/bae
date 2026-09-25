import BaeKit
import Foundation
import Testing

@testable import bae

/// A storage read the test drives: it records every view it is pointed at
/// and hands `next` whatever the test delivers.
private final class StorageFeed: @unchecked Sendable {
    private let lock = NSLock()
    private var openedCount = 0
    private var views: [StorageBrowseView] = []
    private var pending: [BridgeStorageBrowseSnapshot] = []
    private var waiter:
        CheckedContinuation<BridgeStorageBrowseSnapshot, any Error>?

    var opened: Int { lock.withLock { openedCount } }
    var requested: [StorageBrowseView] { lock.withLock { views } }

    func query() -> StorageBrowseQuery {
        lock.withLock { openedCount += 1 }
        return StorageBrowseQuery(
            setView: { [self] view in
                lock.withLock { views.append(view) }
            },
            next: { [self] in
                try await withCheckedThrowingContinuation { continuation in
                    let ready: BridgeStorageBrowseSnapshot? = lock.withLock {
                        if pending.isEmpty {
                            waiter = continuation
                            return nil
                        }
                        return pending.removeFirst()
                    }
                    if let ready { continuation.resume(returning: ready) }
                }
            },
            cancel: {}
        )
    }

    func deliver(_ snapshot: BridgeStorageBrowseSnapshot) {
        let waiter:
            CheckedContinuation<BridgeStorageBrowseSnapshot, any Error>? =
                lock.withLock {
                    if let waiter = self.waiter {
                        self.waiter = nil
                        return waiter
                    }
                    pending.append(snapshot)
                    return nil
                }
        waiter?.resume(returning: snapshot)
    }
}

private let byTitle = BridgeStorageSort(
    field: .albumTitle,
    direction: .ascending
)
private let bySize = BridgeStorageSort(
    field: .totalSize,
    direction: .descending
)

private func snapshot(
    sort: BridgeStorageSort,
    rows: [BridgeStorageRow]
) -> BridgeStorageBrowseSnapshot {
    BridgeStorageBrowseSnapshot(
        sort: sort,
        filter: .all,
        windows: [
            BridgeStorageBrowseWindow(
                window: BridgeLibraryPageWindow(offset: 0, limit: 50),
                rows: rows
            )
        ],
        totalCount: UInt64(rows.count),
        totalSize: 7
    )
}

@MainActor
private func waitUntil(_ predicate: @MainActor () -> Bool) async {
    for _ in 0..<500 {
        if predicate() { return }
        await Task.yield()
    }
}

@Suite("StorageBrowsePageSource")
struct StorageBrowsePageSourceTests {
    @MainActor
    @Test(
        "a new sort moves the one query, and rows read under the old one are dropped"
    )
    func resortMovesOneQuery() async {
        let feed = StorageFeed()
        let rows = PreviewData.storageRows
        let totalSize = TotalSizeBox()
        let source = StorageBrowsePageSource(
            query: feed.query(),
            sort: byTitle,
            filter: .all,
            onTotalSize: { totalSize.value = $0 }
        )
        let delivered = DeliveredRows()

        let first = source.subscribe(
            offset: 0,
            limit: 50,
            onValue: { rows, _ in delivered.ids.append(rows.map(\.id)) },
            onError: { _ in }
        )
        #expect(feed.requested.last?.sort == byTitle)
        #expect(feed.requested.last?.windows.count == 1)

        source.setView(sort: bySize, filter: .all)
        first.cancel()
        let second = source.subscribe(
            offset: 0,
            limit: 50,
            onValue: { rows, _ in delivered.ids.append(rows.map(\.id)) },
            onError: { _ in }
        )
        #expect(feed.opened == 1)
        #expect(feed.requested.last?.sort == bySize)
        #expect(
            feed.requested.last?.windows
                == [BridgeLibraryPageWindow(offset: 0, limit: 50)],
            "the old list's late cancel leaves the new list's window alone"
        )

        feed.deliver(snapshot(sort: byTitle, rows: [rows[0]]))
        feed.deliver(snapshot(sort: bySize, rows: [rows[1]]))
        await waitUntil { !delivered.ids.isEmpty }

        #expect(delivered.ids == [[rows[1].id]])
        #expect(totalSize.value == 7)
        _ = second
    }
}

@MainActor
private final class DeliveredRows {
    var ids: [[String]] = []
}

@MainActor
private final class TotalSizeBox {
    var value: UInt64?
}
