package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.durationClockLabel
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidEyebrow
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.NoticeLook
import kotlinx.coroutines.launch
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

/** The play queue for a sheet that names it: the playing track, then the drag-reorderable lanes. */
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
    ) {
        nowPlaying?.let { np ->
            item(key = "nowplaying") {
                AndroidEyebrow(
                    text = stringResource(R.string.queue_section_now_playing),
                    spaceAbove = 0.dp,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .padding(
                                start = ThemeSpace.edge,
                                top = ThemeSpace.related,
                                end = ThemeSpace.edge,
                                bottom = ThemeSpace.inline,
                            ),
                )
                AndroidQueueRow(title = np.title, artist = np.artist, cover = np.coverImage, current = true)
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
    /** Core's shuffle mode, which the context section's toggle shows. */
    val shuffled: Boolean,
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
    val scope = rememberCoroutineScope()
    val reorderState =
        rememberReorderableLazyListState(listState) { from, to ->
            // Each move is held over its lane until core's read shows it; one that moved nothing, or
            // that failed (and shows its error), shows the lane's read at once.
            val drop = order.drop(from.key as? String ?: "", to.key as? String ?: "")
            if (drop == null) {
                logger.debug("reorder: nothing moved (from=${from.key}, to=${to.key})")
                return@rememberReorderableLazyListState
            }
            val beforeEntryId = (drop.place as? LanePlace.Before)?.entryId
            order.moveSent(drop.lane)
            scope.launch {
                val version =
                    failures.answer("reorderEntry ${drop.entryId} before $beforeEntryId") {
                        session.appHandle.reorderEntry(drop.entryId, beforeEntryId)
                    }
                if (version != null) order.moveAnswered(drop.lane, version) else order.moveFailed(drop.lane)
            }
        }

    // Values wait out a drag, then re-seed the lanes; a page of the tail fills only its rows.
    LaunchedEffect(queue, reorderState.isAnyItemDragging) {
        if (reorderState.isAnyItemDragging) {
            order.dragging()
        } else {
            order.apply(queue.manual, queue.context, queue.manualVersion, queue.contextVersion)
        }
    }
    return ReorderableQueue(order, reorderState, failures, readFailure, queue.shuffled)
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
            AndroidNotice(
                error = failure,
                modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.page),
                look = NoticeLook.QUIET,
            )
        }
        return
    }
    // Only show the empty message when nothing follows at all; with a context
    // present, the "Playing from" section below carries the queue.
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
            QueueLane(size = order.manual.size, itemAt = order.manual::get, changes = 0uL, loadRange = null),
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
                BridgePlaybackSourceKind.TRACKS -> R.string.queue_section_these_tracks
                BridgePlaybackSourceKind.RELEASE, null -> R.string.queue_section_playing_from
            }
        ContextSectionLabel(
            session = session,
            failures = queue.failures,
            text = stringResource(labelRes),
            shuffled = queue.shuffled,
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
            changes = order.contextVersion ?: 0uL,
            loadRange = { offset, limit -> session.playback.loadUpcomingRange(offset, limit) },
        ),
        queue.reorderState,
        queue.failures,
        onSkipped,
    )
    item(key = "upcoming-failure") { UpcomingFailureNotice(session) }
}

/** One lane's rows for [queueRows], read per row; [loadRange] fetches unloaded rows and is null for the manual lane. */
private class QueueLane(
    val size: Int,
    val itemAt: (Int) -> BridgeQueueEntry?,
    /** Core's version of the lane, which restarts a row's load. */
    val changes: ULong,
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
        val scope = rememberCoroutineScope()
        if (item == null) {
            LaunchedEffect(lane.changes, index) {
                lane.loadRange?.invoke(
                    (index - QUEUE_UPCOMING_LOAD_BATCH_SIZE / 2).coerceAtLeast(0),
                    QUEUE_UPCOMING_LOAD_BATCH_SIZE,
                )
            }
            AndroidQueuePlaceholderRow()
        } else {
            ReorderableItem(reorderState, key = item.entryId) { isDragging ->
                Surface(tonalElevation = if (isDragging) 4.dp else 0.dp, color = MaterialTheme.colorScheme.surface) {
                    AndroidQueueRow(
                        title = item.display.title,
                        artist = item.display.artistNames,
                        album = item.display.albumTitle,
                        cover = item.display.coverImage,
                        duration = LocalContext.current.durationClockLabel(item.durationClock),
                        dragHandleModifier = Modifier.draggableHandle(),
                        onClick = {
                            scope.launch {
                                failures
                                    .answer("skipToEntry ${item.entryId}") {
                                        session.appHandle.skipToEntry(item.entryId)
                                    }?.let { onSkipped?.invoke() }
                            }
                        },
                        onRemove = {
                            scope.launch {
                                failures.answer("removeEntry ${item.entryId}") {
                                    session.appHandle.removeEntry(item.entryId)
                                }
                            }
                        },
                    )
                }
            }
        }
    }
}

// The manual lane's label with its Clear.
@Composable
private fun UpNextSectionLabel(
    session: OpenLibrary,
    failures: QueueFailures,
) {
    val scope = rememberCoroutineScope()
    AndroidQueueLabel(
        title = stringResource(R.string.queue_section_up_next),
        clearDescription = stringResource(R.string.queue_clear_up_next),
        onClear = { scope.launch { failures.answer("clearUpNext") { session.appHandle.clearUpNext() } } },
    )
}

// The context section's label with its Clear and shuffle toggle; the playing track keeps playing through both.
@Composable
private fun ContextSectionLabel(
    session: OpenLibrary,
    failures: QueueFailures,
    text: String,
    shuffled: Boolean,
) {
    val scope = rememberCoroutineScope()
    AndroidQueueLabel(
        title = text,
        clearDescription = stringResource(R.string.queue_clear_playing_from),
        onClear = { scope.launch { failures.answer("clearPlayingFrom") { session.appHandle.clearPlayingFrom() } } },
        shuffled = shuffled,
        onShuffle = {
            scope.launch {
                failures.answer("setShuffle ${!shuffled}") { session.appHandle.setShuffle(!shuffled) }
            }
        },
    )
}
