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
protocol PageSource<Row>: Sendable {
    associatedtype Row: Identifiable & Sendable

    /// Total row count for this query.
    func count() async throws -> Int

    /// Fetch a contiguous page of rows starting at `offset`.
    func page(offset: Int, limit: Int) async throws -> [Row]
}

// MARK: - Row load identity

// periphery:ignore
/// Identity a view folds into its row `.task(id:)` (via `RowLoadID`) so row
/// loads restart in the two cases that leave rows stale: the list is
/// *invalidated* (generation bumps) or the list instance is *swapped* for a
/// fresh one (a new sort/filter builds a new list, starting at generation 0
/// with no loaded segments). Keying on row position or generation alone misses
/// the swap — a fresh instance can sit at the same generation, so the per-row
/// task never restarts and the swapped-in rows stay stuck on placeholders. The
/// instance identity closes that gap.
struct LoadEpoch: Hashable {
    let instance: ObjectIdentifier
    let generation: Int
}

// periphery:ignore
/// A row's load-task identity: which list epoch and which row position. Every
/// paginated consumer keys its per-row `.task(id:)` on this, so a row's load
/// restarts when its position changes or when the list is swapped/invalidated.
struct RowLoadID: Hashable {
    let epoch: LoadEpoch
    let index: Int
}

// MARK: - PaginatedList

/// A paginated, ordered view over one or more store slices.
///
/// Tracks loaded data as a sorted list of non-overlapping segments, each
/// tagged with the generation at which it was fetched. No pre-allocation —
/// only loaded positions are stored.
///
/// Shape changes (add / remove / reorder) are handled by `invalidate()`,
/// which fetches the new count and bumps the generation. Visible rows
/// restart their load tasks (task identity includes `loadEpoch`) and call
/// `loadRange()`, which re-fetches any range whose cached segment is stale.
/// Stale segments keep their data visible until fresh data replaces them.
///
/// ## Lists are views, not subscribers
///
/// A `PaginatedList` observes nothing. Content mutations re-render via
/// `@Observable` on the store slices. Shape mutations (add / remove /
/// reorder) require an explicit `invalidate()` call from the action handler.
@MainActor
@Observable
final class PaginatedList<Row: Identifiable & Sendable> where Row.ID: Sendable {
    private struct Segment {
        let range: Range<Int>
        let ids: [Row.ID]
        let generation: Int
    }

    /// Total row count from the most recent `loadInitial()` or `invalidate()`.
    private(set) var totalCount: Int = 0

    /// Bumped by `invalidate()`. Folded into `loadEpoch`, the value views key
    /// their row `.task(id:)` on so loads restart when the list is invalidated.
    private(set) var generation: Int = 0

    /// This list's `LoadEpoch`. Consumers fold it into a row's `RowLoadID`
    /// `.task(id:)` so the row's load restarts when this list is invalidated or
    /// swapped for a fresh instance.
    var loadEpoch: LoadEpoch {
        LoadEpoch(instance: ObjectIdentifier(self), generation: generation)
    }

    /// Sorted, non-overlapping segments of loaded IDs. Only loaded positions
    /// are stored — no sparse pre-allocation.
    private var segments: [Segment] = []

    @ObservationIgnored
    private let pageSource: any PageSource<Row>
    @ObservationIgnored
    private let ingest: ([Row]) -> Void
    @ObservationIgnored
    private var reloadTask: Task<Void, Never>?
    // In-flight `loadRange` fetches, keyed "offset:end:generation", so concurrent
    // callers asking for the same range coalesce onto one DB query instead of
    // each issuing a duplicate.
    @ObservationIgnored
    private var inFlight: [String: Task<Void, Never>] = [:]

    init(pageSource: any PageSource<Row>, ingest: @escaping ([Row]) -> Void) {
        self.pageSource = pageSource
        self.ingest = ingest
    }

    // MARK: - Queries

    /// Returns the ID at `position`, preferring the highest-generation segment
    /// so fresh data wins over stale when they transiently overlap.
    func idAt(_ position: Int) -> Row.ID? {
        var best: Segment?
        for seg in segments where seg.range.contains(position) {
            if best.map({ seg.generation > $0.generation }) ?? true {
                best = seg
            }
        }
        return best.map { $0.ids[position - $0.range.lowerBound] }
    }

