import BaeKit
import Foundation
import Observation
import Testing

@testable import bae

// MARK: - The paged list

/// How many pages the paged list has taken delivery of.
@MainActor
private final class DeliveredPages {
    var count = 0
}

private final class ObservedChange: @unchecked Sendable {
    private let lock = NSLock()
    private var raised = false

    var isSet: Bool { lock.withLock { raised } }

    func set() {
        lock.withLock { raised = true }
    }
}

@MainActor
private final class ViewDeliveryOutcome {
    enum State: Equatable {
        case waiting
        case delivered
        case failed
    }

    var state = State.waiting
}

@Suite("Import list page source")
struct ImportListPageSourceTests {
    private struct ViewReadFailed: Error {}

    private struct SnapshotWindow {
        let window: BridgeLibraryPageWindow
        let keys: [String]

        init(offset: UInt64, limit: UInt64, keys: [String]) {
            self.window = BridgeLibraryPageWindow(
                offset: offset,
                limit: limit
            )
            self.keys = keys
        }
    }

    /// A stub bridge subscription recording the windows asked for and handing
    /// back queued values.
    private final class StubListSubscription: ImportListSubscriptionProtocol,
        @unchecked Sendable
    {
        private let lock = NSLock()
        private var windows: [[BridgeLibraryPageWindow]] = []
        private var pending: [Result<BridgeImportListSnapshot, any Error>] = []
        private var viewRevision: UInt64 = 0
        private var views: [BridgeImportListView] = []
        private var setViewHook: (() -> Void)?
        private var askedCount = 0
        private var countedEntries: [BridgePendingFilterEntry] = []
        private var waiter:
            CheckedContinuation<BridgeImportListSnapshot, any Error>?

        var requestedWindows: [[BridgeLibraryPageWindow]] {
            lock.lock()
            defer { lock.unlock() }
            return windows
        }

        var requestedViews: [BridgeImportListView] {
            lock.lock()
            defer { lock.unlock() }
            return views
        }

        /// How many times the source has asked for a value; an ask past a
        /// delivered value means that value was handled.
        var asks: Int {
            lock.withLock { askedCount }
        }

        func setWindows(windows: [BridgeLibraryPageWindow]) throws {
            lock.lock()
            self.windows.append(windows)
            lock.unlock()
        }

        func setView(view: BridgeImportListView) throws -> UInt64 {
            lock.lock()
            viewRevision += 1
            views.append(view)
            let revision = viewRevision
            let hook = setViewHook
            lock.unlock()
            hook?()
            return revision
        }

        func onSetView(_ hook: @escaping () -> Void) {
            lock.withLock { setViewHook = hook }
        }

        func cancel() async throws {}

        /// What core counts under Found's filter entries from now on.
        func count(_ entries: [BridgePendingFilterEntry]) {
            lock.withLock { countedEntries = entries }
        }

        func pendingFilterEntries() -> [BridgePendingFilterEntry] {
            lock.withLock { countedEntries }
        }

        func deliver(_ snapshot: BridgeImportListSnapshot) {
            lock.lock()
            let waiter = self.waiter
            self.waiter = nil
            if waiter == nil { pending.append(.success(snapshot)) }
            lock.unlock()
            waiter?.resume(returning: snapshot)
        }

        func fail(_ error: any Error) {
            lock.lock()
            let waiter = self.waiter
            self.waiter = nil
            if waiter == nil { pending.append(.failure(error)) }
            lock.unlock()
            waiter?.resume(throwing: error)
        }

        func next() async throws -> BridgeImportListSnapshot {
            try await withCheckedThrowingContinuation { continuation in
                lock.lock()
                askedCount += 1
                if pending.isEmpty {
                    waiter = continuation
                    lock.unlock()
                }
                else {
                    let snapshot = pending.removeFirst()
                    lock.unlock()
                    continuation.resume(with: snapshot)
                }
            }
        }
    }

