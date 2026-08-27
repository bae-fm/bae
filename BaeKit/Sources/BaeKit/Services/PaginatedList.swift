import Foundation
import Observation
import os.log

private let logger = Logger.bae("PaginatedList")

// MARK: - PageSource

/// Source of a paginated stream of rows. Implementations know how to count
/// and fetch contiguous pages for a specific query (sort + scope).
///
/// A page source is paired with exactly one `PaginatedList`. Different
/// lists that share an underlying table (e.g. the full library grid and
/// an artist-scoped grid) use different page source instances — the
/// scope is baked into the source, not configured on the list.
public protocol PageSource<Row>: Sendable {
    associatedtype Row: Identifiable & Sendable

    /// Start a live page query. The initial value and every relevant committed
    /// database change deliver the page rows and total count together.
    func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([Row], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription
}

public protocol PageSubscription: AnyObject, Sendable {
    func cancel()
}

extension LiveSubscription: PageSubscription {}

private final class InitialDeliveryWaiter: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Never>?

    init(_ continuation: CheckedContinuation<Void, Never>) {
        self.continuation = continuation
    }

    func resume() {
        lock.lock()
        let continuation = continuation
        self.continuation = nil
        lock.unlock()
        continuation?.resume()
    }
}

// MARK: - Row load identity

// periphery:ignore
/// Identity a view folds into its row `.task(id:)` so swapping the list for a
/// new sort or filter restarts each visible row's load.
public struct LoadEpoch: Hashable {
    public let instance: ObjectIdentifier
}

// periphery:ignore
/// A row's load-task identity: which list epoch and which row position. Every
/// paginated consumer keys its per-row `.task(id:)` on this, so a row's load
/// restarts when its position changes or when the list is swapped.
public struct RowLoadID: Hashable {
    public let epoch: LoadEpoch
    public let index: Int

    public init(epoch: LoadEpoch, index: Int) {
        self.epoch = epoch
        self.index = index
    }
}

// MARK: - PaginatedList

