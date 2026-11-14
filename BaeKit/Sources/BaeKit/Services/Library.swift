import Foundation

/// Library reads — album/release lookups, pagination, search,
/// storage-summary listing, prefetching release detail, resolving
/// queue-input ids to flat track-id lists. The read side of bae-library's
/// catalog, narrow to what view layers ask for — plus the library page's
/// own display-preference write (`setLibraryFullWidth`).
public final class Library: Sendable, Observable {
    /// The album list under one sort and grouping: its windows, count and
    /// positions, each a live read of its own.
    public let albumList:
        @Sendable (_ sortCriteria: [BridgeSortCriterion], _ groupByArtist: Bool)
            -> BridgeListQueries<BridgeAlbum>
    public let getAlbumIndex:
        @Sendable (
            _ sortCriteria: [BridgeSortCriterion], _ albumId: String,
            _ groupByArtist: Bool
        )
            async throws -> UInt64?
    /// The albums of `albumIds` the album list holds, each once, in its order
    /// under one sort and grouping: what an action on the grid's selection
    /// acts on, in that order.
    public let albumsInBrowseOrder:
        @Sendable (
            _ sortCriteria: [BridgeSortCriterion], _ albumIds: [String],
            _ groupByArtist: Bool
        )
            async throws -> [BridgeAlbum]
    /// The composer list under one sort.
    public let composerList:
        @Sendable (_ sortCriteria: [BridgeComposerSortCriterion])
            -> BridgeListQueries<BridgeComposerSummary>
    /// The artist list under one sort.
    public let artistList:
        @Sendable (_ sortCriteria: [BridgeArtistSortCriterion])
            -> BridgeListQueries<BridgeArtistSummary>
    /// One live library search, pointed at each new query in place.
    public let librarySearch: @Sendable () -> LibrarySearch
    /// Open a detail view's live read, pointed at each item it shows in place.
    public let albumDetail: @Sendable () -> DetailQuery<BridgeAlbumDetail>
    public let releaseDetail: @Sendable () -> DetailQuery<BridgeRelease>
    public let artistDetail: @Sendable () -> DetailQuery<BridgeArtistDetail>
    public let composerDetail: @Sendable () -> DetailQuery<BridgeComposerDetail>
    /// The rows of one composer's page.
    public let composerPageList:
        @Sendable (_ artistId: String)
            -> BridgeListQueries<BridgeComposerDetailRow>
    public let workDetail: @Sendable () -> DetailQuery<BridgeWorkDetail>
    /// Open the live read of every playlist this device holds.
    public let playlists: @Sendable () -> PulledValues<BridgePlaylistListing>
    /// Open the playlist page's live read, pointed at each playlist it shows
    /// in place.
    public let playlistDetail: @Sendable () -> DetailQuery<BridgePlaylistDetail>
    /// One live read of the album grid's multi-selection, pointed at each new
    /// selection in place.
    public let albumSelection: @Sendable () -> AlbumSelectionQuery
    /// The artists an artist picker's text finds: each text is a list of its
    /// own.
    public let artistSearch: ArtistSearchOpener
    /// The Storage Manager list under one sort, filter and search text.
    public let storageList:
        @Sendable (
            _ sort: BridgeStorageSort, _ filter: BridgeStorageFilter,
            _ search: String
        ) -> BridgeListQueries<BridgeStorageRow>
    /// The rows of the releases selected in the Storage Manager list under
    /// one filter and search text that the list holds.
    public let storageSelection:
        @Sendable (_ filter: BridgeStorageFilter, _ search: String)
            -> StorageSelectionQuery
    /// The transfer queues holding work, as the Storage Manager shows them.
    public let storageQueues: @Sendable () -> PulledValues<BridgeStorageQueues>
    /// Open the Storage Manager inspector's live read of the release it
    /// shows: its files beside their uploads and its entry in each transfer
    /// queue, read in core.
    public let storageInspector:
        @Sendable () -> DetailQuery<BridgeStorageInspector>
    /// Open the Storage sheet's live read of a release's files, in the order
    /// its file table sorts by, read in core.
    public let releaseFiles: @Sendable () -> ReleaseFilesQuery
    /// Open the live read of a library release's cover picker, which shows
    /// nothing until a request names a release.
    public let coverGallery: @Sendable () -> CoverGalleryQuery
    public let resolveToTrackIds:
        @Sendable (_ ids: [String]) async throws -> [String]
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. The write's config
    /// config subscription re-renders the page through `ConfigStore`.
    public let setLibraryFullWidth:
        @Sendable (_ enabled: Bool) async throws -> Void
    /// Make a playlist of this device's person holding `trackIds` in order,
    /// answering its id. A failed write leaves no playlist.
    public let createPlaylist:
        @Sendable (
            _ name: String, _ sharing: BridgeSharing, _ trackIds: [String]
        ) async throws -> String
    /// Add a track to the end of a playlist of this device's person.
    public let appendPlaylistEntry:
        @Sendable (_ playlistId: String, _ trackId: String) async throws
            -> Void

