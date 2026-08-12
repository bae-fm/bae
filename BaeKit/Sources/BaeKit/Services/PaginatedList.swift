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
    private struct Segment {
        let range: Range<Int>
        let ids: [Row.ID]
    }

    /// Total row count from the most recent subscription value.
    public private(set) var totalCount: Int = 0

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

    /// Sorted, non-overlapping segments of loaded IDs. Only loaded positions
    /// are stored — no sparse pre-allocation.
    private var segments: [Segment] = []

    @ObservationIgnored
    private let pageSource: any PageSource<Row>
    @ObservationIgnored
    private let ingest: ([Row]) -> Void
    @ObservationIgnored
    /// The failure sink. It takes the error, not a rendered `DisplayError`:
    /// whether a failure is worth showing at all is core's answer (a cancellation
    /// is not), and `showError` is the one place that drops it.
    private let onError: (any Error) -> Void
    @ObservationIgnored
    private var subscriptions: [String: any PageSubscription] = [:]

    public init(
        pageSource: any PageSource<Row>,
        ingest: @escaping ([Row]) -> Void,
        onError: @escaping (any Error) -> Void
    ) {
        self.pageSource = pageSource
        self.ingest = ingest
        self.onError = onError
    }

    // MARK: - Queries

    /// Returns the ID at `position`.
    public func idAt(_ position: Int) -> Row.ID? {
        for seg in segments where seg.range.contains(position) {
            return seg.ids[position - seg.range.lowerBound]
        }
        return nil
    }

    /// Returns the position of `id` in the loaded segments, or nil if not loaded.
    public func position(of id: Row.ID) -> Int? {
        for seg in segments {
            if let local = seg.ids.firstIndex(of: id) {
                return seg.range.lowerBound + local
            }
        }
        return nil
    }

    /// All IDs currently held in loaded segments, in order.
    public var allLoadedIds: [Row.ID] {
        segments.flatMap(\.ids)
    }

    // MARK: - Load API (called from `.task`)

    /// Fetch the total count. Called once when the list is first mounted.
    public func loadInitial() async {
        initialLoadError = nil
        await subscribeRange(offset: 0, limit: 50, initial: true)
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
        if segments.contains(where: {
            $0.range.lowerBound <= offset
                && $0.range.upperBound >= end
        }) {
            return
        }
        await subscribeRange(offset: offset, limit: limit, initial: false)
    }

    private func subscribeRange(offset: Int, limit: Int, initial: Bool) async {
        let key = "\(offset):\(limit)"
        guard subscriptions[key] == nil else { return }
        await withCheckedContinuation { continuation in
            let waiter = InitialDeliveryWaiter(continuation)
            subscriptions[key] = pageSource.subscribe(
                offset: offset,
                limit: limit,
                onValue: { [weak self] rows, totalCount in
                    guard let self else {
                        waiter.resume()
                        return
                    }
                    self.totalCount = totalCount
                    self.segments.removeAll {
                        $0.range.lowerBound >= totalCount
                    }
                    self.initialLoadError = nil
                    self.ingest(rows)
                    let upper = min(offset + rows.count, totalCount)
                    guard offset < upper else {
                        self.segments.removeAll { $0.range.contains(offset) }
                        waiter.resume()
                        return
                    }
                    self.insertSegment(
                        Segment(
                            range: offset..<upper,
                            ids: rows.prefix(upper - offset).map(\.id)
                        )
                    )
                    waiter.resume()
                },
                onError: { [weak self] error in
                    guard let self else {
                        waiter.resume()
                        return
                    }
                    if initial, self.segments.isEmpty {
                        self.initialLoadError = DisplayError(error)
                        self.subscriptions.removeValue(forKey: key)?.cancel()
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
        }
    }

    // MARK: - Layout helpers

    /// Row count for a grid layout with the given column count.
    public func rowCount(columnCount: Int) -> Int {
        guard columnCount > 0 else {
            return 0
        }
        return (totalCount + columnCount - 1) / columnCount
    }

    // MARK: - Segment management

    private func insertSegment(_ new: Segment) {
        var lower = new.range.lowerBound
        var upper = new.range.upperBound
        var leftIds: [Row.ID] = []
        var rightIds: [Row.ID] = []
        var remaining: [Segment] = []

        for seg in segments {
            if seg.range.upperBound >= lower, seg.range.lowerBound <= upper {
                // Touches or overlaps: absorb the parts that extend beyond [lower, upper].
                if seg.range.lowerBound < lower {
                    leftIds =
                        Array(seg.ids.prefix(lower - seg.range.lowerBound))
                        + leftIds
                    lower = seg.range.lowerBound
                }
                if seg.range.upperBound > upper {
                    rightIds += Array(
                        seg.ids.suffix(seg.range.upperBound - upper)
                    )
                    upper = seg.range.upperBound
                }
                // The portion within [lower, upper] is superseded by new.ids.
            }
            else if seg.range.upperBound > new.range.lowerBound,
                seg.range.lowerBound < new.range.upperBound
            {
                // Stale segment overlaps the freshly-fetched range — discard.
            }
            else {
                remaining.append(seg)
            }
        }

        remaining.append(
            Segment(
                range: lower..<upper,
                ids: leftIds + new.ids + rightIds,
            )
        )
        segments = remaining.sorted {
            $0.range.lowerBound < $1.range.lowerBound
        }
    }

    // MARK: - Test/Preview support

    /// Seed segments synchronously for SwiftUI previews and tests.
    public func preloadForPreview(ids: [Row.ID]) {
        segments = [
            Segment(range: 0..<ids.count, ids: ids)
        ]
        totalCount = ids.count
    }
}
