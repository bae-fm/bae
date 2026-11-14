package fm.bae.app.playback

import androidx.media3.common.MediaItem
import fm.bae.app.data.Library
import fm.bae.app.data.LiveQueryEvent
import fm.bae.app.mainRelease
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import uniffi.bae_bridge.BridgeAlbumDetail

// The browse tree's parents whose children it reads whole, from their detail
// reads: an album's main release's tracks, and a work's parts and releases.

private typealias OptionalLiveQuery<Value> = Flow<LiveQueryEvent<Value?>>
private typealias BrowsePageQuery = Flow<LiveQueryEvent<BrowsePage>>

/** The live page of [parentId], an album or a work, whose children the tree reads whole. */
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

        is BrowseId.Work -> {
            library.workDetails(id.workId).mapBrowse { detail ->
                detail
                    ?.let {
                        it.childWorks.map(nodes::work) +
                            it.releases.map { release ->
                                nodes.album(release.albumId, release.albumTitle, release.cover)
                            }
                    }.orEmpty()
            }
        }

        else -> {
            error("$parentId has no fixed parent query")
        }
    }

private fun <Value> OptionalLiveQuery<Value>.mapBrowse(rows: (Value?) -> List<MediaItem>): BrowsePageQuery =
    map { event ->
        event.mapValue { value -> rows(value).let { BrowsePage(it, it.size) } }
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
    val release = detail?.let { it.mainRelease() ?: return LiveQueryEvent.Error(missingMainRelease(it, reportBug)) }
    val tracks = release?.let { it.tracks.map { track -> nodes.track(it, track) } }.orEmpty()
    return LiveQueryEvent.Value(BrowsePage(tracks, tracks.size))
}
