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

    /// The read for a range selected with Shift: the list it asked about,
    /// and the range's two ends.
    struct Run: Equatable {
        let list: List
        let from: String
        let to: String
    }

    private let lock = NSLock()
    private var lists: [List] = []
    private var selections: [[String]] = []
    private var runs: [Run] = []
    private var everyAsks: [(filter: BridgeStorageFilter, search: String)] =
        []

    var opened: [List] { lock.withLock { lists } }
    var selected: [[String]] { lock.withLock { selections } }
    var runsAsked: [Run] { lock.withLock { runs } }
    var allAsked: [(filter: BridgeStorageFilter, search: String)] {
        lock.withLock { everyAsks }
    }

    /// A library over `rows` whose selection read answers each set of ids
    /// with its entry in `answers`.
    func library(
        rows: [BridgeStorageRow],
        answers: [Set<String>: BridgeStorageSelection] = [[]: .nothing]
    ) -> Library {
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
                    answers: UnansweredCount()
                ) { ids in
                    guard let selection = answers[ids] else {
                        preconditionFailure("no selection answer for \(ids)")
                    }
                    return selection
                }
                return StorageSelectionQuery(
                    setSelected: { [self] ids in
                        lock.withLock { selections.append(ids.sorted()) }
                        try fixed.setSelected(ids)
                    },
                    values: fixed.values
                )
            },
            storageHoldsAll: Library.fixedStorageHoldsAll(),
            storageIdsBetween: { [self] sort, filter, search, from, to in
                lock.withLock {
                    let list = List(sort: sort, filter: filter, search: search)
                    runs.append(Run(list: list, from: from, to: to))
                }
                return [from, to]
            },
            storageIds: { [self] filter, search in
                lock.withLock { everyAsks.append((filter, search)) }
                return rows.map(\.id)
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
    @Test("a new sort, filter or search opens new reads of the list")
    func eachViewOpensItsOwnReads() async throws {
        let reads = OpenedReads()
        let rows = Array(PreviewData.storageRows.prefix(3))
        let store = StorageManagerStore(
            library: reads.library(rows: rows),
            onError: { Issue.record("Read failed: \($0)") }
        )

        store.update(filter: .all, sort: byTitle, search: "")
        try await Wait.until { store.list?.count?.rows == 3 }
        #expect(store.totalSize == 7)
        #expect(store.searching == false)

        store.update(filter: .all, sort: bySize, search: "")
        store.update(filter: .local, sort: bySize, search: "Album")
        try await Wait.until { store.searching == true }

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
    @Test("the store reads the transfer queues once while the manager is open")
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
        "the store offers the actions core returned for the selection, and none until core returns them"
    )
    func selectionActionsAreCoresAnswer() async throws {
        let reads = OpenedReads()
        let rows = Array(PreviewData.storageRows.prefix(3))
        let selected: Set<String> = [rows[0].id, rows[1].id]
        let store = StorageManagerStore(
            library: reads.library(
                rows: rows,
                answers: [
                    []: .nothing,
                    selected: .of(
                        [rows[0], rows[1]],
                        actions: commonActions(of: [rows[0], rows[1]])
                    ),
                ]
            ),
            onError: { _ in }
        )
        store.select(selected)
        store.update(filter: .all, sort: byTitle, search: "")
        try await Wait.until { store.list != nil }
        try await Wait.until { store.answered(for: selected) != nil }

        #expect(
            store.answered(for: selected)?.selectionActions
                == commonActions(of: [rows[0], rows[1]])
        )
        #expect(
            store.soleRelease == nil,
            "when the list holds two of the selected releases, the store has no sole release"
        )
        #expect(
            try await store.holdsAll(Array(selected)),
            "the list holds the releases core answered for"
        )

        #expect(
            store.answered(for: selected.union([rows[2].id])) == nil,
            "the store has no answer for a selection that core has not answered"
        )
        #expect(
            reads.selected.last == selected.sorted(),
            "the store passes the selected releases to the selection read"
        )
        store.cancel()
    }

    @MainActor
    @Test("the inspector's release is the one release the list holds")
    func soleReleaseIsCoresAnswer() async throws {
        let reads = OpenedReads()
        let rows = Array(PreviewData.storageRows.prefix(3))
        let store = StorageManagerStore(
            library: reads.library(
                rows: rows,
                answers: [
                    []: .nothing,
                    [rows[0].id]: .of(
                        [rows[0]],
                        actions: rows[0].release.storageActions,
                        sole: BridgeStorageSoleRelease(
                            releaseId: rows[0].id,
                            albumTitle: rows[0].album.title
                        )
                    ),
                    [rows[0].id, rows[1].id]: .of(
                        [rows[0], rows[1]],
                        actions: commonActions(of: [rows[0], rows[1]])
                    ),
                ]
            ),
            onError: { _ in }
        )
        store.update(filter: .all, sort: byTitle, search: "")
        store.select([rows[0].id])
        try await Wait.until { store.soleRelease == rows[0].id }

        store.select([rows[0].id, rows[1].id])
        try await Wait.until {
            store.answered(for: [rows[0].id, rows[1].id]) != nil
        }
        #expect(store.soleRelease == nil)
        store.cancel()
    }

    @MainActor
    @Test(
        "a Shift range and Select All ask core about the list on screen, loaded or not"
    )
    func runAndSelectAllAskCore() async throws {
        let reads = OpenedReads()
        let rows = Array(PreviewData.storageRows.prefix(3))
        let store = StorageManagerStore(
            library: reads.library(rows: rows),
            onError: { _ in }
        )
        #expect(
            await store.idsBetween(from: rows[0].id, to: rows[2].id) == nil,
            "before the list shows, there is no list to ask about"
        )
        store.update(filter: .local, sort: bySize, search: "tide")
        let run = await store.idsBetween(from: rows[0].id, to: rows[2].id)
        #expect(run == [rows[0].id, rows[2].id])
        #expect(
            reads.runsAsked == [
                OpenedReads.Run(
                    list: OpenedReads.List(
                        sort: bySize,
                        filter: .local,
                        search: "tide"
                    ),
                    from: rows[0].id,
                    to: rows[2].id
                )
            ]
        )
        #expect(await store.allIds() == rows.map(\.id))
        #expect(reads.allAsked.map(\.filter) == [.local])
        #expect(reads.allAsked.map(\.search) == ["tide"])
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

extension BridgeStorageSelection {
    /// Core's answer for a selection of no releases.
    fileprivate static let nothing = BridgeStorageSelection(
        asked: [],
        selected: [],
        sole: nil,
        selectionActions: [],
        cancelOnly: false
    )

    /// Core's answer when the list holds every release in `rows`: core
    /// offers `actions` and names `sole`, with its album's title, as the one
    /// release when the selection has only one.
    fileprivate static func of(
        _ rows: [BridgeStorageRow],
        actions: [BridgeReleaseStorageAction],
        sole: BridgeStorageSoleRelease? = nil
    ) -> BridgeStorageSelection {
        BridgeStorageSelection(
            asked: rows.map(\.id),
            selected: rows,
            sole: sole,
            selectionActions: actions,
            cancelOnly: false
        )
    }
}
