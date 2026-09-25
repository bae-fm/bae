package fm.bae.app.data

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
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeWorkDetail

private const val SEARCH_DEBOUNCE_MS = 300L

internal data class LiveQueryState<Value>(
    val value: Value? = null,
    val delivered: Boolean = false,
    val error: BridgeException? = null,
)

/**
 * One detail pane's live read, kept for as long as the pane's store is: [activate] moves it to the
 * item the pane shows, [deactivate] leaves it reading nothing, and only a value answering the item
 * shown now reaches [state].
 */
internal class DetailQueryStore<Value>(
    private val scope: CoroutineScope,
    private val open: () -> DetailRead<Value>,
) {
    private val mutableState = MutableStateFlow(LiveQueryState<Value>())
    val state: StateFlow<LiveQueryState<Value>> = mutableState.asStateFlow()
    private var parameter: String? = null
    private var read: DetailRead<Value>? = null
    private var deliveries: Job? = null

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
        parameter = null
        val current = read ?: return
        // A read that can no longer be pointed anywhere has ended; the next
        // activation opens another.
        runCatching { current.setId(null) }.onFailure { error ->
            if (error !is BridgeException) throw error
            closeRead()
        }
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
        deliveries = scope.launch { deliver(opened) }
        return opened
    }

    private suspend fun deliver(opened: DetailRead<Value>) {
        var reading = true
        while (reading) {
            val delivered = runCatching { opened.next() }
            delivered.getOrNull()?.let { delivery ->
                val shown = parameter
                if (shown != null && delivery.id == shown) {
                    mutableState.value = LiveQueryState(value = delivery.value, delivered = true)
                }
            }
            val error = delivered.exceptionOrNull() ?: continue
            if (error !is BridgeException) throw error
            if (error is BridgeException.Cancelled) {
                reading = false
            } else if (parameter != null) {
                mutableState.value = mutableState.value.copy(error = error)
            }
        }
    }

    private fun closeRead() {
        deliveries?.cancel()
        deliveries = null
        val closing = read ?: return
        read = null
        scope.launch { closing.cancel() }
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
    private var deliveries: Job? = null
    private var debounce: Job? = null

    fun activate(value: String) {
        val trimmed = value.trim()
        if (query == trimmed) return
        query = trimmed
        mutableState.value = LiveQueryState()
        debounce?.cancel()
        if (trimmed.isEmpty()) {
            search?.let { setQuery(it, "") }
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
        deliveries?.cancel()
        deliveries = null
        query = null
        val closing = search ?: return
        search = null
        scope.launch { closing.cancel() }
    }

    fun cancel() = deactivate()

    private fun open(): LibrarySearch {
        search?.let { return it }
        val live = library.librarySearch()
        search = live
        deliveries = scope.launch { deliver(live) }
        return live
    }

    private suspend fun deliver(live: LibrarySearch) {
        var reading = true
        while (reading) {
            val delivered = runCatching { live.next() }
            delivered.onSuccess { snapshot ->
                if (snapshot.query.isNotEmpty() && snapshot.query == query) {
                    mutableState.value = LiveQueryState(value = snapshot.results, delivered = true)
                }
            }
            delivered.onFailure { error ->
                when (error) {
                    is BridgeException.Cancelled -> reading = false
                    is BridgeException -> mutableState.value = mutableState.value.copy(error = error)
                    else -> throw error
                }
            }
        }
    }

    private fun setQuery(
        live: LibrarySearch,
        text: String,
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
