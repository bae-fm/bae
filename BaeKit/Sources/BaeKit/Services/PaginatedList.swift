import Foundation
import Observation
import os.log

private let logger = Logger.bae("PaginatedList")

// MARK: - PaginatedList

/// An ordered list of the rows its view delivers.
///
/// The list reads one view of its rows (`ListViewQuery`): core keeps one
/// live window of the list around the rows on screen, moves and sizes it, and
/// delivers its rows with the position of the first. The count of the whole
/// list and the artist headings of the album grid are reads of their own.
///
/// The rows on screen are the positions held by `holdPage(containing:)` and
/// `withPage(containing:_:)`, and the list reports them to core each time
/// they change.
@MainActor
@Observable
public final class PaginatedList<Row: Identifiable & Sendable & Equatable>
where Row.ID: Sendable {
    /// The list's count, from its last value: how many rows the list has,
    /// and what the count carries beside them, such as the album grid's album
    /// count or the Storage Manager's total size. `nil` until the count
    /// answers after `loadInitial` or `reopen` opened the list. The list
    /// never uses the number of rows its view holds as the count, so a
    /// screen decides whether to show an empty list only after the count's
    /// first value arrives.
    public private(set) var count: BridgeListCount?

    /// How many positions a screen lays out: the count's rows once the count
    /// answers, and none before.
    public var positionCount: Int {
        count.map { Int($0.rows) } ?? 0
    }

    /// How many distinct items the list's rows show, as its count says;
    /// `nil` until the count has answered.
    public var itemCount: Int? {
        count.map { Int($0.items) }
    }

    /// Whether the library holds a track to shuffle, as the album grid's
    /// count says; `nil` until it has answered, and for a list that is not
    /// the album grid.
    public var canShuffle: Bool? {
        guard case .albumGrid(let canShuffle, _)? = count?.extra else {
            return nil
        }
        return canShuffle
    }

    /// The artist headings of the album grid grouped by artist, from their own
    /// read.
    public private(set) var sections: [BridgeLibraryBrowseSection] = []

    /// Advances when a delivery changes what the list holds: the rows at its
    /// positions, or the total. A value that repeats what the list already
    /// holds leaves it alone. When this value changes, the viewport scrolls
    /// back to the row it showed, so the row on top stays on top when rows
    /// above it come or go.
    public private(set) var contentRevision: UInt64 = 0

    /// The first read after `loadInitial`, its first rows or its count,
    /// failed before both delivered. The grid that shows the list shows an
    /// error and Retry instead of the empty-library placeholder: a failed
    /// first read is not an empty library. A later failure keeps what is on
    /// screen and goes to `onError`.
    public private(set) var initialLoadError: DisplayError?

    /// This list's `LoadEpoch`. A row puts it in its `RowLoadID` for
    /// `.task(id:)`, so the row's load restarts when a new list replaces this
    /// one.
    public var loadEpoch: LoadEpoch {
        LoadEpoch(instance: ObjectIdentifier(self))
    }

    /// The rows the view last delivered. Views read it while they draw, so
    /// Observation tracks it.
    private var delivered: ListViewDelivery<Row>?

    @ObservationIgnored
    private let queries: BridgeListQueries<Row>
    @ObservationIgnored
    private let onError: (any Error) -> Void
    @ObservationIgnored
    private var view: ListViewQuery<Row>?
    @ObservationIgnored
    private var viewDeliveries:
        ScreenSubscription<PulledValues<ListViewDelivery<Row>>>?
    @ObservationIgnored
    private var countDeliveries:
        ScreenSubscription<PulledValues<BridgeListCount>>?
    @ObservationIgnored
    private var sectionDeliveries:
        ScreenSubscription<PulledValues<[BridgeLibraryBrowseSection]>>?
    /// How many holds of rows on screen each position has.
    @ObservationIgnored
    private var shown: [Int: Int] = [:]
    /// The position the screen held last, which names the run of held rows
    /// on screen now when rows the screen showed before still hold theirs.
    @ObservationIgnored
    private var lastHeld: Int?
    /// How many reveals wait on each position: a reveal reads one row, which
    /// the screen then scrolls to.
    @ObservationIgnored
    private var reveals: [Int: Int] = [:]
    /// The position the list's view opened at, which the rows on screen
    /// start from until the screen holds rows of its own.
    @ObservationIgnored
    private var opening = 0
    /// The rows on screen the list last reported to its view.
    @ObservationIgnored
    private var reported: ClosedRange<Int>?
    /// The tasks waiting for a position's row, each under the position.
    @ObservationIgnored
    private let rowWaiting = Waiting()
    @ObservationIgnored
    private let firstAnswerWaiting = Waiting()
    /// Whether the view opened last has delivered.
    @ObservationIgnored
    private var firstAnswered = false
    @ObservationIgnored
    private var cancelled = false
    /// Whether the first rows and the count have both delivered since
    /// `loadInitial` last opened the list. Until they have, a failed read is
    /// the first read's failure; after, the list keeps its rows, also through
    /// a `reopen`, and sends the failure to `onError`.
    @ObservationIgnored
    private var firstReadAnswered = false

    /// How many rows the screen fits, from its height and its row height;
    /// nil until the screen sets it. The list opens its view with that many
    /// rows on screen, and reports them before the screen holds any row.
    @ObservationIgnored
    public var screenRows: Int? {
        didSet {
            guard screenRows != oldValue else { return }
            report()
        }
    }

    public init(
        queries: BridgeListQueries<Row>,
        onError: @escaping (any Error) -> Void
    ) {
        self.queries = queries
        self.onError = onError
    }

    // MARK: - Loading

    /// Read the list's count and its view at the rows on screen, or at its
    /// top, and wait for the view's first rows; the view draws the count when
    /// it answers. The owner calls this when it opens the list, and again to
    /// retry a failed first read.
    public func loadInitial() async {
        firstReadAnswered = false
        await reopen(at: 0)
    }

    /// Read the list again with its view opened at `position` until the
    /// screen reports the rows it shows there, and wait for the view's first
    /// rows, not the count. The list drops the view, count and headings it
    /// read before and opens new reads, so every value it receives comes from
    /// after the write that made the reopen necessary. It keeps drawing the
    /// rows it had until the new view answers.
    public func reopen(at position: Int) async {
        initialLoadError = nil
        // The count read before is not this read's: the list holds none until
        // the new one answers.
        count = nil
        contentRevision += 1
        openCount()
        openSections()
        opening = position
        openView(at: position...(position + screen - 1))
        await firstAnswerWaiting.wait(
            unless: firstAnswered || initialLoadError != nil
        )
    }

    /// Hold `position` as a row on screen for as long as the calling task
    /// runs: the list's view keeps the rows on screen loaded, with a screen
    /// of rows on each side. A row on screen holds its position from a
    /// `.task`, which SwiftUI cancels when the row leaves. `loaded` runs
    /// once the view has delivered the row, or the count shows the list ends
    /// before it.
    public func holdPage(
        containing position: Int,
        loaded: @MainActor () -> Void = {}
    ) async {
        precondition(position >= 0, "a list position is never negative")
        shown[position, default: 0] += 1
        lastHeld = position
        defer { release(position, from: \.shown) }
        await waitForRow(at: position)
        if !Task.isCancelled { loaded() }
        await Task.untilCancelled()
    }

    /// Read the row at `position` and keep it while `body` runs, for a
    /// reveal that scrolls to it or a read of the row. Until `body` returns,
    /// the list reports a screen of rows from `position` as the rows on
    /// screen, since the screen scrolls there. `body` starts once the view
    /// has delivered the row at the position, or once the count shows the
    /// list ends before it, so the row is there to read, and stays there
    /// until `body` returns.
    ///
    /// A task that is cancelled before the view delivers the row stops
    /// waiting, so `body` runs without the row.
    public func withPage<T>(
        containing position: Int,
        _ body: @MainActor () async throws -> T
    ) async rethrows -> T {
        precondition(position >= 0, "a list position is never negative")
        reveals[position, default: 0] += 1
        defer { release(position, from: \.reveals) }
        await waitForRow(at: position)
        return try await body()
    }

    /// Report the rows on screen, and wait until the view has delivered the
    /// row at `position` or the count shows the list ends before it.
    private func waitForRow(at position: Int) async {
        guard !Task.isCancelled, !cancelled else { return }
        report()
        await rowWaiting.wait(key: position, unless: isSatisfied(position))
    }

    public func cancel() {
        cancelled = true
        viewDeliveries?.stop()
        viewDeliveries = nil
        view = nil
        countDeliveries?.stop()
        countDeliveries = nil
        sectionDeliveries?.stop()
        sectionDeliveries = nil
        resolveAllWaiters()
    }

    // MARK: - The reads beside the view

    private func openCount() {
        countDeliveries?.stop()
        countDeliveries = ScreenSubscription(
            queries.count(),
            onValue: { [weak self] count in self?.apply(count) },
            onError: { [weak self] error in self?.failed(error) }
        )
    }

    private func openSections() {
        guard let makeSections = queries.sections else { return }
        sectionDeliveries?.stop()
        sectionDeliveries = ScreenSubscription(
            makeSections(),
            onValue: { [weak self] sections in
                guard let self, self.sections != sections else { return }
                self.sections = sections
            },
            onError: { [weak self] error in self?.failed(error) }
        )
    }

    private func apply(_ next: BridgeListCount) {
        if next.rows != count?.rows {
            contentRevision += 1
        }
        if next != count {
            count = next
        }
        settled()
    }

    // MARK: - The view

    /// Open a new view of the list with `rows` on screen. The list keeps
    /// drawing the rows it has until the new view answers, so a reopen or
    /// another order never empties the screen while core reads the new
    /// window.
    private func openView(at rows: ClosedRange<Int>) {
        viewDeliveries?.stop()
        firstAnswered = false
        reported = rows
        let view = queries.view(
            UInt64(rows.lowerBound),
            UInt64(rows.upperBound)
        )
        self.view = view
        viewDeliveries = ScreenSubscription(
            view.values,
            onValue: { [weak self] delivery in self?.take(delivery) },
            onError: { [weak self] error in self?.failed(error) }
        )
    }

    private func take(_ next: ListViewDelivery<Row>) {
        firstAnswered = true
        if next != delivered {
            delivered = next
            contentRevision += 1
        }
        settled()
    }

    /// The rows on screen: the run of held positions around the one the
    /// screen held last. A screen holds its rows in one run, and the holds of
    /// rows it showed before a jump end soon after, so they are not part of
    /// it.
    private var held: ClosedRange<Int>? {
        let latest = lastHeld.flatMap { shown[$0] == nil ? nil : $0 }
        guard let seed = latest ?? shown.keys.max() else { return nil }
        var low = seed
        var high = seed
        while shown[low - 1] != nil { low -= 1 }
        while shown[high + 1] != nil { high += 1 }
        return low...high
    }

    /// How many rows the screen shows: as many as it fits, else as many as
    /// it holds, else one.
    private var screen: Int {
        max(1, screenRows ?? held?.count ?? 1)
    }

    /// The rows on screen: a screen from the last reveal that waits, else the
    /// rows the screen holds, else a screen from where the view opened.
    private var visible: ClosedRange<Int> {
        if let reveal = reveals.keys.max() {
            return reveal...(reveal + screen - 1)
        }
        return held ?? opening...(opening + screen - 1)
    }

    /// Tell the view the rows on screen when they changed since the list last
    /// told it.
    private func report() {
        guard let view, visible != reported else { return }
        reported = visible
        view.setVisible(UInt64(visible.lowerBound), UInt64(visible.upperBound))
    }

    private func release(
        _ position: Int,
        from holds: ReferenceWritableKeyPath<PaginatedList, [Int: Int]>
    ) {
        guard let count = self[keyPath: holds][position] else {
            preconditionFailure("released a hold never taken")
        }
        if count > 1 {
            self[keyPath: holds][position] = count - 1
        }
        else {
            self[keyPath: holds].removeValue(forKey: position)
            report()
        }
    }

    // MARK: - Waiting

    /// Whether the current view has delivered the row at `position`, or the
    /// count shows the list ends before it. A row the list keeps drawing
    /// while it waits for a new view's first rows is not the current view's.
    private func isSatisfied(_ position: Int) -> Bool {
        if firstAnswered, row(at: position) != nil { return true }
        guard let count else { return false }
        return position >= Int(count.rows)
    }

    /// The view or the count delivered: resume every task whose row is now
    /// there or past the list's end.
    private func settled() {
        firstReadAnswered = firstReadAnswered || (firstAnswered && count != nil)
        rowWaiting.release(where: isSatisfied)
        if firstAnswered || initialLoadError != nil {
            firstAnswerWaiting.releaseAll()
        }
    }

    private func resolveAllWaiters() {
        rowWaiting.releaseAll()
        firstAnswerWaiting.releaseAll()
    }

    // MARK: - Failures

    private func failed(_ error: any Error) {
        if !firstReadAnswered {
            initialLoadError = DisplayError(error)
            settled()
            return
        }
        logger.error("Live list read failed: \(String(reflecting: error))")
        onError(error)
    }
}

