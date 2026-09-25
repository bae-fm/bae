import Foundation

public typealias LibraryLiveValue<Value: Sendable> = AsyncStream<
    Result<Value, BridgeError>
>

private final class LibraryLiveValueSink<Value: Sendable>:
    @unchecked Sendable
{
    let continuation: LibraryLiveValue<Value>.Continuation

    init(continuation: LibraryLiveValue<Value>.Continuation) {
        self.continuation = continuation
    }

    func onValue(_ value: Value) {
        continuation.yield(.success(value))
    }

    func onError(_ error: BridgeError) {
        continuation.yield(.failure(error))
    }
}

private final class AlbumDetailSink: AlbumDetailCallback, @unchecked Sendable {
    private let sink: LibraryLiveValueSink<BridgeAlbumDetail?>
    init(_ sink: LibraryLiveValueSink<BridgeAlbumDetail?>) { self.sink = sink }
    func onValue(value: BridgeAlbumDetail?) { sink.onValue(value) }
    func onError(error: BridgeError) { sink.onError(error) }
}

private final class ReleaseDetailSink: ReleaseDetailCallback,
    @unchecked Sendable
{
    private let sink: LibraryLiveValueSink<BridgeRelease?>
    init(_ sink: LibraryLiveValueSink<BridgeRelease?>) { self.sink = sink }
    func onValue(value: BridgeRelease?) { sink.onValue(value) }
    func onError(error: BridgeError) { sink.onError(error) }
}

private final class ComposerDetailSink: ComposerDetailCallback,
    @unchecked Sendable
{
    private let sink: LibraryLiveValueSink<BridgeComposerDetail?>
    init(_ sink: LibraryLiveValueSink<BridgeComposerDetail?>) {
        self.sink = sink
    }
    func onValue(value: BridgeComposerDetail?) { sink.onValue(value) }
    func onError(error: BridgeError) { sink.onError(error) }
}

private final class WorkDetailSink: WorkDetailCallback, @unchecked Sendable {
    private let sink: LibraryLiveValueSink<BridgeWorkDetail?>
    init(_ sink: LibraryLiveValueSink<BridgeWorkDetail?>) { self.sink = sink }
    func onValue(value: BridgeWorkDetail?) { sink.onValue(value) }
    func onError(error: BridgeError) { sink.onError(error) }
}

private final class ArtistDetailSink: ArtistDetailCallback, @unchecked Sendable
{
    private let sink: LibraryLiveValueSink<BridgeArtistDetail?>
    init(_ sink: LibraryLiveValueSink<BridgeArtistDetail?>) { self.sink = sink }
    func onValue(value: BridgeArtistDetail?) { sink.onValue(value) }
    func onError(error: BridgeError) { sink.onError(error) }
}

private final class StorageProjectionSink: StorageProjectionCallback,
    @unchecked Sendable
{
    private let sink: LibraryLiveValueSink<BridgeStorageProjection>
    init(_ sink: LibraryLiveValueSink<BridgeStorageProjection>) {
        self.sink = sink
    }
    func onValue(value: BridgeStorageProjection) { sink.onValue(value) }
    func onError(error: BridgeError) { sink.onError(error) }
}

private func libraryLiveValue<Value: Sendable, Callback: Sendable>(
    callback: (LibraryLiveValueSink<Value>) -> Callback,
    subscribe: (Callback) -> any LiveSubscriptionProtocol
) -> LibraryLiveValue<Value> {
    let (stream, continuation) = LibraryLiveValue<Value>.makeStream()
    let subscription = subscribe(
        callback(LibraryLiveValueSink(continuation: continuation))
    )
    continuation.onTermination = { _ in subscription.cancel() }
    return stream
}

