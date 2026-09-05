import SwiftUI

/// Runtime sync state. The sync-status value stream is its only writer.
@Observable
public final class SyncStatusStore {
    public private(set) var snapshot: BridgeSyncStatusSnapshot?

    public init(snapshot: BridgeSyncStatusSnapshot? = nil) {
        self.snapshot = snapshot
    }

    public var syncReady: Bool {
        snapshot?.syncReady == true
    }

    public var syncing: Bool {
        snapshot?.syncing == true
    }

    public var indicator: BridgeSyncIndicator {
        guard let snapshot else { return .idle }
        return bridgeSyncIndicator(snapshot: snapshot)
    }

    public var error: DisplayError? {
        snapshot?.error.flatMap { DisplayError($0) }
    }

    public var canReconnect: Bool {
        snapshot?.canReconnect == true
    }

    /// The durable sync operations the last completed cycle left waiting on a
    /// person. Empty until a cycle reports some — including before the first
    /// status arrives, when nothing is known to be waiting. Each is retried by
    /// handing its `id` to `Sync.retryBlockedSyncOperation`.
    public var blocked: [BridgeBlockedSyncOperation] {
        snapshot?.blocked ?? []
    }

    public func apply(_ snapshot: BridgeSyncStatusSnapshot) {
        self.snapshot = snapshot
    }
}
