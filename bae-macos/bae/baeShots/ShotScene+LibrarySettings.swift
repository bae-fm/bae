import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Settings window on the Library pane as the boards draw it: for an
    /// admin (`mac.settings.library`), whose Ratings rows are pop-ups, and for
    /// a member (`mac.settings.library.member`), whose Ratings rows are plain
    /// values.
    static let librarySettingsScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.library",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(
                    page: libraryPane(libraryFixture(admin: true))
                )
            )
        },
        ShotScene(
            id: "mac.settings.library.member",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(
                    page: libraryPane(libraryFixture(admin: false))
                )
            )
        },
    ]

    @MainActor
    private static func libraryFixture(admin: Bool)
        -> LibrarySettingsTabPreviewFixture
    {
        LibrarySettingsTabPreviewFixture(
            storage: PreviewData.previewDropboxStorage,
            selfIsAdmin: admin,
            libraryName: "Home music",
            ratingSystems: BridgeRatingSystems(
                albums: .stars,
                tracks: .heart,
                canChange: admin
            )
        )
    }
}
