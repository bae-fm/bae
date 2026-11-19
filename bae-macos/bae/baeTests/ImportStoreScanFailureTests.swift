import BaeKit
import Foundation
import Testing

@testable import bae

@Suite("Watched folder scan alerts")
struct ImportStoreScanFailureTests {
    /// Folder scans that list `alerts`, with no folder listed and no scan
    /// running.
    private static func scans(
        _ alerts: [BridgeFolderScanAlert]
    ) -> BridgeFolderScans {
        BridgeFolderScans(
            statuses: [],
            hasProblem: true,
            scanning: false,
            alerts: alerts
        )
    }

    private static func alert(
        _ path: String,
        _ detail: String,
        _ kind: BridgeFolderScanAlertKind = .failed
    ) -> BridgeFolderScanAlert {
        BridgeFolderScanAlert(
            folder: BridgeWatchedFolder(
                path: Data(path.utf8),
                pathText: path,
                name: URL(fileURLWithPath: path).lastPathComponent
            ),
            kind: kind,
            detail: detail
        )
    }

    /// The raised alerts the store handed the folder scans' query, in order.
    @MainActor
    private final class Requests {
        var sets: [Set<BridgeFolderScanAlert>] = []
    }

    /// The store raises each alert the folder scans list and hands the query
    /// the alerts it raised, including at launch: the app's startup scan
    /// fails before the store subscribes, so its alert arrives in the first
    /// delivery.
    @MainActor
    @Test("the store raises a listed alert and hands it to the query")
    func theStoreRaisesAListedAlertAndHandsItToTheQuery() {
        let store = ImportStore()
        var raised: [BridgeFolderScanAlert] = []
        store.onScanFailure = { raised.append($0) }
        let requests = Requests()
        store.setRaisedScanAlerts = { requests.sets.append(Set($0)) }

        let failed = Self.alert("/Media", "no such column")
        let notWatched = Self.alert("/Other", "limit reached", .notWatched)
        store.applyFolderScans(Self.scans([failed, notWatched]))

        #expect(raised == [failed, notWatched])
        #expect(requests.sets == [[failed, notWatched]])
    }

    /// The query's request holds every alert the store raised while the app
    /// runs, not only the last delivery's.
    @MainActor
    @Test("the store hands the query every alert it raised")
    func theStoreHandsTheQueryEveryAlertItRaised() {
        let store = ImportStore()
        var raised: [BridgeFolderScanAlert] = []
        store.onScanFailure = { raised.append($0) }
        let requests = Requests()
        store.setRaisedScanAlerts = { requests.sets.append(Set($0)) }

        let offline = Self.alert("/Media", "offline")
        let full = Self.alert("/Other", "the disk is full")
        store.applyFolderScans(Self.scans([offline]))
        store.applyFolderScans(Self.scans([full]))
        store.applyFolderScans(Self.scans([]))

        #expect(raised == [offline, full])
        #expect(
            requests.sets == [[offline], [offline, full], [offline, full]]
        )
    }
}
