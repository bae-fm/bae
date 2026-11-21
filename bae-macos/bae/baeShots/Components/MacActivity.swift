import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacActivity` at the width the 360pt panel's 14pt inset leaves it.
    static let macActivityScenes: [ShotScene] = [
        activityPanel("mac.component.activity.up-to-date", height: 90),
        activityPanel(
            "mac.component.activity.transfers",
            height: 330,
            activity: BridgeSyncActivity(
                moving: true,
                held: false,
                failure: nil,
                syncing: true,
                uploads: BridgeUploadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 2),
                    wait: nil
                ),
                pins: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 3),
                    wait: nil,
                    paused: false
                ),
                movingToThisDevice: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 0, total: 1),
                    wait: nil,
                    paused: false
                )
            ),
            bars: ["0": 0.43, "1": 0.3, "2": 0.2]
        ),
        activityPanel(
            "mac.component.activity.queues-held",
            height: 300,
            activity: BridgeSyncActivity(
                moving: false,
                held: true,
                failure: nil,
                syncing: false,
                uploads: BridgeUploadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 2),
                    wait: .metered
                ),
                pins: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 3),
                    wait: .paused,
                    paused: true
                ),
                movingToThisDevice: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 0, total: 1),
                    wait: .paused,
                    paused: true
                )
            )
        ),
        activityPanel(
            "mac.component.activity.status-unknown",
            height: 130,
            readError: ShotReadError(
                text: "bae couldn’t read the sync status."
            )
        ),
        activityPanel(
            "mac.component.activity.sync-needs-you",
            height: 260,
            activity: BridgeSyncActivity(
                moving: false,
                held: false,
                failure: syncRefused,
                syncing: true,
                uploads: nil,
                pins: nil,
                movingToThisDevice: nil
            )
        ),
    ]

    private static func activityPanel(
        _ id: String,
        height: CGFloat,
        activity: BridgeSyncActivity? = nil,
        bars: [String: Double] = [:],
        readError: (any Error)? = nil,
        app: [AppProblem] = []
    ) -> ShotScene {
        component(id, width: 332, height: height) {
            let store = activityStore(
                activity,
                bars: bars,
                readError: readError
            )
            MacActivity(
                problems: ActivityProblems(
                    app: app,
                    activity: activity,
                    error: store.error,
                    subsonic: .off,
                    dismissed: []
                ),
                app: app,
                dismissed: .constant([]),
                onOpenSettings: {},
                onOpenStorageManager: {},
                subsonic: .off,
                onOpenSubsonicSettings: {}
            )
            .environment(store)
            .transferLineStores()
        }
    }
}

/// The text of an error a shot's store failed to read.
private struct ShotReadError: LocalizedError {
    let text: String

    var errorDescription: String? { text }
}
