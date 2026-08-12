package fm.bae.app.data

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import uniffi.bae_bridge.AlbumBrowseSubscription
import uniffi.bae_bridge.AlbumDetailCallback
import uniffi.bae_bridge.AlbumPageCallback
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.ArtistDetailCallback
import uniffi.bae_bridge.ArtistPageCallback
import uniffi.bae_bridge.BridgeAlbumBrowseSnapshot
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumPage
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeComposerBrowseSnapshot
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerPage
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.ComposerBrowseSubscription
import uniffi.bae_bridge.ComposerDetailCallback
import uniffi.bae_bridge.ComposerPageCallback
import uniffi.bae_bridge.LibrarySearchCallback
import uniffi.bae_bridge.LiveSubscription
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
    fun subscribeAlbumPage(
        sortCriteria: List<BridgeSortCriterion>,
        offset: ULong,
        limit: ULong,
        callback: AlbumPageCallback,
    ): LiveSubscription = handle.subscribeAlbumPage(sortCriteria, offset, limit, callback)

    internal fun albumPages(
        sortCriteria: List<BridgeSortCriterion>,
        offset: ULong,
        limit: ULong,
    ): Flow<LiveQueryEvent<BridgeAlbumPage>> =
        callbackFlow {
            val subscription =
                subscribeAlbumPage(
                    sortCriteria,
                    offset,
                    limit,
                    object : AlbumPageCallback {
                        override fun onValue(value: BridgeAlbumPage) {
                            trySend(LiveQueryEvent.Value(value))
                        }

                        override fun onError(error: BridgeException) {
                            trySend(LiveQueryEvent.Error(error))
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }

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

    fun subscribeComposerPage(
        sortCriterion: BridgeComposerSortCriterion,
        offset: ULong,
        limit: ULong,
        callback: ComposerPageCallback,
    ): LiveSubscription = handle.subscribeComposerPage(listOf(sortCriterion), offset, limit, callback)

    internal fun composerPages(
        sortCriterion: BridgeComposerSortCriterion,
        offset: ULong,
        limit: ULong,
    ): Flow<LiveQueryEvent<BridgeComposerPage>> =
        callbackFlow {
            val subscription =
                subscribeComposerPage(
                    sortCriterion,
                    offset,
                    limit,
                    object : ComposerPageCallback {
                        override fun onValue(value: BridgeComposerPage) {
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

    fun subscribeArtistPage(
        sortCriterion: BridgeArtistSortCriterion,
        offset: ULong,
        limit: ULong,
        callback: ArtistPageCallback,
    ): LiveSubscription = handle.subscribeArtistPage(listOf(sortCriterion), offset, limit, callback)

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

    /**
     * Search albums and tracks by free-text query. Values and nonterminal errors
     * share one live flow; collecting continues until the caller cancels.
     */
    internal fun searchResults(query: String): Flow<LiveQueryEvent<BridgeSearchResults>> =
        callbackFlow {
            val subscription =
                handle.subscribeLibrarySearch(
                    query,
                    object : LibrarySearchCallback {
                        override fun onValue(value: BridgeSearchResults) {
                            trySend(LiveQueryEvent.Value(value))
                        }

                        override fun onError(error: BridgeException) {
                            trySend(LiveQueryEvent.Error(error))
                        }
                    },
                )
            awaitClose(subscription::cancel)
        }
}

internal interface CollectionBrowseQuery<Snapshot> {
    fun setWindows(windows: List<BridgeLibraryPageWindow>)

    suspend fun next(): Snapshot

    suspend fun cancel()
}

internal interface AlbumBrowseQuery : CollectionBrowseQuery<BridgeAlbumBrowseSnapshot>

internal interface ComposerBrowseQuery : CollectionBrowseQuery<BridgeComposerBrowseSnapshot>

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
