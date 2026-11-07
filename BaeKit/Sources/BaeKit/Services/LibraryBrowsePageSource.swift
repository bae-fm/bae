import Foundation
import Observation

@MainActor
@Observable
private final class LibraryBrowseSections {
    var values: [BridgeLibraryBrowseSection] = []
    var itemCount: Int?
    nonisolated init() {}
}

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
    public let itemCount: Int?
    /// The album grid's artist sections when they changed since the delivery
    /// before and these are the rows they fit; nil says the last stand.
    public let sections: [BridgeLibraryBrowseSection]?

    public init(
        windows: [Window],
        totalCount: Int,
        itemCount: Int? = nil,
        sections: [BridgeLibraryBrowseSection]? = nil
    ) {
        self.windows = windows
        self.totalCount = totalCount
        self.itemCount = itemCount
        self.sections = sections
    }
}

/// A live browse over one library list (albums, artists, or composers under
/// one sort): which windows of the list it reads is changed in place, and each
/// value it delivers answers every window it was asked for at once.
public struct LibraryBrowseQuery<Row: Sendable>: Sendable {
    public let setWindows: @Sendable ([BridgeLibraryPageWindow]) throws -> Void
    public let values: PulledValues<LibraryBrowseDelivery<Row>>

    public init(
        setWindows:
            @escaping @Sendable ([BridgeLibraryPageWindow]) throws
            -> Void,
        values: PulledValues<LibraryBrowseDelivery<Row>>
    ) {
        self.setWindows = setWindows
        self.values = values
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
        /// What the page last heard: a value names only the windows whose
        /// rows changed, so a page it does not name is told the new total
        /// with these rows.
        var heard: (rows: [Row], total: Int)?
    }

    private let query: LibraryBrowseQuery<Row>
    private let sectionStore = LibraryBrowseSections()
    @MainActor
    public var sections: [BridgeLibraryBrowseSection] { sectionStore.values }
    @MainActor
    public var itemCount: Int? { sectionStore.itemCount }
    private let lock = NSLock()
    private var sinks: [WindowKey: Sink] = [:]
    /// How many `coalescingRequests` are running, and whether a change to the
    /// windows waits for the last of them to return.
    private var requestHolds = 0
    private var requestOwed = false
    private var deliveries:
        ScreenSubscription<PulledValues<LibraryBrowseDelivery<Row>>>?
    /// The read failure this source died of, kept so a page registered after
    /// it hears about it instead of waiting on reads that already stopped.
    private var failure: (any Error)?

    public init(query: LibraryBrowseQuery<Row>) {
        self.query = query
        deliveries = ScreenSubscription(
            query.values,
            onValue: { [weak self] delivery in self?.deliver(delivery) },
            onError: { [weak self] error in
                self?.failEveryPage(with: error)
                self?.deliveries?.stop()
            }
        )
    }

    public func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([Row], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let key = WindowKey(offset: UInt64(offset), limit: UInt64(limit))
        let (failure, windows, held) = lock.withLock {
            sinks[key] = Sink(value: onValue, error: onError)
            if requestHolds > 0 { requestOwed = true }
            return (self.failure, requestedWindows(), requestHolds > 0)
        }
        if let failure {
            Task { @MainActor in onError(failure) }
            return PageWindow(source: self, key: key)
        }
        if !held {
            push(windows, failing: onError)
        }
        return PageWindow(source: self, key: key)
    }

    private func remove(_ key: WindowKey) {
        let (windows, held) = lock.withLock {
            sinks.removeValue(forKey: key)
            if requestHolds > 0 { requestOwed = true }
            return (requestedWindows(), requestHolds > 0)
        }
        if !held {
            push(windows, failing: nil)
        }
    }

    /// Run `body`, which adds and drops pages, as one change: the windows
    /// that are left are asked for once, when it returns.
    public func coalescingRequests(_ body: () -> Void) {
        lock.withLock { requestHolds += 1 }
        body()
        let windows: [BridgeLibraryPageWindow]? = lock.withLock {
            requestHolds -= 1
            guard requestHolds == 0, requestOwed else { return nil }
            requestOwed = false
            return requestedWindows()
        }
        if let windows {
            push(windows, failing: nil)
        }
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

    @MainActor
    private func deliver(_ delivery: LibraryBrowseDelivery<Row>) {
        let calls = hearingThis(delivery)
        // Sections and rows land in this one turn: a delivery that carries
        // sections is the one whose rows they fit.
        if let sections = delivery.sections, sections != sectionStore.values {
            sectionStore.values = sections
        }
        sectionStore.itemCount = delivery.itemCount
        for (sink, rows) in calls {
            sink.value(rows, delivery.totalCount)
        }
    }

    /// The pages this value tells something, each with the rows to tell it:
    /// the rows of its window when the value names it, and otherwise the rows
    /// it already has when the total moved.
    private func hearingThis(
        _ delivery: LibraryBrowseDelivery<Row>
    ) -> [(Sink, [Row])] {
        let total = delivery.totalCount
        return lock.withLock {
            for window in delivery.windows {
                let key = WindowKey(
                    offset: window.window.offset,
                    limit: window.window.limit
                )
                sinks[key]?.heard = (window.rows, total)
            }
            var calls: [(Sink, [Row])] = []
            for (key, sink) in sinks {
                guard let heard = sink.heard else { continue }
                let named = delivery.windows.contains {
                    $0.window.offset == key.offset
                        && $0.window.limit == key.limit
                }
                guard named || heard.total != total else { continue }
                sinks[key]?.heard = (heard.rows, total)
                calls.append((sink, heard.rows))
            }
            return calls
        }
    }

    /// Tell every page the source died of `error`. Only the first failure
    /// is told: a later one, such as a request to the stopped read, follows
    /// from it.
    private func failEveryPage(with error: any Error) {
        let sinks: [WindowKey: Sink]? = lock.withLock {
            guard failure == nil else { return nil }
            failure = error
            return self.sinks
        }
        guard let sinks else { return }
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