extension PaginatedList {
    // MARK: - Another order

    /// Read the rows again in another order. The view's window starts at a
    /// row's sort values in the order the list opened it in, so the list
    /// opens a new view at the rows on screen, and keeps its count: another
    /// order moves the rows, not how many there are. The list keeps drawing
    /// the rows in the old order until the new view answers, and this call
    /// waits for that answer.
    public func reorder() async {
        openView(at: visible)
        await firstAnswerWaiting.wait(
            unless: firstAnswered || initialLoadError != nil
        )
    }
}

extension PaginatedList {
    // MARK: - Test/Preview support

    #if DEBUG
        /// Seed the rows synchronously for shots and tests, with a count of
        /// `rows` that carries `extra`, the extra this list's count carries.
        public func preloadRows(rows: [Row], extra: BridgeListCountExtra) {
            delivered = ListViewDelivery(start: 0, rows: rows)
            firstAnswered = true
            count = BridgeListCount(
                rows: UInt64(rows.count),
                items: UInt64(rows.count),
                extra: extra
            )
        }
    #endif
}

// MARK: - Queries

extension PaginatedList {
    /// Returns the ID at `position`.
    public func idAt(_ position: Int) -> Row.ID? {
        row(at: position)?.id
    }

    /// The row the list holds at `position`, or nil if the view's window
    /// doesn't hold it.
    public func row(at position: Int) -> Row? {
        guard let delivered else { return nil }
        let at = position - delivered.start
        return delivered.rows.indices.contains(at) ? delivered.rows[at] : nil
    }

