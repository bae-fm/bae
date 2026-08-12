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

    public func apply(_ snapshot: BridgeSyncStatusSnapshot) {
        self.snapshot = snapshot
    }
}
