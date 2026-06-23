package fm.bae.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material.icons.filled.Shuffle
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.playback.NowPlaying
import fm.bae.app.playback.QueueItem
import sh.calvin.reorderable.ReorderableItem
import sh.calvin.reorderable.ReorderableLazyListState
import sh.calvin.reorderable.rememberReorderableLazyListState

private const val TAG = "bae.QueueScreen"
private val logger = BaeLogger(TAG)

/**
 * The play queue, presented in a bottom sheet: the currently-playing track, then
 * a drag-reorderable "Up Next" list. Reads the authoritative queue and now-
 * playing off the [fm.bae.app.playback.BaeCorePlayer]; mutations go straight to
 * the bridge (`clearQueue`/`removeEntry`/`reorderEntry`/`skipToEntry`) and
 * reflect back as a `QueueUpdated` event that re-hydrates the projection.
 *
 * Each [QueueItem] carries a unique per-instance `entryId` — stable even when
 * the same track is queued twice — so rows key on it directly and remove/
 * reorder/skip target one instance.
 *
 * The whole sheet is one [LazyColumn] (its own scroll container) so it can't hit
 * the unbounded-height measurement a `LazyColumn` nested in a `Column` would.
 */
@Composable
fun QueueScreen(
    session: OpenLibrary,
    onDismiss: () -> Unit,
) {
    val nowPlaying by session.playback.nowPlaying.collectAsState()
    val listState = rememberLazyListState()
    val (order, reorderState) = rememberReorderableQueue(session, listState)

    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxWidth(),
        contentPadding = PaddingValues(bottom = 24.dp),
    ) {
        // Clear empties only the manual lane (the context survives), so it
        // disables on an empty manual lane regardless of the context.
        item(key = "header") { QueueHeader(session = session, isClearDisabled = order.manual.isEmpty()) }
        nowPlaying?.let { np ->
            item(key = "nowplaying") {
                SectionLabel(stringResource(R.string.queue_section_now_playing))
                NowPlayingRow(np)
            }
        }
        queueContent(
            session = session,
            order = order,
            reorderState = reorderState,
            hasNowPlaying = nowPlaying != null,
            onSkipped = onDismiss,
        )
    }
}

/**
 * The optimistic order of one lane (the manual "Up Next" lane or the context),
 * mutated in place on a drag. Each lane reorders independently — entry ids are
 * unique across both, and core no-ops a cross-lane move — so the two are held
 * apart and a drop is resolved within its own lane.
 */
internal class QueueOrder {
    val manual = mutableStateListOf<QueueItem>()
    val context = mutableStateListOf<QueueItem>()
    var contextShuffled by mutableStateOf(false)

    val isEmpty: Boolean
        get() = manual.isEmpty() && context.isEmpty()

    /** The lane (manual or context) holding the entry id, or null if neither. */
    fun laneOf(entryId: String): SnapshotStateList<QueueItem>? =
        when {
            manual.any { it.entryId == entryId } -> manual
            context.any { it.entryId == entryId } -> context
            else -> null
        }
}

/**
 * The optimistic two-lane queue order plus its reorderable list state, shared by
 * the queue sheet and the expanded player's embedded queue so the conventions
 * stay in one place. The order is seeded from the authoritative flow but NOT while
 * a drag is in progress — re-seeding mid-drag would clobber the optimistic move
 * and snap the row back. A drop maps the dragged/target entry ids to positions
 * WITHIN one lane and calls core's reorder with the entry id of the row the moved
 * item now precedes; a cross-lane drag is ignored (core no-ops it).
 */
@Composable
internal fun rememberReorderableQueue(
    session: OpenLibrary,
    listState: LazyListState,
): Pair<QueueOrder, ReorderableLazyListState> {
    val queue by session.playback.queue.collectAsState()
    val order = remember { QueueOrder() }
    val reorderState =
        rememberReorderableLazyListState(listState) { from, to ->
            // Resolve both keys to their lane; a reorder stays within one lane
            // (the section headers never match a row key, so they're skipped).
            val lane = order.laneOf(from.key as? String ?: "")
            if (lane == null || lane !== order.laneOf(to.key as? String ?: "")) {
                logger.debug("reorder: drag spans lanes or key not a row (from=${from.key}, to=${to.key}); ignoring")
                return@rememberReorderableLazyListState
            }
            val fromPos = lane.indexOfFirst { it.entryId == from.key }
            val toPos = lane.indexOfFirst { it.entryId == to.key }
            if (fromPos < 0 || toPos < 0) {
                return@rememberReorderableLazyListState
            }
            val moved = lane.removeAt(fromPos)
            lane.add(toPos, moved)
            // The moved entry lands before whatever now follows it in the lane;
            // a null `before` (it's now last) means the lane's end.
            val beforeEntryId = lane.getOrNull(toPos + 1)?.entryId
            try {
                session.appHandle.reorderEntry(moved.entryId, beforeEntryId)
            } catch (e: Exception) {
                logger.error("reorderEntry ${moved.entryId} before $beforeEntryId failed", e)
            }
        }

    LaunchedEffect(queue, reorderState.isAnyItemDragging) {
        if (!reorderState.isAnyItemDragging) {
            order.manual.clear()
            order.manual.addAll(queue.manual)
            order.context.clear()
            order.context.addAll(queue.context?.upcoming ?: emptyList())
            order.contextShuffled = queue.context?.shuffled ?: false
        }
    }
    return order to reorderState
}

