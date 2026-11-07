import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Settings panes: Playback with pausing between sides on, so its
    /// countdown shows under the switch; Casting on, with the receivers it
    /// found and still looking for the first; and the Library tab on a
    /// Dropbox shared folder, naming the folder coven recorded with Open in
    /// browser, its people reaching it by their accounts, and on an S3
    /// bucket, its people reaching it by their keys.
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
        castingSettings(
            "settings-casting",
            devices: [
                BridgeCastDevice(
                    id: "cast-den",
                    name: "Den receiver",
                    kind: .dlna
                ),
                BridgeCastDevice(
                    id: "cast-kitchen",
                    name: "Kitchen",
                    kind: .airPlay
                ),
                BridgeCastDevice(
                    id: "cast-living-room",
                    name: "Living Room",
                    kind: .cast
                ),
            ]
        ),
        castingSettings("settings-casting-looking", devices: []),
        librarySettings(
            "settings-sync-dropbox",
            storage: PreviewData.previewDropboxStorage
        ),
        librarySettings(
            "settings-sync-s3",
            storage: PreviewData.previewStorage
        ),
    ]

    /// The Casting tab with casting on and `devices` found.
    private static func castingSettings(
        _ id: String,
        devices: [BridgeCastDevice]
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 600)) {
            let castStore = CastStore()
            castStore.devices = devices
            return ShotStage(
                CastingSettingsTab()
                    .environment(Cast.stub())
                    .environment(castStore)
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            castEnabled: true
                        )
                    )
                    .environment(UiStore())
            )
        }
    }

    /// The Library tab over a library whose storage is `storage`, once its
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
