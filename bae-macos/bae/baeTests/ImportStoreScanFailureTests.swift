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
            activity: nil,
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

    /// The marks the store asked core to write, in order.
    @MainActor
    private final class Marks {
        var marked: [BridgeFolderScanAlert] = []
    }

    private struct MarkRefused: Error {}

    /// The store raises each alert the folder scans list and asks core to
    /// mark it shown, including at launch: the app's startup scan fails
    /// before the store subscribes, so its alert arrives in the first
    /// delivery.
    @MainActor
    @Test("the store raises a listed alert and marks it shown")
    func theStoreRaisesAListedAlertAndMarksItShown() async {
        let store = ImportStore()
        var raised: [BridgeFolderScanAlert] = []
        store.onScanFailure = { raised.append($0) }
        let marks = Marks()
        store.markScanAlertShown = { alert in marks.marked.append(alert) }

        let failed = Self.alert("/Media", "no such column")
        let notWatched = Self.alert("/Other", "limit reached", .notWatched)
        store.applyFolderScans(Self.scans([failed, notWatched]))
        await store.scanAlertMarksReturned()

        #expect(raised == [failed, notWatched])
        #expect(Set(marks.marked) == [failed, notWatched])
    }

    /// A delivery made before an alert's mark finishes still lists the
    /// alert, and the store does not raise it again.
    @MainActor
    @Test("the store raises an alert once while core writes its mark")
    func theStoreRaisesAnAlertOnceWhileCoreWritesItsMark() async {
        let store = ImportStore()
        var raised: [BridgeFolderScanAlert] = []
        store.onScanFailure = { raised.append($0) }
        let (held, release) = AsyncStream<Void>.makeStream()
        store.markScanAlertShown = { _ in
            for await _ in held { break }
        }

        let failed = Self.alert("/Media", "offline")
        store.applyFolderScans(Self.scans([failed]))
        store.applyFolderScans(Self.scans([failed]))
        release.yield()
        await store.scanAlertMarksReturned()

        #expect(raised == [failed])
    }

    /// A mark core refused leaves the alert in the list, so the store raises
    /// it again on the next delivery that lists it.
    @MainActor
    @Test("the store raises an alert again when its mark fails")
    func theStoreRaisesAnAlertAgainWhenItsMarkFails() async {
        let store = ImportStore()
        var raised: [BridgeFolderScanAlert] = []
        store.onScanFailure = { raised.append($0) }
        store.markScanAlertShown = { _ in throw MarkRefused() }

        let failed = Self.alert("/Media", "offline")
        store.applyFolderScans(Self.scans([failed]))
        await store.scanAlertMarksReturned()
        store.applyFolderScans(Self.scans([failed]))
        await store.scanAlertMarksReturned()

        #expect(raised == [failed, failed])
    }
}
