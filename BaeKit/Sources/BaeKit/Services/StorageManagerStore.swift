import Foundation
import Observation

@MainActor
@Observable
public final class StorageManagerStore {
    public private(set) var list: StorageList?
    public private(set) var totalSize: UInt64?
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
    /// The manager's one live read, opened by the first `update` and kept
    /// across sort and filter changes until `cancel`.
    @ObservationIgnored
    private var pageSource: StorageBrowsePageSource?
    /// The releases selected in the list, which the query is asked about.
    @ObservationIgnored
    private var selection: Set<String> = []

    public init(
        library: Library,
        onError: @escaping (any Error) -> Void
    ) {
        self.library = library
        self.onError = onError
    }

    public func update(
        filter: BridgeStorageFilter,
        sort: BridgeStorageSort
    ) {
        rebuildTask?.cancel()
        list?.cancel()
        generation += 1
        let currentGeneration = generation
        totalSize = nil
        // The answer belongs to the view it was read under.
        selectedRows = [:]
        selectedActions = []
        let pageSource: StorageBrowsePageSource
        if let existing = self.pageSource {
            existing.setView(sort: sort, filter: filter)
            pageSource = existing
        }
        else {
            pageSource = StorageBrowsePageSource(
                library: library,
                sort: sort,
                filter: filter,
                selected: selection,
                onListValue: { [weak self] value in
                    self?.totalSize = value.totalSize
                    self?.selectedRows = Dictionary(
                        uniqueKeysWithValues: value.selected.map { ($0.id, $0) }
                    )
                    self?.selectedActions = value.selectionActions
                }
            )
            self.pageSource = pageSource
        }
        let newList = StorageList(
            pageSource: pageSource,
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
        pageSource?.setSelection(selection)
    }

    /// The part of `selection` the list holds: a row it has loaded, or a
    /// release core says it holds. A selected release the list no longer
    /// holds — uploaded out of the sync queue, filtered out, deleted — is
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
        pageSource?.close()
        pageSource = nil
    }
}
