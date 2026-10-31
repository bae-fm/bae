import Foundation

// The import list is a desktop surface: its bridge types are built behind the
// bridge's `desktop` feature, which the iOS bindings leave out.
#if os(macOS)

    /// One item of the import list, addressed by the key core computed for it.
    extension BridgeImportListItem: Identifiable {
        public var id: String {
            switch self {
            case .groupHeader(let stableKey, _, _, _, _),
                .candidate(let stableKey, _, _),
                .invalid(let stableKey, _, _, _):
                return stableKey
            }
        }
    }

    /// A page source over the import list, with the way to change what it is
    /// showing. The view (tab, filter, order) and the groups folded shut
    /// decide which items sit at which offsets, so changing them belongs to
    /// the source rather than to one page; so do the reads that ask where a
    /// candidate sits in the list.
    public struct ImportListPages: Sendable {
        public let source: any PageSource<BridgeImportListItem>
        private let applyView: @Sendable (BridgeImportListView) -> Void
        private let awaitAppliedView:
            @Sendable (BridgeImportListView) async throws -> Void
        private let fold:
            @Sendable (BridgeFolderReleaseDecisionKey, Bool) async throws ->
                Void
        private let foldAll: @Sendable (Bool) async throws -> Void
        private let locate:
            @Sendable (Data) async throws ->
                BridgeImportCandidateListLocation?
        private let firstIdentifying: @Sendable () async throws -> Data?

        public init(
            source: any PageSource<BridgeImportListItem>,
            setView: @escaping @Sendable (BridgeImportListView) -> Void,
            waitForView:
                @escaping @Sendable (BridgeImportListView) async throws -> Void,
            setGroupFolded:
                @escaping @Sendable (BridgeFolderReleaseDecisionKey, Bool)
                async throws -> Void,
            setAllGroupsFolded: @escaping @Sendable (Bool) async throws -> Void,
            locateCandidate:
                @escaping @Sendable (Data) async throws
                -> BridgeImportCandidateListLocation?,
            firstIdentifyingCandidate:
                @escaping @Sendable () async throws -> Data?
        ) {
            self.source = source
            applyView = setView
            awaitAppliedView = waitForView
            fold = setGroupFolded
            foldAll = setAllGroupsFolded
            locate = locateCandidate
            firstIdentifying = firstIdentifyingCandidate
        }

        public func setView(_ view: BridgeImportListView) {
            applyView(view)
        }

        /// Wait until the live list has delivered a read that reflects
        /// `view`: the read of the revision that accepted it, or a later one.
        /// `view` is asked for first unless it is the view last asked for. A
        /// later change of view answers the wait too, so the caller keeps
        /// track of which view it shows.
        public func waitForView(_ view: BridgeImportListView) async throws {
            try await awaitAppliedView(view)
        }

        /// Fold one group shut, or open it, once the view changes asked for
        /// before it have landed.
        public func setGroupFolded(
            _ group: BridgeFolderReleaseDecisionKey,
            _ folded: Bool
        ) async throws {
            try await fold(group, folded)
        }

        /// Fold every group shut, or open them all, as one change.
        public func setAllGroupsFolded(_ folded: Bool) async throws {
            try await foldAll(folded)
        }

        /// Where the candidate sits once revealed at its current placement:
        /// its tab, with the filters cleared and its group opened. `nil` when
        /// the list lists no such candidate.
        public func locateCandidate(_ candidateKey: Data) async throws
            -> BridgeImportCandidateListLocation?
        {
            try await locate(candidateKey)
        }

        /// The first candidate still being identified, in the list's order.
        public func firstIdentifyingCandidate() async throws -> Data? {
            try await firstIdentifying()
        }
    }

    /// The import list's pages, all served by one bridge subscription.
    ///
    /// The bridge object takes a set of windows and answers with one value holding
    /// every one of them, so the pages a `PaginatedList` asks for are registered
    /// here and handed the window that matches them. The chrome around the list —
    /// the tab counts, the watched folders — rides on the same value and goes to
    /// `onSummary`, Found's filter entries with their counts go to
    /// `onPendingFilterEntries`, and the selection — what it holds and can be
    /// told to do, and the revision the rows reflect — goes to `onSelection`
    /// once the rows have been handed out.
    public final class ImportListPageSource: PageSource, @unchecked Sendable {
        public typealias Row = BridgeImportListItem

        private struct WindowKey: Hashable {
            let offset: UInt64
            let limit: UInt64
        }

        private struct Sink {
            let value:
                @MainActor @Sendable ([BridgeImportListItem], Int) -> Void
            let error: @MainActor @Sendable (any Error) -> Void
        }

        private struct ViewWaiter {
            let revision: UInt64
            let continuation:
                CheckedContinuation<BridgeImportQueueSummary, any Error>
        }

        private enum ViewWaiterState {
            case awaitingRegistration(UInt64)
            case cancelled
            case failed(any Error)
            case waiting(ViewWaiter)
        }

        private struct DeliveredSummary {
            let revision: UInt64
            let summary: BridgeImportQueueSummary
        }

        private let subscription: any ImportListSubscriptionProtocol
        private let onSummary:
            @MainActor @Sendable (BridgeImportQueueSummary) -> Void
        private let onPendingFilterEntries:
            @MainActor @Sendable ([BridgePendingFilterEntry]) -> Void
        private let onSelection:
            @MainActor @Sendable (BridgeSelectionSummary, UInt64) -> Void
        private let lock = NSLock()
        private var sinks: [WindowKey: Sink] = [:]
        private var viewWaiters: [UUID: ViewWaiterState] = [:]
        private var latestSummary: DeliveredSummary?
        private var deliveries: Task<Void, Never>?
        /// The last view change asked for — a view, a fold — which the next
        /// one waits on, so they reach core in the order they were made.
        private var viewWrite: Task<Void, Never>?
        /// The last view asked for, and the write asking for it, which a wait
        /// for that view waits on rather than asking for it again.
        private var lastViewWrite:
            (view: BridgeImportListView, write: Task<UInt64, any Error>)?
        /// The read failure this source died of, kept so a page registered
        /// after it hears about it.
        ///
        /// The delivery task starts with this object, before the list has
        /// registered its first page, so a read that fails immediately — a
        /// database this build cannot read — fails with nothing to tell. That
        /// error used to be dropped, and the page that registered a moment
        /// later then waited on a delivery loop that had already returned: no
        /// rows, no summary, no alert, and an import tab that renders its
        /// "add a folder" prompt as though the library simply had none.
        private var failure: (any Error)?

        public init(
            subscription: any ImportListSubscriptionProtocol,
            onSummary:
                @escaping @MainActor @Sendable (BridgeImportQueueSummary)
                -> Void,
            onPendingFilterEntries:
                @escaping @MainActor @Sendable ([BridgePendingFilterEntry])
                -> Void,
            onSelection:
                @escaping @MainActor @Sendable (BridgeSelectionSummary, UInt64)
                -> Void
        ) {
            self.subscription = subscription
            self.onSummary = onSummary
            self.onPendingFilterEntries = onPendingFilterEntries
            self.onSelection = onSelection
            deliveries = Task { [weak self] in
                await self?.deliver()
            }
        }

        deinit {
            deliveries?.cancel()
            let subscription = self.subscription
            Task { try? await subscription.cancel() }
        }

        /// This source's pages, with the view control the list drives it by.
        public var pages: ImportListPages {
            let subscription = self.subscription
            return ImportListPages(
                source: self,
                setView: { [self] view in
                    let write = writeView(view)
                    Task { [weak self] in
                        if case .failure(let error) = await write.result {
                            self?.failEveryPage(with: error)
                        }
                    }
                },
                waitForView: { [self] view in
                    _ = try await deliveredSummary(afterApplying: view)
                },
                setGroupFolded: { [self] group, folded in
                    _ = try await changeView {
                        try await subscription.setGroupFolded(
                            group: group,
                            folded: folded
                        )
                    }
                    .value
                },
                setAllGroupsFolded: { [self] folded in
                    _ = try await changeView {
                        try await subscription.setAllGroupsFolded(
                            folded: folded
                        )
                    }
                    .value
                },
                locateCandidate: { key in
                    try await subscription.locateCandidate(candidateKey: key)
                },
                firstIdentifyingCandidate: {
                    try await subscription.firstIdentifyingCandidate()
                }
            )
        }

        public func subscribe(
            offset: Int,
            limit: Int,
            onValue:
                @escaping @MainActor @Sendable ([BridgeImportListItem], Int)
                -> Void,
            onError: @escaping @MainActor @Sendable (any Error) -> Void
        ) -> any PageSubscription {
            let key = WindowKey(offset: UInt64(offset), limit: UInt64(limit))
            lock.lock()
            sinks[key] = Sink(value: onValue, error: onError)
            let failure = self.failure
            let windows = requestedWindows()
            lock.unlock()
            if let failure {
                Task { @MainActor in onError(failure) }
                return PageWindow(source: self, key: key)
            }
            push(windows, failing: onError)
            return PageWindow(source: self, key: key)
        }

        /// Drop one page's window and ask for the rest. The value that answers
        /// carries no window for it, so nothing more is delivered to its sink.
        private func remove(_ key: WindowKey) {
            lock.lock()
            sinks.removeValue(forKey: key)
            let windows = requestedWindows()
            lock.unlock()
            push(windows, failing: nil)
        }

        /// Every registered page as a window, in offset order so the request is
        /// the same value for the same set of pages.
        private func requestedWindows() -> [BridgeLibraryPageWindow] {
            sinks.keys
                .sorted { ($0.offset, $0.limit) < ($1.offset, $1.limit) }
                .map {
                    BridgeLibraryPageWindow(offset: $0.offset, limit: $0.limit)
                }
        }

        private func push(
            _ windows: [BridgeLibraryPageWindow],
            failing onError: (@MainActor @Sendable (any Error) -> Void)?
        ) {
            do {
                try subscription.setWindows(windows: windows)
            }
            catch {
                // The page that just registered is waiting on its first value, so
                // it has to hear about a request that never reached core.
                if let onError {
                    Task { @MainActor in onError(error) }
                }
                else {
                    failEveryPage(with: error)
                }
            }
        }

        /// Every page registered right now, read under the lock.
        private func registeredSinks() -> [WindowKey: Sink] {
            lock.lock()
            defer { lock.unlock() }
            return sinks
        }

        private func deliver() async {
            while !Task.isCancelled {
                do {
                    let snapshot = try await subscription.next()
                    let sinks = registeredSinks()
                    let onSummary = self.onSummary
                    let onPendingFilterEntries = self.onPendingFilterEntries
                    let onSelection = self.onSelection
                    let summary = snapshot.summary
                    await MainActor.run {
                        onSummary(summary)
                        onPendingFilterEntries(snapshot.pendingFilterEntries)
                        for window in snapshot.windows {
                            let key = WindowKey(
                                offset: window.window.offset,
                                limit: window.window.limit
                            )
                            sinks[key]?
                                .value(window.items, Int(snapshot.totalCount))
                        }
                        onSelection(
                            snapshot.selection,
                            snapshot.selectionRevision
                        )
                    }
                    fulfillViewWaiters(
                        revision: snapshot.viewRevision,
                        summary: summary
                    )
                }
                catch {
                    if Task.isCancelled { return }
                    failEveryPage(with: error)
                    // A failed read leaves nothing to wait on; the list rebuilds
                    // itself from the error its pages just received.
                    return
                }
            }
        }

        private func failEveryPage(with error: any Error) {
            lock.lock()
            failure = error
            let sinks = self.sinks
            let waiters: [(UUID, ViewWaiter)] = viewWaiters.compactMap {
                element in
                guard case .waiting(let waiter) = element.value else {
                    return nil
                }
                return (element.key, waiter)
            }
            for (id, _) in waiters {
                viewWaiters.removeValue(forKey: id)
            }
            let awaiting: [UUID] = viewWaiters.compactMap { element in
                if case .awaitingRegistration = element.value {
                    return element.key
                }
                return nil
            }
            for id in awaiting {
                viewWaiters[id] = .failed(error)
            }
            lock.unlock()
            Task { @MainActor in
                for sink in sinks.values {
                    sink.error(error)
                }
            }
            for (_, waiter) in waiters {
                waiter.continuation.resume(throwing: error)
            }
        }
    }

    extension ImportListPageSource {
        /// Run `write`, a change to what the list shows answering with its
        /// view revision, once every change asked for before it has landed.
        private func changeView(
            _ write: @escaping @Sendable () async throws -> UInt64
        ) -> Task<UInt64, any Error> {
            lock.withLock { changeViewLocked(write) }
        }

        /// Ask core for `view`, unless it is the view last asked for, whose
        /// write is answered with instead.
        private func writeView(
            _ view: BridgeImportListView
        ) -> Task<UInt64, any Error> {
            let subscription = self.subscription
            return lock.withLock {
                if let last = lastViewWrite, last.view == view {
                    return last.write
                }
                let write = changeViewLocked {
                    try await subscription.setView(view: view)
                }
                lastViewWrite = (view, write)
                return write
            }
        }

        /// `changeView`, under the lock.
        private func changeViewLocked(
            _ write: @escaping @Sendable () async throws -> UInt64
        ) -> Task<UInt64, any Error> {
            let previous = viewWrite
            let task = Task {
                await previous?.value
                return try await write()
            }
            viewWrite = Task { _ = await task.result }
            return task
        }

        private func deliveredSummary(
            afterApplying view: BridgeImportListView
        ) async throws -> BridgeImportQueueSummary {
            let revision = try await writeView(view).value
            let id = UUID()
            let failure = lock.withLock {
                let failure = self.failure
                if failure == nil {
                    viewWaiters[id] = .awaitingRegistration(revision)
                }
                return failure
            }
            if let failure { throw failure }
            return try await withTaskCancellationHandler {
                try await withCheckedThrowingContinuation { continuation in
                    if let result = registerViewWaiter(
                        id: id,
                        revision: revision,
                        continuation: continuation
                    ) {
                        continuation.resume(with: result)
                    }
                }
            } onCancel: {
                self.cancelViewWaiter(id)
            }
        }

        private func registerViewWaiter(
            id: UUID,
            revision: UInt64,
            continuation:
                CheckedContinuation<BridgeImportQueueSummary, any Error>
        ) -> Result<BridgeImportQueueSummary, any Error>? {
            lock.lock()
            defer { lock.unlock() }
            switch viewWaiters[id] {
            case .cancelled:
                viewWaiters.removeValue(forKey: id)
                return .failure(CancellationError())
            case .failed(let error):
                viewWaiters.removeValue(forKey: id)
                return .failure(error)
            case .awaitingRegistration:
                if let delivered = deliveredViewResult(revision: revision) {
                    viewWaiters.removeValue(forKey: id)
                    return delivered
                }
                viewWaiters[id] = .waiting(
                    ViewWaiter(
                        revision: revision,
                        continuation: continuation
                    )
                )
                return nil
            case .waiting, .none:
                preconditionFailure(
                    "view waiter registration has one owner"
                )
            }
        }

        /// The summary a waiter on `revision` takes once a read reflecting
        /// it has been delivered: that revision's, or a later one's.
        private func deliveredViewResult(
            revision: UInt64
        ) -> Result<BridgeImportQueueSummary, any Error>? {
            guard let delivered = latestSummary, delivered.revision >= revision
            else { return nil }
            return .success(delivered.summary)
        }

        private func fulfillViewWaiters(
            revision: UInt64,
            summary: BridgeImportQueueSummary
        ) {
            lock.lock()
            latestSummary = DeliveredSummary(
                revision: revision,
                summary: summary
            )
            let ready: [(UUID, ViewWaiter)] = viewWaiters.compactMap {
                element in
                guard case .waiting(let waiter) = element.value,
                    waiter.revision <= revision
                else { return nil }
                return (element.key, waiter)
            }
            for (id, _) in ready {
                viewWaiters.removeValue(forKey: id)
            }
            lock.unlock()
            for (_, waiter) in ready {
                waiter.continuation.resume(returning: summary)
            }
        }

        private func cancelViewWaiter(_ id: UUID) {
            lock.lock()
            let waiter: ViewWaiter?
            switch viewWaiters[id] {
            case .awaitingRegistration:
                viewWaiters[id] = .cancelled
                waiter = nil
            case .waiting(let waiting):
                viewWaiters.removeValue(forKey: id)
                waiter = waiting
            case .failed:
                viewWaiters[id] = .cancelled
                waiter = nil
            case .cancelled, .none:
                waiter = nil
            }
            lock.unlock()
            waiter?.continuation.resume(throwing: CancellationError())
        }

        private final class PageWindow: PageSubscription, @unchecked Sendable {
            private let source: ImportListPageSource
            private let key: WindowKey

            init(source: ImportListPageSource, key: WindowKey) {
                self.source = source
                self.key = key
            }

            func cancel() {
                source.remove(key)
            }
        }
    }

    /// An in-memory import list for previews and tests. Serves contiguous slices
    /// of a fixed item list and reports one fixed summary.
    public struct ImportListPreviewPageSource: PageSource {
        public let items: [BridgeImportListItem]

        public init(items: [BridgeImportListItem]) {
            self.items = items
        }

        /// This source's pages. A fixed list shows one view, so changing the view
        /// or the folds changes nothing, and it finds no candidate.
        public var pages: ImportListPages {
            ImportListPages(
                source: self,
                setView: { _ in },
                waitForView: { _ in },
                setGroupFolded: { _, _ in },
                setAllGroupsFolded: { _ in },
                locateCandidate: { _ in nil },
                firstIdentifyingCandidate: { nil }
            )
        }

        public func subscribe(
            offset: Int,
            limit: Int,
            onValue:
                @escaping @MainActor @Sendable ([BridgeImportListItem], Int)
                -> Void,
            onError: @escaping @MainActor @Sendable (any Error) -> Void
        ) -> any PageSubscription {
            let total = items.count
            let start = min(offset, total)
            let end = min(start + limit, total)
            let page = Array(items[start..<end])
            let task = Task { @MainActor in
                onValue(page, total)
            }
            return PreviewWindow(task: task)
        }

        private final class PreviewWindow: PageSubscription, @unchecked Sendable
        {
            private let task: Task<Void, Never>

            init(task: Task<Void, Never>) {
                self.task = task
            }

            func cancel() {
                task.cancel()
            }
        }
    }
#endif