    private func item(_ key: String) -> BridgeImportListItem {
        .candidate(
            stableKey: "candidate:\(key)",
            row: identifiedRow(key, title: key),
            isGroupMember: false
        )
    }

    private func snapshot(
        _ windows: [SnapshotWindow],
        totalCount: UInt64,
        requestRevision: UInt64 = 0
    ) -> BridgeImportListSnapshot {
        BridgeImportListSnapshot(
            windows: windows.map { fixture in
                BridgeImportListWindow(
                    window: fixture.window,
                    items: fixture.keys.map(item)
                )
            },
            totalCount: totalCount,
            summary: BridgeImportQueueSummary(
                counts: BridgeTriageTabCounts(
                    pending: UInt32(totalCount),
                    done: 0,
                    skipped: 0
                ),
                watchedFolders: [BridgeWatchedFolder(path: "/w", name: "w")],
                folderScanStatuses: [],
                folderScanActivity: nil,
                groupKeys: [],
                pendingCovers: [],
                narrowed: nil,
                narrowing: BridgeImportListNarrowing(
                    tab: .pending,
                    filterText: "",
                    pendingFilter: .all
                ),
                firstSelectedPosition: nil
            ),
            selectionRevision: 0,
            requestRevision: requestRevision,
            cause: .requestChanged
        )
    }
}

extension ImportListPageSourceTests {
    /// The pages ask the subscription for Found's filter entries each time
    /// they are asked, with no delivery between: the counts are core's as
    /// they stand then.
    @Test("the filter entries are the subscription's when asked")
    func theFilterEntriesAreTheSubscriptionsWhenAsked() {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let pages = source.pages
        let idle = [
            BridgePendingFilterEntry(filter: .all, count: 2, selectable: true),
            BridgePendingFilterEntry(
                filter: .inProgress,
                count: 0,
                selectable: false
            ),
        ]
        subscription.count(idle)
        #expect(pages.pendingFilterEntries() == idle)

        let running = [
            BridgePendingFilterEntry(filter: .all, count: 2, selectable: true),
            BridgePendingFilterEntry(
                filter: .inProgress,
                count: 1,
                selectable: true
            ),
        ]
        subscription.count(running)
        #expect(pages.pendingFilterEntries() == running)
    }

    @MainActor
    @Test("a view waits for the revision that answers it")
    func aViewWaitsForTheRevisionThatAnswersIt() async throws {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let view = BridgeImportListView(
            tab: .pending,
            filterText: "",
            pendingFilter: .all,
            collapsedGroups: [],
            order: .pathAscending
        )
        let outcome = ViewDeliveryOutcome()
        Task {
            do {
                try await source.pages.waitForView(view)
                outcome.state = .delivered
            }
            catch {
                outcome.state = .failed
            }
        }
        try await Wait.until({ subscription.requestedViews == [view] })

        // The source's second ask comes after it handled revision 0, and the yield
        // runs this test after any waiter revision 0 wrongly released.
        subscription.deliver(
            snapshot([], totalCount: 70, requestRevision: 0)
        )
        try await Wait.until { subscription.asks >= 2 }
        await Task.yield()
        #expect(outcome.state == .waiting)

        subscription.deliver(
            snapshot([], totalCount: 70, requestRevision: 1)
        )
        try await Wait.until({ outcome.state != .waiting })
        #expect(outcome.state == .delivered)
    }

