import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Sync page's boards (`mac.settings.sync.*`) that show what sync
    /// waits on or why its cloud home isn't shown: the pending records as
    /// rows, the setup that ended undone, and a cloud home that couldn't be
    /// read.
    static let syncSettingsScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.sync.pending",
            size: CGSize(width: 500, height: 640)
        ) {
            ShotStage(
                syncSettingsPagePreview(
                    LibrarySettingsTabPreviewFixture(
                        storage: PreviewData.previewDropboxStorage
                    ),
                    shown: BridgeSyncShown(
                        status: .waitingToSync,
                        statusKey: "core.sync.status.waiting_to_sync",
                        tone: .neutral,
                        cause: nil,
                        fix: nil,
                        lastSynced: Date(timeIntervalSinceNow: -2 * 3_600)
                    ),
                    pending: [
                        BridgePendingRecord(
                            subject: BridgeMessage(
                                key: "core.pending.subject.release_file",
                                args: [
                                    BridgeMessageArg(
                                        name: "file",
                                        value: "03 Second Floor.flac"
                                    ),
                                    BridgeMessageArg(
                                        name: "album",
                                        value: "Paper Lanterns"
                                    ),
                                ]
                            ),
                            reason: BridgePendingReason(
                                key: "core.pending.reason.file_changed",
                                detail:
                                    "the file changed after it was imported",
                                causes: []
                            ),
                            action: .restoreFile,
                            actionKey: "core.pending.action.restore_file",
                            sendsFiles: true
                        ),
                        BridgePendingRecord(
                            subject: BridgeMessage(
                                key: "core.pending.subject.cover",
                                args: [
                                    BridgeMessageArg(
                                        name: "album",
                                        value: "Low Tide Hours"
                                    )
                                ]
                            ),
                            reason: BridgePendingReason(
                                key: "core.pending.reason.storage",
                                detail: "request failed: connection refused",
                                causes: []
                            ),
                            action: .retryOperation(id: 7),
                            actionKey: "core.pending.action.retry",
                            sendsFiles: true
                        ),
                    ]
                )
            )
        },
        ShotScene(
            id: "mac.settings.sync.setup-undone",
            size: CGSize(width: 500, height: 640)
        ) {
            ShotStage(
                syncSettingsPagePreview(
                    LibrarySettingsTabPreviewFixture(storage: nil),
                    shown: .previewNotSynced,
                    undoneSetup: BridgeUndoneStorageSetup(
                        operation: 1,
                        error: BridgeError.Diagnostic(
                            category: .storageSetupUndone,
                            detail: "the setup was stopped partway"
                        )
                    )
                )
            )
        },
        ShotScene(
            id: "mac.settings.sync.cloud-home-unreadable",
            size: CGSize(width: 500, height: 640)
        ) {
            ShotStage(
                syncSettingsPagePreview(
                    LibrarySettingsTabPreviewFixture(storage: nil),
                    shown: .previewSynced,
                    storageReadError: DisplayError(
                        line: "Couldn’t read the cloud home.",
                        detail: "database error: no such record"
                    )
                )
            )
        },
    ]
}
