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
/// `holdPage(containing:)` and `withPage(containing:_:)`. A position far from
/// the chain is a jump: the chain starts over from the key the list has at
/// that position, a plain read.
@MainActor
@Observable
public final class PaginatedList<Row: Identifiable & Sendable & Equatable>
where Row.ID: Sendable {
    /// How many rows the list has, from its count's last value.
    public private(set) var totalCount: Int = 0

    /// What the list's count carries beside its rows, as its last value gave
    /// it: the album grid's album count, or the Storage Manager's total size;
    /// `nil` until the count has answered.
    public private(set) var countExtra: BridgeListCountExtra?

    /// How many distinct items the list's count carries when a row can show an
    /// item more than once; the row count otherwise. `nil` until the count has
    /// answered.
    public var itemCount: Int? {
        guard let countExtra else { return nil }
        if case .albumGrid(let albums) = countExtra { return Int(albums) }
        return totalCount
    }

    /// The artist headings of the album grid grouped by artist, from their own
    /// read.
    public private(set) var sections: [BridgeLibraryBrowseSection] = []

    /// Advances when a delivery changes what the list holds: the rows at its
    /// positions, or the total. A value that repeats what the list already
    /// holds leaves it alone. A rendered viewport uses this boundary to
    /// restore the row it held while rows of different heights were
    /// materialised or changed above it.
    public private(set) var contentRevision: UInt64 = 0

    /// The cold first read failed. The consuming grid shows an error and Retry
    /// instead of the empty-library placeholder: a failed first read is not an
    /// empty library. Only `loadInitial` sets it; a later failure keeps what is
    /// on screen and goes to `onError`.
    public private(set) var initialLoadError: DisplayError?

    /// This list's `LoadEpoch`. Consumers fold it into a row's `RowLoadID`
    /// `.task(id:)` so the row's load restarts when the list is swapped.
    public var loadEpoch: LoadEpoch {
        LoadEpoch(instance: ObjectIdentifier(self))
    }

    /// The rows the chain holds, laid end to end. Read during render, so it
    /// stays observed.
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
    @ObservationIgnored
    private var countAnswered = false
    /// How many holds each position has: the rows on screen, or a reveal on
    /// its way to a row.
    @ObservationIgnored
    private var holds: [Int: Int] = [:]
    /// The tasks waiting for a position's row, each under the position.
    @ObservationIgnored
    private let rowWaiting = Waiting()
    /// The jump in flight, which reads the key at a position.
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

    /// Read the top of the list and its count, and wait for both. Called when
    /// the list is first mounted, and again to retry a failed first read.
    public func loadInitial() async {
        initialLoadError = nil
        countAnswered = false
        openCount()
        openSections()
        startChain(at: nil, position: 0)
        await firstAnswerWaiting.wait(
            unless: (firstAnswered && countAnswered) || initialLoadError != nil
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

    /// Hold `position` while `body` runs. `body` starts once the row at the
    /// position has landed, or once the list is known to end before it, so the
    /// row it holds is there to read, and stays there until `body` returns.
    ///
    /// A calling task cancelled before the row lands stops waiting, so `body`
    /// runs without it.
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

    private func apply(_ count: BridgeListCount) {
        if initialLoadError != nil {
            initialLoadError = nil
        }
        let total = Int(count.rows)
        if total != totalCount {
            totalCount = total
            contentRevision += 1
        }
        if countExtra != count.extra {
            countExtra = count.extra
        }
        countAnswered = true
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
        let known = countAnswered ? max(totalCount - 1, 0) : position
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

    /// Whether the row at `position` has landed, or the list is known to end
    /// before it.
    private func isSatisfied(_ position: Int) -> Bool {
        row(at: position) != nil || (countAnswered && position >= totalCount)
    }

    private var firstAnswered: Bool { chain?.hasAnswered ?? false }

    /// Something landed: let go of everything it answers.
    private func settled() {
        rowWaiting.release(where: isSatisfied)
        if (firstAnswered && countAnswered) || initialLoadError != nil {
            firstAnswerWaiting.releaseAll()
        }
    }

    private func resolveAllWaiters() {
        rowWaiting.releaseAll()
        firstAnswerWaiting.releaseAll()
    }

    // MARK: - Failures

    private func failed(_ error: any Error) {
        if !(firstAnswered && countAnswered), placed.isEmpty {
            initialLoadError = DisplayError(error)
            settled()
            return
        }
        logger.error("Live list read failed: \(String(reflecting: error))")
        onError(error)
    }

    // MARK: - Test/Preview support

    #if DEBUG
        /// Seed the rows synchronously for shots and tests.
        public func preloadRows(rows: [Row]) {
            placed = rows.isEmpty ? [] : [Placed(start: 0, rows: rows)]
            totalCount = rows.count
            countExtra = BridgeListCountExtra.none
            countAnswered = true
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