    /// Returns the position of `id` in the loaded rows, or nil if not loaded.
    public func position(of id: Row.ID) -> Int? {
        guard let delivered,
            let at = delivered.rows.firstIndex(where: { $0.id == id })
        else { return nil }
        return delivered.start + at
    }

    /// The row that belongs above the window's rows without being among
    /// them, when `id` is that row of the window the list holds.
    public func leadingHeader(of id: Row.ID) -> Row? {
        delivered?.leadingHeader.flatMap { $0.id == id ? $0 : nil }
    }

    /// The row of `id` the list holds, or nil if the window doesn't hold it.
    /// A row shown more than once is the same row at each place.
    public func row(of id: Row.ID) -> Row? {
        position(of: id).flatMap(row(at:))
    }

    /// The artist heading whose albums include `position`.
    public func section(at position: Int) -> BridgeLibraryBrowseSection? {
        sections.first {
            position >= Int($0.firstPosition)
                && position < Int($0.firstPosition + $0.albumCount)
        }
    }

    public func position(of id: Row.ID, in sectionId: String?) -> Int? {
        guard let sectionId else { return position(of: id) }
        return loadedEntries.first {
            $0.id == id && section(at: $0.position)?.id == sectionId
        }?
        .position
    }

    /// Every held position with the id held there, in position order.
    public var loadedEntries: [(position: Int, id: Row.ID)] {
        guard let delivered else { return [] }
        return delivered.rows.enumerated()
            .map {
                (position: delivered.start + $0, id: $1.id)
            }
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
