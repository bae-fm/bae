package fm.bae.app.data

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AlbumBrowseSubscription
import uniffi.bae_bridge.AlbumDetailCallback
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.ArtistBrowseSubscription
import uniffi.bae_bridge.ArtistDetailCallback
import uniffi.bae_bridge.BridgeAlbumBrowseSnapshot
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeArtistBrowseSnapshot
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeComposerBrowseSnapshot
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeLibrarySearchSnapshot
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.ComposerBrowseSubscription
import uniffi.bae_bridge.ComposerDetailCallback
import uniffi.bae_bridge.LibrarySearchSubscription
import uniffi.bae_bridge.ReleaseDetailCallback
import uniffi.bae_bridge.WorkDetailCallback

internal sealed interface LiveQueryEvent<out Value> {
    data class Value<Value>(
        val value: Value,
    ) : LiveQueryEvent<Value>

    data class Error(
        val error: BridgeException,
    ) : LiveQueryEvent<Nothing>

    fun <Mapped> mapValue(transform: (Value) -> Mapped): LiveQueryEvent<Mapped> =
        when (this) {
            is LiveQueryEvent.Value -> LiveQueryEvent.Value(transform(value))
            is LiveQueryEvent.Error -> this
        }
}

/**
 * Narrow projection of [AppHandle] for library browse and detail live queries.
 * Each flow stays subscribed after an error and can deliver later values; the
 * error is an event rather than flow termination. Image bytes are not here —
 * every image in the app resolves through [fm.bae.app.data.ImageStore], which
 * owns their caching too.
 * Mirrors the macOS `Library` domain service.
 */
