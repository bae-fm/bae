package fm.bae.app.data

import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeBrowseTrack
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeTopPlayable
import uniffi.bae_bridge.BridgeWorkDetail

/**
 * The library reads a request needs once (Android Auto's browse and search): each is one plain
 * read of the database through the bridge, never a live query taken for its first value.
 */
internal class LibraryOneOffReads(
    private val handle: AppHandleInterface,
) {
    /**
     * The albums a search for [query] finds from [offset] on, at most [limit], read once: the
     * database picks the window, so no page is cut from a longer list here.
     */
    internal suspend fun readSearchAlbumPage(
        query: String,
        offset: Long,
        limit: Int,
    ): List<BridgeAlbumSearchResult> = handle.readSearchAlbumPage(query, offset.toULong(), limit.toULong())

    /** One track of a release as a browse item, read once; null when the release has no such track. */
    internal suspend fun readReleaseTrack(
        releaseId: String,
        trackId: String,
    ): BridgeBrowseTrack? = handle.readReleaseTrack(releaseId, trackId)

    /** The track a spoken "play" of [query] plays, read once, or null when the search finds none. */
    internal suspend fun readSearchTopPlayable(query: String): BridgeTopPlayable? = handle.readSearchTopPlayable(query)

    /** One album's detail, read once; null when the album doesn't exist. */
    internal suspend fun readAlbumDetail(id: String): BridgeAlbumDetail? = handle.readAlbumDetail(id)

    /** One composer's page above its rows, read once; null when the composer doesn't exist. */
    internal suspend fun readComposerDetail(id: String): BridgeComposerDetail? = handle.readComposerDetail(id)

    /** One work's detail, read once; null when the work doesn't exist. */
    internal suspend fun readWorkDetail(id: String): BridgeWorkDetail? = handle.readWorkDetail(id)
}
