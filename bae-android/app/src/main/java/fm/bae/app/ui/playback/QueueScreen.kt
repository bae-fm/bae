package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.playback.QueueItem
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Eyebrow
import sh.calvin.reorderable.ReorderableItem
import sh.calvin.reorderable.ReorderableLazyListState
import sh.calvin.reorderable.rememberReorderableLazyListState
import uniffi.bae_bridge.BridgePlaybackSourceKind

private const val TAG = "bae.QueueScreen"
private val logger = BaeLogger(TAG)

/**
 * Rows fetched around an unloaded context row, the page size
 * [fm.bae.app.playback.BaeCorePlayer.loadUpcomingRange] uses.
 */
private const val QUEUE_UPCOMING_LOAD_BATCH_SIZE = 100

/** The play queue in a bottom sheet: the playing track, then the drag-reorderable lanes. */
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
        // Each lane's Clear sits in its own section label; this header only names the sheet.
        item(key = "header") { QueueHeader() }
        nowPlaying?.let { np ->
            item(key = "nowplaying") {
                Eyebrow(
                    text = stringResource(R.string.queue_section_now_playing),
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
                )
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

/** The optimistic order of the two lanes, each reordered on its own; a null is a context row not yet loaded. */
internal class QueueOrder {
    val manual = mutableStateListOf<QueueItem?>()
    val context = mutableStateListOf<QueueItem?>()
    var contextShuffled by mutableStateOf(false)

    /** What the context plays from, which labels its section; null when nothing plays from a context. */
    var contextKind by mutableStateOf<BridgePlaybackSourceKind?>(null)

    /** The queue revision [context] was seeded from, which re-keys each context row's load. */
    var revision by mutableStateOf(0uL)

    val isEmpty: Boolean
        get() = manual.isEmpty() && context.isEmpty()

    /** The lane (manual or context) holding the entry id, or null if neither. */
    fun laneOf(entryId: String): SnapshotStateList<QueueItem?>? =
        when {
            manual.any { it?.entryId == entryId } -> manual
            context.any { it?.entryId == entryId } -> context
            else -> null
        }
}

/** The optimistic queue order and its reorderable list state, re-seeded from core except during a drag. */
@Composable
internal fun rememberReorderableQueue(
    session: OpenLibrary,
    listState: LazyListState,
): Pair<QueueOrder, ReorderableLazyListState> {
    val queue by session.playback.queue.collectAsState()
    val order = remember { QueueOrder() }
    val reorderState =
        rememberReorderableLazyListState(listState) { from, to ->
            // A reorder stays within one lane; section headers match no row key.
            val lane = order.laneOf(from.key as? String ?: "")
            if (lane == null || lane !== order.laneOf(to.key as? String ?: "")) {
                logger.debug("reorder: drag spans lanes or key not a row (from=${from.key}, to=${to.key}); ignoring")
                return@rememberReorderableLazyListState
            }
            val fromPos = lane.indexOfFirst { it?.entryId == from.key }
            val toPos = lane.indexOfFirst { it?.entryId == to.key }
            if (fromPos < 0 || toPos < 0) {
                // laneOf just matched both keys, so missing them now is an unexpected race.
                logger.warning("reorder: key not found in lane (from=${from.key}, to=${to.key}); ignoring")
                return@rememberReorderableLazyListState
            }
            val moved = lane.removeAt(fromPos)
            if (moved == null) {
                // A drag key only comes from a loaded row.
                logger.warning("reorder: resolved position $fromPos held no loaded entry (from=${from.key}); ignoring")
                return@rememberReorderableLazyListState
            }
            lane.add(toPos, moved)
            // A null `before` means the lane's end.
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
            when (val context = queue.context) {
                null -> {
                    order.contextShuffled = false
                    order.contextKind = null
                }

                else -> {
                    // Unloaded indices seed as null and render as placeholders.
                    order.context.addAll((0 until context.upcomingTotal).map(context::itemAt))
                    order.contextShuffled = context.shuffled
                    order.contextKind = context.kind
                }
            }
            order.revision = queue.revision
        }
    }
    return order to reorderState
}

// The queue's lanes, or an empty message, below the caller's now-playing header; `onSkipped` runs after a tap-to-skip.
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
        item(key = "uphdr") { UpNextSectionLabel(session) }
        // Always fully resolved, so no load hook.
        queueRows(
            session,
            QueueLane(order.manual, revision = 0uL, loadRange = null),
            reorderState,
            onSkipped,
        )
    }

    if (order.context.isNotEmpty()) {
        item(key = "ctxhdr") {
            val labelRes =
                when (order.contextKind) {
                    BridgePlaybackSourceKind.LIBRARY -> R.string.queue_section_your_library
                    BridgePlaybackSourceKind.RELEASE, null -> R.string.queue_section_playing_from
                }
            ContextSectionLabel(
                session = session,
                text = stringResource(labelRes),
                shuffled = order.contextShuffled,
            )
        }
        // The context tail is library-scaled and only partly resolved; a null
        // element renders a placeholder and triggers loadUpcomingRange.
        queueRows(
            session,
            QueueLane(
                order.context,
                revision = order.revision,
                loadRange = { offset, limit -> session.playback.loadUpcomingRange(offset, limit) },
            ),
            reorderState,
            onSkipped,
        )
    }
}

