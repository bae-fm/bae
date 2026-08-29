import BaeKit
import Foundation
import Testing

@testable import bae

@Suite("Import selection")
struct ImportSelectionTests {
    @MainActor
    @Test("a read that says the folder is gone drops it from the selection")
    func aMissingCandidateClearsItsSelection() {
        let uiStore = UiStore()
        var reported: [Set<String>] = []
        uiStore.onFolderCandidateSelectionChanged = { reported.append($0) }

        uiStore.setFolderCandidateSelection(["/w/a", "/w/b"])
        // What the per-key read does when it delivers no candidate.
        uiStore.removeFolderCandidateSelection(["/w/a"])

        #expect(uiStore.selectedFolderCandidates == ["/w/b"])
        #expect(reported == [["/w/a", "/w/b"], ["/w/b"]])
    }
}

@Suite("Watched folder scan failures")
struct ImportStoreScanFailureTests {
    /// A summary carrying `statuses`, with everything else empty. The alert
    /// reads nothing but the scan statuses.
    private static func summary(
        _ statuses: [BridgeWatchedFolderScanStatus]
    ) -> BridgeImportQueueSummary {
        BridgeImportQueueSummary(
            counts: BridgeTriageTabCounts(pending: 0, done: 0, skipped: 0),
            watchedFolders: [],
            folderScanStatuses: statuses,
            folderScanActivity: nil,
            groupKeys: [],
            ready: [],
            firstUnidentified: nil
        )
    }

    private static func failed(
        _ path: String,
        _ error: String
    ) -> BridgeWatchedFolderScanStatus {
        BridgeWatchedFolderScanStatus(
            watchedFolderPath: path,
            watchedFolderName: "Rips",
            status: .failed(error: error),
            onNetworkVolume: false
        )
    }

    private static func complete(_ path: String)
        -> BridgeWatchedFolderScanStatus
    {
        BridgeWatchedFolderScanStatus(
            watchedFolderPath: path,
            watchedFolderName: "Rips",
            status: .complete,
            onNetworkVolume: false
        )
    }

    /// The failure a scan wrote before the UI existed is in the first summary
    /// the store is given, which is the launch case: the app's own startup scan
    /// runs and fails before any of this is subscribed.
    @MainActor
    @Test("a root already failed in the first delivery is reported")
    func firstDeliveryReportsAnAlreadyFailedRoot() {
        let store = ImportStore()
        var raised: [(String, String)] = []
        store.onScanFailure = { path, detail in raised.append((path, detail)) }

        store.applySummary(
            Self.summary([Self.failed("/Media", "no such column")])
        )

        #expect(raised.count == 1)
        #expect(raised.first?.0 == "/Media")
        #expect(raised.first?.1 == "no such column")
    }

    /// The summary is re-delivered on every verdict the sweep commits, and the
    /// timer re-reads every root every quarter hour — the same fault must not
    /// raise the alert again. A different fault on the same root does.
    @MainActor
    @Test("the same failure is reported once, a different one again")
    func repeatedDeliveriesReportOnlyNewFailures() {
        let store = ImportStore()
        var raised: [(String, String)] = []
        store.onScanFailure = { path, detail in raised.append((path, detail)) }

        store.applySummary(Self.summary([Self.failed("/Media", "offline")]))
        // A delivery that says the same thing, then one that adds a row so the
        // summary differs while the failure does not.
        store.applySummary(Self.summary([Self.failed("/Media", "offline")]))
        store.applySummary(
            Self.summary([
                Self.failed("/Media", "offline"), Self.complete("/Other"),
            ])
        )
        store.applySummary(
            Self.summary([Self.failed("/Media", "no such column")])
        )

        #expect(raised.map(\.1) == ["offline", "no such column"])
    }

    /// A root that reads cleanly again has no standing failure, so the next
    /// time it breaks the same way it is news.
    @MainActor
    @Test("a root that recovers reports its next break again")
    func aRecoveredRootReportsItsNextBreak() {
        let store = ImportStore()
        var raised: [(String, String)] = []
        store.onScanFailure = { path, detail in raised.append((path, detail)) }

        store.applySummary(Self.summary([Self.failed("/Media", "offline")]))
        store.applySummary(Self.summary([Self.complete("/Media")]))
        store.applySummary(Self.summary([Self.failed("/Media", "offline")]))

        #expect(raised.map(\.1) == ["offline", "offline"])
    }
}

/// The read behind the import list failing before the list has registered its
/// first page. The delivery task starts with the source, so on a database this
/// build cannot read the failure arrives with nobody to tell — and the page
/// that registers a moment later waits on a loop that has already returned.
/// That is what rendered a broken library as one with no folders.
@Suite("Import list read failures")
struct ImportListPageSourceFailureTests {
    private struct ReadFailed: Error {}

    /// A subscription whose first read fails, and which reports when that
    /// read has been attempted.
    private final class FailingListSubscription: ImportListSubscriptionProtocol,
        @unchecked Sendable
    {
        private let attempted = AsyncStreamSignal()

        var firstReadAttempted: Void {
            get async { await attempted.wait() }
        }

        func setWindows(windows _: [BridgeLibraryPageWindow]) throws {}
        func setView(view _: BridgeImportListView) throws -> UInt64 { 0 }
        func cancel() async throws {}

        func next() async throws -> BridgeImportListSnapshot {
            attempted.signal()
            throw ReadFailed()
        }
    }

    /// One-shot signal: `wait()` returns once `signal()` has been called, then
    /// and later.
    private final class AsyncStreamSignal: @unchecked Sendable {
        private let lock = NSLock()
        private var fired = false
        private var waiters: [CheckedContinuation<Void, Never>] = []

        func signal() {
            lock.lock()
            fired = true
            let waiters = self.waiters
            self.waiters = []
            lock.unlock()
            for waiter in waiters { waiter.resume() }
        }

        func wait() async {
            await withCheckedContinuation { continuation in
                lock.lock()
                if fired {
                    lock.unlock()
                    continuation.resume()
                    return
                }
                waiters.append(continuation)
                lock.unlock()
            }
        }
    }

    @Test("a page registered after the read failed is told")
    func aPageRegisteredAfterTheFailureIsTold() async {
        let subscription = FailingListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in }
        )
        // The read fails before anything subscribes, which is the launch race.
        await subscription.firstReadAttempted

        let reported = AsyncStreamSignal()
        let failures = Mutexed<[any Error]>([])
        let window = source.subscribe(
            offset: 0,
            limit: 10,
            onValue: { _, _ in },
            onError: { error in
                failures.withValue { $0.append(error) }
                reported.signal()
            }
        )
        defer { window.cancel() }

        await reported.wait()
        #expect(failures.value.count == 1)
        #expect(failures.value.first is ReadFailed)
    }

    /// A tiny box so the error can be read back off the main actor.
    private final class Mutexed<Value>: @unchecked Sendable {
        private let lock = NSLock()
        private var stored: Value

        init(_ value: Value) { stored = value }

        var value: Value {
            lock.lock()
            defer { lock.unlock() }
            return stored
        }

        func withValue(_ change: (inout Value) -> Void) {
            lock.lock()
            change(&stored)
            lock.unlock()
        }
    }
}