    public init(
        albumList:
            @escaping @Sendable ([BridgeSortCriterion], Bool)
            -> BridgeListQueries<BridgeAlbum> = { _, _ in
                fatalError("Library album list is not installed")
            },
        getAlbumIndex:
            @escaping @Sendable ([BridgeSortCriterion], String, Bool)
            async throws
            -> UInt64? = { _, _, _ in throw StubError.notImplemented },
        albumsInBrowseOrder:
            @escaping @Sendable ([BridgeSortCriterion], [String], Bool)
            async throws
            -> [BridgeAlbum] = { _, _, _ in throw StubError.notImplemented },
        composerList:
            @escaping @Sendable ([BridgeComposerSortCriterion])
            -> BridgeListQueries<BridgeComposerSummary> = { _ in
                fatalError("Library composer list is not installed")
            },
        artistList:
            @escaping @Sendable ([BridgeArtistSortCriterion])
            -> BridgeListQueries<BridgeArtistSummary> = { _ in
                fatalError("Library artist list is not installed")
            },
        librarySearch: @escaping @Sendable () -> LibrarySearch = {
            fatalError("Library search is not installed")
        },
        albumDetail:
            @escaping @Sendable () -> DetailQuery<BridgeAlbumDetail> = {
                fatalError("Library album detail is not installed")
            },
        releaseDetail: @escaping @Sendable () -> DetailQuery<BridgeRelease> = {
            fatalError("Library release detail is not installed")
        },
        artistDetail:
            @escaping @Sendable () -> DetailQuery<BridgeArtistDetail> = {
                fatalError("Library artist detail is not installed")
            },
        composerDetail:
            @escaping @Sendable () -> DetailQuery<BridgeComposerDetail> = {
                fatalError("Library composer detail is not installed")
            },
        composerPageList:
            @escaping @Sendable (String)
            -> BridgeListQueries<BridgeComposerDetailRow> = { _ in
                fatalError("Library composer page list is not installed")
            },
        workDetail: @escaping @Sendable () -> DetailQuery<BridgeWorkDetail> = {
            fatalError("Library work detail is not installed")
        },
        playlists:
            @escaping @Sendable () -> PulledValues<BridgePlaylistListing> = {
                fatalError("Library playlists are not installed")
            },
        playlistDetail:
            @escaping @Sendable () -> DetailQuery<BridgePlaylistDetail> = {
                fatalError("Library playlist detail is not installed")
            },
        albumSelection: @escaping @Sendable () -> AlbumSelectionQuery = {
            fatalError("Library album selection is not installed")
        },
        artistSearch: @escaping ArtistSearchOpener = { _ in
            fatalError("Library artist search is not installed")
        },
        storageList:
            @escaping @Sendable (
                BridgeStorageSort, BridgeStorageFilter, String
            ) -> BridgeListQueries<BridgeStorageRow> = { _, _, _ in
                fatalError("Library storage list is not installed")
            },
        storageSelection:
            @escaping @Sendable (BridgeStorageFilter, String)
            -> StorageSelectionQuery = { _, _ in
                fatalError("Library storage selection is not installed")
            },
        storageQueues:
            @escaping @Sendable () -> PulledValues<BridgeStorageQueues> = {
                fatalError("Library storage queues are not installed")
            },
        storageInspector:
            @escaping @Sendable () -> DetailQuery<BridgeStorageInspector> = {
                fatalError("Library storage inspector is not installed")
            },
        releaseFiles: @escaping @Sendable () -> ReleaseFilesQuery = {
            fatalError("Library release files are not installed")
        },
        coverGallery: @escaping @Sendable () -> CoverGalleryQuery = {
            fatalError("Library cover gallery is not installed")
        },
        resolveToTrackIds:
            @escaping @Sendable ([String]) async throws -> [String] = {
                _ in throw StubError.notImplemented
            },
        setLibraryFullWidth: @escaping @Sendable (Bool) async throws -> Void = {
            _ in throw StubError.notImplemented
        },
        createPlaylist:
            @escaping @Sendable (String, BridgeSharing, [String]) async throws
            -> String = { _, _, _ in throw StubError.notImplemented },
        appendPlaylistEntry:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in throw StubError.notImplemented
            }
    ) {
        self.albumList = albumList
        self.getAlbumIndex = getAlbumIndex
        self.albumsInBrowseOrder = albumsInBrowseOrder
        self.composerList = composerList
        self.artistList = artistList
        self.librarySearch = librarySearch
        self.albumDetail = albumDetail
        self.releaseDetail = releaseDetail
        self.artistDetail = artistDetail
        self.composerDetail = composerDetail
        self.composerPageList = composerPageList
        self.workDetail = workDetail
        self.playlists = playlists
        self.playlistDetail = playlistDetail
        self.albumSelection = albumSelection
        self.artistSearch = artistSearch
        self.storageList = storageList
        self.storageSelection = storageSelection
        self.storageQueues = storageQueues
        self.storageInspector = storageInspector
        self.releaseFiles = releaseFiles
        self.coverGallery = coverGallery
        self.resolveToTrackIds = resolveToTrackIds
        self.setLibraryFullWidth = setLibraryFullWidth
        self.createPlaylist = createPlaylist
        self.appendPlaylistEntry = appendPlaylistEntry
    }

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Library { Library() }
    #endif
}

