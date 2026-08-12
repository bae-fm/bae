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

internal class SearchQueryStore(
    private val library: Library,
    private val scope: CoroutineScope,
) {
    private val mutableState = MutableStateFlow(LiveQueryState<BridgeSearchResults>())
    val state: StateFlow<LiveQueryState<BridgeSearchResults>> = mutableState.asStateFlow()
    private var query: String? = null
    private var job: Job? = null
    private var generation = 0L

    fun activate(value: String) {
        if (query == value && job?.isActive == true) return
        query = value
        job?.cancel()
        generation++
        val currentGeneration = generation
        mutableState.value = mutableState.value.copy(error = null)
        job =
            scope.launch {
                delay(300)
                library.searchResults(value).collect { event ->
                    if (generation == currentGeneration) {
                        mutableState.apply(event)
                    }
                }
            }
    }

    fun deactivate(value: String) {
        if (query != value) return
        job?.cancel()
        job = null
        query = null
        generation++
    }

    fun cancel() {
        job?.cancel()
        job = null
        query = null
        generation++
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
