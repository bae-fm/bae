import BaeKit
import Foundation
import Testing

@testable import bae

private final class AlbumDetailSubscriptionProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var callbacks: [AlbumDetailCallback] = []
    private var subscriptions: [AlbumProbeSubscription] = []

    func subscribe(callback: AlbumDetailCallback)
        -> any LiveSubscriptionProtocol
    {
        let subscription = AlbumProbeSubscription()
        lock.withLock {
            callbacks.append(callback)
            subscriptions.append(subscription)
        }
        return subscription
    }

    func emitError(subscription: Int) {
        let callback = lock.withLock { callbacks[subscription] }
        callback.onError(
            error: .Diagnostic(
                category: .internal,
                detail: "album detail failed"
            )
        )
    }

    func emitValue(subscription: Int, value: BridgeAlbumDetail?) {
        let callback = lock.withLock { callbacks[subscription] }
        callback.onValue(value: value)
    }

    func isCancelled(subscription: Int) -> Bool {
        lock.withLock { subscriptions[subscription].cancelled }
    }

    var count: Int {
        lock.withLock { callbacks.count }
    }
}

/// Stands in for the album browse query: records the windows the list's pages
/// ask for, and answers them with whatever rows a test emits.
private final class AlbumBrowseProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var windows: [BridgeLibraryPageWindow] = []
    private var pending: [LibraryBrowseDelivery<BridgeAlbum>] = []
    private var waiter:
        CheckedContinuation<LibraryBrowseDelivery<BridgeAlbum>, any Error>?

    var query: LibraryBrowseQuery<BridgeAlbum> {
        LibraryBrowseQuery(
            setWindows: { [self] windows in
                lock.withLock { self.windows = windows }
            },
            next: { [self] in try await nextDelivery() },
            cancel: {}
        )
    }

    /// Answer every requested window with `rows`, laid out from offset zero.
    func emit(rows: [BridgeAlbum], total: UInt64) {
        let delivery = lock.withLock {
            LibraryBrowseDelivery(
                windows: windows.map { window in
                    let start = min(Int(window.offset), rows.count)
                    let end = min(start + Int(window.limit), rows.count)
                    return .init(window: window, rows: Array(rows[start..<end]))
                },
                totalCount: Int(total)
            )
        }
        let waiter = lock.withLock {
            let waiter = self.waiter
            self.waiter = nil
            if waiter == nil { pending.append(delivery) }
            return waiter
        }
        waiter?.resume(returning: delivery)
    }

    var isSubscribed: Bool {
        lock.withLock { !windows.isEmpty }
    }

    private func nextDelivery() async throws -> LibraryBrowseDelivery<
        BridgeAlbum
    > {
        try await withCheckedThrowingContinuation { continuation in
            let ready = lock.withLock {
                if pending.isEmpty {
                    waiter = continuation
                    return nil as LibraryBrowseDelivery<BridgeAlbum>?
                }
                return pending.removeFirst()
            }
            if let ready { continuation.resume(returning: ready) }
        }
    }
}

private final class AlbumProbeSubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
    private let lock = NSLock()
    private var isCancelled = false

    var cancelled: Bool { lock.withLock { isCancelled } }

    func cancel() {
        lock.withLock { isCancelled = true }
    }
}

/// Stands in for the live library search: records each query it is pointed
/// at and each time it is opened, and answers with whatever a test emits.
private final class SearchProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var queries: [String] = []
    private var opened = 0
    private var pending: [Result<BridgeLibrarySearchSnapshot, BridgeError>] = []
    private var waiter:
        CheckedContinuation<BridgeLibrarySearchSnapshot, any Error>?

    func open() -> LibrarySearch {
        lock.withLock { opened += 1 }
        return LibrarySearch(
            setQuery: { [self] query in
                lock.withLock { queries.append(query) }
            },
            next: { [self] in try await nextValue() },
            cancel: {}
        )
    }

    var openCount: Int { lock.withLock { opened } }
    var lastQuery: String? { lock.withLock { queries.last } }

    func emitValue(query: String) {
        deliver(
            .success(
                BridgeLibrarySearchSnapshot(
                    query: query,
                    results: BridgeSearchResults(
                        albums: [],
                        artists: [],
                        tracks: [],
                        composers: [],
                        works: []
                    ),
                    requestRevision: 1
                )
            )
        )
    }

    func emitError() {
        deliver(
            .failure(.Diagnostic(category: .internal, detail: "search failed"))
        )
    }

    private func deliver(
        _ value: Result<BridgeLibrarySearchSnapshot, BridgeError>
    ) {
        let waiter = lock.withLock {
            let waiter = self.waiter
            self.waiter = nil
            if waiter == nil { pending.append(value) }
            return waiter
        }
        waiter?.resume(with: value.mapError { $0 as any Error })
    }

    private func nextValue() async throws -> BridgeLibrarySearchSnapshot {
        try await withCheckedThrowingContinuation { continuation in
            let ready = lock.withLock {
                if pending.isEmpty {
                    waiter = continuation
                    return nil
                        as Result<BridgeLibrarySearchSnapshot, BridgeError>?
                }
                return pending.removeFirst()
            }
            if let ready {
                continuation.resume(with: ready.mapError { $0 as any Error })
            }
        }
    }
}

