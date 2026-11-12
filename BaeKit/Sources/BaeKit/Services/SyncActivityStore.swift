import Foundation
import Observation

/// A transfer line of the Activity panel, by the key its bar is under.
public enum ActivityLine: String, Sendable {
    case uploads
    case pins
    case makingLocal = "making_local"
}

/// Everything sync is doing on this device, as core reports it: the title
/// bar's button and the panel it opens read it here.
@MainActor
@Observable
public final class SyncActivityStore {
    public private(set) var activity: BridgeSyncActivity?
    /// Why the activity couldn't be read, until it is read again.
    public private(set) var error: DisplayError?
    /// The share of each transfer line's batch bytes done, by line.
    public nonisolated let bars: LatestStream<Double>

    public init(
        activity: BridgeSyncActivity? = nil,
        bars: LatestStream<Double> = .activityBars()
    ) {
        self.activity = activity
        self.bars = bars
    }

    /// The latest share of a transfer line's batch bytes done, for the native
    /// bar that reads it each display frame.
    public nonisolated func bar(_ line: ActivityLine) -> LatestSource<Double> {
        bars.source(for: line.rawValue)
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
                    paused: false
                ),
                pins: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 1, total: 3),
                    wait: nil,
                    paused: false
                ),
                makingLocal: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 0, total: 1),
                    wait: nil,
                    paused: false
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
