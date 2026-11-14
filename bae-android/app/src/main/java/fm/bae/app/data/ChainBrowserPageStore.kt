package fm.bae.app.data

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.R
import fm.bae.app.diagnosticDetail
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeListCount
import uniffi.bae_bridge.BridgeListCountExtra
import uniffi.bae_bridge.BridgeSortValue

private val logger = BaeLogger("bae.ChainBrowserPageStore")

/**
 * The reads of one list, which every list shares: each window of it is a live read of its own, its
 * count and the position of a key in it are others, and a jump to a position is a plain read of the key
 * there.
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
 * reads of their own. The visible range decides which windows the chain keeps; a range far from the
 * chain's rows is a jump, which starts the chain over at the key the list has there.
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
    /** How many distinct items the list's count carries; its row count unless a row can show an item more than once. */
    var itemCount by mutableStateOf<Int?>(null)
        private set
    override var totalCount by mutableStateOf(0)
        private set
    override var loading by mutableStateOf(true)
        private set
    override var error by mutableStateOf<PageError?>(null)
        private set
    override val hasRows: Boolean get() = chain?.placed?.any { it.rows.isNotEmpty() } == true

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
    private var countAnswered = false
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
        totalCount = 0
        itemCount = null
        sections = emptyList()
        loading = true
        error = null
        countAnswered = false
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

    /** Stop reading [parameter]'s list, unless another has been activated since. */
    fun deactivate(parameter: Parameter) {
        if (this.parameter == parameter) deactivate()
    }

    fun deactivate() {
        active = false
        close()
        totalCount = 0
        itemCount = null
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
                val known = if (countAnswered) maxOf(totalCount - 1, 0) else position
                chain.restart(key, minOf(position, known))
            }
    }

    private fun counted(
        count: BridgeListCount,
        openedGeneration: Int,
    ) {
        if (openedGeneration != generation || !active) return
        totalCount = count.rows.toInt()
        itemCount = (count.extra as? BridgeListCountExtra.AlbumGrid)?.albums?.toInt() ?: totalCount
        countAnswered = true
        changed(openedGeneration)
    }

    private fun changed(openedGeneration: Int) {
        if (openedGeneration != generation || !active) return
        if (countAnswered && chain?.hasAnswered == true) {
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
        val hadRows = hasRows
        close()
        loading = false
        // This app words the headline; core's diagnostic stays behind it for the disclosure.
        error =
            PageError(
                DisplayError(
                    appContext.getString(if (hadRows) loadMoreFailed else loadFailed),
                    value.diagnosticDetail,
                    // A cancellation, which core shows nothing for, rules nothing out.
                    errors.display(value)?.offersRetry != false,
                ),
                ::retry,
            )
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
