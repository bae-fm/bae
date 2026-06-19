import SwiftUI

/// Mirror of core's cloud outbox processing snapshot, rendered by the Storage
/// Manager's queue panel and used by every storage row to read its
/// per-release upload count (no cached `pendingUploads` field on
/// `ReleaseSummary`). The reducer is the sole writer: it lands the whole
/// `BridgeOutboxSnapshot` on every `outboxChanged` event; views read it at
/// the leaf. The snapshot is swapped wholesale (no per-item interning)
/// because core re-pushes it in full on every change.
@Observable
class OutboxStore {
    var snapshot: BridgeOutboxSnapshot

    init(snapshot: BridgeOutboxSnapshot) {
        self.snapshot = snapshot
    }

    /// Per-release upload progress, or nil if the release has no work in
    /// flight. Storage rows read this to render their badge and to suppress
    /// storage actions while uploads are in flight.
    func progress(forRelease releaseId: String) -> BridgeUploadProgress? {
        snapshot.perRelease[releaseId]
    }

    /// The idle (empty) queue. Seeds the store before the first snapshot read
    /// and serves as the fallback if that read fails.
    static var emptySnapshot: BridgeOutboxSnapshot {
        BridgeOutboxSnapshot(
            uploads: [],
            deletes: [],
            perRelease: [:],
            total: BridgeUploadProgress(
                queued: 0,
                active: 0,
                failed: 0,
                bytesDone: 0,
                bytesTotal: 0,
            ),
            pendingDeletes: 0,
            paused: false,
            throughputBps: 0,
            throughputLabel: "",
            etaSeconds: nil,
            etaLabel: "",
            bytesLabel: "",
        )
    }
}
