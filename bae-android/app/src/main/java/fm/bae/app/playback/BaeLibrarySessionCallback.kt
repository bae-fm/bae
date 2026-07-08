package fm.bae.app.playback

import androidx.media3.common.MediaItem
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService.LibraryParams
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionError
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
import com.google.common.util.concurrent.SettableFuture
import fm.bae.app.BaeLogger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

private const val TAG = "bae.BaeLibrarySessionCallback"
private val logger = BaeLogger(TAG)

/**
 * Serves the media-browse tree to controllers (Android Auto, Bluetooth head
 * units, the system) over a [MediaLibrarySession]. Browse reads run through the
 * [LibraryBrowseTree] on [scope]; the callback bridges those `suspend` reads to
 * the [ListenableFuture]s Media3 expects.
 *
 * Playback of a tapped browse item is NOT handled here: [onAddMediaItems]
 * hands the tapped items (which carry the track's browse media id) back to
 * Media3 unchanged, and [BaeCorePlayer.handleSetMediaItems] parses that id and
 * forwards a play-by-id command to core — the same play path the in-app album
 * detail uses. This keeps the player the single place transport is driven from.
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
internal class BaeLibrarySessionCallback(
    private val tree: LibraryBrowseTree,
    private val scope: CoroutineScope,
) : MediaLibrarySession.Callback {
    override fun onGetLibraryRoot(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<MediaItem>> = scope.future { LibraryResult.ofItem(tree.root(), params) }

    override fun onGetChildren(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        parentId: String,
        page: Int,
        pageSize: Int,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
        scope.future {
            val children = tree.children(parentId, page, pageSize)
            val result: LibraryResult<ImmutableList<MediaItem>> =
                if (children == null) {
                    LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                } else {
                    LibraryResult.ofItemList(children, params)
                }
            result
        }

    override fun onGetItem(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        mediaId: String,
    ): ListenableFuture<LibraryResult<MediaItem>> =
        scope.future {
            val item = tree.item(mediaId)
            val result: LibraryResult<MediaItem> =
                if (item == null) {
                    LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                } else {
                    LibraryResult.ofItem(item, null)
                }
            result
        }

    override fun onSearch(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        query: String,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<Void>> =
        scope.future {
            val count = tree.search(query, page = 0, pageSize = Int.MAX_VALUE).size
            session.notifySearchResultChanged(browser, query, count, params)
            LibraryResult.ofVoid()
        }

    override fun onGetSearchResult(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        query: String,
        page: Int,
        pageSize: Int,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
        scope.future { LibraryResult.ofItemList(tree.search(query, page, pageSize), params) }

    /**
     * Resolve the media items a controller asked to play. Two shapes arrive: an
     * item tapped in the browse tree already carries a track browse id, which is
     * all the player needs (it plays by id through core); a spoken "play X"
     * arrives as an item with only a search query, which we resolve to a
     * playable track here. Handing the items back (rather than the base
     * callback's default failed future) is what lets browse and voice playback
     * reach the player.
     */
    override fun onAddMediaItems(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
        mediaItems: List<MediaItem>,
    ): ListenableFuture<List<MediaItem>> = scope.future { mediaItems.map { resolveForPlayback(it) } }

    private suspend fun resolveForPlayback(item: MediaItem): MediaItem {
        // A tapped browse node already names what to play.
        if (BrowseId.parse(item.mediaId) != null) return item
        // A spoken query ("play X") carries no media id — resolve it to a track.
        val query = item.requestMetadata.searchQuery?.takeUnless { it.isBlank() }
        val track = query?.let { tree.searchTopPlayable(it) }
        if (query != null && track == null) {
            logger.warning("no playable result for spoken query \"$query\"")
        }
        return track?.let {
            MediaItem
                .Builder()
                .setMediaId(it.mediaId)
                .setMediaMetadata(item.mediaMetadata)
                .build()
        } ?: item
    }
}

/**
 * Run a `suspend` block on this scope and expose its result as a
 * [ListenableFuture]. Cancelling the future cancels the block. Guava's
 * [SettableFuture] and Media3 (which pulls Guava in) are already on the
 * classpath, so this bridges coroutines to Media3's future-returning callbacks
 * without a new coroutines-guava dependency whose version would have to track
 * the transitively-resolved coroutines-core.
 */
internal fun <T> CoroutineScope.future(block: suspend () -> T): ListenableFuture<T> {
    val future = SettableFuture.create<T>()
    val job =
        launch {
            try {
                future.set(block())
            } catch (e: CancellationException) {
                future.setException(e)
                throw e
            } catch (e: Throwable) {
                future.setException(e)
            }
        }
    future.addListener({ if (future.isCancelled) job.cancel() }, MoreExecutors.directExecutor())
    return future
}
