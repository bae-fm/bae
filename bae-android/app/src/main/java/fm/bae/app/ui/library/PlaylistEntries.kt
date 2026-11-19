package fm.bae.app.ui.library

import android.content.Context
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import fm.bae.app.R
import fm.bae.app.creditLineText
import fm.bae.app.durationClockLabel
import fm.bae.app.factsLine
import fm.bae.app.ui.components.AndroidListRow
import fm.bae.app.ui.components.TrackOptionsMenu
import sh.calvin.reorderable.ReorderableCollectionItemScope
import sh.calvin.reorderable.ReorderableItem
import sh.calvin.reorderable.ReorderableLazyListState
import uniffi.bae_bridge.BridgeEntryPlace
import uniffi.bae_bridge.BridgePlaylistDetail
import uniffi.bae_bridge.BridgePlaylistEntryRow

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
     * Move the entry named by its position id to [place], where core finds the entry's new place in the
     * playlist; [onReturned] runs when the write returns.
     */
    val onMove: (positionId: String, place: BridgeEntryPlace, onReturned: () -> Unit) -> Unit,
)

/**
 * The order the page draws a playlist's entries in while its owner moves one: the order a drag draws
 * while it runs, and the order the page wrote, which it draws until the write returns. Then the page
 * draws the playlist's rows, in the order the library stores.
 */
internal class PlaylistReorder(
    private val entries: List<BridgePlaylistEntryRow>,
    private val write: (positionId: String, place: BridgeEntryPlace, onReturned: () -> Unit) -> Unit,
    private val held: MutableState<List<String>?>,
) {
    /**
     * The entry the person is dragging, the order the drag draws, and the place the drag's last step put
     * the entry at, null until a step moves it.
     */
    private data class Drag(
        val positionId: String,
        val order: List<String>,
        val place: BridgeEntryPlace?,
    )

    private var dragging by mutableStateOf<Drag?>(null)

    /** The entries as drawn: the drag's order while it runs, the written order until its write returns. */
    val shown: List<BridgePlaylistEntryRow>
        get() {
            val order = dragging?.order ?: held.value ?: return entries
            val byId = entries.associateBy { it.positionId }
            return order.mapNotNull { byId[it] }
        }

    /** The person started dragging the entry [positionId]. */
    fun start(positionId: String) {
        dragging = Drag(positionId, shown.map { it.positionId }, null)
    }

    /**
     * A step of the drag moved the dragged entry past the entry [onto], [down] the list or up it: the
     * page draws it just after [onto] or just before it.
     */
    fun step(
        onto: String,
        down: Boolean,
    ) {
        val drag = dragging?.takeIf { it.positionId != onto } ?: return
        val order = drag.order.filter { it != drag.positionId }.toMutableList()
        val at = order.indexOf(onto).takeIf { it >= 0 } ?: return
        order.add(if (down) at + 1 else at, drag.positionId)
        val place = if (down) BridgeEntryPlace.After(onto) else BridgeEntryPlace.Before(onto)
        dragging = drag.copy(order = order, place = place)
    }

    /** The person let go: write the move when a step moved the entry. */
    fun finish() {
        val drag = dragging ?: return
        dragging = null
        drag.place?.let { hold(drag.positionId, drag.order, it) }
    }

    /** Move the entry [positionId] one place [up] or down and write the move, as a screen reader's moves do. */
    fun move(
        positionId: String,
        up: Boolean,
    ) {
        val order = shown.map { it.positionId }.toMutableList()
        val from = order.indexOf(positionId)
        val to = if (up) from - 1 else from + 1
        if (from >= 0 && to in order.indices) order.add(to, order.removeAt(from))
        hold(positionId, order, if (up) BridgeEntryPlace.Up else BridgeEntryPlace.Down)
    }

    /** Draw [order] until the write of the entry [positionId]'s move to [place] returns. */
    private fun hold(
        positionId: String,
        order: List<String>,
        place: BridgeEntryPlace,
    ) {
        held.value = order
        write(positionId, place) { held.value = null }
    }
}