class Library(
    private val handle: AppHandle,
) {
    internal fun albumBrowse(sortCriteria: List<BridgeSortCriterion>): AlbumBrowseQuery =
        BridgeAlbumBrowseQuery(handle.subscribeAlbumBrowse(sortCriteria))

    internal fun albumDetails(albumId: String): Flow<LiveQueryEvent<BridgeAlbumDetail?>> =
        callbackFlow {
            val subscription =
                handle.subscribeAlbumDetail(
                    albumId,
                    object : AlbumDetailCallback {
                        override fun onValue(value: BridgeAlbumDetail?) {
                            trySend(LiveQueryEvent.Value(value))
                        }

                        override fun onError(error: BridgeException) {
                            trySend(LiveQueryEvent.Error(error))
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    internal fun composerBrowse(sortCriterion: BridgeComposerSortCriterion): ComposerBrowseQuery =
        BridgeComposerBrowseQuery(handle.subscribeComposerBrowse(listOf(sortCriterion)))

    internal fun artistBrowse(sortCriterion: BridgeArtistSortCriterion): ArtistBrowseQuery =
        BridgeArtistBrowseQuery(handle.subscribeArtistBrowse(listOf(sortCriterion)))

    internal fun composerDetails(artistId: String): Flow<LiveQueryEvent<BridgeComposerDetail?>> =
        callbackFlow {
            val subscription =
                handle.subscribeComposerDetail(
                    artistId,
                    object : ComposerDetailCallback {
                        override fun onValue(value: BridgeComposerDetail?) {
                            trySend(LiveQueryEvent.Value(value))
                        }

                        override fun onError(error: BridgeException) {
                            trySend(LiveQueryEvent.Error(error))
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    internal fun artistDetails(artistId: String): Flow<LiveQueryEvent<BridgeArtistDetail?>> =
        callbackFlow {
            val subscription =
                handle.subscribeArtistDetail(
                    artistId,
                    object : ArtistDetailCallback {
                        override fun onValue(value: BridgeArtistDetail?) {
                            trySend(LiveQueryEvent.Value(value))
                        }

                        override fun onError(error: BridgeException) {
                            trySend(LiveQueryEvent.Error(error))
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    internal fun workDetails(workId: String): Flow<LiveQueryEvent<BridgeWorkDetail?>> =
        callbackFlow {
            val subscription =
                handle.subscribeWorkDetail(
                    workId,
                    object : WorkDetailCallback {
                        override fun onValue(value: BridgeWorkDetail?) {
                            trySend(LiveQueryEvent.Value(value))
                        }

                        override fun onError(error: BridgeException) {
                            trySend(LiveQueryEvent.Error(error))
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    internal fun releaseDetails(releaseId: String): Flow<LiveQueryEvent<BridgeRelease?>> =
        callbackFlow {
            val subscription =
                handle.subscribeReleaseDetail(
                    releaseId,
                    object : ReleaseDetailCallback {
                        override fun onValue(value: BridgeRelease?) {
                            trySend(LiveQueryEvent.Value(value))
                        }

                        override fun onError(error: BridgeException) {
                            trySend(LiveQueryEvent.Error(error))
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

    /** One live library search whose query moves in place as the person types. */
    internal fun librarySearch(): LibrarySearch = BridgeLibrarySearch(handle.subscribeLibrarySearch())

    /**
     * The results for one fixed query, for a surface that asks a whole phrase at once (Android
     * Auto's spoken search). Values and nonterminal errors share one live flow; collecting
     * continues until the caller cancels.
     */
    internal fun searchResults(query: String): Flow<LiveQueryEvent<BridgeSearchResults>> =
        callbackFlow {
            val search = librarySearch()
            search.setQuery(query)
            val wanted = query.trim()
            launch {
                var reading = true
                while (reading) {
                    val delivered = runCatching { search.next() }
                    delivered.onSuccess { if (it.query == wanted) send(LiveQueryEvent.Value(it.results)) }
                    delivered.onFailure { error ->
                        when (error) {
                            is BridgeException.Cancelled -> reading = false
                            is BridgeException -> send(LiveQueryEvent.Error(error))
                            else -> throw error
                        }
                    }
                }
            }
            // The reader's pending read is cancelled with this flow; releasing
            // the search then ends it in core.
            awaitClose(search::release)
        }
}

internal interface CollectionBrowseQuery<Snapshot> {
    fun setWindows(windows: List<BridgeLibraryPageWindow>)

    suspend fun next(): Snapshot

    suspend fun cancel()
}

internal interface AlbumBrowseQuery : CollectionBrowseQuery<BridgeAlbumBrowseSnapshot>

internal interface ComposerBrowseQuery : CollectionBrowseQuery<BridgeComposerBrowseSnapshot>

internal interface ArtistBrowseQuery : CollectionBrowseQuery<BridgeArtistBrowseSnapshot>

private class BridgeAlbumBrowseQuery(
    private val subscription: AlbumBrowseSubscription,
) : AlbumBrowseQuery {
    override fun setWindows(windows: List<BridgeLibraryPageWindow>) = subscription.setWindows(windows)

    override suspend fun next(): BridgeAlbumBrowseSnapshot = subscription.next()

    override suspend fun cancel() = subscription.cancel()
}

private class BridgeComposerBrowseQuery(
    private val subscription: ComposerBrowseSubscription,
) : ComposerBrowseQuery {
    override fun setWindows(windows: List<BridgeLibraryPageWindow>) = subscription.setWindows(windows)

    override suspend fun next(): BridgeComposerBrowseSnapshot = subscription.next()

    override suspend fun cancel() = subscription.cancel()
}

private class BridgeArtistBrowseQuery(
    private val subscription: ArtistBrowseSubscription,
) : ArtistBrowseQuery {
    override fun setWindows(windows: List<BridgeLibraryPageWindow>) = subscription.setWindows(windows)

    override suspend fun next(): BridgeArtistBrowseSnapshot = subscription.next()

    override suspend fun cancel() = subscription.cancel()
}

/** A live library search: point it at a query, take each value, which names the query it answers. */
internal interface LibrarySearch {
    fun setQuery(query: String)

    suspend fun next(): BridgeLibrarySearchSnapshot

    suspend fun cancel()

    /** Free the search without waiting, from a scope that is already ending. */
    fun release()
}

private class BridgeLibrarySearch(
    private val subscription: LibrarySearchSubscription,
) : LibrarySearch {
    override fun setQuery(query: String) {
        subscription.setQuery(query)
    }

    override suspend fun next(): BridgeLibrarySearchSnapshot = subscription.next()

    override suspend fun cancel() = subscription.cancel()

    override fun release() = subscription.close()
}
