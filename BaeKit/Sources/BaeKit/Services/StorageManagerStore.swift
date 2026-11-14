import Foundation
import Observation

@MainActor
@Observable
public final class StorageManagerStore {
    public private(set) var list: StorageList?
    /// The transfer queues holding work, by core's last answer; `nil` until
    /// it answers.
    public private(set) var queues: BridgeStorageQueues?
    /// The row of each selected release the list holds, by core's answer,
    /// wherever in the list it sits — loaded or not — by release id.
    private var selectedRows: [String: BridgeStorageRow] = [:]
    /// The storage actions every release of `selectedRows` allows, by core's
    /// answer.
    private var selectedActions: [BridgeReleaseStorageAction] = []

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
    /// The releases selected in the list, which the selection read is asked
    /// about.
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
    /// count's last answer; `nil` until the list has answered.
    public var totalSize: UInt64? {
        guard let list, case .storage(let size, _)? = list.countExtra else {
            return nil
        }
        return size
    }

    /// Whether the search text narrows the list, by core's last answer.
    public var searching: Bool {
        guard let list, case .storage(_, let searching)? = list.countExtra
        else {
            return false
        }
        return searching
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
    /// value on: the answer belongs to the list it was read under.
    private func openSelection(filter: BridgeStorageFilter, search: String) {
        selectionDeliveries?.stop()
        selectedRows = [:]
        selectedActions = []
        let query = library.storageSelection(filter, search)
        selectionQuery = query
        selectionDeliveries = ScreenSubscription(
            query.values,
            onValue: { [weak self] value in
                self?.selectedRows = Dictionary(
                    uniqueKeysWithValues: value.selected.map { ($0.id, $0) }
                )
                self?.selectedActions = value.selectionActions
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

    /// The part of `selection` the list holds: a row it has loaded, or a
    /// release core says it holds. A selected release the list no longer
    /// holds — uploaded out of the Local list, filtered out, deleted — is
    /// never in it, so nothing showing this set shows a release the list
    /// doesn't.
    public func shown(_ selection: Set<String>) -> Set<String> {
        selection.filter { row(of: $0) != nil }
    }

    /// The row the list's query returned for a release the list holds: the
    /// selection's row, wherever the release sits, or the row of a loaded
    /// page, which answers for a release just selected before core has.
    /// `nil` for a release the list doesn't hold.
    public func row(of releaseId: String) -> BridgeStorageRow? {
        selectedRows[releaseId] ?? list?.row(of: releaseId)
    }

    /// The storage actions every release of `shown` allows, as core answered
    /// for the selection, or `nil` while core's answer is for other releases:
    /// `shown` holds one just selected that core has not answered for yet.
    public func selectionActions(
        for shown: Set<String>
    ) -> [BridgeReleaseStorageAction]? {
        guard shown == Set(selectedRows.keys) else { return nil }
        return selectedActions
    }

    public func cancel() {
        generation += 1
        selection = []
        selectedRows = [:]
        selectedActions = []
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
