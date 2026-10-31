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
    search: String,
    rows: [BridgeStorageRow],
    selected: [BridgeStorageRow] = [],
    selectionActions: [BridgeReleaseStorageAction] = []
) -> BridgeStorageBrowseSnapshot {
    BridgeStorageBrowseSnapshot(
        sort: sort,
        filter: .all,
        search: search,
        searching: !search.isEmpty,
        windows: [
            BridgeStorageBrowseWindow(
                window: BridgeLibraryPageWindow(offset: 0, limit: 50),
                rows: rows
            )
        ],
        totalCount: UInt64(rows.count),
        totalSize: 7,
        selected: selected,
        selectionActions: selectionActions,
        queues: BridgeStorageQueues(uploads: nil, pins: nil, outputs: nil)
    )
}

@Suite("StorageBrowsePageSource")
struct StorageBrowsePageSourceTests {
    @MainActor
    @Test(
        "a new sort moves the one query, and rows read under the old one are dropped"
    )
    func resortMovesOneQuery() async throws {
        let feed = StorageFeed()
        let rows = PreviewData.storageRows
        let totalSize = TotalSizeBox()
        let source = StorageBrowsePageSource(
            query: feed.query(),
            sort: byTitle,
            filter: .all,
            search: "",
            selected: [],
            onListValue: { totalSize.value = $0.totalSize }
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

        source.setView(sort: bySize, filter: .all, search: "")
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

        feed.deliver(snapshot(sort: byTitle, search: "", rows: [rows[0]]))
        feed.deliver(snapshot(sort: bySize, search: "", rows: [rows[1]]))
        try await Wait.until { !delivered.ids.isEmpty }

        #expect(delivered.ids == [[rows[1].id]])
        #expect(totalSize.value == 7)
        _ = second
    }

    @MainActor
    @Test(
        "a new search moves the one query, and rows read under the old text are dropped"
    )
    func searchMovesOneQuery() async throws {
        let feed = StorageFeed()
        let rows = PreviewData.storageRows
        let source = StorageBrowsePageSource(
            query: feed.query(),
            sort: byTitle,
            filter: .all,
            search: "",
            selected: [],
            onListValue: { _ in }
        )
        let delivered = DeliveredRows()

        source.setView(sort: byTitle, filter: .all, search: "Album")
        let page = source.subscribe(
            offset: 0,
            limit: 50,
            onValue: { rows, _ in delivered.ids.append(rows.map(\.id)) },
            onError: { _ in }
        )
        #expect(feed.opened == 1)
        #expect(feed.requested.last?.search == "Album")

        feed.deliver(snapshot(sort: byTitle, search: "", rows: [rows[0]]))
        feed.deliver(snapshot(sort: byTitle, search: "Album", rows: [rows[1]]))
        try await Wait.until { !delivered.ids.isEmpty }

        #expect(delivered.ids == [[rows[1].id]])
        _ = page
    }

    @MainActor
    @Test(
        "a new selection moves the one query, leaving its windows, and its rows are delivered"
    )
    func selectionMovesOneQuery() async throws {
        let feed = StorageFeed()
        let rows = PreviewData.storageRows
        let answers = SelectedRowsBox()
        let source = StorageBrowsePageSource(
            query: feed.query(),
            sort: byTitle,
            filter: .all,
            search: "",
            selected: [],
            onListValue: { answers.values.append($0.selected.map(\.id)) }
        )
        let page = source.subscribe(
            offset: 0,
            limit: 50,
            onValue: { _, _ in },
            onError: { _ in }
        )

        source.setSelection([rows[0].id, "rel-gone"])
        #expect(feed.opened == 1)
        #expect(feed.requested.last?.selected == [rows[0].id, "rel-gone"])
        #expect(
            feed.requested.last?.windows
                == [BridgeLibraryPageWindow(offset: 0, limit: 50)],
            "a selection leaves the pages as they are"
        )

        feed.deliver(
            snapshot(
                sort: byTitle,
                search: "",
                rows: [rows[1]],
                selected: [rows[0]]
            )
        )
        try await Wait.until { !answers.values.isEmpty }
        #expect(answers.values == [[rows[0].id]])
        _ = page
    }
}

@Suite("StorageManagerStore selection")
struct StorageManagerStoreSelectionTests {
    @MainActor
    @Test(
        "the selection is offered the actions core answered for it, and none while core has yet to answer"
    )
    func selectionActionsAreCoresAnswer() async throws {
        let feed = StorageFeed()
        let rows = Array(PreviewData.storageRows.prefix(3))
        let store = StorageManagerStore(
            library: Library(storageBrowse: { _, _ in feed.query() }),
            onError: { _ in }
        )
        let selected: Set<String> = [rows[0].id, rows[1].id]
        store.select(selected)
        store.update(filter: .all, sort: byTitle, search: "")
        try await Wait.until { !feed.requested.isEmpty }

        feed.deliver(
            snapshot(
                sort: byTitle,
                search: "",
                rows: rows,
                selected: [rows[0], rows[1]],
                selectionActions: [.detachAudio]
            )
        )
        try await Wait.until { store.list != nil }

        #expect(store.selectionActions(for: selected) == [.detachAudio])
        #expect(
            store.selectionActions(for: selected.union([rows[2].id])) == nil,
            "a release core has not answered for leaves the answer unknown"
        )
        store.cancel()
    }
}

@MainActor
private final class SelectedRowsBox {
    var values: [[String]] = []
}

@MainActor
private final class DeliveredRows {
    var ids: [[String]] = []
}

@MainActor
private final class TotalSizeBox {
    var value: UInt64?
}
