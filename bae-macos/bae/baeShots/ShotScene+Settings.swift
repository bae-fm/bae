import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Settings panes: Playback with pausing between sides on, so its
    /// countdown shows under the switch; Casting on, with the receivers it
    /// found and still looking for the first; and the Library tab of a synced
    /// library, one whose sync failed, and one only on this Mac.
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
        transfersSettings("settings-transfers", uploadsPaused: false),
        transfersSettings(
            "settings-transfers-uploads-paused",
            uploadsPaused: true
        ),
        librarySettings(
            "settings-library",
            storage: PreviewData.previewStorage
        ),
        librarySettings(
            "settings-library-problem",
            storage: PreviewData.previewStorage,
            shown: .previewFailed
        ),
        librarySettings(
            "settings-library-not-synced",
            storage: nil,
            shown: .previewNotSynced
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

    /// The Transfers pane with uploads paused or running and downloads running.
    private static func transfersSettings(
        _ id: String,
        uploadsPaused: Bool
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 420)) {
            ShotStage(
                TransfersSettingsTab()
                    .environment(PreviewData.configStore())
                    .environment(Downloads.stub())
                    .environment(DownloadStore())
                    .environment(OutboxStore(uploadsPaused: uploadsPaused))
                    .environment(Sync.stub())
                    .environment(UiStore())
            )
        }
    }

    /// The Library tab over a library whose storage is `storage`, or none,
    /// with its sync as `shown` says, once its name and people have answered.
    private static func librarySettings(
        _ id: String,
        storage: BridgeStorageLocation?,
        shown: BridgeSyncShown = .previewSynced
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 600)) {
            let fixture = LibrarySettingsTabPreviewFixture(storage: storage)
            return ShotStage(
                librarySettingsTabPreview(fixture, shown: shown)
            ) {
                fixture.answered
            }
        }
    }
}
