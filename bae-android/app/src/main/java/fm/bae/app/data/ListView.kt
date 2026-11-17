package fm.bae.app.data

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeListCount

/** One delivery of a list's view: the rows of the window core keeps around the rows on screen, the first at [start]. */
internal data class ListViewDelivery<Row>(
    val start: Int,
    val rows: List<Row>,
) {
    /** The row at [position], or null when the window doesn't hold it. */
    fun rowAt(position: Int): Row? = rows.getOrNull(position - start)

    /** The rows from [offset], at most [limit] of them, that the window holds. */
    fun page(
        offset: Int,
        limit: Int,
    ): List<Row> = rows.drop((offset - start).coerceAtLeast(0)).take(limit)
}

/**
 * A live read of one list's view: core keeps one window of the list around the rows on screen, which [setVisible]
 * reports.
 */
internal interface ListViewRead<Row> : PulledSubscription<ListViewDelivery<Row>> {
    fun setVisible(
        first: Int,
        last: Int,
    )
}

/** The reads of one list, the same for every list: its view, opened with the rows on screen, and its count. */
internal class ListReads<Row>(
    val view: (first: Int, last: Int) -> ListViewRead<Row>,
    val count: () -> PulledSubscription<BridgeListCount>,
    /** The album grid grouped by artist: each artist's heading with where its albums start. Other lists have none. */
    val sections: (() -> PulledSubscription<List<BridgeLibraryBrowseSection>>)? = null,
)

/**
 * One open list: its view and its count, each a live read. The list reports the rows on screen to core, which
 * moves and sizes the window it reads around them. The list calls [onChange] after each delivery of the count or
 * of the view, and [onFailure] when either read fails. Its owner decides whether a call still counts, since a value
 * can arrive after the owner moved on to other reads.
 */
internal class OpenList<Row>(
    private val scope: CoroutineScope,
    private val reads: ListReads<Row>,
    private val onChange: () -> Unit,
    private val onFailure: (BridgeException) -> Unit,
) {
    /** The list's count from its last value; null until the count delivers. */
    var count by mutableStateOf<BridgeListCount?>(null)
        private set

    /** The view's last delivery; null until the view delivers. The rows stay after [stop]. */
    var delivered by mutableStateOf<ListViewDelivery<Row>?>(null)
        private set

    /** Whether the count and the view have both answered. */
    val answered: Boolean get() = count != null && delivered != null

    private var view: ListViewRead<Row>? = null
    private var shown: IntRange? = null
    private var countDeliveries: ScreenSubscription<BridgeListCount>? = null
    private var viewDeliveries: ScreenSubscription<ListViewDelivery<Row>>? = null
    private var stopped = false

    /** Open the count, and the view with the rows from [first] to [last] on screen. */
    fun start(
        first: Int,
        last: Int,
    ) {
        countDeliveries =
            ScreenSubscription(
                reads.count(),
                scope,
                onValue = {
                    count = it
                    onChange()
                },
                onError = onFailure,
            )
        val opened = reads.view(first, last)
        view = opened
        shown = first..last
        viewDeliveries =
            ScreenSubscription(
                opened,
                scope,
                onValue = {
                    delivered = it
                    onChange()
                },
                onError = onFailure,
            )
    }

    /** The rows from [first] to [last] are on screen now; the list tells core when they changed. */
    fun show(
        first: Int,
        last: Int,
    ) {
        if (stopped || first > last || shown == first..last) return
        shown = first..last
        view?.setVisible(first, last)
    }

    /** The row at [position], or null when the window doesn't hold it. */
    fun rowAt(position: Int): Row? = delivered?.rowAt(position)

    /** End every read. The rows the view last delivered stay in [delivered]. */
    fun stop() {
        stopped = true
        viewDeliveries?.stop()
        viewDeliveries = null
        countDeliveries?.stop()
        countDeliveries = null
    }
}
