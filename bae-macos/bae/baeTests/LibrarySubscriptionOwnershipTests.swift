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
    @Test("a remote page deletion clears the deleted album selection")
    func remoteDeletionClearsSelection() async {
        let probe = AlbumPageSubscriptionProbe()
        let session = LibraryBrowseSession(
            library: Library(
                subscribeAlbumPage: { _, _, _, callback in
                    probe.subscribe(callback: callback)
                }
            ),
            libraryStore: LibraryStore(),
            uiStore: UiStore()
        )

        session.start()
        await waitForStoreUpdate { probe.isSubscribed }
        probe.emit(
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

        probe.emit(rows: [makeBridgeAlbum(id: "album-b")], total: 1)
        await waitForStoreUpdate {
            session.albums.list?.totalCount == 1
        }

        #expect(!session.albumSelection.contains("album-a"))
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
