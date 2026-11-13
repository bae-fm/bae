package fm.bae.app.data

import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.map
import fm.bae.screensubscription.pullEach
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeAlbumBrowseSnapshot
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeArtistBrowseSnapshot
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeComposerBrowseSnapshot
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailRowsSnapshot
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeDownloadBrowseSnapshot
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeLibrarySearchSnapshot
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeWorkDetail
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
 * A detail view's live read: the id it shows changes in place, and each value it is pulled for
 * names the id it answers.
 */
internal interface DetailRead<Value> : PulledSubscription<DetailDelivery<Value>> {
    fun setId(id: String)

    /** Free the read without waiting, from a scope that is already ending. */
    fun release()
}

private class BridgeDetailRead<Value>(
    private val set: (String) -> Unit,
    pulls: PulledSubscription<DetailDelivery<Value>>,
    private val free: () -> Unit,
) : DetailRead<Value>,
    PulledSubscription<DetailDelivery<Value>> by pulls {
    override fun setId(id: String) = set(id)

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
            read.pullEach(
                onValue = { if (it.id == id) send(LiveQueryEvent.Value(it.value)) },
                onError = { send(LiveQueryEvent.Error(it)) },
            )
        }
        // The reader's pending read is cancelled with this flow; releasing the
        // read then ends it in core.
        awaitClose(read::release)
    }

/**
 * Narrow projection of [AppHandleInterface] for library browse and detail live queries.
 * Each flow stays subscribed after an error and can deliver later values; the
 * error is an event rather than flow termination. Image bytes are not here —
 * every image in the app resolves through [fm.bae.app.data.ImageStore], which
 * owns their caching too.
 * Mirrors the macOS `Library` domain service.
 */
class Library(
    private val handle: AppHandleInterface,
) {
    internal fun albumBrowse(
        sortCriteria: List<BridgeSortCriterion>,
        groupByArtist: Boolean = false,
    ): AlbumBrowseQuery =
        handle.subscribeAlbumBrowse(sortCriteria, groupByArtist).let {
            BridgeBrowseQuery(it::setWindows, pulled(it::next, it::cancel))
        }

    /** A detail view's live reads, each moved to every item its view shows in place. */
    internal fun albumDetail(): DetailRead<BridgeAlbumDetail> =
        handle.subscribeAlbumDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun releaseDetail(): DetailRead<BridgeRelease> =
        handle.subscribeReleaseDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun artistDetail(): DetailRead<BridgeArtistDetail> =
        handle.subscribeArtistDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun composerDetail(): DetailRead<BridgeComposerDetail> =
        handle.subscribeComposerDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
                free = read::close,
            )
        }

    internal fun workDetail(): DetailRead<BridgeWorkDetail> =
        handle.subscribeWorkDetail().let { read ->
            BridgeDetailRead(
                set = read::setId,
                pulls = pulled(read::next, read::cancel).map { DetailDelivery(it.id, it.value) },
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
        handle.subscribeComposerBrowse(listOf(sortCriterion)).let {
            BridgeBrowseQuery(it::setWindows, pulled(it::next, it::cancel))
        }

    /** One composer's page rows, in windows that move as its screen scrolls. */
    internal fun composerDetailRows(artistId: String): ComposerDetailRowsQuery =
        handle.subscribeComposerDetailRows(artistId).let {
            BridgeBrowseQuery(it::setWindows, pulled(it::next, it::cancel))
        }

    internal fun artistBrowse(sortCriterion: BridgeArtistSortCriterion): ArtistBrowseQuery =
        handle.subscribeArtistBrowse(listOf(sortCriterion)).let {
            BridgeBrowseQuery(it::setWindows, pulled(it::next, it::cancel))
        }

    /** The download queue as the Downloads screen draws it, in windows that move as it scrolls. */
    internal fun downloadBrowse(): DownloadBrowseQuery =
        handle.subscribeDownloadBrowse().let {
            BridgeBrowseQuery(it::setWindows, pulled(it::next, it::cancel))
        }

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
                search.pullEach(
                    onValue = { if (it.query == wanted) send(LiveQueryEvent.Value(it.results)) },
                    onError = { send(LiveQueryEvent.Error(it)) },
                )
            }
            // The reader's pending read is cancelled with this flow; releasing
            // the search then ends it in core.
            awaitClose(search::release)
        }
}

/** A live browse over one library list whose windows change in place; each value answers them all. */
internal interface CollectionBrowseQuery<Snapshot> : PulledSubscription<Snapshot> {
    fun setWindows(windows: List<BridgeLibraryPageWindow>)
}

private class BridgeBrowseQuery<Snapshot>(
    private val set: (List<BridgeLibraryPageWindow>) -> Unit,
    pulls: PulledSubscription<Snapshot>,
) : CollectionBrowseQuery<Snapshot>,
    PulledSubscription<Snapshot> by pulls {
    override fun setWindows(windows: List<BridgeLibraryPageWindow>) = set(windows)
}

internal typealias AlbumBrowseQuery = CollectionBrowseQuery<BridgeAlbumBrowseSnapshot>

internal typealias ComposerBrowseQuery = CollectionBrowseQuery<BridgeComposerBrowseSnapshot>

internal typealias ComposerDetailRowsQuery = CollectionBrowseQuery<BridgeComposerDetailRowsSnapshot>

internal typealias ArtistBrowseQuery = CollectionBrowseQuery<BridgeArtistBrowseSnapshot>

internal typealias DownloadBrowseQuery = CollectionBrowseQuery<BridgeDownloadBrowseSnapshot>

/** A live library search: point it at a query, and each value it is pulled for names the query it answers. */
internal interface LibrarySearch : PulledSubscription<BridgeLibrarySearchSnapshot> {
    /**
     * Search for [query] from now on, or for nothing when it is null; returns the request revision
     * its answers will carry.
     */
    fun setQuery(query: String?): ULong

    /** Free the search without waiting, from a scope that is already ending. */
    fun release()
}

private class BridgeLibrarySearch(
    private val subscription: LibrarySearchSubscription,
) : LibrarySearch,
    PulledSubscription<BridgeLibrarySearchSnapshot> by pulled(subscription::next, subscription::cancel) {
    override fun setQuery(query: String?): ULong = subscription.setQuery(query)

    override fun release() = subscription.close()
}
