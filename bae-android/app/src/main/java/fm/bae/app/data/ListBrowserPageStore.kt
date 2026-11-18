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
 * list's rows with the failure until every read answers with a value again.
 */
internal abstract class ListBrowserPageStore<Parameter, Row>(
    private val appContext: Context,
    /** Core's answer to whether a failed read is worth reading again. */
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
    /** The headline for a failed read, naming what this list holds. */
    @StringRes private val loadFailed: Int = R.string.library_load_failed,
) : BrowserPageState {
    /** The open list's reads; null until [activate] and after [deactivate]. */
    private var reader by mutableStateOf<OpenList<Row>?>(null)

    /**
     * How many rows the screen fits, from its height and its row height; the list opens its view with that many
     * rows on screen, before any row arrives.
     */
    var screenRows = 1

    /** How many rows the list's count last said it has; 0 until it answers and after it fails. */
    val totalCount: Int
        get() =
            reader
                ?.count
                ?.answer
                ?.rows
                ?.toInt() ?: 0

    /** A failed read, else the list's count once it answers, else waiting for it. */
    override val state: ListPageState
        get() {
            val list = reader ?: return ListPageState.Waiting
            (list.failure ?: (sectionsRead as? LiveQueryState.Failed)?.error)?.let { failure ->
                return ListPageState.Failed(PageError(appContext.getString(loadFailed), failure, errors, ::retry))
            }
            val count = list.count.answer ?: return ListPageState.Waiting
            return ListPageState.Counted(count.rows.toInt())
        }

    /** Whether the album grid's count says the grid groups its rows by artist; other lists never do. */
    val grouped: Boolean get() = (reader?.count?.answer?.extra as? BridgeListCountExtra.AlbumGrid)?.grouped == true

    /** The latest answer of the album grid's artist headings, a read of their own; null for a list without them. */
    private var sectionsRead by mutableStateOf<LiveQueryState<List<BridgeLibraryBrowseSection>>?>(null)

    /** The artist headings of the album grid grouped by artist; none until they answer and after they fail. */
    val sections: List<BridgeLibraryBrowseSection> get() = sectionsRead?.answer.orEmpty()

    private var parameter: Parameter? = null
    private var active = false
    private var generation = 0
    private var sectionDeliveries: ScreenSubscription<List<BridgeLibraryBrowseSection>>? = null

    /** The row at [index], or null when the view's window doesn't hold it. */
    fun rowAt(index: Int): Row? = reader?.rowAt(index)

    fun activate(parameter: Parameter) {
        if (active && this.parameter == parameter) return
        close()
        sectionsRead = null
        this.parameter = parameter
        active = true
        generation++
        val opened = open(parameter)
        val current = generation
        val opening = OpenList(scope, opened, {}, { failed(it, current) })
        reader = opening
        sectionDeliveries =
            opened.sections?.let { open ->
                sectionsRead = LiveQueryState.Waiting
                ScreenSubscription(
                    open(),
                    scope,
                    onValue = {
                        if (current == generation && sectionsRead?.answer != it) {
                            sectionsRead = LiveQueryState.Answered(it)
                        }
                    },
                    onError = {
                        if (current == generation) sectionsRead = LiveQueryState.Failed(it)
                        failed(it, current)
                    },
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
        sectionsRead = null
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

    /** Log a failed read; [state] draws the failure from the read's latest answer. */
    private fun failed(
        value: BridgeException,
        openedGeneration: Int,
    ) {
        if (openedGeneration != generation || !active) return
        logger.error("browser page query failed", value)
    }

    private fun close() {
        reader?.stop()
        sectionDeliveries?.stop()
        sectionDeliveries = null
    }
}
