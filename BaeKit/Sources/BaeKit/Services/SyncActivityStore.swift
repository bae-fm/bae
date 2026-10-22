import Foundation
import Observation

/// Everything sync is doing on this device, as core reports it: the title
/// bar's indicator and the list it opens read it here.
@MainActor
@Observable
public final class SyncActivityStore {
    public private(set) var activity: BridgeSyncActivity?
    /// The last failure to read the activity, or to act from its list.
    public private(set) var error: DisplayError?
    private let retryNowAction:
        @Sendable () async throws -> BridgeSyncRetryOutcome
    private let dismissAbandonedAction:
        @Sendable (BridgeAbandonedRecord) async throws -> Void

    public init(
        activity: BridgeSyncActivity? = nil,
        retryNow:
            @escaping @Sendable () async throws -> BridgeSyncRetryOutcome,
        dismissAbandoned:
            @escaping @Sendable (BridgeAbandonedRecord) async throws -> Void
    ) {
        self.activity = activity
        retryNowAction = retryNow
        dismissAbandonedAction = dismissAbandoned
    }

    public func apply(_ activity: BridgeSyncActivity) {
        self.activity = activity
        error = nil
    }

    public func fail(_ error: any Error) {
        self.error = DisplayError(error)
    }

    /// The indicator shows while bytes move or something needs the person,
    /// and is absent otherwise.
    public var visible: Bool {
        guard let activity else { return error != nil }
        return activity.moving || activity.needsAttention || error != nil
    }

    /// The person's retry: every ordinary wait ends and sync runs now.
    /// Returns once the first pass after it ends; a retry that didn't sync
    /// shows its reason in the list.
    public func retryNow() async {
        do {
            if let failure = try await retryNowAction().failure {
                error = failure
            }
        }
        catch {
            fail(error)
        }
    }

    /// Forget an abandoned upload the person has seen; the next activity
    /// value no longer lists it.
    public func dismiss(_ attachment: BridgeAbandonedAttachment) async throws {
        try await dismissAbandonedAction(attachment.record)
    }
}

#if DEBUG
    extension SyncActivityStore {
        /// A library mid-sync for previews: a few files uploading and a
        /// record that needs signing in again.
        public static func preview(
            needsAttention: Bool = true
        ) -> SyncActivityStore {
            SyncActivityStore(
                activity: .preview(needsAttention: needsAttention),
                retryNow: { .synced },
                dismissAbandoned: { _ in }
            )
        }
    }

    extension BridgeSyncActivity {
        /// The activity `SyncActivityStore.preview` shows.
        public static func preview(needsAttention: Bool) -> BridgeSyncActivity {
            BridgeSyncActivity(
                moving: true,
                needsAttention: needsAttention,
                failure: nil,
                attention: .ready(
                    records: needsAttention ? [previewSignIn] : []
                ),
                abandoned: .ready(attachments: []),
                syncing: true,
                uploads: BridgeUploadsInFlight(
                    summaryParts: [
                        BridgeCountLabel(key: "core.queue.uploading", count: 2),
                        BridgeCountLabel(key: "core.queue.queued", count: 10),
                    ],
                    bytesDone: 3_200_000,
                    bytesTotal: 8_400_000
                ),
                downloads: nil
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
            actionKey: "core.pending.action.sign_in"
        )
    }
#endif
