package fm.bae.app.ui.playback

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.playback.QueueContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry

/** The two lanes of the queue. */
internal enum class OrderLane { MANUAL, CONTEXT }

/**
 * The lanes as the sheet shows them: core's read of each, with the moves of a drag laid over a lane
 * until core's read shows them. A lane's moves are held while the drag runs and while a reorder it
 * sent is unanswered, then until the lane's read draws the moved row where the drag left it, or the
 * lane has changed length; a reorder core refused or that failed ends the hold at once, so a move that
 * didn't take never stays on screen.
 */
internal class QueueOrder {
    val manual = mutableStateListOf<BridgeQueueEntry>()
    val context = ContextLaneOrder()

    val isEmpty: Boolean
        get() = manual.isEmpty() && context.size == 0

    /** Whether a drag is moving rows now. */
    private var dragging = false

    /** Each lane's moves core hasn't shown yet. */
    private val holds = mutableMapOf<OrderLane, LaneHold>()

    /** Where the drag's last step left its row, which its release sends core. */
    private var pending: LaneDrop? = null

    /** The last value [apply] took, which an answer or the end of a drag shows again. */
    private var latest: QueueValue? = null

    /**
     * Move the row keyed [fromKey] to where the row keyed [toKey] shows, as a step of a drag does, and
     * answer where the row then sits. A step moves within one lane; one that spans the lanes, names no row
     * (a section header matches no row key), or would drop a row just before one not read yet
     * ([ContextLaneOrder.moveBefore]) moves nothing and answers null. Core is told nothing until the drag
     * ends: [released] answers the one reorder the drag's steps made, and the lane is held from the first
     * step so core's read can't undo them before then.
     */
    fun drop(
        fromKey: String,
        toKey: String,
    ): LaneDrop? {
        val drop = manualDrop(fromKey, toKey) ?: contextDrop(fromKey, toKey) ?: return null
        if (pending?.lane != drop.lane) moveSent(drop.lane)
        pending = drop
        return drop
    }

    /** The drag ended: the reorder to send core, where the dragged row now sits, or null when it moved nothing. */
    fun released(): LaneDrop? =
        pending.also { drop ->
            pending = null
            drop?.let { holds[it.lane]?.target = it }
        }

    /** [drop] within Up Next, which is read whole, so the row after the drop is always known. */
    private fun manualDrop(
        fromKey: String,
        toKey: String,
    ): LaneDrop? {
        val from = manual.indexOfFirst { it.entryId == fromKey }
        val to = manual.indexOfFirst { it.entryId == toKey }
        if (from < 0 || to < 0) return null
        val moved = manual.removeAt(from)
        manual.add(to, moved)
        val place = manual.getOrNull(to + 1)?.let { LanePlace.Before(it.entryId) } ?: LanePlace.End
        return LaneDrop(OrderLane.MANUAL, moved.entryId, place, to)
    }

    /** [drop] within what's playing from. */
    private fun contextDrop(
        fromKey: String,
        toKey: String,
    ): LaneDrop? {
        val from = context.indexOf(fromKey)
        val to = context.indexOf(toKey)
        val place = if (from != null && to != null) context.moveBefore(from, to) else null
        return if (place != null && to != null) LaneDrop(OrderLane.CONTEXT, fromKey, place, to) else null
    }

    /** A drag is reordering the lanes; values wait until it ends. */
    fun dragging() {
        dragging = true
    }

    /** A reorder of a row of [lane] is held until core answers it: the drag's first step makes it. */
    fun moveSent(lane: OrderLane) {
        val size = if (lane == OrderLane.MANUAL) manual.size else context.size
        holds.getOrPut(lane) { LaneHold(size) }.unanswered++
    }

    /** Core moved the row of [lane]: the lane stays as moved until its read shows it. */
    fun moveAnswered(lane: OrderLane) {
        val hold = holds[lane] ?: return
        hold.unanswered--
        show()
    }

