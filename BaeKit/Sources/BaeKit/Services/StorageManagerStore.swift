import Foundation
import Observation

@MainActor
@Observable
public final class StorageManagerStore {
    public private(set) var list: StorageList?
    /// The transfer queues holding work, by core's last answer; `nil` until
    /// it answers.
    public private(set) var queues: BridgeStorageQueues?
    /// Core's last answer for the selection: the row of each selected release
    /// the list holds, wherever in the list it sits, the storage actions
    /// every one of them allows, and whether Cancel is the only action one of
    /// them offers.
    private var answer: BridgeStorageSelection?

    @ObservationIgnored
    private let library: Library
    @ObservationIgnored
    private let onError: (any Error) -> Void
    @ObservationIgnored
    private var rebuildTask: Task<Void, Never>?
    @ObservationIgnored
    private var generation = 0
    /// The rows of the selection the list holds, read live for the list
    /// shown: a new sort, filter or search opens another.
    @ObservationIgnored
    private var selectionQuery: StorageSelectionQuery?
    @ObservationIgnored
    private var selectionDeliveries:
        ScreenSubscription<PulledValues<BridgeStorageSelection>>?
    /// The queues' one live read, opened by the first `update` and kept until
    /// `cancel`.
    @ObservationIgnored
    private var queueDeliveries:
        ScreenSubscription<PulledValues<BridgeStorageQueues>>?
    /// The filter and search text the list shows, which `holdsAll` passes to
    /// core when it asks about the list's releases.
    @ObservationIgnored
    private var shown: (filter: BridgeStorageFilter, search: String)?
    /// The releases selected in the list. The store asks the selection read
    /// for their rows.
    @ObservationIgnored
    private var selection: Set<String> = []

    public init(
        library: Library,
        onError: @escaping (any Error) -> Void
    ) {
        self.library = library
        self.onError = onError
    }

    /// The total size of the files of the releases the list holds, by its
    /// count's last answer; `nil` until the list's count answers.
    public var totalSize: UInt64? {
        storageCount?.totalSize
    }

    /// Whether the search text narrows the list, by its count's last answer;
    /// `nil` until the list's count answers.
    public var searching: Bool? {
        storageCount?.searching
    }

    /// What the list's count carries beside its rows, which for the Storage
    /// Manager's list is always the total size and whether a search narrows
    /// it.
    private var storageCount: (totalSize: UInt64, searching: Bool)? {
        guard let count = list?.count else { return nil }
        guard case .storage(let totalSize, let searching) = count.extra else {
            preconditionFailure(
                "the Storage Manager's list count carried \(count.extra)"
            )
        }
        return (totalSize, searching)
    }

    /// Show the list under `filter`, `sort` and the `search` text, which
    /// core reads as library search reads its query; blank text narrows
    /// nothing.
    public func update(
        filter: BridgeStorageFilter,
        sort: BridgeStorageSort,
        search: String
    ) {
        rebuildTask?.cancel()
        list?.cancel()
        generation += 1
        let currentGeneration = generation
        shown = (filter, search)
        openQueues()
        openSelection(filter: filter, search: search)
        let newList = StorageList(
            queries: library.storageList(sort, filter, search),
            onError: onError
        )
        rebuildTask = Task { [weak self] in
            await newList.loadInitial()
            guard !Task.isCancelled,
                self?.generation == currentGeneration
            else { return }
            self?.list = newList
        }
    }

    /// Ask the list for the rows of `selection` it holds.
    public func select(_ selection: Set<String>) {
        self.selection = selection
        setSelected()
    }

    private func openQueues() {
        guard queueDeliveries == nil else { return }
        queueDeliveries = ScreenSubscription(
            library.storageQueues(),
            onValue: { [weak self] queues in self?.queues = queues },
            onError: { [weak self] error in self?.onError(error) }
        )
    }

    /// Read the selection's rows under `filter` and `search`, from the next
    /// value on: core answers for the filter and search of the list on
    /// screen.
    private func openSelection(filter: BridgeStorageFilter, search: String) {
        selectionDeliveries?.stop()
        answer = nil
        let query = library.storageSelection(filter, search)
        selectionQuery = query
        selectionDeliveries = ScreenSubscription(
            query.values,
            onValue: { [weak self] value in
                self?.answer = value
            },
            onError: { [weak self] error in self?.onError(error) }
        )
        setSelected()
    }

    private func setSelected() {
        do {
            try selectionQuery?.setSelected(Array(selection))
        }
        catch {
            onError(error)
        }
    }

    /// The one release the list holds among the ones core last answered for,
    /// when it holds exactly one; `nil` for none or several. Nothing showing
    /// it shows a release the list doesn't.
    public var soleRelease: String? {
        answer?.sole
    }

    /// Whether the list, as core reads it now, holds every one of
    /// `releaseIds`; `false` before `update` shows the list.
    public func holdsAll(_ releaseIds: [String]) async throws -> Bool {
        guard let shown else { return false }
        return try await library.storageHoldsAll(
            shown.filter,
            shown.search,
            releaseIds
        )
    }

    /// Core's answer, when it is for `selection` and not for an earlier
    /// one: the rows of the releases the list holds among `selection`, the
    /// storage actions every one allows, and whether Cancel is the only
    /// action one of them offers. `nil` while core's answer is for other
    /// releases.
    public func answered(
        for selection: Set<String>
    ) -> BridgeStorageSelection? {
        guard let answer, Set(answer.asked) == selection else { return nil }
        return answer
    }

    public func cancel() {
        generation += 1
        selection = []
        answer = nil
        shown = nil
        rebuildTask?.cancel()
        rebuildTask = nil
        list?.cancel()
        selectionDeliveries?.stop()
        selectionDeliveries = nil
        selectionQuery = nil
        queueDeliveries?.stop()
        queueDeliveries = nil
    }
}