extension Library {
    // The desktop import surfaces reach the import service through `Importer`,
    // not here; this `handle`-wiring convenience initializer covers the reads
    // the desktop library page makes, some of them absent from the iOS
    // bindings.
    #if os(macOS)
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                albumList: Self.albumList(handle),
                getAlbumIndex: Self.albumIndex(handle),
                albumsInBrowseOrder: Self.albumsInBrowseOrder(handle),
                composerList: Self.composerList(handle),
                artistList: Self.artistList(handle),
                librarySearch: {
                    LibrarySearch(handle.subscribeLibrarySearch())
                },
                albumDetail: { DetailQuery(handle.subscribeAlbumDetail()) },
                releaseDetail: { DetailQuery(handle.subscribeReleaseDetail()) },
                artistDetail: { DetailQuery(handle.subscribeArtistDetail()) },
                composerDetail: {
                    DetailQuery(handle.subscribeComposerDetail())
                },
                composerPageList: Self.composerPageList(handle),
                workDetail: { DetailQuery(handle.subscribeWorkDetail()) },
                playlists: Self.playlists(handle),
                playlistDetail: Self.playlistDetail(handle),
                albumSelection: {
                    AlbumSelectionQuery(handle.subscribeAlbumSelection())
                },
                artistSearch: Self.artistSearch(handle),
                storageList: Self.storageList(handle),
                storageSelection: {
                    StorageSelectionQuery(
                        handle.subscribeStorageSelection(
                            filter: $0,
                            search: $1
                        )
                    )
                },
                storageQueues: {
                    PulledValues(handle.subscribeStorageQueues())
                },
                storageInspector: {
                    DetailQuery(handle.subscribeStorageInspector())
                },
                releaseFiles: {
                    ReleaseFilesQuery(handle.subscribeReleaseFiles())
                },
                coverGallery: {
                    CoverGalleryQuery(handle.subscribeCoverGallery())
                },
                resolveToTrackIds: {
                    try await handle.resolveToTrackIds(ids: $0)
                },
                setLibraryFullWidth: {
                    try await handle.setLibraryFullWidth(enabled: $0)
                },
                createPlaylist: Self.createPlaylist(handle),
                appendPlaylistEntry: Self.appendPlaylistEntry(handle)
            )
        }

        /// Where an album sits in the album list, read through `handle`.
        private static func albumIndex(
            _ handle: any AppHandleProtocol
        )
            -> @Sendable ([BridgeSortCriterion], String, Bool) async throws
            -> UInt64?
        {
            {
                try await handle.getAlbumIndex(
                    sortCriteria: $0,
                    albumId: $1,
                    groupByArtist: $2
                )
            }
        }

        /// The album list's albums of a selection in its order, read through
        /// `handle`.
        private static func albumsInBrowseOrder(
            _ handle: any AppHandleProtocol
        )
            -> @Sendable ([BridgeSortCriterion], [String], Bool) async throws
            -> [BridgeAlbum]
        {
            {
                try await handle.albumsInBrowseOrder(
                    sortCriteria: $0,
                    albumIds: $1,
                    groupByArtist: $2
                )
            }
        }

        /// Every playlist this device holds, read through `handle`.
        private static func playlists(
            _ handle: any AppHandleProtocol
        ) -> @Sendable () -> PulledValues<BridgePlaylistListing> {
            { PulledValues(handle.subscribePlaylists()) }
        }

        /// The playlist page's read through `handle`.
        private static func playlistDetail(
            _ handle: any AppHandleProtocol
        ) -> @Sendable () -> DetailQuery<BridgePlaylistDetail> {
            { DetailQuery(handle.subscribePlaylistDetail()) }
        }

        /// Add a track to a playlist through `handle`.
        private static func appendPlaylistEntry(
            _ handle: any AppHandleProtocol
        ) -> @Sendable (String, String) async throws -> Void {
            {
                _ = try await handle.appendPlaylistEntry(
                    playlist: $0,
                    track: $1
                )
            }
        }

        /// Make a playlist through `handle`.
        private static func createPlaylist(
            _ handle: any AppHandleProtocol
        )
            -> @Sendable (String, BridgeSharing, [String]) async throws
            -> String
        {
            {
                try await handle.createPlaylist(
                    name: $0,
                    sharing: $1,
                    trackIds: $2
                )
            }
        }

        /// The album list read through `handle`, under each sort and grouping
        /// asked for.
        private static func albumList(
            _ handle: any AppHandleProtocol
        )
            -> @Sendable ([BridgeSortCriterion], Bool) -> BridgeListQueries<
                BridgeAlbum
            >
        {
            let list:
                @Sendable ([BridgeSortCriterion], Bool) -> BridgeListQueries<
                    BridgeAlbum
                > = {
                    sort,
                    groupByArtist in
                    var sections:
                        (
                            @Sendable () -> PulledValues<
                                [BridgeLibraryBrowseSection]
                            >
                        )?
                    if groupByArtist {
                        sections = {
                            let read = handle.subscribeAlbumSections(
                                sortCriteria: sort
                            )
                            return PulledValues<[BridgeLibraryBrowseSection]>(
                                read
                            )
                        }
                    }
                    let rows: @Sendable (BridgeListRows) -> [BridgeAlbum] = Self
                        .albumRows
                    return BridgeListQueries<BridgeAlbum>(
                        handle: handle,
                        list: BridgeList.albumGrid(
                            sort: sort,
                            groupByArtist: groupByArtist
                        ),
                        sections: sections,
                        rows: rows
                    )
                }
            return list
        }

        /// The albums of a delivery of the album grid's window.
        private static func albumRows(_ rows: BridgeListRows) -> [BridgeAlbum] {
            guard case .albums(let appearances) = rows else {
                preconditionFailure(
                    "the album grid delivered rows of another list"
                )
            }
            return appearances.map(\.album)
        }

        /// The composer list under each sort asked for, read through `handle`.
        private static func composerList(
            _ handle: any AppHandleProtocol
        )
            -> @Sendable ([BridgeComposerSortCriterion]) -> BridgeListQueries<
                BridgeComposerSummary
            >
        {
            { sort in
                BridgeListQueries(
                    handle: handle,
                    list: .composers(sort: sort),
                    rows: Self.composerRows
                )
            }
        }

        /// The artist list under each sort asked for, read through `handle`.
        private static func artistList(
            _ handle: any AppHandleProtocol
        )
            -> @Sendable ([BridgeArtistSortCriterion]) -> BridgeListQueries<
                BridgeArtistSummary
            >
        {
            { sort in
                BridgeListQueries(
                    handle: handle,
                    list: .artists(sort: sort),
                    rows: Self.artistRows
                )
            }
        }

        /// One composer's page rows, read through `handle`.
        private static func composerPageList(
            _ handle: any AppHandleProtocol
        )
            -> @Sendable (String) -> BridgeListQueries<BridgeComposerDetailRow>
        {
            { artistId in
                BridgeListQueries(
                    handle: handle,
                    list: .composerPage(artistId: artistId),
                    rows: Self.composerPageRows
                )
            }
        }

        /// The artists each text an artist picker types finds, read through
        /// `handle`.
        private static func artistSearch(
            _ handle: any AppHandleProtocol
        ) -> ArtistSearchOpener {
            { text in
                BridgeListQueries(
                    handle: handle,
                    list: .artistSearch(text: text),
                    rows: Self.artistSearchRows
                )
            }
        }

        /// The Storage Manager list under each sort, filter and search text
        /// asked for, read through `handle`.
        private static func storageList(
            _ handle: any AppHandleProtocol
        )
            -> @Sendable (
                BridgeStorageSort, BridgeStorageFilter, String
            ) -> BridgeListQueries<BridgeStorageRow>
        {
            { sort, filter, search in
                BridgeListQueries(
                    handle: handle,
                    list: .storage(sort: sort, filter: filter, search: search),
                    rows: Self.storageRows
                )
            }
        }

        private static func storageRows(
            _ rows: BridgeListRows
        ) -> [BridgeStorageRow] {
            guard case .storage(let rows) = rows else {
                preconditionFailure(
                    "the Storage Manager delivered rows of another list"
                )
            }
            return rows
        }

        private static func composerRows(
            _ rows: BridgeListRows
        ) -> [BridgeComposerSummary] {
            guard case .composers(let rows) = rows else {
                preconditionFailure(
                    "the composer list delivered rows of another list"
                )
            }
            return rows
        }

        private static func artistRows(
            _ rows: BridgeListRows
        ) -> [BridgeArtistSummary] {
            guard case .artists(let rows) = rows else {
                preconditionFailure(
                    "the artist list delivered rows of another list"
                )
            }
            return rows
        }

        private static func composerPageRows(
            _ rows: BridgeListRows
        ) -> [BridgeComposerDetailRow] {
            guard case .composerPage(let rows) = rows else {
                preconditionFailure(
                    "a composer's page delivered rows of another list"
                )
            }
            return rows
        }

        private static func artistSearchRows(
            _ rows: BridgeListRows
        ) -> [BridgeArtistSearchResult] {
            guard case .artistSearch(let rows) = rows else {
                preconditionFailure(
                    "the artist search delivered rows of another list"
                )
            }
            return rows
        }
    #endif
}

