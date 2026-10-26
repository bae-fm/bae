package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
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
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.Eyebrow
import sh.calvin.reorderable.ReorderableItem
import sh.calvin.reorderable.ReorderableLazyListState
import sh.calvin.reorderable.rememberReorderableLazyListState
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry

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
    val queue = rememberReorderableQueue(session, listState)

    LazyColumn(
        state = listState,
        modifier = Modifier.fillMaxWidth(),
        contentPadding = PaddingValues(bottom = ThemeSpace.section),
    ) {
        // Each lane's Clear sits in its own section label; this header only names the sheet.
        item(key = "header") { QueueHeader() }
        nowPlaying?.let { np ->
            item(key = "nowplaying") {
                Eyebrow(
                    text = stringResource(R.string.queue_section_now_playing),
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.inline),
                )
                NowPlayingRow(np)
            }
        }
        queueContent(
            session = session,
            queue = queue,
            hasNowPlaying = nowPlaying != null,
            onSkipped = onDismiss,
        )
    }
}

/**
 * The queue as a sheet shows it: its optimistic order, the drag state that reorders it, and the
 * failures of the commands the sheet sends.
 */
internal class ReorderableQueue(
    val order: QueueOrder,
    val reorderState: ReorderableLazyListState,
    val failures: QueueFailures,
    /** The queue's last read failed, so its order is no longer known to be current; shown in its place. */
    val readFailure: DisplayError?,
)

/** The optimistic queue order and its reorderable list state, re-seeded from core except during a drag. */
@Composable
internal fun rememberReorderableQueue(
    session: OpenLibrary,
    listState: LazyListState,
): ReorderableQueue {
    val queue by session.playback.queue.collectAsState()
    val readFailure by session.playback.readFailures.queue
        .collectAsState()
    val order = remember { QueueOrder() }
    val failures = rememberQueueFailures()
    val reorderState =
        rememberReorderableLazyListState(listState) { from, to ->
            // A reorder stays within one lane; section headers match no row key.
            val fromKey = from.key as? String ?: ""
            val toKey = to.key as? String ?: ""
            val manualFrom = order.manual.indexOfFirst { it.entryId == fromKey }
            val manualTo = order.manual.indexOfFirst { it.entryId == toKey }
            val contextFrom = order.context.indexOf(fromKey)
            val contextTo = order.context.indexOf(toKey)
            when {
                manualFrom >= 0 && manualTo >= 0 -> {
                    val moved = order.manual.removeAt(manualFrom)
                    order.manual.add(manualTo, moved)
                    // A null `before` means the lane's end.
                    val beforeEntryId = order.manual.getOrNull(manualTo + 1)?.entryId
                    failures.run("reorderEntry ${moved.entryId} before $beforeEntryId") {
                        session.appHandle.reorderEntry(moved.entryId, beforeEntryId)
                    }
                }

                contextFrom != null && contextTo != null -> {
                    order.context.move(contextFrom, contextTo)
                    // A null `before` means the lane's end.
                    val beforeEntryId = order.context.itemAt(contextTo + 1)?.entryId
                    failures.run("reorderEntry $fromKey before $beforeEntryId") {
                        session.appHandle.reorderEntry(fromKey, beforeEntryId)
                    }
                }

                else -> {
                    logger.debug("reorder: drag spans lanes or names no row (from=${from.key}, to=${to.key})")
                }
            }
        }

    // Values wait out a drag, then re-seed the lanes; a page of the tail fills only its rows.
    LaunchedEffect(queue, reorderState.isAnyItemDragging) {
        if (reorderState.isAnyItemDragging) {
            order.dragging()
        } else {
            order.apply(queue.manual, queue.context, queue.revision)
        }
    }
    return ReorderableQueue(order, reorderState, failures, readFailure)
}

