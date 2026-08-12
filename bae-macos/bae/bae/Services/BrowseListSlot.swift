import BaeKit
import Foundation
import Observation

/// One library-browse mode's list machinery: the warm `PaginatedList`, its
/// live subscription, the persisted sort criteria, and the
/// reload task that rebuilds the list when the criteria change. The album,
/// composer, and artist modes are three instances of this shape; whether a
/// slot loads eagerly (albums, at session construction) or on first mode
/// switch (composers, artists) is the call site's choice via `startLoad()`
/// vs `ensureLoaded()`.
@MainActor
@Observable
final class BrowseListSlot<
    Row: Identifiable & Sendable,
    Criterion: SortCriterionRepresentable
> where Row.ID: Sendable, Criterion.Field: SortCriterionFieldCodable {
    private(set) var list: PaginatedList<Row>?
    private(set) var sortCriteria: [Criterion]

    private var reloadTask: Task<Void, Never>?

    private let defaultsKey: String
    private let makePageSource: ([Criterion]) -> any PageSource<Row>
    private let ingest: ([Row]) -> Void
    /// Takes the error, not a rendered line: whether a failure is worth showing
    /// is core's answer, and the sink is the one place that drops it.
    private let onError: (any Error) -> Void

    init(
        defaultsKey: String,
        defaultCriteria: [Criterion],
        makePageSource: @escaping ([Criterion]) -> any PageSource<Row>,
        ingest: @escaping ([Row]) -> Void,
        onError: @escaping (any Error) -> Void
    ) {
        self.defaultsKey = defaultsKey
        self.makePageSource = makePageSource
        self.ingest = ingest
        self.onError = onError
        sortCriteria = Self.loadCriteria(
            key: defaultsKey,
            defaultCriteria: defaultCriteria
        )
    }

    /// Replace the sort criteria, persist them, and rebuild the list.
    func setSortCriteria(_ criteria: [Criterion]) {
        sortCriteria = criteria
        if let data = criteria.toJSON() {
            UserDefaults.standard.set(data, forKey: defaultsKey)
        }
        startLoad()
    }

    /// Cancel any in-flight rebuild and start a fresh one. The eager-load
    /// entry point (the session calls it at construction for albums) and
    /// the tail of `setSortCriteria`.
    func startLoad() {
        reloadTask?.cancel()
        reloadTask = Task { [weak self] in
            await self?.reload()
        }
    }

    /// Load the list the first time its mode is shown; a no-op afterward,
    /// so a remount that re-fires this finds a warm list and does nothing.
    /// Runs in the caller's task so SwiftUI cancellation applies.
    func ensureLoaded() async {
        guard list == nil else {
            return
        }
        await reload()
    }

    private func reload() async {
        let newList = PaginatedList<Row>(
            pageSource: makePageSource(sortCriteria),
            ingest: ingest,
            onError: onError
        )
        await newList.loadInitial()
        guard !Task.isCancelled else {
            return
        }
        list = newList
    }

    private static func loadCriteria(
        key: String,
        defaultCriteria: [Criterion]
    ) -> [Criterion] {
        guard let data = UserDefaults.standard.data(forKey: key),
            let criteria = [Criterion].fromJSON(data),
            !criteria.isEmpty
        else {
            return defaultCriteria
        }
        return criteria
    }
}
