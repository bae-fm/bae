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

    @Test("navigateToAlbum records the release override")
    func navigateToAlbumRecordsReleaseOverride() {
        let store = UiStore()
        store.navigateToAlbum("album-1", releaseId: "release-9")
        #expect(store.selectedReleaseIdByAlbum["album-1"] == "release-9")
    }
}
