import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Settings panes: Playback with pausing between sides on, so its
    /// countdown shows under the switch; Casting on, with the receivers it
    /// found and still looking for the first; and the Library tab of a synced
    /// library, one whose sync failed, and one only on this Mac; then the Sync
    /// page in each state the boards draw.
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
        castingSettings("settings-casting", receivers: castPickerReceivers()),
        castingSettings(
            "settings-casting-looking",
            receivers: BridgeCastReceiverList(casting: nil, found: [])
        ),
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
        syncPage("sync-status-synced", .previewSynced),
        syncPage("sync-status-waiting", .previewWaiting),
        syncPage("sync-status-offline", .previewOffline),
        syncPage(
            "sync-status-signed-out",
            .previewSignedOut,
            storage: PreviewData.previewDropboxStorage
        ),
        syncPage("sync-status-failed", .previewFailed),
    ]

    /// The Casting tab with casting on and `receivers` found.
    private static func castingSettings(
        _ id: String,
        receivers: BridgeCastReceiverList
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 600)) {
            let castStore = CastStore()
            castStore.receivers = receivers
            return ShotStage(
                CastingSettingsTab()
                    .environment(Cast.stub())
                    .environment(CastStart.stub())
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

    /// The Sync page of a library in `storage`, its status block showing
    /// `shown`.
    private static func syncPage(
        _ id: String,
        _ shown: BridgeSyncShown,
        storage: BridgeStorageLocation = PreviewData.previewStorage
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 640)) {
            let fixture = LibrarySettingsTabPreviewFixture(storage: storage)
            return ShotStage(syncSettingsPagePreview(fixture, shown: shown))
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

extension LibrarySettingsTabPreviewFixture {
    /// Whether both reads have answered, so the tab shows the name and the
    /// people.
    fileprivate var answered: Bool {
        libraryName.name != nil && membership.members != nil
    }
}
