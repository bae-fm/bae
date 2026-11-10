package fm.bae.app.data

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.diagnosticDetail
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.map
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeSortCriterion

internal const val BROWSER_PAGE_SIZE = 60
private const val MAXIMUM_BROWSER_PAGE_WINDOWS = 3
private val logger = BaeLogger("bae.BrowserPageStores")

internal class PageError(
    val error: DisplayError,
    val onRetry: () -> Unit,
)

/**
 * One value a browse query delivered: each requested window with its rows, the list's total, and
 * what the list delivers [beside] its rows when that changed (the album grid's artist sections, in
 * the delivery whose rows they fit; null says the last ones stand; [Unit] for the other lists).
 */
internal class BrowseRows<Row, Beside>(
    val windows: List<Pair<BridgeLibraryPageWindow, List<Row>>>,
    val totalCount: Int,
    val beside: Beside?,
)

/** A live browse over one library list whose windows change in place, pulled as rows by window. */
internal interface BrowseRowsQuery<Row, Beside> : PulledSubscription<BrowseRows<Row, Beside>> {
    fun setWindows(windows: List<BridgeLibraryPageWindow>)
}

/**
 * One library list (albums, artists, or composers under one sort) read through a single browse
 * query. The visible range decides which pages the query reads; one read per database change
 * answers every visible page, rather than one live query per page each rerunning on its own.
 */
internal abstract class WindowedBrowserPageStore<Parameter, Row, Beside>(
    private val appContext: Context,
    protected val scope: CoroutineScope,
    /** What [beside] holds while no value has been delivered. */
    private val nothingBeside: Beside,
    /** The headline for a later page that failed to load, naming what this list holds. */
    @StringRes private val loadMoreFailed: Int,
    /** The headline for the first page failing to load, naming what this list holds. */
    @StringRes private val loadFailed: Int = R.string.library_load_failed,
) {
    val rows = mutableStateMapOf<Int, Row>()

    /** What the last value delivered beside its rows. */
    var beside by mutableStateOf(nothingBeside)
        private set
    var totalCount by mutableStateOf(0)
        private set
    var loading by mutableStateOf(true)
        private set
    var error by mutableStateOf<PageError?>(null)
        private set

    private var parameter: Parameter? = null
    private var active = false
    private var generation = 0
    private var query: BrowseRowsQuery<Row, Beside>? = null
    private var deliveries: ScreenSubscription<BrowseRows<Row, Beside>>? = null
    private var windows = emptySet<Int>()

    fun activate(parameter: Parameter) {
        if (active && this.parameter == parameter) return
        closeQuery()
        rows.clear()
        beside = nothingBeside
        totalCount = 0
        loading = true
        error = null
        this.parameter = parameter
        active = true
        generation++
        val opened = open(parameter)
        query = opened
        deliveries = deliver(opened, generation)
        requestWindows(setOf(0))
    }

    /** Stop reading [parameter]'s list, unless another has been activated since. */
    fun deactivate(parameter: Parameter) {
        if (this.parameter == parameter) deactivate()
    }

    fun deactivate() {
        active = false
        closeQuery()
        rows.clear()
        beside = nothingBeside
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

    protected abstract fun open(parameter: Parameter): BrowseRowsQuery<Row, Beside>

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
     * ends the pulls quietly; any other failure closes the query and is shown, since the list can
     * no longer update, until [retry] opens another.
     */
    private fun deliver(
        query: BrowseRowsQuery<Row, Beside>,
        openedGeneration: Int,
    ): ScreenSubscription<BrowseRows<Row, Beside>> {
        val current = { active && openedGeneration == generation }
        return ScreenSubscription(
            query,
            scope,
            onValue = { if (current()) apply(it) },
            onError = { error ->
                if (current()) {
                    closeQuery()
                    fail(error)
                }
            },
        )
    }

    /** A value answers the windows it was read for; a window dropped since keeps nothing. */
    private fun apply(delivered: BrowseRows<Row, Beside>) {
        for ((window, windowRows) in delivered.windows) {
            val offset = window.offset.toInt()
            if (offset !in windows) continue
            // Write only what differs: each write invalidates every reader of the map.
            windowRows.forEachIndexed { index, row -> if (rows[offset + index] != row) rows[offset + index] = row }
            for (position in offset + windowRows.size until offset + BROWSER_PAGE_SIZE) rows.remove(position)
        }
        rows.keys.filter { it >= delivered.totalCount }.forEach(rows::remove)
        totalCount = delivered.totalCount
        // Rows and what came with them land in this one turn.
        delivered.beside?.let { beside = it }
        loading = false
        error = null
    }

    private fun fail(value: BridgeException) {
        logger.error("browser page query failed", value)
        loading = false
        // This app words the headline; core's diagnostic stays behind it for the disclosure.
        error =
            PageError(
                DisplayError(
                    appContext.getString(
                        if (rows.isEmpty()) loadFailed else loadMoreFailed,
                    ),
                    value.diagnosticDetail,
                ),
                ::retry,
            )
    }

    private fun removePage(offset: Int) {
        repeat(BROWSER_PAGE_SIZE) { rows.remove(offset + it) }
    }

    private fun closeQuery() {
        deliveries?.stop()
        deliveries = null
        query = null
        windows = emptySet()
    }
}

/** The album grid's artist sections, which its browse delivers beside its rows; none when it is flat. */
internal typealias AlbumGridSections = List<BridgeLibraryBrowseSection>

internal data class AlbumBrowseParameters(
    val criterion: BridgeSortCriterion,
    val groupByArtist: Boolean,
)

internal class AlbumPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<AlbumBrowseParameters, BridgeAlbumAppearance, AlbumGridSections>(
        appContext,
        scope,
        nothingBeside = emptyList(),
        loadMoreFailed = R.string.library_load_more_albums_failed,
    ) {
    val sections: AlbumGridSections get() = beside

    fun activate(
        criterion: BridgeSortCriterion,
        groupByArtist: Boolean = false,
    ) {
        activate(AlbumBrowseParameters(criterion, groupByArtist))
    }

    override fun open(parameter: AlbumBrowseParameters): BrowseRowsQuery<BridgeAlbumAppearance, AlbumGridSections> =
        SnapshotRowsQuery(library.albumBrowse(listOf(parameter.criterion), parameter.groupByArtist)) { snapshot ->
            BrowseRows(
                snapshot.windows.map { it.window to it.rows },
                snapshot.rowCount.toInt(),
                snapshot.sections,
            )
        }
}

internal class ArtistPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeArtistSortCriterion, BridgeArtistSummary, Unit>(
        appContext,
        scope,
        nothingBeside = Unit,
        loadMoreFailed = R.string.library_load_more_artists_failed,
    ) {
    override fun open(parameter: BridgeArtistSortCriterion): BrowseRowsQuery<BridgeArtistSummary, Unit> =
        SnapshotRowsQuery(library.artistBrowse(parameter)) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt(), Unit)
        }
}