    /// Returns the position of `id` in the loaded segments, or nil if not loaded.
    func position(of id: Row.ID) -> Int? {
        for seg in segments {
            if let local = seg.ids.firstIndex(of: id) {
                return seg.range.lowerBound + local
            }
        }
        return nil
    }

    /// All IDs currently held in loaded segments, in order.
    var allLoadedIds: [Row.ID] {
        segments.flatMap(\.ids)
    }

    // MARK: - Load API (called from `.task`)

    /// Fetch the total count. Called once when the list is first mounted.
    func loadInitial() async {
        do {
            let count =
                try await Task.detached { [pageSource] in
                    try await pageSource.count()
                }
                .value
            totalCount = count
            segments = []
        }
        catch {
            logger.error("Failed to load count: \(error.localizedDescription)")
        }
    }

    /// Load a contiguous range of rows and intern them into the store.
    ///
    /// Fast-path: skips if a current-generation segment already covers the range.
    /// Concurrent callers asking for the same range and generation (e.g. a grid
    /// whose visible cells all compute the same page offset on first paint)
    /// coalesce onto one in-flight fetch rather than each issuing a duplicate
    /// query — the segment fast-path can't dedupe a burst that starts before any
    /// fetch returns.
    func loadRange(offset: Int, limit: Int) async {
        let end = min(offset + limit, totalCount)
        guard offset < end else {
            return
        }
        let gen = generation

        // Fast-path: a current-generation segment fully covers this range.
        if segments.contains(where: {
            $0.generation == gen && $0.range.lowerBound <= offset
                && $0.range.upperBound >= end
        }) {
            return
        }

        let key = "\(offset):\(end):\(gen)"
        if let existing = inFlight[key] {
            await existing.value
            return
        }

        let task = Task {
            await self.fetchRange(
                offset: offset,
                end: end,
                limit: limit,
                gen: gen
            )
        }
        inFlight[key] = task
        await task.value
        inFlight[key] = nil
    }

    private func fetchRange(offset: Int, end: Int, limit: Int, gen: Int) async {
        do {
            let rows =
                try await Task.detached { [pageSource] in
                    try await pageSource.page(offset: offset, limit: limit)
                }
                .value
            // Drop result if generation advanced while fetching — the restarted
            // task will re-fetch at the new generation.
            guard generation == gen else {
                return
            }
            ingest(rows)
            insertSegment(
                Segment(
                    range: offset..<end,
                    ids: rows.map(\.id),
                    generation: gen
                )
            )
        }
        catch {
            logger.error(
                "Failed to load range [\(offset) ..< \(end)]: \(error.localizedDescription)"
            )
        }
    }

    // MARK: - Invalidation

    /// Mark the list stale: fetch the new count, bump the generation.
    ///
    /// Visible rows restart their `.task(id:)` (which keys on `loadEpoch`) and
    /// call `loadRange()`, which sees stale segments and re-fetches. Stale
    /// segments remain visible until replaced, so the grid never flashes to
    /// empty.
    func invalidate() {
        reloadTask?.cancel()
        reloadTask = Task { [weak self] in
            guard let self else {
                return
            }
            do {
                let pageSource = pageSource
                let newCount =
                    try await Task.detached {
                        try await pageSource.count()
                    }
                    .value
                totalCount = newCount
                generation &+= 1
            }
            catch is CancellationError {
            }
            catch {
                logger.error(
                    "invalidate() failed: \(error.localizedDescription)"
                )
            }
        }
    }

    // MARK: - Layout helpers

    /// Row count for a grid layout with the given column count.
    func rowCount(columnCount: Int) -> Int {
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
            if seg.generation == new.generation, seg.range.upperBound >= lower,
                seg.range.lowerBound <= upper
            {
                // Same-gen, touches or overlaps: absorb the parts that extend beyond [lower, upper].
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
                generation: new.generation
            )
        )
        segments = remaining.sorted {
            $0.range.lowerBound < $1.range.lowerBound
        }
    }

    // MARK: - Test/Preview support

    // periphery:ignore
    /// Await the in-flight reload task if there is one. Used by tests to
    /// synchronize on post-`invalidate()` state without racing.
    func awaitReload() async {
        await reloadTask?.value
    }

    /// Seed segments synchronously for SwiftUI previews and tests.
    func preloadForPreview(ids: [Row.ID]) {
        segments = [
            Segment(range: 0..<ids.count, ids: ids, generation: generation)
        ]
        totalCount = ids.count
    }
}
