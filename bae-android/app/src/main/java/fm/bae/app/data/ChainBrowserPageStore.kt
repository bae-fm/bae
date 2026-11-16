package fm.bae.app.data

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.ErrorDisplays
import fm.bae.app.R
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeListCount
import uniffi.bae_bridge.BridgeSortValue

private val logger = BaeLogger("bae.ChainBrowserPageStore")

/**
 * The reads of one list, the same for every list: each window of it is a live read of its own, its
 * count and the position of a key in it are live reads too, and a jump to a position reads the key there
 * once with a plain read.
 */
internal class ListReads<Row>(
    val window: (uniffi.bae_bridge.BridgeListWindow) -> ListWindowRead<Row>,
    val count: () -> PulledSubscription<BridgeListCount>,
    val position: (List<BridgeSortValue>) -> ListPositionRead,
    val keyAt: suspend (ULong) -> List<BridgeSortValue>?,
    /** The album grid grouped by artist: each artist's heading with where its albums start. Other lists have none. */
    val sections: (() -> PulledSubscription<List<BridgeLibraryBrowseSection>>)? = null,
)

/**
 * One library list read through a [ListChainReader]: a chain of window reads with its count, and its headings
 * as a read of their own. The visible range decides which windows the chain keeps; for a range far from the
 * chain's rows, the reader starts the chain over at the key the list has there.
 */
internal abstract class ChainBrowserPageStore<Parameter, Row>(
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
    private var reader by mutableStateOf<ListChainReader<Row>?>(null)
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

    /** The artist headings of the album grid grouped by artist, from their own read. */
    var sections by mutableStateOf(emptyList<BridgeLibraryBrowseSection>())
        private set

    private var parameter: Parameter? = null
    private var active = false
    private var generation = 0
    private var sectionDeliveries: ScreenSubscription<List<BridgeLibraryBrowseSection>>? = null

    /** Whether the list's count and its first window have both returned since [activate]. */
    private var firstReadReturned by mutableStateOf(false)

    /** The row at [index], or null when no window holds it. */
    fun rowAt(index: Int): Row? {
        for (run in reader?.chain?.placed.orEmpty()) {
            if (index >= run.start && index < run.start + run.rows.size) return run.rows[index - run.start]
        }
        return null
    }

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
        val opening = ListChainReader(scope, opened, { changed(current) }, { failed(it, current) })
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
        opening.start()
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
