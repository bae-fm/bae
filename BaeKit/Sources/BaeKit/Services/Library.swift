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

private final class LibrarySearchSink: LibrarySearchCallback,
    @unchecked Sendable
{
    private let sink: LibraryLiveValueSink<BridgeSearchResults>
    init(_ sink: LibraryLiveValueSink<BridgeSearchResults>) { self.sink = sink }
    func onValue(value: BridgeSearchResults) { sink.onValue(value) }
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
    public let subscribeAlbumPage:
        @Sendable (
            _ sortCriteria: [BridgeSortCriterion], _ offset: UInt64,
            _ limit: UInt64, _ callback: AlbumPageCallback
        ) -> any LiveSubscriptionProtocol
    public let getAlbumIndex:
        @Sendable (_ sortCriteria: [BridgeSortCriterion], _ albumId: String)
            async throws -> UInt64?
    public let subscribeComposerPage:
        @Sendable (
            _ sortCriteria: [BridgeComposerSortCriterion], _ offset: UInt64,
            _ limit: UInt64, _ callback: ComposerPageCallback
        ) -> any LiveSubscriptionProtocol
    private let subscribeAlbumDetail:
        @Sendable (_ albumId: String, _ callback: AlbumDetailCallback)
            -> any LiveSubscriptionProtocol
    private let subscribeComposerDetail:
        @Sendable (_ artistId: String, _ callback: ComposerDetailCallback)
            -> any LiveSubscriptionProtocol
    private let subscribeWorkDetail:
        @Sendable (_ workId: String, _ callback: WorkDetailCallback)
            -> any LiveSubscriptionProtocol
    public let subscribeArtistPage:
        @Sendable (
            _ sortCriteria: [BridgeArtistSortCriterion], _ offset: UInt64,
            _ limit: UInt64, _ callback: ArtistPageCallback
        ) -> any LiveSubscriptionProtocol
    private let subscribeArtistDetail:
        @Sendable (_ artistId: String, _ callback: ArtistDetailCallback)
            -> any LiveSubscriptionProtocol
    private let subscribeLibrarySearch:
        @Sendable (_ query: String, _ callback: LibrarySearchCallback)
            -> any LiveSubscriptionProtocol
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
        subscribeAlbumPage:
            @escaping @Sendable (
                [BridgeSortCriterion], UInt64, UInt64, AlbumPageCallback
            ) -> any LiveSubscriptionProtocol = { _, _, _, _ in
                fatalError("Library album-page subscription is not installed")
            },
        getAlbumIndex:
            @escaping @Sendable ([BridgeSortCriterion], String) async throws
            -> UInt64? = { _, _ in throw StubError.notImplemented },
        subscribeComposerPage:
            @escaping @Sendable (
                [BridgeComposerSortCriterion], UInt64, UInt64,
                ComposerPageCallback
            ) -> any LiveSubscriptionProtocol = { _, _, _, _ in
                fatalError(
                    "Library composer-page subscription is not installed"
                )
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
        subscribeArtistPage:
            @escaping @Sendable (
                [BridgeArtistSortCriterion], UInt64, UInt64,
                ArtistPageCallback
            ) -> any LiveSubscriptionProtocol = { _, _, _, _ in
                fatalError("Library artist-page subscription is not installed")
            },
        subscribeArtistDetail:
            @escaping @Sendable (String, ArtistDetailCallback)
            -> any LiveSubscriptionProtocol = { _, _ in
                fatalError(
                    "Library artist-detail subscription is not installed"
                )
            },
        subscribeLibrarySearch:
            @escaping @Sendable (String, LibrarySearchCallback)
            -> any LiveSubscriptionProtocol = { _, _ in
                fatalError("Library search subscription is not installed")
            },
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
        self.subscribeAlbumPage = subscribeAlbumPage
        self.getAlbumIndex = getAlbumIndex
        self.subscribeComposerPage = subscribeComposerPage
        self.subscribeAlbumDetail = subscribeAlbumDetail
        self.subscribeComposerDetail = subscribeComposerDetail
        self.subscribeWorkDetail = subscribeWorkDetail
        self.subscribeArtistPage = subscribeArtistPage
        self.subscribeArtistDetail = subscribeArtistDetail
        self.subscribeLibrarySearch = subscribeLibrarySearch
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

    public func searchResults(_ query: String)
        -> LibraryLiveValue<BridgeSearchResults>
    {
        libraryLiveValue(
            callback: LibrarySearchSink.init,
            subscribe: { subscribeLibrarySearch(query, $0) }
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
        // Flat 1:1 argument forwarding from `AppHandleProtocol` to `Library`'s
        // closures; its length tracks the number of Library reads, not
        // logical complexity.
        // swiftlint:disable:next function_body_length
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                subscribeAlbumPage: {
                    handle.subscribeAlbumPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2,
                        callback: $3
                    )
                },
                getAlbumIndex: {
                    try await handle.getAlbumIndex(
                        sortCriteria: $0,
                        albumId: $1
                    )
                },
                subscribeComposerPage: {
                    handle.subscribeComposerPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2,
                        callback: $3
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
                subscribeArtistPage: {
                    handle.subscribeArtistPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2,
                        callback: $3
                    )
                },
                subscribeArtistDetail: {
                    handle.subscribeArtistDetail(artistId: $0, callback: $1)
                },
                subscribeLibrarySearch: {
                    handle.subscribeLibrarySearch(query: $0, callback: $1)
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
                subscribeAlbumPage: {
                    handle.subscribeAlbumPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2,
                        callback: $3
                    )
                },
                subscribeComposerPage: {
                    handle.subscribeComposerPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2,
                        callback: $3
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
                subscribeArtistPage: {
                    handle.subscribeArtistPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2,
                        callback: $3
                    )
                },
                subscribeArtistDetail: {
                    handle.subscribeArtistDetail(artistId: $0, callback: $1)
                },
                subscribeLibrarySearch: {
                    handle.subscribeLibrarySearch(query: $0, callback: $1)
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

/// Error raised by stub closures whose return type can't be defaulted
/// to a trivial value (e.g. compound bridge records). Previews don't
/// reach this in practice — the view tree just renders the placeholder
/// branch when the call throws.
public enum StubError: Error {
    case notImplemented
}
