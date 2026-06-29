import Foundation

/// Library reads — album/release lookups, pagination, search,
/// storage-summary listing, prefetching release detail, resolving
/// queue-input ids to flat track-id lists. The read side of bae-core's
/// catalog, narrow to what view layers ask for.
final class Library: Sendable, Observable {
    let getAlbumCount: @Sendable () throws -> UInt64
    let getAlbumPage:
        @Sendable (
            _ sortCriteria: [BridgeSortCriterion], _ offset: UInt64,
            _ limit: UInt64
        ) throws -> [BridgeAlbum]
    let getComposerCount: @Sendable () throws -> UInt64
    let getComposerPage:
        @Sendable (
            _ sortCriterion: BridgeComposerSortCriterion, _ offset: UInt64,
            _ limit: UInt64
        ) throws -> [BridgeComposerSummary]
    let getComposerDetail:
        @Sendable (_ artistId: String) throws -> BridgeComposerDetail?
    let getWorkDetail: @Sendable (_ workId: String) throws -> BridgeWorkDetail?
    let searchLibrary:
        @Sendable (_ query: String) async throws -> BridgeSearchResults
    let storageCount: @Sendable (_ filter: BridgeStorageFilter) throws -> UInt64
    let storagePage:
        @Sendable (
            _ sort: BridgeStorageSort, _ filter: BridgeStorageFilter,
            _ offset: UInt64, _ limit: UInt64
        ) throws -> BridgeStoragePage
    let findReleaseDetail:
        @Sendable (_ releaseId: String) throws -> BridgeRelease?
    let prefetchRelease:
        @Sendable (
            _ releaseId: String, _ source: BridgeMetadataSource,
            _ localTrackCount: UInt32?
        ) async throws -> BridgeReleaseDetail
    let resolveToTrackIds: @Sendable (_ ids: [String]) throws -> [String]

    init(
        getAlbumCount: @escaping @Sendable () throws -> UInt64 = { 0 },
        getAlbumPage:
            @escaping @Sendable ([BridgeSortCriterion], UInt64, UInt64) throws
            -> [BridgeAlbum] = { _, _, _ in [] },
        getComposerCount: @escaping @Sendable () throws -> UInt64 = {
            throw StubError.notImplemented
        },
        getComposerPage:
            @escaping @Sendable (BridgeComposerSortCriterion, UInt64, UInt64)
            throws
            -> [BridgeComposerSummary] = { _, _, _ in
                throw StubError.notImplemented
            },
        getComposerDetail:
            @escaping @Sendable (String) throws -> BridgeComposerDetail? = {
                _ in throw StubError.notImplemented
            },
        getWorkDetail:
            @escaping @Sendable (String) throws -> BridgeWorkDetail? = { _ in
                throw StubError.notImplemented
            },
        searchLibrary:
            @escaping @Sendable (String) async throws -> BridgeSearchResults = {
                _ in
                throw StubError.notImplemented
            },
        storageCount:
            @escaping @Sendable (BridgeStorageFilter) throws -> UInt64 = { _ in
                0
            },
        storagePage:
            @escaping @Sendable (
                BridgeStorageSort, BridgeStorageFilter, UInt64, UInt64
            ) throws -> BridgeStoragePage = { _, _, _, _ in
                BridgeStoragePage(rows: [], totalCount: 0)
            },
        findReleaseDetail:
            @escaping @Sendable (String) throws -> BridgeRelease? = { _ in nil
            },
        prefetchRelease:
            @escaping @Sendable (String, BridgeMetadataSource, UInt32?)
            async throws -> BridgeReleaseDetail = { _, _, _ in
                throw StubError.notImplemented
            },
        resolveToTrackIds: @escaping @Sendable ([String]) throws -> [String] = {
            $0
        }
    ) {
        self.getAlbumCount = getAlbumCount
        self.getAlbumPage = getAlbumPage
        self.getComposerCount = getComposerCount
        self.getComposerPage = getComposerPage
        self.getComposerDetail = getComposerDetail
        self.getWorkDetail = getWorkDetail
        self.searchLibrary = searchLibrary
        self.storageCount = storageCount
        self.storagePage = storagePage
        self.findReleaseDetail = findReleaseDetail
        self.prefetchRelease = prefetchRelease
        self.resolveToTrackIds = resolveToTrackIds
    }

    // `prefetchRelease` backs the desktop import/metadata-prefetch flow and
    // isn't exported on iOS (the import service is desktop-only). This
    // `handle`-wiring convenience initializer references it, so it's
    // desktop-only; the iOS `AppService` builds `Library` via the designated
    // initializer with just the iOS-available closures.
    #if !os(iOS)
        convenience init(handle: any AppHandleProtocol) {
            self.init(
                getAlbumCount: { try handle.getAlbumCount() },
                getAlbumPage: {
                    try handle.getAlbumPage(
                        sortCriteria: $0,
                        offset: $1,
                        limit: $2
                    )
                },
                getComposerCount: { try handle.getComposerCount() },
                getComposerPage: {
                    try handle.getComposerPage(
                        sortCriterion: $0,
                        offset: $1,
                        limit: $2
                    )
                },
                getComposerDetail: {
                    try handle.getComposerDetail(artistId: $0)
                },
                getWorkDetail: { try handle.getWorkDetail(workId: $0) },
                searchLibrary: { try await handle.searchLibrary(query: $0) },
                storageCount: { try handle.storageCount(filter: $0) },
                storagePage: {
                    try handle.storagePage(
                        sort: $0,
                        filter: $1,
                        offset: $2,
                        limit: $3
                    )
                },
                findReleaseDetail: {
                    try handle.findReleaseDetail(releaseId: $0)
                },
                prefetchRelease: {
                    try await handle.prefetchRelease(
                        releaseId: $0,
                        source: $1,
                        localTrackCount: $2
                    )
                },
                resolveToTrackIds: { try handle.resolveToTrackIds(ids: $0) }
            )
        }
    #endif

    // periphery:ignore
    static let stub = Library()
}

/// Error raised by stub closures whose return type can't be defaulted
/// to a trivial value (e.g. compound bridge records). Previews don't
/// reach this in practice — the view tree just renders the placeholder
/// branch when the call throws.
enum StubError: Error {
    case notImplemented
}