// The queue's lanes, an empty message, or the failure of its last read, below the caller's now-playing header;
// `onSkipped` runs after a tap-to-skip.
// A failed queue command shows above the lanes, and a stopped read of the context tail below it, with Retry.
internal fun LazyListScope.queueContent(
    session: OpenLibrary,
    queue: ReorderableQueue,
    hasNowPlaying: Boolean,
    onSkipped: (() -> Unit)?,
) {
    val order = queue.order
    val reorderState = queue.reorderState
    val failures = queue.failures
    item(key = "command-failure") { QueueCommandFailure(failures) }
    queue.readFailure?.let { failure ->
        item(key = "read-failure") {
            ErrorDetailDisclosure(error = failure, modifier = Modifier.fillMaxWidth().padding(ThemeSpace.page))
        }
        return
    }
    // Only show the empty message when nothing follows at all; with a context
    // present, the "Playing From" section below carries the queue.
    if (order.isEmpty) {
        item(key = "empty") {
            Text(
                text = stringResource(if (hasNowPlaying) R.string.queue_nothing_up_next else R.string.queue_empty),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.fillMaxWidth().padding(ThemeSpace.page),
            )
        }
        return
    }

    if (order.manual.isNotEmpty()) {
        item(key = "uphdr") { UpNextSectionLabel(session, failures) }
        // Always fully resolved, so no load hook.
        queueRows(
            session,
            QueueLane(size = order.manual.size, itemAt = order.manual::get, revision = 0uL, loadRange = null),
            reorderState,
            failures,
            onSkipped,
        )
    }

    if (order.context.size > 0) {
        contextLane(session, queue, onSkipped)
    }
}

// The context section: its label, its rows, and a stopped read of its tail with Retry.
private fun LazyListScope.contextLane(
    session: OpenLibrary,
    queue: ReorderableQueue,
    onSkipped: (() -> Unit)?,
) {
    val order = queue.order
    item(key = "ctxhdr") {
        val labelRes =
            when (order.context.kind) {
                BridgePlaybackSourceKind.LIBRARY -> R.string.queue_section_your_library
                BridgePlaybackSourceKind.RELEASE, null -> R.string.queue_section_playing_from
            }
        ContextSectionLabel(
            session = session,
            failures = queue.failures,
            text = stringResource(labelRes),
            shuffled = order.context.shuffled,
        )
    }
    // The context tail is library-scaled and only partly read: the lazy list
    // asks only the rows it places, and an unread row renders a placeholder
    // and triggers loadUpcomingRange.
    queueRows(
        session,
        QueueLane(
            size = order.context.size,
            itemAt = order.context::itemAt,
            revision = order.revision ?: 0uL,
            loadRange = { offset, limit -> session.playback.loadUpcomingRange(offset, limit) },
        ),
        queue.reorderState,
        queue.failures,
        onSkipped,
    )
    item(key = "upcoming-failure") { UpcomingFailure(session) }
}

/** One lane's rows for [queueRows], read per row; [loadRange] fetches unloaded rows and is null for the manual lane. */
private class QueueLane(
    val size: Int,
    val itemAt: (Int) -> BridgeQueueEntry?,
    val revision: ULong,
    val loadRange: (suspend (offset: Int, limit: Int) -> Unit)?,
)

// One lane's reorderable rows, keyed by entry id; an unloaded row shows a placeholder and loads its batch.
private fun LazyListScope.queueRows(
    session: OpenLibrary,
    lane: QueueLane,
    reorderState: ReorderableLazyListState,
    failures: QueueFailures,
    onSkipped: (() -> Unit)?,
) {
    items(count = lane.size, key = { index -> lane.itemAt(index)?.entryId ?: "placeholder-$index" }) { index ->
        val item = lane.itemAt(index)
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
                            failures.run("skipToEntry ${item.entryId}") {
                                session.appHandle.skipToEntry(item.entryId)
                                onSkipped?.invoke()
                            }
                        },
                        onRemove = {
                            failures.run("removeEntry ${item.entryId}") {
                                session.appHandle.removeEntry(item.entryId)
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
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
    )
}

// A section label with controls beside it — the shape both lane headers share.
@Composable
private fun SectionLabelRow(
    text: String,
    trailing: @Composable () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = ThemeSpace.edge, end = ThemeSpace.related),
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
private fun UpNextSectionLabel(
    session: OpenLibrary,
    failures: QueueFailures,
) {
    SectionLabelRow(text = stringResource(R.string.queue_section_up_next)) {
        ClearLaneButton(R.string.queue_clear_up_next) {
            failures.run("clearUpNext") { session.appHandle.clearUpNext() }
        }
    }
}

// The context section's label with its Clear and shuffle toggle; the playing track keeps playing through both.
@Composable
private fun ContextSectionLabel(
    session: OpenLibrary,
    failures: QueueFailures,
    text: String,
    shuffled: Boolean,
) {
    SectionLabelRow(text = text) {
        ClearLaneButton(R.string.queue_clear_playing_from) {
            failures.run("clearPlayingFrom") { session.appHandle.clearPlayingFrom() }
        }
        IconButton(
            onClick = { failures.run("setShuffle ${!shuffled}") { session.appHandle.setShuffle(!shuffled) } },
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
                modifier = Modifier.size(ThemeIcon.medium),
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
