package fm.bae.app.ui.library

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyItemScope
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import fm.bae.app.R
import fm.bae.app.creditLineText
import fm.bae.app.durationClockLabel
import fm.bae.app.factsLine
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.components.TrackOptionsMenu
import uniffi.bae_bridge.BridgePlaylistDetail
import uniffi.bae_bridge.BridgePlaylistEntryRow
import kotlin.math.roundToInt

/** What a playlist's page does: its buttons, and the menu of each entry. */
internal class PlaylistEntryActions(
    /** Play the playlist from the entry [startPositionId] names, or from its first, shuffled or not. */
    val onPlay: (startPositionId: String?, shuffle: Boolean) -> Unit,
    val onPlayNext: (trackId: String) -> Unit,
    val onAddToQueue: (trackId: String) -> Unit,
    /** The menu shows Add to playlist only when the person can make playlists; null hides it. */
    val onAddToPlaylist: ((trackId: String) -> Unit)?,
    /** Take the entry named by its position id out of the playlist. */
    val onRemove: (positionId: String) -> Unit,
    /**
     * Move the entry named by its position id to the place [to] among the entries, where core changes the
     * order; [onReturned] runs when the write returns.
     */
    val onMove: (positionId: String, to: Int, onReturned: () -> Unit) -> Unit,
)

/**
 * PlaylistReorder holds the order the page draws while the owner moves an entry: the entry being dragged
 * and where it would land, and the order just written, which it keeps until the write returns. Then the
 * page draws the playlist's rows, in the order the library stores.
 */
internal class PlaylistReorder(
    private val entries: List<BridgePlaylistEntryRow>,
    private val write: (positionId: String, to: Int, onReturned: () -> Unit) -> Unit,
    private val held: MutableState<List<String>?>,
) {
    /** The entry a drag holds: its position id, the place it was drawn at, and how far it has moved. */
    private data class Drag(
        val positionId: String,
        val startIndex: Int,
        val offset: Float,
    )

    private var dragging by mutableStateOf<Drag?>(null)

    /** The height of one entry's row, measured from the rows drawn. */
    var rowHeight by mutableFloatStateOf(0f)

    private val ordered: List<BridgePlaylistEntryRow>
        get() {
            val order = held.value ?: return entries
            val byId = entries.associateBy { it.positionId }
            return order.mapNotNull { byId[it] }
        }

    /** The place [dragging]'s entry would land if let go now. */
    private fun Drag.target(): Int =
        if (rowHeight <= 0f) {
            startIndex
        } else {
            (startIndex + offset / rowHeight).roundToInt().coerceIn(0, ordered.lastIndex)
        }

    /** The entries as drawn: the dragged one already in the place it would land. */
    val shown: List<BridgePlaylistEntryRow>
        get() {
            val dragged = dragging ?: return ordered
            val moved = ordered.toMutableList()
            moved.add(dragged.target(), moved.removeAt(dragged.startIndex))
            return moved
        }

    /** How far the dragged entry's row is drawn from the slot it is in, so it follows the finger. */
    fun liftOffset(positionId: String): Float {
        val dragged = dragging?.takeIf { it.positionId == positionId } ?: return 0f
        return (dragged.startIndex - dragged.target()) * rowHeight + dragged.offset
    }

    fun isDragged(positionId: String): Boolean = positionId == dragging?.positionId

    /**
     * Move the entry [positionId], drawn at [index], [by] places (-1 up, 1 down) and write the move, as a
     * screen reader's moves do.
     */
    fun move(
        positionId: String,
        index: Int,
        by: Int,
    ) {
        hold(positionId, index, index + by)
    }

    /** Draw the entry [positionId] moved from [from] to [to] until the move's write returns. */
    private fun hold(
        positionId: String,
        from: Int,
        to: Int,
    ) {
        val order = ordered.map { it.positionId }.toMutableList()
        order.add(to, order.removeAt(from))
        held.value = order
        write(positionId, to) { held.value = null }
    }

    /** A drag took hold of the entry [positionId], drawn at [index]. */
    fun start(
        positionId: String,
        index: Int,
    ) {
        dragging = Drag(positionId, index, 0f)
    }

    fun drag(by: Float) {
        dragging = dragging?.let { it.copy(offset = it.offset + by) }
    }

    /** Let go: write the move when the entry moved. */
    fun finish() {
        val dragged = dragging ?: return
        val to = dragged.target()
        dragging = null
        if (to != dragged.startIndex) hold(dragged.positionId, dragged.startIndex, to)
    }

    fun cancel() {
        dragging = null
    }
}

/**
 * The reorder state of [detail]'s entries, written through [write]; the page keeps a written order until
 * its write returns.
 */
