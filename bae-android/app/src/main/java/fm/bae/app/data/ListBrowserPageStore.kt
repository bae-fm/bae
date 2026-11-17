package fm.bae.app.data

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.ErrorDisplays
import fm.bae.app.R
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeListCountExtra

private val logger = BaeLogger("bae.ListBrowserPageStore")

/**
 * One library list read through an [OpenList]: its view and its count, and its headings as a read of their own.
 * The screen reports the rows it shows, and core keeps the window it reads around them.
 */
internal abstract class ListBrowserPageStore<Parameter, Row>(
    private val appContext: Context,
    /** Core's answer to whether a failed read is worth reading again. */
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
    /** The headline for a later read that failed, naming what this list holds. */
    @StringRes private val loadMoreFailed: Int,
    /** The headline for the first read failing, naming what this list holds. */
    @StringRes private val loadFailed: Int = R.string.library_load_failed,
) : BrowserPageState {
    /** The open list's reads; null until [activate] and after [deactivate]. A failed read stops them and keeps them. */
    private var reader by mutableStateOf<OpenList<Row>?>(null)

    /**
     * How many rows the screen fits, from its height and its row height; the list opens its view with that many
     * rows on screen, before any row arrives.
     */
    var screenRows = 1
    override val totalCount: Int
        get() =
            reader
                ?.count
                ?.rows
                ?.toInt() ?: 0
    override var loading by mutableStateOf(true)
        private set
    override var error by mutableStateOf<PageError?>(null)
        private set
    override val hasRows: Boolean get() = firstReadReturned && totalCount > 0

    /** Whether the album grid's count says the grid groups its rows by artist; other lists never do. */
    val grouped: Boolean get() = (reader?.count?.extra as? BridgeListCountExtra.AlbumGrid)?.grouped == true

    /** The artist headings of the album grid grouped by artist, from their own read. */
    var sections by mutableStateOf(emptyList<BridgeLibraryBrowseSection>())
        private set

    private var parameter: Parameter? = null
    private var active = false
    private var generation = 0
    private var sectionDeliveries: ScreenSubscription<List<BridgeLibraryBrowseSection>>? = null

    /** Whether the list's count and its view have both returned since [activate]. */
    private var firstReadReturned by mutableStateOf(false)

    /** The row at [index], or null when the view's window doesn't hold it. */
    fun rowAt(index: Int): Row? = reader?.rowAt(index)

    fun activate(parameter: Parameter) {
        if (active && this.parameter == parameter) return
        close()
        sections = emptyList()
        loading = true
        error = null
        firstReadReturned = false
        this.parameter = parameter
        active = true
        generation++
        val opened = open(parameter)
        val current = generation
        val opening = OpenList(scope, opened, { changed(current) }, { failed(it, current) })
        reader = opening
        sectionDeliveries =
            opened.sections?.let { open ->
                ScreenSubscription(
                    open(),
                    scope,
                    onValue = { if (current == generation && it != sections) sections = it },
                    onError = { failed(it, current) },
                )
            }
        opening.start(0, maxOf(screenRows, 1) - 1)
    }

    /** Stop reading [parameter]'s list, unless the caller has activated another list since. */
    fun deactivate(parameter: Parameter) {
        if (this.parameter == parameter) deactivate()
    }

    fun deactivate() {
        active = false
        close()
        reader = null
        sections = emptyList()
    }

    fun reportVisibleRange(
        first: Int,
        last: Int,
    ) {
        if (active) reader?.show(first, last)
    }

    fun retry() {
        val current = checkNotNull(parameter) { "browser page retry without parameters" }
        active = false
        activate(current)
    }

    protected abstract fun open(parameter: Parameter): ListReads<Row>

    private fun changed(openedGeneration: Int) {
        if (openedGeneration != generation || !active) return
        if (reader?.answered == true) {
            firstReadReturned = true
            loading = false
            error = null
        }
    }

    /** A failed read closes the list's reads, since it can no longer update, until [retry] opens them again. */
    private fun failed(
        value: BridgeException,
        openedGeneration: Int,
    ) {
        if (openedGeneration != generation || !active) return
        logger.error("browser page query failed", value)
        close()
        loading = false
        error = PageError(appContext.getString(if (hasRows) loadMoreFailed else loadFailed), value, errors, ::retry)
    }

    private fun close() {
        reader?.stop()
        sectionDeliveries?.stop()
        sectionDeliveries = null
    }
}
