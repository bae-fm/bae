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
        let newList = StorageList(
            pageSource: StoragePageSource(
                library: library,
                sort: sort,
                filter: filter,
                onTotalSize: { [weak self] value in
                    guard self?.generation == currentGeneration else { return }
                    self?.totalSize = value
                }
            ),
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
    }
}
