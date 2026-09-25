import Foundation

/// What the Storage Manager asks its query to read: the list under one sort
/// and filter, and the windows of it on screen.
public struct StorageBrowseView: Equatable, Sendable {
    public let sort: BridgeStorageSort
    public let filter: BridgeStorageFilter
    public let windows: [BridgeLibraryPageWindow]

    public init(
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        windows: [BridgeLibraryPageWindow]
    ) {
        self.sort = sort
        self.filter = filter
        self.windows = windows
    }
}

/// The Storage Manager list read live through one query: its sort, filter,
/// and windows change in place, and each value answers every window at once,
/// named with the sort and filter it was read under.
public struct StorageBrowseQuery: Sendable {
    public let setView: @Sendable (StorageBrowseView) throws -> Void
    public let next: @Sendable () async throws -> BridgeStorageBrowseSnapshot
    public let cancel: @Sendable () async -> Void

    public init(
        setView: @escaping @Sendable (StorageBrowseView) throws -> Void,
        next:
            @escaping @Sendable () async throws -> BridgeStorageBrowseSnapshot,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setView = setView
        self.next = next
        self.cancel = cancel
    }

    init(_ subscription: any StorageBrowseSubscriptionProtocol) {
        self.init(
            setView: {
                try subscription.setView(
                    sort: $0.sort,
                    filter: $0.filter,
                    windows: $0.windows
                )
            },
            next: { try await subscription.next() },
            cancel: { try? await subscription.cancel() }
        )
    }
}

extension StorageBrowseQuery {
    /// A query over a fixed, already-sorted list, for previews and tests:
    /// every view is answered with its windows' slices of `rows`, whatever
    /// the sort and filter.
    public static func fixed(_ rows: [BridgeStorageRow]) -> StorageBrowseQuery {
        let requests = FixedRequests<StorageBrowseView>()
        return StorageBrowseQuery(
            setView: { requests.request($0) },
            next: {
                let view = try await requests.next()
                return BridgeStorageBrowseSnapshot(
                    sort: view.sort,
                    filter: view.filter,
                    windows: view.windows.map { window in
                        let start = min(Int(window.offset), rows.count)
                        let end = min(start + Int(window.limit), rows.count)
                        return BridgeStorageBrowseWindow(
                            window: window,
                            rows: Array(rows[start..<end])
                        )
                    },
                    totalCount: UInt64(rows.count),
                    totalSize: UInt64(
                        rows.reduce(Int64(0)) { $0 + $1.release.totalSize }
                    )
                )
            },
            cancel: { requests.close() }
        )
    }
}

/// The Storage Manager's pages, all served by one query for as long as the
/// manager is open.
///
/// A new sort or filter is a new view on the same query: `setView` drops the
/// pages the previous view's list registered, and the next list registers its
/// own. The whole set of registered windows under the current view is what
/// the query reads, and a value read under another view is not delivered.
public final class StorageBrowsePageSource: PageSource, @unchecked Sendable {
    private struct WindowKey: Hashable {
        let offset: UInt64
        let limit: UInt64
    }

    private struct Sink {
        let value: @MainActor @Sendable ([BridgeStorageRow], Int) -> Void
        let error: @MainActor @Sendable (any Error) -> Void
    }

    private let query: StorageBrowseQuery
    private let onTotalSize: @MainActor @Sendable (UInt64) -> Void
    private let lock = NSLock()
    private var sort: BridgeStorageSort
    private var filter: BridgeStorageFilter
    /// Moves with each view, so a page of a previous view's list that
    /// cancels late cannot take a window the current list registered.
    private var generation = 0
    private var sinks: [WindowKey: Sink] = [:]
    private var deliveries: Task<Void, Never>?
    /// The read failure this source died of, kept so a page registered after
    /// it hears about it instead of waiting on a loop that already returned.
    private var failure: (any Error)?

    public init(
        query: StorageBrowseQuery,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        onTotalSize: @escaping @MainActor @Sendable (UInt64) -> Void
    ) {
        self.query = query
        self.sort = sort
        self.filter = filter
        self.onTotalSize = onTotalSize
        deliveries = Task { [weak self] in
            await self?.deliver()
        }
    }

