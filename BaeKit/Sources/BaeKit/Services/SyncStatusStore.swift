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
    /// The one line sync shows: "Synced", "Waiting: 3 items", or that the
    /// library isn't connected.
    public var statusText: String {
        guard let key = bridgeSyncIndicatorKey(indicator: indicator) else {
            return String(localized: "Not connected")
        }
        if case .waiting(let items) = indicator {
            return String.localizedStringWithFormat(
                localizedCoreString(key),
                Int(items)
            )
        }
        return localizedCoreString(key)
    }
    public func apply(_ snapshot: BridgeSyncStatusSnapshot) {
        self.snapshot = snapshot
    }
}
