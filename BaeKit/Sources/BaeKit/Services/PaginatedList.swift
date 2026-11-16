import Foundation
import Observation
import os.log

private let logger = Logger.bae("PaginatedList")

// MARK: - PaginatedList

/// An ordered list of the rows its windows deliver.
///
/// The windows are a `WindowChain`: each a live read of a range of the list's
/// order, laid end to end. The count of the whole list, the artist headings
/// of the album grid and the position of the chain's first row are reads of
/// their own, so a row's position is never read inside a window.
///
/// The chain follows the rows on screen, which are the positions held by
/// `holdPage(containing:)` and `withPage(containing:_:)`. When a held position
/// is far from the chain's rows, the list reads the key at that position once
/// and starts the chain over from it.
@MainActor
@Observable
public final class PaginatedList<Row: Identifiable & Sendable & Equatable>
where Row.ID: Sendable {
    /// The list's count, from its last value: how many rows the list has,
    /// and what the count carries beside them, such as the album grid's album
    /// count or the Storage Manager's total size. `nil` until the count
    /// answers after `loadInitial` or `reopen` opened the list. The list
    /// never uses the number of rows its windows hold as the count, so a
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
        guard case .albumGrid(let canShuffle)? = count?.extra else {
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
    /// back to the row it showed, because rows of different heights above it
    /// may have loaded or changed.
    public private(set) var contentRevision: UInt64 = 0

    /// The first read, made before the list held any rows, failed. The grid
    /// that shows the list shows an error and Retry instead of the
    /// empty-library placeholder: a failed first read is not an empty
    /// library. Only `loadInitial` sets it; a later failure keeps what is on
    /// screen and goes to `onError`.
    public private(set) var initialLoadError: DisplayError?

    /// This list's `LoadEpoch`. A row puts it in its `RowLoadID` for
    /// `.task(id:)`, so the row's load restarts when a new list replaces this
    /// one.
    public var loadEpoch: LoadEpoch {
        LoadEpoch(instance: ObjectIdentifier(self))
    }

    /// The rows the chain holds, laid end to end. Views read it while they
    /// draw, so Observation tracks it.
    private var placed: [Placed<Row>] = []

    @ObservationIgnored
    private let queries: BridgeListQueries<Row>
    @ObservationIgnored
    private let onError: (any Error) -> Void
    @ObservationIgnored
    private var chain: WindowChain<Row>?
    @ObservationIgnored
    private var countDeliveries:
        ScreenSubscription<PulledValues<BridgeListCount>>?
    @ObservationIgnored
    private var sectionDeliveries:
        ScreenSubscription<PulledValues<[BridgeLibraryBrowseSection]>>?
    /// How many holds each position has: the rows on screen, or a reveal
    /// that scrolls to a row.
    @ObservationIgnored
    private var holds: [Int: Int] = [:]
    /// The tasks waiting for a position's row, each under the position.
    @ObservationIgnored
    private let rowWaiting = Waiting()
    /// The task that is reading the key at a position, before the chain
    /// starts over there.
    @ObservationIgnored
    private var jump: Task<Void, Never>?
    @ObservationIgnored
    private var jumpId = UUID()
    @ObservationIgnored
    private let firstAnswerWaiting = Waiting()
    @ObservationIgnored
    private var cancelled = false

    public init(
        queries: BridgeListQueries<Row>,
        onError: @escaping (any Error) -> Void
    ) {
        self.queries = queries
        self.onError = onError
    }

    // MARK: - Loading

    /// Read the top of the list and its count, and wait for the first window;
    /// the view draws the count when it answers. The owner calls this when it
    /// opens the list, and again to retry a failed first read.
    public func loadInitial() async {
        await reopen(at: nil, position: 0)
    }

    /// Read the list again from `key` (its top when nil), using `position` as
    /// the number of rows before `key` until the count query returns, and
    /// wait for the first window, not the count. The list drops the windows,
    /// count and headings it read before and opens new reads, so every value
    /// it receives comes from after the write that made the reopen necessary.
    public func reopen(at key: [BridgeSortValue]?, position: Int) async {
        initialLoadError = nil
        // The count read before is not this read's: the list holds none until
        // the new one answers.
        count = nil
        contentRevision += 1
        jump?.cancel()
        jump = nil
        jumpId = UUID()
        openCount()
        openSections()
        startChain(at: key, position: position)
        await firstAnswerWaiting.wait(
            unless: firstAnswered || initialLoadError != nil
        )
    }

    /// Hold `position` for as long as the calling task runs: the chain keeps
    /// the row loaded and follows it. A row on screen holds its position from
    /// a `.task`, which SwiftUI cancels when the row leaves.
    public func holdPage(containing position: Int) async {
        await withPage(containing: position) {
            await Task.untilCancelled()
        }
    }

    /// Hold `position` while `body` runs. `body` starts once a window has
    /// delivered the row at the position, or once the count shows the list
    /// ends before it, so the row is there to read, and stays there until
    /// `body` returns.
    ///
    /// A task that is cancelled before a window delivers the row stops
    /// waiting, so `body` runs without the row.
    public func withPage<T>(
        containing position: Int,
        _ body: @MainActor () async throws -> T
    ) async rethrows -> T {
        precondition(position >= 0, "a list position is never negative")
        holds[position, default: 0] += 1
        defer { release(position) }
        if !Task.isCancelled, !cancelled {
            reach(position)
            await rowWaiting.wait(key: position, unless: isSatisfied(position))
        }
        return try await body()
    }

    public func cancel() {
        cancelled = true
        jump?.cancel()
        jump = nil
        jumpId = UUID()
        chain?.stop()
        countDeliveries?.stop()
        countDeliveries = nil
        sectionDeliveries?.stop()
        sectionDeliveries = nil
        resolveAllWaiters()
    }

    // MARK: - The reads beside the windows

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

    // MARK: - The chain

    private func startChain(at key: [BridgeSortValue]?, position: Int) {
        let chain =
            self.chain
            ?? WindowChain(
                queries: queries,
                pageSize: 50,
                onChange: { [weak self] in self?.chainChanged() },
                onError: { [weak self] error in self?.failed(error) }
            )
        self.chain = chain
        chain.visible = visible
        chain.restart(at: key, position: position)
        take(chain.placed)
    }

    /// The chain delivered or moved: draw what it holds.
    private func chainChanged() {
        guard let chain else { return }
        take(chain.placed)
        settled()
    }

    private func take(_ next: [Placed<Row>]) {
        guard next != placed else { return }
        placed = next
        contentRevision += 1
    }

    /// The positions on screen, from the holds.
    private var visible: ClosedRange<Int>? {
        guard let low = holds.keys.min(), let high = holds.keys.max() else {
            return nil
        }
        return low...high
    }

    private func reconcile() {
        guard jump == nil, let chain else { return }
        chain.visible = visible
        chain.reconcile()
        take(chain.placed)
        settled()
    }

    // MARK: - Reaching a position

    /// Make the row at `position` reachable: the chain grows towards it, or
    /// starts over from it when it is far from the chain's rows.
    private func reach(_ position: Int) {
        guard jump == nil else { return }
        if let chain, !chain.isEmpty, chain.reaches(position) {
            reconcile()
        }
        else {
            jumpTo(position)
        }
    }

    /// Start the chain over at the row the list has at `position`, which is a
    /// plain read of its key, not a live one.
    private func jumpTo(_ position: Int) {
        let id = UUID()
        jumpId = id
        chain?.stop()
        if let chain { take(chain.placed) }
        jump = Task { [weak self, queries] in
            let key: [BridgeSortValue]?
            do {
                key = try await queries.keyAt(UInt64(position))
            }
            catch {
                if Task.isCancelled { return }
                self?.jumpFailed(error, id: id)
                return
            }
            if Task.isCancelled { return }
            self?.finishJump(key, to: position, id: id)
        }
    }

    private func finishJump(
        _ key: [BridgeSortValue]?,
        to position: Int,
        id: UUID
    ) {
        guard jumpId == id, !cancelled else { return }
        jump = nil
        // A position past the list's end has the last row's key, which sits at
        // the last position.
        let known = count.map { max(Int($0.rows) - 1, 0) } ?? position
        startChain(at: key, position: min(position, known))
    }

    private func jumpFailed(_ error: any Error, id: UUID) {
        guard jumpId == id else { return }
        jump = nil
        failed(error)
        resolveAllWaiters()
    }

    private func release(_ position: Int) {
        guard let count = holds[position] else {
            preconditionFailure("released a hold never taken")
        }
        if count > 1 {
            holds[position] = count - 1
        }
        else {
            holds.removeValue(forKey: position)
            reconcile()
        }
    }

    // MARK: - Waiting

    /// Whether a window has delivered the row at `position`, or the count
    /// shows the list ends before it.
    private func isSatisfied(_ position: Int) -> Bool {
        if row(at: position) != nil { return true }
        guard let count else { return false }
        return position >= Int(count.rows)
    }

    private var firstAnswered: Bool { chain?.hasAnswered ?? false }

    /// A window or the count delivered: resume every task whose row is now
    /// there or past the list's end.
    private func settled() {
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
        if !(firstAnswered && count != nil), placed.isEmpty {
            initialLoadError = DisplayError(error)
            settled()
            return
        }
        logger.error("Live list read failed: \(String(reflecting: error))")
        onError(error)
    }
}

extension PaginatedList {
    // MARK: - Test/Preview support

    #if DEBUG
        /// Seed the rows synchronously for shots and tests, with a count of
        /// `rows` that carries `extra`, the extra this list's count carries.
        public func preloadRows(rows: [Row], extra: BridgeListCountExtra) {
            placed = rows.isEmpty ? [] : [Placed(start: 0, rows: rows)]
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

    /// The row the list holds at `position`, or nil if no window holds it.
    public func row(at position: Int) -> Row? {
        for run in placed
        where position >= run.start && position < run.start + run.rows.count {
            return run.rows[position - run.start]
        }
        return nil
    }

    /// Returns the position of `id` in the loaded rows, or nil if not loaded.
    public func position(of id: Row.ID) -> Int? {
        for run in placed {
            if let local = run.rows.firstIndex(where: { $0.id == id }) {
                return run.start + local
            }
        }
        return nil
    }

    /// The row that belongs above a window's rows without being among them,
    /// when `id` is that row of a window the list holds.
    public func leadingHeader(of id: Row.ID) -> Row? {
        placed.lazy.compactMap(\.leadingHeader).first { $0.id == id }
    }

    /// Every row the list holds, in order.
    public var loadedRows: [Row] {
        placed.flatMap(\.rows)
    }

    /// The row of `id` the list holds, or nil if no window holds it. A row
    /// shown more than once is the same row at each place.
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

    /// All IDs currently held, in order.
    public var allLoadedIds: [Row.ID] {
        placed.flatMap { $0.rows.map(\.id) }
    }

    /// Every held position with the id held there, in position order.
    public var loadedEntries: [(position: Int, id: Row.ID)] {
        placed.flatMap { run in
            run.rows.enumerated().map { (position: run.start + $0, id: $1.id) }
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
