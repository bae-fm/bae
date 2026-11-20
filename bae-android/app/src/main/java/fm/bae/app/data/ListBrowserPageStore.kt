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
 * The screen reports the rows it shows, and core keeps the window it reads around them. A failed read replaces the
 * list's rows with the failure until every read answers with a value again. When the list's parameters change, such
 * as its sort, the page keeps drawing the reads it has until the new parameters' reads answer or fail.
 */
internal abstract class ListBrowserPageStore<Parameter, Row>(
    private val appContext: Context,
    /** Core's answer to whether a failed read is worth reading again. */
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
    /** The headline for a failed read, naming what this list holds. */
    @StringRes private val loadFailed: Int = R.string.library_load_failed,
) : BrowserPageState {
    /** The reads the page draws; null until [activate] and after [deactivate]. */
    private var shown by mutableStateOf<PageReads?>(null)

    /** The reads of the parameters the page moved to, until they answer or fail; [shown] stays drawn until then. */
    private var opening: PageReads? = null

    /**
     * How many rows the screen fits, from its height and its row height; the list opens its view with that many
     * rows on screen, before any row arrives.
     */
    var screenRows = 1

    /** How many rows the list's count last said it has; 0 until it answers and after it fails. */
    val totalCount: Int
        get() =
            shown
                ?.list
                ?.count
                ?.answer
                ?.rows
                ?.toInt() ?: 0

    /** A failed read, else the list's count once it answers, else waiting for it. */
    override val state: ListPageState
        get() {
            val reads = shown ?: return ListPageState.Waiting
            reads.failure?.let { failure ->
                return ListPageState.Failed(PageError(appContext.getString(loadFailed), failure, errors, ::retry))
            }
            val count = reads.list.count.answer ?: return ListPageState.Waiting
            return ListPageState.Counted(count.rows.toInt())
        }

    /** What the list's count carries besides the number of rows, such as History's choice of whose plays. */
    val countExtra: BridgeListCountExtra?
        get() =
            shown
                ?.list
                ?.count
                ?.answer
                ?.extra

    /** Whether the album grid's count says the grid groups its rows by artist; other lists never do. */
    val grouped: Boolean
        get() =
            (countExtra as? BridgeListCountExtra.AlbumGrid)?.grouped == true

    /** The artist headings of the album grid grouped by artist; none until they answer and after they fail. */
    val sections: List<BridgeLibraryBrowseSection> get() = shown?.sections?.answer.orEmpty()

    private var parameter: Parameter? = null
    private var active = false

    /** The row at [index], or null when the view's window doesn't hold it. */
    fun rowAt(index: Int): Row? = shown?.list?.rowAt(index)

    fun activate(parameter: Parameter) {
        if (active && this.parameter == parameter) return
        this.parameter = parameter
        active = true
        opening?.stop()
        opening = null
        val reads = PageReads(open(parameter))
        if (shown == null) shown = reads else opening = reads
        reads.start(0, maxOf(screenRows, 1) - 1)
    }

    /** Stop reading [parameter]'s list, unless the caller has activated another list since. */
    fun deactivate(parameter: Parameter) {
        if (this.parameter == parameter) deactivate()
    }

    fun deactivate() {
        active = false
        opening?.stop()
        opening = null
        shown?.stop()
        shown = null
    }

    fun reportVisibleRange(
        first: Int,
        last: Int,
    ) {
        if (active) (opening ?: shown)?.list?.show(first, last)
    }

    fun retry() {
        val current = checkNotNull(parameter) { "browser page retry without parameters" }
        active = false
        activate(current)
    }

    protected abstract fun open(parameter: Parameter): ListReads<Row>

    /** [reads] answered or failed: the reads the page moved to replace the drawn ones once each has done one. */
    private fun settled(reads: PageReads) {
        if (reads !== opening || !reads.settled) return
        shown?.stop()
        shown = reads
        opening = null
    }

    /** One set of parameters' reads: the list's view and count, and the album grid's headings when it has them. */
    private inner class PageReads(
        private val reads: ListReads<Row>,
    ) {
        val list = OpenList(scope, reads, { settled(this) }, ::failed)

        /** The latest answer of the album grid's artist headings, a read of their own; null for a list without them. */
        var sections by mutableStateOf<LiveQueryState<List<BridgeLibraryBrowseSection>>?>(null)
            private set
        private var sectionDeliveries: ScreenSubscription<List<BridgeLibraryBrowseSection>>? = null

        /** The failure of a read whose latest answer is one, while one stands. */
        val failure: BridgeException? get() = list.failure ?: (sections as? LiveQueryState.Failed)?.error

        /** Whether every read has answered, with a value or a failure. */
        val settled: Boolean
            get() = failure != null || (list.answered && sections !is LiveQueryState.Waiting)

        fun start(
            first: Int,
            last: Int,
        ) {
            sectionDeliveries =
                reads.sections?.let { open ->
                    sections = LiveQueryState.Waiting
                    ScreenSubscription(
                        open(),
                        scope,
                        onValue = {
                            if (sections?.answer != it) sections = LiveQueryState.Answered(it)
                            settled(this)
                        },
                        onError = {
                            sections = LiveQueryState.Failed(it)
                            failed(it)
                        },
                    )
                }
            list.start(first, last)
        }

        fun stop() {
            list.stop()
            sectionDeliveries?.stop()
            sectionDeliveries = null
        }

        private fun failed(value: BridgeException) {
            logger.error("browser page query failed", value)
            settled(this)
        }
    }
}