@Composable
internal fun rememberPlaylistReorder(
    detail: BridgePlaylistDetail,
    write: (positionId: String, to: Int, onReturned: () -> Unit) -> Unit,
): PlaylistReorder {
    val held = remember { mutableStateOf<List<String>?>(null) }
    val latestWrite by rememberUpdatedState(write)
    return remember(detail.entries) {
        PlaylistReorder(detail.entries, { positionId, to, returned -> latestWrite(positionId, to, returned) }, held)
    }
}

/**
 * The playlist's entries, each with its options menu. The playlist's owner holds a row to lift it and
 * drags it to another place: the rows between make room, and letting go writes the new order.
 */
internal fun LazyListScope.playlistEntries(
    detail: BridgePlaylistDetail,
    actions: PlaylistEntryActions,
    reorder: PlaylistReorder,
    context: Context,
) {
    val yours = detail.summary.yours
    itemsIndexed(reorder.shown, key = { _, entry -> entry.positionId }) { index, entry ->
        PlaylistEntry(entry, index, yours, actions, reorder, context)
    }
}

@Composable
private fun LazyItemScope.PlaylistEntry(
    entry: BridgePlaylistEntryRow,
    index: Int,
    yours: Boolean,
    actions: PlaylistEntryActions,
    reorder: PlaylistReorder,
    context: Context,
) {
    val lifted = reorder.isDragged(entry.positionId)
    val latestReorder by rememberUpdatedState(reorder)
    val moveUp = stringResource(R.string.playlist_move_up)
    val moveDown = stringResource(R.string.playlist_move_down)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier =
            Modifier
                .fillMaxWidth()
                .then(if (lifted) Modifier else Modifier.animateItem())
                .onSizeChanged { reorder.rowHeight = it.height.toFloat() }
                .zIndex(if (lifted) 1f else 0f)
                .graphicsLayer { translationY = latestReorder.liftOffset(entry.positionId) }
                .then(
                    if (lifted) {
                        Modifier
                            .shadow(LIFT_SHADOW, MaterialTheme.shapes.medium)
                            .background(MaterialTheme.colorScheme.surfaceContainerHigh, MaterialTheme.shapes.medium)
                    } else {
                        Modifier
                    },
                ).then(
                    if (yours) Modifier.ownerReorder(latestReorder, entry, index, moveUp, moveDown) else Modifier,
                ).padding(end = ENTRY_MENU_END_PADDING),
    ) {
        Box(modifier = Modifier.weight(1f)) {
            AndroidLibraryRow(
                title = entry.title,
                subtitle = context.factsLine(listOf(context.creditLineText(entry.artistLine), entry.albumTitle)),
                trailing = context.durationClockLabel(entry.durationClock),
                art = entry.cover,
                onClick = { actions.onPlay(entry.positionId, false) },
            )
        }
        TrackOptionsMenu(
            onPlayNext = { actions.onPlayNext(entry.trackId) },
            onAddToQueue = { actions.onAddToQueue(entry.trackId) },
            onAddToPlaylist = actions.onAddToPlaylist?.let { add -> { add(entry.trackId) } },
            onRemove = if (yours) ({ actions.onRemove(entry.positionId) }) else null,
        )
    }
}

/**
 * What the owner does to an entry's row to move it: hold and drag it, or ask a screen reader to move it
 * up ([moveUp] names that action) or down ([moveDown]).
 */
private fun Modifier.ownerReorder(
    reorder: PlaylistReorder,
    entry: BridgePlaylistEntryRow,
    index: Int,
    moveUp: String,
    moveDown: String,
): Modifier =
    semantics {
        customActions =
            listOfNotNull(
                entryMoveAction(reorder, entry, index, -1, moveUp).takeIf { entry.canMoveUp },
                entryMoveAction(reorder, entry, index, 1, moveDown).takeIf { entry.canMoveDown },
            )
    }.pointerInput(entry.positionId, index) {
        detectDragGesturesAfterLongPress(
            onDragStart = { reorder.start(entry.positionId, index) },
            onDrag = { change, amount ->
                change.consume()
                reorder.drag(amount.y)
            },
            onDragEnd = { reorder.finish() },
            onDragCancel = { reorder.cancel() },
        )
    }

/** The screen reader action that moves an entry, drawn at [index], [by] places. */
private fun entryMoveAction(
    reorder: PlaylistReorder,
    entry: BridgePlaylistEntryRow,
    index: Int,
    by: Int,
    label: String,
): CustomAccessibilityAction =
    CustomAccessibilityAction(label) {
        reorder.move(entry.positionId, index, by)
        true
    }

private val LIFT_SHADOW = 6.dp
private val ENTRY_MENU_END_PADDING = 4.dp
