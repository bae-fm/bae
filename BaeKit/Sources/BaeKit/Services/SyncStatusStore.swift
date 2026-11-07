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
        public static let previewSynced = BridgeSyncShown(
            status: .synced,
            tone: .success,
            cause: nil,
            fix: nil,
            lastSynced: nil
        )

        /// A library with no storage set up.
        public static let previewNotSynced = BridgeSyncShown(
            status: .notSynced,
            tone: .neutral,
            cause: nil,
            fix: nil,
            lastSynced: nil
        )

        /// A library whose last pass failed because a file it uploads is
        /// gone.
        public static let previewFailed = BridgeSyncShown(
            status: .syncFailed,
            tone: .danger,
            cause: BridgeSyncCause(
                message: BridgeMessage(
                    key: "core.sync.cause.file_changed",
                    args: [BridgeMessageArg(name: "file", value: "Cover.jpg")]
                ),
                resumesAt: nil
            ),
            fix: .retryNow,
            lastSynced: Date(timeIntervalSinceNow: -2 * 3_600)
        )

        /// A Dropbox library this device is signed out of.
        public static let previewSignedOut = BridgeSyncShown(
            status: .signedOutOfDropbox,
            tone: .warning,
            cause: nil,
            fix: .signIn(provider: .dropbox),
            lastSynced: Date(timeIntervalSinceNow: -3 * 24 * 3_600)
        )
    }
#endif
