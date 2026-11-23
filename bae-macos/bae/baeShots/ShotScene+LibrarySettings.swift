import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Settings window on the Library pane as the boards draw it: for an
    /// admin (`mac.settings.library`), whose Ratings rows are pop-ups, and for
    /// a member (`mac.settings.library.member`), whose Ratings rows are plain
    /// values. A library with no storage, whose creator is its admin, draws
    /// its Sync row with Set up a cloud home… above its Ratings
    /// (`mac.settings.library.not-synced`).
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
        ShotScene(
            id: "mac.settings.library.not-synced",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(
                    shown: .fixtureNotSynced,
                    page: libraryPane(
                        LibrarySettingsTabPreviewFixture(
                            storage: nil,
                            libraryName: "Home music",
                            ratingSystems: BridgeRatingSystems(
                                albums: .stars,
                                tracks: .heart,
                                canChange: true
                            )
                        )
                    )
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
