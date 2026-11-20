import BaeKit
import Combine
import Foundation
import Testing

@testable import bae

@Suite("UiStore import folder picker")
struct UiStoreImportFolderPickerTests {
    @Test("duplicate requests share one active presentation")
    func duplicateRequestsShareOnePresentation() {
        let store = UiStore()

        #expect(!store.isImportFolderPickerPresented)
        store.setImportFolderPickerPresented(true)
        store.setImportFolderPickerPresented(true)

        #expect(store.isImportFolderPickerPresented)

        store.setImportFolderPickerPresented(false)
        store.setImportFolderPickerPresented(true)

        #expect(store.isImportFolderPickerPresented)
    }
}

@Suite("UiStore.libraryBrowserMode")
struct UiStoreLibraryBrowserModeTests {
    @Test("setLibraryBrowserMode sets the given value (absolute, idempotent)")
    func setModeIsAbsoluteAndIdempotent() {
        let store = UiStore()
        #expect(store.libraryBrowserMode == .albums)

        store.setLibraryBrowserMode(.composers)
        #expect(store.libraryBrowserMode == .composers)

        store.setLibraryBrowserMode(.composers)
        #expect(store.libraryBrowserMode == .composers)

        store.setLibraryBrowserMode(.artists)
        #expect(store.libraryBrowserMode == .artists)

        store.setLibraryBrowserMode(.albums)
        #expect(store.libraryBrowserMode == .albums)
    }

    @Test("navigateToComposer switches to composers mode and library section")
    func navigateToComposerSwitchesMode() {
        let store = UiStore()
        store.navigateToComposer("artist-1")
        #expect(store.libraryBrowserMode == .composers)
        #expect(store.activeSection == .library)
    }

    @Test("navigateToWork switches to composers mode")
    func navigateToWorkSwitchesMode() {
        let store = UiStore()
        store.navigateToWork("work-1")
        #expect(store.libraryBrowserMode == .composers)
    }

    @Test("navigateToLibraryRoot does not change libraryBrowserMode")
    func navigateToLibraryRootPreservesMode() {
        let store = UiStore()
        store.setLibraryBrowserMode(.composers)
        store.navigateToLibraryRoot()
        #expect(store.libraryBrowserMode == .composers)
        #expect(store.activeSection == .library)
    }

    @Test("navigateToAlbum switches to albums mode and library section")
    func navigateToAlbumSwitchesToAlbumsMode() {
        let store = UiStore()
        store.setLibraryBrowserMode(.composers)
        store.navigateToAlbum("album-1")
        #expect(store.libraryBrowserMode == .albums)
        #expect(store.activeSection == .library)
        #expect(store.selectedAlbumId == "album-1")
    }

    @Test("navigateToAlbum from artists mode switches to albums mode")
    func navigateToAlbumFromArtistsSwitchesToAlbumsMode() {
        let store = UiStore()
        store.setLibraryBrowserMode(.artists)
        store.navigateToAlbum("album-1")
        #expect(store.libraryBrowserMode == .albums)
        #expect(store.selectedAlbumId == "album-1")
    }

    @Test("navigateToAlbum records the release override")
    func navigateToAlbumRecordsReleaseOverride() {
        let store = UiStore()
        store.navigateToAlbum("album-1", releaseId: "release-9")
        #expect(store.selectedReleaseIdByAlbum["album-1"] == "release-9")
    }

    @Test("navigateToAlbum sends the album and the track in it to the grid")
    @MainActor
    func navigateToAlbumSendsReveal() async throws {
        let store = UiStore()
        var received: [AlbumReveal] = []
        let listening = store.albumReveals.requests.sink {
            received.append($0)
            Wait.happened()
        }
        defer { listening.cancel() }

        store.navigateToAlbum("album-1", trackId: "track-7")

        try await Wait.until {
            received == [AlbumReveal(albumId: "album-1", trackId: "track-7")]
        }
    }

    @Test("flashTrack sends the track to the rows")
    func flashTrackSendsTrack() {
        let store = UiStore()
        var flashed: [String] = []
        let subscription = store.trackFlashes.sink { flashed.append($0) }
        defer { subscription.cancel() }

        store.flashTrack("track-7")
        store.flashTrack("track-7")

        #expect(flashed == ["track-7", "track-7"])
    }
}

@MainActor
@Suite("Library navigation in the browse session")
struct LibraryNavigationTests {
    @Test("a navigation opens the artist, composer or work it names")
    func navigationOpensItsTarget() async throws {
        let store = UiStore()
        let session = LibraryBrowseSession(library: .stub(), uiStore: store)

        store.navigateToComposer("artist-1")
        try await Wait.until {
            session.detailSelection
                == .composer(artistId: "artist-1", workId: nil)
        }
        store.navigateToWork("work-1")
        try await Wait.until {
            session.detailSelection == .work(workId: "work-1")
        }
        store.navigateToArtist("artist-2")
        try await Wait.until { session.selectedArtistId == "artist-2" }
    }