/// A paginated, ordered view over one or more store slices.
///
/// Tracks loaded data as a sorted list of non-overlapping segments. Each page
/// subscription delivers both content and count whenever its query changes.
@MainActor
@Observable
public final class PaginatedList<Row: Identifiable & Sendable>
where Row.ID: Sendable {
    /// Total row count from the most recent subscription value.
    public private(set) var totalCount: Int = 0

    /// Advances after one subscribed page value has replaced its positions.
    /// A rendered viewport uses this boundary to restore the row it held while
    /// rows of different heights were materialised or changed above it.
    public private(set) var contentRevision: UInt64 = 0

    /// The cold count load (`loadInitial`) failed. The consuming grid reads this
    /// to show an error + Retry instead of the empty-library placeholder — a
    /// failed initial load is not an empty library. Only `loadInitial` sets it
    /// (a later page failure keeps data on screen and routes to
    /// `onError` instead); cleared when `loadInitial` starts again or succeeds.
    public private(set) var initialLoadError: DisplayError?

    /// This list's `LoadEpoch`. Consumers fold it into a row's `RowLoadID`
    /// `.task(id:)` so the row's load restarts when this list is swapped.
    public var loadEpoch: LoadEpoch {
        LoadEpoch(instance: ObjectIdentifier(self))
    }

    /// The ids this list holds, by position. Read during render, so it stays
    /// observed.
    private var segments = LoadedSegments<Row.ID>()

    @ObservationIgnored
    private let pageSource: any PageSource<Row>
    @ObservationIgnored
    private let ingest: ([Row]) -> Void
    @ObservationIgnored
    private let onSnapshot: (([Row.ID], Int) -> Void)?
    @ObservationIgnored
    /// The failure sink. It takes the error, not a rendered `DisplayError`:
    /// whether a failure is worth showing at all is core's answer (a cancellation
    /// is not), and `showError` is the one place that drops it.
    private let onError: (any Error) -> Void
    @ObservationIgnored
    private var subscriptions: [String: any PageSubscription] = [:]
    @ObservationIgnored
    private var subscriptionRanges: [String: Range<Int>] = [:]
    @ObservationIgnored
    private var subscriptionIdentities: [String: UUID] = [:]
    private static var maximumVisiblePageSubscriptions: Int { 3 }

    /// How many rows one page holds. Every page starts at a multiple of this,
    /// so the same page answers a whole screenful of consecutive rows.
    private static var pageSize: Int { 50 }

    public init(
        pageSource: any PageSource<Row>,
        ingest: @escaping ([Row]) -> Void,
        onError: @escaping (any Error) -> Void,
        onSnapshot: (([Row.ID], Int) -> Void)? = nil
    ) {
        self.pageSource = pageSource
        self.ingest = ingest
        self.onError = onError
        self.onSnapshot = onSnapshot
    }

    // MARK: - Queries

    /// Returns the ID at `position`.
    public func idAt(_ position: Int) -> Row.ID? {
        segments.id(at: position)
    }

    /// Returns the position of `id` in the loaded segments, or nil if not loaded.
    public func position(of id: Row.ID) -> Int? {
        segments.position(of: id)
    }

    /// All IDs currently held in loaded segments, in order.
    public var allLoadedIds: [Row.ID] {
        segments.allIds
    }

    // MARK: - Load API (called from `.task`)

    /// Fetch the total count. Called once when the list is first mounted.
    public func loadInitial() async {
        initialLoadError = nil
        await subscribeRange(offset: 0, limit: Self.pageSize, initial: true)
    }

    /// Load the page holding `position`, so the row there resolves to an id.
    ///
    /// The page is the aligned one, never a window centred on the row that
    /// asked. A centred window is a different `(offset, limit)` for every row
    /// index, so scrolling by a single row misses `loadRange`'s fast path,
    /// opens another page subscription, and pushes an older one past
    /// `maximumVisiblePageSubscriptions` — and the page that gets evicted is
    /// the one a row away, whose ids the viewport is drawing from. Every row on
    /// screen then resolves to nothing until the replacement value arrives.
    /// Aligned pages make consecutive rows ask for the same page, so the set
    /// changes only when the viewport crosses a boundary, and the page evicted
    /// then is two pages from anything visible.
    public func loadPage(containing position: Int) async {
        guard position >= 0 else { return }
        let start = (position / Self.pageSize) * Self.pageSize
        await loadRange(offset: start, limit: Self.pageSize)
    }

    /// Load a contiguous range of rows and intern them into the store.
    ///
    /// Fast-path: skips if a subscribed segment already covers the range.
    /// Concurrent callers asking for the same range (e.g. a grid
    /// whose visible cells all compute the same page offset on first paint)
    /// coalesce onto one in-flight fetch rather than each issuing a duplicate
    /// query — the segment fast-path can't dedupe a burst that starts before any
    /// fetch returns.
    public func loadRange(offset: Int, limit: Int) async {
        let end = min(offset + limit, totalCount)
        guard offset < end else {
            return
        }
        // Fast-path: an active subscription already covers this range.
        if segments.cover(offset..<end) {
            return
        }
        await subscribeRange(offset: offset, limit: limit, initial: false)
    }

    public func cancel() {
        for subscription in subscriptions.values {
            subscription.cancel()
        }
        subscriptions.removeAll()
        subscriptionRanges.removeAll()
        subscriptionIdentities.removeAll()
    }

    private func subscribeRange(offset: Int, limit: Int, initial: Bool) async {
        let key = "\(offset):\(limit)"
        guard subscriptions[key] == nil else { return }
        await withCheckedContinuation { continuation in
            let waiter = InitialDeliveryWaiter(continuation)
            let identity = UUID()
            subscriptionIdentities[key] = identity
            subscriptions[key] = pageSource.subscribe(
                offset: offset,
                limit: limit,
                onValue: { [weak self] rows, totalCount in
                    guard let self,
                        self.isCurrentSubscription(key, identity)
                    else {
                        waiter.resume()
                        return
                    }
                    self.apply(
                        rows,
                        forOffset: offset,
                        limit: limit,
                        totalCount: totalCount
                    )
                    waiter.resume()
                },
                onError: { [weak self] error in
                    guard let self,
                        self.isCurrentSubscription(key, identity)
                    else {
                        waiter.resume()
                        return
                    }
                    if initial, self.segments.isEmpty {
                        self.initialLoadError = DisplayError(error)
                        self.subscriptions.removeValue(forKey: key)?.cancel()
                        self.subscriptionIdentities.removeValue(forKey: key)
                    }
                    else {
                        logger.error(
                            "Live page failed: \(error.localizedDescription)"
                        )
                        self.onError(error)
                    }
                    waiter.resume()
                }
            )
            subscriptionRanges[key] = offset..<(offset + limit)
            evictPages(outsideWindowAround: offset..<(offset + limit))
        }
    }

    /// Take one page's delivered value: the rows go to the store, their ids
    /// take the positions the page was asked for, and the new total clips
    /// anything now past the end.
    private func apply(
        _ rows: [Row],
        forOffset offset: Int,
        limit: Int,
        totalCount: Int
    ) {
        self.totalCount = totalCount
        segments.clip(to: totalCount)
        initialLoadError = nil
        ingest(rows)
        let upper = min(offset + rows.count, totalCount)
        if offset < upper {
            segments.put(
                rows.prefix(upper - offset).map(\.id),
                at: offset,
                totalCount: totalCount
            )
        }
        else {
            // The page answered with nothing, so the positions it was asked
            // for hold nothing, and only those leave.
            segments.remove(
                offset..<max(offset, min(offset + limit, totalCount))
            )
        }
        onSnapshot?(allLoadedIds, totalCount)
        contentRevision += 1
    }

    private func isCurrentSubscription(_ key: String, _ identity: UUID) -> Bool
    {
        subscriptionIdentities[key] == identity
    }

    // MARK: - Layout helpers

    /// Row count for a grid layout with the given column count.
    public func rowCount(columnCount: Int) -> Int {
        guard columnCount > 0 else {
            return 0
        }
        return (totalCount + columnCount - 1) / columnCount
    }

    private func evictPages(outsideWindowAround visible: Range<Int>) {
        while subscriptionRanges.count > Self.maximumVisiblePageSubscriptions {
            let center = visible.lowerBound + visible.count / 2
            guard
                let key = subscriptionRanges.max(by: { lhs, rhs in
                    distance(lhs.value, from: center)
                        < distance(rhs.value, from: center)
                })?
                .key, let range = subscriptionRanges.removeValue(forKey: key)
            else { return }
            subscriptions.removeValue(forKey: key)?.cancel()
            subscriptionIdentities.removeValue(forKey: key)
            segments.remove(range)
        }
    }

    private func distance(_ range: Range<Int>, from position: Int) -> Int {
        abs(range.lowerBound + range.count / 2 - position)
    }

    // MARK: - Test/Preview support

    /// Seed segments synchronously for SwiftUI previews and tests.
    public func preloadForPreview(ids: [Row.ID]) {
        segments = LoadedSegments(ids)
        totalCount = ids.count
    }
}

