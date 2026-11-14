package fm.bae.app.data

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
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
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeSortCriterion

internal const val BROWSER_PAGE_SIZE = 60
private const val MAXIMUM_BROWSER_PAGE_WINDOWS = 3
private val logger = BaeLogger("bae.BrowserPageStores")

internal class PageError(
    val error: DisplayError,
    val onRetry: () -> Unit,
)

/** What a library list's body draws its states from: the read still running, its failure, and how much it holds. */
internal interface BrowserPageState {
    val loading: Boolean
    val error: PageError?

    /** Whether any row is held to draw. */
    val hasRows: Boolean

    /** How many rows the list has. */
    val totalCount: Int
}

/** One value a browse query delivered: each requested window with its rows, and the list's total. */
internal class BrowseRows<Row>(
    val windows: List<Pair<BridgeLibraryPageWindow, List<Row>>>,
    val totalCount: Int,
)

/** A live browse over one library list whose windows change in place, pulled as rows by window. */
internal interface BrowseRowsQuery<Row> : PulledSubscription<BrowseRows<Row>> {
    fun setWindows(windows: List<BridgeLibraryPageWindow>)
}

/**
 * One library list (albums, artists, or composers under one sort) read through a single browse
 * query. The visible range decides which pages the query reads; one read per database change
 * answers every visible page, rather than one live query per page each rerunning on its own.
 */
internal abstract class WindowedBrowserPageStore<Parameter, Row>(
    private val appContext: Context,
    /** Core's answer to whether a failed read is worth reading again. */
    private val errors: ErrorDisplays,
    protected val scope: CoroutineScope,
    /** The headline for a later page that failed to load, naming what this list holds. */
    @StringRes private val loadMoreFailed: Int,
    /** The headline for the first page failing to load, naming what this list holds. */
    @StringRes private val loadFailed: Int = R.string.library_load_failed,
) : BrowserPageState {
    val rows = mutableStateMapOf<Int, Row>()

    override var totalCount by mutableStateOf(0)
        private set
    override var loading by mutableStateOf(true)
        private set
    override var error by mutableStateOf<PageError?>(null)
        private set
    override val hasRows: Boolean get() = rows.isNotEmpty()

    private var parameter: Parameter? = null
    private var active = false
    private var generation = 0
    private var query: BrowseRowsQuery<Row>? = null
    private var deliveries: ScreenSubscription<BrowseRows<Row>>? = null
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
                offsets.sorted().map { BridgeLibraryPageWindow(it.toULong(), BROWSER_PAGE_SIZE.toULong(), null) },
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
        query: BrowseRowsQuery<Row>,
        openedGeneration: Int,
    ): ScreenSubscription<BrowseRows<Row>> {
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
    private fun apply(delivered: BrowseRows<Row>) {
        for ((window, windowRows) in delivered.windows) {
            val offset = window.offset.toInt()
            if (offset !in windows) continue
            // Write only what differs: each write invalidates every reader of the map.
            windowRows.forEachIndexed { index, row -> if (rows[offset + index] != row) rows[offset + index] = row }
            for (position in offset + windowRows.size until offset + BROWSER_PAGE_SIZE) rows.remove(position)
        }
        rows.keys.filter { it >= delivered.totalCount }.forEach(rows::remove)
        totalCount = delivered.totalCount
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
                    // A cancellation, which core shows nothing for, rules nothing out.
                    errors.display(value)?.offersRetry != false,
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

internal data class AlbumBrowseParameters(
    val criterion: BridgeSortCriterion,
    val groupByArtist: Boolean,
)

internal class AlbumPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : ChainBrowserPageStore<AlbumBrowseParameters, BridgeAlbumAppearance>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.library_load_more_albums_failed,
    ) {
    fun activate(
        criterion: BridgeSortCriterion,
        groupByArtist: Boolean = false,
    ) {
        activate(AlbumBrowseParameters(criterion, groupByArtist))
    }

    override fun open(parameter: AlbumBrowseParameters): ListReads<BridgeAlbumAppearance> =
        library.albumList(listOf(parameter.criterion), parameter.groupByArtist)
}

internal class ArtistPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeArtistSortCriterion, BridgeArtistSummary>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.library_load_more_artists_failed,
    ) {
    override fun open(parameter: BridgeArtistSortCriterion): BrowseRowsQuery<BridgeArtistSummary> =
        SnapshotRowsQuery(library.artistBrowse(parameter)) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt())
        }
}

internal class ComposerPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<BridgeComposerSortCriterion, BridgeComposerSummary>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.library_load_more_composers_failed,
    ) {
    override fun open(parameter: BridgeComposerSortCriterion): BrowseRowsQuery<BridgeComposerSummary> =
        SnapshotRowsQuery(library.composerBrowse(parameter)) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt())
        }
}

/** One composer's page rows, by window; another composer is another read. */
internal class ComposerDetailRowsPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<String, BridgeComposerDetailRow>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.composer_detail_load_failed,
        loadFailed = R.string.composer_detail_load_failed,
    ) {
    override fun open(parameter: String): BrowseRowsQuery<BridgeComposerDetailRow> =
        SnapshotRowsQuery(library.composerDetailRows(parameter)) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt())
        }
}

/** The download queue as the Downloads screen draws it: its entries in the order queued, by window. */
internal class DownloadPageStore(
    private val library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) : WindowedBrowserPageStore<Unit, BridgeDownloadOp>(
        appContext,
        errors,
        scope,
        loadMoreFailed = R.string.pinning_load_more_failed,
        loadFailed = R.string.pinning_load_failed,
    ) {
    override fun open(parameter: Unit): BrowseRowsQuery<BridgeDownloadOp> =
        SnapshotRowsQuery(library.downloadBrowse()) { snapshot ->
            BrowseRows(snapshot.windows.map { it.window to it.rows }, snapshot.totalCount.toInt())
        }
}

/** A collection browse query read as rows by window, through [read]. */
private class SnapshotRowsQuery<Snapshot, Row>(
    private val query: CollectionBrowseQuery<Snapshot>,
    read: (Snapshot) -> BrowseRows<Row>,
) : BrowseRowsQuery<Row>,
    PulledSubscription<BrowseRows<Row>> by query.map(read) {
    override fun setWindows(windows: List<BridgeLibraryPageWindow>) = query.setWindows(windows)
}

internal class BrowserPageStores(
    library: Library,
    appContext: Context,
    errors: ErrorDisplays,
    scope: CoroutineScope,
) {
    val albums = AlbumPageStore(library, appContext, errors, scope)
    val artists = ArtistPageStore(library, appContext, errors, scope)
    val composers = ComposerPageStore(library, appContext, errors, scope)
    val composerRows = ComposerDetailRowsPageStore(library, appContext, errors, scope)
    val downloads = DownloadPageStore(library, appContext, errors, scope)
    val playlists = PlaylistListingStore(library::playlists, appContext, errors, scope)

    fun cancel() {
        albums.deactivate()
        artists.deactivate()
        composers.deactivate()
        composerRows.deactivate()
        downloads.deactivate()
        playlists.deactivate()
    }
}