    @Test("asking again for the composer open on a work opens its overview")
    func repeatComposerNavigationOpensOverview() async throws {
        let store = UiStore()
        let session = LibraryBrowseSession(library: .stub(), uiStore: store)
        store.navigateToComposer("artist-1")
        try await Wait.until { session.detailSelection != .none }
        session.selectComposerWork(artistId: "artist-1", workId: "work-1")

        store.navigateToComposer("artist-1")

        try await Wait.until {
            session.detailSelection
                == .composer(artistId: "artist-1", workId: nil)
        }
    }

    @Test("a navigation sent before the session exists waits for it")
    func navigationWaitsForTheSession() async throws {
        let store = UiStore()
        store.navigateToWork("work-1")

        let session = LibraryBrowseSession(library: .stub(), uiStore: store)

        try await Wait.until {
            session.detailSelection == .work(workId: "work-1")
        }
    }
}

/// What one listener to held requests has received so far.
final class RequestListener<Request> {
    private(set) var received: [Request] = []
    private var subscription: AnyCancellable?

    init(_ requests: HeldRequests<Request>) {
        subscription = requests.requests.sink { [unowned self] in
            received.append($0)
            Wait.happened()
        }
    }

    func stop() {
        subscription?.cancel()
    }
}

@MainActor
@Suite("HeldRequests")
struct HeldRequestsTests {
    private typealias Listener = RequestListener<AlbumReveal>
    private let first = AlbumReveal(albumId: "album-1", trackId: "track-7")
    private let second = AlbumReveal(albumId: "album-2", trackId: nil)

    @Test("a listener gets each request as it comes")
    func listenerGetsRequests() async throws {
        let requests = HeldRequests<AlbumReveal>()
        let listener = Listener(requests)
        defer { listener.stop() }

        requests.send(first)
        requests.send(second)

        try await Wait.until { listener.received == [first, second] }
    }

    @Test(
        "the latest request sent with no listener goes to the next one, once"
    )
    func waitingRequestGoesToNextListenerOnce() async throws {
        let requests = HeldRequests<AlbumReveal>()
        requests.send(first)
        requests.send(second)

        let taker = Listener(requests)
        defer { taker.stop() }
        try await Wait.until { taker.received == [second] }

        // The next listener finds nothing waiting: the first request it
        // gets is the one sent after it began to listen.
        let later = Listener(requests)
        defer { later.stop() }
        requests.send(first)
        try await Wait.until { later.received == [first] }
        #expect(taker.received == [second, first])
    }

    @Test("a request sent once every listener stopped waits for the next")
    func requestAfterListenerStopsWaits() async throws {
        let requests = HeldRequests<AlbumReveal>()
        let stopped = Listener(requests)
        stopped.stop()

        requests.send(second)

        let next = Listener(requests)
        defer { next.stop() }
        try await Wait.until { next.received == [second] }
        #expect(stopped.received.isEmpty)
    }
}

@Suite("UiStore import queue interactions")
struct UiStoreImportQueueInteractionTests {
    @Test("refresh state is set and cleared by root")
    func refreshStateIsPerRoot() {
        let store = UiStore()
        store.setWatchedFolderRefreshing(Data("/music/first".utf8), true)
        store.setWatchedFolderRefreshing(Data("/music/second".utf8), true)
        store.setWatchedFolderRefreshing(Data("/music/first".utf8), false)

        #expect(
            !store.refreshingWatchedFolders.contains(Data("/music/first".utf8))
        )
        #expect(
            store.refreshingWatchedFolders.contains(Data("/music/second".utf8))
        )
    }
}

@Suite("UiStore new playlist sheet")
struct UiStoreNewPlaylistSheetTests {
    @Test("the New playlist sheet opens on Only me, holding the tracks given")
    func opensOnOnlyMeWithItsTracks() {
        let store = UiStore()
        #expect(store.newPlaylistSheet == nil)

        store.presentNewPlaylist(pickedTrackIds: ["track-a", "track-b"])

        #expect(store.newPlaylistSheet?.sharing == .private)
        #expect(store.newPlaylistSheet?.name == "")
        #expect(
            store.newPlaylistSheet?.pickedTrackIds == ["track-a", "track-b"]
        )
    }
}

@Suite("UiStore side pane")
struct UiStoreSidePaneTests {
    @Test("opening a playlist shows the side pane on Playlists with it open")
    func openingAPlaylistShowsItInThePane() {
        let store = UiStore()
        store.openPlaylist("playlist-a")
        #expect(store.showSidePane)
        #expect(store.sidePaneTab == .playlists)
        #expect(store.openPlaylistId == "playlist-a")
    }

    @Test("the Playlists tab keeps its open playlist across tabs and closing")
    func theOpenPlaylistOutlivesTabsAndClosing() {
        let store = UiStore()
        store.openPlaylist("playlist-a")
        store.selectSidePaneTab(.queue)
        store.setSidePanePresented(false)
        store.openSidePane(.playlists)
        #expect(store.openPlaylistId == "playlist-a")
    }

    @Test("deleting another playlist keeps the open one; deleting it goes back")
    func deletingTheOpenPlaylistGoesBackToTheList() {
        let store = UiStore()
        store.openPlaylist("playlist-a")
        store.closePlaylist("playlist-b")
        #expect(store.openPlaylistId == "playlist-a")
        store.closePlaylist("playlist-a")
        #expect(store.openPlaylistId == nil)
    }
}