extension KeyedDetailQuery where Id == String, Value == BridgeAlbumDetail {
    init(_ subscription: AlbumDetailSubscription) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            values: PulledValues(subscription) {
                DetailDelivery(id: $0.id, value: $0.value)
            }
        )
    }
}

extension KeyedDetailQuery where Id == String, Value == BridgeRelease {
    init(_ subscription: ReleaseDetailSubscription) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            values: PulledValues(subscription) {
                DetailDelivery(id: $0.id, value: $0.value)
            }
        )
    }
}

extension KeyedDetailQuery where Id == String, Value == BridgeArtistDetail {
    init(_ subscription: ArtistDetailSubscription) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            values: PulledValues(subscription) {
                DetailDelivery(id: $0.id, value: $0.value)
            }
        )
    }
}

extension KeyedDetailQuery where Id == String, Value == BridgeComposerDetail {
    init(_ subscription: ComposerDetailSubscription) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            values: PulledValues(subscription) {
                DetailDelivery(id: $0.id, value: $0.value)
            }
        )
    }
}

extension KeyedDetailQuery where Id == String, Value == BridgePlaylistDetail {
    init(_ subscription: PlaylistDetailSubscription) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            values: PulledValues(subscription) {
                DetailDelivery(id: $0.id, value: $0.value)
            }
        )
    }
}