// MARK: - LoadedSegments

/// Which id sits at which position, for the positions a list has loaded.
///
/// Held as sorted, non-overlapping runs rather than one sparse array: only
/// loaded positions cost anything, and pages that end up adjacent merge into
/// one run. Nothing here knows about subscriptions — a page being dropped and
/// its ids being forgotten are separate decisions, and the list makes the
/// second one deliberately.
private struct LoadedSegments<ID: Hashable> {
    private struct Run {
        let range: Range<Int>
        let ids: [ID]
    }

    private var runs: [Run] = []

    init() {}

    /// One run covering `ids` from position zero.
    init(_ ids: [ID]) {
        runs = ids.isEmpty ? [] : [Run(range: 0..<ids.count, ids: ids)]
    }

    var isEmpty: Bool { runs.isEmpty }

    /// Every id held, in position order.
    var allIds: [ID] { runs.flatMap(\.ids) }

    func id(at position: Int) -> ID? {
        for run in runs where run.range.contains(position) {
            return run.ids[position - run.range.lowerBound]
        }
        return nil
    }

    func position(of id: ID) -> Int? {
        for run in runs {
            if let local = run.ids.firstIndex(of: id) {
                return run.range.lowerBound + local
            }
        }
        return nil
    }

    /// Whether one run already covers all of `positions`.
    func cover(_ positions: Range<Int>) -> Bool {
        runs.contains {
            $0.range.lowerBound <= positions.lowerBound
                && $0.range.upperBound >= positions.upperBound
        }
    }

