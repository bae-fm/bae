import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSyncStatus` for storage that refuses this Mac's S3 key. The
    /// preview's remedy line, what bae does next, has no line in the view:
    /// the cause is the one line core gives.
    static let macSyncStatusScenes: [ShotScene] = [
        syncStatus("mac.component.sync-status.status", height: 100),
        syncStatus(
            "mac.component.sync-status.no-tone",
            height: 100,
            tone: .neutral
        ),
        syncStatus(
            "mac.component.sync-status.cause",
            height: 150,
            cause: .changesStayOnDevice
        ),
        syncStatus(
            "mac.component.sync-status.countdown",
            height: 170,
            cause: .message(
                message: BridgeMessage(
                    key: "core.sync.cause.provider_wait",
                    args: [BridgeMessageArg(name: "provider", value: "Dropbox")]
                ),
                countdown: BridgeCountdown(
                    key: "core.sync.cause.provider_wait.countdown",
                    until: ShotClock.moment(secondsLeft: 42)
                )
            )
        ),
        syncStatus(
            "mac.component.sync-status.tone-success",
            height: 100,
            tone: .success
        ),
        syncStatus(
            "mac.component.sync-status.fix",
            height: 150,
            fix: .enterAccessKey
        ),
        syncStatus(
            "mac.component.sync-status.last-synced",
            height: 140,
            lastSynced: ShotClock.moment(secondsAgo: 3 * 86_400)
        ),
    ]

    private static func syncStatus(
        _ id: String,
        height: CGFloat,
        tone: BridgeSyncTone = .warning,
        cause: BridgeSyncCause? = nil,
        fix: BridgeSyncFix? = nil,
        lastSynced: Date? = nil
    ) -> ShotScene {
        component(id, width: 484, height: height) {
            ShotBox {
                MacSyncStatus(
                    location: nil,
                    actions: SyncFixActions(
                        onResetDevice: {},
                        onRestoreFromCode: {},
                        onCheckForUpdates: {},
                        onRemoveLibrary: {},
                        onEnterAccessKey: {}
                    )
                )
            }
            .environment(
                SyncStatusStore(
                    snapshot: BridgeSyncStatusSnapshot(
                        shown: BridgeSyncShown(
                            status: .signedOutOfS3,
                            statusKey: "core.sync.status.signed_out_s3",
                            shortStatusKey: "core.sync.status.signed_out",
                            tone: tone,
                            cause: cause,
                            fix: fix,
                            lastSynced: lastSynced
                        ),
                        setupRunning: false,
                        hasCloudHome: true,
                        undoneSetup: nil
                    )
                )
            )
            .environment(Sync.stub())
            #if BAE_OAUTH_PROVIDERS
                .environment(CloudSyncSetup())
            #endif
        }
    }
}
