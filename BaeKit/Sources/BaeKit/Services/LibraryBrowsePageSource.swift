import Foundation

/// One value a library browse query delivered: each window it was asked for,
/// with the rows that landed in it, and the whole list's row count.
public struct LibraryBrowseDelivery<Row: Sendable>: Sendable {
    public struct Window: Sendable {
        public let window: BridgeLibraryPageWindow
        public let rows: [Row]

        public init(window: BridgeLibraryPageWindow, rows: [Row]) {
            self.window = window
            self.rows = rows
        }
    }

    public let windows: [Window]
    public let totalCount: Int

    public init(windows: [Window], totalCount: Int) {
        self.windows = windows
        self.totalCount = totalCount
    }
}

/// A live browse over one library list (albums, artists, or composers under
/// one sort): which windows of the list it reads is changed in place, and each
/// value it delivers answers every window it was asked for at once.
public struct LibraryBrowseQuery<Row: Sendable>: Sendable {
    public let setWindows: @Sendable ([BridgeLibraryPageWindow]) throws -> Void
    public let next: @Sendable () async throws -> LibraryBrowseDelivery<Row>
    public let cancel: @Sendable () async -> Void

    public init(
        setWindows:
            @escaping @Sendable ([BridgeLibraryPageWindow]) throws
            -> Void,
        next: @escaping @Sendable () async throws -> LibraryBrowseDelivery<Row>,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setWindows = setWindows
        self.next = next
        self.cancel = cancel
    }

    /// A query over a fixed, already-sorted list, for previews and tests:
    /// every window request is answered with its slice.
    public static func fixed(_ rows: [Row]) -> LibraryBrowseQuery<Row> {
        let requests = FixedRequests()
        return LibraryBrowseQuery(
            setWindows: { requests.request($0) },
            next: {
                let windows = try await requests.next()
                return LibraryBrowseDelivery(
                    windows: windows.map { window in
                        let start = min(Int(window.offset), rows.count)
                        let end = min(start + Int(window.limit), rows.count)
                        return .init(
                            window: window,
                            rows: Array(rows[start..<end])
                        )
                    },
                    totalCount: rows.count
                )
            },
            cancel: { requests.close() }
        )
    }
}

/// The window requests a fixed query answers: the latest one not yet
/// answered, handed to the one reader waiting for it.
private final class FixedRequests: @unchecked Sendable {
    private let lock = NSLock()
    private var pending: [BridgeLibraryPageWindow]?
    private var waiter:
        CheckedContinuation<[BridgeLibraryPageWindow], any Error>?
    private var closed = false

    func request(_ windows: [BridgeLibraryPageWindow]) {
        let waiter: CheckedContinuation<[BridgeLibraryPageWindow], any Error>? =
            lock.withLock {
                if let waiter = self.waiter {
                    self.waiter = nil
                    return waiter
                }
                pending = windows
                return nil
            }
        waiter?.resume(returning: windows)
    }

    func next() async throws -> [BridgeLibraryPageWindow] {
        try await withCheckedThrowingContinuation { continuation in
            let result: Result<[BridgeLibraryPageWindow], any Error>? =
                lock.withLock {
                    if closed { return .failure(CancellationError()) }
                    if let pending {
                        self.pending = nil
                        return .success(pending)
                    }
                    waiter = continuation
                    return nil
                }
            if let result { continuation.resume(with: result) }
        }
    }

    func close() {
        let waiter: CheckedContinuation<[BridgeLibraryPageWindow], any Error>? =
            lock.withLock {
                closed = true
                let waiter = self.waiter
                self.waiter = nil
                return waiter
            }
        waiter?.resume(throwing: CancellationError())
    }
}

/// The pages of one library list, all served by one browse query.
///
/// `PaginatedList` asks for pages one at a time; each page registers its
/// window here, and the whole set of registered windows is what the query
/// reads. One read per database change answers every visible page, instead of
/// one live query per page each rerunning on its own. Dropping a page removes
/// its window, and the value that answers carries nothing more for it.
public final class LibraryBrowsePageSource<Row: Identifiable & Sendable>:
    PageSource, @unchecked Sendable
{
    private struct WindowKey: Hashable {
        let offset: UInt64
        let limit: UInt64
    }

    private struct Sink {
        let value: @MainActor @Sendable ([Row], Int) -> Void
        let error: @MainActor @Sendable (any Error) -> Void
    }

    private let query: LibraryBrowseQuery<Row>
    private let lock = NSLock()
    private var sinks: [WindowKey: Sink] = [:]
    private var deliveries: Task<Void, Never>?
    /// The read failure this source died of, kept so a page registered after
    /// it hears about it instead of waiting on a loop that already returned.
    private var failure: (any Error)?

    public init(query: LibraryBrowseQuery<Row>) {
        self.query = query
        deliveries = Task { [weak self] in
            await self?.deliver()
        }
    }

    deinit {
        deliveries?.cancel()
        let query = self.query
        Task { await query.cancel() }
    }

    public func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([Row], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let key = WindowKey(offset: UInt64(offset), limit: UInt64(limit))
        let (failure, windows) = lock.withLock {
            sinks[key] = Sink(value: onValue, error: onError)
            return (self.failure, requestedWindows())
        }
        if let failure {
            Task { @MainActor in onError(failure) }
            return PageWindow(source: self, key: key)
        }
        push(windows, failing: onError)
        return PageWindow(source: self, key: key)
    }

    private func remove(_ key: WindowKey) {
        let windows = lock.withLock {
            sinks.removeValue(forKey: key)
            return requestedWindows()
        }
        push(windows, failing: nil)
    }

    /// Every registered page as a window, in offset order, so the same pages
    /// make the same request.
    private func requestedWindows() -> [BridgeLibraryPageWindow] {
        sinks.keys
            .sorted { ($0.offset, $0.limit) < ($1.offset, $1.limit) }
            .map { BridgeLibraryPageWindow(offset: $0.offset, limit: $0.limit) }
    }

    private func push(
        _ windows: [BridgeLibraryPageWindow],
        failing onError: (@MainActor @Sendable (any Error) -> Void)?
    ) {
        do {
            try query.setWindows(windows)
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
                let delivery = try await query.next()
                let sinks = lock.withLock { self.sinks }
                await MainActor.run {
                    for window in delivery.windows {
                        let key = WindowKey(
                            offset: window.window.offset,
                            limit: window.window.limit
                        )
                        sinks[key]?.value(window.rows, delivery.totalCount)
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
        private let source: LibraryBrowsePageSource
        private let key: WindowKey

        init(source: LibraryBrowsePageSource, key: WindowKey) {
            self.source = source
            self.key = key
        }

        func cancel() {
            source.remove(key)
        }
    }
}
