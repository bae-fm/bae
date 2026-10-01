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

    @MainActor
    var sections: [BridgeLibraryBrowseSection] { get }
    /// Distinct items, when rows can show the same item more than once.
    @MainActor
    var itemCount: Int? { get }

    /// Start a live page query. The initial value and every relevant committed
    /// database change deliver the page rows and total count together.
    func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([Row], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription
}

extension PageSource {
    @MainActor
    public var sections: [BridgeLibraryBrowseSection] { [] }
    @MainActor
    public var itemCount: Int? { nil }
}

public protocol PageSubscription: AnyObject, Sendable {
    func cancel()
}

extension LiveSubscription: PageSubscription {}

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
/// Rows load a page at a time: the aligned run of `pageSize` positions that
/// holds them, each a live subscription delivering both content and count
/// whenever its query changes. The ids the pages delivered sit in a sorted list
/// of non-overlapping segments.
///
/// A page stays live for as long as anyone holds it — a row on screen, a reveal
/// on its way to a row — and a hold lasts as long as the task that took it.
/// Pages nobody holds stay live too, up to `maximumUnheldPages`, so scrolling
/// back finds them; past that the one farthest from the last page asked for is
/// dropped, and its ids with it. A held page is never dropped, so a page in use
/// never empties under the rows showing it.
@MainActor
@Observable
public final class PaginatedList<Row: Identifiable & Sendable & Equatable>
where Row.ID: Sendable {
    /// Total row count from the most recent subscription value.
    public private(set) var totalCount: Int = 0

    public var sections: [BridgeLibraryBrowseSection] { pageSource.sections }

    /// Advances when a subscribed page value changes what the list holds: the
    /// ids at its positions, the total, or a row's value. A value that repeats
    /// what the list already holds — a source answering every page again
    /// because another page was asked for — leaves it alone. A rendered
    /// viewport uses this boundary to restore the row it held while rows of
    /// different heights were materialised or changed above it.
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
    private var reportedCount: Int?
    @ObservationIgnored
    /// The failure sink. It takes the error, not a rendered `DisplayError`:
    /// whether a failure is worth showing at all is core's answer (a cancellation
    /// is not), and `showError` is the one place that drops it.
    private let onError: (any Error) -> Void
    /// The live pages, by the positions each covers.
    @ObservationIgnored
    private var pages: [Range<Int>: LivePage<Row>] = [:]
    /// How many holds each page has, by the positions it covers. A hold
    /// counts from before its page is asked for until its holder lets it go,
    /// whether or not the page is live in between.
    @ObservationIgnored
    private var holds: [Range<Int>: Int] = [:]
    /// The page asked for last, which unheld pages are dropped farthest from.
    @ObservationIgnored
    private var lastAsked: Range<Int>?

    private static var maximumUnheldPages: Int { 3 }

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

    public func section(at position: Int) -> BridgeLibraryBrowseSection? {
        sections.first {
            position >= Int($0.window.offset)
                && position < Int($0.window.offset + $0.window.limit)
        }
    }

    public func position(of id: Row.ID, in sectionId: String?) -> Int? {
        guard let sectionId else { return position(of: id) }
        return loadedEntries.first {
            $0.id == id && section(at: $0.position)?.id == sectionId
        }?
        .position
    }

    /// All IDs currently held in loaded segments, in order.
    public var allLoadedIds: [Row.ID] {
        segments.allIds
    }

    /// Every loaded position with the id held there, in position order.
    public var loadedEntries: [(position: Int, id: Row.ID)] {
        segments.entries
    }

    // MARK: - Loading

    /// Read the first page, and with it the total count. Called when the list
    /// is first mounted, and again to retry a failed first read.
    public func loadInitial() async {
        initialLoadError = nil
        await load(Self.page(containing: 0), initial: true)
    }

    /// Hold the page holding `position` for as long as the calling task runs:
    /// the page loads, stays live through every page asked for after it, and
    /// is let go when the task is cancelled. A row on screen holds its page
    /// from a `.task`, which SwiftUI cancels when the row leaves.
    public func holdPage(containing position: Int) async {
        await withPage(containing: position) {
            await Task.untilCancelled()
        }
    }

    /// Hold the page holding `position` while `body` runs. `body` starts once
    /// the page has answered, so the ids it holds are there to read, and they
    /// stay there until `body` returns.
    ///
    /// The page is the aligned one, never a window centred on `position`: a
    /// centred window is a different page for every row, so rows a row apart
    /// would each open a page of their own, where aligned pages let a
    /// screenful of rows share one.
    public func withPage<T>(
        containing position: Int,
        _ body: @MainActor () async throws -> T
    ) async rethrows -> T {
        precondition(position >= 0, "a list position is never negative")
        let page = Self.page(containing: position)
        holds[page, default: 0] += 1
        defer { release(page) }
        await load(page, initial: false)
        return try await body()
    }

    public func cancel() {
        for page in pages.values {
            page.subscription?.cancel()
            page.abandon()
        }
        pages.removeAll()
    }

    private static func page(containing position: Int) -> Range<Int> {
        let start = (position / pageSize) * pageSize
        return start..<(start + pageSize)
    }

    /// Make `page` live, unless it is already, and wait for its first answer.
    /// A page past the end has nothing to load; neither does one a preview
    /// seeded. Every caller asking for a page already on its way waits on that
    /// one subscription rather than opening its own.
    private func load(_ page: Range<Int>, initial: Bool) async {
        lastAsked = page
        if let live = pages[page] {
            guard !live.answered else { return }
            await withCheckedContinuation { continuation in
                pages[page]?.waiters?.append(continuation)
            }
            return
        }
        if !initial {
            let end = min(page.upperBound, totalCount)
            guard page.lowerBound < end, !segments.cover(page.lowerBound..<end)
            else { return }
        }
        await withCheckedContinuation { continuation in
            subscribe(page, initial: initial, waiter: continuation)
        }
    }

    private func subscribe(
        _ page: Range<Int>,
        initial: Bool,
        waiter: CheckedContinuation<Void, Never>
    ) {
        let identity = UUID()
        pages[page] = LivePage(identity: identity, waiters: [waiter])
        let subscription = pageSource.subscribe(
            offset: page.lowerBound,
            limit: page.count,
            onValue: { [weak self] rows, totalCount in
                guard let self, self.isCurrent(page, identity) else { return }
                self.apply(rows, page: page, totalCount: totalCount)
                self.pages[page]?.answer()
            },
            onError: { [weak self] error in
                guard let self, self.isCurrent(page, identity) else { return }
                if initial, self.segments.isEmpty {
                    self.initialLoadError = DisplayError(error)
                    self.drop(page)
                }
                else {
                    logger.error(
                        "Live page failed: \(error.localizedDescription)"
                    )
                    self.onError(error)
                    self.pages[page]?.answer()
                }
            }
        )
        // A source may answer, and fail the first page, before handing back
        // its subscription; one the list no longer keeps is ended here.
        guard isCurrent(page, identity) else {
            subscription.cancel()
            return
        }
        pages[page]?.subscription = subscription
        dropUnheldPages()
    }

    /// Take one page's delivered value: the rows go to the store, their ids
    /// take the positions the page was asked for, and the new total clips
    /// anything now past the end. Only what differs from what the list holds
    /// is written, so a value that repeats it notifies no observer.
    private func apply(_ rows: [Row], page: Range<Int>, totalCount: Int) {
        if initialLoadError != nil {
            initialLoadError = nil
        }
        let rowsChanged = pages[page]?.rows != rows
        pages[page]?.rows = rows
        let offset = page.lowerBound
        var next = segments
        next.clip(to: totalCount)
        let upper = min(offset + rows.count, totalCount)
        if offset < upper {
            next.put(
                rows.prefix(upper - offset).map(\.id),
                at: offset,
                totalCount: totalCount
            )
        }
        else {
            // The page answered with nothing, so the positions it was asked
            // for hold nothing, and only those leave.
            next.remove(offset..<max(offset, min(page.upperBound, totalCount)))
        }
        let positionsChanged = next != segments
        let totalChanged = totalCount != self.totalCount
        // Rows whose positions moved are taken again too: the store may have
        // let them go while their positions were gone.
        if rowsChanged || positionsChanged {
            ingest(rows)
        }
        if totalChanged {
            self.totalCount = totalCount
        }
        if positionsChanged {
            segments = next
        }
        let itemCount = pageSource.itemCount ?? totalCount
        if positionsChanged || reportedCount != itemCount {
            reportedCount = itemCount
            onSnapshot?(allLoadedIds, itemCount)
        }
        if rowsChanged || positionsChanged || totalChanged {
            contentRevision += 1
        }
    }

    private func isCurrent(_ page: Range<Int>, _ identity: UUID) -> Bool {
        pages[page]?.identity == identity
    }

    /// Let go of one hold on `page`; a page nobody holds any more may then be
    /// dropped.
    private func release(_ page: Range<Int>) {
        guard let count = holds[page] else {
            preconditionFailure("released a page hold never taken")
        }
        if count > 1 {
            holds[page] = count - 1
        }
        else {
            holds.removeValue(forKey: page)
            dropUnheldPages()
        }
    }

    /// Drop the unheld pages past `maximumUnheldPages`, farthest from the
    /// page asked for last first. Held pages are never dropped and never
    /// counted.
    private func dropUnheldPages() {
        let unheld = pages.keys.filter { holds[$0] == nil }
        guard unheld.count > Self.maximumUnheldPages, let lastAsked else {
            return
        }
        let center = lastAsked.lowerBound + lastAsked.count / 2
        let farthestFirst = unheld.sorted {
            distance($0, from: center) > distance($1, from: center)
        }
        for page in farthestFirst.prefix(
            unheld.count - Self.maximumUnheldPages
        ) {
            drop(page)
            segments.remove(page)
        }
    }

    /// End `page`'s subscription, letting anyone waiting on its first answer
    /// go on without it.
    private func drop(_ page: Range<Int>) {
        guard let live = pages.removeValue(forKey: page) else { return }
        live.subscription?.cancel()
        live.abandon()
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

// MARK: - LivePage

/// One page's subscription, what it last delivered, and who is waiting for its
/// first answer.
private struct LivePage<Row: Equatable> {
    /// Tells the subscription's callbacks apart from those of an earlier one
    /// over the same positions, which the list ended.
    let identity: UUID
    /// Nil only while the source has not handed it back yet.
    var subscription: (any PageSubscription)?
    /// The loads waiting for the page's first value or failure; nil once it
    /// has answered.
    var waiters: [CheckedContinuation<Void, Never>]?
    /// The rows its last value delivered, so a value that repeats them is not
    /// taken into the store again.
    var rows: [Row]?

    init(identity: UUID, waiters: [CheckedContinuation<Void, Never>]) {
        self.identity = identity
        self.waiters = waiters
    }

    var answered: Bool { waiters == nil }

    /// The page answered: everyone waiting goes on.
    mutating func answer() {
        let waiting = waiters ?? []
        waiters = nil
        for waiter in waiting {
            waiter.resume()
        }
    }

    /// Let everyone waiting go on, for a page the list no longer keeps.
    func abandon() {
        for waiter in waiters ?? [] {
            waiter.resume()
        }
    }
}

/// Which id sits at which position, for the positions a list has loaded.
///
/// Held as sorted, non-overlapping runs rather than one sparse array: only
/// loaded positions cost anything, and pages that end up adjacent merge into
/// one run. Nothing here knows about subscriptions — a page being dropped and
/// its ids being forgotten are separate decisions, and the list makes the
/// second one deliberately.
private struct LoadedSegments<ID: Hashable>: Equatable {
    private struct Run: Equatable {
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

    /// Every held position with its id, in position order.
    var entries: [(position: Int, id: ID)] {
        runs.flatMap { run in
            zip(run.range, run.ids).map { (position: $0, id: $1) }
        }
    }

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
