package fm.bae.app.playback

import fm.bae.app.BaeLogger
import fm.bae.app.data.BROWSER_PAGE_SIZE
import fm.bae.app.data.Library
import fm.bae.app.data.ListChain
import fm.bae.app.data.ListReads
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeList
import uniffi.bae_bridge.BridgeListCount
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgeQueueEntry

private val logger = BaeLogger("bae.QueueUpcoming")

/** How many of the context's tracks the Media3 playlist lists after Up Next: the head of the tail the player keeps read
 *  while a context plays. */
internal const val PLAYLIST_CONTEXT_TRACKS = 100

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
 * The context's tracks after the playing one, read as a list: a chain of windows over the order the lane reads in,
 * with its count a read of its own. The rows on screen are the places held with [hold], and the chain follows them;
 * for a place far from the chain's rows, the chain starts over at the key the list has there. [onChange] hears each
 * delivery. When the order changes, the list keeps showing the rows of the old order until the new
 * order's count and first rows arrive. A failed read ends the list, and the tail keeps its error in [failure] until
 * [retry] opens the list again.
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

    /** The tail's length as the shown count last said it; 0 until it answers. */
    val total: Int get() = shown?.total ?: 0

    /** How many holds each place has: the rows on screen. */
    private val holds = mutableMapOf<Int, Int>()

    /** The rows the shown chain holds, by index in the tail. */
    fun entries(): Map<Int, BridgeQueueEntry> =
        shown
            ?.chain
            ?.placed
            .orEmpty()
            .flatMap { run -> run.rows.mapIndexed { i, entry -> run.start + i to entry } }
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
        reads.start()
        follow()
    }

    private fun dropOpening() {
        opening?.stop()
        opening = null
        follow()
    }

    /** The reads that follow the rows on screen: those the list changes to, else the shown ones. */
    private fun following(): OrderReads? = opening ?: shown

    /** The rows on screen changed: the chain grows towards them, or starts over from them when they are far off. */
    private fun follow() {
        val reads = following() ?: return
        val first = holds.keys.minOrNull()
        if (first != null && reads.jump == null) {
            val last = holds.keys.max()
            val chain = reads.chain
            chain.visible = first..last
            if (chain.isEmpty || !chain.reaches(first) || !chain.reaches(last)) {
                jumpTo(reads, first)
            } else {
                chain.reconcile()
            }
        }
    }

    /** Start [reads]' chain over at the row the list has at [position], a plain read of its key, not a live one. */
    private fun jumpTo(
        reads: OrderReads,
        position: Int,
    ) {
        val keyAt = reads.reads.keyAt ?: return
        reads.chain.stop()
        reads.jump =
            scope.launch {
                val key =
                    try {
                        keyAt(position.toULong())
                    } catch (error: BridgeException) {
                        if (reads.current) {
                            reads.jump = null
                            failed(error, reads)
                        }
                        return@launch
                    }
                if (!reads.current) return@launch
                reads.jump = null
                // A position past the list's end has the last row's key, which sits at the last position.
                val known = if (reads.countAnswered) maxOf(reads.total - 1, 0) else position
                reads.chain.restart(key, minOf(position, known))
            }
    }

    private fun counted(
        count: BridgeListCount,
        reads: OrderReads,
    ) {
        if (!reads.current) return
        reads.total = count.rows.toInt()
        reads.countAnswered = true
        answered(reads)
    }

    private fun changed(reads: OrderReads) {
        if (!reads.current) return
        answered(reads)
    }

    /**
     * [reads] delivered: the shown ones redraw, and the ones the list changes to replace them once their count
     * and their chain have both answered.
     */
    private fun answered(reads: OrderReads) {
        if (reads === opening) {
            if (!reads.countAnswered || !reads.chain.hasAnswered) return
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

    /** The reads of the tail in one order: its chain of windows and its count. */
    private inner class OrderReads(
        val shuffled: Boolean,
    ) {
        val reads = source.reads(shuffled)
        val chain = ListChain(scope, reads, BROWSER_PAGE_SIZE, { changed(this) }, { failed(it, this) })
        var total = 0
        var countAnswered = false
        var jump: Job? = null
        private var countDeliveries: ScreenSubscription<BridgeListCount>? = null

        /** Whether the tail still uses these reads: they are the shown ones or the ones the list changes to. */
        val current: Boolean get() = this === shown || this === opening

        /** Start the count and the chain at the list's top, once the tail holds these reads. */
        fun start() {
            countDeliveries =
                ScreenSubscription(
                    reads.count(),
                    scope,
                    onValue = { counted(it, this) },
                    onError = { failed(it, this) },
                )
            chain.restart(null, 0)
        }

        /** End every read; the rows the chain last held stay. */
        fun stop() {
            jump?.cancel()
            jump = null
            chain.stop()
            countDeliveries?.stop()
            countDeliveries = null
        }
    }
}
