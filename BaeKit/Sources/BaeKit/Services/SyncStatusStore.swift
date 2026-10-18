import SwiftUI

@Observable
public final class SyncStatusStore {
    public private(set) var snapshot: BridgeSyncStatusSnapshot?
    public init(snapshot: BridgeSyncStatusSnapshot? = nil) {
        self.snapshot = snapshot
    }
    public var syncReady: Bool { snapshot?.connected == true }
    public var syncing: Bool { snapshot?.running == true }
    public var indicator: BridgeSyncIndicator {
        snapshot?.indicator ?? .loading
    }
    public var pending: BridgePendingRecords { snapshot?.pending ?? .loading }
    public var error: DisplayError? {
        snapshot?.error.flatMap { DisplayError($0) }
    }
    public var canReconnect: Bool { snapshot?.connected == false }
    public func apply(_ snapshot: BridgeSyncStatusSnapshot) {
        self.snapshot = snapshot
    }
}
