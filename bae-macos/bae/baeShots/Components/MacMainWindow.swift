import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacMainWindow` around an empty section, where the preview draws a
    /// slot, with "Low Tide Hours" playing 1:42 into 5:03 or nothing played, and
    /// nothing in the Activity button.
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
            mainWindow(section: .library, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.sync-attention",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.sync-moving",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.sync-paused",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: true)
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
            mainWindow(section: .library, playing: true)
        },
    ]

    /// The main window on `section`, 1100 by 520 as the preview draws it,
    /// with nothing in the space the section fills.
    @MainActor
    private static func mainWindow(
        section: MainSection,
        playing: Bool
    ) -> ShotStage {
        let uiStore = storeShowing(section)
        let backing = boardLibraryBacking(uiStore: uiStore)
        return ShotStage(
            ShotScenes.libraryWindow(
                uiStore: uiStore,
                backing: backing,
                needsAttention: false,
                syncActivity: SyncActivityStore(),
                playbackStore: playing
                    ? boardPlayback() : PlaybackStore(),
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
                )
            )
        ) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
        }
    }
}