extension KeyedDetailQuery where Id == String, Value == BridgeWorkDetail {
    init(_ subscription: WorkDetailSubscription) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            values: PulledValues(subscription) {
                DetailDelivery(id: $0.id, value: $0.value)
            }
        )
    }
}

// The inspector's read exists in the desktop bindings only.
#if os(macOS)
    extension KeyedDetailQuery
    where Id == String, Value == BridgeStorageInspector {
        init(_ subscription: StorageInspectorSubscription) {
            self.init(
                setId: { try subscription.setId(id: $0) },
                values: PulledValues(subscription) {
                    DetailDelivery(id: $0.id, value: $0.value)
                }
            )
        }
    }
#endif

/// A live read of the album grid's multi-selection: set the selected ids,
/// and take each value it delivers — the summary of every selected album
/// still in the library, beside the ids it was asked for.
public struct AlbumSelectionQuery: Sendable {
    public let setAlbums: @Sendable ([String]) throws -> Void
    public let values: PulledValues<BridgeAlbumSelectionSnapshot>

    public init(
        setAlbums: @escaping @Sendable ([String]) throws -> Void,
        values: PulledValues<BridgeAlbumSelectionSnapshot>
    ) {
        self.setAlbums = setAlbums
        self.values = values
    }

    init(_ subscription: AlbumSelectionSubscription) {
        self.init(
            setAlbums: { try subscription.setAlbums(albumIds: $0) },
            values: PulledValues(subscription)
        )
    }
}

/// A live library search whose query changes in place: set the text, or
/// `nil` for no search, and take each value it delivers, which names the query
/// it answers.
public struct LibrarySearch: Sendable {
    public let setQuery: @Sendable (String?) throws -> Void
    public let values: PulledValues<BridgeLibrarySearchSnapshot>

    public init(
        setQuery: @escaping @Sendable (String?) throws -> Void,
        values: PulledValues<BridgeLibrarySearchSnapshot>
    ) {
        self.setQuery = setQuery
        self.values = values
    }

    init(_ subscription: LibrarySearchSubscription) {
        self.init(
            setQuery: { _ = try subscription.setQuery(query: $0) },
            values: PulledValues(subscription)
        )
    }
}

/// Error raised by stub closures whose return type can't be defaulted
/// to a trivial value (e.g. compound bridge records). Shots don't
/// reach this in practice — the view tree just renders the placeholder
/// branch when the call throws.
public enum StubError: Error {
    case notImplemented
}
