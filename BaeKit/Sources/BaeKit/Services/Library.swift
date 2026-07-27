import Foundation

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
    public let getAlbumCount: @Sendable () async throws -> UInt64
    public let getAlbumPage:
        @Sendable (
            _ sortCriteria: [BridgeSortCriterion], _ offset: UInt64,
            _ limit: UInt64
        ) async throws -> [BridgeAlbum]
    public let getAlbumIndex:
        @Sendable (_ sortCriteria: [BridgeSortCriterion], _ albumId: String)
            async throws -> UInt64?
    public let getComposerCount: @Sendable () async throws -> UInt64
    public let getComposerPage:
        @Sendable (
            _ sortCriteria: [BridgeComposerSortCriterion], _ offset: UInt64,
            _ limit: UInt64
        ) async throws -> [BridgeComposerSummary]
    public let getComposerDetail:
        @Sendable (_ artistId: String) async throws -> BridgeComposerDetail?
    public let getWorkDetail:
        @Sendable (_ workId: String) async throws -> BridgeWorkDetail?
    public let getArtistCount: @Sendable () async throws -> UInt64
    public let getArtistPage:
        @Sendable (
            _ sortCriteria: [BridgeArtistSortCriterion], _ offset: UInt64,
            _ limit: UInt64
        ) async throws -> [BridgeArtistSummary]
    public let getArtistDetail:
        @Sendable (_ artistId: String) async throws -> BridgeArtistDetail?
    public let searchLibrary:
        @Sendable (_ query: String) async throws -> BridgeSearchResults
    public let storageCount:
        @Sendable (_ filter: BridgeStorageFilter) async throws -> UInt64
    public let storageTotalSize:
        @Sendable (_ filter: BridgeStorageFilter) async throws -> UInt64
    public let storagePage:
        @Sendable (
            _ sort: BridgeStorageSort, _ filter: BridgeStorageFilter,
            _ offset: UInt64, _ limit: UInt64
        ) async throws -> BridgeStoragePage
    public let findReleaseDetail:
        @Sendable (_ releaseId: String) async throws -> BridgeRelease?
    /// Pick a release for an import candidate: the confirm pane's display
    /// detail, the editor seed masked for the claim, the claim itself, and the
    /// track slots the pick produces. `candidateKey` is what lets bae-core read
    /// the evidence that identified the candidate — which is what the claim
    /// defaults from — and find the folder whose audio the slots map.
    public let prefetchRelease:
        @Sendable (
            _ candidateKey: String, _ releaseId: String,
            _ source: BridgeMetadataSource
        ) async throws -> BridgeReleasePrefetch
    public let resolveToTrackIds:
        @Sendable (_ ids: [String]) async throws -> [String]
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. The write's config
    /// invalidation re-renders the page through `ConfigStore`.
    public let setLibraryFullWidth: @Sendable (_ enabled: Bool) throws -> Void

    public init(
        getAlbumCount: @escaping @Sendable () async throws -> UInt64 = {
            throw StubError.notImplemented
        },
        getAlbumPage:
            @escaping @Sendable ([BridgeSortCriterion], UInt64, UInt64)
            async throws
            -> [BridgeAlbum] = { _, _, _ in
                throw StubError.notImplemented
            },
        getAlbumIndex:
            @escaping @Sendable ([BridgeSortCriterion], String) async throws
            -> UInt64? = { _, _ in throw StubError.notImplemented },
        getComposerCount: @escaping @Sendable () async throws -> UInt64 = {
            throw StubError.notImplemented
        },
        getComposerPage:
            @escaping @Sendable ([BridgeComposerSortCriterion], UInt64, UInt64)
            async throws
            -> [BridgeComposerSummary] = { _, _, _ in
                throw StubError.notImplemented
            },
        getComposerDetail:
            @escaping @Sendable (String) async throws -> BridgeComposerDetail? =
            {
                _ in throw StubError.notImplemented
            },
        getWorkDetail:
            @escaping @Sendable (String) async throws -> BridgeWorkDetail? = {
                _ in
                throw StubError.notImplemented
            },
        getArtistCount: @escaping @Sendable () async throws -> UInt64 = {
            throw StubError.notImplemented
        },
        getArtistPage:
            @escaping @Sendable ([BridgeArtistSortCriterion], UInt64, UInt64)
            async throws
            -> [BridgeArtistSummary] = { _, _, _ in
                throw StubError.notImplemented
            },
        getArtistDetail:
            @escaping @Sendable (String) async throws -> BridgeArtistDetail? =
            {
                _ in throw StubError.notImplemented
            },
        searchLibrary:
            @escaping @Sendable (String) async throws -> BridgeSearchResults = {
                _ in
                throw StubError.notImplemented
            },
        storageCount:
            @escaping @Sendable (BridgeStorageFilter) async throws -> UInt64 = {
                _ in
                throw StubError.notImplemented
            },
        storageTotalSize:
            @escaping @Sendable (BridgeStorageFilter) async throws -> UInt64 = {
                _ in
                throw StubError.notImplemented
            },
        storagePage:
            @escaping @Sendable (
                BridgeStorageSort, BridgeStorageFilter, UInt64, UInt64
            ) async throws -> BridgeStoragePage = { _, _, _, _ in
                throw StubError.notImplemented
            },
        findReleaseDetail:
            @escaping @Sendable (String) async throws -> BridgeRelease? = { _ in
                throw StubError.notImplemented
            },
        prefetchRelease:
            @escaping @Sendable (String, String, BridgeMetadataSource)
            async throws -> BridgeReleasePrefetch = { _, _, _ in
                throw StubError.notImplemented
            },
        resolveToTrackIds:
            @escaping @Sendable ([String]) async throws -> [String] = {
                _ in throw StubError.notImplemented
            },
        setLibraryFullWidth: @escaping @Sendable (Bool) throws -> Void = {
            _ in throw StubError.notImplemented
        }
    ) {
        self.getAlbumCount = getAlbumCount
        self.getAlbumPage = getAlbumPage
        self.getAlbumIndex = getAlbumIndex
        self.getComposerCount = getComposerCount
        self.getComposerPage = getComposerPage
        self.getComposerDetail = getComposerDetail
        self.getWorkDetail = getWorkDetail
        self.getArtistCount = getArtistCount
        self.getArtistPage = getArtistPage
        self.getArtistDetail = getArtistDetail
        self.searchLibrary = searchLibrary
        self.storageCount = storageCount
        self.storageTotalSize = storageTotalSize
        self.storagePage = storagePage
        self.findReleaseDetail = findReleaseDetail
        self.prefetchRelease = prefetchRelease
        self.resolveToTrackIds = resolveToTrackIds
        self.setLibraryFullWidth = setLibraryFullWidth
    }

    // `prefetchRelease` backs the desktop import/metadata-prefetch flow and
    // isn't exported on iOS (the import service is desktop-only). This
    // `handle`-wiring convenience initializer references it, so it's
    // desktop-only; the iOS `AppService` builds `Library` via the designated
    // initializer with just the iOS-available closures.
    #if !os(iOS)
        // Flat 1:1 argument forwarding from `AppHandleProtocol` to `Library`'s
        // closures; its length tracks the number of Library reads, not
        // logical complexity.
        // swiftlint:disable:next function_body_length
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                getAlbumCount: { try await handle.getAlbumCount() },
                getAlbumPage: {
                    try await handle.getAlbumPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2
                    )
                },
                getAlbumIndex: {
                    try await handle.getAlbumIndex(
                        sortCriteria: $0,
                        albumId: $1
                    )
                },
                getComposerCount: { try await handle.getComposerCount() },
                getComposerPage: {
                    try await handle.getComposerPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2
                    )
                },
                getComposerDetail: {
                    try await handle.getComposerDetail(artistId: $0)
                },
                getWorkDetail: { try await handle.getWorkDetail(workId: $0) },
                getArtistCount: { try await handle.getArtistCount() },
                getArtistPage: {
                    try await handle.getArtistPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2
                    )
                },
                getArtistDetail: {
                    try await handle.getArtistDetail(artistId: $0)
                },
                searchLibrary: { try await handle.searchLibrary(query: $0) },
                storageCount: { try await handle.storageCount(filter: $0) },
                storageTotalSize: {
                    try await handle.storageTotalSize(filter: $0)
                },
                storagePage: {
                    try await handle.storagePage(
                        sort: $0,
                        filter: $1,
                        offset: $2,
                        limit: $3
                    )
                },
                findReleaseDetail: {
                    try await handle.findReleaseDetail(releaseId: $0)
                },
                prefetchRelease: {
                    try await handle.prefetchRelease(
                        candidateKey: $0,
                        releaseId: $1,
                        source: $2
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
        // iOS has no desktop import/metadata-prefetch flow, so `prefetchRelease`
        // is absent from its bindings; `getAlbumIndex`, `storageCount`,
        // `storageTotalSize`, and `storagePage` back desktop-only surfaces
        // (album-index scrolling, the Storage Manager) and go unused here.
        // This wires only the reads iOS actually makes; the rest keep their
        // throwing stub defaults.
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                getAlbumCount: { try await handle.getAlbumCount() },
                getAlbumPage: {
                    try await handle.getAlbumPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2
                    )
                },
                getComposerCount: { try await handle.getComposerCount() },
                getComposerPage: {
                    try await handle.getComposerPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2
                    )
                },
                getComposerDetail: {
                    try await handle.getComposerDetail(artistId: $0)
                },
                getWorkDetail: { try await handle.getWorkDetail(workId: $0) },
                getArtistCount: { try await handle.getArtistCount() },
                getArtistPage: {
                    try await handle.getArtistPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2
                    )
                },
                getArtistDetail: {
                    try await handle.getArtistDetail(artistId: $0)
                },
                searchLibrary: { try await handle.searchLibrary(query: $0) },
                findReleaseDetail: {
                    try await handle.findReleaseDetail(releaseId: $0)
                },
                resolveToTrackIds: {
                    try await handle.resolveToTrackIds(ids: $0)
                }
            )
        }
    #endif

    #if DEBUG
        // periphery:ignore
        public static let stub = Library()
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
