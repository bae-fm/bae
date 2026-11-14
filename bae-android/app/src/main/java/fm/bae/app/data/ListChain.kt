package fm.bae.app.data

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListPosition
import uniffi.bae_bridge.BridgeListWindow
import uniffi.bae_bridge.BridgeSortValue

/** One delivery of a list window: the rows of the range it answers, with the keys that chain it to its neighbours. */
internal data class ListWindowDelivery<Row>(
    /** The request this answers; a delivery for a window the chain has since moved draws nothing. */
    val window: BridgeListWindow,
    val rows: List<Row>,
    /** The sort values of the first row; null for a window of no rows. */
    val first: List<BridgeSortValue>?,
    /** The sort values of the row past the window's far end; null when it reached its bound, the list's end or top. */
    val beyond: List<BridgeSortValue>?,
)

/** A live read of one window of a list, moved in place with [setWindow]. */
internal interface ListWindowRead<Row> : PulledSubscription<ListWindowDelivery<Row>> {
    fun setWindow(window: BridgeListWindow)
}

/** A live read of how many rows come before the row at a key, moved in place with [setKey]. */
internal interface ListPositionRead : PulledSubscription<BridgeListPosition> {
    fun setKey(key: List<BridgeSortValue>)
}

/** The rows of one window, with the position of the first. */
internal data class Placed<Row>(
    val start: Int,
    val rows: List<Row>,
)

private class ChainWindow<Row>(
    var request: BridgeListWindow,
    val read: ListWindowRead<Row>,
) {
    val forward: BridgeListWindow.Forward? get() = request as? BridgeListWindow.Forward
    var rows = emptyList<Row>()
    var first: List<BridgeSortValue>? = null
    var beyond: List<BridgeSortValue>? = null

    /**
     * Whether any delivery has landed; a window moved to a new request keeps its last delivery until the new
     * one replaces it.
     */
    var answered = false
    var deliveries: ScreenSubscription<ListWindowDelivery<Row>>? = null

    /** Read [to] from now on; the failure to send it, if any. */
    fun move(to: BridgeListWindow): BridgeException? {
        request = to
        return try {
            read.setWindow(to)
            null
        } catch (error: BridgeException) {
            error
        }
    }

    fun stop() {
        deliveries?.stop()
        deliveries = null
    }
}

/**
 * Where a chain's first row sits in its list: the key of that row and the count of the rows before it, a
 * live read of its own that follows the key. The list's top has no key and sits at zero; until a count
 * answers for a key, the position it was given stands in.
 */
private class StartPosition(
    private val scope: CoroutineScope,
    private val open: (List<BridgeSortValue>) -> ListPositionRead,
    private val onChange: () -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    var key: List<BridgeSortValue>? = null
        private set
    var value = 0
        private set
    private var read: ListPositionRead? = null
    private var deliveries: ScreenSubscription<BridgeListPosition>? = null

    fun follow(
        key: List<BridgeSortValue>?,
        assuming: Int,
    ) {
        this.key = key
        value = if (key == null) 0 else assuming
        if (key == null) {
            stop()
            return
        }
        val current = read
        if (current != null) {
            try {
                current.setKey(key)
            } catch (error: BridgeException) {
                onError(error)
            }
            return
        }
        val opened = open(key)
        read = opened
        deliveries = ScreenSubscription(opened, scope, onValue = ::apply, onError = onError)
    }

    fun stop() {
        deliveries?.stop()
        deliveries = null
        read = null
    }

    private fun apply(position: BridgeListPosition) {
        // A value for a key the chain has since left draws nothing.
        if (position.key != key) return
        val rowsBefore = position.rowsBefore.toInt()
        if (rowsBefore == value) return
        value = rowsBefore
        onChange()
    }
}

/**
 * The windows a list reads, in the order of its rows: each a live read of a range of the order,
 * neighbours sharing a bound so every row is held once. A window owns its subscription and only its last
 * delivery, replaced whole; [placed] lays those deliveries end to end from the position of the chain's
 * first row.
 *
 * The chain fits itself to what it delivers: an open window is closed at the row past its limit, a closed
 * window that rows were added to is split there, a window that went empty is dropped into its neighbour.
 * It grows one window below or above and drops windows well beyond the rows on screen, which [visible]
 * names.
 */