private func waitForSearchQuery(
    _ expected: String,
    probe: SearchProbe
) async throws -> Bool {
    for _ in 0..<100 {
        if probe.lastQuery == expected { return true }
        try await Task.sleep(for: .milliseconds(10))
    }
    return false
}

@Suite("LibraryProjectionStore search")
struct LibraryProjectionStoreSearchTests {
    @MainActor
    @Test("a new query clears the previous query's results on the same search")
    func queryChangeClearsPreviousState() async throws {
        let probe = SearchProbe()
        let store = LibraryProjectionStore(
            library: Library(librarySearch: { probe.open() })
        )

        store.activateSearch("query-a")
        try #require(await waitForSearchQuery("query-a", probe: probe))
        probe.emitValue(query: "query-a")
        await waitForStoreUpdate { store.search.value?.query == "query-a" }
        #expect(store.search.delivered)

        store.activateSearch("query-b")

        #expect(store.search.value == nil)
        #expect(!store.search.delivered)
        #expect(store.search.error == nil)

        try #require(await waitForSearchQuery("query-b", probe: probe))
        probe.emitValue(query: "query-a")
        await Task.yield()
        #expect(
            store.search.value == nil,
            "an answer to the old query is not shown"
        )

        probe.emitError()
        await waitForStoreUpdate { store.search.error != nil }

        #expect(store.search.value == nil)
        #expect(!store.search.delivered)
        #expect(store.search.error != nil)
        #expect(
            probe.openCount == 1,
            "typing moves one search, never opens another"
        )
    }
}

@Suite("LibraryBrowseSession album projection")
struct LibraryBrowseSessionAlbumProjectionTests {
    @MainActor
    @Test(
        "the app-owned album list updates count while no library view is mounted"
    )
    func unmountedListUpdatesAlbumTotal() async {
        let probe = AlbumBrowseProbe()
        let store = LibraryStore()
        let session = LibraryBrowseSession(
            library: Library(
                albumBrowse: { _ in probe.query }
            ),
            libraryStore: store,
            uiStore: UiStore()
        )

        session.start()
        await waitForStoreUpdate { probe.isSubscribed }
        probe.emit(rows: [makeBridgeAlbum()], total: 1)
        await waitForStoreUpdate { store.albumTotal == 1 }

        #expect(store.albumTotal == 1)
    }

    @MainActor
    @Test("page eviction does not clear a selected album")
    func pageEvictionKeepsSelection() async {
        let pageProbe = AlbumBrowseProbe()
        let selectionProbe = AlbumSelectionProbe()
        let session = LibraryBrowseSession(
            library: Library(
                albumBrowse: { _ in pageProbe.query },
                albumSelection: { selectionProbe.query() }
            ),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )

        session.start()
        await waitForStoreUpdate { pageProbe.isSubscribed }
        pageProbe.emit(
            rows: [
                makeBridgeAlbum(id: "album-a"),
                makeBridgeAlbum(id: "album-b"),
            ],
            total: 2
        )
        await waitForStoreUpdate {
            session.albums.list?.totalCount == 2
        }
        session.albumSelection.toggle("album-a")
        #expect(selectionProbe.requested.last == ["album-a"])

        pageProbe.emit(rows: [makeBridgeAlbum(id: "album-c")], total: 3)
        await waitForStoreUpdate {
            session.albums.list?.totalCount == 3
        }

        #expect(session.albumSelection.contains("album-a"))
    }

    @MainActor
    @Test("a remote deletion clears the selected album")
    func remoteDeletionClearsSelection() async {
        let selectionProbe = AlbumSelectionProbe()
        let session = LibraryBrowseSession(
            library: Library(albumSelection: { selectionProbe.query() }),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )

        session.albumSelection.toggle("album-a")
        session.albumSelection.toggle("album-b")
        selectionProbe.emit(
            requested: ["album-a", "album-b"],
            albums: [makeBridgeAlbum(id: "album-b")]
        )
        await waitForStoreUpdate {
            !session.albumSelection.contains("album-a")
        }

        #expect(!session.albumSelection.contains("album-a"))
        #expect(session.albumSelection.contains("album-b"))
    }

