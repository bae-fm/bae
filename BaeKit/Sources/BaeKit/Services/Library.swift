import Foundation

/// Library reads — album/release lookups, pagination, search,
/// storage-summary listing, prefetching release detail, resolving
/// queue-input ids to flat track-id lists. The read side of bae-core's
/// catalog, narrow to what view layers ask for — plus the library page's
/// own display-preference write (`setLibraryFullWidth`).
public final class Library: Sendable, Observable {
    /// The album list under one sort, read through the windows its visible
    /// pages ask for.
    public let albumBrowse:
        @Sendable (_ sortCriteria: [BridgeSortCriterion])
            -> LibraryBrowseQuery<BridgeAlbum>
    public let getAlbumIndex:
        @Sendable (_ sortCriteria: [BridgeSortCriterion], _ albumId: String)
            async throws -> UInt64?
    public let composerBrowse:
        @Sendable (_ sortCriteria: [BridgeComposerSortCriterion])
            -> LibraryBrowseQuery<BridgeComposerSummary>
    public let artistBrowse:
        @Sendable (_ sortCriteria: [BridgeArtistSortCriterion])
            -> LibraryBrowseQuery<BridgeArtistSummary>
    /// One live library search, pointed at each new query in place.
    public let librarySearch: @Sendable () -> LibrarySearch
    /// Open a detail view's live read, pointed at each item it shows in place.
    public let albumDetail: @Sendable () -> DetailQuery<BridgeAlbumDetail>
    public let releaseDetail: @Sendable () -> DetailQuery<BridgeRelease>
    public let artistDetail: @Sendable () -> DetailQuery<BridgeArtistDetail>
    public let composerDetail: @Sendable () -> DetailQuery<BridgeComposerDetail>
    public let workDetail: @Sendable () -> DetailQuery<BridgeWorkDetail>
    /// One live read of the album grid's multi-selection, pointed at each new
    /// selection in place.
    public let albumSelection: @Sendable () -> AlbumSelectionQuery
    public let searchArtists:
        @Sendable (_ query: String) async throws -> [BridgeArtistSearchResult]
    /// The Storage Manager list under a first sort and filter, read through
    /// one query whose view moves in place.
    public let storageBrowse:
        @Sendable (_ sort: BridgeStorageSort, _ filter: BridgeStorageFilter)
            -> StorageBrowseQuery
    public let resolveToTrackIds:
        @Sendable (_ ids: [String]) async throws -> [String]
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. The write's config
    /// config subscription re-renders the page through `ConfigStore`.
    public let setLibraryFullWidth: @Sendable (_ enabled: Bool) throws -> Void

