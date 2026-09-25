package fm.bae.app.data

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AlbumBrowseSubscription
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.ArtistBrowseSubscription
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
import uniffi.bae_bridge.LibrarySearchSubscription

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
 * One value a detail read delivered: the id it was read for, and that item's detail, or none once
 * no such item exists.
 */
internal data class DetailDelivery<Value>(
    val id: String?,
    val value: Value?,
)

/**
 * A detail view's live read: the id it shows changes in place, and each value names the id it
 * answers.
 */
internal interface DetailRead<Value> {
    fun setId(id: String?)

    suspend fun next(): DetailDelivery<Value>

    suspend fun cancel()

    /** Free the read without waiting, from a scope that is already ending. */
    fun release()
}

private class BridgeDetailRead<Value>(
    private val set: (String?) -> Unit,
    private val take: suspend () -> DetailDelivery<Value>,
    private val end: suspend () -> Unit,
    private val free: () -> Unit,
) : DetailRead<Value> {
    override fun setId(id: String?) = set(id)

    override suspend fun next(): DetailDelivery<Value> = take()

    override suspend fun cancel() = end()

    override fun release() = free()
}

/** One item's detail as it changes: each value, and each failure as an event rather than the end. */
internal typealias DetailEvents<Value> = Flow<LiveQueryEvent<Value?>>

/**
 * One item's detail as a flow over a read of its own, opened when collection starts and released
 * when it stops. Errors are events; a cancelled read ends the flow.
 */
private fun <Value> detailFlow(
    open: () -> DetailRead<Value>,
    id: String,
): DetailEvents<Value> =
    callbackFlow {
        val read = open()
        read.setId(id)
        launch {
            var reading = true
            while (reading) {
                val delivered = runCatching { read.next() }
                delivered.onSuccess { if (it.id == id) send(LiveQueryEvent.Value(it.value)) }
                delivered.onFailure { error ->
                    when (error) {
                        is BridgeException.Cancelled -> reading = false
                        is BridgeException -> send(LiveQueryEvent.Error(error))
                        else -> throw error
                    }
                }
            }
        }
        // The reader's pending read is cancelled with this flow; releasing the
        // read then ends it in core.
        awaitClose(read::release)
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

    /** A detail view's live reads, each moved to every item its view shows in place. */
    internal fun albumDetail(): DetailRead<BridgeAlbumDetail> =
        handle.subscribeAlbumDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                take = { read.next().let { DetailDelivery(it.id, it.value) } },
                end = read::cancel,
                free = read::close,
            )
        }

    internal fun releaseDetail(): DetailRead<BridgeRelease> =
        handle.subscribeReleaseDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                take = { read.next().let { DetailDelivery(it.id, it.value) } },
                end = read::cancel,
                free = read::close,
            )
        }

    internal fun artistDetail(): DetailRead<BridgeArtistDetail> =
        handle.subscribeArtistDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                take = { read.next().let { DetailDelivery(it.id, it.value) } },
                end = read::cancel,
                free = read::close,
            )
        }

    internal fun composerDetail(): DetailRead<BridgeComposerDetail> =
        handle.subscribeComposerDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                take = { read.next().let { DetailDelivery(it.id, it.value) } },
                end = read::cancel,
                free = read::close,
            )
        }

    internal fun workDetail(): DetailRead<BridgeWorkDetail> =
        handle.subscribeWorkDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                take = { read.next().let { DetailDelivery(it.id, it.value) } },
                end = read::cancel,
                free = read::close,
            )
        }

    /**
     * One album's detail as a flow, for a surface that holds many items' details at once (Android
     * Auto's browse tree): each item is its own row whose data changes on its own, so each keeps
     * its own read for as long as the flow is collected.
     */
    internal fun albumDetails(id: String): DetailEvents<BridgeAlbumDetail> = detailFlow(::albumDetail, id)

    internal fun composerBrowse(sortCriterion: BridgeComposerSortCriterion): ComposerBrowseQuery =
        BridgeComposerBrowseQuery(handle.subscribeComposerBrowse(listOf(sortCriterion)))

    internal fun artistBrowse(sortCriterion: BridgeArtistSortCriterion): ArtistBrowseQuery =
        BridgeArtistBrowseQuery(handle.subscribeArtistBrowse(listOf(sortCriterion)))

    internal fun composerDetails(id: String): DetailEvents<BridgeComposerDetail> = detailFlow(::composerDetail, id)

    internal fun workDetails(id: String): DetailEvents<BridgeWorkDetail> = detailFlow(::workDetail, id)

    internal fun releaseDetails(id: String): DetailEvents<BridgeRelease> = detailFlow(::releaseDetail, id)

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
