package fm.bae.app.playback

import fm.bae.app.BaeLogger
import fm.bae.app.data.Library
import fm.bae.app.data.ListReads
import fm.bae.app.data.OpenList
import fm.bae.app.data.answer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeList
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgeQueueEntry

private val logger = BaeLogger("bae.QueueUpcoming")

/** Opens the reads of the context's upcoming tracks as a list, in the order the lane reads in: its own, or the shuffled
 *  one when `shuffled`. */
class QueueUpcomingSource internal constructor(
    internal val reads: (shuffled: Boolean) -> ListReads<BridgeQueueEntry>,
) {
    constructor(handle: AppHandleInterface) : this(reads = { shuffled -> Library(handle).upcomingList(shuffled) })
}

/** The context's tracks after the playing one, in the order the lane reads in. */
private fun Library.upcomingList(shuffled: Boolean): ListReads<BridgeQueueEntry> =
    listReads(BridgeList.Upcoming(shuffled)) { rows ->
        (rows as? BridgeListRows.Upcoming)?.rows ?: error("the upcoming list delivered rows of another list")
    }

/**
 * The context's tracks after the playing one, read as an [OpenList] in the order the lane reads in: one view whose
 * window core keeps around the rows on screen, and a count. The rows on screen are the places held with [hold], and
 * the tail reports them to the view. The tail calls
 * [onChange] after each delivery. When the order changes, the list keeps showing the rows of the old order until
 * the new order's count and first rows arrive. A failed read ends the list, and the tail keeps its error in [failure]
 * until [retry] opens the list again.
 */
internal class UpcomingTail(
    private val source: QueueUpcomingSource,
    private val scope: CoroutineScope,
    private val onChange: () -> Unit,
) {
    private val mutableFailure = MutableStateFlow<BridgeException?>(null)

    /** Why the list stopped, until [retry] opens it again. */
    val failure: StateFlow<BridgeException?> = mutableFailure.asStateFlow()

    /** The reads whose rows the tail shows; null while the list is closed. */
    private var shown: OrderReads? = null

    /**
     * The reads of the order the list changed to, until their count and first rows arrive; [shown] stays until
     * then.
     */
    private var opening: OrderReads? = null

    /** The tail's length as the shown count last said it; 0 until the count delivers and after it fails. */
    val total: Int
        get() =
            shown
                ?.list
                ?.count
                ?.answer
                ?.rows
                ?.toInt() ?: 0

    /** How many holds each place has: the rows on screen. */
    private val holds = mutableMapOf<Int, Int>()

    /** The rows the shown view's window holds, by index in the tail; none after the view's read failed. */
    fun entries(): Map<Int, BridgeQueueEntry> =
        shown
            ?.list
            ?.view
            ?.answer
            ?.let { window -> window.rows.mapIndexed { i, entry -> window.start + i to entry } }
            .orEmpty()
            .toMap()

    /**
     * Read the tail in `shuffled`'s order. A list open in the other order shows its rows until the new order
     * answers.
     */
    fun open(shuffled: Boolean) {
        if (mutableFailure.value == null) {
            if (opening?.shuffled == shuffled) return
            if (shown?.shuffled == shuffled) {
                if (opening != null) dropOpening()
                return
            }
        }
        start(shuffled)
    }

    /** Keep the row at [index] read until the returned release runs. */
    fun hold(index: Int): () -> Unit {
        holds[index] = (holds[index] ?: 0) + 1
        follow()
        var released = false
        return {
            if (!released) {
                released = true
                val left = checkNotNull(holds[index]) { "released a hold never taken" } - 1
                if (left > 0) holds[index] = left else holds.remove(index)
                follow()
            }
        }
    }

    /** Open the list again after a failure stopped it. */
    fun retry() {
        val order = (opening ?: shown)?.shuffled ?: return
        start(order)
    }

    /** Stop reading and forget the rows. */
    fun close() {
        opening?.stop()
        opening = null
        shown?.stop()
        shown = null
        mutableFailure.value = null
    }

    /** Open the reads of `shuffled`'s order: the shown ones when none are, else the ones the list changes to. */
    private fun start(shuffled: Boolean) {
        opening?.stop()
        opening = null
        mutableFailure.value = null
        val reads = OrderReads(shuffled)
        if (shown == null) shown = reads else opening = reads
        val first = holds.keys.minOrNull() ?: 0
        reads.list.start(first, holds.keys.maxOrNull() ?: first)
    }

    private fun dropOpening() {
        opening?.stop()
        opening = null
        follow()
    }

    /** The reads that follow the rows on screen: those the list changes to, else the shown ones. */
    private fun following(): OrderReads? = opening ?: shown

    /** The rows on screen changed: tell the view, which core keeps its window around. */
    private fun follow() {
        val reads = following() ?: return
        val first = holds.keys.minOrNull() ?: return
        reads.list.show(first, holds.keys.max())
    }

    private fun changed(reads: OrderReads) {
        if (!reads.current) return
        answered(reads)
    }

    /**
     * [reads] delivered: the shown ones redraw, and the ones the list changes to replace them once their count
     * and their view have both answered.
     */
    private fun answered(reads: OrderReads) {
        if (reads === opening) {
            if (!reads.list.answered) return
            shown?.stop()
            shown = reads
            opening = null
        }
        onChange()
    }

    private fun failed(
        error: BridgeException,
        reads: OrderReads,
    ) {
        if (!reads.current) return
        logger.error("upcoming queue read failed", error)
        opening?.stop()
        opening = null
        shown?.stop()
        mutableFailure.value = error
        onChange()
    }

    /** The reads of the tail in one order. */
    private inner class OrderReads(
        val shuffled: Boolean,
    ) {
        val list = OpenList(scope, source.reads(shuffled), { changed(this) }, { failed(it, this) })

        /** Whether the tail still uses these reads: they are the shown ones or the ones the list changes to. */
        val current: Boolean get() = this === shown || this === opening

        /** End every read; the rows the view last delivered stay. */
        fun stop() = list.stop()
    }
}
