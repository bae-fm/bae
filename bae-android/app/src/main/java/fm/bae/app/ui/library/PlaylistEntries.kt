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
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import fm.bae.app.creditLineText
import fm.bae.app.durationClockLabel
import fm.bae.app.factsLine
import fm.bae.app.ui.albumdetail.TrackOptionsMenu
import fm.bae.app.ui.components.AndroidLibraryRow
import uniffi.bae_bridge.BridgePlaylistDetail
import uniffi.bae_bridge.BridgePlaylistEntryRow
import kotlin.math.roundToInt

/** What a playlist's page does: its buttons, and the menu of each entry. */
internal class PlaylistEntryActions(
    /** Play the playlist from the entry [startPositionId] names, or from its first, shuffled or not. */
    val onPlay: (startPositionId: String?, shuffle: Boolean) -> Unit,
    val onPlayNext: (trackId: String) -> Unit,
    val onAddToQueue: (trackId: String) -> Unit,
    /** Offered only when the person can make playlists. */
    val onAddToPlaylist: ((trackId: String) -> Unit)?,
    /** Take the entry named by its position id out of the playlist. */
    val onRemove: (positionId: String) -> Unit,
    /** Write the entries in [order], each named by its position id; [onFailed] runs if the write fails. */
    val onReorder: (order: List<String>, onFailed: () -> Unit) -> Unit,
)

/**
 * The order a playlist's entries are drawn in while its owner moves one: the entry being dragged and
 * where it would land, and the order just written, held until the playlist's read shows it.
 */
internal class PlaylistReorder(
    private val entries: List<BridgePlaylistEntryRow>,
    private val write: (order: List<String>, onFailed: () -> Unit) -> Unit,
    private val held: MutableState<List<String>?>,
) {
    private var draggedId by mutableStateOf<String?>(null)
    private var startIndex by mutableStateOf(0)
    private var offset by mutableFloatStateOf(0f)

    /** The height of one entry's row, measured from the rows drawn. */
    var rowHeight by mutableFloatStateOf(0f)

    private val ordered: List<BridgePlaylistEntryRow>
        get() {
            val order = held.value
            if (order == null || order.toSet() != entries.map { it.positionId }.toSet()) return entries
            val byId = entries.associateBy { it.positionId }
            return order.mapNotNull { byId[it] }
        }

    private val target: Int
        get() =
            if (draggedId == null || rowHeight <= 0f) {
                startIndex
            } else {
                (startIndex + offset / rowHeight).roundToInt().coerceIn(0, ordered.lastIndex)
            }

    /** The entries as drawn: the dragged one already in the place it would land. */
    val shown: List<BridgePlaylistEntryRow>
        get() {
            val id = draggedId ?: return ordered
            val moved = ordered.toMutableList()
            val index = moved.indexOfFirst { it.positionId == id }
            if (index < 0) return ordered
            moved.add(target, moved.removeAt(index))
            return moved
        }

    /** How far the dragged entry's row is drawn from the slot it is in, so it follows the finger. */
    fun liftOffset(positionId: String): Float {
        if (positionId != draggedId) return 0f
        return (startIndex - target) * rowHeight + offset
    }

    fun isDragged(positionId: String): Boolean = positionId == draggedId

    fun start(positionId: String) {
        draggedId = positionId
        startIndex = ordered.indexOfFirst { it.positionId == positionId }
        offset = 0f
    }

    fun drag(by: Float) {
        offset += by
    }

    /** Let go: write the new order when the entry moved. */
    fun finish() {
        if (draggedId == null) return
        val order = shown.map { it.positionId }
        val moved = target != startIndex
        draggedId = null
        offset = 0f
        if (moved) {
            held.value = order
            write(order) { held.value = null }
        }
    }

    fun cancel() {
        draggedId = null
        offset = 0f
    }
}

/**
 * The reorder state of [detail]'s entries, written through [write]; a written order is held until
 * the read shows it.
 */
@Composable
internal fun rememberPlaylistReorder(
    detail: BridgePlaylistDetail,
    write: (order: List<String>, onFailed: () -> Unit) -> Unit,
): PlaylistReorder {
    val held = remember { mutableStateOf<List<String>?>(null) }
    LaunchedEffect(detail.entries) { held.value = null }
    val latestWrite by rememberUpdatedState(write)
    return remember(detail.entries) {
        PlaylistReorder(detail.entries, { order, failed -> latestWrite(order, failed) }, held)
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
    items(reorder.shown, key = { it.positionId }) { entry ->
        PlaylistEntry(entry, yours, actions, reorder, context)
    }
}

@Composable
private fun LazyItemScope.PlaylistEntry(
    entry: BridgePlaylistEntryRow,
    yours: Boolean,
    actions: PlaylistEntryActions,
    reorder: PlaylistReorder,
    context: Context,
) {
    val lifted = reorder.isDragged(entry.positionId)
    val latestReorder by rememberUpdatedState(reorder)
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
                    if (yours) {
                        Modifier.pointerInput(entry.positionId) {
                            detectDragGesturesAfterLongPress(
                                onDragStart = { latestReorder.start(entry.positionId) },
                                onDrag = { change, amount ->
                                    change.consume()
                                    latestReorder.drag(amount.y)
                                },
                                onDragEnd = { latestReorder.finish() },
                                onDragCancel = { latestReorder.cancel() },
                            )
                        }
                    } else {
                        Modifier
                    },
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

private val LIFT_SHADOW = 6.dp
private val ENTRY_MENU_END_PADDING = 4.dp
