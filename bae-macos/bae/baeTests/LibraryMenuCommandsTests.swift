import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The main menu's commands for the open library, which the primary
/// window's controller takes: disabled without the library, and enabled and
/// checked from what it holds when AppKit asks.
@MainActor
@Suite("Library menu commands")
struct LibraryMenuCommandsTests {
    @Test("a library's commands are disabled without its library")
    func libraryCommandsNeedTheLibrary() throws {
        let menu = try #require(NSApp.mainMenu)
        let controller = MainWindowController(
            appDelegate: AppDelegate(runtime: .testHost),
            library: { nil },
            onClose: {}
        )
        for action in [
            "playPause:", "closeLibrary:", "importFolder:", "renameLibrary:",
            "chooseLibraryBrowserMode:", "fullWidthLibrary:", "focusSearch:",
            "goToNowPlaying:", "chooseRepeatMode:", "shuffleLibrary:",
        ] {
            #expect(
                !controller.validateMenuItem(try MenuLookup.item(menu, action)),
                "\(action)"
            )
        }
    }

    @Test("a library's commands are enabled and checked from what it holds")
    func libraryCommandsFollowTheLibrary() async throws {
        let menu = try #require(NSApp.mainMenu)
        let delegate = AppDelegate(runtime: .testHost)
        let target = Self.target(
            fullWidth: true,
            pauseBetweenSides: false,
            selfIsAdmin: false
        )
        let controller = MainWindowController(
            appDelegate: delegate,
            library: { target },
            onClose: {}
        )
        func valid(_ action: String) throws -> Bool {
            controller.validateMenuItem(try MenuLookup.item(menu, action))
        }

        #expect(try valid("playPause:"))
        #expect(try valid("closeLibrary:"))
        #expect(try !valid("renameLibrary:"), "only an admin renames")
        #expect(try !valid("goToNowPlaying:"), "nothing is playing")

        // Not counted yet, then counted.
        #expect(try !valid("shuffleLibrary:"))
        target.libraryBrowseSession.start()
        try await Wait.until { target.libraryBrowseSession.albumTotal == 3 }
        #expect(try valid("shuffleLibrary:"))

        let fullWidth = try MenuLookup.item(menu, "fullWidthLibrary:")
        #expect(controller.validateMenuItem(fullWidth))
        #expect(fullWidth.state == .on)
        let pause = try MenuLookup.item(menu, "pauseBetweenSides:")
        #expect(controller.validateMenuItem(pause))
        #expect(pause.state == .off)

        target.playbackStore.repeatMode = .track
        let repeatModes = MenuLookup.items(menu, "chooseRepeatMode:")
        for item in repeatModes {
            #expect(controller.validateMenuItem(item))
        }
        #expect(repeatModes.map(\.state) == [.off, .off, .on])

        target.uiStore.setLibraryBrowserMode(.artists)
        let modes = MenuLookup.items(menu, "chooseLibraryBrowserMode:")
        for item in modes {
            #expect(controller.validateMenuItem(item))
        }
        #expect(modes.map(\.state) == [.off, .on, .off])

        // The title bar's search field shows only with the library.
        delegate.screen = .loading
        #expect(try !valid("focusSearch:"))
        delegate.screen = .library
        #expect(try valid("focusSearch:"))
    }

    private static func target(
        fullWidth: Bool,
        pauseBetweenSides: Bool,
        selfIsAdmin: Bool
    ) -> MainAppMenuTarget {
        let uiStore = UiStore()
        let albums = (0..<3).map { makeBridgeAlbum(id: "album-\($0)") }
        return MainAppMenuTarget(
            playbackStore: PlaybackStore(),
            configStore: PreviewData.makeConfigStore(
                libraryFullWidth: fullWidth,
                pauseBetweenSides: pauseBetweenSides
            ),
            libraryBrowseSession: LibraryBrowseSession(
                library: Library(albumBrowse: { _, _ in .fixed(albums) }),
                uiStore: uiStore
            ),
            uiStore: uiStore,
            library: Library.stub(),
            playback: Playback.stub(),
            membershipStore: MembershipStore.stub(
                people: [],
                selfIsAdmin: selfIsAdmin
            )
        )
    }
}
