import SwiftUI

@Observable
public final class SyncStatusStore {
    public private(set) var snapshot: BridgeSyncStatusSnapshot?
    /// Why the sync status couldn't be read, until the next read succeeds.
    public private(set) var readError: DisplayError?
    public init(snapshot: BridgeSyncStatusSnapshot? = nil) {
        self.snapshot = snapshot
    }
    public var syncReady: Bool { snapshot?.connected == true }
    public var indicator: BridgeSyncIndicator {
        if readError != nil { return .unreadable }
        return snapshot?.indicator ?? .loading
    }
    public var pending: BridgePendingRecords { snapshot?.pending ?? .loading }
    public var error: DisplayError? {
        readError ?? snapshot?.error.flatMap { DisplayError($0) }
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
    /// The one line sync shows: what sync is doing, "Waiting: 3 items", or
    /// that the library isn't connected.
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
        readError = nil
    }
    public func fail(_ error: any Error) {
        readError = DisplayError(error)
    }
}
