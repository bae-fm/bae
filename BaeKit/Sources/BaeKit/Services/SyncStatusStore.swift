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
            statusKey: "core.sync.synced",
            tone: .success,
            cause: nil,
            fix: nil,
            lastSynced: nil
        )

        /// A library with no storage set up.
        public static let previewNotSynced = BridgeSyncShown(
            status: .notSynced,
            statusKey: "core.sync.status.not_synced",
            tone: .neutral,
            cause: nil,
            fix: nil,
            lastSynced: nil
        )

        /// A library whose sync holds back without a problem.
        public static let previewWaiting = BridgeSyncShown(
            status: .waitingToSync,
            statusKey: "core.sync.status.waiting_to_sync",
            tone: .neutral,
            cause: nil,
            fix: nil,
            lastSynced: nil
        )

        /// A library whose storage this device can't reach.
        public static let previewOffline = BridgeSyncShown(
            status: .offline,
            statusKey: "core.sync.offline",
            tone: .warning,
            cause: nil,
            fix: nil,
            lastSynced: Date(timeIntervalSinceNow: -2 * 3_600)
        )

        /// A library whose last pass failed because a file it uploads is
        /// gone.
        public static let previewFailed = BridgeSyncShown(
            status: .syncFailed,
            statusKey: "core.sync.failed",
            tone: .danger,
            cause: .message(
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
            statusKey: "core.sync.status.signed_out_dropbox",
            tone: .warning,
            cause: nil,
            fix: .signIn(provider: .dropbox),
            lastSynced: Date(timeIntervalSinceNow: -3 * 24 * 3_600)
        )
    }
#endif
