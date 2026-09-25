package fm.bae.app.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
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

private fun <Value> MutableStateFlow<LiveQueryState<Value>>.apply(event: LiveQueryEvent<Value?>) {
    value =
        when (event) {
            is LiveQueryEvent.Value -> LiveQueryState(value = event.value, delivered = true)
            is LiveQueryEvent.Error -> value.copy(error = event.error)
        }
}

internal class DetailQueryStore<Value>(
    private val scope: CoroutineScope,
    private val subscribe: (String) -> Flow<LiveQueryEvent<Value?>>,
) {
    private val mutableState = MutableStateFlow(LiveQueryState<Value>())
    val state: StateFlow<LiveQueryState<Value>> = mutableState.asStateFlow()
    private var parameter: String? = null
    private var job: Job? = null
    private var generation = 0L

    fun activate(value: String) {
        if (parameter == value && job?.isActive == true) return
        parameter = value
        start(value)
    }

    fun retry() {
        parameter?.let(::start)
    }

    fun deactivate(value: String) {
        if (parameter != value) return
        job?.cancel()
        job = null
        parameter = null
        generation++
    }

    fun cancel() {
        job?.cancel()
        job = null
        parameter = null
        generation++
    }

    private fun start(value: String) {
        job?.cancel()
        generation++
        val currentGeneration = generation
        mutableState.value = LiveQueryState()
        job =
            scope.launch {
                subscribe(value).collect { event ->
                    if (generation == currentGeneration) {
                        mutableState.apply(event)
                    }
                }
            }
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
    val album = DetailQueryStore<BridgeAlbumDetail>(scope, library::albumDetails)
    val artist = DetailQueryStore<BridgeArtistDetail>(scope, library::artistDetails)
    val composer = DetailQueryStore<BridgeComposerDetail>(scope, library::composerDetails)
    val work = DetailQueryStore<BridgeWorkDetail>(scope, library::workDetails)
    val search = SearchQueryStore(library, scope)

    fun cancel() {
        album.cancel()
        artist.cancel()
        composer.cancel()
        work.cancel()
        search.cancel()
    }
}
