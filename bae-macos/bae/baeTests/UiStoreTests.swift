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
            case .composer("artist-1") = store.pendingLibraryNavigation?
                .target
        else {
            Issue.record("expected composer navigation target")
            return
        }
        #expect(store.pendingLibraryNavigation?.seq == 1)
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
        let first = store.pendingLibraryNavigation?.seq
        store.navigateToComposer("artist-1")
        let second = store.pendingLibraryNavigation?.seq

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

        guard case .work("work-1") = store.pendingLibraryNavigation?.target
        else {
            Issue.record("expected work navigation target")
            return
        }
        #expect(store.pendingLibraryNavigation?.seq == 1)
    }

    @Test("consumeLibraryNavigation clears the pending request it names")
    func consumeLibraryNavigationClearsMatchingRequest() {
        let store = UiStore()
        store.navigateToComposer("artist-1")
        let seq = store.pendingLibraryNavigation!.seq

        store.consumeLibraryNavigation(seq: seq)

        #expect(store.pendingLibraryNavigation == nil)
    }

    @Test(
        "consumeLibraryNavigation is a no-op against a superseded request"
    )
    func consumeLibraryNavigationIgnoresStaleSeq() {
        let store = UiStore()
        store.navigateToComposer("artist-1")
        let staleSeq = store.pendingLibraryNavigation!.seq
        store.navigateToWork("work-1")

        store.consumeLibraryNavigation(seq: staleSeq)

        guard case .work("work-1") = store.pendingLibraryNavigation?.target
        else {
            Issue.record("newer request should survive a stale consume")
            return
        }
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

    @Test(
        "navigateToAlbum sets an independent grid-reveal and track-flash command"
    )
    func navigateToAlbumSetsRevealRequest() {
        let store = UiStore()
        store.navigateToAlbum("album-1", trackId: "track-7")
        #expect(store.pendingAlbumReveal?.albumId == "album-1")
        #expect(store.pendingTrackFlash?.trackId == "track-7")
    }

    @Test("navigateToAlbum without a trackId clears any pending track flash")
    func navigateToAlbumWithoutTrackClearsPendingFlash() {
        let store = UiStore()
        store.navigateToAlbum("album-1", trackId: "track-7")
        store.navigateToAlbum("album-2")
        #expect(store.pendingTrackFlash == nil)
    }

    @Test("navigateToAlbum bumps the reveal seq (strictly increasing)")
    func navigateToAlbumBumpsSeq() {
        let store = UiStore()
        store.navigateToAlbum("album-1")
        let first = store.pendingAlbumReveal?.seq
        store.navigateToAlbum("album-2")
        let second = store.pendingAlbumReveal?.seq
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
        let first = store.pendingAlbumReveal?.seq
        store.navigateToAlbum("album-1")
        let second = store.pendingAlbumReveal?.seq
        #expect(first != second)
    }

    @Test("consumeAlbumReveal clears the pending reveal it names")
    func consumeAlbumRevealClearsMatchingReveal() {
        let store = UiStore()
        store.navigateToAlbum("album-1")
        let seq = store.pendingAlbumReveal!.seq

        store.consumeAlbumReveal(seq: seq)

        #expect(store.pendingAlbumReveal == nil)
    }

    @Test("consumeAlbumReveal is a no-op against a superseded reveal")
    func consumeAlbumRevealIgnoresStaleSeq() {
        let store = UiStore()
        store.navigateToAlbum("album-1")
        let staleSeq = store.pendingAlbumReveal!.seq
        store.navigateToAlbum("album-2")

        store.consumeAlbumReveal(seq: staleSeq)

        #expect(store.pendingAlbumReveal?.albumId == "album-2")
    }

    @Test(
        "consumeTrackFlash clears only the track flash, leaving the grid reveal pending"
    )
    func consumeTrackFlashDoesNotStarveGridReveal() {
        let store = UiStore()
        store.navigateToAlbum("album-1", trackId: "track-7")
        let seq = store.pendingTrackFlash!.seq

        store.consumeTrackFlash(seq: seq)

        #expect(store.pendingTrackFlash == nil)
        #expect(store.pendingAlbumReveal?.albumId == "album-1")
    }

    @Test(
        "consumeAlbumReveal clears only the grid reveal, leaving the track flash pending"
    )
    func consumeAlbumRevealDoesNotStarveTrackFlash() {
        let store = UiStore()
        store.navigateToAlbum("album-1", trackId: "track-7")
        let seq = store.pendingAlbumReveal!.seq

        store.consumeAlbumReveal(seq: seq)

        #expect(store.pendingAlbumReveal == nil)
        #expect(store.pendingTrackFlash?.trackId == "track-7")
    }
}
