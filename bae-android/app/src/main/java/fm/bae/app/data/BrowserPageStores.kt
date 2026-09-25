package fm.bae.app.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeSortCriterion

internal const val BROWSER_PAGE_SIZE = 60
private const val MAXIMUM_BROWSER_PAGE_WINDOWS = 3
private val logger = BaeLogger("bae.BrowserPageStores")

internal class PageError(
    val message: String,
    val onRetry: () -> Unit,
)

/** One value a browse query delivered: each requested window with its rows, and the list's total. */
internal class BrowseRows<Row>(
    val windows: List<Pair<BridgeLibraryPageWindow, List<Row>>>,
    val totalCount: Int,
)

/** A live browse over one library list whose windows change in place. */
internal interface BrowseRowsQuery<Row> {
    fun setWindows(windows: List<BridgeLibraryPageWindow>)

    suspend fun next(): BrowseRows<Row>

    suspend fun cancel()
}

/**
 * One library list (albums, artists, or composers under one sort) read through a single browse
 * query. The visible range decides which pages the query reads; one read per database change
 * answers every visible page, rather than one live query per page each rerunning on its own.
 */
internal abstract class WindowedBrowserPageStore<Parameter, Row>(
    private val appContext: Context,
    protected val scope: CoroutineScope,
) {
    val rows = mutableStateMapOf<Int, Row>()
    var totalCount by mutableStateOf(0)
        private set
    var loading by mutableStateOf(true)
        private set
    var error by mutableStateOf<PageError?>(null)
        private set

    private var parameter: Parameter? = null
    private var active = false
    private var generation = 0
    private var query: BrowseRowsQuery<Row>? = null
    private var deliveries: Job? = null
    private var windows = emptySet<Int>()

    fun activate(parameter: Parameter) {
        if (active && this.parameter == parameter) return
        closeQuery()
        rows.clear()
        totalCount = 0
        loading = true
        error = null
        this.parameter = parameter
        active = true
        generation++
        val opened = open(parameter)
        query = opened
        val openedGeneration = generation
        deliveries = scope.launch(Dispatchers.Main.immediate) { deliver(opened, openedGeneration) }
        requestWindows(setOf(0))
    }

    fun deactivate() {
        active = false
        closeQuery()
        rows.clear()
        totalCount = 0
    }

    fun reportVisibleRange(
        first: Int,
        last: Int,
    ) {
        if (!active || first > last) return
        val end = if (totalCount == 0) BROWSER_PAGE_SIZE else totalCount
        val firstPage = first.coerceIn(0, end) / BROWSER_PAGE_SIZE * BROWSER_PAGE_SIZE
        val lastPage = last.coerceIn(0, end) / BROWSER_PAGE_SIZE * BROWSER_PAGE_SIZE
        val wanted =
            generateSequence(firstPage) { offset ->
                (offset + BROWSER_PAGE_SIZE).takeIf { it <= lastPage }
            }.take(MAXIMUM_BROWSER_PAGE_WINDOWS)
                .toSet()
        (windows - wanted).forEach(::removePage)
        requestWindows(wanted)
    }

    fun retry() {
        val current = checkNotNull(parameter) { "browser page retry without parameters" }
        active = false
        activate(current)
    }

    protected abstract fun open(parameter: Parameter): BrowseRowsQuery<Row>

    private fun requestWindows(offsets: Set<Int>) {
        if (offsets == windows) return
        windows = offsets
        val current = query ?: return
        try {
            current.setWindows(
                offsets.sorted().map { BridgeLibraryPageWindow(it.toULong(), BROWSER_PAGE_SIZE.toULong()) },
            )
        } catch (value: BridgeException) {
            fail(value)
        }
    }

    /**
     * Apply each value the query delivers while it is still this store's query. A cancelled read
     * ends the loop quietly; any other failure is shown, since the list can no longer update.
     */
    private suspend fun deliver(
        query: BrowseRowsQuery<Row>,
        openedGeneration: Int,
    ) {
        val current = { active && openedGeneration == generation }
        var reading = true
        while (reading) {
            val delivered = runCatching { query.next() }
            delivered.onSuccess { if (current()) apply(it) }
            delivered.onFailure { error ->
                reading = false
                if (error is BridgeException && error !is BridgeException.Cancelled && current()) fail(error)
                if (error !is BridgeException) throw error
            }
            reading = reading && current()
        }
    }

    /** A value answers the windows it was read for; a window dropped since keeps nothing. */
    private fun apply(delivered: BrowseRows<Row>) {
        for ((window, windowRows) in delivered.windows) {
            val offset = window.offset.toInt()
            if (offset !in windows) continue
            removePage(offset)
            windowRows.forEachIndexed { index, row -> rows[offset + index] = row }
        }
        rows.keys.filter { it >= delivered.totalCount }.forEach(rows::remove)
        totalCount = delivered.totalCount
        loading = false
        error = null
    }

    private fun fail(value: BridgeException) {
        logger.error("browser page query failed", value)
        loading = false
        error =
            PageError(
                value.message
                    ?: appContext.getString(
                        if (rows.isEmpty()) R.string.library_load_failed else R.string.library_load_more_failed,
                    ),
                ::retry,
            )
    }

    private fun removePage(offset: Int) {
        repeat(BROWSER_PAGE_SIZE) { rows.remove(offset + it) }
    }

    private fun closeQuery() {
        deliveries?.cancel()
        deliveries = null
        windows = emptySet()
        val closing = query ?: return
        query = null
        scope.launch { closing.cancel() }
    }
}

internal class AlbumPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeSortCriterion, BridgeAlbum>(appContext, scope) {
    override fun open(parameter: BridgeSortCriterion): BrowseRowsQuery<BridgeAlbum> =
        SnapshotRowsQuery(library.albumBrowse(listOf(parameter))) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt())
        }
}

internal class ArtistPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeArtistSortCriterion, BridgeArtistSummary>(appContext, scope) {
    override fun open(parameter: BridgeArtistSortCriterion): BrowseRowsQuery<BridgeArtistSummary> =
        SnapshotRowsQuery(library.artistBrowse(parameter)) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt())
        }
}

internal class ComposerPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeComposerSortCriterion, BridgeComposerSummary>(appContext, scope) {
    override fun open(parameter: BridgeComposerSortCriterion): BrowseRowsQuery<BridgeComposerSummary> =
        SnapshotRowsQuery(library.composerBrowse(parameter)) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt())
        }
}

/** A collection browse query read as rows by window, through [read]. */
private class SnapshotRowsQuery<Snapshot, Row>(
    private val query: CollectionBrowseQuery<Snapshot>,
    private val read: (Snapshot) -> BrowseRows<Row>,
) : BrowseRowsQuery<Row> {
    override fun setWindows(windows: List<BridgeLibraryPageWindow>) = query.setWindows(windows)

    override suspend fun next(): BrowseRows<Row> = read(query.next())

    override suspend fun cancel() = query.cancel()
}

internal class BrowserPageStores(
    library: Library,
    appContext: Context,
    scope: CoroutineScope,
) {
    val albums = AlbumPageStore(library, appContext, scope)
    val artists = ArtistPageStore(library, appContext, scope)
    val composers = ComposerPageStore(library, appContext, scope)

    fun cancel() {
        albums.deactivate()
        artists.deactivate()
        composers.deactivate()
    }
}