internal class ComposerPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeComposerSortCriterion, BridgeComposerSummary, Unit>(
        appContext,
        scope,
        nothingBeside = Unit,
        loadMoreFailed = R.string.library_load_more_composers_failed,
    ) {
    override fun open(parameter: BridgeComposerSortCriterion): BrowseRowsQuery<BridgeComposerSummary, Unit> =
        SnapshotRowsQuery(library.composerBrowse(parameter)) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt(), Unit)
        }
}

/** One composer's page rows, by window; another composer is another read. */
internal class ComposerDetailRowsPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<String, BridgeComposerDetailRow, Unit>(
        appContext,
        scope,
        nothingBeside = Unit,
        loadMoreFailed = R.string.composer_detail_load_failed,
        loadFailed = R.string.composer_detail_load_failed,
    ) {
    override fun open(parameter: String): BrowseRowsQuery<BridgeComposerDetailRow, Unit> =
        SnapshotRowsQuery(library.composerDetailRows(parameter)) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt(), Unit)
        }
}

/** The download queue as the Downloads screen draws it: its entries in the order queued, by window. */
internal class DownloadPageStore(
    private val library: Library,
    appContext: Context,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<Unit, BridgeDownloadOp, Unit>(
        appContext,
        scope,
        nothingBeside = Unit,
        loadMoreFailed = R.string.pinning_load_more_failed,
        loadFailed = R.string.pinning_load_failed,
    ) {
    override fun open(parameter: Unit): BrowseRowsQuery<BridgeDownloadOp, Unit> =
        SnapshotRowsQuery(library.downloadBrowse()) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt(), Unit)
        }
}

/** A collection browse query read as rows by window, through [read]. */
private class SnapshotRowsQuery<Snapshot, Row, Beside>(
    private val query: CollectionBrowseQuery<Snapshot>,
    read: (Snapshot) -> BrowseRows<Row, Beside>,
) : BrowseRowsQuery<Row, Beside>,
    PulledSubscription<BrowseRows<Row, Beside>> by query.map(read) {
    override fun setWindows(windows: List<BridgeLibraryPageWindow>) = query.setWindows(windows)
}

internal class BrowserPageStores(
    library: Library,
    appContext: Context,
    scope: CoroutineScope,
) {
    val albums = AlbumPageStore(library, appContext, scope)
    val artists = ArtistPageStore(library, appContext, scope)
    val composers = ComposerPageStore(library, appContext, scope)
    val composerRows = ComposerDetailRowsPageStore(library, appContext, scope)
    val downloads = DownloadPageStore(library, appContext, scope)

    fun cancel() {
        albums.deactivate()
        artists.deactivate()
        composers.deactivate()
        composerRows.deactivate()
        downloads.deactivate()
    }
}
