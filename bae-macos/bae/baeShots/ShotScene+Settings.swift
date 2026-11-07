import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Settings panes: Playback with pausing between sides on, so its
    /// countdown shows under the switch; and the Library tab on a Dropbox
    /// shared folder, naming the folder coven recorded with Open in browser,
    /// its people reaching it by their accounts, and on an S3 bucket, its
    /// people reaching it by their keys.
    static let settingsScenes: [ShotScene] = [
        ShotScene(
            id: "settings-playback",
            size: CGSize(width: 500, height: 300)
        ) {
            ShotStage(
                PlaybackSettingsTab()
                    .environment(Playback.stub())
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            pauseBetweenSides: true
                        )
                    )
                    .environment(UiStore())
            )
        },
        librarySettings(
            "settings-sync-dropbox",
            storage: PreviewData.previewDropboxStorage
        ),
        librarySettings(
            "settings-sync-s3",
            storage: PreviewData.previewStorage
        ),
    ]

    /// The Library tab over a library whose cloud home is `storage`, once its
    /// name and people have answered.
    private static func librarySettings(
        _ id: String,
        storage: BridgeStorageLocation
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 640)) {
            let fixture = LibrarySettingsTabPreviewFixture(storage: storage)
            return ShotStage(librarySettingsTabPreview(fixture)) {
                fixture.answered
            }
        }
    }
}
