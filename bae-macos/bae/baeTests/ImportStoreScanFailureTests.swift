import BaeKit
import Foundation
import Testing

@testable import bae

@Suite("Watched folder scan alerts")
struct ImportStoreScanFailureTests {
    /// Folder scans owing `alerts`, with no folder listed and no scan
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
            watchedFolderPath: Data(path.utf8),
            watchedFolderName: URL(fileURLWithPath: path).lastPathComponent,
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

    /// Each alert the folder scans owe is raised and marked shown, the
    /// launch case included: the app's startup scan fails before any of
    /// this is subscribed, and its alert is in the first delivery.
    @MainActor
    @Test("an owed alert is raised and marked shown")
    func anOwedAlertIsRaisedAndMarkedShown() async {
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

    /// A delivery made before an alert's mark returns still owes the alert,
    /// and does not raise it again.
    @MainActor
    @Test("an alert is raised once while its mark is written")
    func anAlertIsRaisedOnceWhileItsMarkIsWritten() async {
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

    /// A mark core refused leaves the alert owed, so the next delivery that
    /// owes it raises it again.
    @MainActor
    @Test("an alert whose mark failed is raised again")
    func anAlertWhoseMarkFailedIsRaisedAgain() async {
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
