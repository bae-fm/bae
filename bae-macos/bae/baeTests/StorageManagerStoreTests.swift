import BaeKit
import Foundation
import Testing

@testable import bae

private let byTitle = BridgeStorageSort(
    field: .albumTitle,
    direction: .ascending
)
private let bySize = BridgeStorageSort(
    field: .totalSize,
    direction: .descending
)

/// What the Storage Manager's store opened its reads for, in order, and the
/// selections it asked each selection read about.
private final class OpenedReads: @unchecked Sendable {
    struct List: Equatable {
        let sort: BridgeStorageSort
        let filter: BridgeStorageFilter
        let search: String
    }

    private let lock = NSLock()
    private var lists: [List] = []
    private var selections: [[String]] = []

    var opened: [List] { lock.withLock { lists } }
    var selected: [[String]] { lock.withLock { selections } }

    func library(rows: [BridgeStorageRow]) -> Library {
        Library(
            storageList: { [self] sort, filter, search in
                lock.withLock {
                    lists.append(
                        List(sort: sort, filter: filter, search: search)
                    )
                }
                return .fixed(
                    rows,
                    answers: UnansweredCount(),
                    extra: .storage(totalSize: 7, searching: !search.isEmpty)
                )
            },
            storageSelection: { [self] _, _ in
                let fixed = StorageSelectionQuery.fixed(
                    rows,
                    answers: UnansweredCount()
                )
                return StorageSelectionQuery(
                    setSelected: { [self] ids in
                        lock.withLock { selections.append(ids.sorted()) }
                        try fixed.setSelected(ids)
                    },
                    values: fixed.values
                )
            },
            storageQueues: {
                .fixed(
                    BridgeStorageQueues(
                        uploads: nil,
                        downloads: nil,
                        outputs: nil
                    )
                )
            }
        )
    }
}

@Suite("StorageManagerStore")
struct StorageManagerStoreTests {
    @MainActor
    @Test("a new sort, filter or search is a list of its own")
    func eachViewIsItsOwnList() async throws {
        let reads = OpenedReads()
        let rows = Array(PreviewData.storageRows.prefix(3))
        let store = StorageManagerStore(
            library: reads.library(rows: rows),
            onError: { Issue.record("Read failed: \($0)") }
        )

        store.update(filter: .all, sort: byTitle, search: "")
        try await Wait.until { store.list?.totalCount == 3 }
        #expect(store.totalSize == 7)
        #expect(store.searching == false)

        store.update(filter: .all, sort: bySize, search: "")
        store.update(filter: .local, sort: bySize, search: "Album")
        try await Wait.until { store.searching }

        #expect(
            reads.opened == [
                .init(sort: byTitle, filter: .all, search: ""),
                .init(sort: bySize, filter: .all, search: ""),
                .init(sort: bySize, filter: .local, search: "Album"),
            ]
        )
        store.cancel()
    }

    @MainActor
    @Test("the transfer queues are read once for the manager's life")
    func queuesAreReadOnce() async throws {
        let reads = OpenedReads()
        let store = StorageManagerStore(
            library: reads.library(rows: []),
            onError: { _ in }
        )

        store.update(filter: .all, sort: byTitle, search: "")
        try await Wait.until { store.queues != nil }
        store.update(filter: .all, sort: bySize, search: "")

        #expect(
            store.queues
                == BridgeStorageQueues(
                    uploads: nil,
                    downloads: nil,
                    outputs: nil
                )
        )
        store.cancel()
    }

    @MainActor
    @Test(
        "the selection is offered the actions core answered for it, and none while core has yet to answer"
    )
    func selectionActionsAreCoresAnswer() async throws {
        let reads = OpenedReads()
        let rows = Array(PreviewData.storageRows.prefix(3))
        let store = StorageManagerStore(
            library: reads.library(rows: rows),
            onError: { _ in }
        )
        let selected: Set<String> = [rows[0].id, rows[1].id]
        store.select(selected)
        store.update(filter: .all, sort: byTitle, search: "")
        try await Wait.until { store.list != nil }
        try await Wait.until { store.answered(for: selected) != nil }

        #expect(
            store.answered(for: selected)?.selectionActions
                == commonActions(of: [rows[0], rows[1]])
        )
        #expect(
            store.heldIds == selected,
            "the releases core answered for are the ones the list holds"
        )
        #expect(
            store.answered(for: selected.union([rows[2].id])) == nil,
            "a selection core has not answered for has no answer"
        )
        #expect(
            reads.selected.last == selected.sorted(),
            "the selection is asked about the releases selected"
        )
        store.cancel()
    }

    /// The fixture's stand-in for core's answer: the actions every row
    /// lists.
    private func commonActions(
        of rows: [BridgeStorageRow]
    ) -> [BridgeReleaseStorageAction] {
        rows[0].release.storageActions
            .filter { action in
                rows.allSatisfy { $0.release.storageActions.contains(action) }
            }
    }
}