// The two-section queue (manual "Up Next" lane + the context section, or an empty
// message) — shared by the queue sheet and the expanded player's embedded queue so
// the reorder / skip / remove conventions stay in one place. The caller owns the
// now-playing header above this (the sheet shows a Now Playing row; the player
// shows the full transport), passing `hasNowPlaying` only to word the empty state.
// `onSkipped` runs after a tap-to-skip — the sheet passes its dismiss; the embedded
// player passes `null` because it stays put on skip.
internal fun LazyListScope.queueContent(
    session: OpenLibrary,
    order: QueueOrder,
    reorderState: ReorderableLazyListState,
    hasNowPlaying: Boolean,
    onSkipped: (() -> Unit)?,
) {
    // Only show the empty message when nothing follows at all; with a context
    // present, the "Playing From" section below carries the queue.
    if (order.isEmpty) {
        item(key = "empty") {
            Text(
                text = stringResource(if (hasNowPlaying) R.string.queue_nothing_up_next else R.string.queue_empty),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.fillMaxWidth().padding(32.dp),
            )
        }
        return
    }

    if (order.manual.isNotEmpty()) {
        item(key = "uphdr") { SectionLabel(stringResource(R.string.queue_section_up_next)) }
        queueRows(session, order.manual, reorderState, onSkipped)
    }

    if (order.context.isNotEmpty()) {
        item(key = "ctxhdr") {
            ContextSectionLabel(
                text = stringResource(R.string.queue_section_playing_from),
                shuffled = order.contextShuffled,
            )
        }
        queueRows(session, order.context, reorderState, onSkipped)
    }
}

// One lane's reorderable rows. Entry ids key the rows (unique across both lanes),
// so skip/remove/reorder target one instance.
private fun LazyListScope.queueRows(
    session: OpenLibrary,
    items: List<QueueItem>,
    reorderState: ReorderableLazyListState,
    onSkipped: (() -> Unit)?,
) {
    itemsIndexed(items, key = { _, item -> item.entryId }) { _, item ->
        ReorderableItem(reorderState, key = item.entryId) { isDragging ->
            Surface(tonalElevation = if (isDragging) 4.dp else 0.dp, color = MaterialTheme.colorScheme.surface) {
                QueueRow(
                    item = item,
                    dragHandleModifier = Modifier.draggableHandle(),
                    onClick = {
                        try {
                            session.appHandle.skipToEntry(item.entryId)
                            onSkipped?.invoke()
                        } catch (e: Exception) {
                            logger.error("skipToEntry ${item.entryId} failed", e)
                        }
                    },
                    onRemove = {
                        try {
                            session.appHandle.removeEntry(item.entryId)
                        } catch (e: Exception) {
                            logger.error("removeEntry ${item.entryId} failed", e)
                        }
                    },
                )
            }
        }
    }
}

@Composable
private fun QueueHeader(
    session: OpenLibrary,
    isClearDisabled: Boolean,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = stringResource(R.string.queue),
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            modifier = Modifier.weight(1f),
        )
        TextButton(
            onClick = {
                try {
                    session.appHandle.clearQueue()
                } catch (e: Exception) {
                    logger.error("clearQueue failed", e)
                }
            },
            enabled = !isClearDisabled,
        ) {
            Text(stringResource(R.string.queue_clear))
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.labelMedium,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 4.dp),
    )
}

// The context section's label, with a shuffle indicator when the release was
// ordered by shuffle.
@Composable
private fun ContextSectionLabel(
    text: String,
    shuffled: Boolean,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (shuffled) {
            Spacer(modifier = Modifier.width(6.dp))
            Icon(
                imageVector = Icons.Filled.Shuffle,
                contentDescription = stringResource(R.string.queue_shuffled),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

@Composable
private fun NowPlayingRow(np: NowPlaying) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            path = np.coverPath,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = np.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.primary,
                maxLines = 1,
            )
            Text(
                text = np.artist,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun QueueRow(
    item: QueueItem,
    dragHandleModifier: Modifier,
    onClick: () -> Unit,
    onRemove: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            path = item.coverPath,
            cornerRadius = 4.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = item.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
            )
            Text(
                text =
                    if (item.albumTitle.isEmpty()) {
                        item.artist
                    } else {
                        "${item.artist} — ${item.albumTitle}"
                    },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
        // durationLabel is pre-formatted (empty when core has no duration); keep
        // the slot in the tree and toggle via alpha so rows align.
        Text(
            text = item.durationLabel,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier =
                Modifier
                    .padding(horizontal = 8.dp)
                    .alpha(if (item.durationLabel.isEmpty()) 0f else 1f),
        )
        IconButton(onClick = onRemove) {
            Icon(
                imageVector = Icons.Filled.Close,
                contentDescription = stringResource(R.string.queue_remove),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Icon(
            imageVector = Icons.Filled.DragHandle,
            contentDescription = stringResource(R.string.queue_drag_to_reorder),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = dragHandleModifier.size(24.dp),
        )
    }
}
