package fm.bae.app.ui.playback

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.playback.QueueContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueLane

/** The two lanes of the queue. */
internal enum class OrderLane {
    MANUAL,
    CONTEXT,
    ;

    /** The lane as core names it. */
    val bridgeLane: BridgeQueueLane
        get() =
            when (this) {
                MANUAL -> BridgeQueueLane.MANUAL
                CONTEXT -> BridgeQueueLane.CONTEXT
            }
}

/**
 * The lanes as the sheet shows them: core's read of each, with the moves of a drag laid over a lane
 * until the reorder the drag sent returns. A lane's moves are held while the drag runs and while its
 * reorder is in flight. When the reorder returns, moved or not, the lane shows core's read again, which
 * shows what is stored.
 */
internal class QueueOrder {
    val manual = mutableStateListOf<BridgeQueueEntry>()
    val context = ContextLaneOrder()

    val isEmpty: Boolean
        get() = manual.isEmpty() && context.size == 0

    /** Whether a drag is moving rows now. */
    private var dragging = false

    /** How many reorders each held lane has in flight, counting the drag in progress. */
    private val holds = mutableMapOf<OrderLane, Int>()

    /** Where the drag's last step left its row, which its release sends core. */
    private var pending: LaneDrop? = null

    /** The last value [apply] took, which a reorder's return or the end of a drag shows again. */
    private var latest: QueueValue? = null

    /** The row a drag holds: its lane, its entry, and its place in the lane now. */
    private var held: HeldRow? = null

    /** A drag took hold of the row [entryId], drawn at [index] of [lane]. */
    fun started(
        lane: OrderLane,
        entryId: String,
        index: Int,
    ) {
        held = HeldRow(lane, entryId, index)
    }

    /**
     * Move the held row [by] places in its lane, as a step of a drag does, and answer where it then sits.
     * A step that would take the row out of its lane (over a section header or into the other lane) moves
     * nothing and answers null. Core is told nothing until the drag ends: [released] answers the one
     * reorder the drag's steps made, and the lane is held from the first step so core's read can't undo
     * them before then.
     */
    fun drop(by: Int): LaneDrop? {
        val row = held?.takeIf { by != 0 && row(it.index + by, it.lane) } ?: return null
        val to = row.index + by
        when (row.lane) {
            OrderLane.MANUAL -> manual.add(to, manual.removeAt(row.index))
            OrderLane.CONTEXT -> context.move(row.index, to)
        }
        row.index = to
        val drop = LaneDrop(row.lane, row.entryId, to)
        if (pending?.lane != drop.lane) moveSent(drop.lane)
        pending = drop
        return drop
    }

    /** Whether [lane] has a row at [index]. */
    private fun row(
        index: Int,
        lane: OrderLane,
    ): Boolean = index in 0 until if (lane == OrderLane.MANUAL) manual.size else context.size

    /** The drag ended: the reorder to send core, where the dragged row now sits, or null when it moved nothing. */
    fun released(): LaneDrop? {
        held = null
        return pending.also { pending = null }
    }

    /** A drag is reordering the lanes; values wait until it ends. */
    fun dragging() {
        dragging = true
    }

    /** A reorder of a row of [lane] is held until it returns: the drag's first step makes it. */
    private fun moveSent(lane: OrderLane) {
        holds[lane] = (holds[lane] ?: 0) + 1
    }

    /**
     * A reorder of a row of [lane] returned, whether it moved the row, moved nothing or failed: the lane
     * shows core's read, unless a later drag's reorder is still in flight, which keeps its own moves held.
     */
    fun reorderReturned(lane: OrderLane) {
        val inFlight = holds[lane] ?: return
        if (inFlight > 1) {
            holds[lane] = inFlight - 1
            return
        }
        holds.remove(lane)
        show()
    }

    /** Take the queue. A lane shows core's read whenever no reorder holds it. */
    fun apply(
        manual: List<BridgeQueueEntry>,
        context: QueueContext?,
    ) {
        dragging = false
        latest = QueueValue(manual, context)
        show()
    }

    private fun show() {
        val value = latest ?: return
        if (dragging) return
        for (lane in OrderLane.entries) {
            if (lane !in holds) reseed(lane, value)
        }
    }

    private fun reseed(
        lane: OrderLane,
        value: QueueValue,
    ) {
        when (lane) {
            OrderLane.MANUAL -> {
                if (manual.toList() != value.manual) {
                    manual.clear()
                    manual.addAll(value.manual)
                }
            }

            OrderLane.CONTEXT -> {
                context.seed(value.context)
            }
        }
    }
}

/** One value of the queue as [QueueOrder.apply] took it. */
private class QueueValue(
    val manual: List<BridgeQueueEntry>,
    val context: QueueContext?,
)

/**
 * The context lane as the sheet shows it: the queue's context, sized to its whole tail and read
 * per row, with the moves of the drag in progress laid over it. A row that is not read yet is
 * null.
 */
internal class ContextLaneOrder {
    private var source by mutableStateOf<QueueContext?>(null)

    /** Each move of the drag in progress, in order, by displayed position. */
    private val moves = mutableStateListOf<LaneMove>()

    val size: Int
        get() = source?.upcomingTotal ?: 0

    /** What the context plays from, which labels its section; null when nothing plays from a context. */
    val kind: BridgePlaybackSourceKind?
        get() = source?.kind

    /** Show [context] from its first row, with no moves over it. */
    fun seed(context: QueueContext?) {
        source = context
        moves.clear()
    }

    /** The row shown at [index], or null while it is not read. */
    fun itemAt(index: Int): BridgeQueueEntry? = source?.itemAt(readIndexAt(index))

    /** The tail index of the row shown at [index], undoing the drag's moves. */
    private fun readIndexAt(index: Int): Int = moves.asReversed().fold(index) { at, move -> move.sourceOf(at) }

    /** Move the row shown at [from] to [to], as a drag does; core finds the row it then sits before. */
    fun move(
        from: Int,
        to: Int,
    ) {
        moves.add(LaneMove(from, to))
    }
}

/** A row a drag moved, in [lane], and the [position] among the lane's rows it then sits at, as core is told it. */
internal data class LaneDrop(
    val lane: OrderLane,
    val entryId: String,
    val position: Int,
)

/** The row a drag holds, in [lane], and its [index] among the lane's rows as the sheet draws them now. */
private class HeldRow(
    val lane: OrderLane,
    val entryId: String,
    var index: Int,
)

/** One row taken out at [from] and put back at [to], both positions in the lane as it was shown. */
internal data class LaneMove(
    val from: Int,
    val to: Int,
) {
    /** The position, before this move, of the row shown at [index] after it. */
    fun sourceOf(index: Int): Int =
        when {
            index == to -> from
            from < to && index in from until to -> index + 1
            to < from && index in (to + 1)..from -> index - 1
            else -> index
        }
}
