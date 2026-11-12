import SwiftUI

@Observable
public final class SyncStatusStore {
    public private(set) var snapshot: BridgeSyncStatusSnapshot?
    /// Why the sync status couldn't be read, until the next read succeeds.
    public private(set) var readError: DisplayError?
    public init(snapshot: BridgeSyncStatusSnapshot? = nil) {
        self.snapshot = snapshot
    }
    /// What the Sync page's top block shows: nothing until the status is
    /// first read, and nothing after a read failed, which the page says in
    /// its own words.
    public var shown: BridgeSyncShown? {
        readError == nil ? snapshot?.shown : nil
    }
    /// Whether the status couldn't be read, until a read succeeds.
    public var unreadable: Bool { readError != nil }
    /// Whether the sync state says this device can't reach its storage.
    public var offline: Bool {
        switch shown?.status {
        case .offline, .offlineWithUnsentChanges: true
        default: false
        }
    }
    /// What coven is waiting on; nil until the status is first read.
    public var pending: [BridgePendingRecord]? { snapshot?.pending }
    public func apply(_ snapshot: BridgeSyncStatusSnapshot) {
        self.snapshot = snapshot
        readError = nil
    }
    public func fail(_ error: any Error) {
        readError = DisplayError(error)
    }
}

#if DEBUG
    extension BridgeSyncShown {
        /// A library in sync.
        public static let fixtureSynced = BridgeSyncShown(
            status: .synced,
            statusKey: "core.sync.synced",
            tone: .success,
            cause: nil,
            fix: nil,
            lastSynced: nil
        )

        /// A library with no storage set up.
        public static let fixtureNotSynced = BridgeSyncShown(
            status: .notSynced,
            statusKey: "core.sync.status.not_synced",
            tone: .neutral,
            cause: nil,
            fix: nil,
            lastSynced: nil
        )

        /// A library whose storage this device can't reach.
        public static let fixtureOffline = BridgeSyncShown(
            status: .offline,
            statusKey: "core.sync.offline",
            tone: .warning,
            cause: nil,
            fix: nil,
            lastSynced: Date(timeIntervalSinceNow: -2 * 3_600)
        )
    }
#endif
