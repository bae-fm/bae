package fm.bae.app.data

import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion

/**
 * Narrow projection of [AppHandle] for library browse and detail reads — DB
 * queries and on-disk cover lookups. These calls touch the database or
 * filesystem and must run off the main thread. Mirrors the macOS `Library` /
 * `MediaPaths` domain services.
 */
class Library(private val handle: AppHandle) {
    fun albumCount(): ULong = handle.getAlbumCount()

    fun albumPage(
        sortCriteria: List<BridgeSortCriterion>,
        offset: ULong,
        limit: ULong,
    ): List<BridgeAlbum> = handle.getAlbumPage(sortCriteria, offset, limit)

    fun albumDetail(albumId: String): BridgeAlbumDetail = handle.getAlbumDetail(albumId)

    /** Absolute path to an image (a release cover) by id, if the file is on disk. */
    fun imagePathIfExists(imageId: String): String? = handle.imagePathIfExists(imageId)

    /**
     * Search albums and tracks by free-text query. Suspends: the bridge call is
     * async, so callers invoke it directly from a coroutine (no `Dispatchers.IO`
     * wrap, unlike the blocking page/cover reads above).
     */
    suspend fun search(query: String): BridgeSearchResults = handle.searchLibrary(query)
}
