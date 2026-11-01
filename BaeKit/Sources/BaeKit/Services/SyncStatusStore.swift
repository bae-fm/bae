import SwiftUI

/// What the sync status line shows: checking before the sync status is first
/// read, unreadable after a read failed, or the indicator of the status read.
public enum SyncLine: Equatable, Sendable {
    case checking
    case unreadable
    case read(BridgeSyncIndicator)
}

@Observable
public final class SyncStatusStore {
    public private(set) var snapshot: BridgeSyncStatusSnapshot?
    /// Why the sync status couldn't be read, until the next read succeeds.
    public private(set) var readError: DisplayError?
    public init(snapshot: BridgeSyncStatusSnapshot? = nil) {
        self.snapshot = snapshot
    }
    public var syncReady: Bool { snapshot?.connected == true }
    /// What the status line shows: checking until the status is first read,
    /// unreadable after a read failed until the next succeeds, otherwise the
    /// indicator of the status as read.
    public var line: SyncLine {
        if readError != nil { return .unreadable }
        guard let snapshot else { return .checking }
        return .read(snapshot.indicator)
    }
    /// What coven is waiting on; nil until the status is first read.
    public var pending: [BridgePendingRecord]? { snapshot?.pending }
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
        let indicator: BridgeSyncIndicator
        switch line {
        case .checking: return String(localized: "Checking sync…")
        case .unreadable: return String(localized: "Couldn't read sync status")
        case .read(let read): indicator = read
        }
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