    @MainActor
    @Test("a view wait's cancellation cannot miss registration")
    func viewWaitCancellationAtRegistration() async {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let task = Task {
            try await source.pages.waitForView(
                BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    pendingFilter: .all,
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }

        task.cancel()

        do {
            _ = try await task.value
            Issue.record("a cancelled view wait returned")
        }
        catch is CancellationError {}
        catch {
            Issue.record("unexpected cancellation error: \(error)")
        }
    }

    @MainActor
    @Test("a view wait's registration receives a source failure")
    func viewWaitFailureAtRegistration() async throws {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let task = Task {
            try await source.pages.waitForView(
                BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    pendingFilter: .all,
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }
        try await Wait.until({ !subscription.requestedViews.isEmpty })

        subscription.fail(ViewReadFailed())

        do {
            _ = try await task.value
            Issue.record("a failed view wait returned")
        }
        catch is ViewReadFailed {}
        catch {
            Issue.record("unexpected view wait error: \(error)")
        }
    }

    @Test("a failure between view acceptance and registration is retained")
    func viewWaitFailureBeforeRegistration() async {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let failureDelivered = DispatchSemaphore(value: 0)
        _ = source.subscribe(
            offset: 0,
            limit: 1,
            onValue: { _, _ in },
            onError: { _ in failureDelivered.signal() }
        )
        subscription.onSetView {
            subscription.fail(ViewReadFailed())
            failureDelivered.wait()
        }
        let pages = source.pages
        let task = Task {
            try await pages.waitForView(
                BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    pendingFilter: .all,
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }

        do {
            _ = try await task.value
            Issue.record("a failed view wait returned")
        }
        catch is ViewReadFailed {}
        catch {
            Issue.record("unexpected view wait error: \(error)")
        }
    }

    @MainActor
    @Test("one value updates every registered page, and its summary")
    func oneValueUpdatesEveryPage() async throws {
        let subscription = StubListSubscription()
        var summaries: [BridgeImportQueueSummary] = []
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { summaries.append($0) },
            onSelectionRevision: { _ in }
        )
        var first: [String] = []
        var second: [String] = []
        var totals: [Int] = []
        _ = source.subscribe(
            offset: 0,
            limit: 2,
            onValue: { items, total in
                first = items.map(\.id)
                totals.append(total)
            },
            onError: { _ in }
        )
        _ = source.subscribe(
            offset: 2,
            limit: 2,
            onValue: { items, total in
                second = items.map(\.id)
                totals.append(total)
            },
            onError: { _ in }
        )

        subscription.deliver(
            snapshot(
                [
                    SnapshotWindow(
                        offset: 0,
                        limit: 2,
                        keys: ["/w/a", "/w/b"]
                    ),
                    SnapshotWindow(offset: 2, limit: 2, keys: ["/w/c"]),
                ],
                totalCount: 3
            )
        )
        try await Wait.until({ totals.count == 2 })

        #expect(first == ["candidate:/w/a", "candidate:/w/b"])
        #expect(second == ["candidate:/w/c"])
        #expect(totals == [3, 3])
        #expect(summaries.last?.counts.pending == 3)
    }

    @MainActor
    @Test("cancelling one page asks for the windows that are left")
    func cancellingOnePageShrinksTheWindows() async throws {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        _ = source.subscribe(
            offset: 0,
            limit: 2,
            onValue: { _, _ in },
            onError: { _ in }
        )
        let second = source.subscribe(
            offset: 2,
            limit: 2,
            onValue: { _, _ in },
            onError: { _ in }
        )

        #expect(
            subscription.requestedWindows.last?.map(\.offset) == [0, 2]
        )

        second.cancel()
        #expect(subscription.requestedWindows.last?.map(\.offset) == [0])
    }

    @MainActor
    @Test("a redelivered window and a new one both keep every loaded item")
    func deliveriesNeverDropALoadedItem() async throws {
        let total = 60
        let keys = (0..<total).map { String(format: "/w/%03d", $0) }
        let subscription = StubListSubscription()
        // Each snapshot handed out, whether or not it changed anything.
        let snapshots = DeliveredPages()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in snapshots.count += 1 },
            onSelectionRevision: { _ in }
        )
        let importStore = ImportStore()
        let delivered = DeliveredPages()
        let list = PaginatedList<BridgeImportListItem>(
            pageSource: source,
            ingest: {
                delivered.count += 1
                importStore.ingest($0)
            },
            onError: { _ in },
            onSnapshot: { ids, _ in importStore.retainItems(ids) }
        )
        let firstPage = Array(keys[0..<50])