    /**
     * Core moved nothing in [lane], or the reorder failed: the lane shows core's read at once, unless a
     * later drag's reorder is still unanswered, which keeps its own moves held.
     */
    fun moveFailed(lane: OrderLane) {
        val hold = holds[lane] ?: return
        hold.unanswered--
        if (hold.unanswered > 0) return
        holds.remove(lane)
        reseed(lane)
    }

    /**
     * Take the queue. A lane shows core's read again once nothing holds it, or its hold's move shows in
     * the read.
     */
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
            if (!held(lane, value)) reseed(lane)
        }
    }

    /** Whether [lane]'s moves still wait for core's read [value]; a hold whose move the read shows ends here. */
    private fun held(
        lane: OrderLane,
        value: QueueValue,
    ): Boolean {
        val hold = holds[lane] ?: return false
        val target = hold.target
        val shown =
            hold.unanswered == 0 &&
                target != null &&
                (
                    value.sizeOf(lane) != hold.size ||
                        value.entryAt(lane, target.index)?.entryId == target.entryId
                )
        if (shown) holds.remove(lane)
        return !shown
    }

    private fun reseed(lane: OrderLane) {
        val value = latest ?: return
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

/** The reorders a lane sent that core hasn't shown: how many are unanswered, how long the lane was when the first
 *  was made, and where the last left its row, once the drag has ended. */
private class LaneHold(
    val size: Int,
) {
    var unanswered = 0
    var target: LaneDrop? = null
}

/** One value of the queue as [QueueOrder.apply] took it. */
private class QueueValue(
    val manual: List<BridgeQueueEntry>,
    val context: QueueContext?,
) {
    fun sizeOf(lane: OrderLane): Int = if (lane == OrderLane.MANUAL) manual.size else context?.upcomingTotal ?: 0

    fun entryAt(
        lane: OrderLane,
        index: Int,
    ): BridgeQueueEntry? = if (lane == OrderLane.MANUAL) manual.getOrNull(index) else context?.itemAt(index)
}

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

    /** Where the read row [entryId] shows, or null when no read row is it. */
    fun indexOf(entryId: String): Int? =
        source?.readIndexOf(entryId)?.let { read ->
            moves.fold(read) { at, move -> move.destinationOf(at) }
        }

    /** The tail index of the read row [entryId], or null when no read row is it. */
    private fun QueueContext.readIndexOf(entryId: String): Int? {
        val read = upcoming.entries.firstOrNull { it.value.entryId == entryId }
        return read?.key
    }

    /**
     * Move the row shown at [from] to [to], as a drag does, and say where it then sits for core: before
     * the row shown after it, or at the lane's end when none is. A drop just before a row not read yet
     * moves nothing and answers null: core places a row only before one the sheet names, and the sheet
     * can't name a row it hasn't read.
     */
    fun moveBefore(
        from: Int,
        to: Int,
    ): LanePlace? {
        val move = LaneMove(from, to)
        val place =
            if (to + 1 >= size) {
                LanePlace.End
            } else {
                val next = itemAt(move.sourceOf(to + 1)) ?: return null
                LanePlace.Before(next.entryId)
            }
        moves.add(move)
        return place
    }
}

/** A row a drag moved, in [lane], and where it then sits, as core is told it: [index] is its place in the lane as
 *  the sheet shows it. */
internal data class LaneDrop(
    val lane: OrderLane,
    val entryId: String,
    val place: LanePlace,
    val index: Int,
)

/** Where a moved row sits in its lane, as core is told it. */
internal sealed interface LanePlace {
    /** Just before the row [entryId]. */
    data class Before(
        val entryId: String,
    ) : LanePlace

    /** At the lane's end. */
    data object End : LanePlace
}

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

    /** The position, after this move, of the row at [index] before it. */
    fun destinationOf(index: Int): Int =
        when {
            index == from -> to
            from < to && index in (from + 1)..to -> index - 1
            to < from && index in to until from -> index + 1
            else -> index
        }
}
