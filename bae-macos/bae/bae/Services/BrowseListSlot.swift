import BaeKit
import Foundation
import OSLog
import Observation

private let logger = Logger.bae("BrowseListSlot")

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
    Row: Identifiable & Sendable & Equatable,
    Criterion: SortCriterionRepresentable & Codable
> where Row.ID: Sendable {
    private(set) var list: PaginatedList<Row>?
    private(set) var sortCriteria: [Criterion]

    private var reloadTask: Task<Void, Never>?

    private let defaults: UserDefaults
    private let defaultsKey: String
    private let makePageSource: ([Criterion]) -> any PageSource<Row>
    /// Takes the error, not a rendered line: whether a failure is worth showing
    /// is core's answer, and the sink is the one place that drops it.
    private let onError: (any Error) -> Void

    init(
        defaults: UserDefaults = .standard,
        defaultsKey: String,
        defaultCriteria: [Criterion],
        makePageSource: @escaping ([Criterion]) -> any PageSource<Row>,
        onError: @escaping (any Error) -> Void
    ) {
        self.defaults = defaults
        self.defaultsKey = defaultsKey
        self.makePageSource = makePageSource
        self.onError = onError
        sortCriteria = Self.loadCriteria(
            defaults: defaults,
            key: defaultsKey,
            defaultCriteria: defaultCriteria
        )
    }

    /// Replace the sort criteria, persist them, and rebuild the list.
    func setSortCriteria(_ criteria: [Criterion]) {
        sortCriteria = criteria
        defaults.setEncoded(criteria, forKey: defaultsKey)
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
            onError: onError
        )
        await newList.loadInitial()
        guard !Task.isCancelled else {
            return
        }
        list = newList
    }

    /// The saved criteria, or `defaultCriteria` when none are saved. A saved
    /// value that doesn't decode whole is logged by `decoded`, and one that
    /// holds no criterion here; either sorts by the default.
    private static func loadCriteria(
        defaults: UserDefaults,
        key: String,
        defaultCriteria: [Criterion]
    ) -> [Criterion] {
        guard let saved = defaults.decoded([Criterion].self, forKey: key)
        else {
            return defaultCriteria
        }
        guard !saved.isEmpty else {
            logger.error(
                "Saved sort criteria for \(key) are empty, sorting by the "
                    + "default"
            )
            return defaultCriteria
        }
        return saved
    }
}
