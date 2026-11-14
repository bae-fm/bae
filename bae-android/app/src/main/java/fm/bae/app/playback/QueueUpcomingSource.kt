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
 * a place far from the chain's rows is a jump, which starts the chain over at the key the list has there. [onChange]
 * hears each delivery. A failed read ends the list and is held in [failure] until [retry] opens it again.
 */
internal class UpcomingTail(
    private val source: QueueUpcomingSource,
    private val scope: CoroutineScope,
    private val onChange: () -> Unit,
) {
    private val mutableFailure = MutableStateFlow<BridgeException?>(null)

    /** Why the list stopped, until [retry] opens it again. */
    val failure: StateFlow<BridgeException?> = mutableFailure.asStateFlow()

    /** The tail's length as the count last said it; 0 until it answers. */
    var total = 0
        private set

    /** The order the list is open in; null while it is closed. */
    private var shuffled: Boolean? = null
    private var generation = 0
    private var reads: ListReads<BridgeQueueEntry>? = null
    private var chain: ListChain<BridgeQueueEntry>? = null
    private var countDeliveries: ScreenSubscription<BridgeListCount>? = null
    private var countAnswered = false
    private var jump: Job? = null

    /** How many holds each place has: the rows on screen. */
    private val holds = mutableMapOf<Int, Int>()

    /** The rows the chain holds, by index in the tail. */
    fun entries(): Map<Int, BridgeQueueEntry> =
        chain
            ?.placed
            .orEmpty()
            .flatMap { run -> run.rows.mapIndexed { i, entry -> run.start + i to entry } }
            .toMap()

    /** Read the tail in `shuffled`'s order, starting over when it is open in the other one. */
    fun open(shuffled: Boolean) {
        if (this.shuffled == shuffled && mutableFailure.value == null) return
        stopReads()
        mutableFailure.value = null
        this.shuffled = shuffled
        generation++
        val current = generation
        val opened = source.reads(shuffled)
        reads = opened
        countAnswered = false
        val opening = ListChain(scope, opened, BROWSER_PAGE_SIZE, { changed(current) }, { failed(it, current) })
        chain = opening
        countDeliveries =
            ScreenSubscription(
                opened.count(),
                scope,
                onValue = { counted(it, current) },
                onError = { failed(it, current) },
            )
        opening.restart(null, 0)
        follow()
    }

    /** Keep the row at [index] read until the returned release runs. */
    fun hold(index: Int): () -> Unit {
        holds[index] = (holds[index] ?: 0) + 1
        follow()
        var released = false
        return {
            if (!released) {
                released = true
                val left = (holds[index] ?: 1) - 1
                if (left > 0) holds[index] = left else holds.remove(index)
                follow()
            }
        }
    }

    /** Open the list again after a failure stopped it. */
    fun retry() {
        val order = shuffled ?: return
        mutableFailure.value = null
        shuffled = null
        open(order)
    }

    /** Stop reading and forget the rows. */
    fun close() {
        stopReads()
        shuffled = null
        generation++
        chain = null
        reads = null
        total = 0
        mutableFailure.value = null
    }

    private fun stopReads() {
        jump?.cancel()
        jump = null
        chain?.stop()
        countDeliveries?.stop()
        countDeliveries = null
    }

    /** The rows on screen changed: the chain grows towards them, or starts over from them when they are far off. */
    private fun follow() {
        val chain = chain
        val first = holds.keys.minOrNull()
        if (chain != null && first != null && jump == null) {
            val last = holds.keys.max()
            chain.visible = first..last
            if (chain.isEmpty || !chain.reaches(first) || !chain.reaches(last)) {
                jumpTo(first)
            } else {
                chain.reconcile()
            }
        }
    }

    /** Start the chain over at the row the list has at [position], a plain read of its key, not a live one. */
    private fun jumpTo(position: Int) {
        val chain = chain ?: return
        val keyAt = reads?.keyAt ?: return
        val current = generation
        chain.stop()
        jump =
            scope.launch {
                val key =
                    try {
                        keyAt(position.toULong())
                    } catch (error: BridgeException) {
                        if (current == generation) {
                            jump = null
                            failed(error, current)
                        }
                        return@launch
                    }
                if (current != generation) return@launch
                jump = null
                // A position past the list's end has the last row's key, which sits at the last position.
                val known = if (countAnswered) maxOf(total - 1, 0) else position
                chain.restart(key, minOf(position, known))
            }
    }

    private fun counted(
        count: BridgeListCount,
        openedGeneration: Int,
    ) {
        if (openedGeneration != generation) return
        total = count.rows.toInt()
        countAnswered = true
        onChange()
    }

    private fun changed(openedGeneration: Int) {
        if (openedGeneration != generation) return
        onChange()
    }

    private fun failed(
        error: BridgeException,
        openedGeneration: Int,
    ) {
        if (openedGeneration != generation) return
        logger.error("upcoming queue read failed", error)
        stopReads()
        mutableFailure.value = error
        onChange()
    }
}
