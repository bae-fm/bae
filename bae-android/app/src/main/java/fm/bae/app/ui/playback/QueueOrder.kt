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
 * sent is unanswered, then until the lane's read reaches the version core answered the last one
 * with; a reorder core refused or that failed ends the hold at once, so a move that didn't take
 * never stays on screen.
 */
internal class QueueOrder {
    val manual = mutableStateListOf<BridgeQueueEntry>()
    val context = ContextLaneOrder()

    /** Core's version of the context lane the sheet shows, which re-keys each context row's load;
     *  null before the first value. */
    var contextVersion by mutableStateOf<ULong?>(null)
        private set

    /** Core's version of Up Next the sheet shows. */
    private var manualVersion: ULong? = null

    val isEmpty: Boolean
        get() = manual.isEmpty() && context.size == 0

    /** Whether a drag is moving rows now. */
    private var dragging = false

    /** Each lane's moves core hasn't shown yet. */
    private val holds = mutableMapOf<OrderLane, LaneHold>()

    /** The lanes whose holds ended while their moves still lie over them, until re-seeded. */
    private val landed = mutableSetOf<OrderLane>()

    /** The last value [apply] took, which an answer or the end of a drag shows again. */
    private var latest: QueueValue? = null

    /**
     * Move the row keyed [fromKey] to where the row keyed [toKey] shows, as a step of a drag does, and
     * say what core is to be told. A step moves within one lane; one that spans the lanes, names no row
     * (a section header matches no row key), or would drop a row just before one not read yet
     * ([ContextLaneOrder.moveBefore]) moves nothing and answers null.
     */
    fun drop(
        fromKey: String,
        toKey: String,
    ): LaneDrop? = manualDrop(fromKey, toKey) ?: contextDrop(fromKey, toKey)

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
        return LaneDrop(OrderLane.MANUAL, moved.entryId, place)
    }

    /** [drop] within what's playing from. */
    private fun contextDrop(
        fromKey: String,
        toKey: String,
    ): LaneDrop? {
        val from = context.indexOf(fromKey)
        val to = context.indexOf(toKey)
        val place = if (from != null && to != null) context.moveBefore(from, to) else null
        return place?.let { LaneDrop(OrderLane.CONTEXT, fromKey, it) }
    }

    /** A drag is reordering the lanes; values wait until it ends. */
    fun dragging() {
        dragging = true
    }

    /** The sheet sent core a reorder of a row of [lane]. */
    fun moveSent(lane: OrderLane) {
        holds.getOrPut(lane) { LaneHold() }.unanswered++
    }

    /** Core moved the row, making [lane]'s [version]. */
    fun moveAnswered(
        lane: OrderLane,
        version: ULong,
    ) {
        val hold = holds[lane] ?: return
        hold.unanswered--
        hold.landsAt = maxOf(hold.landsAt ?: version, version)
        show()
    }

    /** Core moved nothing in [lane], or the reorder failed: the lane shows core's read at once. */
    fun moveFailed(lane: OrderLane) {
        holds.remove(lane) ?: return
        reseed(lane)
    }

    /**
     * Take the queue. A lane shows core's read again once nothing holds it: re-seeded when its version
     * moved or its moves have landed. A context lane core did not change is more of its tail read,
     * which fills only the rows it read.
     */
    fun apply(
        manual: List<BridgeQueueEntry>,
        context: QueueContext?,
        manualVersion: ULong,
        contextVersion: ULong,
    ) {
        dragging = false
        latest = QueueValue(manual, context, manualVersion, contextVersion)
        show()
    }

    private fun show() {
        val value = latest ?: return
        if (dragging) return
        if (!held(OrderLane.MANUAL, value.manualVersion) &&
            (OrderLane.MANUAL in landed || value.manualVersion != manualVersion)
        ) {
            reseed(OrderLane.MANUAL)
        }
        if (held(OrderLane.CONTEXT, value.contextVersion)) {
            // The moves lie over the rows they were made on: more of that read fills in, a read at
            // another version waits for the hold to end.
            if (value.contextVersion == contextVersion) context.fill(value.context)
        } else if (OrderLane.CONTEXT in landed || value.contextVersion != contextVersion) {
            reseed(OrderLane.CONTEXT)
        } else {
            context.fill(value.context)
        }
    }

    /** Whether [lane]'s moves still wait for core at [version]; a hold that has landed ends here. */
    private fun held(
        lane: OrderLane,
        version: ULong,
    ): Boolean {
        val hold = holds[lane] ?: return false
        val landsAt = hold.landsAt
        val waiting = hold.unanswered > 0 || landsAt == null || version < landsAt
        if (!waiting) {
            holds.remove(lane)
            landed += lane
        }
        return waiting
    }

    private fun reseed(lane: OrderLane) {
        val value = latest ?: return
        landed -= lane
        when (lane) {
            OrderLane.MANUAL -> {
                manual.clear()
                manual.addAll(value.manual)
                manualVersion = value.manualVersion
            }

            OrderLane.CONTEXT -> {
                context.seed(value.context)
                contextVersion = value.contextVersion
            }
        }
    }
}

/** The reorders a lane sent that core hasn't shown: how many are unanswered, and the newest version
 *  core answered one with. */
private class LaneHold {
    var unanswered = 0
    var landsAt: ULong? = null
}

/** One value of the queue as [QueueOrder.apply] took it. */
private class QueueValue(
    val manual: List<BridgeQueueEntry>,
    val context: QueueContext?,
    val manualVersion: ULong,
    val contextVersion: ULong,
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

    /** Show [context], unchanged by core with more of its tail read. */
    fun fill(context: QueueContext?) {
        source = context
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

/** A row a drag moved, in [lane], and where it then sits, as core is told it. */
internal data class LaneDrop(
    val lane: OrderLane,
    val entryId: String,
    val place: LanePlace,
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
