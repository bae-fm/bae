package fm.bae.app.ui.playback

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.playback.QueueContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry

/** The optimistic order of the two lanes, each reordered on its own. */
internal class QueueOrder {
    val manual = mutableStateListOf<BridgeQueueEntry>()
    val context = ContextLaneOrder()

    /** The queue revision the lanes were seeded from, which re-keys each context row's load; null before the first. */
    var revision by mutableStateOf<ULong?>(null)
        private set

    val isEmpty: Boolean
        get() = manual.isEmpty() && context.size == 0

    /** Whether a drag reordered the lanes since they were last seeded. */
    private var dragged = false

    /** A drag is reordering the lanes; the next [apply] re-seeds them from core. */
    fun dragging() {
        dragged = true
    }

    /**
     * Take a queue value. A new revision, or the end of a drag, re-seeds both lanes from it: core's
     * value carries a reorder that took and drops one that failed. Otherwise it is a page of the
     * context's tail arriving at the same revision, which fills only the rows it read.
     */
    fun apply(
        manual: List<BridgeQueueEntry>,
        context: QueueContext?,
        revision: ULong,
    ) {
        if (revision == this.revision && !dragged) {
            this.context.fill(context)
            return
        }
        dragged = false
        this.manual.clear()
        this.manual.addAll(manual)
        this.context.seed(context)
        this.revision = revision
    }
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

    /** Whether the context plays shuffled, which its section's toggle shows. */
    val shuffled: Boolean
        get() = source?.shuffled ?: false

    /** What the context plays from, which labels its section; null when nothing plays from a context. */
    val kind: BridgePlaybackSourceKind?
        get() = source?.kind

    /** Show [context] from its first row, with no moves over it. */
    fun seed(context: QueueContext?) {
        source = context
        moves.clear()
    }

    /** Show [context], the same revision with more of its tail read. */
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
    private fun QueueContext.readIndexOf(entryId: String): Int? =
        upcoming.indexOfFirst { it.entryId == entryId }.takeIf { it >= 0 }
            ?: pagedUpcoming.entries.firstOrNull { it.value.entryId == entryId }?.key

    /** Move the row shown at [from] to [to], as a drag does. */
    fun move(
        from: Int,
        to: Int,
    ) {
        moves.add(LaneMove(from, to))
    }
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
