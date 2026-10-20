import SwiftUI

@Observable
public final class SyncStatusStore {
    public private(set) var snapshot: BridgeSyncStatusSnapshot?
    public init(snapshot: BridgeSyncStatusSnapshot? = nil) {
        self.snapshot = snapshot
    }
    public var syncReady: Bool { snapshot?.connected == true }
    /// A sync pass is in progress.
    public var syncing: Bool { snapshot?.syncing == true }
    public var indicator: BridgeSyncIndicator {
        snapshot?.indicator ?? .loading
    }
    public var pending: BridgePendingRecords { snapshot?.pending ?? .loading }
    public var error: DisplayError? {
        snapshot?.error.flatMap { DisplayError($0) }
    }
    public var canReconnect: Bool { snapshot?.connected == false }
    /// Whether the status line has something to say beyond "Synced": a
    /// failure, or records coven is waiting on. The library shows the line
    /// only then.
    public var needsAttention: Bool {
        if case .waiting = indicator { return true }
        return snapshot?.error != nil
    }
    /// Whether sync stopped because another copy of this device is syncing,
    /// which only resetting this device answers.
    public var offersDeviceReset: Bool {
        if case BridgeError.Diagnostic(.deviceResetRequired, _)? = snapshot?
            .error
        {
            return true
        }
        return false
    }
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
