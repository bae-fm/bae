import BaeKit
import Foundation
import Testing

@testable import bae

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

/// Stands in for the live library search: records each query it is pointed
/// at and each time it is opened, and answers with whatever a test emits.
private final class SearchProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var queries: [String] = []
    private var opened = 0
    private var askedCount = 0
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
    /// How many times the search has been asked for a value. The store takes
    /// one value at a time and applies it before asking again, so an ask past
    /// a delivered value means that value has been handled, applied or not.
    var asks: Int { lock.withLock { askedCount } }

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
                askedCount += 1
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
        try await Wait.until { probe.lastQuery == "query-a" }
        probe.emitValue(query: "query-a")
        try await Wait.until { store.search.value?.query == "query-a" }
        #expect(store.search.delivered)

        store.activateSearch("query-b")

        #expect(store.search.value == nil)
        #expect(!store.search.delivered)
        #expect(store.search.error == nil)

        try await Wait.until { probe.lastQuery == "query-b" }
        // The search asked once on opening and once past the first answer;
        // its third ask comes only after it has handled this stale one.
        probe.emitValue(query: "query-a")
        try await Wait.until { probe.asks >= 3 }
        #expect(
            store.search.value == nil,
            "an answer to the old query is not shown"
        )

        probe.emitError()
        try await Wait.until { store.search.error != nil }

        #expect(store.search.value == nil)
        #expect(!store.search.delivered)
        #expect(store.search.error != nil)
        #expect(
            probe.openCount == 1,
            "typing moves one search, never opens another"
        )
    }
}

@Suite("LibraryProjectionStore details")
struct LibraryProjectionStoreDetailTests {
    @MainActor
    @Test(
        "moving the composer pane moves its one read, and clearing it reads nothing"
    )
    func composerPaneMovesOneRead() async throws {
        let feed = DetailFeed<BridgeComposerDetail>()
        let store = LibraryProjectionStore(
            library: Library(composerDetail: { feed.query() })
        )

        store.activateComposer("composer-1")
        store.activateComposer("composer-2")
        feed.emit(id: "composer-1", value: nil)
        feed.emit(id: "composer-2", value: nil)
        try await Wait.until { store.composer.delivered }

        #expect(feed.opened == 1)
        #expect(feed.requested == ["composer-1", "composer-2"])

        store.deactivateComposer("composer-1")
        #expect(
            feed.requested.count == 2,
            "a composer no longer shown is not the one read"
        )
        store.deactivateComposer("composer-2")
        store.activateComposer("composer-3")

        #expect(
            feed.requested == ["composer-1", "composer-2", nil, "composer-3"]
        )
        #expect(feed.opened == 1)
        #expect(!feed.isCancelled(read: 0))
    }
}

@Suite("LibraryBrowseSession album projection")
struct LibraryBrowseSessionAlbumProjectionTests {
    @MainActor
    @Test(
        "the app-owned album list updates count while no library view is mounted"
    )
    func unmountedListUpdatesAlbumTotal() async throws {
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
        try await Wait.until { probe.isSubscribed }
        probe.emit(rows: [makeBridgeAlbum()], total: 1)
        try await Wait.until { store.albumTotal == 1 }

        #expect(store.albumTotal == 1)
    }

    @MainActor
    @Test("page eviction does not clear a selected album")
    func pageEvictionKeepsSelection() async throws {
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
        try await Wait.until { pageProbe.isSubscribed }
        pageProbe.emit(
            rows: [
                makeBridgeAlbum(id: "album-a"),
                makeBridgeAlbum(id: "album-b"),
            ],
            total: 2
        )
        try await Wait.until {
            session.albums.list?.totalCount == 2
        }
        session.albumSelection.toggle("album-a")
        #expect(selectionProbe.requested.last == ["album-a"])

        pageProbe.emit(rows: [makeBridgeAlbum(id: "album-c")], total: 3)
        try await Wait.until {
            session.albums.list?.totalCount == 3
        }

        #expect(session.albumSelection.contains("album-a"))
    }

    @MainActor
    @Test("a remote deletion clears the selected album")
    func remoteDeletionClearsSelection() async throws {
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
        try await Wait.until {
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
    func sessionEndCancelsObservation() async throws {
        let selectionProbe = AlbumSelectionProbe()
        var session: LibraryBrowseSession? = LibraryBrowseSession(
            library: Library(albumSelection: { selectionProbe.query() }),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )
        weak var weakSession = session

        session?.albumSelection.toggle("album-a")
        session = nil
        try await Wait.until { selectionProbe.cancelled }

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
    @Test("retry replaces the failed read and rejects its late value")
    func retryRejectsOldRead() async throws {
        let feed = DetailFeed<BridgeAlbumDetail>()
        let store = LibraryStore()
        let reader = store.albumDetailReader(
            library: Library(albumDetail: { feed.query() })
        )

        reader.show("album-1")
        feed.emitError()
        try await Wait.until {
            store.albumDetailErrors["album-1"] != nil
        }

        reader.retry()
        try await Wait.until { feed.isCancelled(read: 0) }
        #expect(feed.opened == 2)
        feed.emit(
            read: 1,
            id: "album-1",
            value: makeBridgeAlbumDetail(title: "Replacement Title")
        )
        try await Wait.until {
            store.albumSummaries["album-1"]?.title == "Replacement Title"
        }
        feed.emit(
            read: 0,
            id: "album-1",
            value: makeBridgeAlbumDetail(title: "Old Title")
        )
        // Read 0 was cancelled above and its loop has ended, so nothing will
        // ever take this value: the check below is final, not early.

        #expect(store.albumSummaries["album-1"]?.title == "Replacement Title")

        reader.close()
        try await Wait.until { feed.isCancelled(read: 1) }
        #expect(feed.isCancelled(read: 1))
    }
}
