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
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

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
    private val tree: LibraryBrowseTree<MediaSession.ControllerInfo>,
    private val scope: CoroutineScope,
    /** What the controller says when a spoken request names nothing in the library, for that request. */
    private val notFoundMessage: (query: String) -> String,
) : MediaLibrarySession.Callback {
    override fun onGetLibraryRoot(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<MediaItem>> = scope.future { LibraryResult.ofItem(tree.root, params) }

    override fun onGetChildren(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        parentId: String,
        page: Int,
        pageSize: Int,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> {
        val children = getChildren(browser, parentId, page, pageSize, params)
        return children
    }

    internal fun getChildren(
        browser: MediaSession.ControllerInfo,
        parentId: String,
        page: Int,
        pageSize: Int,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
        scope.future {
            queryResult {
                if (!tree.retainImplicitParent(browser, parentId)) {
                    return@queryResult LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                }
                val children = tree.children(parentId, page, pageSize)
                if (children == null) {
                    LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                } else {
                    LibraryResult.ofItemList(children, params)
                }
            }
        }

    override fun onSubscribe(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        parentId: String,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<Void>> =
        scope.future {
            queryResult {
                if (tree.subscribeParent(browser, parentId)) {
                    LibraryResult.ofVoid()
                } else {
                    LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                }
            }
        }

    override fun onUnsubscribe(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        parentId: String,
    ): ListenableFuture<LibraryResult<Void>> {
        tree.unsubscribeParent(browser, parentId)
        return SettableFuture.create<LibraryResult<Void>>().also { it.set(LibraryResult.ofVoid()) }
    }

    override fun onGetItem(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        mediaId: String,
    ): ListenableFuture<LibraryResult<MediaItem>> =
        scope.future {
            queryResult {
                val item = tree.item(mediaId)
                if (item == null) {
                    LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                } else {
                    LibraryResult.ofItem(item, null)
                }
            }
        }

    override fun onSearch(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        query: String,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<Void>> =
        scope.future {
            queryResult {
                tree.subscribeSearch(browser, query) { count ->
                    session.notifySearchResultChanged(browser, query, count, params)
                }
                LibraryResult.ofVoid()
            }
        }

    override fun onGetSearchResult(
        session: MediaLibrarySession,
        browser: MediaSession.ControllerInfo,
        query: String,
        page: Int,
        pageSize: Int,
        params: LibraryParams?,
    ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
        scope.future {
            queryResult {
                val results = tree.search(query, page, pageSize)
                if (results == null) {
                    LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                } else {
                    LibraryResult.ofItemList(results, params)
                }
            }
        }

    /**
     * Resolve the media items a controller asked to play. Two shapes arrive: an
     * item tapped in the browse tree already carries a track browse id, which is
     * all the player needs (it plays by id through core); a spoken "play X"
     * arrives as an item with only a search query, which we resolve to a
     * playable track here. Handing the items back (rather than the base
     * callback's default failed future) is what lets browse and voice playback
     * reach the player. A request that names nothing to play fails, and a
     * spoken one that matches nothing tells the controller it wasn't found, so
     * the Assistant says so instead of a request that went nowhere succeeding.
     */
    override fun onAddMediaItems(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
        mediaItems: List<MediaItem>,
    ): ListenableFuture<List<MediaItem>> =
        scope.future {
            when (val request = resolve(mediaItems)) {
                is PlayRequest.Tracks -> {
                    request.items
                }

                is PlayRequest.NotFound -> {
                    logger.warning("no playable result for spoken query \"${request.query}\"")
                    session.sendError(
                        controller,
                        SessionError(SessionError.ERROR_BAD_VALUE, notFoundMessage(request.query)),
                    )
                    throw NothingToPlay("no playable result for spoken query \"${request.query}\"")
                }

                is PlayRequest.NotPlayable -> {
                    logger.warning("a controller asked to play ${request.mediaId}, which isn't a track")
                    throw NothingToPlay("a controller asked to play ${request.mediaId}, which isn't a track")
                }
            }
        }

    override fun onDisconnected(
        session: MediaSession,
        controller: MediaSession.ControllerInfo,
    ) {
        tree.disconnect(controller)
    }

    /**
     * [block]'s result, or the closest error a controller understands, with
     * its cause logged: the library isn't available while it is closed; a
     * request replaced by a newer one is cancelled; a read that failed is an
     * I/O error; and a broken guarantee inside the app stays unknown.
     */
    private suspend fun <T : Any> queryResult(block: suspend () -> LibraryResult<T>): LibraryResult<T> =
        try {
            block()
        } catch (e: BrowseUnavailable.LibraryClosed) {
            logger.warning("a controller asked the browse tree after its library closed", e)
            LibraryResult.ofError(SessionError.ERROR_INVALID_STATE)
        } catch (e: BrowseUnavailable.Superseded) {
            logger.debug("a controller's browse request was replaced by a newer one", e)
            LibraryResult.ofError(SessionError.INFO_CANCELLED)
        } catch (e: BridgeException) {
            logger.error("reading the library for a controller failed", e)
            LibraryResult.ofError(browseErrorCode(e))
        }

    /**
     * What [mediaItems] ask to play: each a track tapped in the browse tree, or
     * a spoken query resolved to the library's top playable track; the first
     * that names no track decides the whole request.
     */
    internal suspend fun resolve(mediaItems: List<MediaItem>): PlayRequest {
        val tracks = mutableListOf<MediaItem>()
        for (item in mediaItems) {
            when (val one = resolve(item)) {
                is PlayRequest.Tracks -> tracks += one.items
                is PlayRequest.NotFound, is PlayRequest.NotPlayable -> return one
            }
        }
        return PlayRequest.Tracks(tracks)
    }

    private suspend fun resolve(item: MediaItem): PlayRequest {
        val query = item.requestMetadata.searchQuery?.takeUnless { it.isBlank() }
        return when {
            BrowseId.parse(item.mediaId) is BrowseId.Track -> {
                PlayRequest.Tracks(listOf(item))
            }

            // A spoken query ("play X") carries no media id — resolve it to a track.
            query != null -> {
                tree.searchTopPlayable(query)?.let { track ->
                    PlayRequest.Tracks(
                        listOf(
                            MediaItem
                                .Builder()
                                .setMediaId(track.mediaId)
                                .setMediaMetadata(item.mediaMetadata)
                                .build(),
                        ),
                    )
                } ?: PlayRequest.NotFound(query)
            }

            else -> {
                PlayRequest.NotPlayable(item.mediaId)
            }
        }
    }
}

/** The controller's error code for a failed bridge read, by what core says failed. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
private fun browseErrorCode(error: BridgeException): Int =
    when (error) {
        is BridgeException.Cancelled -> {
            SessionError.INFO_CANCELLED
        }

        is BridgeException.NotFound -> {
            SessionError.ERROR_BAD_VALUE
        }

        is BridgeException.Diagnostic -> {
            when (error.category) {
                // Core's own guarantee broke; nothing a controller can name.
                BridgeErrorCategory.Internal -> SessionError.ERROR_UNKNOWN

                else -> SessionError.ERROR_IO
            }
        }
    }

/** What a controller's request to play resolves to. */
internal sealed interface PlayRequest {
    /** Items each naming a track by its browse id, for the player. */
    data class Tracks(
        val items: List<MediaItem>,
    ) : PlayRequest

    /** A spoken [query] that matched nothing playable in the library. */
    data class NotFound(
        val query: String,
    ) : PlayRequest

    /** An item that is neither a track nor a spoken query, such as an album node. */
    data class NotPlayable(
        val mediaId: String,
    ) : PlayRequest
}

/** A request to play that names nothing to play; failing it is how Media3 hears it didn't play. */
internal class NothingToPlay(
    message: String,
) : Exception(message)

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