    @MainActor
    @Test("changing the selection moves one read")
    func selectionMovesOneRead() async {
        let selectionProbe = AlbumSelectionProbe()
        let session = LibraryBrowseSession(
            library: Library(albumSelection: { selectionProbe.query() }),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )

        session.albumSelection.toggle("album-a")
        session.albumSelection.toggle("album-b")
        session.albumSelection.toggle("album-a")

        #expect(selectionProbe.opened == 1)
        #expect(
            selectionProbe.requested == [
                ["album-a"], ["album-a", "album-b"], ["album-b"],
            ]
        )
    }

    @MainActor
    @Test("ending the browse session cancels the selection read")
    func sessionEndCancelsObservation() async {
        let selectionProbe = AlbumSelectionProbe()
        var session: LibraryBrowseSession? = LibraryBrowseSession(
            library: Library(albumSelection: { selectionProbe.query() }),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )
        weak var weakSession = session

        session?.albumSelection.toggle("album-a")
        session = nil
        await waitForStoreUpdate { selectionProbe.cancelled }

        #expect(weakSession == nil)
        #expect(selectionProbe.cancelled)
    }
}

/// Stands in for the album-selection read: records each selection it is
/// pointed at, and answers with whatever a test emits.
private final class AlbumSelectionProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var openedCount = 0
    private var requests: [[String]] = []
    private var pending: [BridgeAlbumSelectionSnapshot] = []
    private var waiter:
        CheckedContinuation<BridgeAlbumSelectionSnapshot, any Error>?
    private var wasCancelled = false

    var opened: Int { lock.withLock { openedCount } }
    var requested: [[String]] { lock.withLock { requests } }
    var cancelled: Bool { lock.withLock { wasCancelled } }

    func query() -> AlbumSelectionQuery {
        lock.withLock { openedCount += 1 }
        return AlbumSelectionQuery(
            setAlbums: { [self] ids in
                lock.withLock { requests.append(ids) }
            },
            next: { [self] in
                try await withCheckedThrowingContinuation { continuation in
                    let ready: BridgeAlbumSelectionSnapshot? = lock.withLock {
                        if pending.isEmpty {
                            waiter = continuation
                            return nil
                        }
                        return pending.removeFirst()
                    }
                    if let ready { continuation.resume(returning: ready) }
                }
            },
            cancel: { [self] in
                lock.withLock { wasCancelled = true }
            }
        )
    }

    func emit(requested: [String], albums: [BridgeAlbum]) {
        let snapshot = BridgeAlbumSelectionSnapshot(
            requested: requested,
            albums: albums
        )
        let waiter:
            CheckedContinuation<BridgeAlbumSelectionSnapshot, any Error>? =
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

@Suite("LibraryStore album detail ownership")
struct LibraryStoreAlbumDetailOwnershipTests {
    @MainActor
    @Test("retry replaces the failed observation and rejects its late value")
    func retryRejectsOldObservation() async {
        let probe = AlbumDetailSubscriptionProbe()
        let store = LibraryStore()
        let library = Library(
            subscribeAlbumDetail: { _, callback in
                probe.subscribe(callback: callback)
            }
        )

        store.activateAlbumDetail(albumId: "album-1", library: library)
        await waitForStoreUpdate { probe.count == 1 }
        probe.emitError(subscription: 0)
        await waitForStoreUpdate {
            store.albumDetailErrors["album-1"] != nil
        }

        store.retryAlbumDetail(albumId: "album-1", library: library)
        await waitForStoreUpdate { probe.count == 2 }
        #expect(probe.isCancelled(subscription: 0))
        probe.emitValue(
            subscription: 1,
            value: makeBridgeAlbumDetail(title: "Replacement Title")
        )
        await waitForStoreUpdate {
            store.albumSummaries["album-1"]?.title == "Replacement Title"
        }
        probe.emitValue(
            subscription: 0,
            value: makeBridgeAlbumDetail(title: "Old Title")
        )
        await Task.yield()

        #expect(store.albumSummaries["album-1"]?.title == "Replacement Title")

        store.deactivateAlbumDetail(albumId: "album-1")
        #expect(probe.isCancelled(subscription: 1))
    }
}