/**
 * The reorder state of [detail]'s entries, written through [write]. The page draws an order it wrote
 * until the write returns.
 */
@Composable
internal fun rememberPlaylistReorder(
    detail: BridgePlaylistDetail,
    write: (positionId: String, place: BridgeEntryPlace, onReturned: () -> Unit) -> Unit,
): PlaylistReorder {
    val held = remember { mutableStateOf<List<String>?>(null) }
    val latestWrite by rememberUpdatedState(write)
    return remember(detail.entries) {
        PlaylistReorder(
            detail.entries,
            { positionId, place, returned -> latestWrite(positionId, place, returned) },
            held,
        )
    }
}

/**
 * The playlist's entries, each with its options menu. The playlist's owner holds a row to lift it and
 * drags it to another place: the rows between make room, and letting go writes the move.
 */
internal fun LazyListScope.playlistEntries(
    detail: BridgePlaylistDetail,
    actions: PlaylistEntryActions,
    reorder: PlaylistReorder,
    reorderState: ReorderableLazyListState,
    context: Context,
) {
    val yours = detail.summary.yours
    items(reorder.shown, key = { it.positionId }) { entry ->
        ReorderableItem(reorderState, key = entry.positionId, enabled = yours) { lifted ->
            PlaylistEntry(entry, lifted, yours, actions, reorder, context)
        }
    }
}

@Composable
private fun ReorderableCollectionItemScope.PlaylistEntry(
    entry: BridgePlaylistEntryRow,
    lifted: Boolean,
    yours: Boolean,
    actions: PlaylistEntryActions,
    reorder: PlaylistReorder,
    context: Context,
) {
    val latestReorder by rememberUpdatedState(reorder)
    val moveUp = stringResource(R.string.playlist_move_up)
    val moveDown = stringResource(R.string.playlist_move_down)
    AndroidListRow(
        title = entry.title,
        subtitle = context.factsLine(listOf(context.creditLineText(entry.artistLine), entry.albumTitle)),
        trailing = context.durationClockLabel(entry.durationClock),
        art = entry.cover,
        onClick = { actions.onPlay(entry.positionId, false) },
        dragging = lifted,
        modifier =
            if (yours) {
                Modifier
                    .screenReaderMoves(latestReorder, entry, moveUp, moveDown)
                    .longPressDraggableHandle(
                        onDragStarted = { latestReorder.start(entry.positionId) },
                        onDragStopped = { latestReorder.finish() },
                    )
            } else {
                Modifier
            },
        menu = {
            TrackOptionsMenu(
                onPlayNext = { actions.onPlayNext(entry.trackId) },
                onAddToQueue = { actions.onAddToQueue(entry.trackId) },
                onAddToPlaylist = actions.onAddToPlaylist?.let { add -> { add(entry.trackId) } },
                onRemove = if (yours) ({ actions.onRemove(entry.positionId) }) else null,
            )
        },
    )
}

/**
 * The screen reader actions that move an entry one place up ([moveUp] names that action) or down
 * ([moveDown]), each offered when the playlist's query says the entry has a place to move to.
 */
private fun Modifier.screenReaderMoves(
    reorder: PlaylistReorder,
    entry: BridgePlaylistEntryRow,
    moveUp: String,
    moveDown: String,
): Modifier =
    semantics {
        customActions =
            listOfNotNull(
                entryMoveAction(reorder, entry, up = true, moveUp).takeIf { entry.canMoveUp },
                entryMoveAction(reorder, entry, up = false, moveDown).takeIf { entry.canMoveDown },
            )
    }

/** The screen reader action that moves an entry one place [up] or down. */
private fun entryMoveAction(
    reorder: PlaylistReorder,
    entry: BridgePlaylistEntryRow,
    up: Boolean,
    label: String,
): CustomAccessibilityAction =
    CustomAccessibilityAction(label) {
        reorder.move(entry.positionId, up)
        true
    }
