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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.DragHandle
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
import androidx.compose.runtime.remember
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
import sh.calvin.reorderable.rememberReorderableLazyListState

private const val TAG = "bae.QueueScreen"
private val logger = BaeLogger(TAG)

/** One row's stable identity for the list. The core queue is a list of track
 *  ids that may repeat (the same track can be enqueued twice), so the track id
 *  alone is not a unique key; the occurrence index disambiguates duplicates and
 *  keeps the key stable for a given entry across re-hydrations and drags. */
private data class KeyedQueueItem(
    val key: String,
    val item: QueueItem,
)

private fun keyed(queue: List<QueueItem>): List<KeyedQueueItem> {
    val counts = HashMap<String, Int>()
    return queue.map { item ->
        val n = counts.getOrDefault(item.trackId, 0)
        counts[item.trackId] = n + 1
        KeyedQueueItem("${item.trackId}#$n", item)
    }
}

/**
 * The play queue, presented in a bottom sheet: the currently-playing track, then
 * a drag-reorderable "Up Next" list. Reads the authoritative queue and now-
 * playing off the [fm.bae.app.playback.BaeCorePlayer]; mutations go straight to
 * the bridge (`clearQueue`/`removeFromQueue`/`reorderQueue`/`skipToQueueIndex`)
 * and reflect back as a `QueueUpdated` event that re-hydrates the projection.
 *
 * The whole sheet is one [LazyColumn] (its own scroll container) so it can't hit
 * the unbounded-height measurement a `LazyColumn` nested in a `Column` would.
 * `itemsIndexed`'s index is the position within the up-next list — i.e. the
 * exact index `removeFromQueue`/`skipToQueueIndex` expect.
 */
@Composable
fun QueueScreen(
    session: OpenLibrary,
    onDismiss: () -> Unit,
) {
    val queue by session.playback.queue.collectAsState()
    val nowPlaying by session.playback.nowPlaying.collectAsState()

    val listState = rememberLazyListState()
    // Local optimistic order so a dragged row follows the finger. Seeded from the
    // authoritative flow, but NOT while a drag is in progress — re-seeding mid-
    // drag would clobber the optimistic move and snap the row back.
    val order = remember { mutableStateListOf<KeyedQueueItem>() }
    val reorderState =
        rememberReorderableLazyListState(listState) { from, to ->
            // Map the dragged/target keys back to positions in `order` (offset-free:
            // the non-row header items never match a row key).
            val fromPos = order.indexOfFirst { it.key == from.key }
            val toPos = order.indexOfFirst { it.key == to.key }
            if (fromPos < 0 || toPos < 0) return@rememberReorderableLazyListState
            order.add(toPos, order.removeAt(fromPos))
            // Core's reorder treats `to` as a gap index: a forward move inserts at
            // `to - 1`, so pass `toPos + 1` to land the item where the drag dropped
            // it. A backward move maps straight through.
            val coreTo = if (toPos > fromPos) toPos + 1 else toPos
            try {
                session.appHandle.reorderQueue(fromPos.toUInt(), coreTo.toUInt())
            } catch (e: Exception) {
                logger.error("reorderQueue $fromPos -> $coreTo failed", e)
            }
        }

    LaunchedEffect(queue, reorderState.isAnyItemDragging) {
        if (!reorderState.isAnyItemDragging) {
            order.clear()
            order.addAll(keyed(queue))
        }
    }

    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxWidth(),
        contentPadding = PaddingValues(bottom = 24.dp),
    ) {
        item(key = "header") { QueueHeader(session = session, isQueueEmpty = order.isEmpty()) }
        queueContent(
            session = session,
            order = order,
            reorderState = reorderState,
            nowPlaying = nowPlaying,
            onDismiss = onDismiss,
        )
    }
}

private fun androidx.compose.foundation.lazy.LazyListScope.queueContent(
    session: OpenLibrary,
    order: List<KeyedQueueItem>,
    reorderState: sh.calvin.reorderable.ReorderableLazyListState,
    nowPlaying: NowPlaying?,
    onDismiss: () -> Unit,
) {
    nowPlaying?.let { np ->
        item(key = "nowplaying") {
            SectionLabel(stringResource(R.string.queue_section_now_playing))
            NowPlayingRow(np)
        }
    }
    if (order.isEmpty()) {
        item(key = "empty") {
            Text(
                text = stringResource(if (nowPlaying != null) R.string.queue_nothing_up_next else R.string.queue_empty),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.fillMaxWidth().padding(32.dp),
            )
        }
    } else {
        item(key = "uphdr") { SectionLabel(stringResource(R.string.queue_section_up_next)) }
        itemsIndexed(order, key = { _, k -> k.key }) { index, k ->
            ReorderableItem(reorderState, key = k.key) { isDragging ->
                Surface(tonalElevation = if (isDragging) 4.dp else 0.dp, color = MaterialTheme.colorScheme.surface) {
                    QueueRow(
                        item = k.item,
                        dragHandleModifier = Modifier.draggableHandle(),
                        onClick = {
                            try {
                                session.appHandle.skipToQueueIndex(index.toUInt())
                                onDismiss()
                            } catch (e: Exception) {
                                logger.error("skipToQueueIndex $index failed", e)
                            }
                        },
                        onRemove = {
                            try {
                                session.appHandle.removeFromQueue(index.toUInt())
                            } catch (e: Exception) {
                                logger.error("removeFromQueue $index failed", e)
                            }
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun QueueHeader(
    session: OpenLibrary,
    isQueueEmpty: Boolean,
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
            enabled = !isQueueEmpty,
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
