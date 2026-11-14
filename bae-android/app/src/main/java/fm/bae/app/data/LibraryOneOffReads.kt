package fm.bae.app.data

import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeTopPlayable
import uniffi.bae_bridge.BridgeWorkDetail

/**
 * The library reads a request needs once (Android Auto's browse and search): each is one plain
 * read of the database through the bridge, never a live query taken for its first value.
 */
internal class LibraryOneOffReads(
    private val handle: AppHandleInterface,
) {
    /** What a search for [query] finds, read once from the database: no subscription is opened. */
    internal suspend fun readSearch(query: String): BridgeSearchResults = handle.readLibrarySearch(query)

    /** The track a spoken "play" of [query] plays, read once, or null when the search finds none. */
    internal suspend fun readSearchTopPlayable(query: String): BridgeTopPlayable? = handle.readSearchTopPlayable(query)

    /** One album's detail, read once; null when the album doesn't exist. */
    internal suspend fun readAlbumDetail(id: String): BridgeAlbumDetail? = handle.readAlbumDetail(id)

    /** One release's detail, read once; null when the release doesn't exist. */
    internal suspend fun readReleaseDetail(id: String): BridgeRelease? = handle.readReleaseDetail(id)

    /** One composer's page above its rows, read once; null when the composer doesn't exist. */
    internal suspend fun readComposerDetail(id: String): BridgeComposerDetail? = handle.readComposerDetail(id)

    /** One work's detail, read once; null when the work doesn't exist. */
    internal suspend fun readWorkDetail(id: String): BridgeWorkDetail? = handle.readWorkDetail(id)
}
