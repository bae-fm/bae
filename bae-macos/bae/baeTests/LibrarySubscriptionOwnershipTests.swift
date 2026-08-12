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

private final class AlbumPageSubscriptionProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var callback: AlbumPageCallback?

    func subscribe(callback: AlbumPageCallback) -> any LiveSubscriptionProtocol
    {
        lock.withLock { self.callback = callback }
        return AlbumProbeSubscription()
    }

    func emit(rows: [BridgeAlbum], total: UInt64) {
        let callback = lock.withLock { self.callback }
        callback?.onValue(value: BridgeAlbumPage(rows: rows, totalCount: total))
    }

    var isSubscribed: Bool {
        lock.withLock { callback != nil }
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

private final class SearchSubscriptionProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var callbacks: [LibrarySearchCallback] = []

    func subscribe(callback: LibrarySearchCallback)
        -> any LiveSubscriptionProtocol
    {
        lock.withLock { callbacks.append(callback) }
        return SearchProbeSubscription()
    }

    func emitValue(subscription: Int) {
        let callback = lock.withLock { callbacks[subscription] }
        callback.onValue(
            value: BridgeSearchResults(
                albums: [],
                artists: [],
                tracks: [],
                composers: [],
                works: []
            )
        )
    }

    func emitError(subscription: Int) {
        let callback = lock.withLock { callbacks[subscription] }
        callback.onError(
            error: .Diagnostic(
                category: .internal,
                detail: "search failed"
            )
        )
    }

    var count: Int { lock.withLock { callbacks.count } }
}

private final class SearchProbeSubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
    func cancel() {}
}

private func waitForSearchSubscription(
    _ expectedCount: Int,
    probe: SearchSubscriptionProbe
) async throws -> Bool {
    for _ in 0..<100 {
        guard probe.count < expectedCount else { return true }
        try await Task.sleep(for: .milliseconds(10))
    }
    return false
}

@Suite("LibraryProjectionStore search")
struct LibraryProjectionStoreSearchTests {
    @MainActor
    @Test("a new query clears the previous query before its first result")
    func queryChangeClearsPreviousState() async throws {
        let probe = SearchSubscriptionProbe()
        let store = LibraryProjectionStore(
            library: Library(
                subscribeLibrarySearch: { _, callback in
                    probe.subscribe(callback: callback)
                }
            )
        )

        store.activateSearch("query-a")
        try #require(await waitForSearchSubscription(1, probe: probe))
        probe.emitValue(subscription: 0)
        await waitForStoreUpdate { store.search.value?.query == "query-a" }
        #expect(store.search.delivered)

        store.activateSearch("query-b")

        #expect(store.search.value == nil)
        #expect(!store.search.delivered)
        #expect(store.search.error == nil)

        try #require(await waitForSearchSubscription(2, probe: probe))
        probe.emitError(subscription: 1)
        await waitForStoreUpdate { store.search.error != nil }

        #expect(store.search.value == nil)
        #expect(!store.search.delivered)
        #expect(store.search.error != nil)
    }
}

@Suite("LibraryBrowseSession album projection")
struct LibraryBrowseSessionAlbumProjectionTests {
    @MainActor
    @Test(
        "the app-owned album list updates count while no library view is mounted"
    )
    func unmountedListUpdatesAlbumTotal() async {
        let probe = AlbumPageSubscriptionProbe()
        let store = LibraryStore()
        let session = LibraryBrowseSession(
            library: Library(
                subscribeAlbumPage: { _, _, _, callback in
                    probe.subscribe(callback: callback)
                }
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
        let pageProbe = AlbumPageSubscriptionProbe()
        let detailProbe = AlbumDetailSubscriptionProbe()
        let session = LibraryBrowseSession(
            library: Library(
                subscribeAlbumPage: { _, _, _, callback in
                    pageProbe.subscribe(callback: callback)
                },
                subscribeAlbumDetail: { _, callback in
                    detailProbe.subscribe(callback: callback)
                }
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
        await waitForStoreUpdate { detailProbe.count == 1 }
        guard detailProbe.count == 1 else {
            Issue.record("selection did not start its album observation")
            return
        }

        pageProbe.emit(rows: [makeBridgeAlbum(id: "album-c")], total: 3)
        await waitForStoreUpdate {
            session.albums.list?.totalCount == 3
        }

        #expect(session.albumSelection.contains("album-a"))
    }

    @MainActor
    @Test("a remote deletion clears the selected album")
    func remoteDeletionClearsSelection() async {
        let pageProbe = AlbumPageSubscriptionProbe()
        let detailProbe = AlbumDetailSubscriptionProbe()
        let session = LibraryBrowseSession(
            library: Library(
                subscribeAlbumPage: { _, _, _, callback in
                    pageProbe.subscribe(callback: callback)
                },
                subscribeAlbumDetail: { _, callback in
                    detailProbe.subscribe(callback: callback)
                }
            ),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )

        session.start()
        await waitForStoreUpdate { pageProbe.isSubscribed }
        pageProbe.emit(rows: [makeBridgeAlbum(id: "album-a")], total: 1)
        await waitForStoreUpdate { session.albums.list?.totalCount == 1 }
        session.albumSelection.toggle("album-a")
        await waitForStoreUpdate { detailProbe.count == 1 }
        guard detailProbe.count == 1 else {
            Issue.record("selection did not start its album observation")
            return
        }

        detailProbe.emitValue(subscription: 0, value: nil)
        await waitForStoreUpdate {
            !session.albumSelection.contains("album-a")
        }

        #expect(!session.albumSelection.contains("album-a"))
    }

    @MainActor
    @Test("deselecting an album cancels its exact observation")
    func deselectionCancelsObservation() async {
        let detailProbe = AlbumDetailSubscriptionProbe()
        let session = LibraryBrowseSession(
            library: Library(
                subscribeAlbumDetail: { _, callback in
                    detailProbe.subscribe(callback: callback)
                }
            ),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )

        session.albumSelection.toggle("album-a")
        await waitForStoreUpdate { detailProbe.count == 1 }
        session.albumSelection.toggle("album-a")
        await waitForStoreUpdate {
            detailProbe.isCancelled(subscription: 0)
        }

        #expect(detailProbe.isCancelled(subscription: 0))
    }

    @MainActor
    @Test("ending the browse session cancels selected album observations")
    func sessionEndCancelsObservation() async {
        let detailProbe = AlbumDetailSubscriptionProbe()
        var session: LibraryBrowseSession? = LibraryBrowseSession(
            library: Library(
                subscribeAlbumDetail: { _, callback in
                    detailProbe.subscribe(callback: callback)
                }
            ),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )
        weak var weakSession = session

        session?.albumSelection.toggle("album-a")
        await waitForStoreUpdate { detailProbe.count == 1 }
        session = nil
        await Task.yield()

        #expect(weakSession == nil)
        #expect(detailProbe.isCancelled(subscription: 0))
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
