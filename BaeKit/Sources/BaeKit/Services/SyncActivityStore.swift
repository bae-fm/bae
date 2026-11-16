import Foundation
import Observation

/// Everything sync is doing on this device, as core reports it: the title
/// bar's button and the panel it opens read it here.
@MainActor
@Observable
public final class SyncActivityStore {
    public private(set) var activity: BridgeSyncActivity?
    /// Why the activity couldn't be read, until it is read again.
    public private(set) var error: DisplayError?

    public init(activity: BridgeSyncActivity? = nil) {
        self.activity = activity
    }

    public func apply(_ activity: BridgeSyncActivity) {
        self.activity = activity
        error = nil
    }

    public func fail(_ error: any Error) {
        self.error = DisplayError(error)
    }
}

#if DEBUG
    extension SyncActivityStore {
        /// A library mid-sync for shots: releases uploading and pinning,
        /// and a record that needs signing in again.
        public static func preview(
            needsAttention: Bool = true
        ) -> SyncActivityStore {
            SyncActivityStore(
                activity: .preview(needsAttention: needsAttention)
            )
        }
    }

    extension BridgeSyncActivity {
        /// The activity `SyncActivityStore.shot` shows.
        public static func preview(needsAttention: Bool) -> BridgeSyncActivity {
            BridgeSyncActivity(
                moving: true,
                held: false,
                needsAttention: needsAttention,
                failure: nil,
                attention: needsAttention ? [previewSignIn] : [],
                syncing: true,
                uploads: BridgeUploadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 2),
                    wait: nil,
                    paused: false,
                    bar: BridgeBatchBar(done: 0.43, moving: [])
                ),
                pins: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 3),
                    wait: nil,
                    paused: false,
                    bar: BridgeBatchBar(done: 0.3, moving: [])
                ),
                movingToThisDevice: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 0, total: 1),
                    wait: nil,
                    paused: false,
                    bar: BridgeBatchBar(done: 0, moving: [])
                )
            )
        }

        /// Dropbox asking to be signed in to again.
        private static let previewSignIn = BridgePendingRecord(
            subject: BridgeMessage(
                key: "core.pending.subject.connection",
                args: []
            ),
            reason: BridgePendingReason(
                key: "core.pending.reason.storage",
                detail: nil,
                causes: []
            ),
            action: .signIn(provider: .dropbox),
            actionKey: "core.pending.action.sign_in",
            sendsFiles: false
        )
    }
#endif