    /// Put `ids` at `offset` onwards, superseding whatever was there and
    /// absorbing the runs they touch.
    mutating func put(_ ids: [ID], at offset: Int, totalCount: Int) {
        let new = Run(range: offset..<(offset + ids.count), ids: ids)
        var lower = new.range.lowerBound
        var upper = new.range.upperBound
        var leftIds: [ID] = []
        var rightIds: [ID] = []
        var remaining: [Run] = []

        for run in runs {
            if run.range.upperBound >= lower, run.range.lowerBound <= upper {
                // Touches or overlaps: absorb the parts outside [lower, upper].
                if run.range.lowerBound < lower {
                    leftIds =
                        Array(run.ids.prefix(lower - run.range.lowerBound))
                        + leftIds
                    lower = run.range.lowerBound
                }
                if run.range.upperBound > upper {
                    rightIds += Array(
                        run.ids.suffix(run.range.upperBound - upper)
                    )
                    upper = run.range.upperBound
                }
                // The portion within [lower, upper] is superseded by `ids`.
            }
            else if run.range.upperBound > new.range.lowerBound,
                run.range.lowerBound < new.range.upperBound
            {
                // Stale run overlapping the freshly-fetched range — discard.
            }
            else {
                remaining.append(run)
            }
        }

        remaining.append(
            Run(range: lower..<upper, ids: leftIds + new.ids + rightIds)
        )
        runs = remaining.sorted { $0.range.lowerBound < $1.range.lowerBound }
        clip(to: totalCount)
    }

    /// Drop everything at or past `totalCount`.
    mutating func clip(to totalCount: Int) {
        runs = runs.compactMap { run in
            let upper = min(run.range.upperBound, totalCount)
            guard run.range.lowerBound < upper else { return nil }
            return Run(
                range: run.range.lowerBound..<upper,
                ids: Array(run.ids.prefix(upper - run.range.lowerBound))
            )
        }
    }

    /// Forget the ids at `removed`, splitting the run that holds them.
    mutating func remove(_ removed: Range<Int>) {
        runs = runs.flatMap { run -> [Run] in
            guard run.range.overlaps(removed) else { return [run] }
            var pieces: [Run] = []
            if run.range.lowerBound < removed.lowerBound {
                let count = removed.lowerBound - run.range.lowerBound
                pieces.append(
                    Run(
                        range: run.range.lowerBound..<removed.lowerBound,
                        ids: Array(run.ids.prefix(count))
                    )
                )
            }
            if run.range.upperBound > removed.upperBound {
                let start = removed.upperBound - run.range.lowerBound
                pieces.append(
                    Run(
                        range: removed.upperBound..<run.range.upperBound,
                        ids: Array(run.ids.dropFirst(start))
                    )
                )
            }
            return pieces
        }
    }
}
