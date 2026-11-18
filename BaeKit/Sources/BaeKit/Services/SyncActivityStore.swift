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
    /// The share of each transfer line's batch bytes done, by line.
    public nonisolated let bars: LatestStream<Double> = .activityBars()

    public init(activity: BridgeSyncActivity? = nil) {
        self.activity = activity
    }

    /// The latest share of a transfer line's batch bytes done, for the native
    /// bar that reads it each display frame. Core keys each line's bar by the
    /// line's number as decimal text.
    public nonisolated func bar(_ line: BridgeActivityLine)
        -> LatestSource<Double>
    {
        bars.source(for: String(line.rawValue))
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
        /// and, when `needsAttention`, sync failing until the person signs
        /// in again.
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
                failure: needsAttention ? previewSignedOut : nil,
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
                movingToThisDevice: BridgeDownloadQueueActivity(
                    batch: BridgeTransferBatch(done: 0, total: 1),
                    wait: nil,
                    paused: false
                )
            )
        }

        /// Storage refusing this device's sign-in.
        private static let previewSignedOut = BridgeError.Diagnostic(
            category: .storage(failure: .authentication),
            detail: "Dropbox answered 401: expired_access_token"
        )
    }
#endif