    public init(
        albumBrowse:
            @escaping @Sendable ([BridgeSortCriterion])
            -> LibraryBrowseQuery<BridgeAlbum> = { _ in
                fatalError("Library album browse is not installed")
            },
        getAlbumIndex:
            @escaping @Sendable ([BridgeSortCriterion], String) async throws
            -> UInt64? = { _, _ in throw StubError.notImplemented },
        composerBrowse:
            @escaping @Sendable ([BridgeComposerSortCriterion])
            -> LibraryBrowseQuery<BridgeComposerSummary> = { _ in
                fatalError("Library composer browse is not installed")
            },
        artistBrowse:
            @escaping @Sendable ([BridgeArtistSortCriterion])
            -> LibraryBrowseQuery<BridgeArtistSummary> = { _ in
                fatalError("Library artist browse is not installed")
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
        workDetail: @escaping @Sendable () -> DetailQuery<BridgeWorkDetail> = {
            fatalError("Library work detail is not installed")
        },
        albumSelection: @escaping @Sendable () -> AlbumSelectionQuery = {
            fatalError("Library album selection is not installed")
        },
        searchArtists:
            @escaping @Sendable (String) async throws
            -> [BridgeArtistSearchResult] = { _ in [] },
        storageBrowse:
            @escaping @Sendable (BridgeStorageSort, BridgeStorageFilter)
            -> StorageBrowseQuery = { _, _ in
                fatalError("Library storage browse is not installed")
            },
        resolveToTrackIds:
            @escaping @Sendable ([String]) async throws -> [String] = {
                _ in throw StubError.notImplemented
            },
        setLibraryFullWidth: @escaping @Sendable (Bool) throws -> Void = {
            _ in throw StubError.notImplemented
        }
    ) {
        self.albumBrowse = albumBrowse
        self.getAlbumIndex = getAlbumIndex
        self.composerBrowse = composerBrowse
        self.artistBrowse = artistBrowse
        self.librarySearch = librarySearch
        self.albumDetail = albumDetail
        self.releaseDetail = releaseDetail
        self.artistDetail = artistDetail
        self.composerDetail = composerDetail
        self.workDetail = workDetail
        self.albumSelection = albumSelection
        self.searchArtists = searchArtists
        self.storageBrowse = storageBrowse
        self.resolveToTrackIds = resolveToTrackIds
        self.setLibraryFullWidth = setLibraryFullWidth
    }

    // The desktop import surfaces reach the import service through `Importer`,
    // not here; this `handle`-wiring convenience initializer covers the reads
    // the desktop library page makes. The iOS `AppService` builds `Library`
    // via the designated initializer with just the iOS-available closures.
    #if !os(iOS)
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                albumBrowse: {
                    LibraryBrowseQuery(
                        handle.subscribeAlbumBrowse(sortCriteria: $0)
                    )
                },
                getAlbumIndex: {
                    try await handle.getAlbumIndex(
                        sortCriteria: $0,
                        albumId: $1
                    )
                },
                composerBrowse: {
                    LibraryBrowseQuery(
                        handle.subscribeComposerBrowse(sortCriteria: $0)
                    )
                },
                artistBrowse: {
                    LibraryBrowseQuery(
                        handle.subscribeArtistBrowse(sortCriteria: $0)
                    )
                },
                librarySearch: {
                    LibrarySearch(handle.subscribeLibrarySearch())
                },
                albumDetail: {
                    DetailQuery(handle.subscribeAlbumDetail())
                },
                releaseDetail: {
                    DetailQuery(handle.subscribeReleaseDetail())
                },
                artistDetail: {
                    DetailQuery(handle.subscribeArtistDetail())
                },
                composerDetail: {
                    DetailQuery(handle.subscribeComposerDetail())
                },
                workDetail: {
                    DetailQuery(handle.subscribeWorkDetail())
                },
                albumSelection: {
                    AlbumSelectionQuery(handle.subscribeAlbumSelection())
                },
                searchArtists: {
                    try await handle.searchArtists(query: $0)
                },
                storageBrowse: {
                    StorageBrowseQuery(
                        handle.subscribeStorageBrowse(sort: $0, filter: $1)
                    )
                },
                resolveToTrackIds: {
                    try await handle.resolveToTrackIds(ids: $0)
                },
                setLibraryFullWidth: {
                    try handle.setLibraryFullWidth(enabled: $0)
                }
            )
        }
    #else
        // `getAlbumIndex` and `storageBrowse` back desktop-only
        // surfaces (album-index scrolling and the Storage Manager) and go
        // unused here.
        // This wires only the reads iOS actually makes; the rest keep their
        // throwing stub defaults.
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                albumBrowse: {
                    LibraryBrowseQuery(
                        handle.subscribeAlbumBrowse(sortCriteria: $0)
                    )
                },
                composerBrowse: {
                    LibraryBrowseQuery(
                        handle.subscribeComposerBrowse(sortCriteria: $0)
                    )
                },
                artistBrowse: {
                    LibraryBrowseQuery(
                        handle.subscribeArtistBrowse(sortCriteria: $0)
                    )
                },
                librarySearch: {
                    LibrarySearch(handle.subscribeLibrarySearch())
                },
                albumDetail: {
                    DetailQuery(handle.subscribeAlbumDetail())
                },
                releaseDetail: {
                    DetailQuery(handle.subscribeReleaseDetail())
                },
                artistDetail: {
                    DetailQuery(handle.subscribeArtistDetail())
                },
                composerDetail: {
                    DetailQuery(handle.subscribeComposerDetail())
                },
                workDetail: {
                    DetailQuery(handle.subscribeWorkDetail())
                },
                searchArtists: {
                    try await handle.searchArtists(query: $0)
                },
                resolveToTrackIds: {
                    try await handle.resolveToTrackIds(ids: $0)
                }
            )
        }
    #endif

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Library { Library() }
    #endif
}

extension LibraryBrowseQuery where Row == BridgeAlbum {
    init(_ subscription: any AlbumBrowseSubscriptionProtocol) {
        self.init(
            setWindows: { try subscription.setWindows(windows: $0) },
            next: {
                let snapshot = try await subscription.next()
                return LibraryBrowseDelivery(
                    windows: snapshot.windows.map {
                        .init(window: $0.window, rows: $0.rows)
                    },
                    totalCount: Int(snapshot.totalCount)
                )
            },
            cancel: { try? await subscription.cancel() }
        )
    }
}

