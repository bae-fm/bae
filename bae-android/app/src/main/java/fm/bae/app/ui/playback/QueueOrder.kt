package fm.bae.app.ui.playback

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.playback.QueueContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueLane

private val logger = BaeLogger("bae.QueueOrder")

/**
 * The lanes as the sheet shows them: core's read of each, with the moves of a drag drawn over a lane
 * until the reorder the drag sent returns. QueueOrder keeps a lane's moves on screen while the drag runs
 * and until core returns its reorder. When the reorder returns, moved or not, the lane shows core's read
 * again, which shows the order the queue's tables hold.
 */
internal class QueueOrder {
    val manual = mutableStateListOf<BridgeQueueEntry>()
    val context = ContextLaneOrder()

    /** Whether a drag is moving rows now. */
    private var dragging = false

    /** How many of each lane's reorders have not returned yet, counting the drag in progress. */
    private val holds = mutableMapOf<BridgeQueueLane, Int>()

    /** The last value [apply] took, which a reorder's return or the end of a drag shows again. */
    private var latest: QueueValue? = null

    /** The row the person is dragging, or null when no drag runs. */
    private var drag: QueueDrag? = null

    /** Where the sheet draws each row it has on screen now, by entry id. */
    private val placed = mutableMapOf<String, RowPlace>()

    /** The sheet draws the row [entryId] at [place]. */
    fun placed(
        entryId: String,
        place: RowPlace,
    ) {
        placed[entryId] = place
    }

    /** The sheet no longer draws the row [entryId]. */
    fun unplaced(entryId: String) {
        placed.remove(entryId)
    }

    /** The person started dragging the row [entryId], drawn at [place]. */
    fun started(
        entryId: String,
        place: RowPlace,
    ) {
        drag = QueueDrag(LaneDrop(entryId, place))
    }

    /**
     * A step of the drag put the dragged row beside the row [onto]: move it to [onto]'s place and return
     * where it then sits. A step onto anything but a row of the dragged row's lane (a section header, a
     * row of the other lane, a row not read yet) moves nothing and returns null. The sheet tells core
     * nothing until the drag ends: [released] returns the one reorder the drag's steps made, and
     * QueueOrder holds the lane from the first step so that core's read cannot undo the steps before then.
     */
    fun step(onto: String): LaneDrop? {
        val drag = drag
        val from = drag?.row?.place
        val to = placed[onto]?.takeIf { it.lane == from?.lane && it != from }
        if (drag == null || from == null || to == null) return null
        when (to.lane) {
            BridgeQueueLane.MANUAL -> manual.add(to.position, manual.removeAt(from.position))
            BridgeQueueLane.CONTEXT -> context.move(from.position, to.position)
        }
        if (!drag.moved) moveSent(to.lane)
        drag.row = LaneDrop(drag.row.entryId, to)
        drag.moved = true
        return drag.row
    }

    /** The drag ended: the reorder to send core, where the dragged row now sits, or null when it moved nothing. */
    fun released(): LaneDrop? {
        val ended = drag
        drag = null
        return ended?.takeIf { it.moved }?.row
    }

    /** A drag is reordering the lanes; values wait until it ends. */
    fun dragging() {
        dragging = true
    }

    /** Hold [lane] until the reorder of one of its rows returns. The drag's first step calls this. */
    private fun moveSent(lane: BridgeQueueLane) {
        holds[lane] = (holds[lane] ?: 0) + 1
    }

    /**
     * A reorder of a row of [lane] returned, whether it moved the row, moved nothing or failed: the lane
     * shows core's read, unless core has not yet returned a later drag's reorder, whose moves stay.
     */
    fun reorderReturned(lane: BridgeQueueLane) {
        val waiting = holds[lane]
        if (waiting == null) {
            // Every reorder the sheet sends holds its lane first, so a return with no hold is a bug here.
            logger.error("reorder of a $lane row returned, but QueueOrder holds no reorder of that lane")
            return
        }
        if (waiting > 1) {
            holds[lane] = waiting - 1
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
        for (lane in BridgeQueueLane.entries) {
            if (lane !in holds) reseed(lane, value)
        }
    }

    private fun reseed(
        lane: BridgeQueueLane,
        value: QueueValue,
    ) {
        when (lane) {
            BridgeQueueLane.MANUAL -> {
                if (manual.toList() != value.manual) {
                    manual.clear()
                    manual.addAll(value.manual)
                }
            }

            BridgeQueueLane.CONTEXT -> {
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

/** Where the sheet draws a row: its [lane], and its [position] among the lane's rows as drawn now. */
internal data class RowPlace(
    val lane: BridgeQueueLane,
    val position: Int,
)

/** A row a drag moved, [entryId], and the [place] it then sits at, as the sheet sends it to core. */
internal data class LaneDrop(
    val entryId: String,
    val place: RowPlace,
)

/** The row the person is dragging, where it sits now, and whether a step has moved it. */
private class QueueDrag(
    var row: LaneDrop,
    var moved: Boolean = false,
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
