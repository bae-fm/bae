import Testing

@testable import bae

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

    @Test("navigateToComposer records a durable navigation request")
    func navigateToComposerRecordsNavigationRequest() {
        let store = UiStore()
        store.navigateToComposer("artist-1")

        guard
            case .composer("artist-1") = store.libraryNavigationRequest?.target
        else {
            Issue.record("expected composer navigation target")
            return
        }
        #expect(store.libraryNavigationRequest?.seq == 1)
    }

    @Test("navigateToWork switches to composers mode")
    func navigateToWorkSwitchesMode() {
        let store = UiStore()
        store.navigateToWork("work-1")
        #expect(store.libraryBrowserMode == .composers)
    }

    @Test("repeat composer navigation records a fresh request")
    func repeatComposerNavigationRecordsFreshRequest() {
        let store = UiStore()
        store.navigateToComposer("artist-1")
        let first = store.libraryNavigationRequest?.seq
        store.navigateToComposer("artist-1")
        let second = store.libraryNavigationRequest?.seq

        guard let first, let second else {
            Issue.record("expected navigation sequence values")
            return
        }
        #expect(second > first)
    }

    @Test("navigateToWork records a durable navigation request")
    func navigateToWorkRecordsNavigationRequest() {
        let store = UiStore()
        store.navigateToWork("work-1")

        guard case .work("work-1") = store.libraryNavigationRequest?.target
        else {
            Issue.record("expected work navigation target")
            return
        }
        #expect(store.libraryNavigationRequest?.seq == 1)
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

    @Test("navigateToAlbum sets the reveal request with albumId and trackId")
    func navigateToAlbumSetsRevealRequest() {
        let store = UiStore()
        store.navigateToAlbum("album-1", trackId: "track-7")
        #expect(store.albumReveal?.albumId == "album-1")
        #expect(store.albumReveal?.trackId == "track-7")
    }

    @Test("navigateToAlbum bumps the reveal seq (strictly increasing)")
    func navigateToAlbumBumpsSeq() {
        let store = UiStore()
        store.navigateToAlbum("album-1")
        let first = store.albumReveal?.seq
        store.navigateToAlbum("album-2")
        let second = store.albumReveal?.seq
        #expect(first != nil)
        #expect(second != nil)
        #expect(second! > first!)
    }

    @Test(
        "repeat navigation to the same album produces two distinct seq values"
    )
    func navigateToSameAlbumTwiceRefires() {
        let store = UiStore()
        store.navigateToAlbum("album-1")
        let first = store.albumReveal?.seq
        store.navigateToAlbum("album-1")
        let second = store.albumReveal?.seq
        #expect(first != second)
    }
}