extension LibraryBrowseQuery where Row == BridgeComposerSummary {
    init(_ subscription: any ComposerBrowseSubscriptionProtocol) {
        self.init(
            setWindows: { try subscription.setWindows(windows: $0) },
            next: {
                let snapshot = try await subscription.next()
                return LibraryBrowseDelivery(
                    windows: snapshot.windows.map {
                        .init(window: $0.window, rows: $0.rows)
                    },
                    totalCount: Int(snapshot.totalCount)
                )
            },
            cancel: { try? await subscription.cancel() }
        )
    }
}

extension LibraryBrowseQuery where Row == BridgeArtistSummary {
    init(_ subscription: any ArtistBrowseSubscriptionProtocol) {
        self.init(
            setWindows: { try subscription.setWindows(windows: $0) },
            next: {
                let snapshot = try await subscription.next()
                return LibraryBrowseDelivery(
                    windows: snapshot.windows.map {
                        .init(window: $0.window, rows: $0.rows)
                    },
                    totalCount: Int(snapshot.totalCount)
                )
            },
            cancel: { try? await subscription.cancel() }
        )
    }
}

extension DetailQuery where Value == BridgeAlbumDetail {
    init(_ subscription: any AlbumDetailSubscriptionProtocol) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            next: {
                let snapshot = try await subscription.next()
                return DetailDelivery(id: snapshot.id, value: snapshot.value)
            },
            cancel: { try? await subscription.cancel() }
        )
    }
}

extension DetailQuery where Value == BridgeRelease {
    init(_ subscription: any ReleaseDetailSubscriptionProtocol) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            next: {
                let snapshot = try await subscription.next()
                return DetailDelivery(id: snapshot.id, value: snapshot.value)
            },
            cancel: { try? await subscription.cancel() }
        )
    }
}

extension DetailQuery where Value == BridgeArtistDetail {
    init(_ subscription: any ArtistDetailSubscriptionProtocol) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            next: {
                let snapshot = try await subscription.next()
                return DetailDelivery(id: snapshot.id, value: snapshot.value)
            },
            cancel: { try? await subscription.cancel() }
        )
    }
}

extension DetailQuery where Value == BridgeComposerDetail {
    init(_ subscription: any ComposerDetailSubscriptionProtocol) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            next: {
                let snapshot = try await subscription.next()
                return DetailDelivery(id: snapshot.id, value: snapshot.value)
            },
            cancel: { try? await subscription.cancel() }
        )
    }
}

extension DetailQuery where Value == BridgeWorkDetail {
    init(_ subscription: any WorkDetailSubscriptionProtocol) {
        self.init(
            setId: { try subscription.setId(id: $0) },
            next: {
                let snapshot = try await subscription.next()
                return DetailDelivery(id: snapshot.id, value: snapshot.value)
            },
            cancel: { try? await subscription.cancel() }
        )
    }
}

/// A live read of the album grid's multi-selection: set the selected ids,
/// and take each value it delivers — the summary of every selected album
/// still in the library, beside the ids it was asked for.
public struct AlbumSelectionQuery: Sendable {
    public let setAlbums: @Sendable ([String]) throws -> Void
    public let next: @Sendable () async throws -> BridgeAlbumSelectionSnapshot
    public let cancel: @Sendable () async -> Void

    public init(
        setAlbums: @escaping @Sendable ([String]) throws -> Void,
        next:
            @escaping @Sendable () async throws -> BridgeAlbumSelectionSnapshot,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setAlbums = setAlbums
        self.next = next
        self.cancel = cancel
    }

    init(_ subscription: any AlbumSelectionSubscriptionProtocol) {
        self.init(
            setAlbums: { try subscription.setAlbums(albumIds: $0) },
            next: { try await subscription.next() },
            cancel: { try? await subscription.cancel() }
        )
    }
}

/// A live library search whose query changes in place: set the text, and
/// take each value it delivers, which names the query it answers.
public struct LibrarySearch: Sendable {
    public let setQuery: @Sendable (String) throws -> Void
    public let next: @Sendable () async throws -> BridgeLibrarySearchSnapshot
    public let cancel: @Sendable () async -> Void

    public init(
        setQuery: @escaping @Sendable (String) throws -> Void,
        next:
            @escaping @Sendable () async throws -> BridgeLibrarySearchSnapshot,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setQuery = setQuery
        self.next = next
        self.cancel = cancel
    }

    init(_ subscription: any LibrarySearchSubscriptionProtocol) {
        self.init(
            setQuery: { _ = try subscription.setQuery(query: $0) },
            next: { try await subscription.next() },
            cancel: { try? await subscription.cancel() }
        )
    }
}

/// Error raised by stub closures whose return type can't be defaulted
/// to a trivial value (e.g. compound bridge records). Previews don't
/// reach this in practice — the view tree just renders the placeholder
/// branch when the call throws.
public enum StubError: Error {
    case notImplemented
}
