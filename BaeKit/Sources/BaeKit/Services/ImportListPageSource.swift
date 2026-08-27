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
                .invalid(let stableKey, _, _):
                return stableKey
            }
        }
    }

    /// A page source over the import list, with the way to change the view it is
    /// showing. The view (tab, filter, folded groups, order) decides which items
    /// sit at which offsets, so changing it belongs to the source rather than to
    /// one page.
    public struct ImportListPages: Sendable {
        public let source: any PageSource<BridgeImportListItem>
        private let applyView: @Sendable (BridgeImportListView) -> Void
        private let resolveFirstUnidentifiedPosition:
            @Sendable (
                BridgeImportListView,
                BridgeFirstUnidentifiedRowRef
            ) async throws -> Int?
        private let awaitAppliedView:
            @Sendable (BridgeImportListView) async throws -> Void

        public init(
            source: any PageSource<BridgeImportListItem>,
            setView: @escaping @Sendable (BridgeImportListView) -> Void,
            firstUnidentifiedPosition candidatePosition:
                @escaping @Sendable (
                    BridgeImportListView,
                    BridgeFirstUnidentifiedRowRef
                ) async throws -> Int?,
            waitForView:
                @escaping @Sendable (BridgeImportListView) async throws -> Void
        ) {
            self.source = source
            applyView = setView
            resolveFirstUnidentifiedPosition = candidatePosition
            awaitAppliedView = waitForView
        }

        public func setView(_ view: BridgeImportListView) {
            applyView(view)
        }

        /// Apply `view`, wait for the exact revision that accepted it, and
        /// return the summary's first-unidentified position in that delivered
        /// view, if the target still names that row.
        public func firstUnidentifiedPosition(
            for target: BridgeFirstUnidentifiedRowRef,
            afterApplying view: BridgeImportListView
        ) async throws -> Int? {
            try await resolveFirstUnidentifiedPosition(
                view,
                target
            )
        }

        /// Apply `view` and wait until the live list has delivered the exact
        /// revision that accepted it.
        public func waitForView(_ view: BridgeImportListView) async throws {
            try await awaitAppliedView(view)
        }
    }

    /// The import list's pages, all served by one bridge subscription.
    ///
    /// The bridge object takes a set of windows and answers with one value holding
    /// every one of them, so the pages a `PaginatedList` asks for are registered
    /// here and handed the window that matches them. The chrome around the list —
    /// the tab counts, the Ready set, the group keys — rides on the same value and
    /// goes to `onSummary`.
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
        private let lock = NSLock()
        private var sinks: [WindowKey: Sink] = [:]
        private var viewWaiters: [UUID: ViewWaiterState] = [:]
        private var latestSummary: DeliveredSummary?
        private var deliveries: Task<Void, Never>?
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
                -> Void
        ) {
            self.subscription = subscription
            self.onSummary = onSummary
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
            ImportListPages(
                source: self,
                setView: { [self] view in
                    do {
                        let revision = try subscription.setView(view: view)
                        cancelViewWaiters(before: revision)
                    }
                    catch {
                        failEveryPage(with: error)
                    }
                },
                firstUnidentifiedPosition: { [self] view, target in
                    let summary = try await deliveredSummary(
                        afterApplying: view
                    )
                    return summary.firstUnidentifiedPosition(for: target)
                },
                waitForView: { [self] view in
                    _ = try await deliveredSummary(afterApplying: view)
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
                    let summary = snapshot.summary
                    await MainActor.run {
                        onSummary(summary)
                        for window in snapshot.windows {
                            let key = WindowKey(
                                offset: window.window.offset,
                                limit: window.window.limit
                            )
                            sinks[key]?
                                .value(window.items, Int(snapshot.totalCount))
                        }
                    }
                    fulfillViewWaiters(
                        revision: snapshot.requestRevision,
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
        private func deliveredSummary(
            afterApplying view: BridgeImportListView
        ) async throws -> BridgeImportQueueSummary {
            let revision = try subscription.setView(view: view)
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

        private func deliveredViewResult(
            revision: UInt64
        ) -> Result<BridgeImportQueueSummary, any Error>? {
            guard let delivered = latestSummary else { return nil }
            if delivered.revision == revision {
                return .success(delivered.summary)
            }
            if delivered.revision > revision {
                return .failure(CancellationError())
            }
            return nil
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
                if waiter.revision == revision {
                    waiter.continuation.resume(returning: summary)
                }
                else {
                    waiter.continuation.resume(
                        throwing: CancellationError()
                    )
                }
            }
        }

        private func cancelViewWaiters(before revision: UInt64) {
            lock.lock()
            let cancelled: [(UUID, ViewWaiter)] =
                viewWaiters.compactMap { element in
                    guard case .waiting(let waiter) = element.value,
                        waiter.revision < revision
                    else { return nil }
                    return (element.key, waiter)
                }
            let awaiting: [UUID] = viewWaiters.compactMap { element in
                guard
                    case .awaitingRegistration(let awaitingRevision) =
                        element.value,
                    awaitingRevision < revision
                else { return nil }
                return element.key
            }
            for (id, _) in cancelled {
                viewWaiters.removeValue(forKey: id)
            }
            for id in awaiting {
                viewWaiters[id] = .cancelled
            }
            lock.unlock()
            for (_, waiter) in cancelled {
                waiter.continuation.resume(throwing: CancellationError())
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
        /// changes nothing.
        public var pages: ImportListPages {
            ImportListPages(
                source: self,
                setView: { _ in },
                firstUnidentifiedPosition: { _, target in
                    items.firstIndex {
                        $0.id == target.stableKey
                    }
                },
                waitForView: { _ in }
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

    extension BridgeImportQueueSummary {
        fileprivate func firstUnidentifiedPosition(
            for target: BridgeFirstUnidentifiedRowRef
        ) -> Int? {
            guard
                firstUnidentified?.candidateKey == target.candidateKey,
                firstUnidentified?.stableKey == target.stableKey,
                firstUnidentified?.groupKey == target.groupKey
            else {
                return nil
            }
            return firstUnidentified?.visiblePosition.map(Int.init)
        }
    }
#endif
