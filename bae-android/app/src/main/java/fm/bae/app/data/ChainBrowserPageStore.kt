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
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
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
 * One library list read as a chain of window reads (see [ListChain]), with its count and its headings as
 * reads of their own. The visible range decides which windows the chain keeps; for a range far from the
 * chain's rows, the store starts the chain over at the key the list has there.
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
    /** The list's count from its last value; null until the count delivers after [activate] opened the list. */
    private var count by mutableStateOf<BridgeListCount?>(null)
    override val totalCount: Int get() = count?.rows?.toInt() ?: 0
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
    private var reads: ListReads<Row>? = null
    private var chain: ListChain<Row>? = null
    private var countDeliveries: ScreenSubscription<BridgeListCount>? = null
    private var sectionDeliveries: ScreenSubscription<List<BridgeLibraryBrowseSection>>? = null

    /** Whether the list's count and its first window have both returned since [activate]. */
    private var firstReadReturned by mutableStateOf(false)
    private var jump: Job? = null

    /** The row at [index], or null when no window holds it. */
    fun rowAt(index: Int): Row? {
        for (run in chain?.placed.orEmpty()) {
            if (index >= run.start && index < run.start + run.rows.size) return run.rows[index - run.start]
        }
        return null
    }

    fun activate(parameter: Parameter) {
        if (active && this.parameter == parameter) return
        close()
        count = null
        sections = emptyList()
        loading = true
        error = null
        firstReadReturned = false
        this.parameter = parameter
        active = true
        generation++
        val opened = open(parameter)
        reads = opened
        val current = generation
        val opening =
            ListChain(scope, opened, BROWSER_PAGE_SIZE, { changed(current) }, { failed(it, current) })
        chain = opening
        countDeliveries =
            ScreenSubscription(
                opened.count(),
                scope,
                onValue = { counted(it, current) },
                onError = { failed(it, current) },
            )
        sectionDeliveries =
            opened.sections?.let { open ->
                ScreenSubscription(
                    open(),
                    scope,
                    onValue = { if (current == generation && it != sections) sections = it },
                    onError = { failed(it, current) },
                )
            }
        opening.restart(null, 0)
    }

    /** Stop reading [parameter]'s list, unless the caller has activated another list since. */
    fun deactivate(parameter: Parameter) {
        if (this.parameter == parameter) deactivate()
    }

    fun deactivate() {
        active = false
        close()
        count = null
        sections = emptyList()
    }

    fun reportVisibleRange(
        first: Int,
        last: Int,
    ) {
        val chain = chain ?: return
        if (!active || first > last || jump != null) return
        chain.visible = first..last
        if (chain.isEmpty || !chain.reaches(first) || !chain.reaches(last)) {
            jumpTo(first)
        } else {
            chain.reconcile()
        }
    }

    fun retry() {
        val current = checkNotNull(parameter) { "browser page retry without parameters" }
        active = false
        activate(current)
    }

    protected abstract fun open(parameter: Parameter): ListReads<Row>

    /** Start the chain over at the row the list has at [position], which is a plain read of its key, not a live one. */
    private fun jumpTo(position: Int) {
        val chain = chain ?: return
        val current = generation
        val keyAt = reads?.keyAt ?: return
        chain.stop()
        jump =
            scope.launch {
                val key =
                    try {
                        keyAt(position.toULong())
                    } catch (value: BridgeException) {
                        if (current == generation) {
                            jump = null
                            failed(value, current)
                        }
                        return@launch
                    }
                if (current != generation || !active) return@launch
                jump = null
                // A position past the list's end has the last row's key, which sits at the last position.
                val known = count?.let { maxOf(it.rows.toInt() - 1, 0) } ?: position
                chain.restart(key, minOf(position, known))
            }
    }

    private fun counted(
        count: BridgeListCount,
        openedGeneration: Int,
    ) {
        if (openedGeneration != generation || !active) return
        this.count = count
        changed(openedGeneration)
    }

    private fun changed(openedGeneration: Int) {
        if (openedGeneration != generation || !active) return
        if (count != null && chain?.hasAnswered == true) {
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
        jump?.cancel()
        jump = null
        chain?.stop()
        countDeliveries?.stop()
        countDeliveries = null
        sectionDeliveries?.stop()
        sectionDeliveries = null
    }
}