/** One lane's rows for [queueRows]; [loadRange] fetches unloaded rows and is null for the manual lane. */
private data class QueueLane(
    val items: List<QueueItem?>,
    val revision: ULong,
    val loadRange: (suspend (offset: Int, limit: Int) -> Unit)?,
)

// One lane's reorderable rows, keyed by entry id; an unloaded row shows a placeholder and loads its batch.
private fun LazyListScope.queueRows(
    session: OpenLibrary,
    lane: QueueLane,
    reorderState: ReorderableLazyListState,
    onSkipped: (() -> Unit)?,
) {
    itemsIndexed(lane.items, key = { index, item -> item?.entryId ?: "placeholder-$index" }) { index, item ->
        if (item == null) {
            LaunchedEffect(lane.revision, index) {
                lane.loadRange?.invoke(
                    (index - QUEUE_UPCOMING_LOAD_BATCH_SIZE / 2).coerceAtLeast(0),
                    QUEUE_UPCOMING_LOAD_BATCH_SIZE,
                )
            }
            QueueRowPlaceholder()
        } else {
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
}

@Composable
private fun QueueHeader() {
    Text(
        text = stringResource(R.string.queue),
        style = ThemeText.heading.style,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
    )
}

// A section label with controls beside it — the shape both lane headers share.
@Composable
private fun SectionLabelRow(
    text: String,
    trailing: @Composable () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Eyebrow(text = text, modifier = Modifier.weight(1f))
        trailing()
    }
}

// A lane's Clear, whose content description names the lane because a screen reader reads the button alone.
@Composable
private fun ClearLaneButton(
    contentDescriptionRes: Int,
    onClear: () -> Unit,
) {
    val description = stringResource(contentDescriptionRes)
    TextButton(
        onClick = onClear,
        modifier = Modifier.semantics { this.contentDescription = description },
    ) {
        Text(stringResource(R.string.queue_clear))
    }
}

// The manual lane's label with its Clear.
@Composable
private fun UpNextSectionLabel(session: OpenLibrary) {
    SectionLabelRow(text = stringResource(R.string.queue_section_up_next)) {
        ClearLaneButton(R.string.queue_clear_up_next) {
            try {
                session.appHandle.clearUpNext()
            } catch (e: Exception) {
                logger.error("clearUpNext failed", e)
            }
        }
    }
}

// The context section's label with its Clear and shuffle toggle; the playing track keeps playing through both.
@Composable
private fun ContextSectionLabel(
    session: OpenLibrary,
    text: String,
    shuffled: Boolean,
) {
    SectionLabelRow(text = text) {
        ClearLaneButton(R.string.queue_clear_playing_from) {
            try {
                session.appHandle.clearPlayingFrom()
            } catch (e: Exception) {
                logger.error("clearPlayingFrom failed", e)
            }
        }
        IconButton(
            onClick = {
                try {
                    session.appHandle.setShuffle(!shuffled)
                } catch (e: Exception) {
                    logger.error("setShuffle ${!shuffled} failed", e)
                }
            },
        ) {
            Icon(
                imageVector = Icons.Filled.Shuffle,
                contentDescription =
                    stringResource(
                        if (shuffled) R.string.queue_shuffle_off else R.string.queue_shuffle_on,
                    ),
                tint =
                    if (shuffled) {
                        MaterialTheme.colorScheme.primary
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                modifier = Modifier.size(20.dp),
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun QueueHeaderPreview() {
    BaeTheme {
        QueueHeader()
    }
}
