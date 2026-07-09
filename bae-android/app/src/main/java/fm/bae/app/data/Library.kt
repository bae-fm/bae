package fm.bae.app.data

import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeWorkDetail

/**
 * Narrow projection of [AppHandle] for library browse and detail reads — DB
 * queries and cover-image reads. The page/detail calls suspend across the
 * bridge; callers invoke them from their existing coroutine.
 * Mirrors the macOS `Library` / `MediaPaths` domain services.
 */
class Library(
    private val handle: AppHandle,
) {
    suspend fun albumCount(): ULong = handle.getAlbumCount()

    suspend fun albumPage(
        sortCriteria: List<BridgeSortCriterion>,
        offset: ULong,
        limit: ULong,
    ): List<BridgeAlbum> = handle.getAlbumPage(sortCriteria, offset, limit)

    suspend fun albumDetail(albumId: String): BridgeAlbumDetail = handle.getAlbumDetail(albumId)

    /** Fat release detail (tracks grouped by side) by release id, or null when
     *  no such release exists. */
    suspend fun releaseDetail(releaseId: String): BridgeRelease? = handle.findReleaseDetail(releaseId)

    suspend fun composerCount(): ULong = handle.getComposerCount()

    suspend fun composerPage(
        sortCriterion: BridgeComposerSortCriterion,
        offset: ULong,
        limit: ULong,
    ): List<BridgeComposerSummary> = handle.getComposerPage(listOf(sortCriterion), offset, limit)

    suspend fun composerDetail(artistId: String): BridgeComposerDetail? = handle.getComposerDetail(artistId)

    suspend fun artistCount(): ULong = handle.getArtistCount()

    suspend fun artistPage(
        sortCriterion: BridgeArtistSortCriterion,
        offset: ULong,
        limit: ULong,
    ): List<BridgeArtistSummary> = handle.getArtistPage(listOf(sortCriterion), offset, limit)

    suspend fun artistDetail(artistId: String): BridgeArtistDetail? = handle.getArtistDetail(artistId)

    suspend fun workDetail(workId: String): BridgeWorkDetail? = handle.getWorkDetail(workId)

    /** Bytes of a release cover by release id, read through coven's
     *  locality-aware store, or null when no such cover exists. */
    suspend fun imageBytes(imageId: String): ByteArray? = handle.fetchCoverImageBytes(imageId)

    /**
     * Search albums and tracks by free-text query. Suspends: the bridge call is
     * async, so callers invoke it directly from a coroutine.
     */
    suspend fun search(query: String): BridgeSearchResults = handle.searchLibrary(query)
}