        async let initial: Void = list.loadInitial()
        try await Wait.until({ !subscription.requestedWindows.isEmpty })
        subscription.deliver(
            snapshot(
                [SnapshotWindow(offset: 0, limit: 50, keys: firstPage)],
                totalCount: UInt64(total)
            )
        )
        await initial
        try await Wait.until({ delivered.count == 1 })
        #expect(loadedKeys(list, importStore, 0..<50) == firstPage)
        let revision = list.contentRevision

        // A commit that leaves this window's rows where they were: nothing is
        // taken in again, and the list's content stays as it was.
        subscription.deliver(
            snapshot(
                [SnapshotWindow(offset: 0, limit: 50, keys: firstPage)],
                totalCount: UInt64(total)
            )
        )
        try await Wait.until({ snapshots.count == 2 })
        #expect(delivered.count == 1)
        #expect(list.contentRevision == revision)
        #expect(loadedKeys(list, importStore, 0..<50) == firstPage)

        // Scrolling past the page boundary registers a second window; the first
        // window's rows stay resolvable meanwhile.
        async let next: Void = list.loadPage(containing: 55)
        try await Wait.until({ subscription.requestedWindows.last?.count == 2 })
        #expect(loadedKeys(list, importStore, 0..<50) == firstPage)

        subscription.deliver(pagedSnapshot(keys))
        await next
        try await Wait.until({ snapshots.count == 3 })
        #expect(loadedKeys(list, importStore, 0..<60) == keys)

        // A new candidate at the top moves every row down one.
        let grown = ["/w/new"] + keys
        subscription.deliver(pagedSnapshot(grown))
        try await Wait.until({ snapshots.count == 4 })
        #expect(list.totalCount == total + 1)
        #expect(loadedKeys(list, importStore, 0..<61) == grown)
    }

    @MainActor
    @Test("ingest writes a changed row and leaves an equal page unwritten")
    func ingestWritesOnlyChangedRows() {
        let store = ImportStore()
        let rows = ["/w/a", "/w/b"].map(item)
        store.ingest(rows)
        let notified = ObservedChange()
        withObservationTracking {
            _ = store.items
        } onChange: {
            notified.set()
        }

        store.ingest(rows)
        #expect(!notified.isSet)

        let recovered = BridgeImportListItem.candidate(
            stableKey: "candidate:/w/b",
            row: identifiedRow(
                "/w/b",
                title: "/w/b",
                cover: .local(
                    file: BridgeFileVersion(
                        path: "/w/b/cover.jpg",
                        size: 4096,
                        modifiedAtNs: 1_700_000_000_000_000_000
                    )
                )
            ),
            isGroupMember: false
        )
        store.ingest([rows[0], recovered])
        #expect(notified.isSet)
        #expect(store.items["candidate:/w/b"] == recovered)
    }

    /// Every key of `keys` as 50-row windows from the top, the way core
    /// answers a list holding those pages.
    private func pagedSnapshot(_ keys: [String]) -> BridgeImportListSnapshot {
        snapshot(
            stride(from: 0, to: keys.count, by: 50)
                .map { offset in
                    SnapshotWindow(
                        offset: UInt64(offset),
                        limit: 50,
                        keys: Array(keys[offset..<min(offset + 50, keys.count)])
                    )
                },
            totalCount: UInt64(keys.count)
        )
    }

    /// The keys the list holds at `positions`, resolved as a row does.
    @MainActor
    private func loadedKeys(
        _ list: PaginatedList<BridgeImportListItem>,
        _ importStore: ImportStore,
        _ positions: Range<Int>
    ) -> [String] {
        positions.compactMap { position in
            guard let id = list.idAt(position),
                let item = importStore.items[id]
            else { return nil }
            switch item {
            case .candidate(_, let row, _): return row.candidateKey
            case .imported(_, let row): return row.candidateKey
            case .groupHeader, .invalid: return nil
            }
        }
    }
}
