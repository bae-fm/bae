package fm.bae.app.playback

import fm.bae.app.data.Library
import fm.bae.app.data.LiveQueryEvent
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import uniffi.bae_bridge.BridgeAlbumDetail

// The browse tree's parent whose children it reads whole, from its detail
// read: an album's main release's tracks.

/** The live page of [parentId], an album, whose children the tree reads whole. */
internal fun parentFlow(
    library: Library,
    nodes: BrowseNodeFactory,
    reportBug: (operation: String, error: Throwable) -> Unit,
    parentId: String,
): Flow<LiveQueryEvent<BrowsePage>> =
    when (val id = checkNotNull(BrowseId.parse(parentId))) {
        is BrowseId.Album -> {
            library.albumDetails(id.albumId).map { event ->
                when (event) {
                    is LiveQueryEvent.Error -> event
                    is LiveQueryEvent.Value -> albumPage(event.value, nodes, reportBug)
                }
            }
        }

        else -> {
            error("$parentId has no fixed parent query")
        }
    }

/**
 * An album's children: its main release's tracks, or none once the album is
 * gone. An album whose main release isn't among its releases is a bug,
 * reported, and an error for the controller instead of another release's
 * tracks.
 */
private fun albumPage(
    detail: BridgeAlbumDetail?,
    nodes: BrowseNodeFactory,
    reportBug: (operation: String, error: Throwable) -> Unit,
): LiveQueryEvent<BrowsePage> {
    val release =
        detail?.let { it.releases.firstOrNull() ?: return LiveQueryEvent.Error(missingMainRelease(it, reportBug)) }
    val tracks = release?.let { it.tracks.map { track -> nodes.track(it, track) } }.orEmpty()
    return LiveQueryEvent.Value(BrowsePage(tracks, tracks.size))
}
