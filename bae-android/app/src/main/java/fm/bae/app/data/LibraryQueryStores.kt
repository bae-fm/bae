package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrarySearchSnapshot
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeWorkDetail

private val logger = BaeLogger("bae.LibraryQueryStores")

internal const val SEARCH_DEBOUNCE_MS = 300L

internal data class LiveQueryState<Value>(
    val value: Value? = null,
    val delivered: Boolean = false,
    val error: BridgeException? = null,
)

/**
 * One detail pane's live read, open while the pane shows an item: [activate] opens it or moves it to
 * the item the pane shows, [deactivate] ends it when the pane stops showing that item, and only a
 * value answering the item shown now reaches [state].
 */
internal class DetailQueryStore<Value>(
    private val scope: CoroutineScope,
    private val open: () -> DetailRead<Value>,
) {
    private val mutableState = MutableStateFlow(LiveQueryState<Value>())
    val state: StateFlow<LiveQueryState<Value>> = mutableState.asStateFlow()
    private var parameter: String? = null
    private var read: DetailRead<Value>? = null
    private var deliveries: ScreenSubscription<DetailDelivery<Value>>? = null

    fun activate(value: String) {
        if (parameter == value && read != null) return
        parameter = value
        mutableState.value = LiveQueryState()
        request(value)
    }

    /** Read the item shown now again on a fresh read, after one failed. */
    fun retry() {
        val current = parameter ?: return
        closeRead()
        mutableState.value = LiveQueryState()
        request(current)
    }

    fun deactivate(value: String) {
        if (parameter != value) return
        cancel()
    }

    fun cancel() {
        parameter = null
        closeRead()
    }

    private fun request(value: String) {
        val current = read ?: openRead()
        try {
            current.setId(value)
        } catch (error: BridgeException) {
            mutableState.value = mutableState.value.copy(error = error)
        }
    }

    private fun openRead(): DetailRead<Value> {
        val opened = open()
        read = opened
        deliveries = ScreenSubscription(opened, scope, ::deliver, ::report)
        return opened
    }

    /** Only a value answering the item shown now reaches [state]. */
    private fun deliver(delivery: DetailDelivery<Value>) {
        val shown = parameter
        if (shown != null && delivery.id == shown) {
            mutableState.value = LiveQueryState(value = delivery.value, delivered = true)
        }
    }

    private fun report(error: BridgeException) {
        if (parameter != null) {
            mutableState.value = mutableState.value.copy(error = error)
        } else {
            // Nothing is shown to report it on; the read goes on.
            logger.warning("detail read failed while showing nothing", error)
        }
    }

    private fun closeRead() {
        deliveries?.stop()
        deliveries = null
        read = null
    }
}

/**
 * The search screen's one live search. [activate] moves it to the field's text once typing pauses,
 * so a run of keystrokes is one query change on one subscription; [deactivate] closes it when the
 * screen goes. Only a value answering the query standing now is shown.
 */
internal class SearchQueryStore(
    private val library: Library,
    private val scope: CoroutineScope,
) {
    private val mutableState = MutableStateFlow(LiveQueryState<BridgeSearchResults>())
    val state: StateFlow<LiveQueryState<BridgeSearchResults>> = mutableState.asStateFlow()
    private var query: String? = null
    private var search: LibrarySearch? = null
    private var deliveries: ScreenSubscription<BridgeLibrarySearchSnapshot>? = null
    private var debounce: Job? = null

    fun activate(value: String) {
        val trimmed = value.trim()
        if (query == trimmed) return
        query = trimmed
        mutableState.value = LiveQueryState()
        debounce?.cancel()
        if (trimmed.isEmpty()) {
            search?.let { setQuery(it, null) }
            return
        }
        val live = open()
        debounce =
            scope.launch {
                delay(SEARCH_DEBOUNCE_MS)
                if (query == trimmed) setQuery(live, trimmed)
            }
    }

    fun deactivate() {
        debounce?.cancel()
        debounce = null
        deliveries?.stop()
        deliveries = null
        query = null
        search = null
    }

    /** Open the standing query again, on a new search, after it failed. */
    fun retry() {
        val standing = query
        deactivate()
        standing?.let(::activate)
    }

    fun cancel() = deactivate()

    private fun open(): LibrarySearch {
        search?.let { return it }
        val live = library.librarySearch()
        search = live
        deliveries =
            ScreenSubscription(
                live,
                scope,
                onValue = { snapshot ->
                    // Only a value answering the query standing now is shown.
                    if (snapshot.query != null && snapshot.query == query) {
                        mutableState.value = LiveQueryState(value = snapshot.results, delivered = true)
                    }
                },
                onError = { error -> mutableState.value = mutableState.value.copy(error = error) },
            )
        return live
    }

    private fun setQuery(
        live: LibrarySearch,
        text: String?,
    ) {
        runCatching { live.setQuery(text) }
            .onFailure { error ->
                if (error !is BridgeException) throw error
                mutableState.value = LiveQueryState(error = error)
            }
    }
}

internal class LibraryQueryStores(
    library: Library,
    scope: CoroutineScope,
) {
    val album = DetailQueryStore<BridgeAlbumDetail>(scope, library::albumDetail)
    val artist = DetailQueryStore<BridgeArtistDetail>(scope, library::artistDetail)
    val composer = DetailQueryStore<BridgeComposerDetail>(scope, library::composerDetail)
    val work = DetailQueryStore<BridgeWorkDetail>(scope, library::workDetail)
    val search = SearchQueryStore(library, scope)

    fun cancel() {
        album.cancel()
        artist.cancel()
        composer.cancel()
        work.cancel()
        search.cancel()
    }
}
