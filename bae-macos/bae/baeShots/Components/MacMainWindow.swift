import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacMainWindow` around the library's albums, or the Import section,
    /// with "Low Tide Hours" playing 1:42 into 5:03 or nothing played, and
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
    ]

    /// The main window on `section`, 1100 by 520 as the preview draws it.
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
                playbackStore: playing ? boardPlayback() : PlaybackStore(),
                size: CGSize(width: 1100, height: 520)
            )
        ) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
        }
    }
}