internal class ListChain<Row>(
    private val scope: CoroutineScope,
    private val reads: ListReads<Row>,
    private val pageSize: Int,
    private val onChange: () -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    /** The rows each window last delivered, laid end to end. */
    var placed by mutableStateOf(emptyList<Placed<Row>>())
        private set

    /** The positions on screen, which the chain stays around. */
    var visible: IntRange? = null

    /** Whether any window has delivered since the chain started. */
    var hasAnswered = false
        private set

    private val windows = mutableListOf<ChainWindow<Row>>()
    private val start = StartPosition(scope, reads.position, ::startMoved, onError)

    val isEmpty: Boolean get() = windows.isEmpty()

    private val end: Int get() = placed.lastOrNull()?.let { it.start + it.rows.size } ?: start.value

    /**
     * Whether the row at [position] is near enough the chain's rows for the chain to grow to it; a
     * position farther is a jump. The last window of the list, once answered, reaches every position past it.
     */
    fun reaches(position: Int): Boolean {
        val near = 2 * pageSize
        val low = if (start.key == null) Int.MIN_VALUE else start.value - near
        val last = windows.lastOrNull()
        val high =
            if (last != null && last.answered && (last.request as? BridgeListWindow.Forward)?.until == null) {
                Int.MAX_VALUE
            } else {
                end + near
            }
        return position in low..high
    }

    /**
     * Replace the windows with one starting at [key] (the list's top when null), [position] standing in for
     * its count.
     */
    fun restart(
        key: List<BridgeSortValue>?,
        position: Int,
    ) {
        windows.forEach(ChainWindow<Row>::stop)
        windows.clear()
        hasAnswered = false
        start.follow(key, position)
        open(BridgeListWindow.Forward(key, null, pageSize.toULong()))
        layOut()
    }

    /** End every read; the rows the last deliveries held stay in [placed]. */
    fun stop() {
        windows.forEach(ChainWindow<Row>::stop)
        windows.clear()
        start.stop()
    }

    /** Add a window reading [request] at [index] and start its reads, once it is in the chain to be delivered to. */
    private fun open(
        request: BridgeListWindow,
        index: Int = windows.size,
    ) {
        val window = ChainWindow(request, reads.window(request))
        windows.add(index, window)
        window.deliveries =
            ScreenSubscription(window.read, scope, onValue = { deliver(it, window) }, onError = onError)
    }

    private fun remove(index: Int) {
        windows[index].stop()
        windows.removeAt(index)
    }

    private fun move(
        window: ChainWindow<Row>,
        to: BridgeListWindow,
    ) {
        window.move(to)?.let(onError)
    }

    private fun startMoved() {
        layOut()
        reconcile()
        onChange()
    }

    private fun deliver(
        delivery: ListWindowDelivery<Row>,
        window: ChainWindow<Row>,
    ) {
        // A value for a window the chain has since moved or dropped draws nothing.
        if (window !in windows || delivery.window != window.request) return
        window.rows = delivery.rows
        window.first = delivery.first
        window.beyond = delivery.beyond
        window.answered = true
        hasAnswered = true
        settle(window)
        layOut()
        reconcile()
        onChange()
    }

    /**
     * Fit the chain to what [window] just delivered: close an open window, split one holding more than it
     * asked for, drop one gone empty.
     */
    private fun settle(window: ChainWindow<Row>) {
        val index = windows.indexOf(window)
        when (val request = window.request) {
            is BridgeListWindow.Forward -> {
                val beyond = window.beyond
                if (beyond != null) {
                    // Rows past the limit: the window ends where its rows do. A closed window also gives the
                    // rest of its range to a new window below it.
                    window.beyond = null
                    move(window, BridgeListWindow.Forward(request.from, beyond, request.limit))
                    if (request.until != null) {
                        open(BridgeListWindow.Forward(beyond, request.until, request.limit), index + 1)
                    }
                } else if (window.rows.isEmpty() && request.until != null && windows.size > 1) {
                    dropEmpty(index, request)
                }
            }

            is BridgeListWindow.Backward -> {
                if (window.rows.isEmpty()) {
                    if (windows.size > 1) remove(index)
                    return
                }
                // The rows above the chain's first row: the chain now starts at them, at the list's top when
                // nothing lies above them.
                val from = if (window.beyond == null) null else window.first
                window.beyond = null
                move(window, BridgeListWindow.Forward(from, request.until, request.limit))
                start.follow(from, maxOf(0, start.value - window.rows.size))
            }
        }
    }

    /**
     * Drop a window holding no rows: the window above takes its range's end, or the window below takes its
     * start when it is the first.
     */
    private fun dropEmpty(
        index: Int,
        request: BridgeListWindow.Forward,
    ) {
        if (index > 0) {
            val above = windows[index - 1]
            when (val aboveRequest = above.request) {
                is BridgeListWindow.Forward -> {
                    move(above, BridgeListWindow.Forward(aboveRequest.from, request.until, aboveRequest.limit))
                }

                is BridgeListWindow.Backward -> {
                    request.until?.let { move(above, BridgeListWindow.Backward(it, aboveRequest.limit)) }
                }
            }
        } else if (index + 1 < windows.size) {
            val below = windows[index + 1]
            val belowRequest = below.request
            if (belowRequest is BridgeListWindow.Forward) {
                move(below, BridgeListWindow.Forward(request.from, belowRequest.until, belowRequest.limit))
                start.follow(request.from, start.value)
            }
        }
        remove(index)
    }

    /**
     * Grow, trim and keep the chain around the rows on screen. A window is added below or above only when the
     * one next to it has answered, so its bound and its row count are known.
     */
    fun reconcile() {
        val visible = visible ?: return
        if (windows.isEmpty()) return
        belowKey(visible)?.let { open(BridgeListWindow.Forward(it, null, pageSize.toULong())) }
        aboveKey(visible)?.let { open(BridgeListWindow.Backward(it, pageSize.toULong()), 0) }
        trim(visible)
        layOut()
    }

    /** The key a new window below starts at: the last window is closed at its end and the rows on screen near it. */
    private fun belowKey(visible: IntRange): List<BridgeSortValue>? {
        val last = windows.last()
        val near = last.answered && end <= visible.last + pageSize
        return if (near) last.forward?.until else null
    }

    /** The key a new window above ends at, when the rows on screen near the chain's first row and rows lie above it. */
    private fun aboveKey(visible: IntRange): List<BridgeSortValue>? {
        val top = windows.first()
        val near = top.answered && start.value > 0 && start.value > visible.first - pageSize
        return if (near) top.forward?.from else null
    }

    /** Drop windows wholly beyond two pages from the rows on screen. */
    private fun trim(visible: IntRange) {
        layOut()
        val keepFrom = visible.first - 2 * pageSize
        val keepTo = visible.last + 2 * pageSize
        var position = start.value
        var trimmedTop = false
        while (topIsBefore(keepFrom, position)) {
            position += windows.first().rows.size
            remove(0)
            trimmedTop = true
        }
        val top = windows.firstOrNull()?.forward
        if (trimmedTop && top != null) start.follow(top.from, position)
        layOut()
        while (windows.size > 1 && placed.size == windows.size && placed.last().start > keepTo) {
            remove(windows.size - 1)
            layOut()
        }
    }

    /** Whether the first window is closed at its end and holds only rows before [keepFrom], standing at [position]. */
    private fun topIsBefore(
        keepFrom: Int,
        position: Int,
    ): Boolean {
        if (windows.size <= 1) return false
        val top = windows.first()
        return top.answered && top.forward?.until != null && position + top.rows.size <= keepFrom
    }

    /**
     * Lay the windows' last deliveries end to end from the chain's start position. A window that has not
     * answered yet ends the laying, except a first window reading upward, whose rows are not what the start
     * position stands at.
     */
    private fun layOut() {
        val first = windows.firstOrNull()
        val skipped = if (first != null && !first.answered && first.request is BridgeListWindow.Backward) 1 else 0
        var position = start.value
        val next =
            windows.drop(skipped).takeWhile { it.answered }.map { window ->
                Placed(position, window.rows).also { position += window.rows.size }
            }
        if (next != placed) placed = next
    }
}
