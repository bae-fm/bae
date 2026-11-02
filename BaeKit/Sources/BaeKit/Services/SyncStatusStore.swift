import SwiftUI

/// What the sync status line shows: checking before the sync status is first
/// read, unreadable after a read failed, no storage while this device has none
/// set up, or the indicator of the status read.
public enum SyncLine: Equatable, Sendable {
    case checking
    case unreadable
    /// This device has no storage set up, so there is no sync to describe.
    /// Under a recorded location it means this device no longer holds its
    /// credentials for it, and Settings offers signing in to it again in
    /// place of the line.
    case noStorage
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
    /// What the status line shows: checking until the status is first read,
    /// unreadable after a read failed until the next succeeds, otherwise the
    /// indicator of the status as read, or no storage.
    public var line: SyncLine {
        if readError != nil { return .unreadable }
        guard let snapshot else { return .checking }
        guard let indicator = snapshot.indicator else { return .noStorage }
        return .read(indicator)
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
    /// The one line sync shows: what sync is doing, or "Waiting: 3 items";
    /// none with no storage set up on this device, which has no sync to
    /// describe.
    public var statusText: String? {
        let indicator: BridgeSyncIndicator
        switch line {
        case .checking: return String(localized: "Checking sync…")
        case .unreadable: return String(localized: "Couldn't read sync status")
        case .noStorage: return nil
        case .read(let read): indicator = read
        }
        let key = bridgeSyncIndicatorKey(indicator: indicator)
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
