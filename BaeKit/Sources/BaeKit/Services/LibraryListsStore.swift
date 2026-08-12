import Observation

@MainActor
@Observable
public final class LibraryListsStore {
    public private(set) var albums: AlbumList?
    public private(set) var composers: ComposerList?
    public private(set) var artists: ArtistList?

    @ObservationIgnored
    private let library: Library
    @ObservationIgnored
    private let libraryStore: LibraryStore
    @ObservationIgnored
    private let onError: (any Error) -> Void
    @ObservationIgnored
    private var albumTask: Task<Void, Never>?
    @ObservationIgnored
    private var composerTask: Task<Void, Never>?
    @ObservationIgnored
    private var artistTask: Task<Void, Never>?
    @ObservationIgnored
    private var albumSort: [BridgeSortCriterion]?
    @ObservationIgnored
    private var composerSort: [BridgeComposerSortCriterion]?
    @ObservationIgnored
    private var artistSort: [BridgeArtistSortCriterion]?

    public init(
        library: Library,
        libraryStore: LibraryStore,
        onError: @escaping (any Error) -> Void
    ) {
        self.library = library
        self.libraryStore = libraryStore
        self.onError = onError
    }

    public func updateAlbums(_ sort: [BridgeSortCriterion]) {
        guard albumSort != sort else { return }
        albumSort = sort
        albumTask?.cancel()
        albums?.cancel()
        albumTask = Task { [weak self, library, libraryStore, onError] in
            let list = AlbumList(
                pageSource: LibraryAlbumPageSource(
                    library: library,
                    sort: sort
                ),
                ingest: { rows in
                    for row in rows {
                        _ = libraryStore.internAlbumSummary(row)
                    }
                },
                onError: onError
            )
            await list.loadInitial()
            guard !Task.isCancelled, self?.albumSort == sort else { return }
            self?.albums = list
        }
    }

    public func updateComposers(_ sort: [BridgeComposerSortCriterion]) {
        guard composerSort != sort else { return }
        composerSort = sort
        composerTask?.cancel()
        composers?.cancel()
        composerTask = Task { [weak self, library, libraryStore, onError] in
            let list = ComposerList(
                pageSource: LibraryComposerPageSource(
                    library: library,
                    sort: sort
                ),
                ingest: { rows in
                    for row in rows {
                        _ = libraryStore.internComposerSummary(row)
                    }
                },
                onError: onError
            )
            await list.loadInitial()
            guard !Task.isCancelled, self?.composerSort == sort else { return }
            self?.composers = list
        }
    }

    public func updateArtists(_ sort: [BridgeArtistSortCriterion]) {
        guard artistSort != sort else { return }
        artistSort = sort
        artistTask?.cancel()
        artists?.cancel()
        artistTask = Task { [weak self, library, libraryStore, onError] in
            let list = ArtistList(
                pageSource: LibraryArtistPageSource(
                    library: library,
                    sort: sort
                ),
                ingest: { rows in
                    for row in rows {
                        _ = libraryStore.internArtistSummary(row)
                    }
                },
                onError: onError
            )
            await list.loadInitial()
            guard !Task.isCancelled, self?.artistSort == sort else { return }
            self?.artists = list
        }
    }
}
