import Foundation
import Observation

@MainActor
@Observable
public final class StorageManagerStore {
    public private(set) var list: StorageList?
    public private(set) var totalSize: UInt64?

    @ObservationIgnored
    private let library: Library
    @ObservationIgnored
    private let libraryStore: LibraryStore
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

    public init(
        library: Library,
        libraryStore: LibraryStore,
        onError: @escaping (any Error) -> Void
    ) {
        self.library = library
        self.libraryStore = libraryStore
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
                onTotalSize: { [weak self] value in
                    self?.totalSize = value
                }
            )
            self.pageSource = pageSource
        }
        let newList = StorageList(
            pageSource: pageSource,
            ingest: { [libraryStore] rows in
                for row in rows {
                    _ = libraryStore.internAlbumSummary(row.album)
                    _ = libraryStore.internReleaseSummary(row.release)
                }
            },
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

    public func cancel() {
        generation += 1
        rebuildTask?.cancel()
        rebuildTask = nil
        list?.cancel()
        pageSource?.close()
        pageSource = nil
    }
}