    public convenience init(
        library: Library,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        onTotalSize: @escaping @MainActor @Sendable (UInt64) -> Void
    ) {
        self.init(
            query: library.storageBrowse(sort, filter),
            sort: sort,
            filter: filter,
            onTotalSize: onTotalSize
        )
    }

    deinit {
        close()
    }

    /// End the read: no value is delivered after this, and core drops the
    /// query.
    public func close() {
        deliveries?.cancel()
        let query = self.query
        Task { await query.cancel() }
    }

    /// Show the list under `sort` and `filter`, with no pages until its new
    /// list registers them.
    public func setView(sort: BridgeStorageSort, filter: BridgeStorageFilter) {
        lock.withLock {
            self.sort = sort
            self.filter = filter
            generation += 1
            sinks = [:]
        }
        push(failing: nil)
    }

    public func subscribe(
        offset: Int,
        limit: Int,
        onValue:
            @escaping @MainActor @Sendable ([BridgeStorageRow], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let key = WindowKey(offset: UInt64(offset), limit: UInt64(limit))
        let (failure, generation) = lock.withLock {
            sinks[key] = Sink(value: onValue, error: onError)
            return (self.failure, self.generation)
        }
        let page = PageWindow(source: self, key: key, generation: generation)
        if let failure {
            Task { @MainActor in onError(failure) }
            return page
        }
        push(failing: onError)
        return page
    }

    private func remove(_ key: WindowKey, generation: Int) {
        let removed = lock.withLock {
            guard generation == self.generation else { return false }
            sinks.removeValue(forKey: key)
            return true
        }
        if removed {
            push(failing: nil)
        }
    }

    /// The current view with every registered page as a window, in offset
    /// order, so the same pages make the same request.
    private func push(
        failing onError: (@MainActor @Sendable (any Error) -> Void)?
    ) {
        let view = lock.withLock {
            StorageBrowseView(
                sort: sort,
                filter: filter,
                windows: sinks.keys
                    .sorted { ($0.offset, $0.limit) < ($1.offset, $1.limit) }
                    .map {
                        BridgeLibraryPageWindow(
                            offset: $0.offset,
                            limit: $0.limit
                        )
                    }
            )
        }
        do {
            try query.setView(view)
        }
        catch {
            // The page that just registered waits on its first value, so it
            // has to hear about a request that never reached core.
            if let onError {
                Task { @MainActor in onError(error) }
            }
            else {
                failEveryPage(with: error)
            }
        }
    }

    private func deliver() async {
        while !Task.isCancelled {
            do {
                let snapshot = try await query.next()
                let sinks: [WindowKey: Sink]? = lock.withLock {
                    guard snapshot.sort == sort, snapshot.filter == filter
                    else { return nil }
                    return self.sinks
                }
                guard let sinks else { continue }
                let onTotalSize = onTotalSize
                await MainActor.run {
                    onTotalSize(snapshot.totalSize)
                    for window in snapshot.windows {
                        let key = WindowKey(
                            offset: window.window.offset,
                            limit: window.window.limit
                        )
                        sinks[key]?.value(window.rows, Int(snapshot.totalCount))
                    }
                }
            }
            catch {
                if Task.isCancelled { return }
                failEveryPage(with: error)
                return
            }
        }
    }

    private func failEveryPage(with error: any Error) {
        let sinks = lock.withLock {
            failure = error
            return self.sinks
        }
        Task { @MainActor in
            for sink in sinks.values {
                sink.error(error)
            }
        }
    }

    private final class PageWindow: PageSubscription, @unchecked Sendable {
        private let source: StorageBrowsePageSource
        private let key: WindowKey
        private let generation: Int

        init(source: StorageBrowsePageSource, key: WindowKey, generation: Int) {
            self.source = source
            self.key = key
            self.generation = generation
        }

        func cancel() {
            source.remove(key, generation: generation)
        }
    }
}