// One stored closure per read, each with a matching designated-init
// parameter and assignment; its length tracks the number of Library reads,
// not logical complexity — the same shape the `handle:` convenience init
// below disables `function_body_length` for.
// swiftlint:disable type_body_length
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
    private let subscribeAlbumDetail:
        @Sendable (_ albumId: String, _ callback: AlbumDetailCallback)
            -> any LiveSubscriptionProtocol
    private let subscribeComposerDetail:
        @Sendable (_ artistId: String, _ callback: ComposerDetailCallback)
            -> any LiveSubscriptionProtocol
    private let subscribeWorkDetail:
        @Sendable (_ workId: String, _ callback: WorkDetailCallback)
            -> any LiveSubscriptionProtocol
    public let artistBrowse:
        @Sendable (_ sortCriteria: [BridgeArtistSortCriterion])
            -> LibraryBrowseQuery<BridgeArtistSummary>
    private let subscribeArtistDetail:
        @Sendable (_ artistId: String, _ callback: ArtistDetailCallback)
            -> any LiveSubscriptionProtocol
    /// One live library search, pointed at each new query in place.
    public let librarySearch: @Sendable () -> LibrarySearch
    public let searchArtists:
        @Sendable (_ query: String) async throws -> [BridgeArtistSearchResult]
    private let subscribeStorageProjection:
        @Sendable (
            _ sort: BridgeStorageSort, _ filter: BridgeStorageFilter,
            _ offset: UInt64, _ limit: UInt64,
            _ callback: StorageProjectionCallback
        ) -> any LiveSubscriptionProtocol
    private let subscribeReleaseDetail:
        @Sendable (_ releaseId: String, _ callback: ReleaseDetailCallback)
            -> any LiveSubscriptionProtocol
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
        subscribeAlbumDetail:
            @escaping @Sendable (String, AlbumDetailCallback)
            -> any LiveSubscriptionProtocol = { _, _ in
                fatalError("Library album-detail subscription is not installed")
            },
        subscribeComposerDetail:
            @escaping @Sendable (String, ComposerDetailCallback)
            -> any LiveSubscriptionProtocol = { _, _ in
                fatalError(
                    "Library composer-detail subscription is not installed"
                )
            },
        subscribeWorkDetail:
            @escaping @Sendable (String, WorkDetailCallback)
            -> any LiveSubscriptionProtocol = { _, _ in
                fatalError("Library work-detail subscription is not installed")
            },
        artistBrowse:
            @escaping @Sendable ([BridgeArtistSortCriterion])
            -> LibraryBrowseQuery<BridgeArtistSummary> = { _ in
                fatalError("Library artist browse is not installed")
            },
        subscribeArtistDetail:
            @escaping @Sendable (String, ArtistDetailCallback)
            -> any LiveSubscriptionProtocol = { _, _ in
                fatalError(
                    "Library artist-detail subscription is not installed"
                )
            },
        librarySearch: @escaping @Sendable () -> LibrarySearch = {
            fatalError("Library search is not installed")
        },
        searchArtists:
            @escaping @Sendable (String) async throws
            -> [BridgeArtistSearchResult] = { _ in [] },
        subscribeStorageProjection:
            @escaping @Sendable (
                BridgeStorageSort, BridgeStorageFilter, UInt64, UInt64,
                StorageProjectionCallback
            ) -> any LiveSubscriptionProtocol = { _, _, _, _, _ in
                fatalError("Library storage subscription is not installed")
            },
        subscribeReleaseDetail:
            @escaping @Sendable (String, ReleaseDetailCallback)
            -> any LiveSubscriptionProtocol = { _, _ in
                fatalError(
                    "Library release-detail subscription is not installed"
                )
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
        self.subscribeAlbumDetail = subscribeAlbumDetail
        self.subscribeComposerDetail = subscribeComposerDetail
        self.subscribeWorkDetail = subscribeWorkDetail
        self.artistBrowse = artistBrowse
        self.subscribeArtistDetail = subscribeArtistDetail
        self.librarySearch = librarySearch
        self.searchArtists = searchArtists
        self.subscribeStorageProjection = subscribeStorageProjection
        self.subscribeReleaseDetail = subscribeReleaseDetail
        self.resolveToTrackIds = resolveToTrackIds
        self.setLibraryFullWidth = setLibraryFullWidth
    }

    public func albumDetails(_ albumId: String)
        -> LibraryLiveValue<BridgeAlbumDetail?>
    {
        libraryLiveValue(
            callback: AlbumDetailSink.init,
            subscribe: { subscribeAlbumDetail(albumId, $0) }
        )
    }

    public func composerDetails(_ artistId: String)
        -> LibraryLiveValue<BridgeComposerDetail?>
    {
        libraryLiveValue(
            callback: ComposerDetailSink.init,
            subscribe: { subscribeComposerDetail(artistId, $0) }
        )
    }

    public func workDetails(_ workId: String)
        -> LibraryLiveValue<BridgeWorkDetail?>
    {
        libraryLiveValue(
            callback: WorkDetailSink.init,
            subscribe: { subscribeWorkDetail(workId, $0) }
        )
    }

    public func artistDetails(_ artistId: String)
        -> LibraryLiveValue<BridgeArtistDetail?>
    {
        libraryLiveValue(
            callback: ArtistDetailSink.init,
            subscribe: { subscribeArtistDetail(artistId, $0) }
        )
    }

    public func storageProjections(
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        offset: UInt64,
        limit: UInt64
    ) -> LibraryLiveValue<BridgeStorageProjection> {
        libraryLiveValue(
            callback: StorageProjectionSink.init,
            subscribe: {
                subscribeStorageProjection(sort, filter, offset, limit, $0)
            }
        )
    }

    public func releaseDetails(_ releaseId: String)
        -> LibraryLiveValue<BridgeRelease?>
    {
        libraryLiveValue(
            callback: ReleaseDetailSink.init,
            subscribe: { subscribeReleaseDetail(releaseId, $0) }
        )
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
                subscribeAlbumDetail: {
                    handle.subscribeAlbumDetail(albumId: $0, callback: $1)
                },
                subscribeComposerDetail: {
                    handle.subscribeComposerDetail(artistId: $0, callback: $1)
                },
                subscribeWorkDetail: {
                    handle.subscribeWorkDetail(workId: $0, callback: $1)
                },
                artistBrowse: {
                    LibraryBrowseQuery(
                        handle.subscribeArtistBrowse(sortCriteria: $0)
                    )
                },
                subscribeArtistDetail: {
                    handle.subscribeArtistDetail(artistId: $0, callback: $1)
                },
                librarySearch: {
                    LibrarySearch(handle.subscribeLibrarySearch())
                },
                searchArtists: {
                    try await handle.searchArtists(query: $0)
                },
                subscribeStorageProjection: {
                    handle.subscribeStorageProjection(
                        sort: $0,
                        filter: $1,
                        offset: $2,
                        limit: $3,
                        callback: $4
                    )
                },
                subscribeReleaseDetail: {
                    handle.subscribeReleaseDetail(releaseId: $0, callback: $1)
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
        // `getAlbumIndex` and `subscribeStorageProjection` back desktop-only
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
                subscribeAlbumDetail: {
                    handle.subscribeAlbumDetail(albumId: $0, callback: $1)
                },
                subscribeComposerDetail: {
                    handle.subscribeComposerDetail(artistId: $0, callback: $1)
                },
                subscribeWorkDetail: {
                    handle.subscribeWorkDetail(workId: $0, callback: $1)
                },
                artistBrowse: {
                    LibraryBrowseQuery(
                        handle.subscribeArtistBrowse(sortCriteria: $0)
                    )
                },
                subscribeArtistDetail: {
                    handle.subscribeArtistDetail(artistId: $0, callback: $1)
                },
                librarySearch: {
                    LibrarySearch(handle.subscribeLibrarySearch())
                },
                searchArtists: {
                    try await handle.searchArtists(query: $0)
                },
                subscribeReleaseDetail: {
                    handle.subscribeReleaseDetail(releaseId: $0, callback: $1)
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
// swiftlint:enable type_body_length

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
