import BaeKit
import Foundation
import Testing

@testable import bae

@Suite("Watched folder scan failures")
struct ImportStoreScanFailureTests {
    /// Folder scans holding `statuses`, with no scan running.
    private static func scans(
        _ statuses: [BridgeWatchedFolderScanStatus]
    ) -> BridgeFolderScans {
        BridgeFolderScans(statuses: statuses, activity: nil)
    }

    private static func failed(
        _ path: String,
        _ error: String
    ) -> BridgeWatchedFolderScanStatus {
        BridgeWatchedFolderScanStatus(
            watchedFolderPath: Data(path.utf8),
            watchedFolderName: URL(fileURLWithPath: path).lastPathComponent,
            status: .failed(error: error),
            onNetworkVolume: false
        )
    }

    private static func notWatched(
        _ path: String,
        _ reason: String
    ) -> BridgeWatchedFolderScanStatus {
        BridgeWatchedFolderScanStatus(
            watchedFolderPath: Data(path.utf8),
            watchedFolderName: URL(fileURLWithPath: path).lastPathComponent,
            status: .notWatched(reason: reason),
            onNetworkVolume: false
        )
    }

    private static func complete(_ path: String)
        -> BridgeWatchedFolderScanStatus
    {
        BridgeWatchedFolderScanStatus(
            watchedFolderPath: Data(path.utf8),
            watchedFolderName: URL(fileURLWithPath: path).lastPathComponent,
            status: .complete,
            onNetworkVolume: false
        )
    }

    /// The failure a scan wrote before the UI existed is in the first folder
    /// scans the store is given, which is the launch case: the app's own startup scan
    /// runs and fails before any of this is subscribed.
    @MainActor
    @Test("a root already failed in the first delivery is reported")
    func firstDeliveryReportsAnAlreadyFailedRoot() {
        let store = ImportStore()
        var raised: [(String, String)] = []
        store.onScanFailure = { name, detail, _ in
            raised.append((name, detail))
        }

        store.applyFolderScans(
            Self.scans([Self.failed("/Media", "no such column")])
        )

        #expect(raised.count == 1)
        #expect(raised.first?.0 == "Media")
        #expect(raised.first?.1 == "no such column")
    }

    /// The timer re-reads every root every quarter hour, and the scans are
    /// re-delivered as any root's scan moves — the same fault must not raise
    /// the alert again. A different fault on the same root does.
    @MainActor
    @Test("the same failure is reported once, a different one again")
    func repeatedDeliveriesReportOnlyNewFailures() {
        let store = ImportStore()
        var raised: [(String, String)] = []
        store.onScanFailure = { name, detail, _ in
            raised.append((name, detail))
        }

        store.applyFolderScans(Self.scans([Self.failed("/Media", "offline")]))
        // A delivery that says the same thing, then one that adds a root so
        // the scans differ while the failure does not.
        store.applyFolderScans(Self.scans([Self.failed("/Media", "offline")]))
        store.applyFolderScans(
            Self.scans([
                Self.failed("/Media", "offline"), Self.complete("/Other"),
            ])
        )
        store.applyFolderScans(
            Self.scans([Self.failed("/Media", "no such column")])
        )

        #expect(raised.map(\.1) == ["offline", "no such column"])
    }

    /// A root read in full whose directories could not all be watched raises
    /// its own alert, once, and a failure with the same text after it is a
    /// different fact.
    @MainActor
    @Test("a root that is not watched raises its own alert, once")
    func aRootThatIsNotWatchedRaisesItsOwnAlert() {
        let store = ImportStore()
        var raised: [(String, FolderScanAlertKind)] = []
        store.onScanFailure = { name, _, kind in raised.append((name, kind)) }

        store.applyFolderScans(
            Self.scans([Self.notWatched("/Media", "limit reached")])
        )
        store.applyFolderScans(
            Self.scans([Self.notWatched("/Media", "limit reached")])
        )
        store.applyFolderScans(
            Self.scans([Self.failed("/Media", "limit reached")])
        )

        #expect(raised.map(\.1) == [.notWatched, .failed])
    }

    /// A root that reads cleanly again has no standing failure, so the next
    /// time it breaks the same way it is news.
    @MainActor
    @Test("a root that recovers reports its next break again")
    func aRecoveredRootReportsItsNextBreak() {
        let store = ImportStore()
        var raised: [(String, String)] = []
        store.onScanFailure = { name, detail, _ in
            raised.append((name, detail))
        }

        store.applyFolderScans(Self.scans([Self.failed("/Media", "offline")]))
        store.applyFolderScans(Self.scans([Self.complete("/Media")]))
        store.applyFolderScans(Self.scans([Self.failed("/Media", "offline")]))

        #expect(raised.map(\.1) == ["offline", "offline"])
    }
}
