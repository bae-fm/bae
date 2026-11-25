import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacMainWindow` around an empty section, where the preview draws a
    /// slot, with "Low Tide Hours" playing 1:42 into 5:03 or nothing played, and
    /// nothing in the Activity button unless `activity` gives it something to
    /// show. With `waitsOnStorage`, the track waits on a download and the bar
    /// draws the waiting arc.
    static let macMainWindowScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.main-window.library",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.import",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .importing, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.nothing-played",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: false)
        },
        ShotScene(
            id: "mac.component.main-window.queue-open",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.search-focused",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(
                section: .library,
                playing: true,
                searchFocused: true
            )
        },
        ShotScene(
            id: "mac.component.main-window.sync-attention",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(
                section: .library,
                playing: true,
                activity: attentionActivity
            )
        },
        ShotScene(
            id: "mac.component.main-window.sync-moving",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(
                section: .library,
                playing: true,
                activity: movingActivity
            )
        },
        ShotScene(
            id: "mac.component.main-window.sync-paused",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(
                section: .library,
                playing: true,
                activity: pausedActivity
            )
        },
        ShotScene(
            id: "mac.component.main-window.track-no-cover",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.waiting",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: true, waitsOnStorage: true)
        },
    ]

    /// The main window on `section`, 1100 by 520 as the preview draws it,
    /// with nothing in the space the section fills.
    @MainActor
    private static func mainWindow(
        section: MainSection,
        playing: Bool,
        waitsOnStorage: Bool = false,
        activity: BridgeSyncActivity? = nil,
        searchFocused: Bool = false
    ) -> ShotStage {
        let uiStore = storeShowing(section)
        let backing = boardLibraryBacking(uiStore: uiStore)
        let images = coverGalleryImages()
        return ShotStage(
            ShotScenes.libraryWindow(
                uiStore: uiStore,
                backing: backing,
                playbackStore: playing
                    ? boardPlayback(waitsOnStorage: waitsOnStorage)
                    : nothingPlayed(),
                images: images,
                size: CGSize(width: 1100, height: 520),
                window: AnyView(
                    MacMainWindow(showSidePane: false) {
                        MacTitleBar(
                            searchText: .constant(""),
                            onSearchFieldFrame: { _ in }
                        )
                    } content: {
                        Color.clear
                    } queuePanel: {
                        EmptyView()
                    } nowPlayingBar: {
                        MacNowPlayingBarContainer(onDropToQueue: { _ in })
                    }
                    .environment(activityStore(activity))
                )
            ),
            images: images
        ) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
        } isDrawn: { host in
            // A window that becomes key gives its first field the keyboard,
            // which draws the search field focused: every board but the
            // focused one draws it at rest.
            if !searchFocused { host.window?.makeFirstResponder(nil) }
            return true
        }
    }

    /// A store with nothing played and the settings core always reports: the
    /// bar draws its Repeat, Mute and Volume.
    @MainActor
    private static func nothingPlayed() -> PlaybackStore {
        let store = PlaybackStore()
        store.settings = PlaybackSettings(
            volume: 0.7,
            isMuted: false,
            repeatMode: .off
        )
        return store
    }
}
